use crate::{Error, ErrorKind, Result};

/// A JSON object is an ordered sequence, including repeated raw member names.
pub type OrderedJsonObject = Vec<(String, JsonValue)>;

/// Validated JSON numeric text. No float conversion occurs during decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonNumber(String);

impl JsonNumber {
    pub fn new(lexeme: impl Into<String>) -> Result<Self> {
        let lexeme = lexeme.into();
        if number_end(lexeme.as_bytes(), 0) != Some(lexeme.len()) {
            return Err(Error::new(ErrorKind::InvalidArgument, "invalid JSON number"));
        }
        Ok(Self(lexeme))
    }
    pub fn lexeme(&self) -> &str { &self.0 }
    pub fn as_i64(&self) -> Option<i64> { self.0.parse().ok() }
    pub fn as_u64(&self) -> Option<u64> { self.0.parse().ok() }
    /// Explicit floating conversion; overflowing/nonfinite values are rejected.
    pub fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|value| value.is_finite())
    }
}

impl From<i64> for JsonNumber { fn from(value: i64) -> Self { Self(value.to_string()) } }
impl From<u64> for JsonNumber { fn from(value: u64) -> Self { Self(value.to_string()) } }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonValue>),
    Object(OrderedJsonObject),
}

impl JsonValue {
    /// Parse at most 16 MiB of UTF-8 (an initial UTF-8 BOM is permitted).
    /// HTTP uses its explicitly configured byte limit and cancellation boundary.
    pub fn parse(bytes: &[u8]) -> Result<Self> { decode(bytes, 16 * 1024 * 1024, &mut || Ok(())) }
    pub fn as_str(&self) -> Option<&str> { if let Self::String(value) = self { Some(value) } else { None } }
    pub fn as_array(&self) -> Option<&[Self]> { if let Self::Array(value) = self { Some(value) } else { None } }
    pub fn as_object(&self) -> Option<&[(String, Self)]> { if let Self::Object(value) = self { Some(value) } else { None } }
    pub fn as_number(&self) -> Option<&JsonNumber> { if let Self::Number(value) = self { Some(value) } else { None } }
    /// Last matching member, as used by the Python compatibility projection.
    /// Use `get_all` to inspect every raw occurrence in its original order.
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.iter().rev().find(|(name, _)| name == key).map(|(_, value)| value)
    }
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Self> + 'a {
        self.as_object().unwrap_or_default().iter()
            .filter_map(move |(name, value)| (name == key).then_some(value))
    }
    /// Compact UTF-8 JSON; string content is not converted to ASCII escapes.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        encode(self, 16 * 1024 * 1024, &mut || Ok(()))
    }
}

impl From<&str> for JsonValue { fn from(value: &str) -> Self { Self::String(value.to_owned()) } }
impl From<String> for JsonValue { fn from(value: String) -> Self { Self::String(value) } }
impl From<i64> for JsonValue { fn from(value: i64) -> Self { Self::Number(value.into()) } }
impl From<u64> for JsonValue { fn from(value: u64) -> Self { Self::Number(value.into()) } }
impl From<bool> for JsonValue { fn from(value: bool) -> Self { Self::Bool(value) } }

fn invalid(offset: usize, message: &str) -> Error {
    Error::new(ErrorKind::Protocol, message).with_context("json_offset", offset.to_string())
}
fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "cannot allocate bounded JSON storage").with_source(error)
}
fn push<T>(target: &mut Vec<T>, item: T) -> Result<()> {
    target.try_reserve(1).map_err(allocation)?;
    target.push(item);
    Ok(())
}

pub(crate) fn decode(bytes: &[u8], maximum: usize, boundary: &mut dyn FnMut() -> Result<()>) -> Result<JsonValue> {
    boundary()?;
    if bytes.len() > maximum { return Err(Error::new(ErrorKind::LimitExceeded, "decoded JSON exceeds byte limit")); }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    std::str::from_utf8(bytes).map_err(|error| Error::new(ErrorKind::Protocol, "TQLEX returned invalid UTF-8").with_source(error))?;
    let mut parser = Parser { bytes, offset: 0, nodes: 0, next_boundary: 0, boundary };
    let value = parser.value(0)?;
    parser.space();
    if parser.offset != bytes.len() { return Err(invalid(parser.offset, "trailing data after JSON value")); }
    (parser.boundary)()?;
    Ok(value)
}

