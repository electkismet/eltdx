use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{
    consume_price, consume_varint, little_f32, little_u16, little_u32, milli_to_float,
    minute_index_datetime, minute_index_label, price_divisor, DateParts, DateTimeParts, Market,
    NormalizedCode,
};

pub const TYPE_INTRADAY_AUX: u16 = 0x051b;
pub const TYPE_TODAY_INTRADAY: u16 = 0x0537;
pub const TYPE_HISTORICAL_INTRADAY: u16 = 0x0fb4;
pub const TYPE_SPARKLINE: u16 = 0x0fd1;
pub const TYPE_RECENT_INTRADAY: u16 = 0x0feb;

pub const RECENT_DATE_SELECTOR_BASE: u32 = 0xfed6_2304;
pub const DEFAULT_TODAY_RESERVED_TAIL: [u8; 4] = [0x00, 0x00, 0x00, 0x93];
pub const DEFAULT_SPARKLINE_SELECTOR: u8 = 1;
pub const DEFAULT_SPARKLINE_WINDOW: u16 = 20;
pub const DEFAULT_SPARKLINE_FIXED_RAW: u32 = 0x0100_0000;
const MAX_SELECTOR_TEXT_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodayIntradayRequest {
    pub code: NormalizedCode,
    pub reserved_tail_raw: [u8; 4],
    pub include_raw: bool,
}

impl TodayIntradayRequest {
    pub const fn new(code: NormalizedCode, reserved_tail_raw: [u8; 4]) -> Self {
        Self::with_include_raw(code, reserved_tail_raw, false)
    }

