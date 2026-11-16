use std::collections::HashMap;
use std::time::Instant;

use crate::operation::{OperationOptions, WorkLimits};
use crate::services::{allocation_error, checked_ids, TradeService};
use crate::types::{Amount, AmountUnit, BatchItemState, Diagnostic, EndEvidence,
    EnrichmentStatus, FundFlow, LegacyQuote, MarketStat, NumericValue, Price, QuantityUnit,
    ResultMeta, RoundingMode, SecurityId, TradeQuery, ValueProvenance};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::market::classify_with_boundary;
use super::service::HelperService;

const BREADTH_IDS: [&str; 3] = ["sh880005", "sh880001", "sh880006"];

impl HelperService {
    /// Associate the three required pseudo-indices by identity. The legacy
    /// Python positional projection is a separate compatibility contract.
    pub fn market_stat(&self, options: &OperationOptions) -> Result<MarketStat> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "helpers_market_stat", &self.config, options)?;
        let ids = BREADTH_IDS.iter().map(|code| code.parse()).collect::<Result<Vec<SecurityId>>>()?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(ids.len()), bytes: None })?;
        let batch = self.quotes().legacy_sparse_in_context(&context, &ids)?;
        let meta = batch.meta().clone();
        let mut quotes = Vec::new(); quotes.try_reserve(ids.len()).map_err(allocation_error)?;
        for item in batch.into_items() {
            let (_, id, state) = item.into_parts();
            match state {
                BatchItemState::Present(quote) => quotes.push(quote),
                BatchItemState::ConfirmedMissing(_) => return Err(missing(&id)),
            }
        }
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let result = project_market_stat(&quotes, meta)?;
        context.finish()?;
        Ok(result)
    }

    /// Full-day collection must prove EOF before any amounts are returned.
    /// Every page, classification pass and pin release shares this context.
    pub fn today_fund_flow(&self, id: &SecurityId, options: &OperationOptions) -> Result<FundFlow> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "helpers_today_fund_flow", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        let query = TradeQuery::new();
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(query.pages().max_pages().saturating_sub(1)),
            records: Some(query.pages().max_records()), bytes: None })?;
        let service = TradeService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let page = service.collect_in_context(&context, id, &query)?;
        if page.ticks().end() != EndEvidence::EmptyPage {
            return Err(Error::new(ErrorKind::Protocol, "fund flow requires complete trade EOF"));
        }
        let mut flow = classify_with_boundary(page.ticks().items(), || context.check_boundary(TimeoutPhase::Aggregate))?;
        // Preserve empty EOF pages, original page payloads and trading date;
        // pure classification metadata alone would lose those source facts.
        flow.meta = page.meta().clone();
        context.finish()?;
        Ok(flow)
    }
}

fn project_market_stat(quotes: &[LegacyQuote], mut meta: ResultMeta) -> Result<MarketStat> {
    let mut by_id = HashMap::new(); by_id.try_reserve(quotes.len()).map_err(allocation_error)?;
    for quote in quotes {
        let id = quote.snapshot().id();
        if !BREADTH_IDS.contains(&id.to_string().as_str()) || by_id.insert(id.clone(), quote).is_some() {
            return Err(protocol("market statistic contains a foreign or duplicate identity"));
        }
    }
    let required = |code: &str| -> Result<&LegacyQuote> {
        let id: SecurityId = code.parse()?;
        by_id.get(&id).copied().ok_or_else(|| missing(&id))
    };
    let breadth = required(BREADTH_IDS[0])?.snapshot();
    let cap = required(BREADTH_IDS[1])?.snapshot();
    let limits = required(BREADTH_IDS[2])?.snapshot();
    let up = counter(breadth.last())?; let down = counter(breadth.open())?;
    let neutral = counter(breadth.low())?; let total = counter(breadth.high())?;
    let classified = up.checked_add(down).and_then(|count| count.checked_add(neutral))
        .ok_or_else(|| protocol("market breadth count sum overflows"))?;
    let suspended_or_unclassified = total.saturating_sub(classified);
    let amount = breadth.amount(); let volume = breadth.total();
    let capitalization = super::calculation::product_with::<_, std::convert::Infallible>(cap.last().as_f64(), 1e10,
        |price, scale| Ok(price * scale)).unwrap_or_else(|never| match never {});
    if !amount.value().is_finite() || amount.value().as_f64() < 0.0
        || volume.unit() != QuantityUnit::Lots || !volume.value().is_finite() || volume.value().as_f64() < 0.0
        || !capitalization.is_finite() || capitalization < 0.0 {
        return Err(protocol("market statistic has invalid amount, volume or capitalization"));
    }
    // Only this documented pseudo-index amount has a CNY interpretation here;
    // the general legacy quotation amount keeps its original unknown unit.
    let total_amount = Amount::with_provenance(amount.value(), AmountUnit::Cny, ValueProvenance::WireRule);
    let market_cap = Amount::with_provenance(NumericValue::F64(capitalization), AmountUnit::Cny, ValueProvenance::WireRule);
    meta = meta.with_diagnostic(Diagnostic::new("hq.legacy:880005", "market_stat.amount_unit",
        EnrichmentStatus::Succeeded, None, Some("CNY follows the field-specific market-stat contract; no global amount conversion".to_owned())));
    Ok(MarketStat { up, down, neutral, suspended_or_unclassified, total,
        limit_up: counter(limits.last())?, limit_down: counter(limits.open())?,
        total_amount, total_volume: volume, market_cap, meta })
}

