use std::sync::Arc;
use std::time::Duration;

use eltdx_protocol::commands::quotes::LegacyQuotesRequest;
use eltdx_protocol::commands::security::{SecurityCountRequest, SecurityListRequest};
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};
use eltdx_protocol::unit::Market;
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::executor::HqExecutor;
use crate::metadata::{MarketCandidate, MetadataRecord, PageEvidence, PrecisionMetadata, SnapshotKey};
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::batch::{Collected, DataPage, EndEvidence, PageCursor, PageOptions};
use crate::types::codes::{query_error, validate_scan_pages};
use crate::types::provenance::ValueProvenance;
use crate::types::{
    Amount, AmountUnit, CodeQuery, CodeScanQuery, Diagnostic, EnrichmentStatus, HqMarket,
    NumericValue, Price, Quantity, QuantityUnit, RawData, ResultMeta, Security, SecurityBoard,
    SecurityCategory, SecurityId, SourceStamp,
};

use super::{aggregate_meta, begin_context, code_fingerprint, result_meta, verify_cursor};

#[derive(Clone, Debug)]
pub struct CodeService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: String,
}

impl CodeService {
    pub(super) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn count(&self, market: HqMarket, options: &OperationOptions) -> Result<u32> {
        let mut context = begin_context(&self.executor, &self.config, "codes_count", options)?;
        let date = self.executor.owner().current_shanghai_date()?;
        let client_date = u32::from(date.year()) * 10_000 + u32::from(date.month()) * 100 + u32::from(date.day());
        let request = CommandRequest::SecurityCount(SecurityCountRequest {
            market: protocol_market(market), client_date,
        });
        let raw = self.executor.execute_raw(&self.engine, &context, request, ChildPurpose::Required, None)?;
        let decoded = self.executor.decode_received(&context, raw)?;
        let CommandResponse::SecurityCount(count) = decoded.response else {
            return Err(context.annotate(Error::new(ErrorKind::Protocol, "security count returned the wrong response"), TimeoutPhase::Decode, Some(0x044e)));
        };
        context.finish()?;
        Ok(u32::from(count))
    }

