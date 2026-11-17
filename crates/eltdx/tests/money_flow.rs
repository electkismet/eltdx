//! Synthetic identity-header/88-byte records, not an actual 0FFC capture.
#[path = "support/hq_server.rs"]
mod hq_server;
use eltdx::types::{AmountUnit, AuctionQuery, NumericValue, RatioUnit};
use eltdx::{
    BatchItemState, CancellationToken, ErrorKind, OperationOptions, RawPolicy, SecurityId,
    TdxClient,
};
use eltdx_protocol::frame::RequestFrame;
use hq_server::Server;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn id() -> SecurityId {
    "sh600000".parse().unwrap()
}
fn money(request: &RequestFrame, nonempty: bool) -> Vec<u8> {
    assert_eq!(request.msg_type, 0x0ffc);
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(&request.data[..8]);
    if !nonempty {
        return bytes;
    }
    bytes[38..40].copy_from_slice(&1_u16.to_le_bytes());
    let mut record = [0; 88];
    record[..4].copy_from_slice(&2026_0813_u32.to_le_bytes());
    record[8..12].copy_from_slice(&100_000_f32.to_le_bytes());
    for (index, low, high) in [
        (10, 100_u16, 20_u16),
        (11, 300, 100),
        (12, 50, 10),
        (13, 200, 100),
        (14, 60, 10),
        (15, 30, 10),
        (16, 20, 70),
        (17, 70, 20),
    ] {
        let packed = u32::from(low) | (u32::from(high) << 16);
        record[4 + index * 4..8 + index * 4].copy_from_slice(&packed.to_le_bytes());
    }
    bytes.extend_from_slice(&record);
    bytes
}

#[test]
fn daily_preserves_raw_business_fields_exact_float_bits_ratios_and_unknown_amount_unit(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| money(request, true))?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let flow = client
        .money_flow()
        .daily(&id(), &OperationOptions::default())?;
    assert_eq!(flow.id(), &id());
    assert!(flow.meta().raw().is_none());
    let day = &flow.records()[0];
    assert_eq!(day.date_raw(), 2026_0813);
    assert_eq!(day.total_amount().value(), NumericValue::F32(100_000.0));
    assert_eq!(day.total_amount().unit(), AmountUnit::Unknown);
    assert_eq!(day.raw_values()[1], 100_000_f32.to_bits());
    assert_eq!(day.buckets()[0..4], [100, 20, 300, 100]);
    assert!((day.main_net().value().as_f64() - 240.0).abs() < 1e-9);
    assert!((day.main_buy_net().value().as_f64() - 740.0).abs() < 1e-9);
    assert_eq!(day.main_ratio().unit(), RatioUnit::PercentagePoints);
    assert!((day.main_ratio().value().as_f64() - 0.24).abs() < 1e-9);
    assert_eq!(
        day.main_size_net().map(|amount| amount.value().as_f64()),
        [160.0, 80.0, 100.0, -100.0]
    );
    let retained = client.money_flow().daily(
        &id(),
        &OperationOptions::default().with_raw(RawPolicy::RetainWire),
    )?;
    assert_eq!(retained.meta().raw().unwrap().row_ranges(), [(0, 40, 88)]);
    assert_eq!(
        retained.meta().raw().unwrap().wire_frames().unwrap().len(),
        1
    );
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn empty_first_security_cannot_suppress_another_securitys_alternate_source(
) -> Result<(), Box<dyn std::error::Error>> {
    let first = Server::start(|request| money(request, false))?;
    let second = Server::start(|request| money(request, false))?;
    let third = Server::start(|request| money(request, &request.data[2..8] == b"600001"))?;
    let fourth = Server::start(|request| money(request, false))?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![
            first.address,
            second.address,
            third.address,
            fourth.address,
        ])
        .heartbeat_interval(None)
        .request_timeout(Duration::from_secs(30))
        .build()?;
    let ids = [id(), "sh600001".parse()?];
    let batch = client
        .money_flow()
        .daily_many(&ids, &OperationOptions::default())?;
    match batch.items()[0].state() {
        BatchItemState::Present(flow) => assert!(flow.records().is_empty()),
        _ => panic!("empty is present"),
    }
    match batch.items()[1].state() {
        BatchItemState::Present(flow) => assert_eq!(flow.records().len(), 1),
        _ => panic!("second security has data"),
    }
    assert_eq!(batch.items()[1].input_index(), 1);
    client.close()?;
    first.finish()?;
    second.finish()?;
    third.finish()?;
    fourth.finish()?;
    Ok(())
}

