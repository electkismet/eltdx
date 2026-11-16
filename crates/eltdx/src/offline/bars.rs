//! Common bounded local I/O and the single append/filter/durability algorithm.
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroUsize;
use std::path::Path;
use crate::error::{Error, ErrorKind, Result};
use crate::{RawPolicy, TradingDate};
use super::{alignment, cancelled, context_value, diagnostic, invalid, io_error, malformed,
    AppendOptions, Durability, ReadOptions, ReadResult, RecordKey, TailPolicy, WriteReport};

pub(crate) struct ByteBudget { remaining: u64 }
impl ByteBudget {
    pub(crate) fn new(options: &ReadOptions) -> Self { Self { remaining: options.max_bytes() } }
}

pub(crate) fn load(path: &Path, options: &ReadOptions) -> Result<Vec<u8>> {
    load_with_budget(path, options, &mut ByteBudget::new(options))
}

pub(crate) fn load_with_budget(path: &Path, options: &ReadOptions, budget: &mut ByteBudget) -> Result<Vec<u8>> {
    cancelled(options.cancel())?;
    let mut handle = File::open(path).map_err(|error| io_error("open", path, error))?;
    let metadata = handle.metadata().map_err(|error| io_error("metadata", path, error))?;
    if !metadata.is_file() {
        return Err(io_error("read", path, std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not a regular file")));
    }
    if metadata.len() > budget.remaining { return Err(byte_limit(path, options.max_bytes())); }
    let mut data = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        cancelled(options.cancel())?;
        // One extra byte observes growth after metadata without silently truncating it.
        let request = (budget.remaining.saturating_add(1)).min(buffer.len() as u64) as usize;
        let count = match handle.read(&mut buffer[..request]) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error("read", path, error)),
        };
        if count == 0 { return Ok(data); }
        if count as u64 > budget.remaining { return Err(byte_limit(path, options.max_bytes())); }
        data.try_reserve(count).map_err(|_| Error::new(ErrorKind::LimitExceeded, "local input allocation limit"))?;
        data.extend_from_slice(&buffer[..count]);
        budget.remaining -= count as u64;
    }
}

fn byte_limit(path: &Path, limit: u64) -> Error {
    Error::new(ErrorKind::LimitExceeded, "local input exceeds byte limit")
        .with_context("domain", "offline").with_context("max_bytes", context_value(limit))
        .with_context("path", format!("{}", path.display()))
}

pub(crate) fn read_records<T>(path: &Path, options: &ReadOptions,
    mut decode: impl FnMut(&[u8; 32], bool) -> Result<T>) -> Result<ReadResult<T>>
{
    let data = load(path, options)?;
    let tail = data.len() % 32;
    if tail != 0 && options.tail() == TailPolicy::Reject {
        return Err(malformed("bar file has a partial trailing record")
            .with_context("record_size", "32").with_context("actual_length", context_value(data.len())));
    }
    let retain = options.raw() != RawPolicy::Omit;
    let mut rows = Vec::new();
    for (index, record) in data.chunks_exact(32).enumerate() {
        cancelled(options.cancel())?;
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(record);
        rows.push(decode(&bytes, retain).map_err(|error| error.with_context("record_index", context_value(index)))?);
    }
    cancelled(options.cancel())?;
    let diagnostics = if tail == 0 { Vec::new() } else {
        vec![diagnostic("tail", format!("ignored {tail} trailing bytes after complete 32-byte records"))]
    };
    Ok(ReadResult::new(rows, tail, diagnostics))
}

pub(crate) fn last_record(path: &Path) -> Result<Option<[u8; 32]>> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("metadata", path, error)),
    };
    if metadata.is_dir() || metadata.len() < 32 { return Ok(None); }
    let length = metadata.len() - metadata.len() % 32;
    let mut handle = File::open(path).map_err(|error| io_error("open", path, error))?;
    handle.seek(SeekFrom::Start(length - 32)).map_err(|error| io_error("seek", path, error))?;
    let mut record = [0_u8; 32];
    handle.read_exact(&mut record).map_err(|error| io_error("read_last", path, error))?;
    Ok(Some(record))
}

pub(crate) fn date_integer(date: TradingDate) -> u32 {
    u32::from(date.year()) * 10000 + u32::from(date.month()) * 100 + u32::from(date.day())
}

pub(crate) fn decode_date(integer: u32) -> Result<TradingDate> {
    let year = u16::try_from(integer / 10000).map_err(|_| malformed("bar year is out of range"))?;
    TradingDate::new(year, ((integer % 10000) / 100) as u8, (integer % 100) as u8)
        .map_err(|error| malformed("bar contains an invalid Gregorian date").with_source(error))
}

