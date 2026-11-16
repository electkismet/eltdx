//! Source-faithful HQ statistics decoding, before public DTO projection.
//!
//! Dates remain source strings, money remains ten-thousand CNY, share counts
//! remain ten-thousand shares and opening volumes remain hands. T17 Stage B
//! performs checked public units/identity/date projection; T18 alone applies
//! same-main-date and coverage rules. No network, local extraction or clock reads.

use std::collections::HashSet;

use crate::archive::{decode_selected, ArchiveLimits};
use crate::error::{Error, ErrorKind, Result};

pub(crate) const MAX_STATS_ZIP_BYTES: usize = 32 * 1024 * 1024;
const STAT: &[u8] = b"tdxstat.cfg";
const STAT2: &[u8] = b"tdxstat2.cfg";
const CPU_CHUNK: usize = 16 * 1024;
const MAX_DIAGNOSTICS: usize = 128;

/// Exact decimal result of Python int(finite_float), including values outside i64.
/// Negative/oversized values are retained for compatibility, never cast to u32.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceInteger(String);

impl SourceInteger {
    pub(crate) fn decimal(&self) -> &str { &self.0 }
    pub(crate) fn as_u32(&self) -> Option<u32> { self.0.parse().ok() }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct StatsKey {
    pub market_id: SourceInteger,
    pub code: String,
}

#[derive(Debug)]
pub(crate) struct SourceStatRow {
    pub key: StatsKey,
    pub stats_date: Option<String>,
    pub beta_60d: Option<f64>,
    pub pe_ttm: Option<f64>,
    pub free_float_shares_10k: Option<f64>,
    pub year_limit_up_days: Option<SourceInteger>,
    pub limit_stat_days: Option<SourceInteger>,
    pub limit_up_count_in_stat_days: Option<SourceInteger>,
    pub limit_up_streak_days: Option<SourceInteger>,
    pub source_line: usize,
}

#[derive(Debug)]
pub(crate) struct SourceStat2Row {
    pub key: StatsKey,
    pub stats_date: Option<String>,
    pub amount_10k: Option<f64>,
    pub seal_amount_10k: Option<f64>,
    pub prev_amount_10k: Option<f64>,
    pub prev_seal_amount_10k: Option<f64>,
    pub prev2_amount_10k: Option<f64>,
    pub prev2_seal_amount_10k: Option<f64>,
    pub open_volume_hand: Option<f64>,
    pub prev_open_volume_hand: Option<f64>,
    pub open_amount_10k: Option<f64>,
    pub prev_open_amount_10k: Option<f64>,
    pub source_line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatsDiagnosticKind { LossyGbk, SkippedRow, UnknownIdentity }

#[derive(Debug)]
pub(crate) struct StatsDiagnostic {
    pub member: &'static str,
    pub line: Option<usize>,
    pub kind: StatsDiagnosticKind,
}

#[derive(Debug)]
pub(crate) struct ParsedStats {
    pub source_path: String,
    pub stat: Vec<SourceStatRow>,
    pub stat2: Vec<SourceStat2Row>,
    pub diagnostics: Vec<StatsDiagnostic>,
    pub omitted_diagnostics: usize,
}

/// `boundary` must check the same operation deadline/cancellation used by the
/// download. The service must first establish complete input (including M+1
/// oversize detection); this function cannot prove transport EOF for a slice.
///
/// Integration seam: crate::encoding::gbk_scalar(u8,u8)->Option<char> is the
/// shared strict Python-GBK scalar table from T21, independent of `offline`.
pub(crate) fn parse_stats_archive(
    payload: &[u8], source_path: &str, boundary: &mut impl FnMut() -> Result<()>,
) -> Result<ParsedStats> {
    boundary()?;
    // This must precede ZIP construction and all inflation.
    if payload.len() > MAX_STATS_ZIP_BYTES { return Err(limit("compressed statistics bytes")); }
    // Source strings come from bounded service paths. Enforce the same upper
    // bound plus tdx:// here so direct internal calls cannot copy an unlimited path.
    if source_path.len() > 306 { return Err(limit("statistics source path")); }
    let members = decode_selected(payload, ArchiveLimits::stats(), |name| name == STAT || name == STAT2, boundary)?;
    if members.len() != 2 || !members.iter().any(|member| member.name == STAT)
        || !members.iter().any(|member| member.name == STAT2)
    { return Err(malformed("statistics ZIP requires unique tdxstat.cfg and tdxstat2.cfg")); }
    let mut result = ParsedStats {
        source_path: source_path.to_owned(), stat: Vec::new(), stat2: Vec::new(),
        diagnostics: Vec::new(), omitted_diagnostics: 0,
    };
    for member in members {
        boundary()?;
        let is_stat = member.name == STAT;
        let member_name = if is_stat { "tdxstat.cfg" } else { "tdxstat2.cfg" };
        let (text, lossy) = decode_gbk_ignore(&member.contents, boundary)?;
        if lossy { result.diagnostic(member_name, None, StatsDiagnosticKind::LossyGbk); }
        let mut identities = HashSet::new();
        // A usable stat row needs 34 pipes, >=1 market digit and >=1 code
        // byte; stat2 needs 20 pipes plus those two bytes. This natural bound
        // comes only from the admitted source length, not an extra policy cap.
        let natural_rows = member.contents.len() / if is_stat { 36 } else { 22 };
        let mut row_count = 0_usize;
        let mut cursor = 0;
        let mut line_no = 0_usize;
        while let Some(line) = next_line(&text, &mut cursor, boundary)? {
            line_no = line_no.checked_add(1).ok_or_else(|| limit("statistics line count"))?;
            boundary()?;
            // Read only the columns the source parser uses, while checking the
            // minimum width. Never collect arbitrary delimiter counts into a Vec.
            let needed = if is_stat { 35 } else { 21 };
            let (columns, count) = columns(line, needed, boundary)?;
            let key = if count == needed { key(columns[0], columns[1], boundary)? } else { None };
            let Some(key) = key else {
                if !line.is_empty() { result.diagnostic(member_name, Some(line_no), StatsDiagnosticKind::SkippedRow); }
                continue;
            };
            if identities.contains(&key) { return Err(malformed("duplicate statistics security identity")
                .with_context("member", member_name).with_context("line", format!("{line_no}"))); }
            row_count = row_count.checked_add(1).ok_or_else(|| limit("statistics row count"))?;
            if row_count > natural_rows { return Err(malformed("statistics row count exceeds source-byte bound")); }
            if !known_identity(&key) { result.diagnostic(member_name, Some(line_no), StatsDiagnosticKind::UnknownIdentity); }
            identities.try_reserve(1).map_err(|_| limit("statistics identity allocation"))?;
            identities.insert(key.clone());
            if is_stat {
                result.stat.try_reserve(1).map_err(|_| limit("statistics rows allocation"))?;
                result.stat.push(SourceStatRow {
                    key, source_line: line_no, stats_date: text_value(columns[4]),
                    beta_60d: float_value(columns[2], boundary)?, pe_ttm: float_value(columns[3], boundary)?,
                    free_float_shares_10k: float_value(columns[11], boundary)?,
                    year_limit_up_days: int_value(columns[26], boundary)?,
                    limit_stat_days: int_value(columns[31], boundary)?,
                    limit_up_count_in_stat_days: int_value(columns[32], boundary)?,
                    limit_up_streak_days: int_value(columns[33], boundary)?,
                });
            } else {
                result.stat2.try_reserve(1).map_err(|_| limit("statistics rows allocation"))?;
                result.stat2.push(SourceStat2Row {
                    key, source_line: line_no, stats_date: text_value(columns[2]),
                    amount_10k: float_value(columns[3], boundary)?, seal_amount_10k: float_value(columns[4], boundary)?,
                    prev_amount_10k: float_value(columns[5], boundary)?, prev_seal_amount_10k: float_value(columns[6], boundary)?,
                    prev2_amount_10k: float_value(columns[7], boundary)?, prev2_seal_amount_10k: float_value(columns[8], boundary)?,
                    open_volume_hand: float_value(columns[9], boundary)?, prev_open_volume_hand: float_value(columns[10], boundary)?,
                    open_amount_10k: float_value(columns[14], boundary)?, prev_open_amount_10k: float_value(columns[15], boundary)?,
                });
            }
            boundary()?;
        }
    }
    if result.stat.is_empty() || result.stat2.is_empty() { return Err(malformed("statistics table has no usable rows")); }
    boundary()?;
    Ok(result)
}

impl ParsedStats {
    fn diagnostic(&mut self, member: &'static str, line: Option<usize>, kind: StatsDiagnosticKind) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(StatsDiagnostic { member, line, kind });
        } else { self.omitted_diagnostics += 1; }
    }
}

fn key(market: &str, code: &str, boundary: &mut impl FnMut() -> Result<()>) -> Result<Option<StatsKey>> {
    let Some(market_id) = int_value(market, boundary)? else { return Ok(None); };
    let code = source_trim(code);
    if code.is_empty() { return Ok(None); }
    Ok(Some(StatsKey { market_id, code: code.to_owned() }))
}

fn known_identity(key: &StatsKey) -> bool {
    matches!(key.market_id.as_u32(), Some(0 | 1 | 2))
        && key.code.len() == 6 && key.code.bytes().all(|value| value.is_ascii_digit())
}

fn source_trim(value: &str) -> &str {
    value.trim_matches(|value: char| value.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&value))
}

