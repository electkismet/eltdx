//! T08 authored test source. Execution is deferred until all T01–T30 implementation.
use std::error::Error as _;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::security::SecurityCountRequest;
use eltdx_protocol::unit::Market;
use eltdx_protocol::CommandRequest;
use eltdx_runtime::execution::{AttemptAuthorization, ReservedAttempt};
use eltdx_runtime::RuntimeError;

use super::*;
use crate::policy::retries::{rust_retry_policy, ChildAuthorization};
use crate::ClientBuilder;

fn context(extras: usize, options: OperationOptions) -> (Arc<SessionOwner>, OperationContext) {
    let client = ClientBuilder::new().extra_attempt_budget(extras).build().expect("lazy valid client");
    let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let operation = OperationContext::rust_public(Instant::now(), "test", Arc::clone(&owner),
        client.config(), &options).expect("admitted operation");
    (owner, operation)
}

fn request() -> CommandRequest {
    CommandRequest::SecurityCount(SecurityCountRequest { market: Market::Shanghai, client_date: 0 })
}

#[cfg(feature = "mac")]
#[test]
fn composition_request_limit_covers_pages_metadata_fallback_and_injected_sources() {
    let (_owner,mut operation) = context(8,OperationOptions::default());
    operation.set_request_limit(NonZeroUsize::new(3).unwrap()).unwrap();
    operation.begin_child(ChildPurpose::Page,None).unwrap();
    operation.begin_child(ChildPurpose::Metadata,None).unwrap();
    operation.sponsor().fallback_authorization().unwrap();
    assert_eq!(operation.counters().unwrap().logical_requests,3);
    assert_eq!(operation.counters().unwrap().pages,1);
    assert_eq!(operation.begin_child(ChildPurpose::Fallback,None).unwrap_err().kind(),ErrorKind::LimitExceeded);
    assert_eq!(operation.sponsor().fallback_authorization().unwrap_err().kind(),ErrorKind::LimitExceeded);
    assert_eq!(operation.counters().unwrap().logical_requests,3);
}

#[cfg(feature = "mac")]
#[test]
fn composition_request_limit_is_opt_in_and_cannot_change_after_a_child_begins() {
    let (_owner,mut operation) = context(8,OperationOptions::default());
    for _ in 0..4 { operation.begin_child(ChildPurpose::Metadata,None).unwrap(); }
    assert_eq!(operation.counters().unwrap().logical_requests,4);
    assert_eq!(operation.set_request_limit(NonZeroUsize::new(1).unwrap()).unwrap_err().kind(),ErrorKind::Internal);
    operation.begin_child(ChildPurpose::Required,None).unwrap();
}

#[test]
fn probe_reservation_refunds_global_batch_and_per_code_limits_before_start() {
    let (_owner, operation) = context(4, OperationOptions::default());
    let limit = Some(RecoveryLimit { name: "money_flow_probes", maximum: 4 });
    let held = operation.budget.reserve_scoped(true, limit, Some(0), None).unwrap();
    assert_eq!(operation.counters().unwrap().extra_reserved, 1);
    drop(held);
    assert_eq!(operation.counters().unwrap().extra_reserved, 0);
    assert_eq!(operation.budget.recovery_started("money_flow_probes").unwrap(), 0);
    for _ in 0..3 {
        let mut permit = operation.budget.reserve_scoped(true, limit, Some(0), None).unwrap();
        permit.commit_with_clock(Instant::now(), None, Instant::now).unwrap();
    }
    assert!(operation.budget.reserve_scoped(true, limit, Some(0), None).is_err());
    assert_eq!(operation.counters().unwrap().extra_started, 3);
    let mut other = operation.budget.reserve_scoped(true, limit, Some(1), None).unwrap();
    other.commit_with_clock(Instant::now(), None, Instant::now).unwrap();
    assert_eq!(operation.budget.probe_started(0).unwrap(), 3);
    assert_eq!(operation.budget.probe_started(1).unwrap(), 1);
    assert_eq!(operation.budget.recovery_started("money_flow_probes").unwrap(), 4);
    assert!(operation.budget.reserve_scoped(true, limit, Some(2), None).is_err());
    assert_eq!(operation.counters().unwrap().extra_reserved, 0);
}

