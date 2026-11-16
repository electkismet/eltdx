//! Eleven EX request/response codecs. No I/O, retries or implicit pagination.
//! Layout authority: src/eltdx/ex/commands at the T24 review baseline.
use bytes::Bytes;
use encoding_rs::GBK;
use crate::{Result, RawPolicy};
use crate::types::{Diagnostic, EnrichmentStatus};
use super::{models::*, query::*, protocol, time};

#[derive(Clone, Debug)]
pub struct Request { command: u16, wire: Bytes }
impl Request {
    pub const fn command(&self) -> u16 { self.command }
    pub fn wire(&self) -> &Bytes { &self.wire }
}
fn request(header: [u8; 12], data: &[u8]) -> Request {
    let command = u16::from_le_bytes([header[10], header[11]]);
    let mut wire = Vec::with_capacity(12 + data.len());
    wire.extend_from_slice(&header); wire.extend_from_slice(data);
    Request { command, wire: wire.into() }
}
fn identity(id: &ExInstrumentId) -> Vec<u8> {
    let mut body = Vec::with_capacity(10);
    body.push(id.market().get()); body.extend_from_slice(&id.code().wire_bytes()); body
}
pub fn markets_request() -> Request { request([1,2,0x48,0x69,0,1,2,0,2,0,0xf4,0x23], &[]) }
pub fn instrument_count_request() -> Request { request([1,3,0x48,0x66,0,1,2,0,2,0,0xf0,0x23], &[]) }
pub fn instruments_request(query: &InstrumentsQuery) -> Request {
    let mut body = query.start().to_le_bytes().to_vec(); body.extend_from_slice(&query.count().get().to_le_bytes());
    request([1,4,0x48,0x67,0,1,8,0,8,0,0xf5,0x23], &body)
}
pub fn quote_request(query: &QuoteQuery) -> Request {
    request([1,1,8,2,2,1,12,0,12,0,0xfa,0x23], &identity(query.id()))
}
pub fn quote_list_request(query: &QuoteListQuery) -> Request {
    // Category affects response interpretation, not the source request layout.
    let mut body = vec![query.market().get(), 0, 0];
    body.extend_from_slice(&query.start().to_le_bytes()); body.extend_from_slice(&query.count().get().to_le_bytes());
    body.extend_from_slice(&1_u16.to_le_bytes());
    request([1,0xc1,6,0x0b,0,2,11,0,11,0,0,0x24], &body)
}
pub fn bars_request(query: &BarsQuery) -> Request {
    let mut body = identity(query.id()); body.extend_from_slice(&query.period().wire().to_le_bytes());
    body.extend_from_slice(&1_u16.to_le_bytes()); body.extend_from_slice(&query.start().to_le_bytes());
    body.extend_from_slice(&query.count().get().to_le_bytes());
    request([1,1,8,0x6a,1,1,22,0,22,0,0xff,0x23], &body)
}
pub fn bars_range_request(query: &BarsRangeQuery, sequence: &super::frame::SequenceLease<'_>) -> Request {
    let mut body = identity(query.id()); body.extend_from_slice(&7_u16.to_le_bytes());
    body.extend_from_slice(&encode_date(query.start_date()).to_le_bytes()); body.extend_from_slice(&encode_date(query.end_date()).to_le_bytes());
    request([1,sequence.sequence(),0x38,0x92,0,1,22,0,22,0,0x0d,0x24], &body)
}
pub fn minutes_request(query: &MinutesQuery) -> Request {
    request([1,7,8,0,1,1,12,0,12,0,0x0b,0x24], &identity(query.id()))
}
pub fn history_minutes_request(query: &HistoryMinutesQuery) -> Request {
    let mut body = encode_date(query.date()).to_le_bytes().to_vec(); body.extend_from_slice(&identity(query.id()));
    request([1,1,0x30,0,1,1,16,0,16,0,0x0c,0x24], &body)
}
pub fn transactions_request(query: &TransactionsQuery) -> Request {
    let mut body = identity(query.id()); body.extend_from_slice(&query.start().to_le_bytes()); body.extend_from_slice(&query.count().get().to_le_bytes());
    request([1,1,8,0,3,1,18,0,18,0,0xfc,0x23], &body)
}
pub fn history_transactions_request(query: &HistoryTransactionsQuery) -> Request {
    let mut body = encode_date(query.date()).to_le_bytes().to_vec(); body.extend_from_slice(&identity(query.id()));
    body.extend_from_slice(&query.start().to_le_bytes()); body.extend_from_slice(&query.count().get().to_le_bytes());
    request([1,1,0x30,0,2,1,22,0,22,0,6,0x24], &body)
}

