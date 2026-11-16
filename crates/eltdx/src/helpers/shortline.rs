use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, allocation_error, KlineService, SessionService};
use crate::services::pagination::{advance_offset, CollectionLimits, PageProgress};
use crate::types::{Amount, AmountUnit, BatchItemState, CalendarQuery, CategoryQuery,
    CodeScanQuery, Diagnostic, EnrichmentStatus, FinanceField, FinanceRecord, FullQuote,
    HandshakeInfo, KlineQuery, KlineSeries, LadderQuery, LimitDecision, LimitStatus,
    NumericValue, Price, Quantity, QuantityUnit, QuoteSort, Ratio, RatioUnit, ResultMeta,
    SecurityId, SecuritySelection, ShortlineIndicator, ShortlineQuery, ShortlineTable,
    Snapshot, Stat2Row, StatRow, StatsAlignment, StatsQuery, Table, TradingDate, ValueProvenance};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::cache::Supplement;
use super::profile::shares;
use super::service::{failed_diagnostic, optional_failure, HelperService};

const RUSH_PAGE: usize = 80;
const RUSH_PAGES: usize = 200;

impl HelperService {
    pub fn shortline_indicators(&self, query: &ShortlineQuery, options: &OperationOptions) -> Result<ShortlineTable> {
        let mut context = self.executor.begin_context_at(Instant::now(), "helpers_shortline", &self.config, options)?;
        query.validate()?;
        shortline_limits(&mut context, query.ids().len(), false)?;
        let (target, handshake) = self.shortline_date_in_context(&context)?;
        let result = self.shortline_in_context(&context, query.ids(), query.stats(), query.refresh_stats(), target,
            vec![handshake.meta().clone()])?;
        context.finish()?; Ok(result)
    }

    pub fn limit_ladder(&self, query: &LadderQuery, options: &OperationOptions) -> Result<Table<ShortlineIndicator>> {
        let mut context = self.executor.begin_context_at(Instant::now(), "helpers_limit_ladder", &self.config, options)?;
        query.validate()?;
        let count = match query.selection() { SecuritySelection::Explicit(ids) => ids.len(), SecuritySelection::AllAShares => 65_535 };
        shortline_limits(&mut context, count, matches!(query.selection(), SecuritySelection::AllAShares))?;
        let (_, result) = self.ladder_in_context(query, &context)?;
        context.finish()?; Ok(result)
    }

    /// The caller freezes the complete work budget before entering this seam.
    /// No child public operation may reset the parent's absolute deadline.
    pub(super) fn ladder_in_context(&self, query: &LadderQuery, context: &OperationContext) -> Result<(Option<TradingDate>, Table<ShortlineIndicator>)> {
        if query.selection().is_empty() { return Ok((None, Table::new(Vec::new(), ResultMeta::default()))); }
        let (target, handshake) = self.shortline_date_in_context(context)?;
        let selected = self.select_in_context(context, query.selection())?;
        if selected.ids.is_empty() { return Ok((Some(target), Table::new(Vec::new(), selected.meta))); }
        let result = self.shortline_in_context(context, &selected.ids, query.stats(), query.refresh_stats(), target,
            vec![handshake.meta().clone(), selected.meta])?;
        let mut meta = result.meta().clone();
        let unknown = result.rows().iter().filter(|row| matches!(row.limit_status(), LimitStatus::UnknownRule | LimitStatus::MissingReference)).count();
        if unknown > 0 {
            meta = meta.with_diagnostic(Diagnostic::new("price_limits_v1", "ladder.unclassified", EnrichmentStatus::Unavailable,
                None, Some(format!("{unknown} input occurrences cannot be classified; an empty ladder does not establish that no stocks reached their limit"))));
        }
        let rows = select_ladder(result.into_rows(), query.include_touched(), query.count(),
            || context.check_boundary(TimeoutPhase::Aggregate))?;
        Ok((Some(target), Table::new(rows, meta)))
    }

    fn shortline_date_in_context(&self, context: &OperationContext) -> Result<(TradingDate, HandshakeInfo)> {
        let info = SessionService::new(self.executor.clone(), self.engine.clone(), self.config.clone()).handshake_in_context(context)?;
        let target = ready_target(&info)?; Ok((target, info))
    }