#[test]
fn per_code_three_batch_twenty_four_and_duplicate_occurrence_scopes_are_bounded(
) -> Result<(), Box<dyn std::error::Error>> {
    for duplicate in [false, true] {
        let probes = Arc::new(AtomicUsize::new(0));
        let primary = Server::start(|request| money(request, false))?;
        let mut alternates = Vec::new();
        for _ in 0..3 {
            let observed = Arc::clone(&probes);
            alternates.push(Server::start(move |request| {
                observed.fetch_add(1, Ordering::SeqCst);
                money(request, false)
            })?);
        }
        let mut endpoints = vec![primary.address];
        endpoints.extend(alternates.iter().map(|server| server.address));
        let client = TdxClient::builder()
            .money_flow_endpoints(endpoints)
            .heartbeat_interval(None)
            .request_timeout(Duration::from_secs(30))
            .build()?;
        let ids = if duplicate {
            vec![id(), id()]
        } else {
            (0..9)
                .map(|index| format!("sh{:06}", 600_000 + index).parse())
                .collect::<eltdx::Result<Vec<_>>>()?
        };
        let batch = client
            .money_flow()
            .daily_many(&ids, &OperationOptions::default())?;
        assert_eq!(batch.len(), ids.len());
        assert_eq!(
            probes.load(Ordering::SeqCst),
            if duplicate { 3 } else { 24 }
        );
        assert!(batch.items().iter().all(|item| matches!(item.state(), BatchItemState::Present(flow) if flow.records().is_empty())));
        client.close()?;
        primary.finish()?;
        for server in alternates {
            server.finish()?;
        }
    }
    Ok(())
}