    pub fn list(&self, market: HqMarket, query: &CodeQuery, options: &OperationOptions) -> Result<DataPage<Security>> {
        let entry = std::time::Instant::now();
        if query.limit() > 1_600 { return Err(query_error("limit", "must be between 0 and 1600")); }
        if query.limit() == 0 {
            return Ok(DataPage::new(Vec::new(), u64::from(query.start()), 0, None,
                EndEvidence::RequestedWindow, empty_meta()));
        }
        let mut context = self.executor.begin_context_at(entry, "codes_list", &self.config, options)?;
        verify_cursor(query.cursor(), market, 1_600, query.limit(), &context)?;
        context.set_work_limits(WorkLimits {
            pages: std::num::NonZeroUsize::new(1), records: Some(query.limit()), bytes: None,
        })?;
        let request = SecurityListRequest::new(protocol_market(market), query.start(), query.limit() as u16)
            .map_err(Error::from_protocol)?;
        let raw = self.executor.execute_raw(&self.engine, &context,
            CommandRequest::SecurityList(request), ChildPurpose::Page, None)?;
        let decoded = self.executor.decode_received(&context, raw)?;
        let CommandResponse::SecurityList(rows) = decoded.response else {
            return Err(context.annotate(Error::new(ErrorKind::Protocol, "security list returned the wrong response"), TimeoutPhase::Decode, Some(0x044d)));
        };
        if rows.len() > query.limit() {
            return Err(context.annotate(Error::new(ErrorKind::Protocol, "security page exceeds requested window"), TimeoutPhase::Decode, Some(0x044d)));
        }
        context.record_work(rows.len(), 0)?;
        let count = u32::try_from(rows.len()).map_err(|_| Error::new(ErrorKind::LimitExceeded, "security page count overflows u32"))?;
        let next_offset = query.start().checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "security page offset overflows u32"))?;
        let market_rows = rows.into_iter().map(|row| security_row(market, row)).collect::<Result<Vec<_>>>()?;
        let ranges = market_rows.iter().map(|row| row.1).collect::<Vec<_>>();
        let source = super::ServiceSource::from_reply(&decoded);
        let meta = result_meta(&context, &self.config, None, &source,
            Some(decoded.payload.clone()), decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "codes");
        let (items, ranges): (Vec<_>, Vec<_>) = market_rows.into_iter().map(|(security, range)| (security, range)).unzip();
        let _ = ranges;
        let end = if count == 0 { EndEvidence::EmptyPage } else { EndEvidence::Unknown };
        let cursor = if count == 0 { None } else {
            Some(PageCursor::new(next_offset, 1_600, code_fingerprint(market, 1_600, query.limit()),
                context.session_id, source.host.clone(), None))
        };
        context.finish()?;
        Ok(DataPage::new(items, u64::from(query.start()), query.limit(), cursor, end, meta))
    }

    pub fn all(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let mut context = begin_context(&self.executor, &self.config, "codes_all", options)?;
        validate_scan_pages(query.pages())?;
        set_scan_limits(&mut context, query.pages(), 1)?;
        let scans = self.scan_markets(&mut context, &[market], query.pages())?;
        let mut items = Vec::new();
        let mut page_count = 0_usize;
        let mut commands = Vec::new();
        let mut candidates = Vec::new();
        for scan in scans {
            page_count = page_count.checked_add(scan.pages).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "page count overflows usize"))?;
            commands.extend(scan.commands);
            candidates.push(scan.candidate);
            items.extend(scan.items);
        }
        let filtered = filter_category(items, query.category());
        let candidates_meta = self.publish_candidates(candidates)?;
        context.finish()?;
        Ok(Collected::new(filtered, EndEvidence::EmptyPage, page_count,
            aggregate_meta(&context, &commands, candidates_meta)))
    }

    pub fn all_markets(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        let mut context = begin_context(&self.executor, &self.config, "codes_all_markets", options)?;
        validate_scan_pages(query.pages())?;
        set_scan_limits(&mut context, query.pages(), 3)?;
        let scans = self.scan_markets(&mut context,
            &[HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing], query.pages())?;
        let mut items = Vec::new();
        let mut page_count = 0_usize;
        let mut commands = Vec::new();
        let mut candidates = Vec::new();
        for scan in scans {
            page_count = page_count.checked_add(scan.pages).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "page count overflows usize"))?;
            commands.extend(scan.commands);
            candidates.push(scan.candidate);
            items.extend(scan.items);
        }
        if items.len() > query.pages().max_records() {
            return Err(scan_limit_error("records", query.pages().max_records(), items.len()));
        }
        let filtered = filter_category(items, query.category());
        let candidates_meta = self.publish_candidates(candidates)?;
        context.finish()?;
        Ok(Collected::new(filtered, EndEvidence::EmptyPage, page_count,
            aggregate_meta(&context, &commands, candidates_meta)))
    }

    pub fn stock_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize> {
        Ok(self.collect_filtered(market, query, options, |item| matches!(item.category(), SecurityCategory::AShare | SecurityCategory::BShare))?.items().len())
    }

    pub fn a_share_count(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<usize> {
        Ok(self.collect_filtered(market, query, options, |item| item.category() == &SecurityCategory::AShare)?.items().len())
    }

    pub fn stocks(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        self.collect_filtered(market, query, options, |item| matches!(item.category(), SecurityCategory::AShare | SecurityCategory::BShare))
    }

    pub fn a_shares(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        self.collect_filtered(market, query, options, |item| item.category() == &SecurityCategory::AShare)
    }

    pub fn etfs(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        self.collect_filtered(market, query, options, |item| item.category() == &SecurityCategory::Etf)
    }

    pub fn indices(&self, market: HqMarket, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<Security>> {
        self.collect_filtered(market, query, options, |item| item.category() == &SecurityCategory::Index)
    }

    pub fn all_stocks(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        map_collected(self.all_markets(query, options)?, |item| matches!(item.category(), SecurityCategory::AShare | SecurityCategory::BShare))
    }

    pub fn all_a_shares(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        map_collected(self.all_markets(query, options)?, |item| item.category() == &SecurityCategory::AShare)
    }

    pub fn all_etfs(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        map_collected(self.all_markets(query, options)?, |item| item.category() == &SecurityCategory::Etf)
    }

    pub fn all_indices(&self, query: &CodeScanQuery, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        map_collected(self.all_markets(query, options)?, |item| item.category() == &SecurityCategory::Index)
    }

    pub fn latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>> {
        let query = CodeScanQuery::default();
        let mut context = begin_context(&self.executor, &self.config, "codes_latest_stock_list", options)?;
        set_scan_limits(&mut context, query.pages(), if market.is_some() { 1 } else { 3 })?;
        let markets = market.map_or_else(|| vec![HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing], |value| vec![value]);
        let scans = self.scan_markets(&mut context, &markets, query.pages())?;
        let (items, pages, commands, candidates) = unpack_scans(scans)?;
        let items = items.into_iter().filter(|item| item.category() == &SecurityCategory::AShare).collect();
        let version = self.publish_candidates(candidates)?;
        context.finish()?;
        Ok(Collected::new(items, EndEvidence::EmptyPage, pages, aggregate_meta(&context, &commands, version)))
    }

    pub fn latest_st(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>> {
        let mut collected = self.latest_stock_list(market, options)?;
        let items = collected.items().iter().filter(|item| is_st_name(item.name())).cloned().collect();
        Ok(Collected::new(items, collected.end(), collected.pages(), collected.meta().clone()))
    }

    pub fn latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<SecurityId>> {
        let mut context = begin_context(&self.executor, &self.config, "codes_latest_suspended", options)?;
        let query = CodeScanQuery::default();
        let markets = market.map_or_else(|| vec![HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing], |value| vec![value]);
        set_scan_limits(&mut context, query.pages(), markets.len())?;
        let scans = self.scan_markets(&mut context, &markets, query.pages())?;
        let (items, pages, mut commands, candidates) = unpack_scans(scans)?;
        let ids = items.into_iter().filter(|item| item.category() == &SecurityCategory::AShare)
            .map(|item| item.id().clone()).collect::<Vec<_>>();
        let requests = legacy_requests(&ids)?;
        if !requests.is_empty() {
            self.executor.prepare_metadata(&self.engine, &context, &self.config, &self.namespace, &requests)?;
        }
        let mut suspended = Vec::new();
        for chunk in ids.chunks(8_000) {
            context.check_boundary(TimeoutPhase::Admission)?;
            let codes = chunk.iter().map(normalize_id).collect::<Result<Vec<_>>>()?;
            let request = LegacyQuotesRequest::new(codes).map_err(Error::from_protocol)?;
            let decoded = self.executor.execute_decoded(&self.engine, &context, &self.config, &self.namespace,
                CommandRequest::LegacyQuotes(request), ChildPurpose::Required, None)?;
            let CommandResponse::LegacyQuotes(rows) = decoded.response else {
                return Err(Error::new(ErrorKind::Protocol, "legacy quote returned the wrong response"));
            };
            for row in rows {
                if row.trading_status_raw & 0x20 != 0 {
                    suspended.push(super::quotes::identity_from_market(row.market, &row.code)?);
                }
            }
            commands.push(0x053e);
            context.record_work(rows.len(), 0)?;
        }
        let version = self.publish_candidates(candidates)?;
        context.finish()?;
        Ok(Collected::new(suspended, EndEvidence::EmptyPage, pages + ids.len().div_ceil(8_000),
            aggregate_meta(&context, &commands, version)))
    }

    fn collect_filtered(
        &self,
        market: HqMarket,
        query: &CodeScanQuery,
        options: &OperationOptions,
        predicate: impl Fn(&Security) -> bool,
    ) -> Result<Collected<Security>> {
        let mut collected = self.all(market, query, options)?;
        let items = collected.items().iter().filter(|item| predicate(item)).cloned().collect();
        Ok(Collected::new(items, collected.end(), collected.pages(), collected.meta().clone()))
    }

    fn scan_markets(&self, context: &mut OperationContext, markets: &[HqMarket], pages: &PageOptions) -> Result<Vec<MarketScan>> {
        let mut scans = Vec::new();
        scans.try_reserve(markets.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "unable to reserve market scan state").with_source(error))?;
        let mut total_records = 0_usize;
        for &market in markets {
            let scan = self.scan_market(context, market, pages)?;
            total_records = total_records.checked_add(scan.items.len())
                .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "code scan record count overflows usize"))?;
            if total_records > pages.max_records() {
                return Err(scan_limit_error("records", pages.max_records(), total_records));
            }
            scans.push(scan);
        }
        Ok(scans)
    }

    fn scan_market(&self, context: &OperationContext, market: HqMarket, pages: &PageOptions) -> Result<MarketScan> {
        let key = SnapshotKey::hq(self.namespace.clone(), market);
        let mut candidate = MarketCandidate::for_code_scan(key, pages.max_records(), pages.max_pages())?;
        let mut offset = 0_u32;
        let mut items = Vec::new();
        let mut commands = Vec::new();
        let mut page_count = 0_usize;
        loop {
            context.check_boundary(TimeoutPhase::Admission)?;
            if page_count >= pages.max_pages() {
                return Err(scan_limit_error("pages", pages.max_pages(), page_count));
            }
            let limit = u16::try_from(pages.page_size()).map_err(|_| query_error("page_size", "exceeds security wire range"))?;
            let request = SecurityListRequest::new(protocol_market(market), offset, limit).map_err(Error::from_protocol)?;
            let decoded = self.executor.execute_decoded(&self.engine, context, &self.config, &self.namespace,
                CommandRequest::SecurityList(request), ChildPurpose::Page, None)?;
            let CommandResponse::SecurityList(rows) = decoded.response else {
                return Err(context.annotate(Error::new(ErrorKind::Protocol, "security page returned the wrong response"), TimeoutPhase::Decode, Some(0x044d)));
            };
            if rows.len() > pages.page_size() {
                return Err(context.annotate(Error::new(ErrorKind::Protocol, "security page exceeds requested limit"), TimeoutPhase::Decode, Some(0x044d)));
            }
            let count = u32::try_from(rows.len()).map_err(|_| Error::new(ErrorKind::LimitExceeded, "security page count overflows u32"))?;
            let evidence = if count == 0 { PageEvidence::TerminalEmpty } else { PageEvidence::More };
            let mut metadata = Vec::new();
            metadata.try_reserve(rows.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "unable to reserve metadata records").with_source(error))?;
            for row in rows {
                let (security, _) = security_row(market, row)?;
                let precision = PrecisionMetadata::new(security.decimal(), ValueProvenance::SecurityTable)?;
                metadata.push(MetadataRecord::new(security.id().clone(), Some(precision), None, None));
                items.push(security);
            }
            candidate.push_page(offset, metadata, evidence)?;
            page_count = page_count.checked_add(1).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "page count overflows usize"))?;
            commands.push(0x044d);
            context.record_work(count as usize, 0)?;
            if count == 0 { break; }
            offset = offset.checked_add(count)
                .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "security list offset overflows u32"))?;
        }
        Ok(MarketScan { items, candidate, pages: page_count, commands })
    }

    fn publish_candidates(&self, candidates: Vec<MarketCandidate>) -> Result<Option<u64>> {
        let mut latest = None;
        for candidate in candidates {
            latest = Some(self.executor.owner().metadata().publish_code_scan(candidate, Duration::from_secs(24 * 60 * 60))?.version());
        }
        Ok(latest)
    }
}

