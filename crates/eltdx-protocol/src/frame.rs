use bytes::{Buf, Bytes, BytesMut};
use flate2::{Decompress, FlushDecompress, Status};

use crate::error::ProtocolError;
use crate::limits::{
    MAX_DECODED_QUEUE_BYTES, MAX_DECODED_QUEUE_FRAMES, MAX_RAW_STAGING_BUFFER_SIZE,
    MAX_REQUEST_DATA_SIZE, MAX_RESPONSE_BUFFER_SIZE, MAX_RESPONSE_PAYLOAD_SIZE,
    MAX_RESPONSE_RESYNC_BYTES, REQUEST_HEADER_SIZE, RESPONSE_HEADER_SIZE,
};
use crate::unit::{little_u16, little_u32};

pub const REQUEST_PREFIX: u8 = 0x0c;
pub const RESPONSE_PREFIX: [u8; 4] = [0xb1, 0xcb, 0x74, 0x00];
pub const DEFAULT_CONTROL: u8 = 0x01;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestFrame {
    pub msg_id: u32,
    pub msg_type: u16,
    pub data: Bytes,
    pub control: u8,
}

impl RequestFrame {
    pub fn new(msg_id: u32, msg_type: u16, data: impl Into<Bytes>) -> Self {
        Self {
            msg_id,
            msg_type,
            data: data.into(),
            control: DEFAULT_CONTROL,
        }
    }

    pub fn with_control(msg_id: u32, msg_type: u16, data: impl Into<Bytes>, control: u8) -> Self {
        Self {
            msg_id,
            msg_type,
            data: data.into(),
            control,
        }
    }

    pub fn encode(&self) -> Result<Bytes, ProtocolError> {
        if self.data.len() > MAX_REQUEST_DATA_SIZE {
            return Err(ProtocolError::LimitExceeded {
                resource: "request data",
                actual: self.data.len(),
                limit: MAX_REQUEST_DATA_SIZE,
            });
        }
        let length = u16::try_from(self.data.len() + 2)
            .map_err(|_| ProtocolError::invalid_data("request frame", "request length overflow"))?;
        let mut raw = Vec::with_capacity(REQUEST_HEADER_SIZE + self.data.len());
        raw.push(REQUEST_PREFIX);
        raw.extend_from_slice(&self.msg_id.to_le_bytes());
        raw.push(self.control);
        raw.extend_from_slice(&length.to_le_bytes());
        raw.extend_from_slice(&length.to_le_bytes());
        raw.extend_from_slice(&self.msg_type.to_le_bytes());
        raw.extend_from_slice(&self.data);
        Ok(Bytes::from(raw))
    }

