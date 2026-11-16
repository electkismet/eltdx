use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use eltdx_protocol::commands::trades::{HistoricalTicksRequest, TodayTicksRequest,
    TradeEventKind as WireKind, TradeSide as WireSide, TradeTick as WireTick};
use eltdx_protocol::unit::{DateParts, NormalizedCode};
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, PinHandle};

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::{BatchItem, BatchItemState, BatchOutcome, DataPage, EndEvidence, EventTime,
    MarketDateChoice, NumericValue, PageCursor, Price, Quantity, QuantityUnit, ResultMeta,
    SecurityId, SourceStamp, TradeColumns, TradeEventKind, TradePage, TradeQuery, TradeSide,
    TradeTick, TradingDate, ValueProvenance};

use super::pagination::{advance_offset, CollectionLimits, PageProgress};
use super::{aggregate_meta, allocation_error, checked_ids, endpoint_engine, result_meta,
    verify_cursor, ServiceSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadMode { Page, Collect, Opening }

struct ReadResult { page: TradePage, opening: Option<TradeTick> }

#[derive(Clone, Debug)]
pub struct TradeService {
    executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String,
}

impl TradeService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn page(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradePage> {
        let entry = Instant::now();
        self.one_at(entry, "trades_page", id, query, options, ReadMode::Page, rows)
    }
    pub fn collect(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradePage> {
        let entry = Instant::now();
        self.one_at(entry, "trades_collect", id, query, options, ReadMode::Collect, rows)
    }
    pub fn columns(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradeColumns> {
        let entry = Instant::now();
        self.one_at(entry, "trades_columns", id, query, options, ReadMode::Page, columns)
    }
    pub fn collect_columns(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<TradeColumns> {
        let entry = Instant::now();
        self.one_at(entry, "trades_collect_columns", id, query, options, ReadMode::Collect, columns)
    }
    pub fn opening_match(&self, id: &SecurityId, query: &TradeQuery, options: &OperationOptions) -> Result<Option<TradeTick>> {
        let entry = Instant::now();
        self.one_at(entry, "trades_opening_match", id, query, options, ReadMode::Opening, opening)
    }
    pub fn page_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradePage>> {
        let entry = Instant::now();
        self.many_at(entry, "trades_page_many", ids, query, options, ReadMode::Page, rows)
    }
    pub fn collect_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradePage>> {
        let entry = Instant::now();
        self.many_at(entry, "trades_collect_many", ids, query, options, ReadMode::Collect, rows)
    }
    pub fn columns_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradeColumns>> {
        let entry = Instant::now();
        self.many_at(entry, "trades_columns_many", ids, query, options, ReadMode::Page, columns)
    }
    pub fn collect_columns_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<TradeColumns>> {
        let entry = Instant::now();
        self.many_at(entry, "trades_collect_columns_many", ids, query, options, ReadMode::Collect, columns)
    }
    pub fn opening_match_many(&self, ids: &[SecurityId], query: &TradeQuery, options: &OperationOptions) -> Result<BatchOutcome<Option<TradeTick>>> {
        let entry = Instant::now();
        self.many_at(entry, "trades_opening_match_many", ids, query, options, ReadMode::Opening, opening)
    }

    #[allow(clippy::too_many_arguments)]
    fn one_at<T>(&self, entry: Instant, name: &'static str, id: &SecurityId, query: &TradeQuery,
        options: &OperationOptions, mode: ReadMode, project: fn(ReadResult) -> Result<T>) -> Result<T> {
        let mut context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        set_limits(&mut context, query, mode, 1)?;
        let value = self.one_in_context(&context, id, query, None, mode, project)?;
        context.finish()?;
        Ok(value)
    }

    /// Caller owns aggregate limits and completion. Collection retains the
    /// existing source pin, EOF, no-progress and cleanup behavior.
    pub(crate) fn collect_in_context(&self, context: &OperationContext, id: &SecurityId,
        query: &TradeQuery) -> Result<TradePage> {
        self.one_in_context(context, id, query, None, ReadMode::Collect, rows)
    }

    pub(crate) fn collect_at_date_in_context(&self, context: &OperationContext, id: &SecurityId,
        query: &TradeQuery, date: TradingDate) -> Result<TradePage> {
        self.one_in_context(context, id, query, Some(date), ReadMode::Collect, rows)
    }

    pub(crate) fn opening_page_in_context(&self, context: &OperationContext, id: &SecurityId,
        query: &TradeQuery, date: TradingDate) -> Result<(TradePage, Option<TradeTick>)> {
        self.one_in_context(context, id, query, Some(date), ReadMode::Opening,
            |result| Ok((result.page, result.opening)))
    }

    #[allow(clippy::too_many_arguments)]
    fn one_in_context<T>(&self, context: &OperationContext, id: &SecurityId, query: &TradeQuery,
        source_date: Option<TradingDate>, mode: ReadMode, project: fn(ReadResult) -> Result<T>) -> Result<T> {
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        let date = match source_date { Some(date) => date, None => resolve_date(context, query.date())? };
        if matches!(query.date(), MarketDateChoice::Date(requested) if requested != date) {
            return Err(Error::new(ErrorKind::InvalidArgument, "trade composition date differs from its requested historical date"));
        }
        let metadata = self.capture_metadata(context, std::slice::from_ref(id))?;
        let size = page_size(query, mode);
        verify_cursor(query.cursor(), fingerprint(id, query.date(), date), size, context, Some(metadata.version()))?;
        let engine = endpoint_engine(&self.executor, &self.config, &self.engine,
            query.cursor().and_then(PageCursor::endpoint))?;
        // A cursor is validated before any source pin or wire admission.
        if mode == ReadMode::Page {
            return project(self.read(context, &engine, None, id, query, date, &metadata, mode)?);
        }
        let mut pin = self.executor.acquire_pin(&engine, context)?;
        let result = self.read(context, &engine, Some(pin.handle()?), id, query, date, &metadata, mode)
            .and_then(project);
        match result {
            Ok(value) => { self.executor.release_pin(context, &mut pin, None)?; Ok(value) }
            Err(root) => match self.executor.release_pin(context, &mut pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(ErrorKind::Internal, "trade pin cleanup lost its failure cause")),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn many_at<T>(&self, entry: Instant, name: &'static str, ids: &[SecurityId], query: &TradeQuery,
        options: &OperationOptions, mode: ReadMode, project: fn(ReadResult) -> Result<T>) -> Result<BatchOutcome<T>> {
        let mut context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        if query.cursor().is_some() {
            return Err(Error::new(ErrorKind::InvalidArgument, "trade many queries do not accept continuation"));
        }
        query.validate()?;
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        checked_ids(ids)?;
        set_limits(&mut context, query, mode, ids.len())?;
        let date = resolve_date(&context, query.date())?;
        let metadata = self.capture_metadata(&context, ids)?;
        let engine = &self.engine;
        let mut pin = self.executor.acquire_pin(engine, &context)?;
        let result = (|| {
            let mut items = Vec::new(); let mut metas = Vec::new();
            items.try_reserve(ids.len()).map_err(allocation_error)?;
            metas.try_reserve(ids.len()).map_err(allocation_error)?;
            for (index, id) in ids.iter().enumerate() {
                context.check_boundary(TimeoutPhase::Admission)?;
                let result = self.read(&context, engine, Some(pin.handle()?), id, query, date, &metadata, mode)?;
                metas.push(result.page.meta().clone());
                items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(project(result)?)));
            }
            let meta = aggregate_meta(&context, &metas, Some(metadata.version()))?.with_trading_date(date);
            Ok(BatchOutcome::with_meta(items, meta))
        })();
        self.finish_pinned(&context, &mut pin, result)
    }

    #[allow(clippy::too_many_arguments)]
    fn read(&self, context: &OperationContext, engine: &Engine, pin: Option<&PinHandle>, id: &SecurityId,
        query: &TradeQuery, date: TradingDate, metadata: &MetadataSnapshot, mode: ReadMode) -> Result<ReadResult> {
        let size = page_size(query, mode);
        let mut offset = query.cursor().map_or(u32::from(query.start()), PageCursor::next_offset);
        let start = offset;
        let limits = CollectionLimits {
            pages: Some(if mode == ReadMode::Page { 1 } else { query.pages().max_pages() }),
            records: Some(if mode == ReadMode::Page { query.count() } else { query.pages().max_records() }),
        };
        let mut progress = PageProgress::default();
        let mut seen = HashSet::<Bytes>::new();
        let mut pages = Vec::<Vec<TradeTick>>::new();
        let mut metas = Vec::<ResultMeta>::new();
        let mut price_base = None;
        loop {
            context.check_boundary(TimeoutPhase::Admission)?;
            progress.before_request(limits)?;
            let remaining = limits.records.unwrap_or(size).saturating_sub(progress.records()).max(1);
            let count = size.min(remaining);
            let request = trade_request(id, query.date(), offset, count)?;
            let purpose = if progress.pages() == 0 { ChildPurpose::Required } else { ChildPurpose::Page };
            let raw = match pin {
                Some(pin) => self.executor.execute_pinned_raw(engine, pin, context, request, purpose, None)?,
                None => self.executor.execute_raw(engine, context, request, purpose, None)?,
            };
            // Diagnostic omission does not erase the private no-progress evidence.
            let page_bytes = raw.response.data.clone();
            let decoded = self.executor.decode_received(context, raw)?;
            let (wire_rows, base) = match &decoded.response {
                CommandResponse::TodayTicks(page) => (&page.ticks, None),
                CommandResponse::HistoricalTicks(page) => (&page.ticks, page.price_base_raw_f32),
                _ => return Err(Error::new(ErrorKind::Protocol, "trade request returned a different response kind")),
            };
            progress.accept(wire_rows.len(), count, limits)?;
            context.record_work(wire_rows.len(), 0)?;
            if base.is_some_and(|base| !base.is_finite()) {
                return Err(Error::new(ErrorKind::Protocol, "historical trade price base is not finite"));
            }
            if progress.pages() == 1 { price_base = base.map(|base| Price::with_value_and_provenance(
                NumericValue::F32(base), ValueProvenance::WireRule)); }
            let source = ServiceSource::from_reply(&decoded);
            let ranges = wire_rows.iter().map(|row| row.record_range).collect::<Vec<_>>();
            let meta = result_meta(context, &self.config, Some(metadata), &source, Some(decoded.payload.clone()),
                decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "trades.page").with_trading_date(date);
            let mut ticks = Vec::new();
            ticks.try_reserve(wire_rows.len()).map_err(allocation_error)?;
            for row in wire_rows { ticks.push(project_tick(row, meta.source().clone(), id)?); }
            let found = if mode == ReadMode::Opening {
                ticks.iter().find(|tick| tick.kind() == TradeEventKind::OpeningMatch).cloned()
            } else { None };
            let empty = ticks.is_empty();
            if !empty && mode != ReadMode::Page && !seen.insert(page_bytes) {
                return Err(Error::new(ErrorKind::Protocol, "trade pagination repeated a wire page"));
            }
            let next = advance_offset(offset, ticks.len())?;
            metas.try_reserve(1).map_err(allocation_error)?; metas.push(meta);
            pages.try_reserve(1).map_err(allocation_error)?; pages.push(ticks);
            if empty || mode == ReadMode::Page || found.is_some() {
                let end = if empty { EndEvidence::EmptyPage }
                    else if mode != ReadMode::Page || next > u32::from(u16::MAX) { EndEvidence::RequestedWindow }
                    else { EndEvidence::Unknown };
                let cursor = if end == EndEvidence::Unknown { Some(PageCursor::new(next, size,
                    fingerprint(id, query.date(), date), context.session_id, Some(source.host), Some(metadata.version()))) }
                    else { None };
                let page = finish_page(context, id, date, price_base, start, size, cursor, end,
                    pages, metas, progress.pages(), metadata.version(), mode != ReadMode::Page)?;
                context.check_boundary(TimeoutPhase::Aggregate)?;
                return Ok(ReadResult { page, opening: found });
            }
            if next > u32::from(u16::MAX) {
                return Err(Error::new(ErrorKind::LimitExceeded, "trade continuation exceeds u16 wire range before EOF"));
            }
            offset = next;
        }
    }

    fn capture_metadata(&self, context: &OperationContext, ids: &[SecurityId]) -> Result<Arc<MetadataSnapshot>> {
        super::capture_existing_metadata(context, &self.namespace, ids)
    }

    fn finish_pinned<T>(&self, context: &OperationContext, pin: &mut crate::session::PinnedSession, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => { self.executor.release_pin(context, pin, None)?; context.finish()?; Ok(value) }
            Err(root) => match self.executor.release_pin(context, pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(ErrorKind::Internal, "trade pin cleanup lost its failure cause")),
            },
        }
    }
}

fn rows(result: ReadResult) -> Result<TradePage> { Ok(result.page) }
fn columns(result: ReadResult) -> Result<TradeColumns> { TradeColumns::from_page(result.page) }
fn opening(result: ReadResult) -> Result<Option<TradeTick>> { Ok(result.opening) }
fn page_size(query: &TradeQuery, mode: ReadMode) -> usize {
    if mode == ReadMode::Page { query.count() } else { query.pages().page_size() }
}
fn set_limits(context: &mut OperationContext, query: &TradeQuery, mode: ReadMode, count: usize) -> Result<()> {
    let records = count.checked_mul(if mode == ReadMode::Page { query.count() } else { query.pages().max_records() })
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "trade batch record cap overflows"))?;
    let pages = if mode == ReadMode::Page { None } else {
        NonZeroUsize::new(count.checked_mul(query.pages().max_pages().saturating_sub(1))
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "trade batch page cap overflows"))?)
    };
    context.set_work_limits(WorkLimits { pages, records: Some(records), bytes: None })
}
fn resolve_date(context: &OperationContext, choice: MarketDateChoice) -> Result<TradingDate> {
    match choice { MarketDateChoice::Current => context.current_market_date(), MarketDateChoice::Date(date) => Ok(date) }
}
fn fingerprint(id: &SecurityId, choice: MarketDateChoice, date: TradingDate) -> u64 {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "hq:trades".hash(&mut state); id.hash(&mut state); choice.hash(&mut state); date.hash(&mut state); state.finish()
}
fn trade_request(id: &SecurityId, date: MarketDateChoice, offset: u32, count: usize) -> Result<CommandRequest> {
    let start = u16::try_from(offset).map_err(|_| Error::new(ErrorKind::LimitExceeded, "trade offset exceeds wire range"))?;
    let count = u16::try_from(count).map_err(|_| Error::new(ErrorKind::InvalidArgument, "trade count exceeds wire range"))?;
    let code = NormalizedCode::parse(&id.to_string()).map_err(Error::from_protocol)?;
    match date {
        MarketDateChoice::Current => TodayTicksRequest::new(code, start, count).map(CommandRequest::TodayTicks),
        MarketDateChoice::Date(date) => {
            let date = DateParts::new(i32::from(date.year()), date.month(), date.day()).map_err(Error::from_protocol)?;
            HistoricalTicksRequest::new(code, date, start, count).map(CommandRequest::HistoricalTicks)
        }
    }.map_err(Error::from_protocol)
}
fn project_tick(row: &WireTick, source: SourceStamp, id: &SecurityId) -> Result<TradeTick> {
    if row.time_minutes >= 1440 { return Err(Error::new(ErrorKind::Protocol, "trade minute is outside a day")); }
    let hour = (row.time_minutes / 60) as u8; let minute = (row.time_minutes % 60) as u8;
    let time = match row.trade_datetime {
        Some(value) => {
            let year = u16::try_from(value.date.year).map_err(|_| Error::new(ErrorKind::Protocol, "trade year exceeds SDK range"))?;
            let date = TradingDate::new(year, value.date.month, value.date.day).map_err(|error|
                Error::new(ErrorKind::Protocol, "invalid historical trade date").with_source(error))?;
            EventTime::local(date, hour, minute, 0)
        }
        None => EventTime::time_of_day(hour, minute, 0),
    }.map_err(|error| Error::new(ErrorKind::Protocol, "invalid trade time").with_source(error))?;
    let scale = match row.price_denominator { 100 => 2, 1000 => 3,
        _ => return Err(Error::new(ErrorKind::Protocol, "unknown trade price denominator")) };
    let quantity = |value| Quantity::with_provenance(NumericValue::ScaledInteger { value, scale: 0 },
        QuantityUnit::Unknown, ValueProvenance::WireRule);
    // The ordinary A-share contract explicitly uses hands. It does not prove
    // units for bonds, indices or the mixed parser's status-8 quote quantities.
    let code = NormalizedCode::parse(&id.to_string()).map_err(Error::from_protocol)?;
    let equity = eltdx_protocol::commands::security::classify_security(code.market(), code.number())
        == eltdx_protocol::commands::security::SecurityCategory::AShare;
    let volume = if equity && row.event_kind != WireKind::AuctionSnapshot {
        Quantity::with_provenance(NumericValue::ScaledInteger { value: row.volume, scale: 0 },
            QuantityUnit::Lots, ValueProvenance::PrefixFallback)
    } else { quantity(row.volume) };
    Ok(TradeTick { index: u32::from(row.index), absolute_index: row.absolute_index, time,
        price: Price::with_value_and_provenance(NumericValue::ScaledInteger { value: row.price_acc_raw, scale }, ValueProvenance::WireRule),
        volume, order_count: row.order_count, status_raw: row.status_raw,
        side: match &row.side { WireSide::Buy => TradeSide::Buy, WireSide::Sell => TradeSide::Sell,
            WireSide::Neutral => TradeSide::Neutral, WireSide::Status(value) => TradeSide::Status(*value) },
        kind: match row.event_kind { WireKind::Trade => TradeEventKind::Trade,
            WireKind::OpeningMatch => TradeEventKind::OpeningMatch, WireKind::AuctionSnapshot => TradeEventKind::AuctionSnapshot },
        auction_matched: row.auction_matched_volume.map(quantity), auction_unmatched_signed: row.auction_unmatched_signed_volume,
        unknown_tail: row.unknown_tail_raw, price_delta_raw: row.price_delta_raw, price_acc_raw: row.price_acc_raw,
        price_denominator: row.price_denominator, reserved_zero: row.reserved_zero, source })
}

