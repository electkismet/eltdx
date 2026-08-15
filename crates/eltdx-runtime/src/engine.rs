use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eltdx_protocol::commands::session::{
    parse_handshake_payload, parse_heartbeat_payload, HandshakeInfo, HandshakeRequest,
    HeartbeatAck, HeartbeatRequest, TYPE_HANDSHAKE, TYPE_HEARTBEAT,
};
use eltdx_protocol::frame::ResponseFrame;
use eltdx_protocol::limits::SLOT_FRAME_BUDGET;
use eltdx_protocol::{CommandRequest, CommandResponse};
use tokio::net::TcpStream;
use tokio::runtime::Builder;
use tokio::sync::{mpsc, oneshot, watch, Notify};
use tokio::time::{interval, timeout_at, MissedTickBehavior};

use crate::deadline::Deadline;
use crate::diagnostics::{
    PoolDiagnostics, PoolState, RuntimeState, SlotSnapshot, TransportDiagnostics,
};
use crate::endpoint::{Endpoint, EndpointRotation};
use crate::error::{RuntimeError, TimeoutPhase};
use crate::pin::PinIdentity;
use crate::push::PushFrame;
use crate::request::{
    ActiveLease, Admission, Promotion, RequestAttempt, RequestState, RequestWireIdentity,
    RetryDecision, RetryPolicy, TerminalBatch, TerminalKind, TerminalNotification,
};
use crate::slot::{
    EngineEpoch, FrameDisposition, GenerationIdentity, HeartbeatCandidate, MessageIdentity,
    ReconnectAck, RequestId, RoutedResponse, Slot, SlotId, SlotState,
};
use crate::supervisor::{CloseClaim, EngineState, PinTerminalBatch, StartClaim, Supervisor};

pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
pub const CANCEL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(1);
pub const SIGNAL_POLL_INTERVAL: Duration = Duration::from_millis(25);

const RUNTIME_TICK: Duration = Duration::from_millis(10);

#[derive(Clone, Debug)]
pub struct EngineConfig {
    endpoints: Arc<[Endpoint]>,
    request_timeout: Duration,
    pool_size: usize,
    heartbeat_interval: Option<Duration>,
    max_pending_requests: usize,
    push_queue_size: usize,
    push_queue_bytes: usize,
}

impl EngineConfig {
    pub fn new(
        hosts: Vec<String>,
        timeout: f64,
        pool_size: usize,
        heartbeat_interval: Option<f64>,
        max_pending_requests: usize,
        push_queue_size: usize,
        push_queue_bytes: usize,
    ) -> Result<Self, RuntimeError> {
        let mut endpoints = Vec::with_capacity(hosts.len());
        let mut seen = BTreeSet::new();
        for host in hosts {
            let endpoint = Endpoint::numeric(&host)?;
            if seen.insert((endpoint.host().to_owned(), endpoint.address())) {
                endpoints.push(endpoint);
            }
        }
        Self::from_endpoints(
            endpoints,
            duration_from_seconds("timeout", timeout, false)?,
            pool_size,
            heartbeat_interval
                .map(|seconds| duration_from_seconds("heartbeat_interval", seconds, true))
                .transpose()?,
            max_pending_requests,
            push_queue_size,
            push_queue_bytes,
        )
    }

