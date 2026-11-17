//! Checked values for requests. Response text stays separate from request IDs:
//! replacement-decoded wire text need not be a valid request identity.
use super::invalid;
use crate::{Result, TradingDate};
use std::num::NonZeroU16;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExMarketId(u8);
impl ExMarketId {
    pub fn new(value: u32) -> Result<Self> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| invalid("EX market exceeds u8"))
    }
    pub const fn from_wire(value: u8) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExCode(String);
impl ExCode {
    /// EX request commands encode UTF-8, even where their responses use GBK.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 9 || value.contains('\0') {
            return Err(invalid(
                "EX request code must be nonempty UTF-8, at most 9 bytes, without NUL",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn wire_bytes(&self) -> [u8; 9] {
        let mut bytes = [0; 9];
        bytes[..self.0.len()].copy_from_slice(self.0.as_bytes());
        bytes
    }
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExInstrumentId {
    market: ExMarketId,
    code: ExCode,
}
impl ExInstrumentId {
    pub fn new(market: ExMarketId, code: ExCode) -> Self {
        Self { market, code }
    }
    pub const fn market(&self) -> ExMarketId {
        self.market
    }
    pub fn code(&self) -> &ExCode {
        &self.code
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExPeriod {
    Minute5,
    Minute15,
    Minute30,
    Minute60,
    Day,
    Week,
    Month,
    Minute1,
    Minute3,
    Days,
    Quarter,
    Year,
    /// Explicit passthrough: no duration or semantic claim for unknown values.
    Wire(u16),
}
impl ExPeriod {
    pub fn known(value: u16) -> Result<Self> {
        Ok(match value {
            0 => Self::Minute5,
            1 => Self::Minute15,
            2 => Self::Minute30,
            3 => Self::Minute60,
            4 => Self::Day,
            5 => Self::Week,
            6 => Self::Month,
            7 => Self::Minute1,
            8 => Self::Minute3,
            9 => Self::Days,
            10 => Self::Quarter,
            11 => Self::Year,
            _ => return Err(invalid("unknown EX period: use explicit ExPeriod::Wire")),
        })
    }
    pub const fn wire(self) -> u16 {
        match self {
            Self::Minute5 => 0,
            Self::Minute15 => 1,
            Self::Minute30 => 2,
            Self::Minute60 => 3,
            Self::Day => 4,
            Self::Week => 5,
            Self::Month => 6,
            Self::Minute1 => 7,
            Self::Minute3 => 8,
            Self::Days => 9,
            Self::Quarter => 10,
            Self::Year => 11,
            Self::Wire(value) => value,
        }
    }
    pub const fn duration_minutes(self) -> Option<u16> {
        // Exact EX source registry; MAC code 8 has a different meaning.
        match self.wire() {
            0 => Some(5),
            1 => Some(15),
            2 => Some(30),
            3 => Some(60),
            7 => Some(1),
            8 => Some(3),
            _ => None,
        }
    }
    pub const fn is_intraday(self) -> bool {
        matches!(self.wire(), 0..=3 | 7 | 8)
    }
    pub const fn has_known_calendar_semantics(self) -> bool {
        self.wire() <= 11
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BarTime {
    #[default]
    Start,
    End,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuoteCategory {
    HongKong,
    Futures,
}
impl QuoteCategory {
    pub const fn wire(self) -> u16 {
        match self {
            Self::HongKong => 2,
            Self::Futures => 3,
        }
    }
    pub fn new(value: u16) -> Result<Self> {
        match value {
            2 => Ok(Self::HongKong),
            3 => Ok(Self::Futures),
            _ => Err(invalid("EX quote category must be 2 or 3")),
        }
    }
}
fn count(value: u32) -> Result<NonZeroU16> {
    u16::try_from(value)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or_else(|| invalid("EX count must be in 1..=65535"))
}
fn date_wire(date: TradingDate) -> Result<u32> {
    if date.year() > 9999 {
        return Err(invalid("EX date exceeds YYYYMMDD range"));
    }
    Ok(u32::from(date.year()) * 10000 + u32::from(date.month()) * 100 + u32::from(date.day()))
}
pub(crate) fn encode_date(date: TradingDate) -> u32 {
    // All dated query construction validates the YYYYMMDD range.
    u32::from(date.year()) * 10000 + u32::from(date.month()) * 100 + u32::from(date.day())
}

#[derive(Clone, Debug)]
pub struct InstrumentsQuery {
    start: u32,
    count: NonZeroU16,
}
impl InstrumentsQuery {
    pub fn new(start: u32) -> Self {
        Self {
            start,
            count: NonZeroU16::new(100).unwrap_or(NonZeroU16::MIN),
        }
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU16 {
        self.count
    }
    pub fn with_start(mut self, start: u32) -> Self {
        self.start = start;
        self
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.count = count(value)?;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct QuoteQuery {
    id: ExInstrumentId,
}
impl QuoteQuery {
    pub fn new(id: ExInstrumentId) -> Self {
        Self { id }
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub fn with_id(mut self, id: ExInstrumentId) -> Self {
        self.id = id;
        self
    }
}
#[derive(Clone, Debug)]
pub struct MinutesQuery {
    id: ExInstrumentId,
}
impl MinutesQuery {
    pub fn new(id: ExInstrumentId) -> Self {
        Self { id }
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub fn with_id(mut self, id: ExInstrumentId) -> Self {
        self.id = id;
        self
    }
}
#[derive(Clone, Debug)]
pub struct QuoteListQuery {
    market: ExMarketId,
    category: QuoteCategory,
    start: u16,
    count: NonZeroU16,
}
impl QuoteListQuery {
    pub fn new(market: ExMarketId, category: QuoteCategory) -> Self {
        Self {
            market,
            category,
            start: 0,
            count: NonZeroU16::new(80).unwrap_or(NonZeroU16::MIN),
        }
    }
    pub const fn market(&self) -> ExMarketId {
        self.market
    }
    pub const fn category(&self) -> QuoteCategory {
        self.category
    }
    pub const fn start(&self) -> u16 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU16 {
        self.count
    }
    pub fn with_market(mut self, value: ExMarketId) -> Self {
        self.market = value;
        self
    }
    pub fn with_category(mut self, value: QuoteCategory) -> Self {
        self.category = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        self.start = u16::try_from(value).map_err(|_| invalid("EX quote start exceeds u16"))?;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.count = count(value)?;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BarsQuery {
    id: ExInstrumentId,
    period: ExPeriod,
    start: u32,
    count: NonZeroU16,
    bar_time: BarTime,
}
impl BarsQuery {
    pub fn new(id: ExInstrumentId, period: ExPeriod) -> Self {
        Self {
            id,
            period,
            start: 0,
            count: NonZeroU16::new(700).unwrap_or(NonZeroU16::MIN),
            bar_time: BarTime::Start,
        }
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn period(&self) -> ExPeriod {
        self.period
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU16 {
        self.count
    }
    pub const fn bar_time(&self) -> BarTime {
        self.bar_time
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Self {
        self.start = value;
        self
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.count = count(value)?;
        Ok(self)
    }
    pub fn with_period(mut self, value: ExPeriod) -> Result<Self> {
        if self.bar_time == BarTime::End && !value.has_known_calendar_semantics() {
            return Err(invalid("unknown period cannot align bar end"));
        }
        self.period = value;
        Ok(self)
    }
    pub fn with_bar_time(mut self, value: BarTime) -> Result<Self> {
        if value == BarTime::End && !self.period.has_known_calendar_semantics() {
            return Err(invalid("unknown period cannot align bar end"));
        }
        self.bar_time = value;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct BarsRangeQuery {
    id: ExInstrumentId,
    start_date: TradingDate,
    end_date: TradingDate,
}
impl BarsRangeQuery {
    pub fn new(id: ExInstrumentId, start_date: TradingDate, end_date: TradingDate) -> Result<Self> {
        date_wire(start_date)?;
        date_wire(end_date)?;
        if start_date > end_date {
            return Err(invalid("EX range dates are reversed"));
        }
        Ok(Self {
            id,
            start_date,
            end_date,
        })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn start_date(&self) -> TradingDate {
        self.start_date
    }
    pub const fn end_date(&self) -> TradingDate {
        self.end_date
    }
    pub const fn bar_time(&self) -> BarTime {
        BarTime::Start
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_start_date(self, value: TradingDate) -> Result<Self> {
        Self::new(self.id, value, self.end_date)
    }
    pub fn with_end_date(self, value: TradingDate) -> Result<Self> {
        Self::new(self.id, self.start_date, value)
    }
    pub fn with_bar_time(self, value: BarTime) -> Result<Self> {
        if value == BarTime::End {
            Err(invalid("EX bars_range has no period metadata for End"))
        } else {
            Ok(self)
        }
    }
}
#[derive(Clone, Debug)]
pub struct HistoryMinutesQuery {
    id: ExInstrumentId,
    date: TradingDate,
}
impl HistoryMinutesQuery {
    pub fn new(id: ExInstrumentId, date: TradingDate) -> Result<Self> {
        date_wire(date)?;
        Ok(Self { id, date })
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn date(&self) -> TradingDate {
        self.date
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_date(self, value: TradingDate) -> Result<Self> {
        Self::new(self.id, value)
    }
}
#[derive(Clone, Debug)]
pub struct TransactionsQuery {
    id: ExInstrumentId,
    start: u32,
    count: NonZeroU16,
}
impl TransactionsQuery {
    pub fn new(id: ExInstrumentId) -> Self {
        Self {
            id,
            start: 0,
            count: NonZeroU16::new(1800).unwrap_or(NonZeroU16::MIN),
        }
    }
    pub fn id(&self) -> &ExInstrumentId {
        &self.id
    }
    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn count(&self) -> NonZeroU16 {
        self.count
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Self {
        self.id = value;
        self
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        if value > i32::MAX as u32 {
            return Err(invalid("EX transaction start exceeds i32::MAX"));
        }
        self.start = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.count = count(value)?;
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct HistoryTransactionsQuery {
    page: TransactionsQuery,
    date: TradingDate,
}
impl HistoryTransactionsQuery {
    pub fn new(id: ExInstrumentId, date: TradingDate) -> Result<Self> {
        date_wire(date)?;
        Ok(Self {
            page: TransactionsQuery::new(id),
            date,
        })
    }
    pub fn id(&self) -> &ExInstrumentId {
        self.page.id()
    }
    pub const fn date(&self) -> TradingDate {
        self.date
    }
    pub const fn start(&self) -> u32 {
        self.page.start()
    }
    pub const fn count(&self) -> NonZeroU16 {
        self.page.count()
    }
    pub fn with_id(mut self, value: ExInstrumentId) -> Self {
        self.page = self.page.with_id(value);
        self
    }
    pub fn with_date(mut self, value: TradingDate) -> Result<Self> {
        date_wire(value)?;
        self.date = value;
        Ok(self)
    }
    pub fn with_start(mut self, value: u32) -> Result<Self> {
        self.page = self.page.with_start(value)?;
        Ok(self)
    }
    pub fn with_count(mut self, value: u32) -> Result<Self> {
        self.page = self.page.with_count(value)?;
        Ok(self)
    }
}
