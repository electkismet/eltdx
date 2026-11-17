#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Authored public-owner/race sources. Execution waits for T01-T30 completion.
//! All login strings here are explicit synthetic fixtures, never production ACKs.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"]
mod wire;
use bytes::Bytes;
use eltdx::mac::handshake::LoginReplyRules;
use eltdx::mac::*;
use eltdx::{CancellationToken, ErrorKind, OperationOptions, TradingDate};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wire::{
    bars, bars_request, eof, frame, frame_bytes, join, peer, qfq, qfq_then_raw, setup_mac, Case,
};

fn ex_directory(offset: u32) -> Vec<u8> {
    let mut body = vec![0; 70];
    body[..4].copy_from_slice(&offset.to_le_bytes());
    body[4..6].copy_from_slice(&1_u16.to_le_bytes());
    body[6] = 5;
    body[7] = 31;
    body[11..16].copy_from_slice(b"00700");
    body[20..24].copy_from_slice(b"name");
    body[37..41].copy_from_slice(b"desc");
    body[69] = 0xa5;
    body
}
fn ex_page_request(stream: &mut TcpStream, start: u32) -> Case {
    let body = wire::request(stream, 1, 0x23f5)?;
    assert_eq!(body.len(), 6);
    assert_eq!(u32::from_le_bytes(body[..4].try_into()?), start);
    assert_eq!(u16::from_le_bytes(body[4..].try_into()?), 1);
    Ok(())
}
fn ex_page_query(start: u32) -> eltdx::Result<eltdx::ex::InstrumentsQuery> {
    eltdx::ex::InstrumentsQuery::new(start).with_count(1)
}
fn catalog_rules() -> eltdx::Result<LoginReplyRules> {
    LoginReplyRules::new(
        vec![Bytes::from_static(b"fixture login accepted")],
        vec![Bytes::from_static(b"fixture login denied")],
        "synthetic lifecycle fixture only",
    )
}

