//! Versioned limit-price rule application. The production registry is empty:
//! no dated authoritative exchange profile was approved in T03.
use crate::types::{LimitDecision, LimitRule, LimitStatus, NumericValue, Ratio, RatioUnit, RoundingMode, ValueProvenance};
use crate::{Error, ErrorKind, HqMarket, Price, Result, SecurityId, TradingDate};

#[derive(Debug)]
struct Profile {
    id: &'static str, from: TradingDate, to: Option<TradingDate>, market: HqMarket,
    exact_security: &'static str, ratio: Option<f64>, tick_scale: u32, rounding: RoundingMode, evidence: &'static [&'static str],
}
const APPROVED: &[Profile] = &[];

/// Policy uncertainty remains visible even when an old Python helper computes
/// a percentage for this identity. No caller-provided ratio bypass is exposed.
pub fn evaluate(id: &SecurityId, date: TradingDate, reference: Option<Price>) -> Result<LimitDecision> {
    evaluate_profiles(id, date, reference, APPROVED)
}

fn evaluate_profiles(id: &SecurityId, date: TradingDate, reference: Option<Price>, profiles: &[Profile]) -> Result<LimitDecision> {
    let selected = profiles.iter().filter(|profile| profile.market == id.market() && profile.exact_security == id.number()
        && date >= profile.from && profile.to.is_none_or(|end| date <= end)).collect::<Vec<_>>();
    if selected.len() != 1 {
        return Ok(LimitDecision::new(LimitRule::Unknown { reason: format!("no unique approved dated rule for {id} on {date}") },
            LimitStatus::UnknownRule, None, None, vec!["price_limits_v1: dated_authoritative_rule_sources=[]".to_owned()]));
    }
    let profile = selected[0];
    if profile.evidence.is_empty() { return Err(invalid("approved profile lacks evidence")); }
    let ratio = profile.ratio.map(|value| Ratio::with_provenance(NumericValue::F64(value), RatioUnit::Fraction, ValueProvenance::WireRule));
    let rule = LimitRule::Approved { id: profile.id.to_owned(), effective_from: profile.from, effective_to: profile.to, ratio };
    let evidence = profile.evidence.iter().map(|source| (*source).to_owned()).collect();
    let Some(ratio) = profile.ratio else { return Ok(LimitDecision::new(rule, LimitStatus::NoPriceLimit, None, None, evidence)); };
    if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) { return Err(invalid("invalid approved limit ratio")); }
    let Some(reference) = reference.filter(|price| price.as_f64().is_finite() && price.as_f64() > 0.0) else {
        return Ok(LimitDecision::new(rule, LimitStatus::MissingReference, None, None, evidence));
    };
    let rounded = |direction| {
        let price = Price::from_f64(reference.as_f64() * (1.0 + direction * ratio));
        price.to_scaled_integer(profile.tick_scale, profile.rounding)
            .map(|value| Price::with_value_and_provenance(NumericValue::ScaledInteger { value, scale: profile.tick_scale }, ValueProvenance::WireRule))
    };
    Ok(LimitDecision::new(rule, LimitStatus::Normal, Some(rounded(1.0)?), Some(rounded(-1.0)?), evidence))
}
fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "price-limit policy") }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artificial_approved_profile_checks_dates_scope_reference_and_tick_rounding() {
        // Deliberately artificial, never registered as an exchange rule.
        let from = TradingDate::new(2026, 1, 1).unwrap(); let to = TradingDate::new(2026, 12, 31).unwrap();
        let profile = Profile { id: "synthetic-only", from, to: Some(to), market: HqMarket::Shanghai,
            exact_security: "600000", ratio: Some(0.1), tick_scale: 2, rounding: RoundingMode::HalfToEven,
            evidence: &["synthetic arithmetic source case; no exchange approval"] };
        let id = "sh600000".parse().unwrap();
        let result = evaluate_profiles(&id, from, Some(Price::scaled_integer(1000, 2)), &[profile]).unwrap();
        assert_eq!(result.status(), LimitStatus::Normal);
        assert_eq!(result.upper().unwrap().to_scaled_integer_exact(2).unwrap(), 1100);
        assert_eq!(result.lower().unwrap().to_scaled_integer_exact(2).unwrap(), 900);
        assert_eq!(evaluate(&id, from, Some(Price::from_f64(10.0))).unwrap().status(), LimitStatus::UnknownRule);
    }
}
