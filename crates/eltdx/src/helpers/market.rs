use std::collections::HashSet;
use crate::{Error, ErrorKind, Result};
use crate::types::{Amount, AmountUnit, FundFlow, NumericValue, QuantityUnit, ResultMeta, SourceStamp,
    TradeEventKind, TradeSide, TradeTick, ValueProvenance};

/// Classify actual Buy/Sell trades using strict source thresholds. Equal
/// boundaries belong to the smaller bucket. Auction snapshots and other sides
/// are skipped before inspecting non-trade quote fields. This function is pure.
pub fn classify_fund_flow(ticks: &[TradeTick]) -> Result<FundFlow> {
    classify_with_boundary(ticks, || Ok(()))
}

pub(crate) fn classify_with_boundary(ticks: &[TradeTick], mut boundary: impl FnMut() -> Result<()>) -> Result<FundFlow> {
    boundary()?;
    let mut buckets = [0.0_f64; 8]; let mut classified_count = 0_usize; let mut skipped_count = 0_usize;
    let mut seen = HashSet::new(); let mut sources = Vec::new();
    for (index, tick) in ticks.iter().enumerate() {
        if index % 1024 == 0 { boundary()?; }
        if !seen.contains(tick.source()) {
            seen.try_reserve(1).map_err(allocation)?; sources.try_reserve(1).map_err(allocation)?;
            seen.insert(tick.source().clone()); sources.push(tick.source().clone());
        }
        let direction = if tick.kind() == TradeEventKind::AuctionSnapshot { None }
            else { match tick.side() { TradeSide::Buy => Some(0), TradeSide::Sell => Some(4), _ => None } };
        let Some(direction) = direction else { skipped_count = increment(skipped_count)?; continue; };
        if tick.volume().unit() != QuantityUnit::Lots {
            return Err(invalid("fund flow requires trade volume in hands/lots").with_context("tick_index", format!("{index}")));
        }
        let price = tick.price().value().as_f64(); let volume = tick.volume().value().as_f64();
        let amount = price * volume * 100.0;
        if !price.is_finite() || !volume.is_finite() || price < 0.0 || volume < 0.0 || !amount.is_finite() {
            return Err(invalid("fund flow requires finite nonnegative trade prices, volumes and CNY amounts")
                .with_context("tick_index", format!("{index}")));
        }
        let bucket = if amount > 1_000_000.0 { 0 } else if amount > 200_000.0 { 1 }
            else if amount > 40_000.0 { 2 } else { 3 };
        buckets[direction + bucket] = finite(buckets[direction + bucket] + amount)?;
        classified_count = increment(classified_count)?;
    }
    let main_in = finite(buckets[0] + buckets[1])?; let main_out = finite(buckets[4] + buckets[5])?;
    let sum = |values: &[f64]| values.iter().try_fold(0.0, |total, value| finite(total + value));
    let main_net = finite(main_in - main_out)?;
    let total_net = finite(sum(&buckets[..4])? - sum(&buckets[4..])?)?;
    let cny = |value| Amount::with_provenance(NumericValue::F64(value), AmountUnit::Cny, ValueProvenance::WireRule);
    boundary()?;
    Ok(FundFlow { buckets: buckets.map(cny), main_net: cny(main_net), total_net: cny(total_net), classified_count, skipped_count,
        meta: ResultMeta::new(SourceStamp::new(None, None, "pure:fund_flow", None, None), Vec::new(), None).with_sources(sources) })
}
fn finite(value: f64) -> Result<f64> { if value.is_finite() { Ok(value) } else { Err(invalid("fund flow accumulation exceeds finite CNY range")) } }
fn increment(value: usize) -> Result<usize> { value.checked_add(1).ok_or_else(|| invalid("fund flow tick count overflows")) }
fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message) }
fn allocation(error: std::collections::TryReserveError) -> Error { Error::new(ErrorKind::LimitExceeded, "unable to reserve fund flow source metadata").with_source(error) }

