//! Public policy source cases. Network/owner integration cases are added with
//! the active subscription service; nothing in this file has been executed.
use std::time::Duration;
use eltdx::{GapPolicy, SubscriptionOptions};
use eltdx::types::RefreshRequest;

#[path = "support/hq_server.rs"]
mod hq_server;
use hq_server::Server;

#[test]
fn subscription_policy_is_independent_of_setup_deadlines_and_has_frozen_defaults() {
    let policy = SubscriptionOptions::default();
    assert_eq!(policy.max_events(), 1024); assert_eq!(policy.max_bytes(), 64 * 1024 * 1024);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
    assert_eq!(policy.gap_policy(), GapPolicy::ReportOnly);
    let changed = policy.clone().with_queue_limits(2, 512 * 1024).with_renew_interval(None)
        .with_gap_policy(GapPolicy::Resynchronize);
    assert_eq!(changed.max_events(), 2); assert_eq!(changed.renew_interval(), None);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
}

#[test]
fn refresh_requests_retain_source_local_cursors_without_monotonic_reinterpretation() {
    let mut request = RefreshRequest::new("sh600000".parse().unwrap());
    assert_eq!(request.cursor(), 0);
    request = request.with_cursor(150000).unwrap().with_cursor(93000).unwrap();
    assert_eq!(request.cursor(), 93000);
    assert_eq!(request.id().to_string(), "sh600000");
}

#[test]
fn cold_metadata_refresh_with_the_only_slot_pinned_uses_that_connection() {
    use std::sync::{Arc, Mutex};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&seen);
    let server = Server::start(move |request| {
        observed.lock().unwrap().push((request.msg_type, request.data.clone()));
        match request.msg_type {
            0x044d if hq_server::directory_offset(request) == 0 => hq_server::directory(&[("600000", 2)]),
            0x044d => hq_server::directory(&[]),
            0x0547 => vec![0x93, 0x93],
            _ => panic!("unexpected command"),
        }
    }).unwrap();
    let client = eltdx::ClientBuilder::new().pool_size(1).endpoints(vec![server.address])
        .heartbeat_interval(None).build().unwrap();
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
    assert_eq!(operations.iter().map(|(command, _)| *command).collect::<Vec<_>>(),
        vec![0x044d, 0x044d, 0x0547, 0x0547]);
    let empty = &operations.last().unwrap().1;
    assert_eq!(empty.as_ref(), &[0, 0], "empty polling is a real wire operation");
    assert_eq!(server.connection_count(), 1);
    drop(operations);
    pin.close().unwrap();
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn empty_refresh_on_a_new_pin_does_not_load_catalogs_and_over_limit_is_no_io() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let refreshes = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&refreshes);
    let server = Server::start(move |request| {
        assert_eq!(request.msg_type, 0x0547);
        assert_eq!(request.data.as_ref(), &[0, 0]);
        count.fetch_add(1, Ordering::SeqCst);
        vec![0x93, 0x93]
    }).unwrap();
    let client = eltdx::ClientBuilder::new().pool_size(1).endpoints(vec![server.address])
        .heartbeat_interval(None).build().unwrap();
    let options = eltdx::OperationOptions::default();
    let mut pin = client.pin(&options).unwrap();
    assert!(pin.quotes().refresh(&[], &options).unwrap().updates().is_empty());
    let too_many = vec![RefreshRequest::new("sh600000".parse().unwrap()); 101];
    assert_eq!(pin.quotes().refresh(&too_many, &options).unwrap_err().kind(), eltdx::ErrorKind::InvalidArgument);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    pin.close().unwrap();
    assert_eq!(pin.quotes().refresh(&[], &options).unwrap_err().kind(), eltdx::ErrorKind::PinLost);
    client.close().unwrap();
    server.finish().unwrap();
}