#[derive(Debug)]
struct MarketScan {
    items: Vec<Security>,
    candidate: MarketCandidate,
    pages: usize,
    commands: Vec<u16>,
}

fn unpack_scans(scans: Vec<MarketScan>) -> Result<(Vec<Security>, usize, Vec<u16>, Vec<MarketCandidate>)> {
    let mut items = Vec::new();
    let mut pages = 0_usize;
    let mut commands = Vec::new();
    let mut candidates = Vec::new();
    for scan in scans {
        items.try_reserve(scan.items.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "unable to reserve collected securities").with_source(error))?;
        items.extend(scan.items);
        pages = pages.checked_add(scan.pages).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "page count overflows usize"))?;
        commands.extend(scan.commands);
        candidates.push(scan.candidate);
    }
    Ok((items, pages, commands, candidates))
}

fn map_collected(
    collected: Collected<Security>,
    predicate: impl Fn(&Security) -> bool,
) -> Result<Collected<SecurityId>> {
    let ids = collected.items().iter().filter(|item| predicate(item)).map(|item| item.id().clone()).collect();
    Ok(Collected::new(ids, collected.end(), collected.pages(), collected.meta().clone()))
}

fn filter_category(items: Vec<Security>, category: Option<&SecurityCategory>) -> Vec<Security> {
    match category {
        Some(category) => items.into_iter().filter(|item| item.category() == category).collect(),
        None => items,
    }
}

