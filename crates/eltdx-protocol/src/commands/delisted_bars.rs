use bytes::Bytes;

use crate::commands::klines::KlineBar;
use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{MAX_KLINE_PAGE_SIZE, MAX_RESPONSE_PAYLOAD_SIZE};
use crate::unit::{
    consume_price, get_volume, little_u16, little_u32, milli_to_float, DateTimeParts,
    NormalizedCode,
};

pub const TYPE_DELISTED_KLINES: u16 = 0x052b;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelistedKlinesRequest {
    pub code: NormalizedCode,
    pub start: u16,
    pub count: u16,
    pub include_raw: bool,
}

impl DelistedKlinesRequest {
    pub fn new(
        code: NormalizedCode,
        start: u16,
        count: u16,
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
            start,
            count,
            include_raw,
        })
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&u16::from(self.code.market().id()).to_le_bytes());
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&self.start.to_le_bytes());
        data.extend_from_slice(&self.count.to_le_bytes());
        RequestFrame::new(msg_id, TYPE_DELISTED_KLINES, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DelistedKlineSeries {
    pub request: DelistedKlinesRequest,
    pub bars: Vec<KlineBar>,
    pub raw_payload: Bytes,
}

pub fn parse_delisted_klines_payload(
    payload: &[u8],
    request: DelistedKlinesRequest,
) -> Result<DelistedKlineSeries, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "delisted klines",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "delisted klines",
            "invalid payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    if count > request.count.into() || count > payload.len().saturating_sub(2) / 17 {
        return Err(ProtocolError::invalid_data(
            "delisted klines",
            "truncated kline record",
        ));
    }

    let mut offset = 2_usize;
    let mut last_close_milli = 0_i64;
    let mut bars = Vec::with_capacity(count);
    for _ in 0..count {
        let record_start = offset;
        let date_raw = read_u32(payload, offset, "truncated date")?;
        offset += 4;
        let date = crate::unit::DateParts::from_yyyymmdd(date_raw)
            .ok_or_else(|| ProtocolError::invalid_data("delisted klines", "invalid date"))?;
        let time = DateTimeParts::shanghai(date, 0, 0, 0)?;
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
        if low_price_milli > open_price_milli.min(close_price_milli)
            || high_price_milli < open_price_milli.max(close_price_milli)
            || low_price_milli > high_price_milli
        {
            return Err(ProtocolError::invalid_data(
                "delisted klines",
                "invalid OHLC",
            ));
        }
        let (volume_shares, next) = consume_price(payload, offset)?;
        offset = next;
        if volume_shares < 0 || volume_shares > i64::from(u32::MAX) {
            return Err(ProtocolError::invalid_data(
                "delisted klines",
                "invalid volume",
            ));
        }
        let volume_raw = volume_shares as u32;
        let amount_raw = read_u32(payload, offset, "truncated amount")?;
        offset += 4;
        let _unknown_suffix = payload
            .get(offset..offset.saturating_add(4))
            .ok_or_else(|| ProtocolError::invalid_data("delisted klines", "truncated suffix"))?;
        offset += 4;
        last_close_milli = close_price_milli;
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
            volume_wire_value: f64::from(volume_raw),
            volume_lots: f64::from(volume_raw) / 100.0,
            amount: get_volume(amount_raw),
            open_delta_raw,
            close_delta_raw,
            high_delta_raw,
            low_delta_raw,
            up_count: None,
            down_count: None,
            record_hex: encode_hex(&payload[record_start..offset]),
        });
    }
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            "delisted klines",
            format!(
                "unexpected trailing payload bytes: {}",
                payload.len() - offset
            ),
        ));
    }
    Ok(DelistedKlineSeries {
        request,
        bars,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn read_u32(data: &[u8], offset: usize, message: &'static str) -> Result<u32, ProtocolError> {
    little_u32(
        data.get(offset..offset.saturating_add(4))
            .ok_or_else(|| ProtocolError::invalid_data("delisted klines", message))?,
    )
}

fn checked_add(left: i64, right: i64) -> Result<i64, ProtocolError> {
    left.checked_add(right)
        .ok_or_else(|| ProtocolError::invalid_data("delisted klines", "price overflow"))
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
    use super::{parse_delisted_klines_payload, DelistedKlinesRequest};
    use crate::unit::NormalizedCode;

    #[test]
    fn request_matches_verified_052b_body() -> Result<(), crate::ProtocolError> {
        let request =
            DelistedKlinesRequest::new(NormalizedCode::parse("sz000038")?, 800, 20, false)?;
        let frame = request.frame(7);
        assert_eq!(
            frame.data.as_ref(),
            &[0, 0, b'0', b'0', b'0', b'0', b'3', b'8', 0x20, 0x03, 0x14, 0]
        );
        Ok(())
    }

    #[test]
    fn parses_one_verified_daily_record() -> Result<(), crate::ProtocolError> {
        let request = DelistedKlinesRequest::new(NormalizedCode::parse("sz000038")?, 0, 1, false)?;
        let payload = bytes::Bytes::from_static(&[
            1, 0, 0x18, 0xb1, 0x34, 0x01, 0xb8, 0x33, 0xb4, 0x02, 0x88, 0x03, 0x00, 0x84, 0xe4,
            0x9a, 0x04, 0xdb, 0xb8, 0x67, 0x4b, 0, 0, 1, 0,
        ]);
        let parsed = parse_delisted_klines_payload(&payload, request)?;
        assert_eq!(parsed.bars.len(), 1);
        assert_eq!(parsed.bars[0].close_price_milli, 3500);
        assert_eq!(parsed.bars[0].volume_raw, 4_413_700);
        Ok(())
    }
}
