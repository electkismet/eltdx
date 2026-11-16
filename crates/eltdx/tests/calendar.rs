#[path = "support/hq_server.rs"] mod hq_server;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use eltdx::{CalendarQuery, Clock, ErrorKind, OperationOptions, TdxClient, TradingDate};
use eltdx_protocol::frame::RequestFrame;

struct FixedClock(AtomicI64);
impl Clock for FixedClock { fn unix_seconds(&self) -> eltdx::Result<i64> { Ok(self.0.load(Ordering::SeqCst)) } }
fn date(day: u8) -> TradingDate { TradingDate::new(1970, 1, day).unwrap() }
fn bars(request: &RequestFrame, dates: &[u32]) -> Vec<u8> {
    assert_eq!(request.msg_type, 0x052d);
    assert_eq!(u16::from_le_bytes(request.data[8..10].try_into().unwrap()), 4); // daily
    let mut payload = (dates.len() as u16).to_le_bytes().to_vec();
    for at in dates { payload.extend_from_slice(&at.to_le_bytes()); payload.extend_from_slice(&[0; 16]); }
    payload
}
fn offset(request: &RequestFrame) -> u16 { u16::from_le_bytes(request.data[12..14].try_into().unwrap()) }
fn client(server: &hq_server::Server, clock: Arc<FixedClock>) -> TdxClient {
    TdxClient::builder().endpoints(vec![server.address]).heartbeat_interval(None).clock(clock).build().unwrap()
}

