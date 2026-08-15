use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{MAX_KLINE_PAGE_SIZE, MAX_RESPONSE_PAYLOAD_SIZE};
use crate::unit::{
    consume_price, decode_kline_datetime, get_volume, little_u16, little_u32, milli_to_float,
    AdjustMode, DateParts, DateTimeParts, KlinePeriod, NormalizedCode,
};

pub const TYPE_KLINES: u16 = 0x052d;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KlineKind {
    Stock,
    Index,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KlinesRequest {
    pub code: NormalizedCode,
    pub period: KlinePeriod,
    pub start: u16,
    pub count: u16,
    pub adjust: AdjustMode,
    pub anchor_date_raw: u32,
    pub kind: KlineKind,
    pub include_raw: bool,
}

impl KlinesRequest {
    pub fn new(
        code: NormalizedCode,
        period: KlinePeriod,
        start: u16,
        count: u16,
        adjust: AdjustMode,
        anchor_date_raw: u32,
        kind: KlineKind,
    ) -> Result<Self, ProtocolError> {
        Self::with_include_raw(
            code,
            period,
            start,
            count,
            adjust,
            anchor_date_raw,
            kind,
            false,
        )
    }

    pub fn with_include_raw(
        code: NormalizedCode,
        period: KlinePeriod,
        start: u16,
        count: u16,
        adjust: AdjustMode,
        anchor_date_raw: u32,
        kind: KlineKind,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        if count == 0 || count > MAX_KLINE_PAGE_SIZE {
            return Err(ProtocolError::invalid_argument(
                "count",
                format!("count must be between 1 and {MAX_KLINE_PAGE_SIZE}"),
            ));
        }
        Ok(Self {
            code,
            period,
            start,
            count,
            adjust,
            anchor_date_raw,
            kind,
            include_raw,
        })
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(42);
        data.extend_from_slice(&u16::from(self.code.market().id()).to_le_bytes());
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&self.period.raw.to_le_bytes());
        data.extend_from_slice(&self.period.parameter.to_le_bytes());
        data.extend_from_slice(&self.start.to_le_bytes());
        data.extend_from_slice(&self.count.to_le_bytes());
        data.extend_from_slice(&(self.adjust as u16).to_le_bytes());
        data.extend_from_slice(&self.anchor_date_raw.to_le_bytes());
        data.extend_from_slice(&[0_u8; 20]);
        RequestFrame::new(msg_id, TYPE_KLINES, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KlineBar {
    pub time: DateTimeParts,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub open_price_milli: i64,
    pub close_price_milli: i64,
    pub high_price_milli: i64,
    pub low_price_milli: i64,
    pub last_close_price_milli: Option<i64>,
    pub volume_raw: u32,
    pub amount_raw: u32,
    pub volume_wire_value: f64,
    pub volume_lots: f64,
    pub amount: f64,
    pub open_delta_raw: i64,
    pub close_delta_raw: i64,
    pub high_delta_raw: i64,
    pub low_delta_raw: i64,
    pub up_count: Option<u16>,
    pub down_count: Option<u16>,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KlineSeries {
    pub request: KlinesRequest,
    pub anchor_date: Option<DateParts>,
    pub bars: Vec<KlineBar>,
    pub raw_payload: Bytes,
}

pub fn parse_klines_payload(
    payload: &[u8],
    request: KlinesRequest,
) -> Result<KlineSeries, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "klines",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "klines",
            "invalid klines payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let minimum_record = if request.kind == KlineKind::Index {
        20
    } else {
        16
    };
    if count > payload.len().saturating_sub(2) / minimum_record {
        return Err(ProtocolError::invalid_data(
            "klines",
            "truncated kline time field",
        ));
    }

    let mut offset = 2;
    let mut last_close_milli = 0_i64;
    let mut bars = Vec::with_capacity(count);
    for _ in 0..count {
        let record_start = offset;
        let time_end = offset.saturating_add(4);
        let time_raw = payload
            .get(offset..time_end)
            .ok_or_else(|| ProtocolError::invalid_data("klines", "truncated kline time field"))?;
        let time = decode_kline_datetime(time_raw, request.period.raw)?;
        offset = time_end;
        let (open_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (close_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (high_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;
        let (low_delta_raw, next) = consume_price(payload, offset)?;
        offset = next;

        let previous_close = if bars.is_empty() {
            None
        } else {
            Some(last_close_milli)
        };
        let open_price_milli = checked_add(last_close_milli, open_delta_raw)?;
        let close_price_milli = checked_add(open_price_milli, close_delta_raw)?;
        let high_price_milli = checked_add(open_price_milli, high_delta_raw)?;
        let low_price_milli = checked_add(open_price_milli, low_delta_raw)?;
        last_close_milli = close_price_milli;

        let volume_raw = read_u32(payload, offset, "truncated kline volume or amount field")?;
        offset += 4;
        let amount_raw = read_u32(payload, offset, "truncated kline volume or amount field")?;
        offset += 4;
        let (up_count, down_count) = if request.kind == KlineKind::Index {
            let up = read_u16(payload, offset, "truncated kline index breadth field")?;
            let down = read_u16(
                payload,
                offset.saturating_add(2),
                "truncated kline index breadth field",
            )?;
            offset += 4;
            (Some(up), Some(down))
        } else {
            (None, None)
        };
        let volume_wire_value = get_volume(volume_raw);
        bars.push(KlineBar {
            time,
            open: milli_to_float(open_price_milli),
            close: milli_to_float(close_price_milli),
            high: milli_to_float(high_price_milli),
            low: milli_to_float(low_price_milli),
            open_price_milli,
            close_price_milli,
            high_price_milli,
            low_price_milli,
            last_close_price_milli: previous_close,
            volume_raw,
            amount_raw,
            volume_wire_value,
            volume_lots: volume_wire_value / 100.0,
            amount: get_volume(amount_raw),
            open_delta_raw,
            close_delta_raw,
            high_delta_raw,
            low_delta_raw,
            up_count,
            down_count,
            record_hex: encode_hex(&payload[record_start..offset]),
        });
    }
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            "klines",
            format!(
                "unexpected trailing kline payload bytes: {}",
                payload.len() - offset
            ),
        ));
    }
    let anchor_date = if request.anchor_date_raw == 0 {
        None
    } else {
        DateParts::from_yyyymmdd(request.anchor_date_raw)
    };
    Ok(KlineSeries {
        request,
        anchor_date,
        bars,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn read_u16(data: &[u8], offset: usize, message: &'static str) -> Result<u16, ProtocolError> {
    let end = offset.saturating_add(2);
    little_u16(
        data.get(offset..end)
            .ok_or_else(|| ProtocolError::invalid_data("klines", message))?,
    )
}

fn read_u32(data: &[u8], offset: usize, message: &'static str) -> Result<u32, ProtocolError> {
    let end = offset.saturating_add(4);
    little_u32(
        data.get(offset..end)
            .ok_or_else(|| ProtocolError::invalid_data("klines", message))?,
    )
}

fn checked_add(left: i64, right: i64) -> Result<i64, ProtocolError> {
    left.checked_add(right)
        .ok_or_else(|| ProtocolError::invalid_data("klines", "kline price overflow"))
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
    use super::{parse_klines_payload, KlineKind, KlinesRequest};
    use crate::{
        unit::{AdjustMode, KlinePeriod, NormalizedCode},
        ProtocolError,
    };

    #[test]
    fn kline_request_matches_frozen_day_wire_data() -> Result<(), ProtocolError> {
        let request = KlinesRequest::new(
            NormalizedCode::parse("sz300308")?,
            KlinePeriod::normalize("day")?,
            0,
            420,
            AdjustMode::None,
            0,
            KlineKind::Stock,
        )?;
        let frame = request.frame(10);
        assert_eq!(frame.data.len(), 42);
        assert_eq!(&frame.data[8..12], &[4, 0, 1, 0]);
        assert_eq!(&frame.data[12..16], &[0, 0, 0xa4, 0x01]);
        assert!(!request.include_raw);
        let raw = KlinesRequest::with_include_raw(
            request.code.clone(),
            request.period,
            request.start,
            request.count,
            request.adjust,
            request.anchor_date_raw,
            request.kind,
            true,
        )?;
        assert!(raw.include_raw);
        Ok(())
    }

    #[test]
    fn parses_minimal_day_bar_without_running_native_code() -> Result<(), ProtocolError> {
        let request = KlinesRequest::new(
            NormalizedCode::parse("sz000001")?,
            KlinePeriod::normalize("day")?,
            0,
            1,
            AdjustMode::None,
            0,
            KlineKind::Stock,
        )?;
        let mut payload = vec![1, 0];
        payload.extend_from_slice(&20_260_814_u32.to_le_bytes());
        payload.extend_from_slice(&[0, 0, 0, 0]);
        payload.extend_from_slice(&[0_u8; 8]);
        let parsed = parse_klines_payload(&payload, request);
        assert!(matches!(parsed, Ok(series) if series.bars.len() == 1));
        Ok(())
    }
}