/// Python `int(round(value))`: nearest with ties to even, followed by u32 bounds.
pub(crate) fn round_u32(value: f64, field: &str) -> Result<u32> {
    if !value.is_finite() { return Err(invalid(format!("{field} is not finite"))); }
    let rounded = value.round_ties_even();
    if rounded < 0.0 || rounded > u32::MAX as f64 {
        return Err(invalid(format!("{field} does not fit u32")));
    }
    Ok(rounded as u32)
}

pub(crate) fn checked_f32(value: f64) -> Result<u32> {
    let narrowed = value as f32;
    if value.is_finite() && !narrowed.is_finite() { return Err(invalid("price does not fit float32")); }
    Ok(narrowed.to_bits())
}

pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}
pub(crate) fn put_u32(bytes: &mut [u8; 32], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Shared bit-level records before strict Rust date validation or Python's
/// permissive scalar projection. No calendar or price guess is made here.
#[doc(hidden)]
pub mod python_legacy {
    #[derive(Clone, Copy, Debug)]
    pub enum Layout { Daily, MinuteFloat, MinuteInteger, Extended }

    #[derive(Clone, Copy, Debug)]
    pub enum Slot { Date, Time, Price(usize), Amount, Volume, Tail }

    /// One field ordering for the strict encoders and Python Struct adapter.
    /// Numeric conversion remains in the caller's checked/object policy.
    pub fn encode_fields<T, E>(layout: Layout,
        mut value: impl FnMut(Slot) -> std::result::Result<T, E>) -> std::result::Result<Vec<T>, E>
    {
        let mut fields = vec![value(Slot::Date)?];
        if matches!(layout, Layout::MinuteFloat | Layout::MinuteInteger) { fields.push(value(Slot::Time)?); }
        for index in 0..4 { fields.push(value(Slot::Price(index))?); }
        fields.push(value(Slot::Amount)?);
        fields.push(value(Slot::Volume)?);
        fields.push(value(Slot::Tail)?);
        Ok(fields)
    }

    pub fn pack_bits(layout: Layout, fields: &[u32]) -> Option<[u8; 32]> {
        let minute = matches!(layout, Layout::MinuteFloat | Layout::MinuteInteger);
        if fields.len() != if minute { 9 } else { 8 } { return None; }
        let mut result = [0; 32];
        let start = if minute {
            result[..2].copy_from_slice(&u16::try_from(fields[0]).ok()?.to_le_bytes());
            result[2..4].copy_from_slice(&u16::try_from(fields[1]).ok()?.to_le_bytes());
            2
        } else {
            result[..4].copy_from_slice(&fields[0].to_le_bytes());
            1
        };
        for (index, value) in fields[start..].iter().enumerate() {
            super::put_u32(&mut result, 4 + index * 4, *value);
        }
        Some(result)
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Record {
        pub date: u32,
        pub time: u16,
        pub prices: [u32; 4],
        pub amount: u32,
        pub volume: u32,
        pub tail: u32,
        pub raw: [u8; 32],
    }

    pub fn decode(raw: &[u8; 32], layout: Layout) -> Record {
        let minute = matches!(layout, Layout::MinuteFloat | Layout::MinuteInteger);
        Record {
            date: if minute { u32::from(super::u16_at(raw, 0)) } else { super::u32_at(raw, 0) },
            time: if minute { super::u16_at(raw, 2) } else { 0 },
            prices: [4, 8, 12, 16].map(|at| super::u32_at(raw, at)),
            amount: super::u32_at(raw, 20), volume: super::u32_at(raw, 24),
            tail: super::u32_at(raw, 28), raw: *raw,
        }
    }

    pub fn visit<E>(data: &[u8], layout: Layout,
        mut emit: impl FnMut(Record) -> std::result::Result<(), E>,
        boundary: &mut impl FnMut() -> std::result::Result<(), E>) -> std::result::Result<(), E>
    {
        for bytes in data.chunks_exact(32) {
            boundary()?;
            let mut raw = [0; 32]; raw.copy_from_slice(bytes);
            emit(decode(&raw, layout))?;
        }
        boundary()
    }

    pub const fn date_parts(date: u32) -> (u32, u32, u32) {
        (date / 10000, (date % 10000) / 100, date % 100)
    }
    pub const fn packed_date_parts(date: u16) -> (u16, u16, u16) {
        (date / 2048 + 2004, (date % 2048) / 100, (date % 2048) % 100)
    }
    pub const fn packed_time_parts(minutes: u16) -> (u16, u16) { (minutes / 60, minutes % 60) }
}

pub(crate) fn append_records<T>(path: &Path, rows: &[T], options: &AppendOptions,
    daily_sync: bool, mut encode: impl FnMut(&T) -> Result<[u8; 32]>,
    key: impl Fn(&T) -> RecordKey, last: impl FnOnce(&Path) -> Result<Option<RecordKey>>,
) -> Result<WriteReport> {
    let outcome = (|| {
    // Validate even records that will later be filtered out; no file mutation occurs here.
    rows.len().checked_mul(32).filter(|length| *length <= isize::MAX as usize)
        .ok_or_else(|| invalid("encoded batch length overflows"))?;
    let mut encoded = Vec::new();
    encoded.try_reserve_exact(rows.len()).map_err(|_| Error::new(ErrorKind::LimitExceeded, "encoded batch allocation limit"))?;
    for row in rows {
        cancelled(options.cancel())?;
        encoded.push((key(row), encode(row)?));
    }
    let size = NonZeroUsize::new(32).ok_or_else(|| invalid("record size is zero"))?;
    alignment::ensure_aligned(path, size)?;
    let original = last(path)?;
    let skipped = encoded.iter().filter(|(key, _)| original.is_some_and(|last| *key <= last)).count();
    if skipped == encoded.len() {
        cancelled(options.cancel())?;
        return Ok(WriteReport::new(0, skipped, 0, original, Durability::FormatDefault));
    }
    cancelled(options.cancel())?;
    if options.create_parents() {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|error| io_error("create_parents", path, error))?;
        }
    }
    let mut handle = OpenOptions::new().create(true).append(true).open(path)
        .map_err(|error| write_context(io_error("open_append", path, error), 0, 0, skipped))?;
    // Also check the opened file. This detects a pre-open change, but is not a lock.
    let actual_length = handle.metadata().map_err(|error| write_context(io_error("metadata", path, error), 0, 0, skipped))?.len();
    if actual_length % 32 != 0 {
        return Err(write_context(malformed("opened file has a partial record; append refused")
            .with_context("record_size", "32").with_context("actual_length", context_value(actual_length)), 0, 0, skipped));
    }
    write_encoded(&mut handle, encoded.iter().filter(|(key, _)| original.is_none_or(|last| *key > last)),
        options, daily_sync, original, skipped)
        .map_err(|error| error.with_context("path", format!("{}", path.display())))
    })();
    outcome.map_err(|error| {
        if error.context().iter().any(|(key, _)| key == "bytes_written") { error }
        else { write_context(error, 0, 0, 0) }
    })
}