    pub fn decode(raw: &[u8]) -> Result<Self, ProtocolError> {
        if raw.len() < REQUEST_HEADER_SIZE {
            return Err(ProtocolError::invalid_data(
                "request frame",
                format!("invalid request length: {}", raw.len()),
            ));
        }
        if raw[0] != REQUEST_PREFIX {
            return Err(ProtocolError::invalid_data(
                "request frame",
                format!("invalid request prefix: {:02x}", raw[0]),
            ));
        }

        let first_length = little_u16(&raw[6..8])?;
        let second_length = little_u16(&raw[8..10])?;
        if first_length != second_length {
            return Err(ProtocolError::invalid_data(
                "request frame",
                format!("request length fields differ: {first_length} != {second_length}"),
            ));
        }
        if first_length < 2 {
            return Err(ProtocolError::invalid_data(
                "request frame",
                format!("invalid request declared length: {first_length}"),
            ));
        }

        let data_length = usize::from(first_length - 2);
        let expected_length = REQUEST_HEADER_SIZE + data_length;
        if raw.len() != expected_length {
            return Err(ProtocolError::LengthMismatch {
                field: "request frame",
                expected: expected_length,
                actual: raw.len(),
            });
        }

        Ok(Self {
            msg_id: little_u32(&raw[1..5])?,
            msg_type: little_u16(&raw[10..12])?,
            data: Bytes::copy_from_slice(&raw[REQUEST_HEADER_SIZE..]),
            control: raw[5],
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseHeader {
    pub control: u8,
    pub msg_id: u32,
    pub reserved: u8,
    pub msg_type: u16,
    pub zip_length: u16,
    pub length: u16,
}

impl ResponseHeader {
    pub fn parse(raw: &[u8]) -> Result<Self, ProtocolError> {
        if raw.len() < RESPONSE_HEADER_SIZE {
            return Err(ProtocolError::invalid_data(
                "response frame",
                format!("invalid response length: {}", raw.len()),
            ));
        }
        if !raw.starts_with(&RESPONSE_PREFIX) {
            return Err(ProtocolError::invalid_data(
                "response frame",
                format!(
                    "invalid response prefix: {}",
                    encode_hex(&raw[..RESPONSE_PREFIX.len()])
                ),
            ));
        }

        let zip_length = little_u16(&raw[12..14])?;
        let length = little_u16(&raw[14..16])?;

        Ok(Self {
            control: raw[4],
            msg_id: little_u32(&raw[5..9])?,
            reserved: raw[9],
            msg_type: little_u16(&raw[10..12])?,
            zip_length,
            length,
        })
    }

    pub fn frame_size(self) -> usize {
        RESPONSE_HEADER_SIZE + usize::from(self.zip_length)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseFrame {
    pub control: u8,
    pub msg_id: u32,
    pub msg_type: u16,
    pub zip_length: u16,
    pub length: u16,
    pub data: Bytes,
    pub raw: Bytes,
    pub response_header_reserved: u8,
}

impl ResponseFrame {
    pub fn from_decoded(
        header: ResponseHeader,
        data: impl Into<Bytes>,
        raw: impl Into<Bytes>,
    ) -> Result<Self, ProtocolError> {
        let data = data.into();
        let raw = raw.into();
        if data.len() != usize::from(header.length) {
            return Err(ProtocolError::LengthMismatch {
                field: "decoded payload",
                expected: usize::from(header.length),
                actual: data.len(),
            });
        }
        if raw.len() != header.frame_size() {
            return Err(ProtocolError::LengthMismatch {
                field: "response frame",
                expected: header.frame_size(),
                actual: raw.len(),
            });
        }
        let raw_header = ResponseHeader::parse(&raw)?;
        if raw_header != header {
            return Err(ProtocolError::invalid_data(
                "response frame",
                "response header does not match raw frame",
            ));
        }

        Ok(Self {
            control: header.control,
            msg_id: header.msg_id,
            msg_type: header.msg_type,
            zip_length: header.zip_length,
            length: header.length,
            data,
            raw,
            response_header_reserved: header.reserved,
        })
    }

    pub fn header(&self) -> ResponseHeader {
        ResponseHeader {
            control: self.control,
            msg_id: self.msg_id,
            reserved: self.response_header_reserved,
            msg_type: self.msg_type,
            zip_length: self.zip_length,
            length: self.length,
        }
    }
}

#[derive(Debug)]
pub struct DecodeBatch {
    pub frames: Vec<ResponseFrame>,
    pub decoded_bytes: usize,
    pub budget_exhausted: bool,
}

#[derive(Debug)]
pub struct ResponseFrameDecoder {
    max_payload_size: usize,
    max_buffer_size: usize,
    max_resync_bytes: usize,
    buffer: BytesMut,
    resync_discarded: usize,
    max_buffer_observed: usize,
}

impl Default for ResponseFrameDecoder {
    fn default() -> Self {
        Self {
            max_payload_size: MAX_RESPONSE_PAYLOAD_SIZE,
            max_buffer_size: MAX_RESPONSE_BUFFER_SIZE,
            max_resync_bytes: MAX_RESPONSE_RESYNC_BYTES,
            buffer: BytesMut::with_capacity(MAX_RESPONSE_BUFFER_SIZE),
            resync_discarded: 0,
            max_buffer_observed: 0,
        }
    }
}

impl ResponseFrameDecoder {
    pub fn with_limits(
        max_payload_size: usize,
        max_buffer_size: usize,
        max_resync_bytes: usize,
    ) -> Result<Self, ProtocolError> {
        if max_payload_size > MAX_RESPONSE_PAYLOAD_SIZE {
            return Err(ProtocolError::invalid_argument(
                "max_payload_size",
                format!("max_payload_size must be between 0 and {MAX_RESPONSE_PAYLOAD_SIZE}"),
            ));
        }
        let minimum_buffer_size = RESPONSE_HEADER_SIZE
            .checked_add(max_payload_size)
            .ok_or_else(|| {
                ProtocolError::invalid_argument("max_buffer_size", "max_buffer_size overflow")
            })?;
        if max_buffer_size < minimum_buffer_size {
            return Err(ProtocolError::invalid_argument(
                "max_buffer_size",
                "max_buffer_size cannot hold the largest configured frame",
            ));
        }
        if max_buffer_size > MAX_RAW_STAGING_BUFFER_SIZE {
            return Err(ProtocolError::invalid_argument(
                "max_buffer_size",
                format!("max_buffer_size must be <= {MAX_RAW_STAGING_BUFFER_SIZE}"),
            ));
        }
        if max_resync_bytes > MAX_RESPONSE_RESYNC_BYTES {
            return Err(ProtocolError::invalid_argument(
                "max_resync_bytes",
                format!("max_resync_bytes must be <= {MAX_RESPONSE_RESYNC_BYTES}"),
            ));
        }

        Ok(Self {
            max_payload_size,
            max_buffer_size,
            max_resync_bytes,
            buffer: BytesMut::with_capacity(max_buffer_size),
            resync_discarded: 0,
            max_buffer_observed: 0,
        })
    }

    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }

    pub fn resync_discarded(&self) -> usize {
        self.resync_discarded
    }

    pub fn max_buffer_observed(&self) -> usize {
        self.max_buffer_observed
    }

    pub fn push(&mut self, data: &[u8]) -> usize {
        let available = self.max_buffer_size.saturating_sub(self.buffer.len());
        let accepted = available.min(data.len());
        self.buffer.extend_from_slice(&data[..accepted]);
        self.max_buffer_observed = self.max_buffer_observed.max(self.buffer.len());
        accepted
    }

    pub fn decode_available(
        &mut self,
        max_frames: usize,
        max_decoded_bytes: usize,
    ) -> Result<DecodeBatch, ProtocolError> {
        let frame_budget = max_frames.min(MAX_DECODED_QUEUE_FRAMES);
        let decoded_budget = max_decoded_bytes.min(MAX_DECODED_QUEUE_BYTES);
        let mut frames = Vec::with_capacity(frame_budget.min(64));
        let mut decoded_bytes = 0_usize;
        let mut budget_exhausted = false;

        loop {
            if self.buffer.is_empty() {
                break;
            }
            let prefix_index = find_response_prefix(&self.buffer);
            let Some(prefix_index) = prefix_index else {
                let keep = response_prefix_suffix_length(&self.buffer);
                let discard = self.buffer.len() - keep;
                self.discard_resync(discard)?;
                break;
            };
            if prefix_index > 0 {
                self.discard_resync(prefix_index)?;
            }
            if self.buffer.len() < RESPONSE_HEADER_SIZE {
                break;
            }

            let header = ResponseHeader::parse(&self.buffer)?;
            self.validate_header_limits(header)?;
            let frame_size = header.frame_size();
            if frame_size > self.max_buffer_size {
                return Err(ProtocolError::LimitExceeded {
                    resource: "response frame",
                    actual: frame_size,
                    limit: self.max_buffer_size,
                });
            }
            if self.buffer.len() < frame_size {
                break;
            }

            let next_decoded_bytes = usize::from(header.length);
            let decoded_after = decoded_bytes
                .checked_add(next_decoded_bytes)
                .ok_or_else(|| {
                    ProtocolError::invalid_data("response decoder", "decoded byte count overflow")
                })?;
            if frames.len() >= frame_budget || decoded_after > decoded_budget {
                budget_exhausted = true;
                break;
            }

            let raw = self.buffer.split_to(frame_size).freeze();
            frames.push(decode_response_bytes(raw, self.max_payload_size)?);
            decoded_bytes = decoded_after;
        }

        Ok(DecodeBatch {
            frames,
            decoded_bytes,
            budget_exhausted,
        })
    }

    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<ResponseFrame>, ProtocolError> {
        let mut frames = Vec::new();
        let mut offset = 0_usize;
        let mut decoded_bytes = 0_usize;

        loop {
            let before = self.buffer.len();
            if offset < data.len() {
                offset += self.push(&data[offset..]);
            }
            let remaining_frames = MAX_DECODED_QUEUE_FRAMES.saturating_sub(frames.len());
            let remaining_bytes = MAX_DECODED_QUEUE_BYTES.saturating_sub(decoded_bytes);
            let mut batch = self.decode_available(remaining_frames, remaining_bytes)?;
            decoded_bytes = decoded_bytes
                .checked_add(batch.decoded_bytes)
                .ok_or_else(|| {
                    ProtocolError::invalid_data("response decoder", "decoded byte count overflow")
                })?;
            frames.append(&mut batch.frames);
            if batch.budget_exhausted {
                let (resource, actual, limit) = if frames.len() >= MAX_DECODED_QUEUE_FRAMES {
                    (
                        "decoded frame queue",
                        MAX_DECODED_QUEUE_FRAMES + 1,
                        MAX_DECODED_QUEUE_FRAMES,
                    )
                } else {
                    (
                        "decoded byte queue",
                        MAX_DECODED_QUEUE_BYTES + 1,
                        MAX_DECODED_QUEUE_BYTES,
                    )
                };
                return Err(ProtocolError::LimitExceeded {
                    resource,
                    actual,
                    limit,
                });
            }

            if offset == data.len() {
                break;
            }
            if self.buffer.len() == before && self.buffer.len() == self.max_buffer_size {
                return Err(ProtocolError::LimitExceeded {
                    resource: "response buffer",
                    actual: self.buffer.len(),
                    limit: self.max_buffer_size,
                });
            }
        }
        Ok(frames)
    }

    pub fn finish(&mut self) -> Result<Vec<ResponseFrame>, ProtocolError> {
        let batch = self.decode_available(usize::MAX, usize::MAX)?;
        if batch.budget_exhausted {
            return Err(ProtocolError::invalid_data(
                "response decoder",
                "decoded queue limit reached at EOF",
            ));
        }
        if !self.buffer.is_empty() {
            return Err(ProtocolError::invalid_data(
                "response frame",
                format!(
                    "truncated response frame at EOF: {} buffered bytes",
                    self.buffer.len()
                ),
            ));
        }
        Ok(batch.frames)
    }

    pub fn clear_generation(&mut self) {
        self.buffer.clear();
        self.resync_discarded = 0;
        self.max_buffer_observed = 0;
    }

    fn validate_header_limits(&self, header: ResponseHeader) -> Result<(), ProtocolError> {
        let zip_length = usize::from(header.zip_length);
        if zip_length > self.max_payload_size {
            return Err(ProtocolError::LimitExceeded {
                resource: "compressed payload",
                actual: zip_length,
                limit: self.max_payload_size,
            });
        }
        let length = usize::from(header.length);
        if length > self.max_payload_size {
            return Err(ProtocolError::LimitExceeded {
                resource: "decoded payload",
                actual: length,
                limit: self.max_payload_size,
            });
        }
        Ok(())
    }

    fn discard_resync(&mut self, count: usize) -> Result<(), ProtocolError> {
        let discarded = self.resync_discarded.checked_add(count).ok_or_else(|| {
            ProtocolError::invalid_data("response decoder", "resync byte count overflow")
        })?;
        self.resync_discarded = discarded;
        if discarded > self.max_resync_bytes {
            return Err(ProtocolError::LimitExceeded {
                resource: "response resync",
                actual: discarded,
                limit: self.max_resync_bytes,
            });
        }
        self.buffer.advance(count);
        Ok(())
    }
}

pub fn decode_response(
    raw: &[u8],
    max_payload_size: usize,
) -> Result<ResponseFrame, ProtocolError> {
    decode_response_bytes(Bytes::copy_from_slice(raw), max_payload_size)
}

fn decode_response_bytes(
    raw: Bytes,
    max_payload_size: usize,
) -> Result<ResponseFrame, ProtocolError> {
    if max_payload_size > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::invalid_argument(
            "max_payload_size",
            format!("max_payload_size must be <= {MAX_RESPONSE_PAYLOAD_SIZE}"),
        ));
    }
    let header = ResponseHeader::parse(&raw)?;
    let zip_length = usize::from(header.zip_length);
    if zip_length > max_payload_size {
        return Err(ProtocolError::LimitExceeded {
            resource: "compressed payload",
            actual: zip_length,
            limit: max_payload_size,
        });
    }
    let length = usize::from(header.length);
    if length > max_payload_size {
        return Err(ProtocolError::LimitExceeded {
            resource: "decoded payload",
            actual: length,
            limit: max_payload_size,
        });
    }

    let expected_raw_length = header.frame_size();
    if raw.len() != expected_raw_length {
        let actual = raw.len().saturating_sub(RESPONSE_HEADER_SIZE);
        return Err(ProtocolError::invalid_data(
            "response frame",
            format!("zip length mismatch: expected {zip_length}, got {actual}"),
        ));
    }

    let data = if zip_length == length {
        raw.slice(RESPONSE_HEADER_SIZE..)
    } else {
        decode_compressed_payload(&raw[RESPONSE_HEADER_SIZE..], length)?
    };
    ResponseFrame::from_decoded(header, data, raw)
}

fn decode_compressed_payload(payload: &[u8], length: usize) -> Result<Bytes, ProtocolError> {
    let output_limit = length
        .checked_add(1)
        .ok_or_else(|| ProtocolError::invalid_data("zlib", "decoded length overflow"))?;
    let mut output = vec![0_u8; output_limit];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(payload, &mut output, FlushDecompress::Finish)
        .map_err(|error| ProtocolError::Compression {
            message: error.to_string(),
        })?;
    let written = usize::try_from(decoder.total_out())
        .map_err(|_| ProtocolError::invalid_data("zlib", "decoded length overflow"))?;
    if written > length {
        return Err(ProtocolError::invalid_data(
            "zlib",
            format!("decoded payload exceeds declared length: {written} > {length}"),
        ));
    }
    if status != Status::StreamEnd {
        return Err(ProtocolError::invalid_data(
            "zlib",
            "compressed response ended before zlib stream EOF",
        ));
    }
    let consumed = usize::try_from(decoder.total_in())
        .map_err(|_| ProtocolError::invalid_data("zlib", "compressed length overflow"))?;
    if consumed != payload.len() {
        return Err(ProtocolError::invalid_data(
            "zlib",
            "compressed response contains trailing data",
        ));
    }
    if written != length {
        return Err(ProtocolError::invalid_data(
            "zlib",
            format!("decoded length mismatch: expected {length}, got {written}"),
        ));
    }

    output.truncate(written);
    Ok(Bytes::from(output))
}

fn find_response_prefix(data: &[u8]) -> Option<usize> {
    data.windows(RESPONSE_PREFIX.len())
        .position(|window| window == RESPONSE_PREFIX.as_slice())
}

fn response_prefix_suffix_length(data: &[u8]) -> usize {
    let maximum = data.len().min(RESPONSE_PREFIX.len() - 1);
    for size in (1..=maximum).rev() {
        if data[data.len() - size..] == RESPONSE_PREFIX[..size] {
            return size;
        }
    }
    0
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
    use bytes::Bytes;

    use super::{
        decode_response, RequestFrame, ResponseFrame, ResponseFrameDecoder, ResponseHeader,
    };
    use crate::limits::MAX_REQUEST_DATA_SIZE;

    #[test]
    fn request_frame_matches_the_frozen_python_header() {
        let frame = RequestFrame::new(
            123,
            0x044e,
            Bytes::from_static(&[0x00, 0x00, 0xa7, 0x26, 0x35, 0x01]),
        );
        let raw = frame.encode();

        assert!(matches!(
            raw.as_ref(),
            Ok(value) if value.as_ref() == [
                0x0c, 0x7b, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x08, 0x00, 0x4e,
                0x04, 0x00, 0x00, 0xa7, 0x26, 0x35, 0x01,
            ]
        ));
        assert_eq!(
            raw.and_then(|value| RequestFrame::decode(&value)),
            Ok(frame)
        );
    }

    #[test]
    fn request_frame_rejects_oversized_data_before_encoding() {
        let frame = RequestFrame::new(1, 4, vec![0; MAX_REQUEST_DATA_SIZE + 1]);

        assert!(frame.encode().is_err());
    }

    #[test]
    fn response_header_and_decoded_frame_preserve_all_fields() {
        let raw = Bytes::from_static(&[
            0xb1, 0xcb, 0x74, 0x00, 0x02, 0x78, 0x56, 0x34, 0x12, 0xaa, 0x4e, 0x04, 0x02, 0x00,
            0x02, 0x00, 0x34, 0x12,
        ]);
        let header = ResponseHeader::parse(&raw);
        assert_eq!(
            header,
            Ok(ResponseHeader {
                control: 2,
                msg_id: 0x1234_5678,
                reserved: 0xaa,
                msg_type: 0x044e,
                zip_length: 2,
                length: 2,
            })
        );

        let frame = header.and_then(|value| {
            ResponseFrame::from_decoded(value, Bytes::from_static(&[0x34, 0x12]), raw)
        });
        assert!(matches!(
            frame,
            Ok(value)
                if value.msg_id == 0x1234_5678
                    && value.response_header_reserved == 0xaa
                    && value.data == Bytes::from_static(&[0x34, 0x12])
        ));
    }

    #[test]
    fn response_frame_rejects_mismatched_decoded_length() {
        let raw = Bytes::from_static(&[
            0xb1, 0xcb, 0x74, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0x00,
            0x02, 0x00, 0x00,
        ]);
        let header = ResponseHeader::parse(&raw);
        assert!(matches!(
            header.and_then(|value| ResponseFrame::from_decoded(value, Bytes::new(), raw)),
            Err(crate::ProtocolError::LengthMismatch {
                field: "decoded payload",
                expected: 2,
                actual: 0,
            })
        ));
    }

    #[test]
    fn response_decoder_handles_zlib_and_rejects_trailing_data() {
        let compressed_hello = [
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
        ];
        let mut raw = vec![
            0xb1, 0xcb, 0x74, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x0d, 0x00,
            0x05, 0x00,
        ];
        raw.extend_from_slice(&compressed_hello);
        assert!(matches!(
            decode_response(&raw, 65_535),
            Ok(frame) if frame.data.as_ref() == b"hello"
        ));

        raw[12..14].copy_from_slice(&14_u16.to_le_bytes());
        raw.push(0);
        assert!(decode_response(&raw, 65_535).is_err());
    }

    #[test]
    fn incremental_decoder_accepts_every_single_byte_fragment() {
        let raw = [
            0xb1, 0xcb, 0x74, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12, 0x00, 0x4e, 0x04, 0x02, 0x00,
            0x02, 0x00, 0x34, 0x12,
        ];
        let mut decoder = ResponseFrameDecoder::default();
        let mut frames = Vec::new();
        for byte in raw {
            let decoded = decoder.feed(&[byte]);
            assert!(decoded.is_ok(), "unexpected decoder result: {decoded:?}");
            if let Ok(mut values) = decoded {
                frames.append(&mut values);
            }
        }

        assert_eq!(frames.len(), 1);
        assert_eq!(decoder.buffered_bytes(), 0);
        assert!(decoder.finish().is_ok());
    }

    #[test]
    fn decoder_resynchronizes_and_honors_per_turn_budgets() {
        let frame = [
            0xb1, 0xcb, 0x74, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0x00,
            0x01, 0x00, 0x7f,
        ];
        let mut wire = vec![0xff, 0xee];
        wire.extend_from_slice(&frame);
        wire.extend_from_slice(&frame);
        let mut decoder = ResponseFrameDecoder::default();
        assert_eq!(decoder.push(&wire), wire.len());

        let first = decoder.decode_available(1, 1);
        assert!(matches!(
            first,
            Ok(ref batch)
                if batch.frames.len() == 1
                    && batch.decoded_bytes == 1
                    && batch.budget_exhausted
        ));
        assert_eq!(decoder.resync_discarded(), 2);
        let second = decoder.decode_available(1, 1);
        assert!(matches!(second, Ok(ref batch) if batch.frames.len() == 1));
    }

    #[test]
    fn decoder_keeps_partial_prefix_and_reports_truncated_eof() {
        let mut decoder = ResponseFrameDecoder::default();
        assert!(decoder.feed(&[0xff, 0xb1, 0xcb]).is_ok());
        assert_eq!(decoder.buffered_bytes(), 2);
        assert_eq!(decoder.resync_discarded(), 1);
        assert!(decoder.finish().is_err());
    }
}
