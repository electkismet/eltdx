#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Policy and deterministic loopback subscription source cases. These are
//! synthetic fixtures, authored without executing the test target.
use eltdx::types::RefreshRequest;
use eltdx::{
    CancellationToken, ErrorKind, GapPolicy, OperationOptions, PushEvent, SecurityId,
    SubscriptionOptions,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[path = "support/hq_server.rs"]
mod hq_server;
use hq_server::Server;

#[test]
fn subscription_policy_is_independent_of_setup_deadlines_and_has_frozen_defaults() {
    let policy = SubscriptionOptions::default();
    assert_eq!(policy.max_events(), 1024);
    assert_eq!(policy.max_bytes(), 64 * 1024 * 1024);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
    assert_eq!(policy.gap_policy(), GapPolicy::ReportOnly);
    let changed = policy
        .clone()
        .with_queue_limits(2, 512 * 1024)
        .with_renew_interval(None)
        .with_gap_policy(GapPolicy::Resynchronize);
    assert_eq!(changed.max_events(), 2);
    assert_eq!(changed.renew_interval(), None);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
}

#[test]
fn refresh_requests_retain_source_local_cursors_without_monotonic_reinterpretation() {
    let mut request = RefreshRequest::new("sh600000".parse().unwrap());
    assert_eq!(request.cursor(), 0);
    request = request
        .with_cursor(150000)
        .unwrap()
        .with_cursor(93000)
        .unwrap();
    assert_eq!(request.cursor(), 93000);
    assert_eq!(request.id().to_string(), "sh600000");
}

#[test]
fn pinned_nonempty_refresh_preserves_identity_cursor_raw_and_releases_its_connection(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::start(|request| match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => {
            hq_server::directory(&[("600000", 2)])
        }
        0x044d => hq_server::directory(&[]),
        0x0547 => {
            assert_eq!(
                hq_server::refresh_requests(request),
                vec![("600000".to_owned(), 93000)]
            );
            hq_server::refresh(&[("600000", 93001)])
        }
        _ => panic!("unexpected pinned quote command"),
    })?;
    let client = eltdx::TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()?;
    let options = OperationOptions::default().with_raw(eltdx::RawPolicy::RetainWire);
    let id = SecurityId::new(eltdx::HqMarket::Shanghai, "600000")?;
    let mut pin = client.pin(&options)?;
    let request = RefreshRequest::new(id.clone()).with_cursor(93000)?;
    let result = pin.quotes().refresh(&[request], &options);
    let pin_closed = pin.close();
    let closed = client.close();
    let joined = server.finish();
    let page = result?;
    pin_closed?;
    closed?;
    joined?;
    assert_eq!(page.requested().len(), 1);
    assert_eq!(&page.requested()[0], &id);
    assert_eq!(page.updates().len(), 1);
    assert_eq!(page.updates()[0].id(), &id);
    assert_eq!(page.updates()[0].cursor(), 93001);
    let raw = page.meta().raw().ok_or("missing pinned response raw")?;
    assert_eq!(raw.row_ranges().len(), 1);
    assert_eq!(raw.payloads().len(), 1);
    assert_eq!(raw.wire_frames().ok_or("missing pinned wire")?.len(), 1);
    Ok(())
}

