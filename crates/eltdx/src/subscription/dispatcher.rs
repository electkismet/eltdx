//! One controlled runtime consumer per exact Engine. Routes retain weak sinks;
//! neither the dispatcher nor its worker retains the SDK session owner.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use eltdx_runtime::engine::{Engine, PushConsumer, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::push::PushFrame;
use eltdx_runtime::slot::GenerationIdentity;
use eltdx_runtime::RuntimeError;

use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::OperationContext;
use crate::session::OwnerTaskGroup;

/// Internal synchronous projection only: implementations cannot invoke user
/// callbacks, start requests or wait for renewal while the consumer dispatches.
pub(crate) trait PushSink: std::fmt::Debug + Send + Sync {
    fn frame(&self, frame: &PushFrame) -> Result<()>;
    fn gap(&self, dropped: u64, cumulative: u64);
    fn failed(&self, error: Error);
}

#[derive(Debug)]
struct Route { identity: u64, source: GenerationIdentity, sink: Weak<dyn PushSink> }

#[derive(Debug, Default)]
struct Routes { next_identity: u64, paused: usize, routes: Vec<Route> }

#[derive(Debug)]
struct Worker {
    consumer: Option<PushConsumer>,
    handle: Option<JoinHandle<()>>,
    finished: bool,
    failure: Option<Error>,
}

#[derive(Debug)]
pub(crate) struct Dispatcher {
    stopped: AtomicBool,
    routes: Mutex<Routes>,
    routing_changed: Condvar,
    worker: Mutex<Worker>,
    worker_changed: Condvar,
}

impl Dispatcher {
    /// Construction claims consumption but performs no networking or startup.
    /// The owner must register this task before calling start.
    pub(crate) fn new(engine: &Engine) -> Result<Arc<Self>> {
        let consumer = engine.claim_push_consumer().map_err(Error::from_runtime)?;
        Ok(Arc::new(Self { stopped: AtomicBool::new(false), routes: Mutex::new(Routes::default()),
            routing_changed: Condvar::new(), worker: Mutex::new(Worker {
                consumer: Some(consumer), handle: None, finished: false, failure: None,
            }), worker_changed: Condvar::new() }))
    }

    pub(crate) fn start(self: &Arc<Self>) -> Result<()> {
        let mut worker = self.worker.lock().map_err(|_| poisoned())?;
        if let Some(error) = &worker.failure { return Err(error.clone()); }
        if self.stopped.load(Ordering::Acquire) { return Err(closed()); }
        if worker.handle.is_some() { return Ok(()); }
        let Some(consumer) = worker.consumer.take() else { return Err(closed()); };
        let dispatcher = Arc::clone(self);
        match thread::Builder::new().name("eltdx-push-dispatch".to_owned()).spawn(move || {
            dispatcher.run(consumer);
        }) {
            Ok(handle) => { worker.handle = Some(handle); Ok(()) }
            Err(source) => {
                let error = Error::new(ErrorKind::Internal, "unable to start push dispatcher").with_source(source);
                worker.failure = Some(error.clone()); worker.finished = true;
                self.stopped.store(true, Ordering::Release);
                self.worker_changed.notify_all();
                Err(error)
            }
        }
    }

