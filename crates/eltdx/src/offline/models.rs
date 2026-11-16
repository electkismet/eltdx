use crate::error::Result;
use crate::TradingDate;
use super::invalid;

/// The original float32 bit pattern is authoritative, including NaNs and negative zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireFloat { bits: u32 }
impl WireFloat {
    pub const fn from_bits(bits: u32) -> Self { Self { bits } }
    pub fn from_f32(value: f32) -> Self { Self { bits: value.to_bits() } }
    pub const fn bits(self) -> u32 { self.bits }
    pub fn as_f32(self) -> f32 { f32::from_bits(self.bits) }
    pub fn as_f64(self) -> f64 { f64::from(self.as_f32()) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficients { price: f64, volume: f64 }
impl Default for Coefficients {
    fn default() -> Self { Self { price: 0.01, volume: 0.01 } }
}
impl Coefficients {
    pub fn new(price: f64, volume: f64) -> Result<Self> {
        if !price.is_finite() || price <= 0.0 || !volume.is_finite() || volume <= 0.0 {
            return Err(invalid("coefficients must be finite and positive"));
        }
        Ok(Self { price, volume })
    }
    pub const fn price(self) -> f64 { self.price }
    pub const fn volume(self) -> f64 { self.volume }
    pub(crate) const fn source(price: f64, volume: f64) -> Self { Self { price, volume } }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurityType { ShA, ShB, ShIndex, ShFund, ShBond, SzA, SzB, SzIndex, SzFund, SzBond, Unknown }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence { WireDefined, SourceDocumented, Inferred, Unknown }
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoefficientDecision { coefficients: Coefficients, security_type: SecurityType, evidence: Evidence }
impl CoefficientDecision {
    pub(crate) fn new(coefficients: Coefficients, security_type: SecurityType, evidence: Evidence) -> Self {
        Self { coefficients, security_type, evidence }
    }
    pub const fn coefficients(self) -> Coefficients { self.coefficients }
    pub const fn security_type(self) -> SecurityType { self.security_type }
    pub const fn evidence(self) -> Evidence { self.evidence }
    pub const fn is_fallback(self) -> bool { matches!(self.security_type, SecurityType::Unknown) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exchange { Shenzhen, Shanghai }
impl Exchange {
    pub const fn as_str(self) -> &'static str { match self { Self::Shenzhen => "sz", Self::Shanghai => "sh" } }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCode(String);
impl LocalCode {
    pub fn new(code: impl Into<String>) -> Result<Self> {
        let code = code.into();
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid("local code must contain exactly six ASCII digits"));
        }
        Ok(Self(code))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TimeOfDay { hour: u8, minute: u8, second: u8 }
impl TimeOfDay {
    pub fn new(hour: u8, minute: u8, second: u8) -> Result<Self> {
        if hour > 23 || minute > 59 || second > 59 { return Err(invalid("invalid time of day")); }
        Ok(Self { hour, minute, second })
    }
    pub const fn hour(self) -> u8 { self.hour }
    pub const fn minute(self) -> u8 { self.minute }
    pub const fn second(self) -> u8 { self.second }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MinuteKey { date: TradingDate, time: TimeOfDay }
impl MinuteKey {
    pub fn new(date: TradingDate, time: TimeOfDay) -> Result<Self> {
        if time.second != 0 { return Err(invalid("minute keys require second zero")); }
        Ok(Self { date, time })
    }
    pub const fn date(self) -> TradingDate { self.date }
    pub const fn time(self) -> TimeOfDay { self.time }
    /// Calendar fields in the same order as the Python offline model key.
    pub fn datetime_key(self) -> (u16, u16, u16, u16, u16) {
        let values = super::bars::python_legacy::datetime_fields(|field| {
            Ok::<_, std::convert::Infallible>(match field {
                "year" => self.date.year(), "month" => u16::from(self.date.month()),
                "day" => u16::from(self.date.day()), "hour" => u16::from(self.time.hour()),
                _ => u16::from(self.time.minute()),
            })
        });
        let [year, month, day, hour, minute] = match values { Ok(values) => values, Err(impossible) => match impossible {} };
        (year, month, day, hour, minute)
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RecordKey { Date(TradingDate), Minute(MinuteKey) }

#[derive(Clone, Debug)]
pub struct DailyBar {
    date: TradingDate, prices: [f64; 4], volume: f64, amount: WireFloat, adjust_flag: u32,
    pub(crate) integers: Option<([u32; 5], Coefficients)>, raw: Option<[u8; 32]>,
}
impl DailyBar {
    /// Prices are open/high/low/close, volume uses the selected coefficient's unit.
    pub fn new(date: TradingDate, prices: [f64; 4], volume: f64, amount: WireFloat, adjust_flag: u32) -> Result<Self> {
        if prices.iter().chain(std::iter::once(&volume)).any(|v| !v.is_finite() || *v < 0.0) {
            return Err(invalid("integer bar prices and volume must be finite and nonnegative"));
        }
        Ok(Self { date, prices, volume, amount, adjust_flag, integers: None, raw: None })
    }
    pub(crate) fn decoded(date: TradingDate, integers: [u32; 5], coefficients: Coefficients,
        amount: WireFloat, adjust_flag: u32, raw: Option<[u8; 32]>) -> Self
    {
        Self { date, prices: [integers[0] as f64 * coefficients.price, integers[1] as f64 * coefficients.price,
            integers[2] as f64 * coefficients.price, integers[3] as f64 * coefficients.price],
            volume: integers[4] as f64 * coefficients.volume, amount, adjust_flag,
            integers: Some((integers, coefficients)), raw }
    }
    pub const fn date(&self) -> TradingDate { self.date }
    pub const fn prices(&self) -> &[f64; 4] { &self.prices }
    pub const fn open(&self) -> f64 { self.prices[0] }
    pub const fn high(&self) -> f64 { self.prices[1] }
    pub const fn low(&self) -> f64 { self.prices[2] }
    pub const fn close(&self) -> f64 { self.prices[3] }
    pub const fn volume(&self) -> f64 { self.volume }
    pub const fn amount(&self) -> WireFloat { self.amount }
    pub const fn adjust_flag(&self) -> u32 { self.adjust_flag }
    pub fn source_integers(&self) -> Option<&[u32; 5]> { self.integers.as_ref().map(|(values, _)| values) }
    pub fn source_coefficients(&self) -> Option<Coefficients> { self.integers.map(|(_, coefficients)| coefficients) }
    pub fn raw(&self) -> Option<&[u8; 32]> { self.raw.as_ref() }
}

#[derive(Clone, Debug)]
pub(crate) enum MinutePrices { Floats([WireFloat; 4]), Integers([u32; 4]), Values([f64; 4]) }
#[derive(Clone, Debug)]
pub struct MinBar {
    key: MinuteKey, pub(crate) prices: MinutePrices, volume: f64,
    volume_integer: Option<u32>,
    amount: WireFloat, reserved: u32, raw: Option<[u8; 32]>,
}
impl MinBar {
    pub fn new(key: MinuteKey, prices: [f64; 4], volume: f64, amount: WireFloat, reserved: u32) -> Result<Self> {
        if !volume.is_finite() || volume < 0.0 { return Err(invalid("minute volume must be finite and nonnegative")); }
        if prices.iter().any(|value| !value.is_finite()) { return Err(invalid("use from_floats for nonfinite float32 bit patterns")); }
        Ok(Self { key, prices: MinutePrices::Values(prices), volume, volume_integer: None, amount, reserved, raw: None })
    }
    pub fn from_floats(key: MinuteKey, prices: [WireFloat; 4], volume: u32, amount: WireFloat, reserved: u32) -> Self {
        Self { key, prices: MinutePrices::Floats(prices), volume: volume as f64, volume_integer: Some(volume), amount, reserved, raw: None }
    }
    pub(crate) fn decoded(key: MinuteKey, prices: MinutePrices, volume: u32,
        amount: WireFloat, reserved: u32, raw: Option<[u8; 32]>) -> Self
    { Self { key, prices, volume: volume as f64, volume_integer: Some(volume), amount, reserved, raw } }
    pub const fn key(&self) -> MinuteKey { self.key }
    pub fn prices(&self) -> [f64; 4] {
        match &self.prices {
            MinutePrices::Floats(values) => values.map(WireFloat::as_f64),
            MinutePrices::Integers(values) => values.map(|v| v as f64 / 100.0),
            MinutePrices::Values(values) => *values,
        }
    }
    pub fn open(&self) -> f64 { self.prices()[0] }
    pub fn high(&self) -> f64 { self.prices()[1] }
    pub fn low(&self) -> f64 { self.prices()[2] }
    pub fn close(&self) -> f64 { self.prices()[3] }
    pub const fn volume(&self) -> f64 { self.volume }
    pub const fn source_volume(&self) -> Option<u32> { self.volume_integer }
    pub const fn amount(&self) -> WireFloat { self.amount }
    pub const fn reserved(&self) -> u32 { self.reserved }
    pub fn float_prices(&self) -> Option<&[WireFloat; 4]> { match &self.prices { MinutePrices::Floats(values) => Some(values), _ => None } }
    pub fn integer_prices(&self) -> Option<&[u32; 4]> { match &self.prices { MinutePrices::Integers(values) => Some(values), _ => None } }
    pub fn raw(&self) -> Option<&[u8; 32]> { self.raw.as_ref() }
}

#[derive(Clone, Debug)]
pub struct ExDailyBar {
    date: TradingDate, prices: [WireFloat; 4], amount: u32, volume: u32,
    settlement: WireFloat, raw: Option<[u8; 32]>,
}
impl ExDailyBar {
    pub fn new(date: TradingDate, prices: [WireFloat; 4], amount_bits: u32, volume: u32, settlement: WireFloat) -> Self {
        Self { date, prices, amount: amount_bits, volume, settlement, raw: None }
    }
    pub(crate) fn with_raw(mut self, raw: Option<[u8; 32]>) -> Self { self.raw = raw; self }
    pub const fn date(&self) -> TradingDate { self.date }
    pub const fn prices(&self) -> &[WireFloat; 4] { &self.prices }
    pub const fn open(&self) -> WireFloat { self.prices[0] }
    pub const fn high(&self) -> WireFloat { self.prices[1] }
    pub const fn low(&self) -> WireFloat { self.prices[2] }
    pub const fn close(&self) -> WireFloat { self.prices[3] }
    pub const fn amount(&self) -> u32 { self.amount }
    pub const fn hk_stock_amount(&self) -> WireFloat { WireFloat::from_bits(self.amount) }
    pub const fn volume(&self) -> u32 { self.volume }
    pub const fn settlement(&self) -> WireFloat { self.settlement }
    pub fn raw(&self) -> Option<&[u8; 32]> { self.raw.as_ref() }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockInfo { name: String, category: u8, declared_count: u16, codes: Vec<String>, reserved: u16 }
impl BlockInfo {
    pub(crate) fn new(name: String, category: u8, declared_count: u16, codes: Vec<String>, reserved: u16) -> Self {
        Self { name, category, declared_count, codes, reserved }
    }
    pub fn name(&self) -> &str { &self.name }
    pub const fn category(&self) -> u8 { self.category }
    pub const fn declared_count(&self) -> u16 { self.declared_count }
    pub const fn count(&self) -> u16 { self.declared_count }
    pub fn codes(&self) -> &[String] { &self.codes }
    pub const fn reserved(&self) -> u16 { self.reserved }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerBlock { blockname: String, block_type: String, codes: Vec<String> }
impl CustomerBlock {
    pub(crate) fn new(blockname: String, block_type: String, codes: Vec<String>) -> Self { Self { blockname, block_type, codes } }
    pub fn blockname(&self) -> &str { &self.blockname }
    pub fn block_type(&self) -> &str { &self.block_type }
    pub fn codes(&self) -> &[String] { &self.codes }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndustryCodes { tdx: String, sw: String }
impl IndustryCodes {
    pub(crate) fn new(tdx: String, sw: String) -> Self { Self { tdx, sw } }
    pub fn tdx(&self) -> &str { &self.tdx }
    pub fn sw(&self) -> &str { &self.sw }
}
/// Stable first insertion order, with a later duplicate replacing only the value.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IndustryMap { entries: Vec<(String, IndustryCodes)> }
impl IndustryMap {
    pub(crate) fn from_entries(entries: Vec<(String, IndustryCodes)>) -> Self { Self { entries } }
    pub fn entries(&self) -> &[(String, IndustryCodes)] { &self.entries }
    pub fn get(&self, code: &str) -> Option<&IndustryCodes> {
        self.entries.iter().find(|(key, _)| key == code).map(|(_, value)| value)
    }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}