fn set_scan_limits(context: &mut OperationContext, pages: &PageOptions, markets: usize) -> Result<()> {
    let page_limit = pages.max_pages().checked_mul(markets)
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "page limit overflows usize"))?;
    context.set_work_limits(WorkLimits {
        pages: std::num::NonZeroUsize::new(page_limit),
        records: Some(pages.max_records()), bytes: None,
    })
}

fn security_row(market: HqMarket, row: eltdx_protocol::commands::security::SecurityCode) -> Result<(Security, RecordRange)> {
    if row.market != protocol_market(market) {
        return Err(Error::new(ErrorKind::Protocol, "security row belongs to a different market"));
    }
    let id = SecurityId::new(market, &row.code)
        .map_err(|error| Error::new(ErrorKind::Protocol, "security row has an invalid identity").with_source(error))?;
    if !row.volume_ratio_base.is_finite() || !row.previous_close_price.is_finite() {
        return Err(Error::new(ErrorKind::Protocol, "security row contains a non-finite required value")
            .with_context("security", format!("{id}")));
    }
    let category = match row.category {
        eltdx_protocol::commands::security::SecurityCategory::Index => SecurityCategory::Index,
        eltdx_protocol::commands::security::SecurityCategory::Etf => SecurityCategory::Etf,
        eltdx_protocol::commands::security::SecurityCategory::AShare => SecurityCategory::AShare,
        eltdx_protocol::commands::security::SecurityCategory::BShare => SecurityCategory::BShare,
        eltdx_protocol::commands::security::SecurityCategory::PrivateConvertibleBond => SecurityCategory::PrivateConvertibleBond,
        eltdx_protocol::commands::security::SecurityCategory::Bond => SecurityCategory::Bond,
        eltdx_protocol::commands::security::SecurityCategory::Unknown => SecurityCategory::Unknown,
    };
    let board = match row.board {
        eltdx_protocol::commands::security::SecurityBoard::SseMainBoard => SecurityBoard::SseMain,
        eltdx_protocol::commands::security::SecurityBoard::SseStarMarket => SecurityBoard::Star,
        eltdx_protocol::commands::security::SecurityBoard::SzseMainBoard => SecurityBoard::SzseMain,
        eltdx_protocol::commands::security::SecurityBoard::SzseChinext => SecurityBoard::ChiNext,
        eltdx_protocol::commands::security::SecurityBoard::BseListedStock => SecurityBoard::Beijing,
        eltdx_protocol::commands::security::SecurityBoard::None => SecurityBoard::None,
    };
    Ok((Security::new(id, row.name, row.multiple, row.decimal,
        Price::with_value_and_provenance(NumericValue::F32(row.previous_close_price), ValueProvenance::SecurityTable),
        f64::from(row.volume_ratio_base), category, row.category_reason.to_owned(), board,
        row.board_reason().to_owned()), row.record_range))
}

