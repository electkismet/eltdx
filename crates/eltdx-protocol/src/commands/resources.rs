use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{
    DEFAULT_FILE_CHUNK_SIZE, MAX_FILE_CHUNK_SIZE, MAX_FILE_PATH_BYTES, MAX_RESPONSE_PAYLOAD_SIZE,
};
use crate::unit::little_u32;

pub const TYPE_FILE_CONTENT: u16 = 0x06b9;
pub const FILE_PATH_FIELD_SIZE: usize = 300;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContentRequest {
    path: String,
    offset: u32,
    size: u32,
}

impl FileContentRequest {
    pub fn new(path: &str, offset: u32, size: u32) -> Result<Self, ProtocolError> {
        let path = normalize_path(path)?;
        if size == 0 {
            return Err(ProtocolError::invalid_argument(
                "size",
                "file content size must be > 0",
            ));
        }
        if size > MAX_FILE_CHUNK_SIZE {
            return Err(ProtocolError::invalid_argument(
                "size",
                format!("file content size must be <= {MAX_FILE_CHUNK_SIZE}"),
            ));
        }
        Ok(Self { path, offset, size })
    }

    pub fn with_defaults(path: &str) -> Result<Self, ProtocolError> {
        Self::new(path, 0, DEFAULT_FILE_CHUNK_SIZE)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn offset(&self) -> u32 {
        self.offset
    }

    pub const fn size(&self) -> u32 {
        self.size
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let path_raw = self.path.as_bytes();
        let mut data = Vec::with_capacity(8 + FILE_PATH_FIELD_SIZE);
        data.extend_from_slice(&self.offset.to_le_bytes());
        data.extend_from_slice(&self.size.to_le_bytes());
        data.extend_from_slice(path_raw);
        data.resize(8 + FILE_PATH_FIELD_SIZE, 0);
        RequestFrame::new(msg_id, TYPE_FILE_CONTENT, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContentChunk {
    pub request: FileContentRequest,
    pub chunk_len: u32,
    pub content: Bytes,
    pub trailing_payload: Bytes,
    pub raw_payload: Bytes,
}

impl FileContentChunk {
    pub fn is_last(&self) -> bool {
        self.chunk_len < self.request.size
    }
}

pub fn parse_file_content_payload(
    payload: &[u8],
    request: FileContentRequest,
) -> Result<FileContentChunk, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "file content",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() < 4 {
        return Err(ProtocolError::invalid_data(
            "file content",
            "invalid file content payload",
        ));
    }
    let chunk_len = little_u32(&payload[..4])?;
    let chunk_len_usize = usize::try_from(chunk_len).map_err(|_| {
        ProtocolError::invalid_data("file content", "file content chunk length overflow")
    })?;
    let expected_length = 4_usize.checked_add(chunk_len_usize).ok_or_else(|| {
        ProtocolError::invalid_data("file content", "file content payload length overflow")
    })?;
    if payload.len() < expected_length {
        return Err(ProtocolError::invalid_data(
            "file content",
            format!(
                "invalid file content payload length: expected {expected_length}, got {}",
                payload.len()
            ),
        ));
    }
    if chunk_len > request.size {
        return Err(ProtocolError::invalid_data(
            "file content",
            format!(
                "file content chunk exceeds requested size: {chunk_len} > {}",
                request.size
            ),
        ));
    }
    Ok(FileContentChunk {
        request,
        chunk_len,
        content: Bytes::copy_from_slice(&payload[4..expected_length]),
        trailing_payload: Bytes::copy_from_slice(&payload[expected_length..]),
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn normalize_path(value: &str) -> Result<String, ProtocolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::invalid_argument(
            "path",
            "file content path is required",
        ));
    }
    if trimmed.len() > MAX_FILE_PATH_BYTES {
        return Err(ProtocolError::invalid_argument(
            "path",
            format!("file content path exceeds {MAX_FILE_PATH_BYTES} ASCII bytes"),
        ));
    }
    if trimmed.as_bytes().contains(&0) {
        return Err(ProtocolError::invalid_argument(
            "path",
            "file content path must not contain NUL",
        ));
    }
    if !trimmed.is_ascii() {
        return Err(ProtocolError::invalid_argument(
            "path",
            "file content path must be ASCII",
        ));
    }
    Ok(trimmed.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::{parse_file_content_payload, FileContentRequest};
    use crate::ProtocolError;

    #[test]
    fn request_normalizes_path_and_matches_fixed_layout() -> Result<(), ProtocolError> {
        let request = FileContentRequest::new(" T0002\\hq_cache.dat ", 30_000, 12_000)?;
        let frame = request.frame(11);
        assert_eq!(frame.data.len(), 308);
        assert_eq!(&frame.data[..8], &[0x30, 0x75, 0, 0, 0xe0, 0x2e, 0, 0]);
        assert_eq!(&frame.data[8..26], b"T0002/hq_cache.dat");
        assert!(frame.data[26..].iter().all(|byte| *byte == 0));
        Ok(())
    }

    #[test]
    fn request_rejects_invalid_paths_and_sizes() {
        assert!(FileContentRequest::with_defaults("统计.zip").is_err());
        assert!(FileContentRequest::new("zhb.zip", 0, 0).is_err());
        assert!(FileContentRequest::new(&"a".repeat(301), 0, 1).is_err());
        assert!(FileContentRequest::new("zhb.zip", 0, 60_001).is_err());
    }

    #[test]
    fn response_preserves_exact_chunk_and_legal_tail() -> Result<(), ProtocolError> {
        let payload = [6, 0, 0, 0, b'a', b'b', b'c', b'1', b'2', b'3', 0xaa, 0xbb];
        let parsed =
            parse_file_content_payload(&payload, FileContentRequest::new("zhb.zip", 10, 30_000)?)?;
        assert_eq!(&parsed.content[..], b"abc123");
        assert_eq!(&parsed.trailing_payload[..], &[0xaa, 0xbb]);
        assert!(parsed.is_last());
        Ok(())
    }
}
