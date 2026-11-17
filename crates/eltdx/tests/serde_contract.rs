#![cfg(feature = "serde")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use bytes::Bytes;
use eltdx::types::{
    Amount, AmountUnit, EventTime, HqMarket, NumericValue, Price, Quantity, QuantityUnit,
    Ratio, RatioUnit, RawData, SecurityId, TradingDate, ValueProvenance,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn roundtrip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let encoded = serde_json::to_string(value).expect("serialize finite contract value");
    serde_json::from_str(&encoded).expect("deserialize finite contract value")
}

#[test]
fn finite_numeric_values_keep_representation_scale_and_provenance() {
    let price = Price::with_value_and_provenance(
        NumericValue::ScaledInteger { value: 12345, scale: 3 },
        ValueProvenance::WireRule,
    );
    assert_eq!(serde_json::to_value(price).unwrap(), json!({
        "value": { "ScaledInteger": { "value": 12345, "scale": 3 } },
        "provenance": "WireRule",
    }));
    assert_eq!(roundtrip(&price), price);
    for value in [
        NumericValue::ScaledInteger { value: i64::MIN, scale: 4 },
        NumericValue::ScaledInteger { value: i64::MAX, scale: 0 },
        NumericValue::F32(1.25),
        NumericValue::F64(-12.5),
    ] {
        assert_eq!(roundtrip(&value), value);
    }
}

#[test]
fn finite_negative_zero_keeps_its_source_width_and_sign_bits() {
    let f32_value = NumericValue::F32(-0.0);
    let f64_value = NumericValue::F64(-0.0);
    let f32_copy: NumericValue = roundtrip(&f32_value);
    let f64_copy: NumericValue = roundtrip(&f64_value);
    assert_eq!(f32_copy.wire_f32_bits(), Some((-0.0_f32).to_bits()));
    assert_eq!(f32_copy.wire_f64_bits(), None);
    assert_eq!(f64_copy.wire_f64_bits(), Some((-0.0_f64).to_bits()));
    assert_eq!(f64_copy.wire_f32_bits(), None);
    let price = Price::from_f32(-0.0);
    let copy: Price = roundtrip(&price);
    assert_eq!(copy.value().wire_f32_bits(), price.value().wire_f32_bits());
}

#[test]
fn nonfinite_values_fail_serialization_without_mutating_original_wire_bits() {
    for bits in [0x7fc0_1234, f32::INFINITY.to_bits(), f32::NEG_INFINITY.to_bits()] {
        let original = NumericValue::F32(f32::from_bits(bits));
        let error = serde_json::to_string(&original).unwrap_err();
        assert!(error.to_string().contains("non-finite wire f32"));
        assert_eq!(original.wire_f32_bits(), Some(bits));
        assert!(serde_json::to_string(&Price::from_f32(f32::from_bits(bits))).is_err());
    }
    for bits in [
        0x7ff8_0000_0000_1234,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ] {
        let original = NumericValue::F64(f64::from_bits(bits));
        let error = serde_json::to_string(&original).unwrap_err();
        assert!(error.to_string().contains("non-finite wire f64"));
        assert_eq!(original.wire_f64_bits(), Some(bits));
        assert!(serde_json::to_string(&Price::from_f64(f64::from_bits(bits))).is_err());
    }
    // JSON null must not silently become a numeric NaN on input either.
    assert!(serde_json::from_value::<NumericValue>(json!({ "F32": null })).is_err());
    assert!(serde_json::from_value::<NumericValue>(json!({ "F64": null })).is_err());
}

#[test]
fn units_roundtrip_both_display_and_original_source_values() {
    let quantity = Quantity::shares_from_ten_thousands(1.25, ValueProvenance::WireRule).unwrap();
    let copy: Quantity = roundtrip(&quantity);
    assert_eq!(copy, quantity);
    assert_eq!(copy.unit(), QuantityUnit::Shares);
    assert_eq!(copy.source_unit(), QuantityUnit::TenThousandShares);
    assert_eq!(copy.value(), NumericValue::F64(12500.0));
    assert_eq!(copy.source_value().wire_f32_bits(), Some(1.25_f32.to_bits()));
    assert_eq!(copy.provenance(), ValueProvenance::WireRule);

    let amount = Amount::cny_from_thousands(1.25, ValueProvenance::SecurityTable).unwrap();
    let copy: Amount = roundtrip(&amount);
    assert_eq!(copy, amount);
    assert_eq!(copy.unit(), AmountUnit::Cny);
    assert_eq!(copy.source_unit(), AmountUnit::ThousandCny);
    assert_eq!(copy.value(), NumericValue::F64(1250.0));
    assert_eq!(copy.source_value().wire_f32_bits(), Some(1.25_f32.to_bits()));

    let ratio = Ratio::fraction_from_percentage_points(
        NumericValue::ScaledInteger { value: 125, scale: 1 },
        ValueProvenance::UserOverride,
    ).unwrap();
    let copy: Ratio = roundtrip(&ratio);
    assert_eq!(copy, ratio);
    assert_eq!(copy.unit(), RatioUnit::Fraction);
    assert_eq!(copy.source_unit(), RatioUnit::PercentagePoints);
    assert_eq!(copy.value(), NumericValue::ScaledInteger { value: 125, scale: 3 });
}

