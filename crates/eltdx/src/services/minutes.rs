use std::num::NonZeroUsize;
use std::time::Instant;

use eltdx_protocol::commands::minutes::{
    HistoricalIntradayRequest, IntradayAuxRequest, RecentIntradayRequest, SparklineRequest,
    TodayIntradayRequest, DEFAULT_SPARKLINE_FIXED_RAW, DEFAULT_TODAY_RESERVED_TAIL,
};
use eltdx_protocol::unit::{DateParts, NormalizedCode};
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::minutes::{AuxKind, AuxValues, MinuteAuxPoint, MinuteAuxSeries,
    MinutePoint, MinuteQuery, MinuteSeries, SparklineQuery, SparklineSeries};
use crate::types::provenance::ValueProvenance;
use crate::types::{BatchItem, BatchItemState, BatchOutcome, EventTime, MarketDateChoice, NumericValue, Price,
    Quantity, QuantityUnit, ResultMeta, SecurityId, TradingDate};

use super::{aggregate_meta, allocation_error, checked_ids, request_metadata, result_meta, ServiceSource};

const MAX_OPERATION_RECORDS: usize = 65_535;

/// Per-domain command facts shared by both service profiles. Auxiliary and
/// sparkline floats are intrinsic wire values and do not require a directory.
pub(crate) fn minute_command(request: &CommandRequest) -> Result<(bool, bool)> {
    Ok(match request {
        CommandRequest::TodayIntraday(request) => (true, request.include_raw),
        CommandRequest::HistoricalIntraday(request) => (true, request.include_raw),
        CommandRequest::RecentIntraday(request) => (true, request.include_raw),
        CommandRequest::IntradayAux(request) => (false, request.include_raw),
        CommandRequest::Sparkline(request) => (false, request.include_raw),
        _ => return Err(Error::new(ErrorKind::InvalidArgument, "minute service requires a minute-domain command")),
    })
}

/// One domain request/decode boundary. Transport and timeout policy are supplied
/// by the operation; model projection and strict-only checks remain separate.
fn fetch_minute_reply(
    context: &OperationContext, request: CommandRequest, purpose: ChildPurpose,
    mut fetch: impl FnMut(CommandRequest, ChildPurpose) -> Result<DecodedReply>,
) -> Result<DecodedReply> {
    minute_command(&request)?;
    context.check_boundary(TimeoutPhase::Admission)?;
    let expected = request.command_code();
    let decoded = fetch(request, purpose)?;
    let matching = matches!((expected, &decoded.response),
        (0x0537, CommandResponse::TodayIntraday(_))
        | (0x0fb4, CommandResponse::HistoricalIntraday(_))
        | (0x0feb, CommandResponse::RecentIntraday(_))
        | (0x051b, CommandResponse::IntradayAux(_))
        | (0x0fd1, CommandResponse::Sparkline(_)));
    if !matching { return Err(wrong_response("minute domain")); }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(decoded)
}

pub(crate) fn legacy_minute_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnMut(CommandRequest, ChildPurpose) -> Result<DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(ErrorKind::InvalidArgument, "legacy minutes require a compatibility operation"));
    }
    let (_, include_raw) = minute_command(&request)?;
    let mut decoded = fetch_minute_reply(context, request, ChildPurpose::Required, fetch)?;
    if include_raw {
        match &mut decoded.response {
            CommandResponse::TodayIntraday(series) => hydrate_minute_hex(context, &mut series.points, &decoded.payload)?,
            CommandResponse::HistoricalIntraday(series) => hydrate_minute_hex(context, &mut series.points, &decoded.payload)?,
            CommandResponse::RecentIntraday(series) => hydrate_minute_hex(context, &mut series.points, &decoded.payload)?,
            CommandResponse::IntradayAux(series) => {
                if series.points.len() != series.record_ranges.len() { return Err(wrong_response("minute raw ranges")); }
                for (index, (point, range)) in series.points.iter_mut().zip(&series.record_ranges).enumerate() {
                    if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
                    let text = range.hex(&decoded.payload).map_err(Error::from_protocol)?;
                    match point {
                        eltdx_protocol::commands::minutes::MinuteAuxPoint::BuySellStrength { record_hex, .. }
                        | eltdx_protocol::commands::minutes::MinuteAuxPoint::VolumeComparison { record_hex, .. } => *record_hex = text,
                    }
                }
            }
            // Sparkline has raw f32 bytes and a page payload, no row hex field.
            CommandResponse::Sparkline(_) => {},
            _ => return Err(wrong_response("legacy minute raw projection")),
        }
    }
    // Legacy protocol output accepts negative volumes and mismatched sparkline
    // echoes. Do not impose the public Rust projection's stronger assertions.
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(decoded.response)
}