#[test]
fn global_extra_budget_is_tighter_than_domain_probe_caps() -> Result<(), Box<dyn std::error::Error>>
{
    let primary = Server::start(|request| money(request, false))?;
    let probes = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&probes);
    let second = Server::start(move |request| {
        observed.fetch_add(1, Ordering::SeqCst);
        money(request, false)
    })?;
    let third = Server::start(|request| money(request, false))?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![primary.address, second.address, third.address])
        .heartbeat_interval(None)
        .extra_attempt_budget(1)
        .build()?;
    assert_eq!(
        client
            .money_flow()
            .daily(&id(), &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    assert_eq!(probes.load(Ordering::SeqCst), 1);
    client.close()?;
    primary.finish()?;
    second.finish()?;
    third.finish()?;
    Ok(())
}

#[test]
fn malformed_empty_payload_identity_nonfinite_and_duplicate_blocks_do_not_become_empty(
) -> Result<(), Box<dyn std::error::Error>> {
    for case in 0..7 {
        let server = Server::start(move |request| {
            let mut bytes = money(request, true);
            match case {
                0 => return vec![],
                1 => bytes[0] = 250,
                2 => bytes[2..8].copy_from_slice(b"600001"),
                3 => bytes[48..52].copy_from_slice(&f32::NAN.to_le_bytes()),
                4 => {
                    let copy = bytes.clone();
                    bytes.extend_from_slice(&copy);
                }
                5 => {
                    bytes.pop();
                }
                _ => bytes.push(0),
            }
            bytes
        })?;
        let client = TdxClient::builder()
            .money_flow_endpoints(vec![server.address])
            .heartbeat_interval(None)
            .build()?;
        assert_eq!(
            client
                .money_flow()
                .daily(&id(), &OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        client.close()?;
        server.finish()?;
    }
    Ok(())
}

#[test]
fn unavailable_record_date_preserves_scalar_and_diagnostic(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| {
        let mut bytes = money(request, true);
        bytes[40..44].copy_from_slice(&2026_0230_u32.to_le_bytes());
        bytes
    })?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let value = client
        .money_flow()
        .daily(&id(), &OperationOptions::default())?;
    assert_eq!(value.records()[0].date(), None);
    assert_eq!(value.records()[0].date_raw(), 2026_0230);
    assert!(value
        .meta()
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.stage() == "record_date"));
    client.close()?;
    server.finish()?;
    Ok(())
}

#[test]
fn dedicated_pools_are_distinct_on_same_address_and_getters_reuse_them(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x0ffc => money(request, true),
        0x056a => vec![0, 0],
        _ => panic!("unexpected command"),
    })?;
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .auction_endpoints(vec![server.address])
        .money_flow_endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let service = client.money_flow();
    assert_eq!(server.connection_count(), 0);
    client.connect(&OperationOptions::default())?;
    client
        .auctions()
        .series(&id(), &AuctionQuery::new(), &OperationOptions::default())?;
    service.daily(&id(), &OperationOptions::default())?;
    client
        .money_flow()
        .daily(&id(), &OperationOptions::default())?;
    assert_eq!(server.connection_count(), 3);
    client.close()?;
    assert_eq!(
        service
            .daily(&id(), &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    server.finish()?;
    Ok(())
}

#[test]
fn empty_batches_and_cancelled_probe_never_return_partial_success(
) -> Result<(), Box<dyn std::error::Error>> {
    let client = TdxClient::builder()
        .money_flow_endpoints(vec!["127.0.0.1:1".parse()?])
        .build()?;
    assert!(client
        .money_flow()
        .daily_many(&[], &OperationOptions::default())?
        .is_empty());
    client.close()?;
    let token = CancellationToken::default();
    let worker_token = token.clone();
    let primary = Server::start(|request| money(request, false))?;
    let alternate = Server::start(move |request| {
        worker_token.cancel();
        money(request, true)
    })?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![primary.address, alternate.address])
        .heartbeat_interval(None)
        .build()?;
    assert_eq!(
        client
            .money_flow()
            .daily(&id(), &OperationOptions::default().with_cancellation(token))
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    client.close()?;
    primary.finish()?;
    alternate.finish()?;
    Ok(())
}

#[test]
fn network_failed_probe_keeps_failure_instead_of_returning_empty(
) -> Result<(), Box<dyn std::error::Error>> {
    let primary = Server::start(|request| money(request, false))?;
    let reservation = std::net::TcpListener::bind("127.0.0.1:0")?;
    let unavailable = reservation.local_addr()?;
    drop(reservation);
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![primary.address, unavailable])
        .heartbeat_interval(None)
        .build()?;
    let error = client
        .money_flow()
        .daily(&id(), &OperationOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Connection);
    client.close()?;
    primary.finish()?;
    Ok(())
}

#[test]
fn probe_admission_is_cancelable_while_another_operation_is_active(
) -> Result<(), Box<dyn std::error::Error>> {
    let started = Arc::new(Mutex::new(None));
    let observed = Arc::clone(&started);
    let server = Server::start(move |request| {
        *observed.lock().unwrap() = Some(());
        std::thread::sleep(Duration::from_millis(50));
        money(request, true)
    })?;
    let client = TdxClient::builder()
        .money_flow_endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()?;
    let worker = client.clone();
    let task = std::thread::spawn(move || {
        worker
            .money_flow()
            .daily(&id(), &OperationOptions::default())
    });
    while started.lock().unwrap().is_none() {
        std::thread::yield_now();
    }
    let token = CancellationToken::default();
    let canceller = token.clone();
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        canceller.cancel();
    });
    assert_eq!(
        client
            .money_flow()
            .daily(&id(), &OperationOptions::default().with_cancellation(token))
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    cancel.join().unwrap();
    task.join().unwrap()?;
    client.close()?;
    server.finish()?;
    Ok(())
}
