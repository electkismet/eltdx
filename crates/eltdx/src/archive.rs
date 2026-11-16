//! Internal, in-memory, bounded classic ZIP reader shared by HQ and financial resources.
//!
//! `zip` is the sole decompressor. This small structural preflight is deliberately
//! performed before constructing it: central-directory allocation must itself be
//! bounded, and local/central/descriptor declarations must agree. ZIP64, split ZIP,
//! self-extracting prefixes, encryption and codecs other than Store/Deflate are not
//! accepted. No member name is ever used as a filesystem path.

use std::collections::BTreeSet;
use std::io::{Cursor, Read};

use crate::error::{Error, ErrorKind, Result};

const MIB: usize = 1024 * 1024;
const BUFFER_BYTES: usize = 16 * 1024;

/// Fixed upper bounds with checked narrowing; callers cannot disable a cap.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ArchiveLimits {
    compressed: usize,
    entries: usize,
    entry_expanded: usize,
    total_expanded: usize,
    selected_expanded: usize,
}

impl ArchiveLimits {
    pub(crate) const fn stats() -> Self {
        Self { compressed: 32 * MIB, entries: 128, entry_expanded: 32 * MIB, total_expanded: 64 * MIB,
            selected_expanded: 32 * MIB }
    }

    pub(crate) const fn financial() -> Self {
        Self { compressed: 64 * MIB, entries: 64, entry_expanded: 256 * MIB, total_expanded: 256 * MIB,
            selected_expanded: 256 * MIB }
    }

    /// Caller limits may only narrow the fixed financial bounds. The selected
    /// DAT bound is independent of the aggregate/all-member expansion bound.
    pub(crate) fn financial_with_limits(compressed: u64, entries: u32, selected: u64) -> Result<Self> {
        let preset = Self::financial();
        if compressed == 0 || compressed > preset.compressed as u64 || entries == 0
            || u64::from(entries) > preset.entries as u64 || selected == 0
            || selected > preset.selected_expanded as u64 {
            return Err(Error::new(ErrorKind::InvalidArgument, "financial archive limits exceed fixed bounds"));
        }
        Ok(Self {
            compressed: usize::try_from(compressed).map_err(|_| limit("compressed address space"))?,
            entries: usize::try_from(entries).map_err(|_| limit("entry address space"))?,
            selected_expanded: usize::try_from(selected).map_err(|_| limit("selected address space"))?,
            ..preset
        })
    }
}

#[derive(Debug)]
pub(crate) struct ArchiveMember {
    /// Exact bytes from the ZIP; selection does not depend on lossy filename decoding.
    pub name: Vec<u8>,
    pub contents: Vec<u8>,
}

/// Borrowed metadata from the exact central/local entry already checked below.
/// Selection happens before any member is inflated or retained.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ArchiveEntryMetadata<'a> {
    pub name: &'a [u8],
    pub index: usize,
    pub declared_size: u64,
    pub is_directory: bool,
}

#[derive(Debug)]
pub(crate) struct SelectedArchiveMember {
    pub member: ArchiveMember,
    pub index: usize,
    pub declared_size: u64,
    pub actual_size: u64,
}

#[derive(Debug)]
struct Entry<'a> {
    name: &'a [u8],
    flags: u16,
    method: u16,
    crc: u32,
    compressed: usize,
    expanded: usize,
    local: usize,
    end: usize,
    is_directory: bool,
}