fn next_line<'a>(text: &'a str, cursor: &mut usize, boundary: &mut impl FnMut() -> Result<()>) -> Result<Option<&'a str>> {
    if *cursor >= text.len() { return Ok(None); }
    let start = *cursor;
    let mut next_boundary = 0;
    for (index, scalar) in text[start..].char_indices() {
        if index >= next_boundary { boundary()?; next_boundary = index + CPU_CHUNK; }
        if matches!(scalar, '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            *cursor = start + index + scalar.len_utf8();
            if scalar == '\r' && text.as_bytes().get(*cursor) == Some(&b'\n') { *cursor += 1; }
            return Ok(Some(&text[start..start + index]));
        }
    }
    *cursor = text.len();
    boundary()?;
    Ok(Some(&text[start..]))
}

fn text_value(value: &str) -> Option<String> {
    let text = source_trim(value);
    (!text.is_empty()).then(|| text.to_owned())
}

fn columns<'a>(line: &'a str, needed: usize, boundary: &mut impl FnMut() -> Result<()>) -> Result<([&'a str; 35], usize)> {
    let mut output = [""; 35];
    let mut count = 0;
    let mut start = 0;
    for (index, byte) in line.bytes().enumerate() {
        if index % CPU_CHUNK == 0 { boundary()?; }
        if byte == b'|' {
            output[count] = &line[start..index];
            count += 1;
            if count == needed { return Ok((output, count)); }
            start = index + 1;
        }
    }
    output[count] = &line[start..];
    Ok((output, count + 1))
}

fn decode_gbk_ignore(bytes: &[u8], boundary: &mut impl FnMut() -> Result<()>) -> Result<(String, bool)> {
    let capacity = bytes.len().checked_mul(3).ok_or_else(|| limit("GBK text size"))?;
    let mut output = String::new();
    output.try_reserve_exact(capacity).map_err(|_| limit("GBK text allocation"))?;
    let mut offset = 0;
    let mut next_boundary = 0;
    let mut lossy = false;
    while offset < bytes.len() {
        if offset >= next_boundary { boundary()?; next_boundary = offset + CPU_CHUNK; }
        let lead = bytes[offset];
        if lead < 0x80 { output.push(char::from(lead)); offset += 1; continue; }
        if let Some(trail) = bytes.get(offset + 1).copied() {
            if let Some(scalar) = crate::encoding::gbk_scalar(lead, trail) {
                output.push(scalar); offset += 2; continue;
            }
        }
        // Python GBK errors='ignore' consumes only the invalid lead, then retries
        // the following byte. In particular standalone 0x80 is not a euro sign.
        lossy = true;
        offset += 1;
    }
    boundary()?;
    Ok((output, lossy))
}

fn float_value(value: &str, boundary: &mut impl FnMut() -> Result<()>) -> Result<Option<f64>> {
    let value = source_trim(value);
    if value.is_empty() { return Ok(None); }
    let mut normalized = String::new();
    normalized.try_reserve_exact(value.len()).map_err(|_| limit("numeric text allocation"))?;
    let mut next_boundary = 0;
    for (index, scalar) in value.char_indices() {
        if index >= next_boundary { boundary()?; next_boundary = index + CPU_CHUNK; }
        if let Some(digit) = decimal_digit(scalar) { normalized.push(char::from(b'0' + digit)); }
        else if scalar.is_ascii() { normalized.push(scalar); }
        else { return Ok(None); }
    }
    // Python permits underscores strictly between digits, including exponent.
    let bytes = normalized.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if index % CPU_CHUNK == 0 { boundary()?; }
        if *byte == b'_' && (index == 0 || index + 1 == bytes.len()
            || !bytes[index - 1].is_ascii_digit() || !bytes[index + 1].is_ascii_digit()) { return Ok(None); }
    }
    let mut cleaned = String::new();
    cleaned.try_reserve_exact(normalized.len()).map_err(|_| limit("numeric text allocation"))?;
    for chunk in bytes.chunks(CPU_CHUNK) {
        boundary()?;
        for byte in chunk { if *byte != b'_' { cleaned.push(char::from(*byte)); } }
    }
    let parsed = cleaned.parse::<f64>().ok().filter(|number| number.is_finite());
    boundary()?;
    Ok(parsed)
}