struct Parser<'a, 'b> {
    bytes: &'a [u8], offset: usize, nodes: usize, next_boundary: usize,
    boundary: &'b mut dyn FnMut() -> Result<()>,
}
impl Parser<'_, '_> {
    fn space(&mut self) {
        while self.bytes.get(self.offset).is_some_and(|byte| matches!(byte, b' ' | b'\r' | b'\n' | b'\t')) { self.offset += 1; }
    }
    fn checkpoint(&mut self) -> Result<()> {
        if self.offset >= self.next_boundary {
            (self.boundary)()?;
            self.next_boundary = self.offset.saturating_add(4096);
        }
        Ok(())
    }
    fn take(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.offset) == Some(&byte) { self.offset += 1; true } else { false }
    }
    fn string(&mut self) -> Result<String> {
        let start = self.offset;
        if !self.take(b'"') { return Err(invalid(start, "JSON object key must be a string")); }
        let mut escaped = false;
        while let Some(&byte) = self.bytes.get(self.offset) {
            self.checkpoint()?;
            self.offset += 1;
            if byte == b'"' && !escaped {
                let value = serde_json::from_slice(&self.bytes[start..self.offset])
                    .map_err(|error| invalid(start, "invalid JSON string").with_source(error))?;
                (self.boundary)()?;
                return Ok(value);
            }
            escaped = byte == b'\\' && !escaped;
        }
        Err(invalid(start, "unterminated JSON string"))
    }
    fn value(&mut self, depth: usize) -> Result<JsonValue> {
        self.checkpoint()?;
        if depth > 128 || self.nodes >= 1_000_000 {
            return Err(Error::new(ErrorKind::LimitExceeded, "JSON nesting or node limit exceeded"));
        }
        self.nodes += 1;
        self.space();
        match self.bytes.get(self.offset).copied() {
            Some(b'"') => Ok(JsonValue::String(self.string()?)),
            Some(b'[') => {
                self.offset += 1;
                self.space();
                let mut values = Vec::new();
                if self.take(b']') { return Ok(JsonValue::Array(values)); }
                loop {
                    push(&mut values, self.value(depth + 1)?)?;
                    self.space();
                    if self.take(b']') { return Ok(JsonValue::Array(values)); }
                    if !self.take(b',') { return Err(invalid(self.offset, "expected JSON array separator")); }
                }
            }
            Some(b'{') => {
                self.offset += 1;
                self.space();
                let mut values = Vec::new();
                if self.take(b'}') { return Ok(JsonValue::Object(values)); }
                loop {
                    self.space();
                    let name = self.string()?;
                    self.space();
                    if !self.take(b':') { return Err(invalid(self.offset, "expected JSON object colon")); }
                    push(&mut values, (name, self.value(depth + 1)?))?;
                    self.space();
                    if self.take(b'}') { return Ok(JsonValue::Object(values)); }
                    if !self.take(b',') { return Err(invalid(self.offset, "expected JSON object separator")); }
                }
            }
            Some(b'n') => { self.literal(b"null")?; Ok(JsonValue::Null) }
            Some(b't') => { self.literal(b"true")?; Ok(JsonValue::Bool(true)) }
            Some(b'f') => { self.literal(b"false")?; Ok(JsonValue::Bool(false)) }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.offset;
                let end = number_end(self.bytes, start).ok_or_else(|| invalid(start, "invalid JSON number"))?;
                // The complete body is already checked UTF-8; this slice is ASCII.
                let text = std::str::from_utf8(&self.bytes[start..end]).map_err(|error| invalid(start, "invalid numeric text").with_source(error))?;
                self.offset = end;
                self.checkpoint()?;
                Ok(JsonValue::Number(JsonNumber(text.to_owned())))
            }
            _ => Err(invalid(self.offset, "expected JSON value")),
        }
    }
    fn literal(&mut self, expected: &[u8]) -> Result<()> {
        let end = self.offset.saturating_add(expected.len());
        if self.bytes.get(self.offset..end) != Some(expected) { return Err(invalid(self.offset, "invalid JSON literal")); }
        self.offset = end;
        Ok(())
    }
}