#[test]
fn duration_starts_at_entry_and_earliest_explicit_limit_wins() {
    let entry = Instant::now();
    let options = OperationOptions::default().with_timeout(Duration::from_secs(8))
        .with_deadline(entry + Duration::from_secs(3));
    assert_eq!(options.resolve_deadline(entry, Duration::from_secs(2)).expect("deadline"),
        entry + Duration::from_secs(3));
    assert_eq!(OperationOptions::default().with_deadline(entry + Duration::from_secs(20))
        .resolve_deadline(entry, Duration::from_secs(8)).expect("explicit absolute"),
        entry + Duration::from_secs(20));
    assert_eq!(OperationOptions::default().with_timeout(Duration::MAX)
        .resolve_deadline(entry, Duration::from_secs(8)).expect_err("overflow").kind(), ErrorKind::InvalidArgument);
}

#[test]
fn zero_and_past_deadline_reject_without_starting_the_owner() {
    let config = ClientBuilder::new().build().expect("lazy client").config().clone();
    for options in [OperationOptions::default().with_timeout(Duration::ZERO),
        OperationOptions::default().with_deadline(Instant::now())] {
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        assert_eq!(OperationContext::rust_public(Instant::now(), "zero", Arc::clone(&owner), &config, &options)
            .expect_err("immediate expiry").kind(), ErrorKind::Timeout);
        owner.close().expect("no admitted work/resources to retire");
    }
}

#[test]
fn cancelled_token_is_irreversible_and_does_not_retain_owner() {
    let token = CancellationToken::default();
    let clone = token.clone();
    token.cancel();
    clone.cancel();
    assert!(token.is_cancelled() && clone.is_cancelled());
    let config = ClientBuilder::new().build().expect("client").config().clone();
    let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let weak = Arc::downgrade(&owner);
    assert_eq!(OperationContext::rust_public(Instant::now(), "cancelled", Arc::clone(&owner), &config,
        &OperationOptions::default().with_cancellation(token)).expect_err("cancelled").kind(), ErrorKind::Cancelled);
    drop(owner);
    assert!(weak.upgrade().is_none());
    assert!(clone.is_cancelled());
}

#[test]
fn final_extra_slot_is_reserved_atomically_across_concurrent_children() {
    let (_owner, operation) = context(1, OperationOptions::default());
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2).map(|_| {
        let budget = Arc::clone(&operation.budget);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let reservation = budget.reserve(true, None, None);
            barrier.wait(); // Keep the winning reservation alive until both have competed.
            match reservation {
                Ok(reservation) => { Box::new(reservation).start(Instant::now()).expect("start"); true }
                Err(error) => { assert_eq!(error.kind(), ErrorKind::LimitExceeded); false }
            }
        })
    }).collect();
    let started: usize = threads.into_iter().map(|thread| usize::from(thread.join().expect("worker"))).sum();
    assert_eq!(started, 1);
    assert_eq!(operation.counters().expect("counters").extra_started, 1);
    assert_eq!(operation.counters().expect("counters").extra_reserved, 0);
}

#[test]
fn unstarted_and_cancelled_reservations_refund_both_scopes() {
    let token = CancellationToken::default();
    let (_owner, operation) = context(1, OperationOptions::default().with_cancellation(token.clone()));
    let limit = Some(RecoveryLimit { name: "single_diagnostic", maximum: 1 });
    drop(operation.budget.reserve(true, limit, None).expect("reserve"));
    let reservation = operation.budget.reserve(true, limit, None).expect("refunded both");
    token.cancel();
    assert!(Box::new(reservation).start(Instant::now()).is_err());
    let counters = operation.counters().expect("counters");
    assert_eq!((counters.wire_attempts, counters.extra_started, counters.extra_reserved), (0, 0, 0));
}

#[test]
fn local_rejection_never_partially_debits_global_budget() {
    let (_owner, operation) = context(1, OperationOptions::default());
    let error = operation.budget.reserve(true, Some(RecoveryLimit { name: "no_probes", maximum: 0 }), None)
        .expect_err("local cap rejects");
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(operation.counters().expect("counters").extra_reserved, 0);
    Box::new(operation.budget.reserve(true, None, None).expect("global still free"))
        .start(Instant::now()).expect("start");
    assert_eq!(operation.counters().expect("counters").extra_started, 1);
}