#[test]
fn cold_metadata_refresh_with_the_only_slot_pinned_uses_that_connection() {
    use std::sync::{Arc, Mutex};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&seen);
    let server = Server::start(move |request| {
        observed
            .lock()
            .unwrap()
            .push((request.msg_type, request.data.clone()));
        match request.msg_type {
            0x044d if hq_server::directory_offset(request) == 0 => {
                hq_server::directory(&[("600000", 2)])
            }
            0x044d => hq_server::directory(&[]),
            0x0547 => vec![0x93, 0x93],
            _ => panic!("unexpected command"),
        }
    })
    .unwrap();
    let client = eltdx::ClientBuilder::new()
        .pool_size(1)
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let options = eltdx::OperationOptions::default();
    let mut pin = client.pin(&options).unwrap();
    let id = "sh600000".parse().unwrap();
    let request = RefreshRequest::new(id).with_cursor(93000).unwrap();
    let page = pin.quotes().refresh(&[request], &options).unwrap();
    assert_eq!(page.requested().len(), 1);
    assert!(page.updates().is_empty());
    let poll = pin.quotes().refresh(&[], &options).unwrap();
    assert_eq!(poll.requested(), page.requested());
    assert!(poll.updates().is_empty());
    let operations = seen.lock().unwrap();
    assert_eq!(
        operations
            .iter()
            .map(|(command, _)| *command)
            .collect::<Vec<_>>(),
        vec![0x044d, 0x044d, 0x0547, 0x0547]
    );
    let empty = &operations.last().unwrap().1;
    assert_eq!(
        empty.as_ref(),
        &[0, 0],
        "empty polling is a real wire operation"
    );
    assert_eq!(server.connection_count(), 1);
    drop(operations);
    pin.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn empty_refresh_on_a_new_pin_does_not_load_catalogs_and_over_limit_is_no_io() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let refreshes = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&refreshes);
    let server = Server::start(move |request| {
        assert_eq!(request.msg_type, 0x0547);
        assert_eq!(request.data.as_ref(), &[0, 0]);
        count.fetch_add(1, Ordering::SeqCst);
        vec![0x93, 0x93]
    })
    .unwrap();
    let client = eltdx::ClientBuilder::new()
        .pool_size(1)
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let options = eltdx::OperationOptions::default();
    let mut pin = client.pin(&options).unwrap();
    assert!(pin
        .quotes()
        .refresh(&[], &options)
        .unwrap()
        .updates()
        .is_empty());
    let too_many = vec![RefreshRequest::new("sh600000".parse().unwrap()); 101];
    assert_eq!(
        pin.quotes()
            .refresh(&too_many, &options)
            .unwrap_err()
            .kind(),
        eltdx::ErrorKind::InvalidArgument
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    pin.close().unwrap();
    assert_eq!(
        pin.quotes().refresh(&[], &options).unwrap_err().kind(),
        eltdx::ErrorKind::PinLost
    );
    client.close().unwrap();
    server.finish().unwrap();
}

fn ids(count: usize) -> Vec<SecurityId> {
    (0..count)
        .map(|index| format!("sh{:06}", 600000 + index).parse().unwrap())
        .collect()
}

fn ordinary_reply(request: &eltdx_protocol::frame::RequestFrame, count: usize) -> Vec<u8> {
    match request.msg_type {
        0x044d if hq_server::directory_offset(request) == 0 => {
            let numbers = (0..count)
                .map(|index| format!("{:06}", 600000 + index))
                .collect::<Vec<_>>();
            hq_server::directory(
                &numbers
                    .iter()
                    .map(|number| (number.as_str(), 2))
                    .collect::<Vec<_>>(),
            )
        }
        0x044d => hq_server::directory(&[]),
        0x0547 => hq_server::refresh(&[]),
        0x054c => {
            let numbers = hq_server::snapshot_codes(request);
            hq_server::snapshots(
                &numbers
                    .iter()
                    .map(|number| (number.as_str(), 1000))
                    .collect::<Vec<_>>(),
            )
        }
        _ => panic!("unexpected subscription fixture command"),
    }
}

