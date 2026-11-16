//! Explicit, synchronous local-file access. No client, network executor, or background work.
//!
//! All legacy `write` entry points append. Callers must serialize shared writers: these
//! functions provide neither cross-process locking nor atomic batches. A write error
//! includes the actual `bytes_written` and may leave a prefix (including a partial record).

pub mod alignment;
pub mod bars;
pub mod block;
pub mod daily;
pub mod ex_daily;
pub mod gbbq;
pub mod industry;
pub mod minute;
pub mod paths;
mod models;
mod options;

pub use models::{BlockInfo, CoefficientDecision, Coefficients, CustomerBlock, DailyBar,
    Evidence, ExDailyBar, Exchange, IndustryCodes, IndustryMap, LocalCode, MinBar,
    MinuteKey, RecordKey, SecurityType, TimeOfDay, WireFloat};
pub use gbbq::GbbqRecord;
pub use options::{AppendOptions, Durability, ReadOptions, ReadResult, TailPolicy, WriteReport};

use crate::error::{Error, ErrorKind, Result};
use crate::types::{Diagnostic, EnrichmentStatus};

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "offline")
}

pub(crate) fn malformed(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "offline")
}

pub(crate) fn diagnostic(stage: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("offline", stage, EnrichmentStatus::Unavailable, None, Some(message.into()))
}

pub(crate) fn cancelled(token: Option<&crate::CancellationToken>) -> Result<()> {
    if token.is_some_and(crate::CancellationToken::is_cancelled) {
        return Err(Error::new(ErrorKind::Cancelled, "local operation cancelled")
            .with_context("domain", "offline"));
    }
    Ok(())
}

pub(crate) fn io_error(operation: &str, path: &std::path::Path, error: std::io::Error) -> Error {
    Error::new(ErrorKind::Io, format!("local {operation} failed"))
        .with_context("domain", "offline").with_context("operation", operation)
        .with_context("path", path.display().to_string()).with_source(error)
}

/// Python's GBK decoder replaces one invalid input byte, then resumes decoding.
/// Decode pairs separately so a bad pair does not swallow its ASCII second byte.
pub(crate) fn decode_gbk(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let first = bytes[offset];
        if first < 0x80 {
            output.push(char::from(first));
            offset += 1;
        } else if (0x81..=0xfe).contains(&first) && offset + 1 < bytes.len()
            && (0x40..=0xfe).contains(&bytes[offset + 1]) && bytes[offset + 1] != 0x7f
        {
            let (text, errors) = encoding_rs::GBK.decode_without_bom_handling(&bytes[offset..offset + 2]);
            if errors {
                output.push('\u{fffd}');
                offset += 1;
            } else {
                output.push_str(&text);
                offset += 2;
            }
        } else {
            output.push('\u{fffd}');
            offset += 1;
        }
    }
    output
}

pub(crate) fn ascii_replace(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| if byte.is_ascii() { char::from(*byte) } else { '\u{fffd}' }).collect()
}

/// `str.splitlines()` separators, including CRLF, as used by the Python readers.
pub(crate) fn source_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(['\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}',
        '\u{85}', '\u{2028}', '\u{2029}'])
}

pub(crate) fn source_trim(text: &str) -> &str {
    text.trim_matches(|value: char| value.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&value))
}