fn hydrate_minute_hex(context: &OperationContext,
    points: &mut [eltdx_protocol::commands::minutes::MinutePoint], payload: &bytes::Bytes,
) -> Result<()> {
    for (index, point) in points.iter_mut().enumerate() {
        if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        point.record_hex = point.record_range.hex(payload).map_err(Error::from_protocol)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum MinuteMode { Today(TradingDate), History(TradingDate), Recent(TradingDate) }

#[derive(Clone, Debug)]
pub struct MinuteService {
    executor: HqExecutor,
    engine: eltdx_runtime::engine::Engine,
    config: ClientConfig,
    namespace: String,
}

impl MinuteService {
    pub(crate) fn new(executor: HqExecutor, engine: eltdx_runtime::engine::Engine,
        config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn today(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteSeries> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "minutes_today", &self.config, options)?;
        self.single_minutes(context, id, MinuteModeRequest::Today)
    }

    pub fn history(&self, id: &SecurityId, date: TradingDate, options: &OperationOptions) -> Result<MinuteSeries> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "minutes_history", &self.config, options)?;
        self.single_minutes(context, id, MinuteModeRequest::History(date))
    }

    pub fn recent(&self, id: &SecurityId, query: &MinuteQuery, options: &OperationOptions) -> Result<MinuteSeries> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "minutes_recent", &self.config, options)?;
        self.single_minutes(context, id, MinuteModeRequest::Recent(query.date()))
    }

    pub fn today_many(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>> {
        self.minutes_many(ids, "minutes_today_many", MinuteModeRequest::Today, options)
    }

    pub fn history_many(&self, ids: &[SecurityId], date: TradingDate, options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>> {
        self.minutes_many(ids, "minutes_history_many", MinuteModeRequest::History(date), options)
    }

    pub fn recent_many(&self, ids: &[SecurityId], query: &MinuteQuery, options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>> {
        self.minutes_many(ids, "minutes_recent_many", MinuteModeRequest::Recent(query.date()), options)
    }

    pub fn aux(&self, id: &SecurityId, kind: AuxKind, options: &OperationOptions) -> Result<MinuteAuxSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "minutes_aux", &self.config, options)?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(MAX_OPERATION_RECORDS), bytes: None })?;
        let result = self.aux_in_context(&context, id, kind)?;
        context.finish()?;
        Ok(result)
    }

    pub(crate) fn aux_in_context(&self, context: &OperationContext, id: &SecurityId,
        kind: AuxKind) -> Result<MinuteAuxSeries> {
        checked_ids(std::slice::from_ref(id))?;
        let request = IntradayAuxRequest::new(normalized_id(id)?, kind.wire());
        let command = CommandRequest::IntradayAux(request);
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace,
            std::slice::from_ref(&command))?;
        let decoded = self.fetch(&context, command, ChildPurpose::Required)?;
        let CommandResponse::IntradayAux(response) = &decoded.response else { return Err(wrong_response("intraday aux")); };
        let expected = kind.wire();
        if response.response_kind != expected { return Err(Error::new(ErrorKind::Protocol, "intraday aux response selector differs from request")); }
        let mut points = Vec::new();
        points.try_reserve(response.points.len()).map_err(allocation_error)?;
        for point in &response.points {
            let (index, label, values) = match point {
                eltdx_protocol::commands::minutes::MinuteAuxPoint::BuySellStrength { index, time_label, series_a, series_b, .. } =>
                    (*index, time_label, AuxValues::BuySell { buy_commission: *series_a, sell_commission: *series_b }),
                eltdx_protocol::commands::minutes::MinuteAuxPoint::VolumeComparison {
                    index, time_label, previous_day_cumulative_volume, current_day_cumulative_volume, ..
                } => (*index, time_label, AuxValues::Volumes {
                    previous: *previous_day_cumulative_volume,
                    current: *current_day_cumulative_volume,
                    cumulative: *current_day_cumulative_volume,
                }),
            };
            points.push(MinuteAuxPoint::new(u32::from(index), time_of_day(label)?, values));
        }
        context.record_work(points.len(), 0)?;
        let ranges = response.record_ranges.clone();
        let meta = response_meta(&context, &self.config, &metadata, &decoded, &ranges, "minutes.aux");
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(MinuteAuxSeries::new(id.clone(), kind, points, meta))
    }

    pub fn buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries> {
        self.aux(id, AuxKind::BuySellStrength, options)
    }

    pub fn volume_comparison(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries> {
        self.aux(id, AuxKind::VolumeComparison, options)
    }

    pub fn sparkline(&self, id: &SecurityId, query: &SparklineQuery, options: &OperationOptions) -> Result<SparklineSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "minutes_sparkline", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(MAX_OPERATION_RECORDS), bytes: None })?;
        let request = SparklineRequest::new(normalized_id(id)?, query.selector(), query.window(), DEFAULT_SPARKLINE_FIXED_RAW);
        let command = CommandRequest::Sparkline(request);
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace,
            std::slice::from_ref(&command))?;
        let decoded = self.fetch(&context, command, ChildPurpose::Required)?;
        let CommandResponse::Sparkline(response) = &decoded.response else { return Err(wrong_response("sparkline")); };
        validate_sparkline_identity(id, response.response_market_id, response.response_code.as_str())?;
        let selector_echo = u16::from(response.selector_echo);
        if selector_echo != u16::from(query.selector()) {
            return Err(Error::new(ErrorKind::Protocol, "sparkline selector echo differs from request")
                .with_context("requested", format!("{}", query.selector()))
                .with_context("received", format!("{selector_echo}")));
        }
        if response.prices.len() > usize::from(query.window()) {
            return Err(Error::new(ErrorKind::Protocol, "sparkline response exceeds requested window"));
        }
        let base = wire_price(response.base_price);
        let mut prices = Vec::new();
        prices.try_reserve(response.prices.len()).map_err(allocation_error)?;
        prices.extend(response.prices.iter().map(|price| wire_price(price.value)));
        context.record_work(prices.len(), 0)?;
        let ranges = response.prices.iter().map(|price| price.record_range).collect::<Vec<_>>();
        let meta = response_meta(&context, &self.config, &metadata, &decoded, &ranges, "minutes.sparkline");
        context.check_boundary(TimeoutPhase::Aggregate)?;
        context.finish()?;
        Ok(SparklineSeries::new(id.clone(), query.selector(), selector_echo, query.window(), response.max_count_raw,
            base, prices, response.reserved_param_u32, meta))
    }

    fn single_minutes(&self, mut context: OperationContext, id: &SecurityId, requested: MinuteModeRequest) -> Result<MinuteSeries> {
        checked_ids(std::slice::from_ref(id))?;
        let mode = requested.resolve(&context)?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(MAX_OPERATION_RECORDS), bytes: None })?;
        let command = minute_request(normalized_id(id)?, mode)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace,
            std::slice::from_ref(&command))?;
        let decoded = self.fetch(&context, command, ChildPurpose::Required)?;
        let (series, _) = project_minutes(&context, &self.config, &metadata, id, mode, &decoded)?;
        context.record_work(series.points().len(), 0)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        context.finish()?;
        Ok(series)
    }

    fn minutes_many(&self, ids: &[SecurityId], name: &'static str, request_mode: MinuteModeRequest,
        options: &OperationOptions) -> Result<BatchOutcome<MinuteSeries>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        let codes = checked_ids(ids)?;
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        let mode = request_mode.resolve(&context)?;
        let mut requests = Vec::new();
        requests.try_reserve(ids.len()).map_err(allocation_error)?;
        for code in codes { requests.push(minute_request(code, mode)?); }
        let continuation_pages = ids.len().saturating_sub(1);
        context.set_work_limits(WorkLimits { pages: NonZeroUsize::new(continuation_pages),
            records: Some(MAX_OPERATION_RECORDS), bytes: None })?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut items = Vec::new();
        let mut metas = Vec::new();
        items.try_reserve(ids.len()).map_err(allocation_error)?;
        metas.try_reserve(ids.len()).map_err(allocation_error)?;
        for (index, (id, request)) in ids.iter().zip(requests).enumerate() {
            context.check_boundary(TimeoutPhase::Admission)?;
            let purpose = if index == 0 { ChildPurpose::Required } else { ChildPurpose::Page };
            let decoded = self.fetch(&context, request, purpose)?;
            let (series, meta) = project_minutes(&context, &self.config, &metadata, id, mode, &decoded)?;
            context.record_work(series.points().len(), 0)?;
            metas.push(meta);
            items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(series)));
        }
        let meta = aggregate_meta(&context, &metas, Some(metadata.version()))?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        context.finish()?;
        Ok(BatchOutcome::with_meta(items, meta))
    }

    fn fetch(&self, context: &OperationContext, request: CommandRequest, purpose: ChildPurpose) -> Result<DecodedReply> {
        fetch_minute_reply(context, request, purpose, |request, purpose| {
            let raw = self.executor.execute_raw(&self.engine, context, request, purpose, None)?;
            self.executor.decode_received(context, raw)
        })
    }
}

