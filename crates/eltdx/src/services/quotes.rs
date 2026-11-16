use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use eltdx_protocol::commands::quotes::{
    CategoryQuotesRequest, LegacyQuotesRequest, RefreshCursor, RefreshStreamRequest, SnapshotsRequest,
};
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, RecoveryLimit};
use crate::session::PinnedSession;
use crate::types::batch::{
    BatchItem, BatchItemState, BatchOutcome, DataPage, EndEvidence, MissingPolicy, MissingReason, PageCursor,
    SnapshotBatch,
};
use crate::types::provenance::{ResultMeta, SourceStamp, ValueProvenance};
use crate::types::quotes::{
    cny_hundredths, parse_identity, percentage_points, price_from_quote_delta, price_from_raw,
    quote_quantity, unknown_amount, unknown_ratio,
};
use crate::types::{
    Amount, CategoryQuery, CategoryQuote, HqMarket, LegacyQuote, NumericValue, QuoteLevel,
    QuoteUpdate, QuoteUpdates, SecurityId, Snapshot,
};

use super::{aggregate_meta, begin_context, checked_ids, request_metadata};

const SNAPSHOT_RECOVERY_LIMIT: usize = 32;
const SNAPSHOT_CHUNK_SIZE: usize = 7_000;
const LEGACY_CHUNK_SIZE: usize = 8_000;

#[derive(Clone, Debug)]
pub struct QuoteService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: String,
}

