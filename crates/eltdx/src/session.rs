use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

static NEXT_OWNER_GENERATION: AtomicUsize = AtomicUsize::new(1);

use eltdx_runtime::engine::Engine;
use eltdx_runtime::engine::{PendingPoll, PinHandle, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::RuntimeError;

use crate::error::{Error, ErrorKind, Result};
use crate::metadata::MetadataStore;
use crate::types::time::ClockHandle;

/// Owner-controlled work uses independent state and never strongly captures its owner.
/// stop is nonblocking; close_until confirms retirement outside the owner state lock.
pub(crate) trait OwnerTaskGroup: std::fmt::Debug + Send + Sync {
    fn stop(&self);
    fn stop_before(&self, _deadline: Instant) { self.stop(); }
    fn close_until(&self, deadline: Instant) -> Result<()>;
    fn completed(&self) -> bool { false }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionPhase {
    Open,
    Closing,
    Closed,
    FailedClosed,
}

#[derive(Clone, Debug)]
struct CloseFailure {
    kind: ErrorKind,
    message: String,
    context: Vec<(String, String)>,
    runtime_source: Option<RuntimeError>,
}

impl CloseFailure {
    fn from_error(error: Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
            context: error.context().to_vec(),
            runtime_source: std::error::Error::source(&error)
                .and_then(|source| source.downcast_ref::<RuntimeError>())
                .cloned(),
        }
    }

    fn into_error(&self) -> Error {
        let mut error = Error::new(self.kind, self.message.clone());
        for (key, value) in &self.context {
            error = error.with_context(key.clone(), value.clone());
        }
        if let Some(source) = &self.runtime_source {
            error = error.with_source(source.clone());
        }
        error
    }
}

#[derive(Debug)]
struct SessionState {
    phase: SessionPhase,
    shutdown_deadline: Option<Instant>,
    close_failure: Option<CloseFailure>,
    close_generation: u64,
    completed_generation: u64,
    last_result: Option<std::result::Result<(), CloseFailure>>,
    cleanup_confirmed: bool,
    active_operations: usize,
    operations: Vec<Weak<OperationCancellation>>,
    engines: Vec<Engine>,
    endpoint_engines: Vec<(Engine, String, Engine)>,
    pins: Vec<PinHandle>,
    tasks: Vec<Arc<dyn OwnerTaskGroup>>,
    dispatchers: Vec<(Engine, Arc<crate::subscription::dispatcher::Dispatcher>)>,
}

#[derive(Debug)]
enum CleanupSignal {
    Drop { engines: Vec<Engine>, tasks: Vec<Arc<dyn OwnerTaskGroup>>, deadline: Instant },
    ExplicitComplete,
}

#[derive(Debug, Default)]
struct CleanupState {
    signal: Option<CleanupSignal>,
    worker: Option<JoinHandle<()>>,
}

/// Independent retirement state. Its worker owns neither SessionOwner nor an
/// Engine task's parent; explicit close can confirm it, while Drop only signals.
#[derive(Debug, Default)]
struct CleanupControl {
    state: Mutex<CleanupState>,
    changed: Condvar,
}

impl CleanupControl {
    fn ensure_started(self: &Arc<Self>) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned_state_error())?;
        if state.worker.is_some() || state.signal.is_some() { return Ok(()); }
        let control = Arc::clone(self);
        let worker = thread::Builder::new().name("eltdx-owner-cleanup".to_owned()).spawn(move || {
            let signal = {
                let mut state = match control.state.lock() { Ok(state) => state, Err(poison) => poison.into_inner() };
                while state.signal.is_none() {
                    state = match control.changed.wait(state) { Ok(state) => state, Err(poison) => poison.into_inner() };
                }
                state.signal.take()
            };
            if let Some(CleanupSignal::Drop { engines, tasks, deadline }) = signal {
                for task in &tasks { task.stop_before(deadline); }
                for engine in engines { let _ = engine.close_until(deadline); }
                for task in tasks { let _ = task.close_until(deadline); }
            }
        }).map_err(|error| Error::new(ErrorKind::Internal, "unable to start owner cleanup controller").with_source(error))?;
        state.worker = Some(worker);
        Ok(())
    }

    fn signal_drop(&self, engines: Vec<Engine>, tasks: Vec<Arc<dyn OwnerTaskGroup>>, deadline: Instant) {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(worker) = state.worker.as_ref() {
            if worker.thread().id() != thread::current().id() {
                state.signal = Some(CleanupSignal::Drop { engines, tasks, deadline });
                self.changed.notify_all();
                return;
            }
        }
        // No SDK-owned work could have started before the lazy controller was
        // armed. Release handles here; EngineInner itself only signals shutdown.
        drop(state);
        drop(engines);
        for task in tasks { task.stop(); }
    }

    fn explicit_complete(&self) -> Result<()> {
        let worker = {
            let mut state = self.state.lock().map_err(|_| poisoned_state_error())?;
            let Some(worker) = state.worker.take() else { return Ok(()); };
            if worker.thread().id() == thread::current().id() {
                state.worker = Some(worker);
                return Err(Error::new(ErrorKind::Internal, "cleanup controller cannot join itself"));
            }
            state.signal = Some(CleanupSignal::ExplicitComplete);
            self.changed.notify_all();
            worker
        };
        worker.join().map_err(|_| Error::new(ErrorKind::Internal, "owner cleanup controller panicked"))
    }
}

/// Shared owner for clients, services, operations, pins, and subscriptions.
#[derive(Debug)]
pub(crate) struct SessionOwner {
    metadata: Arc<MetadataStore>,
    calendar: crate::calendar::CalendarCache,
    helpers: crate::helpers::HelperCache,
    generation: usize,
    clock: ClockHandle,
    close_timeout: Duration,
    cleanup_timeout: Duration,
    state: Mutex<SessionState>,
    changed: Condvar,
    cleanup: Arc<CleanupControl>,
}

