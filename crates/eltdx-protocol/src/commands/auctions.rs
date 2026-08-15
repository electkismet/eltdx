use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{little_f32, little_u16, little_u32, NormalizedCode};

use super::trades::minute_of_day_label;

pub const TYPE_AUCTION_SERIES: u16 = 0x056a;
pub const AUCTION_RECORD_SIZE: usize = 16;
pub const DEFAULT_AUCTION_SELECTOR: u32 = 3;
pub const DEFAULT_AUCTION_START: u32 = 0;
pub const DEFAULT_AUCTION_LIMIT: u32 = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionSeriesRequest {
    pub code: NormalizedCode,
    pub mode_or_selector_raw: u32,
    pub start_raw: u32,
    pub limit_or_count_raw: u32,
    pub include_raw: bool,
}

impl AuctionSeriesRequest {
    pub const fn new(
        code: NormalizedCode,
        mode_or_selector_raw: u32,
        start_raw: u32,
        limit_or_count_raw: u32,
    ) -> Self {
        Self::with_include_raw(
            code,
            mode_or_selector_raw,
            start_raw,
            limit_or_count_raw,
            false,
        )
    }

    pub const fn with_include_raw(
        code: NormalizedCode,
        mode_or_selector_raw: u32,
        start_raw: u32,
        limit_or_count_raw: u32,
        include_raw: bool,
    ) -> Self {
        Self {
            code,
            mode_or_selector_raw,
            start_raw,
            limit_or_count_raw,
            include_raw,
        }
    }

