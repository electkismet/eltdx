use std::collections::HashMap;
use std::time::Instant;

use crate::operation::{OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, allocation_error, checked_ids, is_a_share, CorporateService, KlineService};
use crate::types::{BatchItemState, CalendarQuery, CapitalChangeBlock, CapitalQuery, CodeScanQuery,
    DailyLimitsQuery, DailyPriceLimit, DailyShareCapital, DailySharesQuery, Diagnostic,
    EnrichmentStatus, FinanceField, FinanceRecord, HqMarket, KlinePeriod, KlineQuery, KlineSeries,
    LimitStatus, NumericValue, Price, QuantityUnit, ResultMeta, Security, SecurityId,
    SecuritySelection, StatRow, Table, TradingDate, ValueProvenance};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::profile::shares;
use super::service::HelperService;

const MARKETS: [HqMarket; 3] = [HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing];

pub(super) struct Selected {
    pub ids: Vec<SecurityId>, pub securities: HashMap<SecurityId, Security>, pub meta: ResultMeta,
}

impl HelperService {
    /// Required statistics, optional finance. A missing free-float field is
    /// never replaced by a different financial share count.
    pub fn daily_share_capital(&self, query: &DailySharesQuery, options: &OperationOptions) -> Result<Table<DailyShareCapital>> {
        let mut context = self.executor.begin_context_at(Instant::now(), "helpers_daily_share_capital", &self.config, options)?;
        query.selection().validate()?; query.stats().validate()?;
        if query.selection().is_empty() { context.finish()?; return Ok(Table::new(Vec::new(), ResultMeta::default())); }
        freeze_limits(&mut context, query.selection(), false)?;
        let selected = self.select_in_context(&context, query.selection())?;
        if selected.ids.is_empty() { context.finish()?; return Ok(Table::new(Vec::new(), selected.meta)); }
        let (target, handshake) = self.source_date_in_context(&context)?;
        let calendar = self.calendar_in_context(&context)?;
        let previous = self.previous_in_calendar(&calendar, target)?;
        let (stats, stats_date, refreshed) = self.aligned_stats_in_context(&context, query.stats(), query.refresh_stats(), target, previous)?;
        let (finance, finance_hit) = self.profile_finance(&context, &selected.ids)?;
        let bundle = finance.result.as_ref().map_err(Clone::clone)?;
        if bundle.rows.len() != selected.ids.len() { return Err(internal("daily finance lost input occurrences")); }
        let mut stat_by_id = HashMap::new(); stat_by_id.try_reserve(stats.stat().len()).map_err(allocation_error)?;
        for row in stats.stat() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            if let Some(id) = row.id() {
                if stat_by_id.insert(id, row).is_some() { return Err(protocol("duplicate projected statistics identity")); }
            }
        }
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let mut meta = aggregate_meta(&context, &[selected.meta, handshake.meta().clone(), calendar.meta().clone(),
            stats.meta().clone(), bundle.meta.clone()], version)?;
        meta = meta.with_diagnostic(Diagnostic::new("stats", "daily_shares.stats", EnrichmentStatus::Succeeded, None,
            Some(format!("target={target}; previous={previous}; stats_date={stats_date}; refreshed={refreshed}"))))
            .with_diagnostic(finance.diagnostic("hq.finance", "daily_shares.finance_cache", finance_hit,
                if bundle.any_failed { EnrichmentStatus::Failed } else { EnrichmentStatus::Succeeded }));
        let mut rows = Vec::new(); rows.try_reserve(selected.ids.len()).map_err(allocation_error)?;
        for (id, source) in selected.ids.into_iter().zip(&bundle.rows) {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let mut row_meta = aggregate_meta(&context, &[stats.meta().clone()], version)?
                .with_diagnostic(source.diagnostic.clone());
            if let Some(finance) = &source.value {
                if finance.id() != &id { return Err(internal("daily finance occurrence identity changed")); }
                row_meta = aggregate_meta(&context, &[row_meta, finance.meta().clone()], version)?;
            }
            rows.push(build_shares(id.clone(), target, previous, stat_by_id.get(&id).copied(), source.value.as_ref(), row_meta));
        }
        context.check_boundary(TimeoutPhase::Aggregate)?; context.finish()?;
        Ok(Table::new(rows, meta))
    }

    /// Reference comes from an unadjusted daily bar and ordered same-day
    /// corporate actions. Only the approved dated policy may supply prices.
    pub fn daily_price_limits(&self, query: &DailyLimitsQuery, options: &OperationOptions) -> Result<Table<DailyPriceLimit>> {
        let mut context = self.executor.begin_context_at(Instant::now(), "helpers_daily_price_limits", &self.config, options)?;
        query.selection().validate()?;
        if query.selection().is_empty() { context.finish()?; return Ok(Table::new(Vec::new(), ResultMeta::default())); }
        freeze_limits(&mut context, query.selection(), true)?;
        let calendar = self.calendar_in_context(&context)?;
        calendar.require_coverage(query.trade_date())?;
        if calendar.days().binary_search(&query.trade_date()).is_err() {
            return Err(Error::new(ErrorKind::InvalidArgument, "trade_date is not an observed trading day")
                .with_context("trade_date", format!("{}", query.trade_date())));
        }
        let selected = self.select_in_context(&context, query.selection())?;
        if selected.ids.is_empty() { context.finish()?; return Ok(Table::new(Vec::new(), selected.meta)); }
        let service = KlineService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let corporate = CorporateService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let changes = corporate.capital_collect_in_context(&context, &selected.ids, &CapitalQuery::new())?;
        if changes.len() != selected.ids.len() { return Err(internal("capital changes lost input occurrences")); }
        let mut bars_by_id = HashMap::<SecurityId, KlineSeries>::new();
        bars_by_id.try_reserve(selected.ids.len()).map_err(allocation_error)?;
        let mut metas = vec![selected.meta, calendar.meta().clone(), changes.meta().clone()];
        for id in &selected.ids {
            context.check_boundary(TimeoutPhase::Admission)?;
            if bars_by_id.contains_key(id) { continue; }
            let series = service.collect_with_parent_context(&context, id, &KlineQuery::new())?;
            metas.try_reserve(1).map_err(allocation_error)?; metas.push(series.meta().clone());
            bars_by_id.insert(id.clone(), series);
        }
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let mut rows = Vec::new(); rows.try_reserve(selected.ids.len()).map_err(allocation_error)?;
        for (index, item) in changes.into_items().into_iter().enumerate() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let (input_index, id, state) = item.into_parts();
            if index != input_index || selected.ids.get(index) != Some(&id) { return Err(internal("capital occurrence association changed")); }
            let BatchItemState::Present(changes) = state else { return Err(Error::new(ErrorKind::MissingData, "required capital source is missing")); };
            let bars = bars_by_id.get(&id).ok_or_else(|| internal("required reference series disappeared"))?;
            let reference = latest_reference(bars, query.trade_date(), || context.check_boundary(TimeoutPhase::Aggregate))?;
            let reference_date = reference.map(|(date, _)| date);
            let reference = adjusted_reference(reference.map(|(_, price)| price), &changes, query.trade_date(),
                || context.check_boundary(TimeoutPhase::Aggregate))?;
            let decision = crate::policy::limits::evaluate(&id, query.trade_date(), reference)?;
            let status = if reference.is_none() { LimitStatus::MissingReference } else { decision.status() };
            let mut meta = aggregate_meta(&context, &[bars.meta().clone(), changes.meta().clone(), calendar.meta().clone()], version)?;
            meta = meta.with_diagnostic(Diagnostic::new("price_limits_v1", "daily_limits.rule",
                if matches!(decision.rule(), crate::types::LimitRule::Unknown { .. }) { EnrichmentStatus::Unavailable }
                else { EnrichmentStatus::Succeeded }, None, Some(format!("status={:?}; evidence={:?}", decision.status(), decision.evidence()))));
            if reference.is_none() { meta = meta.with_diagnostic(Diagnostic::new("hq.klines", "daily_limits.reference",
                EnrichmentStatus::Unavailable, Some(ErrorKind::MissingData), Some("no valid prior unadjusted daily close".to_owned()))); }
            rows.push(DailyPriceLimit { name: selected.securities.get(&id).map(|security| security.name().to_owned()), id,
                trade_date: query.trade_date(), reference_date, reference, upper: decision.upper(), lower: decision.lower(),
                rule: decision.rule().clone(), status, reference_source: reference.map(|_| "unadjusted_daily+same_day_tag1".to_owned()), meta });
        }
        let meta = aggregate_meta(&context, &metas, version)?;
        context.finish()?; Ok(Table::new(rows, meta))
    }

    pub(super) fn select_in_context(&self, context: &OperationContext, selection: &SecuritySelection) -> Result<Selected> {
        selection.validate()?;
        match selection {
            SecuritySelection::Explicit(ids) => {
                let mut copy = Vec::new(); copy.try_reserve(ids.len()).map_err(allocation_error)?; copy.extend_from_slice(ids);
                Ok(Selected { ids: copy, securities: HashMap::new(), meta: ResultMeta::default() })
            }
            SecuritySelection::AllAShares => {
                let catalog = self.codes().collect_in_context(context, &MARKETS, &CodeScanQuery::new(), is_a_share)?;
                let mut ids = Vec::new(); ids.try_reserve(catalog.items().len()).map_err(allocation_error)?;
                let mut securities = HashMap::new(); securities.try_reserve(catalog.items().len()).map_err(allocation_error)?;
                for security in catalog.items() {
                    context.check_boundary(TimeoutPhase::Aggregate)?;
                    ids.push(security.id().clone()); securities.insert(security.id().clone(), security.clone());
                }
                checked_ids(&ids)?;
                Ok(Selected { ids, securities, meta: catalog.meta().clone() })
            }
        }
    }
}

