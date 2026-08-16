use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{decode_gbk_text, little_u16, little_u32, DateParts, DateTimeParts};

pub const TYPE_HEARTBEAT: u16 = 0x0004;
pub const TYPE_HANDSHAKE: u16 = 0x000d;
pub const HANDSHAKE_PAYLOAD_MIN_SIZE: usize = 189;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HandshakeRequest;

impl HandshakeRequest {
    pub fn frame(self, msg_id: u32) -> RequestFrame {
        RequestFrame::new(msg_id, TYPE_HANDSHAKE, Bytes::from_static(&[0x01]))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HeartbeatRequest;

impl HeartbeatRequest {
    pub fn frame(self, msg_id: u32) -> RequestFrame {
        RequestFrame::new(msg_id, TYPE_HEARTBEAT, Bytes::new())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandshakeInfo {
    pub server_datetime: Option<DateTimeParts>,
    pub session_minutes_1: Vec<String>,
    pub session_minutes_2: Vec<String>,
    pub server_date_1: Option<DateParts>,
    pub server_date_2: Option<DateParts>,
    pub server_name: String,
    pub product_tag: String,
    pub unknown_time_1_raw: u32,
    pub unknown_time_2_raw: u32,
    pub flags_raw: Bytes,
    pub tail_control_raw: Bytes,
    pub raw_payload: Bytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartbeatAck {
    pub reserved: Bytes,
    pub server_date_raw: u32,
    pub server_date: Option<DateParts>,
    pub raw_payload: Bytes,
}

pub fn parse_handshake_payload(payload: &[u8]) -> Result<HandshakeInfo, ProtocolError> {
    ensure_payload_bound(payload, "handshake")?;
    if payload.len() < HANDSHAKE_PAYLOAD_MIN_SIZE {
        return Err(ProtocolError::invalid_data(
            "handshake",
            format!("invalid handshake payload length: {}", payload.len()),
        ));
    }

    let date_1_raw = little_u32(&payload[42..46])?;
    let date_2_raw = little_u32(&payload[50..54])?;
    Ok(HandshakeInfo {
        server_datetime: parse_server_datetime(payload),
        session_minutes_1: parse_session_minutes(&payload[9..25])?,
        session_minutes_2: parse_session_minutes(&payload[25..41])?,
        server_date_1: DateParts::from_yyyymmdd(date_1_raw),
        server_date_2: DateParts::from_yyyymmdd(date_2_raw),
        server_name: decode_gbk_text(&payload[68..152]),
        product_tag: decode_gbk_text(&payload[160..189]),
        unknown_time_1_raw: little_u32(&payload[46..50])?,
        unknown_time_2_raw: little_u32(&payload[54..58])?,
        flags_raw: Bytes::copy_from_slice(&payload[58..68]),
        tail_control_raw: Bytes::copy_from_slice(&payload[152..160]),
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_heartbeat_payload(payload: &[u8]) -> Result<HeartbeatAck, ProtocolError> {
    ensure_payload_bound(payload, "heartbeat")?;
    if payload.len() < 10 {
        return Err(ProtocolError::invalid_data(
            "heartbeat",
            format!("invalid heartbeat payload length: {}", payload.len()),
        ));
    }
    let server_date_raw = little_u32(&payload[6..10])?;
    Ok(HeartbeatAck {
        reserved: Bytes::copy_from_slice(&payload[..6]),
        server_date_raw,
        server_date: DateParts::from_yyyymmdd(server_date_raw),
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn parse_server_datetime(payload: &[u8]) -> Option<DateTimeParts> {
    let year = little_u16(&payload[1..3]).ok().map(i32::from)?;
    let date = DateParts::new(year, payload[4], payload[3]).ok()?;
    DateTimeParts::naive(date, payload[6], payload[5], payload[8]).ok()
}

fn parse_session_minutes(payload: &[u8]) -> Result<Vec<String>, ProtocolError> {
    let mut values = Vec::with_capacity(8);
    for chunk in payload.chunks_exact(2).take(8) {
        let minute = little_u16(chunk)?;
        values.push(format!("{:02}:{:02}", minute / 60, minute % 60));
    }
    Ok(values)
}

fn ensure_payload_bound(payload: &[u8], context: &'static str) -> Result<(), ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: context,
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_handshake_payload, parse_heartbeat_payload, HandshakeRequest, HeartbeatRequest,
    };

    #[test]
    fn session_request_frames_match_the_frozen_bytes() {
        let handshake = HandshakeRequest.frame(1).encode();
        let heartbeat = HeartbeatRequest.frame(2).encode();

        assert!(matches!(
            handshake,
            Ok(raw) if raw.as_ref() == [0x0c, 1, 0, 0, 0, 1, 3, 0, 3, 0, 0x0d, 0, 1]
        ));
        assert!(matches!(
            heartbeat,
            Ok(raw) if raw.as_ref() == [0x0c, 2, 0, 0, 0, 1, 2, 0, 2, 0, 4, 0]
        ));
    }

    #[test]
    fn parses_naive_handshake_fields_and_raw_slices() {
        let mut payload = vec![0_u8; 189];
        payload[1..3].copy_from_slice(&2026_u16.to_le_bytes());
        payload[3..9].copy_from_slice(&[27, 5, 30, 10, 0, 0]);
        payload[9..25].copy_from_slice(&[
            0x3a, 0x02, 0x2a, 0x03, 0xc5, 0x03, 0x32, 0x04, 0x0c, 0x03, 0xfc, 0x03, 0x57, 0x04,
            0x84, 0x03,
        ]);
        payload[42..46].copy_from_slice(&20_260_527_u32.to_le_bytes());
        payload[50..54].copy_from_slice(&20_260_527_u32.to_le_bytes());
        payload[68..78].copy_from_slice(b"fake-7709\0");
        payload[160..173].copy_from_slice(b"fake-product\0");

        let parsed = parse_handshake_payload(&payload);
        assert!(matches!(
            parsed,
            Ok(info)
                if matches!(info.server_datetime, Some(value)
                    if value.date.year == 2026
                        && value.date.month == 5
                        && value.date.day == 27
                        && value.hour == 10
                        && value.minute == 30
                        && value.utc_offset_seconds.is_none())
                    && info.server_name == "fake-7709"
                    && info.product_tag == "fake-product"
                    && info.session_minutes_1.len() == 8
        ));
    }

    #[test]
    fn heartbeat_retains_invalid_dates_as_none() {
        let valid = parse_heartbeat_payload(&[0, 0, 0, 0, 0, 0, 0xa8, 0x26, 0x35, 0x01]);
        assert!(matches!(
            valid,
            Ok(value) if value.server_date_raw == 20_260_520 && value.server_date.is_some()
        ));

        let invalid = parse_heartbeat_payload(&[0, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
        assert!(matches!(invalid, Ok(value) if value.server_date.is_none()));
    }
}