fn slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    if data.len() > super::frame::WIRE_BODY_LIMIT {
        return Err(crate::Error::new(crate::ErrorKind::LimitExceeded, "EX payload exceeds u16 wire limit").with_context("domain", "ex"));
    }
    data.get(offset..offset.checked_add(len).ok_or_else(|| protocol("EX offset overflow"))?)
        .ok_or_else(|| protocol("truncated EX payload").with_context("offset", format!("{offset}")))
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16> { let b = slice(data, offset, 2)?; Ok(u16::from_le_bytes([b[0], b[1]])) }
fn u32_at(data: &[u8], offset: usize) -> Result<u32> { let b = slice(data, offset, 4)?; Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]])) }
fn float_at(data: &[u8], offset: usize) -> Result<WireFloat> { Ok(WireFloat::from_bits(u32_at(data, offset)?)) }
fn array<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N]> {
    slice(data, offset, N)?.try_into().map_err(|_| protocol("EX fixed record length mismatch"))
}
fn counted(data: &[u8], header: usize, count_offset: usize, width: usize) -> Result<(u16, usize)> {
    slice(data, 0, header)?;
    let count = u16_at(data, count_offset)?;
    let end = header.checked_add(usize::from(count).checked_mul(width).ok_or_else(|| protocol("EX row count overflow"))?)
        .ok_or_else(|| protocol("EX row count overflow"))?;
    slice(data, header, end - header)?;
    Ok((count, end))
}
fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new("ex", "decode", EnrichmentStatus::Unavailable, None, Some(message.to_owned()))
}
fn text(raw: &[u8], gbk: bool, trim_leading: bool, diagnostics: &mut Vec<Diagnostic>) -> String {
    let mut bytes = raw;
    while bytes.last() == Some(&0) { bytes = &bytes[..bytes.len() - 1]; }
    if trim_leading { while bytes.first() == Some(&0) { bytes = &bytes[1..]; } }
    let (value, replaced) = if gbk {
        let (value, _, replaced) = GBK.decode(bytes); (value.into_owned(), replaced)
    } else {
        let value = String::from_utf8_lossy(bytes);
        let replaced = matches!(&value, std::borrow::Cow::Owned(_)); (value.into_owned(), replaced)
    };
    if replaced { diagnostics.push(diagnostic(if gbk { "GBK replacement decoding" } else { "UTF-8 replacement decoding" })); }
    value
}
fn response_id(raw: [u8; 10], gbk: bool, trim_leading: bool, diagnostics: &mut Vec<Diagnostic>) -> ResponseIdentity {
    ResponseIdentity { market: ExMarketId::from_wire(raw[0]), code: text(&raw[1..], gbk, trim_leading, diagnostics), raw }
}
fn finish<T>(value: T, mut diagnostics: Vec<Diagnostic>, ranges: Vec<(usize, usize)>, end: usize, len: usize) -> Decoded<T> {
    if end < len { diagnostics.push(diagnostic("uninterpreted EX response trailer retained by payload policy")); }
    Decoded { value, diagnostics, ranges, wire_row_count: None, response_offset: None }
}
pub fn decode_markets(body: Bytes) -> Result<Decoded<Vec<MarketInfo>>> {
    let (count, end) = counted(&body, 2, 0, 64)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut diagnostics = Vec::new(); let mut ranges = Vec::new();
    for index in 0..usize::from(count) {
        let offset = 2 + index * 64; let raw: [u8; 64] = array(&body, offset)?;
        // Source explicitly treats zero category + zero market as padding.
        if raw[0] == 0 && raw[33] == 0 { continue; }
        rows.push(MarketInfo { category: raw[0], market: ExMarketId::from_wire(raw[33]),
            name: text(&raw[1..33], true, false, &mut diagnostics), short_name: text(&raw[34..36], true, false, &mut diagnostics), record_raw: raw });
        ranges.push((offset, offset + 64));
    }
    Ok(finish(rows, diagnostics, ranges, end, body.len()).with_wire_page(count, None))
}
pub fn decode_instrument_count(body: Bytes) -> Result<Decoded<u32>> {
    let value = u32_at(&body, 19)?;
    Ok(finish(value, Vec::new(), Vec::new(), 23, body.len()))
}
pub fn decode_instruments(body: Bytes) -> Result<Decoded<Vec<InstrumentInfo>>> {
    let (count, end) = counted(&body, 6, 4, 64)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut diagnostics = Vec::new(); let mut ranges = Vec::new();
    for index in 0..usize::from(count) {
        let offset = 6 + index * 64; let raw: [u8; 64] = array(&body, offset)?;
        let mut id = [0; 10]; id[0] = raw[1]; id[1..].copy_from_slice(&raw[5..14]);
        rows.push(InstrumentInfo { category: raw[0], id: response_id(id, true, false, &mut diagnostics),
            name: text(&raw[14..31], true, false, &mut diagnostics), desc: text(&raw[31..40], true, false, &mut diagnostics), record_raw: raw });
        ranges.push((offset, offset + 64));
    }
    Ok(finish(rows, diagnostics, ranges, end, body.len()).with_wire_page(count, Some(u32_at(&body, 0)?)))
}
pub fn decode_quote(body: Bytes, policy: RawPolicy) -> Result<Decoded<Option<Quote>>> {
    // No captured absence marker exists. Even an empty body is not evidence of
    // a missing instrument; keep it a strict protocol error until established.
    slice(&body, 0, 150)?;
    let mut diagnostics = Vec::new(); let id = response_id(array(&body, 0)?, false, false, &mut diagnostics);
    let float = |slot: usize| -> Result<NativeNumber<WireFloat>> { Ok(NativeNumber::unknown(float_at(&body, 14 + slot * 4)?)) };
    let integer = |slot: usize| -> Result<NativeNumber<u32>> { Ok(NativeNumber::unknown(u32_at(&body, 14 + slot * 4)?)) };
    let level = |p: usize, v: usize| -> Result<BookLevel> { Ok(BookLevel { price: float(p)?, volume: integer(v)? }) };
    let mut opaque = vec![Bytes::copy_from_slice(&body[10..14])];
    for slot in [6, 9, 12] { opaque.push(Bytes::copy_from_slice(&body[14 + slot * 4..18 + slot * 4])); }
    if body.len() > 150 { opaque.push(Bytes::copy_from_slice(&body[150..])); }
    let quote = Quote { id, pre_close: float(0)?, open: float(1)?, high: float(2)?, low: float(3)?, price: float(4)?,
        kaicang: integer(5)?, zongliang: integer(7)?, xianliang: integer(8)?, neipan: integer(10)?, waipan: integer(11)?, chicang: integer(13)?,
        bid: [level(14,19)?,level(15,20)?,level(16,21)?,level(17,22)?,level(18,23)?],
        ask: [level(24,29)?,level(25,30)?,level(26,31)?,level(27,32)?,level(28,33)?], opaque,
        record_raw: optional_raw(&body, 0, body.len(), policy) };
    Ok(finish(Some(quote), diagnostics, vec![(0,body.len())], body.len(), body.len()))
}
// (source name, 4-byte slot index, float interpretation). The Python futures
// parser's unpack target has 34 names for a 35-slot format. Preserve the known
// offsets without executing that inconsistent tuple assignment; slot 34 remains
// opaque together with the other unnamed slots. All other slots are
// still available byte-for-byte in QuoteListRow::slot_raw.
const FUTURES: &[(&str, u8, bool)] = &[
    ("BiShu",0,false),("ZuoJie",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("MaiChu",5,true),
    ("KaiCang",6,false),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),
    ("NeiPan",11,false),("WaiPan",12,false),("ChiCangLiang",14,false),("MaiRuJia",15,true),
    ("MaiRuLiang",20,false),("MaiChuJia",25,true),("MaiChuLiang",30,false),
];
const HONG_KONG: &[(&str, u8, bool)] = &[
    ("HuoYueDu",0,false),("ZuoShou",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("XianJia",5,true),
    ("MaiRuJia",7,true),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),("Nei",13,false),("Wai",14,false),
    ("MaiRuJia1",15,true),("MaiRuJia2",16,true),("MaiRuJia3",17,true),("MaiRuJia4",18,true),("MaiRuJia5",19,true),
    ("MaiRuLiang1",20,false),("MaiRuLiang2",21,false),("MaiRuLiang3",22,false),("MaiRuLiang4",23,false),("MaiRuLiang5",24,false),
    ("MaiChuJia1",25,true),("MaiChuJia2",26,true),("MaiChuJia3",27,true),("MaiChuJia4",28,true),("MaiChuJia5",29,true),
    ("MaiChuLiang1",30,false),("MaiChuLiang2",31,false),("MaiChuLiang3",32,false),("MaiChuLiang4",33,false),("MaiChuLiang5",34,false),
];
pub fn decode_quote_list(body: Bytes, category: QuoteCategory) -> Result<Decoded<Vec<QuoteListRow>>> {
    let (count, end) = counted(&body, 2, 0, 300)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut diagnostics = Vec::new(); let mut ranges = Vec::new();
    let schema = match category { QuoteCategory::HongKong => HONG_KONG, QuoteCategory::Futures => FUTURES };
    for index in 0..usize::from(count) {
        let offset = 2 + index * 300; let raw: [u8; 300] = array(&body, offset)?;
        let id = response_id(array(&raw, 0)?, true, true, &mut diagnostics);
        let mut fields = Vec::with_capacity(schema.len());
        for &(name, slot, is_float) in schema {
            let bits = u32_at(&raw, 10 + usize::from(slot) * 4)?;
            fields.push(NamedNativeField { name, slot, value: if is_float { NativeValue::Float(WireFloat::from_bits(bits)) } else { NativeValue::Unsigned(bits) }, meaning: FieldMeaning::unknown() });
        }
        rows.push(QuoteListRow { id, category, fields, slot_raw: raw }); ranges.push((offset,offset + 300));
    }
    Ok(finish(rows, diagnostics, ranges, end, body.len()).with_wire_page(count, None))
}
pub fn decode_bars(body: Bytes, query: &BarsQuery, policy: RawPolicy) -> Result<Decoded<Vec<Bar>>> {
    decode_bar_records(body, 20, 18, query.period(), query.bar_time(), false, policy)
}
pub fn decode_bars_range(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Bar>>> {
    decode_bar_records(body, 14, 12, ExPeriod::Minute1, BarTime::Start, true, policy)
}
fn decode_bar_records(body: Bytes, header: usize, count_offset: usize, period: ExPeriod, alignment: BarTime, range: bool, policy: RawPolicy) -> Result<Decoded<Vec<Bar>>> {
    let (count, end) = counted(&body, header, count_offset, 32)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut ranges = Vec::new();
    for index in 0..usize::from(count) {
        let offset = header + index * 32;
        let (clock, source_time, time_source) = time::bar_time(array(&body, offset)?, period, alignment)?;
        rows.push(Bar { time: clock, source_time, time_source,
            open: NativeNumber::unknown(float_at(&body,offset + 4)?), high: NativeNumber::unknown(float_at(&body,offset + 8)?),
            low: NativeNumber::unknown(float_at(&body,offset + 12)?), close: NativeNumber::unknown(float_at(&body,offset + 16)?),
            position_bits: u32_at(&body,offset + 20)?, trade: NativeNumber::unknown(u32_at(&body,offset + 24)?),
            last_slot: float_at(&body,offset + 28)?, last_slot_source: if range { BarLastSlotSource::RangeSettlement } else { BarLastSlotSource::OrdinaryUninterpreted },
            record_raw: optional_raw(&body,offset,offset + 32,policy) });
        ranges.push((offset,offset + 32));
    }
    let diagnostics = if period.has_known_calendar_semantics() { Vec::new() } else {
        vec![diagnostic("unknown explicit wire period: date layout follows the source decoder; time semantics remain unverified")]
    };
    Ok(finish(rows, diagnostics, ranges, end, body.len()).with_wire_page(count, None))
}
pub fn decode_minutes(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Minute>>> { decode_minute_records(body,12,10,policy) }
pub fn decode_history_minutes(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Minute>>> { decode_minute_records(body,20,18,policy) }
fn decode_minute_records(body: Bytes, header: usize, count_offset: usize, policy: RawPolicy) -> Result<Decoded<Vec<Minute>>> {
    let (count,end) = counted(&body,header,count_offset,18)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut ranges = Vec::new();
    for index in 0..usize::from(count) {
        let offset = header + index * 18; let raw_time = u16_at(&body,offset)?;
        rows.push(Minute { time: time::time_of_day(raw_time,0)?, raw_time,
            price: NativeNumber::unknown(float_at(&body,offset + 2)?), avg_price: NativeNumber::unknown(float_at(&body,offset + 6)?),
            volume: NativeNumber::unknown(u32_at(&body,offset + 10)?), open_interest: NativeNumber::unknown(u32_at(&body,offset + 14)?),
            record_raw: optional_raw(&body,offset,offset + 18,policy) }); ranges.push((offset,offset + 18));
    }
    Ok(finish(rows, Vec::new(), ranges, end, body.len()).with_wire_page(count, None))
}
pub fn decode_transactions(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Transaction>>> { decode_transaction_records(body,policy) }
pub fn decode_history_transactions(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Transaction>>> { decode_transaction_records(body,policy) }
fn decode_transaction_records(body: Bytes, policy: RawPolicy) -> Result<Decoded<Vec<Transaction>>> {
    let (count,end) = counted(&body,16,14,16)?;
    let mut rows = Vec::with_capacity(usize::from(count)); let mut ranges = Vec::new();
    for index in 0..usize::from(count) {
        let offset = 16 + index * 16; let raw_time = u16_at(&body,offset)?; let direction = u16_at(&body,offset + 14)?;
        // Rust rejects an invalid second instead of Python's legacy coercion to 0.
        let clock = time::time_of_day(raw_time,direction % 10000)?;
        rows.push(Transaction { time: clock, raw_time, direction_raw: direction,
            price_raw: NativeNumber::unknown(u32_at(&body,offset + 2)?), volume: NativeNumber::unknown(u32_at(&body,offset + 6)?),
            zengcang: NativeNumber::unknown(i32::from_le_bytes(array(&body,offset + 10)?)), nature: direction / 10000,
            record_raw: optional_raw(&body,offset,offset + 16,policy) }); ranges.push((offset,offset + 16));
    }
    Ok(finish(rows,Vec::new(),ranges,end,body.len()).with_wire_page(count, None))
}