fn counter(price: Price) -> Result<u64> {
    if !price.as_f64().is_finite() || price.as_f64() < 0.0 { return Err(protocol("market statistic count is negative or nonfinite")); }
    // Decimal integer rounding avoids an extra binary-float tie. This is the
    // frozen round(value*10), nearest-even contract, not truncation.
    let value = price.to_scaled_integer(1, RoundingMode::HalfToEven)
        .map_err(|error| protocol("market statistic count exceeds its supported integer range").with_source(error))?;
    u64::try_from(value).map_err(|error| protocol("market statistic count is negative").with_source(error))
}
fn missing(id: &SecurityId) -> Error { Error::new(ErrorKind::MissingData, "required market statistic pseudo-index is missing")
    .with_context("security", id.to_string()).with_context("command", "0x053e") }
fn protocol(message: &str) -> Error { Error::new(ErrorKind::Protocol, message) }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{EventTime, Quantity, Snapshot};

    fn quote(code: &str, last: i64, open: i64, high: i64, low: i64) -> LegacyQuote {
        let price = |value| Price::scaled_integer(value, 2);
        let quantity = Quantity::new(NumericValue::ScaledInteger { value: 23, scale: 0 }, QuantityUnit::Lots);
        LegacyQuote::new(Snapshot::new(code.parse().unwrap(), 0, price(last), price(0), price(open), price(high), price(low),
            EventTime::unknown_raw(0), 0, quantity, quantity, quantity, quantity,
            Amount::new(NumericValue::F64(123.0), AmountUnit::Unknown), Amount::new(NumericValue::F64(0.0), AmountUnit::Cny),
            Vec::new(), Vec::new(), ResultMeta::default()), 0, None, None, [0; 4])
    }

    #[test]
    fn identity_join_and_documented_multipliers_do_not_use_response_position() {
        let quotes = [quote("sh880006", 300, 400, 0, 0), quote("sh880005", 1000, 2000, 4000, 500),
            quote("sh880001", 200, 0, 0, 0)];
        let stats = project_market_stat(&quotes, ResultMeta::default()).unwrap();
        assert_eq!((stats.up(), stats.down(), stats.neutral(), stats.total()), (100, 200, 50, 400));
        assert_eq!(stats.suspended_or_unclassified(), 50); assert_eq!((stats.limit_up(), stats.limit_down()), (30, 40));
        assert_eq!(stats.market_cap().value().as_f64(), 2e10); assert_eq!(stats.total_amount().unit(), AmountUnit::Cny);
        assert_eq!(stats.total_amount().value().as_f64(), 123.0); assert_eq!(stats.total_volume().value().as_f64(), 23.0);
    }

    #[test]
    fn missing_pseudo_index_differs_from_invalid_identity_or_count() {
        let breadth = quote("sh880005", 100, 100, 100, 100);
        assert_eq!(project_market_stat(&[breadth.clone(), quote("sh880006", 0, 0, 0, 0)], ResultMeta::default())
            .unwrap_err().kind(), ErrorKind::MissingData);
        assert_eq!(project_market_stat(&[breadth.clone(), breadth], ResultMeta::default()).unwrap_err().kind(), ErrorKind::Protocol);
        for value in [f64::NAN, f64::INFINITY, -0.01, f64::MAX] {
            assert_eq!(counter(Price::from_f64(value)).unwrap_err().kind(), ErrorKind::Protocol);
        }
        assert_eq!(counter(Price::scaled_integer(105, 2)).unwrap(), 10);
        assert_eq!(counter(Price::scaled_integer(115, 2)).unwrap(), 12);
    }
}
