//! One owner-controlled socket and one complete request/reply lease. The stored
//! socket is nonblocking std I/O between calls, so no reactor outlives its job.
use super::frame::RangeSequence;
use super::hosts::{DiagnosticSnapshot, HostProbe};
use crate::operation::OperationContext;
use crate::session::OwnerTaskGroup;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct Connection {
    pub stream: TcpStream,
    pub endpoint: SocketAddr,
    pub generation: u64,
}
#[derive(Debug)]
struct State {
    connection: Option<Connection>,
    registered: bool,
    candidates: Vec<SocketAddr>,
    excluded: Vec<SocketAddr>,
    probes: Vec<HostProbe>,
    latest_meta: Option<crate::types::ResultMeta>,
}
#[derive(Debug)]
pub(crate) struct Session {
    state: Mutex<State>,
    pub stopped: Arc<AtomicBool>,
    pub sequences: RangeSequence,
    pub profile: Arc<dyn super::profile::SetupProfile>,
    next_generation: AtomicU64,
    gate: Arc<Gate>,
}
impl Session {
    pub fn new(candidates: Vec<SocketAddr>) -> Arc<Self> {
        Self::with_profile(candidates, Arc::new(super::profile::ExProfile))
    }
    pub fn with_profile(
        candidates: Vec<super::hosts::Endpoint>,
        profile: Arc<dyn super::profile::SetupProfile>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                connection: None,
                registered: false,
                candidates,
                excluded: Vec::new(),
                probes: Vec::new(),
                latest_meta: None,
            }),
            stopped: Arc::new(AtomicBool::new(false)),
            sequences: RangeSequence::default(),
            profile,
            next_generation: AtomicU64::new(1),
            gate: Arc::new(Gate::default()),
        })
    }
    pub fn lease(self: &Arc<Self>, context: &OperationContext) -> Result<Lease> {
        let mut busy = self.gate.busy.lock().map_err(|_| poisoned())?;
        loop {
            drop(busy);
            context.owner().reap_completed_tasks();
            context.check_boundary(TimeoutPhase::Queue)?;
            if self.stopped.load(Ordering::Acquire) {
                return Err(closed());
            }
            busy = self.gate.busy.lock().map_err(|_| poisoned())?;
            if !*busy {
                *busy = true;
                break;
            }
            busy = self
                .gate
                .changed
                .wait_timeout(busy, Duration::from_millis(10))
                .map_err(|_| poisoned())?
                .0;
        }
        drop(busy);
        let lease = Lease {
            session: Arc::clone(self),
            _slot: Arc::new(Slot(Arc::clone(&self.gate))),
            accepted: false,
        };
        // Gate serialization makes registration once-only, without calling an
        // owner method while holding a lock an OwnerTaskGroup callback needs.
        let registered = self.state.lock().map_err(|_| poisoned())?.registered;
        if !registered {
            let task: Arc<dyn OwnerTaskGroup> = self.clone();
            context.owner().register_task_group(task)?;
            self.state.lock().map_err(|_| poisoned())?.registered = true;
        }
        context.check_boundary(TimeoutPhase::Queue)?;
        Ok(lease)
    }
    pub fn snapshot(&self) -> Result<DiagnosticSnapshot> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        Ok(DiagnosticSnapshot::new(
            state.connection.as_ref().map(|item| item.endpoint),
            state.candidates.clone(),
            state.probes.clone(),
        )
        .with_latest_meta(state.latest_meta.clone()))
    }
    pub fn record_meta(&self, meta: crate::types::ResultMeta) -> Result<()> {
        self.state.lock().map_err(|_| poisoned())?.latest_meta = Some(meta);
        Ok(())
    }
    /// Read only while holding the complete-exchange lease when establishing
    /// a source-sensitive multi-request composition.
    pub fn connection_identity(&self) -> Result<Option<(super::hosts::Endpoint, u64)>> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        Ok(state
            .connection
            .as_ref()
            .map(|connection| (connection.endpoint, connection.generation)))
    }
    pub fn take(&self) -> Result<Option<Connection>> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if self.stopped.load(Ordering::Acquire) {
            return Err(closed());
        }
        Ok(state.connection.take())
    }
    pub fn publish(&self, connection: Connection, operation_stopped: &AtomicBool) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if self.stopped.load(Ordering::Acquire) {
            drop(connection);
            return Err(closed());
        }
        if operation_stopped.load(Ordering::Acquire) {
            drop(connection);
            return Err(Error::new(
                ErrorKind::Cancelled,
                "extension exchange stopped before publication",
            ));
        }
        state
            .excluded
            .retain(|endpoint| *endpoint != connection.endpoint);
        state.connection = Some(connection);
        Ok(())
    }
    pub fn retire(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.connection.take();
        }
    }
    pub fn next_generation(&self) -> Result<u64> {
        self.next_generation
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| {
                Error::new(
                    ErrorKind::LimitExceeded,
                    "extension socket generation exhausted",
                )
            })
    }
    pub fn candidates(&self) -> Result<Vec<SocketAddr>> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        let candidates: Vec<_> = state
            .candidates
            .iter()
            .filter(|endpoint| !state.excluded.contains(endpoint))
            .copied()
            .collect();
        // A new operation may recover after all candidates failed previously.
        // A single operation still has its own finite attempt maximum.
        Ok(if candidates.is_empty() {
            state.candidates.clone()
        } else {
            candidates
        })
    }
    pub fn failed(&self, endpoint: SocketAddr) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.excluded.contains(&endpoint) {
            state.excluded.push(endpoint);
        }
        Ok(())
    }
    pub fn unhealthy(&self) -> Result<Vec<SocketAddr>> {
        Ok(self.state.lock().map_err(|_| poisoned())?.excluded.clone())
    }
    pub fn update_probes(
        &self,
        probes: Vec<HostProbe>,
        candidates: Option<Vec<super::hosts::Endpoint>>,
        rank: bool,
    ) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if self.stopped.load(Ordering::Acquire) {
            return Err(closed());
        }
        if let Some(candidates) = candidates {
            if state
                .connection
                .as_ref()
                .is_some_and(|connection| !candidates.contains(&connection.endpoint))
            {
                state.connection.take();
            }
            state
                .excluded
                .retain(|endpoint| candidates.contains(endpoint));
            state.candidates = candidates;
        }
        for probe in &probes {
            if probe.mode() != super::hosts::ProbeMode::DataReady {
                continue;
            }
            if probe.data_ready() {
                state
                    .excluded
                    .retain(|endpoint| *endpoint != probe.endpoint());
            } else if state.candidates.contains(&probe.endpoint())
                && !state.excluded.contains(&probe.endpoint())
            {
                state.excluded.push(probe.endpoint());
            }
        }
        if rank {
            state.candidates = super::hosts::rank_endpoints(&state.candidates, &probes);
        }
        state.probes = probes;
        Ok(())
    }
}
impl OwnerTaskGroup for Session {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        // Drop must only signal. Contended state is retired by close_until or
        // the registered in-flight job; it never blocks the owner's Drop.
        if let Ok(mut state) = self.state.try_lock() {
            state.connection.take();
        }
        self.gate.changed.notify_all();
    }
    fn close_until(&self, deadline: Instant) -> Result<()> {
        self.stop();
        loop {
            match self.state.try_lock() {
                Ok(mut state) => {
                    state.connection.take();
                    return Ok(());
                }
                Err(TryLockError::Poisoned(_)) => return Err(poisoned()),
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(Error::new(
                            ErrorKind::CloseTimeout,
                            "extension idle socket retirement unconfirmed",
                        ));
                    }
                    std::thread::sleep(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(1)),
                    );
                }
            }
        }
    }
}
#[derive(Debug, Default)]
struct Gate {
    busy: Mutex<bool>,
    changed: Condvar,
}
#[derive(Debug)]
struct Slot(Arc<Gate>);
impl Drop for Slot {
    fn drop(&mut self) {
        *self
            .0
            .busy
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = false;
        self.0.changed.notify_one();
    }
}
#[derive(Debug)]
pub(crate) struct Lease {
    session: Arc<Session>,
    _slot: Arc<Slot>,
    accepted: bool,
}
impl Lease {
    pub fn worker_slot(&self) -> WorkerSlot {
        WorkerSlot {
            _slot: Arc::clone(&self._slot),
        }
    }
    pub fn accept(mut self) {
        self.accepted = true;
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.accepted {
            self.session.retire();
        }
    }
}
#[derive(Debug)]
pub(crate) struct WorkerSlot {
    _slot: Arc<Slot>,
}
pub(crate) fn poisoned() -> Error {
    Error::new(ErrorKind::Internal, "extension session state unavailable")
}
fn closed() -> Error {
    Error::new(ErrorKind::Closed, "extension session is closed")
}
