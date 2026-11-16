use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use eltdx_protocol::commands::quotes::{CategoryQuotesRequest, LegacyQuotesRequest, QuoteSnapshot,
    RefreshCursor, RefreshStreamRequest, SnapshotsRequest};
use eltdx_protocol::unit::NormalizedCode;
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, RecoveryLimit, WorkLimits};
use crate::types::quotes::{cny_hundredths, parse_identity, percentage_points,
    price_from_quote_delta, price_from_raw, quote_quantity, unknown_amount, unknown_ratio};
use crate::types::{Amount, AmountUnit, BatchItem, BatchItemState, BatchOutcome, CategoryQuery,
    CategoryQuote, DataPage, EndEvidence, HqMarket, LegacyQuote, MissingPolicy, MissingReason,
    NumericValue, PageCursor, QuoteLevel, QuoteUpdate, QuoteUpdates, RefreshRequest, ResultMeta, SecurityId, Snapshot,
    SnapshotBatch, ValueProvenance};

use super::{aggregate_meta, allocation_error, checked_ids, endpoint_engine, identity_from_market,
    request_metadata, result_meta, verify_cursor, ServiceSource};

const SNAPSHOT_CHUNK_SIZE: usize = 300;
const LEGACY_CHUNK_SIZE: usize = 300;
const SNAPSHOT_RECOVERY: RecoveryLimit = RecoveryLimit { name: "snapshot_missing", maximum: 32 };
pub(crate) const SNAPSHOT_RECOVERY_REQUESTS: usize = SNAPSHOT_RECOVERY.maximum;

#[derive(Clone, Debug)]
pub struct QuoteService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: String,
}

/// A quote endpoint that cannot outlive or clone the fixed-connection guard.
pub struct PinnedQuoteService<'a> {
    pin: &'a crate::PinnedSession,
}

impl<'a> PinnedQuoteService<'a> {
    pub(crate) fn new(pin: &'a crate::PinnedSession) -> Self { Self { pin } }

    pub fn refresh(&self, requests: &[RefreshRequest], options: &OperationOptions) -> Result<QuoteUpdates> {
        let entry = Instant::now();
        let source = self.pin.quote_source()?;
        let executor = HqExecutor::new(Arc::clone(self.pin.owner()));
        let context = executor.begin_context_at(entry, "quotes_refresh", &source.config, options)?;
        // Build and validate the actual request before waiting on serialization.
        // Empty is still sent once; it polls the previous activation.
        let request = refresh_command(requests)?;
        let mut activation = loop {
            context.check_boundary(TimeoutPhase::Admission)?;
            match source.activation.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::park_timeout(eltdx_runtime::engine::SIGNAL_POLL_INTERVAL);
                }
                Err(std::sync::TryLockError::Poisoned(_)) => return Err(Error::new(ErrorKind::Internal, "pin activation state is poisoned")),
            }
        };
        let previous = activation.as_ref().ok_or_else(|| Error::new(ErrorKind::PinLost, "refresh activation failed; acquire a new pin"))?;
        let effective = if requests.is_empty() { previous.as_slice() } else { requests };
        let parse_request = refresh_command(effective)?;
        let ids = effective.iter().map(|item| item.id().clone()).collect::<Vec<_>>();
        let next = if requests.is_empty() { None } else { Some(requests.to_vec()) };
        let metadata = executor.prepare_pinned_metadata(&source.engine, self.pin.handle()?, &context,
            &source.namespace, std::slice::from_ref(&parse_request))?;
        let result = (|| {
            let mut raw = executor.execute_pinned_raw(&source.engine, self.pin.handle()?, &context,
                request.clone(), ChildPurpose::Required, None)?;
            // Empty wire polling still needs the previous activation identities
            // to split multirow XOR payloads. Preserve the actual sent command.
            raw.request = parse_request;
            let mut decoded = executor.decode_received(&context, raw)?;
            decoded.request = request;
            let result = project_refresh_updates(&ids, &context, &source.config, &metadata, &decoded, "quotes.refresh")?;
            context.finish()?;
            Ok(result)
        })();
        match result {
            Ok(result) => {
                if let Some(next) = next { *activation = Some(next); }
                Ok(result)
            }
            Err(error) => {
                // A failed stateful call cannot safely restore the previous
                // activation. Owner registration retains retirement responsibility.
                *activation = None;
                let _ = self.pin.handle()?.begin_close();
                Err(error)
            }
        }
    }
}

