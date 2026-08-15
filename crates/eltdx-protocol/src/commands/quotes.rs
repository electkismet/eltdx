use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{MAX_COMMAND_ITEMS, MAX_REFRESH_CODES, MAX_RESPONSE_PAYLOAD_SIZE};
use crate::unit::{
    consume_price, consume_varint, decode_k, get_volume, little_f32, little_u16, little_u32,
    milli_to_float, price_divisor, Market, NormalizedCode,
};

pub const TYPE_LEGACY_QUOTES: u16 = 0x053e;
pub const TYPE_REFRESH_STREAM: u16 = 0x0547;
pub const TYPE_CATEGORY_QUOTES: u16 = 0x054b;
pub const TYPE_SNAPSHOTS: u16 = 0x054c;

const QUOTE_LIST_PREFIX: [u8; 8] = [5, 0, 0, 0, 0, 0, 0, 0];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotsRequest {
    pub codes: Vec<NormalizedCode>,
}

impl SnapshotsRequest {
    pub fn new(codes: Vec<NormalizedCode>) -> Result<Self, ProtocolError> {
        ensure_code_count(codes.len())?;
        Ok(Self { codes })
    }

    pub fn frame(&self, msg_id: u32) -> Result<RequestFrame, ProtocolError> {
        Ok(RequestFrame::new(
            msg_id,
            TYPE_SNAPSHOTS,
            encode_quote_code_list(&self.codes)?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyQuotesRequest {
    pub codes: Vec<NormalizedCode>,
}

impl LegacyQuotesRequest {
    pub fn new(codes: Vec<NormalizedCode>) -> Result<Self, ProtocolError> {
        if codes.is_empty() {
            return Err(ProtocolError::invalid_argument(
                "codes",
                "codes must not be empty",
            ));
        }
        ensure_code_count(codes.len())?;
        Ok(Self { codes })
    }

    pub fn frame(&self, msg_id: u32) -> Result<RequestFrame, ProtocolError> {
        Ok(RequestFrame::new(
            msg_id,
            TYPE_LEGACY_QUOTES,
            encode_quote_code_list(&self.codes)?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshCursor {
    pub code: NormalizedCode,
    pub cursor: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshStreamRequest {
    pub items: Vec<RefreshCursor>,
}

impl RefreshStreamRequest {
    pub fn new(items: Vec<RefreshCursor>) -> Result<Self, ProtocolError> {
        if items.len() > MAX_REFRESH_CODES {
            return Err(ProtocolError::invalid_argument(
                "codes",
                format!("refresh_stream accepts at most {MAX_REFRESH_CODES} codes per request"),
            ));
        }
        Ok(Self { items })
    }

    pub fn frame(&self, msg_id: u32) -> Result<RequestFrame, ProtocolError> {
        let count = u16::try_from(self.items.len())
            .map_err(|_| ProtocolError::invalid_argument("codes", "too many codes"))?;
        let mut data = Vec::with_capacity(2 + self.items.len() * 11);
        data.extend_from_slice(&count.to_le_bytes());
        for item in &self.items {
            data.push(item.code.market.id());
            data.extend_from_slice(item.code.number.as_bytes());
            data.extend_from_slice(&item.cursor.to_le_bytes());
        }
        Ok(RequestFrame::new(msg_id, TYPE_REFRESH_STREAM, data))
    }

    pub fn requested_codes(&self) -> Vec<NormalizedCode> {
        self.items.iter().map(|item| item.code.clone()).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryQuotesRequest {
    pub category: u16,
    pub sort_type: u16,
    pub start: u16,
    pub count: u16,
    pub sort_reverse: u16,
    pub filter_raw: u16,
}

impl CategoryQuotesRequest {
    pub fn new(
        category: u16,
        sort_type: u16,
        start: u16,
        count: u16,
        ascending: bool,
        sort_reverse: Option<u16>,
        filter_raw: u16,
    ) -> Self {
        let sort_reverse = match sort_reverse {
            Some(value) => value,
            None if sort_type == 0 => 0,
            None if ascending => 2,
            None => 1,
        };
        Self {
            category,
            sort_type,
            start,
            count,
            sort_reverse,
            filter_raw,
        }
    }

    pub fn frame(self, msg_id: u32) -> RequestFrame {
        let values = [
            self.category,
            self.sort_type,
            self.start,
            self.count,
            self.sort_reverse,
            5,
            self.filter_raw,
            1,
            0,
        ];
        let mut data = Vec::with_capacity(18);
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        RequestFrame::new(msg_id, TYPE_CATEGORY_QUOTES, data)
    }
}

pub fn normalize_category(value: &str) -> Result<u16, ProtocolError> {
    let text = value.trim();
    if matches!(text, "沪深a股" | "a股" | "A股" | "沪深A股") {
        return Ok(6);
    }
    parse_u16_text(text, "category")
}

pub fn normalize_sort_type(value: Option<&str>) -> Result<u16, ProtocolError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let text = value.trim();
    let known = match text {
        "代码" => Some(0x0000),
        "现价" => Some(0x0006),
        "成交额" => Some(0x000a),
        "涨幅" => Some(0x000e),
        "封单额" => Some(0x001c),
        "开盘金额" => Some(0x001d),
        "涨速" => Some(0x002e),
        "短换手" => Some(0x00cc),
        "量涨速" => Some(0x00d0),
        "开盘抢筹" => Some(0x010a),
        "2分钟金额" => Some(0x010c),
        "开盘涨幅" => Some(0x0119),
        "最高涨幅" => Some(0x011a),
        "最低涨幅" => Some(0x011b),
        "回撤" => Some(0x011e),
        "攻击" => Some(0x011f),
        _ => None,
    };
    match known {
        Some(raw) => Ok(raw),
        None => parse_u16_text(text, "sort_type"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteLevel {
    pub price: f64,
    pub volume: i64,
    pub price_delta_raw: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteSnapshot {
    pub market_id: u8,
    pub code: String,
    pub active1: u16,
    pub last_price: f64,
    pub pre_close_price: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub time_raw: i64,
    pub unknown_after_time_raw: i64,
    pub total_hand: i64,
    pub current_hand: i64,
    pub amount: f64,
    pub amount_raw: u32,
    pub inside_dish: i64,
    pub outer_disc: i64,
    pub unknown_after_outer_raw: i64,
    pub open_amount_raw: i64,
    pub open_amount_yuan: f64,
    pub buy_levels: Vec<QuoteLevel>,
    pub sell_levels: Vec<QuoteLevel>,
    pub tail_raw: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyQuote {
    pub market: Market,
    pub code: String,
    pub active1: u16,
    pub last_price: f64,
    pub pre_close_price: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub server_time_raw: i64,
    pub unknown_after_time_raw: i64,
    pub total_hand: i64,
    pub current_hand: i64,
    pub amount: f64,
    pub amount_raw: u32,
    pub inside_dish: i64,
    pub outer_disc: i64,
    pub unknown_after_outer_raw: i64,
    pub open_amount_raw: i64,
    pub open_amount_yuan: f64,
    pub buy_levels: Vec<QuoteLevel>,
    pub sell_levels: Vec<QuoteLevel>,
    pub trading_status_raw: u16,
    pub tail_metrics_raw: [i64; 4],
    pub rise_speed_raw: Option<i16>,
    pub active2: Option<u16>,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CategoryQuoteRecord {
    pub market_id: u8,
    pub code: String,
    pub active1: u16,
    pub active2: u16,
    pub last_price: f64,
    pub pre_close_price: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub server_time_raw: i64,
    pub neg_price_raw: i64,
    pub total_hand: i64,
    pub current_hand: i64,
    pub amount: f64,
    pub amount_raw: u32,
    pub inside_dish: i64,
    pub outer_disc: i64,
    pub after_outer_raw: i64,
    pub open_amount_raw: i64,
    pub open_amount: f64,
    pub bid1: f64,
    pub ask1: f64,
    pub bid_vol1: i64,
    pub ask_vol1: i64,
    pub status_or_sort_raw: u16,
    pub rise_speed_raw: i16,
    pub rise_speed: f64,
    pub short_turnover_raw: i16,
    pub short_turnover: f64,
    pub min2_amount: f32,
    pub opening_rush_raw: i16,
    pub opening_rush: f64,
    pub extra_pair_raw: [u8; 10],
    pub vol_rise_speed: f32,
    pub depth: f32,
    pub extra_meta_raw: [u8; 24],
    pub tail_raw: [u8; 56],
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CategoryQuotePage {
    pub request: CategoryQuotesRequest,
    pub header: u16,
    pub records: Vec<CategoryQuoteRecord>,
    pub raw_payload: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteRefreshRecord {
    pub market_id: u8,
    pub code: String,
    pub active: u16,
    pub update_time_raw: u32,
    pub last_price: f64,
    pub last_close_price: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub status_or_reserved_raw: i64,
    pub total_hand: i64,
    pub current_hand: i64,
    pub amount: f64,
    pub amount_raw: u32,
    pub inside_dish: i64,
    pub outer_disc: i64,
    pub unknown_after_outer_raw: i64,
    pub open_amount_raw: i64,
    pub open_amount_yuan: f64,
    pub buy_levels: Vec<QuoteLevel>,
    pub sell_levels: Vec<QuoteLevel>,
    pub tail_raw: Bytes,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteRefreshPage {
    pub requested_codes: Vec<NormalizedCode>,
    pub records: Vec<QuoteRefreshRecord>,
    pub decoded_payload: Bytes,
    pub raw_payload: Bytes,
}

pub fn parse_snapshots_payload(
    payload: &[u8],
    requested_codes: &[NormalizedCode],
) -> Result<Vec<QuoteSnapshot>, ProtocolError> {
    ensure_payload_bound(payload, "snapshots")?;
    if payload.len() < 4 {
        return Err(ProtocolError::invalid_data(
            "snapshots",
            "invalid snapshots payload",
        ));
    }
    let count = usize::from(little_u16(&payload[2..4])?);
    if count > 0 && requested_codes.len() < count {
        return Err(ProtocolError::invalid_data(
            "snapshots",
            if requested_codes.is_empty() {
                "snapshot parser requires request codes to split variable records"
            } else {
                "snapshot response count exceeds request code count"
            },
        ));
    }
    if count > payload.len().saturating_sub(4) / 30 {
        return Err(ProtocolError::invalid_data(
            "snapshots",
            "truncated snapshot record",
        ));
    }
    let records = split_snapshot_records(&payload[4..], &requested_codes[..count])?;
    records
        .iter()
        .zip(&requested_codes[..count])
        .map(|(record, code)| parse_snapshot_record(record, code))
        .collect()
}

pub fn parse_legacy_quotes_payload(
    payload: &[u8],
    requested_codes: &[NormalizedCode],
) -> Result<Vec<LegacyQuote>, ProtocolError> {
    ensure_payload_bound(payload, "legacy quotes")?;
    if payload.len() < 4 {
        return Err(ProtocolError::invalid_data(
            "legacy quotes",
            "invalid legacy quotes payload",
        ));
    }
    let count = usize::from(little_u16(&payload[2..4])?);
    if count > payload.len().saturating_sub(4) / 52 {
        return Err(ProtocolError::invalid_data(
            "legacy quotes",
            "truncated legacy quote record header",
        ));
    }
    let mut offset = 4;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let next_marker = requested_codes.get(index + 1).map(code_marker);
        let (record, next) = parse_legacy_quote_record(
            payload,
            offset,
            count - index - 1,
            next_marker.as_ref().map(|marker| marker.as_slice()),
        )?;
        offset = next;
        records.push(record);
    }
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            "legacy quotes",
            format!(
                "unexpected trailing legacy quotes payload bytes: {}",
                payload.len() - offset
            ),
        ));
    }
    Ok(records)
}

pub fn parse_category_quotes_payload(
    payload: &[u8],
    request: CategoryQuotesRequest,
) -> Result<CategoryQuotePage, ProtocolError> {
    ensure_payload_bound(payload, "category quotes")?;
    if payload.len() < 4 {
        return Err(ProtocolError::invalid_data(
            "category quotes",
            "invalid category quotes payload",
        ));
    }
    let header = little_u16(&payload[..2])?;
    let count = usize::from(little_u16(&payload[2..4])?);
    if count > payload.len().saturating_sub(4) / 86 {
        return Err(ProtocolError::invalid_data(
            "category quotes",
            "truncated category quote record header",
        ));
    }
    let mut offset = 4;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let (record, next) = parse_category_quote_record(payload, offset)?;
        offset = next;
        records.push(record);
    }
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            "category quotes",
            format!(
                "unexpected trailing category quotes payload bytes: {}",
                payload.len() - offset
            ),
        ));
    }
    Ok(CategoryQuotePage {
        request,
        header,
        records,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_refresh_stream_payload(
    raw_payload: &[u8],
    requested_codes: &[NormalizedCode],
) -> Result<QuoteRefreshPage, ProtocolError> {
    ensure_payload_bound(raw_payload, "refresh stream")?;
    if requested_codes.len() > MAX_REFRESH_CODES {
        return Err(ProtocolError::invalid_argument(
            "codes",
            format!("refresh_stream accepts at most {MAX_REFRESH_CODES} codes per request"),
        ));
    }
    let decoded: Vec<u8> = raw_payload.iter().map(|byte| *byte ^ 0x93).collect();
    if decoded.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "refresh stream",
            "invalid refresh stream payload",
        ));
    }
    let count = usize::from(little_u16(&decoded[..2])?);
    if count == 0 {
        if decoded.len() != 2 {
            return Err(ProtocolError::invalid_data(
                "refresh stream",
                format!(
                    "unexpected trailing empty refresh bytes: {}",
                    decoded.len() - 2
                ),
            ));
        }
        return Ok(QuoteRefreshPage {
            requested_codes: requested_codes.to_vec(),
            records: Vec::new(),
            decoded_payload: Bytes::from(decoded),
            raw_payload: Bytes::copy_from_slice(raw_payload),
        });
    }
    let raw_records = split_refresh_records(&decoded[2..], requested_codes, count)?;
    let records = raw_records
        .iter()
        .map(|record| parse_refresh_record(record))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(QuoteRefreshPage {
        requested_codes: requested_codes.to_vec(),
        records,
        decoded_payload: Bytes::from(decoded),
        raw_payload: Bytes::copy_from_slice(raw_payload),
    })
}

fn parse_snapshot_record(
    record: &[u8],
    expected_code: &NormalizedCode,
) -> Result<QuoteSnapshot, ProtocolError> {
    if record.len() < 9 {
        return Err(ProtocolError::invalid_data(
            "snapshot",
            "truncated snapshot record",
        ));
    }
    let market_id = record[0];
    let code = parse_ascii_code(&record[1..7], "snapshot")?;
    let active1 = little_u16(&record[7..9])?;
    let (prices, mut offset) = decode_k(record, 9)?;
    let (time_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let (unknown_after_time_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let (total_hand, next) = consume_varint(record, offset)?;
    offset = next;
    let (current_hand, next) = consume_varint(record, offset)?;
    offset = next;
    let amount_raw = read_u32(record, offset, "truncated snapshot amount")?;
    offset += 4;
    let (inside_dish, next) = consume_varint(record, offset)?;
    offset = next;
    let (outer_disc, next) = consume_varint(record, offset)?;
    offset = next;
    let (unknown_after_outer_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let (open_amount_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let divisor = i64::from(price_divisor(expected_code));
    let (bid_delta, next) = consume_price(record, offset)?;
    offset = next;
    let (ask_delta, next) = consume_price(record, offset)?;
    offset = next;
    let (bid_volume, next) = consume_varint(record, offset)?;
    offset = next;
    let (ask_volume, next) = consume_varint(record, offset)?;
    offset = next;
    let bid_milli = scaled_price(prices.current_milli, bid_delta, divisor)?;
    let ask_milli = scaled_price(prices.current_milli, ask_delta, divisor)?;
    Ok(QuoteSnapshot {
        market_id,
        code,
        active1,
        last_price: milli_to_float(floor_div(prices.current_milli, divisor)?),
        pre_close_price: milli_to_float(floor_div(prices.last_close_milli, divisor)?),
        open_price: milli_to_float(floor_div(prices.open_milli, divisor)?),
        high_price: milli_to_float(floor_div(prices.high_milli, divisor)?),
        low_price: milli_to_float(floor_div(prices.low_milli, divisor)?),
        time_raw,
        unknown_after_time_raw,
        total_hand,
        current_hand,
        amount: get_volume(amount_raw),
        amount_raw,
        inside_dish,
        outer_disc,
        unknown_after_outer_raw,
        open_amount_raw,
        open_amount_yuan: checked_scale(open_amount_raw, 100)? as f64,
        buy_levels: vec![QuoteLevel {
            price: milli_to_float(bid_milli),
            volume: bid_volume,
            price_delta_raw: bid_delta,
        }],
        sell_levels: vec![QuoteLevel {
            price: milli_to_float(ask_milli),
            volume: ask_volume,
            price_delta_raw: ask_delta,
        }],
        tail_raw: Bytes::copy_from_slice(&record[offset..]),
    })
}

fn parse_legacy_quote_record(
    payload: &[u8],
    mut offset: usize,
    remaining_records: usize,
    next_marker: Option<&[u8]>,
) -> Result<(LegacyQuote, usize), ProtocolError> {
    let start = offset;
    if payload.len() < offset.saturating_add(9) {
        return Err(ProtocolError::invalid_data(
            "legacy quote",
            "truncated legacy quote record header",
        ));
    }
    let market_id = payload[offset];
    let market = Market::from_id(i64::from(market_id)).map_err(|_| {
        ProtocolError::invalid_data(
            "legacy quote",
            format!("invalid legacy quote market id: {market_id}"),
        )
    })?;
    let code = parse_ascii_digits(&payload[offset + 1..offset + 7], "legacy quote")?;
    let active1 = little_u16(&payload[offset + 7..offset + 9])?;
    offset += 9;
    let normalized = NormalizedCode {
        market,
        number: code.clone(),
    };
    let price_scale = 100.0 * f64::from(price_divisor(&normalized));
    let (close_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (pre_close_diff_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (open_diff_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (high_diff_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (low_diff_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (server_time_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (unknown_after_time_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (total_hand, next) = consume_varint(payload, offset)?;
    offset = next;
    let (current_hand, next) = consume_varint(payload, offset)?;
    offset = next;
    let amount_raw = read_u32(payload, offset, "truncated legacy quote amount")?;
    offset += 4;
    let (inside_dish, next) = consume_varint(payload, offset)?;
    offset = next;
    let (outer_disc, next) = consume_varint(payload, offset)?;
    offset = next;
    let (unknown_after_outer_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (open_amount_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let mut buy_levels = Vec::with_capacity(5);
    let mut sell_levels = Vec::with_capacity(5);
    for _ in 0..5 {
        let (bid_delta, next) = consume_varint(payload, offset)?;
        offset = next;
        let (ask_delta, next) = consume_varint(payload, offset)?;
        offset = next;
        let (bid_volume, next) = consume_varint(payload, offset)?;
        offset = next;
        let (ask_volume, next) = consume_varint(payload, offset)?;
        offset = next;
        buy_levels.push(QuoteLevel {
            price: checked_add(close_raw, bid_delta)? as f64 / price_scale,
            volume: bid_volume,
            price_delta_raw: bid_delta,
        });
        sell_levels.push(QuoteLevel {
            price: checked_add(close_raw, ask_delta)? as f64 / price_scale,
            volume: ask_volume,
            price_delta_raw: ask_delta,
        });
    }
    let trading_status_raw = read_u16(payload, offset, "truncated legacy quote trading status")?;
    offset += 2;
    let mut tail_metrics_raw = [0_i64; 4];
    for value in &mut tail_metrics_raw {
        let (parsed, next) = consume_varint(payload, offset)?;
        *value = parsed;
        offset = next;
    }
    let mut rise_speed_raw = None;
    let mut active2 = None;
    if remaining_records > 0 {
        let has_short_tail =
            next_marker.is_some_and(|marker| starts_with_at(payload, offset, marker));
        let has_long_tail = next_marker
            .is_some_and(|marker| starts_with_at(payload, offset.saturating_add(4), marker));
        if has_long_tail || (!has_short_tail && !is_legacy_record_marker(payload, offset)) {
            rise_speed_raw = Some(read_i16(payload, offset, "truncated legacy quote tail")?);
            active2 = Some(read_u16(
                payload,
                offset.saturating_add(2),
                "truncated legacy quote tail",
            )?);
            offset += 4;
        }
    } else if offset < payload.len() {
        if payload.len() != offset.saturating_add(4) {
            return Err(ProtocolError::invalid_data(
                "legacy quote",
                "truncated legacy quote tail",
            ));
        }
        rise_speed_raw = Some(read_i16(payload, offset, "truncated legacy quote tail")?);
        active2 = Some(read_u16(
            payload,
            offset.saturating_add(2),
            "truncated legacy quote tail",
        )?);
        offset += 4;
    }
    Ok((
        LegacyQuote {
            market,
            code,
            active1,
            last_price: close_raw as f64 / price_scale,
            pre_close_price: checked_add(close_raw, pre_close_diff_raw)? as f64 / price_scale,
            open_price: checked_add(close_raw, open_diff_raw)? as f64 / price_scale,
            high_price: checked_add(close_raw, high_diff_raw)? as f64 / price_scale,
            low_price: checked_add(close_raw, low_diff_raw)? as f64 / price_scale,
            server_time_raw,
            unknown_after_time_raw,
            total_hand,
            current_hand,
            amount: get_volume(amount_raw),
            amount_raw,
            inside_dish,
            outer_disc,
            unknown_after_outer_raw,
            open_amount_raw,
            open_amount_yuan: checked_scale(open_amount_raw, 100)? as f64,
            buy_levels,
            sell_levels,
            trading_status_raw,
            tail_metrics_raw,
            rise_speed_raw,
            active2,
            record_hex: encode_hex(&payload[start..offset]),
        },
        offset,
    ))
}

fn parse_category_quote_record(
    payload: &[u8],
    mut offset: usize,
) -> Result<(CategoryQuoteRecord, usize), ProtocolError> {
    let start = offset;
    if payload.len() < offset.saturating_add(9) {
        return Err(ProtocolError::invalid_data(
            "category quote",
            "truncated category quote record header",
        ));
    }
    let market_id = payload[offset];
    let market = Market::from_id(i64::from(market_id)).map_err(|_| {
        ProtocolError::invalid_data("category quote", "invalid category quote market")
    })?;
    let code = parse_ascii_code(&payload[offset + 1..offset + 7], "category quote")?;
    let active1 = little_u16(&payload[offset + 7..offset + 9])?;
    offset += 9;
    let (close_raw, next) = consume_price(payload, offset)?;
    offset = next;
    let (pre_close_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (open_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (high_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (low_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (server_time_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (neg_price_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (total_hand, next) = consume_varint(payload, offset)?;
    offset = next;
    let (current_hand, next) = consume_varint(payload, offset)?;
    offset = next;
    let amount_raw = read_u32(payload, offset, "truncated category quote amount")?;
    offset += 4;
    let (inside_dish, next) = consume_varint(payload, offset)?;
    offset = next;
    let (outer_disc, next) = consume_varint(payload, offset)?;
    offset = next;
    let (after_outer_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (open_amount_raw, next) = consume_varint(payload, offset)?;
    offset = next;
    let (bid1_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (ask1_diff, next) = consume_price(payload, offset)?;
    offset = next;
    let (bid_vol1, next) = consume_varint(payload, offset)?;
    offset = next;
    let (ask_vol1, next) = consume_varint(payload, offset)?;
    offset = next;
    let end = offset.checked_add(56).ok_or_else(|| {
        ProtocolError::invalid_data("category quote", "category quote length overflow")
    })?;
    if end > payload.len() {
        return Err(ProtocolError::invalid_data(
            "category quote",
            "truncated category quote tail",
        ));
    }
    let tail = exact_array::<56>(&payload[offset..end], "category quote tail")?;
    offset = end;
    let normalized = NormalizedCode {
        market,
        number: code.clone(),
    };
    let divisor = i64::from(price_divisor(&normalized));
    let status_or_sort_raw = little_u16(&tail[..2])?;
    let rise_speed_raw = i16::from_le_bytes(exact_array(&tail[2..4], "rise speed")?);
    let short_turnover_raw = i16::from_le_bytes(exact_array(&tail[4..6], "turnover")?);
    let min2_amount = little_f32(&tail[6..10])?;
    let opening_rush_raw = i16::from_le_bytes(exact_array(&tail[10..12], "opening rush")?);
    let extra_pair_raw = exact_array(&tail[12..22], "category extra pair")?;
    let vol_rise_speed = little_f32(&tail[22..26])?;
    let depth = little_f32(&tail[26..30])?;
    let extra_meta_raw = exact_array(&tail[30..54], "category extra metadata")?;
    let active2 = little_u16(&tail[54..56])?;
    Ok((
        CategoryQuoteRecord {
            market_id,
            code,
            active1,
            active2,
            last_price: quote_price(close_raw, divisor)?,
            pre_close_price: quote_price(checked_add(close_raw, pre_close_diff)?, divisor)?,
            open_price: quote_price(checked_add(close_raw, open_diff)?, divisor)?,
            high_price: quote_price(checked_add(close_raw, high_diff)?, divisor)?,
            low_price: quote_price(checked_add(close_raw, low_diff)?, divisor)?,
            server_time_raw,
            neg_price_raw,
            total_hand,
            current_hand,
            amount: get_volume(amount_raw),
            amount_raw,
            inside_dish,
            outer_disc,
            after_outer_raw,
            open_amount_raw,
            open_amount: checked_scale(open_amount_raw, 100)? as f64,
            bid1: quote_price(checked_add(close_raw, bid1_diff)?, divisor)?,
            ask1: quote_price(checked_add(close_raw, ask1_diff)?, divisor)?,
            bid_vol1,
            ask_vol1,
            status_or_sort_raw,
            rise_speed_raw,
            rise_speed: f64::from(rise_speed_raw) / 100.0,
            short_turnover_raw,
            short_turnover: f64::from(short_turnover_raw) / 100.0,
            min2_amount,
            opening_rush_raw,
            opening_rush: f64::from(opening_rush_raw) / 100.0,
            extra_pair_raw,
            vol_rise_speed,
            depth,
            extra_meta_raw,
            tail_raw: tail,
            record_hex: encode_hex(&payload[start..offset]),
        },
        offset,
    ))
}

fn parse_refresh_record(record: &[u8]) -> Result<QuoteRefreshRecord, ProtocolError> {
    if record.len() < 9 {
        return Err(ProtocolError::invalid_data(
            "refresh quote",
            "truncated refresh record",
        ));
    }
    let market_id = record[0];
    let market = Market::from_id(i64::from(market_id))
        .map_err(|_| ProtocolError::invalid_data("refresh quote", "invalid refresh market"))?;
    let code = parse_ascii_code(&record[1..7], "refresh quote")?;
    let active = little_u16(&record[7..9])?;
    let (prices, mut offset) = decode_k(record, 9)?;
    let update_time_raw = read_u32(record, offset, "truncated refresh update time")?;
    offset += 4;
    let (status_or_reserved_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let (total_hand, next) = consume_varint(record, offset)?;
    offset = next;
    let (current_hand, next) = consume_varint(record, offset)?;
    offset = next;
    let amount_raw = read_u32(record, offset, "truncated refresh amount")?;
    offset += 4;
    let (inside_dish, next) = consume_varint(record, offset)?;
    offset = next;
    let (outer_disc, next) = consume_varint(record, offset)?;
    offset = next;
    let (unknown_after_outer_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let (open_amount_raw, next) = consume_varint(record, offset)?;
    offset = next;
    let normalized = NormalizedCode {
        market,
        number: code.clone(),
    };
    let divisor = i64::from(price_divisor(&normalized));
    let mut buy_levels = Vec::with_capacity(5);
    let mut sell_levels = Vec::with_capacity(5);
    for _ in 0..5 {
        let (buy_delta, next) = consume_price(record, offset)?;
        offset = next;
        let (sell_delta, next) = consume_price(record, offset)?;
        offset = next;
        let (buy_volume, next) = consume_varint(record, offset)?;
        offset = next;
        let (sell_volume, next) = consume_varint(record, offset)?;
        offset = next;
        buy_levels.push(QuoteLevel {
            price: milli_to_float(scaled_price(prices.current_milli, buy_delta, divisor)?),
            volume: buy_volume,
            price_delta_raw: buy_delta,
        });
        sell_levels.push(QuoteLevel {
            price: milli_to_float(scaled_price(prices.current_milli, sell_delta, divisor)?),
            volume: sell_volume,
            price_delta_raw: sell_delta,
        });
    }
    Ok(QuoteRefreshRecord {
        market_id,
        code,
        active,
        update_time_raw,
        last_price: milli_to_float(floor_div(prices.current_milli, divisor)?),
        last_close_price: milli_to_float(floor_div(prices.last_close_milli, divisor)?),
        open_price: milli_to_float(floor_div(prices.open_milli, divisor)?),
        high_price: milli_to_float(floor_div(prices.high_milli, divisor)?),
        low_price: milli_to_float(floor_div(prices.low_milli, divisor)?),
        status_or_reserved_raw,
        total_hand,
        current_hand,
        amount: get_volume(amount_raw),
        amount_raw,
        inside_dish,
        outer_disc,
        unknown_after_outer_raw,
        open_amount_raw,
        open_amount_yuan: checked_scale(open_amount_raw, 10)? as f64,
        buy_levels,
        sell_levels,
        tail_raw: Bytes::copy_from_slice(&record[offset..]),
        record_hex: encode_hex(record),
    })
}

fn split_snapshot_records<'a>(
    data: &'a [u8],
    codes: &[NormalizedCode],
) -> Result<Vec<&'a [u8]>, ProtocolError> {
    if codes.is_empty() {
        return Ok(Vec::new());
    }
    let mut starts = Vec::with_capacity(codes.len());
    let mut search_from = 0;
    for code in codes {
        let marker = code_marker(code);
        let relative = find_subslice(&data[search_from..], &marker).ok_or_else(|| {
            ProtocolError::invalid_data(
                "snapshot",
                format!("snapshot record marker not found: {code}"),
            )
        })?;
        let position = search_from + relative;
        starts.push(position);
        search_from = position + marker.len();
    }
    Ok(slice_at_starts(data, &starts))
}

fn split_refresh_records<'a>(
    data: &'a [u8],
    requested_codes: &[NormalizedCode],
    count: usize,
) -> Result<Vec<&'a [u8]>, ProtocolError> {
    if count == 1 {
        return Ok(vec![data]);
    }
    if requested_codes.is_empty() {
        return Err(ProtocolError::invalid_data(
            "refresh stream",
            "refresh parser needs request codes to split multiple records",
        ));
    }
    let markers: Vec<[u8; 7]> = requested_codes.iter().map(code_marker).collect();
    let mut starts = vec![0];
    let mut search_from = 7;
    while starts.len() < count {
        let next = markers
            .iter()
            .filter_map(|marker| {
                find_subslice(data.get(search_from..)?, marker)
                    .map(|position| search_from + position)
            })
            .min()
            .ok_or_else(|| {
                ProtocolError::invalid_data("refresh stream", "refresh record marker not found")
            })?;
        starts.push(next);
        search_from = next + 7;
    }
    Ok(slice_at_starts(data, &starts))
}

fn slice_at_starts<'a>(data: &'a [u8], starts: &[usize]) -> Vec<&'a [u8]> {
    let mut records = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let end = match starts.get(index + 1).copied() {
            Some(value) => value,
            None => data.len(),
        };
        records.push(&data[start..end]);
    }
    records
}

fn encode_quote_code_list(codes: &[NormalizedCode]) -> Result<Vec<u8>, ProtocolError> {
    let count = u16::try_from(codes.len())
        .map_err(|_| ProtocolError::invalid_argument("codes", "too many codes"))?;
    let mut data = Vec::with_capacity(10 + codes.len() * 7);
    data.extend_from_slice(&QUOTE_LIST_PREFIX);
    data.extend_from_slice(&count.to_le_bytes());
    for code in codes {
        data.push(code.market.id());
        data.extend_from_slice(code.number.as_bytes());
    }
    Ok(data)
}

fn ensure_code_count(count: usize) -> Result<(), ProtocolError> {
    if count > MAX_COMMAND_ITEMS {
        return Err(ProtocolError::invalid_argument("codes", "too many codes"));
    }
    Ok(())
}

fn ensure_payload_bound(payload: &[u8], resource: &'static str) -> Result<(), ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource,
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

fn parse_u16_text(value: &str, name: &'static str) -> Result<u16, ProtocolError> {
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16)
    } else if let Some(octal) = value
        .strip_prefix("0o")
        .or_else(|| value.strip_prefix("0O"))
    {
        u16::from_str_radix(octal, 8)
    } else if let Some(binary) = value
        .strip_prefix("0b")
        .or_else(|| value.strip_prefix("0B"))
    {
        u16::from_str_radix(binary, 2)
    } else {
        value.parse::<u16>()
    };
    parsed.map_err(|_| ProtocolError::invalid_argument(name, format!("invalid {name}: {value:?}")))
}

fn parse_ascii_code(data: &[u8], context: &'static str) -> Result<String, ProtocolError> {
    if data.len() != 6 || !data.is_ascii() {
        return Err(ProtocolError::invalid_data(
            context,
            format!("invalid {context} code"),
        ));
    }
    std::str::from_utf8(data)
        .map(str::to_owned)
        .map_err(|_| ProtocolError::invalid_data(context, format!("invalid {context} code")))
}

fn parse_ascii_digits(data: &[u8], context: &'static str) -> Result<String, ProtocolError> {
    if !data.iter().all(u8::is_ascii_digit) {
        return Err(ProtocolError::invalid_data(
            context,
            "invalid legacy quote code",
        ));
    }
    parse_ascii_code(data, context)
}

fn code_marker(code: &NormalizedCode) -> [u8; 7] {
    let mut marker = [0_u8; 7];
    marker[0] = code.market.id();
    marker[1..].copy_from_slice(code.number.as_bytes());
    marker
}

fn find_subslice(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    data.windows(needle.len())
        .position(|window| window == needle)
}

fn starts_with_at(data: &[u8], offset: usize, marker: &[u8]) -> bool {
    data.get(offset..)
        .is_some_and(|remaining| remaining.starts_with(marker))
}

fn is_legacy_record_marker(data: &[u8], offset: usize) -> bool {
    let Some(header) = data.get(offset..offset.saturating_add(9)) else {
        return false;
    };
    matches!(header[0], 0 | 1 | 2) && header[1..7].iter().all(u8::is_ascii_digit)
}

fn read_u16(data: &[u8], offset: usize, message: &'static str) -> Result<u16, ProtocolError> {
    let end = offset.saturating_add(2);
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| ProtocolError::invalid_data("quote", message))?;
    little_u16(bytes)
}

fn read_i16(data: &[u8], offset: usize, message: &'static str) -> Result<i16, ProtocolError> {
    let bytes = exact_array::<2>(
        data.get(offset..offset.saturating_add(2))
            .ok_or_else(|| ProtocolError::invalid_data("quote", message))?,
        "i16",
    )?;
    Ok(i16::from_le_bytes(bytes))
}

fn read_u32(data: &[u8], offset: usize, message: &'static str) -> Result<u32, ProtocolError> {
    let end = offset.saturating_add(4);
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| ProtocolError::invalid_data("quote", message))?;
    little_u32(bytes)
}

fn exact_array<const N: usize>(data: &[u8], field: &'static str) -> Result<[u8; N], ProtocolError> {
    data.try_into().map_err(|_| ProtocolError::LengthMismatch {
        field,
        expected: N,
        actual: data.len(),
    })
}

fn checked_add(left: i64, right: i64) -> Result<i64, ProtocolError> {
    left.checked_add(right)
        .ok_or_else(|| ProtocolError::invalid_data("quote", "quote price overflow"))
}

fn checked_scale(value: i64, scale: i64) -> Result<i64, ProtocolError> {
    value
        .checked_mul(scale)
        .ok_or_else(|| ProtocolError::invalid_data("quote", "quote value overflow"))
}

fn floor_div(value: i64, divisor: i64) -> Result<i64, ProtocolError> {
    if divisor <= 0 {
        return Err(ProtocolError::invalid_data(
            "quote",
            "invalid price divisor",
        ));
    }
    let quotient = value / divisor;
    let remainder = value % divisor;
    Ok(if remainder < 0 {
        quotient - 1
    } else {
        quotient
    })
}

fn scaled_price(current_milli: i64, delta: i64, divisor: i64) -> Result<i64, ProtocolError> {
    floor_div(
        checked_add(current_milli, checked_scale(delta, 10)?)?,
        divisor,
    )
}

fn quote_price(raw: i64, divisor: i64) -> Result<f64, ProtocolError> {
    Ok(milli_to_float(floor_div(checked_scale(raw, 10)?, divisor)?))
}

fn encode_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(data.len() * 2);
    for byte in data {
        output.push(char::from(DIGITS[usize::from(*byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(*byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_category, normalize_sort_type, parse_refresh_stream_payload,
        parse_snapshots_payload, CategoryQuotesRequest, LegacyQuotesRequest, RefreshCursor,
        RefreshStreamRequest, SnapshotsRequest,
    };
    use crate::{unit::NormalizedCode, ProtocolError};

    #[test]
    fn quote_requests_match_frozen_wire_data() -> Result<(), ProtocolError> {
        let codes = vec![
            NormalizedCode::parse("sz000001")?,
            NormalizedCode::parse("sh600000")?,
            NormalizedCode::parse("bj899050")?,
        ];
        let snapshot = SnapshotsRequest::new(codes).and_then(|value| value.frame(9));
        assert!(matches!(
            snapshot,
            Ok(frame)
                if frame.data.as_ref() == &[
                    5, 0, 0, 0, 0, 0, 0, 0, 3, 0,
                    0, b'0', b'0', b'0', b'0', b'0', b'1',
                    1, b'6', b'0', b'0', b'0', b'0', b'0',
                    2, b'8', b'9', b'9', b'0', b'5', b'0',
                ]
        ));
        assert!(LegacyQuotesRequest::new(Vec::new()).is_err());

        let refresh = RefreshStreamRequest::new(vec![RefreshCursor {
            code: NormalizedCode::parse("sz000001")?,
            cursor: 0,
        }])?
        .frame(1);
        assert!(matches!(
            refresh,
            Ok(frame) if frame.data.as_ref() == &[1, 0, 0, b'0', b'0', b'0', b'0', b'0', b'1', 0, 0, 0, 0]
        ));

        let category = CategoryQuotesRequest::new(6, 0, 0, 42, false, None, 0).frame(1);
        assert_eq!(
            category.data.as_ref(),
            &[6, 0, 0, 0, 0, 0, 42, 0, 0, 0, 5, 0, 0, 0, 1, 0, 0, 0]
        );
        Ok(())
    }

    #[test]
    fn normalizes_category_and_sort_aliases() {
        assert_eq!(normalize_category("沪深A股"), Ok(6));
        assert_eq!(normalize_sort_type(Some("涨幅")), Ok(14));
        assert_eq!(normalize_sort_type(None), Ok(0));
    }

    #[test]
    fn parses_empty_refresh_and_minimal_snapshot() -> Result<(), ProtocolError> {
        let requested = vec![NormalizedCode::parse("sz000001")?];
        let refresh = parse_refresh_stream_payload(&[0x93, 0x93], &requested);
        assert!(matches!(refresh, Ok(page) if page.records.is_empty()));

        let mut record = vec![0, b'0', b'0', b'0', b'0', b'0', b'1', 0, 0];
        record.extend_from_slice(&[0; 5]);
        record.extend_from_slice(&[0; 4]);
        record.extend_from_slice(&[0; 4]);
        record.extend_from_slice(&[0; 4]);
        record.extend_from_slice(&[0; 4]);
        let mut payload = vec![0, 0, 1, 0];
        payload.extend_from_slice(&record);
        let snapshots = parse_snapshots_payload(&payload, &requested);
        assert!(matches!(snapshots, Ok(values) if values.len() == 1));
        Ok(())
    }
}