impl SessionOwner {
    pub(crate) fn new(close_timeout: Duration, cleanup_timeout: Duration) -> Self {
        Self::new_with_clock(close_timeout, cleanup_timeout, ClockHandle::default())
    }

    pub(crate) fn new_with_clock(close_timeout: Duration, cleanup_timeout: Duration, clock: ClockHandle) -> Self {
        let metadata = Arc::new(MetadataStore::new(Arc::clone(&clock.0)));
        Self {
            metadata: Arc::clone(&metadata),
            calendar: crate::calendar::CalendarCache::default(),
            helpers: crate::helpers::HelperCache::default(),
            generation: NEXT_OWNER_GENERATION.fetch_add(1, Ordering::Relaxed),
            clock,
            close_timeout,
            cleanup_timeout,
            state: Mutex::new(SessionState {
                phase: SessionPhase::Open,
                shutdown_deadline: None,
                close_failure: None,
                close_generation: 0,
                completed_generation: 0,
                last_result: None,
                cleanup_confirmed: false,
                active_operations: 0,
                operations: Vec::new(),
                engines: Vec::new(),
                endpoint_engines: Vec::new(),
                pins: Vec::new(),
                tasks: vec![metadata as Arc<dyn OwnerTaskGroup>],
                dispatchers: Vec::new(),
            }),
            changed: Condvar::new(),
            cleanup: Arc::new(CleanupControl::default()),
        }
    }

    pub(crate) fn metadata(&self) -> &Arc<MetadataStore> { &self.metadata }
    pub(crate) fn calendar(&self) -> &crate::calendar::CalendarCache { &self.calendar }
    pub(crate) fn helpers(&self) -> &crate::helpers::HelperCache { &self.helpers }

    pub(crate) const fn generation(&self) -> usize { self.generation }

    pub(crate) fn current_shanghai_date(&self) -> Result<crate::types::TradingDate> {
        crate::types::TradingDate::shanghai_today(self.clock.0.as_ref())
    }

    pub(crate) fn register_task_group(&self, task: Arc<dyn OwnerTaskGroup>) -> Result<()> {
        self.reap_completed_tasks();
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open { return Err(closed_error()); }
        self.cleanup.ensure_started()?;
        state.tasks.push(task);
        Ok(())
    }

    fn reap_completed_tasks(&self) {
        let tasks = match self.state.lock() {
            Ok(state) => state.tasks.clone(),
            Err(_) => return,
        };
        // A task's completion check may lock its own state and consult its
        // owner. Call every task method after releasing the owner state lock.
        // Retain any unconfirmed cleanup for explicit close.
        for task in tasks {
            if task.completed() && task.close_until(Instant::now()).is_ok() {
                self.unregister_task_group(&task);
            }
        }
    }

    pub(crate) fn unregister_task_group(&self, task: &Arc<dyn OwnerTaskGroup>) {
        if let Ok(mut state) = self.state.lock() { state.tasks.retain(|registered| !Arc::ptr_eq(registered, task)); }
    }

    pub(crate) fn retire_registered_pin(&self, pin: &PinHandle, deadline: Instant) -> Result<()> {
        self.close_pin(pin, Some(deadline))?;
        self.unregister_pin(pin);
        Ok(())
    }