impl QuoteService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn snapshots(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<SnapshotBatch> {
        let entry = Instant::now();
        self.snapshots_at(entry, ids, MissingPolicy::Error, options)
    }

    pub fn snapshots_with_policy(&self, ids: &[SecurityId], policy: MissingPolicy,
        options: &OperationOptions) -> Result<SnapshotBatch> {
        let entry = Instant::now();
        self.snapshots_at(entry, ids, policy, options)
    }

    fn snapshots_at(&self, entry: Instant, ids: &[SecurityId], policy: MissingPolicy,
        options: &OperationOptions) -> Result<SnapshotBatch> {
        let mut context = self.executor.begin_context_at(entry, "quotes_snapshots", &self.config, options)?;
        let normalized = checked_ids(ids)?;
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        context.set_work_limits(WorkLimits {
            pages: std::num::NonZeroUsize::new(ids.len().div_ceil(SNAPSHOT_CHUNK_SIZE)),
            records: Some(ids.len()), bytes: None,
        })?;
        let requests = snapshot_requests(&normalized, SNAPSHOT_CHUNK_SIZE)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let raw = self.snapshot_rows_in_context(&context, &normalized, SNAPSHOT_CHUNK_SIZE)?;
        let result = project_snapshot_batch(ids, policy, &context, &self.config, &metadata, raw)?;
        context.finish()?;
        Ok(result)
    }

    pub fn subscribe(&self, ids: &[SecurityId], subscription: &crate::SubscriptionOptions,
        options: &OperationOptions) -> Result<crate::Subscription> {
        let entry = Instant::now();
        crate::subscription::establish(entry, self.executor.clone(), self.engine.clone(),
            self.config.clone(), &self.namespace, ids, subscription, options)
    }

    /// Language-neutral retrieval. T12 passes a Python-profile context and the
    /// original normalized request without applying Rust's aggregate caps.
    /// The returned records preserve every original wire scalar for the ABI.
    pub(crate) fn snapshot_rows_in_context(&self, context: &OperationContext,
        codes: &[NormalizedCode], chunk_size: usize) -> Result<RawSnapshotBatch> {
        self.retrieve_snapshot_rows(context, codes, chunk_size, |part, purpose, recovery| {
            context.check_boundary(TimeoutPhase::Admission)?;
            let request = CommandRequest::Snapshots(SnapshotsRequest::new(part.to_vec()).map_err(Error::from_protocol)?);
            let raw = self.executor.execute_raw(&self.engine, context, request, purpose, recovery)?;
            self.executor.decode_received(context, raw)
        })
    }

    /// Freeze precision for the entire composition before any child decodes.
    pub(crate) fn prepare_in_context(&self, context: &OperationContext, ids: &[SecurityId])
        -> Result<Arc<MetadataSnapshot>> {
        let codes = checked_ids(ids)?;
        let requests = snapshot_requests(&codes, 80)?;
        request_metadata(&self.executor, &self.engine, context, &self.config, &self.namespace, &requests)
    }

    pub(crate) fn snapshots_in_context(&self, context: &OperationContext, ids: &[SecurityId],
        policy: MissingPolicy, chunk_size: usize) -> Result<SnapshotBatch> {
        let codes = checked_ids(ids)?;
        if ids.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        let requests = snapshot_requests(&codes, chunk_size)?;
        let metadata = request_metadata(&self.executor, &self.engine, context, &self.config, &self.namespace, &requests)?;
        let raw = self.snapshot_rows_in_context(context, &codes, chunk_size)?;
        project_snapshot_batch(ids, policy, context, &self.config, &metadata, raw)
    }

    pub(crate) fn retrieve_snapshot_rows(
        &self, context: &OperationContext, codes: &[NormalizedCode], chunk_size: usize,
        execute: impl FnMut(&[NormalizedCode], ChildPurpose, Option<RecoveryLimit>) -> Result<DecodedReply>,
    ) -> Result<RawSnapshotBatch> {
        if chunk_size == 0 { return Err(Error::new(ErrorKind::InvalidArgument, "snapshot chunk size must be positive")); }
        retrieve_snapshot_rows(context, codes, chunk_size, execute)
    }

    pub fn legacy(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<LegacyQuote>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "quotes_legacy", &self.config, options)?;
        checked_ids(ids)?;
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        context.set_work_limits(WorkLimits {
            pages: std::num::NonZeroUsize::new(ids.len().div_ceil(LEGACY_CHUNK_SIZE)),
            records: Some(ids.len()), bytes: None,
        })?;
        let batch = self.legacy_in_context(&context, ids)?;
        context.finish()?;
        Ok(batch)
    }

    pub(crate) fn legacy_in_context(&self, context: &OperationContext, ids: &[SecurityId])
        -> Result<BatchOutcome<LegacyQuote>> {
        let codes = checked_ids(ids)?;
        if ids.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        let requests = legacy_requests(&codes)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut items = Vec::new();
        items.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut metas = Vec::new();
        metas.try_reserve(requests.len()).map_err(allocation_error)?;
        for (chunk_index, request) in requests.into_iter().enumerate() {
            let start = chunk_index * LEGACY_CHUNK_SIZE;
            let end = (start + LEGACY_CHUNK_SIZE).min(ids.len());
            let decoded = self.executor.execute_decoded(&self.engine, &context, &self.config, &self.namespace,
                request, if chunk_index == 0 { ChildPurpose::Required } else { ChildPurpose::Page }, None)?;
            let CommandResponse::LegacyQuotes(rows) = &decoded.response else { return Err(wrong_response("legacy quotes")); };
            let order = occurrence_order(&ids[start..end], rows.iter().map(|row| identity_from_market(row.market, &row.code)))?;
            for (position, row_index) in order.into_iter().enumerate() {
                let row = &rows[row_index];
                let id = ids[start + position].clone();
                let quote = project_legacy(row, id.clone(), &context, &self.config, &metadata, &decoded, row.record_range)?;
                items.push(BatchItem::new(start + position, id, BatchItemState::Present(quote)));
            }
            context.record_work(rows.len(), 0)?;
            metas.push(result_meta(&context, &self.config, Some(&metadata), &ServiceSource::from_reply(&decoded),
                Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()),
                &rows.iter().map(|row| row.record_range).collect::<Vec<_>>(), "quotes.legacy"));
        }
        let meta = aggregate_meta(&context, &metas, Some(metadata.version()))?;
        Ok(BatchOutcome::with_meta(items, meta))
    }

    /* snapshot projection is shared below with source-bound subscription recovery. */
}

fn project_snapshot_batch(ids: &[SecurityId], policy: MissingPolicy, context: &OperationContext,
    config: &ClientConfig, metadata: &MetadataSnapshot, raw: RawSnapshotBatch) -> Result<SnapshotBatch> {
        let mut items = Vec::new();
        items.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut missing = Vec::new();
        for (index, (id, slot)) in ids.iter().zip(raw.slots).enumerate() {
            let state = match slot {
                RawSnapshotSlot::Present { row, reply } => BatchItemState::Present(project_snapshot(
                    &row, id.clone(), context, config, metadata, &reply, row.record_range)?),
                RawSnapshotSlot::Missing => {
                    missing.try_reserve(1).map_err(allocation_error)?;
                    missing.push((index, id.clone(), MissingReason::NotFound));
                    BatchItemState::ConfirmedMissing(MissingReason::NotFound)
                }
            };
            items.push(BatchItem::new(index, id.clone(), state));
        }
        missing_policy_error(&missing, policy)?;
        let metas = raw.replies.iter().map(|reply| snapshot_page_meta(context, config, metadata, reply)).collect::<Vec<_>>();
        let meta = aggregate_meta(context, &metas, Some(metadata.version()))?;
        Ok(BatchOutcome::with_meta(items, meta))
}