#[derive(Clone, Copy)]
enum MinuteModeRequest { Today, History(TradingDate), Recent(MarketDateChoice) }

impl MinuteModeRequest {
    fn resolve(self, context: &OperationContext) -> Result<MinuteMode> {
        Ok(match self {
            Self::Today => MinuteMode::Today(context.current_market_date()?),
            Self::History(date) => MinuteMode::History(date),
            Self::Recent(MarketDateChoice::Current) => MinuteMode::Recent(context.current_market_date()?),
            Self::Recent(MarketDateChoice::Date(date)) => MinuteMode::Recent(date),
        })
    }
}

fn minute_request(code: NormalizedCode, mode: MinuteMode) -> Result<CommandRequest> {
    if mode_date(mode).year() > 9999 {
        return Err(Error::new(ErrorKind::InvalidArgument, "HQ date year must be between 1 and 9999")
            .with_context("field", "date"));
    }
    Ok(match mode {
        MinuteMode::Today(_) => CommandRequest::TodayIntraday(TodayIntradayRequest::new(code, DEFAULT_TODAY_RESERVED_TAIL)),
        MinuteMode::History(date) => CommandRequest::HistoricalIntraday(
            HistoricalIntradayRequest::new(code, protocol_date(date)).map_err(Error::from_protocol)?),
        MinuteMode::Recent(date) => CommandRequest::RecentIntraday(
            RecentIntradayRequest::new(code, protocol_date(date)).map_err(Error::from_protocol)?),
    })
}

