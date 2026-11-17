//! Authored public-owner/race sources. Execution waits for T01-T30 completion.
//! All login strings here are explicit synthetic fixtures, never production ACKs.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"] mod wire;
use std::io::Write;
use std::net::TcpListener;
use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use bytes::Bytes;
use eltdx::{CancellationToken, ErrorKind, OperationOptions, TradingDate};
use eltdx::mac::*;
use eltdx::mac::handshake::LoginReplyRules;
use wire::{Case, bars, bars_request, eof, frame, frame_bytes, join, peer, qfq, qfq_then_raw, setup_mac};

fn events() -> eltdx::Result<XdxrBatch> {
    Ok(XdxrBatch::new(vec![XdxrEvent { fenhong: 1.0,..XdxrEvent::new(TradingDate::new(2026,11,18)?) }]))
}
#[derive(Debug)]
struct CountingProvider(Arc<AtomicUsize>);
impl XdxrProvider for CountingProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        self.0.fetch_add(1,Ordering::SeqCst); context.check_boundary()?; events()
    }
}
#[derive(Debug)]
struct WaitingProvider {
    entered: mpsc::Sender<Instant>, release: Mutex<mpsc::Receiver<()>>, cooperative: bool,
}
impl XdxrProvider for WaitingProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        self.entered.send(context.deadline()).map_err(|_| eltdx::Error::new(ErrorKind::Internal,"fixture observer dropped"))?;
        let release = self.release.lock().map_err(|_| eltdx::Error::new(ErrorKind::Internal,"fixture release poisoned"))?;
        if self.cooperative {
            loop {
                context.check_boundary()?;
                match release.recv_timeout(Duration::from_millis(5)) {
                    Ok(()) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Err(eltdx::Error::new(ErrorKind::Internal,"fixture release disconnected")),
                }
            }
        } else { release.recv().map_err(|_| eltdx::Error::new(ErrorKind::Internal,"fixture release disconnected"))?; }
        context.check_boundary()?; events()
    }
}
fn waiting(cooperative: bool) -> (Arc<WaitingProvider>,mpsc::Receiver<Instant>,mpsc::Sender<()>) {
    let (entered,observer) = mpsc::channel(); let (release,receiver) = mpsc::channel();
    (Arc::new(WaitingProvider { entered,release: Mutex::new(receiver),cooperative }),observer,release)
}
fn join_call<T>(task: thread::JoinHandle<eltdx::Result<T>>) -> Case<eltdx::Result<T>> {
    task.join().map_err(|_| std::io::Error::other("public call fixture panicked").into())
}

#[test]
fn cancellation_during_unadjusted_half_frame_propagates_and_never_invokes_provider() -> Case {
    let (partial,observed) = mpsc::channel();
    let (endpoint,task) = peer(move |mut stream| {
        setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,1,0,2)?;
        frame(&mut stream,&bars(&[(20261117,-1.0),(20261118,20.0)])?)?;
        bars_request(&mut stream,0x1c,0,0,2)?;
        let encoded = frame_bytes(&bars(&[(20261117,10.0),(20261118,20.0)])?)?;
        stream.write_all(&encoded[..36])?; partial.send(())?; eof(&mut stream)
    })?;
    let calls = Arc::new(AtomicUsize::new(0)); let token = CancellationToken::default();
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls)))).build()?;
    let caller = client.clone(); let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?,&options));
    observed.recv_timeout(Duration::from_secs(2))?; token.cancel();
    assert_eq!(join_call(call)?.expect_err("cancelled raw refetch").kind(),ErrorKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst),0); client.close()?; join(task)
}

#[test]
fn cooperative_provider_observes_parent_cancel_and_worker_retirement_is_confirmed() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source,entered,_release) = waiting(true); let token = CancellationToken::default();
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
    let caller = client.clone(); let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?,&options));
    entered.recv_timeout(Duration::from_secs(2))?; token.cancel();
    assert_eq!(join_call(call)?.expect_err("provider cancellation").kind(),ErrorKind::Cancelled);
    client.close()?; join(task)
}

#[test]
fn close_during_provider_stops_the_same_registered_owner_and_shared_clones() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source,entered,_release) = waiting(true);
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
    let caller = client.clone(); let call = thread::spawn(move || caller.bars(&qfq()?,&OperationOptions::default()));
    entered.recv_timeout(Duration::from_secs(2))?; client.close()?;
    assert_eq!(join_call(call)?.expect_err("owner closed during provider").kind(),ErrorKind::Closed);
    assert_eq!(client.connect(&OperationOptions::default()).expect_err("irreversible owner close").kind(),ErrorKind::Closed);
    join(task)
}