pub(crate) fn pinned_snapshot_batch(
    executor: &HqExecutor, engine: &Engine, pin: &eltdx_runtime::engine::PinHandle,
    context: &OperationContext, config: &ClientConfig, metadata: &MetadataSnapshot,
    ids: &[SecurityId], source: eltdx_runtime::slot::GenerationIdentity,
) -> Result<(SnapshotBatch, Vec<(SecurityId, u64)>)> {
    let codes = checked_ids(ids)?;
    let raw = retrieve_snapshot_rows(context, &codes, 100, |part, purpose, recovery| {
        let request = CommandRequest::Snapshots(SnapshotsRequest::new(part.to_vec()).map_err(Error::from_protocol)?);
        let reply = executor.execute_pinned_raw(engine, pin, context, request, purpose, recovery)?;
        if reply.engine_epoch != source.engine_epoch || reply.slot_id != source.slot_id || reply.generation != source.generation {
            return Err(Error::new(ErrorKind::PinLost, "snapshot source changed after subscription activation"));
        }
        executor.decode_received(context, reply)
    })?;
    // Recovery may return each security on a different response. A global
    // maximum would hide a push received after an earlier security's snapshot.
    let mut floors = Vec::new(); floors.try_reserve(raw.slots.len()).map_err(allocation_error)?;
    for (id, slot) in ids.iter().zip(&raw.slots) {
        if let RawSnapshotSlot::Present { reply, .. } = slot { floors.push((id.clone(), reply.source.receive_sequence)); }
    }
    Ok((project_snapshot_batch(ids, MissingPolicy::AllowPartial, context, config, metadata, raw)?, floors))
}

impl QuoteService {
    pub fn category(&self, query: &CategoryQuery, options: &OperationOptions) -> Result<DataPage<CategoryQuote>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "quotes_category", &self.config, options)?;
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(1),
            records: Some(query.count()), bytes: None })?;
        let page = self.category_in_context(&context, query, None, ChildPurpose::Required)?;
        context.finish()?;
        Ok(page)
    }

    /// Reuses the outer deadline, metadata and counters. A collector binds all
    /// later pages to the first successful endpoint, without a new public call.
    pub(crate) fn category_in_context(&self, context: &OperationContext, query: &CategoryQuery,
        endpoint: Option<&str>, purpose: ChildPurpose) -> Result<DataPage<CategoryQuote>> {
        if !(1..=80).contains(&query.count()) { return Err(Error::new(ErrorKind::InvalidArgument, "category count must be between 1 and 80")); }
        let fingerprint = category_fingerprint(query);
        let markets = [HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing];
        let version = self.executor.owner().metadata().catalog_version(&self.namespace, &markets)?;
        verify_cursor(query.cursor(), fingerprint, query.count(), &context, version)?;
        let end_offset = u32::from(query.start()).checked_add(u32::try_from(query.count()).map_err(|_| offset_overflow())?)
            .ok_or_else(offset_overflow)?;
        if end_offset > u32::from(u16::MAX) + 1 { return Err(offset_overflow()); }
        let engine = endpoint_engine(&self.executor, &self.config, &self.engine,
            query.cursor().and_then(PageCursor::endpoint).or(endpoint))?;
        let request = CommandRequest::CategoryQuotes(CategoryQuotesRequest::new(query.category().raw(), query.sort().raw(),
            query.start(), query.count() as u16, query.ascending(), None, 0));
        let metadata = request_metadata(&self.executor, &engine, &context, &self.config, &self.namespace, std::slice::from_ref(&request))?;
        if let Some(cursor) = query.cursor() {
            if cursor.metadata_version() != Some(metadata.version()) { return Err(Error::new(ErrorKind::InvalidArgument, "category metadata changed before continuation")); }
        }
        let raw = self.executor.execute_raw(&engine, context, request, purpose, None)?;
        let decoded = self.executor.decode_received(&context, raw)?;
        let CommandResponse::CategoryQuotes(page) = &decoded.response else { return Err(wrong_response("category quotes")); };
        if page.records.len() > query.count() { return Err(Error::new(ErrorKind::Protocol, "category response exceeds requested count")); }
        let mut items = Vec::new();
        items.try_reserve(page.records.len()).map_err(allocation_error)?;
        for row in &page.records {
            items.push(project_category(row, &context, &self.config, &metadata, &decoded, row.record_range)?);
        }
        context.record_work(items.len(), 0)?;
        let source = ServiceSource::from_reply(&decoded);
        let meta = result_meta(&context, &self.config, Some(&metadata), &source, Some(decoded.payload.clone()),
            decoded.wire.as_ref().map(|wire| wire.raw.clone()),
            &page.records.iter().map(|row| row.record_range).collect::<Vec<_>>(), "quotes.category");
        let (end, cursor) = if items.is_empty() { (EndEvidence::EmptyPage, None) } else {
            let next = u32::from(query.start()).checked_add(u32::try_from(items.len()).map_err(|_| offset_overflow())?).ok_or_else(offset_overflow)?;
            let next_window_end = next.checked_add(u32::try_from(query.count()).map_err(|_| offset_overflow())?)
                .ok_or_else(offset_overflow)?;
            if next > u32::from(u16::MAX) || next_window_end > u32::from(u16::MAX) + 1 {
                (EndEvidence::RequestedWindow, None)
            } else {
                (EndEvidence::Unknown, Some(PageCursor::new(next, query.count(), fingerprint, context.session_id,
                    Some(source.host), Some(metadata.version()))))
            }
        };
        Ok(DataPage::new(items, u64::from(query.start()), query.count(), cursor, end, meta))
    }

    pub fn depth(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<QuoteUpdates> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "quotes_depth", &self.config, options)?;
        let codes = checked_ids(ids)?;
        if ids.is_empty() { context.finish()?; return Ok(QuoteUpdates::new(Vec::new(), Vec::new(), ResultMeta::default())); }
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(codes.len().div_ceil(100)),
            records: Some(codes.len()), bytes: None })?;
        let result = self.depth_in_context(&context, ids, ChildPurpose::Required)?;
        context.finish()?;
        Ok(result)
    }

    pub(crate) fn depth_in_context(&self, context: &OperationContext, ids: &[SecurityId],
        first_purpose: ChildPurpose) -> Result<QuoteUpdates> {
        let codes = checked_ids(ids)?;
        if ids.is_empty() { return Ok(QuoteUpdates::new(Vec::new(), Vec::new(), ResultMeta::default())); }
        let requests = refresh_requests(&codes)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut pin = self.executor.acquire_pin(&self.engine, &context)?;
        let result = (|| {
            let mut updates = Vec::new();
            updates.try_reserve(ids.len()).map_err(allocation_error)?;
            let mut metas = Vec::new();
            metas.try_reserve(requests.len()).map_err(allocation_error)?;
            for (index, (chunk, request)) in ids.chunks(100).zip(requests).enumerate() {
                let purpose = if index == 0 { first_purpose } else { ChildPurpose::Page };
                let raw = self.executor.execute_pinned_raw(&self.engine, pin.handle()?, &context, request, purpose, None)?;
                let decoded = self.executor.decode_received(&context, raw)?;
                let page = project_refresh_updates(chunk, &context, &self.config, &metadata, &decoded, "quotes.depth")?;
                let (_, rows, meta) = page.into_parts();
                updates.extend(rows);
                metas.push(meta);
            }
            let mut requested = Vec::new();
            requested.try_reserve(ids.len()).map_err(allocation_error)?;
            requested.extend_from_slice(ids);
            Ok(QuoteUpdates::new(requested, updates, aggregate_meta(&context, &metas, Some(metadata.version()))?))
        })();
        match result {
            Ok(updates) => { self.executor.release_pin(context, &mut pin, None)?; Ok(updates) }
            Err(root) => match self.executor.release_pin(&context, &mut pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(ErrorKind::Internal, "temporary depth failure lost its root cause")),
            },
        }
    }
}

