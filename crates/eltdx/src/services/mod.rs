mod codes;
mod auctions;
mod klines;
mod bar_collection;
mod minutes;
mod money_flow;
mod quotes;
mod recovery;
mod corporate;
mod resources;
mod company_info;
mod session;
mod health;
mod stats_projection;
pub use resources::ResourceService;
pub use company_info::CompanyInfoService;
pub(crate) use company_info::legacy_company_reply;
pub use session::SessionService;
pub(crate) use session::legacy_session_reply;
pub use health::HealthService;
mod limits;
pub use corporate::CorporateService;
pub use limits::LimitService;
mod trades;
pub(crate) mod pagination;

pub use codes::CodeService;
pub(crate) use codes::{is_a_share, is_st_name};
pub use auctions::AuctionService;
pub(crate) use codes::{legacy_directory_pages, publish_legacy_directory_metadata};
pub use klines::KlineService;
pub(crate) use klines::legacy_bar_series;
pub use minutes::MinuteService;
pub(crate) use minutes::{legacy_minute_reply, minute_command};
pub use money_flow::MoneyFlowService;
pub use quotes::{PinnedQuoteService, QuoteService};
pub use trades::TradeService;
pub(crate) use quotes::{legacy_snapshot_rows, RawSnapshotBatch, RawSnapshotSlot};
pub(crate) use quotes::{pinned_snapshot_batch, project_refresh_updates, refresh_command, SNAPSHOT_RECOVERY_REQUESTS};

use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

use bytes::Bytes;
use eltdx_protocol::{CommandRequest, RecordRange};
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result};
use crate::hq::decode::DecodedReply;
use crate::metadata::MetadataSnapshot;
use crate::operation::OperationContext;
use crate::types::{Diagnostic, EnrichmentStatus, HqMarket, PageCursor, RawData, RawPolicy, ResultMeta, SecurityId, SourceStamp};

#[derive(Clone, Debug)]
struct ServiceSource {
    command: Option<u16>,
    endpoint: Option<SocketAddr>,
    host: String,
}

impl ServiceSource {
    fn from_reply(reply: &DecodedReply) -> Self {
        Self { command: Some(reply.request.command_code()), endpoint: reply.source.host.parse().ok(), host: reply.source.host.clone() }
    }
}

fn result_meta(
    context: &OperationContext,
    _config: &ClientConfig,
    snapshot: Option<&MetadataSnapshot>,
    source: &ServiceSource,
    payload: Option<Bytes>,
    wire: Option<Bytes>,
    ranges: &[RecordRange],
    diagnostic_source: &str,
) -> ResultMeta {
    let diagnostics = vec![Diagnostic::new(diagnostic_source, "response", EnrichmentStatus::Succeeded,
        None, Some(format!("endpoint={}", source.host)))];
    let retained = if context.raw() == RawPolicy::Omit { None } else {
        payload.map(|payload| RawData::new(vec![payload],
            (context.raw() == RawPolicy::RetainWire).then(|| wire.into_iter().collect()),
            ranges.iter().map(|range| (0, range.offset, range.len)).collect())
            .with_encodings(ranges.iter().map(|range| range.encoding == eltdx_protocol::context::RecordEncoding::RefreshXor93).collect()))
    };
    ResultMeta::new(SourceStamp::new(source.command, source.endpoint, format!("owner-{:x}", context.session_id),
        snapshot.map(MetadataSnapshot::version), None), diagnostics, retained)
}

/// Preserve request and physical source identities for unpriced HQ domains.
fn reply_meta(context: &OperationContext, config: &ClientConfig, reply: &DecodedReply,
    ranges: &[RecordRange], source: &str) -> ResultMeta {
    result_meta(context, config, None, &ServiceSource::from_reply(reply), Some(reply.payload.clone()),
        reply.wire.as_ref().map(|wire| wire.raw.clone()), ranges, source)
        .with_diagnostic(Diagnostic::new(source, "identity", EnrichmentStatus::Succeeded, None,
            Some(format!("request={:?}; epoch={:?}; slot={:?}; generation={:?}; receive_sequence={}",
                reply.source.request_id, reply.source.engine_epoch, reply.source.slot_id,
                reply.source.generation, reply.source.receive_sequence))))
}

/// Preserve each response's origin and one shared backing payload per page.
pub(crate) fn aggregate_meta(context: &OperationContext, pages: &[ResultMeta], version: Option<u64>) -> Result<ResultMeta> {
    let mut sources = Vec::new();
    let mut diagnostics = Vec::new();
    let mut payloads = Vec::new();
    let mut wires = Vec::new();
    let mut ranges = Vec::new();
    let mut encodings = Vec::new();
    for page in pages {
        sources.try_reserve(page.sources().len()).map_err(allocation_error)?;
        sources.extend_from_slice(page.sources());
        diagnostics.try_reserve(page.diagnostics().len()).map_err(allocation_error)?;
        diagnostics.extend_from_slice(page.diagnostics());
        if let Some(raw) = page.raw() {
            let base = payloads.len();
            payloads.try_reserve(raw.payloads().len()).map_err(allocation_error)?;
            payloads.extend_from_slice(raw.payloads());
            if let Some(frames) = raw.wire_frames() {
                wires.try_reserve(frames.len()).map_err(allocation_error)?;
                wires.extend_from_slice(frames);
            }
            ranges.try_reserve(raw.row_ranges().len()).map_err(allocation_error)?;
            encodings.try_reserve(raw.row_ranges().len()).map_err(allocation_error)?;
            for (index, &(page_index, offset, len)) in raw.row_ranges().iter().enumerate() {
                ranges.push((base.checked_add(page_index).ok_or_else(|| Error::new(ErrorKind::LimitExceeded,
                    "retained payload index overflows"))?, offset, len));
                encodings.push(raw.row_requires_xor93(index).unwrap_or(false));
            }
        }
    }
    let raw = (context.raw() != RawPolicy::Omit).then(|| RawData::new(payloads,
        (context.raw() == RawPolicy::RetainWire).then_some(wires), ranges).with_encodings(encodings));
    Ok(ResultMeta::new(SourceStamp::new(None, None, format!("owner-{:x}", context.session_id), version, None),
        diagnostics, raw).with_sources(sources))
}

