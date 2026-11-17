//! Source-native simple MAC responses. Private construction prevents callers
//! from fabricating validated records; unresolved units remain explicit.
use encoding_rs::GBK;
use serde_json::Value;
use std::sync::Arc;
use crate::types::ResultMeta;
use crate::{EventTime, TradingDate};
use super::models::{FieldMeaning, MacMarketId, WireFloat};

/// Independent source counters. `returned` alone supplies no EOF evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KlineOffsetInfo { pub(crate) total: u32, pub(crate) returned: u32 }
impl KlineOffsetInfo {
    pub const fn total(&self) -> u32 { self.total }
    pub const fn returned(&self) -> u32 { self.returned }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireText<const N: usize> { raw: [u8; N], text: String, replaced: bool }
impl<const N: usize> WireText<N> {
    pub(crate) fn from_wire(raw: [u8; N]) -> Self {
        // Fixed-width fields may contain padding. Retain all bytes and make
        // replacement decoding observable rather than treating it as an ID.
        let end = raw.iter().rposition(|byte| *byte != 0).map_or(0, |index| index + 1);
        let (text, _, replaced) = GBK.decode(&raw[..end]);
        Self { text: text.into_owned(), raw, replaced }
    }
    pub fn as_str(&self) -> &str { &self.text }
    pub const fn raw(&self) -> &[u8; N] { &self.raw }
    pub const fn replacement_used(&self) -> bool { self.replaced }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseIdentity { pub(crate) market: MacMarketId, pub(crate) code: WireText<22> }
impl ResponseIdentity {
    pub const fn market(&self) -> MacMarketId { self.market }
    pub fn code(&self) -> &WireText<22> { &self.code }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeNumber<T> { value: T, meaning: FieldMeaning }
impl<T> NativeNumber<T> {
    pub(crate) fn unknown(value: T) -> Self { Self { value, meaning: FieldMeaning::unknown() } }
    pub fn value(&self) -> &T { &self.value }
    pub fn meaning(&self) -> &FieldMeaning { &self.meaning }
}
macro_rules! refs {
    ($($field:ident: $ty:ty),* $(,)?) => { $(pub fn $field(&self) -> &$ty { &self.$field })* };
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tick {
    pub(crate) minutes: u16, pub(crate) time: EventTime,
    pub(crate) price: NativeNumber<WireFloat>, pub(crate) avg: NativeNumber<WireFloat>,
    pub(crate) vol: NativeNumber<u32>, pub(crate) momentum: Option<NativeNumber<WireFloat>>,
    pub(crate) reserved: Option<u16>,
}
impl Tick {
    pub const fn minutes_raw(&self) -> u16 { self.minutes }
    pub const fn time(&self) -> EventTime { self.time }
    refs!(price: NativeNumber<WireFloat>, avg: NativeNumber<WireFloat>, vol: NativeNumber<u32>);
    pub fn momentum(&self) -> Option<&NativeNumber<WireFloat>> { self.momentum.as_ref() }
    pub const fn reserved(&self) -> Option<u16> { self.reserved }
}
/// Shared 120-byte snapshot tail of 0x122D and 0x123E, including unknown bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChartSnapshot {
    pub(crate) name: WireText<44>, pub(crate) decimal: u8, pub(crate) category: u16,
    pub(crate) vol_unit: WireFloat, pub(crate) reserved: [u8; 5],
    pub(crate) date_raw: u32, pub(crate) time_raw: u32,
    pub(crate) ohlc: [NativeNumber<WireFloat>; 5], pub(crate) momentum: NativeNumber<WireFloat>,
    pub(crate) vol: NativeNumber<u32>, pub(crate) amount: NativeNumber<WireFloat>,
    pub(crate) tail_pad: [u8; 12], pub(crate) turnover: NativeNumber<WireFloat>,
    pub(crate) avg: NativeNumber<WireFloat>, pub(crate) industry: u32,
}
impl ChartSnapshot {
    refs!(name: WireText<44>, momentum: NativeNumber<WireFloat>, vol: NativeNumber<u32>,
        amount: NativeNumber<WireFloat>, turnover: NativeNumber<WireFloat>, avg: NativeNumber<WireFloat>);
    pub const fn decimal(&self) -> u8 { self.decimal }
    pub const fn category(&self) -> u16 { self.category }
    pub const fn volume_unit_raw(&self) -> WireFloat { self.vol_unit }
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn time_raw(&self) -> u32 { self.time_raw }
    pub fn pre_close(&self) -> &NativeNumber<WireFloat> { &self.ohlc[0] }
    pub fn open(&self) -> &NativeNumber<WireFloat> { &self.ohlc[1] }
    pub fn high(&self) -> &NativeNumber<WireFloat> { &self.ohlc[2] }
    pub fn low(&self) -> &NativeNumber<WireFloat> { &self.ohlc[3] }
    pub fn close(&self) -> &NativeNumber<WireFloat> { &self.ohlc[4] }
    pub const fn reserved(&self) -> &[u8; 5] { &self.reserved }
    pub const fn tail_pad(&self) -> &[u8; 12] { &self.tail_pad }
    pub const fn industry(&self) -> u32 { self.industry }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickChart {
    pub(crate) id: ResponseIdentity, pub(crate) date_raw: u32, pub(crate) reserved: u8,
    pub(crate) ref_price: NativeNumber<WireFloat>, pub(crate) snapshot: ChartSnapshot,
    pub(crate) ticks: Vec<Tick>,
}
impl TickChart {
    refs!(id: ResponseIdentity, ref_price: NativeNumber<WireFloat>, snapshot: ChartSnapshot);
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn reserved(&self) -> u8 { self.reserved }
    pub fn ticks(&self) -> &[Tick] { &self.ticks }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickDay { pub(crate) date: TradingDate, pub(crate) pre_close: NativeNumber<WireFloat>, pub(crate) ticks: Vec<Tick> }
impl TickDay {
    pub const fn date(&self) -> TradingDate { self.date }
    refs!(pre_close: NativeNumber<WireFloat>);
    pub fn ticks(&self) -> &[Tick] { &self.ticks }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiTickChart {
    pub(crate) id: ResponseIdentity, pub(crate) dates_raw: [u32; 5],
    pub(crate) pre_closes_raw: [WireFloat; 5], pub(crate) send_last: u8,
    pub(crate) page_size: u16, pub(crate) total: u16,
    pub(crate) snapshot: ChartSnapshot, pub(crate) days: Vec<TickDay>,
}
impl MultiTickChart {
    refs!(id: ResponseIdentity, snapshot: ChartSnapshot);
    pub const fn dates_raw(&self) -> &[u32; 5] { &self.dates_raw }
    pub const fn pre_closes_raw(&self) -> &[WireFloat; 5] { &self.pre_closes_raw }
    pub const fn send_last(&self) -> u8 { self.send_last }
    pub const fn page_size(&self) -> u16 { self.page_size }
    pub const fn total(&self) -> u16 { self.total }
    pub fn days(&self) -> &[TickDay] { &self.days }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolInfo {
    pub(crate) id: ResponseIdentity, pub(crate) name: WireText<44>,
    pub(crate) date_raw: u32, pub(crate) time_raw: u32, pub(crate) time: Option<EventTime>,
    pub(crate) activity: u32, pub(crate) ohlc: [NativeNumber<WireFloat>; 5],
    pub(crate) momentum: NativeNumber<WireFloat>, pub(crate) vol: NativeNumber<u32>,
    pub(crate) amount: NativeNumber<WireFloat>, pub(crate) inside: NativeNumber<u32>,
    pub(crate) outside: NativeNumber<u32>, pub(crate) turnover: NativeNumber<WireFloat>,
    pub(crate) avg: NativeNumber<WireFloat>, pub(crate) prefix: [u8; 8],
    pub(crate) reserved: [u8; 20], pub(crate) extra: [u8; 46], pub(crate) tail: Vec<u8>,
}
impl SymbolInfo {
    refs!(id: ResponseIdentity, name: WireText<44>, momentum: NativeNumber<WireFloat>,
        vol: NativeNumber<u32>, amount: NativeNumber<WireFloat>, turnover: NativeNumber<WireFloat>, avg: NativeNumber<WireFloat>);
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn time_raw(&self) -> u32 { self.time_raw }
    pub const fn time(&self) -> Option<EventTime> { self.time }
    pub const fn activity(&self) -> u32 { self.activity }
    pub fn pre_close(&self) -> &NativeNumber<WireFloat> { &self.ohlc[0] }
    pub fn open(&self) -> &NativeNumber<WireFloat> { &self.ohlc[1] }
    pub fn high(&self) -> &NativeNumber<WireFloat> { &self.ohlc[2] }
    pub fn low(&self) -> &NativeNumber<WireFloat> { &self.ohlc[3] }
    pub fn close(&self) -> &NativeNumber<WireFloat> { &self.ohlc[4] }
    pub fn inside_volume(&self) -> &NativeNumber<u32> { &self.inside }
    pub fn outside_volume(&self) -> &NativeNumber<u32> { &self.outside }
    pub const fn prefix(&self) -> &[u8; 8] { &self.prefix }
    pub const fn reserved(&self) -> &[u8; 20] { &self.reserved }
    pub const fn extra_raw(&self) -> &[u8; 46] { &self.extra }
    pub fn unknown_tail(&self) -> &[u8] { &self.tail }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardSide {
    pub(crate) id: ResponseIdentity, pub(crate) name: WireText<44>, pub(crate) reserved: [u8; 16],
    pub(crate) price: NativeNumber<WireFloat>, pub(crate) rise_speed: NativeNumber<WireFloat>,
    pub(crate) pre_close: NativeNumber<WireFloat>,
}
impl BoardSide {
    refs!(id: ResponseIdentity, name: WireText<44>, price: NativeNumber<WireFloat>,
        rise_speed: NativeNumber<WireFloat>, pre_close: NativeNumber<WireFloat>);
    pub const fn reserved(&self) -> &[u8; 16] { &self.reserved }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardInfo { pub(crate) board: BoardSide, pub(crate) leader: BoardSide, pub(crate) meta: Arc<ResultMeta> }
impl BoardInfo {
    refs!(board: BoardSide, leader: BoardSide);
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn set_meta(&mut self, value: Arc<ResultMeta>) { self.meta = value; }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardPage { pub(crate) paired_count: u16, pub(crate) total_raw: u16, pub(crate) items: Vec<BoardInfo> }
impl BoardPage {
    pub const fn paired_count(&self) -> u16 { self.paired_count }
    pub const fn total_raw(&self) -> u16 { self.total_raw }
    pub fn items(&self) -> &[BoardInfo] { &self.items }
    pub fn into_items(self) -> Vec<BoardInfo> { self.items }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Auction {
    pub(crate) time_seconds: u32, pub(crate) time: EventTime,
    pub(crate) price: NativeNumber<WireFloat>, pub(crate) matched: NativeNumber<u32>, pub(crate) unmatched: NativeNumber<i32>,
}
impl Auction {
    pub const fn time_seconds(&self) -> u32 { self.time_seconds }
    pub const fn time(&self) -> EventTime { self.time }
    refs!(price: NativeNumber<WireFloat>, matched: NativeNumber<u32>, unmatched: NativeNumber<i32>);
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unusual {
    pub(crate) index: u16, pub(crate) id: ResponseIdentity, pub(crate) name: String,
    pub(crate) time: EventTime, pub(crate) unusual_type: u8,
    pub(crate) description: String, pub(crate) value: String, pub(crate) raw: [u8; 32],
    pub(crate) meta: Arc<ResultMeta>,
}
impl Unusual {
    pub const fn index(&self) -> u16 { self.index }
    refs!(id: ResponseIdentity);
    pub fn name(&self) -> &str { &self.name }
    pub const fn time(&self) -> EventTime { self.time }
    pub const fn unusual_type(&self) -> u8 { self.unusual_type }
    pub fn description(&self) -> &str { &self.description }
    pub fn value(&self) -> &str { &self.value }
    pub const fn raw(&self) -> &[u8; 32] { &self.raw }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn set_meta(&mut self, value: Arc<ResultMeta>) { self.meta = value; }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionWindow { pub(crate) open_minutes: u16, pub(crate) close_minutes: u16 }
impl SessionWindow {
    pub const fn open_minutes(&self) -> u16 { self.open_minutes }
    pub const fn close_minutes(&self) -> u16 { self.close_minutes }
}
/// Dates that failed calendar parsing remain observable; they cannot make a
/// host DataReady or silently become a calendar date.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedDate { pub(crate) raw: u32, pub(crate) parsed: Option<TradingDate> }
impl ObservedDate {
    pub const fn raw(&self) -> u32 { self.raw }
    pub const fn parsed(&self) -> Option<TradingDate> { self.parsed }
    pub fn source_text(&self) -> String { format!("{}-{:02}-{:02}", self.raw / 10000, self.raw % 10000 / 100, self.raw % 100) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerSession {
    pub(crate) today: ObservedDate, pub(crate) last: ObservedDate,
    pub(crate) sessions_1: [SessionWindow; 4], pub(crate) sessions_2: [SessionWindow; 4],
    pub(crate) market_param_1: Option<u32>, pub(crate) market_param_2: Option<u32>,
    pub(crate) prefix: [u8; 22], pub(crate) timestamp_1: u32, pub(crate) timestamp_2: u32,
    pub(crate) flag: u8, pub(crate) tail: Vec<u8>,
}
impl ServerSession {
    pub const fn today(&self) -> ObservedDate { self.today }
    pub const fn last_trading_day(&self) -> ObservedDate { self.last }
    pub const fn sessions_1(&self) -> &[SessionWindow; 4] { &self.sessions_1 }
    pub const fn sessions_2(&self) -> &[SessionWindow; 4] { &self.sessions_2 }
    pub const fn market_param_1(&self) -> Option<u32> { self.market_param_1 }
    pub const fn market_param_2(&self) -> Option<u32> { self.market_param_2 }
    pub const fn prefix(&self) -> &[u8; 22] { &self.prefix }
    pub const fn timestamp_1(&self) -> u32 { self.timestamp_1 }
    pub const fn timestamp_2(&self) -> u32 { self.timestamp_2 }
    pub const fn flag(&self) -> u8 { self.flag }
    pub fn unknown_tail(&self) -> &[u8] { &self.tail }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoodsItem {
    pub(crate) name: WireText<23>, pub(crate) category: u16, pub(crate) u: u16,
    pub(crate) index: u32, pub(crate) switch: u8, pub(crate) code: [WireFloat; 3],
    pub(crate) c1: u16, pub(crate) c2: u16,
}
impl GoodsItem {
    refs!(name: WireText<23>);
    pub const fn category(&self) -> u16 { self.category }
    pub const fn u(&self) -> u16 { self.u }
    pub const fn index(&self) -> u32 { self.index }
    pub const fn switch(&self) -> u8 { self.switch }
    pub const fn code(&self) -> &[WireFloat; 3] { &self.code }
    pub const fn c1(&self) -> u16 { self.c1 }
    pub const fn c2(&self) -> u16 { self.c2 }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMeta {
    pub(crate) offset: u32, pub(crate) size: u32, pub(crate) flag: i8,
    pub(crate) hash: [u8; 32], pub(crate) tail: Vec<u8>,
}
impl FileMeta {
    pub const fn offset(&self) -> u32 { self.offset }
    pub const fn size(&self) -> u32 { self.size }
    pub const fn flag(&self) -> i8 { self.flag }
    pub const fn hash_raw(&self) -> &[u8; 32] { &self.hash }
    /// The wire supplies a string, without evidence for a digest algorithm.
    pub fn hash(&self) -> String {
        String::from_utf8_lossy(&self.hash).trim_end_matches('\0').into()
    }
    pub fn unknown_tail(&self) -> &[u8] { &self.tail }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadChunk { pub(crate) prefix: [u8; 8], pub(crate) bytes: Vec<u8> }
impl DownloadChunk {
    pub const fn prefix(&self) -> &[u8; 8] { &self.prefix }
    pub fn bytes(&self) -> &[u8] { &self.bytes }
    pub fn into_bytes(self) -> Vec<u8> { self.bytes }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonPresence { Missing, Null, Present, Derived }
#[derive(Clone, Debug, PartialEq)]
pub struct JsonNumber { pub(crate) value: Option<f64>, pub(crate) presence: JsonPresence, pub(crate) source: Option<Value> }
impl JsonNumber {
    pub const fn value(&self) -> Option<f64> { self.value }
    pub const fn presence(&self) -> JsonPresence { self.presence }
    pub fn source(&self) -> Option<&Value> { self.source.as_ref() }
    pub fn meaning(&self) -> FieldMeaning { FieldMeaning::unknown() }
}
#[derive(Clone, Debug, PartialEq)]
pub struct BelongBoard {
    pub(crate) board_type: u16, pub(crate) market: MacMarketId,
    pub(crate) code: String, pub(crate) name: String, pub(crate) close: JsonNumber,
    pub(crate) pre_close: JsonNumber, pub(crate) source: Vec<Value>,
}
impl BelongBoard {
    pub const fn board_type(&self) -> u16 { self.board_type }
    pub const fn market(&self) -> MacMarketId { self.market }
    pub fn code(&self) -> &str { &self.code }
    pub fn name(&self) -> &str { &self.name }
    refs!(close: JsonNumber, pre_close: JsonNumber);
    pub fn source(&self) -> &[Value] { &self.source }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CapitalFlow {
    pub(crate) main: [JsonNumber; 3], pub(crate) small: [JsonNumber; 3],
    pub(crate) mid: [JsonNumber; 3], pub(crate) large: [JsonNumber; 3],
    pub(crate) source: Vec<Vec<Value>>,
}
impl CapitalFlow {
    // No date is supplied by this payload; do not fabricate today's date.
    pub const fn date(&self) -> Option<TradingDate> { None }
    pub fn main_in(&self) -> &JsonNumber { &self.main[0] }
    pub fn main_out(&self) -> &JsonNumber { &self.main[1] }
    pub fn main_net(&self) -> &JsonNumber { &self.main[2] }
    pub fn small_in(&self) -> &JsonNumber { &self.small[0] }
    pub fn small_out(&self) -> &JsonNumber { &self.small[1] }
    pub fn small_net(&self) -> &JsonNumber { &self.small[2] }
    pub fn mid_in(&self) -> &JsonNumber { &self.mid[0] }
    pub fn mid_out(&self) -> &JsonNumber { &self.mid[1] }
    pub fn mid_net(&self) -> &JsonNumber { &self.mid[2] }
    pub fn large_in(&self) -> &JsonNumber { &self.large[0] }
    pub fn large_out(&self) -> &JsonNumber { &self.large[1] }
    pub fn large_net(&self) -> &JsonNumber { &self.large[2] }
    pub fn source(&self) -> &[Vec<Value>] { &self.source }
}