pub(crate) fn refresh_command(requests: &[RefreshRequest]) -> Result<CommandRequest> {
    if requests.len() > 100 {
        return Err(Error::new(ErrorKind::InvalidArgument, "refresh accepts at most 100 identities"));
    }
    let ids = requests.iter().map(|item| item.id().clone()).collect::<Vec<_>>();
    let codes = checked_ids(&ids)?;
    RefreshStreamRequest::new(codes.into_iter().zip(requests).map(|(code, item)|
        RefreshCursor { code, cursor: item.cursor() }).collect())
        .map(CommandRequest::RefreshStream).map_err(Error::from_protocol)
}

/// Depth, borrowed refresh, activation, renewal and pushed frames share this
/// identity validation and DTO projection, including raw retention and units.
pub(crate) fn project_refresh_updates(
    ids: &[SecurityId], context: &OperationContext, config: &ClientConfig,
    metadata: &MetadataSnapshot, decoded: &DecodedReply, diagnostic: &str,
) -> Result<QuoteUpdates> {
    let CommandResponse::RefreshStream(page) = &decoded.response else { return Err(wrong_response("quote refresh")); };
    validate_update_identities(ids, page.records.iter().map(|row| parse_identity(row.market_id, &row.code)))?;
    let mut updates = Vec::new();
    updates.try_reserve(page.records.len()).map_err(allocation_error)?;
    for row in &page.records {
        let id = parse_identity(row.market_id, &row.code)?;
        let snapshot = project_refresh_snapshot(row, id.clone(), context, config, metadata, decoded, diagnostic)?;
        updates.push(QuoteUpdate::new(id, row.update_time_raw, row.active, snapshot, row.status_or_reserved_raw));
    }
    context.record_work(page.records.len(), 0)?;
    let meta = result_meta(context, config, Some(metadata), &ServiceSource::from_reply(decoded),
        Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()),
        &page.records.iter().map(|row| row.record_range).collect::<Vec<_>>(), diagnostic);
    Ok(QuoteUpdates::new(ids.to_vec(), updates, meta))
}

pub(crate) enum RawSnapshotSlot {
    Present { row: QuoteSnapshot, reply: Arc<DecodedReply> },
    Missing,
}

pub(crate) struct RawSnapshotBatch {
    pub(crate) slots: Vec<RawSnapshotSlot>,
    pub(crate) replies: Vec<Arc<DecodedReply>>,
    pub(crate) recovered: bool,
}

/// Python binding projection that preserves each source DTO, page and wire
/// range without teaching the binding a second parser or recovery algorithm.
#[derive(Clone, Debug)]
pub struct LegacySnapshotRecord {
    pub input_index: usize,
    pub requested: NormalizedCode,
    pub row: QuoteSnapshot,
    pub payload: bytes::Bytes,
    pub wire: Option<bytes::Bytes>,
    pub endpoint: String,
    pub command: u16,
}

#[derive(Clone, Debug)]
pub struct LegacySnapshotBatch {
    pub records: Vec<LegacySnapshotRecord>,
    pub missing: Vec<(usize, NormalizedCode)>,
    pub unmatched: Vec<usize>,
    pub recovered: bool,
}