fn number_end(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    if bytes.get(cursor) == Some(&b'-') { cursor += 1; }
    match bytes.get(cursor) {
        Some(b'0') => cursor += 1,
        Some(b'1'..=b'9') => { cursor += 1; while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; } }
        _ => return None,
    }
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
        if cursor == start { return None; }
    }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) { cursor += 1; }
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
        if cursor == start { return None; }
    }
    Some(cursor)
}

pub(crate) fn encode(value: &JsonValue, maximum: usize, boundary: &mut dyn FnMut() -> Result<()>) -> Result<Vec<u8>> {
    let mut encoder = Encoder { bytes: Vec::new(), maximum, nodes: 0, boundary };
    encoder.value(value, 0)?;
    (encoder.boundary)()?;
    Ok(encoder.bytes)
}
/// Params is part of the same JSON document. Keep its wrapper and every
/// element within one byte, depth and node budget instead of resetting limits
/// separately for each item.
pub(crate) fn encode_params(values: &[JsonValue], maximum: usize, boundary: &mut dyn FnMut() -> Result<()>) -> Result<Vec<u8>> {
    let mut encoder = Encoder { bytes: Vec::new(), maximum, nodes: 2, boundary };
    encoder.append(b"{\"Params\":[")?;
    for (index, value) in values.iter().enumerate() {
        if index != 0 { encoder.append(b",")?; }
        encoder.value(value, 2)?;
    }
    encoder.append(b"]}")?;
    (encoder.boundary)()?;
    Ok(encoder.bytes)
}