fn freeze_limits(context: &mut OperationContext, selection: &SecuritySelection, references: bool) -> Result<()> {
    let scan = CodeScanQuery::default().pages().max_records();
    let count = match selection { SecuritySelection::Explicit(ids) => ids.len(), SecuritySelection::AllAShares => scan.min(65_535) };
    let per_id = if references { KlineQuery::default().pages().max_records().checked_add(1) } else { Some(1) };
    let records = per_id.and_then(|per_id| count.checked_mul(per_id))
        .and_then(|records| records.checked_add(scan))
        .and_then(|records| records.checked_add(CalendarQuery::default().pages().max_records()))
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "daily helper work allowance overflows"))?;
    context.set_work_limits(WorkLimits { pages: None, records: Some(records), bytes: None })
}

fn build_shares(id: SecurityId, target: TradingDate, previous: TradingDate, stat: Option<&StatRow>,
    finance: Option<&FinanceRecord>, mut meta: ResultMeta) -> DailyShareCapital {
    let total = shares(finance, FinanceField::TotalShares, "daily_shares.total", &mut meta);
    let circulating = shares(finance, FinanceField::CirculatingShares, "daily_shares.circulating", &mut meta);
    let free_float = stat.and_then(|row| {
        if row.stats_date() != Some(target) && row.stats_date() != Some(previous) {
            meta = meta.clone().with_diagnostic(Diagnostic::new("tdxstat.cfg", "daily_shares.free_float", EnrichmentStatus::Unavailable,
                Some(ErrorKind::ResourceFormat), Some(format!("row date {:?} is unaligned", row.stats_date()))));
            return None;
        }
        row.free_float_shares().filter(|value| value.unit() == QuantityUnit::Shares
            && value.value().is_finite() && value.value().as_f64() >= 0.0)
    });
    let sources = super::calculation::share_source_names(total.is_some(), circulating.is_some(), free_float.is_some())
        .into_iter().map(str::to_owned).collect();
    meta = meta.with_diagnostic(Diagnostic::new("tdxstat.cfg", "daily_shares.stats_row",
        if free_float.is_some() { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable }, None,
        Some(format!("stats_date={:?}; source_line={:?}; source_10k={:?}", stat.and_then(StatRow::stats_date),
            stat.map(StatRow::source_line), stat.and_then(StatRow::free_float_shares_source_10k)))));
    DailyShareCapital { id, trade_date: target, total, circulating, free_float,
        finance_updated: finance.and_then(FinanceRecord::updated), sources, meta }
}