#[allow(clippy::too_many_arguments)]
fn finish_page(context: &OperationContext, id: &SecurityId, date: TradingDate, base: Option<Price>,
    start: u32, size: usize, cursor: Option<PageCursor>, end: EndEvidence, mut pages: Vec<Vec<TradeTick>>,
    metas: Vec<ResultMeta>, pages_read: usize, version: u64, reverse: bool) -> Result<TradePage> {
    let total = pages.iter().try_fold(0_usize, |total, page| total.checked_add(page.len()))
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "trade aggregate count overflows"))?;
    let mut ticks = Vec::new(); let mut order = Vec::new();
    ticks.try_reserve(total).map_err(allocation_error)?; order.try_reserve(total).map_err(allocation_error)?;
    if reverse { pages.reverse(); }
    let mut source_row = if reverse { total } else { 0 };
    for page in pages {
        if reverse { source_row -= page.len(); }
        order.extend(source_row..source_row + page.len());
        if !reverse { source_row += page.len(); }
        ticks.extend(page);
    }
    let meta = if metas.len() == 1 { metas.into_iter().next().expect("one trade response") }
        else { aggregate_meta(context, &metas, Some(version))? };
    let meta = meta.with_raw_row_order(&order)?.with_trading_date(date);
    Ok(TradePage::new(id.clone(), Some(date), base, DataPage::new(ticks, u64::from(start), size, cursor, end, meta), pages_read))
}
