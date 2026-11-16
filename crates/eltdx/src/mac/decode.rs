//! Strict single-response codecs. No I/O, page orchestration, or Python policy.
//! A successful empty payload requires its complete command-specific header.
use encoding_rs::GBK;
use serde_json::Value;
use crate::{Error, ErrorKind, EventTime, Result, TradingDate};
use super::models::{MacMarketId, WireFloat};
use super::responses::*;

fn protocol(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Protocol, message).with_context("domain", "mac")
}
/// Every read checks its own span. Decoders never use a declared count to
/// allocate or index records until exact aggregate length has been checked.
struct View<'a> { bytes: &'a [u8], command: &'static str }
impl<'a> View<'a> {
    fn new(bytes: &'a [u8], command: &'static str) -> Self { Self { bytes, command } }
    fn span(&self, offset: usize, len: usize) -> Result<&'a [u8]> {
        offset.checked_add(len).and_then(|end| self.bytes.get(offset..end))
            .ok_or_else(|| protocol("truncated MAC response field").with_context("command", self.command)
                .with_context("offset", format!("{offset}")).with_context("length", format!("{len}")))
    }
    fn array<const N: usize>(&self, offset: usize) -> Result<[u8; N]> {
        self.span(offset, N)?.try_into().map_err(|_| protocol("MAC fixed field width mismatch"))
    }
    fn u8(&self, offset: usize) -> Result<u8> { Ok(self.array::<1>(offset)?[0]) }
    fn u16(&self, offset: usize) -> Result<u16> { Ok(u16::from_le_bytes(self.array(offset)?)) }
    fn u32(&self, offset: usize) -> Result<u32> { Ok(u32::from_le_bytes(self.array(offset)?)) }
    fn i32(&self, offset: usize) -> Result<i32> { Ok(i32::from_le_bytes(self.array(offset)?)) }
    fn float(&self, offset: usize) -> Result<WireFloat> { Ok(WireFloat::from_bits(self.u32(offset)?)) }
    fn number(&self, offset: usize) -> Result<NativeNumber<WireFloat>> { Ok(NativeNumber::unknown(self.float(offset)?)) }
    fn records(&self, header: usize, count: usize, width: usize, tail: usize) -> Result<()> {
        let expected = count.checked_mul(width).and_then(|n| n.checked_add(header)).and_then(|n| n.checked_add(tail))
            .ok_or_else(|| protocol("MAC record length overflow"))?;
        if self.bytes.len() != expected {
            return Err(protocol("MAC response length differs from declared complete records")
                .with_context("command", self.command).with_context("expected_bytes", format!("{expected}"))
                .with_context("actual_bytes", format!("{}", self.bytes.len())));
        }
        Ok(())
    }
    fn identity(&self, offset: usize) -> Result<ResponseIdentity> {
        Ok(ResponseIdentity { market: MacMarketId::new(self.u16(offset)?), code: WireText::from_wire(self.array(offset + 2)?) })
    }
    fn short_identity(&self, offset: usize) -> Result<ResponseIdentity> {
        let mut code = [0; 22];
        code[..6].copy_from_slice(self.span(offset + 2, 6)?);
        Ok(ResponseIdentity { market: MacMarketId::new(self.u16(offset)?), code: WireText::from_wire(code) })
    }
}
fn date(raw: u32) -> Result<TradingDate> {
    let year = u16::try_from(raw / 10000).map_err(|_| protocol("MAC YYYYMMDD year overflows"))?;
    if year > 9999 { return Err(protocol("MAC YYYYMMDD year exceeds four digits")); }
    TradingDate::new(year, ((raw % 10000) / 100) as u8, (raw % 100) as u8)
        .map_err(|error| protocol("invalid MAC calendar date").with_source(error).with_context("date", format!("{raw}")))
}
fn seconds(raw: u32) -> Result<EventTime> {
    if raw >= 86_400 { return Err(protocol("MAC time is outside seconds since midnight")); }
    EventTime::time_of_day((raw / 3600) as u8, ((raw % 3600) / 60) as u8, (raw % 60) as u8)
        .map_err(|error| protocol("invalid MAC clock").with_source(error))
}
fn minutes(raw: u16) -> Result<EventTime> { seconds(u32::from(raw) * 60) }
fn ohlc(view: &View<'_>, offset: usize) -> Result<[NativeNumber<WireFloat>; 5]> {
    Ok([view.number(offset)?, view.number(offset + 4)?, view.number(offset + 8)?, view.number(offset + 12)?, view.number(offset + 16)?])
}
fn snapshot(view: &View<'_>, offset: usize) -> Result<ChartSnapshot> {
    view.span(offset, 120)?;
    Ok(ChartSnapshot {
        name: WireText::from_wire(view.array(offset)?), decimal: view.u8(offset + 44)?, category: view.u16(offset + 45)?,
        vol_unit: view.float(offset + 47)?, reserved: view.array(offset + 51)?,
        date_raw: view.u32(offset + 56)?, time_raw: view.u32(offset + 60)?, ohlc: ohlc(view, offset + 64)?,
        momentum: view.number(offset + 84)?, vol: NativeNumber::unknown(view.u32(offset + 88)?),
        amount: view.number(offset + 92)?, tail_pad: view.array(offset + 96)?,
        turnover: view.number(offset + 108)?, avg: view.number(offset + 112)?, industry: view.u32(offset + 116)?,
    })
}
pub fn tick_chart(body: &[u8]) -> Result<TickChart> {
    let view = View::new(body, "0x122d");
    let count = usize::from(view.u16(33)?);
    view.records(35, count, 18, 120)?;
    let date_raw = view.u32(24)?;
    if date_raw != 0 { date(date_raw)?; }
    let mut ticks = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 35 + index * 18;
        let raw = view.u16(offset)?;
        ticks.push(Tick { minutes: raw, time: minutes(raw)?, price: view.number(offset + 2)?, avg: view.number(offset + 6)?,
            vol: NativeNumber::unknown(view.u32(offset + 10)?), momentum: Some(view.number(offset + 14)?), reserved: None });
    }
    Ok(TickChart { id: view.identity(0)?, date_raw, reserved: view.u8(28)?, ref_price: view.number(29)?,
        snapshot: snapshot(&view, 35 + count * 18)?, ticks })
}
pub fn tick_charts(body: &[u8]) -> Result<MultiTickChart> {
    let view = View::new(body, "0x123e");
    view.span(0, 71)?;
    let mut dates_raw = [0; 5];
    let mut pre_closes_raw = [WireFloat::from_bits(0); 5];
    let mut leading = 0;
    let mut empty_seen = false;
    for index in 0..5 {
        dates_raw[index] = view.u32(24 + index * 4)?;
        pre_closes_raw[index] = view.float(44 + index * 4)?;
        if dates_raw[index] == 0 { empty_seen = true; }
        else {
            date(dates_raw[index])?;
            if empty_seen { return Err(protocol("MAC multi-day date slots are not contiguous")); }
            leading += 1;
        }
    }
    let count = usize::from(view.u16(64)?);
    let page_size = view.u16(67)?;
    let total = view.u16(69)?;
    if count > 5 || count > leading { return Err(protocol("MAC multi-day count exceeds valid date slots")); }
    if count > 0 && page_size == 0 { return Err(protocol("MAC multi-day page size is zero")); }
    let capacity = count * usize::from(page_size);
    if usize::from(total) > capacity { return Err(protocol("MAC multi-day ticks exceed declared day capacity")); }
    view.records(71, usize::from(total), 14, 120)?;
    // Source order is a possibly short first day, then full days, then empty
    // days. Partition by actual declared ticks; never shift a tick across days.
    let tick_counts = if count == 0 { Vec::new() }
        else if usize::from(total) == capacity { vec![usize::from(page_size); count] }
        else {
            let full = (usize::from(total) / usize::from(page_size)).min(count - 1);
            let mut result = vec![0; count];
            result[0] = usize::from(total) - full * usize::from(page_size);
            for value in result.iter_mut().skip(1).take(full) { *value = usize::from(page_size); }
            result
        };
    let mut cursor = 0;
    let mut days = Vec::with_capacity(count);
    for (day_index, tick_count) in tick_counts.into_iter().enumerate() {
        let mut ticks = Vec::with_capacity(tick_count);
        for _ in 0..tick_count {
            let offset = 71 + cursor * 14;
            let raw = view.u16(offset)?;
            ticks.push(Tick { minutes: raw, time: minutes(raw)?, price: view.number(offset + 2)?, avg: view.number(offset + 6)?,
                vol: NativeNumber::unknown(u32::from(view.u16(offset + 10)?)), momentum: None, reserved: Some(view.u16(offset + 12)?) });
            cursor += 1;
        }
        days.push(TickDay { date: date(dates_raw[day_index])?, pre_close: NativeNumber::unknown(pre_closes_raw[day_index]), ticks });
    }
    Ok(MultiTickChart { id: view.identity(0)?, dates_raw, pre_closes_raw, send_last: view.u8(66)?, page_size, total,
        snapshot: snapshot(&view, 71 + usize::from(total) * 14)?, days })
}
pub fn symbol_info(body: &[u8]) -> Result<SymbolInfo> {
    let view = View::new(body, "0x122a");
    view.span(0, 194)?;
    let date_raw = view.u32(96)?;
    let time_raw = view.u32(100)?;
    let time = if date_raw == 0 && time_raw == 0 { None }
        else {
            let day = date(date_raw)?;
            let hour = time_raw / 10000;
            let minute = time_raw % 10000 / 100;
            let second = time_raw % 100;
            if hour > 23 || minute > 59 || second > 59 { return Err(protocol("invalid MAC symbol HHMMSS time")); }
            Some(EventTime::local(day, hour as u8, minute as u8, second as u8)
                .map_err(|error| protocol("invalid MAC symbol event time").with_source(error))?)
        };
    Ok(SymbolInfo { id: view.identity(8)?, name: WireText::from_wire(view.array(32)?),
        date_raw, time_raw, time, activity: view.u32(104)?, ohlc: ohlc(&view, 108)?,
        momentum: view.number(128)?, vol: NativeNumber::unknown(view.u32(132)?), amount: view.number(136)?,
        inside: NativeNumber::unknown(view.u32(140)?), outside: NativeNumber::unknown(view.u32(144)?),
        turnover: view.number(186)?, avg: view.number(190)?, prefix: view.array(0)?, reserved: view.array(76)?,
        extra: view.array(148)?, tail: body[194..].to_vec() })
}
fn board_side(view: &View<'_>, offset: usize) -> Result<BoardSide> {
    Ok(BoardSide { id: view.short_identity(offset)?, reserved: view.array(offset + 8)?, name: WireText::from_wire(view.array(offset + 24)?),
        price: view.number(offset + 68)?, rise_speed: view.number(offset + 72)?, pre_close: view.number(offset + 76)? })
}
pub fn boards(body: &[u8]) -> Result<BoardPage> {
    let view = View::new(body, "0x1231");
    let paired_count = view.u16(0)?;
    let total_raw = view.u16(2)?;
    if paired_count % 2 != 0 { return Err(protocol("MAC board response has an unpaired board/leader record")); }
    let count = usize::from(paired_count / 2);
    view.records(4, count, 160, 0)?;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 4 + index * 160;
        items.push(BoardInfo { board: board_side(&view, offset)?, leader: board_side(&view, offset + 80)? });
    }
    Ok(BoardPage { paired_count, total_raw, items })
}
pub fn chart_sampling(body: &[u8]) -> Result<Vec<WireFloat>> {
    let view = View::new(body, "0x254d");
    let count = usize::from(view.u16(40)?);
    view.records(42, count, 4, 0)?;
    (0..count).map(|index| view.float(42 + index * 4)).collect()
}
pub fn goods(body: &[u8]) -> Result<Vec<GoodsItem>> {
    let view = View::new(body, "0x2562");
    let count = usize::from(view.u16(0)?);
    view.records(2, count, 48, 0)?;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 2 + index * 48;
        items.push(GoodsItem { category: view.u16(offset)?, name: WireText::from_wire(view.array(offset + 2)?),
            u: view.u16(offset + 25)?, index: view.u32(offset + 27)?, switch: view.u8(offset + 31)?,
            code: [view.float(offset + 32)?, view.float(offset + 36)?, view.float(offset + 40)?],
            c1: view.u16(offset + 44)?, c2: view.u16(offset + 46)? });
    }
    Ok(items)
}
pub fn auction(body: &[u8]) -> Result<Vec<Auction>> {
    let view = View::new(body, "0x123d");
    let count = usize::try_from(view.u32(24)?).map_err(|_| protocol("MAC auction row count exceeds address space"))?;
    view.records(36, count, 16, 0)?;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 36 + index * 16;
        let time_seconds = view.u32(offset)?;
        items.push(Auction { time_seconds, time: seconds(time_seconds)?, price: view.number(offset + 4)?,
            matched: NativeNumber::unknown(view.u32(offset + 8)?), unmatched: NativeNumber::unknown(view.i32(offset + 12)?) });
    }
    Ok(items)
}
fn describe_unusual(kind: u8, data: &[u8; 13]) -> (String, String) {
    let direction = data[0];
    let first = f32::from_le_bytes([data[1], data[2], data[3], data[4]]) as f64;
    let second = f32::from_le_bytes([data[5], data[6], data[7], data[8]]) as f64;
    let integer = u32::from_le_bytes([data[9], data[10], data[11], data[12]]);
    let percent = || format!("{:.2}%", first * 100.0);
    match kind {
        0x03 => (format!("主力{}", if direction == 0 { "买入" } else { "卖出" }), format!("{first:.2}/{second:.2}")),
        0x04 => ("加速拉升".into(), percent()), 0x05 => ("加速下跌".into(), String::new()),
        0x06 => ("低位反弹".into(), percent()), 0x07 => ("高位回落".into(), percent()),
        0x08 => ("撑杆跳高".into(), percent()), 0x09 => ("平台跳水".into(), percent()),
        0x0a => (format!("单笔冲{}", if first < 0.0 { "跌" } else { "涨" }), percent()),
        0x0b => (format!("区间放量{}", if second == 0.0 { "平" } else if second < 0.0 { "跌" } else { "涨" }),
            format!("{first:.1}倍{}", if second == 0.0 { String::new() } else { format!("{:.2}%", second * 100.0) })),
        0x0c => ("区间缩量".into(), String::new()),
        0x10 => ("大单托盘".into(), format!("{:.2}/{second:.2}", f64::from(integer))),
        0x11 => ("大单压盘".into(), format!("{first:.2}/{second:.2}")),
        0x12 => ("大单锁盘".into(), String::new()),
        0x13 => ("竞价试买".into(), format!("{first:.2}/{second:.2}")),
        0x14 => {
            let side = if direction == 0 { "涨" } else { "跌" };
            let name = match data[1] { 1 => format!("逼近{side}停"), 2 => format!("封{side}停板"),
                4 => format!("封{side}大减"), 5 => format!("打开{side}停"), _ => format!("涨跌停({side})") };
            let a = f32::from_le_bytes([data[2], data[3], data[4], data[5]]);
            let b = f32::from_le_bytes([data[6], data[7], data[8], data[9]]);
            (name, format!("{a:.2}/{b:.2}"))
        }
        _ => (format!("异动类型{kind:#04x}"), String::new()),
    }
}
pub fn unusual(body: &[u8]) -> Result<Vec<Unusual>> {
    let view = View::new(body, "0x1237");
    let count = usize::from(view.u16(0)?);
    let binary_end = count.checked_mul(32).and_then(|n| n.checked_add(2)).ok_or_else(|| protocol("MAC unusual row length overflow"))?;
    view.span(0, binary_end)?;
    let (names, _, _) = GBK.decode(&body[binary_end..]);
    let names: Vec<&str> = names.trim_matches(',').split(',').collect();
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 2 + index * 32;
        let kind = view.u8(offset + 9)?;
        let data = view.array(offset + 15)?;
        let (description, value) = describe_unusual(kind, &data);
        let hour = view.u8(offset + 29)?;
        let minute_second = view.u16(offset + 30)?;
        let minute = minute_second / 100;
        let second = minute_second % 100;
        if hour > 23 || minute > 59 || second > 59 { return Err(protocol("invalid MAC unusual hour/minute-second clock")); }
        let time = EventTime::time_of_day(hour, minute as u8, second as u8)
            .map_err(|error| protocol("invalid MAC unusual event time").with_source(error))?;
        items.push(Unusual { index: view.u16(offset + 11)?, id: view.short_identity(offset)?,
            name: names.get(index).copied().unwrap_or("").to_owned(), time, unusual_type: kind, description, value, raw: view.array(offset)? });
    }
    Ok(items)
}
fn windows(view: &View<'_>, offset: usize) -> Result<[SessionWindow; 4]> {
    let mut result = [SessionWindow { open_minutes: 0, close_minutes: 0 }; 4];
    for (index, item) in result.iter_mut().enumerate() {
        *item = SessionWindow { open_minutes: view.u16(offset + index * 4)?, close_minutes: view.u16(offset + index * 4 + 2)? };
    }
    Ok(result)
}
pub fn server_info(body: &[u8]) -> Result<ServerSession> {
    let view = View::new(body, "0x120f");
    view.span(0, 71)?;
    // Each optional u32 must be complete. Unknown bytes after both parameters
    // are retained; neither their existence nor zeros imply a calendar rule.
    let (market_param_1, market_param_2, tail_start) = match body.len() {
        71 => (None, None, 71), 75 => (Some(view.u32(71)?), None, 75),
        79.. => (Some(view.u32(71)?), Some(view.u32(75)?), 79),
        _ => return Err(protocol("partial MAC server-info market parameter")),
    };
    let today_raw = view.u32(22)?;
    let last_raw = view.u32(63)?;
    Ok(ServerSession { today: ObservedDate { raw: today_raw, parsed: date(today_raw).ok() },
        last: ObservedDate { raw: last_raw, parsed: date(last_raw).ok() },
        sessions_1: windows(&view, 30)?, sessions_2: windows(&view, 46)?,
        market_param_1, market_param_2, prefix: view.array(0)?, timestamp_1: view.u32(26)?,
        timestamp_2: view.u32(67)?, flag: view.u8(62)?, tail: body[tail_start..].to_vec() })
}
pub fn file_meta(body: &[u8]) -> Result<FileMeta> {
    let view = View::new(body, "0x1215");
    view.span(0, 41)?;
    Ok(FileMeta { offset: view.u32(0)?, size: view.u32(4)?, flag: i8::from_le_bytes(view.array(8)?),
        hash: view.array(9)?, tail: body[41..].to_vec() })
}
pub fn download_chunk(body: &[u8], requested: u32) -> Result<DownloadChunk> {
    if requested == 0 || requested > 30_000 {
        return Err(Error::new(ErrorKind::InvalidArgument, "MAC download chunk size must be in 1..=30000").with_context("domain", "mac"));
    }
    let view = View::new(body, "0x1217");
    let prefix = view.array(0)?;
    if body.len() - 8 > requested as usize { return Err(protocol("MAC download response exceeds requested chunk size")); }
    Ok(DownloadChunk { prefix, bytes: body[8..].to_vec() })
}
fn json_rows(body: &[u8]) -> Result<Vec<Vec<Value>>> {
    let view = View::new(body, "0x1218");
    view.span(0, 27)?;
    let (text, _, replaced) = GBK.decode(&body[27..]);
    if replaced { return Err(protocol("invalid GBK in MAC JSON response")); }
    let value: Value = serde_json::from_str(&text).map_err(|error| protocol("malformed MAC JSON response").with_source(error))?;
    let rows = value.as_array().ok_or_else(|| protocol("MAC JSON response root is not an array"))?;
    rows.iter().map(|row| row.as_array().cloned().ok_or_else(|| protocol("MAC JSON response row is not an array"))).collect()
}
fn json_number(value: Option<&Value>) -> Result<JsonNumber> {
    let presence = match value { None => JsonPresence::Missing, Some(Value::Null) => JsonPresence::Null, _ => JsonPresence::Present };
    let number = match value {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => Some(number.as_f64().filter(|n| n.is_finite()).ok_or_else(|| protocol("MAC JSON number is outside finite f64 range"))?),
        Some(Value::String(text)) if text.trim().is_empty() => None,
        Some(Value::String(text)) => Some(text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
            .ok_or_else(|| protocol("MAC JSON numeric string is invalid or nonfinite"))?),
        _ => return Err(protocol("MAC JSON numeric field has an invalid type")),
    };
    Ok(JsonNumber { value: number, presence, source: value.cloned() })
}
fn json_u16(value: Option<&Value>) -> Result<u16> {
    let value = json_number(value)?.value.ok_or_else(|| protocol("MAC JSON identity number is missing"))?;
    if value < 0.0 || value > f64::from(u16::MAX) || value.fract() != 0.0 { return Err(protocol("MAC JSON identity number is outside u16")); }
    Ok(value as u16)
}
fn json_text(value: Option<&Value>) -> Result<String> {
    match value {
        Some(Value::String(text)) => Ok(text.clone()), Some(Value::Number(number)) => Ok(format!("{number}")),
        _ => Err(protocol("MAC JSON identity text is missing or invalid")),
    }
}
pub fn belonging_boards(body: &[u8]) -> Result<Vec<BelongBoard>> {
    json_rows(body)?.into_iter().map(|row| {
        if !matches!(row.len(), 9 | 13) { return Err(protocol("MAC belonging-board row must have 9 or 13 fields")); }
        Ok(BelongBoard { board_type: json_u16(row.first())?, market: MacMarketId::new(json_u16(row.get(1))?),
            code: json_text(row.get(2))?, name: json_text(row.get(3))?, close: json_number(row.get(4))?,
            pre_close: json_number(row.get(5))?, source: row })
    }).collect()
}
fn difference(left: &JsonNumber, right: &JsonNumber) -> Result<JsonNumber> {
    let value = match (left.value, right.value) {
        (Some(left), Some(right)) => {
            let value = left - right;
            if !value.is_finite() { return Err(protocol("MAC derived net flow overflows finite f64")); }
            Some(value)
        }
        _ => None,
    };
    Ok(JsonNumber { value, presence: JsonPresence::Derived, source: None })
}
pub fn capital_flow(body: &[u8]) -> Result<Option<CapitalFlow>> {
    let source = json_rows(body)?;
    if source.len() < 2 { return Ok(None); }
    let main_in = json_number(source[0].first())?;
    let main_out = json_number(source[0].get(1))?;
    let small_in = json_number(source[0].get(2))?;
    let small_out = json_number(source[0].get(3))?;
    let main_net = difference(&main_in, &main_out)?;
    let small_net = difference(&small_in, &small_out)?;
    let mid_net = json_number(source[1].get(4))?;
    let large_net = json_number(source[1].get(3))?;
    Ok(Some(CapitalFlow { main: [main_in, main_out, main_net], small: [small_in, small_out, small_net],
        mid: [json_number(None)?, json_number(None)?, mid_net],
        large: [json_number(None)?, json_number(None)?, large_net], source }))
}
