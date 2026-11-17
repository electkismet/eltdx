//! Rich MAC records keep wire scalars and exact second precision. Metadata is
//! shared per response; large collections do not clone raw maps into every row.
use std::sync::Arc;
use bytes::Bytes;
use crate::{Error, ErrorKind, EventTime, Result, TradingDate};
use crate::types::{BarTime, ResultMeta};
use super::models::{BarRecord, FieldMeaning, MacInstrumentId, NativeUnit, TransactionPrice, TransactionSlot, WireFloat};
use super::responses::NativeNumber;
use super::query::BarsQuery;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarTimeSource { SecondsSinceMidnight, CalendarMidnight, UnknownPeriod }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarPriceSource { Wire, LocalForwardAdjustment }
/// Effective price and its unchanged source f32. Local calculations use f64;
/// `bits` always means source bits, including after an adjustment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPrice { wire: WireFloat, adjusted: Option<f64> }
impl BarPrice {
    pub const fn wire(self) -> WireFloat { self.wire }
    pub const fn bits(self) -> u32 { self.wire.bits() }
    pub fn as_f64(self) -> f64 { self.adjusted.unwrap_or_else(|| self.wire.as_f64()) }
    pub const fn adjusted_value(self) -> Option<f64> { self.adjusted }
    pub const fn source(self) -> BarPriceSource {
        if self.adjusted.is_some() { BarPriceSource::LocalForwardAdjustment } else { BarPriceSource::Wire }
    }
}
#[derive(Clone, Debug)]
pub struct Bar {
    id: MacInstrumentId, time: EventTime, source_time: EventTime, time_source: BarTimeSource,
    date_raw: u32, time_raw: u32, values: [NativeNumber<WireFloat>;7],
    prices: [NativeNumber<BarPrice>;4],
    adjustment_events: Option<Arc<[super::util::AdjustmentEvent]>>,
    record_raw: Option<Bytes>, meta: Arc<ResultMeta>,
}
impl Bar {
    pub fn id(&self) -> &MacInstrumentId { &self.id }
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn datetime(&self) -> EventTime { self.time }
    pub const fn source_time(&self) -> EventTime { self.source_time }
    pub const fn time_source(&self) -> BarTimeSource { self.time_source }
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn time_raw(&self) -> u32 { self.time_raw }
    pub fn open(&self) -> &NativeNumber<BarPrice> { &self.prices[0] }
    pub fn high(&self) -> &NativeNumber<BarPrice> { &self.prices[1] }
    pub fn low(&self) -> &NativeNumber<BarPrice> { &self.prices[2] }
    pub fn close(&self) -> &NativeNumber<BarPrice> { &self.prices[3] }
    /// All seven source-native values, including original OHLC, remain intact.
    pub fn wire_values(&self) -> &[NativeNumber<WireFloat>;7] { &self.values }
    pub fn adjustment_events(&self) -> &[super::util::AdjustmentEvent] { self.adjustment_events.as_deref().unwrap_or(&[]) }
    pub fn amount(&self) -> &NativeNumber<WireFloat> { &self.values[4] }
    pub fn volume(&self) -> &NativeNumber<WireFloat> { &self.values[5] }
    /// The strict36B MAC layout always supplies this scalar. A wire zero is
    /// Some(zero); another layout must explicitly report absence as None.
    pub fn float_shares(&self) -> Option<&NativeNumber<WireFloat>> { Some(&self.values[6]) }
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn shared_meta(&self) -> Arc<ResultMeta> { Arc::clone(&self.meta) }
    pub(crate) fn set_meta(&mut self, value: Arc<ResultMeta>) { self.meta = value; }
    pub(crate) fn set_adjusted_prices(&mut self, values: [f64;4]) {
        for (index,value) in values.into_iter().enumerate() {
            self.prices[index] = NativeNumber::unknown(BarPrice { wire: *self.values[index].value(),adjusted: Some(value) });
        }
    }
    pub(crate) fn set_adjustment_events(&mut self, events: Arc<[super::util::AdjustmentEvent]>) { self.adjustment_events = Some(events); }
    pub(crate) fn from_record(value: BarRecord, query: &BarsQuery, record_raw: Option<Bytes>) -> Result<Self> {
        let time_raw = u32::from_le_bytes(value.source_bytes[4..8].try_into()
            .map_err(|_| protocol("MAC bar clock field width differs"))?);
        let (source_time,time_source) = if !query.period().has_known_semantics() {
            (EventTime::unknown_raw(i64::from(value.date_raw)),BarTimeSource::UnknownPeriod)
        } else {
            let date = day(value.date_raw)?;
            if query.period().has_intraday_time() {
                let (hour,minute,second) = clock(value.time_seconds)?;
                (EventTime::local(date,hour,minute,second).map_err(|error| protocol("invalid MAC bar time").with_source(error))?,
                    BarTimeSource::SecondsSinceMidnight)
            } else {
                (EventTime::local(date,0,0,0).map_err(|error| protocol("invalid MAC calendar bar").with_source(error))?,
                    BarTimeSource::CalendarMidnight)
            }
        };
        let time = match (query.bar_time(),query.period().duration_seconds(query.times())) {
            (BarTime::End,Some(delta)) => source_time.add_seconds(i64::from(delta))
                .map_err(|error| protocol("MAC bar end time overflows calendar").with_source(error))?,
            _ => source_time,
        };
        Ok(Self { id: query.id().clone(),time,source_time,time_source,date_raw: value.date_raw,time_raw,
            prices: std::array::from_fn(|index| NativeNumber::unknown(BarPrice { wire: value.values[index],adjusted: None })),
            adjustment_events: None,
            values: value.values.map(NativeNumber::unknown),
            record_raw,
            meta: Arc::new(ResultMeta::default()) })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionSide { Buy, Sell, Neutral, AfterHours, Unknown(u32) }
impl TransactionSide {
    pub const fn from_wire(value: u16) -> Self {
        match value { 0 => Self::Buy, 1 => Self::Sell, 2 => Self::Neutral, 5 => Self::AfterHours, other => Self::Unknown(other as u32) }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountPresence { Provided, NotProvided }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionSource { Mac, ExHongKong }
#[derive(Clone, Debug)]
pub struct Transaction {
    id: MacInstrumentId, time: EventTime, seconds_raw: u32,
    price: TransactionPrice, price_meaning: FieldMeaning, volume: NativeNumber<u32>,
    trade_count: u32, count_presence: CountPresence, nature_raw: u16,
    source: TransactionSource, direction_raw: Option<u16>, open_interest_change: Option<i32>,
    record_raw: Option<Bytes>, meta: Arc<ResultMeta>,
}
impl Transaction {
    pub fn id(&self) -> &MacInstrumentId { &self.id }
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn seconds_raw(&self) -> u32 { self.seconds_raw }
    pub const fn price(&self) -> TransactionPrice { self.price }
    pub fn price_meaning(&self) -> &FieldMeaning { &self.price_meaning }
    pub fn volume(&self) -> &NativeNumber<u32> { &self.volume }
    pub const fn trade_count(&self) -> u32 { self.trade_count }
    pub const fn count_presence(&self) -> CountPresence { self.count_presence }
    pub const fn nature_raw(&self) -> u16 { self.nature_raw }
    pub const fn side(&self) -> TransactionSide { TransactionSide::from_wire(self.nature_raw) }
    pub const fn source(&self) -> TransactionSource { self.source }
    pub const fn direction_raw(&self) -> Option<u16> { self.direction_raw }
    pub const fn open_interest_change(&self) -> Option<i32> { self.open_interest_change }
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn set_meta(&mut self, value: Arc<ResultMeta>) { self.meta = value; }
    pub(crate) fn from_slot(id: &MacInstrumentId, value: TransactionSlot, record_raw: Option<Bytes>) -> Result<Self> {
        let (hour,minute,second) = clock(value.time_seconds)?;
        let time = EventTime::time_of_day(hour,minute,second).map_err(|error| protocol("invalid MAC trade time").with_source(error))?;
        Ok(Self { id: id.clone(),time,seconds_raw: value.time_seconds,price: value.price,price_meaning: FieldMeaning::unknown(),
            volume: NativeNumber::unknown(value.volume),trade_count: value.trade_count,
            count_presence: if value.trade_count_provided { CountPresence::Provided } else { CountPresence::NotProvided },
            nature_raw: value.nature,source: TransactionSource::Mac,direction_raw: None,open_interest_change: None,
            record_raw,meta: Arc::new(ResultMeta::default()) })
    }
    pub(crate) fn from_ex(id: &MacInstrumentId, value: crate::ex::Transaction) -> Result<Self> {
        let (hour,minute,second) = value.time().clock().ok_or_else(|| protocol("HK transaction has no validated clock"))?;
        let seconds = u32::from(hour)*3600+u32::from(minute)*60+u32::from(second);
        Ok(Self { id: id.clone(),time: value.time(),seconds_raw: seconds,
            price: TransactionPrice::HongKongScaledU32 { raw: *value.price_raw().value(),divisor: 1000 },
            price_meaning: FieldMeaning::documented(NativeUnit::Currency("HKD".into())),
            volume: NativeNumber::unknown(*value.volume().value()),trade_count: 0,count_presence: CountPresence::NotProvided,
            nature_raw: value.nature(),source: TransactionSource::ExHongKong,direction_raw: Some(value.direction_raw()),
            open_interest_change: Some(*value.zengcang().value()),record_raw: value.record_raw().cloned(),
            meta: Arc::new(ResultMeta::default()) })
    }
}
fn day(raw: u32) -> Result<TradingDate> {
    let year = u16::try_from(raw/10000).map_err(|_| protocol("MAC bar year exceeds source date range"))?;
    if year > 9999 { return Err(protocol("MAC bar year exceeds four digits")); }
    TradingDate::new(year,((raw%10000)/100) as u8,(raw%100) as u8)
        .map_err(|error| protocol("invalid MAC bar calendar date").with_source(error))
}
fn clock(raw: u32) -> Result<(u8,u8,u8)> {
    if raw >= 86400 { return Err(protocol("MAC record clock exceeds seconds since midnight")); }
    Ok(((raw/3600) as u8,((raw%3600)/60) as u8,(raw%60) as u8))
}
fn protocol(message: &'static str) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac") }