    /// The cache key is exact Engine ownership, not an endpoint or slot number.
    /// Construction is lazy; register stop/join before permitting worker start.
    pub(crate) fn push_dispatcher(&self, engine: &Engine) -> Result<Arc<crate::subscription::dispatcher::Dispatcher>> {
        let dispatcher = {
            let mut state = self.lock_state()?;
            if state.phase != SessionPhase::Open { return Err(closed_error()); }
            if let Some((_, dispatcher)) = state.dispatchers.iter().find(|(source, _)| source.same_owner(engine)) {
                Arc::clone(dispatcher)
            } else {
                self.cleanup.ensure_started()?;
                state.tasks.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded,
                    "unable to register push dispatcher").with_source(error))?;
                state.dispatchers.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded,
                    "unable to cache push dispatcher").with_source(error))?;
                let dispatcher = crate::subscription::dispatcher::Dispatcher::new(engine)?;
                state.tasks.push(Arc::clone(&dispatcher) as Arc<dyn OwnerTaskGroup>);
                state.dispatchers.push((engine.clone(), Arc::clone(&dispatcher)));
                dispatcher
            }
        };
        dispatcher.start()?;
        Ok(dispatcher)
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        let state = self.lock_state()?;
        if state.phase == SessionPhase::Open {
            Ok(())
        } else {
            Err(closed_error())
        }
    }

    pub(crate) fn endpoint_engine(&self, base: &Engine, endpoint: &str,
        create: impl FnOnce() -> Result<Engine>) -> Result<Engine> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open { return Err(closed_error()); }
        if let Some((_, _, engine)) = state.endpoint_engines.iter()
            .find(|(source, host, _)| source.same_owner(base) && host == endpoint) {
            return Ok(engine.clone());
        }
        // Factory only constructs lazy in-memory Engine state; no networking,
        // worker startup, metadata lookup or callback occurs under this lock.
        let engine = create()?;
        state.endpoint_engines.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve endpoint-bound engine cache").with_source(error))?;
        state.endpoint_engines.push((base.clone(), endpoint.to_owned(), engine.clone()));
        Ok(engine)
    }

    // The closure is a bounded counter/terminal-state transaction, never I/O or a wait.
    // Lock order for operation admission is owner -> business token -> budget.
    pub(crate) fn with_open_admission<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.lock_state()?;
        if state.phase != SessionPhase::Open { return Err(closed_error()); }
        action()
    }

    pub(crate) fn shutdown_deadline(&self) -> Result<Option<Instant>> {
        Ok(self.lock_state()?.shutdown_deadline)
    }

    pub(crate) fn admit(self: &Arc<Self>) -> Result<SessionOperation> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open {
            return Err(closed_error());
        }
        state.active_operations = state
            .active_operations
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Internal, "active operation count overflow"))?;
        let cancellation = Arc::new(OperationCancellation::default());
        state.operations.push(Arc::downgrade(&cancellation));
        Ok(SessionOperation {
            owner: Arc::clone(self),
            cancellation,
        })
    }

    pub(crate) fn register_pin(&self, pin: PinHandle) -> Result<()> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open {
            return Err(closed_error());
        }
        self.cleanup.ensure_started()?;
        state.pins.push(pin);
        Ok(())
    }

    fn unregister_pin(&self, pin: &PinHandle) {
        if let Ok(mut state) = self.state.lock() {
            state.pins.retain(|registered| !registered.same_reservation(pin));
        }
    }

    fn pin_registered(&self, pin: &PinHandle) -> Result<bool> {
        Ok(self.lock_state()?.pins.iter().any(|registered| registered.same_reservation(pin)))
    }

    fn reconcile_engine_pins(&self, engine: &Engine) -> Result<()> {
        let mut state = self.lock_state()?;
        state.pins.retain(|pin| !pin.belongs_to_engine(engine));
        self.changed.notify_all();
        Ok(())
    }

    // Share the owner deadline even when a pin close began while the owner was open.
    // Never hold the owner state lock while awaiting runtime retirement.
    fn close_pin(&self, pin: &PinHandle, deadline: Option<Instant>) -> Result<()> {
        if !self.pin_registered(pin)? {
            return Ok(());
        }
        let local_deadline = match deadline {
            Some(deadline) => deadline,
            None => Instant::now().checked_add(self.close_timeout)
                .ok_or_else(|| Error::new(ErrorKind::Internal, "pin close deadline overflow"))?,
        };
        let mut pending = match pin.begin_close() {
            Ok(pending) => pending,
            Err(error) => {
                return if !self.pin_registered(pin)? { Ok(()) } else { Err(map_close_error(error)) };
            }
        };
        match self.wait_pin_close(local_deadline, || pending.wait_timeout(Duration::ZERO)) {
            Ok(()) => Ok(()),
            Err(error) => {
                if !self.pin_registered(pin)? { Ok(()) } else { Err(error) }
            }
        }
    }

    fn wait_pin_close(
        &self,
        local_deadline: Instant,
        mut poll: impl FnMut() -> std::result::Result<PendingPoll<()>, RuntimeError>,
    ) -> Result<()> {
        loop {
            match poll().map_err(map_close_error)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
            let state = self.lock_state()?;
            let deadline = state.shutdown_deadline.map_or(local_deadline, |owner_deadline| {
                local_deadline.min(owner_deadline)
            });
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(map_close_error(RuntimeError::CloseTimeout {
                    message: "pinned session did not close before its owner deadline".to_owned(),
                    context: Vec::new(),
                }));
            }
            let _ = self.changed.wait_timeout(state, remaining.min(SIGNAL_POLL_INTERVAL))
                .map_err(|_| poisoned_state_error())?;
        }
    }

    pub(crate) fn register_engine(&self, engine: Engine) -> Result<()> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open { return Err(closed_error()); }
        self.cleanup.ensure_started()?;
        register_engine_locked(&mut state, engine);
        Ok(())
    }

    pub(crate) fn register_engine_lazy(&self, engine: Engine) -> Result<()> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open {
            return Err(closed_error());
        }
        register_engine_locked(&mut state, engine);
        Ok(())
    }

    pub(crate) fn close(&self) -> Result<()> {
        self.close_with_pin_closer(|pin, deadline| self.close_pin(pin, Some(deadline)))
    }

    fn close_with_pin_closer(
        &self,
        mut close_pin: impl FnMut(&PinHandle, Instant) -> Result<()>,
    ) -> Result<()> {
        let (deadline, generation, engines, pins, tasks, prior_failure) = {
            let mut state = self.lock_state()?;
            loop {
                match state.phase {
                    SessionPhase::Closed => return Ok(()),
                    SessionPhase::Closing => {
                        let generation = state.close_generation;
                        let deadline = state.shutdown_deadline.ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "closing session has no deadline")
                        })?;
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            return Err(state
                                .last_result
                                .as_ref()
                                .and_then(|result| result.as_ref().err())
                                .map_or_else(close_timeout_error, CloseFailure::into_error));
                        }
                        let (next, wait) = self
                            .changed
                            .wait_timeout(state, remaining)
                            .map_err(|_| poisoned_state_error())?;
                        state = next;
                        if state.completed_generation == generation {
                            return state
                                .last_result
                                .as_ref()
                                .ok_or_else(|| {
                                    Error::new(
                                        ErrorKind::Internal,
                                        "completed close attempt has no result",
                                    )
                                })?
                                .clone()
                                .map_err(|failure| failure.into_error());
                        }
                        if wait.timed_out() && state.phase == SessionPhase::Closing {
                            return Err(close_timeout_error());
                        }
                    }
                    SessionPhase::Open => {
                        let deadline = Instant::now()
                            .checked_add(self.close_timeout)
                            .ok_or_else(|| {
                                Error::new(ErrorKind::Internal, "close deadline overflow")
                            })?;
                        state.shutdown_deadline = Some(deadline);
                        state.phase = SessionPhase::Closing;
                        signal_operation_cancellation(&mut state);
                        self.changed.notify_all();
                        state.close_failure = None;
                        state.cleanup_confirmed = false;
                        state.close_generation = state.close_generation.checked_add(1).ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "close generation overflow")
                        })?;
                        break (
                            deadline,
                            state.close_generation,
                            state.engines.clone(),
                            state.pins.clone(),
                            state.tasks.clone(),
                            None,
                        );
                    }
                    SessionPhase::FailedClosed => {
                        let prior_failure = state.close_failure.clone().ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "failed session has no failure lineage")
                        })?;
                        if state.cleanup_confirmed {
                            return Err(prior_failure.into_error());
                        }
                        let deadline = Instant::now()
                            .checked_add(self.cleanup_timeout)
                            .ok_or_else(|| {
                                Error::new(ErrorKind::Internal, "cleanup deadline overflow")
                            })?;
                        state.shutdown_deadline = Some(deadline);
                        state.phase = SessionPhase::Closing;
                        signal_operation_cancellation(&mut state);
                        self.changed.notify_all();
                        state.close_generation = state.close_generation.checked_add(1).ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "close generation overflow")
                        })?;
                        break (
                            deadline,
                            state.close_generation,
                            state.engines.clone(),
                            state.pins.clone(),
                            state.tasks.clone(),
                            Some(prior_failure),
                        );
                    }
                }
            }
        };

        let mut failure = None;
        for task in &tasks { task.stop_before(deadline); }
        for pin in pins {
            match close_pin(&pin, deadline) {
                Ok(()) => self.unregister_pin(&pin),
                Err(error) => { failure.get_or_insert(error); }
            }
        }
        let mut engines_confirmed = true;
        for engine in engines {
            match engine.close_until(deadline) {
                Ok(()) => {
                    // Only full, confirmed shutdown of this Engine proves its
                    // remaining pins have retired. Other pools stay registered.
                    self.reconcile_engine_pins(&engine)?;
                }
                Err(error) => {
                    engines_confirmed = false;
                    failure.get_or_insert_with(|| map_close_error(error));
                }
            }
        }

        let mut tasks_confirmed = true;
        for task in tasks {
            if let Err(error) = task.close_until(deadline) {
                tasks_confirmed = false;
                failure.get_or_insert(error);
            }
        }

        let mut state = self.lock_state()?;
        while state.active_operations != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                failure.get_or_insert_with(close_timeout_error);
                break;
            }
            let (next, wait) = self
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| poisoned_state_error())?;
            state = next;
            if wait.timed_out() && state.active_operations != 0 {
                failure.get_or_insert_with(close_timeout_error);
                break;
            }
        }

        let cleanup_confirmed = engines_confirmed && tasks_confirmed && state.pins.is_empty() && state.active_operations == 0;
        let terminal = match (prior_failure, failure) {
            (Some(root), Some(_)) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = cleanup_confirmed;
                state.last_result = Some(Err(root.clone()));
                Err(root.into_error())
            }
            (Some(root), None) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = cleanup_confirmed;
                state.last_result = Some(Err(root.clone()));
                Err(root.into_error())
            }
            (None, Some(error)) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = cleanup_confirmed;
                let root = CloseFailure::from_error(error);
                state.close_failure = Some(root.clone());
                state.last_result = Some(Err(root.clone()));
                Err(root.into_error())
            }
            (None, None) => {
                state.phase = SessionPhase::Closed;
                state.cleanup_confirmed = true;
                state.last_result = Some(Ok(()));
                Ok(())
            }
        };
        state.completed_generation = generation;
        self.changed.notify_all();
        drop(state);
        if terminal.is_ok() { self.cleanup.explicit_complete()?; }
        terminal
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, SessionState>> {
        self.state.lock().map_err(|_| poisoned_state_error())
    }
}

