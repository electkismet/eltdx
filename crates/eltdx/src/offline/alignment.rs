use std::num::NonZeroUsize;
use std::path::Path;
use crate::error::Result;
use super::{io_error, malformed};

/// Checks an existing file before any append, including an empty input batch.
/// Missing files are aligned. No bytes are truncated or repaired.
pub fn ensure_aligned(path: &Path, record_size: NonZeroUsize) -> Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error("metadata", path, error)),
    };
    let size = record_size.get() as u64;
    let tail = metadata.len() % size;
    if tail != 0 {
        return Err(malformed("file has a partial record; append refused")
            .with_context("record_size", size.to_string())
            .with_context("actual_length", metadata.len().to_string())
            .with_context("tail_bytes", tail.to_string()));
    }
    Ok(())
}
