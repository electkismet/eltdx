use std::collections::HashMap;
use std::fmt;
use std::sync::{OnceLock, RwLock};

use encoding_rs::{DecoderResult, GBK};

use crate::error::ProtocolError;
use crate::limits::MAX_VARINT_BYTES;

pub const SHANGHAI_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const PRICING_ETF_PREFIXES: [&str; 8] = ["15", "16", "50", "51", "52", "53", "56", "58"];
const MAX_NORMALIZED_ARGUMENT_BYTES: usize = 64;

static SECURITY_DECIMALS: OnceLock<RwLock<HashMap<String, u8>>> = OnceLock::new();

fn security_decimals() -> &'static RwLock<HashMap<String, u8>> {
    SECURITY_DECIMALS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register precision read from the 0x044d security table.
pub fn register_security_decimal(code: &str, decimal: u8) {
    if let Ok(mut values) = security_decimals().write() {
        values.insert(code.to_ascii_lowercase(), decimal);
    }
}

pub fn security_decimal(code: &NormalizedCode) -> Option<u8> {
    security_decimals()
        .read()
        .ok()
        .and_then(|values| values.get(&code.full_code()).copied())
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Market {
    Shenzhen = 0,
    Shanghai = 1,
    Beijing = 2,
}

impl Market {
    pub const fn id(self) -> u8 {
        self as u8
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shenzhen => "sz",
            Self::Shanghai => "sh",
            Self::Beijing => "bj",
        }
    }

    pub fn from_id(value: i64) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::Shenzhen),
            1 => Ok(Self::Shanghai),
            2 => Ok(Self::Beijing),
            _ => Err(ProtocolError::invalid_argument(
                "market",
                format!("invalid market id: {value}"),
            )),
        }
    }

    pub fn normalize(value: &str) -> Result<Self, ProtocolError> {
        let trimmed = bounded_argument(value, "market")?;
        let text = trimmed.to_lowercase();
        match text.as_str() {
            "sz" | "0" | "sza" | "深市" => Ok(Self::Shenzhen),
            "sh" | "1" | "sha" | "沪市" => Ok(Self::Shanghai),
            "bj" | "2" | "bse" | "北交所" => Ok(Self::Beijing),
            _ => Err(ProtocolError::invalid_argument(
                "market",
                format!("invalid market: {value:?}"),
            )),
        }
    }
}

impl fmt::Display for Market {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NormalizedCode {
    pub(crate) market: Market,
    pub(crate) number: String,
}

impl NormalizedCode {
    pub fn parse(value: &str) -> Result<Self, ProtocolError> {
        let trimmed = value.trim();
        if !matches!(trimmed.len(), 6 | 8) || !trimmed.is_ascii() {
            return Err(invalid_code(value));
        }
        let text = trimmed.to_lowercase();
        let (market, number) = if text.len() == 8 {
            let (prefix, number) = text.split_at(2);
            (Some(Market::normalize(prefix)?), number)
        } else if text.len() == 6 {
            (None, text.as_str())
        } else {
            return Err(invalid_code(value));
        };

        if number.len() != 6 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid_code(value));
        }

        let normalized_market = match market {
            Some(explicit) => explicit,
            None => match number.as_bytes().first().copied() {
                Some(b'9') if number.starts_with("92") => Market::Beijing,
                Some(b'6' | b'9') => Market::Shanghai,
                Some(b'0' | b'1' | b'2' | b'3') => Market::Shenzhen,
                Some(b'8') => Market::Beijing,
                _ => {
                    return Err(ProtocolError::invalid_argument(
                        "code",
                        format!("unable to infer market for code: {value:?}"),
                    ));
                }
            },
        };

        Ok(Self {
            market: normalized_market,
            number: number.to_owned(),
        })
    }

    pub fn full_code(&self) -> String {
        let mut value = String::with_capacity(8);
        value.push_str(self.market.as_str());
        value.push_str(&self.number);
        value
    }

    pub const fn market(&self) -> Market {
        self.market
    }

    pub fn number(&self) -> &str {
        &self.number
    }
}

impl fmt::Display for NormalizedCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.market, self.number)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DateParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl DateParts {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, ProtocolError> {
        if !is_valid_date(year, month, day) {
            return Err(ProtocolError::invalid_data(
                "date",
                format!("invalid date: {year:04}-{month:02}-{day:02}"),
            ));
        }
        Ok(Self { year, month, day })
    }

