//! Pure MAC adjustment and board helpers. No owner, implicit HQ metadata or
//! network access. Native wire prices remain available after f64 projection.
use std::collections::HashMap;
use std::sync::Arc;
use crate::{Error, ErrorKind, Result, TradingDate};
use crate::types::{Diagnostic, EnrichmentStatus};
use super::{Bar, BoardSymbol};

/// Per-share cash/rights quantities. Category1 is the only action interpreted
/// here. Raw nonfinite values may be retained; unusable event factors are
/// skipped with explicit diagnostic evidence by the multi-event operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XdxrEvent {
    pub date: TradingDate, pub category: u8, pub fenhong: f64, pub peigujia: f64,
    pub songzhuangu: f64, pub peigu: f64,
}
impl XdxrEvent {
    pub const fn new(date: TradingDate) -> Self {
        Self { date,category: 1,fenhong: 0.0,peigujia: 0.0,songzhuangu: 0.0,peigu: 0.0 }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct AdjustmentEvent {
    event: XdxrEvent, factor: Option<f64>, affected_rows: usize, reason: Option<&'static str>,
}
impl AdjustmentEvent {
    pub const fn event(&self) -> XdxrEvent { self.event }
    pub const fn factor(&self) -> Option<f64> { self.factor }
    pub const fn affected_rows(&self) -> usize { self.affected_rows }
    pub const fn skipped_reason(&self) -> Option<&'static str> { self.reason }
}
/// Detailed pure computation, also used by the network fallback service.
#[derive(Clone, Debug)]
pub struct AdjustmentResult { pub(crate) rows: Vec<Bar>, events: Arc<[AdjustmentEvent]>, source_indices: Vec<usize> }
impl AdjustmentResult {
    pub fn rows(&self) -> &[Bar] { &self.rows }
    pub fn events(&self) -> &[AdjustmentEvent] { &self.events }
    pub(crate) fn shared_events(&self) -> Arc<[AdjustmentEvent]> { Arc::clone(&self.events) }
    /// Each output occurrence's index in the original input slice, including
    /// duplicates. Useful when matching sorted rows to retained raw ranges.
    pub fn source_indices(&self) -> &[usize] { &self.source_indices }
    pub fn into_rows(self) -> Vec<Bar> { self.rows }
}

pub fn convert_board_code(board: &BoardSymbol) -> Result<u32> { Ok(board.wire_code()) }
/// The arithmetic order is shared with the legacy object projection. Input,
/// denominator and result validity are deliberately caller policy.
#[doc(hidden)]
pub fn forward_denominator_with<T,E>(close: &T, stock: &T, rights: &T, one: T,
    mut add: impl FnMut(T,&T) -> std::result::Result<T,E>,
    multiply: impl FnOnce(&T,T) -> std::result::Result<T,E>) -> std::result::Result<T,E>
{
    let stock_total = add(one,stock)?;
    multiply(close,add(stock_total,rights)?)
}
#[doc(hidden)]
pub fn forward_numerator_with<T,E>(close: &T, cash: &T, price: &T, rights: &T,
    subtract: impl FnOnce(&T,&T) -> std::result::Result<T,E>,
    multiply: impl FnOnce(&T,&T) -> std::result::Result<T,E>,
    add: impl FnOnce(T,T) -> std::result::Result<T,E>) -> std::result::Result<T,E>
{
    let difference = subtract(close,cash)?;
    add(difference,multiply(price,rights)?)
}
/// Applies one event in source OHLC order. Strict callers preflight finite
/// outputs; Python intentionally keeps its existing in-place numeric policy.
#[doc(hidden)]
pub fn scale_prices_with<I,T,E>(indices: I,
    mut multiply: impl FnMut(&T,usize) -> std::result::Result<(),E>,
    mut boundary: impl FnMut(usize) -> std::result::Result<(),E>) -> std::result::Result<(),E>
where I: IntoIterator<Item=std::result::Result<T,E>>
{
    for (index,row) in indices.into_iter().enumerate() {
        let row = row?;
        boundary(index)?;
        for slot in 0..4 { multiply(&row,slot)?; }
    }
    Ok(())
}
#[doc(hidden)]
pub fn price_invalid_with<E>(finite: impl FnOnce() -> std::result::Result<bool,E>,
    nonpositive: impl FnOnce() -> std::result::Result<bool,E>) -> std::result::Result<bool,E>
{
    Ok(!finite()? || nonpositive()?)
}
pub fn compute_forward_factor(cum_close: f64, fenhong: f64, peigujia: f64, songzhuangu: f64, peigu: f64) -> Result<f64> {
    if ![cum_close,fenhong,peigujia,songzhuangu,peigu].into_iter().all(f64::is_finite) || cum_close <= 0.0 {
        return Err(invalid("MAC forward adjustment requires finite inputs and a positive prior close"));
    }
    let denominator = forward_denominator_with::<_,std::convert::Infallible>(
        &cum_close,&songzhuangu,&peigu,1.0,|a,b| Ok(a+b),|a,b| Ok(a*b))
        .unwrap_or_else(|never| match never {});
    if !denominator.is_finite() || denominator == 0.0 { return Err(invalid("MAC forward adjustment denominator is zero or nonfinite")); }
    let numerator = forward_numerator_with::<_,std::convert::Infallible>(
        &cum_close,&fenhong,&peigujia,&peigu,|a,b| Ok(a-b),|a,b| Ok(a*b),|a,b| Ok(a+b))
        .unwrap_or_else(|never| match never {});
    let factor = numerator/denominator;
    if !factor.is_finite() { return Err(invalid("MAC forward adjustment factor is nonfinite")); }
    Ok(factor)
}
pub fn has_bad_prices(bars: &[Bar]) -> bool {
    bars.iter().any(|bar| prices(bar).into_iter().any(|value|
        price_invalid_with::<std::convert::Infallible>(|| Ok(value.is_finite()),|| Ok(value <= 0.0))
            .unwrap_or_else(|never| match never {})))
}
pub fn apply_forward_adjust(bars: &[Bar], events: &[XdxrEvent]) -> Result<Vec<Bar>> {
    forward_adjustment(bars,events).map(AdjustmentResult::into_rows)
}
pub fn forward_adjustment(bars: &[Bar], events: &[XdxrEvent]) -> Result<AdjustmentResult> {
    forward_adjustment_with_boundary(bars,events,|| Ok(()))
}
pub(crate) fn forward_adjustment_with_boundary(bars: &[Bar], events: &[XdxrEvent], mut boundary: impl FnMut() -> Result<()>) -> Result<AdjustmentResult> {
    boundary()?;
    let mut rows = Vec::new(); rows.try_reserve(bars.len()).map_err(crate::services::allocation_error)?;
    let mut keys = Vec::new(); keys.try_reserve(bars.len()).map_err(crate::services::allocation_error)?;
    for (index,bar) in bars.iter().enumerate() {
        if index % 256 == 0 { boundary()?; }
        // An input slice is one instrument series. Never use another market's
        // prior close as the reference for an otherwise identical code.
        if bars.first().is_some_and(|first| bar.id() != first.id()) {
            return Err(invalid("MAC local adjustment requires one instrument identity"));
        }
        // End alignment may cross midnight. The corporate action belongs to
        // the source day, never the shifted display date.
        let date = bar.source_time().date().ok_or_else(|| invalid("MAC local adjustment requires known dated bars"))?;
        let clock = bar.source_time().clock().ok_or_else(|| invalid("MAC local adjustment requires known source clocks"))?;
        keys.push((date,clock,index));
    }
    keys.sort_by_key(|&(date,clock,_)| (date,clock)); // stable source tie order
    boundary()?;
    let mut dates = Vec::new(); dates.try_reserve(keys.len()).map_err(crate::services::allocation_error)?;
    let mut values = Vec::new(); values.try_reserve(keys.len()).map_err(crate::services::allocation_error)?;
    for (position,&(date,_,index)) in keys.iter().enumerate() {
        if position % 256 == 0 { boundary()?; }
        rows.push(bars[index].clone()); dates.push(date); values.push(prices(&bars[index]));
    }
    let mut ordered = Vec::new(); ordered.try_reserve(events.len()).map_err(crate::services::allocation_error)?;
    for (index,event) in events.iter().enumerate() {
        if index % 256 == 0 { boundary()?; }
        ordered.push(*event);
    }
    ordered.sort_by_key(|event| event.date); // stable equal-date event order
    let mut applied = Vec::new(); applied.try_reserve(events.len()).map_err(crate::services::allocation_error)?;
    let mut adjusted_prefix = 0;
    for event in ordered {
        boundary()?;
        let affected = dates.partition_point(|date| *date < event.date);
        let mut evidence = AdjustmentEvent { event,factor: None,affected_rows: 0,reason: None };
        if event.category != 1 { evidence.reason = Some("category is not a per-share price action"); }
        else if [event.fenhong,event.peigujia,event.songzhuangu,event.peigu].into_iter().all(|value| value == 0.0) {
            evidence.reason = Some("event has no price adjustment quantities");
        } else if affected == 0 { evidence.reason = Some("no source bar precedes this event"); }
        else {
            match compute_forward_factor(values[affected-1][3],event.fenhong,event.peigujia,event.songzhuangu,event.peigu) {
                Err(_) => evidence.reason = Some("event factor inputs or result are invalid"),
                Ok(factor) => {
                    // Reject overflowing results before publishing any part
                    // of this event. Finite nonpositive results are retained
                    // so the repair service can report StillInvalid honestly.
                    let mut finite = true;
                    for (index,row) in values[..affected].iter().enumerate() {
                        if index % 256 == 0 { boundary()?; }
                        if row.iter().any(|value| !(value*factor).is_finite()) { finite = false; break; }
                    }
                    if !finite { evidence.reason = Some("event would create a nonfinite price"); }
                    else {
                        scale_prices_with((0..affected).map(Ok::<_,Error>),|index,slot| { values[*index][slot] *= factor; Ok(()) },
                            |index| { if index % 256 == 0 { boundary()?; } Ok(()) })?;
                        evidence.factor = Some(factor); evidence.affected_rows = affected;
                        adjusted_prefix = adjusted_prefix.max(affected);
                    }
                }
            }
        }
        applied.push(evidence);
    }
    let applied_count = applied.iter().filter(|item| item.factor.is_some()).count();
    let evidence: Arc<[AdjustmentEvent]> = applied.into();
    let mut metadata = HashMap::<usize,Arc<crate::types::ResultMeta>>::new();
    for (index,(bar,values)) in rows.iter_mut().zip(values).enumerate() {
        if index % 256 == 0 { boundary()?; }
        // A later untouched row keeps Wire provenance. Even an applied factor
        // of exactly one is still a recorded local calculation for its prefix.
        if index < adjusted_prefix { bar.set_adjusted_prices(values); }
        if evidence.is_empty() { continue; }
        bar.set_adjustment_events(Arc::clone(&evidence));
        let original = bar.shared_meta(); let key = Arc::as_ptr(&original) as usize;
        if !metadata.contains_key(&key) { metadata.try_reserve(1).map_err(crate::services::allocation_error)?; }
        let meta = metadata.entry(key).or_insert_with(|| {
            let state = if applied_count > 0 { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable };
            let message = format!("events={}; applied={applied_count}; skipped={}; detailed per-event evidence is shared by result rows",evidence.len(),evidence.len()-applied_count);
            Arc::new(original.as_ref().clone().with_diagnostic(Diagnostic::new("mac","local-forward-adjustment",state,None,Some(message))))
        });
        bar.set_meta(Arc::clone(meta));
    }
    let mut source_indices = Vec::new(); source_indices.try_reserve(keys.len()).map_err(crate::services::allocation_error)?;
    for (position,&(_,_,index)) in keys.iter().enumerate() {
        if position % 256 == 0 { boundary()?; }
        source_indices.push(index);
    }
    boundary()?; Ok(AdjustmentResult { rows,events: evidence,source_indices })
}
fn prices(bar: &Bar) -> [f64;4] {
    [bar.open().value().as_f64(),bar.high().value().as_f64(),bar.low().value().as_f64(),bar.close().value().as_f64()]
}
fn invalid(message: &'static str) -> Error { Error::new(ErrorKind::InvalidArgument,message).with_context("domain","mac") }

// Source-only fixtures; no test execution before all T01-T30 sources are complete.
#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use crate::mac::{BarPriceSource, BarsQuery, MacCode, MacInstrumentId, MacMarketId, MacPeriod, WireFloat};
    use crate::mac::models::BarRecord;
    use crate::types::{BarTime, ResultMeta};

    fn date(day: u8) -> TradingDate { TradingDate::new(2026,11,day).expect("fixture date") }
    fn query() -> BarsQuery {
        BarsQuery::new(MacInstrumentId::new(MacMarketId::new(1),MacCode::new("600000").expect("fixture code")))
    }
    fn bar_with(date_raw: u32, seconds: u32, price: f32, marker: u32, query: &BarsQuery) -> Bar {
        let values = [WireFloat::from_bits(price.to_bits()),WireFloat::from_bits(price.to_bits()),
            WireFloat::from_bits(price.to_bits()),WireFloat::from_bits(price.to_bits()),
            WireFloat::from_bits(marker),WireFloat::from_bits(0x8000_0000),WireFloat::from_bits(0)];
        let mut raw = [0;36]; raw[..4].copy_from_slice(&date_raw.to_le_bytes()); raw[4..8].copy_from_slice(&seconds.to_le_bytes());
        for (index,value) in values.iter().enumerate() { raw[8+index*4..12+index*4].copy_from_slice(&value.bits().to_le_bytes()); }
        Bar::from_record(BarRecord { date_raw,time_seconds: seconds,values,source_bytes: raw },query,Some(Bytes::copy_from_slice(&raw))).expect("fixture bar")
    }
    fn bar(day: u8, price: f32, marker: u32) -> Bar { bar_with(20261100+u32::from(day),0,price,marker,&query()) }
    fn cash(day: u8, amount: f64) -> XdxrEvent { XdxrEvent { fenhong: amount,..XdxrEvent::new(date(day)) } }
    fn near(actual: f64, expected: f64) { assert!((actual-expected).abs() < 1e-12,"actual={actual:?}; expected={expected:?}"); }

    #[test]
    fn shared_object_arithmetic_and_prefix_keep_source_order_and_short_circuit() {
        use std::cell::RefCell;
        let stages = RefCell::new(Vec::new());
        let denominator = forward_denominator_with::<_,()>(
            &10.0,&0.1,&0.2,1.0,
            |a,b| { stages.borrow_mut().push("add"); Ok(a+b) },
            |a,b| { stages.borrow_mut().push("multiply"); Ok(a*b) }).unwrap();
        near(denominator,13.0);
        assert_eq!(*stages.borrow(),["add","add","multiply"]);
        stages.borrow_mut().clear();
        let numerator = forward_numerator_with::<_,()>(
            &10.0,&1.0,&5.0,&0.2,
            |a,b| { stages.borrow_mut().push("subtract"); Ok(a-b) },
            |a,b| { stages.borrow_mut().push("multiply"); Ok(a*b) },
            |a,b| { stages.borrow_mut().push("add"); Ok(a+b) }).unwrap();
        near(numerator,10.0);
        assert_eq!(*stages.borrow(),["subtract","multiply","add"]);
        let mut mutations = Vec::new();
        scale_prices_with([Ok::<_,()>(2),Ok(0)],|row,slot| { mutations.push((*row,slot)); Ok(()) },|_| Ok(())).unwrap();
        assert_eq!(mutations,[(2,0),(2,1),(2,2),(2,3),(0,0),(0,1),(0,2),(0,3)]);
        assert!(price_invalid_with::<()>(|| Ok(false),|| panic!("nonfinite short-circuits comparison")).unwrap());
        assert_eq!(price_invalid_with::<&str>(|| Err("finite hook failure"),|| Ok(false)),Err("finite hook failure"));
    }

    #[test]
    fn factor_uses_per_share_quantities_and_rejects_invalid_inputs() {
        near(compute_forward_factor(10.0,1.0,0.0,0.0,0.0).expect("cash factor"),0.9);
        near(compute_forward_factor(100.0,1.0,5.0,0.1,0.2).expect("rights factor"),100.0/130.0);
        for values in [[0.0,0.0,0.0,0.0,0.0],[-1.0,0.0,0.0,0.0,0.0],
            [1.0,f64::NAN,0.0,0.0,0.0],[1.0,0.0,f64::INFINITY,0.0,0.0],
            [1.0,0.0,0.0,-1.0,0.0],[f64::MAX,0.0,0.0,1.0,0.0]] {
            assert_eq!(compute_forward_factor(values[0],values[1],values[2],values[3],values[4]).expect_err("invalid factor").kind(),ErrorKind::InvalidArgument);
        }
        near(compute_forward_factor(10.0,20.0,0.0,0.0,0.0).expect("finite negative remains visible"),-1.0);
    }

    #[test]
    fn sequential_equal_date_events_and_tied_source_rows_are_stable() {
        let mut input = vec![bar(3,30.0,3),bar(1,8.0,1),bar(2,20.0,2),bar(1,10.0,4)];
        let original_meta = Arc::new(ResultMeta::default());
        for row in &mut input { row.set_meta(Arc::clone(&original_meta)); }
        let stock = XdxrEvent { songzhuangu: 1.0,..cash(2,1.0) };
        let result = forward_adjustment(&input,&[cash(3,1.0),stock,cash(2,1.0)]).expect("ordered local adjustment");
        let rows = result.rows();
        assert_eq!(rows.iter().map(|row| row.amount().value().bits()).collect::<Vec<_>>(),[1,4,2,3]);
        for (row,expected) in rows.iter().zip([2.66,3.325,19.0,30.0]) { near(row.close().value().as_f64(),expected); }
        assert_eq!(result.events().iter().map(AdjustmentEvent::affected_rows).collect::<Vec<_>>(),[2,2,3]);
        near(result.events()[0].factor().expect("first factor"),0.45);
        near(result.events()[1].factor().expect("accumulated prior close"),3.5/4.5);
        assert_eq!(rows[0].close().value().bits(),8.0_f32.to_bits());
        assert_eq!(rows[1].close().value().wire().as_f32(),10.0);
        assert_eq!(rows[0].close().value().source(),BarPriceSource::LocalForwardAdjustment);
        assert_eq!(rows[3].close().value().source(),BarPriceSource::Wire);
        assert!(Arc::ptr_eq(&rows[0].shared_meta(),&rows[3].shared_meta()));
        assert_eq!(rows[0].adjustment_events().as_ptr(),rows[3].adjustment_events().as_ptr());
        assert_eq!(rows[0].adjustment_events().as_ptr(),result.events().as_ptr());
        assert_eq!(input[1].close().value().source(),BarPriceSource::Wire);
        near(input[1].close().value().as_f64(),8.0);
        assert_eq!(rows[0].record_raw(),input[1].record_raw());
        assert_eq!(rows[0].wire_values(),input[1].wire_values());
        assert_eq!(rows[0].volume().value().bits(),0x8000_0000);
        assert_eq!(rows[0].float_shares().expect("provided zero").value().bits(),0);
    }

    #[test]
    fn f64_effective_price_does_not_round_back_to_source_f32() {
        let input = vec![bar(1,10.0,0x7fc0_1234),bar(2,20.0,0)];
        let rows = apply_forward_adjust(&input,&[cash(2,0.0000001)]).expect("small dividend");
        let price = *rows[0].close().value();
        near(price.as_f64(),9.9999999);
        assert_ne!(price.as_f64(),10.0);
        assert_eq!(price.bits(),10.0_f32.to_bits());
        assert_eq!(price.adjusted_value(),Some(price.as_f64()));
        assert_eq!(rows[0].amount().value().bits(),0x7fc0_1234);
        assert_eq!(rows[0].record_raw(),input[0].record_raw());
        assert!(!has_bad_prices(&rows)); // An unrelated amount NaN is not OHLC.
    }

    #[test]
    fn end_alignment_rollover_uses_the_unshifted_source_day() {
        let query = query().with_period(MacPeriod::Minute5).expect("intraday").with_bar_time(BarTime::End).expect("end alignment");
        let input = [bar_with(20261101,86340,10.0,1,&query),bar_with(20261102,0,20.0,2,&query)];
        assert_eq!(input[0].time().date(),Some(date(2)));
        assert_eq!(input[0].source_time().date(),Some(date(1)));
        let result = forward_adjustment(&input,&[cash(2,1.0)]).expect("source-day adjustment");
        near(result.rows()[0].close().value().as_f64(),9.0);
        near(result.rows()[1].close().value().as_f64(),20.0);
        assert_eq!(result.events()[0].affected_rows(),1);
        assert_eq!(result.rows()[0].time(),input[0].time());
    }

    #[test]
    fn skipped_events_keep_evidence_without_claiming_adjusted_prices() {
        let input = [bar(1,10.0,1),bar(2,20.0,2)];
        let events = [XdxrEvent { category: 2,..cash(2,1.0) },XdxrEvent::new(date(2)),cash(1,1.0),
            cash(2,f64::NAN),XdxrEvent { songzhuangu: -1.0,..cash(2,1.0) }];
        let result = forward_adjustment(&input,&events).expect("invalid events are explicit skips");
        assert_eq!(result.events().len(),5);
        assert!(result.events().iter().all(|event| event.factor().is_none() && event.skipped_reason().is_some()));
        for (original,row) in input.iter().zip(result.rows()) {
            assert_eq!(row.close().value().source(),BarPriceSource::Wire);
            assert_eq!(row.close().value().bits(),original.close().value().bits());
            assert_eq!(row.adjustment_events().len(),5);
        }
    }

    #[test]
    fn nonfinite_prefix_prevents_partial_event_mutation_and_nonpositive_prices_remain_detectable() {
        let input = [bar(1,f32::MAX,1),bar(2,1.0,2),bar(3,20.0,3)];
        let event = XdxrEvent { peigujia: f64::MAX,peigu: 1.0,..XdxrEvent::new(date(3)) };
        let result = forward_adjustment(&input,&[event]).expect("finite factor with overflowing prefix");
        assert!(result.events()[0].factor().is_none());
        assert_eq!(result.events()[0].skipped_reason(),Some("event would create a nonfinite price"));
        for (original,row) in input.iter().zip(result.rows()) {
            assert_eq!(row.close().value().as_f64(),original.close().value().as_f64());
            assert_eq!(row.close().value().source(),BarPriceSource::Wire);
        }
        let negative = apply_forward_adjust(&[bar(1,10.0,0),bar(2,20.0,0)],&[cash(2,20.0)]).expect("finite negative result");
        assert!(has_bad_prices(&negative));
        for value in [-0.0_f32,0.0,f32::NAN,f32::INFINITY,-1.0] { assert!(has_bad_prices(&[bar(1,value,0)])); }
        assert!(!has_bad_prices(&[]));
    }

    #[test]
    fn pure_adjustment_rejects_mixed_identity_and_unknown_source_dates() {
        let different = query().with_id(MacInstrumentId::new(MacMarketId::new(0),MacCode::new("600000").expect("same code other market")));
        let mixed = [bar(1,10.0,0),bar_with(20261102,0,20.0,0,&different)];
        assert_eq!(forward_adjustment(&mixed,&[]).expect_err("mixed market identities").kind(),ErrorKind::InvalidArgument);
        let unknown = query().with_period(MacPeriod::Wire(12)).expect("explicit unknown period");
        let unknown = bar_with(0,0,10.0,0,&unknown);
        assert_eq!(forward_adjustment(&[unknown],&[cash(2,1.0)]).expect_err("unknown dated series").kind(),ErrorKind::InvalidArgument);
        let empty = forward_adjustment(&[],&[cash(2,1.0)]).expect("empty series");
        assert!(empty.rows().is_empty());
        assert_eq!(empty.events()[0].skipped_reason(),Some("no source bar precedes this event"));
    }

    #[test]
    fn parent_boundary_cancellation_propagates_inside_long_series_without_publishing_partial_rows() {
        let input = vec![bar(1,10.0,0);512];
        let mut calls = 0;
        let result = forward_adjustment_with_boundary(&input,&[cash(2,1.0)],|| {
            calls += 1;
            if calls >= 10 { Err(Error::new(ErrorKind::Cancelled,"fixture parent cancellation")) } else { Ok(()) }
        });
        assert_eq!(result.expect_err("cancellation is not skipped event evidence").kind(),ErrorKind::Cancelled);
        assert!(calls >= 10);
        assert!(input.iter().all(|row| row.close().value().source() == BarPriceSource::Wire));
    }
}
