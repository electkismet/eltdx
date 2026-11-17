//! Stage A source cases. Intentionally unexecuted until the T01–T30 validation gate.
//! Include private modules so these cases need no temporary public parser API.
#![allow(dead_code)]

pub use eltdx::{error, Result};
#[path = "../src/archive.rs"]
mod archive;
#[path = "../src/encoding/mod.rs"]
mod encoding;
#[path = "../src/stats.rs"]
mod stats;

#[path = "support/hq_server.rs"]
mod hq_server;

pub use eltdx::{Error, ErrorKind};
use std::io::{Cursor, Write};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

fn archive(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        writer
            .start_file(*name, FileOptions::default().compression_method(method))
            .expect("fixture header");
        writer.write_all(contents).expect("fixture content");
    }
    writer.finish().expect("fixture finish").into_inner()
}

fn stat_line() -> String {
    let mut parts = [""; 35];
    for (index, value) in [
        (0, "0"),
        (1, "000001"),
        (2, "1.25"),
        (3, "8.75"),
        (4, "20260710"),
        (11, "200.00"),
        (26, "9"),
        (31, "7"),
        (32, "5"),
        (33, "3"),
    ] {
        parts[index] = value;
    }
    parts.join("|")
}

fn stat2_line() -> String {
    let mut parts = [""; 21];
    for (index, value) in [
        (0, "0"),
        (1, "000001"),
        (2, "20260711"),
        (3, "242501.84"),
        (5, "254664.58"),
        (6, "851.80"),
        (7, "134819.47"),
        (8, "3468.47"),
        (9, "11141"),
        (10, "9513"),
        (14, "6002.06"),
        (15, "4291.39"),
    ] {
        parts[index] = value;
    }
    parts.join("|")
}