    /// Pause routing before activation and register exact source routes before
    /// dropping the guard. The worker may hold one pending bounded frame while
    /// paused; it does not discard early pushes or consume an unbounded backlog.
    pub(crate) fn pause(self: &Arc<Self>, context: &OperationContext) -> Result<DispatchPause> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let mut routes = self.routes.lock().map_err(|_| poisoned())?;
        if self.stopped.load(Ordering::Acquire) { return Err(closed()); }
        context.check_boundary(TimeoutPhase::Admission)?;
        routes.paused = routes.paused.checked_add(1).ok_or_else(||
            Error::new(ErrorKind::LimitExceeded, "push routing pause counter overflow"))?;
        Ok(DispatchPause { dispatcher: Arc::clone(self) })
    }

    pub(crate) fn register(self: &Arc<Self>, source: GenerationIdentity,
        sink: &Arc<dyn PushSink>) -> Result<RouteLease> {
        let mut routes = self.routes.lock().map_err(|_| poisoned())?;
        if self.stopped.load(Ordering::Acquire) { return Err(closed()); }
        let identity = routes.next_identity.checked_add(1).ok_or_else(||
            Error::new(ErrorKind::LimitExceeded, "push route identities exhausted"))?;
        routes.routes.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve push fanout route").with_source(error))?;
        routes.next_identity = identity;
        routes.routes.push(Route { identity, source, sink: Arc::downgrade(sink) });
        Ok(RouteLease { dispatcher: Arc::downgrade(self), identity })
    }

    fn sinks(&self, source: Option<GenerationIdentity>) -> Result<Vec<Arc<dyn PushSink>>> {
        let mut routes = self.routes.lock().map_err(|_| poisoned())?;
        while routes.paused != 0 && !self.stopped.load(Ordering::Acquire) {
            routes = self.routing_changed.wait_timeout(routes, SIGNAL_POLL_INTERVAL)
                .map_err(|_| poisoned())?.0;
        }
        if self.stopped.load(Ordering::Acquire) { return Ok(Vec::new()); }
        routes.routes.retain(|route| route.sink.strong_count() != 0);
        let mut sinks = Vec::new();
        sinks.try_reserve(routes.routes.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve dispatch targets").with_source(error))?;
        for route in &routes.routes {
            if source.is_some_and(|source| route.source != source) { continue; }
            if let Some(sink) = route.sink.upgrade() {
                // A subscription may own several routes; one raw loss affects
                // that subscription once, with unknown source bounds.
                if !sinks.iter().any(|existing| Arc::ptr_eq(existing, &sink)) { sinks.push(sink); }
            }
        }
        Ok(sinks)
    }

    fn dispatch(&self, frame: &PushFrame) -> Result<()> {
        let source = GenerationIdentity { engine_epoch: frame.engine_epoch, slot_id: frame.slot_id,
            generation: frame.generation };
        for sink in self.sinks(Some(source))? {
            if let Err(error) = sink.frame(frame) { sink.failed(error); }
        }
        Ok(())
    }

    fn run(&self, mut consumer: PushConsumer) {
        let result = (|| {
            let mut reported_dropped = 0_u64;
            while !self.stopped.load(Ordering::Acquire) {
                match consumer.poll_push(SIGNAL_POLL_INTERVAL) {
                    Ok(Some(frame)) => self.dispatch(&frame)?,
                    Ok(None) => {}
                    Err(RuntimeError::PushOverflow { dropped_total, .. }) => {
                        let dropped = dropped_total.saturating_sub(reported_dropped);
                        reported_dropped = dropped_total;
                        for sink in self.sinks(None)? { sink.gap(dropped, dropped_total); }
                    }
                    Err(_) if self.stopped.load(Ordering::Acquire) => break,
                    Err(error) => return Err(Error::from_runtime(error)),
                }
            }
            Ok(())
        })();
        // Release the exclusive claim before announcing worker completion.
        drop(consumer);
        if let Err(error) = &result {
            // Fatal delivery must bypass the establishment pause: parent close
            // cancels setup independently and must never wait on a new read.
            let targets = self.routes.lock().unwrap_or_else(|poison| poison.into_inner())
                .routes.iter().filter_map(|route| route.sink.upgrade()).collect::<Vec<_>>();
            for sink in targets { sink.failed(error.clone()); }
        }
        self.stopped.store(true, Ordering::Release);
        let mut worker = self.worker.lock().unwrap_or_else(|poison| poison.into_inner());
        worker.failure = result.err(); worker.finished = true;
        self.worker_changed.notify_all(); self.routing_changed.notify_all();
    }
}