#[test]
fn actual_ex_client_uses_its_92_byte_setup_nonempty_directory_raw_and_shared_close() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        wire::setup_ex(&mut stream)?;
        ex_page_request(&mut stream, 7)?;
        frame(&mut stream, &ex_directory(7))?;
        ex_page_request(&mut stream, 8)?;
        frame(&mut stream, &ex_directory(8))?;
        eof(&mut stream)
    })?;
    let client = eltdx::ex::ExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(eltdx::RawPolicy::RetainWire)
        .build()?;
    let first = client.instruments(&ex_page_query(7)?, &OperationOptions::default())?;
    let second = client.instruments(&ex_page_query(8)?, &OperationOptions::default())?;
    assert_eq!(first.rows()[0].code(), "00700");
    assert_eq!(first.rows()[0].desc(), "desc");
    assert_eq!(first.completion(), PageCompletion::RequestedCount);
    assert_eq!(
        first.meta().source().session_id(),
        second.meta().source().session_id()
    );
    assert_eq!(first.meta().raw().unwrap().row_ranges(), &[(0, 6, 64)]);
    assert_eq!(first.meta().raw().unwrap().wire_frames().unwrap().len(), 1);
    client.clone().close()?;
    assert_eq!(
        client
            .instruments(&ex_page_query(9)?, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    join(task)
}

#[test]
fn actual_ex_cancel_and_close_during_half_frame_confirm_socket_retirement() -> Case {
    for close in [false, true] {
        let (observed, entered) = mpsc::channel();
        let (endpoint, task) = peer(move |mut stream| {
            wire::setup_ex(&mut stream)?;
            ex_page_request(&mut stream, 0)?;
            let bytes = frame_bytes(&ex_directory(0))?;
            stream.write_all(&bytes[..21])?;
            observed.send(())?;
            eof(&mut stream)
        })?;
        let client = eltdx::ex::ExClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let token = CancellationToken::default();
        let caller = client.clone();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let call = thread::spawn(move || caller.instruments(&ex_page_query(0)?, &options));
        entered.recv_timeout(Duration::from_secs(2))?;
        if close {
            client.close()?;
        } else {
            token.cancel();
        }
        assert_eq!(
            join_call(call)?.unwrap_err().kind(),
            if close {
                ErrorKind::Closed
            } else {
                ErrorKind::Cancelled
            }
        );
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn actual_ex_failover_repeats_setup_and_keeps_the_previous_snapshot_generation_distinct() -> Case {
    let (first, first_task) = peer(|mut stream| {
        wire::setup_ex(&mut stream)?;
        ex_page_request(&mut stream, 0)?;
        frame(&mut stream, &ex_directory(0))?;
        ex_page_request(&mut stream, 1)?;
        Ok(())
    })?;
    let (second, second_task) = peer(|mut stream| {
        wire::setup_ex(&mut stream)?;
        ex_page_request(&mut stream, 1)?;
        frame(&mut stream, &ex_directory(1))?;
        eof(&mut stream)
    })?;
    let client = eltdx::ex::ExClient::builder()
        .with_endpoints(vec![first, second])
        .build()?;
    let old = client.instruments(&ex_page_query(0)?, &OperationOptions::default())?;
    let new = client.instruments(&ex_page_query(1)?, &OperationOptions::default())?;
    assert_eq!(old.meta().source().endpoint(), Some(first));
    assert_eq!(new.meta().source().endpoint(), Some(second));
    assert_ne!(
        old.meta().source().session_id(),
        new.meta().source().session_id()
    );
    assert_eq!(old.rows()[0].record_raw()[63], 0xa5);
    client.close()?;
    join(first_task)?;
    join(second_task)
}

#[test]
fn queued_ex_timeout_does_not_retire_the_active_exchange_or_consume_a_reply() -> Case {
    let (observed, entered) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    let (endpoint, task) = peer(move |mut stream| {
        wire::setup_ex(&mut stream)?;
        ex_page_request(&mut stream, 0)?;
        observed.send(())?;
        release.recv_timeout(Duration::from_secs(2))?;
        frame(&mut stream, &ex_directory(0))?;
        ex_page_request(&mut stream, 2)?;
        frame(&mut stream, &ex_directory(2))?;
        eof(&mut stream)
    })?;
    let client = eltdx::ex::ExClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let caller = client.clone();
    let first =
        thread::spawn(move || caller.instruments(&ex_page_query(0)?, &OperationOptions::default()));
    entered.recv_timeout(Duration::from_secs(2))?;
    assert_eq!(
        client
            .instruments(
                &ex_page_query(1)?,
                &OperationOptions::default().with_timeout(Duration::from_millis(30))
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    resume.send(())?;
    let first = join_call(first)??;
    let next = client.instruments(&ex_page_query(2)?, &OperationOptions::default())?;
    assert_eq!(
        first.meta().source().session_id(),
        next.meta().source().session_id()
    );
    client.close()?;
    join(task)
}

#[test]
fn actual_mac_ex_empty_goods_directory_fallback_retains_one_login_and_one_source() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        wire::request(&mut stream, 1, 0x2454)?;
        frame(&mut stream, b"fixture login accepted")?;
        wire::request(&mut stream, 1, 0x2562)?;
        frame(&mut stream, &[0, 0])?;
        let mut count = vec![0; 23];
        count[19..].copy_from_slice(&1_u32.to_le_bytes());
        wire::request(&mut stream, 1, 0x23f0)?;
        frame(&mut stream, &count)?;
        ex_page_request(&mut stream, 0)?;
        frame(&mut stream, &ex_directory(0))?;
        wire::request(&mut stream, 1, 0x23f0)?;
        frame(&mut stream, &count)?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(catalog_rules()?)
        .with_raw(eltdx::RawPolicy::RetainWire)
        .build()?;
    let rows = client.goods(
        &ex_query::GoodsQuery::new(eltdx::ex::ExMarketId::from_wire(31)),
        &OperationOptions::default(),
    )?;
    assert_eq!(rows.rows()[0].actual_code(), "00700");
    assert!(rows.rows()[0].is_derived());
    assert_eq!(rows.meta().sources().len(), 4);
    let generation = rows.meta().sources()[0].session_id();
    for source in rows.meta().sources() {
        assert_eq!(source.endpoint(), Some(endpoint));
        assert_eq!(source.session_id(), generation);
    }
    client.close()?;
    join(task)
}

#[test]
fn actual_mac_ex_directory_fallback_half_frame_cancel_never_reopens_or_switches_source() -> Case {
    let (observed, entered) = mpsc::channel();
    let unused = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let (endpoint, task) = peer(move |mut stream| {
        wire::request(&mut stream, 1, 0x2454)?;
        frame(&mut stream, b"fixture login accepted")?;
        wire::request(&mut stream, 1, 0x2562)?;
        frame(&mut stream, &[0, 0])?;
        wire::request(&mut stream, 1, 0x23f0)?;
        let mut count = vec![0; 23];
        count[19..].copy_from_slice(&1_u32.to_le_bytes());
        frame(&mut stream, &count)?;
        ex_page_request(&mut stream, 0)?;
        let body = frame_bytes(&ex_directory(0))?;
        stream.write_all(&body[..25])?;
        observed.send(())?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint, unused.local_addr()?])
        .with_login_reply_rules(catalog_rules()?)
        .build()?;
    let token = CancellationToken::default();
    let caller = client.clone();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || {
        caller.goods(
            &ex_query::GoodsQuery::new(eltdx::ex::ExMarketId::from_wire(31)),
            &options,
        )
    });
    entered.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(join_call(call)?.unwrap_err().kind(), ErrorKind::Cancelled);
    unused.set_nonblocking(true)?;
    assert_eq!(
        unused.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    client.close()?;
    join(task)
}

#[test]
fn bounded_mac_download_releases_its_lease_for_the_next_nonempty_business_query() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        let body = wire::request(&mut stream, 0x1c, 0x1217)?;
        assert_eq!(u32::from_le_bytes(body[8..12].try_into()?), 4);
        let mut chunk = vec![0xa5; 8];
        chunk.extend_from_slice(b"data");
        frame(&mut stream, &chunk)?;
        bars_request(&mut stream, 0x1c, 0, 0, 1)?;
        frame(&mut stream, &bars(&[(20261117, 10.0)])?)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let options = eltdx::DownloadOptions::new()
        .with_expected_size(Some(4))?
        .with_max_bytes(4)?
        .with_chunk_size(std::num::NonZeroU32::new(4).unwrap())?;
    let query =
        DownloadFileQuery::new(RemoteFileName::new("fixture.dat")?).with_options(options)?;
    let result = client.download_file(&query, &OperationOptions::default())?;
    assert_eq!(result.completion, eltdx::DownloadCompletion::LimitReached);
    assert_eq!(result.data.as_ref(), b"data");
    let bars = client.bars(
        &BarsQuery::new(wire::id()?).with_count(1)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].close().value().as_f64(), 10.0);
    client.close()?;
    join(task)
}

fn events() -> eltdx::Result<XdxrBatch> {
    Ok(XdxrBatch::new(vec![XdxrEvent {
        fenhong: 1.0,
        ..XdxrEvent::new(TradingDate::new(2026, 11, 18)?)
    }]))
}
#[derive(Debug)]
struct CountingProvider(Arc<AtomicUsize>);
impl XdxrProvider for CountingProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        self.0.fetch_add(1, Ordering::SeqCst);
        context.check_boundary()?;
        events()
    }
}
#[derive(Debug)]
struct WaitingProvider {
    entered: mpsc::Sender<Instant>,
    release: Mutex<mpsc::Receiver<()>>,
    cooperative: bool,
}
impl XdxrProvider for WaitingProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        self.entered
            .send(context.deadline())
            .map_err(|_| eltdx::Error::new(ErrorKind::Internal, "fixture observer dropped"))?;
        let release = self
            .release
            .lock()
            .map_err(|_| eltdx::Error::new(ErrorKind::Internal, "fixture release poisoned"))?;
        if self.cooperative {
            loop {
                context.check_boundary()?;
                match release.recv_timeout(Duration::from_millis(5)) {
                    Ok(()) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(eltdx::Error::new(
                            ErrorKind::Internal,
                            "fixture release disconnected",
                        ))
                    }
                }
            }
        } else {
            release.recv().map_err(|_| {
                eltdx::Error::new(ErrorKind::Internal, "fixture release disconnected")
            })?;
        }
        context.check_boundary()?;
        events()
    }
}
fn waiting(
    cooperative: bool,
) -> (
    Arc<WaitingProvider>,
    mpsc::Receiver<Instant>,
    mpsc::Sender<()>,
) {
    let (entered, observer) = mpsc::channel();
    let (release, receiver) = mpsc::channel();
    (
        Arc::new(WaitingProvider {
            entered,
            release: Mutex::new(receiver),
            cooperative,
        }),
        observer,
        release,
    )
}
fn join_call<T>(task: thread::JoinHandle<eltdx::Result<T>>) -> Case<eltdx::Result<T>> {
    task.join()
        .map_err(|_| std::io::Error::other("public call fixture panicked").into())
}