    fn shortline_in_context(&self, context: &OperationContext, ids: &[SecurityId], query: &StatsQuery,
        refresh: bool, target: TradingDate, mut metas: Vec<ResultMeta>) -> Result<ShortlineTable> {
        // Calendar bars and finance also start decoding. All markets needed by
        // the later optional category sweep must therefore be captured now.
        self.quotes().prepare_composition_in_context(context, ids, ids.len() >= RUSH_PAGE)?;
        let calendar = self.calendar_in_context(context)?;
        let previous = self.previous_in_calendar(&calendar, target)?;
        let (stats, stats_date, refreshed) = self.aligned_stats_in_context(context, query, refresh, target, previous)?;
        let primary = self.full_quotes_in_context(context, ids)?;
        let (finance, finance_hit) = self.profile_finance(context, ids)?;
        let bundle = finance.result.as_ref().map_err(Clone::clone)?;
        if bundle.rows.len() != ids.len() || primary.len() != ids.len() { return Err(internal("shortline sources lost input occurrences")); }
        let mut bars = HashMap::new(); bars.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut bar_order = Vec::new(); bar_order.try_reserve(ids.len()).map_err(allocation_error)?;
        let kline = KlineService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let recent_query = KlineQuery::new().with_count(6)?;
        for id in ids {
            context.check_boundary(TimeoutPhase::Admission)?;
            if bars.contains_key(id) { continue; }
            let source = match kline.page_with_parent_context(context, id, &recent_query, None) {
                Ok(series) => { let status = if series.bars().items().is_empty() { EnrichmentStatus::Unavailable } else { EnrichmentStatus::Succeeded };
                    Supplement { value: Some(series), status, diagnostic: state("hq.klines", "shortline.recent_bars", status, None) } },
                Err(error) => { optional_failure(context, &error)?;
                    Supplement { value: None, status: EnrichmentStatus::Failed,
                        diagnostic: failed_diagnostic("hq.klines", "shortline.recent_bars", &error) } },
            };
            bar_order.push(id.clone()); bars.insert(id.clone(), source);
        }
        let rush = self.opening_rush_in_context(context, ids)?;
        metas.try_reserve(5 + bars.len() + rush.metas.len()).map_err(allocation_error)?;
        metas.extend([calendar.meta().clone(), stats.meta().clone(), primary.meta().clone(), bundle.meta.clone()]);
        for id in &bar_order { if let Some(series) = bars.get(id).and_then(|source| source.value.as_ref()) { metas.push(series.meta().clone()); } }
        metas.extend_from_slice(&rush.metas);
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let finance_diagnostic = finance.diagnostic("hq.finance", "shortline.finance_cache", finance_hit,
            if bundle.any_failed { EnrichmentStatus::Failed } else { EnrichmentStatus::Succeeded });
        let stats_diagnostic = state("stats", "shortline.stats", EnrichmentStatus::Succeeded,
            Some(format!("target={target}; previous={previous}; stats_date={stats_date}; refreshed={refreshed}")));
        let mut stat_index = HashMap::new(); let mut stat2_index = HashMap::new();
        stat_index.try_reserve(stats.stat().len()).map_err(allocation_error)?;
        stat2_index.try_reserve(stats.stat2().len()).map_err(allocation_error)?;
        for row in stats.stat() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            if let Some(id) = row.id() { if stat_index.insert(id, row).is_some() { return Err(protocol("duplicate stat identity")); } }
        }
        for row in stats.stat2() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            if let Some(id) = row.id() { if stat2_index.insert(id, row).is_some() { return Err(protocol("duplicate stat2 identity")); } }
        }
        let mut rows = Vec::new(); rows.try_reserve(ids.len()).map_err(allocation_error)?;
        for (expected, item) in primary.into_items().into_iter().enumerate() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let (index, id, row) = item.into_parts();
            if index != expected || ids.get(index) != Some(&id) { return Err(internal("shortline quote occurrence association changed")); }
            let BatchItemState::Present(quote) = row else { return Err(Error::new(ErrorKind::MissingData, "shortline required quote is missing")); };
            let finance = &bundle.rows[index];
            if finance.value.as_ref().is_some_and(|record| record.id() != &id) { return Err(internal("shortline finance identity changed")); }
            let bars = bars.get(&id).ok_or_else(|| internal("shortline optional bars source disappeared"))?;
            let mut row_metas = vec![quote.snapshot().meta().clone(), stats.meta().clone(), calendar.meta().clone()];
            if let Some(record) = &finance.value { row_metas.push(record.meta().clone()); }
            if let Some(series) = &bars.value { row_metas.push(series.meta().clone()); }
            let opening = rush.values.get(&id);
            if let Some((_, meta)) = opening { row_metas.push(meta.clone()); }
            let mut meta = aggregate_meta(context, &row_metas, version)?
                .with_diagnostic(stats_diagnostic.clone()).with_diagnostic(finance_diagnostic.clone())
                .with_diagnostic(finance.diagnostic.clone()).with_diagnostic(bars.diagnostic.clone());
            for diagnostic in quote.diagnostics() { meta = meta.with_diagnostic(diagnostic.clone()); }
            if let Some(source) = quote.depth_source() {
                let mut sources = meta.sources().to_vec(); sources.push(source.clone()); meta = meta.with_sources(sources);
            }
            let rush_status = if opening.is_some() { EnrichmentStatus::Succeeded } else { rush.unresolved };
            meta = meta.with_diagnostic(Diagnostic::new("hq.category", "shortline.opening_rush", rush_status,
                if opening.is_some() { None } else { rush.error_kind }, rush.message.clone()));
            rows.push(build_indicator(quote, stat_index.get(&id).copied(), stat2_index.get(&id).copied(),
                finance.value.as_ref(), bars.value.as_ref(), opening.map(|(value, _)| *value), target, previous, meta,
                || context.check_boundary(TimeoutPhase::Aggregate))?);
        }
        let mut meta = aggregate_meta(context, &metas, version)?
            .with_diagnostic(stats_diagnostic).with_diagnostic(finance_diagnostic);
        for id in &bar_order { if let Some(source) = bars.get(id) { meta = meta.with_diagnostic(source.diagnostic.clone()); } }
        meta = meta.with_diagnostic(Diagnostic::new("hq.category", "shortline.opening_rush", rush.unresolved, rush.error_kind, rush.message));
        let mut input = Vec::new(); input.try_reserve(ids.len()).map_err(allocation_error)?; input.extend_from_slice(ids);
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(ShortlineTable { ids: input, target, previous, stats_date, stats_path: stats.source_path().to_owned(),
            stats_refreshed: refreshed, rows, meta })
    }

    fn opening_rush_in_context(&self, context: &OperationContext, ids: &[SecurityId]) -> Result<RushSource> {
        let mut source = RushSource { values: HashMap::new(), metas: Vec::new(), unresolved: EnrichmentStatus::NotRequested, error_kind: None,
            message: Some("category sweep requires at least 80 input occurrences".to_owned()) };
        if ids.len() < RUSH_PAGE { return Ok(source); }
        let mut wanted = HashSet::new(); wanted.try_reserve(ids.len()).map_err(allocation_error)?;
        wanted.extend(ids.iter().cloned()); source.values.try_reserve(wanted.len()).map_err(allocation_error)?;
        let limits = CollectionLimits { pages: Some(RUSH_PAGES), records: Some(RUSH_PAGE * RUSH_PAGES) };
        let mut progress = PageProgress::default(); let mut seen = HashSet::new(); let mut offset = 0; let mut endpoint = None;
        let result: Result<()> = (|| {
            loop {
                context.check_boundary(TimeoutPhase::Admission)?;
                if wanted.is_empty() { return Ok(()); }
                progress.before_request(limits)?;
                let query = CategoryQuery::new().with_sort(QuoteSort::Code)?.with_ascending(true)?
                    .with_start(u16::try_from(offset).map_err(|_| overflow())?)?.with_count(RUSH_PAGE)?;
                let purpose = if progress.pages() == 0 { ChildPurpose::Fallback } else { ChildPurpose::Page };
                let page = self.quotes().category_in_context(context, &query, endpoint.as_deref(), purpose)?;
                progress.accept(page.items().len(), RUSH_PAGE, limits)?;
                let host = page.meta().source().endpoint().ok_or_else(|| protocol("opening-rush page has no source endpoint"))?;
                let host = format!("{host}");
                if endpoint.as_ref().is_some_and(|expected| expected != &host) { return Err(Error::new(ErrorKind::Connection, "opening-rush continuation changed source")); }
                endpoint = Some(host);
                source.metas.try_reserve(1).map_err(allocation_error)?; source.metas.push(page.meta().clone());
                if page.items().is_empty() { return Ok(()); }
                let key = page.items().iter().map(|row| row.snapshot().id().clone()).collect::<Vec<_>>();
                seen.try_reserve(1).map_err(allocation_error)?;
                if !seen.insert(key) { return Err(protocol("opening-rush pagination repeated an identity page")); }
                offset = advance_offset(offset, page.items().len())?;
                for row in page.into_items() {
                    context.check_boundary(TimeoutPhase::Aggregate)?;
                    let value = row.opening_rush();
                    if value.value().is_finite() && wanted.remove(row.snapshot().id()) {
                        source.values.insert(row.snapshot().id().clone(), (value, row.snapshot().meta().clone()));
                    }
                }
                // A short nonempty page alone is not proof of EOF.
            }
        })();
        match result {
            Ok(()) => { source.unresolved = if wanted.is_empty() { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable };
                source.message = Some(format!("pages={}; unresolved={}; empty-page EOF or all requested identities found", progress.pages(), wanted.len())); }
            Err(error) => { optional_failure(context, &error)?; source.unresolved = EnrichmentStatus::Failed; source.error_kind = Some(error.kind());
                source.message = Some(format!("{:?}: {}; valid matched supplements retained; unresolved identities are not confirmed missing", error.kind(), error.message())); }
        }
        Ok(source)
    }
}

