use std::sync::Arc;
use std::time::Instant;
use eltdx_protocol::commands::auctions::{AuctionSeriesRequest, DEFAULT_AUCTION_LIMIT,
    DEFAULT_AUCTION_SELECTOR, DEFAULT_AUCTION_START};
use eltdx_protocol::unit::{DateParts, NormalizedCode};
use eltdx_protocol::{CommandRequest, CommandResponse};
use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::domains::{auction_hosts, DomainPools};
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::{AuctionPoint, AuctionQuery, AuctionSeries, EventTime, MarketDateChoice,
    NumericValue, Price, Quantity, QuantityUnit, SecurityId, TradingDate, ValueProvenance};
use super::{allocation_error, checked_ids, result_meta, ServiceSource};

fn fetch_auction_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<crate::hq::decode::DecodedReply> {
    if !matches!(&request, CommandRequest::AuctionSeries(_)) {
        return Err(Error::new(ErrorKind::InvalidArgument, "auction service requires an auction request"));
    }
    context.check_boundary(TimeoutPhase::Admission)?;
    let decoded = fetch(request)?;
    if !matches!(&decoded.response, CommandResponse::AuctionSeries(_)) {
        return Err(Error::new(ErrorKind::Protocol, "auction request returned a different response kind"));
    }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(decoded)
}

pub(crate) fn legacy_auction_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(ErrorKind::InvalidArgument, "legacy auction requires a compatibility operation"));
    }
    Ok(fetch_auction_reply(context, request, fetch)?.response)
}

#[derive(Clone, Debug)]
pub struct AuctionService { executor: HqExecutor, pools: Arc<DomainPools>, config: ClientConfig }

impl AuctionService {
    pub(crate) fn new(executor: HqExecutor, pools: Arc<DomainPools>, config: ClientConfig) -> Self {
        Self { executor, pools, config }
    }
    pub fn series(&self, id: &SecurityId, query: &AuctionQuery, options: &OperationOptions) -> Result<AuctionSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "auction_series", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(DEFAULT_AUCTION_LIMIT as usize), bytes: None })?;
        let date = match query.date() { MarketDateChoice::Current => context.current_market_date()?, MarketDateChoice::Date(date) => date };
        let series = self.series_in_context(&context, id, query, date)?;
        context.finish()?;
        Ok(series)
    }

    pub(crate) fn series_in_context(&self, context: &OperationContext, id: &SecurityId,
        query: &AuctionQuery, date: TradingDate) -> Result<AuctionSeries> {
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        if matches!(query.date(), MarketDateChoice::Date(requested) if requested != date) {
            return Err(Error::new(ErrorKind::InvalidArgument, "auction composition date differs from its historical request"));
        }
        let wire_date = match query.date() {
            MarketDateChoice::Current => None,
            MarketDateChoice::Date(date) => Some(DateParts::new(i32::from(date.year()), date.month(), date.day()).map_err(Error::from_protocol)?),
        };
        let request = AuctionSeriesRequest::with_trading_date_and_include_raw(
            NormalizedCode::parse(&id.to_string()).map_err(Error::from_protocol)?, wire_date,
            DEFAULT_AUCTION_SELECTOR, DEFAULT_AUCTION_START, DEFAULT_AUCTION_LIMIT, false).map_err(Error::from_protocol)?;
        let namespace = format!("hq:auction:{}", auction_hosts(&self.config).join(","));
        let snapshot = match context.metadata()? {
            Some(snapshot) => snapshot,
            None => {
                let snapshot = context.owner().metadata().capture(&namespace, &[])?;
                context.install_metadata(snapshot.clone())?;
                snapshot
            }
        };
        let engine = self.pools.auction(&self.config)?;
        let decoded = fetch_auction_reply(context, CommandRequest::AuctionSeries(request), |request| {
            let raw = self.executor.execute_raw(&engine, context, request, ChildPurpose::Required, None)?;
            self.executor.decode_received(context, raw)
        })?;
        let CommandResponse::AuctionSeries(response) = &decoded.response else {
            return Err(Error::new(ErrorKind::Protocol, "auction request returned a different response kind"));
        };
        if response.points.len() > DEFAULT_AUCTION_LIMIT as usize {
            return Err(Error::new(ErrorKind::Protocol, "auction response exceeds requested limit"));
        }
        context.record_work(response.points.len(), 0)?;
        let mut points = Vec::new(); points.try_reserve(response.points.len()).map_err(allocation_error)?;
        let quantity = |value| Quantity::with_provenance(NumericValue::ScaledInteger { value, scale: 0 },
            QuantityUnit::Unknown, ValueProvenance::WireRule);
        for row in &response.points {
            if row.minute_of_day_raw >= 1440 || row.second_raw >= 60 || !row.price.is_finite() {
                return Err(Error::new(ErrorKind::Protocol, "auction response contains an invalid time or price"));
            }
            let hour = (row.minute_of_day_raw / 60) as u8; let minute = (row.minute_of_day_raw % 60) as u8;
            let time = match query.date() {
                MarketDateChoice::Current => EventTime::time_of_day(hour, minute, row.second_raw),
                MarketDateChoice::Date(date) => EventTime::local(date, hour, minute, row.second_raw),
            }.map_err(|error| Error::new(ErrorKind::Protocol, "auction response contains an invalid time").with_source(error))?;
            points.push(AuctionPoint { index: u32::from(row.index), time,
                price: Price::with_value_and_provenance(NumericValue::F32(f32::from_le_bytes(row.price_raw)), ValueProvenance::WireRule),
                matched: quantity(i64::from(row.matched_volume)), unmatched_signed: i64::from(row.unmatched_signed_raw),
                unmatched: quantity(i64::from(row.unmatched_volume)), direction_raw: i64::from(row.unmatched_direction_raw),
                minute_of_day_raw: row.minute_of_day_raw, second_raw: row.second_raw, reserved: row.reserved_zero_0e });
        }
        let ranges = response.points.iter().map(|row| row.record_range).collect::<Vec<_>>();
        let meta = result_meta(&context, &self.config, Some(&snapshot), &ServiceSource::from_reply(&decoded),
            Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "auctions.series")
            .with_trading_date(date);
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let series = AuctionSeries::new(id.clone(), date, DEFAULT_AUCTION_SELECTOR, DEFAULT_AUCTION_START,
            DEFAULT_AUCTION_LIMIT, points, meta);
        Ok(series)
    }
}
