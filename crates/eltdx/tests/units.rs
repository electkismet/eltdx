//! Authored source cases; execution is deferred until the controller opens validation.
use std::num::NonZeroU32;

use eltdx::types::{Amount, AmountUnit, Clock, NumericValue, Price, Quantity, QuantityUnit, Ratio, RatioUnit, RoundingMode, TradingDate, ValueProvenance};
use eltdx::{EventTime, EventTimeSource};

#[test]
fn exact_integer_sources_survive_large_and_minimum_values() {
    for value in [9_007_199_254_740_993, i64::MIN, i64::MAX] {
        let price = Price::with_value_and_provenance(
            NumericValue::ScaledInteger { value, scale: 3 }, ValueProvenance::WireRule,
        );
        assert_eq!(price.to_scaled_integer_exact(3).unwrap(), value);
        assert_eq!(price.provenance(), ValueProvenance::WireRule);
    }
    assert_eq!(Price::scaled_integer(i64::MIN, 1).to_scaled_integer(0, RoundingMode::TowardZero).unwrap(), -922_337_203_685_477_580);
}

#[test]
fn wire_float_bits_are_preserved_without_claiming_exact_integer_precision() {
    for bits in [0x3d_cc_cc_cd, 0x8000_0000, 0x7fc0_4321, 0x7f80_0000] {
        let price = Price::from_f32(f32::from_bits(bits));
        assert_eq!(price.value().wire_f32_bits(), Some(bits));
        assert!(price.to_scaled_integer_exact(2).is_err());
    }
    assert!(Price::from_f32(f32::INFINITY).to_scaled_integer(2, RoundingMode::HalfToEven).is_err());
}

#[test]
fn documented_units_preserve_original_value_and_provenance() {
    let lots = Quantity::with_provenance(NumericValue::ScaledInteger { value: 23, scale: 0 }, QuantityUnit::Lots, ValueProvenance::WireRule);
    let shares = lots.lots_as_shares(NonZeroU32::new(100).unwrap()).unwrap();
    assert_eq!(shares.value(), NumericValue::ScaledInteger { value: 2300, scale: 0 });
    assert_eq!(shares.source_value(), lots.value());
    assert_eq!(shares.source_unit(), QuantityUnit::Lots);
    assert_eq!(shares.unit(), QuantityUnit::Shares);
    let amount = Amount::cny_from_ten_thousands(NumericValue::ScaledInteger { value: 125, scale: 2 }, ValueProvenance::WireRule).unwrap();
    assert_eq!(amount.value(), NumericValue::ScaledInteger { value: 1_250_000, scale: 2 });
    assert_eq!(amount.source_unit(), AmountUnit::TenThousandCny);
    let ratio = Ratio::fraction_from_percentage_points(NumericValue::ScaledInteger { value: 1234, scale: 2 }, ValueProvenance::WireRule).unwrap();
    assert_eq!(ratio.value(), NumericValue::ScaledInteger { value: 1234, scale: 4 });
    assert_eq!(ratio.source_unit(), RatioUnit::PercentagePoints);
}

#[test]
fn unknown_units_and_nonfinite_conversion_fail_explicitly() {
    assert!(Quantity::new(NumericValue::F32(1.0), QuantityUnit::Unknown).lots_as_shares(NonZeroU32::new(100).unwrap()).is_err());
    assert!(Amount::new(NumericValue::F32(1.0), AmountUnit::Unknown).as_cny().is_err());
    assert!(Ratio::new(NumericValue::F32(1.0), RatioUnit::Unknown).as_fraction().is_err());
    assert!(Amount::cny_from_ten_thousands(NumericValue::F32(f32::MAX), ValueProvenance::WireRule).is_err());
}

struct FixedClock(i64);
impl Clock for FixedClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> { Ok(self.0) }
}

#[test]
fn shanghai_midnight_uses_injected_utc_epoch_and_gregorian_rollover() {
    assert_eq!(TradingDate::shanghai_today(&FixedClock(57_599)).unwrap(), TradingDate::new(1970, 1, 1).unwrap());
    assert_eq!(TradingDate::shanghai_today(&FixedClock(57_600)).unwrap(), TradingDate::new(1970, 1, 2).unwrap());
    for (start, end) in [((2024, 2, 28), (2024, 2, 29)), ((2024, 2, 29), (2024, 3, 1)), ((2026, 12, 31), (2027, 1, 1))] {
        let source = EventTime::local(TradingDate::new(start.0, start.1, start.2).unwrap(), 23, 58, 1).unwrap();
        let derived = source.add_minutes(5).unwrap();
        assert_eq!(derived.date(), Some(TradingDate::new(end.0, end.1, end.2).unwrap()));
        assert_eq!(derived.clock(), Some((0, 3, 1)));
        assert_eq!(derived.source_kind(), EventTimeSource::Local);
        assert_eq!(source.clock(), Some((23, 58, 1)));
    }
}