fn write_context(error: Error, bytes: u64, accepted: usize, skipped: usize) -> Error {
    error.with_context("domain", "offline").with_context("operation", "append")
        .with_context("bytes_written", context_value(bytes)).with_context("accepted", context_value(accepted))
        .with_context("skipped", context_value(skipped))
}

/// The writer seam permits deterministic partial-write, flush and sync failure cases.
pub(crate) trait LocalWriter: Write { fn sync_all(&mut self) -> std::io::Result<()>; }
impl LocalWriter for File { fn sync_all(&mut self) -> std::io::Result<()> { File::sync_all(self) } }

pub(crate) fn write_encoded<'a>(handle: &mut impl LocalWriter,
    records: impl Iterator<Item = &'a (RecordKey, [u8; 32])>, options: &AppendOptions,
    daily_sync: bool, original: Option<RecordKey>, skipped: usize,
) -> Result<WriteReport> {
    let mut bytes = 0_u64;
    let mut accepted = 0;
    let mut final_key = original;
    for (key, record) in records {
        cancelled(options.cancel()).map_err(|error| write_context(error, bytes, accepted, skipped))?;
        let mut offset = 0;
        while offset < record.len() {
            match handle.write(&record[offset..]) {
                Ok(0) => return Err(write_context(Error::new(ErrorKind::Io, "local write made no progress")
                    .with_source(std::io::Error::new(std::io::ErrorKind::WriteZero, "zero-length write")), bytes, accepted, skipped)),
                Ok(count) => { offset += count; bytes += count as u64; }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(write_context(Error::new(ErrorKind::Io, "local append failed").with_source(error), bytes, accepted, skipped)),
            }
        }
        accepted += 1;
        final_key = Some(*key);
    }
    // Cancellation can be observed after the last complete record as well.
    cancelled(options.cancel()).map_err(|error| write_context(error, bytes, accepted, skipped))?;
    let achieved = match options.durability() {
        Durability::FormatDefault if daily_sync => Durability::SyncAll,
        requested => requested,
    };
    if matches!(achieved, Durability::Flush | Durability::SyncAll) {
        handle.flush().map_err(|error| write_context(Error::new(ErrorKind::Io, "local flush failed").with_source(error), bytes, accepted, skipped))?;
    }
    if achieved == Durability::SyncAll {
        handle.sync_all().map_err(|error| write_context(Error::new(ErrorKind::Io, "local sync_all failed").with_source(error), bytes, accepted, skipped))?;
    }
    Ok(WriteReport::new(accepted, skipped, bytes, final_key, achieved))
}