#[test]
fn raw_bytes_use_integer_sequences_and_keep_absent_distinct_from_empty_wire() {
    let payload = Bytes::from_static(&[0, 93, 255]);
    let frame = Bytes::from_static(&[1, 2, 3, 4]);
    let raw = RawData::new(vec![payload.clone()], Some(vec![frame]), vec![(0, 1, 2)]);
    assert_eq!(serde_json::to_value(&raw).unwrap(), json!({
        "payloads": [[0, 93, 255]],
        "wire_frames": [[1, 2, 3, 4]],
        "row_ranges": [[0, 1, 2]],
        "xor93_rows": [false],
    }));
    assert_eq!(raw.payloads()[0].as_ref(), &[0, 93, 255]);
    let omitted = RawData::new(vec![payload.clone()], None, vec![]);
    let retained_empty = RawData::new(vec![payload], Some(vec![]), vec![]);
    assert_eq!(serde_json::to_value(omitted).unwrap()["wire_frames"], Value::Null);
    assert_eq!(serde_json::to_value(retained_empty).unwrap()["wire_frames"], json!([]));
}

#[test]
fn date_and_event_time_formats_keep_zone_and_unknown_distinctions() {
    let date = TradingDate::new(2024, 2, 29).unwrap();
    let fields = json!({ "year": 2024, "month": 2, "day": 29 });
    assert_eq!(serde_json::to_value(date).unwrap(), fields);
    assert_eq!(serde_json::to_value(EventTime::trading_date(date)).unwrap(),
        json!({ "TradingDate": fields }));
    assert_eq!(serde_json::to_value(EventTime::shanghai(date, 9, 30, 1).unwrap()).unwrap(),
        json!({ "Shanghai": { "date": fields, "hour": 9, "minute": 30, "second": 1 } }));
    assert_eq!(serde_json::to_value(EventTime::local(date, 9, 30, 1).unwrap()).unwrap(),
        json!({ "Local": { "date": fields, "hour": 9, "minute": 30, "second": 1 } }));
    assert_eq!(serde_json::to_value(EventTime::time_of_day(9, 30, 1).unwrap()).unwrap(),
        json!({ "TimeOfDay": { "hour": 9, "minute": 30, "second": 1 } }));
    assert_eq!(serde_json::to_value(EventTime::unknown_raw(i64::MAX)).unwrap(),
        json!({ "UnknownRaw": i64::MAX }));
    assert!(TradingDate::new(2023, 2, 29).is_err());
    assert!(EventTime::shanghai(date, 24, 0, 0).is_err());
    assert!(EventTime::time_of_day(9, 60, 0).is_err());
}

#[test]
fn checked_security_exports_fields_while_unknown_units_are_explicit_variants() {
    let security = SecurityId::new(HqMarket::Shanghai, "600000").unwrap();
    assert_eq!(serde_json::to_value(security).unwrap(), json!({
        "market": "Shanghai", "number": "600000",
    }));
    assert!(SecurityId::new(HqMarket::Shanghai, "60000").is_err());
    assert!(SecurityId::new(HqMarket::Shanghai, "60000A").is_err());
    assert_eq!(roundtrip(&QuantityUnit::Unknown), QuantityUnit::Unknown);
    assert_eq!(roundtrip(&AmountUnit::Unknown), AmountUnit::Unknown);
    assert_eq!(roundtrip(&RatioUnit::Unknown), RatioUnit::Unknown);
    assert_eq!(roundtrip(&ValueProvenance::Unknown), ValueProvenance::Unknown);
    assert_eq!(serde_json::to_value(QuantityUnit::Unknown).unwrap(), json!("Unknown"));
    assert!(serde_json::from_value::<QuantityUnit>(json!("FutureUnit")).is_err());
    assert!(serde_json::from_value::<AmountUnit>(json!("FutureCurrency")).is_err());
    assert!(serde_json::from_value::<RatioUnit>(json!("FutureRatio")).is_err());
    assert!(serde_json::from_value::<ValueProvenance>(json!("FutureRule")).is_err());
}
