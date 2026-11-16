//! Authored loopback contracts, deliberately unexecuted during implementation.
use super::{daily_helpers_cases as daily, hq_server};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use eltdx::{CancellationToken, Clock, ErrorKind, LadderQuery, LimitStatus,
    OperationOptions, RawPolicy, SecurityId, SecuritySelection, ShortlineQuery, StatsAlignment,
    StatsQuery, TdxClient, TradingDate};
use eltdx_protocol::frame::RequestFrame;
use eltdx::types::EnrichmentStatus;

struct EpochClock;
impl Clock for EpochClock { fn unix_seconds(&self) -> eltdx::Result<i64> { Ok(0) } }
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder().endpoints(vec![server.address]).pool_size(1).heartbeat_interval(None).clock(Arc::new(EpochClock)).build().unwrap()
}
fn query(count: usize) -> ShortlineQuery { ShortlineQuery::new(vec!["sh600000".parse().unwrap(); count]).unwrap() }
fn session_directory_calendar(request: &RequestFrame) -> Option<Vec<u8>> {
    match request.msg_type {
        0x000d => Some(hq_server::handshake_dates(20261113, 20261116)),
        0x0004 => Some(vec![0; 10]),
        0x044d => Some(if hq_server::directory_offset(request) == 0 && request.data[0] == 1 {
            hq_server::directory(&[("600000", 2), ("600001", 2)]) } else { hq_server::directory(&[]) }),
        0x052d if &request.data[2..8] == b"000001" => Some(if daily::offset(request) == 0 {
            daily::bars(request, &[(20261112, 1000), (20261113, 1000), (20261116, 1000)]) } else { daily::bars(request, &[]) }),
        _ => None,
    }
}
fn usual_source(request: &RequestFrame, stats: &[u8]) -> Vec<u8> {
    if let Some(reply) = session_directory_calendar(request) { return reply; }
    match request.msg_type {
        0x06b9 => daily::file_reply(request, stats),
        0x054c => { let codes = hq_server::snapshot_codes(request);
            hq_server::snapshots(&codes.iter().rev().map(|number| (number.as_str(), 100_000)).collect::<Vec<_>>()) },
        0x0547 => vec![0x93], // Explicit optional depth failure.
        0x0010 => { let codes = hq_server::finance_codes(request);
            hq_server::finance(&codes.iter().rev().map(|number| (number.as_str(), 2.0, 3.0, 0.0)).collect::<Vec<_>>()) },
        0x052d => daily::bars(request, &[(20261113, 10000), (20261112, 10000)]),
        _ => panic!("unexpected shortline command {:04x}", request.msg_type),
    }
}