#[test]
fn sorted_unique_calendar_has_explicit_coverage_inclusive_ranges_and_bounded_neighbors() {
    let server = hq_server::Server::start(|request| if offset(request) == 0 { bars(request, &[19700105, 19700102, 19700102]) } else { bars(request, &[]) }).unwrap();
    let clock = Arc::new(FixedClock(AtomicI64::new(0))); let client = client(&server, clock);
    let service = client.calendar(); let options = OperationOptions::default(); let query = CalendarQuery::new();
    let snapshot = service.refresh(&query, &options).unwrap();
    assert_eq!(snapshot.days(), &[date(2), date(5)]); assert!(snapshot.complete());
    assert_eq!(snapshot.coverage_start(), Some(date(2))); assert_eq!(snapshot.coverage_end(), Some(date(5)));
    assert!(!client.calendar().is_workday(date(3), &query, &options).unwrap());
    assert_eq!(service.range(date(5), date(2), true, &query, &options).unwrap(), vec![date(5), date(2)]);
    assert_eq!(service.next(date(2), true, &query, &options).unwrap(), Some(date(2)));
    assert_eq!(service.previous(date(2), false, &query, &options).unwrap(), None);
    assert_eq!(service.next(date(5), false, &query, &options).unwrap(), None);
    assert_eq!(service.is_workday(date(1), &query, &options).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    client.close().unwrap(); assert_eq!(service.is_workday(date(2), &query, &options).unwrap_err().kind(), ErrorKind::Closed);
    server.finish().unwrap();
}

#[test]
fn owner_cache_is_shared_by_getters_but_expires_at_shanghai_midnight() {
    let loads = Arc::new(AtomicUsize::new(0)); let observed = loads.clone();
    let server = hq_server::Server::start(move |request| {
        if offset(request) == 0 { observed.fetch_add(1, Ordering::SeqCst); bars(request, &[19700102, 19700105]) } else { bars(request, &[]) }
    }).unwrap(); let clock = Arc::new(FixedClock(AtomicI64::new(57599))); let client = client(&server, clock.clone());
    let options = OperationOptions::default(); let query = CalendarQuery::new();
    assert_eq!(client.calendar().today().unwrap(), date(1));
    for _ in 0..2 { assert!(client.calendar().is_workday(date(2), &query, &options).unwrap()); }
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    clock.0.store(57600, Ordering::SeqCst); assert_eq!(client.calendar().today().unwrap(), date(2));
    assert!(client.calendar().is_workday(date(2), &query, &options).unwrap()); assert_eq!(loads.load(Ordering::SeqCst), 2);
    client.calendar().clear_cache(); assert!(client.calendar().is_workday(date(2), &query, &options).unwrap());
    assert_eq!(loads.load(Ordering::SeqCst), 3); client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn failed_refresh_keeps_the_previous_complete_snapshot_and_empty_has_no_coverage() {
    let loads = AtomicUsize::new(0);
    let server = hq_server::Server::start(move |request| {
        if offset(request) == 0 && loads.fetch_add(1, Ordering::SeqCst) != 0 { return vec![1]; }
        if offset(request) == 0 { bars(request, &[19700102, 19700105]) } else { bars(request, &[]) }
    }).unwrap(); let client = client(&server, Arc::new(FixedClock(AtomicI64::new(0))));
    let options = OperationOptions::default(); let query = CalendarQuery::new(); client.calendar().refresh(&query, &options).unwrap();
    assert_eq!(client.calendar().refresh(&query, &options).unwrap_err().kind(), ErrorKind::Protocol);
    assert!(client.calendar().is_workday(date(2), &query, &options).unwrap()); client.close().unwrap(); server.finish().unwrap();

    let empty_server = hq_server::Server::start(|request| bars(request, &[])).unwrap();
    let empty_client = self::client(&empty_server, Arc::new(FixedClock(AtomicI64::new(0))));
    assert_eq!(empty_client.calendar().is_workday(date(2), &query, &options).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    empty_client.close().unwrap(); empty_server.finish().unwrap();
}

#[test]
fn separate_owners_never_reuse_each_others_calendar_snapshot() {
    let loads = Arc::new(AtomicUsize::new(0)); let observed = loads.clone();
    let server = hq_server::Server::start(move |request| {
        if offset(request) == 0 { observed.fetch_add(1, Ordering::SeqCst); bars(request, &[19700102, 19700105]) } else { bars(request, &[]) }
    }).unwrap(); let clock = Arc::new(FixedClock(AtomicI64::new(0)));
    let first = client(&server, clock.clone()); let second = client(&server, clock);
    first.calendar().refresh(&CalendarQuery::new(), &OperationOptions::default()).unwrap();
    second.calendar().is_workday(date(2), &CalendarQuery::new(), &OperationOptions::default()).unwrap();
    assert_eq!(loads.load(Ordering::SeqCst), 2); first.close().unwrap(); second.close().unwrap(); server.finish().unwrap();
}

#[test]
fn crossing_midnight_during_refresh_does_not_publish_a_stale_snapshot() {
    let clock = Arc::new(FixedClock(AtomicI64::new(57599))); let changed = clock.clone();
    let server = hq_server::Server::start(move |request| {
        if offset(request) == 0 { bars(request, &[19700102, 19700105]) }
        else { changed.0.store(57600, Ordering::SeqCst); bars(request, &[]) }
    }).unwrap(); let client = client(&server, clock);
    assert_eq!(client.calendar().refresh(&CalendarQuery::new(), &OperationOptions::default()).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    assert_eq!(client.calendar().today().unwrap(), date(2));
    client.close().unwrap(); server.finish().unwrap();
}

#[test]
fn clock_failure_propagates_without_network_or_date_fallback() {
    struct FailedClock;
    impl Clock for FailedClock { fn unix_seconds(&self) -> eltdx::Result<i64> {
        Err(eltdx::Error::new(ErrorKind::MetadataUnavailable, "injected unavailable clock"))
    } }
    let server = hq_server::Server::start(|_| panic!("clock failure must precede I/O")).unwrap();
    let client = TdxClient::builder().endpoints(vec![server.address]).clock(Arc::new(FailedClock)).build().unwrap();
    assert_eq!(client.calendar().today().unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    assert_eq!(client.calendar().is_workday(date(2), &CalendarQuery::new(), &OperationOptions::default()).unwrap_err().kind(), ErrorKind::MetadataUnavailable);
    assert_eq!(server.connection_count(), 0); client.close().unwrap(); server.finish().unwrap();
}