#[test]
fn cancellation_during_unadjusted_half_frame_propagates_and_never_invokes_provider() -> Case {
    let (partial, observed) = mpsc::channel();
    let (endpoint, task) = peer(move |mut stream| {
        setup_mac(&mut stream)?;
        bars_request(&mut stream, 0x1c, 1, 0, 2)?;
        frame(&mut stream, &bars(&[(20261117, -1.0), (20261118, 20.0)])?)?;
        bars_request(&mut stream, 0x1c, 0, 0, 2)?;
        let encoded = frame_bytes(&bars(&[(20261117, 10.0), (20261118, 20.0)])?)?;
        stream.write_all(&encoded[..36])?;
        partial.send(())?;
        eof(&mut stream)
    })?;
    let calls = Arc::new(AtomicUsize::new(0));
    let token = CancellationToken::default();
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls))))
        .build()?;
    let caller = client.clone();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?, &options));
    observed.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(
        join_call(call)?.expect_err("cancelled raw refetch").kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    client.close()?;
    join(task)
}

#[test]
fn cooperative_provider_observes_parent_cancel_and_worker_retirement_is_confirmed() -> Case {
    let (endpoint, task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source, entered, _release) = waiting(true);
    let token = CancellationToken::default();
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_xdxr_provider(source)
        .build()?;
    let caller = client.clone();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?, &options));
    entered.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(
        join_call(call)?.expect_err("provider cancellation").kind(),
        ErrorKind::Cancelled
    );
    client.close()?;
    join(task)
}