struct RushSource { values: HashMap<SecurityId, (Ratio, ResultMeta)>, metas: Vec<ResultMeta>,
    unresolved: EnrichmentStatus, error_kind: Option<ErrorKind>, message: Option<String> }

fn shortline_limits(context: &mut OperationContext, count: usize, scan: bool) -> Result<()> {
    context.set_work_limits(shortline_work_limits(count, scan)?)
}

pub(super) fn shortline_work_limits(count: usize, scan: bool) -> Result<WorkLimits> {
    let scan_records = if scan { CodeScanQuery::default().pages().max_records().checked_mul(3).ok_or_else(overflow)? } else { 0 };
    let records = count.checked_mul(9).and_then(|value| value.checked_add(scan_records))
        .and_then(|value| value.checked_add(CalendarQuery::default().pages().max_records()))
        .and_then(|value| value.checked_add(if count >= RUSH_PAGE { RUSH_PAGE * RUSH_PAGES } else { 0 })).ok_or_else(overflow)?;
    Ok(WorkLimits { pages: None, records: Some(records), bytes: None })
}

fn ready_target(info: &HandshakeInfo) -> Result<TradingDate> {
    let datetime = info.server_datetime().filter(|value| value.date().is_some() && value.clock().is_some())
        .ok_or_else(|| date_error("shortline requires a usable server datetime"))?;
    let target = info.server_date1().into_iter().chain(info.server_date2()).max()
        .ok_or_else(|| date_error("shortline requires a valid handshake target date"))?;
    if !super::calculation::auction_ready(datetime.date() == Some(target), datetime.clock().is_some_and(|clock| clock < (9, 25, 0))) {
        return Err(Error::new(ErrorKind::DataNotReady, "shortline auction data is not ready before server 09:25")
            .with_context("target", format!("{target}")));
    }
    Ok(target)
}

struct AlignedStat2 {
    alignment: StatsAlignment, amount: Option<Amount>, seal: Option<Amount>, seal2: Option<Amount>,
    open_amount: Option<Amount>, open_volume: Option<Quantity>,
}
fn align_stat2(row: Option<&Stat2Row>, target: TradingDate, previous: TradingDate) -> AlignedStat2 {
    let status = super::stats_alignment::stat_date_alignment(row.and_then(Stat2Row::stats_date).as_ref(), &target, &previous);
    let empty = AlignedStat2 { alignment: status, amount: None, seal: None, seal2: None, open_amount: None, open_volume: None };
    let Some(row) = row else { return empty; };
    if !matches!(status, StatsAlignment::SameDay | StatsAlignment::PreviousTradingDay) { return empty; }
    use super::stats_alignment::Stat2Column;
    let columns = super::stats_alignment::stat2_columns(status == StatsAlignment::PreviousTradingDay);
    let amount = |column| match column {
        Stat2Column::Amount => row.amount(), Stat2Column::Seal => row.seal(),
        Stat2Column::PreviousAmount => row.previous_amount(), Stat2Column::PreviousSeal => row.previous_seal(),
        Stat2Column::Previous2Seal => row.previous2_seal(), Stat2Column::OpenAmount => row.open_amount(),
        Stat2Column::PreviousOpenAmount => row.previous_open_amount(), _ => None,
    };
    AlignedStat2 { alignment: status, amount: amount(columns[0]), seal: amount(columns[1]),
        seal2: amount(columns[2]), open_amount: amount(columns[4]),
        open_volume: match columns[3] { Stat2Column::OpenVolume => row.open_volume(),
            Stat2Column::PreviousOpenVolume => row.previous_open_volume(), _ => None } }
}