    pub fn from_yyyymmdd(raw: u32) -> Option<Self> {
        let year = i32::try_from(raw / 10_000).ok()?;
        let month = u8::try_from((raw / 100) % 100).ok()?;
        let day = u8::try_from(raw % 100).ok()?;
        if is_valid_date(year, month, day) {
            Some(Self { year, month, day })
        } else {
            None
        }
    }

    pub fn yyyymmdd(self) -> Result<u32, ProtocolError> {
        let validated = Self::new(self.year, self.month, self.day)?;
        let year = u32::try_from(validated.year)
            .map_err(|_| ProtocolError::invalid_data("date", "invalid year"))?;
        Ok(year * 10_000 + u32::from(validated.month) * 100 + u32::from(validated.day))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DateTimeParts {
    pub date: DateParts,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub utc_offset_seconds: Option<i32>,
}

impl DateTimeParts {
    pub fn shanghai(
        date: DateParts,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Self, ProtocolError> {
        Self::new(
            date,
            hour,
            minute,
            second,
            Some(SHANGHAI_UTC_OFFSET_SECONDS),
        )
    }

    pub fn naive(date: DateParts, hour: u8, minute: u8, second: u8) -> Result<Self, ProtocolError> {
        Self::new(date, hour, minute, second, None)
    }

    fn new(
        date: DateParts,
        hour: u8,
        minute: u8,
        second: u8,
        utc_offset_seconds: Option<i32>,
    ) -> Result<Self, ProtocolError> {
        DateParts::new(date.year, date.month, date.day)?;
        if hour > 23 || minute > 59 || second > 59 {
            return Err(ProtocolError::invalid_data(
                "datetime",
                format!("invalid time: {hour:02}:{minute:02}:{second:02}"),
            ));
        }
        Ok(Self {
            date,
            hour,
            minute,
            second,
            utc_offset_seconds,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KlinePeriod {
    pub raw: u16,
    pub parameter: u16,
}

impl KlinePeriod {
    pub const fn new(raw: u16, parameter: u16) -> Self {
        Self { raw, parameter }
    }

    pub fn normalize(value: &str) -> Result<Self, ProtocolError> {
        let key = bounded_argument(value, "period")?.to_lowercase();
        let known = match key.as_str() {
            "5m" | "5min" => Some(Self::new(0, 1)),
            "15m" | "15min" => Some(Self::new(1, 1)),
            "30m" | "30min" => Some(Self::new(2, 1)),
            "60m" | "60min" | "1h" | "hour" => Some(Self::new(3, 1)),
            "day" | "1d" | "d" | "daily" => Some(Self::new(4, 1)),
            "week" | "1w" | "w" => Some(Self::new(5, 1)),
            "month" | "1mo" | "mo" => Some(Self::new(6, 1)),
            "1m" | "1min" | "minute" => Some(Self::new(7, 1)),
            "quarter" | "1q" | "q" => Some(Self::new(10, 1)),
            "year" | "1y" | "y" => Some(Self::new(11, 1)),
            _ => None,
        };
        if let Some(period) = known {
            return Ok(period);
        }

        let (number, unit) = split_period_suffix(&key).ok_or_else(|| invalid_period(value))?;
        let parameter = number.parse::<u16>().map_err(|_| invalid_period(value))?;
        match unit {
            "m" | "min" if parameter == 1 => Ok(Self::new(7, 1)),
            "m" | "min" if matches!(parameter, 5 | 15 | 30 | 60) => {
                Self::normalize(&format!("{parameter}m"))
            }
            "m" | "min" => Ok(Self::new(8, parameter)),
            "d" if parameter == 1 => Ok(Self::new(4, 1)),
            "d" => Ok(Self::new(9, parameter)),
            "s" => Ok(Self::new(13, parameter)),
            _ => Err(invalid_period(value)),
        }
    }

    pub fn name(self) -> String {
        match (self.raw, self.parameter) {
            (0, 1) => "5m".to_owned(),
            (1, 1) => "15m".to_owned(),
            (2, 1) => "30m".to_owned(),
            (3, 1) => "60m".to_owned(),
            (4, 1) => "day".to_owned(),
            (5, 1) => "week".to_owned(),
            (6, 1) => "month".to_owned(),
            (7, 1) => "1m".to_owned(),
            (10, 1) => "quarter".to_owned(),
            (11, 1) => "year".to_owned(),
            (8, parameter) => format!("{parameter}m"),
            (9, parameter) => format!("{parameter}d"),
            (13, parameter) => format!("{parameter}s"),
            (raw, parameter) => format!("{raw}/{parameter}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum AdjustMode {
    None = 0,
    Qfq = 1,
    Hfq = 2,
    FixedQfq = 3,
    FixedHfq = 4,
}

impl AdjustMode {
    pub fn from_raw(value: i64) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Qfq),
            2 => Ok(Self::Hfq),
            3 => Ok(Self::FixedQfq),
            4 => Ok(Self::FixedHfq),
            _ => Err(ProtocolError::invalid_argument(
                "adjust",
                format!("invalid adjust mode: {value}"),
            )),
        }
    }

    pub fn normalize(value: Option<&str>) -> Result<Self, ProtocolError> {
        let key = value
            .map(|item| bounded_argument(item, "adjust"))
            .transpose()?
            .map(str::to_lowercase);
        match key.as_deref() {
            None | Some("") | Some("none") => Ok(Self::None),
            Some("qfq" | "front") => Ok(Self::Qfq),
            Some("hfq" | "back") => Ok(Self::Hfq),
            Some("fixed_qfq" | "fixed_front") => Ok(Self::FixedQfq),
            Some("fixed_hfq" | "fixed_back") => Ok(Self::FixedHfq),
            Some(other) => Err(ProtocolError::invalid_argument(
                "adjust",
                format!("invalid adjust mode: {other:?}"),
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Qfq => "qfq",
            Self::Hfq => "hfq",
            Self::FixedQfq => "fixed_qfq",
            Self::FixedHfq => "fixed_hfq",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KPriceValues {
    pub current_milli: i64,
    pub last_close_milli: i64,
    pub open_milli: i64,
    pub high_milli: i64,
    pub low_milli: i64,
}

pub fn little_u16(data: &[u8]) -> Result<u16, ProtocolError> {
    Ok(u16::from_le_bytes(exact_array(data, "u16")?))
}

pub fn little_u32(data: &[u8]) -> Result<u32, ProtocolError> {
    Ok(u32::from_le_bytes(exact_array(data, "u32")?))
}

pub fn little_f32(data: &[u8]) -> Result<f32, ProtocolError> {
    Ok(f32::from_le_bytes(exact_array(data, "f32")?))
}

pub fn decode_gbk_text(data: &[u8]) -> String {
    let mut decoder = GBK.new_decoder_without_bom_handling();
    let mut output = String::with_capacity(data.len().saturating_mul(3));
    let mut offset = 0;

    while offset < data.len() {
        let (result, read) =
            decoder.decode_to_string_without_replacement(&data[offset..], &mut output, true);
        match result {
            DecoderResult::InputEmpty => {
                offset = data.len();
            }
            DecoderResult::OutputFull => {
                offset = offset.saturating_add(read);
                output.reserve(data.len().saturating_sub(offset).saturating_mul(3).max(4));
            }
            DecoderResult::Malformed(_, consumed_after) => {
                let rewind = usize::from(consumed_after).min(read);
                let advance = read.saturating_sub(rewind);
                if advance == 0 {
                    offset = offset.saturating_add(1);
                    decoder = GBK.new_decoder_without_bom_handling();
                } else {
                    offset = offset.saturating_add(advance);
                }
            }
        }
    }

    output.retain(|character| character != '\0');
    output.trim().to_owned()
}

pub fn normalize_yyyymmdd_text(value: &str) -> Result<u32, ProtocolError> {
    let compact = bounded_argument(value, "date")?.replace('-', "");
    if compact.len() != 8 || !compact.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProtocolError::invalid_argument(
            "date",
            format!("invalid date: {value:?}"),
        ));
    }
    compact
        .parse::<u32>()
        .map_err(|_| ProtocolError::invalid_argument("date", format!("invalid date: {value:?}")))
}

pub fn normalize_yyyymmdd_raw(value: i64) -> Result<u32, ProtocolError> {
    u32::try_from(value).map_err(|_| {
        ProtocolError::invalid_argument("date", format!("invalid date integer: {value}"))
    })
}

pub fn consume_varint(payload: &[u8], offset: usize) -> Result<(i64, usize), ProtocolError> {
    if offset >= payload.len() {
        return Err(ProtocolError::unexpected_eof(
            "varint",
            offset,
            1,
            payload.len(),
        ));
    }

    let mut magnitude = 0_u64;
    let mut position = offset;
    let mut shift = 0_u32;
    let mut byte_count = 0_usize;
    loop {
        if position >= payload.len() {
            return Err(ProtocolError::invalid_data("varint", "unterminated varint"));
        }
        if byte_count >= MAX_VARINT_BYTES {
            return Err(ProtocolError::invalid_data(
                "varint",
                format!("varint exceeds {MAX_VARINT_BYTES} bytes"),
            ));
        }

        let byte = payload[position];
        let part = if byte_count == 0 {
            u64::from(byte & 0x3f)
        } else {
            let unshifted = u64::from(byte & 0x7f);
            if shift >= u64::BITS || unshifted > (u64::MAX >> shift) {
                return Err(ProtocolError::invalid_data("varint", "varint overflow"));
            }
            unshifted << shift
        };
        magnitude = magnitude
            .checked_add(part)
            .ok_or_else(|| ProtocolError::invalid_data("varint", "varint overflow"))?;
        position += 1;
        byte_count += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift = if byte_count == 1 { 6 } else { shift + 7 };
    }

    let value = if payload[offset] & 0x40 == 0 {
        i64::try_from(magnitude)
            .map_err(|_| ProtocolError::invalid_data("varint", "varint overflow"))?
    } else if magnitude == (1_u64 << 63) {
        i64::MIN
    } else {
        let signed = i64::try_from(magnitude)
            .map_err(|_| ProtocolError::invalid_data("varint", "varint overflow"))?;
        signed
            .checked_neg()
            .ok_or_else(|| ProtocolError::invalid_data("varint", "varint overflow"))?
    };
    Ok((value, position))
}

pub fn consume_price(payload: &[u8], offset: usize) -> Result<(i64, usize), ProtocolError> {
    consume_varint(payload, offset)
}

pub fn decode_k(payload: &[u8], mut offset: usize) -> Result<(KPriceValues, usize), ProtocolError> {
    let (current_delta, next) = consume_price(payload, offset)?;
    offset = next;
    let (last_close_delta, next) = consume_price(payload, offset)?;
    offset = next;
    let (open_delta, next) = consume_price(payload, offset)?;
    offset = next;
    let (high_delta, next) = consume_price(payload, offset)?;
    offset = next;
    let (low_delta, next) = consume_price(payload, offset)?;
    offset = next;

    let current_milli = checked_price_scale(current_delta)?;
    Ok((
        KPriceValues {
            current_milli,
            last_close_milli: checked_price_scale(checked_add(current_delta, last_close_delta)?)?,
            open_milli: checked_price_scale(checked_add(current_delta, open_delta)?)?,
            high_milli: checked_price_scale(checked_add(current_delta, high_delta)?)?,
            low_milli: checked_price_scale(checked_add(current_delta, low_delta)?)?,
        },
        offset,
    ))
}

pub fn milli_to_float(value: i64) -> f64 {
    value as f64 / 1000.0
}

pub fn price_divisor(code: &NormalizedCode) -> u16 {
    if let Some(decimal) = security_decimal(code) {
        return match decimal {
            0..=2 => 1,
            3 => 10,
            4 => 100,
            _ => 10_u16.saturating_pow(u32::from(decimal - 2)),
        };
    }
    if PRICING_ETF_PREFIXES
        .iter()
        .any(|prefix| code.number.starts_with(prefix))
    {
        10
    } else {
        1
    }
}

pub fn get_volume(value: u32) -> f64 {
    if value == 0 {
        return 0.0;
    }

    let signed = value as i32;
    let logpoint = signed >> 24;
    let hleax = (signed >> 16) & 0xff;
    let lheax = (signed >> 8) & 0xff;
    let lleax = signed & 0xff;

    let base = 2.0_f64.powi(logpoint * 2 - 0x7f);
    let high = if hleax > 0x80 {
        base * f64::from(64 + (hleax & 0x7f)) / 64.0
    } else {
        base * f64::from(hleax) / 128.0
    };
    let scale = if hleax & 0x80 != 0 { 2.0 } else { 1.0 };
    let middle = base * f64::from(lheax) / 32_768.0 * scale;
    let low = base * f64::from(lleax) / 8_388_608.0 * scale;
    base + high + middle + low
}

pub fn decode_kline_datetime(
    raw_value: &[u8],
    period_raw: u16,
) -> Result<DateTimeParts, ProtocolError> {
    if raw_value.len() != 4 {
        return Err(ProtocolError::invalid_data(
            "kline time",
            "invalid kline time length",
        ));
    }

    if matches!(period_raw, 0 | 1 | 2 | 3 | 7 | 8) {
        let date_packed = little_u16(&raw_value[..2])?;
        let minute_of_day = little_u16(&raw_value[2..])?;
        let year = i32::from(date_packed >> 11) + 2004;
        let month = u8::try_from((date_packed % 2048) / 100)
            .map_err(|_| ProtocolError::invalid_data("kline time", "invalid month"))?;
        let day = u8::try_from((date_packed % 2048) % 100)
            .map_err(|_| ProtocolError::invalid_data("kline time", "invalid day"))?;
        let date = DateParts::new(year, month, day)?;
        let hour = u8::try_from(minute_of_day / 60)
            .map_err(|_| ProtocolError::invalid_data("kline time", "invalid hour"))?;
        let minute = u8::try_from(minute_of_day % 60)
            .map_err(|_| ProtocolError::invalid_data("kline time", "invalid minute"))?;
        return DateTimeParts::shanghai(date, hour, minute, 0);
    }

    if period_raw == 13 {
        let seconds = little_u32(raw_value)?;
        return shanghai_from_epoch_2003_12_31(seconds);
    }

    let raw_date = little_u32(raw_value)?;
    let date = DateParts::from_yyyymmdd(raw_date).ok_or_else(|| {
        ProtocolError::invalid_data("kline time", format!("invalid kline date: {raw_date}"))
    })?;
    DateTimeParts::shanghai(date, 15, 0, 0)
}

pub fn minute_index_label(index: i64) -> Result<String, ProtocolError> {
    let total_minutes = minute_index_total(index)?;
    Ok(format!(
        "{:02}:{:02}",
        total_minutes / 60,
        total_minutes % 60
    ))
}

pub fn minute_index_datetime(
    trading_date: DateParts,
    index: i64,
) -> Result<DateTimeParts, ProtocolError> {
    let total_minutes = minute_index_total(index)?;
    let hour = u8::try_from(total_minutes / 60)
        .map_err(|_| ProtocolError::invalid_data("minute index", "invalid hour"))?;
    let minute = u8::try_from(total_minutes % 60)
        .map_err(|_| ProtocolError::invalid_data("minute index", "invalid minute"))?;
    DateTimeParts::shanghai(trading_date, hour, minute, 0)
}

fn exact_array<const N: usize>(data: &[u8], field: &'static str) -> Result<[u8; N], ProtocolError> {
    data.try_into().map_err(|_| ProtocolError::LengthMismatch {
        field,
        expected: N,
        actual: data.len(),
    })
}

fn invalid_code(value: &str) -> ProtocolError {
    ProtocolError::invalid_argument("code", format!("invalid code: {value:?}"))
}

fn invalid_period(value: &str) -> ProtocolError {
    ProtocolError::invalid_argument("period", format!("invalid kline period: {value:?}"))
}

fn bounded_argument<'a>(value: &'a str, name: &'static str) -> Result<&'a str, ProtocolError> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_NORMALIZED_ARGUMENT_BYTES {
        return Err(ProtocolError::LimitExceeded {
            resource: name,
            actual: trimmed.len(),
            limit: MAX_NORMALIZED_ARGUMENT_BYTES,
        });
    }
    Ok(trimmed)
}

fn split_period_suffix(value: &str) -> Option<(&str, &str)> {
    let (number, unit) = if let Some(number) = value.strip_suffix("min") {
        (number, "min")
    } else if let Some(number) = value.strip_suffix('m') {
        (number, "m")
    } else if let Some(number) = value.strip_suffix('d') {
        (number, "d")
    } else if let Some(number) = value.strip_suffix('s') {
        (number, "s")
    } else {
        return None;
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        None
    } else {
        Some((number, unit))
    }
}

fn checked_add(left: i64, right: i64) -> Result<i64, ProtocolError> {
    left.checked_add(right)
        .ok_or_else(|| ProtocolError::invalid_data("price", "price overflow"))
}

fn checked_price_scale(value: i64) -> Result<i64, ProtocolError> {
    value
        .checked_mul(10)
        .ok_or_else(|| ProtocolError::invalid_data("price", "price overflow"))
}

fn minute_index_total(index: i64) -> Result<i64, ProtocolError> {
    if index < 0 {
        return Err(ProtocolError::invalid_argument(
            "minute index",
            format!("invalid minute index: {index}"),
        ));
    }
    if index < 120 {
        Ok(571 + index)
    } else {
        index.checked_add(661).ok_or_else(|| {
            ProtocolError::invalid_argument(
                "minute index",
                format!("invalid minute index: {index}"),
            )
        })
    }
}

fn is_valid_date(year: i32, month: u8, day: u8) -> bool {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    day <= maximum
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn shanghai_from_epoch_2003_12_31(seconds: u32) -> Result<DateTimeParts, ProtocolError> {
    let days = seconds / 86_400;
    let day_seconds = seconds % 86_400;
    let date = add_days(
        DateParts {
            year: 2003,
            month: 12,
            day: 31,
        },
        days,
    )?;
    let hour = u8::try_from(day_seconds / 3_600)
        .map_err(|_| ProtocolError::invalid_data("kline time", "invalid hour"))?;
    let minute = u8::try_from((day_seconds % 3_600) / 60)
        .map_err(|_| ProtocolError::invalid_data("kline time", "invalid minute"))?;
    let second = u8::try_from(day_seconds % 60)
        .map_err(|_| ProtocolError::invalid_data("kline time", "invalid second"))?;
    DateTimeParts::shanghai(date, hour, minute, second)
}

fn add_days(date: DateParts, days: u32) -> Result<DateParts, ProtocolError> {
    let mut year = date.year;
    let mut month = date.month;
    let mut day = date.day;
    let mut remaining = days;
    while remaining > 0 {
        let days_in_month = u32::from(month_length(year, month)?);
        let after_current = days_in_month - u32::from(day);
        if remaining <= after_current {
            day = u8::try_from(u32::from(day) + remaining)
                .map_err(|_| ProtocolError::invalid_data("date", "date overflow"))?;
            remaining = 0;
        } else {
            remaining -= after_current + 1;
            day = 1;
            if month == 12 {
                month = 1;
                year = year
                    .checked_add(1)
                    .ok_or_else(|| ProtocolError::invalid_data("date", "date overflow"))?;
            } else {
                month += 1;
            }
        }
    }
    DateParts::new(year, month, day)
}

fn month_length(year: i32, month: u8) -> Result<u8, ProtocolError> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Ok(31),
        4 | 6 | 9 | 11 => Ok(30),
        2 if is_leap_year(year) => Ok(29),
        2 => Ok(28),
        _ => Err(ProtocolError::invalid_data("date", "invalid month")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        consume_varint, decode_gbk_text, decode_kline_datetime, get_volume, minute_index_label,
        normalize_yyyymmdd_raw, normalize_yyyymmdd_text, price_divisor, AdjustMode, DateParts,
        KlinePeriod, Market, NormalizedCode, SHANGHAI_UTC_OFFSET_SECONDS,
    };

    #[test]
    fn normalizes_markets_and_codes_with_bse_precedence() {
        assert_eq!(Market::normalize(" 深市 "), Ok(Market::Shenzhen));
        assert_eq!(Market::normalize("SHA"), Ok(Market::Shanghai));
        assert_eq!(Market::from_id(2), Ok(Market::Beijing));

        let bse = NormalizedCode::parse("920001");
        assert!(matches!(
            bse,
            Ok(NormalizedCode {
                market: Market::Beijing,
                ref number,
            }) if number == "920001"
        ));
        assert_eq!(
            NormalizedCode::parse("900901").map(|code| code.full_code()),
            Ok("sh900901".to_owned())
        );
        assert_eq!(
            NormalizedCode::parse("bj920001").map(|code| code.full_code()),
            Ok("bj920001".to_owned())
        );
        assert_eq!(
            NormalizedCode::parse("sz400001").map(|code| code.full_code()),
            Ok("sz400001".to_owned())
        );
    }

    #[test]
    fn gbk_decode_ignores_malformed_and_removes_nuls() {
        let decoded = decode_gbk_text(&[b' ', 0xc9, 0xee, 0xff, 0xca, 0xd0, 0, b' ']);

        assert_eq!(decoded, "深市");
    }

    #[test]
    fn parses_dates_without_validating_until_conversion() {
        assert_eq!(normalize_yyyymmdd_text("2026-08-14"), Ok(20_260_814));
        assert_eq!(normalize_yyyymmdd_text("2026--08-14"), Ok(20_260_814));
        assert_eq!(normalize_yyyymmdd_raw(20_260_814), Ok(20_260_814));
        assert!(normalize_yyyymmdd_raw(-1).is_err());
        assert_eq!(
            DateParts::from_yyyymmdd(20_260_814),
            DateParts::new(2026, 8, 14).ok()
        );
        assert_eq!(DateParts::from_yyyymmdd(20_261_399), None);
    }

    #[test]
    fn decodes_signed_tdx_varints() {
        assert_eq!(consume_varint(&[0x01], 0), Ok((1, 1)));
        assert_eq!(consume_varint(&[0x41], 0), Ok((-1, 1)));
        assert_eq!(consume_varint(&[0x81, 0x01], 0), Ok((65, 2)));
        assert!(consume_varint(&[0x80], 0).is_err());
    }

    #[test]
    fn normalizes_periods_and_adjustments() {
        assert_eq!(KlinePeriod::normalize("daily"), Ok(KlinePeriod::new(4, 1)));
        assert_eq!(KlinePeriod::normalize("10m"), Ok(KlinePeriod::new(8, 10)));
        assert_eq!(KlinePeriod::normalize("2d"), Ok(KlinePeriod::new(9, 2)));
        assert_eq!(KlinePeriod::normalize("5s"), Ok(KlinePeriod::new(13, 5)));
        assert_eq!(KlinePeriod::new(13, 5).name(), "5s");
        assert_eq!(
            AdjustMode::normalize(Some("fixed_front")),
            Ok(AdjustMode::FixedQfq)
        );
        assert_eq!(AdjustMode::from_raw(4), Ok(AdjustMode::FixedHfq));
        assert!(AdjustMode::from_raw(5).is_err());
    }

    #[test]
    fn decodes_all_kline_time_forms_in_shanghai_timezone() {
        let packed = ((2026_u16 - 2004) << 11) + 8 * 100 + 14;
        let minute_of_day = 9 * 60 + 31;
        let mut minute_raw = Vec::new();
        minute_raw.extend_from_slice(&packed.to_le_bytes());
        minute_raw.extend_from_slice(&(minute_of_day as u16).to_le_bytes());
        let minute = decode_kline_datetime(&minute_raw, 0);
        assert!(matches!(
            minute,
            Ok(value)
                if value.date == (DateParts { year: 2026, month: 8, day: 14 })
                    && value.hour == 9
                    && value.minute == 31
                    && value.utc_offset_seconds == Some(SHANGHAI_UTC_OFFSET_SECONDS)
        ));

        let second = decode_kline_datetime(&86_405_u32.to_le_bytes(), 13);
        assert!(matches!(
            second,
            Ok(value)
                if value.date == (DateParts { year: 2004, month: 1, day: 1 })
                    && value.second == 5
        ));

        let day = decode_kline_datetime(&20_260_814_u32.to_le_bytes(), 4);
        assert!(matches!(day, Ok(value) if value.hour == 15 && value.minute == 0));
    }

    #[test]
    fn minute_labels_skip_the_midday_break() {
        assert_eq!(minute_index_label(0), Ok("09:31".to_owned()));
        assert_eq!(minute_index_label(119), Ok("11:30".to_owned()));
        assert_eq!(minute_index_label(120), Ok("13:01".to_owned()));
        assert!(minute_index_label(-1).is_err());
    }

    #[test]
    fn price_divisor_and_volume_keep_wire_semantics() {
        let etf = NormalizedCode::parse("sh510300");
        let stock = NormalizedCode::parse("sz000001");
        assert!(matches!(etf, Ok(ref code) if price_divisor(code) == 10));
        assert!(matches!(stock, Ok(ref code) if price_divisor(code) == 1));
        assert_eq!(get_volume(0), 0.0);
    }

    #[test]
    fn registered_decimal_overrides_prefix_fallback() {
        let bond = NormalizedCode::parse("sh118076").expect("code");
        assert_eq!(price_divisor(&bond), 1);
        super::register_security_decimal("sh118076", 4);
        assert_eq!(price_divisor(&bond), 100);
    }
}