#[test]
fn close_during_provider_stops_the_same_registered_owner_and_shared_clones() -> Case {
    let (endpoint, task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source, entered, _release) = waiting(true);
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_xdxr_provider(source)
        .build()?;
    let caller = client.clone();
    let call = thread::spawn(move || caller.bars(&qfq()?, &OperationOptions::default()));
    entered.recv_timeout(Duration::from_secs(2))?;
    client.close()?;
    assert_eq!(
        join_call(call)?
            .expect_err("owner closed during provider")
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(
        client
            .connect(&OperationOptions::default())
            .expect_err("irreversible owner close")
            .kind(),
        ErrorKind::Closed
    );
    join(task)
}

#[test]
fn uncooperative_provider_reports_unconfirmed_cleanup_and_keeps_gate_until_actual_retirement(
) -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let server = thread::spawn(move || -> Case {
        let (mut first, _) = listener.accept()?;
        first.set_read_timeout(Some(Duration::from_secs(3)))?;
        qfq_then_raw(&mut first)?;
        let (mut next, _) = listener.accept()?;
        next.set_read_timeout(Some(Duration::from_secs(3)))?;
        setup_mac(&mut next)?;
        eof(&mut next)
    });
    let (source, entered, release) = waiting(false);
    let token = CancellationToken::default();
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_cleanup_timeout(Duration::from_millis(20))
        .with_xdxr_provider(source)
        .build()?;
    let caller = client.clone();
    let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?, &options));
    entered.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    let error = join_call(call)?.expect_err("blocked provider cannot confirm retirement");
    assert_eq!(error.kind(), ErrorKind::CancelUnconfirmed);
    assert!(error.cleanup_failure().is_some());
    // A free gate would let this connect finish against the already listening
    // second peer. The still-live worker must instead hold it until timeout.
    let queued =
        client.connect(&OperationOptions::default().with_timeout(Duration::from_millis(30)));
    assert_eq!(
        queued
            .expect_err("slot retained by unretired provider")
            .kind(),
        ErrorKind::Timeout
    );
    release.send(())?;
    client.connect(&OperationOptions::default())?;
    client.close()?;
    join(server)
}