fn client(server: &Server, pool: usize) -> eltdx::TdxClient {
    eltdx::ClientBuilder::new()
        .pool_size(pool)
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .request_timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn policy() -> SubscriptionOptions {
    SubscriptionOptions::default().with_renew_interval(None)
}

fn take_initial(subscription: &mut eltdx::Subscription) -> eltdx::SnapshotBatch {
    match subscription
        .next_event(&OperationOptions::default())
        .unwrap()
        .unwrap()
    {
        PushEvent::Initial(batch) => batch,
        _ => panic!("first event must contain the establishment snapshot"),
    }
}

#[test]
fn impossible_capacity_and_invalid_policies_fail_before_any_connection() {
    let server = Server::start(|_| panic!("preflight must not reach the wire")).unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    assert_eq!(
        client
            .quotes()
            .subscribe(&ids(101), &policy(), &options)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        client
            .quotes()
            .subscribe(&[], &policy(), &options)
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    for invalid in [
        policy().with_queue_limits(0, 1024),
        policy().with_queue_limits(2, 1),
        policy().with_renew_interval(Some(Duration::ZERO)),
    ] {
        assert_eq!(
            client
                .quotes()
                .subscribe(&ids(1), &invalid, &options)
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidArgument
        );
    }
    assert_eq!(server.connection_count(), 0);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn cold_catalog_before_two_group_pins_preserves_order_and_releases_both() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let captured = observed.clone();
    let server = Server::start(move |request| {
        if request.msg_type == 0x0547 {
            captured
                .lock()
                .unwrap()
                .push(hq_server::refresh_requests(request).len());
        }
        ordinary_reply(request, 101)
    })
    .unwrap();
    let client = client(&server, 2);
    let options = OperationOptions::default();
    let mut inputs = ids(101);
    inputs.push(inputs[0].clone());
    let mut subscription = client
        .quotes()
        .subscribe(&inputs, &policy(), &options)
        .unwrap();
    let initial = take_initial(&mut subscription);
    assert_eq!(initial.len(), 101);
    assert_eq!(
        initial
            .items()
            .iter()
            .map(|item| item.security().clone())
            .collect::<Vec<_>>(),
        ids(101)
    );
    assert_eq!(*observed.lock().unwrap(), vec![100, 1]);
    subscription.close().unwrap();
    let mut first = client.pin(&options).unwrap();
    let mut second = client.pin(&options).unwrap();
    first.close().unwrap();
    second.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn failure_of_second_activation_rolls_back_the_failing_group_as_well() {
    let activations = Arc::new(AtomicUsize::new(0));
    let count = activations.clone();
    let server = Server::start(move |request| {
        if request.msg_type == 0x0547 && count.fetch_add(1, Ordering::SeqCst) == 1 {
            return vec![0x93];
        }
        ordinary_reply(request, 101)
    })
    .unwrap();
    let client = client(&server, 2);
    let options = OperationOptions::default();
    assert_eq!(
        client
            .quotes()
            .subscribe(&ids(101), &policy(), &options)
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    assert_eq!(activations.load(Ordering::SeqCst), 2);
    let mut first = client.pin(&options).unwrap();
    let mut second = client.pin(&options).unwrap();
    first.close().unwrap();
    second.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn occupied_pool_capacity_has_bounded_acquisition_and_rolls_back_its_first_group() {
    let activations = Arc::new(AtomicUsize::new(0));
    let count = activations.clone();
    let server = Server::start(move |request| {
        if request.msg_type == 0x0547 {
            count.fetch_add(1, Ordering::SeqCst);
        }
        ordinary_reply(request, 101)
    })
    .unwrap();
    let client = client(&server, 2);
    let options = OperationOptions::default();
    client.quotes().snapshots(&ids(1), &options).unwrap(); // Warm the whole fixture catalog.
    let mut occupied = client.pin(&options).unwrap();
    let bounded = options.clone().with_timeout(Duration::from_secs(1));
    assert_eq!(
        client
            .quotes()
            .subscribe(&ids(101), &policy(), &bounded)
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    assert_eq!(activations.load(Ordering::SeqCst), 1);
    let mut remaining = client.pin(&options).unwrap();
    occupied.close().unwrap();
    remaining.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn overlapping_subscriptions_keep_separate_source_routes_and_independent_close() {
    let (source_tx, source_rx) = mpsc::channel();
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 {
            source_tx.send(connection.clone()).unwrap();
        }
        ordinary_reply(request, 1)
    })
    .unwrap();
    let client = client(&server, 2);
    let options = OperationOptions::default();
    let mut first = client
        .quotes()
        .subscribe(&ids(1), &policy(), &options)
        .unwrap();
    let first_source = source_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut second = client
        .quotes()
        .subscribe(&ids(1), &policy(), &options)
        .unwrap();
    let second_source = source_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    take_initial(&mut first);
    take_initial(&mut second);
    first_source
        .push(0x0547, &hq_server::refresh(&[("600000", 93001)]))
        .unwrap();
    assert!(
        matches!(first.next_event(&options).unwrap(), Some(PushEvent::Update(row)) if row.cursor() == 93001)
    );
    assert_eq!(
        second
            .next_event(&options.clone().with_timeout(Duration::ZERO))
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    first.close().unwrap();
    second_source
        .push(0x0547, &hq_server::refresh(&[("600000", 93002)]))
        .unwrap();
    assert!(
        matches!(second.next_event(&options).unwrap(), Some(PushEvent::Update(row)) if row.cursor() == 93002)
    );
    second.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn read_cancellation_keeps_initial_and_setup_cancellation_does_not_stop_the_lease() {
    let (source_tx, source_rx) = mpsc::channel();
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 {
            source_tx.send(connection.clone()).unwrap();
        }
        ordinary_reply(request, 1)
    })
    .unwrap();
    let client = client(&server, 1);
    let setup_token = CancellationToken::default();
    let setup = OperationOptions::default().with_cancellation(setup_token.clone());
    let mut subscription = client
        .quotes()
        .subscribe(&ids(1), &policy(), &setup)
        .unwrap();
    let connection = source_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    setup_token.cancel();
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let read = OperationOptions::default().with_cancellation(cancelled);
    assert_eq!(
        subscription.next_event(&read).unwrap_err().kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(take_initial(&mut subscription).len(), 1);
    assert_eq!(
        subscription
            .next_event(&OperationOptions::default().with_timeout(Duration::ZERO))
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    connection
        .push(0x0547, &hq_server::refresh(&[("600000", 93000)]))
        .unwrap();
    assert!(matches!(
        subscription
            .next_event(&OperationOptions::default())
            .unwrap(),
        Some(PushEvent::Update(_))
    ));
    subscription.close().unwrap();
    assert!(subscription.next_event(&read).unwrap().is_none());
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn count_overflow_keeps_the_gap_reachable_and_marks_retained_updates() {
    let (source_tx, source_rx) = mpsc::channel();
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 {
            source_tx.send(connection.clone()).unwrap();
        }
        ordinary_reply(request, 3)
    })
    .unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(3),
            &policy().with_queue_limits(2, 8 * 1024 * 1024),
            &options,
        )
        .unwrap();
    take_initial(&mut subscription);
    source_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .push(
            0x0547,
            &hq_server::refresh(&[("600000", 93001), ("600001", 93002), ("600002", 93003)]),
        )
        .unwrap();
    assert!(
        matches!(subscription.next_event(&options).unwrap(), Some(PushEvent::Gap(gap)) if gap.dropped() == 1)
    );
    for cursor in [93002, 93003] {
        assert!(
            matches!(subscription.next_event(&options).unwrap(), Some(PushEvent::Update(row))
            if row.cursor() == cursor && row.desynchronized())
        );
    }
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn drop_retires_a_pin_without_another_read_and_parent_close_overrides_pending_data() {
    let server = Server::start(|request| ordinary_reply(request, 1)).unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let subscription = client
        .quotes()
        .subscribe(&ids(1), &policy(), &options)
        .unwrap();
    drop(subscription);
    let mut pin = client.pin(&options).unwrap();
    pin.close().unwrap();
    let mut subscription = client
        .quotes()
        .subscribe(&ids(1), &policy(), &options)
        .unwrap();
    client.close().unwrap();
    assert_eq!(
        subscription.next_event(&options).unwrap_err().kind(),
        ErrorKind::Closed
    );
    subscription.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn lost_source_reports_gap_and_failure_instead_of_rebinding_the_subscription() {
    let (source_tx, source_rx) = mpsc::channel();
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 {
            source_tx.send(connection.clone()).unwrap();
        }
        ordinary_reply(request, 1)
    })
    .unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(&ids(1), &policy(), &options)
        .unwrap();
    take_initial(&mut subscription);
    source_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .disconnect()
        .unwrap();
    assert!(
        matches!(subscription.next_event(&options).unwrap(), Some(PushEvent::Gap(gap))
        if gap.reason() == eltdx::types::GapReason::SourceLost)
    );
    assert_eq!(
        subscription.next_event(&options).unwrap_err().kind(),
        ErrorKind::PinLost
    );
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn renewal_uses_the_new_morning_cursor_after_setup_cancellation() {
    let (source_tx, source_rx) = mpsc::channel();
    let (cursor_tx, cursor_rx) = mpsc::channel();
    let activations = AtomicUsize::new(0);
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 {
            if activations.fetch_add(1, Ordering::SeqCst) == 0 {
                source_tx.send(connection.clone()).unwrap();
                return hq_server::refresh(&[("600000", 150000)]);
            }
            cursor_tx
                .send(hq_server::refresh_requests(request)[0].1)
                .unwrap();
        }
        ordinary_reply(request, 1)
    })
    .unwrap();
    let client = client(&server, 1);
    let token = CancellationToken::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(1),
            &policy().with_renew_interval(Some(Duration::from_millis(20))),
            &OperationOptions::default().with_cancellation(token.clone()),
        )
        .unwrap();
    take_initial(&mut subscription);
    token.cancel();
    source_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .push(0x0547, &hq_server::refresh(&[("600000", 93000)]))
        .unwrap();
    assert!(
        matches!(subscription.next_event(&OperationOptions::default()).unwrap(),
        Some(PushEvent::Update(row)) if row.cursor() == 93000)
    );
    let until = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let remaining = until.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "renewal must use the latest lower source cursor"
        );
        if cursor_rx.recv_timeout(remaining).unwrap() == 93000 {
            break;
        }
    }
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn successful_recovery_emits_a_snapshot_then_continuous_updates_on_the_same_source() {
    let (source_tx, source_rx) = mpsc::channel();
    let activations = AtomicUsize::new(0);
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 && activations.fetch_add(1, Ordering::SeqCst) == 0 {
            source_tx.send(connection.clone()).unwrap();
        }
        ordinary_reply(request, 3)
    })
    .unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(3),
            &policy()
                .with_queue_limits(2, 8 * 1024 * 1024)
                .with_gap_policy(GapPolicy::Resynchronize),
            &options,
        )
        .unwrap();
    take_initial(&mut subscription);
    let connection = source_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    connection
        .push(
            0x0547,
            &hq_server::refresh(&[("600000", 93001), ("600001", 93002), ("600002", 93003)]),
        )
        .unwrap();
    assert!(matches!(
        subscription.next_event(&options).unwrap(),
        Some(PushEvent::Gap(_))
    ));
    let mut restored = false;
    for _ in 0..3 {
        match subscription.next_event(&options).unwrap().unwrap() {
            PushEvent::Resynchronized(batch) => {
                assert_eq!(batch.len(), 3);
                restored = true;
                break;
            }
            PushEvent::Update(row) => assert!(row.desynchronized()),
            _ => panic!("gap must precede resynchronization"),
        }
    }
    assert!(restored);
    connection
        .push(0x0547, &hq_server::refresh(&[("600000", 93005)]))
        .unwrap();
    assert!(
        matches!(subscription.next_event(&options).unwrap(), Some(PushEvent::Update(row))
        if row.cursor() == 93005 && !row.desynchronized())
    );
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn recovery_failure_keeps_the_gap_and_terminates_without_a_resynchronized_event() {
    let (source_tx, source_rx) = mpsc::channel();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let release_rx = Mutex::new(release_rx);
    let snapshots = AtomicUsize::new(0);
    let activations = AtomicUsize::new(0);
    let server = Server::start_controlled(move |request, connection| {
        if request.msg_type == 0x0547 && activations.fetch_add(1, Ordering::SeqCst) == 0 {
            source_tx.send(connection.clone()).unwrap();
        }
        if request.msg_type == 0x054c && snapshots.fetch_add(1, Ordering::SeqCst) == 1 {
            started_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            return vec![0];
        }
        ordinary_reply(request, 3)
    })
    .unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(3),
            &policy()
                .with_queue_limits(2, 8 * 1024 * 1024)
                .with_gap_policy(GapPolicy::Resynchronize),
            &options,
        )
        .unwrap();
    take_initial(&mut subscription);
    source_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .push(
            0x0547,
            &hq_server::refresh(&[("600000", 93001), ("600001", 93002), ("600002", 93003)]),
        )
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        subscription.next_event(&options).unwrap(),
        Some(PushEvent::Gap(_))
    ));
    release_tx.send(()).unwrap();
    let mut failed = false;
    for _ in 0..3 {
        match subscription.next_event(&options) {
            Err(error) => {
                assert_eq!(error.kind(), ErrorKind::Protocol);
                failed = true;
                break;
            }
            Ok(Some(PushEvent::Update(row))) => assert!(row.desynchronized()),
            _ => panic!("failed recovery must not announce resynchronization or normal end"),
        }
    }
    assert!(failed);
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn failed_renewal_retires_the_lease_without_needing_close_or_another_read() {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let release_rx = Mutex::new(release_rx);
    let activations = AtomicUsize::new(0);
    let server = Server::start(move |request| {
        if request.msg_type == 0x0547 && activations.fetch_add(1, Ordering::SeqCst) == 1 {
            ready_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            return vec![0x93];
        }
        ordinary_reply(request, 1)
    })
    .unwrap();
    let client = client(&server, 1);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(1),
            &policy().with_renew_interval(Some(Duration::from_millis(20))),
            &options,
        )
        .unwrap();
    take_initial(&mut subscription);
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release_tx.send(()).unwrap();
    // Admission proves the background task released the occupied slot even
    // though the consumer has not read the renewal failure or closed its handle.
    let mut pin = client.pin(&options).unwrap();
    assert_eq!(
        subscription.next_event(&options).unwrap_err().kind(),
        ErrorKind::Protocol
    );
    pin.close().unwrap();
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn renewal_does_not_publish_an_earlier_group_before_a_later_group_succeeds() {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let release_rx = Mutex::new(release_rx);
    let refreshes = AtomicUsize::new(0);
    let server = Server::start(move |request| {
        if request.msg_type == 0x0547 {
            match refreshes.fetch_add(1, Ordering::SeqCst) {
                2 => return hq_server::refresh(&[("600000", 93001)]),
                3 => {
                    ready_tx.send(()).unwrap();
                    release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap();
                    return vec![0x93];
                }
                _ => {}
            }
        }
        ordinary_reply(request, 101)
    })
    .unwrap();
    let client = client(&server, 2);
    let options = OperationOptions::default();
    let mut subscription = client
        .quotes()
        .subscribe(
            &ids(101),
            &policy().with_renew_interval(Some(Duration::from_millis(20))),
            &options,
        )
        .unwrap();
    take_initial(&mut subscription);
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        subscription
            .next_event(&options.clone().with_timeout(Duration::ZERO))
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    release_tx.send(()).unwrap();
    assert_eq!(
        subscription.next_event(&options).unwrap_err().kind(),
        ErrorKind::Protocol
    );
    subscription.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}
