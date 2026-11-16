use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, allocation_error, checked_ids, CorporateService};
use crate::types::{Amount, AmountUnit, BatchItemState, CodeScanQuery, Collected, Diagnostic,
    EnrichmentStatus, FinanceField, FinanceQuery, FinanceRecord, FinanceValue, FullQuote,
    HqMarket, MissingPolicy, NumericValue, Price, ProfileQuery, Quantity, QuantityUnit,
    Ratio, RatioUnit, ResultMeta, Security, SecurityId, StockProfile, Table, ValueProvenance};
use crate::{Error, ErrorKind, Result, TimeoutPhase};

use super::cache::{CacheStamp, CachedSource, FinanceBundle, Supplement};
use super::service::{failed_diagnostic, optional_failure, HelperService};

impl HelperService {
    pub fn stock_profile_table(&self, ids: &[SecurityId], query: &ProfileQuery,
        options: &OperationOptions) -> Result<Table<StockProfile>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "helpers_stock_profile_table", &self.config, options)?;
        checked_ids(ids)?;
        if ids.is_empty() { context.finish()?; return Ok(Table::new(Vec::new(), ResultMeta::default())); }
        let mut markets = Vec::new();
        for id in ids { if !markets.contains(&id.market()) { markets.push(id.market()); } }
        let directory_records = if query.include_security() {
            markets.len().checked_mul(CodeScanQuery::default().pages().max_records()).ok_or_else(overflow)?
        } else { 0 };
        let records = ids.len().checked_mul(3).and_then(|count| count.checked_add(directory_records)).ok_or_else(overflow)?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(records), bytes: None })?;
        let primary = self.full_quotes_in_context(&context, ids)?;
        let mut metas = Vec::new(); metas.try_reserve(markets.len() + 2).map_err(allocation_error)?;
        metas.push(primary.meta().clone());
        let mut source_diagnostics = Vec::new();
        let mut catalogs = HashMap::new(); catalogs.try_reserve(markets.len()).map_err(allocation_error)?;
        let mut securities = HashMap::new();
        if query.include_security() {
            for market in markets {
                let (source, hit) = self.profile_catalog(&context, market)?;
                let status = if source.result.is_ok() { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Failed };
                source_diagnostics.push(source.diagnostic("hq.directory", "profile.security_cache", hit, status));
                match &source.result {
                    Ok(catalog) => {
                        metas.push(catalog.meta().clone());
                        securities.try_reserve(catalog.items().len()).map_err(allocation_error)?;
                        for security in catalog.items() { securities.insert(security.id().clone(), security.clone()); }
                    }
                    Err(error) => { optional_failure(&context, error)?; }
                }
                catalogs.insert(market, source);
            }
        }
        let finance = if query.include_finance() {
            let (source, hit) = self.profile_finance(&context, ids)?;
            let bundle = source.result.as_ref().map_err(Clone::clone)?;
            let status = if bundle.any_failed { EnrichmentStatus::Failed } else { EnrichmentStatus::Succeeded };
            source_diagnostics.push(source.diagnostic("hq.finance", "profile.finance_cache", hit, status));
            metas.push(bundle.meta.clone());
            Some(source)
        } else { None };
        let finance_bundle = finance.as_ref().map(|source| source.result.as_ref().map_err(Clone::clone)).transpose()?;
        if finance_bundle.is_some_and(|bundle| bundle.rows.len() != ids.len()) {
            return Err(Error::new(ErrorKind::Internal, "cached finance occurrences do not match the profile request"));
        }
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let mut rows = Vec::new(); rows.try_reserve(ids.len()).map_err(allocation_error)?;
        for item in primary.into_items() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let (index, id, state) = item.into_parts();
            if ids.get(index) != Some(&id) { return Err(Error::new(ErrorKind::Internal, "profile quote occurrence changed")); }
            let BatchItemState::Present(quote) = state else {
                return Err(Error::new(ErrorKind::MissingData, "profile is missing its required quote"));
            };
            let catalog = catalogs.get(&id.market());
            let security = if !query.include_security() { not_requested("hq.directory", "profile.security") }
                else if let Some(source) = catalog {
                    match &source.result {
                        Ok(_) => match securities.get(&id) {
                            Some(security) => Supplement { value: Some(security.clone()), status: EnrichmentStatus::Succeeded,
                                diagnostic: Diagnostic::new("hq.directory", "profile.security", EnrichmentStatus::Succeeded, None, None) },
                            None => unavailable("hq.directory", "profile.security", "security is absent from the complete directory"),
                        },
                        Err(error) => Supplement { value: None, status: EnrichmentStatus::Failed,
                            diagnostic: failed_diagnostic("hq.directory", "profile.security", error) },
                    }
                } else { return Err(Error::new(ErrorKind::Internal, "profile directory source was not retained")); };
            let finance = if let Some(bundle) = finance_bundle { bundle.rows[index].clone() }
                else { not_requested("hq.finance", "profile.finance") };
            let mut row_metas = Vec::new(); row_metas.try_reserve(2).map_err(allocation_error)?;
            row_metas.push(quote.snapshot().meta().clone());
            if let Some(record) = &finance.value { row_metas.push(record.meta().clone()); }
            let mut meta = aggregate_meta(&context, &row_metas, version)?;
            let mut sources = Vec::new(); sources.try_reserve(meta.sources().len() + 1).map_err(allocation_error)?;
            sources.extend_from_slice(meta.sources());
            if let Some(source) = quote.depth_source() { sources.push(source.clone()); }
            if let Some(catalog) = catalog.and_then(|source| source.result.as_ref().ok()) {
                sources.try_reserve(catalog.meta().sources().len()).map_err(allocation_error)?;
                sources.extend_from_slice(catalog.meta().sources());
            }
            meta = meta.with_sources(sources);
            for diagnostic in quote.diagnostics() { meta = meta.with_diagnostic(diagnostic.clone()); }
            for diagnostic in &source_diagnostics { meta = meta.with_diagnostic(diagnostic.clone()); }
            rows.push(build_profile(quote, security, finance, meta)?);
        }
        let mut meta = aggregate_meta(&context, &metas, version)?;
        for diagnostic in source_diagnostics { meta = meta.with_diagnostic(diagnostic); }
        context.finish()?;
        Ok(Table::new(rows, meta))
    }

    /// Clears this owner's helper memory only. It does not clear workday or
    /// precision caches, close the session, or read/delete user files.
    pub fn clear_cache(&self) { self.executor.owner().helpers().clear(); }

    fn profile_catalog(&self, context: &OperationContext, market: HqMarket)
        -> Result<(CachedSource<Collected<Security>>, bool)> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let version = context.owner().metadata().catalog_version(&self.namespace, &[market])?;
        let mut stamp = self.cache_stamp(context, version)?;
        let cache = context.owner().helpers();
        let (generation, hit) = cache.catalog(market, stamp)?;
        if let Some(source) = hit { return Ok((source, true)); }
        let result = self.codes().collect_in_context(context, &[market], &CodeScanQuery::default(), |_| true);
        if let Err(error) = &result { optional_failure(context, error)?; }
        if let Ok(catalog) = &result {
            // A complete scan publishes a newer catalog. Key by that actual
            // version; retaining the pre-scan version would force endless misses.
            stamp.version = catalog.meta().source().metadata_version();
        }
        let source = CachedSource::new(result.map(Arc::new), stamp, true)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        cache.publish_catalog(generation, market, source.clone())?;
        Ok((source, false))
    }

    pub(super) fn profile_finance(&self, context: &OperationContext, ids: &[SecurityId]) -> Result<(CachedSource<FinanceBundle>, bool)> {
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let stamp = self.cache_stamp(context, version)?;
        let cache = context.owner().helpers();
        let (generation, hit) = cache.finance(ids, stamp)?;
        if let Some(source) = hit { return Ok((source, true)); }
        let corporate = CorporateService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let query = FinanceQuery::new().with_missing(MissingPolicy::AllowPartial)?;
        let mut rows = Vec::new(); rows.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut metas = Vec::new(); metas.try_reserve(ids.len().div_ceil(80)).map_err(allocation_error)?;
        let mut any_failed = false;
        for chunk in super::service::full_quote_chunks(ids) {
            context.check_boundary(TimeoutPhase::Admission)?;
            match corporate.finance_collect_in_context(context, chunk, &query, 80, ChildPurpose::Fallback) {
                Ok(batch) => {
                    if batch.len() != chunk.len() { return Err(Error::new(ErrorKind::Internal, "finance result lost input occurrences")); }
                    metas.push(batch.meta().clone());
                    for (index, item) in batch.into_items().into_iter().enumerate() {
                        let (input_index, id, state) = item.into_parts();
                        if index != input_index || chunk.get(index) != Some(&id) {
                            return Err(Error::new(ErrorKind::Internal, "finance result changed its input association"));
                        }
                        rows.push(match state {
                            BatchItemState::Present(record) => Supplement { value: Some(record), status: EnrichmentStatus::Succeeded,
                                diagnostic: Diagnostic::new("hq.finance", "profile.finance", EnrichmentStatus::Succeeded, None, None) },
                            BatchItemState::ConfirmedMissing(_) => unavailable("hq.finance", "profile.finance", "finance record is confirmed missing"),
                        });
                    }
                }
                Err(error) => {
                    optional_failure(context, &error)?; any_failed = true;
                    let diagnostic = failed_diagnostic("hq.finance", "profile.finance", &error);
                    for _ in chunk { rows.push(Supplement { value: None, status: EnrichmentStatus::Failed, diagnostic: diagnostic.clone() }); }
                }
            }
        }
        let mut meta = aggregate_meta(context, &metas, version)?;
        for row in &rows {
            if row.status != EnrichmentStatus::Succeeded { meta = meta.with_diagnostic(row.diagnostic.clone()); }
        }
        let source = CachedSource::new(Ok(Arc::new(FinanceBundle { rows, meta, any_failed })), stamp, !any_failed)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        cache.publish_finance(generation, ids, source.clone())?;
        Ok((source, false))
    }

    pub(super) fn cache_stamp(&self, context: &OperationContext, version: Option<u64>) -> Result<CacheStamp> {
        context.check_boundary(TimeoutPhase::Admission)?;
        Ok(CacheStamp { date: context.current_market_date()?, version, raw: context.raw(),
            loaded_at: self.config.clock().unix_seconds()? })
    }
}

