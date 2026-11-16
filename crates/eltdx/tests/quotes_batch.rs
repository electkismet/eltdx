#[path = "support/hq_server.rs"]
mod hq_server;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eltdx::{BatchItemState, CancellationToken, ErrorKind, MissingPolicy, OperationOptions, RawPolicy, SecurityId, TdxClient};
use hq_server::{directory, directory_offset, snapshots, Server};

fn ids(values: &[&str]) -> Vec<SecurityId> { values.iter().map(|value| value.parse().unwrap()).collect() }

fn catalog(request: &eltdx_protocol::frame::RequestFrame) -> Vec<u8> {
    assert_eq!(request.msg_type, 0x044d);
    if directory_offset(request) == 0 { directory(&[("600000", 2), ("600001", 2)]) } else { directory(&[]) }
}

#[test]
fn complete_reordered_duplicate_occurrences_preserve_distinct_values_with_zero_extra_budget() -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d => catalog(request),
        0x054c => snapshots(&[("600001", 500), ("600000", 1_000), ("600000", 1_200)]),
        _ => panic!("unexpected request"),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).extra_attempt_budget(0).build()?;
    let batch = client.quotes().snapshots(&ids(&["sh600000", "sh600001", "sh600000"]),
        &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
    let prices = batch.items().iter().map(|item| match item.state() {
        BatchItemState::Present(snapshot) => snapshot.last().as_f64(), _ => panic!("complete input unexpectedly missing"),
    }).collect::<Vec<_>>();
    assert_eq!(prices, vec![10.0, 5.0, 12.0]);
    assert_eq!(batch.items().iter().map(|item| item.input_index()).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(batch.meta().raw().unwrap().payloads().len(), 1);
    assert_eq!(batch.meta().raw().unwrap().row_ranges().len(), 3);
    client.close()?;
    server.finish()?;
    Ok(())
}

// COMP-028: each A occurrence has its own successful diagnostic response.
fn recovery_server() -> std::io::Result<(Server, Arc<AtomicUsize>)> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x054c => match observed.fetch_add(1, Ordering::SeqCst) {
            0 => snapshots(&[("600000", 900)]), // advertised partial batch.
            1 => snapshots(&[("600000", 1_000)]),
            2 => snapshots(&[("600000", 1_100)]), // partial [B,A].
            3 => snapshots(&[]),
            4 => snapshots(&[("600000", 1_200)]),
            _ => panic!("unexpected extra recovery request"),
        },
        _ => panic!("unexpected request"),
    })?;
    Ok((server, calls))
}

#[test]
fn confirmed_missing_preserves_all_slots_and_strict_policy_reports_index_and_identity() -> Result<(), Box<dyn std::error::Error>> {
    let input = ids(&["sh600000", "sh600001", "sh600000"]);
    let (server, calls) = recovery_server()?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let result = client.quotes().snapshots_with_policy(&input, MissingPolicy::AllowPartial, &OperationOptions::default())?;
    assert_eq!(result.len(), 3);
    assert!(matches!(result.items()[0].state(), BatchItemState::Present(value) if value.last().as_f64() == 10.0));
    assert!(matches!(result.items()[1].state(), BatchItemState::ConfirmedMissing(_)));
    assert!(matches!(result.items()[2].state(), BatchItemState::Present(value) if value.last().as_f64() == 12.0));
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    client.close()?;
    server.finish()?;
    let (server, _) = recovery_server()?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let error = client.quotes().snapshots(&input, &OperationOptions::default()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::MissingData);
    assert!(error.context().iter().any(|(key, value)| key == "missing.1.security" && value == "sh600001"));
    assert!(error.context().iter().any(|(key, value)| key == "missing.1.input_index" && value == "1"));
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn foreign_singleton_and_corrupted_scalar_are_protocol_without_recovery() -> Result<(), Box<dyn std::error::Error>> {
    for corrupt in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let server = Server::start(move |request| match request.msg_type {
            0x044d => catalog(request),
            0x054c => {
                observed.fetch_add(1, Ordering::SeqCst);
                if corrupt {
                    let mut payload = vec![0, 0, 1, 0, 1];
                    payload.extend_from_slice(b"600000");
                    payload.extend_from_slice(&[0, 0]);
                    payload.extend_from_slice(&[0x80; 11]);
                    payload
                } else { snapshots(&[("600001", 1_000)]) }
            }
            _ => panic!("unexpected request"),
        })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
        let error = client.quotes().snapshots_with_policy(&ids(&["sh600000"]), MissingPolicy::AllowPartial, &OperationOptions::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Protocol);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn excess_duplicate_identity_cannot_be_converted_to_missing_data() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x054c => { observed.fetch_add(1, Ordering::SeqCst); snapshots(&[("600000", 1_000), ("600000", 1_200)]) }
        _ => panic!("unexpected request"),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    assert_eq!(client.quotes().snapshots_with_policy(&ids(&["sh600000", "sh600001"]), MissingPolicy::AllowPartial,
        &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Protocol);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn recovery_stops_before_send_at_shared_or_local_extra_budget() -> Result<(), Box<dyn std::error::Error>> {
    for (input_count, shared, expected_calls) in [(2, 1, 2), (64, 100, 33)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let server = Server::start(move |request| match request.msg_type {
            0x044d => catalog(request),
            0x054c => { observed.fetch_add(1, Ordering::SeqCst); snapshots(&[]) }
            _ => panic!("unexpected request"),
        })?;
        let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).extra_attempt_budget(shared).build()?;
        let input = vec!["sh600000".parse()?; input_count];
        let error = client.quotes().snapshots_with_policy(&input, MissingPolicy::AllowPartial, &OperationOptions::default()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::LimitExceeded);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn cancellation_during_diagnostic_remains_cancelled() -> Result<(), Box<dyn std::error::Error>> {
    let token = CancellationToken::default();
    let cancel = token.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let server = Server::start(move |request| match request.msg_type {
        0x044d => catalog(request),
        0x054c => {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            if index == 1 { cancel.cancel(); }
            snapshots(&[])
        }
        _ => panic!("unexpected request"),
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).build()?;
    let error = client.quotes().snapshots_with_policy(&ids(&["sh600000", "sh600001"]), MissingPolicy::AllowPartial,
        &OperationOptions::default().with_cancellation(token).with_timeout(Duration::from_secs(3))).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    client.close()?;
    server.finish()?;
    Ok(())
}