fn empty_meta() -> ResultMeta {
    ResultMeta::new(SourceStamp::new(None, None, String::new(), None, None), Vec::new(), None)
}

fn empty_collected<T>(items: Vec<T>) -> Collected<T> {
    Collected::new(items, EndEvidence::EmptyPage, 0, empty_meta())
}

fn filter_owned(items: Vec<Security>, predicate: impl Fn(&Security) -> bool) -> Vec<Security> {
    items.into_iter().filter(predicate).collect()
}

fn legacy_requests(ids: &[SecurityId]) -> Result<Vec<CommandRequest>> {
    let mut requests = Vec::new();
    for chunk in ids.chunks(8_000) {
        let codes = chunk.iter().map(normalize_id).collect::<Result<Vec<_>>>()?;
        requests.push(CommandRequest::LegacyQuotes(LegacyQuotesRequest::new(codes).map_err(Error::from_protocol)?));
    }
    Ok(requests)
}

fn normalize_id(id: &SecurityId) -> Result<eltdx_protocol::unit::NormalizedCode> {
    eltdx_protocol::unit::NormalizedCode::parse(&id.to_string())
        .map_err(|error| Error::new(ErrorKind::InvalidArgument, "invalid HQ identity").with_source(error))
}

fn is_st_name(name: &str) -> bool {
    let value = name.trim().to_ascii_uppercase();
    ["ST", "*ST", "SST", "S*ST"].iter().any(|prefix| value.starts_with(prefix))
}

fn scan_limit_error(resource: &'static str, limit: usize, actual: usize) -> Error {
    Error::new(ErrorKind::LimitExceeded, "complete security scan reached its configured work cap")
        .with_context("resource", resource).with_context("limit", format!("{limit}"))
        .with_context("actual", format!("{actual}"))
}

fn protocol_market(market: HqMarket) -> Market {
    match market { HqMarket::Shenzhen => Market::Shenzhen, HqMarket::Shanghai => Market::Shanghai, HqMarket::Beijing => Market::Beijing }
}