#[test]
fn required_requests_and_valid_empty_work_succeed_with_zero_extras() {
    let (_owner, operation) = context(0, OperationOptions::default());
    operation.record_work(0, 0).expect("legitimate empty results");
    for purpose in [ChildPurpose::Required, ChildPurpose::Page, ChildPurpose::Metadata] {
        let authorization = ChildAuthorization { budget: Arc::clone(&operation.budget), purpose,
            recovery: None, probe_scope: None, rust_public: true };
        authorization.reserve(1, None).expect("normal first attempt is free")
            .start(Instant::now()).expect("start");
    }
    assert_eq!(operation.counters().expect("counters").wire_attempts, 3);
    assert_eq!(operation.counters().expect("counters").extra_started, 0);
    let authorization = ChildAuthorization { budget: Arc::clone(&operation.budget), purpose: ChildPurpose::Required,
        recovery: None, probe_scope: None, rust_public: true };
    let network = RuntimeError::connection_closed("real network failure").with_context("host", "fixture:7709");
    let error = authorization.reserve(2, Some(&network)).expect_err("no retry budget");
    let error = operation.runtime_error(error, request().command_code());
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    let source = error.source().expect("recoverable SDK cause");
    assert_eq!(source.source().and_then(|source| source.downcast_ref::<RuntimeError>()), Some(&network));
}

#[test]
fn normal_pages_metadata_recovery_first_attempt_and_retry_are_counted_once_each() {
    let (_owner, operation) = context(8, OperationOptions::default());
    for _ in 0..3 {
        operation.begin_child(ChildPurpose::Page, None).expect("logical page");
        ChildAuthorization { budget: Arc::clone(&operation.budget), purpose: ChildPurpose::Page,
            recovery: None, probe_scope: None, rust_public: true }.reserve(1, None).expect("normal page")
            .start(Instant::now()).expect("wire");
    }
    let page = ChildAuthorization { budget: Arc::clone(&operation.budget), purpose: ChildPurpose::Page,
        recovery: None, probe_scope: None, rust_public: true };
    page.reserve(2, None).expect("page retry").start(Instant::now()).expect("wire");
    let recovery = ChildAuthorization { budget: Arc::clone(&operation.budget), purpose: ChildPurpose::EmptyProbe,
        recovery: Some(RecoveryLimit { name: "empty_probe", maximum: 1 }), probe_scope: None, rust_public: true };
    recovery.reserve(1, None).expect("first probe").start(Instant::now()).expect("wire");
    recovery.reserve(2, None).expect("wire retry does not consume another logical probe")
        .start(Instant::now()).expect("wire");
    assert!(recovery.reserve(1, None).is_err());
    ChildAuthorization { budget: Arc::clone(&operation.budget), purpose: ChildPurpose::Metadata,
        recovery: None, probe_scope: None, rust_public: true }.reserve(1, None).expect("required prefetch")
        .start(Instant::now()).expect("wire");
    let counters = operation.counters().expect("counters");
    assert_eq!((counters.pages, counters.wire_attempts, counters.extra_started), (3, 7, 3));
}

#[test]
fn pages_use_one_total_deadline_while_legacy_has_no_operation_clock() {
    let entry = Instant::now();
    let (_owner, rust) = context(32, OperationOptions::default().with_timeout(Duration::from_secs(8)));
    let legacy_owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let legacy = OperationContext::python_legacy(entry, "legacy_pages", legacy_owner,
        Duration::from_secs(8), Duration::from_secs(1), None, RawPolicy::Omit).expect("legacy context");
    assert!(rust.budget.check(entry + Duration::from_secs(5)).is_ok());
    assert_eq!(rust.budget.check(entry + Duration::from_secs(10)).expect_err("total expired").kind(), ErrorKind::Timeout);
    assert!(legacy.budget.check(entry + Duration::from_secs(10)).is_ok());
    assert!(legacy.sponsor().deadline().is_none());
    legacy.finish().expect("no invented operation deadline");
}

#[test]
fn cancellation_before_second_batch_does_not_cancel_another_operation() {
    let token = CancellationToken::default();
    let (_first_owner, first) = context(32, OperationOptions::default().with_cancellation(token.clone()));
    let (_second_owner, second) = context(32, OperationOptions::default());
    first.begin_child(ChildPurpose::Required, None).expect("first batch");
    token.cancel();
    assert_eq!(first.begin_child(ChildPurpose::Required, None).expect_err("second batch cancelled").kind(), ErrorKind::Cancelled);
    second.begin_child(ChildPurpose::Required, None).expect("unrelated operation remains active");
    second.finish().expect("can complete");
}

