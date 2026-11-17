use std::num::NonZeroU32;

use crate::types::provenance::ValueProvenance;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NumericValue {
    ScaledInteger { value: i64, scale: u32 },
    F32(#[cfg_attr(feature = "serde", serde(serialize_with = "serialize_finite_f32"))] f32),
    F64(#[cfg_attr(feature = "serde", serde(serialize_with = "serialize_finite_f64"))] f64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PriceSource {
    ScaledInteger { scale: u32 },
    WireF32,
    WireF64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RoundingMode {
    TowardZero,
    Floor,
    Ceiling,
    HalfAwayFromZero,
    HalfToEven,
}

impl NumericValue {
    pub fn as_f64(self) -> f64 {
        match self {
            Self::ScaledInteger { value, scale } => value as f64 / 10_f64.powf(f64::from(scale)),
            Self::F32(value) => f64::from(value),
            Self::F64(value) => value,
        }
    }

    pub const fn precision(self) -> Option<u32> {
        match self {
            Self::ScaledInteger { scale, .. } => Some(scale),
            Self::F32(_) | Self::F64(_) => None,
        }
    }

    pub fn is_finite(self) -> bool {
        match self {
            Self::ScaledInteger { .. } => true,
            Self::F32(value) => value.is_finite(),
            Self::F64(value) => value.is_finite(),
        }
    }

    pub fn wire_f32_bits(self) -> Option<u32> {
        match self {
            Self::F32(value) => Some(value.to_bits()),
            _ => None,
        }
    }

    pub fn wire_f64_bits(self) -> Option<u64> {
        match self {
            Self::F64(value) => Some(value.to_bits()),
            _ => None,
        }
    }

    fn checked_mul_integer(self, factor: u32) -> crate::error::Result<Self> {
        if !self.is_finite() {
            return Err(conversion_error(
                "non-finite wire value cannot be converted",
            ));
        }
        match self {
            Self::ScaledInteger { value, scale } => value
                .checked_mul(i64::from(factor))
                .map(|value| Self::ScaledInteger { value, scale })
                .ok_or_else(|| conversion_error("scaled numeric conversion overflows i64")),
            Self::F32(value) => finite_result(Self::F32(value * factor as f32)),
            Self::F64(value) => finite_result(Self::F64(value * f64::from(factor))),
        }
    }

    fn divided_by_power_of_ten(self, exponent: u32) -> crate::error::Result<Self> {
        if !self.is_finite() {
            return Err(conversion_error(
                "non-finite wire value cannot be converted",
            ));
        }
        match self {
            Self::ScaledInteger { value, scale } => Ok(Self::ScaledInteger {
                value,
                scale: scale
                    .checked_add(exponent)
                    .ok_or_else(|| conversion_error("numeric scale overflows u32"))?,
            }),
            Self::F32(value) => Ok(Self::F32(value / 10_f32.powi(exponent as i32))),
            Self::F64(value) => Ok(Self::F64(value / 10_f64.powi(exponent as i32))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Price {
    value: NumericValue,
    provenance: ValueProvenance,
}

impl Price {
    pub const fn scaled_integer(value: i64, scale: u32) -> Self {
        Self::with_value_and_provenance(
            NumericValue::ScaledInteger { value, scale },
            ValueProvenance::Unknown,
        )
    }
    pub const fn from_f32(value: f32) -> Self {
        Self::with_value_and_provenance(NumericValue::F32(value), ValueProvenance::Unknown)
    }
    pub const fn from_f64(value: f64) -> Self {
        Self::with_value_and_provenance(NumericValue::F64(value), ValueProvenance::Unknown)
    }
    pub const fn with_value_and_provenance(
        value: NumericValue,
        provenance: ValueProvenance,
    ) -> Self {
        Self { value, provenance }
    }
    pub const fn value(self) -> NumericValue {
        self.value
    }
    pub const fn provenance(self) -> ValueProvenance {
        self.provenance
    }
    pub fn as_f64(self) -> f64 {
        self.value.as_f64()
    }
    pub const fn precision(self) -> Option<u32> {
        self.value.precision()
    }

    pub const fn source(self) -> PriceSource {
        match self.value {
            NumericValue::ScaledInteger { scale, .. } => PriceSource::ScaledInteger { scale },
            NumericValue::F32(_) => PriceSource::WireF32,
            NumericValue::F64(_) => PriceSource::WireF64,
        }
    }

    pub fn to_scaled_integer(
        self,
        scale: u32,
        rounding: RoundingMode,
    ) -> crate::error::Result<i64> {
        match self.value {
            NumericValue::ScaledInteger {
                value,
                scale: source_scale,
            } => convert_scaled_integer(value, source_scale, scale, rounding),
            NumericValue::F32(_) | NumericValue::F64(_) => {
                let scaled = self.as_f64() * 10_f64.powf(f64::from(scale));
                round_f64_to_i64(scaled, rounding)
            }
        }
    }

    pub fn to_scaled_integer_exact(self, scale: u32) -> crate::error::Result<i64> {
        let NumericValue::ScaledInteger {
            value,
            scale: source_scale,
        } = self.value
        else {
            return Err(conversion_error(
                "exact conversion requires reliable scaled-integer source precision",
            ));
        };
        if scale >= source_scale {
            let factor = 10_i64
                .checked_pow(scale - source_scale)
                .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
            return value
                .checked_mul(factor)
                .ok_or_else(|| conversion_error("price scale conversion overflows i64"));
        }
        let factor = 10_i64
            .checked_pow(source_scale - scale)
            .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
        if value % factor != 0 {
            return Err(conversion_error(
                "price conversion would discard fractional precision",
            ));
        }
        Ok(value / factor)
    }
}

fn round_f64_to_i64(scaled: f64, rounding: RoundingMode) -> crate::error::Result<i64> {
    if !scaled.is_finite() {
        return Err(conversion_error(
            "price cannot be represented as a finite scaled integer",
        ));
    }
    let rounded = match rounding {
        RoundingMode::TowardZero => scaled.trunc(),
        RoundingMode::Floor => scaled.floor(),
        RoundingMode::Ceiling => scaled.ceil(),
        RoundingMode::HalfAwayFromZero => {
            if scaled.is_sign_negative() {
                (scaled - 0.5).ceil()
            } else {
                (scaled + 0.5).floor()
            }
        }
        RoundingMode::HalfToEven => round_half_to_even(scaled),
    };
    if rounded < i64::MIN as f64 || rounded >= 9_223_372_036_854_775_808.0 {
        return Err(conversion_error("scaled price is outside the i64 range"));
    }
    Ok(rounded as i64)
}

fn convert_scaled_integer(
    value: i64,
    source_scale: u32,
    target_scale: u32,
    rounding: RoundingMode,
) -> crate::error::Result<i64> {
    if target_scale >= source_scale {
        let factor = 10_i64
            .checked_pow(target_scale - source_scale)
            .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
        return value
            .checked_mul(factor)
            .ok_or_else(|| conversion_error("price scale conversion overflows i64"));
    }

    let exponent = source_scale - target_scale;
    let Some(divisor) = 10_u128.checked_pow(exponent) else {
        let rounded = match rounding {
            RoundingMode::Floor if value < 0 => -1,
            RoundingMode::Ceiling if value > 0 => 1,
            _ => 0,
        };
        return Ok(rounded);
    };
    let value_wide = i128::from(value);
    let quotient = value_wide / (divisor as i128);
    let remainder = value_wide - quotient * (divisor as i128);
    round_integer_quotient(quotient, remainder, divisor, rounding)
}

fn round_integer_quotient(
    quotient: i128,
    remainder: i128,
    divisor: u128,
    rounding: RoundingMode,
) -> crate::error::Result<i64> {
    if remainder == 0 {
        return i64::try_from(quotient)
            .map_err(|_| conversion_error("scaled price is outside the i64 range"));
    }
    let adjust_away = match rounding {
        RoundingMode::TowardZero => false,
        RoundingMode::Floor => remainder < 0,
        RoundingMode::Ceiling => remainder > 0,
        RoundingMode::HalfAwayFromZero | RoundingMode::HalfToEven => {
            let magnitude = remainder.unsigned_abs();
            let doubled = magnitude.saturating_mul(2);
            if doubled > divisor {
                true
            } else if doubled < divisor {
                false
            } else if rounding == RoundingMode::HalfAwayFromZero {
                true
            } else {
                quotient % 2 != 0
            }
        }
    };
    let rounded = if adjust_away {
        quotient + if remainder < 0 { -1 } else { 1 }
    } else {
        quotient
    };
    i64::try_from(rounded).map_err(|_| conversion_error("scaled price is outside the i64 range"))
}

fn round_half_to_even(value: f64) -> f64 {
    let floor = value.floor();
    let fraction = value - floor;
    if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else if floor % 2.0 == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}

fn conversion_error(message: &str) -> crate::error::Error {
    crate::error::Error::new(crate::error::ErrorKind::InvalidArgument, message)
        .with_context("field", "numeric unit conversion")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum QuantityUnit {
    Shares,
    TenThousandShares,
    Lots,
    Bonds,
    FundUnits,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Quantity {
    value: NumericValue,
    source_value: NumericValue,
    unit: QuantityUnit,
    source_unit: QuantityUnit,
    provenance: ValueProvenance,
}

impl Quantity {
    pub fn shares_from_ten_thousands_f64(
        value: f64,
        provenance: ValueProvenance,
    ) -> crate::Result<Self> {
        let converted = value * 10_000.0;
        if !value.is_finite() || !converted.is_finite() {
            return Err(conversion_error(
                "ten-thousand-share value cannot be converted to finite shares",
            ));
        }
        Ok(Self {
            value: NumericValue::F64(converted),
            source_value: NumericValue::F64(value),
            unit: QuantityUnit::Shares,
            source_unit: QuantityUnit::TenThousandShares,
            provenance,
        })
    }
    pub fn shares_from_ten_thousands(
        value: f32,
        provenance: ValueProvenance,
    ) -> crate::Result<Self> {
        if !value.is_finite() {
            return Err(conversion_error("non-finite ten-thousand-share wire value"));
        }
        Ok(Self {
            value: NumericValue::F64(f64::from(value) * 10_000.0),
            source_value: NumericValue::F32(value),
            unit: QuantityUnit::Shares,
            source_unit: QuantityUnit::TenThousandShares,
            provenance,
        })
    }
    pub const fn new(value: NumericValue, unit: QuantityUnit) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance: ValueProvenance::Unknown,
        }
    }
    pub const fn with_provenance(
        value: NumericValue,
        unit: QuantityUnit,
        provenance: ValueProvenance,
    ) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance,
        }
    }
    pub const fn value(self) -> NumericValue {
        self.value
    }
    pub const fn source_value(self) -> NumericValue {
        self.source_value
    }
    pub const fn unit(self) -> QuantityUnit {
        self.unit
    }
    pub const fn source_unit(self) -> QuantityUnit {
        self.source_unit
    }
    pub const fn provenance(self) -> ValueProvenance {
        self.provenance
    }

    pub fn lots_as_shares(self, shares_per_lot: NonZeroU32) -> crate::error::Result<Self> {
        if self.unit != QuantityUnit::Lots {
            return Err(conversion_error(
                "lot-to-share conversion requires a lots quantity",
            ));
        }
        Ok(Self {
            value: self.value.checked_mul_integer(shares_per_lot.get())?,
            unit: QuantityUnit::Shares,
            ..self
        })
    }

    /// Converts a share count using an explicitly evidenced lot size. Fractional lots
    /// require a decimal lot size (10, 100, ...), preserving the source integer exactly.
    pub fn shares_as_decimal_lots(self, shares_per_lot: NonZeroU32) -> crate::error::Result<Self> {
        if self.unit != QuantityUnit::Shares {
            return Err(conversion_error(
                "share-to-lot conversion requires a shares quantity",
            ));
        }
        let mut factor = shares_per_lot.get();
        let mut exponent = 0;
        while factor > 1 && factor % 10 == 0 {
            factor /= 10;
            exponent += 1;
        }
        if factor != 1 {
            return Err(conversion_error(
                "exact decimal lots require a power-of-ten lot size",
            ));
        }
        Ok(Self {
            value: self.value.divided_by_power_of_ten(exponent)?,
            unit: QuantityUnit::Lots,
            ..self
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AmountUnit {
    Cny,
    ThousandCny,
    TenThousandCny,
    Hkd,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Amount {
    value: NumericValue,
    source_value: NumericValue,
    unit: AmountUnit,
    source_unit: AmountUnit,
    provenance: ValueProvenance,
}

impl Amount {
    pub fn cny_from_thousands(value: f32, provenance: ValueProvenance) -> crate::Result<Self> {
        if !value.is_finite() {
            return Err(conversion_error("non-finite thousand-CNY wire value"));
        }
        Ok(Self {
            value: NumericValue::F64(f64::from(value) * 1_000.0),
            source_value: NumericValue::F32(value),
            unit: AmountUnit::Cny,
            source_unit: AmountUnit::ThousandCny,
            provenance,
        })
    }
    pub const fn new(value: NumericValue, unit: AmountUnit) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance: ValueProvenance::Unknown,
        }
    }
    pub const fn with_provenance(
        value: NumericValue,
        unit: AmountUnit,
        provenance: ValueProvenance,
    ) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance,
        }
    }
    pub fn cny_from_ten_thousands(
        value: NumericValue,
        provenance: ValueProvenance,
    ) -> crate::error::Result<Self> {
        Ok(Self {
            value: value.checked_mul_integer(10_000)?,
            source_value: value,
            unit: AmountUnit::Cny,
            source_unit: AmountUnit::TenThousandCny,
            provenance,
        })
    }
    pub const fn value(self) -> NumericValue {
        self.value
    }
    pub const fn source_value(self) -> NumericValue {
        self.source_value
    }
    pub const fn unit(self) -> AmountUnit {
        self.unit
    }
    pub const fn source_unit(self) -> AmountUnit {
        self.source_unit
    }
    pub const fn provenance(self) -> ValueProvenance {
        self.provenance
    }

    pub fn as_cny(self) -> crate::error::Result<Self> {
        match self.unit {
            AmountUnit::Cny => Ok(self),
            AmountUnit::ThousandCny => Ok(Self {
                value: self.value.checked_mul_integer(1_000)?,
                unit: AmountUnit::Cny,
                ..self
            }),
            AmountUnit::TenThousandCny => Ok(Self {
                value: self.value.checked_mul_integer(10_000)?,
                unit: AmountUnit::Cny,
                ..self
            }),
            AmountUnit::Unknown => Err(conversion_error("unknown amount unit cannot be converted")),
            AmountUnit::Hkd => Err(conversion_error(
                "currency conversion requires an explicit exchange-rate source",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum RatioUnit {
    Fraction,
    PercentagePoints,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ratio {
    value: NumericValue,
    source_value: NumericValue,
    unit: RatioUnit,
    source_unit: RatioUnit,
    provenance: ValueProvenance,
}

impl Ratio {
    pub const fn new(value: NumericValue, unit: RatioUnit) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance: ValueProvenance::Unknown,
        }
    }
    pub const fn with_provenance(
        value: NumericValue,
        unit: RatioUnit,
        provenance: ValueProvenance,
    ) -> Self {
        Self {
            value,
            source_value: value,
            unit,
            source_unit: unit,
            provenance,
        }
    }
    pub fn fraction_from_percentage_points(
        value: NumericValue,
        provenance: ValueProvenance,
    ) -> crate::error::Result<Self> {
        Ok(Self {
            value: value.divided_by_power_of_ten(2)?,
            source_value: value,
            unit: RatioUnit::Fraction,
            source_unit: RatioUnit::PercentagePoints,
            provenance,
        })
    }
    pub const fn value(self) -> NumericValue {
        self.value
    }
    pub const fn source_value(self) -> NumericValue {
        self.source_value
    }
    pub const fn unit(self) -> RatioUnit {
        self.unit
    }
    pub const fn source_unit(self) -> RatioUnit {
        self.source_unit
    }
    pub const fn provenance(self) -> ValueProvenance {
        self.provenance
    }

    pub fn as_fraction(self) -> crate::error::Result<Self> {
        match self.unit {
            RatioUnit::Fraction => Ok(self),
            RatioUnit::PercentagePoints => Ok(Self {
                value: self.value.divided_by_power_of_ten(2)?,
                unit: RatioUnit::Fraction,
                ..self
            }),
            RatioUnit::Unknown => Err(conversion_error("unknown ratio unit cannot be converted")),
        }
    }
}

fn finite_result(value: NumericValue) -> crate::error::Result<NumericValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(conversion_error(
            "numeric conversion overflowed finite wire representation",
        ))
    }
}

#[cfg(feature = "serde")]
pub(crate) fn serialize_finite_f32<S: serde::Serializer>(
    value: &f32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom(
            "non-finite wire f32 requires explicit raw-bit diagnostics",
        ));
    }
    serializer.serialize_f32(*value)
}

#[cfg(feature = "serde")]
fn serialize_finite_f64<S: serde::Serializer>(
    value: &f64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom(
            "non-finite wire f64 requires explicit raw-bit diagnostics",
        ));
    }
    serializer.serialize_f64(*value)
}
