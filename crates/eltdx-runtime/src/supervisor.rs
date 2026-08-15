use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::session::HeartbeatAck;

use crate::deadline::Deadline;
use crate::error::RuntimeError;
use crate::pin::{PinId, PinIdentity, PinRegistry, PinnedCallLease};
use crate::push::{
    PushBuffer, PushBufferSnapshot, PushFrame, DEFAULT_PUSH_MAX_BYTES, DEFAULT_PUSH_MAX_FRAMES,
};
use crate::request::{
    ActiveLease, Admission, AdmissionQueue, Promotion, ReleaseOutcome, WaitingPermit,
};
use crate::request::{
    RequestAttempt, RequestState, RequestTracker, RequestWireIdentity, RetryDecision, RetryPolicy,
    TerminalBatch, TerminalKind, TerminalNotification,
};
use crate::slot::RequestId;
use crate::slot::{
    EngineEpoch, GenerationIdentity, HeartbeatCandidate, ReconnectAck, RetiredGeneration, SlotId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineState {
    Stopped,
    Starting,
    Running,
    Closing,
    Failed,
    FailedClosing,
    FailedClosed,
}

impl EngineState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Closing => "closing",
            Self::Failed => "failed",
            Self::FailedClosing => "failed_closing",
            Self::FailedClosed => "failed_closed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AttemptId(u64);

impl AttemptId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartAttempt {
    id: AttemptId,
    observed_epoch: u64,
    candidate_epoch: EngineEpoch,
}

impl StartAttempt {
    pub const fn id(self) -> AttemptId {
        self.id
    }

    pub const fn observed_epoch(self) -> u64 {
        self.observed_epoch
    }

    pub const fn candidate_epoch(self) -> EngineEpoch {
        self.candidate_epoch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseAttempt {
    id: AttemptId,
    target_epoch: Option<EngineEpoch>,
    invalidated_epoch: u64,
}

impl CloseAttempt {
    pub const fn id(self) -> AttemptId {
        self.id
    }

    pub const fn target_epoch(self) -> Option<EngineEpoch> {
        self.target_epoch
    }

    pub const fn invalidated_epoch(self) -> u64 {
        self.invalidated_epoch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartClaim {
    Owner(StartAttempt),
    Existing(StartAttempt),
    Running(EngineEpoch),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseClaim {
    Owner(CloseAttempt),
    Existing(CloseAttempt),
    AlreadyStopped,
    AlreadyFailedClosed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinTerminalBatch {
    pub notifications: Vec<TerminalNotification>,
    pub pin_promotion: Option<PinnedCallLease>,
    pub ordinary_promotion: Option<Promotion>,
    pub pin_released: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeartbeatClaim {
    pub lease: ActiveLease,
    pub candidate: HeartbeatCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleRetirement {
    pub wire: RequestWireIdentity,
    pub terminal_kind: TerminalKind,
    pub error: RuntimeError,
    preserve_prior_terminal: bool,
}

#[derive(Debug)]
pub struct Supervisor {
    pool_size: usize,
    state: EngineState,
    epoch_counter: u64,
    diagnostic_epoch: u64,
    active_epoch: Option<EngineEpoch>,
    attempt_counter: u64,
    pin_counter: u64,
    start_attempt: Option<StartAttempt>,
    close_attempt: Option<CloseAttempt>,
    instantiated_slots: Vec<SlotId>,
    admission: AdmissionQueue,
    requests: RequestTracker,
    pins: PinRegistry,
    heartbeats: BTreeMap<RequestId, HeartbeatClaim>,
    last_heartbeats: BTreeMap<SlotId, HeartbeatAck>,
    lifecycle_retirements: BTreeMap<RequestId, LifecycleRetirement>,
    lifecycle_notifications: Vec<TerminalNotification>,
    push: Option<PushBuffer>,
    push_max_frames: usize,
    push_max_bytes: usize,
    rejected_waiters: Vec<WaitingPermit>,
    rejected_pin_calls: Vec<PinnedCallLease>,
    fatal: Option<RuntimeError>,
    cleanup_error: Option<RuntimeError>,
    last_error: Option<RuntimeError>,
    failure_lineage: bool,
    stale_event_count: u64,
}

impl Supervisor {
    pub fn new(pool_size: usize) -> Result<Self, RuntimeError> {
        Self::with_admission(pool_size, 256)
    }

    pub fn with_admission(
        pool_size: usize,
        max_pending_requests: usize,
    ) -> Result<Self, RuntimeError> {
        Self::with_limits(
            pool_size,
            max_pending_requests,
            DEFAULT_PUSH_MAX_FRAMES,
            DEFAULT_PUSH_MAX_BYTES,
        )
    }

    pub fn with_limits(
        pool_size: usize,
        max_pending_requests: usize,
        push_max_frames: usize,
        push_max_bytes: usize,
    ) -> Result<Self, RuntimeError> {
        if pool_size == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "pool_size must be a positive integer",
            ));
        }
        if push_max_frames == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_size must be > 0",
            ));
        }
        if push_max_bytes == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_bytes must be > 0",
            ));
        }
        Ok(Self {
            pool_size,
            state: EngineState::Stopped,
            epoch_counter: 0,
            diagnostic_epoch: 0,
            active_epoch: None,
            attempt_counter: 0,
            pin_counter: 0,
            start_attempt: None,
            close_attempt: None,
            instantiated_slots: Vec::new(),
            admission: AdmissionQueue::new(pool_size, max_pending_requests)?,
            requests: RequestTracker::default(),
            pins: PinRegistry::new(pool_size)?,
            heartbeats: BTreeMap::new(),
            last_heartbeats: BTreeMap::new(),
            lifecycle_retirements: BTreeMap::new(),
            lifecycle_notifications: Vec::new(),
            push: None,
            push_max_frames,
            push_max_bytes,
            rejected_waiters: Vec::new(),
            rejected_pin_calls: Vec::new(),
            fatal: None,
            cleanup_error: None,
            last_error: None,
            failure_lineage: false,
            stale_event_count: 0,
        })
    }

    pub const fn state(&self) -> EngineState {
        self.state
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch_counter
    }

    pub const fn diagnostic_epoch(&self) -> u64 {
        self.diagnostic_epoch
    }

    pub(crate) const fn pool_size(&self) -> usize {
        self.pool_size
    }

    pub(crate) fn diagnostic_owner_epoch(&self) -> Option<EngineEpoch> {
        self.event_epoch()
    }

    pub(crate) const fn broker_epoch(&self) -> Option<EngineEpoch> {
        self.admission.epoch()
    }

    pub(crate) fn idle_slot_count(&self) -> usize {
        self.admission.idle_count()
    }

    pub(crate) fn ordinary_waiter_count(&self) -> usize {
        self.admission.ordinary_waiting_count()
    }

    pub(crate) fn pin_waiter_count(&self) -> usize {
        self.admission.pin_waiting_count()
    }

    pub(crate) const fn broker_closed(&self) -> bool {
        self.admission.is_sealed()
    }

    pub const fn active_epoch(&self) -> Option<EngineEpoch> {
        self.active_epoch
    }

    pub const fn start_attempt(&self) -> Option<StartAttempt> {
        self.start_attempt
    }

    pub const fn close_attempt(&self) -> Option<CloseAttempt> {
        self.close_attempt
    }

    pub fn instantiated_slots(&self) -> &[SlotId] {
        &self.instantiated_slots
    }

    pub fn fatal(&self) -> Option<&RuntimeError> {
        self.fatal.as_ref()
    }

    pub fn cleanup_error(&self) -> Option<&RuntimeError> {
        self.cleanup_error.as_ref()
    }

    pub fn last_error(&self) -> Option<&RuntimeError> {
        self.last_error.as_ref()
    }

    pub const fn stale_event_count(&self) -> u64 {
        self.stale_event_count
    }

    pub fn lifecycle_retirements(&self) -> Vec<LifecycleRetirement> {
        self.lifecycle_retirements.values().cloned().collect()
    }

    pub fn take_lifecycle_notifications(&mut self) -> Vec<TerminalNotification> {
        std::mem::take(&mut self.lifecycle_notifications)
    }

    pub fn begin_start(&mut self) -> Result<StartClaim, RuntimeError> {
        match self.state {
            EngineState::Starting => {
                let attempt = self
                    .start_attempt
                    .ok_or_else(|| self.invariant_error("Starting engine has no StartAttempt"))?;
                Ok(StartClaim::Existing(attempt))
            }
            EngineState::Running => {
                let epoch = self
                    .active_epoch
                    .ok_or_else(|| self.invariant_error("Running engine has no active epoch"))?;
                Ok(StartClaim::Running(epoch))
            }
            EngineState::Stopped => {
                if !self.rejected_waiters.is_empty()
                    || !self.rejected_pin_calls.is_empty()
                    || !self.lifecycle_retirements.is_empty()
                    || !self.lifecycle_notifications.is_empty()
                {
                    return Err(self
                        .invariant_error("cannot reopen before rejected admissions are observed"));
                }
                let candidate_value = next_identity(self.epoch_counter, "engine epoch")?;
                let candidate_epoch = EngineEpoch::new(candidate_value)?;
                let id = self.next_attempt_id()?;
                let attempt = StartAttempt {
                    id,
                    observed_epoch: self.epoch_counter,
                    candidate_epoch,
                };
                self.epoch_counter = candidate_value;
                self.diagnostic_epoch = candidate_value;
                self.start_attempt = Some(attempt);
                self.state = EngineState::Starting;
                self.fatal = None;
                self.cleanup_error = None;
                self.last_error = None;
                self.failure_lineage = false;
                Ok(StartClaim::Owner(attempt))
            }
            EngineState::Closing
            | EngineState::Failed
            | EngineState::FailedClosing
            | EngineState::FailedClosed => Err(RuntimeError::connection_closed(format!(
                "7709 Engine is not usable: {}",
                self.state.as_str()
            ))),
        }
    }

    pub fn publish_start(&mut self, attempt: StartAttempt) -> Result<bool, RuntimeError> {
        if self.state != EngineState::Starting || self.start_attempt != Some(attempt) {
            self.record_stale();
            return Ok(false);
        }
        let push = PushBuffer::new(
            attempt.candidate_epoch,
            self.push_max_frames,
            self.push_max_bytes,
        )?;
        self.admission.open(attempt.candidate_epoch)?;
        self.heartbeats.clear();
        self.last_heartbeats.clear();
        self.push = Some(push);
        self.active_epoch = Some(attempt.candidate_epoch);
        self.start_attempt = None;
        self.state = EngineState::Running;
        Ok(true)
    }

    pub fn fail_start(
        &mut self,
        attempt: StartAttempt,
        error: RuntimeError,
        cleanup_complete: bool,
    ) -> bool {
        if self.start_attempt != Some(attempt) {
            self.record_stale();
            return false;
        }
        self.last_error = Some(error.clone());
        if cleanup_complete {
            self.start_attempt = None;
            self.instantiated_slots.clear();
            if self.state == EngineState::Starting {
                self.active_epoch = None;
                self.state = EngineState::Stopped;
            }
            return true;
        }

        self.active_epoch = Some(attempt.candidate_epoch);
        self.cleanup_error.get_or_insert(error);
        self.failure_lineage = true;
        self.state = EngineState::FailedClosing;
        true
    }

    pub fn begin_close(&mut self) -> Result<CloseClaim, RuntimeError> {
        if let Some(attempt) = self.close_attempt {
            return Ok(CloseClaim::Existing(attempt));
        }
        match self.state {
            EngineState::Stopped => return Ok(CloseClaim::AlreadyStopped),
            EngineState::FailedClosed => return Ok(CloseClaim::AlreadyFailedClosed),
            EngineState::Closing => {
                return Err(self.invariant_error("Closing engine has no CloseAttempt"));
            }
            EngineState::Starting
            | EngineState::Running
            | EngineState::Failed
            | EngineState::FailedClosing => {}
        }

        let invalidated_epoch = if matches!(
            self.state,
            EngineState::Starting | EngineState::Running | EngineState::Failed
        ) {
            next_identity(self.epoch_counter, "engine epoch")?
        } else {
            self.epoch_counter
        };
        let target_epoch = self.event_epoch();
        self.prevalidate_lifecycle_cleanup(target_epoch)?;
        let id = self.next_attempt_id()?;
        let attempt = CloseAttempt {
            id,
            target_epoch,
            invalidated_epoch,
        };
        let now = Instant::now();
        self.seal_and_close_pins(now)?;
        let (terminal_kind, terminal_error) = match self.fatal.clone() {
            Some(error) => (TerminalKind::Failed, error),
            None => (
                TerminalKind::Cancelled,
                RuntimeError::connection_closed("request cancelled by Engine close"),
            ),
        };
        self.begin_lifecycle_cleanup(terminal_kind, terminal_error, now)?;
        self.close_push(target_epoch, self.fatal.clone())?;
        self.epoch_counter = invalidated_epoch;
        self.state = if self.failure_lineage || self.state == EngineState::Failed {
            EngineState::FailedClosing
        } else {
            EngineState::Closing
        };
        self.close_attempt = Some(attempt);
        Ok(CloseClaim::Owner(attempt))
    }

    pub fn finish_close(
        &mut self,
        attempt: CloseAttempt,
        result: Result<(), RuntimeError>,
    ) -> bool {
        if self.close_attempt != Some(attempt) {
            self.record_stale();
            return false;
        }
        self.close_attempt = None;
        match result {
            Ok(()) => {
                let push_cleanup_pending = self.push.as_ref().is_some_and(|push| {
                    let snapshot = push.snapshot();
                    !snapshot.closed || snapshot.frame_count != 0 || snapshot.byte_count != 0
                });
                if !self.requests.is_empty()
                    || !self.pins.is_empty()
                    || !self.heartbeats.is_empty()
                    || self.start_attempt.is_some()
                    || !self.lifecycle_retirements.is_empty()
                    || !self.lifecycle_notifications.is_empty()
                    || !self.rejected_waiters.is_empty()
                    || !self.rejected_pin_calls.is_empty()
                    || self.admission.total_owned() != 0
                    || !self.instantiated_slots.is_empty()
                    || push_cleanup_pending
                {
                    let error = self
                        .invariant_error("close completed while runtime cleanup ownership remains");
                    self.last_error = Some(error.clone());
                    self.cleanup_error.get_or_insert(error);
                    self.failure_lineage = true;
                    self.state = EngineState::FailedClosing;
                    return true;
                }
                self.start_attempt = None;
                self.active_epoch = None;
                self.instantiated_slots.clear();
                self.admission.close_complete();
                self.last_heartbeats.clear();
                self.push = None;
                self.cleanup_error = None;
                self.state = if self.failure_lineage {
                    EngineState::FailedClosed
                } else {
                    EngineState::Stopped
                };
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                self.cleanup_error.get_or_insert(error);
                self.failure_lineage = true;
                self.state = EngineState::FailedClosing;
            }
        }
        true
    }

    pub fn publish_fatal(
        &mut self,
        epoch: EngineEpoch,
        error: RuntimeError,
    ) -> Result<bool, RuntimeError> {
        if self.event_epoch() != Some(epoch)
            || matches!(self.state, EngineState::Stopped | EngineState::FailedClosed)
        {
            self.record_stale();
            return Ok(false);
        }
        self.prevalidate_lifecycle_cleanup(Some(epoch))?;
        if self.fatal.is_none() {
            self.fatal = Some(error.clone());
            self.last_error = Some(error);
        }
        let canonical = self
            .fatal
            .clone()
            .ok_or_else(|| self.invariant_error("fatal publication lost its canonical error"))?;
        let now = Instant::now();
        self.seal_and_close_pins(now)?;
        self.begin_lifecycle_cleanup(TerminalKind::Failed, canonical, now)?;
        self.close_push(Some(epoch), self.fatal.clone())?;
        self.failure_lineage = true;
        match self.state {
            EngineState::Starting => {
                self.active_epoch = Some(epoch);
                self.state = EngineState::Failed;
            }
            EngineState::Running => self.state = EngineState::Failed,
            EngineState::Closing => self.state = EngineState::FailedClosing,
            EngineState::Failed | EngineState::FailedClosing => {}
            EngineState::Stopped | EngineState::FailedClosed => {}
        }
        Ok(true)
    }

    pub fn submit(
        &mut self,
        request_id: RequestId,
        deadline: Deadline,
        now: Instant,
    ) -> Result<Admission, RuntimeError> {
        self.submit_with_retry_policy(request_id, deadline, RetryPolicy::default(), now)
    }

    pub fn submit_with_retry_policy(
        &mut self,
        request_id: RequestId,
        deadline: Deadline,
        retry_policy: RetryPolicy,
        now: Instant,
    ) -> Result<Admission, RuntimeError> {
        if self.state != EngineState::Running {
            return Err(RuntimeError::connection_closed(format!(
                "7709 Engine is not usable: {}",
                self.state.as_str()
            )));
        }
        let epoch = self
            .active_epoch
            .ok_or_else(|| self.invariant_error("Running engine has no active epoch"))?;
        if self.requests.contains(request_id) {
            return Err(RuntimeError::internal("request lifecycle already exists")
                .with_context("request_id", request_id.get().to_string()));
        }
        let admission = self.admission.submit(epoch, request_id, deadline, now)?;
        self.requests
            .admit_with_retry_policy(admission, retry_policy)?;
        Ok(admission)
    }

    pub fn open_pin(
        &mut self,
        reservation_request_id: RequestId,
    ) -> Result<PinIdentity, RuntimeError> {
        if self.state != EngineState::Running {
            return Err(RuntimeError::connection_closed(format!(
                "7709 Engine is not usable: {}",
                self.state.as_str()
            )));
        }
        let lease = self
            .requests
            .assigned_active_for_pin(reservation_request_id)?;
        if self.active_epoch != Some(lease.engine_epoch) {
            return Err(self.invariant_error("pin reservation belongs to a stale engine epoch"));
        }
        let pin_value = next_identity(self.pin_counter, "pin id")?;
        let pin_id = PinId::new(pin_value)?;
        self.pins.validate_register(pin_id, lease)?;
        let transferred = self
            .requests
            .transfer_assigned_to_pin(reservation_request_id)?;
        if transferred != lease {
            return Err(self.invariant_error("pin reservation lease changed during transfer"));
        }
        let identity = self.pins.register(pin_id, lease)?;
        self.pin_counter = pin_value;
        Ok(identity)
    }

    pub fn submit_pin(
        &mut self,
        pin: PinIdentity,
        request_id: RequestId,
        deadline: Deadline,
        retry_policy: RetryPolicy,
        now: Instant,
    ) -> Result<Admission, RuntimeError> {
        if self.state != EngineState::Running || self.active_epoch != Some(pin.engine_epoch) {
            return Err(RuntimeError::connection_closed(
                "pinned proxy belongs to a closed engine epoch",
            ));
        }
        if self.requests.contains(request_id) {
            return Err(RuntimeError::internal("request lifecycle already exists")
                .with_context("request_id", request_id.get().to_string()));
        }
        let admission = if self.pins.can_admit_direct(pin, request_id, now, deadline)? {
            Admission::Pinned(self.pins.admit_direct(pin, request_id, deadline, now)?)
        } else {
            let permit =
                self.admission
                    .reserve_pin_waiting(pin.engine_epoch, request_id, deadline, now)?;
            self.pins.enqueue(pin, permit, now)?;
            Admission::Waiting(permit)
        };
        self.requests
            .admit_with_retry_policy(admission, retry_policy)?;
        Ok(admission)
    }

    pub fn pin_count(&self) -> usize {
        self.pins.len()
    }

    pub fn claim_heartbeat(
        &mut self,
        request_id: RequestId,
        candidate: HeartbeatCandidate,
        request_timeout: Duration,
        now: Instant,
    ) -> Result<Option<HeartbeatClaim>, RuntimeError> {
        if request_timeout.is_zero() {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "heartbeat request timeout must be > 0",
            ));
        }
        if self.state != EngineState::Running
            || self.active_epoch != Some(candidate.generation.engine_epoch)
        {
            return Ok(None);
        }
        if self.requests.contains(request_id) || self.heartbeats.contains_key(&request_id) {
            return Err(
                RuntimeError::internal("heartbeat request identity is already owned")
                    .with_context("request_id", request_id.get().to_string()),
            );
        }
        let internal_active = self
            .pins
            .len()
            .checked_add(self.heartbeats.len())
            .ok_or_else(|| self.invariant_error("internal active owner count overflow"))?;
        let ordinary_active = self
            .admission
            .active_count()
            .checked_sub(internal_active)
            .ok_or_else(|| {
                self.invariant_error("internal owners exceed admission active leases")
            })?;
        if ordinary_active != 0 || self.admission.waiting_count() != 0 {
            return Ok(None);
        }
        if self
            .pins
            .identities()
            .iter()
            .any(|pin| pin.slot_id == candidate.generation.slot_id)
        {
            return Ok(None);
        }
        let deadline = Deadline::after_at(now, request_timeout)?;
        let Some(lease) = self.admission.claim_idle_for_heartbeat(
            candidate.generation.engine_epoch,
            request_id,
            candidate.generation.slot_id,
            deadline,
            now,
        )?
        else {
            return Ok(None);
        };
        let claim = HeartbeatClaim { lease, candidate };
        self.requests
            .admit_with_retry_policy(Admission::Active(lease), RetryPolicy::internal_heartbeat())?;
        if self.heartbeats.insert(request_id, claim).is_some() {
            return Err(self.invariant_error("heartbeat claim identity was reused"));
        }
        Ok(Some(claim))
    }

    pub fn heartbeat_count(&self) -> usize {
        self.heartbeats.len()
    }

    pub fn last_heartbeat(&self, slot_id: SlotId) -> Option<&HeartbeatAck> {
        self.last_heartbeats.get(&slot_id)
    }

    pub fn withdraw_heartbeat(
        &mut self,
        claim: HeartbeatClaim,
        error: RuntimeError,
        now: Instant,
    ) -> Result<Option<TerminalBatch>, RuntimeError> {
        self.terminal_unstarted_heartbeat(claim, TerminalKind::Cancelled, error, now)
    }

    fn terminal_unstarted_heartbeat(
        &mut self,
        claim: HeartbeatClaim,
        kind: TerminalKind,
        error: RuntimeError,
        now: Instant,
    ) -> Result<Option<TerminalBatch>, RuntimeError> {
        validate_terminal_payload(kind, true)?;
        if kind == TerminalKind::Completed {
            return Err(self.invariant_error("unstarted heartbeat cannot complete"));
        }
        if self.heartbeats.get(&claim.lease.request_id) != Some(&claim) {
            self.record_stale();
            return Ok(None);
        }
        let identity = RequestWireIdentity {
            engine_epoch: claim.lease.engine_epoch,
            request_id: claim.lease.request_id,
            lease_id: claim.lease.lease_id,
            slot_id: claim.lease.slot_id,
            generation: None,
            message: None,
        };
        let Some(lease) = self.requests.validate_active_terminal(identity) else {
            self.record_stale();
            return Ok(None);
        };
        if lease != claim.lease || !self.admission.validate_active_release(lease, now)? {
            return Err(
                self.invariant_error("unstarted heartbeat lacks exact admission release ownership")
            );
        }
        let release = self.admission.release_active(lease, now)?;
        if !release.released {
            return Err(self.invariant_error("unstarted heartbeat release was rejected"));
        }
        let notification =
            self.requests
                .commit_terminal(claim.lease.request_id, kind, Some(error))?;
        if self.heartbeats.remove(&claim.lease.request_id) != Some(claim) {
            return Err(self.invariant_error("heartbeat claim disappeared during withdrawal"));
        }
        let mut notifications = Vec::with_capacity(release.timed_out.len().saturating_add(1));
        notifications.push(notification);
        let promotion = self.apply_release_outcome(release, &mut notifications)?;
        Ok(Some(TerminalBatch {
            notifications,
            promotion,
        }))
    }

    pub fn begin_request_attempt(
        &mut self,
        request_id: RequestId,
        now: Instant,
    ) -> Result<RequestAttempt, RuntimeError> {
        if self.state != EngineState::Running
            || self.lifecycle_retirements.contains_key(&request_id)
        {
            return Err(RuntimeError::connection_closed(
                "request cannot start after Engine cleanup linearized",
            ));
        }
        self.requests.begin_attempt(request_id, now)
    }

    pub fn mark_business_bytes_sent(&mut self, identity: RequestWireIdentity) -> bool {
        if self.state != EngineState::Running
            || self
                .lifecycle_retirements
                .contains_key(&identity.request_id)
        {
            self.record_stale();
            return false;
        }
        if self.requests.mark_business_bytes_sent(identity) {
            return true;
        }
        self.record_stale();
        false
    }

    pub fn begin_retry(
        &mut self,
        identity: RequestWireIdentity,
        error: RuntimeError,
        retryable: bool,
        now: Instant,
    ) -> Option<RetryDecision> {
        if self.state != EngineState::Running
            || self
                .lifecycle_retirements
                .contains_key(&identity.request_id)
        {
            self.record_stale();
            return None;
        }
        let decision = self.requests.begin_retry(identity, error, retryable, now);
        if decision.is_none() {
            self.record_stale();
        }
        decision
    }

    pub fn finish_retry_retirement(
        &mut self,
        acknowledgement: ReconnectAck,
        now: Instant,
    ) -> Result<Option<RequestAttempt>, RuntimeError> {
        if self
            .lifecycle_retirements
            .contains_key(&acknowledgement.request_id)
        {
            let generation = GenerationIdentity {
                engine_epoch: acknowledgement.engine_epoch,
                slot_id: acknowledgement.slot_id,
                generation: acknowledgement.retired_generation,
            };
            let retired = RetiredGeneration {
                request_id: Some(acknowledgement.request_id),
                next_generation: acknowledgement.next_generation,
            };
            self.finish_lifecycle_retirement(generation, retired, now)?;
            return Ok(None);
        }
        if self.state != EngineState::Running {
            self.record_stale();
            return Ok(None);
        }
        let pin_call = self.pins.call_for_request(acknowledgement.request_id);
        if let Some(call) = pin_call {
            if !self
                .pins
                .validate_advance_generation(call, acknowledgement)?
            {
                self.record_stale();
                return Ok(None);
            }
        }
        let attempt = self
            .requests
            .finish_retry_retirement(acknowledgement, now)?;
        if attempt.is_none() {
            self.record_stale();
        } else if let Some(call) = pin_call {
            if !self.pins.advance_generation(call, acknowledgement)? {
                return Err(self.invariant_error(
                    "pin call rejected an accepted retry generation acknowledgement",
                ));
            }
        }
        Ok(attempt)
    }

    pub fn finish_terminal_retirement(
        &mut self,
        acknowledgement: ReconnectAck,
    ) -> Option<RequestWireIdentity> {
        if self.state != EngineState::Running
            || self
                .lifecycle_retirements
                .contains_key(&acknowledgement.request_id)
        {
            self.record_stale();
            return None;
        }
        let identity = self.requests.finish_terminal_retirement(acknowledgement);
        if identity.is_none() {
            self.record_stale();
        }
        identity
    }

    pub fn continue_connect_after_reconnect(
        &mut self,
        acknowledgement: ReconnectAck,
        now: Instant,
    ) -> Result<Option<RequestAttempt>, RuntimeError> {
        if self
            .lifecycle_retirements
            .contains_key(&acknowledgement.request_id)
        {
            let generation = GenerationIdentity {
                engine_epoch: acknowledgement.engine_epoch,
                slot_id: acknowledgement.slot_id,
                generation: acknowledgement.retired_generation,
            };
            let retired = RetiredGeneration {
                request_id: Some(acknowledgement.request_id),
                next_generation: acknowledgement.next_generation,
            };
            self.finish_lifecycle_retirement(generation, retired, now)?;
            return Ok(None);
        }
        if self.state != EngineState::Running {
            self.record_stale();
            return Ok(None);
        }
        let pin_call = self.pins.call_for_request(acknowledgement.request_id);
        if let Some(call) = pin_call {
            if !self
                .pins
                .validate_advance_generation(call, acknowledgement)?
            {
                self.record_stale();
                return Ok(None);
            }
        }
        let attempt = self
            .requests
            .continue_connect_after_reconnect(acknowledgement, now)?;
        if attempt.is_none() {
            self.record_stale();
        } else if let Some(call) = pin_call {
            if !self.pins.advance_generation(call, acknowledgement)? {
                return Err(
                    self.invariant_error("pin call rejected an accepted reconnect acknowledgement")
                );
            }
        }
        Ok(attempt)
    }

    pub fn finish_lifecycle_retirement(
        &mut self,
        generation: GenerationIdentity,
        retired: RetiredGeneration,
        now: Instant,
    ) -> Result<bool, RuntimeError> {
        let action = match retired.request_id {
            Some(request_id) => self.lifecycle_retirements.get(&request_id),
            None => self.lifecycle_retirements.values().find(|action| {
                action.wire.engine_epoch == generation.engine_epoch
                    && action.wire.slot_id == generation.slot_id
                    && action.wire.generation == Some(generation.generation)
            }),
        }
        .cloned();
        let Some(action) = action else {
            self.record_stale();
            return Ok(false);
        };
        if action.wire.engine_epoch != generation.engine_epoch
            || action.wire.slot_id != generation.slot_id
            || action.wire.generation != Some(generation.generation)
            || retired
                .request_id
                .is_some_and(|request_id| request_id != action.wire.request_id)
            || generation
                .generation
                .get()
                .checked_add(1)
                .is_none_or(|next| next != retired.next_generation.get())
            || !self.requests.validate_terminal_retirement(action.wire)
        {
            self.record_stale();
            return Ok(false);
        }

        let candidate = self
            .requests
            .cleanup_candidate(action.wire.request_id)
            .ok_or_else(|| self.invariant_error("cleanup request lifecycle disappeared"))?;
        let heartbeat = self.heartbeats.get(&action.wire.request_id).copied();
        let pin_call = self.pins.call_for_request(action.wire.request_id);
        if heartbeat.is_some() && pin_call.is_some() {
            return Err(self.invariant_error("cleanup request has two internal owners"));
        }
        match (candidate.ownership, heartbeat, pin_call) {
            (Admission::Active(lease), Some(claim), None) => {
                if claim.lease != lease
                    || claim.candidate.generation.engine_epoch != generation.engine_epoch
                    || claim.candidate.generation.slot_id != generation.slot_id
                    || claim.candidate.generation.generation != generation.generation
                    || !self.admission.validate_active_release(lease, now)?
                {
                    return Err(
                        self.invariant_error("heartbeat cleanup lacks exact retained ownership")
                    );
                }
            }
            (Admission::Pinned(call), None, Some(registered)) if call == registered => {
                let plan = self
                    .pins
                    .plan_terminal(action.wire, now)?
                    .ok_or_else(|| self.invariant_error("pin cleanup terminal plan is missing"))?;
                if plan.call() != call || plan.released_lease().is_none() {
                    return Err(
                        self.invariant_error("pin cleanup did not retain its closing lease")
                    );
                }
                if let Some(lease) = plan.released_lease() {
                    if !self.admission.validate_active_release(lease, now)? {
                        return Err(
                            self.invariant_error("pin cleanup lease cannot be released exactly")
                        );
                    }
                }
            }
            (Admission::Active(lease), None, None) => {
                if !self.admission.validate_active_release(lease, now)? {
                    return Err(
                        self.invariant_error("ordinary cleanup lacks exact active lease ownership")
                    );
                }
            }
            _ => {
                return Err(self.invariant_error(
                    "cleanup request ownership does not match ordinary, pin, or heartbeat owner",
                ));
            }
        }

        let acknowledgement = ReconnectAck {
            engine_epoch: generation.engine_epoch,
            slot_id: generation.slot_id,
            request_id: action.wire.request_id,
            retired_generation: generation.generation,
            next_generation: retired.next_generation,
            next_endpoint_index: 0,
            endpoints_remaining_in_attempt: 0,
        };
        let acknowledged = self
            .requests
            .finish_terminal_retirement(acknowledgement)
            .ok_or_else(|| self.invariant_error("validated cleanup retirement was not accepted"))?;
        if acknowledged != action.wire {
            return Err(self.invariant_error("cleanup retirement acknowledged a different wire"));
        }

        let mut notifications = if heartbeat.is_some() {
            let batch = self
                .terminal_heartbeat(
                    action.wire,
                    action.terminal_kind,
                    None,
                    Some(action.error.clone()),
                    now,
                )?
                .ok_or_else(|| self.invariant_error("heartbeat cleanup terminal was rejected"))?;
            if batch.promotion.is_some() {
                return Err(self.invariant_error("sealed heartbeat cleanup promoted a waiter"));
            }
            batch.notifications
        } else if pin_call.is_some() {
            let batch = self
                .terminal_pin(
                    action.wire,
                    action.terminal_kind,
                    Some(action.error.clone()),
                    now,
                )?
                .ok_or_else(|| self.invariant_error("pin cleanup terminal was rejected"))?;
            if batch.pin_promotion.is_some() || batch.ordinary_promotion.is_some() {
                return Err(self.invariant_error("sealed pin cleanup promoted a waiter"));
            }
            batch.notifications
        } else {
            let batch = self
                .terminal_active(
                    action.wire,
                    action.terminal_kind,
                    Some(action.error.clone()),
                    now,
                )?
                .ok_or_else(|| self.invariant_error("ordinary cleanup terminal was rejected"))?;
            if batch.promotion.is_some() {
                return Err(self.invariant_error("sealed ordinary cleanup promoted a waiter"));
            }
            batch.notifications
        };
        if self.lifecycle_retirements.remove(&action.wire.request_id) != Some(action) {
            return Err(self.invariant_error("cleanup retirement owner changed during terminal"));
        }
        self.lifecycle_notifications.append(&mut notifications);
        Ok(true)
    }

    pub fn request_attempt_count(&self, request_id: RequestId) -> Option<u8> {
        self.requests.attempt_count(request_id)
    }

    pub fn request_attempt_deadline(&self, request_id: RequestId) -> Option<Deadline> {
        self.requests.attempt_deadline(request_id)
    }

    pub fn last_retry_error(&self, request_id: RequestId) -> Option<&RuntimeError> {
        self.requests.last_retry_error(request_id)
    }

    pub fn transition_request(
        &mut self,
        request_id: RequestId,
        next: RequestState,
        wire: Option<RequestWireIdentity>,
    ) -> Result<(), RuntimeError> {
        if self.state != EngineState::Running
            || self.lifecycle_retirements.contains_key(&request_id)
        {
            return Err(RuntimeError::connection_closed(
                "request cannot advance after Engine cleanup linearized",
            ));
        }
        let pin_call = self.pins.call_for_request(request_id);
        if let (Some(call), Some(identity)) = (pin_call, wire) {
            if !self.pins.validate_bind_wire(call, identity)? {
                return Err(
                    self.invariant_error("pin registry rejected request wire prevalidation")
                );
            }
        }
        self.requests.transition(request_id, next, wire)?;
        if let (Some(call), Some(identity)) = (pin_call, wire) {
            if !self.pins.bind_wire(call, identity)? {
                return Err(self
                    .invariant_error("pin registry rejected an accepted request wire transition"));
            }
        }
        Ok(())
    }

    pub fn terminal_waiting(
        &mut self,
        permit: WaitingPermit,
        kind: TerminalKind,
        error: Option<RuntimeError>,
    ) -> Result<Option<TerminalNotification>, RuntimeError> {
        validate_terminal_payload(kind, error.is_some())?;
        if kind == TerminalKind::Completed {
            return Err(
                self.invariant_error("queued request cannot complete without an active lease")
            );
        }
        if !self.requests.validate_waiting_terminal(permit)
            || !self.admission.cancel_waiting(permit)
        {
            self.record_stale();
            return Ok(None);
        }
        self.requests
            .commit_terminal(permit.request_id, kind, error)
            .map(Some)
    }

    pub fn terminal_active(
        &mut self,
        identity: RequestWireIdentity,
        kind: TerminalKind,
        error: Option<RuntimeError>,
        now: Instant,
    ) -> Result<Option<TerminalBatch>, RuntimeError> {
        validate_terminal_payload(kind, error.is_some())?;
        if self.heartbeats.contains_key(&identity.request_id) {
            return Err(
                self.invariant_error("internal heartbeat must use its dedicated terminal reducer")
            );
        }
        let Some(lease) = self.requests.validate_active_terminal(identity) else {
            self.record_stale();
            return Ok(None);
        };
        let release = self.admission.release_active(lease, now)?;
        if !release.released {
            self.record_stale();
            return Ok(None);
        }
        let mut notifications = Vec::with_capacity(release.timed_out.len().saturating_add(1));
        notifications.push(
            self.requests
                .commit_terminal(identity.request_id, kind, error)?,
        );
        for permit in &release.timed_out {
            if !self.requests.validate_waiting_terminal(*permit) {
                return Err(self
                    .invariant_error("expired waiting permit has no matching request lifecycle"));
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                TerminalKind::TimedOut,
                Some(RuntimeError::timeout(crate::error::TimeoutPhase::Queue)),
            )?);
        }
        if let Some(promotion) = release.promotion {
            self.requests.promote(promotion)?;
        }
        Ok(Some(TerminalBatch {
            notifications,
            promotion: release.promotion,
        }))
    }

    pub fn terminal_heartbeat(
        &mut self,
        identity: RequestWireIdentity,
        kind: TerminalKind,
        acknowledgement: Option<HeartbeatAck>,
        error: Option<RuntimeError>,
        now: Instant,
    ) -> Result<Option<TerminalBatch>, RuntimeError> {
        validate_terminal_payload(kind, error.is_some())?;
        if matches!(kind, TerminalKind::Completed) != acknowledgement.is_some() {
            return Err(self.invariant_error(
                "completed heartbeat requires an acknowledgement and failed heartbeat forbids one",
            ));
        }
        let Some(claim) = self.heartbeats.get(&identity.request_id).copied() else {
            self.record_stale();
            return Ok(None);
        };
        if claim.lease.engine_epoch != identity.engine_epoch
            || claim.lease.request_id != identity.request_id
            || claim.lease.lease_id != identity.lease_id
            || claim.lease.slot_id != identity.slot_id
            || claim.candidate.generation.engine_epoch != identity.engine_epoch
            || claim.candidate.generation.slot_id != identity.slot_id
            || identity.generation != Some(claim.candidate.generation.generation)
        {
            self.record_stale();
            return Ok(None);
        }
        let Some(lease) = self.requests.validate_active_terminal(identity) else {
            self.record_stale();
            return Ok(None);
        };
        if lease != claim.lease || !self.admission.validate_active_release(lease, now)? {
            return Err(
                self.invariant_error("heartbeat terminal lacks exact admission release ownership")
            );
        }
        let release = self.admission.release_active(lease, now)?;
        if !release.released {
            return Err(self.invariant_error("validated heartbeat lease release was rejected"));
        }
        let notification = self
            .requests
            .commit_terminal(identity.request_id, kind, error)?;
        if self.heartbeats.remove(&identity.request_id) != Some(claim) {
            return Err(self.invariant_error("heartbeat claim disappeared during terminal commit"));
        }
        if let Some(value) = acknowledgement {
            self.last_heartbeats.insert(identity.slot_id, value);
        }
        let mut notifications = Vec::with_capacity(release.timed_out.len().saturating_add(1));
        notifications.push(notification);
        let promotion = self.apply_release_outcome(release, &mut notifications)?;
        Ok(Some(TerminalBatch {
            notifications,
            promotion,
        }))
    }

    pub fn terminal_pin(
        &mut self,
        identity: RequestWireIdentity,
        kind: TerminalKind,
        error: Option<RuntimeError>,
        now: Instant,
    ) -> Result<Option<PinTerminalBatch>, RuntimeError> {
        validate_terminal_payload(kind, error.is_some())?;
        let Some(call) = self.requests.validate_pinned_terminal(identity) else {
            self.record_stale();
            return Ok(None);
        };
        let Some(plan) = self.pins.plan_terminal(identity, now)? else {
            self.record_stale();
            return Ok(None);
        };
        if plan.call() != call {
            self.record_stale();
            return Ok(None);
        }
        for permit in plan.expired() {
            if !self.admission.validate_pin_waiting(*permit)
                || !self.requests.validate_waiting_terminal(*permit)
            {
                return Err(
                    self.invariant_error("expired pin-local waiter lacks exact permit ownership")
                );
            }
        }
        if let Some(permit) = plan.promotion() {
            if !self.admission.validate_pin_waiting(permit)
                || !self.requests.validate_waiting_terminal(permit)
            {
                return Err(
                    self.invariant_error("promoted pin-local waiter lacks exact permit ownership")
                );
            }
        }
        if let Some(lease) = plan.released_lease() {
            if !self.admission.validate_active_release(lease, now)? {
                return Err(
                    self.invariant_error("terminal pin release lacks exact active lease ownership")
                );
            }
        }
        let outcome = self
            .pins
            .commit_terminal(&plan)?
            .ok_or_else(|| self.invariant_error("validated pin terminal plan became stale"))?;
        let mut notifications = Vec::with_capacity(outcome.expired.len().saturating_add(1));
        notifications.push(
            self.requests
                .commit_terminal(identity.request_id, kind, error)?,
        );
        for permit in &outcome.expired {
            if !self.admission.release_pin_waiting(*permit) {
                return Err(self.invariant_error("expired pin-local permit could not be returned"));
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                TerminalKind::TimedOut,
                Some(RuntimeError::timeout(crate::error::TimeoutPhase::Pin)),
            )?);
        }
        let pin_promotion = if let Some((permit, promoted)) = outcome.promotion {
            if !self.admission.release_pin_waiting(permit) {
                return Err(self.invariant_error("promoted pin-local permit could not be returned"));
            }
            self.requests.promote_pinned(permit, promoted)?;
            Some(promoted)
        } else {
            None
        };
        let mut ordinary_promotion = None;
        let pin_released = if let Some(lease) = outcome.released_lease {
            let release = self.admission.release_active(lease, now)?;
            if !release.released {
                return Err(self.invariant_error("closed pin lease was not admission-owned"));
            }
            ordinary_promotion = self.apply_release_outcome(release, &mut notifications)?;
            true
        } else {
            false
        };
        Ok(Some(PinTerminalBatch {
            notifications,
            pin_promotion,
            ordinary_promotion,
            pin_released,
        }))
    }

    pub fn close_pin(
        &mut self,
        identity: PinIdentity,
        now: Instant,
    ) -> Result<Option<PinTerminalBatch>, RuntimeError> {
        if !self.validate_pin_close_ownership(identity)? {
            self.record_stale();
            return Ok(None);
        }
        if self.pins.releases_on_close(identity) == Some(true) {
            let lease = self
                .pins
                .lease(identity)
                .ok_or_else(|| self.invariant_error("closing pin lost its active lease"))?;
            if !self.admission.validate_active_release(lease, now)? {
                return Err(
                    self.invariant_error("closing pin lease cannot be released transactionally")
                );
            }
        }
        let Some(outcome) = self.pins.begin_close(identity)? else {
            self.record_stale();
            return Ok(None);
        };
        let withdrawn_count = if outcome.withdrawn_unstarted.is_some() {
            1
        } else {
            0
        };
        let mut notifications =
            Vec::with_capacity(outcome.rejected.len().saturating_add(withdrawn_count));
        for permit in outcome.rejected {
            if !self.admission.validate_pin_waiting(permit)
                || !self.requests.validate_waiting_terminal(permit)
            {
                return Err(
                    self.invariant_error("closed pin-local waiter lacks exact permit ownership")
                );
            }
            if !self.admission.release_pin_waiting(permit) {
                return Err(self.invariant_error("closed pin-local permit could not be returned"));
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                TerminalKind::Cancelled,
                Some(RuntimeError::connection_closed(
                    "pinned proxy closed during admission",
                )),
            )?);
        }
        if let Some(call) = outcome.withdrawn_unstarted {
            let unstarted = RequestWireIdentity {
                engine_epoch: call.pin.engine_epoch,
                request_id: call.request_id,
                lease_id: call.pin.lease_id,
                slot_id: call.pin.slot_id,
                generation: None,
                message: None,
            };
            if self.requests.validate_pinned_terminal(unstarted) != Some(call) {
                return Err(
                    self.invariant_error("withdrawn pin call lacks exact request ownership")
                );
            }
            notifications.push(self.requests.commit_terminal(
                call.request_id,
                TerminalKind::Cancelled,
                Some(RuntimeError::connection_closed(
                    "pinned proxy closed before wire submission",
                )),
            )?);
        }
        let mut ordinary_promotion = None;
        let pin_released = if let Some(lease) = outcome.released_lease {
            let release = self.admission.release_active(lease, now)?;
            if !release.released {
                return Err(self.invariant_error("closed pin lease was not admission-owned"));
            }
            ordinary_promotion = self.apply_release_outcome(release, &mut notifications)?;
            true
        } else {
            false
        };
        Ok(Some(PinTerminalBatch {
            notifications,
            pin_promotion: None,
            ordinary_promotion,
            pin_released,
        }))
    }

    pub fn terminal_pin_waiting(
        &mut self,
        identity: PinIdentity,
        permit: WaitingPermit,
        kind: TerminalKind,
        error: RuntimeError,
    ) -> Result<Option<TerminalNotification>, RuntimeError> {
        validate_terminal_payload(kind, true)?;
        if kind == TerminalKind::Completed {
            return Err(self.invariant_error("queued pin request cannot complete before promotion"));
        }
        if !self.pins.validate_waiting(identity, permit)
            || !self.admission.validate_pin_waiting(permit)
            || !self.requests.validate_waiting_terminal(permit)
        {
            self.record_stale();
            return Ok(None);
        }
        if !self.pins.cancel_waiting(identity, permit)? {
            return Err(self.invariant_error("validated pin-local FIFO cancellation was rejected"));
        }
        if !self.admission.release_pin_waiting(permit) {
            return Err(self.invariant_error("validated pin-local permit could not be returned"));
        }
        self.requests
            .commit_terminal(permit.request_id, kind, Some(error))
            .map(Some)
    }

    pub fn terminal_rejected_waiters(
        &mut self,
        kind: TerminalKind,
        error: RuntimeError,
    ) -> Result<Vec<TerminalNotification>, RuntimeError> {
        validate_terminal_payload(kind, true)?;
        let permits = self.take_rejected_waiters();
        let mut notifications = Vec::with_capacity(permits.len());
        for permit in permits {
            if !self.requests.validate_waiting_terminal(permit) {
                return Err(self
                    .invariant_error("rejected waiting permit has no matching request lifecycle"));
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                kind,
                Some(error.clone()),
            )?);
        }
        let pin_calls = std::mem::take(&mut self.rejected_pin_calls);
        notifications.reserve(pin_calls.len());
        for call in pin_calls {
            let identity = RequestWireIdentity {
                engine_epoch: call.pin.engine_epoch,
                request_id: call.request_id,
                lease_id: call.pin.lease_id,
                slot_id: call.pin.slot_id,
                generation: None,
                message: None,
            };
            if self.requests.validate_pinned_terminal(identity) != Some(call) {
                return Err(
                    self.invariant_error("rejected unstarted pin call has no request lifecycle")
                );
            }
            notifications.push(self.requests.commit_terminal(
                call.request_id,
                kind,
                Some(error.clone()),
            )?);
        }
        Ok(notifications)
    }

    pub fn expire_waiting_terminals(
        &mut self,
        now: Instant,
    ) -> Result<Vec<TerminalNotification>, RuntimeError> {
        let permits = self.admission.expire_waiting(now)?;
        let mut notifications = Vec::with_capacity(permits.len());
        for permit in permits {
            if !self.requests.validate_waiting_terminal(permit) {
                return Err(
                    self.invariant_error("expired permit has no matching request lifecycle")
                );
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                TerminalKind::TimedOut,
                Some(RuntimeError::timeout(crate::error::TimeoutPhase::Queue)),
            )?);
        }
        for (identity, permit) in self.pins.expired_waiters(now) {
            let notification = self
                .terminal_pin_waiting(
                    identity,
                    permit,
                    TerminalKind::TimedOut,
                    RuntimeError::timeout(crate::error::TimeoutPhase::Pin),
                )?
                .ok_or_else(|| {
                    self.invariant_error("snapshotted expired pin-local waiter became stale")
                })?;
            notifications.push(notification);
        }
        Ok(notifications)
    }

    fn validate_pin_close_ownership(&self, identity: PinIdentity) -> Result<bool, RuntimeError> {
        if !self.pins.validate_begin_close(identity)? {
            return Ok(false);
        }
        let lease = self
            .pins
            .lease(identity)
            .ok_or_else(|| self.invariant_error("validated pin has no active lease"))?;
        if self.admission.admission_for(lease.request_id) != Some(Admission::Active(lease)) {
            return Err(self.invariant_error("pin close lacks exact active lease ownership"));
        }
        let waiters = self
            .pins
            .waiting_for(identity)
            .ok_or_else(|| self.invariant_error("validated pin has no waiting snapshot"))?;
        for permit in waiters {
            if !self.admission.validate_pin_waiting(permit)
                || !self.requests.validate_waiting_terminal(permit)
            {
                return Err(self.invariant_error(
                    "pin close waiter lacks exact admission and request ownership",
                ));
            }
        }
        let active = self
            .pins
            .active_owner(identity)
            .ok_or_else(|| self.invariant_error("validated pin has no active-call snapshot"))?;
        if let Some((call, wire)) = active {
            if !self.requests.matches_pinned_owner(call, wire) {
                return Err(
                    self.invariant_error("pin close active call lacks exact request ownership")
                );
            }
        }
        Ok(true)
    }

    fn seal_and_close_pins(&mut self, now: Instant) -> Result<(), RuntimeError> {
        let identities = self.pins.identities();
        for identity in &identities {
            if !self.validate_pin_close_ownership(*identity)? {
                return Err(self.invariant_error("current pin disappeared before close seal"));
            }
        }
        let mut withdrawn_pin_calls = Vec::new();
        let mut idle_pin_leases = Vec::new();
        for identity in identities {
            let outcome = self
                .pins
                .begin_close(identity)?
                .ok_or_else(|| self.invariant_error("current pin disappeared during close seal"))?;
            if let Some(call) = outcome.withdrawn_unstarted {
                withdrawn_pin_calls.push(call);
            }
            if let Some(lease) = outcome.released_lease {
                idle_pin_leases.push(lease);
            }
        }
        let rejected = self.admission.seal()?;
        for lease in idle_pin_leases {
            let release = self.admission.release_active(lease, now)?;
            if !release.released || release.promotion.is_some() || !release.timed_out.is_empty() {
                return Err(self.invariant_error(
                    "sealed close did not release an idle pin lease exactly once",
                ));
            }
        }
        self.rejected_waiters.extend(rejected);
        self.rejected_pin_calls.extend(withdrawn_pin_calls);
        Ok(())
    }

    fn begin_lifecycle_cleanup(
        &mut self,
        kind: TerminalKind,
        error: RuntimeError,
        now: Instant,
    ) -> Result<(), RuntimeError> {
        validate_terminal_payload(kind, true)?;
        if kind == TerminalKind::Completed {
            return Err(self.invariant_error("lifecycle cleanup cannot complete a request"));
        }

        let mut rejected = self.terminal_rejected_waiters(kind, error.clone())?;
        self.lifecycle_notifications.append(&mut rejected);
        let candidates = self.requests.cleanup_candidates();
        for candidate in candidates {
            let request_id = candidate.request_id();
            if let Some(existing) = self.lifecycle_retirements.get_mut(&request_id) {
                if kind == TerminalKind::Failed && !existing.preserve_prior_terminal {
                    existing.terminal_kind = kind;
                    existing.error = error.clone();
                }
                continue;
            }
            if let Some(wire) = candidate.wire {
                let prior_terminal_error = self.requests.terminal_retirement_error(wire).cloned();
                if !self.requests.begin_terminal_retirement(wire) {
                    return Err(self.invariant_error(
                        "started lifecycle cleanup request rejected terminal retirement",
                    ));
                }
                let preserve_prior_terminal = prior_terminal_error.is_some();
                let (terminal_kind, terminal_error) = match prior_terminal_error {
                    Some(previous @ RuntimeError::Timeout { .. }) => {
                        (TerminalKind::TimedOut, previous)
                    }
                    Some(previous) => (TerminalKind::Failed, previous),
                    None => (kind, error.clone()),
                };
                self.lifecycle_retirements.insert(
                    request_id,
                    LifecycleRetirement {
                        wire,
                        terminal_kind,
                        error: terminal_error,
                        preserve_prior_terminal,
                    },
                );
                continue;
            }

            let Admission::Active(lease) = candidate.ownership else {
                return Err(self.invariant_error(
                    "sealed unstarted request retained waiting or pinned ownership",
                ));
            };
            if candidate.state != RequestState::Assigned {
                return Err(
                    self.invariant_error("unstarted lifecycle cleanup request is not assigned")
                );
            }
            let identity = RequestWireIdentity {
                engine_epoch: lease.engine_epoch,
                request_id: lease.request_id,
                lease_id: lease.lease_id,
                slot_id: lease.slot_id,
                generation: None,
                message: None,
            };
            let batch = if let Some(claim) = self.heartbeats.get(&request_id).copied() {
                self.terminal_unstarted_heartbeat(claim, kind, error.clone(), now)?
                    .ok_or_else(|| {
                        self.invariant_error("unstarted heartbeat cleanup was rejected")
                    })?
            } else {
                self.terminal_active(identity, kind, Some(error.clone()), now)?
                    .ok_or_else(|| {
                        self.invariant_error("unstarted ordinary cleanup was rejected")
                    })?
            };
            if batch.promotion.is_some() {
                return Err(self.invariant_error("sealed cleanup promoted an ordinary waiter"));
            }
            self.lifecycle_notifications.extend(batch.notifications);
        }
        Ok(())
    }

    fn apply_release_outcome(
        &mut self,
        release: ReleaseOutcome,
        notifications: &mut Vec<TerminalNotification>,
    ) -> Result<Option<Promotion>, RuntimeError> {
        for permit in &release.timed_out {
            if !self.requests.validate_waiting_terminal(*permit) {
                return Err(
                    self.invariant_error("expired ordinary waiter has no request lifecycle")
                );
            }
            notifications.push(self.requests.commit_terminal(
                permit.request_id,
                TerminalKind::TimedOut,
                Some(RuntimeError::timeout(crate::error::TimeoutPhase::Queue)),
            )?);
        }
        if let Some(promotion) = release.promotion {
            self.requests.promote(promotion)?;
        }
        Ok(release.promotion)
    }

    fn take_rejected_waiters(&mut self) -> Vec<WaitingPermit> {
        std::mem::take(&mut self.rejected_waiters)
    }

    pub fn waiting_count(&self) -> usize {
        self.admission.waiting_count()
    }

    pub fn rejected_waiter_count(&self) -> usize {
        self.rejected_waiters
            .len()
            .saturating_add(self.rejected_pin_calls.len())
    }

    pub fn active_count(&self) -> usize {
        self.admission
            .active_count()
            .saturating_sub(self.heartbeats.len())
    }

    pub fn total_admission_owned(&self) -> usize {
        self.admission
            .total_owned()
            .saturating_sub(self.heartbeats.len())
    }

    pub fn offer_push(&mut self, frame: PushFrame) -> bool {
        if self.state != EngineState::Running || self.active_epoch != Some(frame.engine_epoch) {
            self.record_stale();
            return false;
        }
        self.push.as_mut().is_some_and(|buffer| buffer.offer(frame))
    }

    pub fn record_push_drop(&mut self, epoch: EngineEpoch, count: u64) -> bool {
        if self.state != EngineState::Running || self.active_epoch != Some(epoch) {
            self.record_stale();
            return false;
        }
        self.push
            .as_mut()
            .is_some_and(|buffer| buffer.record_external_drop(epoch, count))
    }

    pub fn poll_push(&mut self) -> Result<Option<PushFrame>, RuntimeError> {
        match self.push.as_mut() {
            Some(buffer) => buffer.poll(),
            None => Ok(None),
        }
    }

    pub fn drain_pushes(&mut self) -> Result<Vec<PushFrame>, RuntimeError> {
        match self.push.as_mut() {
            Some(buffer) => buffer.drain(),
            None => Ok(Vec::new()),
        }
    }

    pub fn push_snapshot(&self) -> Option<PushBufferSnapshot> {
        self.push.as_ref().map(PushBuffer::snapshot)
    }

    pub fn check_admission_invariants(&self) -> Result<(), RuntimeError> {
        self.admission.check_invariants()?;
        self.pins.check_invariants()?;
        self.requests.check_matches_admission_with_pins(
            &self.admission,
            &self.pins.active_pin_leases(),
            &self.pins.waiting_permits(),
            &self.pins.active_calls(),
        )?;
        if self.heartbeats.len() > self.pool_size || self.last_heartbeats.len() > self.pool_size {
            return Err(self.invariant_error("heartbeat state exceeds configured Slot count"));
        }
        if let Some(push) = &self.push {
            push.check_invariants()?;
            let snapshot = push.snapshot();
            if self.event_epoch() != Some(snapshot.owner_epoch)
                && !matches!(
                    self.state,
                    EngineState::Closing | EngineState::Failed | EngineState::FailedClosing
                )
            {
                return Err(self.invariant_error("push buffer belongs to a stale engine epoch"));
            }
            if self.state == EngineState::Running && snapshot.closed {
                return Err(self.invariant_error("running Engine has a closed push buffer"));
            }
            if matches!(
                self.state,
                EngineState::Closing | EngineState::Failed | EngineState::FailedClosing
            ) && !snapshot.closed
            {
                return Err(
                    self.invariant_error("closing or failed Engine has an open push buffer")
                );
            }
        } else if self.state == EngineState::Running {
            return Err(self.invariant_error("running Engine has no push buffer"));
        }
        let mut heartbeat_slots = BTreeMap::new();
        for (request_id, claim) in &self.heartbeats {
            if *request_id != claim.lease.request_id
                || claim.candidate.generation.engine_epoch != claim.lease.engine_epoch
                || claim.candidate.generation.slot_id != claim.lease.slot_id
                || self.admission.admission_for(*request_id) != Some(Admission::Active(claim.lease))
                || !self.requests.contains(*request_id)
            {
                return Err(self.invariant_error(
                    "heartbeat claim does not match request and admission ownership",
                ));
            }
            if heartbeat_slots
                .insert(claim.lease.slot_id, *request_id)
                .is_some()
            {
                return Err(self.invariant_error("multiple heartbeats alias one Slot"));
            }
            if self
                .pins
                .identities()
                .iter()
                .any(|pin| pin.slot_id == claim.lease.slot_id)
            {
                return Err(self.invariant_error("heartbeat claim aliases a pinned Slot"));
            }
        }
        let mut cleanup_slots = BTreeMap::new();
        for (request_id, action) in &self.lifecycle_retirements {
            if *request_id != action.wire.request_id
                || !self.requests.validate_terminal_retirement(action.wire)
                || action.wire.generation.is_none()
                || matches!(action.terminal_kind, TerminalKind::Completed)
            {
                return Err(self.invariant_error(
                    "lifecycle retirement does not match its exact request owner",
                ));
            }
            if cleanup_slots
                .insert(action.wire.slot_id, action.wire.request_id)
                .is_some()
            {
                return Err(self.invariant_error("multiple lifecycle retirements alias one Slot"));
            }
        }
        if !self.lifecycle_retirements.is_empty()
            && (self.state == EngineState::Running || !self.admission.is_sealed())
        {
            return Err(self
                .invariant_error("lifecycle cleanup ownership exists before admission is sealed"));
        }
        for notification in &self.lifecycle_notifications {
            if self.requests.contains(notification.request_id) {
                return Err(
                    self.invariant_error("terminal notification retained a live request lifecycle")
                );
            }
        }
        Ok(())
    }

    pub fn register_slot(
        &mut self,
        epoch: EngineEpoch,
        slot_id: SlotId,
    ) -> Result<bool, RuntimeError> {
        if slot_id.get() >= self.pool_size {
            return Err(self
                .invariant_error("Slot id is outside configured pool size")
                .with_context("slot_id", slot_id.get().to_string())
                .with_context("pool_size", self.pool_size.to_string()));
        }
        if self.event_epoch() != Some(epoch)
            || !matches!(self.state, EngineState::Starting | EngineState::Running)
        {
            self.record_stale();
            return Ok(false);
        }
        match self.instantiated_slots.binary_search(&slot_id) {
            Ok(_) => {}
            Err(index) => self.instantiated_slots.insert(index, slot_id),
        }
        Ok(true)
    }

    pub fn retire_slot(&mut self, epoch: EngineEpoch, slot_id: SlotId) -> bool {
        if self.event_epoch() != Some(epoch)
            || !matches!(
                self.state,
                EngineState::Closing | EngineState::Failed | EngineState::FailedClosing
            )
        {
            self.record_stale();
            return false;
        }
        if let Ok(index) = self.instantiated_slots.binary_search(&slot_id) {
            self.instantiated_slots.remove(index);
            return true;
        }
        self.record_stale();
        false
    }

    fn event_epoch(&self) -> Option<EngineEpoch> {
        match self.state {
            EngineState::Starting => self.start_attempt.map(|attempt| attempt.candidate_epoch),
            EngineState::Running
            | EngineState::Closing
            | EngineState::Failed
            | EngineState::FailedClosing => self
                .active_epoch
                .or_else(|| self.start_attempt.map(|attempt| attempt.candidate_epoch)),
            EngineState::Stopped | EngineState::FailedClosed => None,
        }
    }

    fn next_attempt_id(&mut self) -> Result<AttemptId, RuntimeError> {
        let next = next_identity(self.attempt_counter, "attempt id")?;
        self.attempt_counter = next;
        Ok(AttemptId(next))
    }

    fn close_push(
        &mut self,
        target_epoch: Option<EngineEpoch>,
        fatal: Option<RuntimeError>,
    ) -> Result<(), RuntimeError> {
        let Some(epoch) = target_epoch else {
            return Ok(());
        };
        let Some(push) = self.push.as_mut() else {
            if self.state == EngineState::Starting || self.start_attempt.is_some() {
                return Ok(());
            }
            return Err(self.invariant_error("active engine epoch has no push buffer to close"));
        };
        if !push.close(epoch, fatal) {
            return Err(self.invariant_error("push close rejected the active engine epoch"));
        }
        Ok(())
    }

    fn prevalidate_lifecycle_cleanup(
        &self,
        target_epoch: Option<EngineEpoch>,
    ) -> Result<(), RuntimeError> {
        self.check_admission_invariants()?;
        let Some(epoch) = target_epoch else {
            return Ok(());
        };
        match self.push.as_ref() {
            Some(push) if push.snapshot().owner_epoch == epoch => Ok(()),
            None if self.state == EngineState::Starting || self.start_attempt.is_some() => Ok(()),
            Some(_) => Err(self.invariant_error("push buffer belongs to a different engine epoch")),
            None => Err(self.invariant_error("active engine epoch has no push buffer to close")),
        }
    }

    fn record_stale(&mut self) {
        self.stale_event_count = self.stale_event_count.saturating_add(1);
    }

    fn invariant_error(&self, message: impl Into<String>) -> RuntimeError {
        RuntimeError::internal(message)
            .with_context("engine_state", self.state.as_str())
            .with_context("engine_epoch", self.epoch_counter.to_string())
    }
}