#[test]
fn public_stats_service_downloads_decodes_and_projects_both_source_tables() {
    use eltdx::{
        AmountUnit, OperationOptions, QuantityUnit, RawPolicy, SecurityId, StatsQuery, TdxClient,
        TradingDate,
    };
    let stat = stat_line();
    let stat2 = stat2_line();
    let payload = archive(
        &[
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        CompressionMethod::Deflated,
    );
    let server = hq_server::Server::start(move |request| {
        assert_eq!(request.msg_type, 0x06b9);
        let offset = u32::from_le_bytes(request.data[..4].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(request.data[4..8].try_into().unwrap()) as usize;
        let data =
            &payload[offset.min(payload.len())..offset.saturating_add(size).min(payload.len())];
        let mut bytes = (data.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(data);
        bytes
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let value = client
        .resources()
        .read_stats(
            &StatsQuery::new().with_chunk_size(37).unwrap(),
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )
        .unwrap();
    let id = SecurityId::new(eltdx::HqMarket::Shenzhen, "000001").unwrap();
    let stat = value.stat_for(&id).unwrap();
    let stat2 = value.stat2_for(&id).unwrap();
    assert_eq!(value.source_path(), "tdx://zhb.zip");
    assert_eq!(
        stat.stats_date(),
        Some(TradingDate::new(2026, 7, 10).unwrap())
    );
    assert_eq!(
        stat2.stats_date(),
        Some(TradingDate::new(2026, 7, 11).unwrap())
    );
    assert_eq!(
        stat.free_float_shares().unwrap().unit(),
        QuantityUnit::Shares
    );
    assert_eq!(
        stat.free_float_shares().unwrap().source_unit(),
        QuantityUnit::TenThousandShares
    );
    assert_eq!(
        stat.free_float_shares().unwrap().value().as_f64(),
        2_000_000.0
    );
    assert_eq!(stat2.amount().unwrap().unit(), AmountUnit::Cny);
    assert_eq!(
        stat2.amount().unwrap().source_unit(),
        AmountUnit::TenThousandCny
    );
    assert_eq!(
        stat2.amount().unwrap().value().as_f64(),
        242501.84 * 10_000.0
    );
    assert_eq!(stat2.open_volume().unwrap().unit(), QuantityUnit::Lots);
    assert_eq!(stat2.open_volume().unwrap().value().as_f64(), 11141.0);
    assert_eq!(stat.source_counts()[0].as_deref(), Some("9"));
    assert!(value.meta().raw().unwrap().payloads().len() > 1);
    assert!(value
        .meta()
        .sources()
        .iter()
        .all(|source| source.endpoint() == Some(server.address)));
    assert_eq!(server.connection_count(), 1);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn public_stats_rejects_more_than_the_compressed_cap_before_inflate() {
    use eltdx::{OperationOptions, StatsQuery, TdxClient};
    let server = hq_server::Server::start(|request| {
        let size = u32::from_le_bytes(request.data[4..8].try_into().unwrap()) as usize;
        let mut payload = (size as u32).to_le_bytes().to_vec();
        payload.resize(4 + size, b'x');
        payload
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let error = client
        .resources()
        .read_stats(
            &StatsQuery::new(),
            &OperationOptions::default().with_timeout(std::time::Duration::from_secs(120)),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert!(error.message().contains("resource byte cap"));
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn public_stats_keeps_unknown_identity_dates_and_values_when_typed_projection_is_unavailable() {
    use eltdx::{OperationOptions, StatsQuery, TdxClient};
    let mut stat = stat_line()
        .split('|')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    stat[0] = "9".to_owned();
    stat[1] = "UNKNOWN".to_owned();
    stat[4] = "20260230".to_owned();
    stat[11] = "1e308".to_owned();
    stat[26] = "-1".to_owned();
    stat[31] = "4294967296".to_owned();
    let mut stat2 = stat2_line()
        .split('|')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    stat2[0] = "9".to_owned();
    stat2[1] = "UNKNOWN".to_owned();
    stat2[3] = "1e308".to_owned();
    let stat = stat.join("|");
    let stat2 = stat2.join("|");
    let bytes = archive(
        &[
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        CompressionMethod::Stored,
    );
    let server = hq_server::Server::start(move |_| {
        let mut output = (bytes.len() as u32).to_le_bytes().to_vec();
        output.extend_from_slice(&bytes);
        output
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let value = client
        .resources()
        .read_stats(&StatsQuery::new(), &OperationOptions::default())
        .unwrap();
    let stat = &value.stat()[0];
    let stat2 = &value.stat2()[0];
    assert!(stat.id().is_none());
    assert_eq!(stat.source_market(), "9");
    assert_eq!(stat.source_code(), "UNKNOWN");
    assert!(stat.stats_date().is_none());
    assert_eq!(stat.stats_date_raw(), Some("20260230"));
    assert!(stat.free_float_shares().is_none());
    assert_eq!(stat.free_float_shares_source_10k(), Some(1e308));
    assert!(stat.year_limit_up_days().is_none());
    assert!(stat.limit_stat_days().is_none());
    assert_eq!(stat.source_counts()[0].as_deref(), Some("-1"));
    assert_eq!(stat.source_counts()[1].as_deref(), Some("4294967296"));
    assert!(stat2.amount().is_none());
    assert_eq!(stat2.source_values()[0], Some(1e308));
    assert!(value
        .meta()
        .diagnostics()
        .iter()
        .any(|value| value.stage() == "source_projection"));
    client.close().unwrap();
    server.finish().unwrap();
}

fn parse(bytes: &[u8]) -> eltdx::Result<stats::ParsedStats> {
    stats::parse_stats_archive(bytes, "tdx://zhb.zip", &mut || Ok(()))
}

#[test]
fn stored_and_deflated_stats_preserve_source_dates_columns_and_native_units() {
    let stat = stat_line();
    let stat2 = stat2_line();
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let bytes = archive(
            &[
                ("tdxstat.cfg", stat.as_bytes()),
                ("tdxstat2.cfg", stat2.as_bytes()),
            ],
            method,
        );
        let parsed = parse(&bytes).expect("two valid independent table dates");
        assert_eq!(parsed.source_path, "tdx://zhb.zip");
        assert_eq!(parsed.stat.len(), 1);
        assert_eq!(parsed.stat2.len(), 1);
        let row = &parsed.stat[0];
        assert_eq!(row.key.market_id.decimal(), "0");
        assert_eq!(row.key.code, "000001");
        assert_eq!(row.stats_date.as_deref(), Some("20260710"));
        assert_eq!(row.beta_60d, Some(1.25));
        assert_eq!(row.pe_ttm, Some(8.75));
        assert_eq!(row.free_float_shares_10k, Some(200.0));
        assert_eq!(
            row.year_limit_up_days
                .as_ref()
                .map(stats::SourceInteger::decimal),
            Some("9")
        );
        assert_eq!(
            row.limit_stat_days
                .as_ref()
                .map(stats::SourceInteger::decimal),
            Some("7")
        );
        assert_eq!(
            row.limit_up_count_in_stat_days
                .as_ref()
                .map(stats::SourceInteger::decimal),
            Some("5")
        );
        assert_eq!(
            row.limit_up_streak_days
                .as_ref()
                .map(stats::SourceInteger::decimal),
            Some("3")
        );
        let row2 = &parsed.stat2[0];
        assert_eq!(row2.stats_date.as_deref(), Some("20260711"));
        assert_eq!(row2.amount_10k, Some(242501.84));
        assert_eq!(row2.seal_amount_10k, None);
        assert_eq!(row2.prev_amount_10k, Some(254664.58));
        assert_eq!(row2.prev_seal_amount_10k, Some(851.8));
        assert_eq!(row2.prev2_amount_10k, Some(134819.47));
        assert_eq!(row2.prev2_seal_amount_10k, Some(3468.47));
        assert_eq!(row2.open_volume_hand, Some(11141.0));
        assert_eq!(row2.prev_open_volume_hand, Some(9513.0));
        assert_eq!(row2.open_amount_10k, Some(6002.06));
        assert_eq!(row2.prev_open_amount_10k, Some(4291.39));
    }
}

#[test]
fn missing_duplicate_empty_and_duplicate_identity_tables_are_rejected() {
    let stat = stat_line();
    let stat2 = stat2_line();
    let doubled = format!("{stat}\n{stat}");
    for entries in [
        vec![("tdxstat.cfg", stat.as_bytes())],
        vec![
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        vec![
            ("tdxstat.cfg", b"short".as_slice()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        vec![
            ("tdxstat.cfg", doubled.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
    ] {
        assert_eq!(
            parse(&archive(&entries, CompressionMethod::Stored))
                .expect_err("invalid table")
                .kind(),
            ErrorKind::ResourceFormat
        );
    }
}

#[test]
fn python_numeric_nulls_truncation_and_unknown_source_identity_are_retained() {
    let line = stat_line();
    let mut parts: Vec<_> = line.split('|').collect();
    parts[0] = "99.75";
    parts[1] = "not-a-security";
    parts[2] = "NaN";
    parts[3] = "-inf";
    parts[11] = "invalid";
    parts[26] = "-2.9";
    parts[31] = "1_000";
    parts[32] = "DIGITS";
    parts[33] = "1e100";
    let mut changed = parts.join("|").into_bytes();
    let digits = changed
        .windows(6)
        .position(|window| window == b"DIGITS")
        .expect("digit marker");
    changed.splice(digits..digits + 6, [0xa3, 0xb9, b'.', 0xa3, 0xb7]);
    let stat2 = stat2_line();
    let parsed = parse(&archive(
        &[
            ("tdxstat.cfg", &changed),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        CompressionMethod::Stored,
    ))
    .expect("source rows retained");
    let row = &parsed.stat[0];
    assert_eq!(row.key.market_id.decimal(), "99");
    assert_eq!(row.key.code, "not-a-security");
    assert_eq!(row.beta_60d, None);
    assert_eq!(row.pe_ttm, None);
    assert_eq!(row.free_float_shares_10k, None);
    assert_eq!(
        row.year_limit_up_days
            .as_ref()
            .map(stats::SourceInteger::decimal),
        Some("-2")
    );
    assert_eq!(
        row.limit_stat_days
            .as_ref()
            .map(stats::SourceInteger::decimal),
        Some("1000")
    );
    assert_eq!(
        row.limit_up_count_in_stat_days
            .as_ref()
            .map(stats::SourceInteger::decimal),
        Some("9")
    );
    assert!(
        row.limit_up_streak_days
            .as_ref()
            .expect("finite unbounded integer")
            .decimal()
            .len()
            > 90
    );
    assert!(parsed
        .diagnostics
        .iter()
        .any(|item| matches!(item.kind, stats::StatsDiagnosticKind::UnknownIdentity)));
}

#[test]
fn gbk_invalid_bytes_are_dropped_without_eating_the_following_ascii_byte() {
    let mut stat = stat_line().into_bytes();
    stat.splice(0..1, [0x80, b'0']);
    let mut stat2 = stat2_line().into_bytes();
    stat2.extend_from_slice(b"\n");
    stat2.extend_from_slice(&[0xb9, 0xab, 0xcb, 0xbe]);
    let parsed = parse(&archive(
        &[("tdxstat.cfg", &stat), ("tdxstat2.cfg", &stat2)],
        CompressionMethod::Deflated,
    ))
    .expect("invalid bytes ignored");
    assert_eq!(parsed.stat[0].key.market_id.decimal(), "0");
    assert!(parsed
        .diagnostics
        .iter()
        .any(|item| matches!(item.kind, stats::StatsDiagnosticKind::LossyGbk)));
}

fn central_start(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("central directory")
}

#[test]
fn encryption_crc_truncation_and_local_central_disagreement_are_rejected() {
    let original = archive(&[("one", b"abcdef")], CompressionMethod::Stored);
    let central = central_start(&original);
    let mut encrypted = original.clone();
    encrypted[6] |= 1;
    encrypted[central + 8] |= 1;
    let mut bad_crc = original.clone();
    bad_crc[30 + 3] ^= 0x40;
    let mut bad_size = original.clone();
    bad_size[22] = 7;
    let mut truncated = original;
    truncated.pop();
    for bytes in [encrypted, bad_crc, bad_size, truncated] {
        let result = archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(()),
        );
        assert_eq!(
            result.expect_err("invalid ZIP").kind(),
            ErrorKind::ResourceFormat
        );
    }
}

#[test]
fn unselected_entries_are_also_crc_checked_and_duplicate_names_rejected() {
    let original = archive(
        &[("one", b"abcdef"), ("ignored", b"bad")],
        CompressionMethod::Stored,
    );
    let mut corrupt = original;
    let position = corrupt
        .windows(3)
        .position(|window| window == b"bad")
        .expect("fixture bytes");
    corrupt[position] ^= 1;
    assert_eq!(
        archive::decode_selected(
            &corrupt,
            archive::ArchiveLimits::stats(),
            |_| false,
            &mut || Ok(())
        )
        .expect_err("ignored bad CRC")
        .kind(),
        ErrorKind::ResourceFormat
    );
    let duplicate = archive(&[("one", b"a"), ("one", b"b")], CompressionMethod::Stored);
    assert_eq!(
        archive::decode_selected(
            &duplicate,
            archive::ArchiveLimits::stats(),
            |_| false,
            &mut || Ok(())
        )
        .expect_err("duplicates")
        .kind(),
        ErrorKind::ResourceFormat
    );
}

#[test]
fn compressed_size_and_claimed_expansion_caps_reject_before_streaming() {
    let oversized = vec![0; 32 * 1024 * 1024 + 1];
    let mut calls = 0;
    let error = archive::decode_selected(
        &oversized,
        archive::ArchiveLimits::stats(),
        |_| {
            calls += 1;
            true
        },
        &mut || Ok(()),
    )
    .expect_err("compressed cap");
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(calls, 0);
    let mut bytes = archive(&[("one", b"a")], CompressionMethod::Deflated);
    let central = central_start(&bytes);
    bytes[central + 24..central + 28].copy_from_slice(&(32_u32 * 1024 * 1024 + 1).to_le_bytes());
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("claimed cap")
        .kind(),
        ErrorKind::LimitExceeded
    );
}

#[test]
fn actual_expansion_is_bounded_even_when_sizes_lie_and_cancellation_is_propagated() {
    let mut bytes = archive(&[("one", &[b'a'; 65536])], CompressionMethod::Deflated);
    let central = central_start(&bytes);
    bytes[22..26].copy_from_slice(&1_u32.to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&1_u32.to_le_bytes());
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("actual length differs")
        .kind(),
        ErrorKind::ResourceFormat
    );
    let bytes = archive(&[("one", &[b'a'; 65536])], CompressionMethod::Deflated);
    let selected = std::cell::Cell::new(false);
    let mut stream_boundaries = 0;
    let result = archive::decode_selected(
        &bytes,
        archive::ArchiveLimits::stats(),
        |_| {
            selected.set(true);
            true
        },
        &mut || {
            if selected.get() {
                stream_boundaries += 1;
            }
            if stream_boundaries >= 3 {
                Err(Error::new(ErrorKind::Cancelled, "fixture cancellation"))
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result.expect_err("cancelled during bounded work").kind(),
        ErrorKind::Cancelled
    );
}

#[test]
fn stats_entry_count_policy_differs_from_financial_policy() {
    let names: Vec<String> = (0..65).map(|index| format!("{index}.dat")).collect();
    let entries: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), b"".as_slice()))
        .collect();
    let bytes = archive(&entries, CompressionMethod::Stored);
    assert!(archive::decode_selected(
        &bytes,
        archive::ArchiveLimits::stats(),
        |_| false,
        &mut || Ok(())
    )
    .is_ok());
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::financial(),
            |_| false,
            &mut || Ok(())
        )
        .expect_err("financial count cap")
        .kind(),
        ErrorKind::LimitExceeded
    );
}

#[test]
fn aggregate_declared_expansion_and_stats_entry_count_are_capped() {
    let mut bytes = archive(
        &[("one", b"a"), ("two", b"b"), ("three", b"c")],
        CompressionMethod::Deflated,
    );
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(at, signature)| (signature == b"PK\x01\x02").then_some(at))
        .collect();
    for position in positions {
        bytes[position + 24..position + 28].copy_from_slice(&(32_u32 * 1024 * 1024).to_le_bytes());
    }
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("aggregate expanded cap")
        .kind(),
        ErrorKind::LimitExceeded
    );
    let names: Vec<String> = (0..129).map(|index| format!("{index}")).collect();
    let entries: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), b"".as_slice()))
        .collect();
    assert_eq!(
        archive::decode_selected(
            &archive(&entries, CompressionMethod::Stored),
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("129 entries")
        .kind(),
        ErrorKind::LimitExceeded
    );
}

#[test]
fn already_cancelled_or_expired_cpu_work_returns_the_original_error() {
    for kind in [ErrorKind::Cancelled, ErrorKind::Timeout] {
        let result = archive::decode_selected(
            b"invalid",
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Err(Error::new(kind, "operation boundary")),
        );
        assert_eq!(result.expect_err("boundary before parsing").kind(), kind);
    }
}

#[test]
fn newline_variants_keep_physical_source_lines_and_skip_malformed_rows() {
    let stat = format!(
        "short\r\n{}\rshort\u{b}{}",
        stat_line(),
        stat_line().replace("000001", "000002")
    );
    let stat2 = stat2_line();
    let parsed = parse(&archive(
        &[
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        CompressionMethod::Stored,
    ))
    .expect("ASCII source rows and skipped malformed line");
    assert_eq!(parsed.stat[0].source_line, 2);
    assert_eq!(parsed.stat[1].source_line, 4);
}

#[test]
fn signed_and_unsigned_descriptors_must_match_the_directory() {
    for signed in [false, true] {
        let mut bytes = archive(&[("one", b"abcdef")], CompressionMethod::Stored);
        let central = central_start(&bytes);
        let crc = bytes[14..18].to_vec();
        bytes[6] |= 8;
        bytes[14..26].fill(0);
        bytes[central + 8] |= 8;
        let mut descriptor = Vec::new();
        if signed {
            descriptor.extend_from_slice(b"PK\x07\x08");
        }
        descriptor.extend_from_slice(&crc);
        descriptor.extend_from_slice(&6_u32.to_le_bytes());
        descriptor.extend_from_slice(&6_u32.to_le_bytes());
        bytes.splice(central..central, descriptor.iter().copied());
        let eocd = bytes.len() - 22;
        bytes[eocd + 16..eocd + 20]
            .copy_from_slice(&((central + descriptor.len()) as u32).to_le_bytes());
        let decoded = archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(()),
        )
        .expect("valid descriptor");
        assert_eq!(decoded[0].contents, b"abcdef");
        let wrong_size = central + if signed { 12 } else { 8 };
        bytes[wrong_size] = 7;
        assert_eq!(
            archive::decode_selected(
                &bytes,
                archive::ArchiveLimits::stats(),
                |_| true,
                &mut || Ok(())
            )
            .expect_err("descriptor mismatch")
            .kind(),
            ErrorKind::ResourceFormat
        );
    }
}

#[test]
fn exact_expanded_entry_cap_is_accepted_without_retaining_unselected_data() {
    let contents = vec![b'a'; 32 * 1024 * 1024];
    let bytes = archive(&[("one", &contents)], CompressionMethod::Deflated);
    let result = archive::decode_selected(
        &bytes,
        archive::ArchiveLimits::stats(),
        |_| false,
        &mut || Ok(()),
    )
    .expect("exact 32 MiB entry");
    assert!(result.is_empty());
}

#[test]
fn damaged_deflate_and_aliasing_local_offsets_do_not_create_successful_members() {
    let mut bytes = archive(&[("one", &[b'a'; 100])], CompressionMethod::Deflated);
    bytes[33] = 0xff; // first compressed byte after 30-byte local header and name
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("damaged deflate")
        .kind(),
        ErrorKind::ResourceFormat
    );
    let mut bytes = archive(&[("one", b"a"), ("two", b"b")], CompressionMethod::Stored);
    let second = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(at, value)| (value == b"PK\x01\x02").then_some(at))
        .nth(1)
        .expect("second central member");
    bytes[second + 42..second + 46].fill(0);
    assert_eq!(
        archive::decode_selected(
            &bytes,
            archive::ArchiveLimits::stats(),
            |_| true,
            &mut || Ok(())
        )
        .expect_err("aliased local offset")
        .kind(),
        ErrorKind::ResourceFormat
    );
}

#[test]
fn capped_diagnostics_preserve_exact_omitted_count() {
    let stat = format!("{}{}", "short\n".repeat(130), stat_line());
    let stat2 = stat2_line();
    let parsed = parse(&archive(
        &[
            ("tdxstat.cfg", stat.as_bytes()),
            ("tdxstat2.cfg", stat2.as_bytes()),
        ],
        CompressionMethod::Stored,
    ))
    .expect("usable row after malformed rows");
    assert_eq!(parsed.diagnostics.len(), 128);
    assert_eq!(parsed.omitted_diagnostics, 2);
    assert_eq!(parsed.stat[0].source_line, 131);
}
