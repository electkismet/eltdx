//! Loss-preserving response values. Required opaque bytes survive RawPolicy::Omit.
use bytes::Bytes;
use crate::types::{Diagnostic, RawData, RawPolicy, ResultMeta, SourceStamp};
use crate::EventTime;
use super::query::{ExMarketId, QuoteCategory};

pub type ExtensionMeta = ResultMeta;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireFloat { bits: u32 }
impl WireFloat {
    pub const fn from_bits(bits: u32) -> Self { Self { bits } }
    pub const fn bits(self) -> u32 { self.bits }
    pub fn as_f32(self) -> f32 { f32::from_bits(self.bits) }
    pub fn as_f64(self) -> f64 { f64::from(self.as_f32()) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeUnit { Currency(String), Shares, Lots, Contracts, Percent, Ratio, IndexPoints, Count, Unknown }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence { WireDefined, SourceDocumented, Inferred, Unknown }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldMeaning { unit: NativeUnit, evidence: Evidence }
impl FieldMeaning {
    pub fn unknown() -> Self { Self { unit: NativeUnit::Unknown, evidence: Evidence::Unknown } }
    pub fn unit(&self) -> &NativeUnit { &self.unit }
    pub const fn evidence(&self) -> Evidence { self.evidence }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeNumber<T> { value: T, meaning: FieldMeaning }
impl<T> NativeNumber<T> {
    pub(crate) fn unknown(value: T) -> Self { Self { value, meaning: FieldMeaning::unknown() } }
    pub fn value(&self) -> &T { &self.value }
    pub fn meaning(&self) -> &FieldMeaning { &self.meaning }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeValue { Unsigned(u32), Float(WireFloat) }
#[derive(Clone, Debug)]
pub struct NamedNativeField { pub(crate) name: &'static str, pub(crate) slot: u8, pub(crate) value: NativeValue, pub(crate) meaning: FieldMeaning }
impl NamedNativeField {
    pub fn name(&self) -> &str { self.name }
    pub const fn slot(&self) -> u8 { self.slot }
    pub const fn value(&self) -> NativeValue { self.value }
    pub fn meaning(&self) -> &FieldMeaning { &self.meaning }
}
#[derive(Clone, Debug)]
pub struct ResponseIdentity { pub(crate) market: ExMarketId, pub(crate) code: String, pub(crate) raw: [u8; 10] }
impl ResponseIdentity {
    pub const fn market(&self) -> ExMarketId { self.market }
    pub fn code(&self) -> &str { &self.code }
    pub const fn raw(&self) -> &[u8; 10] { &self.raw }
}
macro_rules! getters {
    ($($name:ident : $ty:ty),* $(,)?) => { $(pub fn $name(&self) -> &$ty { &self.$name })* };
}
#[derive(Clone, Debug)]
pub struct MarketInfo { pub(crate) market: ExMarketId, pub(crate) category: u8, pub(crate) name: String, pub(crate) short_name: String, pub(crate) record_raw: [u8; 64] }
impl MarketInfo {
    pub const fn market(&self) -> ExMarketId { self.market }
    pub const fn category(&self) -> u8 { self.category }
    pub fn name(&self) -> &str { &self.name }
    pub fn short_name(&self) -> &str { &self.short_name }
    pub const fn record_raw(&self) -> &[u8; 64] { &self.record_raw }
}
#[derive(Clone, Debug)]
pub struct InstrumentInfo { pub(crate) category: u8, pub(crate) id: ResponseIdentity, pub(crate) name: String, pub(crate) desc: String, pub(crate) record_raw: [u8; 64] }
impl InstrumentInfo {
    pub const fn category(&self) -> u8 { self.category }
    getters!(id: ResponseIdentity);
    pub const fn market(&self) -> ExMarketId { self.id.market() }
    pub fn code(&self) -> &str { self.id.code() }
    pub fn name(&self) -> &str { &self.name }
    pub fn desc(&self) -> &str { &self.desc }
    pub const fn record_raw(&self) -> &[u8; 64] { &self.record_raw }
}
#[derive(Clone, Debug)]
pub struct BookLevel { pub(crate) price: NativeNumber<WireFloat>, pub(crate) volume: NativeNumber<u32> }
impl BookLevel { getters!(price: NativeNumber<WireFloat>, volume: NativeNumber<u32>); }
#[derive(Clone, Debug)]
pub struct Quote {
    pub(crate) id: ResponseIdentity,
    pub(crate) pre_close: NativeNumber<WireFloat>, pub(crate) open: NativeNumber<WireFloat>,
    pub(crate) high: NativeNumber<WireFloat>, pub(crate) low: NativeNumber<WireFloat>, pub(crate) price: NativeNumber<WireFloat>,
    pub(crate) kaicang: NativeNumber<u32>, pub(crate) zongliang: NativeNumber<u32>, pub(crate) xianliang: NativeNumber<u32>,
    pub(crate) neipan: NativeNumber<u32>, pub(crate) waipan: NativeNumber<u32>, pub(crate) chicang: NativeNumber<u32>,
    pub(crate) bid: [BookLevel; 5], pub(crate) ask: [BookLevel; 5],
    pub(crate) opaque: Vec<Bytes>, pub(crate) record_raw: Option<Bytes>,
}
impl Quote {
    getters!(id: ResponseIdentity, pre_close: NativeNumber<WireFloat>, open: NativeNumber<WireFloat>,
        high: NativeNumber<WireFloat>, low: NativeNumber<WireFloat>, price: NativeNumber<WireFloat>,
        kaicang: NativeNumber<u32>, zongliang: NativeNumber<u32>, xianliang: NativeNumber<u32>,
        neipan: NativeNumber<u32>, waipan: NativeNumber<u32>, chicang: NativeNumber<u32>);
    pub fn bid(&self) -> &[BookLevel; 5] { &self.bid }
    pub fn ask(&self) -> &[BookLevel; 5] { &self.ask }
    pub fn opaque(&self) -> &[Bytes] { &self.opaque }
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
}
#[derive(Clone, Debug)]
pub struct QuoteListRow {
    pub(crate) id: ResponseIdentity, pub(crate) category: QuoteCategory,
    pub(crate) fields: Vec<NamedNativeField>, pub(crate) slot_raw: [u8; 300],
}
impl QuoteListRow {
    getters!(id: ResponseIdentity);
    pub const fn category(&self) -> QuoteCategory { self.category }
    pub fn fields(&self) -> &[NamedNativeField] { &self.fields }
    pub fn field(&self, name: &str) -> Option<&NamedNativeField> { self.fields.iter().find(|field| field.name == name) }
    pub const fn slot_raw(&self) -> &[u8; 300] { &self.slot_raw }
    pub fn unused_tail(&self) -> &[u8] { &self.slot_raw[150..] }
    pub fn hong_kong(&self) -> Option<QuoteListView<'_>> {
        (self.category == QuoteCategory::HongKong).then_some(QuoteListView { row: self })
    }
    pub fn futures(&self) -> Option<QuoteListView<'_>> {
        (self.category == QuoteCategory::Futures).then_some(QuoteListView { row: self })
    }
}
/// Category-scoped view; the source names and native types are authoritative.
#[derive(Clone, Copy, Debug)]
pub struct QuoteListView<'a> { row: &'a QuoteListRow }
impl<'a> QuoteListView<'a> {
    pub fn field(self, name: &str) -> Option<&'a NamedNativeField> { self.row.field(name) }
    pub fn fields(self) -> &'a [NamedNativeField] { self.row.fields() }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarTimeSource { PackedMinute, SyntheticClose }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarLastSlotSource { OrdinaryUninterpreted, RangeSettlement }
#[derive(Clone, Debug)]
pub struct Bar {
    pub(crate) time: EventTime, pub(crate) source_time: EventTime, pub(crate) time_source: BarTimeSource,
    pub(crate) open: NativeNumber<WireFloat>, pub(crate) high: NativeNumber<WireFloat>,
    pub(crate) low: NativeNumber<WireFloat>, pub(crate) close: NativeNumber<WireFloat>,
    pub(crate) position_bits: u32, pub(crate) trade: NativeNumber<u32>,
    pub(crate) last_slot: WireFloat, pub(crate) last_slot_source: BarLastSlotSource,
    pub(crate) record_raw: Option<Bytes>,
}
impl Bar {
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn source_time(&self) -> EventTime { self.source_time }
    pub const fn time_source(&self) -> BarTimeSource { self.time_source }
    getters!(open: NativeNumber<WireFloat>, high: NativeNumber<WireFloat>, low: NativeNumber<WireFloat>, close: NativeNumber<WireFloat>, trade: NativeNumber<u32>);
    pub const fn position_bits(&self) -> u32 { self.position_bits }
    pub fn position(&self) -> NativeNumber<u32> { NativeNumber::unknown(self.position_bits) }
    /// A reinterpretation of the position slot, not another economic observation.
    pub fn amount_view(&self) -> Option<NativeNumber<WireFloat>> {
        (self.last_slot_source == BarLastSlotSource::OrdinaryUninterpreted)
            .then(|| NativeNumber::unknown(WireFloat::from_bits(self.position_bits)))
    }
    pub const fn last_slot(&self) -> WireFloat { self.last_slot }
    pub const fn last_slot_source(&self) -> BarLastSlotSource { self.last_slot_source }
    pub fn settlement(&self) -> Option<NativeNumber<WireFloat>> {
        (self.last_slot_source == BarLastSlotSource::RangeSettlement).then(|| NativeNumber::unknown(self.last_slot))
    }
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
}
#[derive(Clone, Debug)]
pub struct Minute { pub(crate) time: EventTime, pub(crate) raw_time: u16,
    pub(crate) price: NativeNumber<WireFloat>, pub(crate) avg_price: NativeNumber<WireFloat>,
    pub(crate) volume: NativeNumber<u32>, pub(crate) open_interest: NativeNumber<u32>, pub(crate) record_raw: Option<Bytes> }
impl Minute {
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn raw_time(&self) -> u16 { self.raw_time }
    getters!(price: NativeNumber<WireFloat>, avg_price: NativeNumber<WireFloat>, volume: NativeNumber<u32>, open_interest: NativeNumber<u32>);
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
}
#[derive(Clone, Debug)]
pub struct Transaction { pub(crate) time: EventTime, pub(crate) raw_time: u16, pub(crate) direction_raw: u16,
    pub(crate) price_raw: NativeNumber<u32>, pub(crate) volume: NativeNumber<u32>, pub(crate) zengcang: NativeNumber<i32>,
    pub(crate) nature: u16, pub(crate) record_raw: Option<Bytes> }
impl Transaction {
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn raw_time(&self) -> u16 { self.raw_time }
    pub const fn direction_raw(&self) -> u16 { self.direction_raw }
    pub const fn nature(&self) -> u16 { self.nature }
    getters!(price_raw: NativeNumber<u32>, volume: NativeNumber<u32>, zengcang: NativeNumber<i32>);
    pub fn record_raw(&self) -> Option<&Bytes> { self.record_raw.as_ref() }
}

/// Pure decoder output. The owner supplies SourceStamp and its operation byte
/// budget; no fabricated session ID or second metadata definition is created here.
#[derive(Clone, Debug)]
pub struct Decoded<T> {
    pub(crate) value: T, pub(crate) diagnostics: Vec<Diagnostic>, pub(crate) ranges: Vec<(usize, usize)>,
    pub(crate) wire_row_count: Option<u16>, pub(crate) response_offset: Option<u32>,
}
impl<T> Decoded<T> {
    pub fn value(&self) -> &T { &self.value }
    pub fn into_value(self) -> T { self.value }
    pub fn diagnostics(&self) -> &[Diagnostic] { &self.diagnostics }
    pub fn row_ranges(&self) -> &[(usize, usize)] { &self.ranges }
    pub const fn wire_row_count(&self) -> Option<u16> { self.wire_row_count }
    pub const fn response_offset(&self) -> Option<u32> { self.response_offset }
    pub(crate) fn with_wire_page(mut self, count: u16, offset: Option<u32>) -> Self {
        self.wire_row_count = Some(count); self.response_offset = offset; self
    }
    pub fn with_meta(self, source: SourceStamp, raw: Option<RawData>) -> (T, ExtensionMeta) {
        (self.value, ResultMeta::new(source, self.diagnostics, raw))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageCompletion { EndOfData, RequestedCount, MoreAvailable, Unknown, Incomplete }
#[derive(Clone, Debug)]
pub struct PageResult<T> { rows: Vec<T>, server_total: Option<u64>, next_offset: Option<u32>, completion: PageCompletion, meta: ExtensionMeta }
impl<T> PageResult<T> {
    pub fn rows(&self) -> &[T] { &self.rows }
    pub const fn server_total(&self) -> Option<u64> { self.server_total }
    pub const fn next_offset(&self) -> Option<u32> { self.next_offset }
    pub const fn completion(&self) -> PageCompletion { self.completion }
    pub fn meta(&self) -> &ExtensionMeta { &self.meta }
    // Stage B must supply command-specific completion evidence. Decoders never
    // construct a page from row count alone: neither short nor empty replies
    // prove EOF, and an unfilled request does not prove more data exists.
    pub(crate) fn new(rows: Vec<T>, server_total: Option<u64>, next_offset: Option<u32>, completion: PageCompletion, meta: ExtensionMeta) -> crate::Result<Self> {
        if completion == PageCompletion::Incomplete && meta.diagnostics().is_empty() {
            return Err(super::invalid("incomplete EX page requires a diagnostic"));
        }
        Ok(Self { rows, server_total, next_offset, completion, meta })
    }
}
/// Project one response into the shared raw container. The caller supplies the
/// remaining operation byte allowance; this helper owns no separate budget.
/// Bytes and row ranges refer to the same allocation, including RetainWire.
pub fn project_raw(payload: Bytes, wire: Option<Bytes>, ranges: &[(usize, usize)], policy: RawPolicy, remaining_bytes: usize) -> crate::Result<Option<RawData>> {
    if policy == RawPolicy::Omit { return Ok(None); }
    for &(start, end) in ranges {
        if start > end || end > payload.len() { return Err(super::protocol("invalid EX raw row range")); }
    }
    let frames = if policy == RawPolicy::RetainWire {
        Some(vec![wire.ok_or_else(|| super::protocol("RetainWire requires the complete EX wire frame"))?])
    } else { None };
    let wire_bytes = frames.as_ref().map_or(0, |items| items[0].len());
    let retained_bytes = payload.len().checked_add(wire_bytes).ok_or_else(|| super::protocol("EX raw byte count overflow"))?;
    if retained_bytes > remaining_bytes {
        return Err(crate::Error::new(crate::ErrorKind::LimitExceeded, "EX raw response exceeds remaining byte allowance").with_context("domain", "ex"));
    }
    let row_ranges = ranges.iter().map(|&(start, end)| (0, start, end)).collect();
    Ok(Some(RawData::new(vec![payload], frames, row_ranges)))
}

pub(crate) fn optional_raw(body: &Bytes, start: usize, end: usize, policy: RawPolicy) -> Option<Bytes> {
    if policy == RawPolicy::Omit { None } else { Some(body.slice(start..end)) }
}
