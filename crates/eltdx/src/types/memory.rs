//! Checked retained-memory charges for queued DTOs. Shared immutable buffers
//! are counted once per event; different events may conservatively pay again.
use std::collections::HashSet;
use std::mem::size_of;

use crate::error::{Error, ErrorKind, Result};

pub(crate) const BUFFER_CONTROL_BYTES: usize = 4 * size_of::<usize>();

pub(crate) fn canonical_vec<T>(values: &mut Vec<T>) {
    *values = std::mem::take(values).into_boxed_slice().into_vec();
}
pub(crate) fn canonical_string(value: &mut String) {
    *value = std::mem::take(value).into_boxed_str().into_string();
}

#[derive(Default)]
pub(crate) struct MemoryCharge {
    bytes: usize,
    buffers: HashSet<(usize, usize)>,
}

impl MemoryCharge {
    pub(crate) fn add(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(overflow)?;
        Ok(())
    }
    pub(crate) fn array<T>(&mut self, capacity: usize) -> Result<()> {
        self.add(capacity.checked_mul(size_of::<T>()).ok_or_else(overflow)?)
    }
    pub(crate) fn string(&mut self, string: &String) -> Result<()> { self.add(string.capacity()) }

    /// Queued raw buffers originate from exact-sized copies in the HQ decoder.
    /// Include a conservative shared-owner control block as well as content.
    pub(crate) fn buffer(&mut self, buffer: &bytes::Bytes) -> Result<()> {
        if buffer.is_empty() { return Ok(()); }
        let key = (buffer.as_ptr() as usize, buffer.len());
        if !self.buffers.contains(&key) {
            self.buffers.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded,
                "unable to account retained push buffers").with_source(error))?;
            self.buffers.insert(key);
            self.add(buffer.len())?;
            self.add(BUFFER_CONTROL_BYTES)?;
        }
        Ok(())
    }
    pub(crate) const fn total(&self) -> usize { self.bytes }
}

fn overflow() -> Error { Error::new(ErrorKind::LimitExceeded, "retained push memory charge overflow") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_uses_reserved_string_and_vector_storage_and_checked_arithmetic() {
        let mut text = String::with_capacity(4096);
        text.push_str("short");
        let mut charge = MemoryCharge::default();
        charge.string(&text).unwrap();
        charge.array::<u64>(20).unwrap();
        assert_eq!(charge.total(), text.capacity() + 20 * size_of::<u64>());
        charge.add(usize::MAX).unwrap_err();
        assert_eq!(charge.total(), text.capacity() + 20 * size_of::<u64>());
    }

    #[test]
    fn a_shared_payload_is_counted_once_within_an_event() {
        let payload = bytes::Bytes::copy_from_slice(&[7; 256]);
        let clone = payload.clone();
        let distinct = bytes::Bytes::copy_from_slice(&payload);
        let mut charge = MemoryCharge::default();
        charge.buffer(&payload).unwrap();
        let one = charge.total();
        charge.buffer(&clone).unwrap();
        assert_eq!(charge.total(), one);
        charge.buffer(&distinct).unwrap();
        assert_eq!(charge.total(), one * 2);
        charge.buffer(&bytes::Bytes::new()).unwrap();
        assert_eq!(charge.total(), one * 2);
    }
}
