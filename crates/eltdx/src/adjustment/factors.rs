use crate::{Error, ErrorKind, Result};
use crate::types::{AdjustmentFactor, AdjustmentFactors, CapitalChangeBlock, CapitalEvent, FactorQuery};

/// Compute the frozen source-ordered affine composition. No price rounding is
/// performed here; offsets carry price units and cannot be discarded.
pub fn build_adjustment_factors(changes: &CapitalChangeBlock, query: &FactorQuery) -> Result<AdjustmentFactors> {
    query.validate()?;
    let mut events = Vec::new(); events.try_reserve(changes.records().len()).map_err(crate::services::allocation_error)?;
    for event in changes.records() {
        if event.category() == 1 && event.date().is_some_and(|date| query.start_date().is_none_or(|start| date >= start)) {
            events.push((event, coefficients(event)?));
        }
    }
    // Stable sorting preserves the original order within each event date.
    events.sort_by_key(|(event, _)| event.date());
    let mut dates = Vec::new(); dates.try_reserve(events.len()).map_err(crate::services::allocation_error)?;
    for (event, _) in &events { let date = event.date().expect("filtered dated event"); if dates.last() != Some(&date) { dates.push(date); } }
    let mut rows = Vec::new(); rows.try_reserve(dates.len()).map_err(crate::services::allocation_error)?;
    for date in dates {
        let (mut q_scale, mut q_offset) = (1.0, 0.0);
        for (event, (multiplier, offset)) in &events {
            let at = event.date().expect("filtered dated event");
            if at >= date && query.anchor_date().is_none_or(|anchor| at <= anchor) {
                q_scale /= multiplier; q_offset = (q_offset - offset) / multiplier;
                finite(q_scale, q_offset)?;
            }
        }
        let (mut h_scale, mut h_offset) = (1.0, 0.0);
        // Traverse date groups backwards, while keeping each group forwards.
        let mut end = events.len();
        while end != 0 {
            let at = events[end - 1].0.date().expect("filtered dated event");
            let mut begin = end - 1;
            while begin != 0 && events[begin - 1].0.date() == Some(at) { begin -= 1; }
            if at <= date {
                for (_, (multiplier, offset)) in &events[begin..end] {
                    h_scale *= multiplier; h_offset = multiplier * h_offset + offset;
                    finite(h_scale, h_offset)?;
                }
            }
            end = begin;
        }
        rows.push(AdjustmentFactor::new(date, q_scale, q_offset, h_scale, h_offset));
    }
    Ok(AdjustmentFactors::new(changes.id().clone(), query, rows, changes.meta().clone()))
}

fn coefficients(event: &CapitalEvent) -> Result<(f64, f64)> {
    let [cash, rights_price, bonus, rights] = *event.coefficients();
    if ![cash, rights_price, bonus, rights].into_iter().all(f64::is_finite) { return Err(invalid("non-finite adjustment coefficient")); }
    let multiplier = (10.0 + bonus + rights) / 10.0;
    let offset = (cash - rights * rights_price) / 10.0;
    if !multiplier.is_finite() || multiplier <= 0.0 || !offset.is_finite() { return Err(invalid("invalid affine event multiplier or offset")); }
    Ok((multiplier, offset))
}
fn finite(scale: f64, offset: f64) -> Result<()> {
    if scale.is_finite() && scale > 0.0 && offset.is_finite() { Ok(()) }
    else { Err(invalid("adjustment composition overflowed or underflowed")) }
}
fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "adjustment") }