impl OwnerTaskGroup for Dispatcher {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.routing_changed.notify_all(); self.worker_changed.notify_all();
    }

    fn close_until(&self, deadline: Instant) -> Result<()> {
        self.stop();
        let handle = {
            let mut worker = self.worker.lock().map_err(|_| poisoned())?;
            if worker.handle.is_none() {
                worker.consumer.take();
                return worker.failure.clone().map_or(Ok(()), Err);
            }
            loop {
                // A panicking worker cannot publish finished; an exited handle
                // must still be joined so its panic becomes the close error.
                let finished = worker.handle.as_ref().is_some_and(JoinHandle::is_finished);
                if finished { break; }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() { return Err(Error::new(ErrorKind::CloseTimeout, "push dispatcher did not retire before shutdown deadline")); }
                worker = self.worker_changed.wait_timeout(worker, remaining.min(SIGNAL_POLL_INTERVAL))
                    .map_err(|_| poisoned())?.0;
            }
            worker.handle.take()
        };
        if let Some(handle) = handle {
            handle.join().map_err(|_| Error::new(ErrorKind::Internal, "push dispatcher panicked"))?;
        }
        self.worker.lock().map_err(|_| poisoned())?.failure.clone().map_or(Ok(()), Err)
    }
}

/// Unique route registration. Removing one subscription never stops other
/// routes, the shared dispatcher or the Engine owner.
#[derive(Debug)]
pub(crate) struct RouteLease { dispatcher: Weak<Dispatcher>, identity: u64 }

impl Drop for RouteLease {
    fn drop(&mut self) {
        if let Some(dispatcher) = self.dispatcher.upgrade() {
            let mut routes = dispatcher.routes.lock().unwrap_or_else(|poison| poison.into_inner());
            routes.routes.retain(|route| route.identity != self.identity);
        }
    }
}

pub(crate) struct DispatchPause { dispatcher: Arc<Dispatcher> }

impl Drop for DispatchPause {
    fn drop(&mut self) {
        let mut routes = self.dispatcher.routes.lock().unwrap_or_else(|poison| poison.into_inner());
        routes.paused -= 1;
        self.dispatcher.routing_changed.notify_all();
    }
}