#[test]
fn owner_close_rechecks_at_start_and_refunds_reserved_attempt() {
    let (owner, operation) = context(1, OperationOptions::default());
    let reservation = operation.budget.reserve(true, None, None).expect("reserved while open");
    let closer = { let owner = Arc::clone(&owner); thread::spawn(move || owner.close()) };
    while owner.ensure_open().is_ok() { thread::yield_now(); }
    assert!(Box::new(reservation).start(Instant::now()).is_err());
    assert_eq!(operation.counters().expect("counters").extra_reserved, 0);
    assert_eq!(operation.counters().expect("counters").wire_attempts, 0);
    drop(operation);
    closer.join().expect("closer").expect("confirmed owner close");
}

#[test]
fn shorter_attempt_deadline_is_rechecked_inside_admission_gate_and_refunds_both_scopes() {
    let (_owner, operation) = context(2, OperationOptions::default());
    let authorization = ChildAuthorization {
        budget: Arc::clone(&operation.budget), purpose: ChildPurpose::EmptyProbe,
        recovery: Some(RecoveryLimit { name: "short_slice", maximum: 1 }), probe_scope: None, rust_public: true,
    };
    let mut reservation = operation.budget.reserve(true, authorization.recovery, None)
        .expect("atomic local/global reservation");
    let deadline = Instant::now() + Duration::from_secs(1);
    let observed = Instant::now();
    let error = reservation.commit_with_clock(observed, Some(deadline), || deadline)
        .expect_err("admission-lock wait consumes the shorter attempt window");
    assert_eq!(error.kind(), ErrorKind::Timeout);
    drop(reservation);
    let counters = operation.counters().expect("counters");
    assert_eq!(counters.extra_reserved, 0);
    assert_eq!(counters.extra_started, 0);
    assert_eq!(counters.wire_attempts, 0);
    authorization.reserve(1, None).expect("both local and global debits were refunded");
}

#[test]
fn legacy_endpoint_continuation_does_not_consume_the_logical_recovery_twice() {
    let legacy_owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let operation = OperationContext::python_legacy(Instant::now(), "legacy_probe", legacy_owner,
        Duration::from_secs(8), Duration::from_secs(1), None, RawPolicy::Omit).expect("legacy");
    let authorization = ChildAuthorization { budget: Arc::clone(&operation.budget),
        purpose: ChildPurpose::EmptyProbe,
        recovery: Some(RecoveryLimit { name: "legacy_probe", maximum: 1 }), probe_scope: None, rust_public: false };
    authorization.reserve(1, None).expect("logical recovery on endpoint 1")
        .start(Instant::now()).expect("start endpoint 1");
    for _ in 0..4 {
        authorization.reserve_endpoint_continuation(1, None)
            .expect("same logical recovery continues across endpoints")
            .start_before(Instant::now() + Duration::from_secs(2))
            .expect("endpoint continuation starts");
    }
    assert_eq!(operation.counters().expect("counters").wire_attempts, 1);
    assert!(authorization.reserve(1, None).is_err(), "one logical recovery was consumed exactly once");
    operation.finish().expect("legacy child completed");
}

#[test]
fn close_and_start_share_one_admission_linearization() {
    for _ in 0..16 {
        let (owner, operation) = context(1, OperationOptions::default());
        let reservation = operation.budget.reserve(true, None, None).expect("reserve");
        let barrier = Arc::new(Barrier::new(2));
        let closer = { let owner = Arc::clone(&owner); let barrier = Arc::clone(&barrier);
            thread::spawn(move || { barrier.wait(); owner.close() }) };
        barrier.wait();
        let started = Box::new(reservation).start(Instant::now()).is_ok();
        let counters = operation.counters().expect("counters");
        assert_eq!(counters.wire_attempts, usize::from(started));
        assert_eq!(counters.extra_started, usize::from(started));
        assert_eq!(counters.extra_reserved, 0);
        drop(operation);
        closer.join().expect("closer").expect("close");
    }
}

#[test]
fn legacy_recovery_scope_does_not_gain_a_new_32_attempt_cap() {
    let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let operation = OperationContext::python_legacy(Instant::now(), "legacy", owner,
        Duration::from_secs(8), Duration::from_secs(1), None, RawPolicy::Omit).expect("legacy");
    let authorization = ChildAuthorization { budget: Arc::clone(&operation.budget),
        purpose: ChildPurpose::Fallback, recovery: None, probe_scope: None, rust_public: false };
    for _ in 0..64 { authorization.reserve(1, None).expect("no extra global cap").start(Instant::now()).expect("start"); }
    assert_eq!(operation.counters().expect("counters").extra_started, 64);
    assert!(!authorization.count_endpoint_failover());
    operation.finish().expect("done");
}

