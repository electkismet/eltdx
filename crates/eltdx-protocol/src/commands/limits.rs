use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{little_f32, little_u16, little_u32, Market};

pub const TYPE_SPECIAL_LIMITS: u16 = 0x0452;
pub const LIMIT_RECORD_SIZE: usize = 13;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpecialLimitsRequest {
    pub start_index: u16,
}

impl SpecialLimitsRequest {
    pub const fn new(start_index: u16) -> Self {
        Self { start_index }
    }

    pub fn frame(self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(14);
        data.extend_from_slice(&self.start_index.to_le_bytes());
        data.extend_from_slice(&[0_u8; 12]);
        RequestFrame::new(msg_id, TYPE_SPECIAL_LIMITS, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecialLimitRecord {
    pub market_id: u8,
    pub market: Option<Market>,
    pub code_num: u32,
    pub code: String,
    pub upper_price_raw_f32: f32,
    pub upper_price_raw: [u8; 4],
    pub lower_price_raw_f32: f32,
    pub lower_price_raw: [u8; 4],
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecialLimitPage {
    pub request: SpecialLimitsRequest,
    pub records: Vec<SpecialLimitRecord>,
    pub raw_payload: Bytes,
}

pub fn parse_special_limits_payload(
    payload: &[u8],
    request: SpecialLimitsRequest,
) -> Result<SpecialLimitPage, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "special limits",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "special limits",
            "invalid special limits payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let records_length = count.checked_mul(LIMIT_RECORD_SIZE).ok_or_else(|| {
        ProtocolError::invalid_data("special limits", "special limits length overflow")
    })?;
    let expected_length = 2_usize.checked_add(records_length).ok_or_else(|| {
        ProtocolError::invalid_data("special limits", "special limits length overflow")
    })?;
    if payload.len() != expected_length {
        return Err(ProtocolError::invalid_data(
            "special limits",
            format!(
                "invalid special limits length: expected {expected_length}, got {}",
                payload.len()
            ),
        ));
    }
    let mut records = Vec::with_capacity(count);
    let mut offset = 2;
    for _ in 0..count {
        let end = offset.saturating_add(LIMIT_RECORD_SIZE);
        let record = payload.get(offset..end).ok_or_else(|| {
            ProtocolError::invalid_data("special limits", "truncated special limit record")
        })?;
        offset = end;
        let market_id = record[0];
        let market = Market::from_id(i64::from(market_id)).ok();
        let code_num = little_u32(&record[1..5])?;
        let upper_price_raw: [u8; 4] = record[5..9].try_into().map_err(|_| {
            ProtocolError::invalid_data("special limits", "invalid upper price field")
        })?;
        let lower_price_raw: [u8; 4] = record[9..13].try_into().map_err(|_| {
            ProtocolError::invalid_data("special limits", "invalid lower price field")
        })?;
        records.push(SpecialLimitRecord {
            market_id,
            market,
            code_num,
            code: format!("{code_num:06}"),
            upper_price_raw_f32: little_f32(&upper_price_raw)?,
            upper_price_raw,
            lower_price_raw_f32: little_f32(&lower_price_raw)?,
            lower_price_raw,
            record_hex: encode_hex(record),
        });
    }
    Ok(SpecialLimitPage {
        request,
        records,
        raw_payload: Bytes::copy_from_slice(payload),
    })
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
    use super::{parse_special_limits_payload, SpecialLimitsRequest};
    use crate::ProtocolError;

    #[test]
    fn request_has_fixed_fourteen_byte_layout() {
        let frame = SpecialLimitsRequest::new(2).frame(1);
        assert_eq!(frame.data.len(), 14);
        assert_eq!(&frame.data[..2], &[2, 0]);
        assert!(frame.data[2..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn response_preserves_code_and_float_bits() -> Result<(), ProtocolError> {
        let mut payload = vec![1, 0, 0];
        payload.extend_from_slice(&123_054_u32.to_le_bytes());
        payload.extend_from_slice(&212.531_f32.to_le_bytes());
        payload.extend_from_slice(&141.687_f32.to_le_bytes());
        let parsed = parse_special_limits_payload(&payload, SpecialLimitsRequest::new(2))?;
        assert_eq!(parsed.records[0].code, "123054");
        assert_eq!(parsed.records[0].upper_price_raw, 212.531_f32.to_le_bytes());
        assert_eq!(parsed.request.start_index, 2);
        Ok(())
    }
}
