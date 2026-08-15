use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProtocolError {
    #[error("{message}")]
    InvalidArgument { name: &'static str, message: String },

    #[error("unexpected end of payload")]
    UnexpectedEof {
        context: &'static str,
        offset: usize,
        needed: usize,
        remaining: usize,
    },

    #[error("{message}")]
    InvalidData {
        context: &'static str,
        message: String,
    },

    #[error("{resource} exceeds limit: {actual} > {limit}")]
    LimitExceeded {
        resource: &'static str,
        actual: usize,
        limit: usize,
    },

    #[error("{field} length mismatch: expected {expected}, got {actual}")]
    LengthMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("invalid compressed response payload: {message}")]
    Compression { message: String },

    #[error("unsupported command: 0x{command:04x}")]
    UnsupportedCommand { command: u16 },
}

impl ProtocolError {
    pub fn invalid_argument(name: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            name,
            message: message.into(),
        }
    }

    pub fn invalid_data(context: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidData {
            context,
            message: message.into(),
        }
    }

    pub fn unexpected_eof(
        context: &'static str,
        offset: usize,
        needed: usize,
        payload_len: usize,
    ) -> Self {
        Self::UnexpectedEof {
            context,
            offset,
            needed,
            remaining: payload_len.saturating_sub(offset),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProtocolError;

    #[test]
    fn unexpected_eof_never_underflows_remaining() {
        let error = ProtocolError::unexpected_eof("varint", 9, 1, 4);

        assert_eq!(
            error,
            ProtocolError::UnexpectedEof {
                context: "varint",
                offset: 9,
                needed: 1,
                remaining: 0,
            }
        );
    }

    #[test]
    fn display_preserves_public_facing_message() {
        let error = ProtocolError::invalid_argument("market", "invalid market: 'xx'");

        assert_eq!(error.to_string(), "invalid market: 'xx'");
    }
}