#[test]
fn profile_page_options_and_work_overflow_are_explicit() {
    let (_owner, mut operation) = context(32, OperationOptions::default());
    operation.set_work_limits(WorkLimits { pages: NonZeroUsize::new(1), records: Some(2), bytes: Some(8) }).expect("limits");
    operation.begin_child(ChildPurpose::Page, None).expect("page one");
    assert_eq!(operation.begin_child(ChildPurpose::Page, None).expect_err("page two exceeds cap").kind(), ErrorKind::LimitExceeded);
    operation.record_work(2, 8).expect("exact cap");
    assert_eq!(operation.record_work(1, 0).expect_err("records cap").kind(), ErrorKind::LimitExceeded);
    assert_eq!(checked_add(usize::MAX, 1).expect_err("checked addition").kind(), ErrorKind::LimitExceeded);
}

#[test]
fn cleanup_is_once_only_independent_of_business_token_and_tightens_on_close() {
    let token = CancellationToken::default();
    let (owner, operation) = context(32, OperationOptions::default().with_cancellation(token.clone()));
    token.cancel();
    operation.claim_cleanup().expect("cancelled token cannot prevent cleanup");
    assert!(operation.claim_cleanup().is_err());
    let closer = { let owner = Arc::clone(&owner); thread::spawn(move || owner.close()) };
    while owner.ensure_open().is_ok() { thread::yield_now(); }
    let deadline = owner.shutdown_deadline().expect("state").expect("closing");
    assert!(operation.cleanup_remaining().expect("remaining") <= deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1));
    drop(operation);
    closer.join().expect("closer").expect("close");
}

#[test]
fn structured_protocol_source_and_cancel_cleanup_cause_remain_accessible() {
    let (_owner, operation) = context(32, OperationOptions::default());
    let source = RuntimeError::Protocol { code: "malformed_frame".to_owned(), message: "broken frame".to_owned(),
        context: vec![("phase".to_owned(), "response".to_owned()), ("host".to_owned(), "fixture:7709".to_owned()),
            ("attempt".to_owned(), "2".to_owned())] };
    let error = operation.runtime_error(source.clone(), request().command_code());
    assert_eq!(error.kind(), ErrorKind::Protocol);
    assert_eq!(error.source().and_then(|source| source.downcast_ref::<RuntimeError>()), Some(&source));
    for key in ["phase", "host", "attempt", "operation", "command"] {
        assert!(error.context().iter().any(|(actual, _)| actual == key));
    }
    let root = Error::new(ErrorKind::Cancelled, "first stop");
    let error = Error::new(ErrorKind::CancelUnconfirmed, "not confirmed").with_source(root)
        .with_cleanup_failure(Error::new(ErrorKind::Timeout, "retirement timeout"));
    assert_eq!(error.source().and_then(|source| source.downcast_ref::<Error>()).expect("root").kind(), ErrorKind::Cancelled);
    assert_eq!(error.cleanup_failure().expect("cleanup").kind(), ErrorKind::Timeout);
}

#[test]
fn sponsorship_retains_no_owner_or_operation_guard_and_keeps_command_retry_policy() {
    let (owner, operation) = context(32, OperationOptions::default());
    let weak = Arc::downgrade(&owner);
    let sponsor = operation.sponsor();
    let authorization = sponsor.metadata_authorization().expect("one logical metadata request");
    assert_eq!(operation.counters().expect("counters").logical_requests, 1);
    authorization.reserve(1, None).expect("metadata first attempt").start(Instant::now()).expect("start");
    assert_eq!(operation.counters().expect("counters").logical_requests, 1);
    assert!(rust_retry_policy(&request(), NonZeroU32::new(300).expect("nonzero")).retry_safe());
    assert_eq!(sponsor.retry_policy(&request()).max_attempts(), 2);
    drop(operation);
    drop(owner);
    assert!(weak.upgrade().is_none());
    assert!(authorization.reserve(2, None).is_err());
}