    pub fn from_endpoints(
        endpoints: Vec<Endpoint>,
        request_timeout: Duration,
        pool_size: usize,
        heartbeat_interval: Option<Duration>,
        max_pending_requests: usize,
        push_queue_size: usize,
        push_queue_bytes: usize,
    ) -> Result<Self, RuntimeError> {
        if endpoints.is_empty() {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "at least one resolved endpoint is required",
            ));
        }
        if request_timeout.is_zero() {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "timeout must be > 0",
            ));
        }
        if pool_size == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "pool_size must be a positive integer",
            ));
        }
        if max_pending_requests == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "max_pending_requests must be > 0",
            ));
        }
        if push_queue_size == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_size must be > 0",
            ));
        }
        if push_queue_bytes == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_bytes must be > 0",
            ));
        }
        if heartbeat_interval.is_some_and(|value| value.is_zero()) {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "heartbeat_interval must be > 0 or None",
            ));
        }
        pool_size.checked_add(max_pending_requests).ok_or_else(|| {
            RuntimeError::invalid_argument("OverflowError", "request capacity overflow")
        })?;
        Supervisor::with_limits(
            pool_size,
            max_pending_requests,
            push_queue_size,
            push_queue_bytes,
        )?;
        Ok(Self {
            endpoints: endpoints.into(),
            request_timeout,
            pool_size,
            heartbeat_interval,
            max_pending_requests,
            push_queue_size,
            push_queue_bytes,
        })
    }

    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub const fn pool_size(&self) -> usize {
        self.pool_size
    }

    pub const fn heartbeat_interval(&self) -> Option<Duration> {
        self.heartbeat_interval
    }

    pub const fn max_pending_requests(&self) -> usize {
        self.max_pending_requests
    }

    pub const fn push_queue_size(&self) -> usize {
        self.push_queue_size
    }

    pub const fn push_queue_bytes(&self) -> usize {
        self.push_queue_bytes
    }

    fn total_capacity(&self) -> Result<usize, RuntimeError> {
        self.pool_size
            .checked_add(self.max_pending_requests)
            .ok_or_else(|| RuntimeError::internal("request capacity overflow"))
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

#[derive(Clone, Debug)]
pub struct PinHandle {
    engine: Engine,
    identity: PinIdentity,
}

#[derive(Debug)]
struct EngineInner {
    created_pid: u32,
    config: EngineConfig,
    host: Mutex<HostState>,
    ingress_owned: Arc<AtomicUsize>,
    diagnostics: Arc<Mutex<DiagnosticsCache>>,
    sessions: Arc<Mutex<SessionCache>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SessionCache {
    handshakes: BTreeMap<SlotId, HandshakeInfo>,
    heartbeats: BTreeMap<SlotId, HeartbeatAck>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiagnosticsCache {
    pool: PoolDiagnostics,
    transport: Option<TransportDiagnostics>,
}

impl DiagnosticsCache {
    fn stopped(pool_size: usize) -> Self {
        Self {
            pool: PoolDiagnostics {
                epoch: 0,
                state: PoolState::Stopped,
                broker: None,
                actors: Vec::new(),
                push_frames: 0,
                push_bytes: 0,
                push_dropped: 0,
            },
            transport: (pool_size == 1).then_some(TransportDiagnostics {
                epoch: 0,
                actor: None,
                push_frames: 0,
                push_bytes: 0,
                push_dropped: 0,
                push_max_frames: 0,
                push_max_bytes: 0,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostLifecycle {
    Stopped,
    Starting,
    Running,
    Closing,
    FailedClosing,
    FailedClosed,
}

#[derive(Debug)]
struct HostState {
    lifecycle: HostLifecycle,
    request_counter: u64,
    connect_counter: u64,
    connect_attempt: Option<Arc<HostConnectAttempt>>,
    close_counter: u64,
    close_attempt: Option<Arc<HostCloseAttempt>>,
    fully_connected: bool,
    runtime: Option<RuntimeThread>,
    failed_close_lineage: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConnectAttemptId(u64);

impl ConnectAttemptId {
    fn next(previous: u64) -> Result<Self, RuntimeError> {
        previous
            .checked_add(1)
            .filter(|value| *value != 0)
            .map(Self)
            .ok_or_else(|| RuntimeError::internal("connect attempt identity space exhausted"))
    }
}

#[derive(Debug)]
struct HostConnectAttempt {
    id: ConnectAttemptId,
    completion: Arc<Completion<Result<(), RuntimeError>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HostCloseAttemptId(u64);

impl HostCloseAttemptId {
    fn next(previous: u64) -> Result<Self, RuntimeError> {
        previous
            .checked_add(1)
            .filter(|value| *value != 0)
            .map(Self)
            .ok_or_else(|| RuntimeError::internal("close attempt identity space exhausted"))
    }
}

#[derive(Debug)]
struct HostCloseAttempt {
    id: HostCloseAttemptId,
    deadline: Instant,
    completion: Arc<Completion<Result<(), RuntimeError>>>,
}

#[derive(Debug)]
struct RuntimeThread {
    command_tx: mpsc::Sender<RuntimeCommand>,
    control: Arc<ControlCell>,
    startup: Arc<Completion<Result<(), RuntimeError>>>,
    exited: Arc<Completion<Result<(), RuntimeError>>>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct Completion<T> {
    value: Mutex<Option<T>>,
    changed: Condvar,
}

impl<T> Completion<T> {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
            changed: Condvar::new(),
        }
    }

    fn publish(&self, value: T) {
        if let Ok(mut current) = self.value.lock() {
            if current.is_none() {
                *current = Some(value);
                self.changed.notify_all();
            }
        }
    }
}

impl<T: Clone> Completion<T> {
    fn wait(&self, timeout: Duration) -> Result<Option<T>, RuntimeError> {
        let current = lock_mutex(&self.value, "completion")?;
        if current.is_some() {
            return Ok(current.clone());
        }
        let (current, _) = self
            .changed
            .wait_timeout(current, timeout)
            .map_err(|_| RuntimeError::internal("completion condition is poisoned"))?;
        Ok(current.clone())
    }
}

#[derive(Debug, Default)]
struct ControlState {
    close_requested: bool,
    close_timed_out: bool,
    cancellations: BTreeMap<RequestId, Arc<Completion<Result<(), RuntimeError>>>>,
}

#[derive(Debug)]
struct ControlCell {
    state: Mutex<ControlState>,
    changed: Notify,
}

impl ControlCell {
    fn new() -> Self {
        Self {
            state: Mutex::new(ControlState::default()),
            changed: Notify::new(),
        }
    }

    fn request_close(&self) -> Result<(), RuntimeError> {
        let mut state = lock_mutex(&self.state, "runtime control")?;
        state.close_requested = true;
        drop(state);
        self.changed.notify_one();
        Ok(())
    }

    fn mark_close_timeout(&self) -> Result<(), RuntimeError> {
        let mut state = lock_mutex(&self.state, "runtime control")?;
        state.close_requested = true;
        state.close_timed_out = true;
        drop(state);
        self.changed.notify_one();
        Ok(())
    }

    fn request_cancel(
        &self,
        request_id: RequestId,
        confirmation: Arc<Completion<Result<(), RuntimeError>>>,
    ) -> Result<(), RuntimeError> {
        let mut state = lock_mutex(&self.state, "runtime control")?;
        state
            .cancellations
            .entry(request_id)
            .or_insert(confirmation);
        drop(state);
        self.changed.notify_one();
        Ok(())
    }

    fn take_snapshot(&self) -> Result<ControlSnapshot, RuntimeError> {
        let mut state = lock_mutex(&self.state, "runtime control")?;
        Ok(ControlSnapshot {
            close_requested: state.close_requested,
            close_timed_out: std::mem::take(&mut state.close_timed_out),
            cancellations: std::mem::take(&mut state.cancellations),
        })
    }
}

#[derive(Debug)]
struct ControlSnapshot {
    close_requested: bool,
    close_timed_out: bool,
    cancellations: BTreeMap<RequestId, Arc<Completion<Result<(), RuntimeError>>>>,
}

#[derive(Debug)]
struct IngressOwnership {
    owned: Arc<AtomicUsize>,
}

impl Drop for IngressOwnership {
    fn drop(&mut self) {
        let previous = self.owned.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

#[derive(Debug)]
struct RawExecution {
    request: CommandRequest,
    response: ResponseFrame,
}

#[derive(Debug)]
pub struct PendingExecution {
    created_pid: u32,
    request_id: RequestId,
    control: Arc<ControlCell>,
    result_rx: std_mpsc::Receiver<Result<RawExecution, RuntimeError>>,
    terminal: Option<Result<CommandResponse, RuntimeError>>,
}

#[derive(Debug)]
pub struct PendingPin {
    created_pid: u32,
    engine: Engine,
    request_id: RequestId,
    control: Arc<ControlCell>,
    result_rx: std_mpsc::Receiver<Result<PinIdentity, RuntimeError>>,
}

#[derive(Debug)]
pub struct PendingPinClose {
    created_pid: u32,
    result_rx: std_mpsc::Receiver<Result<(), RuntimeError>>,
    terminal: Option<Result<(), RuntimeError>>,
}

#[derive(Debug)]
pub struct PendingConnect {
    created_pid: u32,
    engine: Engine,
    attempt: Arc<HostConnectAttempt>,
    terminal: Option<Result<(), RuntimeError>>,
}

#[derive(Debug)]
pub struct PendingClose {
    created_pid: u32,
    engine: Engine,
    attempt: Option<Arc<HostCloseAttempt>>,
    runtime: Option<RuntimeRef>,
    terminal: Option<Result<(), RuntimeError>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PendingPoll<T> {
    Ready(T),
    Pending,
}

impl PendingExecution {
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<PendingPoll<CommandResponse>, RuntimeError> {
        check_pid(self.created_pid)?;
        if let Some(result) = self.terminal.take() {
            return result.map(PendingPoll::Ready);
        }
        match self.result_rx.recv_timeout(timeout) {
            Ok(Ok(raw)) => {
                let parsed = CommandResponse::parse(raw.request, &raw.response.data)
                    .map_err(RuntimeError::from)?;
                Ok(PendingPoll::Ready(parsed))
            }
            Ok(Err(error)) => Err(error),
            Err(std_mpsc::RecvTimeoutError::Timeout) => Ok(PendingPoll::Pending),
            Err(std_mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::internal(
                "runtime result channel disconnected before terminal publication",
            )),
        }
    }

    pub fn wait(mut self) -> Result<CommandResponse, RuntimeError> {
        loop {
            match self.wait_timeout(SIGNAL_POLL_INTERVAL)? {
                PendingPoll::Ready(value) => return Ok(value),
                PendingPoll::Pending => {}
            }
        }
    }

    pub fn cancel_and_confirm(&self, timeout: Duration) -> Result<(), RuntimeError> {
        check_pid(self.created_pid)?;
        let confirmation = Arc::new(Completion::new());
        self.control
            .request_cancel(self.request_id, Arc::clone(&confirmation))?;
        match confirmation.wait(timeout)? {
            Some(result) => result,
            None => {
                let error = RuntimeError::timeout(TimeoutPhase::CancelConfirmation)
                    .with_context("request_id", self.request_id.get().to_string());
                self.control.mark_close_timeout()?;
                self.control.request_close()?;
                Err(error)
            }
        }
    }
}

impl PendingPin {
    pub fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<PendingPoll<PinHandle>, RuntimeError> {
        check_pid(self.created_pid)?;
        match self.result_rx.recv_timeout(timeout) {
            Ok(Ok(identity)) => Ok(PendingPoll::Ready(PinHandle {
                engine: self.engine.clone(),
                identity,
            })),
            Ok(Err(error)) => Err(error),
            Err(std_mpsc::RecvTimeoutError::Timeout) => Ok(PendingPoll::Pending),
            Err(std_mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::internal(
                "runtime pin channel disconnected before terminal publication",
            )),
        }
    }

    pub fn wait(mut self) -> Result<PinHandle, RuntimeError> {
        loop {
            match self.wait_timeout(SIGNAL_POLL_INTERVAL)? {
                PendingPoll::Ready(value) => return Ok(value),
                PendingPoll::Pending => {}
            }
        }
    }

    pub fn cancel_and_confirm(&self, timeout: Duration) -> Result<(), RuntimeError> {
        check_pid(self.created_pid)?;
        let confirmation = Arc::new(Completion::new());
        self.control
            .request_cancel(self.request_id, Arc::clone(&confirmation))?;
        match confirmation.wait(timeout)? {
            Some(result) => result,
            None => {
                let error = RuntimeError::timeout(TimeoutPhase::CancelConfirmation)
                    .with_context("request_id", self.request_id.get().to_string());
                self.control.mark_close_timeout()?;
                self.control.request_close()?;
                Err(error)
            }
        }
    }
}

impl PendingPinClose {
    pub fn wait_timeout(&mut self, timeout: Duration) -> Result<PendingPoll<()>, RuntimeError> {
        check_pid(self.created_pid)?;
        if let Some(result) = self.terminal.as_ref().cloned() {
            return result.map(|()| PendingPoll::Ready(()));
        }
        match self.result_rx.recv_timeout(timeout) {
            Ok(result) => {
                self.terminal = Some(result.clone());
                result.map(|()| PendingPoll::Ready(()))
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => Ok(PendingPoll::Pending),
            Err(std_mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeError::internal(
                "runtime pin-close channel disconnected before terminal publication",
            )),
        }
    }

    pub fn wait(mut self) -> Result<(), RuntimeError> {
        loop {
            match self.wait_timeout(SIGNAL_POLL_INTERVAL)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }
}

impl PinHandle {
    pub const fn identity(&self) -> PinIdentity {
        self.identity
    }

    pub fn begin_execute(&self, request: CommandRequest) -> Result<PendingExecution, RuntimeError> {
        self.engine.begin_pinned_execute(self.identity, request)
    }

    pub fn execute(&self, request: CommandRequest) -> Result<CommandResponse, RuntimeError> {
        self.begin_execute(request)?.wait()
    }

    pub fn connected_host(&self) -> Result<Option<String>, RuntimeError> {
        let diagnostics = self.engine.pool_diagnostics()?;
        Ok(diagnostics
            .actors
            .into_iter()
            .find(|actor| actor.slot_id() == self.identity.slot_id)
            .and_then(|actor| actor.connected_host))
    }

    pub fn session_snapshot(
        &self,
    ) -> Result<(Option<HandshakeInfo>, Option<HeartbeatAck>), RuntimeError> {
        self.engine.session_snapshot(self.identity.slot_id.get())
    }

    pub fn begin_close(&self) -> Result<PendingPinClose, RuntimeError> {
        self.engine.begin_pin_close(self.identity)
    }

    pub fn close(&self) -> Result<(), RuntimeError> {
        self.begin_close()?.wait()
    }
}

impl PendingConnect {
    pub fn wait_timeout(&mut self, timeout: Duration) -> Result<PendingPoll<()>, RuntimeError> {
        check_pid(self.created_pid)?;
        if self.terminal.is_none() {
            match self.attempt.completion.wait(timeout)? {
                Some(result) => self.terminal = Some(result),
                None => {
                    let Some(error) = self.engine.connect_runtime_exit(self.attempt.id)? else {
                        return Ok(PendingPoll::Pending);
                    };
                    self.attempt.completion.publish(Err(error.clone()));
                    self.terminal = Some(Err(error));
                }
            }
        }
        let result = self
            .terminal
            .as_ref()
            .cloned()
            .ok_or_else(|| RuntimeError::internal("connect terminal result disappeared"))?;
        if !self
            .engine
            .finish_connect_attempt(self.attempt.id, &result)?
        {
            return Ok(PendingPoll::Pending);
        }
        result.map(|()| PendingPoll::Ready(()))
    }

    pub fn wait(mut self) -> Result<(), RuntimeError> {
        loop {
            match self.wait_timeout(SIGNAL_POLL_INTERVAL)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }

    pub fn cancel_and_confirm(&self, timeout: Duration) -> Result<(), RuntimeError> {
        check_pid(self.created_pid)?;
        self.engine.begin_close_with_timeout(timeout)?.wait()
    }
}

impl PendingClose {
    pub fn wait_timeout(&mut self, timeout: Duration) -> Result<PendingPoll<()>, RuntimeError> {
        check_pid(self.created_pid)?;
        if let Some(result) = self.terminal.as_ref().cloned() {
            return result.map(|()| PendingPoll::Ready(()));
        }
        let (Some(attempt), Some(runtime)) = (self.attempt.as_ref(), self.runtime.as_ref()) else {
            self.terminal = Some(Ok(()));
            return Ok(PendingPoll::Ready(()));
        };
        if let Some(result) = attempt.completion.wait(Duration::ZERO)? {
            self.terminal = Some(result.clone());
            return result.map(|()| PendingPoll::Ready(()));
        }
        let now = Instant::now();
        let remaining = attempt.deadline.saturating_duration_since(now);
        let wait = timeout.min(remaining);
        if let Some(result) = runtime.exited.wait(wait)? {
            let completed = self.engine.finish_host_close(attempt, runtime, result);
            self.terminal = Some(completed.clone());
            return completed.map(|()| PendingPoll::Ready(()));
        }
        if Instant::now() >= attempt.deadline {
            if let Some(result) = runtime.exited.wait(Duration::ZERO)? {
                let completed = self.engine.finish_host_close(attempt, runtime, result);
                self.terminal = Some(completed.clone());
                return completed.map(|()| PendingPoll::Ready(()));
            }
            self.engine.mark_host_close_timeout(attempt, runtime)?;
            let completed = attempt.completion.wait(Duration::ZERO)?.ok_or_else(|| {
                RuntimeError::internal("close timeout did not publish its attempt result")
            })?;
            self.terminal = Some(completed.clone());
            return completed.map(|()| PendingPoll::Ready(()));
        }
        Ok(PendingPoll::Pending)
    }

    pub fn wait(mut self) -> Result<(), RuntimeError> {
        loop {
            match self.wait_timeout(SIGNAL_POLL_INTERVAL)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self, RuntimeError> {
        config.total_capacity()?;
        let diagnostics = DiagnosticsCache::stopped(config.pool_size);
        Ok(Self {
            inner: Arc::new(EngineInner {
                created_pid: std::process::id(),
                config,
                host: Mutex::new(HostState {
                    lifecycle: HostLifecycle::Stopped,
                    request_counter: 0,
                    connect_counter: 0,
                    connect_attempt: None,
                    close_counter: 0,
                    close_attempt: None,
                    fully_connected: false,
                    runtime: None,
                    failed_close_lineage: false,
                }),
                ingress_owned: Arc::new(AtomicUsize::new(0)),
                diagnostics: Arc::new(Mutex::new(diagnostics)),
                sessions: Arc::new(Mutex::new(SessionCache::default())),
            }),
        })
    }

    pub fn connect(&self) -> Result<(), RuntimeError> {
        self.begin_connect()?.wait()
    }

    pub fn begin_connect(&self) -> Result<PendingConnect, RuntimeError> {
        self.check_pid()?;
        let (attempt, runtime) = {
            let mut host = lock_mutex(&self.inner.host, "Engine connect gate")?;
            if let Some(attempt) = host.connect_attempt.as_ref() {
                return Ok(PendingConnect {
                    created_pid: self.inner.created_pid,
                    engine: self.clone(),
                    attempt: Arc::clone(attempt),
                    terminal: None,
                });
            }
            match host.lifecycle {
                HostLifecycle::Closing
                | HostLifecycle::FailedClosing
                | HostLifecycle::FailedClosed => {
                    return Err(RuntimeError::connection_closed(
                        "7709 Engine cannot connect after close linearization",
                    ));
                }
                HostLifecycle::Running if host.fully_connected => {
                    let id = ConnectAttemptId(host.connect_counter);
                    let completion = Arc::new(Completion::new());
                    completion.publish(Ok(()));
                    return Ok(PendingConnect {
                        created_pid: self.inner.created_pid,
                        engine: self.clone(),
                        attempt: Arc::new(HostConnectAttempt { id, completion }),
                        terminal: None,
                    });
                }
                HostLifecycle::Stopped => {
                    let runtime = spawn_runtime(
                        self.inner.config.clone(),
                        Arc::clone(&self.inner.ingress_owned),
                        Arc::clone(&self.inner.diagnostics),
                        Arc::clone(&self.inner.sessions),
                    )?;
                    host.runtime = Some(runtime);
                    host.fully_connected = false;
                }
                HostLifecycle::Starting | HostLifecycle::Running => {}
            }
            let id = ConnectAttemptId::next(host.connect_counter)?;
            host.connect_counter = id.0;
            let attempt = Arc::new(HostConnectAttempt {
                id,
                completion: Arc::new(Completion::new()),
            });
            host.connect_attempt = Some(Arc::clone(&attempt));
            host.lifecycle = HostLifecycle::Starting;
            let runtime = host
                .runtime
                .as_ref()
                .map(RuntimeRef::from)
                .ok_or_else(|| RuntimeError::internal("connect attempt has no runtime"))?;
            (attempt, runtime)
        };

        let command = RuntimeCommand::ConnectAll {
            attempt_id: attempt.id,
            completion: Arc::clone(&attempt.completion),
        };
        if let Err(send_error) = runtime.command_tx.try_send(command) {
            let error = RuntimeError::connection_closed(format!(
                "7709 runtime command channel rejected explicit connect: {send_error}"
            ));
            attempt.completion.publish(Err(error));
            runtime.control.request_close()?;
        }
        Ok(PendingConnect {
            created_pid: self.inner.created_pid,
            engine: self.clone(),
            attempt,
            terminal: None,
        })
    }

    pub fn begin_execute(&self, request: CommandRequest) -> Result<PendingExecution, RuntimeError> {
        self.check_pid()?;
        let (runtime, request_id, ingress) = self.reserve_submission()?;
        let deadline = Deadline::after(self.inner.config.request_timeout)?;
        let (admission_tx, admission_rx) = std_mpsc::sync_channel(1);
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        let command = RuntimeCommand::Execute {
            request_id,
            request,
            deadline,
            admission: admission_tx,
            result: result_tx,
            ingress,
        };
        runtime.command_tx.blocking_send(command).map_err(|_| {
            RuntimeError::connection_closed("7709 runtime command channel is closed")
        })?;
        admission_rx.recv().map_err(|_| {
            RuntimeError::connection_closed("7709 runtime stopped before admission")
        })??;
        Ok(PendingExecution {
            created_pid: self.inner.created_pid,
            request_id,
            control: runtime.control,
            result_rx,
            terminal: None,
        })
    }

    pub fn execute(&self, request: CommandRequest) -> Result<CommandResponse, RuntimeError> {
        self.begin_execute(request)?.wait()
    }

    pub fn begin_pin(&self) -> Result<PendingPin, RuntimeError> {
        self.check_pid()?;
        let (runtime, request_id, ingress) = self.reserve_submission()?;
        let deadline = Deadline::after(self.inner.config.request_timeout)?;
        let (admission_tx, admission_rx) = std_mpsc::sync_channel(1);
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        runtime
            .command_tx
            .blocking_send(RuntimeCommand::OpenPin {
                request_id,
                deadline,
                admission: admission_tx,
                result: result_tx,
                ingress,
            })
            .map_err(|_| {
                RuntimeError::connection_closed("7709 runtime command channel is closed")
            })?;
        admission_rx.recv().map_err(|_| {
            RuntimeError::connection_closed("7709 runtime stopped before pin admission")
        })??;
        Ok(PendingPin {
            created_pid: self.inner.created_pid,
            engine: self.clone(),
            request_id,
            control: runtime.control,
            result_rx,
        })
    }

    pub fn pin(&self) -> Result<PinHandle, RuntimeError> {
        self.begin_pin()?.wait()
    }

    fn begin_pinned_execute(
        &self,
        pin: PinIdentity,
        request: CommandRequest,
    ) -> Result<PendingExecution, RuntimeError> {
        self.check_pid()?;
        let (runtime, request_id, ingress) = self.reserve_submission()?;
        let deadline = Deadline::after(self.inner.config.request_timeout)?;
        let (admission_tx, admission_rx) = std_mpsc::sync_channel(1);
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        runtime
            .command_tx
            .blocking_send(RuntimeCommand::ExecutePinned {
                pin,
                request_id,
                request,
                deadline,
                admission: admission_tx,
                result: result_tx,
                ingress,
            })
            .map_err(|_| {
                RuntimeError::connection_closed("7709 runtime command channel is closed")
            })?;
        admission_rx.recv().map_err(|_| {
            RuntimeError::connection_closed("7709 runtime stopped before pinned admission")
        })??;
        Ok(PendingExecution {
            created_pid: self.inner.created_pid,
            request_id,
            control: runtime.control,
            result_rx,
            terminal: None,
        })
    }

    fn begin_pin_close(&self, pin: PinIdentity) -> Result<PendingPinClose, RuntimeError> {
        self.check_pid()?;
        let runtime = self.current_runtime()?.ok_or_else(|| {
            RuntimeError::connection_closed("pinned proxy belongs to a stopped Engine")
        })?;
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        runtime
            .command_tx
            .blocking_send(RuntimeCommand::ClosePin {
                pin,
                reply: result_tx,
            })
            .map_err(|_| {
                RuntimeError::connection_closed("7709 runtime command channel is closed")
            })?;
        Ok(PendingPinClose {
            created_pid: self.inner.created_pid,
            result_rx,
            terminal: None,
        })
    }

    pub fn poll_push(&self, timeout: Duration) -> Result<Option<PushFrame>, RuntimeError> {
        self.check_pid()?;
        let runtime = self.current_runtime()?;
        let Some(runtime) = runtime else {
            return Ok(None);
        };
        let deadline = Deadline::after(timeout)?;
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(1);
        runtime
            .command_tx
            .blocking_send(RuntimeCommand::PollPush {
                deadline,
                reply: reply_tx,
            })
            .map_err(|_| {
                RuntimeError::connection_closed("7709 runtime command channel is closed")
            })?;
        reply_rx
            .recv()
            .map_err(|_| RuntimeError::connection_closed("7709 runtime stopped during push poll"))?
    }

    pub fn drain_pushes(&self) -> Result<Vec<PushFrame>, RuntimeError> {
        self.check_pid()?;
        let runtime = self.current_runtime()?;
        let Some(runtime) = runtime else {
            return Ok(Vec::new());
        };
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(1);
        runtime
            .command_tx
            .blocking_send(RuntimeCommand::DrainPushes { reply: reply_tx })
            .map_err(|_| {
                RuntimeError::connection_closed("7709 runtime command channel is closed")
            })?;
        reply_rx.recv().map_err(|_| {
            RuntimeError::connection_closed("7709 runtime stopped during push drain")
        })?
    }

    pub fn pool_diagnostics(&self) -> Result<PoolDiagnostics, RuntimeError> {
        self.check_pid()?;
        if let Some(runtime) = self.diagnostics_runtime()? {
            let (reply, received) = std_mpsc::sync_channel(1);
            if runtime
                .command_tx
                .blocking_send(RuntimeCommand::PoolDiagnostics { reply })
                .is_ok()
            {
                if let Ok(result) = received.recv() {
                    return result.and_then(|value| self.overlay_pool_diagnostics(value));
                }
            }
        }
        let cached = lock_mutex(&self.inner.diagnostics, "Engine diagnostics")?
            .pool
            .clone();
        self.overlay_pool_diagnostics(cached)
    }

    pub fn transport_diagnostics(&self) -> Result<TransportDiagnostics, RuntimeError> {
        self.check_pid()?;
        if self.inner.config.pool_size != 1 {
            return Err(RuntimeError::internal(
                "standalone transport diagnostics require pool_size=1",
            ));
        }
        if let Some(runtime) = self.diagnostics_runtime()? {
            let (reply, received) = std_mpsc::sync_channel(1);
            if runtime
                .command_tx
                .blocking_send(RuntimeCommand::TransportDiagnostics { reply })
                .is_ok()
            {
                if let Ok(result) = received.recv() {
                    return result.and_then(|value| self.overlay_transport_diagnostics(value));
                }
            }
        }
        let cached = lock_mutex(&self.inner.diagnostics, "Engine diagnostics")?
            .transport
            .clone()
            .ok_or_else(|| RuntimeError::internal("transport diagnostics cache is missing"))?;
        self.overlay_transport_diagnostics(cached)
    }

    pub fn session_snapshot(
        &self,
        slot_index: usize,
    ) -> Result<(Option<HandshakeInfo>, Option<HeartbeatAck>), RuntimeError> {
        self.check_pid()?;
        if slot_index >= self.inner.config.pool_size {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "session snapshot Slot index is outside the configured pool",
            ));
        }
        let sessions = lock_mutex(&self.inner.sessions, "Engine session snapshots")?;
        let slot_id = SlotId::new(slot_index);
        Ok((
            sessions.handshakes.get(&slot_id).cloned(),
            sessions.heartbeats.get(&slot_id).cloned(),
        ))
    }

    fn overlay_pool_diagnostics(
        &self,
        mut diagnostics: PoolDiagnostics,
    ) -> Result<PoolDiagnostics, RuntimeError> {
        let host = lock_mutex(&self.inner.host, "Engine diagnostics lifecycle")?;
        let lifecycle = host.lifecycle;
        let (pool_state, runtime_state) = host_diagnostic_states(lifecycle);
        if lifecycle != HostLifecycle::Running {
            diagnostics.state = pool_state;
            for actor in &mut diagnostics.actors {
                actor.state = runtime_state;
            }
        }
        if host.lifecycle == HostLifecycle::Starting {
            diagnostics.broker = None;
        }
        Ok(diagnostics)
    }

    fn overlay_transport_diagnostics(
        &self,
        mut diagnostics: TransportDiagnostics,
    ) -> Result<TransportDiagnostics, RuntimeError> {
        let host = lock_mutex(&self.inner.host, "Engine diagnostics lifecycle")?;
        let (_, runtime_state) = host_diagnostic_states(host.lifecycle);
        if host.lifecycle != HostLifecycle::Running {
            if let Some(actor) = diagnostics.actor.as_mut() {
                actor.state = runtime_state;
            }
        }
        Ok(diagnostics)
    }

    pub fn close(&self) -> Result<(), RuntimeError> {
        self.begin_close()?.wait()
    }

    pub fn begin_close(&self) -> Result<PendingClose, RuntimeError> {
        self.begin_close_with_timeout(CLOSE_TIMEOUT)
    }

    fn begin_close_with_timeout(&self, timeout: Duration) -> Result<PendingClose, RuntimeError> {
        self.check_pid()?;
        let proposed_deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| RuntimeError::internal("close deadline overflow"))?;
        let (attempt, runtime) = {
            let mut host = lock_mutex(&self.inner.host, "Engine host")?;
            let Some(runtime) = host.runtime.as_ref().map(RuntimeRef::from) else {
                let result = match host.lifecycle {
                    HostLifecycle::FailedClosed | HostLifecycle::Stopped => Ok(()),
                    _ => Err(RuntimeError::internal(
                        "Engine lifecycle retained no runtime thread",
                    )),
                };
                return Ok(PendingClose {
                    created_pid: self.inner.created_pid,
                    engine: self.clone(),
                    attempt: None,
                    runtime: None,
                    terminal: Some(result),
                });
            };
            if let Some(attempt) = host.close_attempt.as_ref() {
                return Ok(PendingClose {
                    created_pid: self.inner.created_pid,
                    engine: self.clone(),
                    attempt: Some(Arc::clone(attempt)),
                    runtime: Some(runtime),
                    terminal: None,
                });
            }
            let id = HostCloseAttemptId::next(host.close_counter)?;
            let attempt = Arc::new(HostCloseAttempt {
                id,
                deadline: proposed_deadline,
                completion: Arc::new(Completion::new()),
            });
            host.lifecycle = if host.failed_close_lineage {
                HostLifecycle::FailedClosing
            } else {
                HostLifecycle::Closing
            };
            host.close_counter = id.0;
            host.close_attempt = Some(Arc::clone(&attempt));
            if let Err(error) = runtime.control.request_close() {
                host.lifecycle = HostLifecycle::FailedClosing;
                host.failed_close_lineage = true;
                host.close_attempt = None;
                attempt.completion.publish(Err(error.clone()));
                return Err(error);
            }
            (attempt, runtime)
        };
        Ok(PendingClose {
            created_pid: self.inner.created_pid,
            engine: self.clone(),
            attempt: Some(attempt),
            runtime: Some(runtime),
            terminal: None,
        })
    }

    fn mark_host_close_timeout(
        &self,
        attempt: &HostCloseAttempt,
        runtime: &RuntimeRef,
    ) -> Result<(), RuntimeError> {
        let mut host = lock_mutex(&self.inner.host, "Engine host")?;
        let runtime_matches = host
            .runtime
            .as_ref()
            .is_some_and(|thread| Arc::ptr_eq(&thread.exited, &runtime.exited));
        let attempt_matches = host
            .close_attempt
            .as_ref()
            .is_some_and(|current| current.id == attempt.id);
        if !runtime_matches || !attempt_matches {
            return Ok(());
        }
        runtime.control.mark_close_timeout()?;
        host.lifecycle = HostLifecycle::FailedClosing;
        host.failed_close_lineage = true;
        host.close_attempt = None;
        let error = RuntimeError::CloseTimeout {
            message: "7709 Engine close did not finish within 1.0 seconds".to_owned(),
            context: Vec::new(),
        };
        attempt.completion.publish(Err(error));
        Ok(())
    }

    fn finish_host_close(
        &self,
        attempt: &HostCloseAttempt,
        runtime: &RuntimeRef,
        result: Result<(), RuntimeError>,
    ) -> Result<(), RuntimeError> {
        let mut host = lock_mutex(&self.inner.host, "Engine host")?;
        let runtime_matches = host
            .runtime
            .as_ref()
            .is_some_and(|thread| Arc::ptr_eq(&thread.exited, &runtime.exited));
        let attempt_matches = host
            .close_attempt
            .as_ref()
            .is_some_and(|current| current.id == attempt.id);
        if !runtime_matches || !attempt_matches {
            drop(host);
            return attempt.completion.wait(Duration::ZERO)?.unwrap_or(result);
        }
        let failed_lineage = host.failed_close_lineage;
        if let Some(mut thread) = host.runtime.take() {
            if let Some(join) = thread.join.take() {
                if join.join().is_err() {
                    let error = RuntimeError::internal("7709 runtime thread panicked during join");
                    host.lifecycle = HostLifecycle::FailedClosed;
                    host.failed_close_lineage = true;
                    if let Some(attempt) = host.connect_attempt.as_ref() {
                        attempt.completion.publish(Err(RuntimeError::internal(
                            "7709 runtime thread panicked during connect cleanup",
                        )));
                    }
                    host.connect_attempt = None;
                    host.close_attempt = None;
                    host.fully_connected = false;
                    attempt.completion.publish(Err(error.clone()));
                    return Err(error);
                }
            }
        }
        let failed = result.is_err();
        host.lifecycle = if failed_lineage || failed {
            HostLifecycle::FailedClosed
        } else {
            HostLifecycle::Stopped
        };
        if let Some(attempt) = host.connect_attempt.as_ref() {
            let error = result.clone().err().unwrap_or_else(|| {
                RuntimeError::connection_closed("Engine closed during explicit connect")
            });
            attempt.completion.publish(Err(error));
        }
        host.connect_attempt = None;
        host.close_attempt = None;
        host.fully_connected = false;
        host.failed_close_lineage |= failed;
        attempt.completion.publish(result.clone());
        result
    }

    fn finish_connect_attempt(
        &self,
        attempt_id: ConnectAttemptId,
        result: &Result<(), RuntimeError>,
    ) -> Result<bool, RuntimeError> {
        self.check_pid()?;
        let runtime = {
            let mut host = lock_mutex(&self.inner.host, "Engine connect completion")?;
            let current_matches = host
                .connect_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.id == attempt_id);
            if !current_matches {
                return match result {
                    Ok(()) if host.lifecycle == HostLifecycle::Running && host.fully_connected => {
                        Ok(true)
                    }
                    Ok(()) => Err(RuntimeError::connection_closed(
                        "7709 Engine close won the idempotent connect race",
                    )),
                    Err(_) => Ok(matches!(
                        host.lifecycle,
                        HostLifecycle::Stopped | HostLifecycle::FailedClosed
                    )),
                };
            }
            if result.is_ok() {
                if !matches!(
                    host.lifecycle,
                    HostLifecycle::Starting | HostLifecycle::Running
                ) {
                    return Err(RuntimeError::connection_closed(
                        "7709 Engine close won the explicit-connect publication race",
                    ));
                }
                host.lifecycle = HostLifecycle::Running;
                host.fully_connected = true;
                host.connect_attempt = None;
                return Ok(true);
            }
            host.runtime.as_ref().map(RuntimeRef::from)
        };
        let Some(runtime) = runtime else {
            return Ok(false);
        };
        let Some(runtime_result) = runtime.exited.wait(Duration::ZERO)? else {
            return Ok(false);
        };
        let mut host = lock_mutex(&self.inner.host, "Engine failed connect cleanup")?;
        if host
            .connect_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.id == attempt_id)
        {
            let mut thread = host.runtime.take().ok_or_else(|| {
                RuntimeError::internal("failed connect cleanup lost its runtime thread")
            })?;
            if let Some(join) = thread.join.take() {
                if join.join().is_err() {
                    let error = RuntimeError::internal(
                        "7709 runtime thread panicked during connect rollback join",
                    );
                    host.lifecycle = HostLifecycle::FailedClosed;
                    host.failed_close_lineage = true;
                    host.connect_attempt = None;
                    if let Some(close) = host.close_attempt.take() {
                        close.completion.publish(Err(error.clone()));
                    }
                    host.fully_connected = false;
                    return Err(error);
                }
            }
            let cleanup_failed = runtime_result.is_err() || host.failed_close_lineage;
            host.lifecycle = if cleanup_failed {
                HostLifecycle::FailedClosed
            } else {
                HostLifecycle::Stopped
            };
            host.failed_close_lineage |= cleanup_failed;
            host.connect_attempt = None;
            if let Some(close) = host.close_attempt.take() {
                close.completion.publish(runtime_result.clone());
            }
            host.fully_connected = false;
        }
        Ok(true)
    }

    fn connect_runtime_exit(
        &self,
        attempt_id: ConnectAttemptId,
    ) -> Result<Option<RuntimeError>, RuntimeError> {
        self.check_pid()?;
        let runtime = {
            let host = lock_mutex(&self.inner.host, "Engine connect exit observation")?;
            if !host
                .connect_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.id == attempt_id)
            {
                return Ok(None);
            }
            host.runtime.as_ref().map(RuntimeRef::from)
        };
        let Some(runtime) = runtime else {
            return Ok(Some(RuntimeError::internal(
                "connect attempt lost its runtime before terminal publication",
            )));
        };
        Ok(match runtime.exited.wait(Duration::ZERO)? {
            Some(Err(error)) => Some(error),
            Some(Ok(())) => Some(RuntimeError::internal(
                "runtime exited without publishing the explicit-connect terminal",
            )),
            None => None,
        })
    }

    fn check_pid(&self) -> Result<(), RuntimeError> {
        check_pid(self.inner.created_pid)
    }

    fn ensure_runtime(&self) -> Result<RuntimeRef, RuntimeError> {
        self.check_pid()?;
        let startup =
            {
                let mut host = lock_mutex(&self.inner.host, "Engine host")?;
                match host.lifecycle {
                    HostLifecycle::Running => {
                        return host.runtime.as_ref().map(RuntimeRef::from).ok_or_else(|| {
                            RuntimeError::internal("running Engine has no runtime")
                        });
                    }
                    HostLifecycle::Starting => host
                        .runtime
                        .as_ref()
                        .map(|runtime| Arc::clone(&runtime.startup))
                        .ok_or_else(|| RuntimeError::internal("starting Engine has no runtime"))?,
                    HostLifecycle::Stopped => {
                        let runtime = spawn_runtime(
                            self.inner.config.clone(),
                            Arc::clone(&self.inner.ingress_owned),
                            Arc::clone(&self.inner.diagnostics),
                            Arc::clone(&self.inner.sessions),
                        )?;
                        let startup = Arc::clone(&runtime.startup);
                        host.runtime = Some(runtime);
                        host.lifecycle = HostLifecycle::Starting;
                        host.fully_connected = false;
                        startup
                    }
                    HostLifecycle::Closing
                    | HostLifecycle::FailedClosing
                    | HostLifecycle::FailedClosed => {
                        return Err(RuntimeError::connection_closed(
                            "7709 Engine cannot start after close linearization",
                        ));
                    }
                }
            };
        let startup_timeout = self.inner.config.request_timeout;
        let result = startup
            .wait(startup_timeout)?
            .ok_or_else(|| RuntimeError::timeout(TimeoutPhase::Startup))?;
        let mut host = lock_mutex(&self.inner.host, "Engine host")?;
        match result {
            Ok(()) => {
                match host.lifecycle {
                    HostLifecycle::Starting if host.connect_attempt.is_none() => {
                        host.lifecycle = HostLifecycle::Running;
                    }
                    HostLifecycle::Starting => {}
                    HostLifecycle::Running => {}
                    HostLifecycle::Closing
                    | HostLifecycle::FailedClosing
                    | HostLifecycle::FailedClosed => {
                        return Err(RuntimeError::connection_closed(
                            "7709 Engine close linearized during startup",
                        ));
                    }
                    HostLifecycle::Stopped => {
                        return Err(RuntimeError::internal(
                            "runtime published startup after host returned to Stopped",
                        ));
                    }
                }
                host.runtime
                    .as_ref()
                    .map(RuntimeRef::from)
                    .ok_or_else(|| RuntimeError::internal("started Engine lost its runtime"))
            }
            Err(error) => {
                host.lifecycle = HostLifecycle::Stopped;
                if let Some(attempt) = host.connect_attempt.take() {
                    attempt.completion.publish(Err(error.clone()));
                }
                host.fully_connected = false;
                let runtime = host.runtime.take();
                drop(host);
                if let Some(mut runtime) = runtime {
                    let _ = runtime.exited.wait(CLOSE_TIMEOUT);
                    if let Some(join) = runtime.join.take() {
                        let _ = join.join();
                    }
                }
                Err(error)
            }
        }
    }

    fn current_runtime(&self) -> Result<Option<RuntimeRef>, RuntimeError> {
        self.check_pid()?;
        let host = lock_mutex(&self.inner.host, "Engine host")?;
        match host.lifecycle {
            HostLifecycle::Stopped | HostLifecycle::FailedClosed => Ok(None),
            HostLifecycle::Starting if host.connect_attempt.is_some() => Err(
                RuntimeError::connection_closed("7709 explicit connect is still in progress"),
            ),
            HostLifecycle::Starting | HostLifecycle::Running => {
                Ok(host.runtime.as_ref().map(RuntimeRef::from))
            }
            HostLifecycle::Closing | HostLifecycle::FailedClosing => Err(
                RuntimeError::connection_closed("7709 Engine close has linearized"),
            ),
        }
    }

    fn diagnostics_runtime(&self) -> Result<Option<RuntimeRef>, RuntimeError> {
        let host = lock_mutex(&self.inner.host, "Engine diagnostics gate")?;
        Ok(match host.lifecycle {
            HostLifecycle::Starting | HostLifecycle::Running => {
                host.runtime.as_ref().map(RuntimeRef::from)
            }
            HostLifecycle::Stopped
            | HostLifecycle::Closing
            | HostLifecycle::FailedClosing
            | HostLifecycle::FailedClosed => None,
        })
    }

    fn reserve_submission(
        &self,
    ) -> Result<(RuntimeRef, RequestId, IngressOwnership), RuntimeError> {
        let runtime = self.ensure_runtime()?;
        let mut host = lock_mutex(&self.inner.host, "Engine submit gate")?;
        if host.lifecycle != HostLifecycle::Running {
            return Err(RuntimeError::connection_closed(
                "7709 Engine close has linearized",
            ));
        }
        let capacity = self.inner.config.total_capacity()?;
        let owned = self.inner.ingress_owned.load(Ordering::Acquire);
        if owned >= capacity {
            return Err(RuntimeError::PoolBusy {
                message: "7709 request capacity is full".to_owned(),
                capacity,
                context: Vec::new(),
            });
        }
        self.inner.ingress_owned.fetch_add(1, Ordering::AcqRel);
        let next = if host.request_counter == 0 {
            1
        } else {
            host.request_counter
                .checked_add(2)
                .ok_or_else(|| RuntimeError::internal("request identity space exhausted"))?
        };
        let request_id = match RequestId::new(next) {
            Ok(value) => value,
            Err(error) => {
                self.inner.ingress_owned.fetch_sub(1, Ordering::AcqRel);
                return Err(error);
            }
        };
        host.request_counter = next;
        Ok((
            runtime,
            request_id,
            IngressOwnership {
                owned: Arc::clone(&self.inner.ingress_owned),
            },
        ))
    }
}

impl Drop for EngineInner {
    fn drop(&mut self) {
        if std::process::id() != self.created_pid {
            return;
        }
        match self.host.try_lock() {
            Ok(host) => {
                if let Some(runtime) = host.runtime.as_ref() {
                    let _ = runtime.control.request_close();
                }
            }
            Err(TryLockError::Poisoned(error)) => {
                if let Some(runtime) = error.into_inner().runtime.as_ref() {
                    let _ = runtime.control.request_close();
                }
            }
            Err(TryLockError::WouldBlock) => {}
        }
    }
}

#[derive(Clone, Debug)]
struct RuntimeRef {
    command_tx: mpsc::Sender<RuntimeCommand>,
    control: Arc<ControlCell>,
    exited: Arc<Completion<Result<(), RuntimeError>>>,
}

impl From<&RuntimeThread> for RuntimeRef {
    fn from(value: &RuntimeThread) -> Self {
        Self {
            command_tx: value.command_tx.clone(),
            control: Arc::clone(&value.control),
            exited: Arc::clone(&value.exited),
        }
    }
}

#[derive(Debug)]
enum RuntimeCommand {
    ConnectAll {
        attempt_id: ConnectAttemptId,
        completion: Arc<Completion<Result<(), RuntimeError>>>,
    },
    Execute {
        request_id: RequestId,
        request: CommandRequest,
        deadline: Deadline,
        admission: std_mpsc::SyncSender<Result<(), RuntimeError>>,
        result: std_mpsc::SyncSender<Result<RawExecution, RuntimeError>>,
        ingress: IngressOwnership,
    },
    OpenPin {
        request_id: RequestId,
        deadline: Deadline,
        admission: std_mpsc::SyncSender<Result<(), RuntimeError>>,
        result: std_mpsc::SyncSender<Result<PinIdentity, RuntimeError>>,
        ingress: IngressOwnership,
    },
    ExecutePinned {
        pin: PinIdentity,
        request_id: RequestId,
        request: CommandRequest,
        deadline: Deadline,
        admission: std_mpsc::SyncSender<Result<(), RuntimeError>>,
        result: std_mpsc::SyncSender<Result<RawExecution, RuntimeError>>,
        ingress: IngressOwnership,
    },
    ClosePin {
        pin: PinIdentity,
        reply: std_mpsc::SyncSender<Result<(), RuntimeError>>,
    },
    PollPush {
        deadline: Deadline,
        reply: std_mpsc::SyncSender<Result<Option<PushFrame>, RuntimeError>>,
    },
    DrainPushes {
        reply: std_mpsc::SyncSender<Result<Vec<PushFrame>, RuntimeError>>,
    },
    PoolDiagnostics {
        reply: std_mpsc::SyncSender<Result<PoolDiagnostics, RuntimeError>>,
    },
    TransportDiagnostics {
        reply: std_mpsc::SyncSender<Result<TransportDiagnostics, RuntimeError>>,
    },
}

fn spawn_runtime(
    config: EngineConfig,
    ingress_owned: Arc<AtomicUsize>,
    diagnostics: Arc<Mutex<DiagnosticsCache>>,
    sessions: Arc<Mutex<SessionCache>>,
) -> Result<RuntimeThread, RuntimeError> {
    let capacity = config.total_capacity()?;
    let (command_tx, command_rx) = mpsc::channel(capacity);
    let control = Arc::new(ControlCell::new());
    let startup = Arc::new(Completion::new());
    let exited = Arc::new(Completion::new());
    let thread_control = Arc::clone(&control);
    let thread_startup = Arc::clone(&startup);
    let thread_startup_fallback = Arc::clone(&startup);
    let thread_exited = Arc::clone(&exited);
    let join = thread::Builder::new()
        .name("eltdx-runtime".to_owned())
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                runtime_thread_main(
                    config,
                    command_rx,
                    thread_control,
                    thread_startup,
                    ingress_owned,
                    diagnostics,
                    sessions,
                )
            }))
            .unwrap_or_else(|_| Err(RuntimeError::internal("7709 runtime thread panicked")));
            let startup_fallback = match &result {
                Ok(()) => Err(RuntimeError::internal(
                    "7709 runtime exited before publishing startup",
                )),
                Err(error) => Err(error.clone()),
            };
            thread_startup_fallback.publish(startup_fallback);
            thread_exited.publish(result);
        })
        .map_err(|error| {
            RuntimeError::internal(format!("unable to start 7709 runtime thread: {error}"))
        })?;
    Ok(RuntimeThread {
        command_tx,
        control,
        startup,
        exited,
        join: Some(join),
    })
}

fn runtime_thread_main(
    config: EngineConfig,
    command_rx: mpsc::Receiver<RuntimeCommand>,
    control: Arc<ControlCell>,
    startup: Arc<Completion<Result<(), RuntimeError>>>,
    ingress_owned: Arc<AtomicUsize>,
    diagnostics: Arc<Mutex<DiagnosticsCache>>,
    sessions: Arc<Mutex<SessionCache>>,
) -> Result<(), RuntimeError> {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            RuntimeError::internal(format!("unable to build Tokio runtime: {error}"))
        })?;
    runtime.block_on(
        RuntimeCore::new(
            config,
            command_rx,
            control,
            ingress_owned,
            diagnostics,
            sessions,
        )?
        .run(startup),
    )
}

fn duration_from_seconds(
    name: &'static str,
    seconds: f64,
    allow_none_value: bool,
) -> Result<Duration, RuntimeError> {
    if !seconds.is_finite() || seconds <= 0.0 {
        let suffix = if allow_none_value { " or None" } else { "" };
        return Err(RuntimeError::invalid_argument(
            "ValueError",
            format!("{name} must be a finite number > 0{suffix}"),
        ));
    }
    Duration::try_from_secs_f64(seconds).map_err(|_| {
        RuntimeError::invalid_argument("OverflowError", format!("{name} is too large"))
    })
}

fn check_pid(created_pid: u32) -> Result<(), RuntimeError> {
    let current_pid = std::process::id();
    if current_pid == created_pid {
        return Ok(());
    }
    Err(RuntimeError::connection_closed(
        "7709 Engine cannot be reused after fork; create a new TdxClient in the child process",
    )
    .with_context("created_pid", created_pid.to_string())
    .with_context("current_pid", current_pid.to_string()))
}

fn host_diagnostic_states(lifecycle: HostLifecycle) -> (PoolState, RuntimeState) {
    match lifecycle {
        HostLifecycle::Stopped => (PoolState::Stopped, RuntimeState::Stopped),
        HostLifecycle::Starting => (PoolState::Starting, RuntimeState::Starting),
        HostLifecycle::Running => (PoolState::Running, RuntimeState::Running),
        HostLifecycle::Closing => (PoolState::Closing, RuntimeState::Closing),
        HostLifecycle::FailedClosing => (PoolState::FailedClosing, RuntimeState::FailedClosing),
        HostLifecycle::FailedClosed => (PoolState::FailedClosed, RuntimeState::FailedClosed),
    }
}

fn lock_mutex<'a, T>(
    mutex: &'a Mutex<T>,
    name: &'static str,
) -> Result<MutexGuard<'a, T>, RuntimeError> {
    mutex
        .lock()
        .map_err(|_| RuntimeError::internal(format!("{name} mutex is poisoned")))
}

// RuntimeCore and SlotWorker are defined below. Keeping the host boundary above free of Tokio
// handles is what makes the pre-lock PID check and child-process finalizer rule enforceable.

#[derive(Debug)]
struct PendingRequest {
    request: CommandRequest,
    result: std_mpsc::SyncSender<Result<RawExecution, RuntimeError>>,
    _ingress: IngressOwnership,
    admission: Admission,
    wire: Option<RequestWireIdentity>,
    response: Option<ResponseFrame>,
    dispatched: bool,
    pin: Option<PinIdentity>,
}

#[derive(Debug)]
struct PendingPinReservation {
    result: std_mpsc::SyncSender<Result<PinIdentity, RuntimeError>>,
    _ingress: IngressOwnership,
    admission: Admission,
}

#[derive(Clone, Debug)]
enum RetirementPlan {
    ContinueEndpoint,
    Retry,
    Terminal {
        kind: TerminalKind,
        error: RuntimeError,
    },
    PinnedTerminal {
        pin: PinIdentity,
        kind: TerminalKind,
        error: RuntimeError,
    },
    HeartbeatTerminal {
        kind: TerminalKind,
        error: RuntimeError,
    },
    Lifecycle,
}

#[derive(Debug)]
struct ConnectBatch {
    attempt_id: ConnectAttemptId,
    completion: Arc<Completion<Result<(), RuntimeError>>>,
    remaining: BTreeSet<SlotId>,
    first_error: Option<RuntimeError>,
    rolling_back: bool,
}

#[derive(Debug)]
struct PushPollWaiter {
    deadline: Deadline,
    reply: std_mpsc::SyncSender<Result<Option<PushFrame>, RuntimeError>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HeartbeatWork {
    attempt: RequestAttempt,
    candidate: HeartbeatCandidate,
}

#[derive(Debug)]
struct RuntimeCore {
    config: EngineConfig,
    supervisor: Supervisor,
    command_rx: mpsc::Receiver<RuntimeCommand>,
    control: Arc<ControlCell>,
    ingress_owned: Arc<AtomicUsize>,
    diagnostics: Arc<Mutex<DiagnosticsCache>>,
    sessions: Arc<Mutex<SessionCache>>,
    pending: BTreeMap<RequestId, PendingRequest>,
    pin_reservations: BTreeMap<RequestId, PendingPinReservation>,
    pin_close_waiters: BTreeMap<u64, Vec<std_mpsc::SyncSender<Result<(), RuntimeError>>>>,
    internal_request_counter: u64,
    heartbeat_requests: BTreeSet<RequestId>,
    retirements: BTreeMap<RequestId, RetirementPlan>,
    cancel_confirmations: BTreeMap<RequestId, Arc<Completion<Result<(), RuntimeError>>>>,
    slots: Vec<Option<SlotHandle>>,
    slot_event_tx: mpsc::Sender<SlotEvent>,
    slot_event_rx: mpsc::Receiver<SlotEvent>,
    push_tx: mpsc::Sender<PushFrame>,
    push_rx: mpsc::Receiver<PushFrame>,
    push_dropped: Arc<AtomicU64>,
    connect_batch: Option<ConnectBatch>,
    push_waiters: Vec<PushPollWaiter>,
    close_attempt: Option<crate::supervisor::CloseAttempt>,
    exit_error: Option<RuntimeError>,
    closing: bool,
    command_closed: bool,
}

impl RuntimeCore {
    fn new(
        config: EngineConfig,
        command_rx: mpsc::Receiver<RuntimeCommand>,
        control: Arc<ControlCell>,
        ingress_owned: Arc<AtomicUsize>,
        diagnostics: Arc<Mutex<DiagnosticsCache>>,
        sessions: Arc<Mutex<SessionCache>>,
    ) -> Result<Self, RuntimeError> {
        let event_capacity = config
            .total_capacity()?
            .checked_add(config.pool_size.saturating_mul(8))
            .ok_or_else(|| RuntimeError::internal("runtime event capacity overflow"))?;
        let (slot_event_tx, slot_event_rx) = mpsc::channel(event_capacity.max(1));
        let (push_tx, push_rx) = mpsc::channel(config.push_queue_size);
        Ok(Self {
            supervisor: Supervisor::with_limits(
                config.pool_size,
                config.max_pending_requests,
                config.push_queue_size,
                config.push_queue_bytes,
            )?,
            slots: (0..config.pool_size).map(|_| None).collect(),
            config,
            command_rx,
            control,
            ingress_owned,
            diagnostics,
            sessions,
            pending: BTreeMap::new(),
            pin_reservations: BTreeMap::new(),
            pin_close_waiters: BTreeMap::new(),
            internal_request_counter: 0,
            heartbeat_requests: BTreeSet::new(),
            retirements: BTreeMap::new(),
            cancel_confirmations: BTreeMap::new(),
            slot_event_tx,
            slot_event_rx,
            push_tx,
            push_rx,
            push_dropped: Arc::new(AtomicU64::new(0)),
            connect_batch: None,
            push_waiters: Vec::new(),
            close_attempt: None,
            exit_error: None,
            closing: false,
            command_closed: false,
        })
    }

    async fn run(
        mut self,
        startup: Arc<Completion<Result<(), RuntimeError>>>,
    ) -> Result<(), RuntimeError> {
        *lock_mutex(&self.sessions, "runtime session snapshots")? = SessionCache::default();
        let start_claim = match self.supervisor.begin_start() {
            Ok(claim) => claim,
            Err(error) => {
                startup.publish(Err(error.clone()));
                self.reject_startup_commands(error.clone());
                return Err(error);
            }
        };
        let attempt = match start_claim {
            StartClaim::Owner(attempt) => attempt,
            StartClaim::Existing(_) | StartClaim::Running(_) => {
                let error = RuntimeError::internal("new runtime did not own its StartAttempt");
                startup.publish(Err(error.clone()));
                self.reject_startup_commands(error.clone());
                return Err(error);
            }
        };
        let published = match self.supervisor.publish_start(attempt) {
            Ok(published) => published,
            Err(error) => {
                startup.publish(Err(error.clone()));
                self.reject_startup_commands(error.clone());
                return Err(error);
            }
        };
        if !published {
            let error = RuntimeError::internal("runtime StartAttempt publication was rejected");
            startup.publish(Err(error.clone()));
            self.reject_startup_commands(error.clone());
            return Err(error);
        }
        if let Err(error) = self.refresh_diagnostics_cache() {
            startup.publish(Err(error.clone()));
            self.reject_startup_commands(error.clone());
            return Err(error);
        }
        startup.publish(Ok(()));

        let mut tick = interval(RUNTIME_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let step = tokio::select! {
                biased;
                _ = self.control.changed.notified() => self.handle_control().await,
                event = self.slot_event_rx.recv() => {
                    match event {
                        Some(event) => self.handle_slot_event(event).await,
                        None if self.slots.iter().all(Option::is_none) => Ok(()),
                        None => Err(RuntimeError::internal("Slot control event channel closed")),
                    }
                }
                frame = self.push_rx.recv() => {
                    if let Some(frame) = frame {
                        self.supervisor.offer_push(frame);
                        self.fulfill_push_waiters();
                    }
                    Ok(())
                }
                command = self.command_rx.recv(), if !self.command_closed => {
                    match command {
                        Some(command) => self.handle_command(command).await,
                        None => {
                            self.command_closed = true;
                            if !self.closing {
                                self.begin_close().await
                            } else {
                                Ok(())
                            }
                        }
                    }
                }
                _ = tick.tick() => self.handle_tick().await,
            };
            if let Err(error) = step {
                if let Err(cleanup_error) = self.begin_runtime_fatal(error).await {
                    return Err(self.emergency_stop_and_join(cleanup_error).await);
                }
            }
            if let Err(error) = self.refresh_diagnostics_cache() {
                if let Err(cleanup_error) = self.begin_runtime_fatal(error).await {
                    return Err(self.emergency_stop_and_join(cleanup_error).await);
                }
            }
            let close_ready = match self.close_ready() {
                Ok(ready) => ready,
                Err(error) => {
                    if let Err(cleanup_error) = self.begin_runtime_fatal(error).await {
                        return Err(self.emergency_stop_and_join(cleanup_error).await);
                    }
                    false
                }
            };
            if self.closing && close_ready {
                let exit_error = self.exit_error.clone();
                self.finish_close()?;
                return exit_error.map_or(Ok(()), Err);
            }
        }
    }

    async fn emergency_stop_and_join(&mut self, cleanup_error: RuntimeError) -> RuntimeError {
        if self.exit_error.is_none() {
            self.exit_error = Some(cleanup_error.clone());
        }
        let terminal = self.exit_error.clone().unwrap_or(cleanup_error);
        self.closing = true;

        for handle in self.slots.iter().flatten() {
            let _ = handle.directive_tx.send(SlotDirective::Stop);
        }
        self.slot_event_rx.close();
        let handles = self
            .slots
            .iter_mut()
            .filter_map(Option::take)
            .collect::<Vec<_>>();
        for handle in handles {
            let _ = handle.join.await;
        }

        if let Ok(mut cache) = self.diagnostics.lock() {
            cache.pool.state = PoolState::FailedClosed;
            cache.pool.broker = None;
            cache.pool.actors.clear();
            cache.pool.push_frames = 0;
            cache.pool.push_bytes = 0;
            if let Some(transport) = cache.transport.as_mut() {
                transport.actor = None;
                transport.push_frames = 0;
                transport.push_bytes = 0;
            }
        }

        for (_, pending) in std::mem::take(&mut self.pending) {
            let _ = pending.result.send(Err(terminal.clone()));
        }
        for (_, reservation) in std::mem::take(&mut self.pin_reservations) {
            let _ = reservation.result.send(Err(terminal.clone()));
        }
        for (_, waiters) in std::mem::take(&mut self.pin_close_waiters) {
            for waiter in waiters {
                let _ = waiter.send(Err(terminal.clone()));
            }
        }
        for (_, confirmation) in std::mem::take(&mut self.cancel_confirmations) {
            confirmation.publish(Err(terminal.clone()));
        }
        for waiter in self.push_waiters.drain(..) {
            let _ = waiter.reply.send(Err(terminal.clone()));
        }
        if let Some(batch) = self.connect_batch.take() {
            batch.completion.publish(Err(terminal.clone()));
        }
        self.reject_startup_commands(terminal.clone());
        terminal
    }

    async fn begin_runtime_fatal(&mut self, error: RuntimeError) -> Result<(), RuntimeError> {
        if self.exit_error.is_none() {
            self.exit_error = Some(error.clone());
        }
        if let Some(batch) = self.connect_batch.as_mut() {
            batch.first_error.get_or_insert_with(|| error.clone());
            batch.rolling_back = true;
        }
        let epoch = self.supervisor.active_epoch().or_else(|| {
            self.supervisor
                .close_attempt()
                .and_then(|attempt| attempt.target_epoch())
        });
        if let Some(epoch) = epoch {
            self.supervisor.publish_fatal(epoch, error)?;
        }
        if self.closing {
            self.synchronize_lifecycle_cleanup().await
        } else {
            self.begin_close().await
        }
    }

    fn reject_startup_commands(&mut self, error: RuntimeError) {
        while let Ok(command) = self.command_rx.try_recv() {
            match command {
                RuntimeCommand::ConnectAll { completion, .. } => {
                    completion.publish(Err(error.clone()));
                }
                RuntimeCommand::Execute {
                    admission, result, ..
                } => {
                    let _ = admission.send(Err(error.clone()));
                    let _ = result.send(Err(error.clone()));
                }
                RuntimeCommand::OpenPin {
                    admission, result, ..
                } => {
                    let _ = admission.send(Err(error.clone()));
                    let _ = result.send(Err(error.clone()));
                }
                RuntimeCommand::ExecutePinned {
                    admission, result, ..
                } => {
                    let _ = admission.send(Err(error.clone()));
                    let _ = result.send(Err(error.clone()));
                }
                RuntimeCommand::ClosePin { reply, .. } => {
                    let _ = reply.send(Err(error.clone()));
                }
                RuntimeCommand::PollPush { reply, .. } => {
                    let _ = reply.send(Err(error.clone()));
                }
                RuntimeCommand::DrainPushes { reply } => {
                    let _ = reply.send(Err(error.clone()));
                }
                RuntimeCommand::PoolDiagnostics { reply } => {
                    let _ = reply.send(Err(error.clone()));
                }
                RuntimeCommand::TransportDiagnostics { reply } => {
                    let _ = reply.send(Err(error.clone()));
                }
            }
        }
    }

    async fn handle_command(&mut self, command: RuntimeCommand) -> Result<(), RuntimeError> {
        if self.closing {
            return self.reject_command_after_close(command);
        }
        match command {
            RuntimeCommand::ConnectAll {
                attempt_id,
                completion,
            } => self.connect_all(attempt_id, completion).await,
            RuntimeCommand::Execute {
                request_id,
                request,
                deadline,
                admission,
                result,
                ingress,
            } => {
                let now = Instant::now();
                let accepted = self.supervisor.submit_with_retry_policy(
                    request_id,
                    deadline,
                    RetryPolicy::ordinary(request.retry_safe()),
                    now,
                );
                match accepted {
                    Ok(accepted) => {
                        self.pending.insert(
                            request_id,
                            PendingRequest {
                                request,
                                result,
                                _ingress: ingress,
                                admission: accepted,
                                wire: None,
                                response: None,
                                dispatched: false,
                                pin: None,
                            },
                        );
                        let _ = admission.send(Ok(()));
                        if let Admission::Active(lease) = accepted {
                            self.dispatch_initial(lease.request_id).await?;
                        }
                    }
                    Err(error) => {
                        let _ = admission.send(Err(error.clone()));
                        let _ = result.send(Err(error));
                    }
                }
                Ok(())
            }
            RuntimeCommand::OpenPin {
                request_id,
                deadline,
                admission,
                result,
                ingress,
            } => {
                let accepted = self.supervisor.submit_with_retry_policy(
                    request_id,
                    deadline,
                    RetryPolicy::ordinary(false),
                    Instant::now(),
                );
                match accepted {
                    Ok(accepted) => {
                        self.pin_reservations.insert(
                            request_id,
                            PendingPinReservation {
                                result,
                                _ingress: ingress,
                                admission: accepted,
                            },
                        );
                        let _ = admission.send(Ok(()));
                        if matches!(accepted, Admission::Active(_)) {
                            self.complete_pin_reservation(request_id).await?;
                        }
                    }
                    Err(error) => {
                        let _ = admission.send(Err(error.clone()));
                        let _ = result.send(Err(error));
                    }
                }
                Ok(())
            }
            RuntimeCommand::ExecutePinned {
                pin,
                request_id,
                request,
                deadline,
                admission,
                result,
                ingress,
            } => {
                let accepted = self.supervisor.submit_pin(
                    pin,
                    request_id,
                    deadline,
                    RetryPolicy::ordinary(request.retry_safe()),
                    Instant::now(),
                );
                match accepted {
                    Ok(accepted) => {
                        self.pending.insert(
                            request_id,
                            PendingRequest {
                                request,
                                result,
                                _ingress: ingress,
                                admission: accepted,
                                wire: None,
                                response: None,
                                dispatched: false,
                                pin: Some(pin),
                            },
                        );
                        let _ = admission.send(Ok(()));
                        if matches!(accepted, Admission::Pinned(_)) {
                            self.dispatch_initial(request_id).await?;
                        }
                    }
                    Err(error) => {
                        let _ = admission.send(Err(error.clone()));
                        let _ = result.send(Err(error));
                    }
                }
                Ok(())
            }
            RuntimeCommand::ClosePin { pin, reply } => self.close_pin(pin, reply).await,
            RuntimeCommand::PollPush { deadline, reply } => {
                match self.supervisor.poll_push() {
                    Ok(Some(frame)) => {
                        let _ = reply.send(Ok(Some(frame)));
                    }
                    Ok(None) if deadline.is_elapsed() => {
                        let _ = reply.send(Ok(None));
                    }
                    Ok(None) if self.push_waiters.len() >= self.config.max_pending_requests => {
                        let _ = reply.send(Err(RuntimeError::PoolBusy {
                            message: "7709 push waiter queue is full".to_owned(),
                            capacity: self.config.max_pending_requests,
                            context: Vec::new(),
                        }));
                    }
                    Ok(None) => self.push_waiters.push(PushPollWaiter { deadline, reply }),
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
                Ok(())
            }
            RuntimeCommand::DrainPushes { reply } => {
                let _ = reply.send(self.supervisor.drain_pushes());
                Ok(())
            }
            RuntimeCommand::PoolDiagnostics { reply } => {
                let diagnostics = self.refresh_diagnostics_cache().map(|cache| cache.pool);
                let _ = reply.send(diagnostics);
                Ok(())
            }
            RuntimeCommand::TransportDiagnostics { reply } => {
                let diagnostics = self.refresh_diagnostics_cache().and_then(|cache| {
                    cache.transport.ok_or_else(|| {
                        RuntimeError::internal("transport diagnostics cache is missing")
                    })
                });
                let _ = reply.send(diagnostics);
                Ok(())
            }
        }
    }

    fn reject_command_after_close(&mut self, command: RuntimeCommand) -> Result<(), RuntimeError> {
        let error =
            self.supervisor.fatal().cloned().unwrap_or_else(|| {
                RuntimeError::connection_closed("7709 Engine close has linearized")
            });
        match command {
            RuntimeCommand::ConnectAll { completion, .. } => {
                completion.publish(Err(error));
            }
            RuntimeCommand::Execute {
                admission, result, ..
            } => {
                let _ = admission.send(Err(error.clone()));
                let _ = result.send(Err(error));
            }
            RuntimeCommand::OpenPin {
                admission, result, ..
            } => {
                let _ = admission.send(Err(error.clone()));
                let _ = result.send(Err(error));
            }
            RuntimeCommand::ExecutePinned {
                admission, result, ..
            } => {
                let _ = admission.send(Err(error.clone()));
                let _ = result.send(Err(error));
            }
            RuntimeCommand::ClosePin { reply, .. } => {
                let _ = reply.send(Ok(()));
            }
            RuntimeCommand::PollPush { reply, .. } => {
                let _ = reply.send(self.supervisor.poll_push());
            }
            RuntimeCommand::DrainPushes { reply } => {
                let _ = reply.send(self.supervisor.drain_pushes());
            }
            RuntimeCommand::PoolDiagnostics { reply } => {
                let _ = reply.send(self.refresh_diagnostics_cache().map(|cache| cache.pool));
            }
            RuntimeCommand::TransportDiagnostics { reply } => {
                let result = self.refresh_diagnostics_cache().and_then(|cache| {
                    cache.transport.ok_or_else(|| {
                        RuntimeError::internal("transport diagnostics cache is missing")
                    })
                });
                let _ = reply.send(result);
            }
        }
        Ok(())
    }

    fn refresh_diagnostics_cache(&self) -> Result<DiagnosticsCache, RuntimeError> {
        let mut snapshots = Vec::new();
        for handle in self.slots.iter().flatten() {
            snapshots.push(lock_mutex(&handle.snapshot, "Slot diagnostics")?.clone());
        }
        let pool = PoolDiagnostics::capture_snapshots(&self.supervisor, snapshots.clone())?;
        let transport = if self.config.pool_size == 1 {
            Some(TransportDiagnostics::capture_snapshots(
                &self.supervisor,
                snapshots,
            )?)
        } else {
            None
        };
        let cache = DiagnosticsCache { pool, transport };
        *lock_mutex(&self.diagnostics, "Engine diagnostics")? = cache.clone();
        Ok(cache)
    }

    async fn connect_all(
        &mut self,
        attempt_id: ConnectAttemptId,
        completion: Arc<Completion<Result<(), RuntimeError>>>,
    ) -> Result<(), RuntimeError> {
        if let Some(batch) = self.connect_batch.as_ref() {
            if batch.attempt_id == attempt_id && Arc::ptr_eq(&batch.completion, &completion) {
                return Ok(());
            }
            completion.publish(Err(RuntimeError::internal(
                "runtime received a second explicit-connect identity",
            )));
            return Ok(());
        }
        let epoch = self
            .supervisor
            .active_epoch()
            .ok_or_else(|| RuntimeError::connection_closed("7709 Engine is not running"))?;
        self.connect_batch = Some(ConnectBatch {
            attempt_id,
            completion,
            remaining: BTreeSet::new(),
            first_error: None,
            rolling_back: false,
        });
        let mut dispatch_error = None;
        for index in 0..self.config.pool_size {
            let slot_id = SlotId::new(index);
            if let Err(error) = self.ensure_slot(epoch, slot_id) {
                dispatch_error = Some(error);
                break;
            }
            let batch = self
                .connect_batch
                .as_mut()
                .ok_or_else(|| RuntimeError::internal("connect batch disappeared"))?;
            batch.remaining.insert(slot_id);
        }
        let deadline = Deadline::after(self.config.request_timeout)?;
        let slot_ids = self
            .connect_batch
            .as_ref()
            .map(|batch| batch.remaining.iter().copied().collect::<Vec<_>>())
            .ok_or_else(|| RuntimeError::internal("connect batch disappeared"))?;
        if dispatch_error.is_none() {
            for slot_id in slot_ids {
                if let Err(error) =
                    self.send_slot_work(slot_id, SlotWork::EnsureConnected { deadline })
                {
                    dispatch_error = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = dispatch_error {
            if let Some(batch) = self.connect_batch.as_mut() {
                batch.first_error = Some(error);
                batch.rolling_back = true;
            }
            self.begin_close().await?;
        }
        Ok(())
    }

    fn ensure_slot(&mut self, epoch: EngineEpoch, slot_id: SlotId) -> Result<(), RuntimeError> {
        let index = slot_id.get();
        let current = self
            .slots
            .get(index)
            .ok_or_else(|| RuntimeError::internal("Slot id is outside runtime inventory"))?;
        if current.is_some() {
            return Ok(());
        }
        if !self.supervisor.register_slot(epoch, slot_id)? {
            return Err(RuntimeError::connection_closed(
                "Slot registration lost the active engine epoch",
            ));
        }
        let rotation = EndpointRotation::new(self.config.endpoints.to_vec(), index)?;
        let slot = Slot::new(epoch, slot_id, rotation)?;
        let snapshot = Arc::new(Mutex::new(SlotSnapshot::capture(&slot, true)));
        let (work_tx, work_rx) = mpsc::channel(1);
        let (directive_tx, directive_rx) = watch::channel(SlotDirective::Run);
        let worker = SlotWorker {
            slot,
            stream: None,
            message_ids: MessageIdGenerator::new(),
            work_rx,
            directive_rx,
            event_tx: self.slot_event_tx.clone(),
            push_tx: self.push_tx.clone(),
            push_dropped: Arc::clone(&self.push_dropped),
            heartbeat_interval: self.config.heartbeat_interval,
            snapshot: Arc::clone(&snapshot),
        };
        let worker_join = tokio::spawn(worker.run());
        let event_tx = self.slot_event_tx.clone();
        let join = tokio::spawn(async move {
            let result = match worker_join.await {
                Ok(result) => result,
                Err(error) => Err(RuntimeError::internal(format!(
                    "Slot worker task did not exit normally: {error}"
                ))),
            };
            let _ = event_tx.send(SlotEvent::Stopped { slot_id, result }).await;
        });
        self.slots[index] = Some(SlotHandle {
            work_tx,
            directive_tx,
            join,
            snapshot,
        });
        Ok(())
    }

    fn send_slot_work(&self, slot_id: SlotId, work: SlotWork) -> Result<(), RuntimeError> {
        let handle = self
            .slots
            .get(slot_id.get())
            .and_then(Option::as_ref)
            .ok_or_else(|| RuntimeError::internal("dispatch target Slot is not instantiated"))?;
        handle.work_tx.try_send(work).map_err(|error| {
            RuntimeError::internal(format!("Slot work channel rejected dispatch: {error}"))
                .with_context("slot_id", slot_id.get().to_string())
        })
    }

    async fn dispatch_initial(&mut self, request_id: RequestId) -> Result<(), RuntimeError> {
        let attempt = self
            .supervisor
            .begin_request_attempt(request_id, Instant::now())?;
        self.dispatch_attempt(attempt, true, true).await
    }

    fn next_internal_request_id(&mut self) -> Result<RequestId, RuntimeError> {
        let next = if self.internal_request_counter == 0 {
            2
        } else {
            self.internal_request_counter
                .checked_add(2)
                .ok_or_else(|| {
                    RuntimeError::internal("internal request identity space exhausted")
                })?
        };
        let request_id = RequestId::new(next)?;
        self.internal_request_counter = next;
        Ok(request_id)
    }

    async fn claim_heartbeat(
        &mut self,
        candidate: HeartbeatCandidate,
        reply: oneshot::Sender<Result<Option<HeartbeatWork>, RuntimeError>>,
    ) -> Result<(), RuntimeError> {
        let request_id = self.next_internal_request_id()?;
        let Some(claim) = self.supervisor.claim_heartbeat(
            request_id,
            candidate,
            self.config.request_timeout,
            Instant::now(),
        )?
        else {
            let _ = reply.send(Ok(None));
            return Ok(());
        };
        let attempt = match self
            .supervisor
            .begin_request_attempt(request_id, Instant::now())
        {
            Ok(attempt) => attempt,
            Err(error) => {
                let withdrawal = RuntimeError::internal(
                    "heartbeat claim could not begin its single wire attempt",
                )
                .with_context("request_id", request_id.get().to_string());
                if let Some(batch) =
                    self.supervisor
                        .withdraw_heartbeat(claim, withdrawal, Instant::now())?
                {
                    self.process_terminal_batch(batch).await?;
                }
                let _ = reply.send(Err(error));
                return Ok(());
            }
        };
        if !self.heartbeat_requests.insert(request_id) {
            return Err(RuntimeError::internal(
                "runtime heartbeat request identity was reused",
            ));
        }
        let work = HeartbeatWork { attempt, candidate };
        if reply.send(Ok(Some(work))).is_err() {
            let error =
                RuntimeError::connection_closed("Slot disappeared before accepting heartbeat work")
                    .with_context("request_id", request_id.get().to_string());
            let batch = self
                .supervisor
                .withdraw_heartbeat(claim, error, Instant::now())?
                .ok_or_else(|| RuntimeError::internal("orphaned heartbeat withdrawal was stale"))?;
            self.process_terminal_batch(batch).await?;
        }
        Ok(())
    }

    async fn dispatch_attempt(
        &mut self,
        attempt: RequestAttempt,
        begin_endpoint_attempt: bool,
        publish_connect_transition: bool,
    ) -> Result<(), RuntimeError> {
        let epoch = attempt.engine_epoch;
        self.ensure_slot(epoch, attempt.slot_id)?;
        let request = self
            .pending
            .get(&attempt.request_id)
            .map(|pending| pending.request.clone())
            .ok_or_else(|| RuntimeError::internal("dispatched request is not pending"))?;
        self.send_slot_work(
            attempt.slot_id,
            SlotWork::Execute {
                attempt,
                request,
                begin_endpoint_attempt,
                publish_connect_transition,
            },
        )?;
        let pending = self
            .pending
            .get_mut(&attempt.request_id)
            .ok_or_else(|| RuntimeError::internal("dispatched request disappeared"))?;
        pending.dispatched = true;
        Ok(())
    }

    async fn handle_slot_event(&mut self, event: SlotEvent) -> Result<(), RuntimeError> {
        match event {
            SlotEvent::Transition {
                request_id,
                state,
                wire,
                reply,
            } => {
                let result = self
                    .supervisor
                    .transition_request(request_id, state, Some(wire));
                if result.is_ok() {
                    if let Some(pending) = self.pending.get_mut(&request_id) {
                        pending.wire = Some(wire);
                    }
                }
                let _ = reply.send(result.clone());
                if self.closing && result.is_err() {
                    Ok(())
                } else {
                    result
                }
            }
            SlotEvent::BusinessBytes { wire, reply } => {
                let accepted = self.supervisor.mark_business_bytes_sent(wire);
                let result = if accepted {
                    Ok(())
                } else {
                    Err(RuntimeError::internal(
                        "Supervisor rejected exact business send boundary",
                    ))
                };
                let _ = reply.send(result.clone());
                if self.closing && result.is_err() {
                    Ok(())
                } else {
                    result
                }
            }
            SlotEvent::HeartbeatDue { candidate, reply } => {
                self.claim_heartbeat(candidate, reply).await
            }
            SlotEvent::HandshakeObserved { slot_id, handshake } => {
                lock_mutex(&self.sessions, "runtime session snapshots")?
                    .handshakes
                    .insert(slot_id, handshake);
                Ok(())
            }
            SlotEvent::Completed {
                wire,
                response,
                reply,
            } => {
                let request_id = wire.request_id;
                if let Some(pending) = self.pending.get(&request_id) {
                    match &pending.request {
                        CommandRequest::Handshake(_) => {
                            if let Ok(handshake) = parse_handshake_payload(&response.data) {
                                lock_mutex(&self.sessions, "runtime session snapshots")?
                                    .handshakes
                                    .insert(wire.slot_id, handshake);
                            }
                        }
                        CommandRequest::Heartbeat(_) => {
                            if let Ok(heartbeat) = parse_heartbeat_payload(&response.data) {
                                lock_mutex(&self.sessions, "runtime session snapshots")?
                                    .heartbeats
                                    .insert(wire.slot_id, heartbeat);
                            }
                        }
                        _ => {}
                    }
                }
                let pin = if let Some(pending) = self.pending.get_mut(&request_id) {
                    pending.response = Some(response);
                    pending.pin
                } else {
                    None
                };
                let result = if let Some(pin) = pin {
                    match self.supervisor.terminal_pin(
                        wire,
                        TerminalKind::Completed,
                        None,
                        Instant::now(),
                    ) {
                        Ok(Some(batch)) => self.process_pin_terminal_batch(pin, batch).await,
                        Ok(None) if self.has_lifecycle_retirement(request_id) => {
                            Err(RuntimeError::connection_closed(
                                "Engine close won the pinned response terminal race",
                            ))
                        }
                        Ok(None) => Err(RuntimeError::internal(
                            "completed pinned request terminal was stale",
                        )),
                        Err(error) => Err(error),
                    }
                } else {
                    match self.supervisor.terminal_active(
                        wire,
                        TerminalKind::Completed,
                        None,
                        Instant::now(),
                    ) {
                        Ok(Some(batch)) => self.process_terminal_batch(batch).await,
                        Ok(None) if self.has_lifecycle_retirement(request_id) => {
                            Err(RuntimeError::connection_closed(
                                "Engine close won the response terminal race",
                            ))
                        }
                        Ok(None) => Err(RuntimeError::internal(
                            "completed request terminal was stale",
                        )),
                        Err(error) => Err(error),
                    }
                };
                let close_race = result.is_err() && self.closing;
                let _ = reply.send(result.clone());
                if close_race {
                    Ok(())
                } else {
                    result
                }
            }
            SlotEvent::HeartbeatCompleted {
                wire,
                acknowledgement,
                reply,
            } => {
                let cached_acknowledgement = acknowledgement.clone();
                if !self.heartbeat_requests.contains(&wire.request_id) {
                    let error =
                        RuntimeError::internal("heartbeat completion lacks runtime ownership");
                    let _ = reply.send(Err(error.clone()));
                    if self.closing {
                        return Ok(());
                    }
                    return Err(error);
                }
                let result = match self.supervisor.terminal_heartbeat(
                    wire,
                    TerminalKind::Completed,
                    Some(acknowledgement),
                    None,
                    Instant::now(),
                ) {
                    Ok(Some(batch)) => self.process_terminal_batch(batch).await,
                    Ok(None) if self.has_lifecycle_retirement(wire.request_id) => {
                        Err(RuntimeError::connection_closed(
                            "Engine close won the heartbeat terminal race",
                        ))
                    }
                    Ok(None) => Err(RuntimeError::internal("completed heartbeat was stale")),
                    Err(error) => Err(error),
                };
                let close_race = result.is_err() && self.closing;
                if result.is_ok() {
                    lock_mutex(&self.sessions, "runtime session snapshots")?
                        .heartbeats
                        .insert(wire.slot_id, cached_acknowledgement);
                }
                let _ = reply.send(result.clone());
                if close_race {
                    Ok(())
                } else {
                    result
                }
            }
            SlotEvent::UnstartedCancelled { request_id } => {
                let Some(pending) = self.pending.get(&request_id) else {
                    return Ok(());
                };
                if pending.wire.is_some() {
                    return Err(RuntimeError::internal(
                        "unstarted Slot cancellation already owns a wire identity",
                    ));
                }
                let error = RuntimeError::connection_closed("request cancelled by caller")
                    .with_context("request_id", request_id.get().to_string());
                match pending.admission {
                    Admission::Active(lease) => {
                        let batch = self
                            .supervisor
                            .terminal_active(
                                wire_without_generation(lease),
                                TerminalKind::Cancelled,
                                Some(error),
                                Instant::now(),
                            )?
                            .ok_or_else(|| {
                                RuntimeError::internal("unstarted cancellation was stale")
                            })?;
                        self.process_terminal_batch(batch).await
                    }
                    Admission::Pinned(call) => {
                        let pin = call.pin;
                        let batch = self
                            .supervisor
                            .terminal_pin(
                                RequestWireIdentity {
                                    engine_epoch: pin.engine_epoch,
                                    request_id,
                                    lease_id: pin.lease_id,
                                    slot_id: pin.slot_id,
                                    generation: None,
                                    message: None,
                                },
                                TerminalKind::Cancelled,
                                Some(error),
                                Instant::now(),
                            )?
                            .ok_or_else(|| {
                                RuntimeError::internal("unstarted pinned cancellation was stale")
                            })?;
                        self.process_pin_terminal_batch(pin, batch).await
                    }
                    Admission::Waiting(_) => Err(RuntimeError::internal(
                        "unstarted Slot cancellation lacks assigned admission",
                    )),
                }
            }
            SlotEvent::Failed {
                wire,
                error,
                retryable,
                same_attempt_possible,
                reply,
            } => {
                let pin = self
                    .pending
                    .get(&wire.request_id)
                    .and_then(|pending| pending.pin);
                if self.closing {
                    if self.has_lifecycle_retirement(wire.request_id) {
                        self.retirements
                            .insert(wire.request_id, RetirementPlan::Lifecycle);
                    }
                    let _ = reply.send(Ok(FailureAction::Retire));
                    return Ok(());
                }
                let action = if self.heartbeat_requests.contains(&wire.request_id) {
                    if same_attempt_possible {
                        Err(RuntimeError::internal(
                            "existing-generation heartbeat cannot continue on another endpoint",
                        ))
                    } else {
                        match self.supervisor.begin_retry(
                            wire,
                            error.clone(),
                            retryable,
                            Instant::now(),
                        ) {
                            Some(RetryDecision::RetireThenTerminal(terminal)) => {
                                self.retirements.insert(
                                    wire.request_id,
                                    RetirementPlan::HeartbeatTerminal {
                                        kind: TerminalKind::Failed,
                                        error: terminal.error,
                                    },
                                );
                                Ok(FailureAction::Retire)
                            }
                            Some(RetryDecision::Retire(_)) => Err(RuntimeError::internal(
                                "one-attempt heartbeat unexpectedly selected a retry",
                            )),
                            None => Err(RuntimeError::internal(
                                "Supervisor rejected exact heartbeat failure",
                            )),
                        }
                    }
                } else if same_attempt_possible {
                    self.retirements
                        .insert(wire.request_id, RetirementPlan::ContinueEndpoint);
                    Ok(FailureAction::Retire)
                } else {
                    match self.supervisor.begin_retry(
                        wire,
                        error.clone(),
                        retryable,
                        Instant::now(),
                    ) {
                        Some(RetryDecision::Retire(_)) => {
                            self.retirements
                                .insert(wire.request_id, RetirementPlan::Retry);
                            Ok(FailureAction::Retire)
                        }
                        Some(RetryDecision::RetireThenTerminal(terminal)) => {
                            let plan = if let Some(pin) = pin {
                                RetirementPlan::PinnedTerminal {
                                    pin,
                                    kind: TerminalKind::Failed,
                                    error: terminal.error,
                                }
                            } else {
                                RetirementPlan::Terminal {
                                    kind: TerminalKind::Failed,
                                    error: terminal.error,
                                }
                            };
                            self.retirements.insert(wire.request_id, plan);
                            Ok(FailureAction::Retire)
                        }
                        None => Err(RuntimeError::internal(
                            "Supervisor rejected exact request failure",
                        )),
                    }
                };
                let _ = reply.send(action.clone());
                action.map(|_| ())
            }
            SlotEvent::Retired { acknowledgement } => self.handle_retired(acknowledgement).await,
            SlotEvent::ConnectFinished { slot_id, result } => {
                self.handle_connect_finished(slot_id, result).await
            }
            SlotEvent::Stopped { slot_id, result } => {
                let handle = self
                    .slots
                    .get_mut(slot_id.get())
                    .and_then(Option::take)
                    .ok_or_else(|| RuntimeError::internal("stopped Slot handle is absent"))?;
                let monitor_result = handle.join.await.map_err(|error| {
                    RuntimeError::internal(format!(
                        "Slot worker monitor did not exit normally: {error}"
                    ))
                });
                let worker_error = result.err().or_else(|| monitor_result.err());
                if let Some(error) = worker_error {
                    self.begin_runtime_fatal(error).await?;
                }
                let epoch = self.supervisor.active_epoch().or_else(|| {
                    self.supervisor
                        .close_attempt()
                        .and_then(|attempt| attempt.target_epoch())
                });
                if let Some(epoch) = epoch {
                    self.finish_joined_slot_retirements(epoch, slot_id).await?;
                    self.supervisor.retire_slot(epoch, slot_id);
                }
                Ok(())
            }
        }
    }

    async fn handle_connect_finished(
        &mut self,
        slot_id: SlotId,
        result: Result<(), RuntimeError>,
    ) -> Result<(), RuntimeError> {
        let Some(batch) = self.connect_batch.as_mut() else {
            return Ok(());
        };
        if !batch.remaining.remove(&slot_id) {
            return Err(RuntimeError::internal(
                "connect completion did not belong to the current batch",
            ));
        }
        let failed = if let Err(error) = result {
            batch.first_error.get_or_insert(error);
            true
        } else {
            false
        };
        let start_rollback = failed && !batch.rolling_back;
        if start_rollback {
            batch.rolling_back = true;
        }
        if start_rollback {
            self.begin_close().await?;
            return Ok(());
        }
        let Some(batch) = self.connect_batch.as_ref() else {
            return Ok(());
        };
        if !batch.remaining.is_empty() || batch.rolling_back {
            return Ok(());
        }
        let completed = self
            .connect_batch
            .take()
            .ok_or_else(|| RuntimeError::internal("connect batch disappeared"))?;
        completed.completion.publish(Ok(()));
        Ok(())
    }

    async fn handle_retired(&mut self, acknowledgement: ReconnectAck) -> Result<(), RuntimeError> {
        let request_id = acknowledgement.request_id;
        let plan = self.retirements.remove(&request_id).or_else(|| {
            self.supervisor
                .lifecycle_retirements()
                .iter()
                .any(|action| action.wire.request_id == request_id)
                .then_some(RetirementPlan::Lifecycle)
        });
        let Some(plan) = plan else {
            if !self.pending.contains_key(&request_id) {
                return Ok(());
            }
            return Err(RuntimeError::internal(
                "retired generation has no Supervisor retirement owner",
            ));
        };
        match plan {
            RetirementPlan::ContinueEndpoint => {
                let attempt = self
                    .supervisor
                    .continue_connect_after_reconnect(acknowledgement, Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("same-attempt reconnect was rejected"))?;
                self.dispatch_attempt(attempt, false, false).await
            }
            RetirementPlan::Retry => {
                let attempt = self
                    .supervisor
                    .finish_retry_retirement(acknowledgement, Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("retry retirement was rejected"))?;
                self.dispatch_attempt(attempt, true, true).await
            }
            RetirementPlan::Terminal { kind, error } => {
                let wire = self
                    .supervisor
                    .finish_terminal_retirement(acknowledgement)
                    .ok_or_else(|| RuntimeError::internal("terminal retirement was rejected"))?;
                let batch = self
                    .supervisor
                    .terminal_active(wire, kind, Some(error), Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("retired terminal was stale"))?;
                self.process_terminal_batch(batch).await
            }
            RetirementPlan::PinnedTerminal { pin, kind, error } => {
                let wire = self
                    .supervisor
                    .finish_terminal_retirement(acknowledgement)
                    .ok_or_else(|| {
                        RuntimeError::internal("pinned terminal retirement was rejected")
                    })?;
                let batch = self
                    .supervisor
                    .terminal_pin(wire, kind, Some(error), Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("retired pinned terminal was stale"))?;
                self.process_pin_terminal_batch(pin, batch).await
            }
            RetirementPlan::HeartbeatTerminal { kind, error } => {
                let wire = self
                    .supervisor
                    .finish_terminal_retirement(acknowledgement)
                    .ok_or_else(|| {
                        RuntimeError::internal("heartbeat terminal retirement was rejected")
                    })?;
                let batch = self
                    .supervisor
                    .terminal_heartbeat(wire, kind, None, Some(error), Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("retired heartbeat was stale"))?;
                self.process_terminal_batch(batch).await
            }
            RetirementPlan::Lifecycle => {
                let generation = GenerationIdentity {
                    engine_epoch: acknowledgement.engine_epoch,
                    slot_id: acknowledgement.slot_id,
                    generation: acknowledgement.retired_generation,
                };
                let retired = crate::slot::RetiredGeneration {
                    request_id: Some(request_id),
                    next_generation: acknowledgement.next_generation,
                };
                if !self.supervisor.finish_lifecycle_retirement(
                    generation,
                    retired,
                    Instant::now(),
                )? {
                    return Err(RuntimeError::internal(
                        "lifecycle generation retirement was rejected",
                    ));
                }
                let notifications = self.supervisor.take_lifecycle_notifications();
                self.process_notifications(notifications).await
            }
        }
    }

    async fn process_terminal_batch(&mut self, batch: TerminalBatch) -> Result<(), RuntimeError> {
        let promotion = batch.promotion;
        self.process_notifications(batch.notifications).await?;
        if let Some(promotion) = promotion {
            self.apply_promotion(promotion).await?;
        }
        Ok(())
    }

    async fn process_pin_terminal_batch(
        &mut self,
        pin: PinIdentity,
        batch: PinTerminalBatch,
    ) -> Result<(), RuntimeError> {
        let pin_promotion = batch.pin_promotion;
        let ordinary_promotion = batch.ordinary_promotion;
        let pin_released = batch.pin_released;
        self.process_notifications(batch.notifications).await?;
        if let Some(promotion) = pin_promotion {
            self.apply_pin_promotion(promotion).await?;
        }
        if let Some(promotion) = ordinary_promotion {
            self.apply_promotion(promotion).await?;
        }
        if pin_released {
            if let Some(waiters) = self.pin_close_waiters.remove(&pin.pin_id.get()) {
                for waiter in waiters {
                    let _ = waiter.send(Ok(()));
                }
            }
        }
        Ok(())
    }

    async fn finish_joined_slot_retirements(
        &mut self,
        epoch: EngineEpoch,
        slot_id: SlotId,
    ) -> Result<(), RuntimeError> {
        let actions = self
            .supervisor
            .lifecycle_retirements()
            .into_iter()
            .filter(|action| action.wire.engine_epoch == epoch && action.wire.slot_id == slot_id)
            .collect::<Vec<_>>();
        for action in actions {
            let generation = action.wire.generation.ok_or_else(|| {
                RuntimeError::internal("joined Slot lifecycle owner has no generation")
            })?;
            let next_generation = generation
                .get()
                .checked_add(1)
                .ok_or_else(|| RuntimeError::internal("TCP generation space exhausted"))?;
            let proof = crate::slot::RetiredGeneration {
                request_id: Some(action.wire.request_id),
                next_generation: crate::slot::GenerationId::new(next_generation)?,
            };
            let identity = GenerationIdentity {
                engine_epoch: epoch,
                slot_id,
                generation,
            };
            if !self
                .supervisor
                .finish_lifecycle_retirement(identity, proof, Instant::now())?
            {
                return Err(RuntimeError::internal(
                    "joined Slot retirement proof was rejected",
                ));
            }
            self.retirements.remove(&action.wire.request_id);
        }
        let notifications = self.supervisor.take_lifecycle_notifications();
        self.process_notifications(notifications).await
    }

    fn has_lifecycle_retirement(&self, request_id: RequestId) -> bool {
        self.supervisor
            .lifecycle_retirements()
            .iter()
            .any(|action| action.wire.request_id == request_id)
    }

    async fn process_notifications(
        &mut self,
        notifications: Vec<TerminalNotification>,
    ) -> Result<(), RuntimeError> {
        for notification in notifications {
            let request_id = notification.request_id;
            self.heartbeat_requests.remove(&request_id);
            if let Some(reservation) = self.pin_reservations.remove(&request_id) {
                let error = notification.error.unwrap_or_else(|| {
                    RuntimeError::connection_closed("pin reservation was cancelled")
                });
                let _ = reservation.result.send(Err(error));
                if let Some(confirmation) = self.cancel_confirmations.remove(&request_id) {
                    confirmation.publish(Ok(()));
                }
                continue;
            }
            let pending = self.pending.remove(&request_id);
            if let Some(pending) = pending {
                let terminal = match notification.kind {
                    TerminalKind::Completed => {
                        let response = pending.response.ok_or_else(|| {
                            RuntimeError::internal("completed terminal has no raw response")
                        })?;
                        Ok(RawExecution {
                            request: pending.request,
                            response,
                        })
                    }
                    TerminalKind::Cancelled | TerminalKind::TimedOut | TerminalKind::Failed => {
                        Err(notification.error.unwrap_or_else(|| {
                            RuntimeError::internal("failed terminal has no error")
                        }))
                    }
                };
                let _ = pending.result.send(terminal);
            }
            if let Some(confirmation) = self.cancel_confirmations.remove(&request_id) {
                confirmation.publish(Ok(()));
            }
        }
        Ok(())
    }

    async fn apply_promotion(&mut self, promotion: Promotion) -> Result<(), RuntimeError> {
        let request_id = promotion.active_lease.request_id;
        if let Some(reservation) = self.pin_reservations.get_mut(&request_id) {
            if reservation.admission != Admission::Waiting(promotion.returned_permit) {
                return Err(RuntimeError::internal(
                    "pin reservation promotion does not match waiting ownership",
                ));
            }
            reservation.admission = Admission::Active(promotion.active_lease);
            return self.complete_pin_reservation(request_id).await;
        }
        let pending = self
            .pending
            .get_mut(&request_id)
            .ok_or_else(|| RuntimeError::internal("promoted request is not pending"))?;
        if pending.admission != Admission::Waiting(promotion.returned_permit) {
            return Err(RuntimeError::internal(
                "promotion does not match runtime pending admission",
            ));
        }
        pending.admission = Admission::Active(promotion.active_lease);
        self.dispatch_initial(request_id).await
    }

    async fn apply_pin_promotion(
        &mut self,
        promotion: crate::pin::PinnedCallLease,
    ) -> Result<(), RuntimeError> {
        let pending = self
            .pending
            .get_mut(&promotion.request_id)
            .ok_or_else(|| RuntimeError::internal("promoted pinned request is not pending"))?;
        let Admission::Waiting(permit) = pending.admission else {
            return Err(RuntimeError::internal(
                "pinned promotion does not match waiting admission",
            ));
        };
        if permit.request_id != promotion.request_id || pending.pin != Some(promotion.pin) {
            return Err(RuntimeError::internal(
                "pinned promotion identity does not match pending request",
            ));
        }
        pending.admission = Admission::Pinned(promotion);
        self.dispatch_initial(promotion.request_id).await
    }

    async fn complete_pin_reservation(
        &mut self,
        request_id: RequestId,
    ) -> Result<(), RuntimeError> {
        let identity = match self.supervisor.open_pin(request_id) {
            Ok(identity) => identity,
            Err(error) => {
                let admission = self
                    .pin_reservations
                    .get(&request_id)
                    .map(|reservation| reservation.admission)
                    .ok_or_else(|| RuntimeError::internal("pin reservation disappeared"))?;
                let Admission::Active(lease) = admission else {
                    return Err(RuntimeError::internal(
                        "failed pin reservation is not actively assigned",
                    ));
                };
                let batch = self
                    .supervisor
                    .terminal_active(
                        wire_without_generation(lease),
                        TerminalKind::Failed,
                        Some(error),
                        Instant::now(),
                    )?
                    .ok_or_else(|| RuntimeError::internal("failed pin reservation was stale"))?;
                return self.process_terminal_batch(batch).await;
            }
        };
        let reservation = self
            .pin_reservations
            .remove(&request_id)
            .ok_or_else(|| RuntimeError::internal("opened pin reservation disappeared"))?;
        let _ = reservation.result.send(Ok(identity));
        Ok(())
    }

    async fn close_pin(
        &mut self,
        pin: PinIdentity,
        reply: std_mpsc::SyncSender<Result<(), RuntimeError>>,
    ) -> Result<(), RuntimeError> {
        let pin_key = pin.pin_id.get();
        let waiter_count = self.pin_close_waiter_count()?;
        if let Some(waiters) = self.pin_close_waiters.get_mut(&pin_key) {
            if waiter_count >= self.config.max_pending_requests {
                let _ = reply.send(Err(RuntimeError::PoolBusy {
                    message: "7709 pin close waiter queue is full".to_owned(),
                    capacity: self.config.max_pending_requests,
                    context: Vec::new(),
                }));
            } else {
                waiters.push(reply);
            }
            return Ok(());
        }
        let Some(batch) = self.supervisor.close_pin(pin, Instant::now())? else {
            let _ = reply.send(Ok(()));
            return Ok(());
        };
        if batch.pin_released {
            self.process_pin_terminal_batch(pin, batch).await?;
            let _ = reply.send(Ok(()));
            return Ok(());
        }
        if waiter_count >= self.config.max_pending_requests {
            let _ = reply.send(Err(RuntimeError::PoolBusy {
                message: "7709 pin close waiter queue is full".to_owned(),
                capacity: self.config.max_pending_requests,
                context: Vec::new(),
            }));
        } else {
            self.pin_close_waiters.insert(pin_key, vec![reply]);
        }
        self.process_pin_terminal_batch(pin, batch).await?;
        if let Some(request_id) = self
            .pending
            .iter()
            .find_map(|(request_id, pending)| (pending.pin == Some(pin)).then_some(*request_id))
        {
            let slot_id = pin.slot_id;
            self.send_slot_directive(slot_id, SlotDirective::Cancel(request_id))?;
        }
        Ok(())
    }

    fn pin_close_waiter_count(&self) -> Result<usize, RuntimeError> {
        self.pin_close_waiters
            .values()
            .try_fold(0_usize, |total, waiters| {
                total
                    .checked_add(waiters.len())
                    .ok_or_else(|| RuntimeError::internal("pin close waiter count overflow"))
            })
    }

    async fn handle_control(&mut self) -> Result<(), RuntimeError> {
        let snapshot = self.control.take_snapshot()?;
        if snapshot.close_timed_out {
            if let Some(attempt) = self.close_attempt.take() {
                let error = RuntimeError::CloseTimeout {
                    message: "7709 Engine close exceeded the public 1.0 second hard gate"
                        .to_owned(),
                    context: Vec::new(),
                };
                self.supervisor.finish_close(attempt, Err(error));
                match self.supervisor.begin_close()? {
                    CloseClaim::Owner(retry) | CloseClaim::Existing(retry) => {
                        self.close_attempt = Some(retry);
                    }
                    CloseClaim::AlreadyStopped | CloseClaim::AlreadyFailedClosed => {}
                }
            }
        }
        for (request_id, confirmation) in snapshot.cancellations {
            self.cancel_request(request_id, confirmation).await?;
        }
        if snapshot.close_requested && !self.closing {
            self.begin_close().await?;
        }
        Ok(())
    }

    async fn cancel_request(
        &mut self,
        request_id: RequestId,
        confirmation: Arc<Completion<Result<(), RuntimeError>>>,
    ) -> Result<(), RuntimeError> {
        if let Some(reservation) = self.pin_reservations.get(&request_id) {
            let admission = reservation.admission;
            self.cancel_confirmations.insert(request_id, confirmation);
            let error = RuntimeError::connection_closed("pin reservation cancelled by caller")
                .with_context("request_id", request_id.get().to_string());
            return match admission {
                Admission::Waiting(permit) => {
                    let notification = self
                        .supervisor
                        .terminal_waiting(permit, TerminalKind::Cancelled, Some(error))?
                        .ok_or_else(|| {
                            RuntimeError::internal("queued pin reservation cancellation was stale")
                        })?;
                    self.process_notifications(vec![notification]).await
                }
                Admission::Active(lease) => {
                    let batch = self
                        .supervisor
                        .terminal_active(
                            wire_without_generation(lease),
                            TerminalKind::Cancelled,
                            Some(error),
                            Instant::now(),
                        )?
                        .ok_or_else(|| {
                            RuntimeError::internal("active pin reservation cancellation was stale")
                        })?;
                    self.process_terminal_batch(batch).await
                }
                Admission::Pinned(_) => Err(RuntimeError::internal(
                    "pin reservation unexpectedly became a pinned call",
                )),
            };
        }
        let Some(pending) = self.pending.get(&request_id) else {
            confirmation.publish(Ok(()));
            return Ok(());
        };
        self.cancel_confirmations.insert(request_id, confirmation);
        let error = RuntimeError::connection_closed("request cancelled by caller")
            .with_context("request_id", request_id.get().to_string());
        match (pending.admission, pending.wire, pending.dispatched) {
            (Admission::Waiting(permit), None, _) => {
                let notification = if let Some(pin) = pending.pin {
                    self.supervisor
                        .terminal_pin_waiting(pin, permit, TerminalKind::Cancelled, error)?
                        .ok_or_else(|| {
                            RuntimeError::internal("queued pinned cancellation was stale")
                        })?
                } else {
                    self.supervisor
                        .terminal_waiting(permit, TerminalKind::Cancelled, Some(error))?
                        .ok_or_else(|| RuntimeError::internal("queued cancellation was stale"))?
                };
                self.process_notifications(vec![notification]).await
            }
            (Admission::Active(lease), None, false) => {
                let wire = wire_without_generation(lease);
                let batch = self
                    .supervisor
                    .terminal_active(wire, TerminalKind::Cancelled, Some(error), Instant::now())?
                    .ok_or_else(|| RuntimeError::internal("unstarted cancellation was stale"))?;
                self.process_terminal_batch(batch).await
            }
            (Admission::Active(lease), None, true) => {
                self.send_slot_directive(lease.slot_id, SlotDirective::Cancel(request_id))
            }
            (Admission::Active(_), Some(wire), _) => {
                let decision = self
                    .supervisor
                    .begin_retry(wire, error.clone(), false, Instant::now())
                    .ok_or_else(|| RuntimeError::internal("started cancellation was stale"))?;
                if !matches!(decision, RetryDecision::RetireThenTerminal(_)) {
                    return Err(RuntimeError::internal(
                        "caller cancellation unexpectedly selected retry",
                    ));
                }
                self.retirements.insert(
                    request_id,
                    RetirementPlan::Terminal {
                        kind: TerminalKind::Cancelled,
                        error,
                    },
                );
                self.send_slot_directive(wire.slot_id, SlotDirective::Cancel(request_id))
            }
            (Admission::Pinned(call), None, false) => {
                let pin = call.pin;
                let batch = self
                    .supervisor
                    .terminal_pin(
                        RequestWireIdentity {
                            engine_epoch: pin.engine_epoch,
                            request_id,
                            lease_id: pin.lease_id,
                            slot_id: pin.slot_id,
                            generation: None,
                            message: None,
                        },
                        TerminalKind::Cancelled,
                        Some(error),
                        Instant::now(),
                    )?
                    .ok_or_else(|| {
                        RuntimeError::internal("unstarted pinned cancellation was stale")
                    })?;
                self.process_pin_terminal_batch(pin, batch).await
            }
            (Admission::Pinned(call), None, true) => {
                self.send_slot_directive(call.pin.slot_id, SlotDirective::Cancel(request_id))
            }
            (Admission::Pinned(call), Some(wire), _) => {
                let decision = self
                    .supervisor
                    .begin_retry(wire, error.clone(), false, Instant::now())
                    .ok_or_else(|| {
                        RuntimeError::internal("started pinned cancellation was stale")
                    })?;
                if !matches!(decision, RetryDecision::RetireThenTerminal(_)) {
                    return Err(RuntimeError::internal(
                        "pinned cancellation unexpectedly selected retry",
                    ));
                }
                self.retirements.insert(
                    request_id,
                    RetirementPlan::PinnedTerminal {
                        pin: call.pin,
                        kind: TerminalKind::Cancelled,
                        error,
                    },
                );
                self.send_slot_directive(wire.slot_id, SlotDirective::Cancel(request_id))
            }
            (Admission::Waiting(_), Some(_), _) => Err(RuntimeError::internal(
                "waiting request unexpectedly owns wire identity",
            )),
        }
    }

    async fn begin_close(&mut self) -> Result<(), RuntimeError> {
        self.closing = true;
        match self.supervisor.begin_close()? {
            CloseClaim::Owner(attempt) | CloseClaim::Existing(attempt) => {
                self.close_attempt = Some(attempt);
            }
            CloseClaim::AlreadyStopped | CloseClaim::AlreadyFailedClosed => return Ok(()),
        }
        self.synchronize_lifecycle_cleanup().await?;
        let pin_close_error = RuntimeError::connection_closed("Engine closed pinned proxy");
        for (_, waiters) in std::mem::take(&mut self.pin_close_waiters) {
            for waiter in waiters {
                let _ = waiter.send(Err(pin_close_error.clone()));
            }
        }
        for waiter in self.push_waiters.drain(..) {
            let _ = waiter.reply.send(self.supervisor.poll_push());
        }
        if let Some(batch) = self.connect_batch.as_mut() {
            if batch.first_error.is_none() {
                batch.first_error = Some(RuntimeError::connection_closed(
                    "Engine closed during explicit connect",
                ));
            }
            batch.rolling_back = true;
        }
        Ok(())
    }

    async fn synchronize_lifecycle_cleanup(&mut self) -> Result<(), RuntimeError> {
        let notifications = self.supervisor.take_lifecycle_notifications();
        self.process_notifications(notifications).await?;
        for action in self.supervisor.lifecycle_retirements() {
            self.retirements
                .insert(action.wire.request_id, RetirementPlan::Lifecycle);
        }
        let slot_ids = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| SlotId::new(index)))
            .collect::<Vec<_>>();
        for slot_id in slot_ids {
            let _ = self.send_slot_directive(slot_id, SlotDirective::Stop);
        }
        Ok(())
    }

    fn send_slot_directive(
        &self,
        slot_id: SlotId,
        directive: SlotDirective,
    ) -> Result<(), RuntimeError> {
        let handle = self
            .slots
            .get(slot_id.get())
            .and_then(Option::as_ref)
            .ok_or_else(|| RuntimeError::internal("directive target Slot is absent"))?;
        handle.directive_tx.send(directive).map_err(|_| {
            RuntimeError::internal("Slot directive receiver is closed")
                .with_context("slot_id", slot_id.get().to_string())
        })
    }

    async fn handle_tick(&mut self) -> Result<(), RuntimeError> {
        let dropped = self.push_dropped.swap(0, Ordering::AcqRel);
        if dropped != 0 {
            if let Some(epoch) = self.supervisor.active_epoch() {
                self.supervisor.record_push_drop(epoch, dropped);
            }
        }
        let notifications = self.supervisor.expire_waiting_terminals(Instant::now())?;
        self.process_notifications(notifications).await?;
        self.fulfill_push_waiters();
        Ok(())
    }

    fn fulfill_push_waiters(&mut self) {
        let now = Instant::now();
        let mut remaining = Vec::with_capacity(self.push_waiters.len());
        for waiter in self.push_waiters.drain(..) {
            match self.supervisor.poll_push() {
                Ok(Some(frame)) => {
                    let _ = waiter.reply.send(Ok(Some(frame)));
                }
                Ok(None) if waiter.deadline.is_elapsed_at(now) => {
                    let _ = waiter.reply.send(Ok(None));
                }
                Ok(None) => remaining.push(waiter),
                Err(error) => {
                    let _ = waiter.reply.send(Err(error));
                }
            }
        }
        self.push_waiters = remaining;
    }

    fn close_ready(&self) -> Result<bool, RuntimeError> {
        if self.slots.iter().any(Option::is_some)
            || !self.pending.is_empty()
            || !self.pin_reservations.is_empty()
            || !self.pin_close_waiters.is_empty()
            || !self.heartbeat_requests.is_empty()
            || !self.retirements.is_empty()
            || !self.cancel_confirmations.is_empty()
        {
            return Ok(false);
        }
        if self.ingress_owned.load(Ordering::Acquire) != 0 {
            return Ok(false);
        }
        self.supervisor.check_admission_invariants()?;
        Ok(true)
    }

    fn finish_close(&mut self) -> Result<(), RuntimeError> {
        let notifications = self.supervisor.take_lifecycle_notifications();
        if !notifications.is_empty() {
            return Err(RuntimeError::internal(
                "close readiness retained terminal notifications",
            ));
        }
        let attempt = self
            .close_attempt
            .take()
            .ok_or_else(|| RuntimeError::internal("close readiness has no CloseAttempt"))?;
        if !self.supervisor.finish_close(attempt, Ok(())) {
            return Err(RuntimeError::internal("CloseAttempt completion was stale"));
        }
        if !matches!(
            self.supervisor.state(),
            EngineState::Stopped | EngineState::FailedClosed
        ) {
            return Err(RuntimeError::internal(
                "runtime cleanup did not reach a closed Engine state",
            ));
        }
        self.refresh_diagnostics_cache()?;
        if let Some(batch) = self.connect_batch.take() {
            let error = batch.first_error.unwrap_or_else(|| {
                RuntimeError::connection_closed("Engine closed during explicit connect")
            });
            batch.completion.publish(Err(error));
        }
        Ok(())
    }
}

fn wire_without_generation(lease: ActiveLease) -> RequestWireIdentity {
    RequestWireIdentity {
        engine_epoch: lease.engine_epoch,
        request_id: lease.request_id,
        lease_id: lease.lease_id,
        slot_id: lease.slot_id,
        generation: None,
        message: None,
    }
}

#[derive(Debug)]
struct SlotHandle {
    work_tx: mpsc::Sender<SlotWork>,
    directive_tx: watch::Sender<SlotDirective>,
    join: tokio::task::JoinHandle<()>,
    snapshot: Arc<Mutex<SlotSnapshot>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotDirective {
    Run,
    Cancel(RequestId),
    Stop,
}

#[derive(Debug)]
enum SlotWork {
    EnsureConnected {
        deadline: Deadline,
    },
    Execute {
        attempt: RequestAttempt,
        request: CommandRequest,
        begin_endpoint_attempt: bool,
        publish_connect_transition: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FailureAction {
    Retire,
}

#[derive(Debug)]
enum SlotEvent {
    Transition {
        request_id: RequestId,
        state: RequestState,
        wire: RequestWireIdentity,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    BusinessBytes {
        wire: RequestWireIdentity,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    HeartbeatDue {
        candidate: HeartbeatCandidate,
        reply: oneshot::Sender<Result<Option<HeartbeatWork>, RuntimeError>>,
    },
    HandshakeObserved {
        slot_id: SlotId,
        handshake: HandshakeInfo,
    },
    Completed {
        wire: RequestWireIdentity,
        response: ResponseFrame,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    HeartbeatCompleted {
        wire: RequestWireIdentity,
        acknowledgement: HeartbeatAck,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    UnstartedCancelled {
        request_id: RequestId,
    },
    Failed {
        wire: RequestWireIdentity,
        error: RuntimeError,
        retryable: bool,
        same_attempt_possible: bool,
        reply: oneshot::Sender<Result<FailureAction, RuntimeError>>,
    },
    Retired {
        acknowledgement: ReconnectAck,
    },
    ConnectFinished {
        slot_id: SlotId,
        result: Result<(), RuntimeError>,
    },
    Stopped {
        slot_id: SlotId,
        result: Result<(), RuntimeError>,
    },
}

#[derive(Debug)]
struct SlotWorker {
    slot: Slot,
    stream: Option<TcpStream>,
    message_ids: MessageIdGenerator,
    work_rx: mpsc::Receiver<SlotWork>,
    directive_rx: watch::Receiver<SlotDirective>,
    event_tx: mpsc::Sender<SlotEvent>,
    push_tx: mpsc::Sender<PushFrame>,
    push_dropped: Arc<AtomicU64>,
    heartbeat_interval: Option<Duration>,
    snapshot: Arc<Mutex<SlotSnapshot>>,
}

#[derive(Debug)]
enum WorkOutcome {
    Completed {
        wire: RequestWireIdentity,
        response: ResponseFrame,
    },
    HeartbeatCompleted {
        wire: RequestWireIdentity,
        acknowledgement: HeartbeatAck,
    },
    UnstartedCancelled {
        request_id: RequestId,
    },
    Interrupted {
        wire: RequestWireIdentity,
    },
    Failed {
        wire: RequestWireIdentity,
        error: RuntimeError,
        retryable: bool,
        same_attempt_possible: bool,
    },
}

impl SlotWorker {
    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut result = self.run_inner().await;
        if let Err(error) = self.refresh_snapshot(false) {
            if result.is_ok() {
                result = Err(error);
            }
        }
        result
    }

    async fn run_inner(&mut self) -> Result<(), RuntimeError> {
        let mut idle_tick = interval(RUNTIME_TICK);
        idle_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            let current = *self.directive_rx.borrow_and_update();
            if current == SlotDirective::Stop {
                return self.stop_slot();
            }
            tokio::select! {
                biased;
                changed = self.directive_rx.changed() => {
                    if changed.is_err() {
                        return self.stop_slot();
                    }
                }
                work = self.work_rx.recv() => {
                    let Some(work) = work else {
                        return self.stop_slot();
                    };
                    match work {
                        SlotWork::EnsureConnected { deadline } => {
                            let result = self.ensure_connected(deadline).await;
                            self.refresh_snapshot(true)?;
                            self.event_tx.send(SlotEvent::ConnectFinished {
                                slot_id: self.slot.slot_id(),
                                result,
                            })
                            .await
                            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
                        }
                        SlotWork::Execute {
                            attempt,
                            request,
                            begin_endpoint_attempt,
                            publish_connect_transition,
                        } => {
                            let outcome = self.execute_work(
                                attempt,
                                request,
                                begin_endpoint_attempt,
                                publish_connect_transition,
                            ).await?;
                            self.refresh_snapshot(true)?;
                            self.publish_work_outcome(outcome).await?;
                        }
                    }
                }
                _ = idle_tick.tick(), if self.stream.is_some() => {
                    if self.read_idle_push_turn()? {
                        self.refresh_snapshot(true)?;
                        tokio::task::yield_now().await;
                    } else {
                        self.run_heartbeat_if_due().await?;
                        self.refresh_snapshot(true)?;
                    }
                }
            }
        }
    }

    async fn publish_handshake(&self, handshake: HandshakeInfo) -> Result<(), RuntimeError> {
        self.event_tx
            .send(SlotEvent::HandshakeObserved {
                slot_id: self.slot.slot_id(),
                handshake,
            })
            .await
            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))
    }

    fn refresh_snapshot(&self, actor_alive: bool) -> Result<(), RuntimeError> {
        *lock_mutex(&self.snapshot, "Slot diagnostics")? =
            SlotSnapshot::capture(&self.slot, actor_alive);
        Ok(())
    }

    async fn publish_work_outcome(&mut self, outcome: WorkOutcome) -> Result<(), RuntimeError> {
        match outcome {
            WorkOutcome::Completed { wire, response } => {
                let (reply, received) = oneshot::channel();
                self.event_tx
                    .send(SlotEvent::Completed {
                        wire,
                        response,
                        reply,
                    })
                    .await
                    .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
                match received.await.map_err(|_| {
                    RuntimeError::internal("request terminal acknowledgement sender closed")
                })? {
                    Ok(()) => {
                        if !self.slot.finish_business(wire.request_id)? {
                            return Err(RuntimeError::internal(
                                "Slot rejected completed business ownership release",
                            ));
                        }
                        self.refresh_snapshot(true)?;
                        Ok(())
                    }
                    Err(_) => self.retire_wire(wire).await,
                }
            }
            WorkOutcome::HeartbeatCompleted {
                wire,
                acknowledgement,
            } => {
                let (reply, received) = oneshot::channel();
                self.event_tx
                    .send(SlotEvent::HeartbeatCompleted {
                        wire,
                        acknowledgement,
                        reply,
                    })
                    .await
                    .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
                match received.await.map_err(|_| {
                    RuntimeError::internal("heartbeat terminal acknowledgement sender closed")
                })? {
                    Ok(()) => {
                        if !self.slot.finish_heartbeat(wire.request_id)? {
                            return Err(RuntimeError::internal(
                                "Slot rejected completed heartbeat ownership release",
                            ));
                        }
                        self.refresh_snapshot(true)?;
                        Ok(())
                    }
                    Err(_) => self.retire_wire(wire).await,
                }
            }
            WorkOutcome::UnstartedCancelled { request_id } => self
                .event_tx
                .send(SlotEvent::UnstartedCancelled { request_id })
                .await
                .map_err(|_| RuntimeError::internal("runtime event receiver closed")),
            WorkOutcome::Interrupted { wire } => self.retire_wire(wire).await,
            WorkOutcome::Failed {
                wire,
                error,
                retryable,
                same_attempt_possible,
            } => {
                let (reply, received) = oneshot::channel();
                self.event_tx
                    .send(SlotEvent::Failed {
                        wire,
                        error,
                        retryable,
                        same_attempt_possible,
                        reply,
                    })
                    .await
                    .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
                received
                    .await
                    .map_err(|_| RuntimeError::internal("failure action sender closed"))??;
                self.retire_wire(wire).await
            }
        }
    }

    async fn run_heartbeat_if_due(&mut self) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let Some(candidate) = self.slot.heartbeat_candidate(self.heartbeat_interval, now) else {
            return Ok(());
        };
        let (reply, received) = oneshot::channel();
        self.event_tx
            .send(SlotEvent::HeartbeatDue { candidate, reply })
            .await
            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
        let Some(work) = received
            .await
            .map_err(|_| RuntimeError::internal("heartbeat claim sender closed"))??
        else {
            if !self.slot.defer_heartbeat(candidate, Instant::now()) {
                return Err(RuntimeError::internal(
                    "Slot rejected exact heartbeat pressure deferral",
                ));
            }
            return Ok(());
        };
        if work.candidate != candidate
            || work.attempt.engine_epoch != candidate.generation.engine_epoch
            || work.attempt.slot_id != candidate.generation.slot_id
            || work.attempt.attempt_number != 1
            || work.attempt.attempts_including_current != 1
            || work.attempt.expected_generation.is_some()
        {
            return Err(RuntimeError::internal(
                "heartbeat claim reply does not match the due Slot generation",
            ));
        }
        if !self
            .slot
            .assign_heartbeat(candidate, work.attempt.request_id)?
        {
            return Err(RuntimeError::internal(
                "Slot rejected its identity-bound heartbeat claim",
            ));
        }
        let outcome = self.execute_heartbeat(work).await?;
        self.publish_work_outcome(outcome).await
    }

    async fn execute_heartbeat(
        &mut self,
        work: HeartbeatWork,
    ) -> Result<WorkOutcome, RuntimeError> {
        let attempt = work.attempt;
        let generation = work.candidate.generation;
        let receive_boundary = self.slot.receive_sequence();
        let message = self.message_ids.next(TYPE_HEARTBEAT)?;
        let wire = request_wire(attempt, generation, Some(message));
        self.slot.begin_heartbeat(message, receive_boundary)?;
        if self
            .publish_transition(attempt.request_id, RequestState::Sending, wire)
            .await
            .is_err()
        {
            return Ok(WorkOutcome::Interrupted { wire });
        }
        let frame = HeartbeatRequest
            .frame(message.msg_id())
            .encode()
            .map_err(RuntimeError::from)?;
        match self
            .write_frame(
                attempt.request_id,
                &frame,
                attempt.deadline,
                TimeoutPhase::Heartbeat,
                None,
            )
            .await
        {
            Ok(_) => {}
            Err(OperationFailure::Interrupted) => {
                return Ok(WorkOutcome::Interrupted { wire });
            }
            Err(OperationFailure::Error(error)) => {
                return Ok(WorkOutcome::Failed {
                    wire,
                    error,
                    retryable: true,
                    same_attempt_possible: false,
                });
            }
        }
        if self
            .publish_transition(attempt.request_id, RequestState::WaitingResponse, wire)
            .await
            .is_err()
        {
            return Ok(WorkOutcome::Interrupted { wire });
        }
        let routed = match self
            .read_matching_response(
                attempt.request_id,
                generation,
                attempt.total_deadline,
                TimeoutPhase::Heartbeat,
            )
            .await
        {
            Ok(routed) => routed,
            Err(OperationFailure::Interrupted) => {
                return Ok(WorkOutcome::Interrupted { wire });
            }
            Err(OperationFailure::Error(error)) => {
                return Ok(WorkOutcome::Failed {
                    wire,
                    error,
                    retryable: true,
                    same_attempt_possible: false,
                });
            }
        };
        match parse_heartbeat_payload(&routed.response.data) {
            Ok(acknowledgement) => Ok(WorkOutcome::HeartbeatCompleted {
                wire,
                acknowledgement,
            }),
            Err(error) => Ok(WorkOutcome::Failed {
                wire,
                error: RuntimeError::from(error),
                retryable: true,
                same_attempt_possible: false,
            }),
        }
    }

    async fn execute_work(
        &mut self,
        attempt: RequestAttempt,
        request: CommandRequest,
        begin_endpoint_attempt: bool,
        publish_connect_transition: bool,
    ) -> Result<WorkOutcome, RuntimeError> {
        if let Some(interrupted) = self.current_interrupt(attempt.request_id) {
            if interrupted {
                return Ok(WorkOutcome::UnstartedCancelled {
                    request_id: attempt.request_id,
                });
            }
        }
        let boundary = if self.slot.state() == SlotState::Idle {
            self.slot.assign_ready(attempt.request_id)?;
            self.slot.receive_sequence()
        } else {
            match self
                .connect_for_request(attempt, begin_endpoint_attempt, publish_connect_transition)
                .await?
            {
                ConnectForRequest::Ready { receive_boundary } => receive_boundary,
                ConnectForRequest::Outcome(outcome) => return Ok(outcome),
            }
        };

        let message = self.message_ids.next(request.command_code())?;
        let generation = self
            .slot
            .generation_identity()
            .ok_or_else(|| RuntimeError::internal("ready Slot has no TCP generation"))?;
        let wire = request_wire(attempt, generation, Some(message));
        self.slot.begin_business(message, boundary)?;
        if self
            .publish_transition(attempt.request_id, RequestState::Sending, wire)
            .await
            .is_err()
        {
            return Ok(WorkOutcome::Interrupted { wire });
        }
        let frame = request
            .frame(message.msg_id())
            .map_err(RuntimeError::from)?
            .encode()
            .map_err(RuntimeError::from)?;
        match self
            .write_frame(
                attempt.request_id,
                &frame,
                attempt.deadline,
                TimeoutPhase::Send,
                Some(wire),
            )
            .await
        {
            Ok(_) => {}
            Err(OperationFailure::Interrupted) => return Ok(WorkOutcome::Interrupted { wire }),
            Err(OperationFailure::Error(error)) => {
                return Ok(WorkOutcome::Failed {
                    wire,
                    error,
                    retryable: true,
                    same_attempt_possible: false,
                });
            }
        }
        if self
            .publish_transition(attempt.request_id, RequestState::WaitingResponse, wire)
            .await
            .is_err()
        {
            return Ok(WorkOutcome::Interrupted { wire });
        }
        match self
            .read_matching_response(
                attempt.request_id,
                generation,
                attempt.total_deadline,
                TimeoutPhase::Response,
            )
            .await
        {
            Ok(routed) => Ok(WorkOutcome::Completed {
                wire,
                response: routed.response,
            }),
            Err(OperationFailure::Interrupted) => Ok(WorkOutcome::Interrupted { wire }),
            Err(OperationFailure::Error(error)) => Ok(WorkOutcome::Failed {
                wire,
                error,
                retryable: true,
                same_attempt_possible: false,
            }),
        }
    }

    async fn connect_for_request(
        &mut self,
        attempt: RequestAttempt,
        begin_endpoint_attempt: bool,
        publish_connect_transition: bool,
    ) -> Result<ConnectForRequest, RuntimeError> {
        if self.slot.state() != SlotState::Disconnected {
            return Err(RuntimeError::internal(
                "request assigned to a Slot that is neither ready nor disconnected",
            ));
        }
        if begin_endpoint_attempt {
            self.slot.begin_endpoint_attempt()?;
        }
        let start = self
            .slot
            .start_connect(attempt.request_id, attempt.deadline, Instant::now())?
            .ok_or_else(|| RuntimeError::timeout(TimeoutPhase::Connect))?;
        let wire = request_wire(attempt, start.identity, None);
        if publish_connect_transition {
            if self
                .publish_transition(attempt.request_id, RequestState::Connecting, wire)
                .await
                .is_err()
            {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Interrupted {
                    wire,
                }));
            }
        }
        let stream = match self
            .connect_stream(
                attempt.request_id,
                start.attempt.endpoint.address(),
                start.attempt.deadline,
            )
            .await
        {
            Ok(stream) => stream,
            Err(OperationFailure::Interrupted) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Interrupted {
                    wire,
                }));
            }
            Err(OperationFailure::Error(error)) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Failed {
                    wire,
                    error,
                    retryable: true,
                    same_attempt_possible: start.attempt.endpoints_remaining > 1,
                }));
            }
        };
        self.stream = Some(stream);
        if !self.slot.on_connected(start.identity, Instant::now())? {
            return Err(RuntimeError::internal(
                "Slot rejected its own connect completion",
            ));
        }
        let handshake = self.message_ids.next(TYPE_HANDSHAKE)?;
        self.slot.begin_handshake(handshake, 0, false)?;
        let handshake_wire = request_wire(attempt, start.identity, Some(handshake));
        if self
            .publish_transition(
                attempt.request_id,
                RequestState::Handshaking,
                handshake_wire,
            )
            .await
            .is_err()
        {
            return Ok(ConnectForRequest::Outcome(WorkOutcome::Interrupted {
                wire: handshake_wire,
            }));
        }
        let frame = HandshakeRequest
            .frame(handshake.msg_id())
            .encode()
            .map_err(RuntimeError::from)?;
        match self
            .write_frame(
                attempt.request_id,
                &frame,
                attempt.deadline,
                TimeoutPhase::Handshake,
                None,
            )
            .await
        {
            Ok(_) => {}
            Err(OperationFailure::Interrupted) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Interrupted {
                    wire: handshake_wire,
                }));
            }
            Err(OperationFailure::Error(error)) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Failed {
                    wire: handshake_wire,
                    error,
                    retryable: true,
                    same_attempt_possible: false,
                }));
            }
        }
        let routed = match self
            .read_matching_response(
                attempt.request_id,
                start.identity,
                attempt.deadline,
                TimeoutPhase::Handshake,
            )
            .await
        {
            Ok(routed) => routed,
            Err(OperationFailure::Interrupted) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Interrupted {
                    wire: handshake_wire,
                }));
            }
            Err(OperationFailure::Error(error)) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Failed {
                    wire: handshake_wire,
                    error,
                    retryable: true,
                    same_attempt_possible: false,
                }));
            }
        };
        let handshake = match parse_handshake_payload(&routed.response.data) {
            Ok(handshake) => handshake,
            Err(error) => {
                return Ok(ConnectForRequest::Outcome(WorkOutcome::Failed {
                    wire: handshake_wire,
                    error: RuntimeError::from(error),
                    retryable: true,
                    same_attempt_possible: false,
                }));
            }
        };
        self.publish_handshake(handshake).await?;
        Ok(ConnectForRequest::Ready {
            receive_boundary: routed.frame.receive_sequence,
        })
    }

    async fn ensure_connected(&mut self, deadline: Deadline) -> Result<(), RuntimeError> {
        if self.slot.state() == SlotState::Idle {
            return Ok(());
        }
        if self.slot.state() != SlotState::Disconnected {
            return Err(RuntimeError::internal("explicit connect found a busy Slot"));
        }
        self.slot.begin_endpoint_attempt()?;
        let synthetic_value = u64::MAX
            .checked_sub(u64::try_from(self.slot.slot_id().get()).map_err(|_| {
                RuntimeError::internal("Slot id does not fit synthetic request identity")
            })?)
            .ok_or_else(|| RuntimeError::internal("synthetic request identity underflow"))?;
        let request_id = RequestId::new(synthetic_value)?;
        loop {
            let Some(start) = self
                .slot
                .start_connect(request_id, deadline, Instant::now())?
            else {
                return Err(RuntimeError::connection_closed(
                    "unable to connect any configured 7709 endpoint",
                ));
            };
            self.refresh_snapshot(true)?;
            match self
                .connect_stream(
                    request_id,
                    start.attempt.endpoint.address(),
                    start.attempt.deadline,
                )
                .await
            {
                Ok(stream) => self.stream = Some(stream),
                Err(OperationFailure::Interrupted) => {
                    self.retire_synthetic(request_id, start.identity)?;
                    return Err(RuntimeError::connection_closed(
                        "explicit connect interrupted by Engine close",
                    ));
                }
                Err(OperationFailure::Error(error)) => {
                    self.retire_synthetic(request_id, start.identity)?;
                    if start.attempt.endpoints_remaining > 1 {
                        continue;
                    }
                    return Err(error);
                }
            }
            if !self.slot.on_connected(start.identity, Instant::now())? {
                return Err(RuntimeError::internal(
                    "Slot rejected explicit connect completion",
                ));
            }
            let message = self.message_ids.next(TYPE_HANDSHAKE)?;
            self.slot.begin_handshake(message, 0, false)?;
            self.refresh_snapshot(true)?;
            let frame = HandshakeRequest
                .frame(message.msg_id())
                .encode()
                .map_err(RuntimeError::from)?;
            let handshake = self
                .write_frame(request_id, &frame, deadline, TimeoutPhase::Handshake, None)
                .await
                .map(|_| ());
            if let Err(failure) = handshake {
                let error = failure.into_error("explicit connect interrupted");
                self.retire_synthetic(request_id, start.identity)?;
                if start.attempt.endpoints_remaining > 1 {
                    continue;
                }
                return Err(error);
            }
            let routed = match self
                .read_matching_response(
                    request_id,
                    start.identity,
                    deadline,
                    TimeoutPhase::Handshake,
                )
                .await
            {
                Ok(routed) => routed,
                Err(failure) => {
                    let error = failure.into_error("explicit connect interrupted");
                    self.retire_synthetic(request_id, start.identity)?;
                    if start.attempt.endpoints_remaining > 1 {
                        continue;
                    }
                    return Err(error);
                }
            };
            let handshake = match parse_handshake_payload(&routed.response.data) {
                Ok(handshake) => handshake,
                Err(error) => {
                    self.retire_synthetic(request_id, start.identity)?;
                    if start.attempt.endpoints_remaining > 1 {
                        continue;
                    }
                    return Err(RuntimeError::from(error));
                }
            };
            self.publish_handshake(handshake).await?;
            if !self.slot.release_unstarted_request(request_id)? {
                return Err(RuntimeError::internal(
                    "explicit connect did not release synthetic request ownership",
                ));
            }
            return Ok(());
        }
    }

    fn retire_synthetic(
        &mut self,
        request_id: RequestId,
        identity: GenerationIdentity,
    ) -> Result<(), RuntimeError> {
        self.slot
            .begin_reconnect_retire(request_id, identity, "explicit connect retry")?;
        self.stream = None;
        self.slot
            .finish_reconnect_retire(request_id, identity)?
            .ok_or_else(|| {
                RuntimeError::internal("synthetic generation retirement was rejected")
            })?;
        Ok(())
    }

    async fn retire_wire(&mut self, wire: RequestWireIdentity) -> Result<(), RuntimeError> {
        let generation = GenerationIdentity {
            engine_epoch: wire.engine_epoch,
            slot_id: wire.slot_id,
            generation: wire
                .generation
                .ok_or_else(|| RuntimeError::internal("started retirement has no generation"))?,
        };
        if !self.slot.begin_reconnect_retire(
            wire.request_id,
            generation,
            "request generation retirement",
        )? {
            return Err(RuntimeError::internal(
                "Slot rejected exact generation retirement",
            ));
        }
        self.stream = None;
        let acknowledgement = self
            .slot
            .finish_reconnect_retire(wire.request_id, generation)?
            .ok_or_else(|| RuntimeError::internal("Slot retirement acknowledgement was stale"))?;
        self.refresh_snapshot(true)?;
        self.event_tx
            .send(SlotEvent::Retired { acknowledgement })
            .await
            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))
    }

    fn stop_slot(&mut self) -> Result<(), RuntimeError> {
        if self.slot.state() == SlotState::Disconnected {
            self.stream = None;
            return Ok(());
        }
        if self.slot.active_request().is_some() {
            return Err(RuntimeError::internal(
                "Slot stop reached an active request without retirement",
            ));
        }
        let identity = self
            .slot
            .begin_retire("Engine close")?
            .ok_or_else(|| RuntimeError::internal("connected Slot has no retirement identity"))?;
        self.stream = None;
        self.slot
            .finish_retire(identity)?
            .ok_or_else(|| RuntimeError::internal("idle Slot retirement was rejected"))?;
        Ok(())
    }

    async fn connect_stream(
        &mut self,
        request_id: RequestId,
        address: std::net::SocketAddr,
        deadline: Deadline,
    ) -> Result<TcpStream, OperationFailure> {
        tokio::select! {
            biased;
            directive = wait_for_interrupt(&mut self.directive_rx, request_id) => {
                let _ = directive;
                Err(OperationFailure::Interrupted)
            }
            result = timeout_at(deadline.tokio_instant(), TcpStream::connect(address)) => {
                match result {
                    Ok(Ok(stream)) => {
                        stream.set_nodelay(true).map_err(|error| {
                            OperationFailure::Error(RuntimeError::connection_closed(
                                format!("unable to configure TCP_NODELAY: {error}")
                            ))
                        })?;
                        Ok(stream)
                    }
                    Ok(Err(error)) => Err(OperationFailure::Error(
                        RuntimeError::connection_closed(format!("7709 TCP connect failed: {error}"))
                    )),
                    Err(_) => Err(OperationFailure::Error(RuntimeError::timeout(
                        TimeoutPhase::Connect,
                    ))),
                }
            }
        }
    }

    async fn write_frame(
        &mut self,
        request_id: RequestId,
        frame: &[u8],
        deadline: Deadline,
        phase: TimeoutPhase,
        business_wire: Option<RequestWireIdentity>,
    ) -> Result<usize, OperationFailure> {
        let mut offset = 0_usize;
        let mut business_boundary_published = false;
        while offset < frame.len() {
            let stream = self.stream.as_ref().ok_or_else(|| {
                OperationFailure::Error(RuntimeError::connection_closed(
                    "TCP stream disappeared during send",
                ))
            })?;
            tokio::select! {
                biased;
                directive = wait_for_interrupt(&mut self.directive_rx, request_id) => {
                    let _ = directive;
                    return Err(OperationFailure::Interrupted);
                }
                ready = timeout_at(deadline.tokio_instant(), stream.writable()) => {
                    match ready {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => return Err(OperationFailure::Error(
                            RuntimeError::connection_closed(format!(
                                "7709 TCP writable failed: {error}"
                            ))
                        )),
                        Err(_) => return Err(OperationFailure::Error(RuntimeError::timeout(phase))),
                    }
                }
            }
            match stream.try_write(&frame[offset..]) {
                Ok(0) => {
                    return Err(OperationFailure::Error(RuntimeError::connection_closed(
                        "7709 TCP stream closed during send",
                    )));
                }
                Ok(written) => {
                    offset = offset.checked_add(written).ok_or_else(|| {
                        OperationFailure::Error(RuntimeError::internal("send offset overflow"))
                    })?;
                    if written != 0 && !business_boundary_published {
                        if let Some(wire) = business_wire {
                            if self.publish_business_bytes(wire).await.is_err() {
                                return Err(OperationFailure::Interrupted);
                            }
                            business_boundary_published = true;
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    return Err(OperationFailure::Error(RuntimeError::connection_closed(
                        format!("7709 TCP send failed: {error}"),
                    )));
                }
            }
        }
        Ok(offset)
    }

    async fn read_matching_response(
        &mut self,
        request_id: RequestId,
        generation: GenerationIdentity,
        deadline: Deadline,
        phase: TimeoutPhase,
    ) -> Result<RoutedResponse, OperationFailure> {
        loop {
            let decoded = self
                .slot
                .decode_turn(generation)
                .map_err(OperationFailure::Error)?
                .ok_or_else(|| {
                    OperationFailure::Error(RuntimeError::internal(
                        "Slot rejected its current decode generation",
                    ))
                })?;
            let route_frames = self.slot.decoded_queue_usage().0.min(SLOT_FRAME_BUDGET);
            if let Some(matched) = self.route_available(generation, true)? {
                return Ok(matched);
            }
            if decoded.budget_exhausted
                || route_frames >= SLOT_FRAME_BUDGET
                || self.slot.decoded_queue_usage().0 != 0
            {
                tokio::task::yield_now().await;
                continue;
            }
            let capacity = self.slot.wire_read_capacity(generation);
            if capacity == 0 {
                return Err(OperationFailure::Error(RuntimeError::internal(
                    "Slot has no bounded wire read capacity",
                )));
            }
            let mut buffer = vec![0_u8; capacity];
            let stream = self.stream.as_ref().ok_or_else(|| {
                OperationFailure::Error(RuntimeError::connection_closed(
                    "TCP stream disappeared during response wait",
                ))
            })?;
            tokio::select! {
                biased;
                directive = wait_for_interrupt(&mut self.directive_rx, request_id) => {
                    let _ = directive;
                    return Err(OperationFailure::Interrupted);
                }
                ready = timeout_at(deadline.tokio_instant(), stream.readable()) => {
                    match ready {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => return Err(OperationFailure::Error(
                            RuntimeError::connection_closed(format!(
                                "7709 TCP readable failed: {error}"
                            ))
                        )),
                        Err(_) => return Err(OperationFailure::Error(RuntimeError::timeout(phase))),
                    }
                }
            }
            let read = match stream.try_read(&mut buffer) {
                Ok(0) => {
                    return Err(OperationFailure::Error(RuntimeError::connection_closed(
                        "7709 TCP stream closed during response wait",
                    )));
                }
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(error) => {
                    return Err(OperationFailure::Error(RuntimeError::connection_closed(
                        format!("7709 TCP receive failed: {error}"),
                    )));
                }
            };
            let accepted = self.slot.push_wire_bytes(generation, &buffer[..read]);
            if accepted != read {
                return Err(OperationFailure::Error(RuntimeError::internal(
                    "Slot rejected bytes within its advertised read capacity",
                )));
            }
        }
    }

    fn route_available(
        &mut self,
        generation: GenerationIdentity,
        send_complete: bool,
    ) -> Result<Option<RoutedResponse>, OperationFailure> {
        let routed = self
            .slot
            .route_decoded_turn(generation, send_complete, Instant::now())
            .map_err(OperationFailure::Error)?
            .ok_or_else(|| {
                OperationFailure::Error(RuntimeError::internal(
                    "Slot rejected its current route generation",
                ))
            })?;
        let mut matched = None;
        for routed in routed {
            match routed.disposition {
                FrameDisposition::Matched(_) => {
                    if matched.is_some() {
                        return Err(OperationFailure::Error(RuntimeError::internal(
                            "one route turn matched multiple exchanges",
                        )));
                    }
                    matched = Some(routed);
                }
                FrameDisposition::Push => self.publish_push(generation, routed.response),
                FrameDisposition::Stale => {}
            }
        }
        Ok(matched)
    }

    fn read_idle_push_turn(&mut self) -> Result<bool, RuntimeError> {
        if self.slot.state() != SlotState::Idle || self.slot.active_request().is_some() {
            return Ok(false);
        }
        let Some(generation) = self.slot.generation_identity() else {
            return Ok(false);
        };
        if self.process_idle_decoded(generation)? {
            return Ok(true);
        }
        let capacity = self.slot.wire_read_capacity(generation);
        if capacity == 0 {
            return Ok(false);
        }
        let mut buffer = vec![0_u8; capacity];
        let Some(stream) = self.stream.as_ref() else {
            return Ok(false);
        };
        let read = match stream.try_read(&mut buffer) {
            Ok(0) => {
                self.retire_idle("idle TCP stream closed")?;
                return Ok(false);
            }
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => {
                self.retire_idle(format!("idle TCP receive failed: {error}"))?;
                return Ok(false);
            }
        };
        if self.slot.push_wire_bytes(generation, &buffer[..read]) != read {
            return Err(RuntimeError::internal(
                "idle Slot rejected advertised wire capacity",
            ));
        }
        self.process_idle_decoded(generation)
    }

    fn process_idle_decoded(
        &mut self,
        generation: GenerationIdentity,
    ) -> Result<bool, RuntimeError> {
        let decoded = match self.slot.decode_turn(generation) {
            Ok(Some(decoded)) => decoded,
            Ok(None) => {
                return Err(RuntimeError::internal(
                    "idle decode generation was rejected",
                ));
            }
            Err(error) => {
                self.retire_idle(error.to_string())?;
                return Ok(true);
            }
        };
        let route_frames = self.slot.decoded_queue_usage().0.min(SLOT_FRAME_BUDGET);
        let routed = match self
            .slot
            .route_decoded_turn(generation, false, Instant::now())
        {
            Ok(Some(routed)) => routed,
            Ok(None) => {
                return Err(RuntimeError::internal("idle route generation was rejected"));
            }
            Err(error) => {
                self.retire_idle(error.to_string())?;
                return Ok(true);
            }
        };
        for routed in routed {
            match routed.disposition {
                FrameDisposition::Push => self.publish_push(generation, routed.response),
                FrameDisposition::Matched(_) => {
                    return Err(RuntimeError::internal(
                        "idle Slot matched a response without an exchange",
                    ));
                }
                FrameDisposition::Stale => {}
            }
        }
        Ok(decoded.budget_exhausted
            || route_frames >= SLOT_FRAME_BUDGET
            || self.slot.decoded_queue_usage().0 != 0)
    }

    fn retire_idle(&mut self, reason: impl Into<String>) -> Result<(), RuntimeError> {
        let identity = self
            .slot
            .begin_retire(reason)?
            .ok_or_else(|| RuntimeError::internal("idle retirement has no generation"))?;
        self.stream = None;
        self.slot
            .finish_retire(identity)?
            .ok_or_else(|| RuntimeError::internal("idle generation retirement was rejected"))?;
        Ok(())
    }

    fn publish_push(&self, generation: GenerationIdentity, response: ResponseFrame) {
        let Some(host) = self.slot.connected_host() else {
            self.push_dropped.fetch_add(1, Ordering::AcqRel);
            return;
        };
        let frame = PushFrame {
            engine_epoch: generation.engine_epoch,
            slot_id: generation.slot_id,
            generation: generation.generation,
            connected_host: Arc::<str>::from(host),
            response,
        };
        if self.push_tx.try_send(frame).is_err() {
            self.push_dropped.fetch_add(1, Ordering::AcqRel);
        }
    }

    async fn publish_transition(
        &mut self,
        request_id: RequestId,
        state: RequestState,
        wire: RequestWireIdentity,
    ) -> Result<(), RuntimeError> {
        self.refresh_snapshot(true)?;
        let (reply, received) = oneshot::channel();
        self.event_tx
            .send(SlotEvent::Transition {
                request_id,
                state,
                wire,
                reply,
            })
            .await
            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
        received
            .await
            .map_err(|_| RuntimeError::internal("transition acknowledgement sender closed"))?
    }

    async fn publish_business_bytes(&self, wire: RequestWireIdentity) -> Result<(), RuntimeError> {
        let (reply, received) = oneshot::channel();
        self.event_tx
            .send(SlotEvent::BusinessBytes { wire, reply })
            .await
            .map_err(|_| RuntimeError::internal("runtime event receiver closed"))?;
        received
            .await
            .map_err(|_| RuntimeError::internal("send-boundary acknowledgement sender closed"))?
    }

    fn current_interrupt(&mut self, request_id: RequestId) -> Option<bool> {
        match *self.directive_rx.borrow_and_update() {
            SlotDirective::Run => None,
            SlotDirective::Cancel(cancelled) => Some(cancelled == request_id),
            SlotDirective::Stop => Some(true),
        }
    }
}

#[derive(Debug)]
enum ConnectForRequest {
    Ready { receive_boundary: u64 },
    Outcome(WorkOutcome),
}

#[derive(Debug)]
enum OperationFailure {
    Interrupted,
    Error(RuntimeError),
}

impl OperationFailure {
    fn into_error(self, interrupted_message: &'static str) -> RuntimeError {
        match self {
            Self::Interrupted => RuntimeError::connection_closed(interrupted_message),
            Self::Error(error) => error,
        }
    }
}

async fn wait_for_interrupt(
    receiver: &mut watch::Receiver<SlotDirective>,
    request_id: RequestId,
) -> SlotDirective {
    loop {
        if receiver.changed().await.is_err() {
            return SlotDirective::Stop;
        }
        let directive = *receiver.borrow_and_update();
        match directive {
            SlotDirective::Run => {}
            SlotDirective::Cancel(cancelled) if cancelled != request_id => {}
            SlotDirective::Cancel(_) | SlotDirective::Stop => return directive,
        }
    }
}

fn request_wire(
    attempt: RequestAttempt,
    generation: GenerationIdentity,
    message: Option<MessageIdentity>,
) -> RequestWireIdentity {
    RequestWireIdentity {
        engine_epoch: attempt.engine_epoch,
        request_id: attempt.request_id,
        lease_id: attempt.lease_id,
        slot_id: attempt.slot_id,
        generation: Some(generation.generation),
        message,
    }
}

#[derive(Debug)]
struct MessageIdGenerator {
    counter: u32,
    key: u64,
}

impl MessageIdGenerator {
    fn new() -> Self {
        Self {
            counter: 0,
            key: rand::random::<u64>(),
        }
    }

    fn next(&mut self, message_type: u16) -> Result<MessageIdentity, RuntimeError> {
        loop {
            self.counter = self
                .counter
                .checked_add(1)
                .ok_or_else(|| RuntimeError::internal("message identity space exhausted"))?;
            let message_id = keyed_permutation(self.counter, self.key);
            if message_id != 0 {
                return MessageIdentity::new(message_id, message_type);
            }
        }
    }
}

fn keyed_permutation(value: u32, key: u64) -> u32 {
    let mut left = (value >> 16) as u16;
    let mut right = value as u16;
    for round in 0_u32..4 {
        let mixed = u64::from(right)
            ^ key.rotate_left(round.saturating_mul(13))
            ^ u64::from(round).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut output = mixed ^ (mixed >> 30);
        output = output.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        output ^= output >> 27;
        output = output.wrapping_mul(0x94d0_49bb_1331_11eb);
        output ^= output >> 31;
        let next = left ^ output as u16;
        left = right;
        right = next;
    }
    (u32::from(left) << 16) | u32::from(right)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::{
        check_pid, keyed_permutation, ConnectAttemptId, ControlCell, Engine, EngineConfig,
        HostCloseAttemptId, HostConnectAttempt, HostLifecycle, IngressOwnership, PendingConnect,
        PendingPoll, RuntimeCore, CLOSE_TIMEOUT,
    };
    use crate::diagnostics::PoolState;
    use crate::endpoint::Endpoint;
    use crate::error::RuntimeError;
    use crate::slot::RequestId;

    fn config(pool_size: usize, max_pending: usize) -> Result<EngineConfig, RuntimeError> {
        EngineConfig::from_endpoints(
            vec![Endpoint::numeric("127.0.0.1:7709")?],
            Duration::from_secs(8),
            pool_size,
            Some(Duration::from_secs(30)),
            max_pending,
            1_024,
            8 * 1024 * 1024,
        )
    }

    #[test]
    fn configuration_freezes_all_bounded_runtime_capacities() -> Result<(), RuntimeError> {
        let config = config(4, 256)?;
        assert_eq!(config.pool_size(), 4);
        assert_eq!(config.max_pending_requests(), 256);
        assert_eq!(config.total_capacity()?, 260);
        assert_eq!(config.push_queue_size(), 1_024);
        assert_eq!(config.push_queue_bytes(), 8 * 1024 * 1024);
        assert_eq!(config.heartbeat_interval(), Some(Duration::from_secs(30)));
        assert_eq!(CLOSE_TIMEOUT, Duration::from_secs(1));
        Ok(())
    }

    #[test]
    fn keyed_message_identity_is_nonsequential_and_collision_free_over_a_prefix() {
        let key = 0x0123_4567_89ab_cdef;
        let mut values = BTreeSet::new();
        for counter in 1..=65_536_u32 {
            let value = keyed_permutation(counter, key);
            assert!(values.insert(value));
        }
        assert_ne!(keyed_permutation(1, key), 1);
        assert_ne!(keyed_permutation(2, key), 2);
    }

    #[test]
    fn internal_request_identities_are_even_and_disjoint_from_public_ids(
    ) -> Result<(), RuntimeError> {
        let (_command_tx, command_rx) = tokio::sync::mpsc::channel(1);
        let mut runtime = RuntimeCore::new(
            config(1, 1)?,
            command_rx,
            Arc::new(ControlCell::new()),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(std::sync::Mutex::new(super::DiagnosticsCache::stopped(1))),
        )?;
        let first = runtime.next_internal_request_id()?;
        let second = runtime.next_internal_request_id()?;
        assert_eq!(first.get(), 2);
        assert_eq!(second.get(), 4);
        assert_ne!(first, RequestId::new(1)?);
        assert_ne!(second, RequestId::new(3)?);
        Ok(())
    }

    #[test]
    fn control_close_and_cancel_are_independent_of_data_ingress() -> Result<(), RuntimeError> {
        let control = ControlCell::new();
        let confirmation = Arc::new(super::Completion::new());
        let request_id = RequestId::new(7)?;
        control.request_cancel(request_id, Arc::clone(&confirmation))?;
        control.request_close()?;

        let snapshot = control.take_snapshot()?;
        assert!(snapshot.close_requested);
        assert!(snapshot.cancellations.contains_key(&request_id));
        Ok(())
    }

    #[test]
    fn concurrent_connect_callers_share_one_publication_identity() -> Result<(), RuntimeError> {
        let engine = Engine::new(config(1, 1)?)?;
        let attempt = Arc::new(HostConnectAttempt {
            id: ConnectAttemptId::next(0)?,
            completion: Arc::new(super::Completion::new()),
        });
        {
            let mut host = super::lock_mutex(&engine.inner.host, "test connect gate")?;
            host.lifecycle = HostLifecycle::Starting;
            host.connect_counter = attempt.id.0;
            host.connect_attempt = Some(Arc::clone(&attempt));
            assert!(!host.fully_connected);
        }
        let starting = engine.pool_diagnostics()?;
        assert_eq!(starting.state, PoolState::Starting);
        assert!(starting.broker.is_none());
        let mut first = PendingConnect {
            created_pid: std::process::id(),
            engine: engine.clone(),
            attempt: Arc::clone(&attempt),
            terminal: None,
        };
        let mut second = PendingConnect {
            created_pid: std::process::id(),
            engine: engine.clone(),
            attempt: Arc::clone(&attempt),
            terminal: None,
        };
        assert_eq!(first.wait_timeout(Duration::ZERO)?, PendingPoll::Pending);
        attempt.completion.publish(Ok(()));
        assert_eq!(first.wait_timeout(Duration::ZERO)?, PendingPoll::Ready(()));
        assert_eq!(second.wait_timeout(Duration::ZERO)?, PendingPoll::Ready(()));
        let host = super::lock_mutex(&engine.inner.host, "test connect gate")?;
        assert_eq!(host.lifecycle, HostLifecycle::Running);
        assert!(host.fully_connected);
        assert!(host.connect_attempt.is_none());
        drop(host);
        assert_eq!(engine.pool_diagnostics()?.state, PoolState::Running);
        Ok(())
    }

    #[test]
    fn connect_attempt_identity_exhaustion_is_non_mutating() -> Result<(), RuntimeError> {
        assert_eq!(ConnectAttemptId::next(0)?, ConnectAttemptId(1));
        assert!(ConnectAttemptId::next(u64::MAX).is_err());
        assert_eq!(HostCloseAttemptId::next(0)?, HostCloseAttemptId(1));
        assert!(HostCloseAttemptId::next(u64::MAX).is_err());
        Ok(())
    }

    #[test]
    fn idempotent_connect_success_cannot_survive_close_publication() -> Result<(), RuntimeError> {
        let engine = Engine::new(config(1, 1)?)?;
        {
            let mut host = super::lock_mutex(&engine.inner.host, "test connect close race")?;
            host.lifecycle = HostLifecycle::Closing;
            host.fully_connected = false;
            host.connect_attempt = None;
        }

        let result = engine.finish_connect_attempt(ConnectAttemptId(1), &Ok(()));
        assert!(matches!(
            result,
            Err(error) if error.kind() == "ConnectionClosed"
        ));
        Ok(())
    }

    #[test]
    fn ingress_ownership_is_returned_exactly_when_terminal_owner_drops() {
        let owned = Arc::new(AtomicUsize::new(1));
        {
            let _ownership = IngressOwnership {
                owned: Arc::clone(&owned),
            };
            assert_eq!(owned.load(Ordering::Acquire), 1);
        }
        assert_eq!(owned.load(Ordering::Acquire), 0);
    }

    #[test]
    fn concurrent_close_callers_share_one_host_attempt_and_deadline() -> Result<(), RuntimeError> {
        let engine = Engine::new(config(1, 1)?)?;
        engine.ensure_runtime()?;
        let first = engine.begin_close()?;
        let second = engine.begin_close()?;
        let first_attempt = first
            .attempt
            .as_ref()
            .ok_or_else(|| RuntimeError::internal("first close has no host attempt"))?;
        let second_attempt = second
            .attempt
            .as_ref()
            .ok_or_else(|| RuntimeError::internal("second close has no host attempt"))?;
        assert!(Arc::ptr_eq(first_attempt, second_attempt));
        assert_eq!(first_attempt.deadline, second_attempt.deadline);
        first.wait()?;
        second.wait()?;
        Ok(())
    }

    #[test]
    fn pid_rejection_precedes_any_engine_lock_or_channel_access() {
        let current = std::process::id();
        let different = current.wrapping_add(1);
        assert!(check_pid(current).is_ok());
        assert!(matches!(
            check_pid(different),
            Err(error) if error.kind() == "ConnectionClosed"
        ));
    }

    #[test]
    fn stopped_runtime_can_start_and_close_without_instantiating_slots() -> Result<(), RuntimeError>
    {
        let engine = Engine::new(config(1, 1)?)?;
        let initial = engine.pool_diagnostics()?;
        assert_eq!(initial.epoch, 0);
        assert_eq!(initial.state, PoolState::Stopped);
        assert!(initial.actors.is_empty());
        engine.ensure_runtime()?;
        engine.close()?;
        let closed = engine.pool_diagnostics()?;
        assert_eq!(closed.state, PoolState::Stopped);
        assert!(closed.epoch > 0);
        assert!(closed.actors.is_empty());
        assert!(engine.transport_diagnostics()?.actor.is_none());
        Ok(())
    }
}