fn build_indicator(quote: FullQuote, stat: Option<&StatRow>, stat2: Option<&Stat2Row>, finance: Option<&FinanceRecord>,
    bars: Option<&KlineSeries>, opening_rush: Option<Ratio>, target: TradingDate, previous: TradingDate,
    mut meta: ResultMeta, mut boundary: impl FnMut() -> Result<()>) -> Result<ShortlineIndicator> {
    boundary()?;
    let snapshot = quote.snapshot(); let id = snapshot.id().clone();
    let aligned = align_stat2(stat2, target, previous);
    let stats_date = stat2.and_then(Stat2Row::stats_date).or_else(|| stat.and_then(StatRow::stats_date));
    let valid_stat = stat.filter(|row| row.stats_date() == Some(target) || row.stats_date() == Some(previous));
    let alignment = if stat.is_none() { StatsAlignment::RowMissing }
        else if valid_stat.is_none() { StatsAlignment::DateUnaligned } else { aligned.alignment };
    meta = meta.with_diagnostic(state("tdxstat.cfg", "shortline.stat_row", if valid_stat.is_some() {
        EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable },
        Some(format!("alignment={alignment:?}; source_date={:?}; source_line={:?}", stat.and_then(StatRow::stats_date), stat.map(StatRow::source_line)))))
        .with_diagnostic(state("tdxstat2.cfg", "shortline.stat2_row", if matches!(aligned.alignment, StatsAlignment::SameDay | StatsAlignment::PreviousTradingDay) {
            EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable },
            Some(format!("alignment={:?}; source_date={:?}; source_line={:?}", aligned.alignment, stat2.and_then(Stat2Row::stats_date), stat2.map(Stat2Row::source_line)))));
    let free_float = valid_stat.and_then(StatRow::free_float_shares).filter(|value| quantity_value(*value, QuantityUnit::Shares).is_some());
    let float = shares(finance, FinanceField::CirculatingShares, "shortline.float_shares", &mut meta);
    let last = finite_nonnegative(snapshot.last().as_f64());
    let open = finite_nonnegative(snapshot.open().as_f64());
    let pre_close = finite_nonnegative(snapshot.pre_close().as_f64());
    let open_amount = valid_amount(Some(snapshot.open_amount()));
    let open_volume = divide(amount_value(open_amount), open.map(|value| value * 100.0)).map(lots);
    let free_value = product(free_float.and_then(|value| quantity_value(value, QuantityUnit::Shares)), last).map(cny);
    let float_value = product(float.and_then(|value| quantity_value(value, QuantityUnit::Shares)), last).map(cny);
    let seal = seal_amount(snapshot);
    if valid_amount(Some(snapshot.amount())).is_none() {
        meta = meta.with_diagnostic(state("hq.quotes", "shortline.seal_to_amount", EnrichmentStatus::Unavailable,
            Some("source amount lacks finite nonnegative CNY units; no currency assumption is applied".to_owned())));
    }
    if seal.is_none() {
        meta = meta.with_diagnostic(state("hq.quotes", "shortline.seal_amount", EnrichmentStatus::Unavailable,
            Some("requires a finite bid1 price and a nonnegative hand volume".to_owned())));
    }
    let decision = crate::policy::limits::evaluate(&id, target, Some(snapshot.pre_close()))?;
    let limit_status = classify_limit(snapshot, &decision);
    meta = meta.with_diagnostic(state("price_limits_v1", "shortline.limit_status",
        if limit_status == LimitStatus::UnknownRule { EnrichmentStatus::Unavailable } else { EnrichmentStatus::Succeeded },
        Some(format!("status={limit_status:?}; evidence={:?}", decision.evidence()))));
    let (board_text, ladder_level) = board_progress(valid_stat, target, previous, limit_status)?;
    let average = prior_minute_volume(bars, target, &mut boundary)?;
    let prior_amount = valid_amount(aligned.amount); let prior_seal = valid_amount(aligned.seal);
    let prior2_seal = valid_amount(aligned.seal2); let prior_open = valid_amount(aligned.open_amount);
    let prior_volume = aligned.open_volume.filter(|value| quantity_value(*value, QuantityUnit::Lots).is_some());
    if average.is_none() { meta = meta.with_diagnostic(state("hq.klines", "shortline.open_volume_ratio", EnrichmentStatus::Unavailable,
        Some("requires five distinct prior daily bars with finite hand volumes and a nonzero total".to_owned()))); }
    let open_hands = open_volume.and_then(|value| quantity_value(value, QuantityUnit::Lots));
    let open_yuan = amount_value(open_amount); let seal_yuan = amount_value(seal);
    let free_shares = free_float.and_then(|value| quantity_value(value, QuantityUnit::Shares));
    let change = match (open, pre_close) { (Some(open), Some(previous)) => divide(Some(open - previous), Some(previous)), _ => None };
    boundary()?;
    Ok(ShortlineIndicator { id, target, previous, stats_date, alignment, limit_status,
        beta_60d: valid_stat.and_then(StatRow::beta_60d).filter(|value| value.is_finite()),
        pe_ttm: valid_stat.and_then(StatRow::pe_ttm).filter(|value| value.is_finite()),
        free_float_shares: free_float, float_shares: float, previous_amount: prior_amount,
        previous_seal: prior_seal, previous2_seal: prior2_seal, previous_open_amount: prior_open, previous_open_volume: prior_volume,
        limit_stat_days: valid_stat.and_then(StatRow::limit_stat_days), limit_up_count: valid_stat.and_then(StatRow::limit_up_count),
        limit_streak: valid_stat.and_then(StatRow::limit_streak), year_limit_up_days: valid_stat.and_then(StatRow::year_limit_up_days),
        free_float_market_value: free_value, float_market_value: float_value,
        open_turnover: percent(divide(open_hands, free_shares.map(|value| value / 100.0))),
        open_previous_amount: percent(divide(open_yuan, amount_value(prior_amount))),
        open_previous_seal: percent(divide(open_yuan, amount_value(prior_seal))),
        seal_to_float: percent(divide(seal_yuan, amount_value(free_value))), open_change: percent(change),
        auction_previous_volume: fraction(divide(open_hands, prior_volume.and_then(|value| quantity_value(value, QuantityUnit::Lots)))),
        seal_previous: fraction(divide(seal_yuan, amount_value(prior_seal))), open_volume_ratio: fraction(divide(open_hands, average)),
        seal_to_amount: fraction(divide(seal_yuan, amount_value(valid_amount(Some(snapshot.amount()))))),
        board_text, ladder_level, open: open.map(|_| snapshot.open()), pre_close: pre_close.map(|_| snapshot.pre_close()),
        open_amount, seal_amount: seal, open_volume, opening_rush, meta })
}