    pub const fn with_include_raw(
        code: NormalizedCode,
        reserved_tail_raw: [u8; 4],
        include_raw: bool,
    ) -> Self {
        Self {
            code,
            reserved_tail_raw,
            include_raw,
        }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&u16::from(self.code.market().id()).to_le_bytes());
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&self.reserved_tail_raw);
        RequestFrame::new(msg_id, TYPE_TODAY_INTRADAY, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoricalIntradayRequest {
    pub code: NormalizedCode,
    pub trading_date: DateParts,
    pub trading_date_raw: u32,
    pub include_raw: bool,
}

impl HistoricalIntradayRequest {
    pub fn new(code: NormalizedCode, trading_date: DateParts) -> Result<Self, ProtocolError> {
        Self::with_include_raw(code, trading_date, false)
    }

    pub fn with_include_raw(
        code: NormalizedCode,
        trading_date: DateParts,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        let trading_date = DateParts::new(trading_date.year, trading_date.month, trading_date.day)?;
        let trading_date_raw = trading_date.yyyymmdd()?;
        Ok(Self {
            code,
            trading_date,
            trading_date_raw,
            include_raw,
        })
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(11);
        data.extend_from_slice(&self.trading_date_raw.to_le_bytes());
        data.push(self.code.market().id());
        data.extend_from_slice(self.code.number().as_bytes());
        RequestFrame::new(msg_id, TYPE_HISTORICAL_INTRADAY, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentIntradayRequest {
    pub code: NormalizedCode,
    pub trading_date: DateParts,
    pub trading_date_raw: u32,
    pub date_selector_raw: u32,
    pub include_raw: bool,
}

impl RecentIntradayRequest {
    pub fn new(code: NormalizedCode, trading_date: DateParts) -> Result<Self, ProtocolError> {
        Self::with_include_raw(code, trading_date, false)
    }

    pub fn with_include_raw(
        code: NormalizedCode,
        trading_date: DateParts,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        let trading_date = DateParts::new(trading_date.year, trading_date.month, trading_date.day)?;
        let trading_date_raw = trading_date.yyyymmdd()?;
        let ordinal = python_date_ordinal(trading_date)?;
        let date_selector_raw =
            RECENT_DATE_SELECTOR_BASE
                .checked_sub(ordinal)
                .ok_or_else(|| {
                    ProtocolError::invalid_data("recent intraday", "recent date selector underflow")
                })?;
        Ok(Self {
            code,
            trading_date,
            trading_date_raw,
            date_selector_raw,
            include_raw,
        })
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(11);
        data.extend_from_slice(&self.date_selector_raw.to_le_bytes());
        data.push(self.code.market().id());
        data.extend_from_slice(self.code.number().as_bytes());
        RequestFrame::new(msg_id, TYPE_RECENT_INTRADAY, data)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntradayAuxKind {
    BuySellStrength,
    VolumeComparison,
    Unknown(u8),
}

impl IntradayAuxKind {
    pub const fn from_raw(value: u8) -> Self {
        match value {
            0x00 => Self::BuySellStrength,
            0x0b => Self::VolumeComparison,
            other => Self::Unknown(other),
        }
    }

    pub const fn raw(self) -> u8 {
        match self {
            Self::BuySellStrength => 0x00,
            Self::VolumeComparison => 0x0b,
            Self::Unknown(value) => value,
        }
    }

    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::VolumeComparison => "volume_comparison",
            Self::BuySellStrength | Self::Unknown(_) => "buy_sell_strength",
        }
    }

    pub fn normalize(value: &str) -> Result<Self, ProtocolError> {
        let trimmed = value.trim();
        if trimmed.len() > MAX_SELECTOR_TEXT_BYTES {
            return Err(ProtocolError::LimitExceeded {
                resource: "selector",
                actual: trimmed.len(),
                limit: MAX_SELECTOR_TEXT_BYTES,
            });
        }
        let key = trimmed.to_ascii_lowercase();
        match key.as_str() {
            "buy_sell_strength" | "buy_sell" | "commission" => Ok(Self::BuySellStrength),
            "volume_comparison" | "volume_compare" => Ok(Self::VolumeComparison),
            _ => parse_selector_text(&key).map(Self::from_raw),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntradayAuxRequest {
    pub code: NormalizedCode,
    pub kind: IntradayAuxKind,
    pub include_raw: bool,
}

impl IntradayAuxRequest {
    pub const fn new(code: NormalizedCode, kind: IntradayAuxKind) -> Self {
        Self::with_include_raw(code, kind, false)
    }

    pub const fn with_include_raw(
        code: NormalizedCode,
        kind: IntradayAuxKind,
        include_raw: bool,
    ) -> Self {
        Self {
            code,
            kind,
            include_raw,
        }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(28);
        data.extend_from_slice(&u16::from(self.code.market().id()).to_le_bytes());
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&[0_u8; 19]);
        data.push(self.kind.raw());
        RequestFrame::new(msg_id, TYPE_INTRADAY_AUX, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparklineRequest {
    pub code: NormalizedCode,
    pub selector: u8,
    pub window_or_count_raw: u16,
    pub fixed_raw: u32,
    pub include_raw: bool,
}

impl SparklineRequest {
    pub const fn new(
        code: NormalizedCode,
        selector: u8,
        window_or_count_raw: u16,
        fixed_raw: u32,
    ) -> Self {
        Self::with_include_raw(code, selector, window_or_count_raw, fixed_raw, false)
    }

    pub const fn with_include_raw(
        code: NormalizedCode,
        selector: u8,
        window_or_count_raw: u16,
        fixed_raw: u32,
        include_raw: bool,
    ) -> Self {
        Self {
            code,
            selector,
            window_or_count_raw,
            fixed_raw,
            include_raw,
        }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(37);
        data.extend_from_slice(&[self.code.market().id(), 0]);
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&[0_u8; 16]);
        data.extend_from_slice(&[self.selector, 0]);
        data.extend_from_slice(&self.window_or_count_raw.to_le_bytes());
        data.extend_from_slice(&self.fixed_raw.to_le_bytes());
        data.extend_from_slice(&[0_u8; 5]);
        RequestFrame::new(msg_id, TYPE_SPARKLINE, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MinutePoint {
    pub index: u16,
    pub time_label: String,
    pub time: Option<DateTimeParts>,
    pub price: f64,
    pub price_milli: i64,
    pub volume: i64,
    pub price_field: Option<i64>,
    pub avg_field: Option<i64>,
    pub avg_price: Option<f64>,
    pub price_raw: Option<i64>,
    pub avg_raw: Option<i64>,
    pub price_delta_raw: Option<i64>,
    pub aux_delta_raw: Option<i64>,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MinuteSeries<R> {
    pub request: R,
    pub points: Vec<MinutePoint>,
    pub reserved_zero: Option<u16>,
    pub prev_close: Option<f32>,
    pub prev_close_raw: Option<[u8; 4]>,
    pub open_price: Option<f32>,
    pub open_price_raw: Option<[u8; 4]>,
    pub date_selector_raw: Option<u32>,
    pub raw_payload: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MinuteAuxPoint {
    BuySellStrength {
        index: u16,
        time_label: String,
        series_a: i64,
        series_b: i64,
        record_hex: String,
    },
    VolumeComparison {
        index: u16,
        time_label: String,
        previous_day_cumulative_volume: f32,
        previous_day_raw: [u8; 4],
        current_day_cumulative_volume: f32,
        current_day_raw: [u8; 4],
        record_hex: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MinuteAuxSeries {
    pub request: IntradayAuxRequest,
    pub response_kind: IntradayAuxKind,
    pub points: Vec<MinuteAuxPoint>,
    pub raw_payload: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SparklinePrice {
    pub value: f32,
    pub raw: [u8; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct SparklineSeries {
    pub request: SparklineRequest,
    pub response_market_id: u8,
    pub response_market: Market,
    pub response_code: String,
    pub selector_echo: u8,
    pub reserved_param_u32: u32,
    pub max_count_raw: u16,
    pub base_price: f32,
    pub base_price_raw: [u8; 4],
    pub prices: Vec<SparklinePrice>,
    pub raw_payload: Bytes,
}

pub fn parse_today_intraday_payload(
    payload: &[u8],
    request: TodayIntradayRequest,
) -> Result<MinuteSeries<TodayIntradayRequest>, ProtocolError> {
    check_payload(payload, "today intraday")?;
    if payload.len() < 4 {
        return Err(ProtocolError::invalid_data(
            "today intraday",
            "invalid today intraday payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let reserved_zero = little_u16(&payload[2..4])?;
    validate_variable_count(count, payload.len() - 4, 3, "today intraday")?;

    let mut offset = 4;
    let mut first_price = None;
    let mut first_avg = None;
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let record_start = offset;
        let (price_field, next) = consume_price(payload, offset)?;
        offset = next;
        let (avg_field, next) = consume_price(payload, offset)?;
        offset = next;
        let (volume, next) = consume_varint(payload, offset)?;
        offset = next;

        if index == 0 {
            first_price = Some(price_field);
            first_avg = Some(avg_field);
        }
        let price_raw = if index == 0 {
            price_field
        } else {
            checked_add(
                first_price.ok_or_else(|| missing_base("today intraday price"))?,
                price_field,
                "today intraday price",
            )?
        };
        let avg_raw = if index == 0 {
            avg_field
        } else {
            checked_add(
                first_avg.ok_or_else(|| missing_base("today intraday average"))?,
                avg_field,
                "today intraday average",
            )?
        };
        points.push(relative_minute_point(
            index,
            None,
            price_field,
            avg_field,
            volume,
            price_raw,
            avg_raw,
            i64::from(price_divisor(&request.code)),
            &payload[record_start..offset],
        )?);
    }
    ensure_consumed(payload, offset, "today intraday")?;
    Ok(MinuteSeries {
        request,
        points,
        reserved_zero: Some(reserved_zero),
        prev_close: None,
        prev_close_raw: None,
        open_price: None,
        open_price_raw: None,
        date_selector_raw: None,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_historical_intraday_payload(
    payload: &[u8],
    request: HistoricalIntradayRequest,
) -> Result<MinuteSeries<HistoricalIntradayRequest>, ProtocolError> {
    check_payload(payload, "historical intraday")?;
    if payload.len() < 6 {
        return Err(ProtocolError::invalid_data(
            "historical intraday",
            "invalid historical intraday payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let (prev_close, prev_close_raw) = read_f32(payload, 2, "historical prev close")?;
    validate_variable_count(count, payload.len() - 6, 3, "historical intraday")?;

    let mut offset = 6;
    let mut price_acc_raw = 0_i64;
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let record_start = offset;
        let (price_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (aux_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (volume, next) = consume_varint(payload, offset)?;
        offset = next;
        price_acc_raw = checked_add(price_acc_raw, price_delta_raw, "historical intraday price")?;
        let price_milli = checked_scale(
            price_acc_raw,
            i64::from(price_divisor(&request.code)),
            "historical intraday price",
        )?;
        let point_index = checked_index(index)?;
        let minute_index = i64::from(point_index);
        points.push(MinutePoint {
            index: point_index,
            time_label: minute_index_label(minute_index)?,
            time: Some(minute_index_datetime(request.trading_date, minute_index)?),
            price: milli_to_float(price_milli),
            price_milli,
            volume,
            price_field: None,
            avg_field: None,
            avg_price: None,
            price_raw: Some(price_acc_raw),
            avg_raw: None,
            price_delta_raw: Some(price_delta_raw),
            aux_delta_raw: Some(aux_delta_raw),
            record_hex: encode_hex(&payload[record_start..offset]),
        });
    }
    ensure_consumed(payload, offset, "historical intraday")?;
    Ok(MinuteSeries {
        request,
        points,
        reserved_zero: None,
        prev_close: Some(prev_close),
        prev_close_raw: Some(prev_close_raw),
        open_price: None,
        open_price_raw: None,
        date_selector_raw: None,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_recent_intraday_payload(
    payload: &[u8],
    request: RecentIntradayRequest,
) -> Result<MinuteSeries<RecentIntradayRequest>, ProtocolError> {
    check_payload(payload, "recent intraday")?;
    if payload.len() < 10 {
        return Err(ProtocolError::invalid_data(
            "recent intraday",
            "invalid recent intraday payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let (prev_close, prev_close_raw) = read_f32(payload, 2, "recent prev close")?;
    let (open_price, open_price_raw) = read_f32(payload, 6, "recent open price")?;
    validate_variable_count(count, payload.len() - 10, 3, "recent intraday")?;

    let mut offset = 10;
    let mut first_price = None;
    let mut first_avg = None;
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let record_start = offset;
        let (price_field, next) = consume_price(payload, offset)?;
        offset = next;
        let (avg_field, next) = consume_price(payload, offset)?;
        offset = next;
        let (volume, next) = consume_varint(payload, offset)?;
        offset = next;
        if index == 0 {
            first_price = Some(price_field);
            first_avg = Some(avg_field);
        }
        let price_raw = if index == 0 {
            price_field
        } else {
            checked_add(
                first_price.ok_or_else(|| missing_base("recent intraday price"))?,
                price_field,
                "recent intraday price",
            )?
        };
        let avg_raw = if index == 0 {
            avg_field
        } else {
            checked_add(
                first_avg.ok_or_else(|| missing_base("recent intraday average"))?,
                avg_field,
                "recent intraday average",
            )?
        };
        let point_index = checked_index(index)?;
        points.push(relative_minute_point(
            index,
            Some(minute_index_datetime(
                request.trading_date,
                i64::from(point_index),
            )?),
            price_field,
            avg_field,
            volume,
            price_raw,
            avg_raw,
            i64::from(price_divisor(&request.code)),
            &payload[record_start..offset],
        )?);
    }
    ensure_consumed(payload, offset, "recent intraday")?;
    let date_selector_raw = request.date_selector_raw;
    Ok(MinuteSeries {
        request,
        points,
        reserved_zero: None,
        prev_close: Some(prev_close),
        prev_close_raw: Some(prev_close_raw),
        open_price: Some(open_price),
        open_price_raw: Some(open_price_raw),
        date_selector_raw: Some(date_selector_raw),
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_intraday_aux_payload(
    payload: &[u8],
    request: IntradayAuxRequest,
) -> Result<MinuteAuxSeries, ProtocolError> {
    check_payload(payload, "intraday aux")?;
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "intraday aux",
            "invalid intraday aux payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let response_kind = IntradayAuxKind::from_raw(request.kind.raw());
    let mut points = Vec::with_capacity(count);
    let mut offset = 2;
    if response_kind == IntradayAuxKind::VolumeComparison {
        let records_length = count.checked_mul(8).ok_or_else(|| {
            ProtocolError::invalid_data("intraday aux", "intraday aux length overflow")
        })?;
        let expected_length = 2_usize.checked_add(records_length).ok_or_else(|| {
            ProtocolError::invalid_data("intraday aux", "intraday aux length overflow")
        })?;
        if payload.len() != expected_length {
            return Err(ProtocolError::invalid_data(
                "intraday aux",
                format!(
                    "invalid intraday volume comparison length: expected {expected_length}, got {}",
                    payload.len()
                ),
            ));
        }
        for index in 0..count {
            let record_start = offset;
            let (previous_day, previous_day_raw) =
                read_f32(payload, offset, "previous day cumulative volume")?;
            offset += 4;
            let (current_day, current_day_raw) =
                read_f32(payload, offset, "current day cumulative volume")?;
            offset += 4;
            let point_index = checked_index(index)?;
            points.push(MinuteAuxPoint::VolumeComparison {
                index: point_index,
                time_label: minute_index_label(i64::from(point_index))?,
                previous_day_cumulative_volume: previous_day,
                previous_day_raw,
                current_day_cumulative_volume: current_day,
                current_day_raw,
                record_hex: encode_hex(&payload[record_start..offset]),
            });
        }
    } else {
        validate_variable_count(count, payload.len() - 2, 2, "intraday aux")?;
        for index in 0..count {
            let record_start = offset;
            let (series_a, next) = consume_varint(payload, offset)?;
            offset = next;
            let (series_b, next) = consume_varint(payload, offset)?;
            offset = next;
            let point_index = checked_index(index)?;
            points.push(MinuteAuxPoint::BuySellStrength {
                index: point_index,
                time_label: minute_index_label(i64::from(point_index))?,
                series_a,
                series_b,
                record_hex: encode_hex(&payload[record_start..offset]),
            });
        }
        ensure_consumed(payload, offset, "intraday aux")?;
    }
    Ok(MinuteAuxSeries {
        request,
        response_kind,
        points,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_sparkline_payload(
    payload: &[u8],
    request: SparklineRequest,
) -> Result<SparklineSeries, ProtocolError> {
    check_payload(payload, "sparkline")?;
    if payload.len() < 42 {
        return Err(ProtocolError::invalid_data(
            "sparkline",
            "invalid sparkline payload",
        ));
    }
    let response_market_id = payload[0];
    let response_market = match Market::from_id(i64::from(response_market_id)) {
        Ok(market) => market,
        Err(_) => request.code.market(),
    };
    let code_raw = payload
        .get(2..8)
        .ok_or_else(|| ProtocolError::invalid_data("sparkline", "truncated sparkline code"))?;
    let response_code = std::str::from_utf8(code_raw)
        .map_err(|_| ProtocolError::invalid_data("sparkline", "invalid ASCII sparkline code"))?
        .to_owned();
    let selector_echo = payload[24];
    let reserved_param_u32 = little_u32(&payload[26..30])?;
    let max_count_raw = little_u16(&payload[34..36])?;
    let (base_price, base_price_raw) = read_f32(payload, 36, "sparkline base price")?;
    let price_count = usize::from(little_u16(&payload[40..42])?);
    let prices_length = price_count
        .checked_mul(4)
        .ok_or_else(|| ProtocolError::invalid_data("sparkline", "sparkline length overflow"))?;
    let expected_length = 42_usize
        .checked_add(prices_length)
        .ok_or_else(|| ProtocolError::invalid_data("sparkline", "sparkline length overflow"))?;
    if payload.len() != expected_length {
        return Err(ProtocolError::invalid_data(
            "sparkline",
            format!(
                "invalid sparkline length: expected {expected_length}, got {}",
                payload.len()
            ),
        ));
    }
    let mut prices = Vec::with_capacity(price_count);
    let mut offset = 42;
    for _ in 0..price_count {
        let (value, raw) = read_f32(payload, offset, "sparkline price")?;
        prices.push(SparklinePrice { value, raw });
        offset += 4;
    }
    Ok(SparklineSeries {
        request,
        response_market_id,
        response_market,
        response_code,
        selector_echo,
        reserved_param_u32,
        max_count_raw,
        base_price,
        base_price_raw,
        prices,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

#[allow(clippy::too_many_arguments)]
fn relative_minute_point(
    index: usize,
    time: Option<DateTimeParts>,
    price_field: i64,
    avg_field: i64,
    volume: i64,
    price_raw: i64,
    avg_raw: i64,
    divisor: i64,
    record: &[u8],
) -> Result<MinutePoint, ProtocolError> {
    let index = checked_index(index)?;
    let price_milli = checked_scale(price_raw, divisor, "intraday price")?;
    Ok(MinutePoint {
        index,
        time_label: minute_index_label(i64::from(index))?,
        time,
        price: milli_to_float(price_milli),
        price_milli,
        volume,
        price_field: Some(price_field),
        avg_field: Some(avg_field),
        avg_price: Some(avg_raw as f64 / (10_000.0 * divisor as f64)),
        price_raw: Some(price_raw),
        avg_raw: Some(avg_raw),
        price_delta_raw: None,
        aux_delta_raw: None,
        record_hex: encode_hex(record),
    })
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

fn validate_variable_count(
    count: usize,
    available: usize,
    minimum_record_size: usize,
    context: &'static str,
) -> Result<(), ProtocolError> {
    if count > available / minimum_record_size {
        return Err(ProtocolError::invalid_data(
            context,
            format!("record count {count} exceeds payload capacity"),
        ));
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

fn read_f32(
    payload: &[u8],
    offset: usize,
    context: &'static str,
) -> Result<(f32, [u8; 4]), ProtocolError> {
    let end = offset.saturating_add(4);
    let bytes: [u8; 4] = payload
        .get(offset..end)
        .ok_or_else(|| ProtocolError::invalid_data(context, "truncated f32 field"))?
        .try_into()
        .map_err(|_| ProtocolError::invalid_data(context, "invalid f32 field length"))?;
    Ok((little_f32(&bytes)?, bytes))
}

fn checked_index(index: usize) -> Result<u16, ProtocolError> {
    u16::try_from(index)
        .map_err(|_| ProtocolError::invalid_data("minute index", "minute index overflow"))
}

fn checked_add(left: i64, right: i64, context: &'static str) -> Result<i64, ProtocolError> {
    left.checked_add(right)
        .ok_or_else(|| ProtocolError::invalid_data(context, "price overflow"))
}

fn checked_scale(value: i64, divisor: i64, context: &'static str) -> Result<i64, ProtocolError> {
    if divisor <= 0 {
        return Err(ProtocolError::invalid_data(
            context,
            "invalid price divisor",
        ));
    }
    let scaled = value
        .checked_mul(10)
        .ok_or_else(|| ProtocolError::invalid_data(context, "price overflow"))?;
    let quotient = scaled / divisor;
    let remainder = scaled % divisor;
    Ok(if remainder < 0 {
        quotient - 1
    } else {
        quotient
    })
}

fn missing_base(context: &'static str) -> ProtocolError {
    ProtocolError::invalid_data(context, "missing first record base")
}

fn parse_selector_text(value: &str) -> Result<u8, ProtocolError> {
    let parsed = if let Some(hex) = value.strip_prefix("0x") {
        u16::from_str_radix(hex, 16)
    } else {
        value.parse::<u16>()
    }
    .map_err(|_| {
        ProtocolError::invalid_argument(
            "selector",
            format!("invalid intraday aux selector: {value:?}"),
        )
    })?;
    u8::try_from(parsed).map_err(|_| {
        ProtocolError::invalid_argument(
            "selector",
            format!("intraday aux selector out of range: {parsed}"),
        )
    })
}

fn python_date_ordinal(date: DateParts) -> Result<u32, ProtocolError> {
    let validated = DateParts::new(date.year, date.month, date.day)?;
    let previous_year = u32::try_from(validated.year - 1)
        .map_err(|_| ProtocolError::invalid_data("date", "invalid ordinal year"))?;
    let mut ordinal = previous_year
        .checked_mul(365)
        .and_then(|value| value.checked_add(previous_year / 4))
        .and_then(|value| value.checked_sub(previous_year / 100))
        .and_then(|value| value.checked_add(previous_year / 400))
        .ok_or_else(|| ProtocolError::invalid_data("date", "date ordinal overflow"))?;
    const DAYS_BEFORE_MONTH: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let month_index = usize::from(validated.month.saturating_sub(1));
    let before_month = DAYS_BEFORE_MONTH
        .get(month_index)
        .copied()
        .ok_or_else(|| ProtocolError::invalid_data("date", "invalid ordinal month"))?;
    ordinal = ordinal
        .checked_add(u32::from(before_month))
        .and_then(|value| value.checked_add(u32::from(validated.day)))
        .ok_or_else(|| ProtocolError::invalid_data("date", "date ordinal overflow"))?;
    if validated.month > 2 && is_leap_year(validated.year) {
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| ProtocolError::invalid_data("date", "date ordinal overflow"))?;
    }
    Ok(ordinal)
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
        parse_historical_intraday_payload, parse_intraday_aux_payload,
        parse_recent_intraday_payload, parse_sparkline_payload, parse_today_intraday_payload,
        HistoricalIntradayRequest, IntradayAuxKind, IntradayAuxRequest, MinuteAuxPoint,
        RecentIntradayRequest, SparklineRequest, TodayIntradayRequest, DEFAULT_SPARKLINE_FIXED_RAW,
        DEFAULT_SPARKLINE_SELECTOR, DEFAULT_SPARKLINE_WINDOW, DEFAULT_TODAY_RESERVED_TAIL,
    };
    use crate::unit::{DateParts, NormalizedCode};
    use crate::ProtocolError;

    #[test]
    fn all_five_request_layouts_match_frozen_wire_data() -> Result<(), ProtocolError> {
        let date = DateParts::new(2026, 5, 11)?;
        let today = TodayIntradayRequest::new(
            NormalizedCode::parse("sz000001")?,
            DEFAULT_TODAY_RESERVED_TAIL,
        )
        .frame(1);
        let history =
            HistoricalIntradayRequest::new(NormalizedCode::parse("sz300308")?, date)?.frame(1);
        let recent = RecentIntradayRequest::new(NormalizedCode::parse("sz300308")?, date)?.frame(1);
        let aux = IntradayAuxRequest::new(
            NormalizedCode::parse("sz000988")?,
            IntradayAuxKind::VolumeComparison,
        )
        .frame(1);
        let sparkline = SparklineRequest::new(
            NormalizedCode::parse("sz000001")?,
            DEFAULT_SPARKLINE_SELECTOR,
            DEFAULT_SPARKLINE_WINDOW,
            DEFAULT_SPARKLINE_FIXED_RAW,
        )
        .frame(1);

        assert_eq!(
            &today.data[..],
            &bytes_from_hex("000030303030303100000093")?
        );
        assert_eq!(
            &history.data[..],
            &bytes_from_hex("9f26350100333030333038")?
        );
        assert_eq!(&recent.data[..], &bytes_from_hex("61d9cafe00333030333038")?);
        assert_eq!(aux.data.len(), 28);
        assert_eq!(aux.data[27], 0x0b);
        assert_eq!(sparkline.data.len(), 37);
        assert_eq!(&sparkline.data[24..32], &[1, 0, 20, 0, 0, 0, 0, 1]);
        assert!(
            !TodayIntradayRequest::new(
                NormalizedCode::parse("sz000001")?,
                DEFAULT_TODAY_RESERVED_TAIL,
            )
            .include_raw
        );
        assert!(
            !HistoricalIntradayRequest::new(NormalizedCode::parse("sz300308")?, date,)?.include_raw
        );
        assert!(!RecentIntradayRequest::new(NormalizedCode::parse("sz300308")?, date)?.include_raw);
        assert!(
            !IntradayAuxRequest::new(
                NormalizedCode::parse("sz000988")?,
                IntradayAuxKind::VolumeComparison,
            )
            .include_raw
        );
        assert!(
            !SparklineRequest::new(
                NormalizedCode::parse("sz000001")?,
                DEFAULT_SPARKLINE_SELECTOR,
                DEFAULT_SPARKLINE_WINDOW,
                DEFAULT_SPARKLINE_FIXED_RAW,
            )
            .include_raw
        );
        Ok(())
    }

    #[test]
    fn all_five_requests_retain_explicit_raw_context() -> Result<(), ProtocolError> {
        let date = DateParts::new(2026, 5, 11)?;
        assert!(
            TodayIntradayRequest::with_include_raw(
                NormalizedCode::parse("sz000001")?,
                DEFAULT_TODAY_RESERVED_TAIL,
                true,
            )
            .include_raw
        );
        assert!(
            HistoricalIntradayRequest::with_include_raw(
                NormalizedCode::parse("sz300308")?,
                date,
                true,
            )?
            .include_raw
        );
        assert!(
            RecentIntradayRequest::with_include_raw(
                NormalizedCode::parse("sz300308")?,
                date,
                true,
            )?
            .include_raw
        );
        assert!(
            IntradayAuxRequest::with_include_raw(
                NormalizedCode::parse("sz000988")?,
                IntradayAuxKind::VolumeComparison,
                true,
            )
            .include_raw
        );
        assert!(
            SparklineRequest::with_include_raw(
                NormalizedCode::parse("sz000001")?,
                DEFAULT_SPARKLINE_SELECTOR,
                DEFAULT_SPARKLINE_WINDOW,
                DEFAULT_SPARKLINE_FIXED_RAW,
                true,
            )
            .include_raw
        );
        Ok(())
    }

    #[test]
    fn parses_today_recent_and_historical_price_semantics() -> Result<(), ProtocolError> {
        let code = NormalizedCode::parse("sz000001")?;
        let today_request = TodayIntradayRequest::new(code.clone(), DEFAULT_TODAY_RESERVED_TAIL);
        let today =
            parse_today_intraday_payload(&[2, 0, 0, 0, 10, 11, 12, 1, 2, 3], today_request)?;
        assert_eq!(today.points[0].price_milli, 100);
        assert_eq!(today.points[1].price_milli, 110);
        assert_eq!(today.points[1].avg_raw, Some(13));

        let date = DateParts::new(2026, 5, 11)?;
        let recent_request = RecentIntradayRequest::new(code.clone(), date)?;
        let mut recent_payload = vec![1, 0];
        recent_payload.extend_from_slice(&10.0_f32.to_le_bytes());
        recent_payload.extend_from_slice(&10.1_f32.to_le_bytes());
        recent_payload.extend_from_slice(&[10, 11, 12]);
        let recent = parse_recent_intraday_payload(&recent_payload, recent_request)?;
        assert_eq!(recent.points[0].price_milli, 100);
        assert_eq!(recent.points[0].avg_raw, Some(11));

        let history_request = HistoricalIntradayRequest::new(code, date)?;
        let mut history_payload = vec![2, 0];
        history_payload.extend_from_slice(&10.0_f32.to_le_bytes());
        history_payload.extend_from_slice(&[10, 0, 1, 2, 0, 1]);
        let history = parse_historical_intraday_payload(&history_payload, history_request)?;
        assert_eq!(history.points[0].price_milli, 100);
        assert_eq!(history.points[1].price_milli, 120);
        assert_eq!(history.points[1].time_label, "09:32");
        Ok(())
    }

    #[test]
    fn parses_both_aux_record_families() -> Result<(), ProtocolError> {
        let code = NormalizedCode::parse("sz000988")?;
        let strength = parse_intraday_aux_payload(
            &[1, 0, 5, 6],
            IntradayAuxRequest::new(code.clone(), IntradayAuxKind::BuySellStrength),
        )?;
        assert!(matches!(
            strength.points.first(),
            Some(MinuteAuxPoint::BuySellStrength {
                series_a: 5,
                series_b: 6,
                ..
            })
        ));

        let mut volume_payload = vec![1, 0];
        volume_payload.extend_from_slice(&100.5_f32.to_le_bytes());
        volume_payload.extend_from_slice(&120.5_f32.to_le_bytes());
        let volume = parse_intraday_aux_payload(
            &volume_payload,
            IntradayAuxRequest::new(code, IntradayAuxKind::VolumeComparison),
        )?;
        assert!(matches!(
            volume.points.first(),
            Some(MinuteAuxPoint::VolumeComparison {
                previous_day_cumulative_volume,
                current_day_cumulative_volume,
                ..
            }) if *previous_day_cumulative_volume == 100.5
                && *current_day_cumulative_volume == 120.5
        ));
        Ok(())
    }

    #[test]
    fn parses_exact_sparkline_layout_and_raw_floats() -> Result<(), ProtocolError> {
        let request = SparklineRequest::new(
            NormalizedCode::parse("sz000001")?,
            1,
            20,
            DEFAULT_SPARKLINE_FIXED_RAW,
        );
        let mut payload = vec![0, 0];
        payload.extend_from_slice(b"000001");
        payload.extend_from_slice(&[0_u8; 16]);
        payload.extend_from_slice(&[1, 0]);
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&60_u16.to_le_bytes());
        payload.extend_from_slice(&10.0_f32.to_le_bytes());
        payload.extend_from_slice(&2_u16.to_le_bytes());
        payload.extend_from_slice(&10.0_f32.to_le_bytes());
        payload.extend_from_slice(&10.1_f32.to_le_bytes());

        let parsed = parse_sparkline_payload(&payload, request)?;
        assert_eq!(parsed.response_code, "000001");
        assert_eq!(parsed.max_count_raw, 60);
        assert_eq!(parsed.prices.len(), 2);
        assert_eq!(parsed.prices[1].raw, 10.1_f32.to_le_bytes());
        Ok(())
    }

    fn bytes_from_hex(value: &str) -> Result<Vec<u8>, ProtocolError> {
        if value.len() % 2 != 0 {
            return Err(ProtocolError::invalid_data("test hex", "odd hex length"));
        }
        let mut output = Vec::with_capacity(value.len() / 2);
        let mut offset = 0;
        while offset < value.len() {
            let byte = u8::from_str_radix(&value[offset..offset + 2], 16)
                .map_err(|_| ProtocolError::invalid_data("test hex", "invalid hex"))?;
            output.push(byte);
            offset += 2;
        }
        Ok(output)
    }
}
