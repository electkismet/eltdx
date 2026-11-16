use crate::{Error, ErrorKind, Result};
use crate::types::{AdjustmentFactor, AdjustmentFactors, CapitalChangeBlock, CapitalEvent, FactorQuery};

/// Compute the frozen source-ordered affine composition. No price rounding is
/// performed here; offsets carry price units and cannot be discarded.
pub fn build_adjustment_factors(changes: &CapitalChangeBlock, query: &FactorQuery) -> Result<AdjustmentFactors> {
    build_with_boundary(changes, query, || Ok(()))
}

pub(crate) fn build_with_boundary(changes: &CapitalChangeBlock, query: &FactorQuery,
    mut boundary: impl FnMut() -> Result<()>) -> Result<AdjustmentFactors> {
    boundary()?;
    query.validate()?;
    let mut events = Vec::new(); events.try_reserve(changes.records().len()).map_err(crate::services::allocation_error)?;
    for event in changes.records() {
        boundary()?;
        if event.category() == 1 && event.date().is_some_and(|date| query.start_date().is_none_or(|start| date >= start)) {
            events.push((event.date().expect("filtered dated event"), coefficients(event)?));
        }
    }
    let mut dates = Vec::new(); dates.try_reserve(events.len()).map_err(crate::services::allocation_error)?;
    let mut composed = Vec::new(); composed.try_reserve(events.len()).map_err(crate::services::allocation_error)?;
    compose_affine(events, query.anchor_date(), |value| Ok(*value), finite, &mut boundary, &mut dates, &mut composed)?;
    let mut rows = Vec::new(); rows.try_reserve(composed.len()).map_err(crate::services::allocation_error)?;
    rows.extend(composed.into_iter().map(|(date, [q_scale, q_offset, h_scale, h_offset])|
        AdjustmentFactor::new(date, q_scale, q_offset, h_scale, h_offset)));
    Ok(AdjustmentFactors::new(changes.id().clone(), query, rows, changes.meta().clone()))
}

/// One source-stable composition for both public Rust and Python-compatible
/// factors. Coefficient and result policies remain explicit at the boundary.
fn compose_affine<D: Copy + Ord, T, E>(mut events: Vec<(D, T)>, anchor: Option<D>,
    coefficient: impl Fn(&T) -> std::result::Result<(f64, f64), E>,
    validate: impl Fn(f64, f64) -> std::result::Result<(), E>,
    boundary: &mut impl FnMut() -> std::result::Result<(), E>, dates: &mut Vec<D>,
    rows: &mut Vec<(D, [f64; 4])>) -> std::result::Result<(), E> {
    // Stable sorting preserves the original order within each event date.
    events.sort_by_key(|(date, _)| *date);
    for (date, _) in &events { if dates.last() != Some(date) { dates.push(*date); } }
    for &date in dates.iter() {
        boundary()?;
        let (mut q_scale, mut q_offset) = (1.0, 0.0);
        for (at, event) in &events {
            boundary()?;
            if *at >= date && anchor.is_none_or(|anchor| *at <= anchor) {
                let (multiplier, offset) = coefficient(event)?;
                q_scale /= multiplier; q_offset = (q_offset - offset) / multiplier;
                validate(q_scale, q_offset)?;
            }
        }
        let (mut h_scale, mut h_offset) = (1.0, 0.0);
        // Traverse date groups backwards, while keeping each group forwards.
        let mut end = events.len();
        while end != 0 {
            boundary()?;
            let at = events[end - 1].0;
            let mut begin = end - 1;
            while begin != 0 && events[begin - 1].0 == at { begin -= 1; }
            if at <= date {
                for (_, event) in &events[begin..end] {
                    boundary()?;
                    let (multiplier, offset) = coefficient(event)?;
                    h_scale *= multiplier; h_offset = multiplier * h_offset + offset;
                    validate(h_scale, h_offset)?;
                }
            }
            end = begin;
        }
        rows.push((date, [q_scale, q_offset, h_scale, h_offset]));
    }
    Ok(())
}