    pub fn with_defaults(code: NormalizedCode) -> Self {
        Self::new(
            code,
            DEFAULT_AUCTION_SELECTOR,
            DEFAULT_AUCTION_START,
            DEFAULT_AUCTION_LIMIT,
        )
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(28);
        data.extend_from_slice(&[self.code.market().id(), 0]);
        data.extend_from_slice(self.code.number().as_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&self.mode_or_selector_raw.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&self.start_raw.to_le_bytes());
        data.extend_from_slice(&self.limit_or_count_raw.to_le_bytes());
        RequestFrame::new(msg_id, TYPE_AUCTION_SERIES, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuctionPoint {
    pub index: u16,
    pub minute_of_day_raw: u16,
    pub second_raw: u8,
    pub time_label: String,
    pub time_seconds: u32,
    pub price: f32,
    pub price_raw: [u8; 4],
    pub price_milli: i64,
    pub matched_volume: u32,
    pub unmatched_signed_raw: i32,
    pub unmatched_volume: u32,
    pub unmatched_direction_raw: i8,
    pub reserved_zero_0e: u8,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuctionSeries {
    pub request: AuctionSeriesRequest,
    pub points: Vec<AuctionPoint>,
    pub raw_payload: Bytes,
}

pub fn parse_auction_series_payload(
    payload: &[u8],
    request: AuctionSeriesRequest,
) -> Result<AuctionSeries, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "auction series",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "auction series",
            "invalid auction series payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let records_length = count.checked_mul(AUCTION_RECORD_SIZE).ok_or_else(|| {
        ProtocolError::invalid_data("auction series", "auction series length overflow")
    })?;
    let expected_length = 2_usize.checked_add(records_length).ok_or_else(|| {
        ProtocolError::invalid_data("auction series", "auction series length overflow")
    })?;
    if payload.len() != expected_length {
        return Err(ProtocolError::invalid_data(
            "auction series",
            format!(
                "invalid auction series length: expected {expected_length}, got {}",
                payload.len()
            ),
        ));
    }

    let mut points = Vec::with_capacity(count);
    let mut offset = 2;
    for index in 0..count {
        let end = offset.saturating_add(AUCTION_RECORD_SIZE);
        let record = payload.get(offset..end).ok_or_else(|| {
            ProtocolError::invalid_data("auction series", "truncated auction record")
        })?;
        offset = end;
        let minute_of_day_raw = little_u16(&record[..2])?;
        let price_raw: [u8; 4] = record[2..6]
            .try_into()
            .map_err(|_| ProtocolError::invalid_data("auction series", "invalid price field"))?;
        let price = little_f32(&price_raw)?;
        let matched_volume = little_u32(&record[6..10])?;
        let unmatched_signed_raw = i32::from_le_bytes(record[10..14].try_into().map_err(|_| {
            ProtocolError::invalid_data("auction series", "invalid unmatched volume field")
        })?);
        let reserved_zero_0e = record[14];
        let second_raw = record[15];
        let point_index = u16::try_from(index)
            .map_err(|_| ProtocolError::invalid_data("auction series", "auction index overflow"))?;
        let time_seconds = u32::from(minute_of_day_raw)
            .checked_mul(60)
            .and_then(|value| value.checked_add(u32::from(second_raw)))
            .ok_or_else(|| {
                ProtocolError::invalid_data("auction series", "auction time overflow")
            })?;
        points.push(AuctionPoint {
            index: point_index,
            minute_of_day_raw,
            second_raw,
            time_label: minute_of_day_label(minute_of_day_raw, Some(second_raw)),
            time_seconds,
            price,
            price_raw,
            price_milli: python_round_to_i64(f64::from(price) * 1_000.0)?,
            matched_volume,
            unmatched_signed_raw,
            unmatched_volume: unmatched_signed_raw.unsigned_abs(),
            unmatched_direction_raw: if unmatched_signed_raw >= 0 { 1 } else { -1 },
            reserved_zero_0e,
            record_hex: encode_hex(record),
        });
    }
    Ok(AuctionSeries {
        request,
        points,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn python_round_to_i64(value: f64) -> Result<i64, ProtocolError> {
    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_MAX_PLUS_ONE_F64: f64 = 9_223_372_036_854_775_808.0;
    if !value.is_finite() || !(I64_MIN_F64..I64_MAX_PLUS_ONE_F64).contains(&value) {
        return Err(ProtocolError::invalid_data(
            "auction series",
            "auction price cannot be represented as milli integer",
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
    .ok_or_else(|| ProtocolError::invalid_data("auction series", "auction price overflow"))
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
    use super::{parse_auction_series_payload, python_round_to_i64, AuctionSeriesRequest};
    use crate::unit::NormalizedCode;
    use crate::ProtocolError;

    #[test]
    fn request_matches_frozen_default_wire_layout() -> Result<(), ProtocolError> {
        let frame =
            AuctionSeriesRequest::with_defaults(NormalizedCode::parse("sz000988")?).frame(1);
        assert_eq!(frame.data.len(), 28);
        assert_eq!(
            &frame.data[..],
            &[
                0, 0, b'0', b'0', b'0', b'9', b'8', b'8', 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0xf4, 0x01, 0, 0,
            ]
        );
        let raw = AuctionSeriesRequest::with_include_raw(
            NormalizedCode::parse("sz000988")?,
            3,
            0,
            500,
            true,
        );
        assert!(raw.include_raw);
        Ok(())
    }

    #[test]
    fn parses_fixed_record_and_preserves_raw_fields() -> Result<(), ProtocolError> {
        let payload = [
            1, 0, 0x2b, 0x02, 0xb8, 0x1e, 0x22, 0x43, 0x08, 0x0a, 0x00, 0x00, 0x81, 0x09, 0x00,
            0x00, 0x00, 0x00,
        ];
        let parsed = parse_auction_series_payload(
            &payload,
            AuctionSeriesRequest::with_defaults(NormalizedCode::parse("sz000988")?),
        )?;
        assert_eq!(parsed.points[0].time_label, "09:15:00");
        assert_eq!(parsed.points[0].matched_volume, 2_568);
        assert_eq!(parsed.points[0].unmatched_signed_raw, 2_433);
        assert_eq!(parsed.points[0].price_raw, [0xb8, 0x1e, 0x22, 0x43]);
        Ok(())
    }

    #[test]
    fn python_rounding_is_half_even_without_rust_round() -> Result<(), ProtocolError> {
        assert_eq!(python_round_to_i64(2.5)?, 2);
        assert_eq!(python_round_to_i64(3.5)?, 4);
        assert_eq!(python_round_to_i64(-2.5)?, -2);
        assert_eq!(python_round_to_i64(-3.5)?, -4);
        Ok(())
    }
}
