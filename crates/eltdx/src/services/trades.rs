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

#[derive(Clone, Copy)]
enum TradeReadPolicy {
    Strict { limits: CollectionLimits, single: bool },
    Legacy { maximum_pages: Option<usize>, single: bool },
}

/// Both language surfaces advance by actual rows, require an empty terminal
/// page for collection, and stop as soon as a formal opening match is found.
struct TradePager {
    offset: u32,
    size: usize,
    progress: PageProgress,
    seen: HashSet<Bytes>,
    policy: TradeReadPolicy,
}

impl TradePager {
    fn new(start: u32, size: usize, policy: TradeReadPolicy) -> Self {
        Self { offset: start, size, progress: PageProgress::default(), seen: HashSet::new(), policy }
    }

    fn prepare(&self, context: &OperationContext) -> Result<(u32, usize, ChildPurpose)> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let count = match self.policy {
            TradeReadPolicy::Strict { limits, .. } => {
                self.progress.before_request(limits)?;
                self.size.min(limits.records.unwrap_or(self.size).saturating_sub(self.progress.records()).max(1))
            }
            TradeReadPolicy::Legacy { .. } => {
                if self.offset > u32::from(u16::MAX) {
                    return Err(Error::new(ErrorKind::InvalidArgument, "start must be between 0 and 65535")
                        .with_context("name", "start"));
                }
                self.size
            }
        };
        Ok((self.offset, count, if self.progress.pages() == 0 { ChildPurpose::Required } else { ChildPurpose::Page }))
    }

    fn accept(&mut self, context: &OperationContext, count: usize, requested: usize) -> Result<()> {
        match self.policy {
            TradeReadPolicy::Strict { limits, .. } => self.progress.accept(count, requested, limits)?,
            TradeReadPolicy::Legacy { .. } => self.progress.accept_legacy(count)?,
        }
        context.record_work(count, 0)
    }

    fn advance(&mut self, count: usize, evidence: Bytes, found: bool) -> Result<bool> {
        match self.policy {
            TradeReadPolicy::Strict { single, .. } => {
                if count != 0 && !single && !self.seen.insert(evidence) {
                    return Err(Error::new(ErrorKind::Protocol, "trade pagination repeated a wire page"));
                }
                self.offset = advance_offset(self.offset, count)?;
                if count == 0 || single || found { return Ok(true); }
                if self.offset > u32::from(u16::MAX) {
                    return Err(Error::new(ErrorKind::LimitExceeded, "trade continuation exceeds u16 wire range before EOF"));
                }
            }
            TradeReadPolicy::Legacy { maximum_pages, single } => {
                if count == 0 || single || found { return Ok(true); }
                if maximum_pages.is_some_and(|maximum| self.progress.pages() >= maximum) {
                    return Err(Error::new(ErrorKind::LimitExceeded,
                        "trade pagination reached max_pages before an empty page")
                        .with_context("python_kind", "RuntimeError"));
                }
                self.offset = advance_offset(self.offset, count)?;
            }
        }
        Ok(false)
    }
}

/// One wire-page boundary for row, column and legacy consumers. Higher-level
/// collectors choose their own page limits and projection policies.
fn fetch_trade_reply(
    context: &OperationContext, request: CommandRequest, purpose: ChildPurpose,
    fetch: impl FnOnce(CommandRequest, ChildPurpose) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<crate::hq::decode::DecodedReply> {
    let historical = match &request {
        CommandRequest::TodayTicks(_) => false,
        CommandRequest::HistoricalTicks(_) => true,
        _ => return Err(Error::new(ErrorKind::InvalidArgument, "trade service requires today or historical ticks")),
    };
    context.check_boundary(TimeoutPhase::Admission)?;
    let decoded = fetch(request, purpose)?;
    if !matches!((historical, &decoded.response),
        (false, CommandResponse::TodayTicks(_)) | (true, CommandResponse::HistoricalTicks(_))) {
        return Err(Error::new(ErrorKind::Protocol, "trade request returned a different response kind"));
    }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(decoded)
}

pub(crate) fn legacy_trade_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnOnce(CommandRequest, ChildPurpose) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(ErrorKind::InvalidArgument, "legacy trades require a compatibility operation"));
    }
    Ok(fetch_trade_reply(context, request, ChildPurpose::Required, fetch)?.response)
}