pub(crate) fn legacy_snapshot_rows(
    context: &OperationContext, codes: &[NormalizedCode],
    execute: impl FnMut(&[NormalizedCode], ChildPurpose, Option<RecoveryLimit>) -> Result<DecodedReply>,
) -> Result<LegacySnapshotBatch> {
    if !context.is_legacy() { return Err(Error::new(ErrorKind::InvalidArgument, "legacy snapshot adapter requires a Python-profile operation")); }
    if codes.is_empty() {
        return Ok(LegacySnapshotBatch { records: Vec::new(), missing: Vec::new(), unmatched: Vec::new(), recovered: false });
    }
    let fetched = retrieve_snapshot_rows(context, codes, codes.len(), execute)?;
    let mut records = Vec::new();
    let mut missing = Vec::new();
    let mut unmatched = Vec::new();
    records.try_reserve(fetched.slots.len()).map_err(allocation_error)?;
    missing.try_reserve(fetched.slots.len()).map_err(allocation_error)?;

    if fetched.recovered {
        let mut by_identity = HashMap::<SecurityId, VecDeque<LegacySnapshotRecord>>::new();
        let mut missing_by_index = HashMap::new();
        by_identity.try_reserve(codes.len()).map_err(allocation_error)?;
        missing_by_index.try_reserve(codes.len()).map_err(allocation_error)?;
        for (index, slot) in fetched.slots.into_iter().enumerate() {
            let requested = codes.get(index).ok_or_else(|| Error::new(ErrorKind::Protocol,
                "snapshot recovery returned more slots than requested"))?;
            match slot {
                RawSnapshotSlot::Missing => {
                    missing_by_index.insert(index, requested.clone());
                }
                RawSnapshotSlot::Present { row, reply } => {
                    let Ok(identity) = parse_identity(row.market_id, &row.code) else {
                        continue;
                    };
                    let bucket = by_identity.entry(identity).or_default();
                    bucket.try_reserve(1).map_err(allocation_error)?;
                    bucket.push_back(LegacySnapshotRecord {
                        input_index: index,
                        requested: requested.clone(),
                        row,
                        payload: reply.payload.clone(),
                        wire: reply.wire.as_ref().map(|frame| frame.raw.clone()),
                        endpoint: reply.source.host.clone(),
                        command: reply.request.command_code(),
                    });
                }
            }
        }
        records.try_reserve(codes.len()).map_err(allocation_error)?;
        missing.try_reserve(codes.len()).map_err(allocation_error)?;
        unmatched.try_reserve(codes.len()).map_err(allocation_error)?;
        for (index, requested) in codes.iter().enumerate() {
            let identity = identity_from_market(requested.market(), requested.number())?;
            if let Some(mut record) = by_identity.get_mut(&identity).and_then(VecDeque::pop_front) {
                record.input_index = index;
                record.requested = requested.clone();
                records.push(record);
            } else if let Some(requested) = missing_by_index.remove(&index) {
                missing.push((index, requested));
            } else {
                unmatched.push(index);
            }
        }
    } else {
        for (index, (requested, slot)) in codes.iter().zip(fetched.slots).enumerate() {
            match slot {
                RawSnapshotSlot::Missing => missing.push((index, requested.clone())),
                RawSnapshotSlot::Present { row, reply } => records.push(LegacySnapshotRecord {
                    input_index: index,
                    requested: requested.clone(),
                    row,
                    payload: reply.payload.clone(),
                    wire: reply.wire.as_ref().map(|frame| frame.raw.clone()),
                    endpoint: reply.source.host.clone(),
                    command: reply.request.command_code(),
                }),
            }
        }
    }
    Ok(LegacySnapshotBatch { records, missing, unmatched, recovered: fetched.recovered })
}

pub(crate) fn retrieve_snapshot_rows(
    context: &OperationContext, codes: &[NormalizedCode], chunk_size: usize,
    mut execute: impl FnMut(&[NormalizedCode], ChildPurpose, Option<RecoveryLimit>) -> Result<DecodedReply>,
) -> Result<RawSnapshotBatch> {
    if chunk_size == 0 { return Err(Error::new(ErrorKind::InvalidArgument, "snapshot chunk size must be positive")); }
    let mut output = RawSnapshotBatch { slots: Vec::new(), replies: Vec::new(), recovered: false };
    output.slots.try_reserve(codes.len()).map_err(allocation_error)?;
    let mut first_required = true;
    let mut query = |part: &[NormalizedCode], recovery: bool| {
        context.check_boundary(TimeoutPhase::Admission)?;
        let purpose = if recovery { ChildPurpose::MissingRecovery } else if first_required {
            first_required = false;
            ChildPurpose::Required
        } else { ChildPurpose::Page };
        execute(part, purpose, recovery.then_some(SNAPSHOT_RECOVERY))
    };
    if codes.is_empty() {
        return Ok(RawSnapshotBatch { slots: Vec::new(), replies: Vec::new(), recovered: false });
    }
    for chunk in codes.chunks(chunk_size) {
        let fetched = recover_snapshot_tree(context, chunk, false, &mut query)?;
        output.slots.extend(fetched.slots);
        output.replies.try_reserve(fetched.replies.len()).map_err(allocation_error)?;
        output.replies.extend(fetched.replies);
        output.recovered |= fetched.recovered;
    }
    Ok(output)
}

/// The same ordered binary isolation is used by both execution profiles. The
/// callback's T08 authorization charges diagnostics only when attempts start.
fn recover_snapshot_tree(
    context: &OperationContext, codes: &[NormalizedCode], recovery: bool,
    query: &mut impl FnMut(&[NormalizedCode], bool) -> Result<DecodedReply>,
) -> Result<RawSnapshotBatch> {
    super::recovery::ordered_tree(context, codes, recovery,
        &mut |part, recovery| recover_snapshot_step(context, part, recovery, query), &merge_snapshot_trees)
}

