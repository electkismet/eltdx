use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{DEFAULT_TRADE_PAGE_SIZE, MAX_RESPONSE_PAYLOAD_SIZE, MAX_TRADE_PAGE_SIZE};
use crate::unit::{
    consume_price, consume_varint, little_f32, little_u16, DateParts, DateTimeParts, NormalizedCode,
};

pub const TYPE_TODAY_TICKS: u16 = 0x0fc5;
pub const TYPE_HISTORICAL_TICKS: u16 = 0x0fc6;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodayTicksRequest {
    pub code: NormalizedCode,
    pub start: u16,
    pub count: u16,
    pub include_raw: bool,
}

impl TodayTicksRequest {
    pub fn new(code: NormalizedCode, start: u16, count: u16) -> Result<Self, ProtocolError> {
        Self::with_include_raw(code, start, count, false)
    }

    pub fn with_include_raw(
        code: NormalizedCode,
        start: u16,
        count: u16,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        validate_count(count)?;
        Ok(Self {
            code,
            start,
            count,
            include_raw,
        })
    }

    pub fn with_plan_defaults(code: NormalizedCode) -> Self {
        Self {
            code,
            start: 0,
            count: DEFAULT_TRADE_PAGE_SIZE,
            include_raw: false,
        }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&[self.code.market().id(), 0]);
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&self.start.to_le_bytes());
        data.extend_from_slice(&self.count.to_le_bytes());
        RequestFrame::new(msg_id, TYPE_TODAY_TICKS, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoricalTicksRequest {
    pub code: NormalizedCode,
    pub trading_date: DateParts,
    pub trading_date_raw: u32,
    pub start: u16,
    pub count: u16,
    pub include_raw: bool,
}

impl HistoricalTicksRequest {
    pub fn new(
        code: NormalizedCode,
        trading_date: DateParts,
        start: u16,
        count: u16,
    ) -> Result<Self, ProtocolError> {
        Self::with_include_raw(code, trading_date, start, count, false)
    }

    pub fn with_include_raw(
        code: NormalizedCode,
        trading_date: DateParts,
        start: u16,
        count: u16,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        validate_count(count)?;
        let trading_date = DateParts::new(trading_date.year, trading_date.month, trading_date.day)?;
        let trading_date_raw = trading_date.yyyymmdd()?;
        Ok(Self {
            code,
            trading_date,
            trading_date_raw,
            start,
            count,
            include_raw,
        })
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&self.trading_date_raw.to_le_bytes());
        data.extend_from_slice(&u16::from(self.code.market().id()).to_le_bytes());
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&self.start.to_le_bytes());
        data.extend_from_slice(&self.count.to_le_bytes());
        RequestFrame::new(msg_id, TYPE_HISTORICAL_TICKS, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
    Neutral,
    Status(i64),
}

impl TradeSide {
    pub const fn from_raw(value: i64) -> Self {
        match value {
            0 => Self::Buy,
            1 => Self::Sell,
            2 => Self::Neutral,
            other => Self::Status(other),
        }
    }

    pub fn canonical_name(&self) -> String {
        match self {
            Self::Buy => "buy".to_owned(),
            Self::Sell => "sell".to_owned(),
            Self::Neutral => "neutral".to_owned(),
            Self::Status(value) => format!("status_{value}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TradeEventKind {
    Trade,
    OpeningMatch,
    AuctionSnapshot,
}

impl TradeEventKind {
    pub const fn classify(status_raw: i64, time_minutes: u16) -> Self {
        if status_raw == 8 {
            Self::AuctionSnapshot
        } else if time_minutes == 9 * 60 + 25 {
            Self::OpeningMatch
        } else {
            Self::Trade
        }
    }

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::OpeningMatch => "opening_match",
            Self::AuctionSnapshot => "auction_snapshot",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradeTick {
    pub index: u16,
    pub absolute_index: u32,
    pub time_minutes: u16,
    pub time_label: String,
    pub trade_datetime: Option<DateTimeParts>,
    pub price: f64,
    pub price_milli: i64,
    pub volume: i64,
    pub order_count: i64,
    pub status_raw: i64,
    pub side: TradeSide,
    pub price_delta_raw: i64,
    pub price_acc_raw: i64,
    pub unknown_tail_raw: Option<i64>,
    pub reserved_zero: Option<i64>,
    pub record_hex: String,
    pub event_kind: TradeEventKind,
    pub auction_matched_volume: Option<i64>,
    pub auction_unmatched_signed_volume: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TradePage<R> {
    pub request: R,
    pub ticks: Vec<TradeTick>,
    pub price_base_raw_f32: Option<f32>,
    pub price_base_raw_bytes: Option<[u8; 4]>,
    pub raw_payload: Bytes,
}

pub fn parse_today_ticks_payload(
    payload: &[u8],
    request: TodayTicksRequest,
) -> Result<TradePage<TodayTicksRequest>, ProtocolError> {
    check_payload(payload, "today ticks")?;
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "today ticks",
            "invalid today ticks payload",
        ));
    }
    let record_count = usize::from(little_u16(&payload[..2])?);
    validate_record_count(record_count, payload.len() - 2, "today ticks")?;
    let (ticks, offset) = parse_tick_records(
        payload,
        2,
        record_count,
        request.start,
        None,
        TickTailKind::Unknown,
    )?;
    ensure_consumed(payload, offset, "today ticks")?;
    Ok(TradePage {
        request,
        ticks,
        price_base_raw_f32: None,
        price_base_raw_bytes: None,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_historical_ticks_payload(
    payload: &[u8],
    request: HistoricalTicksRequest,
) -> Result<TradePage<HistoricalTicksRequest>, ProtocolError> {
    check_payload(payload, "historical ticks")?;
    if payload.len() < 6 {
        return Err(ProtocolError::invalid_data(
            "historical ticks",
            "invalid historical ticks payload",
        ));
    }
    let record_count = usize::from(little_u16(&payload[..2])?);
    let price_base_raw_bytes: [u8; 4] = payload[2..6]
        .try_into()
        .map_err(|_| ProtocolError::invalid_data("historical ticks", "invalid price base"))?;
    let price_base_raw_f32 = little_f32(&price_base_raw_bytes)?;
    validate_record_count(record_count, payload.len() - 6, "historical ticks")?;
    let trading_date = request.trading_date;
    let (ticks, offset) = parse_tick_records(
        payload,
        6,
        record_count,
        request.start,
        Some(trading_date),
        TickTailKind::Reserved,
    )?;
    ensure_consumed(payload, offset, "historical ticks")?;
    Ok(TradePage {
        request,
        ticks,
        price_base_raw_f32: Some(price_base_raw_f32),
        price_base_raw_bytes: Some(price_base_raw_bytes),
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

#[derive(Clone, Copy)]
enum TickTailKind {
    Unknown,
    Reserved,
}

fn parse_tick_records(
    payload: &[u8],
    mut offset: usize,
    record_count: usize,
    start: u16,
    trading_date: Option<DateParts>,
    tail_kind: TickTailKind,
) -> Result<(Vec<TradeTick>, usize), ProtocolError> {
    let mut price_acc_raw = 0_i64;
    let mut ticks = Vec::with_capacity(record_count);
    for index in 0..record_count {
        let record_start = offset;
        let time_end = offset.saturating_add(2);
        let time_minutes =
            little_u16(payload.get(offset..time_end).ok_or_else(|| {
                ProtocolError::invalid_data("ticks", "truncated tick time field")
            })?)?;
        offset = time_end;
        let (price_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (volume, next) = consume_varint(payload, offset)?;
        offset = next;
        let (order_count, next) = consume_varint(payload, offset)?;
        offset = next;
        let (status_raw, next) = consume_varint(payload, offset)?;
        offset = next;
        let (tail_value, next) = consume_varint(payload, offset)?;
        offset = next;
        price_acc_raw = price_acc_raw
            .checked_add(price_delta_raw)
            .ok_or_else(|| ProtocolError::invalid_data("ticks", "tick price overflow"))?;
        let price = price_acc_raw as f64 / 100.0;
        let price_milli = python_round_to_i64(price * 1_000.0)?;
        let point_index = u16::try_from(index)
            .map_err(|_| ProtocolError::invalid_data("ticks", "tick index overflow"))?;
        let absolute_index = u32::from(start)
            .checked_add(u32::from(point_index))
            .ok_or_else(|| ProtocolError::invalid_data("ticks", "tick index overflow"))?;
        let trade_datetime = trading_date
            .map(|date| trade_datetime(date, time_minutes))
            .transpose()?;
        let event_kind = TradeEventKind::classify(status_raw, time_minutes);
        ticks.push(TradeTick {
            index: point_index,
            absolute_index,
            time_minutes,
            time_label: minute_of_day_label(time_minutes, None),
            trade_datetime,
            price,
            price_milli,
            volume,
            order_count,
            status_raw,
            side: TradeSide::from_raw(status_raw),
            price_delta_raw,
            price_acc_raw,
            unknown_tail_raw: match tail_kind {
                TickTailKind::Unknown => Some(tail_value),
                TickTailKind::Reserved => None,
            },
            reserved_zero: match tail_kind {
                TickTailKind::Unknown => None,
                TickTailKind::Reserved => Some(tail_value),
            },
            record_hex: encode_hex(&payload[record_start..offset]),
            event_kind,
            auction_matched_volume: if event_kind == TradeEventKind::AuctionSnapshot {
                Some(volume)
            } else {
                None
            },
            auction_unmatched_signed_volume: if event_kind == TradeEventKind::AuctionSnapshot {
                Some(order_count)
            } else {
                None
            },
        });
    }
    Ok((ticks, offset))
}

pub fn minute_of_day_label(value: u16, with_seconds: Option<u8>) -> String {
    let hour = value / 60;
    let minute = value % 60;
    match with_seconds {
        Some(second) => format!("{hour:02}:{minute:02}:{second:02}"),
        None => format!("{hour:02}:{minute:02}"),
    }
}

fn trade_datetime(
    trading_date: DateParts,
    time_minutes: u16,
) -> Result<DateTimeParts, ProtocolError> {
    let hour = u8::try_from(time_minutes / 60)
        .map_err(|_| ProtocolError::invalid_data("ticks", "invalid trade hour"))?;
    let minute = u8::try_from(time_minutes % 60)
        .map_err(|_| ProtocolError::invalid_data("ticks", "invalid trade minute"))?;
    DateTimeParts::naive(trading_date, hour, minute, 0)
}

fn validate_count(count: u16) -> Result<(), ProtocolError> {
    if count == 0 || count > MAX_TRADE_PAGE_SIZE {
        return Err(ProtocolError::invalid_argument(
            "count",
            format!("count must be between 1 and {MAX_TRADE_PAGE_SIZE}"),
        ));
    }
    Ok(())
}

fn validate_record_count(
    count: usize,
    available: usize,
    context: &'static str,
) -> Result<(), ProtocolError> {
    const MINIMUM_TICK_RECORD_SIZE: usize = 7;
    if count > available / MINIMUM_TICK_RECORD_SIZE {
        return Err(ProtocolError::invalid_data(
            context,
            format!("record count {count} exceeds payload capacity"),
        ));
    }
    Ok(())
}

fn check_payload(payload: &[u8], resource: &'static str) -> Result<(), ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource,
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

fn ensure_consumed(
    payload: &[u8],
    offset: usize,
    context: &'static str,
) -> Result<(), ProtocolError> {
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            context,
            format!(
                "unexpected trailing {context} payload bytes: {}",
                payload.len().saturating_sub(offset)
            ),
        ));
    }
    Ok(())
}

fn python_round_to_i64(value: f64) -> Result<i64, ProtocolError> {
    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_MAX_PLUS_ONE_F64: f64 = 9_223_372_036_854_775_808.0;
    if !value.is_finite() || !(I64_MIN_F64..I64_MAX_PLUS_ONE_F64).contains(&value) {
        return Err(ProtocolError::invalid_data(
            "ticks",
            "tick price cannot be represented as milli integer",
        ));
    }
    let truncated_float = value.trunc();
    let truncated = truncated_float as i64;
    let fraction = value - truncated_float;
    let magnitude = fraction.abs();
    if magnitude < 0.5 || (magnitude == 0.5 && truncated % 2 == 0) {
        return Ok(truncated);
    }
    if fraction.is_sign_negative() {
        truncated.checked_sub(1)
    } else {
        truncated.checked_add(1)
    }
    .ok_or_else(|| ProtocolError::invalid_data("ticks", "tick price overflow"))
}

fn encode_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(data.len().saturating_mul(2));
    for byte in data {
        output.push(char::from(DIGITS[usize::from(*byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(*byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        parse_historical_ticks_payload, parse_today_ticks_payload, python_round_to_i64,
        HistoricalTicksRequest, TodayTicksRequest, TradeEventKind, TradeSide,
    };
    use crate::unit::{DateParts, NormalizedCode};
    use crate::ProtocolError;

    #[test]
    fn tick_requests_match_frozen_wire_layouts() -> Result<(), ProtocolError> {
        let today = TodayTicksRequest::new(NormalizedCode::parse("sz000001")?, 0, 115)?.frame(1);
        let history = HistoricalTicksRequest::new(
            NormalizedCode::parse("sz300308")?,
            DateParts::new(2026, 5, 11)?,
            0,
            900,
        )?
        .frame(1);
        assert_eq!(
            &today.data[..],
            &[0, 0, b'0', b'0', b'0', b'0', b'0', b'1', 0, 0, 115, 0]
        );
        assert_eq!(
            &history.data[..],
            &[0x9f, 0x26, 0x35, 0x01, 0, 0, b'3', b'0', b'0', b'3', b'0', b'8', 0, 0, 0x84, 0x03]
        );
        Ok(())
    }

    #[test]
    fn plan_defaults_are_1800_for_both_tick_commands() -> Result<(), ProtocolError> {
        let request = TodayTicksRequest::with_plan_defaults(NormalizedCode::parse("sz000001")?);
        assert_eq!(request.count, 1_800);
        assert!(!request.include_raw);
        let history = HistoricalTicksRequest::new(
            NormalizedCode::parse("sz000001")?,
            DateParts::new(2026, 8, 15)?,
            0,
            1_800,
        )?;
        assert_eq!(history.count, 1_800);
        assert!(!history.include_raw);
        let today_raw = TodayTicksRequest::with_include_raw(
            NormalizedCode::parse("sz000001")?,
            0,
            1_800,
            true,
        )?;
        let history_raw = HistoricalTicksRequest::with_include_raw(
            NormalizedCode::parse("sz000001")?,
            DateParts::new(2026, 8, 15)?,
            0,
            1_800,
            true,
        )?;
        assert!(today_raw.include_raw);
        assert!(history_raw.include_raw);
        Ok(())
    }

    #[test]
    fn parses_regular_auction_and_historical_ticks() -> Result<(), ProtocolError> {
        let regular = parse_today_ticks_payload(
            &[1, 0, 0x50, 0x03, 0x0a, 0x14, 0x03, 0x00, 0x00],
            TodayTicksRequest::new(NormalizedCode::parse("sz000001")?, 0, 115)?,
        )?;
        assert_eq!(regular.ticks[0].time_label, "14:08");
        assert_eq!(regular.ticks[0].price_milli, 100);
        assert_eq!(regular.ticks[0].side, TradeSide::Buy);

        let auction = parse_today_ticks_payload(
            &[1, 0, 0x2b, 0x02, 0x00, 0x8f, 0x0f, 0x96, 0x03, 0x08, 0x00],
            TodayTicksRequest::new(NormalizedCode::parse("sz000001")?, 0, 115)?,
        )?;
        assert_eq!(auction.ticks[0].event_kind, TradeEventKind::AuctionSnapshot);
        assert_eq!(auction.ticks[0].auction_matched_volume, Some(975));
        assert_eq!(auction.ticks[0].auction_unmatched_signed_volume, Some(214));

        let mut history_payload = vec![1, 0];
        history_payload.extend_from_slice(&35.5_f32.to_le_bytes());
        history_payload.extend_from_slice(&[0x50, 0x03, 0x0a, 0x14, 0x03, 0x05, 0x00]);
        let history = parse_historical_ticks_payload(
            &history_payload,
            HistoricalTicksRequest::new(
                NormalizedCode::parse("sz300308")?,
                DateParts::new(2026, 5, 11)?,
                0,
                900,
            )?,
        )?;
        assert_eq!(history.price_base_raw_f32, Some(35.5));
        assert_eq!(history.ticks[0].side, TradeSide::Status(5));
        assert_eq!(history.ticks[0].reserved_zero, Some(0));
        assert!(matches!(
            history.ticks[0].trade_datetime,
            Some(value) if value.utc_offset_seconds.is_none()
        ));
        Ok(())
    }

    #[test]
    fn tick_price_rounding_is_python_half_even() -> Result<(), ProtocolError> {
        assert_eq!(python_round_to_i64(2.5)?, 2);
        assert_eq!(python_round_to_i64(3.5)?, 4);
        assert_eq!(python_round_to_i64(-2.5)?, -2);
        assert_eq!(python_round_to_i64(-3.5)?, -4);
        Ok(())
    }
}