pub(crate) fn legacy_trade_read(
    context: &OperationContext, request: CommandRequest, collect: bool, opening: bool,
    maximum_pages: Option<usize>,
    mut fetch: impl FnMut(CommandRequest, ChildPurpose) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<crate::__python_compat::LegacyTradeRead> {
    use crate::__python_compat::LegacyTradeRead;
    if !context.is_legacy() { return Err(Error::new(ErrorKind::InvalidArgument, "legacy trade read requires compatibility profile")); }
    let (start, size) = match &request {
        CommandRequest::TodayTicks(query) => (query.start, query.count),
        CommandRequest::HistoricalTicks(query) => (query.start, query.count),
        _ => return Err(Error::new(ErrorKind::InvalidArgument, "trade read requires today or historical ticks")),
    };
    let mut pager = TradePager::new(u32::from(start), usize::from(size),
        TradeReadPolicy::Legacy { maximum_pages, single: !collect && !opening });
    let mut first = None;
    let mut pages = Vec::new();
    let mut page_lengths = Vec::new();
    loop {
        let (offset, count, purpose) = pager.prepare(context)?;
        let mut query = request.clone();
        match &mut query {
            CommandRequest::TodayTicks(query) => { query.start = offset as u16; query.count = count as u16; }
            CommandRequest::HistoricalTicks(query) => { query.start = offset as u16; query.count = count as u16; }
            _ => unreachable!("trade request validated above"),
        }
        let mut decoded = fetch_trade_reply(context, query, purpose, &mut fetch)?;
        let ticks = match &mut decoded.response {
            CommandResponse::TodayTicks(page) => std::mem::take(&mut page.ticks),
            CommandResponse::HistoricalTicks(page) => std::mem::take(&mut page.ticks),
            _ => return Err(Error::new(ErrorKind::Protocol, "trade page response kind changed")),
        };
        pager.accept(context, ticks.len(), count)?;
        let found = opening.then(|| ticks.iter().find(|row| row.event_kind == WireKind::OpeningMatch).cloned()).flatten();
        if found.is_some() { return Ok(LegacyTradeRead { response: decoded.response, request_count: count,
            page_lengths: Vec::new(), opening: found, opening_only: true }); }
        let done = pager.advance(ticks.len(), Bytes::new(), false)?;
        // Searching only needs the current page. Do not retain every earlier
        // tick while waiting for the first formal match or terminal empty page.
        if opening {
            if done {
                return Ok(LegacyTradeRead { response: decoded.response, request_count: count, page_lengths: Vec::new(),
                    opening: None, opening_only: true });
            }
            continue;
        }
        pages.try_reserve(1).map_err(allocation_error)?;
        page_lengths.try_reserve(1).map_err(allocation_error)?;
        page_lengths.push(ticks.len());
        pages.push(ticks);
        if first.is_none() { first = Some(decoded.response); }
        if done { break; }
    }
    let mut response = first.ok_or_else(|| Error::new(ErrorKind::Internal, "trade read has no first response"))?;
    let (ticks, _) = order_trade_pages(pages, collect)?;
    if collect { page_lengths.reverse(); }
    let request_count = if collect { ticks.len() } else { usize::from(size) };
    match &mut response {
        CommandResponse::TodayTicks(page) => {
            page.ticks = ticks;
            if collect { page.request.start = 0; page.raw_payload = Bytes::new(); }
        }
        CommandResponse::HistoricalTicks(page) => {
            page.ticks = ticks;
            if collect { page.request.start = 0; page.raw_payload = Bytes::new(); }
        }
        _ => return Err(Error::new(ErrorKind::Internal, "trade read has a nontrade first response")),
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(LegacyTradeRead { response, request_count, page_lengths, opening: None, opening_only: opening })
}

fn order_trade_pages<T>(mut pages: Vec<Vec<T>>, reverse: bool) -> Result<(Vec<T>, Vec<usize>)> {
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
    Ok((ticks, order))
}

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
        let start = query.cursor().map_or(u32::from(query.start()), PageCursor::next_offset);
        let limits = CollectionLimits {
            pages: Some(if mode == ReadMode::Page { 1 } else { query.pages().max_pages() }),
            records: Some(if mode == ReadMode::Page { query.count() } else { query.pages().max_records() }),
        };
        let mut pager = TradePager::new(start, size, TradeReadPolicy::Strict { limits, single: mode == ReadMode::Page });
        let mut pages = Vec::<Vec<TradeTick>>::new();
        let mut metas = Vec::<ResultMeta>::new();
        let mut price_base = None;
        loop {
            let (offset, count, purpose) = pager.prepare(context)?;
            let request = trade_request(id, query.date(), offset, count)?;
            let mut page_bytes = Bytes::new();
            let decoded = fetch_trade_reply(context, request, purpose, |request, purpose| {
                let raw = match pin {
                    Some(pin) => self.executor.execute_pinned_raw(engine, pin, context, request, purpose, None)?,
                    None => self.executor.execute_raw(engine, context, request, purpose, None)?,
                };
                page_bytes = raw.response.data.clone();
                self.executor.decode_received(context, raw)
            })?;
            // Diagnostic omission does not erase the private no-progress evidence.
            let (wire_rows, base) = match &decoded.response {
                CommandResponse::TodayTicks(page) => (&page.ticks, None),
                CommandResponse::HistoricalTicks(page) => (&page.ticks, page.price_base_raw_f32),
                _ => return Err(Error::new(ErrorKind::Protocol, "trade request returned a different response kind")),
            };
            pager.accept(context, wire_rows.len(), count)?;
            if base.is_some_and(|base| !base.is_finite()) {
                return Err(Error::new(ErrorKind::Protocol, "historical trade price base is not finite"));
            }
            if pager.progress.pages() == 1 { price_base = base.map(|base| Price::with_value_and_provenance(
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
            let done = pager.advance(ticks.len(), page_bytes, found.is_some())?;
            let next = pager.offset;
            metas.try_reserve(1).map_err(allocation_error)?; metas.push(meta);
            pages.try_reserve(1).map_err(allocation_error)?; pages.push(ticks);
            if done {
                let end = if empty { EndEvidence::EmptyPage }
                    else if mode != ReadMode::Page || next > u32::from(u16::MAX) { EndEvidence::RequestedWindow }
                    else { EndEvidence::Unknown };
                let cursor = if end == EndEvidence::Unknown { Some(PageCursor::new(next, size,
                    fingerprint(id, query.date(), date), context.session_id, Some(source.host), Some(metadata.version()))) }
                    else { None };
                let page = finish_page(context, id, date, price_base, start, size, cursor, end,
                    pages, metas, pager.progress.pages(), metadata.version(), mode != ReadMode::Page)?;
                context.check_boundary(TimeoutPhase::Aggregate)?;
                return Ok(ReadResult { page, opening: found });
            }
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
    start: u32, size: usize, cursor: Option<PageCursor>, end: EndEvidence, pages: Vec<Vec<TradeTick>>,
    metas: Vec<ResultMeta>, pages_read: usize, version: u64, reverse: bool) -> Result<TradePage> {
    let (ticks, order) = order_trade_pages(pages, reverse)?;
    let meta = if metas.len() == 1 { metas.into_iter().next().expect("one trade response") }
        else { aggregate_meta(context, &metas, Some(version))? };
    let meta = meta.with_raw_row_order(&order)?.with_trading_date(date);
    Ok(TradePage::new(id.clone(), Some(date), base, DataPage::new(ticks, u64::from(start), size, cursor, end, meta), pages_read))
}

#[cfg(test)]
mod legacy_pager_tests {
    use super::*;

    #[test]
    fn page_reversal_preserves_equal_time_wire_rows_and_raw_indices() {
        let pages = vec![vec!["newer-a", "newer-b"], vec!["older-a", "older-b"], vec![]];
        let (rows, indices) = order_trade_pages(pages.clone(), true).expect("reverse pages");
        assert_eq!(rows, ["older-a", "older-b", "newer-a", "newer-b"]);
        assert_eq!(indices, [2, 3, 0, 1]);
        let (rows, indices) = order_trade_pages(pages, false).expect("single-page order");
        assert_eq!(rows, ["newer-a", "newer-b", "older-a", "older-b"]);
        assert_eq!(indices, [0, 1, 2, 3]);
    }

    #[test]
    fn legacy_allows_repeated_pages_and_returns_empty_or_match_before_cap() {
        let mut pager = TradePager::new(0, 3, TradeReadPolicy::Legacy {
            maximum_pages: Some(2), single: false,
        });
        pager.progress.accept_legacy(1).expect("first short page");
        assert!(!pager.advance(1, Bytes::from_static(b"same"), false).expect("continue"));
        pager.progress.accept_legacy(1).expect("second short page");
        assert!(pager.advance(1, Bytes::from_static(b"same"), true).expect("match wins"));

        let mut empty = TradePager::new(0, 3, TradeReadPolicy::Legacy {
            maximum_pages: Some(1), single: false,
        });
        empty.progress.accept_legacy(0).expect("terminal empty");
        assert!(empty.advance(0, Bytes::new(), false).expect("EOF wins"));

        let mut repeated = TradePager::new(0, 3, TradeReadPolicy::Legacy {
            maximum_pages: Some(2), single: false,
        });
        repeated.progress.accept_legacy(1).expect("first page");
        assert!(!repeated.advance(1, Bytes::from_static(b"same"), false).unwrap());
        repeated.progress.accept_legacy(1).expect("second page");
        let error = repeated.advance(1, Bytes::from_static(b"same"), false).expect_err("original page cap");
        assert_eq!(error.message(), "trade pagination reached max_pages before an empty page");
        assert!(error.context().iter().any(|(key, value)| key == "python_kind" && value == "RuntimeError"));
    }

    #[test]
    fn strict_repetition_precedes_opening_match_and_offset_range_is_retained() {
        let limits = CollectionLimits { pages: Some(4), records: Some(10) };
        let mut strict = TradePager::new(0, 3, TradeReadPolicy::Strict { limits, single: false });
        assert!(!strict.advance(1, Bytes::from_static(b"same"), false).unwrap());
        assert_eq!(strict.advance(1, Bytes::from_static(b"same"), true).expect_err("duplicate evidence").kind(),
            ErrorKind::Protocol);
        let mut boundary = TradePager::new(65535, 1, TradeReadPolicy::Strict { limits, single: false });
        assert_eq!(boundary.advance(1, Bytes::from_static(b"last"), false).expect_err("wire range").kind(),
            ErrorKind::LimitExceeded);
        let mut single = TradePager::new(65535, 1, TradeReadPolicy::Strict { limits, single: true });
        assert!(single.advance(1, Bytes::from_static(b"last"), false).unwrap());
        assert_eq!(single.offset, 65536);
    }
}