fn recover_snapshot_step(
    context: &OperationContext, codes: &[NormalizedCode], recovery: bool,
    query: &mut impl FnMut(&[NormalizedCode], bool) -> Result<DecodedReply>,
) -> Result<super::recovery::RecoveryStep<RawSnapshotBatch>> {
    use super::recovery::RecoveryStep;
    let decoded = match query(codes, recovery) {
        Ok(decoded) => decoded,
        Err(error) => {
            if codes.len() == 1 && context.is_legacy() && is_legacy_single_absence(&error) {
                return Ok(RecoveryStep::Complete(RawSnapshotBatch { slots: vec![RawSnapshotSlot::Missing], replies: Vec::new(), recovered: true }));
            }
            if codes.len() <= 1 || !is_retryable_snapshot_structure(&error) { return Err(error); }
            context.remember_recoverable(error);
            return Ok(RecoveryStep::Split);
        }
    };
    let CommandResponse::Snapshots(rows) = &decoded.response else { return Err(wrong_response("snapshots")); };
    let ids = codes.iter().map(|code| identity_from_market(code.market(), code.number())).collect::<Result<Vec<_>>>()?;
    if !context.is_legacy() {
        validate_update_identities(&ids, rows.iter().map(|row| parse_identity(row.market_id, &row.code)))?;
    }
    if rows.is_empty() && !codes.is_empty() {
        if codes.len() == 1 {
            return Ok(RecoveryStep::Complete(RawSnapshotBatch { slots: vec![RawSnapshotSlot::Missing], replies: vec![Arc::new(decoded)], recovered: true }));
        }
        context.remember_recoverable(Error::new(ErrorKind::Protocol, "empty snapshot response"));
        return Ok(RecoveryStep::Split);
    }
    if rows.len() != codes.len() {
        if !context.is_legacy() && rows.len() > codes.len() {
            return Err(Error::new(ErrorKind::Protocol, "snapshot response contains excess records"));
        }
        let mismatch = Error::new(ErrorKind::Protocol, if context.is_legacy() {
            format!("snapshot response count mismatch: expected {}, got {}", codes.len(), rows.len())
        } else {
            "snapshot response count mismatch".to_owned()
        });
        if context.is_legacy() && codes.len() == 1 { return Err(mismatch); }
        context.remember_recoverable(mismatch);
        return Ok(RecoveryStep::Split);
    }
    // The Python facade returned a successful first response as-is. Only its
    // recovery path rebuilt rows by requested occurrence, so preserve the
    // server order until recovery was actually needed.
    let order = if context.is_legacy() {
        (0..rows.len()).collect()
    } else {
        occurrence_order(&ids, rows.iter().map(|row| parse_identity(row.market_id, &row.code)))?
    };
    let decoded = Arc::new(decoded);
    let CommandResponse::Snapshots(rows) = &decoded.response else { unreachable!() };
    let mut slots = Vec::new();
    slots.try_reserve(codes.len()).map_err(allocation_error)?;
    for index in order { slots.push(RawSnapshotSlot::Present { row: rows[index].clone(), reply: Arc::clone(&decoded) }); }
    context.record_work(rows.len(), 0)?;
    Ok(RecoveryStep::Complete(RawSnapshotBatch { slots, replies: vec![decoded], recovered: recovery }))
}

fn merge_snapshot_trees(mut first: RawSnapshotBatch, second: RawSnapshotBatch) -> Result<RawSnapshotBatch> {
    first.slots.try_reserve(second.slots.len()).map_err(allocation_error)?;
    first.slots.extend(second.slots);
    first.replies.try_reserve(second.replies.len()).map_err(allocation_error)?;
    first.replies.extend(second.replies);
    first.recovered = true;
    Ok(first)
}

/// Result indices in input-occurrence order. Distinct values for repeated
/// identities are consumed once each, never deduplicated or overwritten.
pub(super) fn occurrence_order(ids: &[SecurityId], returned: impl IntoIterator<Item = Result<SecurityId>>) -> Result<Vec<usize>> {
    let mut by_id: HashMap<SecurityId, VecDeque<usize>> = HashMap::new();
    by_id.try_reserve(ids.len()).map_err(allocation_error)?;
    for id in ids { by_id.entry(id.clone()).or_default(); }
    let mut count = 0_usize;
    for (index, id) in returned.into_iter().enumerate() {
        let id = id?;
        let Some(bucket) = by_id.get_mut(&id) else { return Err(Error::new(ErrorKind::Protocol,
            "quote response contains an unrequested identity").with_context("security", format!("{id}"))); };
        bucket.try_reserve(1).map_err(allocation_error)?;
        bucket.push_back(index);
        count = count.checked_add(1).ok_or_else(offset_overflow)?;
        if count > ids.len() { return Err(Error::new(ErrorKind::Protocol, "quote response contains excess duplicate records")); }
    }
    if count != ids.len() { return Err(Error::new(ErrorKind::Protocol, "quote response identity count is incomplete")); }
    let mut order = Vec::new();
    order.try_reserve(ids.len()).map_err(allocation_error)?;
    for id in ids {
        let index = by_id.get_mut(id).and_then(VecDeque::pop_front)
            .ok_or_else(|| Error::new(ErrorKind::Protocol, "quote response multiplicity does not match input occurrences")
                .with_context("security", format!("{id}")))?;
        order.push(index);
    }
    if by_id.values().any(|bucket| !bucket.is_empty()) { return Err(Error::new(ErrorKind::Protocol, "quote response has conflicting excess duplicates")); }
    Ok(order)
}

fn validate_update_identities(ids: &[SecurityId], returned: impl IntoIterator<Item = Result<SecurityId>>) -> Result<()> {
    let mut remaining = HashMap::<SecurityId, usize>::new();
    remaining.try_reserve(ids.len()).map_err(allocation_error)?;
    for id in ids { *remaining.entry(id.clone()).or_default() += 1; }
    for id in returned {
        let id = id?;
        let count = remaining.get_mut(&id).filter(|count| **count != 0).ok_or_else(|| Error::new(ErrorKind::Protocol,
            "refresh response contains a foreign or excess duplicate identity").with_context("security", format!("{id}")))?;
        *count -= 1;
    }
    Ok(())
}

fn missing_policy_error(missing: &[(usize, SecurityId, MissingReason)], policy: MissingPolicy) -> Result<()> {
    if policy == MissingPolicy::Error && !missing.is_empty() {
        let mut error = Error::new(ErrorKind::MissingData, "one or more requested snapshots are missing").with_context("command", "0x054c");
        for (index, id, reason) in missing {
            error = error.with_context(format!("missing.{index}.security"), format!("{id}"))
                .with_context(format!("missing.{index}.input_index"), format!("{index}"))
                .with_context(format!("missing.{index}.reason"), format!("{reason:?}"));
        }
        return Err(error);
    }
    Ok(())
}

