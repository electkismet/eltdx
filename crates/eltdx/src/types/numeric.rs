#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NumericValue {
    ScaledInteger { value: i64, scale: u32 },
    F32(f32),
    F64(f64),
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Price(NumericValue);

impl Price {
    pub const fn scaled_integer(value: i64, scale: u32) -> Self {
        Self(NumericValue::ScaledInteger { value, scale })
    }
    pub const fn from_f32(value: f32) -> Self { Self(NumericValue::F32(value)) }
    pub const fn from_f64(value: f64) -> Self { Self(NumericValue::F64(value)) }
    pub const fn value(self) -> NumericValue { self.0 }
    pub fn as_f64(self) -> f64 { self.0.as_f64() }
    pub const fn precision(self) -> Option<u32> { self.0.precision() }

    pub const fn source(self) -> PriceSource {
        match self.0 {
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
        match self.0 {
            NumericValue::ScaledInteger { value, scale: source_scale } => {
                convert_scaled_integer(value, source_scale, scale, rounding)
            }
            NumericValue::F32(_) | NumericValue::F64(_) => {
                let scaled = self.as_f64() * 10_f64.powf(f64::from(scale));
                round_f64_to_i64(scaled, rounding)
            }
        }
    }

    pub fn to_scaled_integer_exact(self, scale: u32) -> crate::error::Result<i64> {
        let NumericValue::ScaledInteger { value, scale: source_scale } = self.0 else {
            return Err(conversion_error("exact conversion requires reliable scaled-integer source precision"));
        };
        if scale >= source_scale {
            let factor = 10_i64.checked_pow(scale - source_scale)
                .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
            return value.checked_mul(factor)
                .ok_or_else(|| conversion_error("price scale conversion overflows i64"));
        }
        let factor = 10_i64.checked_pow(source_scale - scale)
            .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
        if value % factor != 0 {
            return Err(conversion_error("price conversion would discard fractional precision"));
        }
        Ok(value / factor)
    }
}

fn round_f64_to_i64(scaled: f64, rounding: RoundingMode) -> crate::error::Result<i64> {
    if !scaled.is_finite() {
        return Err(conversion_error("price cannot be represented as a finite scaled integer"));
    }
    let rounded = match rounding {
        RoundingMode::TowardZero => scaled.trunc(),
        RoundingMode::Floor => scaled.floor(),
        RoundingMode::Ceiling => scaled.ceil(),
        RoundingMode::HalfAwayFromZero => {
            if scaled.is_sign_negative() { (scaled - 0.5).ceil() } else { (scaled + 0.5).floor() }
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
        let factor = 10_i64.checked_pow(target_scale - source_scale)
            .ok_or_else(|| conversion_error("price scale conversion overflows i64"))?;
        return value.checked_mul(factor)
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
    if fraction < 0.5 { floor }
    else if fraction > 0.5 { floor + 1.0 }
    else if floor % 2.0 == 0.0 { floor }
    else { floor + 1.0 }
}

fn conversion_error(message: &str) -> crate::error::Error {
    crate::error::Error::new(crate::error::ErrorKind::InvalidArgument, message)
        .with_context("field", "price")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum QuantityUnit {
    Shares,
    Lots,
    Bonds,
    FundUnits,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Quantity {
    value: NumericValue,
    unit: QuantityUnit,
}

impl Quantity {
    pub const fn new(value: NumericValue, unit: QuantityUnit) -> Self { Self { value, unit } }
    pub const fn value(self) -> NumericValue { self.value }
    pub const fn unit(self) -> QuantityUnit { self.unit }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum AmountUnit {
    Cny,
    Hkd,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Amount {
    value: NumericValue,
    unit: AmountUnit,
}

impl Amount {
    pub const fn new(value: NumericValue, unit: AmountUnit) -> Self { Self { value, unit } }
    pub const fn value(self) -> NumericValue { self.value }
    pub const fn unit(self) -> AmountUnit { self.unit }
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
    unit: RatioUnit,
}

impl Ratio {
    pub const fn new(value: NumericValue, unit: RatioUnit) -> Self { Self { value, unit } }
    pub const fn value(self) -> NumericValue { self.value }
    pub const fn unit(self) -> RatioUnit { self.unit }
}