impl QuoteService {
    pub(super) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn snapshots(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<SnapshotBatch> {
        self.snapshots_with_policy(ids, MissingPolicy::Error, options)
    }

    pub fn snapshots_with_policy(
        &self,
        ids: &[SecurityId],
        missing_policy: MissingPolicy,
        options: &OperationOptions,
    ) -> Result<SnapshotBatch> {
        let entry = std::time::Instant::now();
        let normalized = checked_ids(ids)?;
        if normalized.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        let context = self.executor.begin_context_at(entry, "quotes_snapshots", &self.config, options)?;
        let mut requests = Vec::new();
        for chunk in normalized.chunks(SNAPSHOT_CHUNK_SIZE) {
            requests.push(CommandRequest::Snapshots(SnapshotsRequest::new(chunk.to_vec()).map_err(Error::from_protocol)?));
        }
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut outcomes: Vec<Option<Snapshot>> = Vec::new();
        outcomes.try_reserve(ids.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "unable to reserve snapshot result slots").with_source(error))?;
        outcomes.resize_with(ids.len(), || None);
        let mut missing_slots = vec![None; ids.len()];
        let mut sources = Vec::new();
        let mut recovery = RecoveryBudget::default();
        let mut position = 0_usize;
        for (request_index, request) in requests.into_iter().enumerate() {
            let chunk_start = request_index.checked_mul(SNAPSHOT_CHUNK_SIZE)
                .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "snapshot chunk index overflows usize"))?;
            let chunk_end = chunk_start.saturating_add(SNAPSHOT_CHUNK_SIZE).min(normalized.len());
            let chunk = &normalized[chunk_start..chunk_end];
            let start_index = position;
            position = position.checked_add(chunk.len()).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "snapshot index overflows usize"))?;
            let mut rows = self.fetch_snapshot_tree(&context, request, chunk, false, &mut recovery)?;
            for missing in rows.missing.drain(..) {
                missing_slots[start_index + missing] = Some(MissingReason::NotFound);
            }
            for (relative_index, snapshot) in rows.items.drain(..) {
                outcomes[start_index + relative_index] = Some(snapshot);
            }
            sources.extend(rows.sources);
        }

        let mut items = Vec::new();
        items.try_reserve(ids.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "unable to reserve snapshot batch items").with_source(error))?;
        let mut missing = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            match outcomes[index].take() {
                Some(snapshot) => items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(snapshot))),
                None => {
                    let reason = missing_slots[index].clone().unwrap_or(MissingReason::Unavailable);
                    missing.push((index, id.clone(), reason.clone()));
                    items.push(BatchItem::new(index, id.clone(), BatchItemState::ConfirmedMissing(reason)));
                }
            }
        }
        missing_policy_error(&missing, missing_policy)?;
        let meta = batch_meta(&context, &metadata, &sources);
        context.finish()?;
        Ok(BatchOutcome::with_meta(items, meta))
    }

    fn fetch_snapshot_tree(
        &self,
        context: &OperationContext,
        request: CommandRequest,
        codes: &[eltdx_protocol::unit::NormalizedCode],
        recovery: bool,
        budget: &mut RecoveryBudget,
    ) -> Result<SnapshotFetch> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let purpose = if recovery { ChildPurpose::MissingRecovery } else { ChildPurpose::Required };
        let limit = recovery.then_some(RecoveryLimit { name: "snapshot_missing", maximum: SNAPSHOT_RECOVERY_LIMIT });
        if recovery {
            if budget.started >= SNAPSHOT_RECOVERY_LIMIT {
                return Err(recovery_limit_error(budget.started));
            }
            budget.started += 1;
        }
        let raw = self.executor.execute_raw(&self.engine, context, request.clone(), purpose, limit);
        let decoded = match raw {
            Ok(raw) => match self.executor.decode_received(context, raw) {
                Ok(reply) => reply,
                Err(error) if is_retryable_snapshot_structure(&error) && codes.len() > 1 => {
                    context.remember_recoverable(error);
                    return self.split_snapshot_request(context, codes, budget);
                }
                Err(error) if is_confirmed_single_absence(&error) && codes.len() == 1 => {
                    return Ok(SnapshotFetch { items: Vec::new(), missing: vec![0], sources: Vec::new() });
                }
                Err(error) => return Err(attach_batch_identity(error, codes.first())),
            },
            Err(error) => return Err(error),
        };

        let CommandResponse::Snapshots(rows) = &decoded.response else {
            return Err(context.annotate(Error::new(ErrorKind::Protocol, "snapshot request returned the wrong response"), TimeoutPhase::Decode, Some(0x054c)));
        };
        if rows.is_empty() && !codes.is_empty() {
            if codes.len() == 1 {
                return Ok(SnapshotFetch { items: Vec::new(), missing: vec![0], sources: vec![ServiceSource::from_reply(&decoded)] });
            }
            return self.split_snapshot_request(context, codes, budget);
        }
        if rows.len() != codes.len() {
            let error = Error::new(ErrorKind::Protocol, "snapshot response count does not match request")
                .with_context("requested", format!("{}", codes.len())).with_context("returned", format!("{}", rows.len()));
            if codes.len() > 1 {
                context.remember_recoverable(error);
                return self.split_snapshot_request(context, codes, budget);
            }
            return Err(attach_batch_identity(error, codes.first()));
        }

        let mut by_code: HashMap<String, VecDeque<Snapshot>> = HashMap::new();
        for (row, range) in rows.iter().zip(snapshot_ranges(rows, &decoded)?) {
            let id = parse_identity(row.market_id, &row.code)?;
            let identity = normalize_id(&id)?;
            if !codes.iter().any(|requested| requested == &identity) {
                return Err(Error::new(ErrorKind::Protocol, "snapshot response contains an unrequested identity")
                    .with_context("security", format!("{id}")).with_context("command", "0x054c"));
            }
            let metadata = context.metadata()?.ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "snapshot metadata is unavailable"))?;
            let snapshot = project_snapshot(row, id.clone(), context, &self.config, &metadata, &decoded, range)?;
            by_code.entry(id.to_string()).or_default().push_back(snapshot);
        }
        let mut fetched = SnapshotFetch { items: Vec::new(), missing: Vec::new(), sources: vec![ServiceSource::from_reply(&decoded)] };
        for (position, code) in codes.iter().enumerate() {
            let key = normalized_identity(code);
            let Some(queue) = by_code.get_mut(&key) else {
                return Err(Error::new(ErrorKind::Protocol, "snapshot response identity set is incomplete")
                    .with_context("security", key).with_context("command", "0x054c"));
            };
            let Some(snapshot) = queue.pop_front() else {
                return Err(Error::new(ErrorKind::Protocol, "snapshot response has excess duplicate identities")
                    .with_context("security", key).with_context("command", "0x054c"));
            };
            fetched.items.push((position, snapshot));
        }
        if by_code.values().any(|queue| !queue.is_empty()) {
            return Err(Error::new(ErrorKind::Protocol, "snapshot response has excess records"));
        }
        context.record_work(rows.len(), decoded.payload.len())?;
        Ok(fetched)
    }

    fn split_snapshot_request(
        &self,
        context: &OperationContext,
        codes: &[eltdx_protocol::unit::NormalizedCode],
        budget: &mut RecoveryBudget,
    ) -> Result<SnapshotFetch> {
        if codes.len() <= 1 {
            return Err(Error::new(ErrorKind::Protocol, "snapshot recovery cannot split a malformed singleton")
                .with_context("command", "0x054c"));
        }
        let middle = codes.len() / 2;
        let first = self.fetch_snapshot_tree(context,
            CommandRequest::Snapshots(SnapshotsRequest::new(codes[..middle].to_vec()).map_err(Error::from_protocol)?),
            &codes[..middle], true, budget)?;
        let second = self.fetch_snapshot_tree(context,
            CommandRequest::Snapshots(SnapshotsRequest::new(codes[middle..].to_vec()).map_err(Error::from_protocol)?),
            &codes[middle..], true, budget)?;
        let mut combined = SnapshotFetch { items: first.items, missing: first.missing, sources: first.sources };
        combined.items.extend(second.items.into_iter().map(|(index, item)| (index + middle, item)));
        combined.missing.extend(second.missing.into_iter().map(|index| index + middle));
        combined.sources.extend(second.sources);
        Ok(combined)
    }

    pub fn legacy(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<LegacyQuote>> {
        let entry = std::time::Instant::now();
        let normalized = checked_ids(ids)?;
        if ids.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        let context = self.executor.begin_context_at(entry, "quotes_legacy", &self.config, options)?;
        let requests = legacy_requests(&normalized)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut output = vec![None; ids.len()];
        let mut sources = Vec::new();
        for (chunk_index, request) in requests.into_iter().enumerate() {
            let start = chunk_index * LEGACY_CHUNK_SIZE;
            let end = (start + LEGACY_CHUNK_SIZE).min(ids.len());
            let decoded = self.executor.execute_decoded(&self.engine, &context, &self.config, &self.namespace,
                request, ChildPurpose::Page, None)?;
            let CommandResponse::LegacyQuotes(rows) = &decoded.response else {
                return Err(Error::new(ErrorKind::Protocol, "legacy quote returned the wrong response"));
            };
            if rows.len() != end - start {
                return Err(Error::new(ErrorKind::Protocol, "legacy quote response count does not match request")
                    .with_context("expected", format!("{}", end - start)).with_context("actual", format!("{}", rows.len())));
            }
            let mut available: HashMap<String, VecDeque<usize>> = HashMap::new();
            for index in start..end { available.entry(ids[index].to_string()).or_default().push_back(index); }
            for (row, range) in rows.iter().zip(legacy_ranges(rows, &decoded)?) {
                let id = super::codes::identity_from_market(row.market, &row.code)?;
                let Some(index) = available.get_mut(&id.to_string()).and_then(VecDeque::pop_front) else {
                    return Err(Error::new(ErrorKind::Protocol, "legacy quote returned an unrequested or excess identity")
                        .with_context("security", format!("{id}")));
                };
                output[index] = Some(project_legacy(row, id, &context, &self.config, &metadata, &decoded, range)?);
            }
            if available.values().any(|indices| !indices.is_empty()) {
                return Err(Error::new(ErrorKind::Protocol, "legacy quote response omitted a requested identity"));
            }
            context.record_work(rows.len(), decoded.payload.len())?;
            sources.push(super::ServiceSource::from_reply(&decoded));
        }
        let items = output.into_iter().enumerate().zip(ids).map(|((index, item), id)| {
            item.map(|quote| BatchItem::new(index, id.clone(), BatchItemState::Present(quote)))
                .ok_or_else(|| Error::new(ErrorKind::Protocol, "legacy quote result slot was not populated"))
        }).collect::<Result<Vec<_>>>()?;
        let meta = batch_meta(&context, &metadata, &sources);
        context.finish()?;
        Ok(BatchOutcome::with_meta(items, meta))
    }

    pub fn category(&self, query: &CategoryQuery, options: &OperationOptions) -> Result<DataPage<CategoryQuote>> {
        let entry = std::time::Instant::now();
        if !(1..=80).contains(&query.count()) {
            return Err(Error::new(ErrorKind::InvalidArgument, "category count must be between 1 and 80"));
        }
        let end_offset = u32::from(query.start()).checked_add(query.count() as u32)
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "category offset overflows"))?;
        if end_offset > u32::from(u16::MAX) + 1 {
            return Err(Error::new(ErrorKind::LimitExceeded, "category page reaches beyond u16 offsets"));
        }
        let context = self.executor.begin_context_at(entry, "quotes_category", &self.config, options)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        query.category().raw().hash(&mut hasher);
        query.sort().raw().hash(&mut hasher);
        query.ascending().hash(&mut hasher);
        if let Some(cursor) = query.cursor() {
            if cursor.owner_id() != context.session_id || cursor.version() != 1 || cursor.page_size() != query.count()
                || cursor.query_fingerprint() != hasher.finish() {
                return Err(Error::new(ErrorKind::InvalidArgument, "category cursor does not match this owner or query"));
            }
        }
        context.set_work_limits(crate::operation::WorkLimits {
            pages: std::num::NonZeroUsize::new(1), records: Some(query.count()), bytes: None,
        })?;
        let request = CategoryQuotesRequest::new(query.category().raw(), query.sort().raw(), query.start(),
            query.count() as u16, query.ascending(), None, 0);
        let decoded = self.executor.execute_decoded(&self.engine, &context, &self.config, &self.namespace,
            CommandRequest::CategoryQuotes(request), ChildPurpose::Page, None)?;
        let CommandResponse::CategoryQuotes(page) = &decoded.response else {
            return Err(Error::new(ErrorKind::Protocol, "category quote returned the wrong response"));
        };
        if page.records.len() > query.count() {
            return Err(Error::new(ErrorKind::Protocol, "category response exceeds requested count"));
        }
        let snapshot = context.metadata()?.ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "category quote metadata snapshot is missing"))?;
        let mut items = Vec::new();
        for (row, range) in page.records.iter().zip(category_ranges(&page.records, &decoded)?) {
            items.push(project_category(row, &context, &self.config, &snapshot, &decoded, range)?);
        }
        context.record_work(items.len(), decoded.payload.len())?;
        let source = super::ServiceSource::from_reply(&decoded);
        let ranges = page.records.iter().map(|row| row.record_range).collect::<Vec<_>>();
        let meta = super::result_meta(&context, &self.config, Some(&snapshot), &source,
            Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "quotes.category");
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        query.category().raw().hash(&mut fingerprint);
        query.sort().raw().hash(&mut fingerprint);
        query.ascending().hash(&mut fingerprint);
        let next = if items.is_empty() { None } else {
            Some(PageCursor::new(end_offset, query.count(), fingerprint.finish(), context.session_id,
                source.host.clone(), Some(snapshot.version())))
        };
        let end = if items.is_empty() { EndEvidence::EmptyPage } else { EndEvidence::Unknown };
        context.finish()?;
        Ok(DataPage::new(items, u64::from(query.start()), query.count(), next, end, meta))
    }

    pub fn depth(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<QuoteUpdates> {
        let entry = std::time::Instant::now();
        let normalized = checked_ids(ids)?;
        if ids.is_empty() {
            return Ok(QuoteUpdates::new(Vec::new(), Vec::new(), ResultMeta::default()));
        }
        let context = self.executor.begin_context_at(entry, "quotes_depth", &self.config, options)?;
        let requests = refresh_requests(&normalized)?;
        let metadata = request_metadata(&self.executor, &self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut pin = self.executor.acquire_pin(&self.engine, &context)?;
        let pin_handle = pin.handle()?;
        let mut updates = Vec::new();
        let mut commands = Vec::new();
        for request in requests {
            context.check_boundary(TimeoutPhase::Admission)?;
            let raw = self.executor.execute_pinned_raw(&self.engine, pin_handle, &context, request,
                ChildPurpose::Page, None)?;
            let decoded = self.executor.decode_received(&context, raw)?;
            let CommandResponse::RefreshStream(page) = &decoded.response else {
                return Err(Error::new(ErrorKind::Protocol, "depth refresh returned the wrong response"));
            };
            for row in &page.records {
                let id = parse_identity(row.market_id, &row.code)?;
                let snapshot = project_refresh_snapshot(row, id.clone(), &context, &self.config, &metadata, &decoded)?;
                updates.push(QuoteUpdate::new(id, 0, row.active, snapshot, row.status_or_reserved_raw));
            }
            context.record_work(page.records.len(), decoded.payload.len())?;
            commands.push(0x0547);
        }
        drop(pin_handle);
        pin.close()?;
        context.finish()?;
        Ok(QuoteUpdates::new(ids.to_vec(), updates, aggregate_meta(&context, &commands, Some(metadata.version()))))
    }
}

#[derive(Default)]
struct RecoveryBudget { started: usize }

#[derive(Default)]
struct SnapshotFetch {
    items: Vec<(usize, Snapshot)>,
    missing: Vec<usize>,
    sources: Vec<super::ServiceSource>,
}

fn missing_policy_error(missing: &[(usize, SecurityId, MissingReason)], policy: MissingPolicy) -> Result<()> {
    if policy == MissingPolicy::Error {
        let details = missing.iter().map(|(index, id, reason)| format!("{id}@{index}:{reason:?}")).collect::<Vec<_>>().join(",");
        return Err(Error::new(ErrorKind::MissingData, "one or more requested snapshots are missing")
            .with_context("missing", details).with_context("command", "0x054c"));
    }
    Ok(())
}

fn is_retryable_snapshot_structure(error: &Error) -> bool {
    let Some(source) = std::error::Error::source(error).and_then(|source| source.downcast_ref::<eltdx_protocol::ProtocolError>()) else {
        return false;
    };
    matches!(source,
        eltdx_protocol::ProtocolError::InvalidData { context: "snapshots", message }
            if message.starts_with("snapshot response count mismatch"))
        || matches!(source,
            eltdx_protocol::ProtocolError::InvalidData { context: "snapshot", message }
                if message.starts_with("snapshot record marker not found:") || message.starts_with("snapshot missing requested record:"))
}

fn is_confirmed_single_absence(error: &Error) -> bool {
    std::error::Error::source(error).and_then(|source| source.downcast_ref::<eltdx_protocol::ProtocolError>())
        .is_some_and(|source| matches!(source,
            eltdx_protocol::ProtocolError::InvalidData { context: "snapshot", message }
                if message.starts_with("snapshot missing requested record:")))
}

fn recovery_limit_error(started: usize) -> Error {
    Error::new(ErrorKind::LimitExceeded, "snapshot recovery reached its local diagnostic request limit")
        .with_context("resource", "snapshot recovery requests")
        .with_context("limit", format!("{SNAPSHOT_RECOVERY_LIMIT}"))
        .with_context("started", format!("{started}"))
}

fn attach_batch_identity(mut error: Error, code: Option<&eltdx_protocol::unit::NormalizedCode>) -> Error {
    if let Some(code) = code {
        if !error.context().iter().any(|(key, _)| key == "security") {
            error = error.with_context("security", code.to_string());
        }
    }
    if !error.context().iter().any(|(key, _)| key == "command") {
        error = error.with_context("command", "0x054c");
    }
    error
}

fn normalize_id(id: &SecurityId) -> Result<eltdx_protocol::unit::NormalizedCode> {
    eltdx_protocol::unit::NormalizedCode::parse(&id.to_string())
        .map_err(|error| Error::new(ErrorKind::InvalidArgument, "invalid security identity").with_source(error))
}

fn normalized_identity(code: &eltdx_protocol::unit::NormalizedCode) -> String { code.full_code() }

fn legacy_requests(codes: &[eltdx_protocol::unit::NormalizedCode]) -> Result<Vec<CommandRequest>> {
    codes.chunks(LEGACY_CHUNK_SIZE).map(|chunk| {
        LegacyQuotesRequest::new(chunk.to_vec()).map(CommandRequest::LegacyQuotes).map_err(Error::from_protocol)
    }).collect()
}

fn refresh_requests(codes: &[eltdx_protocol::unit::NormalizedCode]) -> Result<Vec<CommandRequest>> {
    codes.chunks(100).map(|chunk| {
        let items = chunk.iter().cloned().map(|code| RefreshCursor { code, cursor: 0 }).collect();
        RefreshStreamRequest::new(items).map(CommandRequest::RefreshStream).map_err(Error::from_protocol)
    }).collect()
}

fn batch_meta(context: &OperationContext, metadata: &MetadataSnapshot, sources: &[super::ServiceSource]) -> ResultMeta {
    let mut hosts = sources.iter().filter_map(|source| source.host.as_deref()).collect::<Vec<_>>();
    hosts.sort_unstable();
    hosts.dedup();
    let diagnostics = hosts.into_iter().map(|host| crate::types::Diagnostic::new(
        "hq", "batch_source", crate::types::EnrichmentStatus::Succeeded, None, Some(host.to_owned()))).collect();
    ResultMeta::new(SourceStamp::new(None, None, format!("owner-{:x}", context.session_id),
        Some(metadata.version()), None), diagnostics, None)
}

fn project_snapshot(
    row: &eltdx_protocol::commands::quotes::QuoteSnapshot,
    id: SecurityId,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
    range: RecordRange,
) -> Result<Snapshot> {
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
        quote_quantity(row.outer_disc), amount, open_amount, levels, asks, meta))
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
        levels, asks, meta);
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
        vec![bid.clone()], vec![ask.clone()], meta.clone());
    Ok(CategoryQuote::new(snapshot, bid, ask, row.status_or_sort_raw,
        percentage_points(row.rise_speed_raw), percentage_points(row.short_turnover_raw),
        percentage_points(row.opening_rush_raw), unknown_ratio(row.vol_rise_speed)?,
        unknown_amount(f64::from(row.min2_amount)), f64::from(row.depth), meta))
}

