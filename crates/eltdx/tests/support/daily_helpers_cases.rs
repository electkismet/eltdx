//! Synthetic public-client contracts. Authored only; no execution yet.
use super::hq_server;
use eltdx::{
    CancellationToken, Clock, DailyLimitsQuery, DailySharesQuery, ErrorKind, LimitRule,
    LimitStatus, OperationOptions, QuantityUnit, RawPolicy, SecuritySelection, StatsQuery,
    TdxClient, TradingDate,
};
use eltdx_protocol::frame::RequestFrame;
use std::io::{Cursor, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

fn date(day: u8) -> TradingDate {
    TradingDate::new(2026, 11, day).unwrap()
}
struct EpochClock;
impl Clock for EpochClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> {
        Ok(0)
    }
}
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .clock(Arc::new(EpochClock))
        .build()
        .unwrap()
}
fn explicit() -> SecuritySelection {
    SecuritySelection::Explicit(vec!["sh600000".parse().unwrap()])
}
pub(super) fn stats_zip(stat_date: &str, stat2_date: &str) -> Vec<u8> {
    let mut stat = [""; 35];
    let mut stat2 = [""; 21];
    for (index, value) in [(0, "1"), (1, "600000"), (4, stat_date), (11, "1.5")] {
        stat[index] = value;
    }
    for (index, value) in [(0, "1"), (1, "600000"), (2, stat2_date)] {
        stat2[index] = value;
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        ("tdxstat.cfg", stat.join("|")),
        ("tdxstat2.cfg", stat2.join("|")),
    ] {
        writer
            .start_file(
                name,
                FileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
pub(super) fn file_reply(request: &RequestFrame, bytes: &[u8]) -> Vec<u8> {
    let offset = u32::from_le_bytes(request.data[..4].try_into().unwrap()) as usize;
    let size = u32::from_le_bytes(request.data[4..8].try_into().unwrap()) as usize;
    let contents = &bytes[offset.min(bytes.len())..offset.saturating_add(size).min(bytes.len())];
    let mut reply = (contents.len() as u32).to_le_bytes().to_vec();
    reply.extend_from_slice(contents);
    reply
}
pub(super) fn offset(request: &RequestFrame) -> u16 {
    u16::from_le_bytes(request.data[12..14].try_into().unwrap())
}
pub(super) fn bars(request: &RequestFrame, rows: &[(u32, i64)]) -> Vec<u8> {
    assert_eq!(
        u16::from_le_bytes(request.data[8..10].try_into().unwrap()),
        4
    );
    assert_eq!(
        u16::from_le_bytes(request.data[16..18].try_into().unwrap()),
        0,
        "must remain unadjusted"
    );
    let mut bytes = (rows.len() as u16).to_le_bytes().to_vec();
    let mut previous = 0;
    for &(day, milli) in rows {
        bytes.extend_from_slice(&day.to_le_bytes());
        hq_server::append_varint(&mut bytes, milli - previous);
        previous = milli;
        bytes.extend_from_slice(&[0; 3]);
        bytes.extend_from_slice(&[0; 8]);
        if &request.data[2..8] == b"000001" {
            bytes.extend_from_slice(&[0; 4]);
        }
    }
    bytes
}
fn session_or_calendar(request: &RequestFrame) -> Option<Vec<u8>> {
    match request.msg_type {
        0x000d => Some(hq_server::handshake_dates(20261113, 20261116)),
        0x0004 => Some(vec![0; 10]),
        0x052d if &request.data[2..8] == b"000001" => Some(if offset(request) == 0 {
            bars(
                request,
                &[(20261112, 1000), (20261113, 1000), (20261116, 1000)],
            )
        } else {
            bars(request, &[])
        }),
        _ => None,
    }
}

#[test]
fn explicit_empty_is_lazy_and_still_honors_closed_cancelled_and_expired_entry() {
    let server = hq_server::Server::start(|_| panic!("explicit empty must not send")).unwrap();
    let client = client(&server);
    let shares = DailySharesQuery::new()
        .with_selection(SecuritySelection::Explicit(vec![]))
        .unwrap();
    let limits = DailyLimitsQuery::new(date(16))
        .with_selection(SecuritySelection::Explicit(vec![]))
        .unwrap();
    assert!(client
        .helpers()
        .daily_share_capital(&shares, &OperationOptions::default())
        .unwrap()
        .rows()
        .is_empty());
    assert!(client
        .helpers()
        .daily_price_limits(&limits, &OperationOptions::default())
        .unwrap()
        .rows()
        .is_empty());
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        client
            .helpers()
            .daily_share_capital(
                &shares,
                &OperationOptions::default().with_cancellation(token)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(
        client
            .helpers()
            .daily_price_limits(
                &limits,
                &OperationOptions::default().with_timeout(std::time::Duration::ZERO)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    assert_eq!(server.connection_count(), 0);
    client.close().unwrap();
    assert_eq!(
        client
            .helpers()
            .daily_share_capital(&shares, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    server.finish().unwrap();
}

#[test]
fn capital_uses_server_target_preserves_three_share_sources_and_reuses_or_refreshes_stats() {
    let files = Arc::new(AtomicUsize::new(0));
    let observed = files.clone();
    let bytes = stats_zip("20261113", "20261113");
    let server = hq_server::Server::start_all(move |request| {
        if let Some(reply) = session_or_calendar(request) {
            return reply;
        }
        match request.msg_type {
            0x06b9 => {
                observed.fetch_add(1, Ordering::SeqCst);
                file_reply(request, &bytes)
            }
            0x0010 => hq_server::finance(&[("600000", 2.0, 3.0, 0.0)]),
            _ => panic!("explicit daily capital must not request a directory or quotes"),
        }
    })
    .unwrap();
    let client = client(&server);
    let query = DailySharesQuery::new().with_selection(explicit()).unwrap();
    let options = OperationOptions::default().with_raw(RawPolicy::RetainPayload);
    for _ in 0..2 {
        let table = client
            .helpers()
            .daily_share_capital(&query, &options)
            .unwrap();
        let row = &table.rows()[0];
        assert_eq!(row.trade_date(), date(16));
        assert_eq!(row.total().unwrap().value().as_f64(), 30000.0);
        assert_eq!(row.circulating().unwrap().value().as_f64(), 20000.0);
        assert_eq!(row.free_float().unwrap().value().as_f64(), 15000.0);
        assert_eq!(row.free_float().unwrap().unit(), QuantityUnit::Shares);
        assert_eq!(
            row.free_float().unwrap().source_unit(),
            QuantityUnit::TenThousandShares
        );
        assert_eq!(row.finance_updated(), Some(date(16)));
        assert_eq!(row.sources(), &["finance_snapshot", "tdx_stats"]);
        assert!(row.meta().raw().unwrap().payloads().len() >= 2);
        assert!(row
            .meta()
            .diagnostics()
            .iter()
            .any(|item| item.message().unwrap_or("").contains("2026-11-13")));
    }
    assert_eq!(files.load(Ordering::SeqCst), 1);
    client
        .helpers()
        .daily_share_capital(&query.clone().with_refresh_stats(true).unwrap(), &options)
        .unwrap();
    assert_eq!(files.load(Ordering::SeqCst), 2);
    client.helpers().clear_cache();
    client
        .helpers()
        .daily_share_capital(&query, &options)
        .unwrap();
    assert_eq!(files.load(Ordering::SeqCst), 3);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn capital_rejects_unaligned_required_resources_before_optional_finance() {
    for (first, second, date_error) in [
        ("20261113", "20261116", false),
        ("20261112", "20261112", true),
    ] {
        let bytes = stats_zip(first, second);
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_or_calendar(request) {
                return reply;
            }
            assert_eq!(
                request.msg_type, 0x06b9,
                "invalid required statistics must precede finance"
            );
            file_reply(request, &bytes)
        })
        .unwrap();
        let client = client(&server);
        let error = client
            .helpers()
            .daily_share_capital(
                &DailySharesQuery::new().with_selection(explicit()).unwrap(),
                &OperationOptions::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ResourceFormat);
        assert_eq!(
            error
                .context()
                .iter()
                .any(|(key, value)| key == "code" && value == "stats_date_mismatch"),
            date_error
        );
        client.close().unwrap();
        server.finish().unwrap();
    }
}

#[test]
fn capital_optional_finance_failure_preserves_free_float_but_cancellation_escapes() {
    for cancel in [false, true] {
        let bytes = stats_zip("20261116", "20261116");
        let token = CancellationToken::default();
        let source_token = token.clone();
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_or_calendar(request) {
                return reply;
            }
            match request.msg_type {
                0x06b9 => file_reply(request, &bytes),
                0x0010 => {
                    if cancel {
                        source_token.cancel();
                    }
                    vec![1]
                }
                _ => panic!("unexpected optional source"),
            }
        })
        .unwrap();
        let client = client(&server);
        let query = DailySharesQuery::new().with_selection(explicit()).unwrap();
        let result = client.helpers().daily_share_capital(
            &query,
            &OperationOptions::default().with_cancellation(token),
        );
        if cancel {
            assert!(matches!(
                result.unwrap_err().kind(),
                ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed
            ));
        } else {
            let table = result.unwrap();
            let row = &table.rows()[0];
            assert!(row.total().is_none());
            assert!(row.circulating().is_none());
            assert!(row.free_float().is_some());
            assert!(row
                .meta()
                .diagnostics()
                .iter()
                .any(|item| item.status() == eltdx::types::EnrichmentStatus::Failed));
        }
        client.close().unwrap();
        server.finish().unwrap();
    }
}

fn capital_changes() -> Vec<u8> {
    let mut payload = 1_u16.to_le_bytes().to_vec();
    payload.push(1);
    payload.extend_from_slice(b"600000");
    payload.extend_from_slice(&2_u16.to_le_bytes());
    for (cash, bonus) in [(10_f32, 10_f32), (5_f32, 0_f32)] {
        payload.push(1);
        payload.extend_from_slice(b"600000");
        payload.push(0);
        payload.extend_from_slice(&20261116_u32.to_le_bytes());
        payload.push(1);
        for value in [cash, 0.0, bonus, 0.0] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

#[test]
fn daily_limits_retains_ordered_reference_with_unknown_rule_and_separate_missing_reference() {
    for missing in [false, true] {
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_or_calendar(request) {
                return reply;
            }
            match request.msg_type {
                0x000f => capital_changes(),
                0x052d => {
                    if offset(request) != 0 || missing {
                        bars(request, &[])
                    } else {
                        bars(
                            request,
                            &[(20261112, 8000), (20261116, 20000), (20261113, 10000)],
                        )
                    }
                }
                _ => panic!(
                    "limits must not substitute a quote, adjusted bar, guessed policy or finance"
                ),
            }
        })
        .unwrap();
        let client = client(&server);
        let table = client
            .helpers()
            .daily_price_limits(
                &DailyLimitsQuery::new(date(16))
                    .with_selection(explicit())
                    .unwrap(),
                &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
            )
            .unwrap();
        let row = &table.rows()[0];
        assert!(matches!(row.rule(), LimitRule::Unknown { .. }));
        assert!(row.upper().is_none());
        assert!(row.lower().is_none());
        if missing {
            assert_eq!(row.status(), LimitStatus::MissingReference);
            assert!(row.reference().is_none());
            assert!(row.reference_date().is_none());
        } else {
            assert_eq!(row.status(), LimitStatus::UnknownRule);
            assert_eq!(row.reference_date(), Some(date(13)));
            assert_eq!(row.reference().unwrap().as_f64(), 4.0);
            assert!(row.meta().raw().unwrap().payloads().len() >= 3);
        }
        client.close().unwrap();
        server.finish().unwrap();
    }
}

#[test]
fn daily_limits_rejects_nontrading_day_before_security_and_reference_work() {
    let server = hq_server::Server::start_all(|request| {
        session_or_calendar(request).expect("calendar gate must run first")
    })
    .unwrap();
    let client = client(&server);
    let error = client
        .helpers()
        .daily_price_limits(
            &DailyLimitsQuery::new(date(14))
                .with_selection(explicit())
                .unwrap(),
            &OperationOptions::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn daily_selection_defaults_are_all_shares_and_stats_queries_preserve_the_requested_source() {
    assert_eq!(
        DailySharesQuery::new().selection(),
        &SecuritySelection::AllAShares
    );
    assert_eq!(
        DailyLimitsQuery::new(date(16)).selection(),
        &SecuritySelection::AllAShares
    );
    let query = DailySharesQuery::new()
        .with_stats(
            StatsQuery::new()
                .with_path("custom.zip".to_owned())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(query.stats().path(), "custom.zip");
    assert!(!query.refresh_stats());
}