#[test]
fn provider_receives_the_exact_original_deadline_and_cannot_renew_it() -> Case {
    let (endpoint, task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source, entered, _release) = waiting(true);
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_xdxr_provider(source)
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let caller = client.clone();
    let options = OperationOptions::default().with_deadline(deadline);
    let call = thread::spawn(move || caller.bars(&qfq()?, &options));
    assert_eq!(entered.recv_timeout(Duration::from_secs(2))?, deadline);
    assert_eq!(
        join_call(call)?.expect_err("total parent deadline").kind(),
        ErrorKind::Timeout
    );
    client.close()?;
    join(task)
}

#[test]
fn mac_ex_repair_uses_its_own_login_and_only_rewritten_mac_heads_on_the_same_owner() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        let login = wire::request(&mut stream, 1, 0x2454)?;
        assert_eq!(login.len(), 80);
        assert_eq!(&login[..4], &[0xe5, 0xbb, 0x1c, 0x2f]);
        frame(&mut stream, b"fixture login accepted")?;
        for (adjust, price) in [(1, -1.0), (0, 10.0)] {
            bars_request(&mut stream, 1, adjust, 0, 2)?;
            frame(&mut stream, &bars(&[(20261117, price), (20261118, 20.0)])?)?;
        }
        eof(&mut stream)
    })?;
    let rules = LoginReplyRules::new(
        vec![Bytes::from_static(b"fixture login accepted")],
        vec![Bytes::from_static(b"fixture login denied")],
        "synthetic public MAC-EX repair fixture only",
    )?;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules)
        .with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls))))
        .build()?;
    let id = eltdx::ex::ExInstrumentId::new(
        eltdx::ex::ExMarketId::from_wire(31),
        eltdx::ex::ExCode::new("00001")?,
    );
    let query = ex_query::BarsQuery::new(id)?
        .with_count(2)?
        .with_adjust(MacAdjust::Qfq);
    let rows = client.bars(&query, &OperationOptions::default())?;
    assert_eq!(rows[0].id().market().get(), 31);
    assert!((rows[0].close().value().as_f64() - 9.0).abs() < 1e-12);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(rows[0].meta().sources()[0].session_id() == rows[0].meta().sources()[1].session_id());
    client.close()?;
    join(task)
}

#[test]
fn raw_refetch_failover_repeats_setup_and_preserves_both_physical_sources() -> Case {
    let (first, first_task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        bars_request(&mut stream, 0x1c, 1, 0, 2)?;
        frame(&mut stream, &bars(&[(20261117, -1.0), (20261118, 20.0)])?)?;
        bars_request(&mut stream, 0x1c, 0, 0, 2)?;
        Ok(()) // Retire before a raw reply.
    })?;
    let (second, second_task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        bars_request(&mut stream, 0x1c, 0, 0, 2)?;
        frame(&mut stream, &bars(&[(20261117, 10.0), (20261118, 20.0)])?)?;
        eof(&mut stream)
    })?;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = MacClient::builder()
        .with_endpoints(vec![first, second])
        .with_extra_attempt_budget(2)
        .with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls))))
        .build()?;
    let rows = client.bars(&qfq()?, &OperationOptions::default())?;
    assert!((rows[0].close().value().as_f64() - 9.0).abs() < 1e-12);
    assert_eq!(rows[0].meta().sources()[0].endpoint(), Some(first));
    assert_eq!(rows[0].meta().sources()[1].endpoint(), Some(second));
    assert_ne!(
        rows[0].meta().sources()[0].session_id(),
        rows[0].meta().sources()[1].session_id()
    );
    client.close()?;
    join(first_task)?;
    join(second_task)
}
