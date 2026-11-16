use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use eltdx_runtime::engine::Engine;
use eltdx_runtime::engine::PinHandle;

use crate::error::{Error, ErrorKind, Result};

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
}

impl CloseFailure {
    fn from_error(error: Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
            context: error.context().to_vec(),
        }
    }

    fn into_error(&self) -> Error {
        let mut error = Error::new(self.kind, self.message.clone());
        for (key, value) in &self.context {
            error = error.with_context(key.clone(), value.clone());
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
    pins: Vec<PinHandle>,
}

/// Shared owner for clients, services, operations, pins, and subscriptions.
#[derive(Debug)]
pub(crate) struct SessionOwner {
    close_timeout: Duration,
    cleanup_timeout: Duration,
    state: Mutex<SessionState>,
    changed: Condvar,
}

impl SessionOwner {
    pub(crate) fn new(close_timeout: Duration, cleanup_timeout: Duration) -> Self {
        Self {
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
                pins: Vec::new(),
            }),
            changed: Condvar::new(),
        }
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        let state = self.lock_state()?;
        if state.phase == SessionPhase::Open {
            Ok(())
        } else {
            Err(closed_error())
        }
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
        state.pins.push(pin);
        Ok(())
    }

    fn unregister_pin(&self, identity: eltdx_runtime::pin::PinIdentity) {
        if let Ok(mut state) = self.state.lock() {
            state.pins.retain(|pin| pin.identity() != identity);
        }
    }

    pub(crate) fn register_engine(&self, engine: Engine) -> Result<()> {
        let mut state = self.lock_state()?;
        if state.phase != SessionPhase::Open {
            return Err(closed_error());
        }
        state.engines.push(engine);
        Ok(())
    }

    pub(crate) fn close(&self) -> Result<()> {
        let (deadline, generation, engines, pins, prior_failure) = {
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
                        state.close_generation = state.close_generation.checked_add(1).ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "close generation overflow")
                        })?;
                        break (
                            deadline,
                            state.close_generation,
                            state.engines.clone(),
                            state.pins.clone(),
                            Some(prior_failure),
                        );
                    }
                }
            }
        };

        let mut failure = None;
        for pin in pins {
            if let Err(error) = pin.close() {
                failure.get_or_insert_with(|| Error::new(ErrorKind::Internal, error.to_string()));
            }
        }
        for engine in engines {
            if let Err(error) = engine.close_until(deadline) {
                failure.get_or_insert_with(|| {
                    Error::new(
                        if matches!(&error, eltdx_runtime::RuntimeError::CloseTimeout { .. }) {
                            ErrorKind::CloseTimeout
                        } else {
                            ErrorKind::Internal
                        },
                        error.to_string(),
                    )
                });
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

        let terminal = match (prior_failure, failure) {
            (Some(root), Some(_)) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = false;
                state.last_result = Some(Err(root.clone()));
                Err(root.into_error())
            }
            (Some(root), None) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = true;
                state.last_result = Some(Err(root.clone()));
                Err(root.into_error())
            }
            (None, Some(error)) => {
                state.phase = SessionPhase::FailedClosed;
                state.cleanup_confirmed = false;
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
        terminal
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, SessionState>> {
        self.state.lock().map_err(|_| poisoned_state_error())
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
}

impl PinnedSession {
    pub(crate) fn new(owner: Arc<SessionOwner>, pin: PinHandle) -> Result<Self> {
        owner.register_pin(pin.clone())?;
        Ok(Self {
            owner,
            pin: Some(pin),
        })
    }

    pub fn identity(&self) -> Option<eltdx_runtime::pin::PinIdentity> {
        self.pin.as_ref().map(PinHandle::identity)
    }

    pub fn close(&mut self) -> Result<()> {
        let Some(pin) = self.pin.as_ref() else {
            return Ok(());
        };
        pin.close().map_err(|error| {
            Error::new(ErrorKind::Internal, error.to_string())
        })?;
        self.owner.unregister_pin(pin.identity());
        self.pin = None;
        Ok(())
    }
}

impl Drop for PinnedSession {
    fn drop(&mut self) {
        if let Some(pin) = self.pin.take() {
            let identity = pin.identity();
            let _ = pin.begin_close();
            self.owner.unregister_pin(identity);
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    use super::SessionOwner;
    use crate::error::ErrorKind;
    use crate::hq::executor::HqExecutor;

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