fn build_profile(quote: FullQuote, security: Supplement<Security>, finance: Supplement<FinanceRecord>,
    mut meta: ResultMeta) -> Result<StockProfile> {
    let id = quote.snapshot().id();
    if security.value.as_ref().is_some_and(|security| security.id() != id)
        || finance.value.as_ref().is_some_and(|finance| finance.id() != id) {
        return Err(Error::new(ErrorKind::Internal, "profile supplemental identity differs from its primary quote"));
    }
    meta = meta.with_diagnostic(security.diagnostic).with_diagnostic(finance.diagnostic);
    let circulating_shares = shares(finance.value.as_ref(), FinanceField::CirculatingShares, "profile.circulating_shares", &mut meta);
    let total_shares = shares(finance.value.as_ref(), FinanceField::TotalShares, "profile.total_shares", &mut meta);
    let turnover = circulating_shares.and_then(|shares| {
        let volume = quote.snapshot().total();
        let denominator = shares.value().as_f64();
        let value = super::calculation::turnover_with::<_, std::convert::Infallible>(
            volume.value().as_f64(), &denominator, |value| Ok(value * 100.0),
            |value, denominator| Ok(value / denominator)).unwrap_or_else(|never| match never {});
        if volume.unit() == QuantityUnit::Lots && volume.value().as_f64() >= 0.0 && denominator > 0.0 && value.is_finite() {
            Some(Ratio::with_provenance(NumericValue::F64(value), RatioUnit::PercentagePoints, ValueProvenance::WireRule))
        } else {
            derived_unavailable(&mut meta, "profile.turnover", "turnover requires finite hand volume and positive circulating shares"); None
        }
    });
    let circulating_market_value = market_value(quote.snapshot().last(), circulating_shares, "profile.circulating_market_value", &mut meta);
    let total_market_value = market_value(quote.snapshot().last(), total_shares, "profile.total_market_value", &mut meta);
    let eps = finance.value.as_ref().and_then(|finance| finance.value(FinanceField::Eps)).and_then(|value| match value {
        FinanceValue::Unclassified { value, .. } if value.is_finite() => Some(Price::with_value_and_provenance(
            NumericValue::F32(value), ValueProvenance::WireRule)),
        FinanceValue::PerShare(price) if price.as_f64().is_finite() => Some(price),
        _ => { derived_unavailable(&mut meta, "profile.eps", "EPS is absent, nonfinite or has an unsupported source type"); None },
    });
    Ok(StockProfile { quote, security: security.value, finance: finance.value, circulating_shares, total_shares,
        turnover, circulating_market_value, total_market_value, eps, security_status: security.status,
        finance_status: finance.status, meta })
}