pub(crate) fn allocation_error(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "unable to reserve bounded service result storage").with_source(error)
}

pub(crate) fn checked_ids(ids: &[SecurityId]) -> Result<Vec<eltdx_protocol::unit::NormalizedCode>> {
    validate_batch_len(ids.len())?;
    normalize_ids(ids)
}

fn validate_batch_len(count: usize) -> Result<()> {
    if count > 65_535 {
        return Err(Error::new(ErrorKind::LimitExceeded, "HQ batch exceeds 65535 input occurrences")
            .with_context("resource", "batch occurrences").with_context("limit", "65535"));
    }
    let bytes = count.checked_mul(7).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "normalized identity size overflows"))?;
    if bytes > 2_097_152 {
        return Err(Error::new(ErrorKind::LimitExceeded, "normalized identities exceed 2 MiB"));
    }
    Ok(())
}

fn normalize_ids(ids: &[SecurityId]) -> Result<Vec<eltdx_protocol::unit::NormalizedCode>> {
    let mut values = Vec::new();
    values.try_reserve(ids.len()).map_err(allocation_error)?;
    for id in ids {
        values.push(eltdx_protocol::unit::NormalizedCode::parse(&id.to_string())
            .map_err(|error| Error::new(ErrorKind::InvalidArgument, "invalid HQ identity").with_source(error))?);
    }
    Ok(values)
}

fn code_fingerprint(market: HqMarket, page_size: usize, category: Option<&crate::types::SecurityCategory>) -> u64 {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "hq:044d".hash(&mut state);
    market.hash(&mut state);
    page_size.hash(&mut state);
    category.hash(&mut state);
    state.finish()
}

fn verify_cursor(cursor: Option<&PageCursor>, fingerprint: u64, page_size: usize,
    context: &OperationContext, metadata_version: Option<u64>) -> Result<()> {
    if let Some(cursor) = cursor {
        if cursor.version() != 1 || cursor.owner_id() != context.session_id
            || cursor.page_size() != page_size || cursor.query_fingerprint() != fingerprint
            || cursor.metadata_version() != metadata_version || cursor.endpoint().is_none() {
            return Err(Error::new(ErrorKind::InvalidArgument, "continuation does not match this owner, query or metadata version"));
        }
    }
    Ok(())
}

fn endpoint_engine(executor: &crate::hq::executor::HqExecutor, config: &ClientConfig, base: &Engine, endpoint: Option<&str>) -> Result<Engine> {
    let Some(endpoint) = endpoint else { return Ok(base.clone()); };
    if !crate::client::configured_hosts(config).iter().any(|host| host == endpoint) {
        return Err(Error::new(ErrorKind::InvalidArgument, "continuation endpoint is outside this client's source catalog"));
    }
    // An endpoint-bound Engine cannot silently fail over to a different source.
    let engine = executor.owner().endpoint_engine(base, endpoint,
        || crate::client::engine_with_hosts(config, vec![endpoint.to_owned()]))?;
    executor.register_engine(engine.clone())?;
    Ok(engine)
}

fn identity_from_market(market: eltdx_protocol::unit::Market, number: &str) -> Result<SecurityId> {
    SecurityId::new(crate::metadata::hq_market(market), number)
        .map_err(|error| Error::new(ErrorKind::Protocol, "response contains an invalid security identity").with_source(error))
}

fn request_metadata(
    executor: &crate::hq::executor::HqExecutor, engine: &Engine, context: &OperationContext,
    config: &ClientConfig, namespace: &str, requests: &[CommandRequest],
) -> Result<std::sync::Arc<MetadataSnapshot>> {
    executor.prepare_metadata(engine, context, config, namespace, requests)
}

/// Domains with intrinsic wire precision can freeze only already-published
/// catalogs. Reuse an outer operation's capture instead of replacing it after
/// another child has decoded or starting an unnecessary metadata request.
fn capture_existing_metadata(context: &OperationContext, namespace: &str, ids: &[SecurityId])
    -> Result<std::sync::Arc<MetadataSnapshot>> {
    if let Some(snapshot) = context.metadata()? { return Ok(snapshot); }
    let mut markets = Vec::new(); let mut snapshots = Vec::new();
    for id in ids {
        if markets.contains(&id.market()) { continue; }
        markets.push(id.market());
        if let Some(snapshot) = context.owner().metadata().current(&crate::metadata::SnapshotKey::hq(namespace, id.market()))? {
            snapshots.push(snapshot);
        }
    }
    let snapshot = context.owner().metadata().capture(namespace, &snapshots)?;
    context.install_metadata(snapshot.clone())?; Ok(snapshot)
}
