//! Historical financial DAT/list parsing and bounded local DAT/ZIP reads.
//! ZIP metadata selection and inflation reuse the shared HQ archive decoder.
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use super::{
    ascii_replace, bars, cancelled, invalid, malformed, source_trim, ReadOptions, ReadResult,
    WireFloat,
};
use crate::error::{Error, ErrorKind, Result};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use crate::{CancellationToken, RawPolicy, TradingDate};

pub const FINANCIAL_LIST_PATH: &str = "tdxfin/gpcw.txt";
pub const MAX_DAT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: u32 = 64;
pub const MAX_FIELDS_PER_RECORD: u32 = 4096;
pub const MAX_TOTAL_FIELDS: u64 = 4 * 1024 * 1024;
const HEADER_BYTES: usize = 20;
const INDEX_BYTES: usize = 11;

/// Checked remote leaf; it never denotes a local filesystem destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinancialArchiveName(String);
impl FinancialArchiveName {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        // HQ permits 300 ASCII path bytes; reserve the fixed `tdxfin/` prefix.
        if value.is_empty()
            || value.len() > 293
            || !value.is_ascii()
            || value.trim() != value
            || value.chars().any(|c| c.is_control())
            || value.contains(['/', '\\', ':'])
            || value == "."
            || value == ".."
        {
            return Err(invalid(
                "financial archive name must be a bounded ASCII remote leaf",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn remote_path(&self) -> String {
        format!("tdxfin/{}", self.0)
    }
}

#[derive(Clone, Debug)]
pub struct FinancialParseOptions {
    max_dat_bytes: u64,
    max_fields_per_record: u32,
    max_total_fields: u64,
    strict_index: bool,
    raw: RawPolicy,
    cancel: Option<CancellationToken>,
}
impl Default for FinancialParseOptions {
    fn default() -> Self {
        Self {
            max_dat_bytes: MAX_DAT_BYTES,
            max_fields_per_record: MAX_FIELDS_PER_RECORD,
            max_total_fields: MAX_TOTAL_FIELDS,
            strict_index: false,
            raw: RawPolicy::Omit,
            cancel: None,
        }
    }
}
impl FinancialParseOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn max_dat_bytes(&self) -> u64 {
        self.max_dat_bytes
    }
    pub const fn max_fields_per_record(&self) -> u32 {
        self.max_fields_per_record
    }
    pub const fn max_total_fields(&self) -> u64 {
        self.max_total_fields
    }
    pub const fn strict_index(&self) -> bool {
        self.strict_index
    }
    pub const fn raw(&self) -> RawPolicy {
        self.raw
    }
    pub fn cancel(&self) -> Option<&CancellationToken> {
        self.cancel.as_ref()
    }
    pub fn with_max_dat_bytes(mut self, value: u64) -> Result<Self> {
        bounded(value, MAX_DAT_BYTES, "max_dat_bytes")?;
        self.max_dat_bytes = value;
        Ok(self)
    }
    pub fn with_max_fields_per_record(mut self, value: u32) -> Result<Self> {
        bounded(
            u64::from(value),
            u64::from(MAX_FIELDS_PER_RECORD),
            "max_fields_per_record",
        )?;
        self.max_fields_per_record = value;
        Ok(self)
    }
    pub fn with_max_total_fields(mut self, value: u64) -> Result<Self> {
        bounded(value, MAX_TOTAL_FIELDS, "max_total_fields")?;
        value
            .checked_mul(4)
            .ok_or_else(|| invalid("total field byte size overflows"))?;
        self.max_total_fields = value;
        Ok(self)
    }
    pub fn with_strict_index(mut self, value: bool) -> Self {
        self.strict_index = value;
        self
    }
    pub fn with_raw(mut self, value: RawPolicy) -> Self {
        self.raw = value;
        self
    }
    pub fn with_cancel(mut self, value: Option<CancellationToken>) -> Self {
        self.cancel = value;
        self
    }
}
fn bounded(value: u64, maximum: u64, name: &str) -> Result<()> {
    if value == 0 || value > maximum || value > (isize::MAX as u64).saturating_sub(1) {
        return Err(invalid(format!(
            "{name} must be positive and no greater than {maximum}"
        )));
    }
    Ok(())
}

/// Exact archive member name; checked construction is available through `named`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum DatSelector {
    #[default]
    RequireSingle,
    Named(String),
}
impl DatSelector {
    pub fn named(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.contains('\0') || !name.ends_with(".dat") {
            return Err(invalid(
                "DAT selector must be a nonempty, NUL-free exact .dat member name",
            ));
        }
        Ok(Self::Named(name))
    }
    fn validate(&self) -> Result<()> {
        if let Self::Named(name) = self {
            Self::named(name.clone())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FinancialReadOptions {
    parse: FinancialParseOptions,
    max_archive_bytes: u64,
    max_entries: u32,
    selector: DatSelector,
    report_date: Option<TradingDate>,
}
impl Default for FinancialReadOptions {
    fn default() -> Self {
        Self {
            parse: FinancialParseOptions::default(),
            max_archive_bytes: MAX_ARCHIVE_BYTES,
            max_entries: MAX_ARCHIVE_ENTRIES,
            selector: DatSelector::RequireSingle,
            report_date: None,
        }
    }
}
impl FinancialReadOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn parse(&self) -> &FinancialParseOptions {
        &self.parse
    }
    pub const fn max_archive_bytes(&self) -> u64 {
        self.max_archive_bytes
    }
    pub const fn max_entries(&self) -> u32 {
        self.max_entries
    }
    pub fn selector(&self) -> &DatSelector {
        &self.selector
    }
    pub const fn report_date(&self) -> Option<TradingDate> {
        self.report_date
    }
    pub fn with_parse(mut self, value: FinancialParseOptions) -> Self {
        self.parse = value;
        self
    }
    pub fn with_max_archive_bytes(mut self, value: u64) -> Result<Self> {
        bounded(value, MAX_ARCHIVE_BYTES, "max_archive_bytes")?;
        self.max_archive_bytes = value;
        Ok(self)
    }
    pub fn with_max_entries(mut self, value: u32) -> Result<Self> {
        bounded(
            u64::from(value),
            u64::from(MAX_ARCHIVE_ENTRIES),
            "max_entries",
        )?;
        self.max_entries = value;
        Ok(self)
    }
    pub fn with_selector(mut self, value: DatSelector) -> Result<Self> {
        value.validate()?;
        self.selector = value;
        Ok(self)
    }
    pub fn with_report_date(mut self, value: Option<TradingDate>) -> Self {
        self.report_date = value;
        self
    }
}

/// Pure checked query consumed by the Stage B HQ composition.
#[derive(Clone, Debug)]
pub struct FinancialRecordsQuery {
    filename: FinancialArchiveName,
    selector: DatSelector,
    report_date: Option<TradingDate>,
}
impl FinancialRecordsQuery {
    pub fn new(filename: FinancialArchiveName) -> Self {
        Self {
            filename,
            selector: DatSelector::RequireSingle,
            report_date: None,
        }
    }
    pub fn filename(&self) -> &FinancialArchiveName {
        &self.filename
    }
    pub fn selector(&self) -> &DatSelector {
        &self.selector
    }
    pub const fn report_date(&self) -> Option<TradingDate> {
        self.report_date
    }
    pub fn with_selector(mut self, value: DatSelector) -> Result<Self> {
        value.validate()?;
        self.selector = value;
        Ok(self)
    }
    pub fn with_report_date(mut self, value: Option<TradingDate>) -> Self {
        self.report_date = value;
        self
    }
}

/// Filled only by the shared archive adapter after it reads the validated index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveProvenance {
    entry_name: Vec<u8>,
    entry_index: usize,
    declared_size: u64,
    actual_size: u64,
    selector: DatSelector,
}
impl ArchiveProvenance {
    pub(crate) fn new(
        entry_name: Vec<u8>,
        entry_index: usize,
        declared_size: u64,
        actual_size: u64,
        selector: DatSelector,
    ) -> Result<Self> {
        selector.validate()?;
        if declared_size != actual_size || actual_size > MAX_DAT_BYTES {
            return Err(malformed(
                "archive DAT declared/actual size disagrees or exceeds limit",
            ));
        }
        Ok(Self {
            entry_name,
            entry_index,
            declared_size,
            actual_size,
            selector,
        })
    }
    pub fn entry_name(&self) -> &[u8] {
        &self.entry_name
    }
    pub const fn entry_index(&self) -> usize {
        self.entry_index
    }
    pub const fn declared_size(&self) -> u64 {
        self.declared_size
    }
    pub const fn actual_size(&self) -> u64 {
        self.actual_size
    }
    pub fn selector(&self) -> &DatSelector {
        &self.selector
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinancialHeader {
    unknown: i16,
    report_date_raw: u32,
    max_count: u16,
    unknown1: u32,
    report_size: u32,
    unknown2: u32,
}
impl FinancialHeader {
    pub const fn unknown(&self) -> i16 {
        self.unknown
    }
    pub const fn report_date_raw(&self) -> u32 {
        self.report_date_raw
    }
    pub const fn max_count(&self) -> u16 {
        self.max_count
    }
    pub const fn unknown1(&self) -> u32 {
        self.unknown1
    }
    pub const fn report_size(&self) -> u32 {
        self.report_size
    }
    pub const fn unknown2(&self) -> u32 {
        self.unknown2
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinancialCompletion {
    Complete,
    Incomplete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportDateSource {
    Explicit,
    ArchiveFilename,
    Header,
    Unavailable,
    PythonLegacyOverride,
}
#[derive(Clone, Debug)]
pub struct FinancialRecord {
    code: String,
    market_id: u8,
    report_date: Option<TradingDate>,
    report_date_raw: u32,
    date_source: ReportDateSource,
    fields: Vec<WireFloat>,
    header: FinancialHeader,
    index: usize,
    data_range: Range<usize>,
    raw: Option<Arc<[u8]>>,
}
impl FinancialRecord {
    pub fn code(&self) -> &str {
        &self.code
    }
    pub const fn market_id(&self) -> u8 {
        self.market_id
    }
    pub const fn report_date(&self) -> Option<TradingDate> {
        self.report_date
    }
    /// Effective unvalidated integer, including the legacy override when selected.
    pub const fn report_date_raw(&self) -> u32 {
        self.report_date_raw
    }
    pub const fn date_source(&self) -> ReportDateSource {
        self.date_source
    }
    pub fn fields(&self) -> &[WireFloat] {
        &self.fields
    }
    pub const fn header(&self) -> &FinancialHeader {
        &self.header
    }
    pub const fn index(&self) -> usize {
        self.index
    }
    pub fn data_range(&self) -> Range<usize> {
        self.data_range.clone()
    }
    pub fn raw(&self) -> Option<&[u8]> {
        self.raw
            .as_deref()
            .map(|data| &data[self.data_range.clone()])
    }
}
/// Rich result retains facts that the shared T21 `ReadResult` cannot represent.
#[derive(Clone, Debug)]
pub struct FinancialDatResult {
    records: ReadResult<FinancialRecord>,
    header: Option<FinancialHeader>,
    completion: FinancialCompletion,
    consumed_indexes: usize,
    omitted_indexes: usize,
    date_source: ReportDateSource,
    raw: Option<Arc<[u8]>>,
    archive: Option<ArchiveProvenance>,
    archive_raw: Option<Arc<[u8]>>,
    source_meta: Option<ResultMeta>,
}
impl FinancialDatResult {
    pub fn records(&self) -> &ReadResult<FinancialRecord> {
        &self.records
    }
    pub fn rows(&self) -> &[FinancialRecord] {
        self.records.rows()
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.records.diagnostics()
    }
    pub const fn header(&self) -> Option<&FinancialHeader> {
        self.header.as_ref()
    }
    pub const fn completion(&self) -> FinancialCompletion {
        self.completion
    }
    pub const fn consumed_indexes(&self) -> usize {
        self.consumed_indexes
    }
    pub const fn omitted_indexes(&self) -> usize {
        self.omitted_indexes
    }
    pub const fn date_source(&self) -> ReportDateSource {
        self.date_source
    }
    pub fn raw(&self) -> Option<&[u8]> {
        self.raw.as_deref()
    }
    pub fn archive(&self) -> Option<&ArchiveProvenance> {
        self.archive.as_ref()
    }
    /// Original compressed ZIP bytes, when retained; `raw()` is the selected DAT.
    pub fn archive_raw(&self) -> Option<&[u8]> {
        self.archive_raw.as_deref()
    }
    /// Actual HQ source/raw metadata for a network composition, absent locally.
    pub fn source_meta(&self) -> Option<&ResultMeta> {
        self.source_meta.as_ref()
    }
    pub fn into_records(self) -> ReadResult<FinancialRecord> {
        self.records
    }
    pub(crate) fn with_archive(mut self, value: ArchiveProvenance) -> Self {
        self.archive = Some(value);
        self
    }
    pub(crate) fn with_source_meta(mut self, value: ResultMeta) -> Self {
        self.source_meta = Some(value);
        self
    }
}

pub fn parse_dat(
    data: &[u8],
    report_date: Option<TradingDate>,
    options: &FinancialParseOptions,
) -> Result<ReadResult<FinancialRecord>> {
    Ok(parse_dat_detailed(data, report_date, options)?.into_records())
}

pub fn parse_dat_detailed(
    data: &[u8],
    report_date: Option<TradingDate>,
    options: &FinancialParseOptions,
) -> Result<FinancialDatResult> {
    parse_core(
        data,
        DatePolicy::Rust {
            explicit: report_date,
            filename: None,
        },
        options,
        false,
    )
}

/// Pure date-precedence entry point for Stage B after shared ZIP selection.
pub fn parse_dat_for_archive(
    data: &[u8],
    filename: &str,
    report_date: Option<TradingDate>,
    options: &FinancialParseOptions,
) -> Result<FinancialDatResult> {
    parse_core(
        data,
        DatePolicy::Rust {
            explicit: report_date,
            filename: Some(filename),
        },
        options,
        false,
    )
}

/// Stage B supplies its existing operation checkpoint, preserving its error and
/// total deadline at each bounded CPU unit without constructing a new operation.
#[cfg(test)]
pub(crate) fn parse_dat_for_archive_with_boundary(
    data: &[u8],
    filename: &str,
    report_date: Option<TradingDate>,
    options: &FinancialParseOptions,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<FinancialDatResult> {
    parse_core_with_boundary(
        data,
        DatePolicy::Rust {
            explicit: report_date,
            filename: Some(filename),
        },
        options,
        false,
        boundary,
    )
}

/// Direct local DAT reading through the shared bounded file loader.
/// Filename dates are intentionally ignored on this local route.
pub fn read_dat_file(path: &Path, options: &FinancialReadOptions) -> Result<FinancialDatResult> {
    let local = ReadOptions::new()
        .with_max_bytes(options.parse.max_dat_bytes)?
        .with_cancel(options.parse.cancel.clone());
    let bytes = bars::load(path, &local)?;
    parse_dat_detailed(&bytes, options.report_date, &options.parse)
}

/// Read a local DAT or ZIP. Extension matching is case-insensitive; member
/// selection remains case-sensitive. Local filenames never override DAT dates.
pub fn read(path: &Path, options: &FinancialReadOptions) -> Result<ReadResult<FinancialRecord>> {
    Ok(read_detailed(path, options)?.into_records())
}

pub fn read_detailed(path: &Path, options: &FinancialReadOptions) -> Result<FinancialDatResult> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
    {
        let local = ReadOptions::new()
            .with_max_bytes(options.max_archive_bytes)?
            .with_cancel(options.parse.cancel.clone());
        let bytes = bars::load(path, &local)?;
        parse_zip_detailed(&bytes, options)
    } else {
        read_dat_file(path, options)
    }
}

/// Parse a local archive using header/explicit dates, without inferring a date
/// from the selected member name. Download compositions supply the remote name.
pub fn parse_zip(
    data: &[u8],
    options: &FinancialReadOptions,
) -> Result<ReadResult<FinancialRecord>> {
    Ok(parse_zip_detailed(data, options)?.into_records())
}

pub fn parse_zip_detailed(
    data: &[u8],
    options: &FinancialReadOptions,
) -> Result<FinancialDatResult> {
    parse_zip_with_boundary(data, None, options, &mut || Ok(()))
}

pub(crate) fn parse_zip_with_boundary(
    data: &[u8],
    filename: Option<&str>,
    options: &FinancialReadOptions,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<FinancialDatResult> {
    checkpoint(&options.parse, boundary)?;
    options.selector.validate()?;
    let limits = crate::archive::ArchiveLimits::financial_with_limits(
        options.max_archive_bytes,
        options.max_entries,
        options.parse.max_dat_bytes,
    )?;
    let mut selected = crate::archive::decode_with_selector(
        data,
        limits,
        |entries| {
            let mut policy = StrictDatSelection {
                selector: &options.selector,
            };
            let entry = python_legacy::select_dat_entry(
                entries.iter().copied().map(Ok),
                entries.len(),
                &mut policy,
            )?;
            Ok(vec![entry.index])
        },
        &mut || checkpoint(&options.parse, boundary),
    )?;
    let selected = selected.pop().ok_or_else(|| {
        Error::new(
            ErrorKind::Internal,
            "financial ZIP selection produced no member",
        )
    })?;
    let provenance = ArchiveProvenance::new(
        selected.member.name,
        selected.index,
        selected.declared_size,
        selected.actual_size,
        options.selector.clone(),
    )?;
    let mut result = parse_core_with_boundary(
        &selected.member.contents,
        DatePolicy::Rust {
            explicit: options.report_date,
            filename,
        },
        &options.parse,
        false,
        boundary,
    )?
    .with_archive(provenance);
    result.archive_raw = retain_payload(data, &options.parse, boundary)?;
    checkpoint(&options.parse, boundary)?;
    Ok(result)
}

struct StrictDatSelection<'a> {
    selector: &'a DatSelector,
}
impl<'a> python_legacy::DatArchiveSelection<crate::archive::ArchiveEntryMetadata<'a>>
    for StrictDatSelection<'_>
{
    type Error = Error;
    fn check_count(&mut self, _: usize) -> Result<()> {
        Ok(())
    }
    fn is_candidate(&mut self, entry: &crate::archive::ArchiveEntryMetadata<'a>) -> Result<bool> {
        Ok(!entry.is_directory && entry.name.ends_with(b".dat"))
    }
    fn check_candidates(&mut self, _: &[crate::archive::ArchiveEntryMetadata<'a>]) -> Result<()> {
        Ok(())
    }
    fn choose(&mut self, entries: &[crate::archive::ArchiveEntryMetadata<'a>]) -> Result<usize> {
        match self.selector {
            DatSelector::RequireSingle => match entries.len() {
                0 => Err(Error::new(
                    ErrorKind::MissingData,
                    "financial archive contains no .dat member",
                )),
                1 => Ok(0),
                _ => Err(malformed(
                    "financial archive contains multiple .dat members; select an exact name",
                )
                .with_context(
                    "candidates",
                    entries
                        .iter()
                        .map(|entry| String::from_utf8_lossy(entry.name).into_owned())
                        .collect::<Vec<_>>()
                        .join(", "),
                )),
            },
            DatSelector::Named(name) => entries
                .iter()
                .position(|entry| entry.name == name.as_bytes())
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::MissingData,
                        "selected financial DAT member is absent",
                    )
                    .with_context("member", name.clone())
                }),
        }
    }
    fn check_selected(&mut self, _: &crate::archive::ArchiveEntryMetadata<'a>) -> Result<()> {
        Ok(())
    }
    fn invalid_selection(&mut self) -> Error {
        Error::new(ErrorKind::Internal, "invalid financial archive selection")
    }
}

#[derive(Clone, Copy)]
enum DatePolicy<'a> {
    Rust {
        explicit: Option<TradingDate>,
        filename: Option<&'a str>,
    },
    Legacy(u32),
}
fn date_parts(raw: u32) -> Option<TradingDate> {
    let year = u16::try_from(raw / 10000).ok()?;
    TradingDate::new(year, ((raw % 10000) / 100) as u8, (raw % 100) as u8).ok()
}
fn date_integer(value: TradingDate) -> u32 {
    u32::from(value.year()) * 10000 + u32::from(value.month()) * 100 + u32::from(value.day())
}
fn decimal_digit(value: char) -> Option<u8> {
    // Frozen Unicode 15.0.0 Nd zeroes; Python's bridge keeps runtime `re` semantics.
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60,
        0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0,
        0x1e950, 0x1fbf0,
    ];
    let value = value as u32;
    ZEROES.iter().find_map(|start| {
        value
            .checked_sub(*start)
            .filter(|digit| *digit < 10)
            .map(|digit| digit as u8)
    })
}
/// Dates are found in the leaf name, in non-overlapping eight-digit runs, matching
/// the legacy first-match shape using Unicode 15.0.0 decimal digits, while Rust
/// additionally requires a valid date.
pub fn filename_report_date(filename: &str) -> Option<TradingDate> {
    filename_dates(filename).find_map(date_parts)
}
fn filename_dates(filename: &str) -> impl Iterator<Item = u32> + '_ {
    let leaf = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let mut characters = leaf.chars();
    std::iter::from_fn(move || {
        let mut count = 0;
        let mut value = 0_u32;
        for character in characters.by_ref() {
            if let Some(digit) = decimal_digit(character) {
                count += 1;
                value = value * 10 + u32::from(digit);
                if count == 8 {
                    return Some(value);
                }
            } else {
                count = 0;
                value = 0;
            }
        }
        None
    })
}
fn resolve_date(
    policy: DatePolicy<'_>,
    header: u32,
) -> (u32, Option<TradingDate>, ReportDateSource) {
    match policy {
        DatePolicy::Legacy(value) if value != 0 => (
            value,
            date_parts(value),
            ReportDateSource::PythonLegacyOverride,
        ),
        DatePolicy::Rust {
            explicit: Some(date),
            ..
        } => (date_integer(date), Some(date), ReportDateSource::Explicit),
        DatePolicy::Rust {
            filename: Some(name),
            ..
        } => {
            if let Some(date) = filename_report_date(name) {
                return (
                    date_integer(date),
                    Some(date),
                    ReportDateSource::ArchiveFilename,
                );
            }
            (
                header,
                date_parts(header),
                if date_parts(header).is_some() {
                    ReportDateSource::Header
                } else {
                    ReportDateSource::Unavailable
                },
            )
        }
        _ => (
            header,
            date_parts(header),
            if date_parts(header).is_some() {
                ReportDateSource::Header
            } else {
                ReportDateSource::Unavailable
            },
        ),
    }
}
fn issue(stage: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "offline.financial",
        stage,
        EnrichmentStatus::Unavailable,
        Some(ErrorKind::ResourceFormat),
        Some(message.into()),
    )
}
fn index_issue(
    options: &FinancialParseOptions,
    diagnostics: &mut Vec<Diagnostic>,
    index: usize,
    message: &str,
) -> Result<()> {
    if options.strict_index {
        return Err(malformed(message).with_context("record_index", format!("{index}")));
    }
    diagnostics.push(issue("index", format!("record {index}: {message}")));
    Ok(())
}

fn parse_core(
    data: &[u8],
    policy: DatePolicy<'_>,
    options: &FinancialParseOptions,
    legacy: bool,
) -> Result<FinancialDatResult> {
    parse_core_with_boundary(data, policy, options, legacy, &mut || Ok(()))
}
fn checkpoint(
    options: &FinancialParseOptions,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    cancelled(options.cancel())?;
    boundary()
}
fn parse_core_with_boundary(
    data: &[u8],
    policy: DatePolicy<'_>,
    options: &FinancialParseOptions,
    legacy: bool,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<FinancialDatResult> {
    parse_core_with_adapter(data, policy, options, legacy, boundary, None)
}

fn dat_checkpoint(
    options: &FinancialParseOptions,
    boundary: &mut impl FnMut() -> Result<()>,
    adapter: &mut Option<&mut dyn python_legacy::DatCallbacks>,
) -> Result<()> {
    checkpoint(options, boundary)?;
    if let Some(adapter) = adapter.as_deref_mut() {
        adapter.checkpoint()?;
    }
    Ok(())
}

/// One traversal and float decoder for owned SDK results and the streaming
/// Python projection. The latter supplies dynamic Python limits and retains
/// arbitrary report-date objects without narrowing them to an SDK integer.
fn parse_core_with_adapter(
    data: &[u8],
    policy: DatePolicy<'_>,
    options: &FinancialParseOptions,
    legacy: bool,
    boundary: &mut impl FnMut() -> Result<()>,
    mut adapter: Option<&mut dyn python_legacy::DatCallbacks>,
) -> Result<FinancialDatResult> {
    dat_checkpoint(options, boundary, &mut adapter)?;
    if !legacy && data.len() as u64 > options.max_dat_bytes {
        return Err(Error::new(
            ErrorKind::LimitExceeded,
            "financial DAT exceeds byte limit",
        ));
    }
    let mut diagnostics = Vec::new();
    let mut rows = Vec::new();
    let mut completion = FinancialCompletion::Complete;
    if data.len() < HEADER_BYTES {
        if !data.is_empty() {
            index_issue(
                options,
                &mut diagnostics,
                0,
                "truncated financial DAT header",
            )?;
            completion = FinancialCompletion::Incomplete;
        }
        return Ok(FinancialDatResult {
            records: ReadResult::new(rows, data.len(), diagnostics),
            header: None,
            completion,
            consumed_indexes: 0,
            omitted_indexes: 0,
            date_source: ReportDateSource::Unavailable,
            raw: retain_payload(data, options, boundary)?,
            archive: None,
            archive_raw: None,
            source_meta: None,
        });
    }
    let header = FinancialHeader {
        unknown: i16::from_le_bytes([data[0], data[1]]),
        report_date_raw: bars::u32_at(data, 2),
        max_count: bars::u16_at(data, 6),
        unknown1: bars::u32_at(data, 8),
        report_size: bars::u32_at(data, 12),
        unknown2: bars::u32_at(data, 16),
    };
    if let Some(adapter) = adapter.as_deref_mut() {
        adapter.header(&header)?;
    }
    let (date_raw, date, date_source) = resolve_date(policy, header.report_date_raw);
    if date_parts(header.report_date_raw).is_none() {
        diagnostics.push(issue(
            "date",
            format!(
                "invalid raw header date {} retained",
                header.report_date_raw
            ),
        ));
    }
    if date.is_none() && date_raw != header.report_date_raw {
        diagnostics.push(issue(
            "date",
            format!("invalid legacy override date {date_raw} retained"),
        ));
    }
    let fields = header.report_size / 4;
    if header.report_size != 0 {
        if let Some(adapter) = adapter.as_deref_mut() {
            adapter.field_count(&header, fields)?;
        }
    }
    if !header.report_size.is_multiple_of(4)
        || (adapter.is_none() && fields > options.max_fields_per_record)
    {
        return Err(malformed(
            "financial report_size is unaligned or exceeds field limit",
        ));
    }
    let count = usize::from(header.max_count);
    let index_end = count
        .checked_mul(INDEX_BYTES)
        .and_then(|length| HEADER_BYTES.checked_add(length))
        .ok_or_else(|| malformed("financial index size overflows"))?;
    let mut consumed = 0;
    let mut omitted = 0;
    let mut total_fields = 0_u64;
    let mut ranges = BTreeMap::<usize, usize>::new();
    if header.report_size != 0 {
        for index in 0..count {
            dat_checkpoint(options, boundary, &mut adapter)?;
            let at = HEADER_BYTES + index * INDEX_BYTES; // bounded by checked index_end
            let Some(entry) = data.get(at..at + INDEX_BYTES) else {
                index_issue(
                    options,
                    &mut diagnostics,
                    index,
                    "truncated financial index table",
                )?;
                omitted += count - index;
                completion = FinancialCompletion::Incomplete;
                break;
            };
            consumed += 1;
            let code = ascii_replace(&entry[..6]).trim_end_matches('\0').to_owned();
            let start = bars::u32_at(entry, 7) as usize;
            if code.is_empty() || start == 0 {
                diagnostics.push(issue(
                    "index",
                    format!("record {index}: empty code or zero offset omitted"),
                ));
                omitted += 1;
                completion = FinancialCompletion::Incomplete;
                continue;
            }
            // Python already rejects a repeated previously accepted offset. New
            // Rust additionally rejects any overlap with metadata or other rows.
            if ranges.contains_key(&start) {
                let message = if legacy {
                    format!("财报索引偏移重复: offset={start}（第 {index} 条，code={code}）")
                } else {
                    "duplicate valid financial offset".to_owned()
                };
                return Err(malformed(message).with_context("record_index", format!("{index}")));
            }
            let end = match start.checked_add(header.report_size as usize) {
                Some(end) => end,
                None if legacy => {
                    index_issue(
                        options,
                        &mut diagnostics,
                        index,
                        "financial data offset is out of range",
                    )?;
                    omitted += 1;
                    completion = FinancialCompletion::Incomplete;
                    continue;
                }
                None => return Err(malformed("financial data offset overflows")),
            };
            if end > data.len() {
                index_issue(
                    options,
                    &mut diagnostics,
                    index,
                    "financial data offset is out of range",
                )?;
                omitted += 1;
                completion = FinancialCompletion::Incomplete;
                continue;
            }
            if !legacy
                && (start < index_end
                    || ranges
                        .range(..start)
                        .next_back()
                        .is_some_and(|(_, previous_end)| *previous_end > start)
                    || ranges
                        .range(start..)
                        .next()
                        .is_some_and(|(next_start, _)| *next_start < end))
            {
                return Err(malformed("overlapping financial data/index ranges")
                    .with_context("record_index", format!("{index}")));
            }
            total_fields = total_fields
                .checked_add(u64::from(fields))
                .ok_or_else(|| malformed("financial total field count overflows"))?;
            if let Some(adapter) = adapter.as_deref_mut() {
                adapter.total_fields(&header, index, total_fields)?;
            } else if total_fields > options.max_total_fields {
                return Err(malformed("financial total field limit exceeded"));
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(fields as usize)
                .map_err(|_| malformed("financial field allocation failed"))?;
            for chunk in data[start..end].chunks(16 * 1024) {
                dat_checkpoint(options, boundary, &mut adapter)?;
                for bytes in chunk.as_chunks::<4>().0 {
                    values.push(WireFloat::from_bits(bars::u32_at(bytes, 0)));
                }
            }
            ranges.insert(start, end);
            let row = FinancialRecord {
                code,
                market_id: entry[6],
                report_date: date,
                report_date_raw: date_raw,
                date_source,
                fields: values,
                header,
                index,
                data_range: start..end,
                raw: None,
            };
            if let Some(adapter) = adapter.as_deref_mut() {
                adapter.record(&row)?;
            } else {
                rows.try_reserve(1)
                    .map_err(|_| malformed("financial record allocation failed"))?;
                rows.push(row);
            }
        }
    }
    dat_checkpoint(options, boundary, &mut adapter)?;
    // Copy only after all header/index/field budget checks have succeeded.
    let raw = retain_payload(data, options, boundary)?;
    for row in &mut rows {
        checkpoint(options, boundary)?;
        row.raw = raw.clone();
    }
    checkpoint(options, boundary)?;
    Ok(FinancialDatResult {
        records: ReadResult::new(rows, 0, diagnostics),
        header: Some(header),
        completion,
        consumed_indexes: consumed,
        omitted_indexes: omitted,
        date_source,
        raw,
        archive: None,
        archive_raw: None,
        source_meta: None,
    })
}

fn retain_payload(
    data: &[u8],
    options: &FinancialParseOptions,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<Option<Arc<[u8]>>> {
    if options.raw == RawPolicy::Omit {
        return Ok(None);
    }
    // One shared payload; field/index budgets are checked separately before allocation.
    let mut copy = Vec::new();
    copy.try_reserve_exact(data.len())
        .map_err(|_| malformed("financial raw allocation failed"))?;
    for chunk in data.chunks(64 * 1024) {
        checkpoint(options, boundary)?;
        copy.extend_from_slice(chunk);
    }
    checkpoint(options, boundary)?;
    Ok(Some(Arc::from(copy)))
}

#[derive(Clone, Debug)]
pub struct FinancialFileInfo {
    filename: String,
    hash: String,
    filesize: Option<u64>,
    filesize_text: String,
    diagnostics: Vec<Diagnostic>,
}
impl FinancialFileInfo {
    pub fn filename(&self) -> &str {
        &self.filename
    }
    /// Unverified server metadata; no digest authentication has been performed.
    pub fn hash(&self) -> &str {
        &self.hash
    }
    pub const fn filesize(&self) -> Option<u64> {
        self.filesize
    }
    pub fn filesize_text(&self) -> &str {
        &self.filesize_text
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

pub fn parse_file_list(data: &[u8]) -> Result<Vec<FinancialFileInfo>> {
    parse_file_list_with_strict(data, false)
}
pub fn parse_file_list_with_strict(data: &[u8], strict: bool) -> Result<Vec<FinancialFileInfo>> {
    parse_file_list_with_boundary(data, strict, &mut || Ok(()))
}
pub(crate) fn parse_file_list_with_boundary(
    data: &[u8],
    strict: bool,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<FinancialFileInfo>> {
    boundary()?;
    let mut result = Vec::new();
    let text = bounded_lossy_utf8(data, boundary)?;
    for (filename, hash, size) in file_list_rows(&text) {
        boundary()?;
        let Some(integer) = decimal_integer(size) else {
            continue;
        };
        let digits = integer.trim_start_matches(['+', '-']);
        let filesize = if integer.starts_with('-') && digits.bytes().any(|b| b != b'0') {
            None
        } else {
            digits.parse::<u64>().ok()
        };
        let diagnostics = if filesize.is_none() {
            if strict {
                return Err(malformed("financial list size is negative or exceeds u64"));
            }
            vec![issue(
                "list",
                "financial list size is negative or exceeds u64; raw text retained",
            )]
        } else {
            Vec::new()
        };
        result.push(FinancialFileInfo {
            filename: filename.to_owned(),
            hash: hash.to_owned(),
            filesize,
            filesize_text: size.to_owned(),
            diagnostics,
        });
    }
    boundary()?;
    Ok(result)
}

fn file_list_rows(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    source_trim(text).split('\n').filter_map(|line| {
        let mut columns = source_trim(line).split(',');
        Some((columns.next()?, columns.next()?, columns.next()?))
    })
}

/// UTF-8 replacement with bounded cancellation points, including incomplete
/// multibyte scalars split at a chunk boundary. Matches from_utf8_lossy.
fn bounded_lossy_utf8(data: &[u8], boundary: &mut impl FnMut() -> Result<()>) -> Result<String> {
    let mut result = String::new();
    let mut offset = 0;
    while offset < data.len() {
        boundary()?;
        let end = offset.saturating_add(16 * 1024).min(data.len());
        while offset < end {
            match std::str::from_utf8(&data[offset..end]) {
                Ok(text) => {
                    result
                        .try_reserve(text.len())
                        .map_err(|_| malformed("financial list text allocation failed"))?;
                    result.push_str(text);
                    offset = end;
                }
                Err(error) => {
                    let valid_end = offset + error.valid_up_to();
                    let valid = std::str::from_utf8(&data[offset..valid_end])
                        .map_err(|_| malformed("invalid financial UTF-8 prefix"))?;
                    result
                        .try_reserve(valid.len() + 3)
                        .map_err(|_| malformed("financial list text allocation failed"))?;
                    result.push_str(valid);
                    offset = valid_end;
                    if let Some(length) = error.error_len() {
                        result.push('\u{fffd}');
                        offset += length;
                    } else if end == data.len() {
                        result.push('\u{fffd}');
                        offset = end;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    boundary()?;
    Ok(result)
}
// Arbitrary-length signed decimal, including Python's between-digit underscores.
// Unicode decimal digits outside ASCII are deferred to the T23 Python boundary.
fn decimal_integer(text: &str) -> Option<String> {
    let text = source_trim(text);
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() {
        return None;
    }
    let bytes = digits.as_bytes();
    if !bytes.first()?.is_ascii_digit() || !bytes.last()?.is_ascii_digit() {
        return None;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if !(byte.is_ascii_digit()
            || *byte == b'_'
                && index > 0
                && bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    Some(text.replace('_', ""))
}

/// Internal T23 policy seam. It cannot be selected through public Rust options.
/// This is the existing Python DAT policy, including no direct-DAT byte cap.
#[doc(hidden)]
pub mod python_legacy {
    use super::*;
    /// Scalar/object behavior and strictness are supplied by the caller; the
    /// inventory traversal and ordered candidate selection are shared.
    pub trait DatArchiveSelection<T> {
        type Error;
        fn check_count(&mut self, count: usize) -> std::result::Result<(), Self::Error>;
        fn is_candidate(&mut self, entry: &T) -> std::result::Result<bool, Self::Error>;
        fn check_candidates(&mut self, entries: &[T]) -> std::result::Result<(), Self::Error>;
        fn choose(&mut self, entries: &[T]) -> std::result::Result<usize, Self::Error>;
        fn check_selected(&mut self, entry: &T) -> std::result::Result<(), Self::Error>;
        fn invalid_selection(&mut self) -> Self::Error;
    }

    pub fn select_dat_entry<T, A: DatArchiveSelection<T>>(
        entries: impl IntoIterator<Item = std::result::Result<T, A::Error>>,
        count: usize,
        adapter: &mut A,
    ) -> std::result::Result<T, A::Error> {
        adapter.check_count(count)?;
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry?;
            if adapter.is_candidate(&entry)? {
                candidates.push(entry);
            }
        }
        adapter.check_candidates(&candidates)?;
        let choice = adapter.choose(&candidates)?;
        if choice >= candidates.len() {
            return Err(adapter.invalid_selection());
        }
        let selected = candidates.remove(choice);
        adapter.check_selected(&selected)?;
        Ok(selected)
    }
    /// Object-language policy/projection only. Header/index traversal, accepted
    /// offset tracking and float decoding always stay in the shared DAT core.
    pub trait DatCallbacks {
        fn checkpoint(&mut self) -> Result<()>;
        fn header(&mut self, header: &FinancialHeader) -> Result<()>;
        fn field_count(&mut self, header: &FinancialHeader, fields: u32) -> Result<()>;
        fn total_fields(
            &mut self,
            header: &FinancialHeader,
            index: usize,
            total: u64,
        ) -> Result<()>;
        fn record(&mut self, record: &FinancialRecord) -> Result<()>;
    }
    /// Streams a row at a time so the Python result never coexists with a
    /// second full Rust table. No strict byte/overlap/default limit is added.
    pub fn visit_dat(data: &[u8], adapter: &mut dyn DatCallbacks) -> Result<()> {
        parse_core_with_adapter(
            data,
            DatePolicy::Legacy(0),
            &FinancialParseOptions::default(),
            true,
            &mut || Ok(()),
            Some(adapter),
        )
        .map(|_| ())
    }
    /// Shared row splitting with the strict Rust parser. Numeric conversion
    /// remains Python `int` so arbitrary precision and runtime Unicode survive.
    pub fn file_list_rows(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
        super::file_list_rows(text)
    }
    pub fn parse_dat(data: &[u8], nonzero_override: u32) -> Result<FinancialDatResult> {
        parse_core(
            data,
            DatePolicy::Legacy(nonzero_override),
            &FinancialParseOptions::default(),
            true,
        )
    }
    pub fn first_filename_date(filename: &str) -> u32 {
        filename_dates(filename).next().unwrap_or(0)
    }
    /// The binding must apply Python `int(text)` and skip ValueError. Keeping the
    /// original token here also preserves Unicode decimal digits and arbitrary
    /// precision that the Rust public u64 projection cannot represent.
    pub fn file_list_candidates(data: &[u8]) -> Vec<(String, String, String)> {
        let text = String::from_utf8_lossy(data);
        file_list_rows(&text)
            .map(|(filename, hash, size)| (filename.to_owned(), hash.to_owned(), size.to_owned()))
            .collect()
    }
    /// Return signed size text for conversion to Python's arbitrary precision int.
    pub fn parse_file_list(data: &[u8]) -> Vec<(String, String, String)> {
        let text = String::from_utf8_lossy(data);
        file_list_rows(&text)
            .filter_map(|(filename, hash, size)| {
                Some((filename.to_owned(), hash.to_owned(), decimal_integer(size)?))
            })
            .collect()
    }
}

#[cfg(test)]
mod source_cases {
    use super::*;

    #[test]
    fn list_utf8_replacement_and_cancellation_work_across_chunk_edges() {
        let mut bytes = vec![b'a'; 16 * 1024 - 1];
        bytes.extend_from_slice("中😀".as_bytes());
        bytes.extend_from_slice(&[0xff, 0xe2, 0x82]);
        let expected = String::from_utf8_lossy(&bytes).into_owned();
        assert_eq!(
            bounded_lossy_utf8(&bytes, &mut || Ok(())).unwrap(),
            expected
        );
        let mut calls = 0;
        let error = bounded_lossy_utf8(&bytes, &mut || {
            calls += 1;
            if calls >= 2 {
                Err(Error::new(ErrorKind::Timeout, "list decode expired"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Timeout);
    }

    #[test]
    fn operation_boundary_errors_survive_before_any_dat_or_list_parsing() {
        for kind in [ErrorKind::Timeout, ErrorKind::Cancelled, ErrorKind::Closed] {
            let mut stop = || Err(Error::new(kind, "inherited operation stopped"));
            let error = parse_dat_for_archive_with_boundary(
                &[],
                "file.zip",
                None,
                &FinancialParseOptions::default(),
                &mut stop,
            )
            .unwrap_err();
            assert_eq!(error.kind(), kind);
            let error = parse_file_list_with_boundary(b"a,h,1", false, &mut stop).unwrap_err();
            assert_eq!(error.kind(), kind);
        }
    }

    #[test]
    fn legacy_policy_has_no_direct_dat_byte_cap() {
        let bytes = [0_u8; HEADER_BYTES];
        let options = FinancialParseOptions::default()
            .with_max_dat_bytes(1)
            .unwrap();
        assert_eq!(
            parse_core(&bytes, DatePolicy::Legacy(0), &options, false)
                .unwrap_err()
                .kind(),
            ErrorKind::LimitExceeded
        );
        assert!(parse_core(&bytes, DatePolicy::Legacy(0), &options, true).is_ok());
    }

    #[test]
    fn archive_provenance_keeps_exact_identity_and_size_without_guessing() {
        let provenance = ArchiveProvenance::new(
            b"folder/a.dat".to_vec(),
            7,
            20,
            20,
            DatSelector::RequireSingle,
        )
        .unwrap();
        let result = parse_dat_detailed(&[], None, &FinancialParseOptions::default())
            .unwrap()
            .with_archive(provenance);
        let retained = result.archive().unwrap();
        assert_eq!(retained.entry_name(), b"folder/a.dat");
        assert_eq!(retained.entry_index(), 7);
        assert_eq!(retained.declared_size(), 20);
        assert_eq!(retained.actual_size(), 20);
        assert_eq!(retained.selector(), &DatSelector::RequireSingle);
        assert!(
            ArchiveProvenance::new(b"a.dat".to_vec(), 0, 20, 19, DatSelector::RequireSingle)
                .is_err()
        );
    }
}