fn prior_minute_volume(series: Option<&KlineSeries>, target: TradingDate,
    boundary: &mut impl FnMut() -> Result<()>) -> Result<Option<f64>> {
    let Some(series) = series else { return Ok(None); };
    if series.period() != crate::types::KlinePeriod::Day || series.adjust() != crate::types::AdjustMode::None { return Ok(None); }
    let mut prior = BTreeMap::new();
    for bar in series.bars().items() {
        boundary()?;
        let Some(date) = bar.source_time().date() else { return Ok(None); };
        if date >= target { continue; }
        let volume = quantity_value(bar.volume(), QuantityUnit::Lots);
        if let Some(existing) = prior.insert(date, volume) { if existing != volume { return Ok(None); } }
    }
    if prior.len() < 5 { return Ok(None); }
    let mut sum = 0.0;
    for volume in prior.into_values().rev().take(5) { boundary()?; let Some(volume) = volume else { return Ok(None); }; sum += volume; }
    let value = sum / (240.0 * 5.0);
    Ok((value.is_finite() && value > 0.0).then_some(value))
}

fn classify_limit(snapshot: &Snapshot, decision: &LimitDecision) -> LimitStatus {
    if decision.status() != LimitStatus::Normal { return decision.status(); }
    let Some(upper) = decision.upper() else { return LimitStatus::UnknownRule; };
    let upper = upper.as_f64();
    if !upper.is_finite() || upper <= 0.0 { return LimitStatus::UnknownRule; }
    // Quote comparison tolerance is independent of the approved price rule;
    // it cannot provide or infer a missing executable upper limit.
    let close = |value: f64| value.is_finite() && (value - upper).abs() <= 0.0051;
    let bid_at_limit = snapshot.bids().first().is_some_and(|level| close(level.price().as_f64()));
    super::calculation::classify_limit_state::<std::convert::Infallible>(close(snapshot.last().as_f64()),
        || Ok(bid_at_limit || amount_value(seal_amount(snapshot)).is_some_and(|value| value > 0.0)),
        || Ok([snapshot.high(), snapshot.last()].iter().any(|price| price.as_f64().is_finite() && price.as_f64() + 0.0051 >= upper)))
        .unwrap_or_else(|never| match never {})
}

fn board_progress(stat: Option<&StatRow>, target: TradingDate, previous: TradingDate, status: LimitStatus)
    -> Result<(Option<String>, Option<u32>)> {
    let Some(stat) = stat else { return Ok((None, None)); };
    let sealed = status == LimitStatus::Sealed;
    let advance = sealed && stat.stats_date() == Some(previous);
    let increment = |value: Option<u32>| -> Result<Option<u32>> {
        if advance { Ok(Some(value.unwrap_or(0).checked_add(1).ok_or_else(overflow)?)) } else { Ok(value) }
    };
    let days = increment(stat.limit_stat_days())?; let count = increment(stat.limit_up_count())?;
    let text = match (days, count) { (Some(days), Some(count)) if days > 0 && count > 0 => Some(format!("{days}天{count}板")), _ => None };
    let level = if !sealed { None } else { super::calculation::ladder_level_with(stat.limit_streak().unwrap_or(0),
        stat.stats_date() == Some(target), advance, |prior| prior.checked_add(1).ok_or_else(overflow), |prior| Ok(prior.max(1)))? };
    Ok((text, level))
}

fn select_ladder(mut rows: Vec<ShortlineIndicator>, include_touched: bool, count: Option<usize>,
    mut boundary: impl FnMut() -> Result<()>) -> Result<Vec<ShortlineIndicator>> {
    let mut selected = Vec::new(); selected.try_reserve(rows.len()).map_err(allocation_error)?;
    for row in rows.drain(..) { boundary()?;
        if super::calculation::include_ladder_row(row.limit_status == LimitStatus::Sealed,
            row.limit_status == LimitStatus::Touched, include_touched) { selected.push(row); }
    }
    boundary()?;
    selected.sort_by(|left, right| right.ladder_level.unwrap_or(0).cmp(&left.ladder_level.unwrap_or(0))
        .then_with(|| amount_value(right.seal_amount).unwrap_or(0.0).total_cmp(&amount_value(left.seal_amount).unwrap_or(0.0)))
        .then_with(|| left.id.market().code().cmp(right.id.market().code())).then_with(|| left.id.number().cmp(right.id.number())));
    boundary()?;
    if let Some(count) = count { selected.truncate(count); }
    Ok(selected)
}