/// Dates are already normalized and events filtered by the Python facade.
/// Legacy arithmetic deliberately allows NaN, infinity and underflow, and only
/// rejects a non-positive multiplier at the same selected-event boundary.
pub fn legacy_adjustment_factors<E>(events: Vec<(u32, [f64; 4])>, anchor: Option<u32>,
    invalid: impl Fn() -> E, mut boundary: impl FnMut() -> std::result::Result<(), E>)
    -> std::result::Result<Vec<(u32, [f64; 4])>, E> {
    let mut dates = Vec::new();
    let mut rows = Vec::new();
    compose_affine(events, anchor, |values| {
        let (multiplier, offset) = affine_coefficients(*values);
        if multiplier <= 0.0 { Err(invalid()) } else { Ok((multiplier, offset)) }
    }, |_, _| Ok(()), &mut boundary, &mut dates, &mut rows)?;
    Ok(rows)
}

fn affine_coefficients([cash, rights_price, bonus, rights]: [f64; 4]) -> (f64, f64) {
    ((10.0 + bonus + rights) / 10.0, (cash - rights * rights_price) / 10.0)
}

pub(crate) fn coefficients(event: &CapitalEvent) -> Result<(f64, f64)> {
    let [cash, rights_price, bonus, rights] = *event.coefficients();
    if ![cash, rights_price, bonus, rights].into_iter().all(f64::is_finite) { return Err(invalid("non-finite adjustment coefficient")); }
    let (multiplier, offset) = affine_coefficients([cash, rights_price, bonus, rights]);
    if !multiplier.is_finite() || multiplier <= 0.0 || !offset.is_finite() { return Err(invalid("invalid affine event multiplier or offset")); }
    Ok((multiplier, offset))
}
fn finite(scale: f64, offset: f64) -> Result<()> {
    if scale.is_finite() && scale > 0.0 && offset.is_finite() { Ok(()) }
    else { Err(invalid("adjustment composition overflowed or underflowed")) }
}
fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "adjustment") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_same_day_source_order_retains_affine_offset() {
        let result = legacy_adjustment_factors(vec![(1, [0.0, 25.0, 0.0, 2.0]),
            (1, [1.0, 0.0, 1.0, 0.0])], None, || "invalid", || Ok(())).unwrap();
        assert_eq!(result.len(), 1);
        assert!((result[0].1[3] - -5.4).abs() < 1e-12);
    }
    #[test]
    fn legacy_nonfinite_values_are_not_rejected_by_strict_policy() {
        let result = legacy_adjustment_factors(vec![(1, [f64::NAN, 0.0, 0.0, 0.0])],
            None, || "invalid", || Ok(())).unwrap();
        assert!(result[0].1[1].is_nan());
        assert!(result[0].1[3].is_nan());
    }
    #[test]
    fn legacy_composition_still_obeys_cancellation_boundary() {
        let mut calls = 0;
        let result = legacy_adjustment_factors(vec![(1, [0.0; 4])], None, || "invalid", || {
            calls += 1;
            if calls == 2 { Err("cancelled") } else { Ok(()) }
        });
        assert_eq!(result.err(), Some("cancelled"));
    }
    #[test]
    fn network_factor_composition_checks_the_shared_operation_boundary() {
        let event = CapitalEvent::from_wire(20260102, 1, [1_f32.to_le_bytes(); 4], 0);
        let block = CapitalChangeBlock::new("sh600000".parse().unwrap(), vec![event], 1, crate::types::ResultMeta::default());
        let mut calls = 0;
        let error = build_with_boundary(&block, &FactorQuery::new(), || {
            calls += 1;
            if calls >= 4 { Err(Error::new(ErrorKind::Cancelled, "synthetic composition cancellation")) } else { Ok(()) }
        }).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Cancelled); assert_eq!(calls, 4);
    }
}
