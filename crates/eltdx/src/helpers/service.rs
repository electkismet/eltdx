use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, allocation_error, checked_ids, is_a_share, is_st_name,
    CodeService, MinuteService, QuoteService};
use crate::services::pagination::{advance_offset, CollectionLimits, PageProgress};
use crate::types::{AuxKind, BatchItem, BatchItemState, BatchOutcome, CategoryQuery, CodeScanQuery,
    Collected, Diagnostic, EndEvidence, EnrichmentStatus, FullQuote, HqMarket, MinuteAuxSeries,
    MissingPolicy, RankQuery, RealtimeRankRow, Security, SecurityId, Snapshot,
    Table};
use crate::{Error, ErrorKind, Result, TimeoutPhase};

const MARKETS: [HqMarket; 3] = [HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing];
const FULL_QUOTE_CHUNK: usize = 80;

/// HQ business operations. Getters and cloning do not start the runtime or
/// touch the filesystem. Every network composition owns one operation context.
#[derive(Clone, Debug)]
pub struct HelperService {
    pub(super) executor: HqExecutor, pub(super) engine: Engine,
    pub(super) config: ClientConfig, pub(super) namespace: String,
    pub(super) pools: Arc<crate::hq::domains::DomainPools>,
}

impl HelperService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String,
        pools: Arc<crate::hq::domains::DomainPools>) -> Self {
        Self { executor, engine, config, namespace, pools }
    }

    pub(super) fn quotes(&self) -> QuoteService {
        QuoteService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone())
    }
    pub(super) fn codes(&self) -> CodeService {
        CodeService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone())
    }
    fn minutes(&self) -> MinuteService {
        MinuteService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone())
    }

    pub fn full_quotes(&self, ids: &[SecurityId], options: &OperationOptions) -> Result<BatchOutcome<FullQuote>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "helpers_full_quotes", &self.config, options)?;
        checked_ids(ids)?;
        context.set_work_limits(WorkLimits { pages: None,
            records: Some(ids.len().checked_mul(2).ok_or_else(helper_overflow)?), bytes: None })?;
        let result = self.full_quotes_in_context(&context, ids)?;
        context.finish()?;
        Ok(result)
    }

    pub(crate) fn full_quotes_in_context(&self, context: &OperationContext, ids: &[SecurityId])
        -> Result<BatchOutcome<FullQuote>> {
        if ids.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        let quotes = self.quotes();
        // The first chunk must not freeze precision for only one of several
        // markets present in the caller's complete occurrence list.
        let metadata = quotes.prepare_in_context(context, ids)?;
        let mut items = Vec::new();
        let mut metas = Vec::new();
        items.try_reserve(ids.len()).map_err(allocation_error)?;
        metas.try_reserve(ids.len().div_ceil(FULL_QUOTE_CHUNK) * 2).map_err(allocation_error)?;
        for (chunk_index, chunk) in ids.chunks(FULL_QUOTE_CHUNK).enumerate() {
            context.check_boundary(TimeoutPhase::Admission)?;
            let primary = quotes.snapshots_in_context(context, chunk, MissingPolicy::Error, FULL_QUOTE_CHUNK)?;
            let (rows, primary_meta) = primary.into_parts();
            metas.push(primary_meta);
            let depth = quotes.depth_in_context(context, chunk, ChildPurpose::Fallback);
            // A shorter optional-source failure may degrade. Cancellation,
            // owner closure or the outer absolute deadline must always escape.
            if let Err(error) = &depth { optional_failure(context, error)?; }
            let mut by_identity = HashMap::<SecurityId, VecDeque<Snapshot>>::new();
            by_identity.try_reserve(chunk.len()).map_err(allocation_error)?;
            let failure = match depth {
                Ok(depth) => {
                    let (_, updates, meta) = depth.into_parts();
                    metas.push(meta);
                    for update in updates {
                        let bucket = by_identity.entry(update.id().clone()).or_default();
                        bucket.try_reserve(1).map_err(allocation_error)?;
                        bucket.push_back(update.snapshot().clone());
                    }
                    None
                }
                Err(error) => Some(error),
            };
            for row in rows {
                let (local_index, id, state) = row.into_parts();
                let BatchItemState::Present(snapshot) = state else {
                    return Err(Error::new(ErrorKind::MissingData, "full quote is missing its required snapshot")
                        .with_context("security", format!("{id}")));
                };
                let depth = by_identity.get_mut(&id).and_then(VecDeque::pop_front);
                let full = merge_depth(snapshot, depth.as_ref(), failure.as_ref())?;
                let index = chunk_index.checked_mul(FULL_QUOTE_CHUNK)
                    .and_then(|base| base.checked_add(local_index)).ok_or_else(helper_overflow)?;
                items.push(BatchItem::new(index, id, BatchItemState::Present(full)));
            }
        }
        let mut meta = aggregate_meta(context, &metas, Some(metadata.version()))?;
        for item in &items {
            if let BatchItemState::Present(full) = item.state() {
                for diagnostic in full.diagnostics() { meta = meta.with_diagnostic(diagnostic.clone()); }
            }
        }
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(BatchOutcome::with_meta(items, meta))
    }

    pub fn latest_stock_list(&self, market: Option<HqMarket>, options: &OperationOptions)
        -> Result<Collected<Security>> {
        self.directory(Instant::now(), "helpers_latest_stock_list", market, options, is_a_share)
    }

    pub fn latest_st(&self, market: Option<HqMarket>, options: &OperationOptions) -> Result<Collected<Security>> {
        self.directory(Instant::now(), "helpers_latest_st", market, options,
            |security| is_a_share(security) && is_st_name(security.name()))
    }

    fn directory(&self, entry: Instant, name: &'static str, market: Option<HqMarket>,
        options: &OperationOptions, predicate: impl Fn(&Security) -> bool) -> Result<Collected<Security>> {
        let context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        let markets = market.map_or_else(|| MARKETS.to_vec(), |market| vec![market]);
        let result = self.codes().collect_in_context(&context, &markets, &CodeScanQuery::default(), predicate)?;
        context.finish()?;
        Ok(result)
    }

    pub fn latest_suspended(&self, market: Option<HqMarket>, options: &OperationOptions)
        -> Result<Collected<Security>> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "helpers_latest_suspended", &self.config, options)?;
        let markets = market.map_or_else(|| MARKETS.to_vec(), |market| vec![market]);
        let catalog = self.codes().collect_in_context(&context, &markets, &CodeScanQuery::default(), is_a_share)?;
        let mut ids = Vec::new();
        ids.try_reserve(catalog.items().len()).map_err(allocation_error)?;
        ids.extend(catalog.items().iter().map(|security| security.id().clone()));
        let quotes = self.quotes().legacy_in_context(&context, &ids)?;
        let pages = catalog.pages().checked_add(ids.len().div_ceil(300)).ok_or_else(helper_overflow)?;
        let meta = aggregate_meta(&context, &[catalog.meta().clone(), quotes.meta().clone()],
            quotes.meta().source().metadata_version())?;
        let mut selected = Vec::new();
        selected.try_reserve(ids.len()).map_err(allocation_error)?;
        // The quote service has already validated and restored input occurrences.
        for (security, quote) in catalog.into_items().into_iter().zip(quotes.into_items()) {
            match quote.state() {
                BatchItemState::Present(quote) if quote.is_suspended() => selected.push(security),
                BatchItemState::Present(_) => {},
                BatchItemState::ConfirmedMissing(_) => return Err(Error::new(ErrorKind::MissingData,
                    "suspension classification is missing its required status quote")),
            }
        }
        context.finish()?;
        Ok(Collected::new(selected, EndEvidence::EmptyPage, pages, meta))
    }

    pub fn buy_sell_strength(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries> {
        self.aux(Instant::now(), "helpers_buy_sell_strength", id, AuxKind::BuySellStrength, options)
    }

    pub fn volume_comparison(&self, id: &SecurityId, options: &OperationOptions) -> Result<MinuteAuxSeries> {
        self.aux(Instant::now(), "helpers_volume_comparison", id, AuxKind::VolumeComparison, options)
    }

    fn aux(&self, entry: Instant, name: &'static str, id: &SecurityId, kind: AuxKind,
        options: &OperationOptions) -> Result<MinuteAuxSeries> {
        let mut context = self.executor.begin_context_at(entry, name, &self.config, options)?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(65_535), bytes: None })?;
        let series = self.minutes().aux_in_context(&context, id, kind)?;
        context.finish()?;
        Ok(series)
    }

    pub fn realtime_rank(&self, query: &RankQuery, options: &OperationOptions) -> Result<Table<RealtimeRankRow>> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "helpers_realtime_rank", &self.config, options)?;
        // Directory enrichment is optional and precedes the immutable quote
        // precision capture. A failed scan never becomes a successful empty map.
        let names = self.codes().collect_in_context(&context, &MARKETS, &CodeScanQuery::default(), |_| true);
        let mut name_map = HashMap::new();
        let mut metas = Vec::new();
        let mut diagnostics = Vec::new();
        match names {
            Ok(catalog) => {
                name_map.try_reserve(catalog.items().len()).map_err(allocation_error)?;
                metas.push(catalog.meta().clone());
                for security in catalog.into_items() { name_map.insert(security.id().clone(), security.name().to_owned()); }
            }
            Err(error) => {
                optional_failure(&context, &error)?;
                diagnostics.push(failed_diagnostic("hq.directory", "rank.names", &error));
            }
        }
        let mut progress = PageProgress::default();
        let limits = CollectionLimits { pages: Some(query.pages().max_pages()), records: Some(query.pages().max_records()) };
        let mut rows = Vec::new();
        let mut offset = 0_u32;
        let mut endpoint = None;
        let quotes = self.quotes();
        let end = loop {
            context.check_boundary(TimeoutPhase::Admission)?;
            if query.count().is_some_and(|count| rows.len() >= count) { break EndEvidence::RequestedWindow; }
            progress.before_request(limits)?;
            let count = query.count().map_or(query.pages().page_size(), |target|
                query.pages().page_size().min(target - rows.len()));
            let start = u16::try_from(offset).map_err(|_| helper_overflow())?;
            let page_query = CategoryQuery::new().with_category(query.category())?.with_sort(query.sort())?
                .with_ascending(query.ascending())?.with_start(start)?.with_count(count)?;
            let purpose = if progress.pages() == 0 { ChildPurpose::Required } else { ChildPurpose::Page };
            let page = quotes.category_in_context(&context, &page_query, endpoint.as_deref(), purpose)?;
            progress.accept(page.items().len(), count, limits)?;
            let host = page.meta().source().endpoint().ok_or_else(|| Error::new(ErrorKind::Protocol,
                "rank response does not identify its endpoint"))?;
            let host = format!("{host}");
            if endpoint.as_ref().is_some_and(|expected| expected != &host) {
                return Err(Error::new(ErrorKind::Connection, "rank source changed during collection"));
            }
            endpoint = Some(host);
            metas.try_reserve(1).map_err(allocation_error)?;
            metas.push(page.meta().clone());
            if page.items().is_empty() { break EndEvidence::EmptyPage; }
            offset = advance_offset(offset, page.items().len())?;
            rows.try_reserve(page.items().len()).map_err(allocation_error)?;
            for quote in page.into_items() {
                let name = name_map.get(quote.snapshot().id()).cloned();
                let seal_amount = quote.seal_amount().ok_or_else(|| Error::new(ErrorKind::Protocol,
                    "rank seal amount exceeds finite CNY range"))?;
                rows.push(RealtimeRankRow { rank: rows.len().checked_add(1).ok_or_else(helper_overflow)?, name, quote, seal_amount });
            }
            // No identity deduplication, sorting or short-page EOF assumption.
        };
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let mut meta = aggregate_meta(&context, &metas, version)?;
        for diagnostic in diagnostics { meta = meta.with_diagnostic(diagnostic); }
        context.finish()?;
        Ok(Table::new(rows, meta).with_end(end))
    }
}