impl Drop for SessionOwner {
    fn drop(&mut self) {
        // The finalizer signals an independent owner of retirement work and never waits.
        let deadline = Instant::now().checked_add(self.cleanup_timeout).unwrap_or_else(Instant::now);
        let state = self.state.get_mut().unwrap_or_else(|poison| poison.into_inner());
        state.phase = SessionPhase::Closing;
        signal_operation_cancellation(state);
        self.cleanup.signal_drop(std::mem::take(&mut state.engines), std::mem::take(&mut state.tasks), deadline);
    }
}

fn register_engine_locked(state: &mut SessionState, engine: Engine) {
    if !state.engines.iter().any(|registered| registered.same_owner(&engine)) {
        state.engines.push(engine);
    }
}

#[derive(Debug)]
pub(crate) struct SessionOperation {
    owner: Arc<SessionOwner>,
    cancellation: Arc<OperationCancellation>,
}

/// A non-cloneable runtime pin whose release belongs to one SDK session.
pub struct PinnedSession {
    owner: Arc<SessionOwner>,
    pin: Option<PinHandle>,
    quote_source: Option<PinnedQuoteSource>,
}

pub(crate) struct PinnedQuoteSource {
    pub(crate) engine: Engine,
    pub(crate) config: crate::config::ClientConfig,
    pub(crate) namespace: String,
    // Also serializes activation state with the wire operation. Empty polling
    // keeps the last successful activation's parser identities and precision.
    pub(crate) activation: Mutex<Option<Vec<crate::types::RefreshRequest>>>,
}

impl PinnedSession {
    pub(crate) fn new(owner: Arc<SessionOwner>, pin: PinHandle) -> Result<Self> {
        if let Err(error) = owner.register_pin(pin.clone()) {
            let _ = pin.begin_close();
            return Err(error);
        }
        Ok(Self {
            owner,
            pin: Some(pin),
            quote_source: None,
        })
    }