/// Validate every member, retaining only selected data. Cancellation/deadline is
/// supplied by the operation owner and observed before/after each bounded unit of
/// work; its error is returned unchanged. Drop releases all allocations on error.
pub(crate) fn decode_selected(
    payload: &[u8],
    limits: ArchiveLimits,
    mut select: impl FnMut(&[u8]) -> bool,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<ArchiveMember>> {
    Ok(decode_with_selector(payload, limits, |entries| {
        Ok(entries.iter().filter(|entry| select(entry.name)).map(|entry| entry.index).collect())
    }, boundary)?.into_iter().map(|selected| selected.member).collect())
}

/// Metadata-aware selection reuses the same preflight and inflate loop as HQ
/// statistics. Returned indexes always address the entries validated here;
/// member names are never looked up again through the ZIP library.
pub(crate) fn decode_with_selector(
    payload: &[u8], limits: ArchiveLimits,
    select: impl FnOnce(&[ArchiveEntryMetadata<'_>]) -> Result<Vec<usize>>,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<SelectedArchiveMember>> {
    boundary()?;
    if payload.len() > limits.compressed { return Err(limit("compressed bytes")); }
    if payload.is_empty() { return Err(malformed("empty ZIP")); }
    let entries = preflight(payload, limits, boundary)?;
    let metadata: Vec<_> = entries.iter().enumerate().map(|(index, entry)| ArchiveEntryMetadata {
        name: entry.name, index, declared_size: entry.expanded as u64, is_directory: entry.is_directory,
    }).collect();
    let selected = select(&metadata)?;
    let mut selection = vec![false; entries.len()];
    for index in selected {
        let Some(retain) = selection.get_mut(index) else { return Err(malformed("selector returned invalid ZIP index")); };
        if *retain { return Err(malformed("selector returned duplicate ZIP index")); }
        *retain = true;
        if entries[index].expanded > limits.selected_expanded { return Err(limit("selected member expanded bytes")); }
    }
    boundary()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(payload))
        .map_err(|error| malformed(format!("ZIP decoder rejected headers: {error}")))?;
    if archive.len() != entries.len() { return Err(malformed("ZIP decoder entry count disagrees")); }
    let mut output = Vec::new();
    output.try_reserve_exact(entries.len()).map_err(|_| limit("member allocation"))?;
    let mut total = 0_usize;
    let mut buffer = [0_u8; BUFFER_BYTES];
    for (index, entry) in entries.iter().enumerate() {
        boundary()?;
        let retain = selection[index];
        let mut reader = archive.by_index(index)
            .map_err(|error| malformed(format!("ZIP member cannot be opened: {error}")))?;
        if reader.name_raw() != entry.name || reader.size() != entry.expanded as u64
            || reader.compressed_size() != entry.compressed as u64 || reader.crc32() != entry.crc
        {
            return Err(malformed("ZIP decoder metadata disagrees with preflight"));
        }
        let mut contents = Vec::new();
        if retain { contents.try_reserve_exact(entry.expanded).map_err(|_| limit("expanded allocation"))?; }
        let mut expanded = 0_usize;
        let mut crc = crc32fast::Hasher::new();
        loop {
            boundary()?;
            // One extra byte detects dishonesty even exactly at the declared/cap
            // boundary. It is a fixed stack byte, never appended beyond a limit.
            let room = entry.expanded.saturating_sub(expanded)
                .min(limits.entry_expanded.saturating_sub(expanded))
                .min(if retain { limits.selected_expanded.saturating_sub(expanded) } else { usize::MAX })
                .min(limits.total_expanded.saturating_sub(total));
            let request = BUFFER_BYTES.min(room.saturating_add(1));
            let count = reader.read(&mut buffer[..request])
                .map_err(|error| malformed(format!("ZIP member stream or CRC failed: {error}")))?;
            boundary()?;
            if count == 0 { break; }
            let next = expanded.checked_add(count).ok_or_else(|| limit("expanded bytes"))?;
            let next_total = total.checked_add(count).ok_or_else(|| limit("total expanded bytes"))?;
            if next > limits.entry_expanded || next_total > limits.total_expanded
                || (retain && next > limits.selected_expanded) {
                return Err(limit("actual expanded bytes"));
            }
            if next > entry.expanded { return Err(malformed("member exceeds declared expanded length")); }
            crc.update(&buffer[..count]);
            if retain {
                contents.extend_from_slice(&buffer[..count]);
            }
            expanded = next;
            total = next_total;
        }
        if expanded != entry.expanded || crc.finalize() != entry.crc {
            return Err(malformed("member length or CRC differs from directory"));
        }
        if retain { output.push(SelectedArchiveMember {
            member: ArchiveMember { name: entry.name.to_vec(), contents }, index,
            declared_size: entry.expanded as u64, actual_size: expanded as u64,
        }); }
        boundary()?;
    }
    boundary()?;
    Ok(output)
}

fn preflight<'a>(
    bytes: &'a [u8], limits: ArchiveLimits, boundary: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<Entry<'a>>> {
    if bytes.len() < 22 { return Err(malformed("truncated ZIP end record")); }
    let lower = bytes.len().saturating_sub(22 + usize::from(u16::MAX));
    let mut eocd = None;
    for offset in (lower..=bytes.len() - 22).rev() {
        if offset % BUFFER_BYTES == 0 { boundary()?; }
        if bytes.get(offset..offset + 4) == Some(b"PK\x05\x06".as_slice())
            && offset + 22 + usize::from(u16_at(bytes, offset + 20)?) == bytes.len()
        { eocd = Some(offset); break; }
    }
    let eocd = eocd.ok_or_else(|| malformed("missing complete ZIP end record"))?;
    let count = usize::from(u16_at(bytes, eocd + 10)?);
    if count > limits.entries { return Err(limit("entry count")); }
    if count == usize::from(u16::MAX) || u32_at(bytes, eocd + 12)? == u32::MAX
        || u32_at(bytes, eocd + 16)? == u32::MAX
    { return Err(malformed("ZIP64 is unsupported")); }
    if u16_at(bytes, eocd + 4)? != 0 || u16_at(bytes, eocd + 6)? != 0
        || usize::from(u16_at(bytes, eocd + 8)?) != count
    { return Err(malformed("split ZIP is unsupported")); }
    let central = usize32(bytes, eocd + 16)?;
    let central_size = usize32(bytes, eocd + 12)?;
    if central.checked_add(central_size) != Some(eocd) { return Err(malformed("central directory bounds disagree")); }
    let mut entries = Vec::new();
    entries.try_reserve_exact(count).map_err(|_| limit("directory allocation"))?;
    let mut names = BTreeSet::new();
    let mut offset = central;
    let mut total = 0_usize;
    for _ in 0..count {
        boundary()?;
        let header = slice(bytes, offset, 46)?;
        if &header[..4] != b"PK\x01\x02" { return Err(malformed("invalid central directory signature")); }
        let flags = u16_at(header, 8)?;
        if flags & (1 | 0x40 | 0x2000) != 0 { return Err(malformed("encrypted ZIP member")); }
        if flags & !0x080e != 0 { return Err(malformed("unsupported ZIP flags")); }
        let method = u16_at(header, 10)?;
        if !matches!(method, 0 | 8) { return Err(malformed("unsupported ZIP compression method")); }
        let compressed = usize32(header, 20)?;
        let expanded = usize32(header, 24)?;
        if expanded > limits.entry_expanded { return Err(limit("declared member expanded bytes")); }
        total = total.checked_add(expanded).ok_or_else(|| limit("declared total expanded bytes"))?;
        if total > limits.total_expanded { return Err(limit("declared total expanded bytes")); }
        if u16_at(header, 34)? != 0 || u32_at(header, 42)? == u32::MAX
            || compressed == u32::MAX as usize || expanded == u32::MAX as usize
        { return Err(malformed("split/ZIP64 member is unsupported")); }
        let name_len = usize::from(u16_at(header, 28)?);
        let extra_len = usize::from(u16_at(header, 30)?);
        let comment_len = usize::from(u16_at(header, 32)?);
        let name = slice(bytes, add(offset, 46)?, name_len)?;
        if name.is_empty() || name.contains(&0) { return Err(malformed("empty or NUL ZIP member name")); }
        if !names.insert(name) { return Err(malformed("duplicate ZIP member name")); }
        let extra = slice(bytes, add(add(offset, 46)?, name_len)?, extra_len)?;
        validate_extra(extra)?;
        offset = add(add(add(add(offset, 46)?, name_len)?, extra_len)?, comment_len)?;
        if offset > eocd { return Err(malformed("central member exceeds directory")); }
        let local = usize32(header, 42)?;
        let external_attributes = u32_at(header, 38)?;
        let is_directory = name.ends_with(b"/") || external_attributes & 0x10 != 0
            || (external_attributes >> 16) & 0xf000 == 0x4000;
        entries.push(Entry { name, flags, method, crc: u32_at(header, 16)?, compressed, expanded, local,
            end: 0, is_directory });
    }
    if offset != eocd { return Err(malformed("central directory count/size mismatch")); }
    for entry in &mut entries {
        boundary()?;
        let header = slice(bytes, entry.local, 30)?;
        if &header[..4] != b"PK\x03\x04" || u16_at(header, 6)? != entry.flags
            || u16_at(header, 8)? != entry.method
        { return Err(malformed("local/central headers disagree")); }
        let descriptor = entry.flags & 8 != 0;
        for (at, expected) in [(14, entry.crc), (18, entry.compressed as u32), (22, entry.expanded as u32)] {
            let actual = u32_at(header, at)?;
            if actual != expected && !(descriptor && actual == 0) { return Err(malformed("local/central CRC or length disagreement")); }
        }
        let name_len = usize::from(u16_at(header, 26)?);
        let extra_len = usize::from(u16_at(header, 28)?);
        let name_start = add(entry.local, 30)?;
        if slice(bytes, name_start, name_len)? != entry.name { return Err(malformed("local/central names disagree")); }
        let extra_start = add(name_start, name_len)?;
        validate_extra(slice(bytes, extra_start, extra_len)?)?;
        let data_start = add(extra_start, extra_len)?;
        let mut end = add(data_start, entry.compressed)?;
        if end > central { return Err(malformed("member data exceeds local area")); }
        if descriptor {
            // A signatureless descriptor's CRC can equal the optional signature.
            // Compare all fields before choosing the signed form.
            let unsigned = descriptor_matches(bytes, end, entry)?;
            let signed = bytes.get(end..end.saturating_add(4)) == Some(b"PK\x07\x08".as_slice())
                && descriptor_matches(bytes, add(end, 4)?, entry)?;
            end = if unsigned { add(end, 12)? } else if signed { add(end, 16)? }
                else { return Err(malformed("data descriptor disagrees")); };
        }
        if end > central { return Err(malformed("member descriptor exceeds local area")); }
        if entry.method == 0 && entry.compressed != entry.expanded { return Err(malformed("stored member lengths differ")); }
        entry.end = end;
    }
    let mut physical = Vec::new();
    physical.try_reserve_exact(entries.len()).map_err(|_| limit("directory ordering allocation"))?;
    physical.extend(entries.iter());
    physical.sort_unstable_by_key(|entry| entry.local);
    let mut end = 0;
    for entry in physical {
        if entry.local != end { return Err(malformed("overlapping members, prefix or unlisted local bytes")); }
        end = entry.end;
    }
    if end != central { return Err(malformed("unlisted bytes before central directory")); }
    boundary()?;
    Ok(entries)
}

fn descriptor_matches(bytes: &[u8], offset: usize, entry: &Entry<'_>) -> Result<bool> {
    let Some(record) = bytes.get(offset..add(offset, 12)?) else { return Ok(false); };
    Ok(u32_at(record, 0)? == entry.crc && usize32(record, 4)? == entry.compressed
        && usize32(record, 8)? == entry.expanded)
}

fn validate_extra(mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        let tag = u16_at(bytes, 0)?;
        let len = usize::from(u16_at(bytes, 2)?);
        if matches!(tag, 0x0001 | 0x0017 | 0x9901) { return Err(malformed("ZIP64/encryption extra field unsupported")); }
        bytes = bytes.get(add(4, len)?..).ok_or_else(|| malformed("truncated ZIP extra field"))?;
    }
    Ok(())
}

fn slice(bytes: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    bytes.get(start..add(start, len)?).ok_or_else(|| malformed("truncated ZIP structure"))
}
fn add(a: usize, b: usize) -> Result<usize> { a.checked_add(b).ok_or_else(|| malformed("ZIP offset overflow")) }
fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let value = slice(bytes, at, 2)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let value = slice(bytes, at, 4)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
fn usize32(bytes: &[u8], at: usize) -> Result<usize> {
    usize::try_from(u32_at(bytes, at)?).map_err(|_| malformed("ZIP offset not addressable"))
}
fn malformed(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "archive")
}
fn limit(resource: &'static str) -> Error {
    Error::new(ErrorKind::LimitExceeded, "ZIP resource limit exceeded").with_context("resource", resource)
}