fn project_refresh_snapshot(
    row: &eltdx_protocol::commands::quotes::QuoteRefreshRecord,
    id: SecurityId,
    context: &OperationContext,
    config: &ClientConfig,
    metadata: &MetadataSnapshot,
    decoded: &DecodedReply,
) -> Result<Snapshot> {
    let raw = &row.price_raw;
    let bids = row.buy_levels.iter().map(|level| QuoteLevel::new(Price::from_f64(level.price), quote_quantity(level.volume))).collect();
    let asks = row.sell_levels.iter().map(|level| QuoteLevel::new(Price::from_f64(level.price), quote_quantity(level.volume))).collect();
    let source = super::ServiceSource::from_reply(decoded);
    let meta = super::result_meta(context, config, Some(metadata), &source, Some(decoded.payload.clone()),
        decoded.wire.as_ref().map(|wire| wire.raw.clone()), &[], "quotes.depth");
    Ok(Snapshot::new(id, row.active,
        price_from_raw(raw.current, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.last_close, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.open, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.high, raw.wire_denominator, raw.divisor)?,
        price_from_raw(raw.low, raw.wire_denominator, raw.divisor)?,
        crate::types::EventTime::unknown_raw(i64::from(row.update_time_raw)), i64::from(row.update_time_raw),
        quote_quantity(row.total_hand), quote_quantity(row.current_hand), quote_quantity(row.inside_dish),
        quote_quantity(row.outer_disc), unknown_amount(row.amount), cny_hundredths(row.open_amount_raw)?, bids, asks, meta))
}