fn project_minutes(context: &OperationContext, config: &ClientConfig, metadata: &MetadataSnapshot,
    id: &SecurityId, mode: MinuteMode, decoded: &DecodedReply) -> Result<(MinuteSeries, ResultMeta)> {
    let precision = metadata.require_precision(id)?;
    let provenance = precision.provenance();
    let price_scale = u32::from(precision.decimals().max(2));
    let mut points = Vec::new();
    let (previous_close, open, ranges) = match (&decoded.response, mode) {
        (CommandResponse::TodayIntraday(response), MinuteMode::Today(date)) => {
            points.try_reserve(response.points.len()).map_err(allocation_error)?;
            let mut sum = 0_i64;
            for point in &response.points {
                validate_minute_volume(point.volume)?;
                sum = sum.checked_add(point.volume).ok_or_else(|| limit_error("minute volume sum overflows"))?;
                let time = time_of_day(&point.time_label)?;
                let average = point.avg_raw.map(|raw| scaled_price(raw, price_scale + 2, provenance));
                points.push(MinutePoint::new(u32::from(point.index), time, minute_point_price(point, price_scale, provenance)?,
                    unknown_volume(point.volume), average));
            }
            let ranges = response.points.iter().map(|point| point.record_range).collect::<Vec<_>>();
            let volume = quantity_sum(sum);
            let source = ServiceSource::from_reply(decoded);
            let meta = result_meta(context, config, Some(metadata), &source, Some(decoded.payload.clone()),
                decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "minutes.today");
            let series = MinuteSeries::new(id.clone(), Some(date), None, None, points, volume, meta.clone());
            return Ok((series, meta));
        }
        (CommandResponse::HistoricalIntraday(response), MinuteMode::History(_)) => {
            points.try_reserve(response.points.len()).map_err(allocation_error)?;
            for point in &response.points {
                validate_minute_volume(point.volume)?;
                let time = event_time(point.time.ok_or_else(|| Error::new(ErrorKind::Protocol, "history minute has no dated time"))?)?;
                points.push(MinutePoint::new(u32::from(point.index), time, minute_point_price(point, price_scale, provenance)?,
                    unknown_volume(point.volume), None));
            }
            let ranges = response.points.iter().map(|point| point.record_range).collect::<Vec<_>>();
            (Some(wire_price(response.prev_close.ok_or_else(|| Error::new(ErrorKind::Protocol, "history minute has no previous close"))?)),
                None, ranges)
        }
        (CommandResponse::RecentIntraday(response), MinuteMode::Recent(_)) => {
            points.try_reserve(response.points.len()).map_err(allocation_error)?;
            for point in &response.points {
                validate_minute_volume(point.volume)?;
                let time = event_time(point.time.ok_or_else(|| Error::new(ErrorKind::Protocol, "recent minute has no dated time"))?)?;
                let average = point.avg_raw.map(|raw| scaled_price(raw, price_scale + 2, provenance));
                points.push(MinutePoint::new(u32::from(point.index), time, minute_point_price(point, price_scale, provenance)?,
                    unknown_volume(point.volume), average));
            }
            let ranges = response.points.iter().map(|point| point.record_range).collect::<Vec<_>>();
            (Some(wire_price(response.prev_close.ok_or_else(|| Error::new(ErrorKind::Protocol, "recent minute has no previous close"))?)),
                Some(wire_price(response.open_price.ok_or_else(|| Error::new(ErrorKind::Protocol, "recent minute has no open price"))?)), ranges)
        }
        _ => return Err(wrong_response("intraday")),
    };
    let volume_sum = points.iter().try_fold(0_i64, |sum, point| match point.volume().value() {
        NumericValue::ScaledInteger { value, scale: 0 } => sum.checked_add(value),
        _ => None,
    }).ok_or_else(|| limit_error("minute volume sum overflows"))?;
    let meta = response_meta(context, config, metadata, decoded, &ranges, match mode {
        MinuteMode::History(_) => "minutes.history",
        MinuteMode::Recent(_) => "minutes.recent",
        MinuteMode::Today(_) => "minutes.today",
    });
    let series = MinuteSeries::new(id.clone(), Some(mode_date(mode)), previous_close, open, points,
        quantity_sum(volume_sum), meta.clone());
    Ok((series, meta))
}

