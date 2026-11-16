use crate::error::{Error, ErrorKind, Result};
use crate::types::batch::PageCursor;
use crate::types::codes::query_error;
use crate::types::provenance::{ResultMeta, ValueProvenance};
use crate::types::time::EventTime;
use crate::types::{Amount, AmountUnit, NumericValue, Price, Quantity, QuantityUnit, Ratio, RatioUnit, SecurityId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum QuoteCategory {
    HqAShares,
    Wire(u16),
}

impl Default for QuoteCategory {
    fn default() -> Self { Self::HqAShares }
}

impl QuoteCategory {
    pub const fn raw(self) -> u16 { match self { Self::HqAShares => 6, Self::Wire(value) => value } }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum QuoteSort {
    Code,
    Last,
    Amount,
    ChangePct,
    SealAmount,
    OpenAmount,
    RiseSpeed,
    ShortTurnover,
    VolumeRiseSpeed,
    OpeningRush,
    TwoMinuteAmount,
    OpenChange,
    HighChange,
    LowChange,
    Drawdown,
    Attack,
    Wire(u16),
}

impl Default for QuoteSort {
    fn default() -> Self { Self::Code }
}

impl QuoteSort {
    pub const fn raw(self) -> u16 {
        match self {
            Self::Code => 0,
            Self::Last => 6,
            Self::Amount => 10,
            Self::ChangePct => 14,
            Self::SealAmount => 28,
            Self::OpenAmount => 29,
            Self::RiseSpeed => 46,
            Self::ShortTurnover => 204,
            Self::VolumeRiseSpeed => 208,
            Self::OpeningRush => 266,
            Self::TwoMinuteAmount => 268,
            Self::OpenChange => 281,
            Self::HighChange => 282,
            Self::LowChange => 283,
            Self::Drawdown => 286,
            Self::Attack => 287,
            Self::Wire(value) => value,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryQuery {
    category: QuoteCategory,
    sort: QuoteSort,
    start: u16,
    count: usize,
    ascending: bool,
    cursor: Option<PageCursor>,
}

impl CategoryQuery {
    pub fn new() -> Self { Self::default() }

    pub fn with_category(mut self, value: QuoteCategory) -> Result<Self> { self.category = value; Ok(self) }
    pub fn with_sort(mut self, value: QuoteSort) -> Result<Self> { self.sort = value; Ok(self) }
    pub fn with_start(mut self, value: u16) -> Result<Self> {
        self.start = value;
        self.cursor = None;
        Ok(self)
    }
    pub fn with_count(mut self, value: usize) -> Result<Self> {
        if !(1..=80).contains(&value) { return Err(query_error("count", "must be between 1 and 80")); }
        self.count = value;
        Ok(self)
    }
    pub fn with_ascending(mut self, value: bool) -> Result<Self> { self.ascending = value; Ok(self) }
    pub fn with_cursor(mut self, cursor: &PageCursor) -> Result<Self> {
        self.start = u16::try_from(cursor.next_offset())
            .map_err(|_| query_error("cursor", "category offset exceeds the wire range"))?;
        self.cursor = Some(cursor.clone());
        Ok(self)
    }

    pub const fn category(&self) -> QuoteCategory { self.category }
    pub const fn sort(&self) -> QuoteSort { self.sort }
    pub const fn start(&self) -> u16 { self.start }
    pub const fn count(&self) -> usize { self.count }
    pub const fn ascending(&self) -> bool { self.ascending }
    pub fn cursor(&self) -> Option<&PageCursor> { self.cursor.as_ref() }
}

impl Default for CategoryQuery {
    fn default() -> Self {
        Self { category: QuoteCategory::HqAShares, sort: QuoteSort::Code, start: 0,
            count: 80, ascending: false, cursor: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct QuoteLevel {
    price: Price,
    volume: Quantity,
}

impl QuoteLevel {
    pub(crate) const fn new(price: Price, volume: Quantity) -> Self { Self { price, volume } }
    pub const fn price(&self) -> Price { self.price }
    pub const fn volume(&self) -> Quantity { self.volume }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Snapshot {
    id: SecurityId,
    active: u16,
    last: Price,
    pre_close: Price,
    open: Price,
    high: Price,
    low: Price,
    event_time: EventTime,
    server_time_raw: i64,
    total: Quantity,
    current: Quantity,
    inside: Quantity,
    outside: Quantity,
    amount: Amount,
    amount_wire_word: Option<u32>,
    unknown_after_time_raw: Option<i64>,
    unknown_after_outer_raw: Option<i64>,
    open_amount_raw: Option<i64>,
    open_amount: Amount,
    bids: Vec<QuoteLevel>,
    asks: Vec<QuoteLevel>,
    meta: ResultMeta,
}

impl Snapshot {
    pub(crate) fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> Result<()> {
        self.id.charge_heap(charge)?;
        charge.array::<QuoteLevel>(self.bids.capacity())?;
        charge.array::<QuoteLevel>(self.asks.capacity())?;
        self.meta.charge_heap(charge)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: SecurityId, active: u16, last: Price, pre_close: Price, open: Price, high: Price, low: Price,
        event_time: EventTime, server_time_raw: i64, total: Quantity, current: Quantity,
        inside: Quantity, outside: Quantity, amount: Amount, open_amount: Amount,
        bids: Vec<QuoteLevel>, asks: Vec<QuoteLevel>, meta: ResultMeta,
    ) -> Self {
        Self { id, active, last, pre_close, open, high, low, event_time, server_time_raw, total, current,
            inside, outside, amount, amount_wire_word: None, unknown_after_time_raw: None,
            unknown_after_outer_raw: None, open_amount_raw: None, open_amount, bids, asks, meta }
    }

    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn active(&self) -> u16 { self.active }
    pub const fn last(&self) -> Price { self.last }
    pub const fn pre_close(&self) -> Price { self.pre_close }
    pub const fn open(&self) -> Price { self.open }
    pub const fn high(&self) -> Price { self.high }
    pub const fn low(&self) -> Price { self.low }
    pub const fn event_time(&self) -> EventTime { self.event_time }
    pub const fn server_time_raw(&self) -> i64 { self.server_time_raw }
    pub const fn total(&self) -> Quantity { self.total }
    pub const fn current(&self) -> Quantity { self.current }
    pub const fn inside(&self) -> Quantity { self.inside }
    pub const fn outside(&self) -> Quantity { self.outside }
    pub const fn amount(&self) -> Amount { self.amount }
    /// Original custom wire scalar, whose encoding is not IEEE f32.
    pub const fn amount_wire_word(&self) -> Option<u32> { self.amount_wire_word }
    pub(crate) fn with_amount_wire_word(mut self, value: u32) -> Self { self.amount_wire_word = Some(value); self }
    pub(crate) fn with_quote_wire_fields(
        mut self, after_time: Option<i64>, after_outer: Option<i64>, open_amount: Option<i64>,
    ) -> Self {
        self.unknown_after_time_raw = after_time;
        self.unknown_after_outer_raw = after_outer;
        self.open_amount_raw = open_amount;
        self
    }
    pub const fn unknown_after_time_raw(&self) -> Option<i64> { self.unknown_after_time_raw }
    pub const fn unknown_after_outer_raw(&self) -> Option<i64> { self.unknown_after_outer_raw }
    pub const fn open_amount_raw(&self) -> Option<i64> { self.open_amount_raw }
    pub const fn open_amount(&self) -> Amount { self.open_amount }
    pub fn bids(&self) -> &[QuoteLevel] { &self.bids }
    pub fn asks(&self) -> &[QuoteLevel] { &self.asks }
    pub fn meta(&self) -> &ResultMeta { &self.meta }

    pub fn change(&self) -> Price {
        match (self.last.value(), self.pre_close.value()) {
            (NumericValue::ScaledInteger { value: last, scale }, NumericValue::ScaledInteger { value: previous, scale: other }) if scale == other => {
                last.checked_sub(previous).map_or_else(
                    || Price::with_value_and_provenance(NumericValue::F64(self.last.as_f64() - self.pre_close.as_f64()), ValueProvenance::WireRule),
                    |value| Price::with_value_and_provenance(NumericValue::ScaledInteger { value, scale }, ValueProvenance::WireRule),
                )
            }
            _ => Price::with_value_and_provenance(
                NumericValue::F64(self.last.as_f64() - self.pre_close.as_f64()), ValueProvenance::WireRule),
        }
    }

    pub fn change_pct(&self) -> Option<Ratio> {
        let previous = self.pre_close.as_f64();
        let value = (self.last.as_f64() - previous) / previous;
        (previous != 0.0 && value.is_finite()).then(|| Ratio::with_provenance(
            NumericValue::F64(value), RatioUnit::Fraction, ValueProvenance::WireRule))
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LegacyQuote {
    snapshot: Snapshot,
    trading_status: u16,
    rise_speed: Option<Ratio>,
    active2: Option<u16>,
    tail_metrics: [i64; 4],
}

impl LegacyQuote {
    pub(crate) const fn new(snapshot: Snapshot, trading_status: u16, rise_speed: Option<Ratio>, active2: Option<u16>, tail_metrics: [i64; 4]) -> Self {
        Self { snapshot, trading_status, rise_speed, active2, tail_metrics }
    }
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub const fn trading_status(&self) -> u16 { self.trading_status }
    pub const fn rise_speed(&self) -> Option<Ratio> { self.rise_speed }
    pub const fn active2(&self) -> Option<u16> { self.active2 }
    pub const fn tail_metrics(&self) -> &[i64; 4] { &self.tail_metrics }
    pub const fn is_suspended(&self) -> bool { self.trading_status & 0x20 != 0 }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CategoryQuote {
    snapshot: Snapshot,
    bid1: QuoteLevel,
    ask1: QuoteLevel,
    status_sort_raw: u16,
    rise_speed: Ratio,
    short_turnover: Ratio,
    opening_rush: Ratio,
    volume_rise_speed: Ratio,
    two_minute_amount: Amount,
    depth: f64,
    meta: ResultMeta,
}

impl CategoryQuote {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        snapshot: Snapshot, bid1: QuoteLevel, ask1: QuoteLevel, status_sort_raw: u16,
        rise_speed: Ratio, short_turnover: Ratio, opening_rush: Ratio, volume_rise_speed: Ratio,
        two_minute_amount: Amount, depth: f64, meta: ResultMeta,
    ) -> Self {
        Self { snapshot, bid1, ask1, status_sort_raw, rise_speed, short_turnover, opening_rush,
            volume_rise_speed, two_minute_amount, depth, meta }
    }
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub const fn bid1(&self) -> &QuoteLevel { &self.bid1 }
    pub const fn ask1(&self) -> &QuoteLevel { &self.ask1 }
    pub const fn status_sort_raw(&self) -> u16 { self.status_sort_raw }
    pub const fn rise_speed(&self) -> Ratio { self.rise_speed }
    pub const fn short_turnover(&self) -> Ratio { self.short_turnover }
    pub const fn opening_rush(&self) -> Ratio { self.opening_rush }
    pub const fn volume_rise_speed(&self) -> Ratio { self.volume_rise_speed }
    pub const fn two_minute_amount(&self) -> Amount { self.two_minute_amount }
    pub const fn depth(&self) -> f64 { self.depth }
    pub fn meta(&self) -> &ResultMeta { &self.meta }

    pub fn seal_amount(&self) -> Option<Amount> {
        let value = self.bid1.price().as_f64() * self.bid1.volume().as_f64() * 100.0;
        value.is_finite().then(|| Amount::with_provenance(
            NumericValue::F64(value), AmountUnit::Cny, ValueProvenance::WireRule))
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QuoteUpdate {
    id: SecurityId,
    cursor: u32,
    active: u16,
    snapshot: Snapshot,
    status_raw: i64,
    desynchronized: bool,
}

impl QuoteUpdate {
    pub(crate) fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> Result<()> {
        self.id.charge_heap(charge)?;
        self.snapshot.charge_heap(charge)
    }
    pub(crate) const fn new(id: SecurityId, cursor: u32, active: u16, snapshot: Snapshot, status_raw: i64) -> Self {
        Self { id, cursor, active, snapshot, status_raw, desynchronized: false }
    }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn cursor(&self) -> u32 { self.cursor }
    pub const fn active(&self) -> u16 { self.active }
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub const fn status_raw(&self) -> i64 { self.status_raw }
    pub const fn desynchronized(&self) -> bool { self.desynchronized }
    pub(crate) fn mark_desynchronized(&mut self) { self.desynchronized = true; }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QuoteUpdates {
    requested: Vec<SecurityId>,
    updates: Vec<QuoteUpdate>,
    meta: ResultMeta,
}

impl QuoteUpdates {
    pub(crate) fn new(requested: Vec<SecurityId>, updates: Vec<QuoteUpdate>, meta: ResultMeta) -> Self {
        Self { requested, updates, meta }
    }
    pub fn requested(&self) -> &[SecurityId] { &self.requested }
    pub fn updates(&self) -> &[QuoteUpdate] { &self.updates }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn into_parts(self) -> (Vec<SecurityId>, Vec<QuoteUpdate>, ResultMeta) {
        (self.requested, self.updates, self.meta)
    }
}

pub(crate) fn price_from_raw(raw: i64, wire_denominator: u16, divisor: u16) -> Result<Price> {
    if wire_denominator == 0 || divisor == 0 {
        return Err(Error::new(ErrorKind::MetadataUnavailable, "price precision denominator is unavailable"));
    }
    let mut denominator = u32::from(wire_denominator).checked_mul(u32::from(divisor))
        .ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "price denominator overflows"))?;
    let mut scale = 0_u32;
    while denominator > 1 && denominator % 10 == 0 {
        denominator /= 10;
        scale = scale.checked_add(1).ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "price scale overflows"))?;
    }
    if denominator != 1 {
        return Err(Error::new(ErrorKind::MetadataUnavailable, "price denominator is not a decimal scale"));
    }
    Ok(Price::with_value_and_provenance(NumericValue::ScaledInteger { value: raw, scale }, ValueProvenance::SecurityTable))
}

pub(crate) fn quote_quantity(raw: i64) -> Quantity {
    Quantity::with_provenance(NumericValue::ScaledInteger { value: raw, scale: 0 }, QuantityUnit::Lots, ValueProvenance::WireRule)
}

pub(crate) fn unknown_amount(value: f64) -> Amount {
    Amount::with_provenance(NumericValue::F64(value), AmountUnit::Unknown, ValueProvenance::WireRule)
}

pub(crate) fn cny_hundredths(raw: i64) -> Result<Amount> {
    let value = raw.checked_mul(100).ok_or_else(|| query_error("amount", "wire amount overflows CNY representation"))?;
    Ok(Amount::with_provenance(NumericValue::ScaledInteger { value, scale: 0 }, AmountUnit::Cny, ValueProvenance::WireRule))
}

pub(crate) fn percentage_points(raw: i16) -> Ratio {
    Ratio::with_provenance(NumericValue::ScaledInteger { value: i64::from(raw), scale: 2 }, RatioUnit::PercentagePoints, ValueProvenance::WireRule)
}

pub(crate) fn unknown_ratio(raw: f32) -> Result<Ratio> {
    if !raw.is_finite() { return Err(Error::new(ErrorKind::Protocol, "ratio contains a non-finite wire value")); }
    Ok(Ratio::with_provenance(NumericValue::F32(raw), RatioUnit::Unknown, ValueProvenance::WireRule))
}

pub(crate) fn price_from_quote_delta(current_milli: i64, delta: i64, divisor: u16) -> Result<Price> {
    let adjusted = delta.checked_mul(10).and_then(|delta| current_milli.checked_add(delta))
        .ok_or_else(|| Error::new(ErrorKind::Protocol, "quote level raw price overflows"))?;
    price_from_raw(adjusted, 1000, divisor)
}

pub(crate) fn parse_identity(market_id: u8, code: &str) -> Result<SecurityId> {
    let market = match market_id {
        0 => crate::types::HqMarket::Shenzhen,
        1 => crate::types::HqMarket::Shanghai,
        2 => crate::types::HqMarket::Beijing,
        _ => return Err(Error::new(ErrorKind::Protocol, "quote contains an unknown market identity")
            .with_context("market_id", format!("{market_id}"))),
    };
    SecurityId::new(market, code).map_err(|error| Error::new(ErrorKind::Protocol, "quote contains an invalid security identity")
        .with_source(error))
}
