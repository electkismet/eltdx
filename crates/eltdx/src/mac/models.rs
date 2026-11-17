//! Source-native MAC identities and values. No owner or transport state lives here.
use crate::{Error, ErrorKind, Result};
use encoding_rs::GBK;

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "mac")
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MacMarketId(u16);
impl MacMarketId {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }
    pub fn checked(value: u32) -> Result<Self> {
        u16::try_from(value)
            .map(Self)
            .map_err(|_| invalid("MAC market exceeds u16"))
    }
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Request code encoded as strict Python-compatible GBK, occupying at most 22 bytes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MacCode {
    text: String,
    wire: Vec<u8>,
}
impl MacCode {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let text = value.into();
        if text.is_empty() || text.contains('\0') {
            return Err(invalid("MAC code must be nonempty and contain no NUL"));
        }
        let (wire, _, had_errors) = GBK.encode(&text);
        if had_errors {
            return Err(invalid("MAC code is not representable in GBK"));
        }
        if wire.len() > 22 {
            return Err(invalid("MAC code exceeds the 22-byte MAC wire field"));
        }
        let wire = wire.into_owned();
        Ok(Self { text, wire })
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub fn encoded(&self) -> &[u8] {
        &self.wire
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        self.text.capacity() + self.wire.capacity()
    }
    pub fn wire_bytes(&self) -> [u8; 22] {
        let mut bytes = [0; 22];
        bytes[..self.wire.len()].copy_from_slice(&self.wire);
        bytes
    }
}