fn merge_depth(mut snapshot: Snapshot, depth: Option<&Snapshot>, failure: Option<&Error>) -> Result<FullQuote> {
    let (status, source, diagnostic) = match (failure, depth) {
        (Some(error), _) => (EnrichmentStatus::Failed, None, failed_diagnostic("hq.depth", "full_quotes.depth", error)),
        (None, Some(depth)) if depth.bids().len() >= 5 && depth.asks().len() >= 5 => {
            snapshot.replace_depth(depth)?;
            (EnrichmentStatus::Succeeded, Some(depth.meta().source().clone()),
                Diagnostic::new("hq.depth", "full_quotes.depth", EnrichmentStatus::Succeeded, None, None))
        }
        (None, depth) => (EnrichmentStatus::Unavailable, depth.map(|depth| depth.meta().source().clone()),
            Diagnostic::new("hq.depth", "full_quotes.depth", EnrichmentStatus::Unavailable, None,
                Some("both depth sides must contain at least five levels".to_owned()))),
    };
    Ok(FullQuote { snapshot, depth_status: status, depth_source: source, diagnostics: vec![diagnostic] })
}

pub(crate) fn optional_failure(context: &OperationContext, error: &Error) -> Result<()> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    match error.kind() {
        ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed | ErrorKind::Closed | ErrorKind::CloseTimeout
        | ErrorKind::ForkedProcess | ErrorKind::Internal => Err(error.clone()),
        _ => Ok(()),
    }
}
pub(super) fn failed_diagnostic(source: &str, stage: &str, error: &Error) -> Diagnostic {
    Diagnostic::new(source, stage, EnrichmentStatus::Failed, Some(error.kind()), Some(error.message().to_owned()))
}
fn helper_overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "HQ helper count or offset exceeds its finite range") }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{Amount, AmountUnit, EventTime, NumericValue, Price, Quantity, QuantityUnit,
        QuoteLevel, RawData, ResultMeta, SourceStamp};

    fn snapshot(number: &str, value: i64, bids: usize, asks: usize, command: u16) -> Snapshot {
        let id = SecurityId::new(HqMarket::Shanghai, number).unwrap();
        let price = Price::scaled_integer(value, 2);
        let quantity = Quantity::new(NumericValue::ScaledInteger { value: 17, scale: 0 }, QuantityUnit::Lots);
        let amount = Amount::new(NumericValue::ScaledInteger { value, scale: 0 }, AmountUnit::Cny);
        let level = QuoteLevel::new(price, quantity);
        let payload = bytes::Bytes::from_static(b"primary-raw");
        let meta = ResultMeta::new(SourceStamp::new(Some(command), None, "source", Some(7), None),
            Vec::new(), Some(RawData::new(vec![payload], None, vec![(0, 0, 11)])));
        Snapshot::new(id, 3, price, price, price, price, price, EventTime::unknown_raw(1234),
            1234, quantity, quantity, quantity, quantity, amount, amount,
            vec![level.clone(); bids], vec![level; asks], meta)
            .with_amount_wire_word(0x1234).with_quote_wire_fields(Some(19), Some(20), Some(21))
    }

    #[test]
    fn full_depth_replaces_only_levels_and_opening_amount() {
        let primary = snapshot("600000", 1000, 1, 1, 0x054c);
        let depth = snapshot("600000", 2000, 5, 5, 0x0547);
        let full = merge_depth(primary.clone(), Some(&depth), None).unwrap();
        assert_eq!(full.depth_status(), EnrichmentStatus::Succeeded);
        let merged = full.snapshot();
        assert_eq!(merged.last(), primary.last()); assert_eq!(merged.pre_close(), primary.pre_close());
        assert_eq!(merged.open(), primary.open()); assert_eq!(merged.high(), primary.high());
        assert_eq!(merged.low(), primary.low()); assert_eq!(merged.amount(), primary.amount());
        assert_eq!(merged.total(), primary.total()); assert_eq!(merged.event_time(), primary.event_time());
        assert_eq!(merged.server_time_raw(), primary.server_time_raw()); assert_eq!(merged.active(), primary.active());
        assert_eq!(merged.open_amount(), depth.open_amount());
        assert_eq!(merged.bids(), depth.bids()); assert_eq!(merged.asks(), depth.asks());
        assert_eq!(merged.meta(), primary.meta()); assert_eq!(merged.amount_wire_word(), primary.amount_wire_word());
        assert_eq!(merged.open_amount_raw(), primary.open_amount_raw());
        assert_eq!(merged.unknown_after_time_raw(), Some(19));
        assert_eq!(full.depth_source().unwrap().command(), Some(0x0547));
    }

    #[test]
    fn insufficient_or_absent_depth_keeps_every_primary_field() {
        for (bids, asks) in [(4, 5), (5, 4), (0, 0)] {
            let primary = snapshot("600000", 1000, 1, 1, 0x054c);
            let depth = snapshot("600000", 2000, bids, asks, 0x0547);
            let full = merge_depth(primary.clone(), Some(&depth), None).unwrap();
            assert_eq!(full.snapshot(), &primary); assert_eq!(full.depth_status(), EnrichmentStatus::Unavailable);
        }
        let primary = snapshot("600000", 1000, 1, 1, 0x054c);
        let full = merge_depth(primary.clone(), None, None).unwrap();
        assert_eq!(full.snapshot(), &primary); assert!(full.depth_source().is_none());
        assert_eq!(full.depth_status(), EnrichmentStatus::Unavailable);
    }

    #[test]
    fn depth_failure_has_a_structured_diagnostic_and_keeps_the_primary() {
        let primary = snapshot("600000", 1000, 1, 1, 0x054c);
        let error = Error::new(ErrorKind::Protocol, "damaged depth payload");
        let full = merge_depth(primary.clone(), None, Some(&error)).unwrap();
        assert_eq!(full.snapshot(), &primary); assert_eq!(full.depth_status(), EnrichmentStatus::Failed);
        assert_eq!(full.diagnostics()[0].error_kind(), Some(ErrorKind::Protocol));
        assert_eq!(full.diagnostics()[0].message(), Some("damaged depth payload"));
        assert_eq!(merge_depth(primary, Some(&snapshot("600001", 2000, 5, 5, 0x0547)), None)
            .unwrap_err().kind(), ErrorKind::Internal);
    }
}