// Synthetic loopback frames exercise the actual runtime scheduler, rather than a
// facade retry model. These are authored fixtures, not captured market responses.
fn wire_response(request: &eltdx_protocol::frame::RequestFrame, payload: &[u8]) -> Vec<u8> {
    let mut response = Vec::from(eltdx_protocol::frame::RESPONSE_PREFIX);
    response.push(1);
    response.extend_from_slice(&request.msg_id.to_le_bytes());
    response.push(0);
    response.extend_from_slice(&request.msg_type.to_le_bytes());
    response.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    response.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    response.extend_from_slice(payload);
    response
}

fn read_wire(stream: &mut std::net::TcpStream) -> std::io::Result<eltdx_protocol::frame::RequestFrame> {
    use std::io::Read;
    let mut bytes = vec![0; 12];
    stream.read_exact(&mut bytes)?;
    let encoded = usize::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let body = encoded.checked_sub(2).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid request length"))?;
    bytes.resize(12 + body, 0);
    stream.read_exact(&mut bytes[12..])?;
    eltdx_protocol::frame::RequestFrame::decode(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
}

fn loopback_engine(endpoints: Vec<eltdx_runtime::endpoint::Endpoint>, timeout: Duration) -> eltdx_runtime::Engine {
    eltdx_runtime::Engine::new(eltdx_runtime::EngineConfig::from_endpoints(endpoints, timeout,
        eltdx_runtime::EngineConfigOptions {
            pool_size: 1, runtime_workers: Some(1), server_count: 1,
            max_connections_per_host: None, connect_concurrency: None, connect_concurrency_per_host: None,
            global_raw_bytes: None, global_decoded_bytes: None, heartbeat_interval: None,
            max_pending_requests: 8, push_queue_size: 8, push_queue_bytes: 1024,
        }).expect("config")).expect("lazy engine")
}

fn page_server(delay: Duration, pages: usize) -> (eltdx_runtime::Engine, thread::JoinHandle<usize>) {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback");
    let address = listener.local_addr().expect("address");
    listener.set_nonblocking(true).expect("bounded accept");
    let server = thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                    thread::sleep(Duration::from_millis(1)),
                Err(_) => return 0,
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(2))).expect("bounded read");
        stream.set_write_timeout(Some(Duration::from_secs(2))).expect("bounded write");
        let handshake = read_wire(&mut stream).expect("handshake request");
        stream.write_all(&wire_response(&handshake, &[0; 189])).expect("handshake reply");
        let mut received = 0;
        for _ in 0..pages {
            let Ok(request) = read_wire(&mut stream) else { break; };
            received += 1;
            thread::sleep(delay);
            if stream.write_all(&wire_response(&request, &[1, 0])).is_err() { break; }
        }
        received
    });
    let engine = loopback_engine(vec![eltdx_runtime::endpoint::Endpoint::numeric(&address.to_string()).expect("endpoint")],
        Duration::from_millis(120));
    (engine, server)
}

#[test]
fn runtime_two_page_comparison_rust_times_out_both_python_profiles_keep_request_windows() {
    use crate::hq::executor::HqExecutor;
    use crate::__python_compat::{LegacyChildPurpose, LegacyExecutor};
    let (engine, server) = page_server(Duration::from_millis(80), 2);
    let client = ClientBuilder::new().request_timeout(Duration::from_millis(120)).build().expect("config");
    let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
    let executor = HqExecutor::new(owner);
    let operation = executor.begin_context("two_pages", client.config(), &OperationOptions::default()).expect("operation");
    executor.execute_raw(&engine, &operation, request(), ChildPurpose::Page, None).expect("first page under total");
    let error = executor.execute_raw(&engine, &operation, request(), ChildPurpose::Page, None).expect_err("combined total exceeded");
    assert_eq!(error.kind(), ErrorKind::Timeout);
    drop(operation);
    executor.close().expect("retired Rust operation");
    assert_eq!(server.join().expect("server"), 2);

    let (engine, server) = page_server(Duration::from_millis(80), 2);
    let legacy = LegacyExecutor::new(engine, Duration::from_millis(120), Duration::from_secs(1), Duration::from_secs(1)).expect("legacy bridge");
    let operation = legacy.begin_operation("two_pages", RawPolicy::Omit, None).expect("legacy operation")
        .with_page_limit(None).expect("unlimited");
    for _ in 0..2 { operation.execute_raw(request(), LegacyChildPurpose::Page, None).expect("new Python per-request clock"); }
    operation.finish().expect("legacy final delivery");
    legacy.close().expect("legacy close");
    assert_eq!(server.join().expect("server"), 2);

    let (engine, server) = page_server(Duration::from_millis(80), 2);
    for _ in 0..2 { engine.execute(request()).expect("old native Python transport per-request clock"); }
    engine.close().expect("old engine close");
    assert_eq!(server.join().expect("server"), 2);
}

