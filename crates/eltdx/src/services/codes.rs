use std::collections::HashSet;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::quotes::LegacyQuotesRequest;
use eltdx_protocol::commands::security::{SecurityCountRequest, SecurityListRequest};
use eltdx_protocol::unit::Market;
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::metadata::{MarketCandidate, MetadataRecord, PageEvidence, PrecisionMetadata, SnapshotKey};
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::codes::{query_error, validate_scan_pages};
use crate::types::{CodeQuery, CodeScanQuery, Collected, DataPage, EndEvidence, HqMarket,
    NumericValue, PageCursor, Price, ResultMeta, Security, SecurityBoard, SecurityCategory, SecurityId, ValueProvenance};

use super::{aggregate_meta, allocation_error, endpoint_engine, identity_from_market,
    normalize_ids, result_meta, verify_cursor, ServiceSource};
use super::pagination::{advance_offset, CollectionLimits, PageProgress};
use super::code_fingerprint;

const ALL_MARKETS: [HqMarket; 3] = [HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing];

#[derive(Clone, Copy)]
enum DirectoryValidation {
    Strict,
    Legacy,
}

/// One directory algorithm for strict Rust collectors and the native legacy
/// bridge. The callback owns transport/ABI policy; pagination owns progress.
pub(crate) fn walk_directory(
    context: &OperationContext, market: HqMarket, start: u32, page_size: usize,
    limits: CollectionLimits, validation: DirectoryValidation, progress: &mut PageProgress,
    mut fetch: impl FnMut(u32) -> Result<DecodedReply>,
    mut consume: impl FnMut(u32, DecodedReply) -> Result<()>,
) -> Result<()> {
    if !(1..=1_600).contains(&page_size) { return Err(query_error("page_size", "must be between 1 and 1600")); }
    let mut offset = start;
    let mut seen = HashSet::new();
    loop {
        context.check_boundary(TimeoutPhase::Admission)?;
        progress.before_request(limits)?;
        let decoded = fetch(offset)?;
        let CommandResponse::SecurityList(rows) = &decoded.response else { return Err(wrong_response()); };
        match validation {
            DirectoryValidation::Strict => {
                progress.accept(rows.len(), page_size, limits)?;
                seen.try_reserve(rows.len()).map_err(allocation_error)?;
                for row in rows {
                    if row.market != protocol_market(market) { return Err(Error::new(ErrorKind::Protocol, "directory row has a foreign market")); }
                    let id = identity_from_market(row.market, &row.code)?;
                    if !seen.insert(id.clone()) { return Err(duplicate_identity(&id)); }
                }
            }
            DirectoryValidation::Legacy => progress.accept_legacy(rows.len())?,
        }
        let count = rows.len();
        context.record_work(count, 0)?;
        consume(offset, decoded)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if count == 0 { return Ok(()); }
        offset = advance_offset(offset, count)?;
    }
}

pub(crate) fn legacy_directory_pages(
    context: &OperationContext, market: HqMarket, page_size: usize,
    fetch: impl FnMut(u32) -> Result<DecodedReply>,
) -> Result<Vec<DecodedReply>> {
    if !context.is_legacy() { return Err(query_error("profile", "legacy directory requires a compatibility operation")); }
    let mut replies = Vec::new();
    walk_directory(context, market, 0, page_size, CollectionLimits { pages: None, records: None },
        DirectoryValidation::Legacy,
        &mut PageProgress::default(), fetch, |_, reply| {
            replies.try_reserve(1).map_err(allocation_error)?;
            replies.push(reply);
            Ok(())
        })?;
    Ok(replies)
}

pub(crate) fn publish_legacy_directory_metadata(
    context: &OperationContext,
    namespace: &str,
    market: HqMarket,
    replies: &[DecodedReply],
) -> Result<()> {
    if !context.is_legacy() {
        return Err(query_error("profile", "legacy directory metadata requires a compatibility operation"));
    }
    let mut candidate = MarketCandidate::for_legacy_scan(SnapshotKey::hq(namespace, market))?;
    let mut offset = 0u32;
    for reply in replies {
        let CommandResponse::SecurityList(rows) = &reply.response else { return Err(wrong_response()); };
        let mut records = Vec::new();
        records.try_reserve(rows.len()).map_err(allocation_error)?;
        for row in rows {
            let id = identity_from_market(row.market, &row.code)?;
            let precision = PrecisionMetadata::new(row.decimal, ValueProvenance::SecurityTable)?;
            records.push(MetadataRecord::new(id, Some(precision), None, None));
        }
        candidate.push_page(offset, records, if rows.is_empty() { PageEvidence::TerminalEmpty } else { PageEvidence::More })?;
        if !rows.is_empty() {
            offset = advance_offset(offset, rows.len())?;
        }
    }
    context.owner().metadata().publish_code_scans(
        vec![candidate],
        Duration::from_secs(24 * 60 * 60),
        context,
    )?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CodeService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: String,
}