fn response_meta(context: &OperationContext, config: &ClientConfig, metadata: &MetadataSnapshot,
    decoded: &DecodedReply, ranges: &[RecordRange], name: &str) -> ResultMeta {
    result_meta(context, config, Some(metadata), &ServiceSource::from_reply(decoded), Some(decoded.payload.clone()),
        decoded.wire.as_ref().map(|wire| wire.raw.clone()), ranges, name)
}

fn event_time(value: eltdx_protocol::unit::DateTimeParts) -> Result<EventTime> {
    let date = crate::types::TradingDate::new(u16::try_from(value.date.year).map_err(|_| Error::new(ErrorKind::Protocol,
        "minute date year is outside the SDK range"))?, value.date.month, value.date.day)
        .map_err(|error| Error::new(ErrorKind::Protocol, "minute response contains an invalid date").with_source(error))?;
    EventTime::shanghai(date, value.hour, value.minute, value.second)
        .map_err(|error| Error::new(ErrorKind::Protocol, "minute response contains an invalid time").with_source(error))
}

fn time_of_day(value: &str) -> Result<EventTime> {
    let Some((hour, minute)) = value.split_once(':') else { return Err(Error::new(ErrorKind::Protocol, "minute response has an invalid time label")); };
    let hour = hour.parse::<u8>().map_err(|error| Error::new(ErrorKind::Protocol, "minute response has an invalid hour").with_source(error))?;
    let minute = minute.parse::<u8>().map_err(|error| Error::new(ErrorKind::Protocol, "minute response has an invalid minute").with_source(error))?;
    EventTime::time_of_day(hour, minute, 0).map_err(|error| Error::new(ErrorKind::Protocol, "minute response has an invalid time label").with_source(error))
}