#[cfg(test)]
mod source_cases {
    use super::*;
    use crate::types::{EventTime, Price, Quantity};
    fn tick(amount: f64, side: TradeSide, kind: TradeEventKind) -> TradeTick {
        TradeTick { index: 0, absolute_index: 0, time: EventTime::time_of_day(15, 10, 0).unwrap(),
            price: Price::from_f64(amount / 100.0), volume: Quantity::new(NumericValue::F64(1.0), QuantityUnit::Lots),
            order_count: 1, status_raw: 0, side, kind, auction_matched: None, auction_unmatched_signed: None,
            unknown_tail: None, price_delta_raw: 0, price_acc_raw: 0, price_denominator: 100, reserved_zero: None,
            source: SourceStamp::new(Some(0x0fc5), None, "synthetic", None, None) }
    }
    #[test]
    fn exact_thresholds_opening_matches_and_postmarket_trades_follow_the_source_contract() {
        let ticks = [tick(40_000.0, TradeSide::Buy, TradeEventKind::Trade),
            tick(200_000.0, TradeSide::Buy, TradeEventKind::Trade), tick(1_000_000.0, TradeSide::Buy, TradeEventKind::Trade),
            tick(1_000_001.0, TradeSide::Buy, TradeEventKind::OpeningMatch), tick(2_000_000.0, TradeSide::Buy, TradeEventKind::AuctionSnapshot),
            tick(10.0, TradeSide::Neutral, TradeEventKind::Trade)];
        let flow = classify_fund_flow(&ticks).unwrap();
        assert_eq!(flow.small_in().value().as_f64(), 40_000.0); assert_eq!(flow.medium_in().value().as_f64(), 200_000.0);
        assert_eq!(flow.large_in().value().as_f64(), 1_000_000.0); assert_eq!(flow.super_in().value().as_f64(), 1_000_001.0);
        assert_eq!(flow.total_net().value().as_f64(), 2_440_001.0); assert_eq!(flow.main_net().value().as_f64(), 2_000_001.0);
        assert_eq!(flow.classified_count(), 4); assert_eq!(flow.skipped_count(), 2); assert_eq!(flow.meta().sources().len(), 1);
    }
    #[test]
    fn sell_buckets_subtract_and_empty_input_produces_zero_cny() {
        let empty = classify_fund_flow(&[]).unwrap(); assert_eq!(empty.total_net().value().as_f64(), 0.0);
        assert_eq!(empty.classified_count(), 0); assert_eq!(empty.super_out().unit(), AmountUnit::Cny);
        let flow = classify_fund_flow(&[tick(40_001.0, TradeSide::Sell, TradeEventKind::OpeningMatch),
            tick(5.0, TradeSide::Buy, TradeEventKind::Trade), tick(1_000_001.0, TradeSide::Sell, TradeEventKind::Trade)]).unwrap();
        assert_eq!(flow.medium_out().value().as_f64(), 40_001.0); assert_eq!(flow.total_net().value().as_f64(), -1_039_997.0);
        assert_eq!(flow.main_net().value().as_f64(), -1_000_001.0);
    }
    #[test]
    fn unknown_units_negative_nonfinite_and_accumulation_overflow_are_not_silently_bucketed() {
        for value in [-1.0, f64::INFINITY, f64::NAN] {
            assert_eq!(classify_fund_flow(&[tick(value, TradeSide::Buy, TradeEventKind::Trade)]).unwrap_err().kind(), ErrorKind::InvalidArgument);
        }
        let mut unknown = tick(1.0, TradeSide::Buy, TradeEventKind::Trade);
        unknown.volume = Quantity::new(NumericValue::F64(1.0), QuantityUnit::Unknown);
        assert!(classify_fund_flow(&[unknown]).is_err());
        let overflow = tick(1e308, TradeSide::Buy, TradeEventKind::Trade);
        assert!(classify_fund_flow(&[overflow.clone(), overflow]).is_err());
    }
    #[test]
    fn nontrade_quote_fields_are_skipped_and_boundary_error_never_returns_partial_totals() {
        let mut skipped = tick(f64::NAN, TradeSide::Buy, TradeEventKind::AuctionSnapshot);
        skipped.volume = Quantity::new(NumericValue::F64(-1.0), QuantityUnit::Unknown);
        let result = classify_fund_flow(&[skipped]).unwrap(); assert_eq!(result.skipped_count(), 1);
        let ticks = vec![tick(1.0, TradeSide::Buy, TradeEventKind::Trade); 2048]; let mut calls = 0;
        assert_eq!(classify_with_boundary(&ticks, || { calls += 1; if calls == 3 { Err(Error::new(ErrorKind::Cancelled, "cancelled")) } else { Ok(()) } })
            .unwrap_err().kind(), ErrorKind::Cancelled);
    }
}