/// A server-side resource name has its own 70-byte field. It is not a
/// security code and is never interpreted as a local filesystem path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RemoteFileName {
    text: String,
    wire: Vec<u8>,
}
impl RemoteFileName {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let text = value.into();
        if text.is_empty() || text.contains('\0') {
            return Err(invalid(
                "remote MAC filename must be nonempty and contain no NUL",
            ));
        }
        let (wire, _, errors) = GBK.encode(&text);
        if errors {
            return Err(invalid("remote MAC filename is not representable in GBK"));
        }
        if wire.len() > 70 {
            return Err(invalid("remote MAC filename exceeds 70 GBK bytes"));
        }
        Ok(Self {
            wire: wire.into_owned(),
            text,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub fn encoded(&self) -> &[u8] {
        &self.wire
    }
    pub fn wire_bytes(&self) -> [u8; 70] {
        let mut bytes = [0; 70];
        bytes[..self.wire.len()].copy_from_slice(&self.wire);
        bytes
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MacInstrumentId {
    market: MacMarketId,
    code: MacCode,
}
impl MacInstrumentId {
    pub fn new(market: MacMarketId, code: MacCode) -> Self {
        Self { market, code }
    }
    pub const fn market(&self) -> MacMarketId {
        self.market
    }
    pub fn code(&self) -> &MacCode {
        &self.code
    }
}

/// Conversion is explicit and never infers an HQ exchange from a market
/// number. EX uses UTF-8 request codes; MAC additionally requires GBK.
impl TryFrom<&crate::ex::ExInstrumentId> for MacInstrumentId {
    type Error = Error;
    fn try_from(value: &crate::ex::ExInstrumentId) -> Result<Self> {
        Ok(Self::new(
            MacMarketId::new(u16::from(value.market().get())),
            MacCode::new(value.code().as_str())?,
        ))
    }
}
impl TryFrom<crate::ex::ExInstrumentId> for MacInstrumentId {
    type Error = Error;
    fn try_from(value: crate::ex::ExInstrumentId) -> Result<Self> {
        Self::try_from(&value)
    }
}
impl TryFrom<&MacInstrumentId> for crate::ex::ExInstrumentId {
    type Error = Error;
    fn try_from(value: &MacInstrumentId) -> Result<Self> {
        let market = crate::ex::ExMarketId::new(u32::from(value.market().get()))?;
        let code = crate::ex::ExCode::new(value.code().as_str())?;
        Ok(Self::new(market, code))
    }
}
impl TryFrom<MacInstrumentId> for crate::ex::ExInstrumentId {
    type Error = Error;
    fn try_from(value: MacInstrumentId) -> Result<Self> {
        Self::try_from(&value)
    }
}

/// MAC-EX market identity remains an independent u8 namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MacExMarketId(u8);
impl MacExMarketId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
    pub fn checked(value: u16) -> Result<Self> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| invalid("MAC-EX market exceeds u8"))
    }
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacExInstrumentId {
    market: MacExMarketId,
    code: MacCode,
}
impl MacExInstrumentId {
    pub fn new(market: MacExMarketId, code: MacCode) -> Self {
        Self { market, code }
    }
    pub const fn market(&self) -> MacExMarketId {
        self.market
    }
    pub fn code(&self) -> &MacCode {
        &self.code
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireFloat {
    bits: u32,
}
impl WireFloat {
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }
    pub const fn bits(self) -> u32 {
        self.bits
    }
    pub fn as_f32(self) -> f32 {
        f32::from_bits(self.bits)
    }
    pub fn as_f64(self) -> f64 {
        f64::from(self.as_f32())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeUnit {
    Currency(String),
    Shares,
    Lots,
    Contracts,
    Percent,
    Ratio,
    IndexPoints,
    Count,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence {
    WireDefined,
    SourceDocumented,
    Inferred,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldMeaning {
    unit: NativeUnit,
    evidence: Evidence,
    source_semantics: Option<&'static str>,
}
impl FieldMeaning {
    pub fn unknown() -> Self {
        Self {
            unit: NativeUnit::Unknown,
            evidence: Evidence::Unknown,
            source_semantics: None,
        }
    }
    pub fn documented(unit: NativeUnit) -> Self {
        Self {
            unit,
            evidence: Evidence::SourceDocumented,
            source_semantics: None,
        }
    }
    pub fn documented_as(unit: NativeUnit, semantics: &'static str) -> Self {
        Self {
            unit,
            evidence: Evidence::SourceDocumented,
            source_semantics: Some(semantics),
        }
    }
    pub fn unit(&self) -> &NativeUnit {
        &self.unit
    }
    pub const fn evidence(&self) -> Evidence {
        self.evidence
    }
    pub const fn source_semantics(&self) -> Option<&'static str> {
        self.source_semantics
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotKind {
    F32,
    U32,
    I32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotValue {
    F32(WireFloat),
    U32(u32),
    I32(i32),
    Unknown([u8; 4]),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldContext {
    Instrument,
    Board,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuoteField {
    pub bit: crate::mac::bitmap::FieldId,
    pub name: Option<&'static str>,
    pub context: FieldContext,
    pub value: SlotValue,
    pub derived_text: Option<String>,
    pub meaning: FieldMeaning,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuoteRow {
    pub market: MacMarketId,
    pub code_raw: [u8; 22],
    pub name_raw: [u8; 44],
    pub fields: Vec<QuoteField>,
    /// Response bitmap as received, including the reserved/control region.
    pub reserved_bitmap: [u8; 20],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacPeriod {
    Minute5,
    Minute15,
    Minute30,
    Minute60,
    Daily,
    Weekly,
    Monthly,
    Minute1,
    Mins,
    Days,
    Quarterly,
    Yearly,
    Seconds,
    Wire(u16),
}
impl MacPeriod {
    pub const fn wire(self) -> u16 {
        match self {
            Self::Minute5 => 0,
            Self::Minute15 => 1,
            Self::Minute30 => 2,
            Self::Minute60 => 3,
            Self::Daily => 4,
            Self::Weekly => 5,
            Self::Monthly => 6,
            Self::Minute1 => 7,
            Self::Mins => 8,
            Self::Days => 9,
            Self::Quarterly => 10,
            Self::Yearly => 11,
            Self::Seconds => 13,
            Self::Wire(v) => v,
        }
    }
    pub const fn has_intraday_time(self) -> bool {
        matches!(self,Self::Minute5|Self::Minute15|Self::Minute30|Self::Minute60|Self::Minute1|Self::Mins|Self::Seconds|Self::Wire(0..=3|7..=8|13))
    }
    pub fn known(value: u16) -> Result<Self> {
        Ok(match value {
            0 => Self::Minute5,
            1 => Self::Minute15,
            2 => Self::Minute30,
            3 => Self::Minute60,
            4 => Self::Daily,
            5 => Self::Weekly,
            6 => Self::Monthly,
            7 => Self::Minute1,
            8 => Self::Mins,
            9 => Self::Days,
            10 => Self::Quarterly,
            11 => Self::Yearly,
            13 => Self::Seconds,
            _ => {
                return Err(invalid(
                    "unknown MAC period requires explicit MacPeriod::Wire",
                ))
            }
        })
    }
    pub const fn has_known_semantics(self) -> bool {
        matches!(self.wire(), 0..=11 | 13)
    }
    /// MAC source uses five minutes per MINS unit. This is not HQ's period8
    /// parameter or EX's fixed three-minute period8. Calendar periods have no
    /// exact elapsed duration. Seconds remain an explicit known duration.
    pub const fn duration_seconds(self, times: std::num::NonZeroU16) -> Option<u32> {
        match self.wire() {
            0 => Some(300),
            1 => Some(900),
            2 => Some(1800),
            3 => Some(3600),
            7 => Some(60),
            8 => Some(300 * times.get() as u32),
            13 => Some(times.get() as u32),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacAdjust {
    None,
    Qfq,
    Hfq,
}
impl MacAdjust {
    pub const fn wire(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Qfq => 1,
            Self::Hfq => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BarRecord {
    pub date_raw: u32,
    /// Seconds from midnight; meaningful only for intraday periods.
    pub time_seconds: u32,
    pub values: [WireFloat; 7],
    pub source_bytes: [u8; 36],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacExRoute {
    MacCommand,
    ExHongKongTransactions,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionPrice {
    MacF32(WireFloat),
    HongKongScaledU32 { raw: u32, divisor: u32 },
}
impl TransactionPrice {
    pub fn as_f64(self) -> f64 {
        match self {
            Self::MacF32(value) => value.as_f64(),
            Self::HongKongScaledU32 { raw, divisor } => {
                let value = super::routing::hk_price_with::<_, std::convert::Infallible>(
                    f64::from(raw),
                    f64::from(divisor),
                    |raw, divisor| Ok(raw / divisor),
                );
                match value {
                    Ok(value) => value,
                    Err(never) => match never {},
                }
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionSlot {
    pub time_seconds: u32,
    pub price: TransactionPrice,
    pub volume: u32,
    pub trade_count: u32,
    pub nature: u16,
    pub trade_count_provided: bool,
}
