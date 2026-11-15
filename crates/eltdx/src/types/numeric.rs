#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NumericValue {
    ScaledInteger { value: i64, scale: u32 },
    F32(f32),
    F64(f64),
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