fn next_identity(current: u64, name: &'static str) -> Result<u64, RuntimeError> {
    current
        .checked_add(1)
        .ok_or_else(|| RuntimeError::internal(format!("{name} identity space exhausted")))
}

fn validate_terminal_payload(kind: TerminalKind, has_error: bool) -> Result<(), RuntimeError> {
    if matches!(kind, TerminalKind::Completed) == has_error {
        return Err(RuntimeError::internal(
            "completed terminal must not have an error and failed terminals must have one",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use bytes::Bytes;
    use eltdx_protocol::commands::session::HeartbeatAck;
    use eltdx_protocol::frame::{ResponseFrame, ResponseHeader, RESPONSE_PREFIX};
    use proptest::prelude::*;

    use super::{CloseClaim, EngineState, StartClaim, Supervisor};
    use crate::deadline::Deadline;
    use crate::endpoint::{Endpoint, EndpointRotation};
    use crate::error::RuntimeError;
    use crate::push::PushFrame;
    use crate::request::{
        Admission, RequestState, RequestWireIdentity, RetryDecision, RetryPolicy, RetryStopReason,
        TerminalKind,
    };
    use crate::slot::{
        EngineEpoch, GenerationId, GenerationIdentity, HeartbeatCandidate, MessageIdentity,
        ReconnectAck, RequestId, RetiredGeneration, Slot, SlotId,
    };

    fn start(supervisor: &mut Supervisor) -> Result<EngineEpoch, RuntimeError> {
        let attempt = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start ownership, got {other:?}"
                )))
            }
        };
        let epoch = attempt.candidate_epoch();
        if !supervisor.publish_start(attempt)? {
            return Err(RuntimeError::internal("start publication was rejected"));
        }
        Ok(epoch)
    }

    fn close_owner(supervisor: &mut Supervisor) -> Result<super::CloseAttempt, RuntimeError> {
        match supervisor.begin_close()? {
            CloseClaim::Owner(attempt) => Ok(attempt),
            other => Err(RuntimeError::internal(format!(
                "expected close ownership, got {other:?}"
            ))),
        }
    }

    fn runtime_slot(epoch: EngineEpoch) -> Result<Slot, RuntimeError> {
        Slot::new(
            epoch,
            SlotId::new(0),
            EndpointRotation::new(
                vec![
                    Endpoint::numeric("127.0.0.1:7709")?,
                    Endpoint::numeric("127.0.0.2:7709")?,
                ],
                0,
            )?,
        )
    }

    fn push_frame(epoch: EngineEpoch, message: u32) -> Result<PushFrame, RuntimeError> {
        let header = ResponseHeader {
            control: 0,
            msg_id: message,
            reserved: 0,
            msg_type: 0x0547,
            zip_length: 1,
            length: 1,
        };
        let mut raw = vec![0_u8; 17];
        raw[..4].copy_from_slice(&RESPONSE_PREFIX);
        raw[5..9].copy_from_slice(&message.to_le_bytes());
        raw[10..12].copy_from_slice(&0x0547_u16.to_le_bytes());
        raw[12..14].copy_from_slice(&1_u16.to_le_bytes());
        raw[14..16].copy_from_slice(&1_u16.to_le_bytes());
        Ok(PushFrame {
            engine_epoch: epoch,
            slot_id: SlotId::new(0),
            generation: GenerationId::new(1)?,
            connected_host: Arc::from("127.0.0.1:7709"),
            response: ResponseFrame::from_decoded(
                header,
                Bytes::from_static(&[1]),
                Bytes::from(raw),
            )?,
        })
    }

    fn active_lease(admission: Admission) -> Result<crate::request::ActiveLease, RuntimeError> {
        match admission {
            Admission::Active(lease) => Ok(lease),
            Admission::Waiting(_) => Err(RuntimeError::internal("expected active lease")),
            Admission::Pinned(_) => Err(RuntimeError::internal("expected active lease")),
        }
    }

    fn pinned_call(admission: Admission) -> Result<crate::pin::PinnedCallLease, RuntimeError> {
        match admission {
            Admission::Pinned(call) => Ok(call),
            Admission::Active(_) | Admission::Waiting(_) => {
                Err(RuntimeError::internal("expected pinned call lease"))
            }
        }
    }

    fn waiting_permit(admission: Admission) -> Result<crate::request::WaitingPermit, RuntimeError> {
        match admission {
            Admission::Waiting(permit) => Ok(permit),
            Admission::Active(_) | Admission::Pinned(_) => {
                Err(RuntimeError::internal("expected waiting permit"))
            }
        }
    }

    #[test]
    fn normal_close_reopens_with_a_fresh_candidate_epoch() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(2)?;
        let first_epoch = start(&mut supervisor)?;
        let close = close_owner(&mut supervisor)?;

        assert_eq!(supervisor.state(), EngineState::Closing);
        assert!(close.invalidated_epoch() > first_epoch.get());
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);

        let next = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected new start owner, got {other:?}"
                )))
            }
        };
        assert!(next.candidate_epoch().get() > close.invalidated_epoch());
        Ok(())
    }

    #[test]
    fn close_during_starting_rejects_late_publication() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let start = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        let close = close_owner(&mut supervisor)?;

        assert!(!supervisor.publish_start(start)?);
        assert_eq!(supervisor.state(), EngineState::Closing);
        assert_eq!(supervisor.stale_event_count(), 1);
        assert!(supervisor.fail_start(
            start,
            RuntimeError::connection_closed("startup cancelled by close"),
            true,
        ));
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn concurrent_start_claims_share_one_identity() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let owner = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        let existing = match supervisor.begin_start()? {
            StartClaim::Existing(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected existing start, got {other:?}"
                )))
            }
        };

        assert_eq!(owner, existing);
        assert!(supervisor.publish_start(owner)?);
        assert_eq!(supervisor.state(), EngineState::Running);
        Ok(())
    }

    #[test]
    fn startup_cleanup_cannot_undo_an_already_linearized_close() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let start = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        let close = close_owner(&mut supervisor)?;

        assert!(supervisor.fail_start(
            start,
            RuntimeError::connection_closed("startup cancelled by close"),
            true,
        ));
        assert_eq!(supervisor.state(), EngineState::Closing);
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn concurrent_close_claims_share_one_identity() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        start(&mut supervisor)?;
        let owner = close_owner(&mut supervisor)?;
        let existing = match supervisor.begin_close()? {
            CloseClaim::Existing(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected existing close, got {other:?}"
                )))
            }
        };

        assert_eq!(owner, existing);
        assert!(supervisor.finish_close(owner, Ok(())));
        Ok(())
    }

    #[test]
    fn close_timeout_creates_permanent_failed_lineage() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        start(&mut supervisor)?;
        let first_close = close_owner(&mut supervisor)?;
        let timeout = RuntimeError::CloseTimeout {
            message: "7709 close timed out".to_owned(),
            context: Vec::new(),
        };
        assert!(supervisor.finish_close(first_close, Err(timeout)));
        assert_eq!(supervisor.state(), EngineState::FailedClosing);

        let retry = close_owner(&mut supervisor)?;
        assert!(supervisor.finish_close(retry, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        assert!(supervisor.begin_start().is_err());
        assert_eq!(supervisor.begin_close()?, CloseClaim::AlreadyFailedClosed);
        Ok(())
    }

    #[test]
    fn first_fatal_wins_and_successful_cleanup_ends_failed_closed() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let epoch = start(&mut supervisor)?;
        let first = RuntimeError::internal("first fatal");
        let second = RuntimeError::internal("second fatal");

        assert!(supervisor.publish_fatal(epoch, first.clone())?);
        assert!(supervisor.publish_fatal(epoch, second)?);
        assert_eq!(supervisor.fatal(), Some(&first));
        assert_eq!(supervisor.state(), EngineState::Failed);

        let close = close_owner(&mut supervisor)?;
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        Ok(())
    }

    #[test]
    fn fatal_during_close_prevents_normal_stopped_publication() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let epoch = start(&mut supervisor)?;
        let close = close_owner(&mut supervisor)?;

        assert!(supervisor.publish_fatal(epoch, RuntimeError::internal("fatal during close"),)?);
        assert_eq!(supervisor.state(), EngineState::FailedClosing);
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        Ok(())
    }

    #[test]
    fn startup_cleanup_failure_requires_failed_close_retry() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(1)?;
        let attempt = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        assert!(supervisor.fail_start(
            attempt,
            RuntimeError::internal("startup cleanup failed"),
            false,
        ));
        assert_eq!(supervisor.state(), EngineState::FailedClosing);

        let close = close_owner(&mut supervisor)?;
        assert!(supervisor.fail_start(
            attempt,
            RuntimeError::connection_closed("startup cleanup retry completed"),
            true,
        ));
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        Ok(())
    }

    #[test]
    fn slot_inventory_is_sorted_bounded_and_epoch_owned() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::new(3)?;
        let epoch = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt.candidate_epoch(),
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        assert!(supervisor.register_slot(epoch, SlotId::new(2))?);
        assert!(supervisor.register_slot(epoch, SlotId::new(0))?);
        assert!(supervisor.register_slot(epoch, SlotId::new(2))?);
        assert_eq!(
            supervisor.instantiated_slots(),
            &[SlotId::new(0), SlotId::new(2)]
        );
        assert!(supervisor.register_slot(epoch, SlotId::new(3)).is_err());
        Ok(())
    }

    #[test]
    fn close_seal_and_admission_share_one_supervisor_transition() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        start(&mut supervisor)?;
        let active = match supervisor.submit(RequestId::new(1)?, deadline, now)? {
            Admission::Active(lease) => lease,
            Admission::Waiting(_) => {
                return Err(RuntimeError::internal(
                    "first request did not get active lease",
                ))
            }
            Admission::Pinned(_) => {
                return Err(RuntimeError::internal(
                    "ordinary request received a pinned lease",
                ))
            }
        };
        let waiting = match supervisor.submit(RequestId::new(2)?, deadline, now)? {
            Admission::Waiting(permit) => permit,
            Admission::Active(_) => {
                return Err(RuntimeError::internal(
                    "second request bypassed FIFO waiting",
                ))
            }
            Admission::Pinned(_) => {
                return Err(RuntimeError::internal(
                    "ordinary request received a pinned lease",
                ))
            }
        };

        let close = close_owner(&mut supervisor)?;
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 2);
        assert!(notifications
            .iter()
            .any(|notification| notification.request_id == waiting.request_id));
        assert!(notifications
            .iter()
            .any(|notification| notification.request_id == active.request_id));
        assert_eq!(supervisor.waiting_count(), 0);
        assert_eq!(supervisor.active_count(), 0);
        assert!(supervisor
            .submit(RequestId::new(3)?, deadline, now)
            .is_err());
        supervisor.check_admission_invariants()?;
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn close_retains_started_owner_until_exact_generation_retirement() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        assert!(supervisor.register_slot(epoch, SlotId::new(0))?);
        let lease = active_lease(supervisor.submit(RequestId::new(30)?, deadline, now)?)?;
        supervisor.begin_request_attempt(lease.request_id, now)?;
        let generation = GenerationId::new(1)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(generation),
            message: None,
        };
        supervisor.transition_request(lease.request_id, RequestState::Connecting, Some(wire))?;

        let close = close_owner(&mut supervisor)?;
        assert!(supervisor.take_lifecycle_notifications().is_empty());
        assert_eq!(supervisor.lifecycle_retirements().len(), 1);
        assert!(supervisor
            .transition_request(lease.request_id, RequestState::Handshaking, Some(wire))
            .is_err());
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosing);

        assert!(!supervisor.finish_lifecycle_retirement(
            GenerationIdentity {
                engine_epoch: epoch,
                slot_id: lease.slot_id,
                generation: GenerationId::new(9)?,
            },
            RetiredGeneration {
                request_id: Some(lease.request_id),
                next_generation: GenerationId::new(10)?,
            },
            now,
        )?);
        assert_eq!(supervisor.lifecycle_retirements().len(), 1);
        assert!(supervisor.finish_lifecycle_retirement(
            GenerationIdentity {
                engine_epoch: epoch,
                slot_id: lease.slot_id,
                generation,
            },
            RetiredGeneration {
                request_id: Some(lease.request_id),
                next_generation: GenerationId::new(2)?,
            },
            now,
        )?);
        assert!(supervisor.retire_slot(epoch, lease.slot_id));
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].request_id, lease.request_id);
        assert_eq!(notifications[0].kind, TerminalKind::Cancelled);
        assert_eq!(supervisor.active_count(), 0);
        let retry = close_owner(&mut supervisor)?;
        assert!(supervisor.finish_close(retry, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        Ok(())
    }

    #[test]
    fn retry_ack_after_close_terminalizes_without_starting_another_attempt(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            RequestId::new(31)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        supervisor.begin_request_attempt(lease.request_id, now)?;
        let generation = GenerationId::new(1)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(generation),
            message: None,
        };
        supervisor.transition_request(lease.request_id, RequestState::Connecting, Some(wire))?;
        assert!(matches!(
            supervisor.begin_retry(
                wire,
                RuntimeError::connection_closed("first attempt failed"),
                true,
                now,
            ),
            Some(RetryDecision::Retire(_))
        ));

        let close = close_owner(&mut supervisor)?;
        let acknowledgement = ReconnectAck {
            engine_epoch: epoch,
            slot_id: lease.slot_id,
            request_id: lease.request_id,
            retired_generation: generation,
            next_generation: GenerationId::new(2)?,
            next_endpoint_index: 1,
            endpoints_remaining_in_attempt: 1,
        };
        assert!(supervisor
            .finish_retry_retirement(acknowledgement, now)?
            .is_none());
        assert!(supervisor.lifecycle_retirements().is_empty());
        assert_eq!(supervisor.request_attempt_count(lease.request_id), None);
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].kind, TerminalKind::Cancelled);
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn close_preserves_a_timeout_that_already_owned_terminal_retirement() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let lease = active_lease(supervisor.submit(RequestId::new(34)?, deadline, now)?)?;
        supervisor.begin_request_attempt(lease.request_id, now)?;
        let generation = GenerationId::new(1)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(generation),
            message: None,
        };
        supervisor.transition_request(lease.request_id, RequestState::Connecting, Some(wire))?;
        let timeout = RuntimeError::timeout(crate::error::TimeoutPhase::Connect);
        assert!(matches!(
            supervisor.begin_retry(wire, timeout.clone(), false, now),
            Some(RetryDecision::RetireThenTerminal(_))
        ));

        let close = close_owner(&mut supervisor)?;
        let actions = supervisor.lifecycle_retirements();
        let action = &actions[0];
        assert_eq!(action.terminal_kind, TerminalKind::TimedOut);
        assert_eq!(action.error, timeout);
        assert!(supervisor.finish_lifecycle_retirement(
            GenerationIdentity {
                engine_epoch: epoch,
                slot_id: lease.slot_id,
                generation,
            },
            RetiredGeneration {
                request_id: Some(lease.request_id),
                next_generation: GenerationId::new(2)?,
            },
            now,
        )?);
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].kind, TerminalKind::TimedOut);
        assert_eq!(notifications[0].error.as_ref(), Some(&timeout));
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn fatal_cleanup_keeps_first_reason_through_exact_retirement() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let lease = active_lease(supervisor.submit(RequestId::new(32)?, deadline, now)?)?;
        supervisor.begin_request_attempt(lease.request_id, now)?;
        let generation = GenerationId::new(1)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(generation),
            message: None,
        };
        supervisor.transition_request(lease.request_id, RequestState::Connecting, Some(wire))?;
        let first = RuntimeError::internal("first fatal");
        assert!(supervisor.publish_fatal(epoch, first.clone())?);
        assert!(supervisor.publish_fatal(epoch, RuntimeError::internal("second fatal"))?);
        let actions = supervisor.lifecycle_retirements();
        assert_eq!(actions[0].error, first);

        assert!(supervisor.finish_lifecycle_retirement(
            GenerationIdentity {
                engine_epoch: epoch,
                slot_id: lease.slot_id,
                generation,
            },
            RetiredGeneration {
                request_id: None,
                next_generation: GenerationId::new(2)?,
            },
            now,
        )?);
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].kind, TerminalKind::Failed);
        assert_eq!(notifications[0].error.as_ref(), Some(&first));
        let close = close_owner(&mut supervisor)?;
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::FailedClosed);
        Ok(())
    }

    #[test]
    fn started_heartbeat_close_retains_internal_owner_until_retirement() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let generation = GenerationId::new(1)?;
        let claim = supervisor
            .claim_heartbeat(
                RequestId::new(33)?,
                HeartbeatCandidate {
                    generation: GenerationIdentity {
                        engine_epoch: epoch,
                        slot_id: SlotId::new(0),
                        generation,
                    },
                    observed_last_activity: now,
                },
                Duration::from_secs(1),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("heartbeat was not claimed"))?;
        supervisor.begin_request_attempt(claim.lease.request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: claim.lease.request_id,
            lease_id: claim.lease.lease_id,
            slot_id: claim.lease.slot_id,
            generation: Some(generation),
            message: None,
        };
        supervisor.transition_request(
            claim.lease.request_id,
            RequestState::Connecting,
            Some(wire),
        )?;

        let close = close_owner(&mut supervisor)?;
        assert_eq!(supervisor.heartbeat_count(), 1);
        assert_eq!(supervisor.lifecycle_retirements().len(), 1);
        assert!(supervisor.finish_lifecycle_retirement(
            claim.candidate.generation,
            RetiredGeneration {
                request_id: Some(claim.lease.request_id),
                next_generation: GenerationId::new(2)?,
            },
            now,
        )?);
        assert_eq!(supervisor.heartbeat_count(), 0);
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].kind, TerminalKind::Cancelled);
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        Ok(())
    }

    #[test]
    fn pin_local_fifo_shares_waiting_capacity_and_reuses_one_active_lease(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 2)?;
        let epoch = start(&mut supervisor)?;
        let reservation = active_lease(supervisor.submit(RequestId::new(40)?, deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        let first = pinned_call(supervisor.submit_pin(
            pin,
            RequestId::new(41)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let local_waiter = waiting_permit(supervisor.submit_pin(
            pin,
            RequestId::new(42)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let ordinary_waiter =
            waiting_permit(supervisor.submit(RequestId::new(43)?, deadline, now)?)?;
        let overflow = supervisor
            .submit_pin(
                pin,
                RequestId::new(44)?,
                deadline,
                RetryPolicy::ordinary(true),
                now,
            )
            .err()
            .ok_or_else(|| RuntimeError::internal("shared waiting capacity overflowed"))?;
        assert_eq!(overflow.kind(), "PoolBusy");
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.waiting_count(), 2);

        supervisor.begin_request_attempt(first.request_id, now)?;
        let first_wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: first.request_id,
            lease_id: first.pin.lease_id,
            slot_id: first.pin.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: Some(MessageIdentity::new(140, 0x044e)?),
        };
        supervisor.transition_request(first.request_id, RequestState::Sending, Some(first_wire))?;
        supervisor.transition_request(
            first.request_id,
            RequestState::WaitingResponse,
            Some(first_wire),
        )?;
        let first_terminal = supervisor
            .terminal_pin(first_wire, TerminalKind::Completed, None, now)?
            .ok_or_else(|| RuntimeError::internal("first pin terminal was rejected"))?;
        let second = first_terminal
            .pin_promotion
            .ok_or_else(|| RuntimeError::internal("pin-local waiter was not promoted"))?;
        assert_eq!(second.request_id, local_waiter.request_id);
        assert_eq!(second.pin.lease_id, first.pin.lease_id);
        assert_eq!(first_terminal.ordinary_promotion, None);
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.waiting_count(), 1);
        supervisor.check_admission_invariants()?;

        supervisor.begin_request_attempt(second.request_id, now)?;
        let second_wire = RequestWireIdentity {
            request_id: second.request_id,
            message: Some(MessageIdentity::new(141, 0x044e)?),
            ..first_wire
        };
        supervisor.transition_request(
            second.request_id,
            RequestState::Sending,
            Some(second_wire),
        )?;
        supervisor.transition_request(
            second.request_id,
            RequestState::WaitingResponse,
            Some(second_wire),
        )?;
        let second_terminal = supervisor
            .terminal_pin(second_wire, TerminalKind::Completed, None, now)?
            .ok_or_else(|| RuntimeError::internal("second pin terminal was rejected"))?;
        assert_eq!(second_terminal.pin_promotion, None);
        assert!(!second_terminal.pin_released);

        let closed = supervisor
            .close_pin(pin, now)?
            .ok_or_else(|| RuntimeError::internal("pin close was rejected"))?;
        assert!(closed.pin_released);
        assert_eq!(closed.pin_promotion, None);
        assert_eq!(
            closed
                .ordinary_promotion
                .map(|promotion| promotion.returned_permit),
            Some(ordinary_waiter)
        );
        assert_eq!(supervisor.pin_count(), 0);
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.waiting_count(), 0);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn pin_close_waits_for_exact_started_wire_terminal_before_releasing_slot(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let reservation = active_lease(supervisor.submit(RequestId::new(50)?, deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        let call = pinned_call(supervisor.submit_pin(
            pin,
            RequestId::new(51)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        supervisor.begin_request_attempt(call.request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: call.request_id,
            lease_id: call.pin.lease_id,
            slot_id: call.pin.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: Some(MessageIdentity::new(150, 0x044e)?),
        };
        supervisor.transition_request(call.request_id, RequestState::Sending, Some(wire))?;

        let closing = supervisor
            .close_pin(pin, now)?
            .ok_or_else(|| RuntimeError::internal("pin close was rejected"))?;
        assert!(!closing.pin_released);
        assert_eq!(supervisor.active_count(), 1);
        let stale = RequestWireIdentity {
            message: Some(MessageIdentity::new(151, 0x044e)?),
            ..wire
        };
        assert!(supervisor
            .terminal_pin(
                stale,
                TerminalKind::Cancelled,
                Some(RuntimeError::connection_closed("stale pin terminal")),
                now,
            )?
            .is_none());
        assert_eq!(supervisor.active_count(), 1);

        let terminal = supervisor
            .terminal_pin(
                wire,
                TerminalKind::Cancelled,
                Some(RuntimeError::connection_closed(
                    "pinned request cancelled by close",
                )),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("exact pin terminal was rejected"))?;
        assert!(terminal.pin_released);
        assert_eq!(supervisor.active_count(), 0);
        assert_eq!(supervisor.pin_count(), 0);
        assert_eq!(supervisor.stale_event_count(), 1);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn pin_local_waiter_timeout_returns_exact_shared_permit() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let active_deadline = Deadline::at(now + Duration::from_secs(3));
        let waiting_deadline = Deadline::at(now + Duration::from_millis(1));
        let mut supervisor = Supervisor::with_admission(1, 2)?;
        start(&mut supervisor)?;
        let reservation =
            active_lease(supervisor.submit(RequestId::new(55)?, active_deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        let active = pinned_call(supervisor.submit_pin(
            pin,
            RequestId::new(56)?,
            active_deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let waiting = waiting_permit(supervisor.submit_pin(
            pin,
            RequestId::new(57)?,
            waiting_deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;

        let notifications = supervisor.expire_waiting_terminals(now + Duration::from_millis(2))?;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].request_id, waiting.request_id);
        assert_eq!(notifications[0].kind, TerminalKind::TimedOut);
        assert_eq!(supervisor.waiting_count(), 0);
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.pin_count(), 1);
        assert!(supervisor
            .terminal_pin_waiting(
                pin,
                waiting,
                TerminalKind::Cancelled,
                RuntimeError::connection_closed("late pin waiter cancellation"),
            )?
            .is_none());
        assert_eq!(supervisor.stale_event_count(), 1);
        assert_eq!(active.request_id, RequestId::new(56)?);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn precise_pin_waiter_cancel_preserves_fifo_successor() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 2)?;
        let epoch = start(&mut supervisor)?;
        let reservation = active_lease(supervisor.submit(RequestId::new(70)?, deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        let active = pinned_call(supervisor.submit_pin(
            pin,
            RequestId::new(71)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let cancelled = waiting_permit(supervisor.submit_pin(
            pin,
            RequestId::new(72)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let successor = waiting_permit(supervisor.submit_pin(
            pin,
            RequestId::new(73)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let notification = supervisor
            .terminal_pin_waiting(
                pin,
                cancelled,
                TerminalKind::Cancelled,
                RuntimeError::connection_closed("pin waiter cancelled"),
            )?
            .ok_or_else(|| RuntimeError::internal("pin waiter cancellation was rejected"))?;
        assert_eq!(notification.request_id, cancelled.request_id);

        supervisor.begin_request_attempt(active.request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: active.request_id,
            lease_id: active.pin.lease_id,
            slot_id: active.pin.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: Some(MessageIdentity::new(170, 0x044e)?),
        };
        supervisor.transition_request(active.request_id, RequestState::Sending, Some(wire))?;
        supervisor.transition_request(
            active.request_id,
            RequestState::WaitingResponse,
            Some(wire),
        )?;
        let terminal = supervisor
            .terminal_pin(wire, TerminalKind::Completed, None, now)?
            .ok_or_else(|| RuntimeError::internal("pin terminal was rejected"))?;
        assert_eq!(
            terminal.pin_promotion.map(|call| call.request_id),
            Some(successor.request_id)
        );
        assert_eq!(supervisor.waiting_count(), 0);
        assert_eq!(supervisor.active_count(), 1);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn heartbeat_yields_to_business_pressure_but_not_other_heartbeats() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(2, 2)?;
        let epoch = start(&mut supervisor)?;
        let business = active_lease(supervisor.submit(RequestId::new(80)?, deadline, now)?)?;
        let second_candidate = HeartbeatCandidate {
            generation: GenerationIdentity {
                engine_epoch: epoch,
                slot_id: SlotId::new(1),
                generation: GenerationId::new(1)?,
            },
            observed_last_activity: now,
        };
        assert_eq!(
            supervisor.claim_heartbeat(
                RequestId::new(81)?,
                second_candidate,
                Duration::from_secs(1),
                now,
            )?,
            None
        );
        supervisor
            .terminal_active(
                RequestWireIdentity {
                    engine_epoch: epoch,
                    request_id: business.request_id,
                    lease_id: business.lease_id,
                    slot_id: business.slot_id,
                    generation: None,
                    message: None,
                },
                TerminalKind::Cancelled,
                Some(RuntimeError::connection_closed("business finished")),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("business terminal was rejected"))?;

        let first = supervisor
            .claim_heartbeat(
                RequestId::new(82)?,
                HeartbeatCandidate {
                    generation: GenerationIdentity {
                        slot_id: SlotId::new(0),
                        ..second_candidate.generation
                    },
                    ..second_candidate
                },
                Duration::from_secs(1),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("first heartbeat was not claimed"))?;
        let second = supervisor
            .claim_heartbeat(
                RequestId::new(83)?,
                second_candidate,
                Duration::from_secs(1),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("second heartbeat was not claimed"))?;
        assert_ne!(first.lease.slot_id, second.lease.slot_id);
        assert_eq!(supervisor.heartbeat_count(), 2);
        assert_eq!(supervisor.active_count(), 0);
        let waiting = waiting_permit(supervisor.submit(RequestId::new(84)?, deadline, now)?)?;
        assert_eq!(supervisor.waiting_count(), 1);
        assert_eq!(waiting.request_id, RequestId::new(84)?);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn exact_heartbeat_terminal_updates_ack_and_promotes_business_fifo() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let candidate = HeartbeatCandidate {
            generation: GenerationIdentity {
                engine_epoch: epoch,
                slot_id: SlotId::new(0),
                generation: GenerationId::new(1)?,
            },
            observed_last_activity: now,
        };
        let claim = supervisor
            .claim_heartbeat(RequestId::new(90)?, candidate, Duration::from_secs(1), now)?
            .ok_or_else(|| RuntimeError::internal("heartbeat was not claimed"))?;
        let waiting = waiting_permit(supervisor.submit(
            RequestId::new(91)?,
            Deadline::at(now + Duration::from_secs(2)),
            now,
        )?)?;
        supervisor.begin_request_attempt(claim.lease.request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: claim.lease.request_id,
            lease_id: claim.lease.lease_id,
            slot_id: claim.lease.slot_id,
            generation: Some(candidate.generation.generation),
            message: Some(MessageIdentity::new(190, 0x0004)?),
        };
        supervisor.transition_request(claim.lease.request_id, RequestState::Sending, Some(wire))?;
        supervisor.transition_request(
            claim.lease.request_id,
            RequestState::WaitingResponse,
            Some(wire),
        )?;
        let acknowledgement = HeartbeatAck {
            reserved: Bytes::from_static(&[0; 6]),
            server_date_raw: 20_260_815,
            server_date: None,
            raw_payload: Bytes::from_static(&[0; 10]),
        };
        let terminal = supervisor
            .terminal_heartbeat(
                wire,
                TerminalKind::Completed,
                Some(acknowledgement.clone()),
                None,
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("heartbeat terminal was rejected"))?;
        assert_eq!(
            supervisor.last_heartbeat(SlotId::new(0)),
            Some(&acknowledgement)
        );
        assert_eq!(supervisor.heartbeat_count(), 0);
        assert_eq!(
            terminal.promotion.map(|value| value.returned_permit),
            Some(waiting)
        );
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.waiting_count(), 0);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn stale_unstarted_heartbeat_withdrawal_hands_slot_to_business() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let claim = supervisor
            .claim_heartbeat(
                RequestId::new(95)?,
                HeartbeatCandidate {
                    generation: GenerationIdentity {
                        engine_epoch: epoch,
                        slot_id: SlotId::new(0),
                        generation: GenerationId::new(1)?,
                    },
                    observed_last_activity: now,
                },
                Duration::from_secs(1),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("heartbeat was not claimed"))?;
        let business = waiting_permit(supervisor.submit(
            RequestId::new(96)?,
            Deadline::at(now + Duration::from_secs(2)),
            now,
        )?)?;

        let withdrawal = supervisor
            .withdraw_heartbeat(
                claim,
                RuntimeError::connection_closed("heartbeat candidate became stale"),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("heartbeat withdrawal was rejected"))?;
        assert_eq!(supervisor.heartbeat_count(), 0);
        assert_eq!(
            withdrawal.promotion.map(|value| value.returned_permit),
            Some(business)
        );
        assert_eq!(supervisor.active_count(), 1);
        assert_eq!(supervisor.waiting_count(), 0);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn push_buffer_drops_oldest_reports_gap_and_rejects_close_race() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::with_limits(1, 1, 2, 36)?;
        let epoch = start(&mut supervisor)?;
        assert!(supervisor.offer_push(push_frame(epoch, 1)?));
        assert!(supervisor.offer_push(push_frame(epoch, 2)?));
        assert!(supervisor.offer_push(push_frame(epoch, 3)?));
        assert!(matches!(
            supervisor.poll_push(),
            Err(RuntimeError::PushOverflow {
                dropped_total: 1,
                ..
            })
        ));
        assert_eq!(
            supervisor.poll_push()?.map(|frame| frame.response.msg_id),
            Some(2)
        );
        assert_eq!(
            supervisor.poll_push()?.map(|frame| frame.response.msg_id),
            Some(3)
        );

        let close = close_owner(&mut supervisor)?;
        assert!(!supervisor.offer_push(push_frame(epoch, 4)?));
        assert_eq!(supervisor.poll_push()?, None);
        let snapshot = supervisor
            .push_snapshot()
            .ok_or_else(|| RuntimeError::internal("closing push buffer is missing"))?;
        assert!(snapshot.closed);
        assert_eq!((snapshot.frame_count, snapshot.byte_count), (0, 0));
        assert_eq!(supervisor.stale_event_count(), 1);
        assert!(supervisor.finish_close(close, Ok(())));
        Ok(())
    }

    #[test]
    fn fatal_push_priority_clears_gap_and_frames() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::with_limits(1, 1, 1, 64)?;
        let epoch = start(&mut supervisor)?;
        assert!(supervisor.offer_push(push_frame(epoch, 1)?));
        assert!(supervisor.offer_push(push_frame(epoch, 2)?));
        let fatal = RuntimeError::connection_closed("business fatal");
        assert!(supervisor.publish_fatal(epoch, fatal.clone())?);

        assert_eq!(supervisor.poll_push(), Err(fatal.clone()));
        assert_eq!(supervisor.drain_pushes(), Err(fatal.clone()));
        let snapshot = supervisor
            .push_snapshot()
            .ok_or_else(|| RuntimeError::internal("failed push buffer is missing"))?;
        assert!(snapshot.closed);
        assert!(!snapshot.gap_pending);
        assert_eq!((snapshot.frame_count, snapshot.byte_count), (0, 0));
        assert_eq!(supervisor.fatal(), Some(&fatal));
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn pool_close_rejects_pin_local_fifo_and_old_proxy_stays_invalid_after_reopen(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 2)?;
        start(&mut supervisor)?;
        let reservation = active_lease(supervisor.submit(RequestId::new(60)?, deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        pinned_call(supervisor.submit_pin(
            pin,
            RequestId::new(61)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        waiting_permit(supervisor.submit_pin(
            pin,
            RequestId::new(62)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;

        let close = close_owner(&mut supervisor)?;
        let notifications = supervisor.take_lifecycle_notifications();
        assert_eq!(notifications.len(), 2);
        assert_eq!(supervisor.pin_count(), 0);
        assert_eq!(supervisor.total_admission_owned(), 0);
        assert!(supervisor.finish_close(close, Ok(())));
        assert_eq!(supervisor.state(), EngineState::Stopped);
        let new_epoch = start(&mut supervisor)?;
        assert_ne!(new_epoch, pin.engine_epoch);
        assert!(supervisor
            .submit_pin(
                pin,
                RequestId::new(63)?,
                Deadline::at(now + Duration::from_secs(3)),
                RetryPolicy::ordinary(true),
                now,
            )
            .is_err());
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn wire_terminal_requires_exact_identity_and_is_accepted_once() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let lease = match supervisor.submit(RequestId::new(10)?, deadline, now)? {
            Admission::Active(lease) => lease,
            Admission::Waiting(_) => {
                return Err(RuntimeError::internal(
                    "request did not receive active lease",
                ))
            }
            Admission::Pinned(_) => {
                return Err(RuntimeError::internal(
                    "ordinary request received a pinned lease",
                ))
            }
        };
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: Some(MessageIdentity::new(77, 0x044e)?),
        };
        supervisor.begin_request_attempt(lease.request_id, now)?;
        supervisor.transition_request(lease.request_id, RequestState::Connecting, Some(wire))?;
        supervisor.transition_request(lease.request_id, RequestState::Handshaking, Some(wire))?;
        supervisor.transition_request(lease.request_id, RequestState::Sending, Some(wire))?;
        supervisor.transition_request(
            lease.request_id,
            RequestState::WaitingResponse,
            Some(wire),
        )?;
        let stale = RequestWireIdentity {
            message: Some(MessageIdentity::new(78, 0x044e)?),
            ..wire
        };

        assert!(supervisor
            .terminal_active(stale, TerminalKind::Completed, None, now)?
            .is_none());
        let accepted = supervisor
            .terminal_active(wire, TerminalKind::Completed, None, now)?
            .ok_or_else(|| RuntimeError::internal("exact terminal was rejected"))?;
        assert_eq!(accepted.notifications.len(), 1);
        assert_eq!(accepted.notifications[0].kind, TerminalKind::Completed);
        assert!(supervisor
            .terminal_active(wire, TerminalKind::Completed, None, now)?
            .is_none());
        assert_eq!(supervisor.stale_event_count(), 2);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    #[test]
    fn first_failure_retries_on_next_generation_and_old_terminal_is_stale(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let total_deadline = Deadline::at(now + Duration::from_secs(2));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(30)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            total_deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let first_attempt = supervisor.begin_request_attempt(request_id, now)?;
        let mut slot = runtime_slot(epoch)?;
        slot.begin_endpoint_attempt()?;
        let first_connect = slot
            .start_connect(request_id, first_attempt.deadline, now)?
            .ok_or_else(|| RuntimeError::internal("first endpoint is missing"))?;
        let first_connect_wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(first_connect.identity.generation),
            message: None,
        };
        supervisor.transition_request(
            request_id,
            RequestState::Connecting,
            Some(first_connect_wire),
        )?;
        assert!(slot.on_connected(first_connect.identity, now)?);
        let first_handshake = MessageIdentity::new(70, 0x000d)?;
        slot.begin_handshake(first_handshake, 0, false)?;
        let first_handshake_wire = RequestWireIdentity {
            message: Some(first_handshake),
            ..first_connect_wire
        };
        supervisor.transition_request(
            request_id,
            RequestState::Handshaking,
            Some(first_handshake_wire),
        )?;
        assert!(matches!(
            slot.on_frame(
                crate::slot::FrameIdentity {
                    generation: first_connect.identity,
                    message: first_handshake,
                    receive_sequence: 1,
                    send_complete: true,
                },
                now,
            ),
            crate::slot::FrameDisposition::Matched(_)
        ));
        let first_business = MessageIdentity::new(71, 0x044e)?;
        slot.begin_business(first_business, 1)?;
        let first_business_wire = RequestWireIdentity {
            message: Some(first_business),
            ..first_connect_wire
        };
        supervisor.transition_request(
            request_id,
            RequestState::Sending,
            Some(first_business_wire),
        )?;
        assert!(supervisor.mark_business_bytes_sent(first_business_wire));

        let retry = supervisor
            .begin_retry(
                first_business_wire,
                RuntimeError::connection_closed("first generation failed"),
                true,
                now + Duration::from_millis(100),
            )
            .ok_or_else(|| RuntimeError::internal("retry decision was stale"))?;
        assert!(matches!(retry, RetryDecision::Retire(_)));
        assert!(slot.begin_reconnect_retire(
            request_id,
            first_connect.identity,
            "first generation failed",
        )?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, first_connect.identity)?
            .ok_or_else(|| RuntimeError::internal("retirement ack was rejected"))?;
        let retry_attempt = supervisor
            .finish_retry_retirement(acknowledgement, now + Duration::from_millis(100))?
            .ok_or_else(|| RuntimeError::internal("retry attempt was not authorized"))?;
        assert_eq!(retry_attempt.attempt_number, 2);

        slot.begin_endpoint_attempt()?;
        let second_connect = slot
            .start_connect(
                request_id,
                retry_attempt.deadline,
                now + Duration::from_millis(100),
            )?
            .ok_or_else(|| RuntimeError::internal("retry endpoint is missing"))?;
        let second_connect_wire = RequestWireIdentity {
            generation: Some(second_connect.identity.generation),
            message: None,
            ..first_connect_wire
        };
        supervisor.transition_request(
            request_id,
            RequestState::Connecting,
            Some(second_connect_wire),
        )?;
        let handshake_wire = RequestWireIdentity {
            message: Some(MessageIdentity::new(80, 0x000d)?),
            ..second_connect_wire
        };
        supervisor.transition_request(
            request_id,
            RequestState::Handshaking,
            Some(handshake_wire),
        )?;
        let business_wire = RequestWireIdentity {
            message: Some(MessageIdentity::new(81, 0x044e)?),
            ..second_connect_wire
        };
        supervisor.transition_request(request_id, RequestState::Sending, Some(business_wire))?;
        supervisor.transition_request(
            request_id,
            RequestState::WaitingResponse,
            Some(business_wire),
        )?;

        assert!(supervisor
            .terminal_active(first_business_wire, TerminalKind::Completed, None, now)?
            .is_none());
        let completed = supervisor
            .terminal_active(business_wire, TerminalKind::Completed, None, now)?
            .ok_or_else(|| RuntimeError::internal("second generation did not complete"))?;
        assert_eq!(completed.notifications[0].kind, TerminalKind::Completed);
        assert_eq!(supervisor.stale_event_count(), 1);
        Ok(())
    }

    #[test]
    fn non_retry_safe_request_is_terminal_after_business_byte() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(31)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            Deadline::at(now + Duration::from_secs(2)),
            RetryPolicy::ordinary(false),
            now,
        )?)?;
        let attempt = supervisor.begin_request_attempt(request_id, now)?;
        let mut slot = runtime_slot(epoch)?;
        slot.begin_endpoint_attempt()?;
        let connect = slot
            .start_connect(request_id, attempt.deadline, now)?
            .ok_or_else(|| RuntimeError::internal("test endpoint is missing"))?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(connect.identity.generation),
            message: Some(MessageIdentity::new(90, 0x044e)?),
        };
        supervisor.transition_request(request_id, RequestState::Sending, Some(wire))?;
        assert!(supervisor.mark_business_bytes_sent(wire));

        let decision = supervisor
            .begin_retry(
                wire,
                RuntimeError::connection_closed("partial send failed"),
                true,
                now,
            )
            .ok_or_else(|| RuntimeError::internal("retry decision was stale"))?;
        assert!(matches!(
            decision,
            RetryDecision::RetireThenTerminal(ref terminal)
                if terminal.reason == RetryStopReason::UnsafeAfterBusinessBytes
        ));
        assert_eq!(supervisor.request_attempt_count(request_id), Some(1));
        assert!(supervisor
            .terminal_active(
                wire,
                TerminalKind::Failed,
                Some(RuntimeError::connection_closed("premature terminal")),
                now,
            )?
            .is_none());
        assert!(slot.begin_reconnect_retire(
            request_id,
            connect.identity,
            "partial send failed"
        )?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, connect.identity)?
            .ok_or_else(|| RuntimeError::internal("terminal retirement ack was rejected"))?;
        let terminal_wire = supervisor
            .finish_terminal_retirement(acknowledgement)
            .ok_or_else(|| RuntimeError::internal("terminal retirement was not accepted"))?;
        let terminal = supervisor
            .terminal_active(
                terminal_wire,
                TerminalKind::Failed,
                Some(RuntimeError::connection_closed("partial send failed")),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("terminal was rejected after retirement"))?;
        assert_eq!(terminal.notifications[0].kind, TerminalKind::Failed);
        assert_eq!(supervisor.stale_event_count(), 1);
        assert_eq!(supervisor.request_attempt_count(request_id), None);
        Ok(())
    }

    #[test]
    fn pre_send_failure_may_retry_even_when_command_is_not_retry_safe() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(32)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            Deadline::at(now + Duration::from_secs(2)),
            RetryPolicy::ordinary(false),
            now,
        )?)?;
        supervisor.begin_request_attempt(request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: None,
        };
        supervisor.transition_request(request_id, RequestState::Connecting, Some(wire))?;

        let decision = supervisor
            .begin_retry(
                wire,
                RuntimeError::connection_closed("connect failed before send"),
                true,
                now,
            )
            .ok_or_else(|| RuntimeError::internal("retry decision was stale"))?;
        assert!(matches!(decision, RetryDecision::Retire(_)));
        Ok(())
    }

    #[test]
    fn connect_failover_keeps_the_same_attempt_and_uses_the_next_endpoint(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(35)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            Deadline::at(now + Duration::from_secs(2)),
            RetryPolicy::ordinary(false),
            now,
        )?)?;
        let first_attempt = supervisor.begin_request_attempt(request_id, now)?;
        let mut slot = runtime_slot(epoch)?;
        slot.begin_endpoint_attempt()?;
        let first = slot
            .start_connect(request_id, first_attempt.deadline, now)?
            .ok_or_else(|| RuntimeError::internal("first endpoint is missing"))?;
        let first_wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(first.identity.generation),
            message: None,
        };
        supervisor.transition_request(request_id, RequestState::Connecting, Some(first_wire))?;
        assert!(slot.begin_reconnect_retire(request_id, first.identity, "connect failed")?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, first.identity)?
            .ok_or_else(|| RuntimeError::internal("reconnect ack was rejected"))?;
        let continued = supervisor
            .continue_connect_after_reconnect(acknowledgement, now + Duration::from_millis(50))?
            .ok_or_else(|| RuntimeError::internal("same-attempt failover was rejected"))?;
        let second = slot
            .start_connect(
                request_id,
                continued.deadline,
                now + Duration::from_millis(50),
            )?
            .ok_or_else(|| RuntimeError::internal("second endpoint is missing"))?;

        assert_eq!(continued.attempt_number, 1);
        assert_eq!(continued.deadline, first_attempt.deadline);
        assert_eq!(supervisor.request_attempt_count(request_id), Some(1));
        assert_eq!(second.attempt.endpoint_index, 1);
        assert_eq!(
            second.identity.generation,
            continued
                .expected_generation
                .ok_or_else(|| RuntimeError::internal("expected generation is missing"))?
        );
        Ok(())
    }

    #[test]
    fn retry_attempt_uses_remaining_original_deadline_without_reset() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let total_deadline = Deadline::at(now + Duration::from_millis(1_000));
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(33)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            total_deadline,
            RetryPolicy::ordinary(true),
            now,
        )?)?;
        let first_attempt = supervisor.begin_request_attempt(request_id, now)?;
        assert_eq!(
            first_attempt.deadline.instant(),
            now + Duration::from_millis(500)
        );
        let mut slot = runtime_slot(epoch)?;
        slot.begin_endpoint_attempt()?;
        let first = slot
            .start_connect(request_id, first_attempt.deadline, now)?
            .ok_or_else(|| RuntimeError::internal("first endpoint is missing"))?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(first.identity.generation),
            message: None,
        };
        supervisor.transition_request(request_id, RequestState::Connecting, Some(wire))?;
        assert!(matches!(
            supervisor.begin_retry(
                wire,
                RuntimeError::connection_closed("first attempt failed"),
                true,
                now + Duration::from_millis(400),
            ),
            Some(RetryDecision::Retire(_))
        ));
        assert!(slot.begin_reconnect_retire(request_id, first.identity, "first attempt failed")?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, first.identity)?
            .ok_or_else(|| RuntimeError::internal("retirement ack was rejected"))?;
        let second_attempt = supervisor
            .finish_retry_retirement(acknowledgement, now + Duration::from_millis(400))?
            .ok_or_else(|| RuntimeError::internal("second attempt was not authorized"))?;

        assert_eq!(second_attempt.attempt_number, 2);
        assert_eq!(second_attempt.total_deadline, total_deadline);
        assert_eq!(second_attempt.deadline, total_deadline);
        assert_eq!(supervisor.request_attempt_count(request_id), Some(2));
        let second_wire = RequestWireIdentity {
            generation: second_attempt.expected_generation,
            ..wire
        };
        supervisor.transition_request(request_id, RequestState::Connecting, Some(second_wire))?;
        assert!(matches!(
            supervisor.begin_retry(
                second_wire,
                RuntimeError::connection_closed("second attempt failed"),
                true,
                now + Duration::from_millis(500),
            ),
            Some(RetryDecision::RetireThenTerminal(ref terminal))
                if terminal.reason == RetryStopReason::AttemptsExhausted
        ));
        Ok(())
    }

    #[test]
    fn internal_heartbeat_has_no_second_wire_attempt() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let epoch = start(&mut supervisor)?;
        let request_id = RequestId::new(34)?;
        let lease = active_lease(supervisor.submit_with_retry_policy(
            request_id,
            Deadline::at(now + Duration::from_secs(2)),
            RetryPolicy::internal_heartbeat(),
            now,
        )?)?;
        supervisor.begin_request_attempt(request_id, now)?;
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            generation: Some(GenerationId::new(1)?),
            message: Some(MessageIdentity::new(100, 0x0004)?),
        };
        supervisor.transition_request(request_id, RequestState::Sending, Some(wire))?;

        let decision = supervisor
            .begin_retry(
                wire,
                RuntimeError::connection_closed("heartbeat failed"),
                true,
                now,
            )
            .ok_or_else(|| RuntimeError::internal("retry decision was stale"))?;
        assert!(matches!(
            decision,
            RetryDecision::RetireThenTerminal(ref terminal)
                if terminal.reason == RetryStopReason::AttemptsExhausted
        ));
        Ok(())
    }

    #[test]
    fn queued_expiry_releases_permit_and_notifies_once() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        start(&mut supervisor)?;
        let active_deadline = Deadline::at(now + Duration::from_secs(3));
        let waiting_deadline = Deadline::at(now + Duration::from_secs(1));
        supervisor.submit(RequestId::new(20)?, active_deadline, now)?;
        supervisor.submit(RequestId::new(21)?, waiting_deadline, now)?;

        let notifications = supervisor.expire_waiting_terminals(now + Duration::from_secs(2))?;
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].request_id, RequestId::new(21)?);
        assert_eq!(notifications[0].kind, TerminalKind::TimedOut);
        assert_eq!(supervisor.waiting_count(), 0);
        supervisor.check_admission_invariants()?;
        Ok(())
    }

    proptest! {
        #[test]
        fn arbitrary_stale_epoch_cannot_publish_fatal(delta in 1_u64..10_000) {
            let mut supervisor = match Supervisor::new(1) {
                Ok(supervisor) => supervisor,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let epoch = match start(&mut supervisor) {
                Ok(epoch) => epoch,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let stale = match EngineEpoch::new(epoch.get().saturating_add(delta)) {
                Ok(epoch) => epoch,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };

            let accepted = match supervisor.publish_fatal(
                stale,
                RuntimeError::internal("stale fatal"),
            ) {
                Ok(accepted) => accepted,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            prop_assert!(!accepted);
            prop_assert_eq!(supervisor.state(), EngineState::Running);
            prop_assert!(supervisor.fatal().is_none());
            prop_assert_eq!(supervisor.stale_event_count(), 1);
        }
    }
}