fn mode_date(value: MinuteMode) -> TradingDate {
    match value { MinuteMode::Today(date) | MinuteMode::History(date) | MinuteMode::Recent(date) => date }
}

fn protocol_date(value: TradingDate) -> DateParts {
    DateParts { year: i32::from(value.year()), month: value.month(), day: value.day() }
}

fn normalized_id(id: &SecurityId) -> Result<NormalizedCode> {
    NormalizedCode::parse(&id.to_string()).map_err(|error| Error::new(ErrorKind::InvalidArgument,
        "invalid HQ security identity").with_source(error))
}

fn wire_price(value: f32) -> Price {
    Price::with_value_and_provenance(NumericValue::F32(value), ValueProvenance::WireRule)
}

fn scaled_price(raw: i64, scale: u32, provenance: ValueProvenance) -> Price {
    Price::with_value_and_provenance(NumericValue::ScaledInteger { value: raw, scale }, provenance)
}

fn minute_point_price(point: &eltdx_protocol::commands::minutes::MinutePoint,
    scale: u32, provenance: ValueProvenance) -> Result<Price> {
    let raw = point.price_raw.ok_or_else(|| Error::new(ErrorKind::Protocol, "minute price has no preserved source integer"))?;
    Ok(scaled_price(raw, scale, provenance))
}

fn unknown_volume(value: i64) -> Quantity {
    Quantity::with_provenance(NumericValue::ScaledInteger { value, scale: 0 }, QuantityUnit::Unknown, ValueProvenance::WireRule)
}

fn quantity_sum(value: i64) -> Quantity { unknown_volume(value) }

fn validate_minute_volume(value: i64) -> Result<()> {
    if value < 0 { return Err(Error::new(ErrorKind::Protocol, "minute volume is negative")); }
    Ok(())
}

fn validate_sparkline_identity(id: &SecurityId, response_market: u8, response_code: &str) -> Result<()> {
    let expected_market = match id.market() {
        crate::types::HqMarket::Shenzhen => 0,
        crate::types::HqMarket::Shanghai => 1,
        crate::types::HqMarket::Beijing => 2,
    };
    if response_market != expected_market || response_code != id.number() {
        return Err(Error::new(ErrorKind::Protocol, "sparkline response identity differs from request")
            .with_context("requested", format!("{id}"))
            .with_context("response_market", format!("{response_market}"))
            .with_context("response_code", response_code));
    }
    Ok(())
}

fn limit_error(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded, message) }
fn wrong_response(name: &str) -> Error { Error::new(ErrorKind::Protocol, format!("{name} request returned the wrong response kind")) }
