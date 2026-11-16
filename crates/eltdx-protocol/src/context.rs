//! Explicit precision and diagnostic retention for one immutable decode context.

use bytes::Bytes;

use crate::unit::NormalizedCode;
use crate::ProtocolError;

/// A read-only precision source captured by the caller for an entire operation.
///
/// Implementations must not refresh metadata while decoding. A missing value is
/// not a verified decimal; audited fallback belongs in the source itself.
pub trait PricePrecisionSource: Send + Sync {
    fn security_decimal(&self, code: &NormalizedCode) -> Option<u8>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParseRawPolicy {
    #[default]
    Omit,
    RetainPayload,
}

pub struct ParseOptions<'a> {
    pub price_precision: &'a dyn PricePrecisionSource,
    pub raw: ParseRawPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordEncoding {
    Payload,
    /// Offsets are unchanged by XOR; decode only the requested range with 0x93.
    RefreshXor93,
}

/// A record's range in the original response payload, without retaining it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordRange {
    pub offset: usize,
    pub len: usize,
    pub encoding: RecordEncoding,
}

impl RecordRange {
    /// Returns a shared payload slice, or decodes just this refresh record.
    pub fn bytes(self, payload: &Bytes) -> Result<Bytes, ProtocolError> {
        let end = self.offset.checked_add(self.len).ok_or_else(|| {
            ProtocolError::invalid_argument("record range", "record range overflow")
        })?;
        if end > payload.len() {
            return Err(ProtocolError::invalid_argument(
                "record range",
                "record range exceeds retained payload",
            ));
        }
        let record = payload.slice(self.offset..end);
        Ok(match self.encoding {
            RecordEncoding::Payload => record,
            RecordEncoding::RefreshXor93 => {
                Bytes::from(record.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>())
            }
        })
    }

    /// Allocates hexadecimal diagnostics only when explicitly requested.
    pub fn hex(self, payload: &Bytes) -> Result<String, ProtocolError> {
        Ok(encode_hex(&self.bytes(payload)?))
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PricePrecision {
    pub(crate) divisor: u16,
    pub(crate) decimal: Option<u8>,
}

/// Parser implementation context. Only compatibility wrappers construct legacy.
pub(crate) struct ParseContext<'a> {
    options: Option<&'a ParseOptions<'a>>,
    payload: Option<Bytes>,
    base: usize,
    encoding: RecordEncoding,
}

impl<'a> ParseContext<'a> {
    pub(crate) fn new(payload: &Bytes, options: &'a ParseOptions<'a>) -> Self {
        Self {
            options: Some(options),
            payload: Some(payload.clone()),
            base: payload.as_ptr() as usize,
            encoding: RecordEncoding::Payload,
        }
    }

    pub(crate) fn legacy(payload: &[u8]) -> Self {
        Self {
            options: None,
            payload: None,
            base: payload.as_ptr() as usize,
            encoding: RecordEncoding::Payload,
        }
    }

    pub(crate) fn refresh_view(&self, decoded: &[u8]) -> Self {
        Self {
            options: self.options,
            payload: self.payload.clone(),
            base: decoded.as_ptr() as usize,
            encoding: RecordEncoding::RefreshXor93,
        }
    }

    pub(crate) fn is_legacy(&self) -> bool {
        self.options.is_none()
    }

    pub(crate) fn retains_payload(&self) -> bool {
        self.options
            .map_or(true, |options| options.raw == ParseRawPolicy::RetainPayload)
    }

    pub(crate) fn payload(&self, original: &[u8]) -> Bytes {
        if !self.retains_payload() {
            return Bytes::new();
        }
        self.payload
            .clone()
            .unwrap_or_else(|| Bytes::copy_from_slice(original))
    }

    /// Semantic unknown/native bytes survive Omit. In retain mode plain fields
    /// can share the page; Omit copies only that field, not its backing frame.
    pub(crate) fn business_bytes(&self, field: &[u8]) -> Bytes {
        if self.retains_payload() && self.encoding == RecordEncoding::Payload {
            if let Some(payload) = &self.payload {
                return payload.slice_ref(field);
            }
        }
        Bytes::copy_from_slice(field)
    }

    pub(crate) fn record_hex(&self, record: &[u8]) -> String {
        if self.is_legacy() {
            encode_hex(record)
        } else {
            String::new()
        }
    }

    pub(crate) fn record_range(&self, record: &[u8]) -> RecordRange {
        // All callers pass a subslice of the current parser view. No pointer is
        // dereferenced; the stored range is validated again by RecordRange::bytes.
        RecordRange {
            offset: (record.as_ptr() as usize).saturating_sub(self.base),
            len: record.len(),
            encoding: self.encoding,
        }
    }

    pub(crate) fn precision(&self, code: &NormalizedCode) -> Result<PricePrecision, ProtocolError> {
        let Some(options) = self.options else {
            return Ok(PricePrecision {
                divisor: crate::unit::price_divisor(code),
                decimal: crate::unit::security_decimal(code),
            });
        };
        let decimal = options.price_precision.security_decimal(code).ok_or_else(|| {
            ProtocolError::invalid_data(
                "price precision",
                format!("metadata unavailable for {code}"),
            )
        })?;
        Ok(PricePrecision {
            divisor: crate::unit::checked_price_divisor(decimal)?,
            decimal: Some(decimal),
        })
    }

}

fn encode_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(data.len() * 2);
    for byte in data {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