#[test]
fn uncooperative_provider_reports_unconfirmed_cleanup_and_keeps_gate_until_actual_retirement() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0))?; let endpoint = listener.local_addr()?;
    let server = thread::spawn(move || -> Case {
        let (mut first,_) = listener.accept()?; first.set_read_timeout(Some(Duration::from_secs(3)))?;
        qfq_then_raw(&mut first)?;
        let (mut next,_) = listener.accept()?; next.set_read_timeout(Some(Duration::from_secs(3)))?;
        setup_mac(&mut next)?; eof(&mut next)
    });
    let (source,entered,release) = waiting(false); let token = CancellationToken::default();
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_cleanup_timeout(Duration::from_millis(20))
        .with_xdxr_provider(source).build()?;
    let caller = client.clone(); let options = OperationOptions::default().with_cancellation(token.clone());
    let call = thread::spawn(move || caller.bars(&qfq()?,&options));
    entered.recv_timeout(Duration::from_secs(2))?; token.cancel();
    let error = join_call(call)?.expect_err("blocked provider cannot confirm retirement");
    assert_eq!(error.kind(),ErrorKind::CancelUnconfirmed); assert!(error.cleanup_failure().is_some());
    // A free gate would let this connect finish against the already listening
    // second peer. The still-live worker must instead hold it until timeout.
    let queued = client.connect(&OperationOptions::default().with_timeout(Duration::from_millis(30)));
    assert_eq!(queued.expect_err("slot retained by unretired provider").kind(),ErrorKind::Timeout);
    release.send(())?;
    client.connect(&OperationOptions::default())?; client.close()?; join(server)
}

#[test]
fn provider_receives_the_exact_original_deadline_and_cannot_renew_it() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let (source,entered,_release) = waiting(true);
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
    let deadline = Instant::now()+Duration::from_secs(2);
    let caller = client.clone(); let options = OperationOptions::default().with_deadline(deadline);
    let call = thread::spawn(move || caller.bars(&qfq()?,&options));
    assert_eq!(entered.recv_timeout(Duration::from_secs(2))?,deadline);
    assert_eq!(join_call(call)?.expect_err("total parent deadline").kind(),ErrorKind::Timeout);
    client.close()?; join(task)
}

#[test]
fn mac_ex_repair_uses_its_own_login_and_only_rewritten_mac_heads_on_the_same_owner() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        let login = wire::request(&mut stream,1,0x2454)?; assert_eq!(login.len(),80);
        assert_eq!(&login[..4],&[0xe5,0xbb,0x1c,0x2f]); frame(&mut stream,b"fixture login accepted")?;
        for (adjust,price) in [(1,-1.0),(0,10.0)] {
            bars_request(&mut stream,1,adjust,0,2)?;
            frame(&mut stream,&bars(&[(20261117,price),(20261118,20.0)])?)?;
        }
        eof(&mut stream)
    })?;
    let rules = LoginReplyRules::new(vec![Bytes::from_static(b"fixture login accepted")],
        vec![Bytes::from_static(b"fixture login denied")],"synthetic public MAC-EX repair fixture only")?;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = MacExClient::builder().with_endpoints(vec![endpoint]).with_login_reply_rules(rules)
        .with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls)))).build()?;
    let id = eltdx::ex::ExInstrumentId::new(eltdx::ex::ExMarketId::from_wire(31),eltdx::ex::ExCode::new("00001")?);
    let query = ex_query::BarsQuery::new(id)?.with_count(2)?.with_adjust(MacAdjust::Qfq);
    let rows = client.bars(&query,&OperationOptions::default())?;
    assert_eq!(rows[0].id().market().get(),31); assert!((rows[0].close().value().as_f64()-9.0).abs() < 1e-12);
    assert_eq!(calls.load(Ordering::SeqCst),1);
    assert!(rows[0].meta().sources()[0].session_id() == rows[0].meta().sources()[1].session_id());
    client.close()?; join(task)
}

#[test]
fn raw_refetch_failover_repeats_setup_and_preserves_both_physical_sources() -> Case {
    let (first,first_task) = peer(|mut stream| {
        setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,1,0,2)?;
        frame(&mut stream,&bars(&[(20261117,-1.0),(20261118,20.0)])?)?;
        bars_request(&mut stream,0x1c,0,0,2)?; Ok(()) // Retire before a raw reply.
    })?;
    let (second,second_task) = peer(|mut stream| {
        setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,0,0,2)?;
        frame(&mut stream,&bars(&[(20261117,10.0),(20261118,20.0)])?)?; eof(&mut stream)
    })?;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = MacClient::builder().with_endpoints(vec![first,second]).with_extra_attempt_budget(2)
        .with_xdxr_provider(Arc::new(CountingProvider(Arc::clone(&calls)))).build()?;
    let rows = client.bars(&qfq()?,&OperationOptions::default())?;
    assert!((rows[0].close().value().as_f64()-9.0).abs() < 1e-12);
    assert_eq!(rows[0].meta().sources()[0].endpoint(),Some(first));
    assert_eq!(rows[0].meta().sources()[1].endpoint(),Some(second));
    assert_ne!(rows[0].meta().sources()[0].session_id(),rows[0].meta().sources()[1].session_id());
    client.close()?; join(first_task)?; join(second_task)
}