fn int_value(value: &str, boundary: &mut impl FnMut() -> Result<()>) -> Result<Option<SourceInteger>> {
    let Some(number) = float_value(value, boundary)? else { return Ok(None); };
    let number = number.trunc();
    if number == 0.0 { return Ok(Some(SourceInteger("0".to_owned()))); }
    // Convert the integral binary64 exactly instead of saturating an i64 cast.
    let bits = number.abs().to_bits();
    let shift = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52);
    let integer = if shift < 0 { mantissa >> (-shift as u32) } else { mantissa };
    let mut digits: Vec<u8> = format!("{integer}").bytes().rev().map(|digit| digit - b'0').collect();
    for _ in 0..shift.max(0) {
        let mut carry = 0;
        for digit in &mut digits { let value = *digit * 2 + carry; *digit = value % 10; carry = value / 10; }
        if carry != 0 { digits.push(carry); }
    }
    let mut decimal = String::with_capacity(digits.len() + 1);
    if number.is_sign_negative() { decimal.push('-'); }
    for digit in digits.iter().rev() { decimal.push(char::from(b'0' + *digit)); }
    Ok(Some(SourceInteger(decimal)))
}

fn decimal_digit(value: char) -> Option<u8> {
    // Unicode Nd blocks accepted by Python float, including non-ASCII digits.
    // Each start identifies ten consecutive codepoints; mathematical digits have
    // five such alphabets. No other Unicode numeric category is accepted.
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6,
        0xc66, 0xce6, 0xd66, 0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0,
        0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50,
        0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10, 0x104a0,
        0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0,
        0x16a60, 0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6,
        0x1e140, 0x1e2f0, 0x1e950, 0x1fbf0,
    ];
    let value = value as u32;
    ZEROES.iter().find_map(|start| value.checked_sub(*start).filter(|digit| *digit < 10).map(|digit| digit as u8))
}

fn malformed(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "stats")
}
fn limit(resource: &'static str) -> Error {
    Error::new(ErrorKind::LimitExceeded, "statistics resource limit exceeded").with_context("resource", resource)
}