fn is_retryable_snapshot_structure(error: &Error) -> bool {
    match protocol_source(error) {
        Some(eltdx_protocol::ProtocolError::InvalidData { context: "snapshots", message }) =>
            message.starts_with("snapshot response count mismatch:"),
        Some(eltdx_protocol::ProtocolError::InvalidData { context: "snapshot", message }) =>
            message.starts_with("snapshot record marker not found:")
                || message.starts_with("snapshot missing requested record:"),
        _ => false,
    }
}
fn is_legacy_single_absence(error: &Error) -> bool {
    protocol_source(error).is_some_and(|source| matches!(source, eltdx_protocol::ProtocolError::InvalidData { context: "snapshot", message }
        if message.starts_with("snapshot missing requested record:")))
}
fn protocol_source(error: &Error) -> Option<&eltdx_protocol::ProtocolError> {
    std::error::Error::source(error).and_then(|source| source.downcast_ref::<eltdx_protocol::ProtocolError>())
}
fn snapshot_requests(codes: &[NormalizedCode], chunk_size: usize) -> Result<Vec<CommandRequest>> {
    let mut requests = Vec::new();
    requests.try_reserve(codes.len().div_ceil(chunk_size)).map_err(allocation_error)?;
    for chunk in codes.chunks(chunk_size) { requests.push(CommandRequest::Snapshots(SnapshotsRequest::new(chunk.to_vec()).map_err(Error::from_protocol)?)); }
    Ok(requests)
}
fn legacy_requests(codes: &[NormalizedCode]) -> Result<Vec<CommandRequest>> {
    codes.chunks(LEGACY_CHUNK_SIZE).map(|chunk| LegacyQuotesRequest::new(chunk.to_vec())
        .map(CommandRequest::LegacyQuotes).map_err(Error::from_protocol)).collect()
}
fn refresh_requests(codes: &[NormalizedCode]) -> Result<Vec<CommandRequest>> {
    codes.chunks(100).map(|chunk| RefreshStreamRequest::new(chunk.iter().cloned().map(|code| RefreshCursor { code, cursor: 0 }).collect())
        .map(CommandRequest::RefreshStream).map_err(Error::from_protocol)).collect()
}
fn snapshot_page_meta(context: &OperationContext, config: &ClientConfig, metadata: &MetadataSnapshot, reply: &DecodedReply) -> ResultMeta {
    let ranges = match &reply.response { CommandResponse::Snapshots(rows) => rows.iter().map(|row| row.record_range).collect::<Vec<_>>(), _ => Vec::new() };
    result_meta(context, config, Some(metadata), &ServiceSource::from_reply(reply), Some(reply.payload.clone()),
        reply.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "quotes.snapshots")
}
fn category_fingerprint(query: &CategoryQuery) -> u64 {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "hq:054b".hash(&mut state);
    query.category().raw().hash(&mut state); query.sort().raw().hash(&mut state);
    query.ascending().hash(&mut state); query.count().hash(&mut state);
    state.finish()
}
fn offset_overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "quote continuation offset exceeds the wire range") }
fn wrong_response(name: &str) -> Error { Error::new(ErrorKind::Protocol, format!("{name} request returned the wrong response kind")) }

fn project_snapshot(
    row: &eltdx_protocol::commands::quotes::QuoteSnapshot,
    id: SecurityId,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
    range: RecordRange,
) -> Result<Snapshot> {
    finite_amount(row.amount)?;
    let raw = &row.price_raw;
    let last = price_from_raw(raw.current, raw.wire_denominator, raw.divisor)?;
    let pre_close = price_from_raw(raw.last_close, raw.wire_denominator, raw.divisor)?;
    let open = price_from_raw(raw.open, raw.wire_denominator, raw.divisor)?;
    let high = price_from_raw(raw.high, raw.wire_denominator, raw.divisor)?;
    let low = price_from_raw(raw.low, raw.wire_denominator, raw.divisor)?;
    let levels = vec![
        QuoteLevel::new(price_from_quote_delta(raw.current, row.buy_levels.first().map_or(0, |level| level.price_delta_raw), raw.divisor)?,
            quote_quantity(row.buy_levels.first().map_or(0, |level| level.volume))),
    ];
    let asks = vec![
        QuoteLevel::new(price_from_quote_delta(raw.current, row.sell_levels.first().map_or(0, |level| level.price_delta_raw), raw.divisor)?,
            quote_quantity(row.sell_levels.first().map_or(0, |level| level.volume))),
    ];
    let source = super::ServiceSource::from_reply(decoded);
    let meta = super::result_meta(context, config, Some(metadata), &source,
        Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &[range], "quotes.snapshot");
    let amount = unknown_amount(row.amount);
    let open_amount = cny_hundredths(row.open_amount_raw)?;
    Ok(Snapshot::new(id, row.active1, last, pre_close, open, high, low,
        crate::types::EventTime::unknown_raw(row.time_raw), row.time_raw,
        quote_quantity(row.total_hand), quote_quantity(row.current_hand), quote_quantity(row.inside_dish),
        quote_quantity(row.outer_disc), amount, open_amount, levels, asks, meta)
        .with_amount_wire_word(row.amount_raw)
        .with_quote_wire_fields(Some(row.unknown_after_time_raw), Some(row.unknown_after_outer_raw), Some(row.open_amount_raw)))
}