fn latest_reference(series: &KlineSeries, target: TradingDate, mut boundary: impl FnMut() -> Result<()>) -> Result<Option<(TradingDate, Price)>> {
    if series.period() != KlinePeriod::Day || series.adjust() != crate::types::AdjustMode::None {
        return Err(internal("daily limit reference is not an unadjusted daily series"));
    }
    let mut reference: Option<(TradingDate, Price)> = None;
    for bar in series.bars().items() {
        boundary()?;
        let date = bar.source_time().date().ok_or_else(|| protocol("daily reference bar has no source date"))?;
        if date >= target { continue; }
        if reference.is_none_or(|(existing, _)| date > existing) { reference = Some((date, bar.close())); }
        else if reference.is_some_and(|(existing, price)| date == existing && price != bar.close()) {
            return Err(protocol("daily reference has conflicting closes on the same date"));
        }
    }
    Ok(reference.filter(|(_, price)| price.as_f64().is_finite() && price.as_f64() > 0.0))
}

fn adjusted_reference(reference: Option<Price>, changes: &CapitalChangeBlock, target: TradingDate,
    mut boundary: impl FnMut() -> Result<()>) -> Result<Option<Price>> {
    let Some(price) = reference else { return Ok(None); };
    let mut value = price.as_f64(); let mut changed = false;
    for event in changes.records() {
        boundary()?;
        if event.category() != 1 || event.date() != Some(target) { continue; }
        let (multiplier, offset) = crate::adjustment::event_coefficients(event)
            .map_err(|error| protocol("same-day capital event has invalid source coefficients").with_source(error))?;
        value = super::calculation::inverse_affine_with::<_, std::convert::Infallible>(value, &multiplier, &offset,
            |value, offset| Ok(value - offset), |value, multiplier| Ok(value / multiplier))
            .unwrap_or_else(|never| match never {});
        changed = true;
        if !value.is_finite() || value <= 0.0 { return Err(protocol("same-day capital adjustment produces an invalid reference")); }
    }
    Ok(Some(if changed { Price::with_value_and_provenance(NumericValue::F64(value), ValueProvenance::WireRule) } else { price }))
}
fn protocol(message: &str) -> Error { Error::new(ErrorKind::Protocol, message) }
fn internal(message: &str) -> Error { Error::new(ErrorKind::Internal, message) }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{Amount, AmountUnit, BarTime, DataPage, EndEvidence, EventTime,
        FinanceValue, KlineBar, KlineKind, StatRow, Quantity};

    fn date(day: u8) -> TradingDate { TradingDate::new(2026, 11, day).unwrap() }
    fn stat(at: TradingDate, value: Option<f64>) -> StatRow {
        StatRow { id: Some("sh600000".parse().unwrap()), source_market: "1".to_owned(), source_code: "600000".to_owned(),
            stats_date: Some(at), stats_date_raw: Some(format!("202611{:02}", at.day())), beta_60d: None, pe_ttm: None,
            free_float_shares: value.map(|value| Quantity::shares_from_ten_thousands_f64(value, ValueProvenance::WireRule).unwrap()),
            free_float_shares_source_10k: value, year_limit_up_days: None, limit_stat_days: None, limit_up_count: None,
            limit_streak: None, source_line: 17, source_counts: [None, None, None, None] }
    }
    #[test]
    fn share_sources_keep_free_float_distinct_and_unaligned_or_missing_rows_do_not_substitute() {
        let id = "sh600000".parse().unwrap();
        let finance = FinanceRecord::new(id, 0, 0, 20261113, 0, Some(date(13)), None,
            vec![(FinanceField::TotalShares, FinanceValue::Shares(Quantity::new(NumericValue::F64(30000.0), QuantityUnit::Shares))),
                (FinanceField::CirculatingShares, FinanceValue::Shares(Quantity::new(NumericValue::F64(20000.0), QuantityUnit::Shares)))],
            ResultMeta::default());
        let row = build_shares(finance.id().clone(), date(16), date(13), Some(&stat(date(13), Some(1.5))), Some(&finance), ResultMeta::default());
        assert_eq!(row.trade_date(), date(16)); assert_eq!(row.total().unwrap().value().as_f64(), 30000.0);
        assert_eq!(row.circulating().unwrap().value().as_f64(), 20000.0);
        assert_eq!(row.free_float().unwrap().value().as_f64(), 15000.0);
        assert_eq!(row.free_float().unwrap().source_unit(), QuantityUnit::TenThousandShares);
        assert_eq!(row.finance_updated(), Some(date(13)));
        for stat in [None, Some(stat(date(12), Some(1.5))), Some(stat(date(13), None))] {
            let row = build_shares(finance.id().clone(), date(16), date(13), stat.as_ref(), Some(&finance), ResultMeta::default());
            assert!(row.free_float().is_none()); assert!(row.circulating().is_some()); assert_eq!(row.sources(), &["finance_snapshot"]);
        }
    }

    fn series(rows: &[(u8, i64)]) -> KlineSeries {
        let bars = rows.iter().map(|&(day, value)| {
            let time = EventTime::trading_date(date(day)); let price = Price::scaled_integer(value, 2);
            KlineBar::new(time, time, price, price, price, price, None, Quantity::new(NumericValue::F64(0.0), QuantityUnit::Lots),
                Amount::new(NumericValue::F64(0.0), AmountUnit::Unknown), None, None, 0, 0, None)
        }).collect();
        KlineSeries::new("sh600000".parse().unwrap(), KlinePeriod::Day, KlineKind::Stock, crate::types::AdjustMode::None,
            None, BarTime::Start, DataPage::new(bars, 0, 800, None, EndEvidence::EmptyPage, ResultMeta::default()), 1, None)
    }
    #[test]
    fn reference_uses_latest_prior_source_day_and_applies_same_day_tag_one_in_original_order() {
        let bars = series(&[(12, 800), (16, 4000), (13, 1000)]);
        let prior = latest_reference(&bars, date(16), || Ok(())).unwrap().unwrap();
        assert_eq!(prior, (date(13), Price::scaled_integer(1000, 2)));
        let event = |at, category, cash, bonus| crate::types::CapitalEvent::from_wire(at, category,
            [cash, 0.0, bonus, 0.0].map(f32::to_le_bytes), 0);
        let changes = CapitalChangeBlock::new("sh600000".parse().unwrap(), vec![
            event(20261116, 1, 10.0, 10.0), event(20261116, 2, 999.0, 999.0),
            event(20261113, 1, 999.0, 999.0), event(20261116, 1, 5.0, 0.0)], 1, ResultMeta::default());
        let price = adjusted_reference(Some(prior.1), &changes, date(16), || Ok(())).unwrap().unwrap();
        assert_eq!(price.as_f64(), 4.0); // ((10 - 1) / 2 - .5) / 1.
        let decision = crate::policy::limits::evaluate(changes.id(), date(16), Some(price)).unwrap();
        assert_eq!(decision.status(), LimitStatus::UnknownRule); assert!(decision.upper().is_none()); assert!(decision.lower().is_none());
        assert!(latest_reference(&series(&[(16, 1000)]), date(16), || Ok(())).unwrap().is_none());
        assert_eq!(latest_reference(&bars, date(16), || Err(Error::new(ErrorKind::Cancelled, "stop"))).unwrap_err().kind(), ErrorKind::Cancelled);
        assert_eq!(latest_reference(&series(&[(13, 1000), (13, 1001)]), date(16), || Ok(())).unwrap_err().kind(), ErrorKind::Protocol);
    }
}
