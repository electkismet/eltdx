use std::num::NonZeroUsize;
use std::path::Path;
use crate::error::Result;
use super::{context_value, io_error, malformed};

/// Checks an existing file before any append, including an empty input batch.
/// Missing files are aligned. No bytes are truncated or repaired.
pub fn ensure_aligned(path: &Path, record_size: NonZeroUsize) -> Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error("metadata", path, error)),
    };
    let size = record_size.get() as u64;
    let tail = python_legacy::remainder_with(metadata.len(), size, |length, width| Ok::<_, crate::Error>(length % width))?;
    if tail != 0 {
        return Err(malformed("file has a partial record; append refused")
            .with_context("record_size", context_value(size))
            .with_context("actual_length", context_value(metadata.len()))
            .with_context("tail_bytes", context_value(tail)));
    }
    Ok(())
}

#[doc(hidden)]
pub mod python_legacy {
    /// The Python boundary supplies arbitrary-number modulo and its original
    /// exception semantics; Rust supplies positive integral record sizes.
    pub fn remainder_with<T, U, R, E>(length: T, width: U,
        modulo: impl FnOnce(T, U) -> std::result::Result<R, E>) -> std::result::Result<R, E>
    { modulo(length, width) }
}