impl CodeService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn count(&self, market: HqMarket, options: &OperationOptions) -> Result<u32> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "codes_count", &self.config, options)?;
        let date = self.executor.owner().current_shanghai_date()?;
        let client_date = u32::from(date.year()) * 10_000 + u32::from(date.month()) * 100 + u32::from(date.day());
        let raw = self.executor.execute_raw(&self.engine, &context,
            CommandRequest::SecurityCount(SecurityCountRequest { market: protocol_market(market), client_date }),
            ChildPurpose::Required, None)?;
        let decoded = self.executor.decode_unpriced(&context, raw)?;
        let CommandResponse::SecurityCount(count) = decoded.response else { return Err(wrong_response()); };
        context.finish()?;
        Ok(u32::from(count))
    }

    pub fn list(&self, market: HqMarket, query: &CodeQuery, options: &OperationOptions) -> Result<DataPage<Security>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "codes_list", &self.config, options)?;
        if query.limit() > 1_600 { return Err(query_error("limit", "must be between 0 and 1600")); }
        let version = self.executor.owner().metadata().catalog_version(&self.namespace, &[market])?;
        verify_cursor(query.cursor(), code_fingerprint(market, query.limit(), None), query.limit(), &context, version)?;
        if query.limit() == 0 {
            context.finish()?;
            return Ok(DataPage::new(Vec::new(), u64::from(query.start()), 0, None,
                EndEvidence::RequestedWindow, ResultMeta::default()));
        }
        context.set_work_limits(WorkLimits {
            pages: std::num::NonZeroUsize::new(1), records: Some(query.limit()), bytes: None,
        })?;
        let engine = endpoint_engine(&self.executor, &self.config, &self.engine, query.cursor().and_then(PageCursor::endpoint))?;
        let decoded = self.fetch_page(&engine, &context, market, query.start(), query.limit(), ChildPurpose::Required)?;
        let CommandResponse::SecurityList(rows) = &decoded.response else { return Err(wrong_response()); };
        validate_page_size(rows.len(), query.limit())?;
        context.record_work(rows.len(), 0)?;
        let next_offset = advance_offset(query.start(), rows.len())?;
        let mut items = Vec::new();
        items.try_reserve(rows.len()).map_err(allocation_error)?;
        let mut seen = HashSet::new();
        seen.try_reserve(rows.len()).map_err(allocation_error)?;
        for row in rows {
            let (security, _) = security_row(market, row)?;
            if !seen.insert(security.id().clone()) { return Err(duplicate_identity(security.id())); }
            items.push(security);
        }
        let source = ServiceSource::from_reply(&decoded);
        let ranges = rows.iter().map(|row| row.record_range).collect::<Vec<_>>();
        let meta = result_meta(&context, &self.config, None, &source, Some(decoded.payload.clone()),
            decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "codes.list");
        let (end, cursor) = if items.is_empty() { (EndEvidence::EmptyPage, None) } else {
            (EndEvidence::Unknown, Some(PageCursor::new(next_offset, query.limit(), code_fingerprint(market, query.limit(), None),
                context.session_id, Some(source.host), version)))
        };
        context.finish()?;
        Ok(DataPage::new(items, u64::from(query.start()), query.limit(), cursor, end, meta))
    }

    pub fn all(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_all", &[market], query, options, |_| true)
    }

    pub fn all_markets(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_all_markets", &ALL_MARKETS, query, options, |_| true)
    }

    pub fn stock_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize> {
        let entry = Instant::now();
        Ok(self.collect_at(entry, "codes_stock_count", &[market], query, options, is_stock)?.items().len())
    }

    pub fn a_share_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize> {
        let entry = Instant::now();
        Ok(self.collect_at(entry, "codes_a_share_count", &[market], query, options, is_a_share)?.items().len())
    }

    pub fn stocks(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_stocks", &[market], query, options, is_stock)
    }

    pub fn a_shares(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_a_shares", &[market], query, options, is_a_share)
    }

    pub fn etfs(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_etfs", &[market], query, options, |item| item.category() == &SecurityCategory::Etf)
    }

    pub fn indices(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        self.collect_at(entry, "codes_indices", &[market], query, options, |item| item.category() == &SecurityCategory::Index)
    }

    pub fn all_stocks(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let entry = Instant::now();
        self.collect_ids_at(entry, "codes_all_stocks", query, options, is_stock)
    }

    pub fn all_a_shares(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let entry = Instant::now();
        self.collect_ids_at(entry, "codes_all_a_shares", query, options, is_a_share)
    }

    pub fn all_etfs(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let entry = Instant::now();
        self.collect_ids_at(entry, "codes_all_etfs", query, options, |item| item.category() == &SecurityCategory::Etf)
    }

    pub fn all_indices(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let entry = Instant::now();
        self.collect_ids_at(entry, "codes_all_indices", query, options, |item| item.category() == &SecurityCategory::Index)
    }

    pub fn latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        let markets = market.map_or_else(|| ALL_MARKETS.to_vec(), |value| vec![value]);
        self.collect_at(entry, "codes_latest_stock_list", &markets, &CodeScanQuery::default(), options, is_a_share)
    }

    pub fn latest_st(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>> {
        let entry = Instant::now();
        let markets = market.map_or_else(|| ALL_MARKETS.to_vec(), |value| vec![value]);
        self.collect_at(entry, "codes_latest_st", &markets, &CodeScanQuery::default(), options,
            |item| is_a_share(item) && is_st_name(item.name()))
    }

    pub fn latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "codes_latest_suspended", &self.config, options)?;
        let query = CodeScanQuery::default();
        let markets = market.map_or_else(|| ALL_MARKETS.to_vec(), |value| vec![value]);
        let mut scan = self.scan(&context, &markets, &query)?;
        let snapshots = self.publish_candidates(&context, std::mem::take(&mut scan.candidates))?;
        let metadata = self.executor.owner().metadata().capture(&self.namespace, &snapshots)?;
        context.install_metadata(metadata.clone())?;
        let ids = scan.items.iter().filter(|item| is_a_share(item)).map(|item| item.id().clone()).collect::<Vec<_>>();
        let normalized = normalize_ids(&ids)?;
        let mut requests = Vec::new();
        requests.try_reserve(normalized.len().div_ceil(8_000)).map_err(allocation_error)?;
        for chunk in normalized.chunks(8_000) {
            requests.push(CommandRequest::LegacyQuotes(LegacyQuotesRequest::new(chunk.to_vec()).map_err(Error::from_protocol)?));
        }
        self.executor.prepare_metadata(&self.engine, &context, &self.config, &self.namespace, &requests)?;
        let mut suspended = Vec::new();
        suspended.try_reserve(ids.len()).map_err(allocation_error)?;
        for (chunk, request) in ids.chunks(8_000).zip(requests) {
            let decoded = self.executor.execute_decoded(&self.engine, &context, &self.config, &self.namespace,
                request, ChildPurpose::Page, None)?;
            let CommandResponse::LegacyQuotes(rows) = &decoded.response else { return Err(wrong_response()); };
            let order = super::quotes::occurrence_order(chunk, rows.iter().map(|row| identity_from_market(row.market, &row.code)))?;
            for index in order {
                let row = &rows[index];
                if row.trading_status_raw & 0x20 != 0 { suspended.push(identity_from_market(row.market, &row.code)?); }
            }
            context.record_work(rows.len(), 0)?;
            scan.metas.push(result_meta(&context, &self.config, Some(&metadata), &ServiceSource::from_reply(&decoded),
                Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()),
                &rows.iter().map(|row| row.record_range).collect::<Vec<_>>(), "codes.suspended"));
            scan.pages = scan.pages.checked_add(1).ok_or_else(offset_overflow)?;
        }
        let meta = aggregate_meta(&context, &scan.metas, Some(metadata.version()))?;
        context.finish()?;
        Ok(Collected::new(suspended, EndEvidence::EmptyPage, scan.pages, meta))
    }

    fn collect_at(&self, entry: Instant, name: &'static str, markets: &[HqMarket], query: &CodeScanQuery,
        options: &OperationOptions, predicate: impl Fn(&Security) -> bool) -> Result<Collected<Security>> {
        let context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        let collected = self.collect_in_context(&context, markets, query, predicate)?;
        context.finish()?;
        Ok(collected)
    }

    fn collect_in_context(&self, context: &OperationContext, markets: &[HqMarket], query: &CodeScanQuery,
        predicate: impl Fn(&Security) -> bool) -> Result<Collected<Security>> {
        let mut scan = self.scan(&context, markets, query)?;
        scan.items.retain(|item| query.category().map_or(true, |category| item.category() == category) && predicate(item));
        let catalog_version = scan.catalog_version;
        let snapshots = self.publish_candidates(&context, scan.candidates)?;
        let version = snapshots.iter().map(|snapshot| snapshot.version()).max().or(catalog_version);
        let meta = aggregate_meta(&context, &scan.metas, version)?;
        Ok(Collected::new(scan.items, EndEvidence::EmptyPage, scan.pages, meta))
    }

    fn collect_ids_at(&self, entry: Instant, name: &'static str, query: &CodeScanQuery,
        options: &OperationOptions, predicate: impl Fn(&Security) -> bool) -> Result<Collected<SecurityId>> {
        let context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        let collected = self.collect_in_context(&context, &ALL_MARKETS, query, predicate)?;
        let pages = collected.pages();
        let meta = collected.meta().clone();
        let mut ids = Vec::new();
        ids.try_reserve(collected.items().len()).map_err(allocation_error)?;
        ids.extend(collected.into_items().into_iter().map(|item| item.id().clone()));
        context.finish()?;
        Ok(Collected::new(ids, EndEvidence::EmptyPage, pages, meta))
    }

    fn fetch_page(&self, engine: &Engine, context: &OperationContext, market: HqMarket, offset: u32,
        size: usize, purpose: ChildPurpose) -> Result<DecodedReply> {
        let limit = u16::try_from(size).map_err(|_| query_error("page_size", "exceeds the wire range"))?;
        let request = SecurityListRequest::new(protocol_market(market), offset, limit).map_err(Error::from_protocol)?;
        let raw = self.executor.execute_raw(engine, context, CommandRequest::SecurityList(request), purpose, None)?;
        self.executor.decode_unpriced(context, raw)
    }

    fn scan(&self, context: &OperationContext, markets: &[HqMarket], query: &CodeScanQuery) -> Result<ScanResult> {
        validate_scan_pages(query.pages())?;
        if query.cursor().is_some() && markets.len() != 1 {
            return Err(query_error("cursor", "directory continuation requires its original single-market query"));
        }
        let version = self.executor.owner().metadata().catalog_version(&self.namespace, markets)?;
        if let Some(&market) = markets.first() {
            verify_cursor(query.cursor(), code_fingerprint(market, query.pages().page_size(), query.category()),
                query.pages().page_size(), context, version)?;
        }
        let mut engine = endpoint_engine(&self.executor, &self.config, &self.engine, query.cursor().and_then(PageCursor::endpoint))?;
        let mut bound_host = query.cursor().and_then(PageCursor::endpoint).map(str::to_owned);
        let mut result = ScanResult { catalog_version: version, ..ScanResult::default() };
        let mut progress = PageProgress::default();
        let mut first_request = true;
        let limits = CollectionLimits { pages: Some(query.pages().max_pages()), records: Some(query.pages().max_records()) };
        for &market in markets {
            let offset = query.cursor().map_or(0, PageCursor::next_offset);
            let mut candidate = if offset == 0 { Some(MarketCandidate::for_code_scan(
                SnapshotKey::hq(&self.namespace, market), query.pages().max_records(), query.pages().max_pages())?) } else { None };
            walk_directory(context, market, offset, query.pages().page_size(), limits,
                DirectoryValidation::Strict, &mut progress, |offset| {
                let purpose = if first_request {
                    first_request = false;
                    ChildPurpose::Required
                } else {
                    ChildPurpose::Page
                };
                let decoded = self.fetch_page(&engine, context, market, offset, query.pages().page_size(), purpose)?;
                let source = ServiceSource::from_reply(&decoded);
                if let Some(host) = &bound_host {
                    if host != &source.host { return Err(Error::new(ErrorKind::Connection, "directory source changed during pagination")); }
                } else {
                    engine = endpoint_engine(&self.executor, &self.config, &self.engine, Some(&source.host))?;
                    bound_host = Some(source.host.clone());
                }
                Ok(decoded)
            }, |offset, decoded| {
                let source = ServiceSource::from_reply(&decoded);
                let CommandResponse::SecurityList(rows) = &decoded.response else { return Err(wrong_response()); };
                result.items.try_reserve(rows.len()).map_err(allocation_error)?;
                let mut records = Vec::new();
                records.try_reserve(rows.len()).map_err(allocation_error)?;
                for row in rows {
                    let (security, _) = security_row(market, row)?;
                    if candidate.is_some() {
                        records.push(MetadataRecord::new(security.id().clone(), Some(PrecisionMetadata::new(
                            security.decimal(), ValueProvenance::SecurityTable)?), None, None));
                    }
                    result.items.push(security);
                }
                if let Some(candidate) = &mut candidate {
                    candidate.push_page(offset, records, if rows.is_empty() { PageEvidence::TerminalEmpty } else { PageEvidence::More })?;
                }
                result.metas.try_reserve(1).map_err(allocation_error)?;
                result.metas.push(result_meta(context, &self.config, None, &source, Some(decoded.payload.clone()),
                    decoded.wire.as_ref().map(|wire| wire.raw.clone()),
                    &rows.iter().map(|row| row.record_range).collect::<Vec<_>>(), "codes.scan"));
                Ok(())
            })?;
            if let Some(candidate) = candidate { result.candidates.push(candidate); }
        }
        result.pages = progress.pages();
        Ok(result)
    }

    fn publish_candidates(&self, context: &OperationContext, candidates: Vec<MarketCandidate>) -> Result<Vec<std::sync::Arc<crate::metadata::MetadataSnapshot>>> {
        self.executor.owner().metadata().publish_code_scans(candidates, Duration::from_secs(86_400), context)
    }
}