    pub(crate) fn bind_quotes(&mut self, engine: Engine, config: crate::config::ClientConfig, namespace: String) {
        self.quote_source = Some(PinnedQuoteSource { engine, config, namespace, activation: Mutex::new(Some(Vec::new())) });
    }

    pub(crate) fn quote_source(&self) -> Result<&PinnedQuoteSource> {
        self.quote_source.as_ref().ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "internal pin has no public quote source"))
    }

    pub(crate) fn owner(&self) -> &Arc<SessionOwner> { &self.owner }

    pub fn quotes(&self) -> crate::services::PinnedQuoteService<'_> {
        crate::services::PinnedQuoteService::new(self)
    }

    pub fn identity(&self) -> Option<eltdx_runtime::pin::PinIdentity> {
        self.pin.as_ref().map(PinHandle::identity)
    }

    pub(crate) fn handle(&self) -> Result<&PinHandle> {
        self.pin.as_ref().ok_or_else(|| Error::new(ErrorKind::PinLost, "pinned session is closed"))
    }

    pub(crate) fn mark_released(&mut self) {
        if let Some(pin) = self.pin.take() { self.owner.unregister_pin(&pin); }
    }

    /// Transfer cleanup to an owner-registered task without unregistering the
    /// reservation. The receiving task must already own a cloned handle.
    pub(crate) fn transfer_to_task(mut self) -> Result<PinHandle> {
        self.pin.take().ok_or_else(|| Error::new(ErrorKind::PinLost, "pinned session is closed"))
    }

    pub fn close(&mut self) -> Result<()> {
        let Some(pin) = self.pin.as_ref() else {
            return Ok(());
        };
        self.owner.close_pin(pin, None)?;
        self.owner.unregister_pin(pin);
        self.pin = None;
        Ok(())
    }
}

impl Drop for PinnedSession {
    fn drop(&mut self) {
        if let Some(pin) = self.pin.take() {
            let _ = pin.begin_close();
            // The owner retains its handle until a close confirms retirement.
        }
    }
}

#[derive(Debug, Default)]
struct OperationCancellation {
    cancelled: AtomicBool,
    changed: (Mutex<()>, Condvar),
}

impl OperationCancellation {
    fn cancel(&self) {
        let _guard = self.changed.0.lock().ok();
        self.cancelled.store(true, Ordering::Release);
        self.changed.1.notify_all();
    }
}

impl SessionOperation {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn wait_cancelled(&self, timeout: Duration) -> bool {
        if self.is_cancelled() {
            return true;
        }
        let Ok(guard) = self.cancellation.changed.0.lock() else {
            return self.is_cancelled();
        };
        if self.is_cancelled() {
            return true;
        }
        let _ = self.cancellation.changed.1.wait_timeout(guard, timeout);
        self.is_cancelled()
    }
}

fn signal_operation_cancellation(state: &mut SessionState) {
    state.operations.retain(|operation| {
        if let Some(operation) = operation.upgrade() {
            operation.cancel();
            true
        } else {
            false
        }
    });
}

impl Drop for SessionOperation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.owner.state.lock() {
            state.active_operations = state.active_operations.saturating_sub(1);
            self.owner.changed.notify_all();
        }
    }
}

fn closed_error() -> Error {
    Error::new(ErrorKind::Closed, "client session is closing or closed")
}

fn close_timeout_error() -> Error {
    Error::new(
        ErrorKind::CloseTimeout,
        "client session did not close before its owner deadline",
    )
}

fn poisoned_state_error() -> Error {
    Error::new(ErrorKind::Internal, "client session state is poisoned")
}

