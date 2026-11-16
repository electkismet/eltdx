//! Authored public default-HQ helper source cases. Execution deferred.
use eltdx::{helpers::classify_fund_flow, AmountUnit};
#[path = "support/hq_server.rs"] mod hq_server;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;
use eltdx::{BatchItemState, CancellationToken, EndEvidence, ErrorKind, OperationOptions,
    Clock, PageOptions, ProfileQuery, QuantityUnit, RankQuery, RatioUnit, RawPolicy, SecurityId, TdxClient};
use eltdx::types::EnrichmentStatus;

#[test]
fn pure_fund_flow_is_available_without_optional_features_or_a_python_runtime() {
    let flow = classify_fund_flow(&[]).unwrap();
    assert_eq!(flow.classified_count(), 0); assert_eq!(flow.skipped_count(), 0);
    assert_eq!(flow.total_net().unit(), AmountUnit::Cny); assert_eq!(flow.total_net().value().as_f64(), 0.0);
}

#[test]
fn helper_getter_empty_input_and_rejected_entry_have_no_startup() {
    let server = hq_server::Server::start(|_| panic!("no command expected")).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
    let helpers = client.helpers();
    assert!(helpers.full_quotes(&[], &OperationOptions::default()).unwrap().is_empty());
    let id = "sh600000".parse().unwrap();
    assert_eq!(helpers.full_quotes(&[id], &OperationOptions::default().with_timeout(Duration::ZERO))
        .unwrap_err().kind(), ErrorKind::Timeout);
    let token = CancellationToken::default(); token.cancel();
    assert_eq!(helpers.full_quotes(&[], &OperationOptions::default().with_cancellation(token))
        .unwrap_err().kind(), ErrorKind::Cancelled);
    assert_eq!(server.connection_count(), 0);
    client.close().unwrap();
    assert_eq!(helpers.full_quotes(&[], &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
    server.finish().unwrap();
}

#[test]
fn full_quotes_batches_eighty_and_joins_reordered_depth_by_identity() {
    let seen = Arc::new(Mutex::new(Vec::new())); let observed = seen.clone();
    let numbers = (0..81).map(|index| format!("{:06}", 600000 + index)).collect::<Vec<_>>();
    let catalog = numbers.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 =>
            hq_server::directory(&catalog.iter().map(|number| (number.as_str(), 2)).collect::<Vec<_>>()),
        0x044d => hq_server::directory(&[]),
        0x054c => {
            let mut codes = hq_server::snapshot_codes(request);
            observed.lock().unwrap().push((0x054c, codes.len())); codes.reverse();
            hq_server::snapshots(&codes.iter().map(|number| (number.as_str(), 100_000)).collect::<Vec<_>>())
        }
        0x0547 => {
            let mut codes = hq_server::refresh_requests(request);
            observed.lock().unwrap().push((0x0547, codes.len())); codes.reverse();
            hq_server::refresh(&codes.iter().map(|(number, _)| (number.as_str(), 93000)).collect::<Vec<_>>())
        }
        _ => panic!("unexpected helper source"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).pool_size(1).heartbeat_interval(None).build().unwrap();
    let ids = numbers.iter().map(|number| format!("sh{number}").parse::<SecurityId>().unwrap()).collect::<Vec<_>>();
    let result = client.helpers().full_quotes(&ids, &OperationOptions::default().with_raw(RawPolicy::RetainPayload)).unwrap();
    assert_eq!(result.len(), 81);
    for (index, (id, item)) in ids.iter().zip(result.items()).enumerate() {
        assert_eq!(item.input_index(), index); assert_eq!(item.security(), id);
        let BatchItemState::Present(full) = item.state() else { panic!("required snapshot missing") };
        assert_eq!(full.snapshot().id(), id); assert_eq!(full.depth_status(), EnrichmentStatus::Succeeded);
        assert_eq!(full.snapshot().bids().len(), 5); assert_eq!(full.snapshot().asks().len(), 5);
        assert_eq!(full.snapshot().meta().source().command(), Some(0x054c));
        assert_eq!(full.depth_source().unwrap().command(), Some(0x0547));
    }
    assert_eq!(*seen.lock().unwrap(), vec![(0x054c, 80), (0x0547, 80), (0x054c, 1), (0x0547, 1)]);
    assert!(result.meta().raw().unwrap().payloads().len() >= 4);
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn optional_depth_protocol_error_degrades_but_cancellation_terminates() {
    for cancel in [false, true] {
        let token = CancellationToken::default(); let source_token = token.clone();
        let server = hq_server::Server::start(move |request| match request.msg_type {
            0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
            0x044d => hq_server::directory(&[]),
            0x054c => hq_server::snapshots(&[("600000", 100_000)]),
            0x0547 => { if cancel { source_token.cancel(); } vec![0x93] },
            _ => panic!("unexpected command"),
        }).unwrap();
        let client = TdxClient::builder().endpoints(vec![server.address]).pool_size(1).heartbeat_interval(None).build().unwrap();
        let result = client.helpers().full_quotes(&["sh600000".parse().unwrap()],
            &OperationOptions::default().with_cancellation(token));
        if cancel {
            assert!(matches!(result.unwrap_err().kind(), ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed));
        } else {
            let result = result.unwrap();
            let BatchItemState::Present(full) = result.items()[0].state() else { panic!("snapshot missing") };
            assert_eq!(full.depth_status(), EnrichmentStatus::Failed); assert_eq!(full.snapshot().bids().len(), 1);
            assert!(full.diagnostics().iter().any(|item| item.error_kind() == Some(ErrorKind::Protocol)));
        }
        client.close().unwrap(); server.finish().unwrap();
    }
}

#[test]
fn rank_query_limits_are_checked_before_io() {
    assert_eq!(RankQuery::new().count(), Some(80));
    assert_eq!(RankQuery::new().pages(), &PageOptions::new(80, 200, 16_000).unwrap());
    assert_eq!(RankQuery::new().with_count(Some(0)).unwrap_err().kind(), ErrorKind::InvalidArgument);
    assert_eq!(RankQuery::new().with_pages(PageOptions::new(81, 1, 100).unwrap()).unwrap_err().kind(), ErrorKind::InvalidArgument);
}

#[test]
fn ranks_preserve_server_order_and_cross_page_duplicates_with_global_numbers() {
    let windows = Arc::new(Mutex::new(Vec::new())); let seen = windows.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2), ("600001", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054b => {
            let window = hq_server::category_window(request); seen.lock().unwrap().push(window);
            match window { (0, 2) => hq_server::category(&["600001", "600000"]),
                (2, 1) => hq_server::category(&["600001"]), _ => panic!("unexpected rank window") }
        }
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
    let query = RankQuery::new().with_count(Some(3)).unwrap().with_pages(PageOptions::new(2, 4, 8).unwrap()).unwrap();
    let table = client.helpers().realtime_rank(&query, &OperationOptions::default()).unwrap();
    assert_eq!(table.end(), Some(EndEvidence::RequestedWindow));
    assert_eq!(table.rows().iter().map(|row| (row.rank(), row.id().number())).collect::<Vec<_>>(),
        vec![(1, "600001"), (2, "600000"), (3, "600001")]);
    assert!(table.rows().iter().all(|row| row.name() == Some("TEST")));
    assert_eq!(*windows.lock().unwrap(), vec![(0, 2), (2, 1)]);
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn complete_rank_requires_empty_eof_and_rejects_an_unproven_page_cap() {
    for maximum in [2, 3] {
        let server = hq_server::Server::start(move |request| match request.msg_type {
            0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
            0x044d => hq_server::directory(&[]),
            0x054b => match hq_server::category_window(request) {
                (0, 2) | (1, 2) => hq_server::category(&["600000"]),
                (2, 2) => hq_server::category(&[]), _ => panic!("unexpected rank window"),
            },
            _ => panic!("unexpected command"),
        }).unwrap();
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
        let query = RankQuery::new().with_count(None).unwrap().with_pages(PageOptions::new(2, maximum, 2).unwrap()).unwrap();
        let result = client.helpers().realtime_rank(&query, &OperationOptions::default());
        if maximum == 2 { assert_eq!(result.unwrap_err().kind(), ErrorKind::LimitExceeded); }
        else { let table = result.unwrap(); assert_eq!(table.rows().len(), 2); assert_eq!(table.end(), Some(EndEvidence::EmptyPage)); }
        client.close().unwrap(); server.finish().unwrap();
    }
}

struct HelperClock(AtomicI64);
impl Clock for HelperClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> { Ok(self.0.load(Ordering::SeqCst)) }
}

#[test]
fn profile_preserves_input_occurrences_finance_dates_units_and_zero_denominator() {
    let server = hq_server::Server::start(|request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2), ("600001", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054c => {
            let codes = hq_server::snapshot_codes(request);
            hq_server::snapshots_with_volume(&codes.iter().map(|code| (code.as_str(), 100_000, 1000)).collect::<Vec<_>>())
        }
        0x0547 => hq_server::refresh(&[]),
        0x0010 => {
            let mut codes = hq_server::finance_codes(request); codes.reverse();
            hq_server::finance(&codes.iter().map(|code| (code.as_str(), if code == "600000" { 0.0 } else { 2.0 },
                4.0, -0.25)).collect::<Vec<_>>())
        }
        _ => panic!("unexpected profile source"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
    let input = ["sh600001", "sh600000", "sh600001"].map(|code| code.parse().unwrap());
    let table = client.helpers().stock_profile_table(&input, &ProfileQuery::new(),
        &OperationOptions::default().with_raw(RawPolicy::RetainPayload)).unwrap();
    assert_eq!(table.rows().iter().map(|row| row.id()).collect::<Vec<_>>(), input.iter().collect::<Vec<_>>());
    for row in table.rows() {
        assert_eq!(row.name(), Some("TEST")); assert!(row.security().is_some()); assert!(row.finance().is_some());
        assert_eq!(row.security_status(), EnrichmentStatus::Succeeded); assert_eq!(row.finance_status(), EnrichmentStatus::Succeeded);
        assert_eq!(row.quote().depth_status(), EnrichmentStatus::Unavailable);
        assert_eq!(row.total_shares().unwrap().unit(), QuantityUnit::Shares);
        assert_eq!(row.total_shares().unwrap().source_unit(), QuantityUnit::TenThousandShares);
        assert_eq!(row.total_shares().unwrap().source_value().as_f64(), 4.0);
        assert_eq!(row.total_shares().unwrap().value().as_f64(), 40_000.0);
        assert_eq!(row.last().unwrap().as_f64(), 10.0);
        assert_eq!(row.total_market_value().unwrap().unit(), AmountUnit::Cny);
        assert_eq!(row.total_market_value().unwrap().value().as_f64(), 400_000.0);
        assert_eq!(row.volume().unwrap().unit(), QuantityUnit::Lots);
        assert_eq!(row.eps().unwrap().as_f64(), -0.25);
        assert_eq!(row.ipo(), Some(eltdx::TradingDate::new(1991, 1, 1).unwrap()));
        assert_eq!(row.updated(), Some(eltdx::TradingDate::new(2026, 11, 16).unwrap()));
        assert_eq!(row.finance().unwrap().province(), 3); assert_eq!(row.finance().unwrap().industry(), 7);
        assert!(row.meta().sources().iter().any(|source| source.command() == Some(0x0010)));
        assert!(row.meta().raw().is_some());
        if row.id().number() == "600000" {
            assert!(row.turnover().is_none());
            assert_eq!(row.circulating_market_value().unwrap().value().as_f64(), 0.0);
            assert!(row.meta().diagnostics().iter().any(|item| item.stage() == "profile.turnover"));
        } else {
            assert_eq!(row.turnover().unwrap().unit(), RatioUnit::PercentagePoints);
            assert_eq!(row.turnover().unwrap().value().as_f64(), 500.0);
            assert_eq!(row.circulating_market_value().unwrap().value().as_f64(), 200_000.0);
        }
    }
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn profile_disabled_supplements_and_empty_inputs_do_not_fetch_optional_sources() {
    let source_calls = Arc::new(AtomicUsize::new(0)); let seen = source_calls.clone();
    let server = hq_server::Server::start(move |request| {
        seen.fetch_add(1, Ordering::SeqCst);
        match request.msg_type {
            0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
            0x044d => hq_server::directory(&[]),
            0x054c => hq_server::snapshots(&[("600000", 100_000)]),
            0x0547 => hq_server::refresh(&[]), _ => panic!("disabled source requested"),
        }
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
    let query = ProfileQuery::new().with_security(false).unwrap().with_finance(false).unwrap();
    assert!(client.helpers().stock_profile_table(&[], &query, &OperationOptions::default()).unwrap().rows().is_empty());
    client.helpers().clear_cache(); assert_eq!(source_calls.load(Ordering::SeqCst), 0);
    let table = client.helpers().stock_profile_table(&["sh600000".parse().unwrap()], &query, &OperationOptions::default()).unwrap();
    let row = &table.rows()[0]; assert_eq!(row.security_status(), EnrichmentStatus::NotRequested);
    assert_eq!(row.finance_status(), EnrichmentStatus::NotRequested); assert!(row.finance().is_none());
    assert!(row.security().is_none()); assert!(row.turnover().is_none()); assert!(row.total_market_value().is_none());
    assert_eq!(source_calls.load(Ordering::SeqCst), 4, "two required precision pages, snapshot, optional depth");
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn failed_finance_is_cached_as_failed_until_expiry_clear_or_raw_policy_change() {
    let finance_calls = Arc::new(AtomicUsize::new(0)); let seen = finance_calls.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054c => hq_server::snapshots(&[("600000", 100_000)]),
        0x0547 => hq_server::refresh(&[]),
        0x0010 => { seen.fetch_add(1, Ordering::SeqCst); vec![1] },
        _ => panic!("unexpected command"),
    }).unwrap();
    let clock = Arc::new(HelperClock(AtomicI64::new(0)));
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).clock(clock.clone()).build().unwrap();
    let query = ProfileQuery::new().with_security(false).unwrap(); let input = ["sh600000".parse().unwrap()];
    let options = OperationOptions::default();
    for (at, expected_calls, hit) in [(0, 1, false), (29, 1, true), (30, 2, false)] {
        clock.0.store(at, Ordering::SeqCst);
        let table = client.helpers().stock_profile_table(&input, &query, &options).unwrap();
        assert_eq!(finance_calls.load(Ordering::SeqCst), expected_calls);
        assert_eq!(table.rows()[0].finance_status(), EnrichmentStatus::Failed); assert!(table.rows()[0].finance().is_none());
        assert!(table.rows()[0].meta().diagnostics().iter().any(|item| item.stage() == "profile.finance"
            && item.status() == EnrichmentStatus::Failed && item.error_kind() == Some(ErrorKind::Protocol)));
        assert!(table.meta().diagnostics().iter().any(|item| item.stage() == "profile.finance_cache"
            && item.message().unwrap_or("").contains(&format!("cache_hit={hit}"))));
    }
    client.clone().helpers().clear_cache();
    client.helpers().stock_profile_table(&input, &query, &options).unwrap();
    assert_eq!(finance_calls.load(Ordering::SeqCst), 3);
    client.helpers().stock_profile_table(&input, &query, &options.with_raw(RawPolicy::RetainPayload)).unwrap();
    assert_eq!(finance_calls.load(Ordering::SeqCst), 4);
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn profile_finance_partial_batches_keep_successes_and_failure_state_in_cache() {
    let numbers = (0..81).map(|index| format!("{:06}", 600000 + index)).collect::<Vec<_>>();
    let catalog = numbers.clone(); let calls = Arc::new(AtomicUsize::new(0)); let seen = calls.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&catalog.iter().map(|code| (code.as_str(), 2)).collect::<Vec<_>>()),
        0x044d => hq_server::directory(&[]),
        0x054c => { let codes = hq_server::snapshot_codes(request);
            hq_server::snapshots(&codes.iter().map(|code| (code.as_str(), 100_000)).collect::<Vec<_>>()) }
        0x0547 => hq_server::refresh(&[]),
        0x0010 => {
            seen.fetch_add(1, Ordering::SeqCst); let codes = hq_server::finance_codes(request);
            if codes.len() == 1 { vec![1] } else { assert_eq!(codes.len(), 80);
                hq_server::finance(&codes.iter().map(|code| (code.as_str(), 2.0, 4.0, 0.25)).collect::<Vec<_>>()) }
        }
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None)
        .clock(Arc::new(HelperClock(AtomicI64::new(0)))).build().unwrap();
    let input = numbers.iter().map(|number| format!("sh{number}").parse().unwrap()).collect::<Vec<_>>();
    for _ in 0..2 {
        let table = client.helpers().stock_profile_table(&input, &ProfileQuery::new().with_security(false).unwrap(),
            &OperationOptions::default()).unwrap();
        assert_eq!(table.rows().len(), 81);
        assert!(table.rows()[..80].iter().all(|row| row.finance_status() == EnrichmentStatus::Succeeded && row.finance().is_some()));
        assert_eq!(table.rows()[80].finance_status(), EnrichmentStatus::Failed); assert!(table.rows()[80].finance().is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn clear_during_a_finance_request_prevents_the_old_result_from_repopulating_cache() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel(); let release_rx = Mutex::new(release_rx);
    let calls = Arc::new(AtomicUsize::new(0)); let seen = calls.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054c => hq_server::snapshots(&[("600000", 100_000)]),
        0x0547 => hq_server::refresh(&[]),
        0x0010 => { if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                entered_tx.send(()).unwrap(); release_rx.lock().unwrap().recv_timeout(Duration::from_secs(2)).unwrap(); }
            hq_server::finance(&[("600000", 2.0, 4.0, 0.25)]) }
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None)
        .clock(Arc::new(HelperClock(AtomicI64::new(0)))).build().unwrap();
    let worker_client = client.clone();
    let worker = std::thread::spawn(move || worker_client.helpers().stock_profile_table(&["sh600000".parse().unwrap()],
        &ProfileQuery::new().with_security(false).unwrap(), &OperationOptions::default()));
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(); client.helpers().clear_cache(); release_tx.send(()).unwrap();
    assert!(worker.join().unwrap().unwrap().rows()[0].finance().is_some());
    client.helpers().stock_profile_table(&["sh600000".parse().unwrap()], &ProfileQuery::new().with_security(false).unwrap(),
        &OperationOptions::default()).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn complete_directory_cache_uses_the_published_version_and_finance_settles_after_version_change() {
    let directory_loads = Arc::new(AtomicUsize::new(0)); let directories = directory_loads.clone();
    let finance_calls = Arc::new(AtomicUsize::new(0)); let finances = finance_calls.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => {
            directories.fetch_add(1, Ordering::SeqCst); hq_server::directory(&[("600000", 2)]) }
        0x044d => hq_server::directory(&[]),
        0x054c => hq_server::snapshots(&[("600000", 100_000)]),
        0x0547 => hq_server::refresh(&[]),
        0x0010 => { finances.fetch_add(1, Ordering::SeqCst); hq_server::finance(&[("600000", 2.0, 4.0, 0.25)]) },
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None)
        .clock(Arc::new(HelperClock(AtomicI64::new(0)))).build().unwrap();
    for _ in 0..3 {
        assert_eq!(client.helpers().stock_profile_table(&["sh600000".parse().unwrap()], &ProfileQuery::new(),
            &OperationOptions::default()).unwrap().rows()[0].name(), Some("TEST"));
    }
    assert_eq!(directory_loads.load(Ordering::SeqCst), 2, "initial precision scan plus one explicit helper catalog scan");
    assert_eq!(finance_calls.load(Ordering::SeqCst), 2, "one conservative reload when the catalog version changes");
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn confirmed_missing_finance_is_distinct_from_source_failure() {
    let server = hq_server::Server::start(|request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054c => hq_server::snapshots(&[("600000", 100_000)]),
        0x0547 => hq_server::refresh(&[]), 0x0010 => hq_server::finance(&[]),
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build().unwrap();
    let table = client.helpers().stock_profile_table(&["sh600000".parse().unwrap()],
        &ProfileQuery::new().with_security(false).unwrap(), &OperationOptions::default()).unwrap();
    let row = &table.rows()[0]; assert!(row.finance().is_none()); assert_eq!(row.finance_status(), EnrichmentStatus::Unavailable);
    assert!(row.meta().diagnostics().iter().any(|item| item.stage() == "profile.finance"
        && item.status() == EnrichmentStatus::Unavailable && item.error_kind().is_none()));
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn cancelled_finance_never_degrades_to_a_profile_or_enters_the_cache() {
    let token = CancellationToken::default(); let cancel = token.clone();
    let calls = Arc::new(AtomicUsize::new(0)); let seen = calls.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
        0x044d => hq_server::directory(&[]),
        0x054c => hq_server::snapshots(&[("600000", 100_000)]),
        0x0547 => hq_server::refresh(&[]),
        0x0010 => { if seen.fetch_add(1, Ordering::SeqCst) == 0 { cancel.cancel(); }
            hq_server::finance(&[("600000", 2.0, 4.0, 0.25)]) }
        _ => panic!("unexpected command"),
    }).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None)
        .clock(Arc::new(HelperClock(AtomicI64::new(0)))).build().unwrap();
    let ids = ["sh600000".parse().unwrap()]; let query = ProfileQuery::new().with_security(false).unwrap();
    let error = client.helpers().stock_profile_table(&ids, &query, &OperationOptions::default().with_cancellation(token)).unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed));
    let table = client.helpers().stock_profile_table(&ids, &query, &OperationOptions::default()).unwrap();
    assert!(table.rows()[0].finance().is_some()); assert_eq!(calls.load(Ordering::SeqCst), 2);
    client.close().unwrap(); server.finish().unwrap();
}