fn project_legacy(
    row: &eltdx_protocol::commands::quotes::LegacyQuote,
    id: SecurityId,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
    range: RecordRange,
) -> Result<LegacyQuote> {
    finite_amount(row.amount)?;
    let raw = &row.price_raw;
    let levels = row.buy_levels.iter().map(|level| {
        let value = raw.current.checked_add(level.price_delta_raw)
            .ok_or_else(|| Error::new(ErrorKind::Protocol, "legacy bid raw price overflows"))?;
        Ok(QuoteLevel::new(price_from_raw(value, raw.wire_denominator, raw.divisor)?, quote_quantity(level.volume)))
    }).collect::<Result<Vec<_>>>()?;
    let asks = row.sell_levels.iter().map(|level| {
        let value = raw.current.checked_add(level.price_delta_raw)
            .ok_or_else(|| Error::new(ErrorKind::Protocol, "legacy ask raw price overflows"))?;
        Ok(QuoteLevel::new(price_from_raw(value, raw.wire_denominator, raw.divisor)?, quote_quantity(level.volume)))
    }).collect::<Result<Vec<_>>>()?;
    let source = super::ServiceSource::from_reply(decoded);
    let meta = super::result_meta(context, config, Some(metadata), &source,
        Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &[range], "quotes.legacy");
    let snapshot = Snapshot::new(id, row.active1,
        price_from_raw(raw.current, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.last_close, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.open, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.high, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.low, raw.wire_denominator, raw.divisor)?,
        crate::types::EventTime::unknown_raw(row.server_time_raw), row.server_time_raw,
        quote_quantity(row.total_hand), quote_quantity(row.current_hand), quote_quantity(row.inside_dish),
        quote_quantity(row.outer_disc), unknown_amount(row.amount), cny_hundredths(row.open_amount_raw)?,
        levels, asks, meta).with_amount_wire_word(row.amount_raw)
        .with_quote_wire_fields(Some(row.unknown_after_time_raw), Some(row.unknown_after_outer_raw), Some(row.open_amount_raw));
    let rise_speed = row.rise_speed_raw.map(percentage_points);
    Ok(LegacyQuote::new(snapshot, row.trading_status_raw, rise_speed, row.active2, row.tail_metrics_raw))
}

fn project_category(
    row: &eltdx_protocol::commands::quotes::CategoryQuoteRecord,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
    range: RecordRange,
) -> Result<CategoryQuote> {
    finite_amount(row.amount)?;
    if !row.min2_amount.is_finite() || !row.depth.is_finite() {
        return Err(Error::new(ErrorKind::Protocol, "category quote contains a non-finite required value"));
    }
    let id = parse_identity(row.market_id, &row.code)?;
    let raw = &row.price_raw;
    let last = price_from_raw(raw.current, raw.wire_denominator, raw.divisor)?;
    let bid_raw = raw.current.checked_add(row.bid1_delta_raw)
        .ok_or_else(|| Error::new(ErrorKind::Protocol, "category bid raw price overflows"))?;
    let ask_raw = raw.current.checked_add(row.ask1_delta_raw)
        .ok_or_else(|| Error::new(ErrorKind::Protocol, "category ask raw price overflows"))?;
    let bid = QuoteLevel::new(price_from_raw(bid_raw, raw.wire_denominator, raw.divisor)?, quote_quantity(row.bid_vol1));
    let ask = QuoteLevel::new(price_from_raw(ask_raw, raw.wire_denominator, raw.divisor)?, quote_quantity(row.ask_vol1));
    let source = super::ServiceSource::from_reply(decoded);
    let meta = super::result_meta(context, config, Some(metadata), &source,
        Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &[range], "quotes.category");
    let snapshot = Snapshot::new(id, row.active1, last,
        price_from_raw(raw.last_close, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.open, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.high, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.low, raw.wire_denominator, raw.divisor)?,
        crate::types::EventTime::unknown_raw(row.server_time_raw), row.server_time_raw,
        quote_quantity(row.total_hand), quote_quantity(row.current_hand), quote_quantity(row.inside_dish),
        quote_quantity(row.outer_disc), unknown_amount(row.amount), cny_hundredths(row.open_amount_raw)?,
        vec![bid.clone()], vec![ask.clone()], meta.clone()).with_amount_wire_word(row.amount_raw)
        .with_quote_wire_fields(None, Some(row.after_outer_raw), Some(row.open_amount_raw));
    Ok(CategoryQuote::new(snapshot, bid, ask, row.status_or_sort_raw,
        percentage_points(row.rise_speed_raw), percentage_points(row.short_turnover_raw),
        percentage_points(row.opening_rush_raw), unknown_ratio(row.vol_rise_speed)?,
        Amount::with_provenance(NumericValue::F32(row.min2_amount), AmountUnit::Unknown, ValueProvenance::WireRule), f64::from(row.depth), meta))
}

fn project_refresh_snapshot(
    row: &eltdx_protocol::commands::quotes::QuoteRefreshRecord,
    id: SecurityId,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
    diagnostic: &str,
) -> Result<Snapshot> {
    finite_amount(row.amount)?;
    let raw = &row.price_raw;
    let bids = row.buy_levels.iter().map(|level| Ok(QuoteLevel::new(
        price_from_quote_delta(raw.current, level.price_delta_raw, raw.divisor)?, quote_quantity(level.volume)))).collect::<Result<Vec<_>>>()?;
    let asks = row.sell_levels.iter().map(|level| Ok(QuoteLevel::new(
        price_from_quote_delta(raw.current, level.price_delta_raw, raw.divisor)?, quote_quantity(level.volume)))).collect::<Result<Vec<_>>>()?;
    let source = super::ServiceSource::from_reply(decoded);
    let meta = super::result_meta(context, config, Some(metadata), &source, Some(decoded.payload.clone()),
        decoded.wire.as_ref().map(|wire| wire.raw.clone()), &[row.record_range], diagnostic);
    let open_amount = row.open_amount_raw.checked_mul(10)
        .ok_or_else(|| Error::new(ErrorKind::Protocol, "refresh opening amount overflows"))?;
    Ok(Snapshot::new(id, row.active,
        price_from_raw(raw.current, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.last_close, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.open, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.high, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.low, raw.wire_denominator, raw.divisor)?,
        crate::types::EventTime::unknown_raw(i64::from(row.update_time_raw)), i64::from(row.update_time_raw),
        quote_quantity(row.total_hand), quote_quantity(row.current_hand), quote_quantity(row.inside_dish),
        quote_quantity(row.outer_disc), unknown_amount(row.amount),
        Amount::with_provenance(NumericValue::ScaledInteger { value: open_amount, scale: 0 }, AmountUnit::Cny, ValueProvenance::WireRule),
        bids, asks, meta).with_amount_wire_word(row.amount_raw)
        .with_quote_wire_fields(None, Some(row.unknown_after_outer_raw), Some(row.open_amount_raw)))
}

fn finite_amount(value: f64) -> Result<()> {
    if !value.is_finite() { return Err(Error::new(ErrorKind::Protocol, "quote amount contains a non-finite wire scalar")); }
    Ok(())
}