fn snapshot_ranges(
    rows: &[eltdx_protocol::commands::quotes::QuoteSnapshot],
    _decoded: &DecodedReply,
) -> Result<Vec<RecordRange>> {
    Ok(rows.iter().map(|row| row.record_range).collect())
}

fn legacy_ranges(
    rows: &[eltdx_protocol::commands::quotes::LegacyQuote],
    _decoded: &DecodedReply,
) -> Result<Vec<RecordRange>> {
    Ok(rows.iter().map(|row| row.record_range).collect())
}

fn category_ranges(
    rows: &[eltdx_protocol::commands::quotes::CategoryQuoteRecord],
    _decoded: &DecodedReply,
) -> Result<Vec<RecordRange>> {
    Ok(rows.iter().map(|row| row.record_range).collect())
}

fn code_fingerprint_query(query: &CategoryQuery) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    query.category().raw().hash(&mut hasher);
    query.sort().raw().hash(&mut hasher);
    query.ascending().hash(&mut hasher);
    hasher.finish()
}

fn _source_fingerprint(market: HqMarket, query: &CategoryQuery, metadata: &MetadataSnapshot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    market.hash(&mut hasher);
    metadata.version().hash(&mut hasher);
    code_fingerprint_query(query).hash(&mut hasher);
    hasher.finish()
}

fn _ensure_range(range: RecordRange, payload: &bytes::Bytes) -> Result<bytes::Bytes> {
    range.bytes(payload).map_err(|error| Error::new(ErrorKind::Protocol, "invalid quote record range").with_source(error))
}

fn _security_value(id: &SecurityId) -> String { id.to_string() }

fn _market_of(id: &SecurityId) -> HqMarket { id.market() }

fn _empty_metadata_version(context: &OperationContext) -> Option<u64> { metadata_version(context) }