/// Copy retained values with the same bounded traversal used by parsing. A
/// derived Clone on a whole response can run for arbitrarily long after the
/// parent's deadline, even if parsing itself checked cancellation frequently.
pub(crate) fn clone_value(value: &JsonValue, boundary: &mut dyn FnMut() -> Result<()>) -> Result<JsonValue> {
    Cloner { nodes: 0, boundary }.value(value, 0)
}
pub(crate) fn clone_object(values: &OrderedJsonObject, boundary: &mut dyn FnMut() -> Result<()>) -> Result<OrderedJsonObject> {
    Cloner { nodes: 1, boundary }.object(values, 0)
}
struct Cloner<'a> { nodes: usize, boundary: &'a mut dyn FnMut() -> Result<()> }
impl Cloner<'_> {
    fn text(&mut self, value: &str) -> Result<String> {
        let mut output = Vec::new();
        for chunk in value.as_bytes().chunks(4096) {
            (self.boundary)()?;
            output.try_reserve(chunk.len()).map_err(allocation)?;
            output.extend_from_slice(chunk);
        }
        String::from_utf8(output).map_err(|error| Error::new(ErrorKind::Internal, "invalid copied JSON text").with_source(error))
    }
    fn object(&mut self, values: &OrderedJsonObject, depth: usize) -> Result<OrderedJsonObject> {
        let mut output = Vec::new();
        for (name, value) in values {
            let name = self.text(name)?;
            let value = self.value(value, depth + 1)?;
            push(&mut output, (name, value))?;
        }
        Ok(output)
    }
    fn value(&mut self, value: &JsonValue, depth: usize) -> Result<JsonValue> {
        (self.boundary)()?;
        if depth > 128 || self.nodes >= 1_000_000 {
            return Err(Error::new(ErrorKind::LimitExceeded, "retained JSON nesting or node limit exceeded"));
        }
        self.nodes += 1;
        Ok(match value {
            JsonValue::Null => JsonValue::Null,
            JsonValue::Bool(value) => JsonValue::Bool(*value),
            JsonValue::Number(value) => JsonValue::Number(JsonNumber(self.text(value.lexeme())?)),
            JsonValue::String(value) => JsonValue::String(self.text(value)?),
            JsonValue::Array(values) => {
                let mut output = Vec::new();
                for value in values { push(&mut output, self.value(value, depth + 1)?)?; }
                JsonValue::Array(output)
            }
            JsonValue::Object(values) => JsonValue::Object(self.object(values, depth)?),
        })
    }
}
struct Encoder<'a> { bytes: Vec<u8>, maximum: usize, nodes: usize, boundary: &'a mut dyn FnMut() -> Result<()> }
impl Encoder<'_> {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        (self.boundary)()?;
        let size = self.bytes.len().checked_add(bytes.len()).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "JSON size overflow"))?;
        if size > self.maximum { return Err(Error::new(ErrorKind::LimitExceeded, "encoded JSON exceeds byte limit")); }
        self.bytes.try_reserve(bytes.len()).map_err(allocation)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn string(&mut self, value: &str) -> Result<()> {
        self.append(b"\"")?;
        for chunk in value.as_bytes().chunks(4096) {
            let mut start = 0;
            for (index, &byte) in chunk.iter().enumerate() {
                if byte > 31 && byte != b'"' && byte != b'\\' { continue; }
                self.append(&chunk[start..index])?;
                match byte {
                    b'"' => self.append(b"\\\"")?,
                    b'\\' => self.append(b"\\\\")?,
                    0..=31 => self.append(format!("\\u{byte:04x}").as_bytes())?,
                    _ => unreachable!("only JSON escapes enter this branch"),
                }
                start = index + 1;
            }
            self.append(&chunk[start..])?;
        }
        self.append(b"\"")
    }
    fn value(&mut self, value: &JsonValue, depth: usize) -> Result<()> {
        if depth > 128 || self.nodes >= 1_000_000 { return Err(Error::new(ErrorKind::LimitExceeded, "JSON nesting or node limit exceeded")); }
        self.nodes += 1;
        match value {
            JsonValue::Null => self.append(b"null"),
            JsonValue::Bool(true) => self.append(b"true"),
            JsonValue::Bool(false) => self.append(b"false"),
            JsonValue::Number(number) => self.append(number.lexeme().as_bytes()),
            JsonValue::String(text) => self.string(text),
            JsonValue::Array(values) => {
                self.append(b"[")?;
                for (index, value) in values.iter().enumerate() {
                    if index != 0 { self.append(b",")?; }
                    self.value(value, depth + 1)?;
                }
                self.append(b"]")
            }
            JsonValue::Object(values) => {
                self.append(b"{")?;
                for (index, (name, value)) in values.iter().enumerate() {
                    if index != 0 { self.append(b",")?; }
                    self.string(name)?;
                    self.append(b":")?;
                    self.value(value, depth + 1)?;
                }
                self.append(b"}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_limits_apply_before_json_allocation_and_during_encoding() {
        assert_eq!(decode(b"[1,2]", 4, &mut || Ok(())).unwrap_err().kind(), ErrorKind::LimitExceeded);
        assert_eq!(encode(&JsonValue::String("中文".to_owned()), 7, &mut || Ok(())).unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn decoder_and_encoder_honor_the_parent_cancellation_boundary() {
        let cancelled = || Err(Error::new(ErrorKind::Cancelled, "synthetic parent cancellation"));
        assert_eq!(decode(b"{}", 100, &mut { cancelled }).unwrap_err().kind(), ErrorKind::Cancelled);
        assert_eq!(encode(&JsonValue::Null, 100, &mut { cancelled }).unwrap_err().kind(), ErrorKind::Cancelled);
    }

    #[test]
    fn limits_prevent_unbounded_recursive_json() {
        let nested = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert_eq!(JsonValue::parse(nested.as_bytes()).unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn params_elements_share_node_and_wrapper_depth_limits() {
        let values = vec![JsonValue::Null; 999_999];
        assert_eq!(encode_params(&values, 16 * 1024 * 1024, &mut || Ok(())).unwrap_err().kind(), ErrorKind::LimitExceeded);
        let nested = format!("{}0{}", "[".repeat(127), "]".repeat(127));
        let value = JsonValue::parse(nested.as_bytes()).unwrap();
        assert_eq!(encode_params(&[value], 16 * 1024 * 1024, &mut || Ok(())).unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn retained_copy_checks_cancellation_inside_large_strings_and_preserves_order() -> Result<()> {
        let original = JsonValue::parse(br#"{"same":1e100,"same":null,"nested":["text",true]}"#)?;
        assert_eq!(clone_value(&original, &mut || Ok(()))?, original);
        let mut boundaries = 0;
        let error = clone_value(&JsonValue::String("x".repeat(20_000)), &mut || {
            boundaries += 1;
            if boundaries == 3 { Err(Error::new(ErrorKind::Cancelled, "cancel retained copy")) } else { Ok(()) }
        }).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Cancelled);
        Ok(())
    }
}