#[test]
fn shortline_query_and_empty_ladder_are_lazy_and_entry_guards_remain_effective() {
    assert_eq!(ShortlineQuery::new(vec![]).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(LadderQuery::new().selection(), &SecuritySelection::AllAShares);
    assert!(!LadderQuery::new().include_touched()); assert_eq!(LadderQuery::new().count(), None);
    assert_eq!(LadderQuery::new().with_count(Some(0)).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(query(1).with_stats(StatsQuery::new().with_path("custom.zip".to_owned()).unwrap()).unwrap().stats().path(), "custom.zip");
    let server = hq_server::Server::start(|_| panic!("no command expected")).unwrap(); let client = client(&server);
    let empty = LadderQuery::new().with_selection(SecuritySelection::Explicit(vec![])).unwrap();
    assert!(client.helpers().limit_ladder(&empty, &OperationOptions::default()).unwrap().rows().is_empty());
    let token = CancellationToken::default(); token.cancel();
    assert_eq!(client.helpers().shortline_indicators(&query(1), &OperationOptions::default().with_cancellation(token)).unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(client.helpers().limit_ladder(&empty, &OperationOptions::default().with_timeout(Duration::ZERO)).unwrap_err().kind(), ErrorKind::Timeout);
    assert_eq!(server.connection_count(), 0); client.close().unwrap();
    assert_eq!(client.helpers().limit_ladder(&empty, &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    server.finish().unwrap();
}

#[test]
fn shortline_readiness_fails_before_directory_calendar_quotes_or_finance() {
    for invalid_datetime in [false, true] {
        let server = hq_server::Server::start_all(move |request| match request.msg_type {
            0x000d => { let mut reply = hq_server::handshake_dates(20261116, 20261116);
                if invalid_datetime { reply[1..3].fill(0); } else { reply[6] = 9; reply[5] = 24; }
                reply },
            0x0004 => vec![0; 10], _ => panic!("readiness must precede data work"),
        }).unwrap(); let client = client(&server);
        let error = client.helpers().shortline_indicators(&query(1), &OperationOptions::default()).unwrap_err();
        assert_eq!(error.kind(), if invalid_datetime { ErrorKind::MetadataUnavailable } else { ErrorKind::DataNotReady });
        client.close().unwrap(); server.finish().unwrap();
    }
}

#[test]
fn shortline_required_statistics_gate_precedes_quote_and_finance_work() {
    for (first, second) in [("20261112", "20261112"), ("20261113", "20261116")] {
        let stats = daily::stats_zip(first, second);
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_directory_calendar(request) { return reply; }
            assert_eq!(request.msg_type, 0x06b9, "required statistics gate must precede optional business sources");
            daily::file_reply(request, &stats)
        }).unwrap(); let client = client(&server);
        assert_eq!(client.helpers().shortline_indicators(&query(1), &OperationOptions::default()).unwrap_err().kind(), ErrorKind::ResourceFormat);
        client.close().unwrap(); server.finish().unwrap();
    }
}

#[test]
fn small_shortline_preserves_occurrences_server_dates_and_raw_without_category_sweep() {
    let stats = daily::stats_zip("20261113", "20261113");
    let server = hq_server::Server::start_all(move |request| usual_source(request, &stats)).unwrap(); let client = client(&server);
    let query = query(2);
    let table = client.helpers().shortline_indicators(&query, &OperationOptions::default().with_raw(RawPolicy::RetainPayload)).unwrap();
    assert_eq!(table.ids(), query.ids()); assert_eq!(table.rows().len(), 2);
    assert_eq!(table.target(), TradingDate::new(2026, 11, 16).unwrap()); assert_eq!(table.previous(), TradingDate::new(2026, 11, 13).unwrap());
    assert_eq!(table.stats_date(), table.previous()); assert_eq!(table.stats_path(), "zhb.zip"); assert!(table.stats_refreshed());
    for row in table.rows() {
        assert_eq!(row.id(), &query.ids()[0]); assert_eq!(row.alignment(), StatsAlignment::PreviousTradingDay);
        assert_eq!(row.free_float_shares().unwrap().value().as_f64(), 15000.0); assert_eq!(row.float_shares().unwrap().value().as_f64(), 20000.0);
        assert_eq!(row.limit_status(), LimitStatus::UnknownRule); assert!(row.ladder_level().is_none()); assert!(row.open_volume_ratio().is_none());
        assert!(row.opening_rush().is_none()); assert!(row.meta().raw().unwrap().payloads().len() >= 3);
        assert!(row.meta().diagnostics().iter().any(|item| item.stage() == "shortline.opening_rush" && item.status() == EnrichmentStatus::NotRequested));
    }
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn optional_finance_and_bars_failures_keep_primary_but_cancellation_terminates() {
    for cancel in [false, true] {
        let stats = daily::stats_zip("20261116", "20261116"); let token = CancellationToken::default(); let source_token = token.clone();
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_directory_calendar(request) { return reply; }
            if request.msg_type == 0x0010 { if cancel { source_token.cancel(); } return vec![1]; }
            if request.msg_type == 0x052d { return vec![1]; }
            usual_source(request, &stats)
        }).unwrap(); let client = client(&server);
        let result = client.helpers().shortline_indicators(&query(1), &OperationOptions::default().with_cancellation(token));
        if cancel { assert!(matches!(result.unwrap_err().kind(), ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed)); }
        else { let table = result.unwrap(); let row = &table.rows()[0];
            assert!(row.float_shares().is_none()); assert!(row.free_float_shares().is_some()); assert!(row.open_volume_ratio().is_none());
            assert!(row.meta().diagnostics().iter().any(|item| item.stage() == "shortline.recent_bars" && item.status() == EnrichmentStatus::Failed)); }
        client.close().unwrap(); server.finish().unwrap();
    }
}

#[test]
fn eighty_inputs_prepare_all_category_markets_before_first_decode_and_join_supplement() {
    let stats = daily::stats_zip("20261116", "20261116"); let pages = Arc::new(AtomicUsize::new(0)); let observed = pages.clone();
    let server = hq_server::Server::start_all(move |request| {
        if request.msg_type == 0x054b {
            observed.fetch_add(1, Ordering::SeqCst); assert_eq!(hq_server::category_window(request), (0, 80));
            return hq_server::category(&["600000"]);
        }
        usual_source(request, &stats)
    }).unwrap(); let client = client(&server);
    let table = client.helpers().shortline_indicators(&query(80), &OperationOptions::default()).unwrap();
    assert_eq!(table.rows().len(), 80); assert_eq!(pages.load(Ordering::SeqCst), 1);
    assert!(table.rows().iter().all(|row| row.opening_rush().is_some()));
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn category_loop_retains_valid_matches_and_marks_unresolved_supplements_failed() {
    let stats = daily::stats_zip("20261116", "20261116"); let pages = Arc::new(AtomicUsize::new(0)); let observed = pages.clone();
    let server = hq_server::Server::start_all(move |request| {
        if request.msg_type == 0x054b { observed.fetch_add(1, Ordering::SeqCst); return hq_server::category(&["600000"]); }
        usual_source(request, &stats)
    }).unwrap(); let client = client(&server);
    let mut ids: Vec<SecurityId> = vec!["sh600000".parse().unwrap(); 79]; ids.push("sh600001".parse().unwrap());
    let table = client.helpers().shortline_indicators(&ShortlineQuery::new(ids).unwrap(), &OperationOptions::default()).unwrap();
    assert_eq!(pages.load(Ordering::SeqCst), 2); assert!(table.rows()[0].opening_rush().is_some()); assert!(table.rows()[79].opening_rush().is_none());
    assert_eq!(table.rows()[79].alignment(), StatsAlignment::RowMissing);
    assert!(table.rows()[79].meta().diagnostics().iter().any(|item| item.stage() == "shortline.opening_rush" && item.status() == EnrichmentStatus::Failed));
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn unknown_rule_ladder_returns_explicit_unclassified_diagnostic_with_empty_rows() {
    let stats = daily::stats_zip("20261116", "20261116");
    let server = hq_server::Server::start_all(move |request| usual_source(request, &stats)).unwrap(); let client = client(&server);
    let query = LadderQuery::new().with_selection(SecuritySelection::Explicit(vec!["sh600000".parse().unwrap()])).unwrap();
    let table = client.helpers().limit_ladder(&query, &OperationOptions::default()).unwrap();
    assert!(table.rows().is_empty()); assert!(table.meta().diagnostics().iter().any(|item| item.stage() == "ladder.unclassified"
        && item.status() == EnrichmentStatus::Unavailable && item.message().unwrap_or("").contains("1 input occurrences")));
    client.close().unwrap(); server.finish().unwrap();
}