#[test]
fn real_host_failover_consumes_extra_budget_and_preserves_failed_host_source() {
    use crate::hq::executor::HqExecutor;
    let listeners: Vec<_> = (0..2).map(|_| std::net::TcpListener::bind("127.0.0.1:0").expect("reserved loopback port")).collect();
    let endpoints = listeners.iter().map(|listener| eltdx_runtime::endpoint::Endpoint::numeric(
        &listener.local_addr().expect("address").to_string()).expect("endpoint")).collect();
    drop(listeners); // Both connect attempts fail locally; no network service or external dependency.
    let engine = loopback_engine(endpoints, Duration::from_secs(1));
    let (owner, operation) = context(0, OperationOptions::default().with_timeout(Duration::from_secs(1)));
    let executor = HqExecutor::new(owner);
    let error = executor.execute_raw(&engine, &operation, request(), ChildPurpose::Required, None)
        .expect_err("first real failure then no failover budget");
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert_eq!(operation.counters().expect("counters").wire_attempts, 1);
    assert_eq!(operation.counters().expect("counters").extra_started, 0);
    let source = error.source().expect("SDK recoverable cause").source().expect("real runtime failure");
    let source = source.downcast_ref::<RuntimeError>().expect("runtime source");
    assert!(source.context().iter().any(|(key, _)| key == "host"));
    assert!(source.context().iter().any(|(key, value)| key == "attempt" && value == "1"));
    drop(operation);
    executor.close().expect("close");
}

#[test]
fn worker_unstarted_rejection_does_not_close_unrelated_requests() {
    #[derive(Debug)]
    struct Reject;
    impl AttemptAuthorization for Reject {
        fn check_boundary(&self, _: Instant) -> std::result::Result<(), RuntimeError> { Ok(()) }
        fn reserve(&self, _: u32, _: Option<&RuntimeError>) -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
            Err(RuntimeError::internal("fixture start refusal").with_context("sdk_kind", "Cancelled"))
        }
        fn count_endpoint_failover(&self) -> bool { true }
    }
    let (engine, server) = page_server(Duration::ZERO, 1);
    let options = SubmissionOptions::new(Instant::now() + Duration::from_secs(1))
        .with_attempt_authorization(rust_retry_policy(&request(), NonZeroU32::new(2).expect("nonzero")), Arc::new(Reject));
    let error = engine.execute_raw_with_options(request(), options).expect_err("rejected before actual wire work");
    assert!(error.context().iter().any(|(key, value)| key == "sdk_kind" && value == "Cancelled"));
    engine.execute(request()).expect("other request still works on same runtime");
    engine.close().expect("close");
    assert_eq!(server.join().expect("server"), 1);
}

#[test]
fn domain_neutral_permit_shares_existing_budget_and_refunds_unstarted_reservations() {
    let (_owner, operation) = context(1, OperationOptions::default());
    let authorization = operation.begin_child(ChildPurpose::Required, None).unwrap().into_transport_authorization();
    authorization.reserve(1, None).unwrap().start(Instant::now()).unwrap();
    let held = authorization.reserve(2, None).unwrap();
    assert_eq!(operation.counters().unwrap().extra_reserved, 1);
    drop(held);
    assert_eq!(operation.counters().unwrap().extra_reserved, 0);
    authorization.reserve(2, None).unwrap().start(Instant::now()).unwrap();
    assert_eq!(operation.counters().unwrap().wire_attempts, 2);
    assert_eq!(operation.counters().unwrap().extra_started, 1);
    assert!(authorization.reserve(3, None).is_err());
}

#[test]
fn domain_neutral_permit_cannot_start_after_owner_close() {
    let (owner, operation) = context(1, OperationOptions::default());
    let authorization = operation.begin_child(ChildPurpose::Required, None).unwrap().into_transport_authorization();
    let held = authorization.reserve(1, None).unwrap();
    drop(operation);
    owner.close().unwrap();
    assert!(held.start(Instant::now()).is_err());
}