pub(super) fn shares(finance: Option<&FinanceRecord>, field: FinanceField, stage: &str, meta: &mut ResultMeta) -> Option<Quantity> {
    match finance?.value(field) {
        Some(FinanceValue::Shares(value)) if value.unit() == QuantityUnit::Shares && value.value().as_f64().is_finite()
            && value.value().as_f64() >= 0.0 => Some(value),
        _ => { derived_unavailable(meta, stage, "share count requires a finite nonnegative source quantity in shares"); None },
    }
}
fn market_value(price: Price, shares: Option<Quantity>, stage: &str, meta: &mut ResultMeta) -> Option<Amount> {
    let value = super::calculation::product_with::<_, std::convert::Infallible>(
        price.as_f64(), shares?.value().as_f64(), |left, right| Ok(left * right))
        .unwrap_or_else(|never| match never {});
    if price.as_f64() >= 0.0 && value.is_finite() {
        Some(Amount::with_provenance(NumericValue::F64(value), AmountUnit::Cny, ValueProvenance::WireRule))
    } else { derived_unavailable(meta, stage, "market value exceeds its finite nonnegative CNY range"); None }
}
fn derived_unavailable(meta: &mut ResultMeta, stage: &str, message: &str) {
    *meta = meta.clone().with_diagnostic(Diagnostic::new("hq.profile", stage, EnrichmentStatus::Unavailable,
        Some(ErrorKind::Protocol), Some(message.to_owned())));
}
fn unavailable<T>(source: &str, stage: &str, message: &str) -> Supplement<T> {
    Supplement { value: None, status: EnrichmentStatus::Unavailable,
        diagnostic: Diagnostic::new(source, stage, EnrichmentStatus::Unavailable, None, Some(message.to_owned())) }
}
fn not_requested<T>(source: &str, stage: &str) -> Supplement<T> {
    Supplement { value: None, status: EnrichmentStatus::NotRequested,
        diagnostic: Diagnostic::new(source, stage, EnrichmentStatus::NotRequested, None, None) }
}
fn overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "profile aggregate record allowance overflows") }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{EventTime, Snapshot};

    fn primary() -> FullQuote {
        let price = Price::from_f64(12.0);
        let quantity = Quantity::new(NumericValue::F64(50.0), QuantityUnit::Lots);
        let amount = Amount::new(NumericValue::F64(60000.0), AmountUnit::Cny);
        FullQuote { snapshot: Snapshot::new("sh600000".parse().unwrap(), 0, price, price, price, price, price,
            EventTime::unknown_raw(0), 0, quantity, quantity, quantity, quantity, amount, amount,
            Vec::new(), Vec::new(), ResultMeta::default()), depth_status: EnrichmentStatus::NotRequested,
            depth_source: None, diagnostics: Vec::new() }
    }
    fn source(circulation: FinanceValue, eps: f32) -> Supplement<FinanceRecord> {
        let id = "sh600000".parse().unwrap();
        Supplement { value: Some(FinanceRecord::new(id, 1, 2, 0, 0, None, None,
            vec![(FinanceField::CirculatingShares, circulation),
                (FinanceField::TotalShares, FinanceValue::Shares(Quantity::new(NumericValue::F64(20000.0), QuantityUnit::Shares))),
                (FinanceField::Eps, FinanceValue::Unclassified { value: eps, bits: eps.to_le_bytes() })], ResultMeta::default())),
            status: EnrichmentStatus::Succeeded, diagnostic: Diagnostic::new("hq.finance", "profile.finance", EnrichmentStatus::Succeeded, None, None) }
    }

    #[test]
    fn invalid_shares_and_nonfinite_eps_preserve_original_finance_and_omit_only_derived_fields() {
        for quantity in [Quantity::new(NumericValue::F64(-1.0), QuantityUnit::Shares),
            Quantity::new(NumericValue::F64(1.0), QuantityUnit::Lots)] {
            let profile = build_profile(primary(), not_requested("hq.directory", "profile.security"),
                source(FinanceValue::Shares(quantity), f32::INFINITY), ResultMeta::default()).unwrap();
            assert!(profile.circulating_shares().is_none()); assert!(profile.turnover().is_none());
            assert!(profile.circulating_market_value().is_none()); assert!(profile.eps().is_none());
            assert_eq!(profile.total_market_value().unwrap().value().as_f64(), 240000.0);
            assert_eq!(profile.finance().unwrap().value(FinanceField::CirculatingShares), Some(FinanceValue::Shares(quantity)));
            assert_eq!(profile.finance_status(), EnrichmentStatus::Succeeded);
            assert!(profile.meta().diagnostics().iter().any(|item| item.stage() == "profile.eps"
                && item.status() == EnrichmentStatus::Unavailable));
        }
    }

    #[test]
    fn helper_retains_exact_share_units_and_does_not_substitute_other_finance_fields() {
        let shares = Quantity::shares_from_ten_thousands(1.0, ValueProvenance::WireRule).unwrap();
        let profile = build_profile(primary(), not_requested("hq.directory", "profile.security"),
            source(FinanceValue::Shares(shares), -0.5), ResultMeta::default()).unwrap();
        assert_eq!(profile.circulating_shares().unwrap().source_unit(), QuantityUnit::TenThousandShares);
        assert_eq!(profile.circulating_shares().unwrap().value().as_f64(), 10000.0);
        assert_eq!(profile.turnover().unwrap().unit(), RatioUnit::PercentagePoints);
        assert_eq!(profile.turnover().unwrap().value().as_f64(), 50.0);
        assert_eq!(profile.eps().unwrap().value(), NumericValue::F32(-0.5));
        let missing = build_profile(primary(), not_requested("hq.directory", "profile.security"),
            unavailable("hq.finance", "profile.finance", "missing"), ResultMeta::default()).unwrap();
        assert!(missing.circulating_shares().is_none()); assert!(missing.total_shares().is_none());
        assert!(missing.total_market_value().is_none()); assert!(missing.turnover().is_none());
    }
}
