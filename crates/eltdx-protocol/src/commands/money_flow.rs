use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{little_f32, little_u16, little_u32, DateParts, Market, NormalizedCode};

/// Daily money-flow records used by the `ASK_OneZJLX` service.
pub const TYPE_MONEY_FLOW: u16 = 0x0ffc;
pub const MONEY_FLOW_HEADER_SIZE: usize = 0x28;
pub const MONEY_FLOW_RECORD_SIZE: usize = 0x58;
pub const MONEY_FLOW_REQUEST_DATA_SIZE: usize = 38;
pub const MONEY_FLOW_ROUTE: u8 = 0x7e;
pub const MONEY_FLOW_CHANNEL: u8 = 0x2d;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneyFlowRequest {
    pub code: NormalizedCode,
    pub include_raw: bool,
}

impl MoneyFlowRequest {
    pub fn new(code: NormalizedCode) -> Self {
        Self {
            code,
            include_raw: false,
        }
    }

    pub fn with_include_raw(code: NormalizedCode, include_raw: bool) -> Self {
        Self { code, include_raw }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(MONEY_FLOW_REQUEST_DATA_SIZE);
        data.push(self.code.market().id());
        data.push(0);
        data.extend_from_slice(self.code.number().as_bytes());
        data.resize(MONEY_FLOW_REQUEST_DATA_SIZE, 0);
        // `msg_id` carries the wire sequence, channel, and service route in
        // this legacy service. Keep the route internal to the command frame.
        let wire_msg_id = u32::from(msg_id as u8)
            | (u32::from(MONEY_FLOW_CHANNEL) << 8)
            | (u32::from(MONEY_FLOW_ROUTE) << 16);
        RequestFrame::new(wire_msg_id, TYPE_MONEY_FLOW, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoneyFlowRecord {
    pub date_raw: u32,
    pub date: Option<DateParts>,
    pub raw: [u32; 21],
    pub total_amount: f32,
    pub buckets: [u16; 16],
    pub main_net: f64,
    pub main_ratio: f64,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoneyFlowBlock {
    pub market_id: u8,
    pub market: Option<Market>,
    pub code: String,
    pub records: Vec<MoneyFlowRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoneyFlowBatch {
    pub request: MoneyFlowRequest,
    pub blocks: Vec<MoneyFlowBlock>,
    pub raw_payload: Bytes,
}

pub fn parse_money_flow_payload(
    payload: &[u8],
    request: MoneyFlowRequest,
) -> Result<MoneyFlowBatch, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "money flow payload",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < MONEY_FLOW_HEADER_SIZE {
        return Err(ProtocolError::invalid_data(
            "money flow",
            "payload is shorter than the instrument header",
        ));
    }

    let mut blocks = Vec::new();
    let mut offset = 0_usize;
    while offset < payload.len() {
        let header_end = offset + MONEY_FLOW_HEADER_SIZE;
        let header = payload.get(offset..header_end).ok_or_else(|| {
            ProtocolError::invalid_data("money flow", "truncated instrument header")
        })?;
        let market_id = header[0];
        let market = Market::from_id(i64::from(market_id)).ok();
        let code = String::from_utf8(header[2..8].to_vec()).map_err(|_| {
            ProtocolError::invalid_data("money flow", "instrument code is not ASCII")
        })?;
        if !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ProtocolError::invalid_data(
                "money flow",
                "instrument code contains non-digit bytes",
            ));
        }
        let reported_count = usize::from(little_u16(&header[0x26..0x28])?);
        let available = (payload.len() - header_end) / MONEY_FLOW_RECORD_SIZE;
        let count = if reported_count == 0 {
            available
        } else {
            reported_count
        };
        let records_end = header_end
            .checked_add(count.checked_mul(MONEY_FLOW_RECORD_SIZE).ok_or_else(|| {
                ProtocolError::invalid_data("money flow", "record length overflow")
            })?)
            .ok_or_else(|| ProtocolError::invalid_data("money flow", "record length overflow"))?;
        if records_end > payload.len() {
            return Err(ProtocolError::invalid_data(
                "money flow",
                "truncated money-flow records",
            ));
        }
        let mut records = Vec::with_capacity(count);
        for index in 0..count {
            let start = header_end + index * MONEY_FLOW_RECORD_SIZE;
            records.push(parse_money_flow_record(
                &payload[start..start + MONEY_FLOW_RECORD_SIZE],
            )?);
        }
        blocks.push(MoneyFlowBlock {
            market_id,
            market,
            code,
            records,
        });
        offset = records_end;
    }

    Ok(MoneyFlowBatch {
        request,
        blocks,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn parse_money_flow_record(record: &[u8]) -> Result<MoneyFlowRecord, ProtocolError> {
    if record.len() != MONEY_FLOW_RECORD_SIZE {
        return Err(ProtocolError::invalid_data(
            "money flow",
            "invalid record length",
        ));
    }
    let date_raw = little_u32(&record[..4])?;
    let date = DateParts::from_yyyymmdd(date_raw);
    let mut raw = [0_u32; 21];
    for (index, value) in raw.iter_mut().enumerate() {
        *value = little_u32(&record[4 + index * 4..8 + index * 4])?;
    }
    let total_amount = little_f32(&record[8..12])?;
    let mut buckets = [0_u16; 16];
    for index in 0..8 {
        let packed = raw[10 + index];
        buckets[index * 2] = packed as u16;
        buckets[index * 2 + 1] = (packed >> 16) as u16;
    }
    let bucket = f64::from(buckets[0]) - f64::from(buckets[1]) + f64::from(buckets[4])
        - f64::from(buckets[5]);
    Ok(MoneyFlowRecord {
        date_raw,
        date,
        raw,
        total_amount,
        buckets,
        main_net: bucket / 50_000.0 * f64::from(total_amount),
        main_ratio: bucket / 500.0,
        record_hex: hex(record),
    })
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_money_flow_payload, MoneyFlowRequest, MONEY_FLOW_RECORD_SIZE};
    use crate::unit::NormalizedCode;

    #[test]
    fn request_uses_the_observed_40_byte_body() {
        let request = MoneyFlowRequest::new(NormalizedCode::parse("sz000063").unwrap());
        let raw = request.frame(0x007e_2d5e).encode().unwrap();
        assert_eq!(raw.len(), 50);
        assert_eq!(
            &raw[0..12],
            &[0x0c, 0x5e, 0x2d, 0x7e, 0, 1, 0x28, 0, 0x28, 0, 0xfc, 0x0f]
        );
        assert_eq!(&raw[12..20], &[0, 0, b'0', b'0', b'0', b'0', b'6', b'3']);
    }

    #[test]
    fn parses_a_five_record_body() {
        let mut payload = vec![0_u8; 0x28 + 5 * MONEY_FLOW_RECORD_SIZE];
        payload[2..8].copy_from_slice(b"000063");
        payload[0x26..0x28].copy_from_slice(&5_u16.to_le_bytes());
        let request = MoneyFlowRequest::new(NormalizedCode::parse("sz000063").unwrap());
        let parsed = parse_money_flow_payload(&payload, request).unwrap();
        assert_eq!(parsed.blocks[0].code, "000063");
        assert_eq!(parsed.blocks[0].records.len(), 5);
    }
}