fn seal_amount(snapshot: &Snapshot) -> Option<Amount> {
    let bid = snapshot.bids().first()?;
    product(finite_nonnegative(bid.price().as_f64()), quantity_value(bid.volume(), QuantityUnit::Lots))
        .and_then(|value| finite_nonnegative(value * 100.0)).map(cny)
}
fn finite_nonnegative(value: f64) -> Option<f64> { (value.is_finite() && value >= 0.0).then_some(value) }
fn quantity_value(value: Quantity, unit: QuantityUnit) -> Option<f64> {
    if value.unit() == unit { finite_nonnegative(value.value().as_f64()) } else { None }
}
fn valid_amount(value: Option<Amount>) -> Option<Amount> {
    value.filter(|value| value.unit() == AmountUnit::Cny && finite_nonnegative(value.value().as_f64()).is_some())
}
fn amount_value(value: Option<Amount>) -> Option<f64> { valid_amount(value).map(|value| value.value().as_f64()) }
fn product(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    let value = super::calculation::product_with::<_, std::convert::Infallible>(left?, right?, |a, b| Ok(a * b))
        .unwrap_or_else(|never| match never {});
    finite_nonnegative(value)
}
fn divide(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    let denominator = denominator?; let numerator = numerator?;
    if denominator == 0.0 || !denominator.is_finite() || !numerator.is_finite() { return None; }
    let value = super::calculation::ratio_with::<_, std::convert::Infallible>(numerator, denominator, |a, b| Ok(a / b))
        .unwrap_or_else(|never| match never {});
    value.is_finite().then_some(value)
}
fn cny(value: f64) -> Amount { Amount::with_provenance(NumericValue::F64(value), AmountUnit::Cny, ValueProvenance::WireRule) }
fn lots(value: f64) -> Quantity { Quantity::with_provenance(NumericValue::F64(value), QuantityUnit::Lots, ValueProvenance::WireRule) }
fn fraction(value: Option<f64>) -> Option<Ratio> {
    value.filter(|value| value.is_finite()).map(|value| Ratio::with_provenance(NumericValue::F64(value), RatioUnit::Fraction, ValueProvenance::WireRule))
}
fn percent(value: Option<f64>) -> Option<Ratio> {
    value.and_then(|value| { let value = value * 100.0; value.is_finite().then(|| Ratio::with_provenance(
        NumericValue::F64(value), RatioUnit::PercentagePoints, ValueProvenance::WireRule)) })
}
fn state(source: &str, stage: &str, status: EnrichmentStatus, message: Option<String>) -> Diagnostic { Diagnostic::new(source, stage, status, None, message) }
fn protocol(message: &str) -> Error { Error::new(ErrorKind::Protocol, message) }
fn internal(message: &str) -> Error { Error::new(ErrorKind::Internal, message) }
fn overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "shortline count or work allowance overflows") }
fn date_error(message: &str) -> Error { Error::new(ErrorKind::MetadataUnavailable, message).with_context("code", "stats_date_unavailable") }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{AdjustMode, BarTime, DataPage, EndEvidence, EventTime, FinanceValue,
        KlineBar, KlineKind, KlinePeriod, LimitRule, QuoteLevel};

    fn date(day: u8) -> TradingDate { TradingDate::new(2026, 11, day).unwrap() }
    fn stat(day: u8) -> StatRow {
        StatRow { id: Some("sh600000".parse().unwrap()), source_market: "1".to_owned(), source_code: "600000".to_owned(),
            stats_date: Some(date(day)), stats_date_raw: Some(format!("202611{day:02}")), beta_60d: Some(1.5), pe_ttm: Some(-2.0),
            free_float_shares: Some(Quantity::shares_from_ten_thousands_f64(1.0, ValueProvenance::WireRule).unwrap()),
            free_float_shares_source_10k: Some(1.0), year_limit_up_days: Some(7), limit_stat_days: Some(3),
            limit_up_count: Some(2), limit_streak: Some(2), source_line: 1, source_counts: [None, None, None, None] }
    }
    fn stat2(day: u8) -> Stat2Row {
        Stat2Row { id: Some("sh600000".parse().unwrap()), source_market: "1".to_owned(), source_code: "600000".to_owned(),
            stats_date: Some(date(day)), stats_date_raw: Some(format!("202611{day:02}")), amount: Some(cny(10000.0)), seal: Some(cny(2000.0)),
            previous_amount: Some(cny(9000.0)), previous_seal: Some(cny(1000.0)), previous2_amount: Some(cny(8000.0)), previous2_seal: Some(cny(500.0)),
            open_amount: Some(cny(400.0)), previous_open_amount: Some(cny(300.0)), open_volume: Some(lots(4.0)), previous_open_volume: Some(lots(3.0)),
            source_line: 1, source_values: [None; 10] }
    }
    fn quote(last: f64, high: f64, bid: f64, hand: f64, unit: QuantityUnit) -> FullQuote {
        let price = Price::from_f64(last); let volume = Quantity::new(NumericValue::F64(hand), unit);
        FullQuote { snapshot: Snapshot::new("sh600000".parse().unwrap(), 0, price, Price::from_f64(10.0), Price::from_f64(10.0),
            Price::from_f64(high), price, EventTime::trading_date(date(16)), 0, volume, volume, volume, volume,
            cny(5000.0), cny(1000.0), vec![QuoteLevel::new(Price::from_f64(bid), volume)], Vec::new(), ResultMeta::default()),
            depth_status: EnrichmentStatus::Unavailable, depth_source: None, diagnostics: Vec::new() }
    }
    fn finance() -> FinanceRecord {
        FinanceRecord::new("sh600000".parse().unwrap(), 0, 0, 0, 0, None, None,
            vec![(FinanceField::CirculatingShares, FinanceValue::Shares(Quantity::new(NumericValue::F64(20000.0), QuantityUnit::Shares)))], ResultMeta::default())
    }
    fn bars(rows: &[(u8, f64)]) -> KlineSeries {
        let bars = rows.iter().map(|&(day, hand)| { let time = EventTime::trading_date(date(day)); let price = Price::from_f64(10.0);
            KlineBar::new(time, time, price, price, price, price, None, lots(hand), cny(0.0), None, None, 0, 0, None) }).collect();
        KlineSeries::new("sh600000".parse().unwrap(), KlinePeriod::Day, KlineKind::Stock, AdjustMode::None, None, BarTime::Start,
            DataPage::new(bars, 0, 6, None, EndEvidence::Unknown, ResultMeta::default()), 1, None)
    }
    fn info(day: u8, clock: (u8, u8, u8), first: Option<TradingDate>, second: Option<TradingDate>) -> HandshakeInfo {
        HandshakeInfo { server_datetime: Some(EventTime::local(date(day), clock.0, clock.1, clock.2).unwrap()),
            server_date1: first, server_date2: second, session_minutes1: Vec::new(), session_minutes2: Vec::new(),
            server_name: String::new(), product_tag: String::new(), unknown_time1: None, unknown_time2: None,
            flags: bytes::Bytes::new(), control_tail: bytes::Bytes::new(), meta: ResultMeta::default() }
    }

    #[test]
    fn readiness_requires_datetime_and_explicit_dates_with_inclusive_0925_boundary() {
        let mut source = info(16, (9, 24, 59), Some(date(13)), Some(date(16)));
        assert_eq!(ready_target(&source).unwrap_err().kind(), ErrorKind::DataNotReady);
        source.server_datetime = Some(EventTime::local(date(16), 9, 25, 0).unwrap()); assert_eq!(ready_target(&source).unwrap(), date(16));
        source.server_datetime = Some(EventTime::local(date(13), 8, 0, 0).unwrap()); assert_eq!(ready_target(&source).unwrap(), date(16));
        source.server_date1 = None; source.server_date2 = None;
        assert_eq!(ready_target(&source).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
        source.server_date1 = Some(date(16)); source.server_datetime = Some(EventTime::trading_date(date(16)));
        assert_eq!(ready_target(&source).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    }

    #[test]
    fn stat2_uses_source_day_columns_and_never_substitutes_an_unaligned_row() {
        let same = stat2(16); let prior = stat2(13);
        let aligned = align_stat2(Some(&same), date(16), date(13));
        assert_eq!(aligned.alignment, StatsAlignment::SameDay); assert_eq!(aligned.amount, same.previous_amount());
        assert_eq!(aligned.seal2, same.previous2_seal()); assert_eq!(aligned.open_volume, same.previous_open_volume());
        let aligned = align_stat2(Some(&prior), date(16), date(13));
        assert_eq!(aligned.alignment, StatsAlignment::PreviousTradingDay); assert_eq!(aligned.amount, prior.amount());
        assert_eq!(aligned.seal, prior.seal()); assert_eq!(aligned.seal2, prior.previous_seal()); assert_eq!(aligned.open_volume, prior.open_volume());
        let bad = stat2(12); let aligned = align_stat2(Some(&bad), date(16), date(13));
        assert_eq!(aligned.alignment, StatsAlignment::DateUnaligned); assert!(aligned.amount.is_none());
        assert_eq!(align_stat2(None, date(16), date(13)).alignment, StatsAlignment::RowMissing);
    }

    #[test]
    fn indicator_keeps_shares_hands_yuan_percentage_points_and_fractions_distinct() {
        let row = build_indicator(quote(10.0, 10.0, 10.0, 2.0, QuantityUnit::Lots), Some(&stat(13)), Some(&stat2(13)),
            Some(&finance()), Some(&bars(&[(13, 240.0), (12, 240.0), (11, 240.0), (10, 240.0), (9, 240.0), (16, 99999.0)])),
            None, date(16), date(13), ResultMeta::default(), || Ok(())).unwrap();
        assert_eq!(row.open_volume().unwrap().value().as_f64(), 1.0); assert_eq!(row.open_volume().unwrap().unit(), QuantityUnit::Lots);
        assert_eq!(row.free_float_shares().unwrap().value().as_f64(), 10000.0); assert_eq!(row.float_shares().unwrap().value().as_f64(), 20000.0);
        assert_eq!(row.free_float_market_value().unwrap().value().as_f64(), 100000.0); assert_eq!(row.float_market_value().unwrap().value().as_f64(), 200000.0);
        for (value, expected) in [(row.open_turnover(), 1.0), (row.open_previous_amount(), 10.0), (row.open_previous_seal(), 50.0),
            (row.seal_to_float(), 2.0), (row.open_change(), 0.0)] {
            let value = value.unwrap(); assert_eq!(value.unit(), RatioUnit::PercentagePoints); assert_eq!(value.value().as_f64(), expected);
        }
        for (value, expected) in [(row.auction_previous_volume(), 0.25), (row.seal_previous(), 1.0), (row.open_volume_ratio(), 1.0), (row.seal_to_amount(), 0.4)] {
            let value = value.unwrap(); assert_eq!(value.unit(), RatioUnit::Fraction); assert_eq!(value.value().as_f64(), expected);
        }
        assert_eq!(row.limit_status(), LimitStatus::UnknownRule); assert!(row.ladder_level().is_none());
        assert_eq!(row.board_text(), Some("3天2板")); assert_eq!(row.limit_stat_days(), Some(3)); assert_eq!(row.pe_ttm(), Some(-2.0));
    }

    #[test]
    fn missing_or_unaligned_rows_do_not_borrow_another_sources_free_float_or_prior_amount() {
        for first in [None, Some(stat(12))] {
            let row = build_indicator(quote(10.0, 10.0, 10.0, 1.0, QuantityUnit::Lots), first.as_ref(), Some(&stat2(12)),
                Some(&finance()), None, None, date(16), date(13), ResultMeta::default(), || Ok(())).unwrap();
            assert!(row.free_float_shares().is_none()); assert!(row.float_shares().is_some()); assert!(row.previous_amount().is_none());
            assert!(row.open_turnover().is_none()); assert!(row.open_volume_ratio().is_none()); assert!(row.ladder_level().is_none());
            assert!(matches!(row.alignment(), StatsAlignment::RowMissing | StatsAlignment::DateUnaligned));
        }
        let row = build_indicator(quote(10.0, 10.0, 10.0, 1.0, QuantityUnit::Shares), Some(&stat(16)), None,
            None, None, None, date(16), date(13), ResultMeta::default(), || Ok(())).unwrap();
        assert!(row.seal_amount().is_none()); assert!(row.seal_to_float().is_none()); assert!(row.float_shares().is_none());
        assert_eq!(row.alignment(), StatsAlignment::RowMissing); assert!(fraction(divide(Some(1.0), Some(0.0))).is_none());
        assert!(divide(Some(1.0), Some(f64::INFINITY)).is_none()); assert!(divide(Some(f64::INFINITY), Some(1.0)).is_none());
    }

    #[test]
    fn prior_volume_uses_five_latest_distinct_dates_and_rejects_insufficient_conflicting_or_zero_units() {
        assert_eq!(prior_minute_volume(Some(&bars(&[(13, 240.0), (12, 240.0), (11, 240.0), (10, 240.0), (9, 240.0), (8, 99999.0)])), date(16), &mut || Ok(())).unwrap(), Some(1.0));
        for rows in [vec![(13, 1.0), (13, 1.0), (12, 1.0), (11, 1.0), (10, 1.0)],
            vec![(13, 1.0), (13, 2.0), (12, 1.0), (11, 1.0), (10, 1.0), (9, 1.0)],
            vec![(13, 0.0), (12, 0.0), (11, 0.0), (10, 0.0), (9, 0.0)]] {
            assert!(prior_minute_volume(Some(&bars(&rows)), date(16), &mut || Ok(())).unwrap().is_none());
        }
        assert_eq!(prior_minute_volume(Some(&bars(&[(13, 1.0)])), date(16), &mut || Err(Error::new(ErrorKind::Cancelled, "stop")))
            .unwrap_err().kind(), ErrorKind::Cancelled);
    }

    #[test]
    fn synthetic_approved_price_classifies_without_registering_a_guessed_exchange_rule() {
        let approved = LimitDecision::new(LimitRule::Approved { id: "synthetic-only".to_owned(), effective_from: date(1), effective_to: None, ratio: None },
            LimitStatus::Normal, Some(Price::from_f64(11.0)), None, vec!["synthetic arithmetic only".to_owned()]);
        for (last, high, bid, hands, status) in [(11.0, 11.0, 11.0, 2.0, LimitStatus::Sealed),
            (10.0, 11.0, 10.0, 1.0, LimitStatus::Touched), (10.0, 10.0, 10.0, 1.0, LimitStatus::NotAtLimit),
            (11.0, 11.0, 10.0, 0.0, LimitStatus::Touched)] {
            assert_eq!(classify_limit(quote(last, high, bid, hands, QuantityUnit::Lots).snapshot(), &approved), status);
        }
        let previous = stat(13); let current = stat(16);
        assert_eq!(board_progress(Some(&previous), date(16), date(13), LimitStatus::Sealed).unwrap(), (Some("4天3板".to_owned()), Some(3)));
        assert_eq!(board_progress(Some(&current), date(16), date(13), LimitStatus::Sealed).unwrap(), (Some("3天2板".to_owned()), Some(2)));
        assert_eq!(previous.limit_stat_days(), Some(3)); assert_eq!(previous.limit_streak(), Some(2));
        let mut over = previous; over.limit_streak = Some(u32::MAX);
        assert_eq!(board_progress(Some(&over), date(16), date(13), LimitStatus::Sealed).unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn ladder_filters_then_sorts_all_rows_before_truncating_and_observes_cancellation() {
        let make = |number: &str, status, level, seal| {
            let mut row = build_indicator(quote(10.0, 10.0, 10.0, 1.0, QuantityUnit::Lots), None, None,
                None, None, None, date(16), date(13), ResultMeta::default(), || Ok(())).unwrap();
            row.id = format!("sh{number}").parse().unwrap(); row.limit_status = status; row.ladder_level = level; row.seal_amount = Some(cny(seal)); row
        };
        let input = vec![make("600005", LimitStatus::UnknownRule, Some(9), 9999.0), make("600003", LimitStatus::Sealed, Some(2), 100.0),
            make("600002", LimitStatus::Sealed, Some(2), 100.0), make("600001", LimitStatus::Sealed, Some(3), 1.0),
            make("600004", LimitStatus::Touched, None, 999.0)];
        let selected = select_ladder(input.clone(), false, Some(2), || Ok(())).unwrap();
        assert_eq!(selected.iter().map(|row| row.id().number()).collect::<Vec<_>>(), vec!["600001", "600002"]);
        assert_eq!(select_ladder(input.clone(), true, None, || Ok(())).unwrap().len(), 4);
        assert_eq!(select_ladder(input, false, None, || Err(Error::new(ErrorKind::Cancelled, "stop"))).unwrap_err().kind(), ErrorKind::Cancelled);
    }
}
