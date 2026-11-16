//! Authored public default-HQ helper source cases. Execution deferred.
use eltdx::{helpers::classify_fund_flow, AmountUnit};
#[path = "support/hq_server.rs"] mod hq_server;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use eltdx::{BatchItemState, CancellationToken, EndEvidence, ErrorKind, OperationOptions,
    PageOptions, RankQuery, RawPolicy, SecurityId, TdxClient};
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