#[cfg(test)]
mod authored_writer_cases {
    use super::*;
    use crate::CancellationToken;

    #[derive(Default)]
    struct InjectedWriter {
        output: Vec<u8>, limit: Option<usize>, fail_flush: bool, fail_sync: bool,
        flushes: usize, syncs: usize, cancel_after_record: Option<CancellationToken>,
    }
    impl Write for InjectedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let remaining = self.limit.map(|limit| limit.saturating_sub(self.output.len())).unwrap_or(bytes.len());
            if remaining == 0 { return Err(std::io::Error::other("injected partial write failure")); }
            let count = bytes.len().min(remaining);
            self.output.extend_from_slice(&bytes[..count]);
            if self.output.len() >= 32 {
                if let Some(token) = &self.cancel_after_record { token.cancel(); }
            }
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.fail_flush { Err(std::io::Error::other("injected flush failure")) } else { Ok(()) }
        }
    }
    impl LocalWriter for InjectedWriter {
        fn sync_all(&mut self) -> std::io::Result<()> {
            self.syncs += 1;
            if self.fail_sync { Err(std::io::Error::other("injected sync failure")) } else { Ok(()) }
        }
    }
    fn records() -> Result<[(RecordKey, [u8; 32]); 2]> {
        Ok([(RecordKey::Date(TradingDate::new(2026, 11, 16)?), [0x11; 32]),
            (RecordKey::Date(TradingDate::new(2026, 11, 17)?), [0x22; 32])])
    }

    #[test]
    fn actual_partial_bytes_and_record_boundary_cancel_are_error_context() -> Result<()> {
        let records = records()?;
        let mut writer = InjectedWriter { limit: Some(45), ..Default::default() };
        let error = match write_encoded(&mut writer, records.iter(), &AppendOptions::default(), false, None, 3) {
            Err(error) => error, Ok(_) => return Err(invalid("expected injected write failure")),
        };
        assert_eq!(writer.output.len(), 45);
        assert_eq!(error.kind(), ErrorKind::Io);
        assert!(error.context().iter().any(|(key, value)| key == "bytes_written" && value == "45"));
        assert!(error.context().iter().any(|(key, value)| key == "accepted" && value == "1"));
        let token = CancellationToken::default();
        let mut writer = InjectedWriter { cancel_after_record: Some(token.clone()), ..Default::default() };
        let result = write_encoded(&mut writer, records.iter(), &AppendOptions::default().with_cancel(Some(token)), false, None, 0);
        assert_eq!(result.err().map(|error| error.kind()), Some(ErrorKind::Cancelled));
        assert_eq!(writer.output.len(), 32);
        assert_eq!(writer.flushes, 0);
        Ok(())
    }

    #[test]
    fn durability_grades_and_failure_never_claim_sync_success() -> Result<()> {
        let records = records()?;
        for (daily, request, flushes, syncs, achieved) in [
            (true, Durability::FormatDefault, 1, 1, Durability::SyncAll),
            (false, Durability::FormatDefault, 0, 0, Durability::FormatDefault),
            (false, Durability::Flush, 1, 0, Durability::Flush),
            (false, Durability::SyncAll, 1, 1, Durability::SyncAll),
        ] {
            let mut writer = InjectedWriter::default();
            let report = write_encoded(&mut writer, records[..1].iter(), &AppendOptions::default().with_durability(request), daily, None, 0)?;
            assert_eq!((writer.flushes, writer.syncs, report.durability()), (flushes, syncs, achieved));
        }
        for flush_failure in [false, true] {
            let mut writer = InjectedWriter { fail_flush: flush_failure, fail_sync: !flush_failure, ..Default::default() };
            let error = match write_encoded(&mut writer, records[..1].iter(), &AppendOptions::default().with_durability(Durability::SyncAll), false, None, 0) {
                Err(error) => error, Ok(_) => return Err(invalid("expected injected durability failure")),
            };
            assert_eq!(error.kind(), ErrorKind::Io);
            assert!(error.context().iter().any(|(key, value)| key == "bytes_written" && value == "32"));
            assert_eq!(writer.output.len(), 32);
        }
        Ok(())
    }
}
