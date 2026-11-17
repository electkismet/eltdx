#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg(feature = "offline")]
//! Authored source cases; execution is deferred by the T01-T30 implementation gate.
//! Includes T22 Stage B shared archive/local dispatch source cases.
use eltdx::offline::financial::{
    self, python_legacy, DatSelector, FinancialArchiveName, FinancialCompletion,
    FinancialParseOptions, FinancialReadOptions, FinancialRecordsQuery, ReportDateSource,
};
use eltdx::{CancellationToken, ErrorKind, RawPolicy, TradingDate};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn header(count: u16, size: u32, date: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(-7_i16).to_le_bytes());
    bytes.extend_from_slice(&date.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.extend_from_slice(&0x12345678_u32.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&0xfedcba98_u32.to_le_bytes());
    bytes
}
fn entry(code: [u8; 6], market: u8, offset: u32) -> Vec<u8> {
    let mut bytes = code.to_vec();
    bytes.push(market);
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes
}
fn fixture(rows: &[([u8; 6], u8, Vec<u32>)], date: u32) -> Vec<u8> {
    let fields = rows.first().map_or(0, |row| row.2.len());
    let base = 20 + rows.len() * 11;
    let mut bytes = header(rows.len() as u16, (fields * 4) as u32, date);
    for (index, (code, market, _)) in rows.iter().enumerate() {
        bytes.extend(entry(*code, *market, (base + index * fields * 4) as u32));
    }
    for (_, _, values) in rows {
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes
}
fn options() -> FinancialParseOptions {
    FinancialParseOptions::default()
}
fn sample() -> Vec<u8> {
    fixture(&[(*b"600000", 1, vec![0x3f800000])], 20240331)
}
fn kind<T: std::fmt::Debug>(value: eltdx::Result<T>, expected: ErrorKind) {
    assert_eq!(value.expect_err("expected rejection").kind(), expected);
}

#[test]
fn exact_bits_raw_header_market_code_and_ranges() -> TestResult {
    let bits = vec![
        0x80000000, 0x7fc12345, 0x7fa54321, 0xff800000, 0x7f800000, 0x00000001, 0x7f7fffff,
        0xffffffff,
    ];
    let bytes = fixture(
        &[([b'A', 0xff, b'B', 0, 0, 0], 255, bits.clone())],
        20240229,
    );
    for policy in [
        RawPolicy::Omit,
        RawPolicy::RetainPayload,
        RawPolicy::RetainWire,
    ] {
        let result = financial::parse_dat_detailed(&bytes, None, &options().with_raw(policy))?;
        assert_eq!(result.completion(), FinancialCompletion::Complete);
        let row = &result.rows()[0];
        assert_eq!(row.code(), "A\u{fffd}B");
        assert_eq!(row.market_id(), 255);
        assert_eq!(
            row.fields().iter().map(|v| v.bits()).collect::<Vec<_>>(),
            bits
        );
        assert_eq!(row.header().unknown(), -7);
        assert_eq!(row.header().unknown1(), 0x12345678);
        assert_eq!(row.header().unknown2(), 0xfedcba98);
        assert_eq!(row.header().report_size(), 32);
        assert_eq!(row.index(), 0);
        assert_eq!(row.data_range(), 31..63);
        assert_eq!(
            row.raw(),
            (policy != RawPolicy::Omit).then_some(&bytes[31..])
        );
        assert_eq!(
            result.raw(),
            (policy != RawPolicy::Omit).then_some(bytes.as_slice())
        );
    }
    Ok(())
}

#[test]
fn empty_short_and_zero_report_size_are_distinct() -> TestResult {
    let empty = financial::parse_dat_detailed(&[], None, &options())?;
    assert_eq!(empty.completion(), FinancialCompletion::Complete);
    assert!(empty.header().is_none());
    for length in 1..20 {
        let bytes = vec![0; length];
        let short = financial::parse_dat_detailed(&bytes, None, &options())?;
        assert_eq!(short.completion(), FinancialCompletion::Incomplete);
        assert_eq!(short.records().ignored_tail_bytes(), length);
        assert_eq!(short.diagnostics().len(), 1);
        kind(
            financial::parse_dat(&bytes, None, &options().with_strict_index(true)),
            ErrorKind::ResourceFormat,
        );
    }
    let zero = financial::parse_dat_detailed(
        &header(u16::MAX, 0, 20240331),
        None,
        &options().with_strict_index(true),
    )?;
    assert_eq!(zero.completion(), FinancialCompletion::Complete);
    assert_eq!(zero.header().unwrap().max_count(), u16::MAX);
    assert!(zero.rows().is_empty());
    Ok(())
}

#[test]
fn header_and_field_limits_are_checked_before_record_allocation() -> TestResult {
    for size in [1, 2, 3, 5, u32::MAX, 4097 * 4] {
        kind(
            financial::parse_dat(&header(u16::MAX, size, 20240331), None, &options()),
            ErrorKind::ResourceFormat,
        );
    }
    let bytes = fixture(&[(*b"600000", 1, vec![0; 4096])], 20240331);
    assert_eq!(
        financial::parse_dat(&bytes, None, &options())?.rows()[0]
            .fields()
            .len(),
        4096
    );
    kind(
        financial::parse_dat(&bytes, None, &options().with_max_fields_per_record(4095)?),
        ErrorKind::ResourceFormat,
    );
    Ok(())
}

#[test]
fn exact_and_smaller_dat_byte_budgets() -> TestResult {
    let bytes = sample();
    assert_eq!(
        financial::parse_dat(
            &bytes,
            None,
            &options().with_max_dat_bytes(bytes.len() as u64)?
        )?
        .rows()
        .len(),
        1
    );
    kind(
        financial::parse_dat(
            &bytes,
            None,
            &options().with_max_dat_bytes(bytes.len() as u64 - 1)?,
        ),
        ErrorKind::LimitExceeded,
    );
    Ok(())
}

#[test]
fn total_field_budget_counts_accepted_rows_before_allocation() -> TestResult {
    let bytes = fixture(
        &[(*b"600000", 1, vec![0; 3]), (*b"000001", 0, vec![0; 3])],
        20240331,
    );
    assert_eq!(
        financial::parse_dat(&bytes, None, &options().with_max_total_fields(6)?)?
            .rows()
            .len(),
        2
    );
    kind(
        financial::parse_dat(&bytes, None, &options().with_max_total_fields(5)?),
        ErrorKind::ResourceFormat,
    );
    Ok(())
}

#[test]
fn full_default_total_budget_boundary() -> TestResult {
    // Synthetic 16 MiB payload, 1024 * 4096 fields exactly, then one extra row.
    let rows = (0..1024)
        .map(|_| (*b"600000", 1, vec![0; 4096]))
        .collect::<Vec<_>>();
    assert_eq!(
        financial::parse_dat(&fixture(&rows, 20240331), None, &options())?
            .rows()
            .len(),
        1024
    );
    let mut over = rows;
    over.push((*b"000001", 0, vec![0; 4096]));
    kind(
        financial::parse_dat(&fixture(&over, 20240331), None, &options()),
        ErrorKind::ResourceFormat,
    );
    Ok(())
}

#[test]
fn truncated_indexes_are_partial_and_strict_rejects() -> TestResult {
    let mut bytes = header(2, 4, 20240331);
    bytes.extend(entry([0; 6], 0, 0));
    bytes.extend([1; 5]);
    let result = financial::parse_dat_detailed(&bytes, None, &options())?;
    assert_eq!(result.completion(), FinancialCompletion::Incomplete);
    assert_eq!(result.consumed_indexes(), 1);
    assert_eq!(result.omitted_indexes(), 2);
    assert_eq!(result.diagnostics().len(), 2);
    kind(
        financial::parse_dat(&bytes, None, &options().with_strict_index(true)),
        ErrorKind::ResourceFormat,
    );
    Ok(())
}

#[test]
fn invalid_offsets_are_skipped_with_diagnostics_and_never_complete() -> TestResult {
    for offset in [36, u32::MAX] {
        let mut bytes = header(1, 4, 20240331);
        bytes.extend(entry(*b"600000", 1, offset));
        bytes.extend([0; 4]);
        let result = financial::parse_dat_detailed(&bytes, None, &options())?;
        assert!(result.rows().is_empty());
        assert_eq!(result.completion(), FinancialCompletion::Incomplete);
        assert_eq!(result.omitted_indexes(), 1);
        assert_eq!(result.diagnostics()[0].source(), "offline.financial");
        assert_eq!(
            result.diagnostics()[0].error_kind(),
            Some(ErrorKind::ResourceFormat)
        );
        kind(
            financial::parse_dat(&bytes, None, &options().with_strict_index(true)),
            ErrorKind::ResourceFormat,
        );
    }
    Ok(())
}

#[test]
fn duplicate_valid_offsets_and_partial_overlaps_fail() {
    for second in [42_u32, 44] {
        let mut bytes = header(2, 8, 20240331);
        bytes.extend(entry(*b"600000", 1, 42));
        bytes.extend(entry(*b"000001", 0, second));
        bytes.extend([0; 16]);
        kind(
            financial::parse_dat(&bytes, None, &options()),
            ErrorKind::ResourceFormat,
        );
    }
}

#[test]
fn out_of_order_nonoverlapping_data_is_valid_and_wire_order_is_kept() -> TestResult {
    let mut bytes = header(2, 4, 20240331);
    bytes.extend(entry(*b"600000", 1, 46));
    bytes.extend(entry(*b"000001", 0, 42));
    bytes.extend(0x80000000_u32.to_le_bytes());
    bytes.extend(0x7fc12345_u32.to_le_bytes());
    let result = financial::parse_dat(&bytes, None, &options())?;
    assert_eq!(result.rows()[0].code(), "600000");
    assert_eq!(result.rows()[0].fields()[0].bits(), 0x7fc12345);
    assert_eq!(result.rows()[1].fields()[0].bits(), 0x80000000);
    Ok(())
}

#[test]
fn metadata_overlap_is_rust_error_but_legacy_keeps_existing_policy() -> TestResult {
    let mut bytes = header(1, 4, 20240331);
    bytes.extend(entry(*b"600000", 1, 8));
    kind(
        financial::parse_dat(&bytes, None, &options()),
        ErrorKind::ResourceFormat,
    );
    assert_eq!(
        python_legacy::parse_dat(&bytes, 0)?.rows()[0].fields()[0].bits(),
        0x12345678
    );
    Ok(())
}

#[test]
fn legacy_also_rejects_previously_accepted_duplicate_offsets() {
    let mut bytes = header(2, 4, 20240331);
    bytes.extend(entry(*b"600000", 1, 42));
    bytes.extend(entry(*b"000001", 0, 42));
    bytes.extend([0; 4]);
    kind(
        python_legacy::parse_dat(&bytes, 0),
        ErrorKind::ResourceFormat,
    );
}

#[test]
fn skip_markers_are_recorded_even_when_other_rows_are_valid() -> TestResult {
    let mut bytes = fixture(&[([0; 6], 0, vec![0]), (*b"600000", 1, vec![0])], 20240331);
    let result = financial::parse_dat_detailed(&bytes, None, &options())?;
    assert_eq!(result.rows().len(), 1);
    assert_eq!(result.completion(), FinancialCompletion::Incomplete);
    assert_eq!(result.omitted_indexes(), 1);
    bytes[7 + 20 + 11..11 + 20 + 11].copy_from_slice(&0_u32.to_le_bytes());
    let result = financial::parse_dat_detailed(&bytes, None, &options())?;
    assert!(result.rows().is_empty());
    assert_eq!(result.omitted_indexes(), 2);
    Ok(())
}

#[test]
fn date_precedence_header_archive_and_explicit() -> TestResult {
    let bytes = sample();
    let explicit = TradingDate::new(2025, 6, 30)?;
    assert_eq!(
        financial::parse_dat_detailed(&bytes, None, &options())?.rows()[0].report_date_raw(),
        20240331
    );
    let archive =
        financial::parse_dat_for_archive(&bytes, "x20240230_20241231.zip", None, &options())?;
    assert_eq!(archive.rows()[0].report_date_raw(), 20241231);
    assert_eq!(archive.date_source(), ReportDateSource::ArchiveFilename);
    let result =
        financial::parse_dat_for_archive(&bytes, "20241231.zip", Some(explicit), &options())?;
    assert_eq!(result.rows()[0].report_date(), Some(explicit));
    assert_eq!(result.date_source(), ReportDateSource::Explicit);
    assert!(financial::filename_report_date("20240230.zip").is_none());
    assert!(financial::filename_report_date("parent20240101/no_date.zip").is_none());
    assert_eq!(
        financial::filename_report_date(
            "tdxfin/\u{ff12}\u{ff10}\u{ff12}\u{ff14}\u{ff10}\u{ff13}\u{ff13}\u{ff11}.zip"
        ),
        Some(TradingDate::new(2024, 3, 31)?)
    );
    assert_eq!(financial::filename_report_date("tdxfin/\u{0662}\u{0660}\u{0662}\u{0664}\u{0660}\u{0662}\u{0663}\u{0660}_\u{0662}\u{0660}\u{0662}\u{0664}\u{0661}\u{0662}\u{0663}\u{0661}.zip"),
        Some(TradingDate::new(2024, 12, 31)?));
    assert_eq!(
        financial::filename_report_date(
            "\u{11f52}\u{11f50}\u{11f52}\u{11f54}\u{11f50}\u{11f53}\u{11f53}\u{11f51}.zip"
        ),
        Some(TradingDate::new(2024, 3, 31)?)
    );
    assert_eq!(
        financial::filename_report_date(
            "\u{1e4f2}\u{1e4f0}\u{1e4f2}\u{1e4f4}\u{1e4f0}\u{1e4f3}\u{1e4f3}\u{1e4f1}.zip"
        ),
        Some(TradingDate::new(2024, 3, 31)?)
    );
    Ok(())
}

#[test]
fn invalid_header_remains_visible_even_with_valid_override() -> TestResult {
    let bytes = fixture(&[(*b"600000", 1, vec![0])], 20240230);
    let result = financial::parse_dat_detailed(&bytes, None, &options())?;
    assert_eq!(result.rows()[0].report_date(), None);
    assert_eq!(result.rows()[0].report_date_raw(), 20240230);
    assert_eq!(result.date_source(), ReportDateSource::Unavailable);
    assert_eq!(result.diagnostics()[0].stage(), "date");
    let result =
        financial::parse_dat_detailed(&bytes, Some(TradingDate::new(2024, 3, 31)?), &options())?;
    assert_eq!(result.rows()[0].header().report_date_raw(), 20240230);
    assert_eq!(result.diagnostics().len(), 1);
    Ok(())
}

#[test]
fn python_date_override_keeps_first_unvalidated_eight_digit_text() -> TestResult {
    let filename = "tdxfin/a20240230_20241231.zip";
    assert_eq!(python_legacy::first_filename_date(filename), 20240230);
    assert_eq!(python_legacy::first_filename_date("tdxfin/\u{0662}\u{0660}\u{0662}\u{0664}\u{0660}\u{0662}\u{0663}\u{0660}_\u{0662}\u{0660}\u{0662}\u{0664}\u{0661}\u{0662}\u{0663}\u{0661}.zip"), 20240230);
    assert_eq!(python_legacy::first_filename_date("none"), 0);
    let result = python_legacy::parse_dat(&sample(), python_legacy::first_filename_date(filename))?;
    assert_eq!(result.rows()[0].report_date_raw(), 20240230);
    assert_eq!(result.rows()[0].report_date(), None);
    assert_eq!(
        python_legacy::parse_dat(&sample(), 0)?.rows()[0].report_date_raw(),
        20240331
    );
    Ok(())
}

#[test]
fn list_replacement_trimming_extra_columns_and_invalid_sizes() -> TestResult {
    let bytes = b" \n abc.zip , hash , +42 ,ignored\r\nnegative,h,-1\nhuge,h,18446744073709551616\n\xff.zip,h,7\nbad,h,no\nshort,h\nzero,h,-0\nunder,h,1_000\n";
    let list = financial::parse_file_list(bytes)?;
    assert_eq!(list.len(), 6);
    assert_eq!(list[0].filename(), "abc.zip ");
    assert_eq!(list[0].hash(), " hash ");
    assert_eq!(list[0].filesize(), Some(42));
    assert_eq!(list[1].filesize(), None);
    assert_eq!(list[1].filesize_text(), "-1");
    assert_eq!(list[1].diagnostics().len(), 1);
    assert_eq!(list[2].filesize(), None);
    assert_eq!(list[3].filename(), "\u{fffd}.zip");
    assert_eq!(list[4].filesize(), Some(0));
    assert_eq!(list[5].filesize(), Some(1000));
    kind(
        financial::parse_file_list_with_strict(bytes, true),
        ErrorKind::ResourceFormat,
    );
    assert!(financial::parse_file_list(&[])?.is_empty());
    let legacy = python_legacy::parse_file_list(bytes);
    assert_eq!(legacy[1].2, "-1");
    assert_eq!(legacy[2].2, "18446744073709551616");
    let candidates = python_legacy::file_list_candidates("unicode,h,１２３\nbad,h,no".as_bytes());
    assert_eq!(candidates[0].2, "１２３");
    assert_eq!(candidates[1].2, "no"); // Python int/ValueError remains the binding's decision.
    Ok(())
}

#[test]
fn options_reject_zero_unbounded_and_oversized_values() -> TestResult {
    for value in [0, u64::MAX, financial::MAX_DAT_BYTES + 1] {
        kind(
            options().with_max_dat_bytes(value),
            ErrorKind::InvalidArgument,
        );
    }
    for value in [0, u32::MAX, 4097] {
        kind(
            options().with_max_fields_per_record(value),
            ErrorKind::InvalidArgument,
        );
    }
    for value in [0, u64::MAX, financial::MAX_TOTAL_FIELDS + 1] {
        kind(
            options().with_max_total_fields(value),
            ErrorKind::InvalidArgument,
        );
    }
    for value in [0, u64::MAX, financial::MAX_ARCHIVE_BYTES + 1] {
        kind(
            FinancialReadOptions::default().with_max_archive_bytes(value),
            ErrorKind::InvalidArgument,
        );
    }
    for value in [0, u32::MAX, 65] {
        kind(
            FinancialReadOptions::default().with_max_entries(value),
            ErrorKind::InvalidArgument,
        );
    }
    assert_eq!(
        FinancialReadOptions::default()
            .with_max_entries(1)?
            .max_entries(),
        1
    );
    assert_eq!(
        FinancialReadOptions::default()
            .with_max_archive_bytes(1)?
            .max_archive_bytes(),
        1
    );
    Ok(())
}

#[test]
fn remote_names_and_selectors_are_checked_without_file_or_network_io() -> TestResult {
    for name in [
        "", ".", "..", "a/b.zip", "a\\b.zip", "C:a.zip", "x\0.zip", " x.zip", "中.zip",
    ] {
        kind(FinancialArchiveName::new(name), ErrorKind::InvalidArgument);
    }
    assert!(FinancialArchiveName::new("a".repeat(293)).is_ok());
    kind(
        FinancialArchiveName::new("a".repeat(294)),
        ErrorKind::InvalidArgument,
    );
    let name = FinancialArchiveName::new("gpcw20240331.zip")?;
    assert_eq!(name.remote_path(), "tdxfin/gpcw20240331.zip");
    let query =
        FinancialRecordsQuery::new(name).with_selector(DatSelector::named("inside/report.dat")?)?;
    assert_eq!(query.filename().as_str(), "gpcw20240331.zip");
    assert_eq!(
        query.selector(),
        &DatSelector::Named("inside/report.dat".into())
    );
    for name in ["", "data.DAT", "x\0.dat"] {
        kind(DatSelector::named(name), ErrorKind::InvalidArgument);
    }
    kind(
        FinancialReadOptions::default().with_selector(DatSelector::Named("bad.DAT".into())),
        ErrorKind::InvalidArgument,
    );
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "eltdx-financial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn local_dat_uses_shared_loader_limits_and_header_not_filename() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("gpcw20241231.dat");
    let bytes = sample();
    std::fs::write(&path, &bytes)?;
    let result = financial::read_dat_file(&path, &FinancialReadOptions::default())?;
    assert_eq!(result.rows()[0].report_date_raw(), 20240331);
    let small = FinancialReadOptions::default()
        .with_parse(options().with_max_dat_bytes(bytes.len() as u64 - 1)?);
    kind(
        financial::read_dat_file(&path, &small),
        ErrorKind::LimitExceeded,
    );
    let overridden =
        FinancialReadOptions::default().with_report_date(Some(TradingDate::new(2025, 6, 30)?));
    assert_eq!(
        financial::read_dat_file(&path, &overridden)?.rows()[0].report_date_raw(),
        20250630
    );
    kind(
        financial::read_dat_file(
            &scratch.0.join("missing.dat"),
            &FinancialReadOptions::default(),
        ),
        ErrorKind::Io,
    );
    kind(
        financial::read_dat_file(&scratch.0, &FinancialReadOptions::default()),
        ErrorKind::Io,
    );
    Ok(())
}

#[test]
fn cancelled_parser_and_local_loader_return_cancelled() -> TestResult {
    let token = CancellationToken::default();
    token.cancel();
    let options = options().with_cancel(Some(token));
    kind(
        financial::parse_dat(&sample(), None, &options),
        ErrorKind::Cancelled,
    );
    kind(
        financial::parse_dat(&[], None, &options),
        ErrorKind::Cancelled,
    );
    kind(
        financial::read_dat_file(
            &PathBuf::from("not-opened.dat"),
            &FinancialReadOptions::default().with_parse(options),
        ),
        ErrorKind::Cancelled,
    );
    Ok(())
}

fn zip_fixture(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        writer
            .start_file(
                *name,
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn zip_selects_exact_checked_entry_and_retains_both_raw_sources() -> TestResult {
    let dat = sample();
    let bytes = zip_fixture(&[("note", b"retained separately"), ("folder/data.dat", &dat)]);
    let options =
        FinancialReadOptions::new().with_parse(options().with_raw(RawPolicy::RetainPayload));
    let result = financial::parse_zip_detailed(&bytes, &options)?;
    assert_eq!(result.rows()[0].report_date_raw(), 20240331);
    assert_eq!(result.archive_raw(), Some(bytes.as_slice()));
    assert_eq!(result.raw(), Some(dat.as_slice()));
    assert!(result.source_meta().is_none());
    let source = result.archive().ok_or("missing archive provenance")?;
    assert_eq!(source.entry_name(), b"folder/data.dat");
    assert_eq!(source.entry_index(), 1);
    assert_eq!(source.actual_size(), dat.len() as u64);
    assert_eq!(source.declared_size(), dat.len() as u64);
    Ok(())
}

#[test]
fn zip_ambiguity_case_sensitive_suffix_and_exact_named_selection() -> TestResult {
    let first = sample();
    let second = fixture(&[(*b"000001", 0, vec![0x40000000])], 20240630);
    let bytes = zip_fixture(&[
        ("A.DAT", &first),
        ("first.dat", &first),
        ("second.dat", &second),
    ]);
    kind(
        financial::parse_zip(&bytes, &FinancialReadOptions::new()),
        ErrorKind::ResourceFormat,
    );
    let named = FinancialReadOptions::new().with_selector(DatSelector::named("second.dat")?)?;
    let result = financial::parse_zip_detailed(&bytes, &named)?;
    assert_eq!(result.rows()[0].code(), "000001");
    assert_eq!(result.archive().unwrap().entry_index(), 2);
    let absent = FinancialReadOptions::new().with_selector(DatSelector::named("SECOND.dat")?)?;
    kind(
        financial::parse_zip(&bytes, &absent),
        ErrorKind::MissingData,
    );
    kind(
        financial::parse_zip(
            &zip_fixture(&[("A.DAT", &first)]),
            &FinancialReadOptions::new(),
        ),
        ErrorKind::MissingData,
    );
    Ok(())
}

#[test]
fn zip_rejects_duplicate_encrypted_corrupt_and_truncated_members() -> TestResult {
    let dat = sample();
    let duplicate = zip_fixture(&[("same.dat", &dat), ("same.dat", &dat)]);
    kind(
        financial::parse_zip(&duplicate, &FinancialReadOptions::new()),
        ErrorKind::ResourceFormat,
    );
    let original = zip_fixture(&[("a.dat", &dat), ("note", b"abcdef")]);
    let central = original
        .windows(4)
        .position(|w| w == b"PK\x01\x02")
        .unwrap();
    let mut encrypted = original.clone();
    encrypted[6] |= 1;
    encrypted[central + 8] |= 1;
    kind(
        financial::parse_zip(&encrypted, &FinancialReadOptions::new()),
        ErrorKind::ResourceFormat,
    );
    let mut corrupt = original.clone();
    let note = corrupt.windows(6).position(|w| w == b"abcdef").unwrap();
    corrupt[note] ^= 1;
    kind(
        financial::parse_zip(&corrupt, &FinancialReadOptions::new()),
        ErrorKind::ResourceFormat,
    );
    let mut truncated = original;
    truncated.pop();
    kind(
        financial::parse_zip(&truncated, &FinancialReadOptions::new()),
        ErrorKind::ResourceFormat,
    );
    Ok(())
}

#[test]
fn zip_narrows_selected_dat_without_limiting_unselected_member_to_same_size() -> TestResult {
    let dat = sample();
    let note = vec![b'x'; dat.len() + 100];
    let bytes = zip_fixture(&[("data.dat", &dat), ("large-note", &note)]);
    let exact =
        FinancialReadOptions::new().with_parse(options().with_max_dat_bytes(dat.len() as u64)?);
    assert_eq!(financial::parse_zip(&bytes, &exact)?.rows().len(), 1);
    let small =
        FinancialReadOptions::new().with_parse(options().with_max_dat_bytes(dat.len() as u64 - 1)?);
    kind(
        financial::parse_zip(&bytes, &small),
        ErrorKind::LimitExceeded,
    );
    let cap = FinancialReadOptions::new().with_max_archive_bytes(bytes.len() as u64 - 1)?;
    kind(financial::parse_zip(&bytes, &cap), ErrorKind::LimitExceeded);
    let count = FinancialReadOptions::new().with_max_entries(1)?;
    kind(
        financial::parse_zip(&bytes, &count),
        ErrorKind::LimitExceeded,
    );
    Ok(())
}

#[test]
fn zip_directory_metadata_is_not_a_dat_candidate() -> TestResult {
    let mut bytes = zip_fixture(&[("folder.dat", b""), ("data.dat", &sample())]);
    let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    // DOS directory bit on a member whose name has no slash.
    let attributes =
        u32::from_le_bytes(bytes[central + 38..central + 42].try_into().unwrap()) | 0x10;
    bytes[central + 38..central + 42].copy_from_slice(&attributes.to_le_bytes());
    let result = financial::parse_zip_detailed(&bytes, &FinancialReadOptions::new())?;
    assert_eq!(result.archive().unwrap().entry_name(), b"data.dat");
    assert_eq!(result.archive().unwrap().entry_index(), 1);
    Ok(())
}

#[test]
fn zip_declared_aggregate_expansion_rejects_before_allocation_or_inflation() -> TestResult {
    let mut bytes = zip_fixture(&[("data.dat", &sample()), ("note", b"a")]);
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(index, w)| (w == b"PK\x01\x02").then_some(index))
        .collect();
    let central = positions[1];
    let local = u32::from_le_bytes(bytes[central + 42..central + 46].try_into().unwrap()) as usize;
    // A deflate declaration avoids stored-size equality being the rejection;
    // total declared output exceeds the financial 256 MiB aggregate bound.
    bytes[central + 10..central + 12].copy_from_slice(&8_u16.to_le_bytes());
    bytes[local + 8..local + 10].copy_from_slice(&8_u16.to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&(256_u32 * 1024 * 1024).to_le_bytes());
    bytes[local + 22..local + 26].copy_from_slice(&(256_u32 * 1024 * 1024).to_le_bytes());
    kind(
        financial::parse_zip(&bytes, &FinancialReadOptions::new()),
        ErrorKind::LimitExceeded,
    );
    Ok(())
}

#[test]
fn local_zip_uses_header_date_explicit_override_and_cancellation() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("gpcw20241231.ZIP");
    let bytes = zip_fixture(&[("gpcw20250930.dat", &sample())]);
    std::fs::write(&path, bytes)?;
    let parsed = financial::read_detailed(&path, &FinancialReadOptions::new())?;
    assert_eq!(parsed.rows()[0].report_date_raw(), 20240331);
    assert_eq!(parsed.date_source(), ReportDateSource::Header);
    let explicit =
        FinancialReadOptions::new().with_report_date(Some(TradingDate::new(2026, 6, 30)?));
    assert_eq!(
        financial::read(&path, &explicit)?.rows()[0].report_date_raw(),
        20260630
    );
    let token = CancellationToken::default();
    token.cancel();
    let cancelled = FinancialReadOptions::new().with_parse(options().with_cancel(Some(token)));
    kind(financial::read(&path, &cancelled), ErrorKind::Cancelled);
    Ok(())
}

#[derive(Default)]
struct DatVisitor {
    dates: Vec<u32>,
    counts: Vec<(usize, u64)>,
    rows: Vec<(String, u8, Vec<u32>)>,
    total_limit: Option<u64>,
}
impl python_legacy::DatCallbacks for DatVisitor {
    fn checkpoint(&mut self) -> eltdx::Result<()> {
        Ok(())
    }
    fn header(&mut self, header: &financial::FinancialHeader) -> eltdx::Result<()> {
        self.dates.push(header.report_date_raw());
        Ok(())
    }
    fn field_count(&mut self, header: &financial::FinancialHeader, _: u32) -> eltdx::Result<()> {
        if !header.report_size().is_multiple_of(4) {
            return Err(eltdx::Error::new(
                ErrorKind::ResourceFormat,
                "unaligned visitor header",
            ));
        }
        Ok(())
    }
    fn total_fields(
        &mut self,
        _: &financial::FinancialHeader,
        index: usize,
        total: u64,
    ) -> eltdx::Result<()> {
        self.counts.push((index, total));
        if self.total_limit.is_some_and(|limit| total > limit) {
            return Err(eltdx::Error::new(
                ErrorKind::ResourceFormat,
                "dynamic visitor limit",
            ));
        }
        Ok(())
    }
    fn record(&mut self, row: &financial::FinancialRecord) -> eltdx::Result<()> {
        self.rows.push((
            row.code().to_owned(),
            row.market_id(),
            row.fields().iter().map(|v| v.bits()).collect(),
        ));
        Ok(())
    }
}

#[test]
fn streaming_legacy_visit_and_owned_rust_result_share_order_and_bits() -> TestResult {
    let bytes = fixture(
        &[
            (*b"600000", 255, vec![0x80000000, 0x7fc12345]),
            (*b"000001", 0, vec![0x7f800000, 0x00000001]),
        ],
        20240331,
    );
    let rust = financial::parse_dat(&bytes, None, &options())?;
    let mut visitor = DatVisitor::default();
    python_legacy::visit_dat(&bytes, &mut visitor)?;
    assert_eq!(visitor.dates, [20240331]);
    assert_eq!(visitor.counts, [(0, 2), (1, 4)]);
    let expected: Vec<_> = rust
        .rows()
        .iter()
        .map(|row| {
            (
                row.code().to_owned(),
                row.market_id(),
                row.fields().iter().map(|v| v.bits()).collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(visitor.rows, expected);
    Ok(())
}

#[test]
fn streaming_legacy_visit_obeys_dynamic_limit_before_emitting_next_row() -> TestResult {
    let bytes = fixture(
        &[(*b"600000", 1, vec![0]), (*b"000001", 0, vec![0])],
        20240331,
    );
    let mut visitor = DatVisitor {
        total_limit: Some(1),
        ..DatVisitor::default()
    };
    let error = python_legacy::visit_dat(&bytes, &mut visitor).unwrap_err();
    assert_eq!(error.message(), "dynamic visitor limit");
    assert_eq!(visitor.rows.len(), 1);
    assert_eq!(visitor.counts, [(0, 1), (1, 2)]);
    Ok(())
}

#[test]
fn streaming_legacy_visit_does_not_impose_strict_default_fields_or_overlap() -> TestResult {
    let bytes = fixture(&[(*b"600000", 1, vec![0; 4097])], 20240331);
    kind(
        financial::parse_dat(&bytes, None, &options()),
        ErrorKind::ResourceFormat,
    );
    let mut visitor = DatVisitor::default();
    python_legacy::visit_dat(&bytes, &mut visitor)?;
    assert_eq!(visitor.rows[0].2.len(), 4097);

    let mut overlap = header(2, 4, 20240331);
    overlap.extend(entry(*b"600000", 1, 42));
    overlap.extend(entry(*b"000001", 0, 43));
    overlap.extend([0; 5]);
    kind(
        financial::parse_dat(&overlap, None, &options()),
        ErrorKind::ResourceFormat,
    );
    let mut visitor = DatVisitor::default();
    python_legacy::visit_dat(&overlap, &mut visitor)?;
    assert_eq!(visitor.rows.len(), 2);
    Ok(())
}

#[test]
fn archive_selection_shared_traversal_preserves_candidate_order_and_failure_stage() -> TestResult {
    struct Selection {
        calls: Vec<String>,
    }
    impl python_legacy::DatArchiveSelection<&str> for Selection {
        type Error = eltdx::Error;
        fn check_count(&mut self, count: usize) -> eltdx::Result<()> {
            self.calls.push(format!("count:{count}"));
            Ok(())
        }
        fn is_candidate(&mut self, value: &&str) -> eltdx::Result<bool> {
            self.calls.push(format!("candidate:{value}"));
            Ok(value.ends_with(".dat"))
        }
        fn check_candidates(&mut self, values: &[&str]) -> eltdx::Result<()> {
            self.calls.push(format!("candidates:{}", values.join(",")));
            Ok(())
        }
        fn choose(&mut self, _: &[&str]) -> eltdx::Result<usize> {
            Ok(0)
        }
        fn check_selected(&mut self, value: &&str) -> eltdx::Result<()> {
            self.calls.push(format!("selected:{value}"));
            Ok(())
        }
        fn invalid_selection(&mut self) -> eltdx::Error {
            eltdx::Error::new(ErrorKind::Internal, "empty selection")
        }
    }
    let mut selector = Selection { calls: Vec::new() };
    let selected =
        python_legacy::select_dat_entry([Ok("note"), Ok("b.dat"), Ok("a.dat")], 3, &mut selector)?;
    assert_eq!(selected, "b.dat");
    assert_eq!(
        selector.calls,
        [
            "count:3",
            "candidate:note",
            "candidate:b.dat",
            "candidate:a.dat",
            "candidates:b.dat,a.dat",
            "selected:b.dat"
        ]
    );
    let mut selector = Selection { calls: Vec::new() };
    let error = python_legacy::select_dat_entry(
        [
            Ok("b.dat"),
            Err(eltdx::Error::new(ErrorKind::Io, "iterator failed")),
        ],
        2,
        &mut selector,
    )
    .unwrap_err();
    assert_eq!(error.message(), "iterator failed");
    assert_eq!(selector.calls, ["count:2", "candidate:b.dat"]);
    Ok(())
}