#[derive(Default)]
struct ScanResult {
    items: Vec<Security>,
    metas: Vec<ResultMeta>,
    candidates: Vec<MarketCandidate>,
    pages: usize,
    catalog_version: Option<u64>,
}

fn security_row(market: HqMarket, row: &eltdx_protocol::commands::security::SecurityCode) -> Result<(Security, RecordRange)> {
    if row.market != protocol_market(market) { return Err(Error::new(ErrorKind::Protocol, "security row has a foreign market")); }
    let id = identity_from_market(row.market, &row.code)?;
    if !row.volume_ratio_base.is_finite() || !row.previous_close_price.is_finite() {
        return Err(Error::new(ErrorKind::Protocol, "security row contains a non-finite required value").with_context("security", format!("{id}")));
    }
    use eltdx_protocol::commands::security::{SecurityCategory as C, SecurityBoard as B};
    let category = match row.category { C::Index => SecurityCategory::Index, C::Etf => SecurityCategory::Etf,
        C::AShare => SecurityCategory::AShare, C::BShare => SecurityCategory::BShare,
        C::PrivateConvertibleBond => SecurityCategory::PrivateConvertibleBond, C::Bond => SecurityCategory::Bond,
        C::Unknown => SecurityCategory::Unknown };
    let board = match row.board { B::SseMainBoard => SecurityBoard::SseMain, B::SseStarMarket => SecurityBoard::Star,
        B::SzseMainBoard => SecurityBoard::SzseMain, B::SzseChinext => SecurityBoard::ChiNext,
        B::BseListedStock => SecurityBoard::Beijing, B::None => SecurityBoard::None };
    Ok((Security::new(id, row.name.clone(), row.multiple, row.decimal,
        Price::with_value_and_provenance(NumericValue::F32(row.previous_close_price), ValueProvenance::SecurityTable),
        f64::from(row.volume_ratio_base), category, row.category_reason.to_owned(), board, row.board_reason().to_owned()), row.record_range))
}

fn is_stock(item: &Security) -> bool { matches!(item.category(), SecurityCategory::AShare | SecurityCategory::BShare) }
fn is_a_share(item: &Security) -> bool { item.category() == &SecurityCategory::AShare }
fn is_st_name(name: &str) -> bool {
    let name = name.trim().to_ascii_uppercase();
    ["ST", "*ST", "SST", "S*ST"].iter().any(|prefix| name.starts_with(prefix))
}
fn offset_overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "security page offset or page count overflows") }
fn wrong_response() -> Error { Error::new(ErrorKind::Protocol, "security request returned the wrong response kind") }
fn duplicate_identity(id: &SecurityId) -> Error {
    Error::new(ErrorKind::Protocol, "directory scan returned a repeated security identity").with_context("security", format!("{id}"))
}
fn validate_page_size(count: usize, limit: usize) -> Result<()> {
    if count > limit { return Err(Error::new(ErrorKind::Protocol, "security page exceeds requested limit")); }
    Ok(())
}
fn protocol_market(market: HqMarket) -> Market {
    match market { HqMarket::Shenzhen => Market::Shenzhen, HqMarket::Shanghai => Market::Shanghai, HqMarket::Beijing => Market::Beijing }
}