fn map_close_error(error: RuntimeError) -> Error {
    let kind = match &error {
        RuntimeError::CloseTimeout { .. } => ErrorKind::CloseTimeout,
        RuntimeError::Timeout { .. } => ErrorKind::Timeout,
        RuntimeError::ConnectionClosed { .. } => ErrorKind::Closed,
        RuntimeError::PoolBusy { .. } => ErrorKind::PoolBusy,
        RuntimeError::InvalidArgument { .. } => ErrorKind::InvalidArgument,
        RuntimeError::Protocol { .. } => ErrorKind::Protocol,
        RuntimeError::UnsupportedCommand { .. } => ErrorKind::Unsupported,
        RuntimeError::PushOverflow { .. } => ErrorKind::PushGap,
        RuntimeError::Internal { .. } => ErrorKind::Internal,
    };
    let mut mapped = Error::new(kind, error.to_string());
    for (key, value) in error.context() {
        mapped = mapped.with_context(key, value);
    }
    mapped.with_source(error)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::SessionOwner;
    use crate::error::ErrorKind;
    use crate::hq::executor::HqExecutor;

    #[test]
    fn task_methods_run_without_the_owner_state_lock() {
        #[derive(Debug)]
        struct CompletedTask { owner: std::sync::Weak<SessionOwner> }
        impl super::OwnerTaskGroup for CompletedTask {
            fn stop(&self) {}
            fn completed(&self) -> bool {
                let owner = self.owner.upgrade().expect("live source-case owner");
                assert!(owner.state.try_lock().is_ok(), "completion inspection must not hold the owner lock");
                true
            }
            fn close_until(&self, _: Instant) -> crate::Result<()> {
                self.owner.upgrade().expect("live source-case owner").ensure_open()
            }
        }
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        let task: Arc<dyn super::OwnerTaskGroup> = Arc::new(CompletedTask { owner: Arc::downgrade(&owner) });
        owner.register_task_group(task.clone()).unwrap();
        owner.reap_completed_tasks();
        assert!(!owner.state.lock().unwrap().tasks.iter().any(|registered| Arc::ptr_eq(registered, &task)));
        owner.close().unwrap();
    }

    #[test]
    fn pin_wait_is_capped_by_the_supplied_deadline() {
        let owner = SessionOwner::new(Duration::from_secs(10), Duration::from_secs(10));
        let started = Instant::now();
        let error = owner.wait_pin_close(started + Duration::from_millis(10), || {
            Ok(eltdx_runtime::engine::PendingPoll::Pending)
        }).expect_err("no retirement confirmation before deadline");
        assert_eq!(error.kind(), ErrorKind::CloseTimeout);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(owner.ensure_open().is_ok());
    }

    #[test]
    fn pin_wait_adopts_a_concurrent_owner_deadline() {
        let owner = Arc::new(SessionOwner::new(Duration::from_millis(30), Duration::from_secs(10)));
        let operation = owner.admit().expect("open owner");
        let (poll_tx, poll_rx) = std::sync::mpsc::sync_channel(1);
        let pin_owner = Arc::clone(&owner);
        let waiter = thread::spawn(move || {
            let mut first = true;
            pin_owner.wait_pin_close(Instant::now() + Duration::from_secs(5), || {
                if first {
                    first = false;
                    poll_tx.send(()).expect("test observes first poll");
                }
                Ok(eltdx_runtime::engine::PendingPoll::Pending)
            })
        });
        poll_rx.recv_timeout(Duration::from_secs(1)).expect("pin wait started before owner close");
        let started = Instant::now();
        let result = owner.close();
        assert!(matches!(result, Err(error) if error.kind() == ErrorKind::CloseTimeout));
        assert!(matches!(waiter.join().expect("waiter exits"), Err(error) if error.kind() == ErrorKind::CloseTimeout));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(operation);
    }

    #[test]
    fn pin_wait_does_not_extend_local_cap_to_cleanup_deadline() {
        let owner = SessionOwner::new(Duration::from_secs(1), Duration::from_secs(10));
        {
            let mut state = owner.state.lock().expect("state available");
            state.phase = super::SessionPhase::Closing;
            state.shutdown_deadline = Some(Instant::now() + Duration::from_secs(10));
        }
        let result = owner.wait_pin_close(Instant::now(), || {
            Ok(eltdx_runtime::engine::PendingPoll::Pending)
        });
        assert!(matches!(result, Err(error) if error.kind() == ErrorKind::CloseTimeout));
    }

    #[test]
    fn close_error_mapping_and_shared_failure_keep_runtime_source() {
        let runtime = eltdx_runtime::RuntimeError::internal("pin retirement failed")
            .with_context("pin_id", "7");
        let failure = super::CloseFailure::from_error(super::map_close_error(runtime.clone()));
        let error = failure.into_error();
        assert_eq!(error.message(), "pin retirement failed");
        assert_eq!(error.context(), runtime.context().as_slice());
        assert_eq!(std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<eltdx_runtime::RuntimeError>()), Some(&runtime));
    }

    fn loopback_pin() -> (Arc<SessionOwner>, super::PinnedSession, thread::JoinHandle<()>) {
        loopback_pin_for_owner(None)
    }

    fn loopback_pin_for_owner(existing_owner: Option<Arc<SessionOwner>>) -> (Arc<SessionOwner>, super::PinnedSession, thread::JoinHandle<()>) {
        use eltdx_protocol::frame::{RequestFrame, RESPONSE_PREFIX};
        use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions};
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
        let address = listener.local_addr().expect("loopback address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("engine connects");
            stream.set_read_timeout(Some(Duration::from_secs(3))).expect("bounded server read");
            let mut raw = vec![0; 12];
            stream.read_exact(&mut raw).expect("handshake header");
            let body = usize::from(u16::from_le_bytes([raw[6], raw[7]])) - 2;
            raw.resize(12 + body, 0);
            stream.read_exact(&mut raw[12..]).expect("handshake body");
            let request = RequestFrame::decode(&raw).expect("valid handshake frame");
            let mut response = Vec::from(RESPONSE_PREFIX);
            response.push(1);
            response.extend_from_slice(&request.msg_id.to_le_bytes());
            response.push(0);
            response.extend_from_slice(&request.msg_type.to_le_bytes());
            response.extend_from_slice(&189_u16.to_le_bytes());
            response.extend_from_slice(&189_u16.to_le_bytes());
            response.extend_from_slice(&[0; 189]);
            stream.write_all(&response).expect("handshake response");
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).expect("owner closes socket"), 0);
        });
        let engine = Engine::new(EngineConfig::new(
            vec![format!("{}:{}", address.ip(), address.port())], 2.0,
            EngineConfigOptions {
                pool_size: 1, runtime_workers: Some(1), server_count: 1,
                max_connections_per_host: None, connect_concurrency: None,
                connect_concurrency_per_host: None, global_raw_bytes: None,
                global_decoded_bytes: None, heartbeat_interval: None,
                max_pending_requests: 8, push_queue_size: 1, push_queue_bytes: 1024,
            },
        ).expect("valid config")).expect("lazy engine");
        let owner = existing_owner.unwrap_or_else(|| Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1))));
        owner.register_engine(engine.clone()).expect("owner open");
        let pin = super::PinnedSession::new(Arc::clone(&owner), engine.pin().expect("pin handshake"))
            .expect("pin registered");
        (owner, pin, server)
    }

    fn unstarted_engine() -> eltdx_runtime::engine::Engine {
        use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions};
        Engine::new(EngineConfig::new(vec!["127.0.0.1:1".to_owned()], 1.0, EngineConfigOptions {
            pool_size: 1, runtime_workers: Some(1), server_count: 1,
            max_connections_per_host: None, connect_concurrency: None, connect_concurrency_per_host: None,
            global_raw_bytes: None, global_decoded_bytes: None, heartbeat_interval: None,
            max_pending_requests: 1, push_queue_size: 1, push_queue_bytes: 1024,
        }).expect("valid unstarted Engine config")).expect("lazy Engine construction")
    }

    #[test]
    fn lazy_engine_registration_starts_no_cleanup_controller() {
        let owner = SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1));
        owner.register_engine_lazy(unstarted_engine()).expect("owner open");
        assert!(owner.cleanup.state.lock().unwrap().worker.is_none());
        owner.close().expect("explicit close handles lazy Engine synchronously");
        assert!(owner.cleanup.state.lock().unwrap().worker.is_none());
    }

    #[test]
    fn engine_registration_racing_close_cannot_leave_a_late_cleanup_worker() {
        for _ in 0..8 {
            let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
            let barrier = Arc::new(Barrier::new(3));
            let register_owner = Arc::clone(&owner);
            let register_barrier = Arc::clone(&barrier);
            let registration = thread::spawn(move || {
                register_barrier.wait();
                register_owner.register_engine(unstarted_engine())
            });
            let close_owner = Arc::clone(&owner);
            let close_barrier = Arc::clone(&barrier);
            let closer = thread::spawn(move || {
                close_barrier.wait();
                close_owner.close()
            });
            barrier.wait();

            assert!(closer.join().expect("closer exits").is_ok());
            match registration.join().expect("registration exits") {
                Ok(()) => {}
                Err(error) => assert_eq!(error.kind(), ErrorKind::Closed),
            }
            assert_eq!(owner.state.lock().unwrap().phase, super::SessionPhase::Closed);
            assert!(owner.cleanup.state.lock().unwrap().worker.is_none());
        }
    }

    #[test]
    fn dropped_pin_remains_owned_until_owner_confirms_release() {
        let (owner, pin, server) = loopback_pin();
        let identity = pin.identity().expect("pin live");
        drop(pin);
        assert!(owner.state.lock().expect("state").pins.iter().any(|pin| pin.identity() == identity));
        owner.close().expect("owner confirms release and engine join");
        assert!(owner.state.lock().expect("state").pins.is_empty());
        server.join().expect("server observed socket shutdown");
    }

    #[test]
    fn engine_join_confirms_timed_out_pin_without_losing_root_failure() {
        let (owner, mut pin, server) = loopback_pin();
        let original = owner.close_with_pin_closer(|runtime_pin, _owner_deadline| {
            // Issue a real runtime close, but deliberately withhold its receipt
            // from this wait. Engine shutdown below must supply actual join proof.
            let _pending = runtime_pin.begin_close().expect("runtime accepts close");
            owner.wait_pin_close(Instant::now(), || {
                Ok(eltdx_runtime::engine::PendingPoll::Pending)
            })
        }).expect_err("first pin confirmation wait expired");
        assert_eq!(original.kind(), ErrorKind::CloseTimeout);
        let original_source = std::error::Error::source(&original)
            .and_then(|source| source.downcast_ref::<eltdx_runtime::RuntimeError>())
            .expect("original timeout source remains structured").clone();
        let generation = {
            let state = owner.state.lock().expect("state");
            assert!(state.pins.is_empty(), "confirmed Engine join retires its remaining pins");
            assert!(state.cleanup_confirmed);
            assert_eq!(state.phase, super::SessionPhase::FailedClosed);
            state.close_generation
        };

        let subsequent = owner.close().expect_err("cleanup confirmation preserves first error");
        assert_eq!(subsequent.kind(), original.kind());
        assert_eq!(subsequent.message(), original.message());
        assert_eq!(subsequent.context(), original.context());
        assert_eq!(std::error::Error::source(&subsequent)
            .and_then(|source| source.downcast_ref::<eltdx_runtime::RuntimeError>()), Some(&original_source));
        assert_eq!(owner.state.lock().expect("state").close_generation, generation,
            "confirmed cleanup does not open another attempt");

        pin.close().expect("owner-confirmed pin close is idempotent even after runtime stopped");
        assert!(pin.identity().is_none());
        pin.close().expect("explicit repeated close is idempotent");
        server.join().expect("actual socket shutdown observed");
    }

    #[test]
    fn explicit_pin_close_after_successful_owner_shutdown_is_idempotent() {
        let (owner, mut pin, server) = loopback_pin();
        owner.close().expect("owner confirms pin and Engine shutdown");
        assert!(owner.state.lock().expect("state").cleanup_confirmed);
        pin.close().expect("no fresh request is sent to a stopped Engine");
        assert!(pin.identity().is_none());
        server.join().expect("actual socket shutdown observed");
    }

    #[test]
    fn stopped_engine_error_alone_does_not_unregister_retained_pin() {
        let (owner, mut pin, server) = loopback_pin();
        let engine = owner.state.lock().expect("state").engines[0].clone();
        engine.close_until(Instant::now() + Duration::from_secs(1)).expect("external Engine shutdown joins");
        let error = pin.close().expect_err("owner has not reconciled its retained registration");
        assert_eq!(error.kind(), ErrorKind::Closed);
        assert!(pin.identity().is_some());
        assert_eq!(owner.state.lock().expect("state").pins.len(), 1);

        let original = owner.close().expect_err("pin begin-close error remains first failure");
        assert_eq!(original.kind(), ErrorKind::Closed);
        {
            let state = owner.state.lock().expect("state");
            assert!(state.pins.is_empty());
            assert!(state.cleanup_confirmed);
        }
        let repeated = owner.close().expect_err("confirmed cleanup retains original failure");
        assert_eq!(repeated.kind(), original.kind());
        assert_eq!(repeated.message(), original.message());
        pin.close().expect("reconciliation now proves this guard is retired");
        server.join().expect("actual socket shutdown observed");
    }

    #[test]
    fn closing_one_of_two_colliding_local_pin_ids_preserves_the_other_registration() {
        let (owner, mut first, first_server) = loopback_pin();
        let (_, mut second, second_server) = loopback_pin_for_owner(Some(Arc::clone(&owner)));
        assert_eq!(first.identity(), second.identity(), "fresh independent engines reuse local counters");
        assert!(!first.pin.as_ref().expect("first pin").same_reservation(second.pin.as_ref().expect("second pin")));
        first.close().expect("first reservation releases");
        assert!(owner.pin_registered(second.pin.as_ref().expect("second guard stays live")).expect("state"));
        assert_eq!(owner.state.lock().expect("state").pins.len(), 1);
        owner.close().expect("both actual runtimes join");
        second.close().expect("second confirmed by its own Engine shutdown");
        first_server.join().expect("first socket closed");
        second_server.join().expect("second socket closed");
    }

    #[test]
    fn reconciling_one_engine_keeps_colliding_pin_from_the_other_engine() {
        let (owner, mut first, first_server) = loopback_pin();
        let (_, mut second, second_server) = loopback_pin_for_owner(Some(Arc::clone(&owner)));
        assert_eq!(first.identity(), second.identity());
        let first_engine = owner.state.lock().expect("state").engines[0].clone();
        first_engine.close_until(Instant::now() + Duration::from_secs(1)).expect("first actual Engine joins");
        owner.reconcile_engine_pins(&first_engine).expect("join evidence reconciled");
        assert!(!owner.pin_registered(first.pin.as_ref().expect("first guard")).expect("state"));
        assert!(owner.pin_registered(second.pin.as_ref().expect("second guard")).expect("state"));
        assert_eq!(owner.state.lock().expect("state").pins.len(), 1);
        first.close().expect("first confirmed retired");
        owner.close().expect("remaining actual Engine joins");
        second.close().expect("second confirmed retired");
        first_server.join().expect("first socket closed");
        second_server.join().expect("second socket closed");
    }

    #[test]
    fn explicit_pin_close_racing_owner_preserves_unconfirmed_guard() {
        let (owner, mut pin, server) = loopback_pin();
        let barrier = Arc::new(Barrier::new(2));
        let close_owner = Arc::clone(&owner);
        let close_barrier = Arc::clone(&barrier);
        let closer = thread::spawn(move || {
            close_barrier.wait();
            close_owner.close()
        });
        barrier.wait();
        match pin.close() {
            Ok(()) => assert!(pin.identity().is_none()),
            Err(error) => {
                assert!(matches!(error.kind(), ErrorKind::Closed | ErrorKind::CloseTimeout));
                assert!(pin.identity().is_some());
            }
        }
        closer.join().expect("owner closer exits").expect("owner closes owned resources");
        assert!(owner.state.lock().expect("state").pins.is_empty());
        server.join().expect("server observed socket shutdown");
    }

    #[test]
    fn admission_gate_closes_before_waiting_for_active_operations() {
        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        let operation = owner.admit().expect("owner starts open");
        let closer_owner = Arc::clone(&owner);
        let closer = thread::spawn(move || closer_owner.close());

        while owner.ensure_open().is_ok() {
            thread::yield_now();
        }
        assert!(operation.wait_cancelled(Duration::from_millis(10)));
        assert!(matches!(owner.admit(), Err(error) if error.kind() == ErrorKind::Closed));

        drop(operation);
        assert!(closer.join().expect("close thread exits").is_ok());
        assert!(matches!(owner.admit(), Err(error) if error.kind() == ErrorKind::Closed));
    }

    #[test]
    fn concurrent_closers_share_the_terminal_attempt() {
        use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions};
        use std::net::{Ipv4Addr, TcpListener};

        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback bind");
        let address = listener.local_addr().expect("loopback address");
        let engine = Engine::new(
            EngineConfig::new(
                vec![format!("{}:{}", address.ip(), address.port())],
                1.0,
                EngineConfigOptions {
                    pool_size: 1,
                    runtime_workers: Some(1),
                    server_count: 1,
                    max_connections_per_host: None,
                    connect_concurrency: None,
                    connect_concurrency_per_host: None,
                    global_raw_bytes: None,
                    global_decoded_bytes: None,
                    heartbeat_interval: None,
                    max_pending_requests: 1,
                    push_queue_size: 1,
                    push_queue_bytes: 1024,
                },
            )
            .expect("valid loopback engine config"),
        )
        .expect("engine constructs lazily");
        engine.ensure_runtime().expect("runtime resource starts");
        owner.register_engine(engine).expect("engine is attached to owner");
        let barrier = Arc::new(Barrier::new(3));
        let mut closers = Vec::new();
        for _ in 0..2 {
            let owner = Arc::clone(&owner);
            let barrier = Arc::clone(&barrier);
            closers.push(thread::spawn(move || {
                barrier.wait();
                owner.close()
            }));
        }
        barrier.wait();
        for closer in closers {
            assert!(closer.join().expect("close thread exits").is_ok());
        }
        assert!(owner.close().is_ok());
        assert!(owner.cleanup.state.lock().unwrap().worker.is_none(), "explicit close joins its cleanup controller");
    }

    #[test]
    fn owned_executor_keeps_owner_alive_after_client_reference_is_dropped() {
        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        let executor = HqExecutor::new(Arc::clone(&owner));
        drop(owner);

        assert!(executor.ensure_open().is_ok());
        assert!(executor.close().is_ok());
        assert!(matches!(executor.begin_operation(), Err(error) if error.kind() == ErrorKind::Closed));
    }
}