fn poisoned() -> Error { Error::new(ErrorKind::Internal, "push dispatcher state is poisoned") }
fn closed() -> Error { Error::new(ErrorKind::Closed, "push dispatcher is stopped") }

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use eltdx_runtime::slot::{EngineEpoch, GenerationId, SlotId};

    #[derive(Debug, Default)]
    struct Sink { messages: Mutex<Vec<u32>>, gaps: Mutex<Vec<(u64, u64)>>, failures: Mutex<Vec<Error>> }
    impl PushSink for Sink {
        fn frame(&self, frame: &PushFrame) -> Result<()> {
            self.messages.lock().unwrap().push(frame.response.msg_id); Ok(())
        }
        fn gap(&self, delta: u64, total: u64) { self.gaps.lock().unwrap().push((delta, total)); }
        fn failed(&self, error: Error) { self.failures.lock().unwrap().push(error); }
    }

    fn source(epoch: u64, slot: usize, generation: u64) -> GenerationIdentity {
        GenerationIdentity { engine_epoch: EngineEpoch::new(epoch).unwrap(), slot_id: SlotId::new(slot),
            generation: GenerationId::new(generation).unwrap() }
    }

    fn frame(source: GenerationIdentity, message: u32) -> PushFrame {
        PushFrame { engine_epoch: source.engine_epoch, slot_id: source.slot_id,
            generation: source.generation, connected_host: Arc::from("127.0.0.1:7709"),
            response: eltdx_protocol::frame::ResponseFrame { control: 1, msg_id: message,
                msg_type: 0x0547, zip_length: 2, length: 2,
                data: bytes::Bytes::from_static(&[0x93, 0x93]), raw: bytes::Bytes::new(),
                response_header_reserved:0 } }
    }

    fn lazy() -> (Engine, Arc<Dispatcher>) {
        let config = crate::config::ConfigDraft::default().validate().unwrap();
        let engine = crate::client::engine_with_hosts(&config, vec!["127.0.0.1:9".to_owned()]).unwrap();
        let dispatcher = Dispatcher::new(&engine).unwrap();
        (engine, dispatcher)
    }

    #[test]
    fn fanout_routes_the_same_frame_to_each_sink_and_drops_only_one_lease() {
        let (_, dispatcher) = lazy();
        let identity = source(1, 0, 1);
        let first = Arc::new(Sink::default()); let second = Arc::new(Sink::default());
        let first_sink: Arc<dyn PushSink> = first.clone();
        let second_sink: Arc<dyn PushSink> = second.clone();
        let first_route = dispatcher.register(identity, &first_sink).unwrap();
        let _second_route = dispatcher.register(identity, &second_sink).unwrap();
        dispatcher.dispatch(&frame(identity, 10)).unwrap();
        assert_eq!(*first.messages.lock().unwrap(), vec![10]);
        assert_eq!(*second.messages.lock().unwrap(), vec![10]);
        drop(first_route);
        dispatcher.dispatch(&frame(identity, 11)).unwrap();
        assert_eq!(*first.messages.lock().unwrap(), vec![10]);
        assert_eq!(*second.messages.lock().unwrap(), vec![10, 11]);
    }

    #[test]
    fn every_source_component_is_checked_before_delivering_any_frame() {
        let (_, dispatcher) = lazy();
        let sink = Arc::new(Sink::default()); let target: Arc<dyn PushSink> = sink.clone();
        let _route = dispatcher.register(source(2, 3, 4), &target).unwrap();
        for stale in [source(1, 3, 4), source(2, 2, 4), source(2, 3, 3)] {
            dispatcher.dispatch(&frame(stale, 1)).unwrap();
        }
        assert!(sink.messages.lock().unwrap().is_empty());
        dispatcher.dispatch(&frame(source(2, 3, 4), 2)).unwrap();
        assert_eq!(*sink.messages.lock().unwrap(), vec![2]);
    }

    #[test]
    fn several_group_routes_deliver_one_unknown_source_loss_per_sink() {
        let (_, dispatcher) = lazy();
        let sink = Arc::new(Sink::default()); let target: Arc<dyn PushSink> = sink.clone();
        let _first = dispatcher.register(source(1, 0, 1), &target).unwrap();
        let _second = dispatcher.register(source(1, 1, 1), &target).unwrap();
        for sink in dispatcher.sinks(None).unwrap() { sink.gap(3, 8); }
        assert_eq!(*sink.gaps.lock().unwrap(), vec![(3, 8)]);
    }

    #[test]
    fn stop_wakes_an_establishment_pause_without_waiting_for_its_guard() {
        let (_, dispatcher) = lazy();
        let config = crate::config::ConfigDraft::default().validate().unwrap();
        let owner = Arc::new(crate::session::SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        let context = crate::operation::OperationContext::rust_public(Instant::now(), "pause-source-case",
            owner.clone(), &config, &crate::OperationOptions::default()).unwrap();
        let pause = dispatcher.pause(&context).unwrap();
        let worker_dispatcher = dispatcher.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            entered_tx.send(()).unwrap();
            done_tx.send(worker_dispatcher.sinks(None).unwrap().len()).unwrap();
        });
        entered_rx.recv().unwrap();
        dispatcher.stop();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 0);
        worker.join().unwrap();
        drop(pause); drop(context); owner.close().unwrap();
    }

    #[test]
    fn consumer_claim_is_exclusive_and_released_without_starting_a_worker() {
        let (engine, dispatcher) = lazy();
        assert!(matches!(engine.claim_push_consumer(), Err(RuntimeError::PoolBusy { .. })));
        dispatcher.close_until(Instant::now() + Duration::from_secs(1)).unwrap();
        let consumer = engine.claim_push_consumer().unwrap();
        drop(consumer);
    }
}
