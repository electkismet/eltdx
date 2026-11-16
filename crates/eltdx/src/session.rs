use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use eltdx_runtime::engine::Engine;

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
    engines: Vec<Engine>,
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
                engines: Vec::new(),
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
        Ok(SessionOperation {
            owner: Arc::clone(self),
        })
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
        let (deadline, generation, engines, prior_failure) = {
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
                        state.close_failure = None;
                        state.cleanup_confirmed = false;
                        state.close_generation = state.close_generation.checked_add(1).ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "close generation overflow")
                        })?;
                        break (
                            deadline,
                            state.close_generation,
                            state.engines.clone(),
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
                        state.close_generation = state.close_generation.checked_add(1).ok_or_else(|| {
                            Error::new(ErrorKind::Internal, "close generation overflow")
                        })?;
                        break (
                            deadline,
                            state.close_generation,
                            state.engines.clone(),
                            Some(prior_failure),
                        );
                    }
                }
            }
        };

        let mut failure = None;
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
        assert!(matches!(owner.admit(), Err(error) if error.kind() == ErrorKind::Closed));

        drop(operation);
        assert!(closer.join().expect("close thread exits").is_ok());
        assert!(matches!(owner.admit(), Err(error) if error.kind() == ErrorKind::Closed));
    }

    #[test]
    fn concurrent_closers_share_the_terminal_attempt() {
        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
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
