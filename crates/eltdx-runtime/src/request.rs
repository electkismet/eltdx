use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use crate::deadline::Deadline;
use crate::error::{RuntimeError, TimeoutPhase};
use crate::pin::PinnedCallLease;
use crate::slot::{EngineEpoch, GenerationId, MessageIdentity, ReconnectAck, RequestId, SlotId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaitingPermitId(u64);

impl WaitingPermitId {
    #[cfg(test)]
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseId(u64);

impl LeaseId {
    #[cfg(test)]
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaitingPermit {
    pub engine_epoch: EngineEpoch,
    pub request_id: RequestId,
    pub permit_id: WaitingPermitId,
    pub deadline: Deadline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveLease {
    pub engine_epoch: EngineEpoch,
    pub request_id: RequestId,
    pub lease_id: LeaseId,
    pub slot_id: SlotId,
    pub deadline: Deadline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    Active(ActiveLease),
    Waiting(WaitingPermit),
    Pinned(PinnedCallLease),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Promotion {
    pub returned_permit: WaitingPermit,
    pub active_lease: ActiveLease,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseOutcome {
    pub released: bool,
    pub promotion: Option<Promotion>,
    pub timed_out: Vec<WaitingPermit>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionOwnership {
    Waiting(WaitingPermit),
    Active(ActiveLease),
}

#[derive(Debug)]
pub struct AdmissionQueue {
    pool_size: usize,
    max_pending_requests: usize,
    epoch: Option<EngineEpoch>,
    sealed: bool,
    idle_slots: VecDeque<SlotId>,
    waiting: VecDeque<RequestId>,
    pin_waiting: BTreeMap<RequestId, WaitingPermit>,
    ownership: BTreeMap<RequestId, AdmissionOwnership>,
    active_by_slot: Vec<Option<LeaseId>>,
    permit_counter: u64,
    lease_counter: u64,
}

impl AdmissionQueue {
    pub fn new(pool_size: usize, max_pending_requests: usize) -> Result<Self, RuntimeError> {
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
        Ok(Self {
            pool_size,
            max_pending_requests,
            epoch: None,
            sealed: true,
            idle_slots: VecDeque::with_capacity(pool_size),
            waiting: VecDeque::with_capacity(max_pending_requests),
            pin_waiting: BTreeMap::new(),
            ownership: BTreeMap::new(),
            active_by_slot: vec![None; pool_size],
            permit_counter: 0,
            lease_counter: 0,
        })
    }

    pub fn open(&mut self, epoch: EngineEpoch) -> Result<(), RuntimeError> {
        if self.epoch.is_some()
            || !self.ownership.is_empty()
            || !self.waiting.is_empty()
            || !self.pin_waiting.is_empty()
            || self.active_by_slot.iter().any(Option::is_some)
        {
            return Err(RuntimeError::internal(
                "cannot open admission while prior epoch ownership remains",
            ));
        }
        self.idle_slots.clear();
        self.idle_slots.extend((0..self.pool_size).map(SlotId::new));
        self.epoch = Some(epoch);
        self.sealed = false;
        Ok(())
    }

    pub fn submit(
        &mut self,
        epoch: EngineEpoch,
        request_id: RequestId,
        deadline: Deadline,
        now: Instant,
    ) -> Result<Admission, RuntimeError> {
        self.require_open_epoch(epoch)?;
        deadline.require_remaining_at(now, TimeoutPhase::Admission)?;
        if self.ownership.contains_key(&request_id) {
            return Err(RuntimeError::internal("request already owns admission")
                .with_context("request_id", request_id.get().to_string()));
        }

        if self.waiting.is_empty() && !self.idle_slots.is_empty() {
            let lease_id = self.next_lease_id()?;
            let slot_id = self.idle_slots.pop_front().ok_or_else(|| {
                RuntimeError::internal("idle Slot disappeared during direct admission")
            })?;
            let lease = ActiveLease {
                engine_epoch: epoch,
                request_id,
                lease_id,
                slot_id,
                deadline,
            };
            self.set_active_slot(lease)?;
            self.ownership
                .insert(request_id, AdmissionOwnership::Active(lease));
            return Ok(Admission::Active(lease));
        }

        if self.waiting_count() >= self.max_pending_requests {
            return Err(RuntimeError::PoolBusy {
                message: "7709 pool admission queue is full".to_owned(),
                capacity: self.max_pending_requests,
                context: Vec::new(),
            });
        }
        let permit_id = self.next_permit_id()?;
        let permit = WaitingPermit {
            engine_epoch: epoch,
            request_id,
            permit_id,
            deadline,
        };
        self.waiting.push_back(request_id);
        self.ownership
            .insert(request_id, AdmissionOwnership::Waiting(permit));
        Ok(Admission::Waiting(permit))
    }

    pub fn claim_idle_for_heartbeat(
        &mut self,
        epoch: EngineEpoch,
        request_id: RequestId,
        slot_id: SlotId,
        deadline: Deadline,
        now: Instant,
    ) -> Result<Option<ActiveLease>, RuntimeError> {
        self.require_open_epoch(epoch)?;
        deadline.require_remaining_at(now, TimeoutPhase::Heartbeat)?;
        if self.ownership.contains_key(&request_id) {
            return Err(RuntimeError::internal("request already owns admission")
                .with_context("request_id", request_id.get().to_string()));
        }
        if self.waiting_count() != 0 {
            return Ok(None);
        }
        let Some(index) = self
            .idle_slots
            .iter()
            .position(|candidate| *candidate == slot_id)
        else {
            return Ok(None);
        };
        let slot_index = slot_id.get();
        let slot_owner = self
            .active_by_slot
            .get(slot_index)
            .ok_or_else(|| RuntimeError::internal("heartbeat references an unknown Slot"))?;
        if slot_owner.is_some() {
            return Err(RuntimeError::internal(
                "idle heartbeat Slot already has an active owner",
            ));
        }
        let lease_id = self.next_lease_id()?;
        self.idle_slots.remove(index);
        let lease = ActiveLease {
            engine_epoch: epoch,
            request_id,
            lease_id,
            slot_id,
            deadline,
        };
        self.active_by_slot[slot_index] = Some(lease_id);
        self.ownership
            .insert(request_id, AdmissionOwnership::Active(lease));
        Ok(Some(lease))
    }

    pub fn reserve_pin_waiting(
        &mut self,
        epoch: EngineEpoch,
        request_id: RequestId,
        deadline: Deadline,
        now: Instant,
    ) -> Result<WaitingPermit, RuntimeError> {
        self.require_open_epoch(epoch)?;
        deadline.require_remaining_at(now, TimeoutPhase::Pin)?;
        if self.ownership.contains_key(&request_id) {
            return Err(RuntimeError::internal("request already owns admission")
                .with_context("request_id", request_id.get().to_string()));
        }
        if self.waiting_count() >= self.max_pending_requests {
            return Err(RuntimeError::PoolBusy {
                message: "7709 pool admission queue is full".to_owned(),
                capacity: self.max_pending_requests,
                context: Vec::new(),
            });
        }
        let permit = WaitingPermit {
            engine_epoch: epoch,
            request_id,
            permit_id: self.next_permit_id()?,
            deadline,
        };
        self.pin_waiting.insert(request_id, permit);
        self.ownership
            .insert(request_id, AdmissionOwnership::Waiting(permit));
        Ok(permit)
    }

    pub fn release_pin_waiting(&mut self, permit: WaitingPermit) -> bool {
        if self.epoch != Some(permit.engine_epoch)
            || self.pin_waiting.get(&permit.request_id) != Some(&permit)
            || self.ownership.get(&permit.request_id) != Some(&AdmissionOwnership::Waiting(permit))
        {
            return false;
        }
        self.pin_waiting.remove(&permit.request_id);
        self.ownership.remove(&permit.request_id);
        true
    }

    pub fn validate_pin_waiting(&self, permit: WaitingPermit) -> bool {
        self.epoch == Some(permit.engine_epoch)
            && self.pin_waiting.get(&permit.request_id) == Some(&permit)
            && self.ownership.get(&permit.request_id) == Some(&AdmissionOwnership::Waiting(permit))
    }

    pub fn pin_waiting_permits(&self) -> Vec<WaitingPermit> {
        self.pin_waiting.values().copied().collect()
    }

    pub fn cancel_waiting(&mut self, permit: WaitingPermit) -> bool {
        if self.epoch != Some(permit.engine_epoch)
            || self.ownership.get(&permit.request_id) != Some(&AdmissionOwnership::Waiting(permit))
        {
            return false;
        }
        let Some(index) = self
            .waiting
            .iter()
            .position(|request_id| *request_id == permit.request_id)
        else {
            return false;
        };
        self.waiting.remove(index);
        self.ownership.remove(&permit.request_id);
        true
    }

    pub fn release_active(
        &mut self,
        lease: ActiveLease,
        now: Instant,
    ) -> Result<ReleaseOutcome, RuntimeError> {
        if self.epoch != Some(lease.engine_epoch)
            || self.ownership.get(&lease.request_id) != Some(&AdmissionOwnership::Active(lease))
            || self.active_lease_for_slot(lease.slot_id) != Some(lease.lease_id)
        {
            return Ok(ReleaseOutcome {
                released: false,
                promotion: None,
                timed_out: Vec::new(),
            });
        }

        let live_waiter = if self.sealed {
            None
        } else {
            self.first_live_waiter(now)?
        };
        let promoted_lease_id = live_waiter.map(|_| self.next_lease_id()).transpose()?;

        self.ownership.remove(&lease.request_id);
        self.clear_active_slot(lease)?;
        let mut timed_out = self.remove_expired_front(now)?;
        let promotion = if self.sealed {
            None
        } else if let (Some(request_id), Some(lease_id)) = (live_waiter, promoted_lease_id) {
            let waiting = self.waiting.pop_front().ok_or_else(|| {
                RuntimeError::internal("live FIFO waiter disappeared during promotion")
            })?;
            if waiting != request_id {
                return Err(RuntimeError::internal(
                    "FIFO waiter changed during promotion",
                ));
            }
            let ownership = self.ownership.remove(&request_id).ok_or_else(|| {
                RuntimeError::internal("promoted waiter has no admission ownership")
            })?;
            let AdmissionOwnership::Waiting(permit) = ownership else {
                return Err(RuntimeError::internal(
                    "promoted waiter does not own a waiting permit",
                ));
            };
            if permit.deadline.is_elapsed_at(now) {
                timed_out.push(permit);
                self.idle_slots.push_back(lease.slot_id);
                None
            } else {
                let active_lease = ActiveLease {
                    engine_epoch: lease.engine_epoch,
                    request_id,
                    lease_id,
                    slot_id: lease.slot_id,
                    deadline: permit.deadline,
                };
                self.set_active_slot(active_lease)?;
                self.ownership
                    .insert(request_id, AdmissionOwnership::Active(active_lease));
                Some(Promotion {
                    returned_permit: permit,
                    active_lease,
                })
            }
        } else {
            self.idle_slots.push_back(lease.slot_id);
            None
        };

        Ok(ReleaseOutcome {
            released: true,
            promotion,
            timed_out,
        })
    }

    pub fn validate_active_release(
        &self,
        lease: ActiveLease,
        now: Instant,
    ) -> Result<bool, RuntimeError> {
        if self.epoch != Some(lease.engine_epoch)
            || self.ownership.get(&lease.request_id) != Some(&AdmissionOwnership::Active(lease))
            || self.active_lease_for_slot(lease.slot_id) != Some(lease.lease_id)
        {
            return Ok(false);
        }
        if !self.sealed && self.first_live_waiter(now)?.is_some() {
            next_identity(self.lease_counter, "active lease")?;
        }
        Ok(true)
    }

    pub fn expire_waiting(&mut self, now: Instant) -> Result<Vec<WaitingPermit>, RuntimeError> {
        let mut expired = Vec::new();
        let mut index = 0;
        while index < self.waiting.len() {
            let request_id = self.waiting[index];
            let permit = self.waiting_permit(request_id)?;
            if permit.deadline.is_elapsed_at(now) {
                self.waiting.remove(index);
                self.ownership.remove(&request_id);
                expired.push(permit);
            } else {
                index += 1;
            }
        }
        Ok(expired)
    }

    pub fn seal(&mut self) -> Result<Vec<WaitingPermit>, RuntimeError> {
        if self.sealed {
            return Ok(Vec::new());
        }
        self.sealed = true;
        self.idle_slots.clear();
        let mut rejected = Vec::with_capacity(self.waiting_count());
        while let Some(request_id) = self.waiting.pop_front() {
            let ownership = self
                .ownership
                .remove(&request_id)
                .ok_or_else(|| RuntimeError::internal("sealed FIFO waiter has no ownership"))?;
            let AdmissionOwnership::Waiting(permit) = ownership else {
                return Err(RuntimeError::internal(
                    "sealed FIFO waiter does not own a waiting permit",
                ));
            };
            rejected.push(permit);
        }
        for (request_id, permit) in std::mem::take(&mut self.pin_waiting) {
            let ownership = self.ownership.remove(&request_id).ok_or_else(|| {
                RuntimeError::internal("sealed pin-local waiter has no ownership")
            })?;
            if ownership != AdmissionOwnership::Waiting(permit) {
                return Err(RuntimeError::internal(
                    "sealed pin-local waiter does not own its waiting permit",
                ));
            }
            rejected.push(permit);
        }
        Ok(rejected)
    }

    pub fn close_complete(&mut self) {
        self.epoch = None;
        self.sealed = true;
        self.idle_slots.clear();
        self.waiting.clear();
        self.pin_waiting.clear();
        self.ownership.clear();
        self.active_by_slot.fill(None);
    }

    pub const fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub(crate) const fn epoch(&self) -> Option<EngineEpoch> {
        self.epoch
    }

    pub(crate) fn idle_count(&self) -> usize {
        self.idle_slots.len()
    }

    pub(crate) fn ordinary_waiting_count(&self) -> usize {
        self.waiting.len()
    }

    pub(crate) fn pin_waiting_count(&self) -> usize {
        self.pin_waiting.len()
    }

    pub fn waiting_count(&self) -> usize {
        self.waiting.len().saturating_add(self.pin_waiting.len())
    }

    pub fn active_count(&self) -> usize {
        self.active_by_slot
            .iter()
            .filter(|lease| lease.is_some())
            .count()
    }

    pub fn total_owned(&self) -> usize {
        self.waiting_count().saturating_add(self.active_count())
    }

    pub fn admission_for(&self, request_id: RequestId) -> Option<Admission> {
        self.ownership
            .get(&request_id)
            .map(|ownership| match ownership {
                AdmissionOwnership::Waiting(permit) => Admission::Waiting(*permit),
                AdmissionOwnership::Active(lease) => Admission::Active(*lease),
            })
    }

    pub const fn total_capacity(&self) -> usize {
        self.pool_size.saturating_add(self.max_pending_requests)
    }

    pub fn check_invariants(&self) -> Result<(), RuntimeError> {
        if self.waiting_count() > self.max_pending_requests {
            return Err(RuntimeError::internal("waiting permit capacity exceeded"));
        }
        if self.active_count() > self.pool_size {
            return Err(RuntimeError::internal("active lease capacity exceeded"));
        }
        if self.total_owned() > self.total_capacity() {
            return Err(RuntimeError::internal("total admission capacity exceeded"));
        }
        if self.ownership.len() != self.total_owned() {
            return Err(RuntimeError::internal(
                "admission ownership count does not match permits plus leases",
            ));
        }
        for request_id in &self.waiting {
            if !matches!(
                self.ownership.get(request_id),
                Some(AdmissionOwnership::Waiting(_))
            ) {
                return Err(RuntimeError::internal(
                    "FIFO waiter is missing its waiting permit",
                ));
            }
        }
        for (request_id, permit) in &self.pin_waiting {
            if self.ownership.get(request_id) != Some(&AdmissionOwnership::Waiting(*permit)) {
                return Err(RuntimeError::internal(
                    "pin-local waiter is missing its waiting permit",
                ));
            }
            if self.waiting.contains(request_id) {
                return Err(RuntimeError::internal(
                    "request appears in both ordinary and pin-local waiting queues",
                ));
            }
        }
        for (slot_index, lease_id) in self.active_by_slot.iter().enumerate() {
            let Some(lease_id) = lease_id else {
                continue;
            };
            let mut matching = 0_usize;
            for ownership in self.ownership.values() {
                if let AdmissionOwnership::Active(lease) = ownership {
                    if lease.slot_id == SlotId::new(slot_index) && lease.lease_id == *lease_id {
                        matching = matching.saturating_add(1);
                    }
                }
            }
            if matching != 1 {
                return Err(RuntimeError::internal(
                    "active Slot does not have exactly one matching lease",
                ));
            }
        }
        if self.sealed {
            if !self.idle_slots.is_empty() {
                return Err(RuntimeError::internal(
                    "sealed admission retained idle Slots",
                ));
            }
        } else {
            if self.idle_slots.len().saturating_add(self.active_count()) != self.pool_size {
                return Err(RuntimeError::internal(
                    "idle plus active Slot ownership does not equal pool size",
                ));
            }
            if !self.idle_slots.is_empty() && !self.waiting.is_empty() {
                return Err(RuntimeError::internal(
                    "live FIFO waiters were not promoted onto idle Slots",
                ));
            }
        }
        Ok(())
    }

    fn require_open_epoch(&self, epoch: EngineEpoch) -> Result<(), RuntimeError> {
        if self.epoch == Some(epoch) && !self.sealed {
            return Ok(());
        }
        Err(RuntimeError::connection_closed(
            "7709 pool is closed during admission",
        ))
    }

    fn first_live_waiter(&self, now: Instant) -> Result<Option<RequestId>, RuntimeError> {
        for request_id in &self.waiting {
            let permit = self.waiting_permit(*request_id)?;
            if !permit.deadline.is_elapsed_at(now) {
                return Ok(Some(*request_id));
            }
        }
        Ok(None)
    }

    fn remove_expired_front(&mut self, now: Instant) -> Result<Vec<WaitingPermit>, RuntimeError> {
        let mut expired = Vec::new();
        loop {
            let Some(request_id) = self.waiting.front().copied() else {
                break;
            };
            let permit = self.waiting_permit(request_id)?;
            if !permit.deadline.is_elapsed_at(now) {
                break;
            }
            self.waiting.pop_front();
            self.ownership.remove(&request_id);
            expired.push(permit);
        }
        Ok(expired)
    }

    fn waiting_permit(&self, request_id: RequestId) -> Result<WaitingPermit, RuntimeError> {
        match self.ownership.get(&request_id) {
            Some(AdmissionOwnership::Waiting(permit)) => Ok(*permit),
            _ => Err(
                RuntimeError::internal("waiting request does not own a waiting permit")
                    .with_context("request_id", request_id.get().to_string()),
            ),
        }
    }

    fn set_active_slot(&mut self, lease: ActiveLease) -> Result<(), RuntimeError> {
        let slot_index = lease.slot_id.get();
        let slot = self
            .active_by_slot
            .get_mut(slot_index)
            .ok_or_else(|| RuntimeError::internal("active lease references an unknown Slot"))?;
        if slot.is_some() {
            return Err(RuntimeError::internal(
                "active lease would duplicate Slot ownership",
            ));
        }
        *slot = Some(lease.lease_id);
        Ok(())
    }

    fn clear_active_slot(&mut self, lease: ActiveLease) -> Result<(), RuntimeError> {
        let slot = self
            .active_by_slot
            .get_mut(lease.slot_id.get())
            .ok_or_else(|| RuntimeError::internal("released lease references an unknown Slot"))?;
        if *slot != Some(lease.lease_id) {
            return Err(RuntimeError::internal(
                "released lease does not own its Slot",
            ));
        }
        *slot = None;
        Ok(())
    }

    fn active_lease_for_slot(&self, slot_id: SlotId) -> Option<LeaseId> {
        self.active_by_slot.get(slot_id.get()).copied().flatten()
    }

    fn next_permit_id(&mut self) -> Result<WaitingPermitId, RuntimeError> {
        let next = next_identity(self.permit_counter, "waiting permit")?;
        self.permit_counter = next;
        Ok(WaitingPermitId(next))
    }

    fn next_lease_id(&mut self) -> Result<LeaseId, RuntimeError> {
        let next = next_identity(self.lease_counter, "active lease")?;
        self.lease_counter = next;
        Ok(LeaseId(next))
    }
}

fn next_identity(current: u64, name: &'static str) -> Result<u64, RuntimeError> {
    current
        .checked_add(1)
        .ok_or_else(|| RuntimeError::internal(format!("{name} identity space exhausted")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestState {
    Queued,
    Assigned,
    Connecting,
    Handshaking,
    Sending,
    WaitingResponse,
    Retrying,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalKind {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestWireIdentity {
    pub engine_epoch: EngineEpoch,
    pub request_id: RequestId,
    pub lease_id: LeaseId,
    pub slot_id: SlotId,
    pub generation: Option<GenerationId>,
    pub message: Option<MessageIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestCleanupCandidate {
    pub ownership: Admission,
    pub state: RequestState,
    pub wire: Option<RequestWireIdentity>,
}

impl RequestCleanupCandidate {
    pub const fn request_id(self) -> RequestId {
        match self.ownership {
            Admission::Active(lease) => lease.request_id,
            Admission::Waiting(permit) => permit.request_id,
            Admission::Pinned(call) => call.request_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalNotification {
    pub request_id: RequestId,
    pub kind: TerminalKind,
    pub error: Option<RuntimeError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalBatch {
    pub notifications: Vec<TerminalNotification>,
    pub promotion: Option<Promotion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    retry_safe: bool,
    max_attempts: u8,
}

impl RetryPolicy {
    pub const fn ordinary(retry_safe: bool) -> Self {
        Self {
            retry_safe,
            max_attempts: 2,
        }
    }

    pub const fn internal_heartbeat() -> Self {
        Self {
            retry_safe: false,
            max_attempts: 1,
        }
    }

    pub const fn retry_safe(self) -> bool {
        self.retry_safe
    }

    pub const fn max_attempts(self) -> u8 {
        self.max_attempts
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::ordinary(false)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestAttempt {
    pub engine_epoch: EngineEpoch,
    pub request_id: RequestId,
    pub lease_id: LeaseId,
    pub slot_id: SlotId,
    pub attempt_number: u8,
    pub attempts_including_current: u8,
    pub deadline: Deadline,
    pub total_deadline: Deadline,
    pub expected_generation: Option<GenerationId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryStopReason {
    NotRetryable,
    AttemptsExhausted,
    DeadlineElapsed,
    UnsafeAfterBusinessBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryRetirement {
    pub wire: RequestWireIdentity,
    pub failed_attempt: u8,
    pub error: RuntimeError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryTerminal {
    pub wire: RequestWireIdentity,
    pub reason: RetryStopReason,
    pub error: RuntimeError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    Retire(RetryRetirement),
    RetireThenTerminal(RetryTerminal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetirementIntent {
    Retry,
    Terminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AttemptRecord {
    policy: RetryPolicy,
    attempts_started: u8,
    attempt_deadline: Option<Deadline>,
    business_bytes_sent: bool,
    last_failure: Option<RuntimeError>,
    retirement_intent: Option<RetirementIntent>,
}

impl AttemptRecord {
    const fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            attempts_started: 0,
            attempt_deadline: None,
            business_bytes_sent: false,
            last_failure: None,
            retirement_intent: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestRecord {
    state: RequestState,
    ownership: Admission,
    wire: Option<RequestWireIdentity>,
    attempt: AttemptRecord,
}

#[derive(Debug, Default)]
pub struct RequestTracker {
    records: BTreeMap<RequestId, RequestRecord>,
}

impl RequestTracker {
    pub fn contains(&self, request_id: RequestId) -> bool {
        self.records.contains_key(&request_id)
    }

    pub fn state(&self, request_id: RequestId) -> Option<RequestState> {
        self.records.get(&request_id).map(|record| record.state)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn admit(&mut self, admission: Admission) -> Result<(), RuntimeError> {
        self.admit_with_retry_policy(admission, RetryPolicy::default())
    }

    pub fn admit_with_retry_policy(
        &mut self,
        admission: Admission,
        retry_policy: RetryPolicy,
    ) -> Result<(), RuntimeError> {
        let (request_id, state) = match admission {
            Admission::Active(lease) => (lease.request_id, RequestState::Assigned),
            Admission::Waiting(permit) => (permit.request_id, RequestState::Queued),
            Admission::Pinned(call) => (call.request_id, RequestState::Assigned),
        };
        if self.records.contains_key(&request_id) {
            return Err(RuntimeError::internal("request lifecycle already exists")
                .with_context("request_id", request_id.get().to_string()));
        }
        self.records.insert(
            request_id,
            RequestRecord {
                state,
                ownership: admission,
                wire: None,
                attempt: AttemptRecord::new(retry_policy),
            },
        );
        Ok(())
    }

    pub fn promote(&mut self, promotion: Promotion) -> Result<(), RuntimeError> {
        let request_id = promotion.returned_permit.request_id;
        let record = self
            .records
            .get_mut(&request_id)
            .ok_or_else(|| RuntimeError::internal("promoted request lifecycle is missing"))?;
        if record.state != RequestState::Queued
            || record.ownership != Admission::Waiting(promotion.returned_permit)
            || promotion.active_lease.request_id != request_id
        {
            return Err(RuntimeError::internal(
                "promotion does not match queued request ownership",
            ));
        }
        record.state = RequestState::Assigned;
        record.ownership = Admission::Active(promotion.active_lease);
        Ok(())
    }

    pub fn promote_pinned(
        &mut self,
        returned_permit: WaitingPermit,
        pinned_call: PinnedCallLease,
    ) -> Result<(), RuntimeError> {
        let request_id = returned_permit.request_id;
        let record = self
            .records
            .get_mut(&request_id)
            .ok_or_else(|| RuntimeError::internal("promoted pin request lifecycle is missing"))?;
        if record.state != RequestState::Queued
            || record.ownership != Admission::Waiting(returned_permit)
            || pinned_call.request_id != request_id
            || pinned_call.pin.engine_epoch != returned_permit.engine_epoch
            || pinned_call.deadline != returned_permit.deadline
        {
            return Err(RuntimeError::internal(
                "pin promotion does not match queued request ownership",
            ));
        }
        record.state = RequestState::Assigned;
        record.ownership = Admission::Pinned(pinned_call);
        Ok(())
    }

    pub fn transfer_assigned_to_pin(
        &mut self,
        request_id: RequestId,
    ) -> Result<ActiveLease, RuntimeError> {
        let lease = self.assigned_active_for_pin(request_id)?;
        self.records.remove(&request_id);
        Ok(lease)
    }

    pub fn assigned_active_for_pin(
        &self,
        request_id: RequestId,
    ) -> Result<ActiveLease, RuntimeError> {
        let record = self.records.get(&request_id).ok_or_else(|| {
            RuntimeError::internal("pin reservation request lifecycle is missing")
        })?;
        let Admission::Active(lease) = record.ownership else {
            return Err(RuntimeError::internal(
                "pin reservation does not own an active Slot lease",
            ));
        };
        if record.state != RequestState::Assigned
            || record.wire.is_some()
            || record.attempt.attempts_started != 0
            || record.attempt.retirement_intent.is_some()
        {
            return Err(RuntimeError::internal(
                "pin reservation must transfer before starting wire work",
            ));
        }
        Ok(lease)
    }

    pub fn begin_attempt(
        &mut self,
        request_id: RequestId,
        now: Instant,
    ) -> Result<RequestAttempt, RuntimeError> {
        let record = self.records.get_mut(&request_id).ok_or_else(|| {
            RuntimeError::internal("request lifecycle is missing")
                .with_context("request_id", request_id.get().to_string())
        })?;
        if record.state != RequestState::Assigned
            || record.attempt.attempts_started != 0
            || record.wire.is_some()
            || record.attempt.retirement_intent.is_some()
        {
            return Err(RuntimeError::internal(
                "initial attempt requires a newly assigned request",
            )
            .with_context("request_id", request_id.get().to_string()));
        }
        start_attempt(record, now, None)
    }

    pub fn mark_business_bytes_sent(&mut self, identity: RequestWireIdentity) -> bool {
        let Some(record) = self.records.get_mut(&identity.request_id) else {
            return false;
        };
        if record.wire != Some(identity)
            || record.attempt.retirement_intent.is_some()
            || identity.message.is_none()
            || !matches!(
                record.state,
                RequestState::Sending | RequestState::WaitingResponse
            )
        {
            return false;
        }
        record.attempt.business_bytes_sent = true;
        true
    }

    pub fn begin_retry(
        &mut self,
        identity: RequestWireIdentity,
        error: RuntimeError,
        retryable: bool,
        now: Instant,
    ) -> Option<RetryDecision> {
        let record = self.records.get_mut(&identity.request_id)?;
        if record.wire != Some(identity)
            || record.attempt.retirement_intent.is_some()
            || !matches!(
                record.state,
                RequestState::Connecting
                    | RequestState::Handshaking
                    | RequestState::Sending
                    | RequestState::WaitingResponse
            )
        {
            return None;
        }

        let reason = if !retryable {
            Some(RetryStopReason::NotRetryable)
        } else if record.attempt.attempts_started >= record.attempt.policy.max_attempts {
            Some(RetryStopReason::AttemptsExhausted)
        } else if active_deadline(record).is_elapsed_at(now) {
            Some(RetryStopReason::DeadlineElapsed)
        } else if record.attempt.business_bytes_sent && !record.attempt.policy.retry_safe {
            Some(RetryStopReason::UnsafeAfterBusinessBytes)
        } else {
            None
        };
        if let Some(reason) = reason {
            record.attempt.last_failure = Some(error.clone());
            record.attempt.retirement_intent = Some(RetirementIntent::Terminal);
            return Some(RetryDecision::RetireThenTerminal(RetryTerminal {
                wire: identity,
                reason,
                error,
            }));
        }

        record.state = RequestState::Retrying;
        record.attempt.attempt_deadline = None;
        record.attempt.last_failure = Some(error.clone());
        record.attempt.retirement_intent = Some(RetirementIntent::Retry);
        Some(RetryDecision::Retire(RetryRetirement {
            wire: identity,
            failed_attempt: record.attempt.attempts_started,
            error,
        }))
    }

    pub fn cleanup_candidates(&self) -> Vec<RequestCleanupCandidate> {
        self.records
            .values()
            .map(|record| RequestCleanupCandidate {
                ownership: record.ownership,
                state: record.state,
                wire: record.wire,
            })
            .collect()
    }

    pub fn cleanup_candidate(&self, request_id: RequestId) -> Option<RequestCleanupCandidate> {
        self.records
            .get(&request_id)
            .map(|record| RequestCleanupCandidate {
                ownership: record.ownership,
                state: record.state,
                wire: record.wire,
            })
    }

    pub fn begin_terminal_retirement(&mut self, identity: RequestWireIdentity) -> bool {
        let Some(record) = self.records.get_mut(&identity.request_id) else {
            return false;
        };
        if record.wire != Some(identity)
            || identity.generation.is_none()
            || !matches!(
                record.state,
                RequestState::Connecting
                    | RequestState::Handshaking
                    | RequestState::Sending
                    | RequestState::WaitingResponse
                    | RequestState::Retrying
            )
        {
            return false;
        }
        record.attempt.retirement_intent = Some(RetirementIntent::Terminal);
        true
    }

    pub fn validate_terminal_retirement(&self, identity: RequestWireIdentity) -> bool {
        self.records
            .get(&identity.request_id)
            .is_some_and(|record| {
                record.wire == Some(identity)
                    && identity.generation.is_some()
                    && record.attempt.retirement_intent == Some(RetirementIntent::Terminal)
                    && matches!(
                        record.state,
                        RequestState::Connecting
                            | RequestState::Handshaking
                            | RequestState::Sending
                            | RequestState::WaitingResponse
                            | RequestState::Retrying
                    )
            })
    }

    pub fn terminal_retirement_error(
        &self,
        identity: RequestWireIdentity,
    ) -> Option<&RuntimeError> {
        let record = self.records.get(&identity.request_id)?;
        if record.wire == Some(identity)
            && record.attempt.retirement_intent == Some(RetirementIntent::Terminal)
        {
            return record.attempt.last_failure.as_ref();
        }
        None
    }

    pub fn finish_retry_retirement(
        &mut self,
        acknowledgement: ReconnectAck,
        now: Instant,
    ) -> Result<Option<RequestAttempt>, RuntimeError> {
        let Some(record) = self.records.get_mut(&acknowledgement.request_id) else {
            return Ok(None);
        };
        let Some(wire) = record.wire else {
            return Ok(None);
        };
        if record.state != RequestState::Retrying
            || record.attempt.retirement_intent != Some(RetirementIntent::Retry)
            || wire.engine_epoch != acknowledgement.engine_epoch
            || wire.slot_id != acknowledgement.slot_id
            || wire.generation != Some(acknowledgement.retired_generation)
            || acknowledgement.next_generation == acknowledgement.retired_generation
        {
            return Ok(None);
        }
        let mut next_record = record.clone();
        next_record.attempt.retirement_intent = None;
        let attempt = start_attempt(&mut next_record, now, Some(acknowledgement.next_generation))?;
        *record = next_record;
        Ok(Some(attempt))
    }

    pub fn finish_terminal_retirement(
        &mut self,
        acknowledgement: ReconnectAck,
    ) -> Option<RequestWireIdentity> {
        let record = self.records.get_mut(&acknowledgement.request_id)?;
        let wire = record.wire?;
        if record.attempt.retirement_intent != Some(RetirementIntent::Terminal)
            || wire.engine_epoch != acknowledgement.engine_epoch
            || wire.slot_id != acknowledgement.slot_id
            || wire.generation != Some(acknowledgement.retired_generation)
            || acknowledgement.next_generation == acknowledgement.retired_generation
        {
            return None;
        }
        record.attempt.retirement_intent = None;
        Some(wire)
    }

    pub fn continue_connect_after_reconnect(
        &mut self,
        acknowledgement: ReconnectAck,
        now: Instant,
    ) -> Result<Option<RequestAttempt>, RuntimeError> {
        let Some(record) = self.records.get_mut(&acknowledgement.request_id) else {
            return Ok(None);
        };
        let Some(wire) = record.wire else {
            return Ok(None);
        };
        if record.state != RequestState::Connecting
            || record.attempt.retirement_intent.is_some()
            || wire.engine_epoch != acknowledgement.engine_epoch
            || wire.slot_id != acknowledgement.slot_id
            || wire.generation != Some(acknowledgement.retired_generation)
            || acknowledgement.next_generation == acknowledgement.retired_generation
            || acknowledgement.endpoints_remaining_in_attempt == 0
        {
            return Ok(None);
        }
        let attempt_deadline = record.attempt.attempt_deadline.ok_or_else(|| {
            RuntimeError::internal("connecting request has no active attempt deadline")
        })?;
        attempt_deadline.require_remaining_at(now, TimeoutPhase::Connect)?;
        let owner = wire_owner(record.ownership)?;
        let attempts_including_current = record
            .attempt
            .policy
            .max_attempts
            .checked_sub(record.attempt.attempts_started)
            .and_then(|remaining| remaining.checked_add(1))
            .ok_or_else(|| RuntimeError::internal("request attempt count is inconsistent"))?;
        record.wire = Some(RequestWireIdentity {
            engine_epoch: owner.engine_epoch,
            request_id: owner.request_id,
            lease_id: owner.lease_id,
            slot_id: owner.slot_id,
            generation: Some(acknowledgement.next_generation),
            message: None,
        });
        Ok(Some(RequestAttempt {
            engine_epoch: owner.engine_epoch,
            request_id: owner.request_id,
            lease_id: owner.lease_id,
            slot_id: owner.slot_id,
            attempt_number: record.attempt.attempts_started,
            attempts_including_current,
            deadline: attempt_deadline,
            total_deadline: owner.deadline,
            expected_generation: Some(acknowledgement.next_generation),
        }))
    }

    pub fn attempt_count(&self, request_id: RequestId) -> Option<u8> {
        self.records
            .get(&request_id)
            .map(|record| record.attempt.attempts_started)
    }

    pub fn attempt_deadline(&self, request_id: RequestId) -> Option<Deadline> {
        self.records
            .get(&request_id)
            .and_then(|record| record.attempt.attempt_deadline)
    }

    pub fn last_retry_error(&self, request_id: RequestId) -> Option<&RuntimeError> {
        self.records
            .get(&request_id)
            .and_then(|record| record.attempt.last_failure.as_ref())
    }

    pub fn transition(
        &mut self,
        request_id: RequestId,
        next: RequestState,
        wire: Option<RequestWireIdentity>,
    ) -> Result<(), RuntimeError> {
        let record = self.records.get_mut(&request_id).ok_or_else(|| {
            RuntimeError::internal("request lifecycle is missing")
                .with_context("request_id", request_id.get().to_string())
        })?;
        if !valid_transition(record.state, next) {
            return Err(RuntimeError::internal(format!(
                "invalid request transition: {:?} -> {next:?}",
                record.state
            ))
            .with_context("request_id", request_id.get().to_string()));
        }
        if record.attempt.retirement_intent.is_some() {
            return Err(RuntimeError::internal(
                "request cannot advance while generation retirement is pending",
            ));
        }
        let identity = wire.ok_or_else(|| {
            RuntimeError::internal("active request transition requires an exact wire identity")
        })?;
        validate_wire_owner(record.ownership, identity)?;
        if identity.generation.is_none() {
            return Err(RuntimeError::internal(
                "wire-bound request requires a TCP generation",
            ));
        }
        if record.attempt.attempt_deadline.is_none() {
            return Err(RuntimeError::internal(
                "wire transition requires a started request attempt",
            ));
        }
        if let Some(current) = record.wire {
            if current.generation != identity.generation {
                return Err(RuntimeError::internal(
                    "request attempt cannot change TCP generation",
                ));
            }
            if record.state == RequestState::Sending
                && next == RequestState::WaitingResponse
                && current.message != identity.message
            {
                return Err(RuntimeError::internal(
                    "response wait must retain the sent message identity",
                ));
            }
        }
        record.wire = Some(identity);
        record.state = next;
        Ok(())
    }

    pub fn validate_waiting_terminal(&self, permit: WaitingPermit) -> bool {
        self.records.get(&permit.request_id).is_some_and(|record| {
            record.state == RequestState::Queued && record.ownership == Admission::Waiting(permit)
        })
    }

    pub fn validate_active_terminal(&self, identity: RequestWireIdentity) -> Option<ActiveLease> {
        let record = self.records.get(&identity.request_id)?;
        let Admission::Active(lease) = record.ownership else {
            return None;
        };
        if lease.engine_epoch != identity.engine_epoch
            || lease.lease_id != identity.lease_id
            || lease.slot_id != identity.slot_id
            || !matches!(
                record.state,
                RequestState::Assigned
                    | RequestState::Connecting
                    | RequestState::Handshaking
                    | RequestState::Sending
                    | RequestState::WaitingResponse
                    | RequestState::Retrying
            )
        {
            return None;
        }
        if record.attempt.retirement_intent.is_some() {
            return None;
        }
        if let Some(wire) = record.wire {
            if wire != identity {
                return None;
            }
        } else if identity.generation.is_some()
            || identity.message.is_some()
            || record.state != RequestState::Assigned
        {
            return None;
        }
        Some(lease)
    }

    pub fn validate_pinned_terminal(
        &self,
        identity: RequestWireIdentity,
    ) -> Option<PinnedCallLease> {
        let record = self.records.get(&identity.request_id)?;
        let Admission::Pinned(call) = record.ownership else {
            return None;
        };
        if call.pin.engine_epoch != identity.engine_epoch
            || call.pin.lease_id != identity.lease_id
            || call.pin.slot_id != identity.slot_id
            || call.request_id != identity.request_id
            || !matches!(
                record.state,
                RequestState::Assigned
                    | RequestState::Connecting
                    | RequestState::Handshaking
                    | RequestState::Sending
                    | RequestState::WaitingResponse
                    | RequestState::Retrying
            )
            || record.attempt.retirement_intent.is_some()
        {
            return None;
        }
        if let Some(wire) = record.wire {
            if wire != identity {
                return None;
            }
        } else if identity.generation.is_some()
            || identity.message.is_some()
            || record.state != RequestState::Assigned
        {
            return None;
        }
        Some(call)
    }

    pub fn matches_pinned_owner(
        &self,
        call: PinnedCallLease,
        wire: Option<RequestWireIdentity>,
    ) -> bool {
        self.records.get(&call.request_id).is_some_and(|record| {
            record.ownership == Admission::Pinned(call) && record.wire == wire
        })
    }

    pub fn commit_terminal(
        &mut self,
        request_id: RequestId,
        kind: TerminalKind,
        error: Option<RuntimeError>,
    ) -> Result<TerminalNotification, RuntimeError> {
        if matches!(kind, TerminalKind::Completed) == error.is_some() {
            return Err(RuntimeError::internal(
                "completed terminal must not have an error and failed terminals must have one",
            ));
        }
        if self.records.remove(&request_id).is_none() {
            return Err(
                RuntimeError::internal("terminal request lifecycle is missing")
                    .with_context("request_id", request_id.get().to_string()),
            );
        }
        Ok(TerminalNotification {
            request_id,
            kind,
            error,
        })
    }

    pub fn check_matches_admission(&self, admission: &AdmissionQueue) -> Result<(), RuntimeError> {
        self.check_matches_admission_with_pins(admission, &[], &[], &[])
    }

    pub fn check_matches_admission_with_pins(
        &self,
        admission: &AdmissionQueue,
        pin_leases: &[ActiveLease],
        pin_waiters: &[WaitingPermit],
        pin_calls: &[PinnedCallLease],
    ) -> Result<(), RuntimeError> {
        let admission_records = self
            .records
            .values()
            .filter(|record| !matches!(record.ownership, Admission::Pinned(_)))
            .count();
        if admission_records.saturating_add(pin_leases.len()) != admission.total_owned() {
            return Err(RuntimeError::internal(
                "request and pin owner count does not match admission ownership",
            ));
        }
        for (request_id, record) in &self.records {
            match record.ownership {
                Admission::Pinned(call) => {
                    if !pin_calls.contains(&call) || admission.admission_for(*request_id).is_some()
                    {
                        return Err(RuntimeError::internal(
                            "pinned request lifecycle does not match pin call ownership",
                        )
                        .with_context("request_id", request_id.get().to_string()));
                    }
                }
                ownership => {
                    if admission.admission_for(*request_id) != Some(ownership) {
                        return Err(RuntimeError::internal(
                            "request lifecycle ownership does not match admission token",
                        )
                        .with_context("request_id", request_id.get().to_string()));
                    }
                }
            }
        }
        for lease in pin_leases {
            if admission.admission_for(lease.request_id) != Some(Admission::Active(*lease)) {
                return Err(RuntimeError::internal(
                    "pin reservation lease does not match admission ownership",
                ));
            }
        }
        for permit in pin_waiters {
            if !admission.validate_pin_waiting(*permit) || !self.validate_waiting_terminal(*permit)
            {
                return Err(RuntimeError::internal(
                    "pin-local waiter does not match permit and request ownership",
                ));
            }
        }
        let detached = admission.pin_waiting_permits();
        if detached.len() != pin_waiters.len()
            || detached.iter().any(|permit| !pin_waiters.contains(permit))
        {
            return Err(RuntimeError::internal(
                "detached pin-local permit set does not match PinRegistry FIFO ownership",
            ));
        }
        for call in pin_calls {
            if !self
                .records
                .get(&call.request_id)
                .is_some_and(|record| record.ownership == Admission::Pinned(*call))
            {
                return Err(RuntimeError::internal(
                    "pin active call does not have a request lifecycle owner",
                ));
            }
        }
        Ok(())
    }
}

fn active_deadline(record: &RequestRecord) -> Deadline {
    match record.ownership {
        Admission::Active(lease) => lease.deadline,
        Admission::Waiting(permit) => permit.deadline,
        Admission::Pinned(call) => call.deadline,
    }
}

#[derive(Clone, Copy)]
struct WireOwner {
    engine_epoch: EngineEpoch,
    request_id: RequestId,
    lease_id: LeaseId,
    slot_id: SlotId,
    deadline: Deadline,
}

fn wire_owner(ownership: Admission) -> Result<WireOwner, RuntimeError> {
    match ownership {
        Admission::Active(lease) => Ok(WireOwner {
            engine_epoch: lease.engine_epoch,
            request_id: lease.request_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            deadline: lease.deadline,
        }),
        Admission::Pinned(call) => Ok(WireOwner {
            engine_epoch: call.pin.engine_epoch,
            request_id: call.request_id,
            lease_id: call.pin.lease_id,
            slot_id: call.pin.slot_id,
            deadline: call.deadline,
        }),
        Admission::Waiting(_) => Err(RuntimeError::internal(
            "waiting request cannot own a wire attempt",
        )),
    }
}

fn start_attempt(
    record: &mut RequestRecord,
    now: Instant,
    expected_generation: Option<GenerationId>,
) -> Result<RequestAttempt, RuntimeError> {
    let owner = wire_owner(record.ownership)?;
    let attempts_including_current = record
        .attempt
        .policy
        .max_attempts
        .checked_sub(record.attempt.attempts_started)
        .ok_or_else(|| RuntimeError::internal("request attempt count exceeded retry policy"))?;
    if attempts_including_current == 0 {
        return Err(RuntimeError::internal(
            "request retry policy has no remaining attempts",
        ));
    }
    let deadline = owner.deadline.fair_slice_at(
        now,
        usize::from(attempts_including_current),
        TimeoutPhase::Retry,
    )?;
    let attempt_number = record
        .attempt
        .attempts_started
        .checked_add(1)
        .ok_or_else(|| RuntimeError::internal("request attempt counter overflow"))?;
    record.attempt.attempts_started = attempt_number;
    record.attempt.attempt_deadline = Some(deadline);
    record.attempt.business_bytes_sent = false;
    record.attempt.retirement_intent = None;
    record.wire = expected_generation.map(|generation| RequestWireIdentity {
        engine_epoch: owner.engine_epoch,
        request_id: owner.request_id,
        lease_id: owner.lease_id,
        slot_id: owner.slot_id,
        generation: Some(generation),
        message: None,
    });
    Ok(RequestAttempt {
        engine_epoch: owner.engine_epoch,
        request_id: owner.request_id,
        lease_id: owner.lease_id,
        slot_id: owner.slot_id,
        attempt_number,
        attempts_including_current,
        deadline,
        total_deadline: owner.deadline,
        expected_generation,
    })
}

fn validate_wire_owner(
    ownership: Admission,
    wire: RequestWireIdentity,
) -> Result<(), RuntimeError> {
    let owner = wire_owner(ownership)?;
    if owner.engine_epoch != wire.engine_epoch
        || owner.request_id != wire.request_id
        || owner.lease_id != wire.lease_id
        || owner.slot_id != wire.slot_id
    {
        return Err(RuntimeError::internal(
            "wire identity does not match active lease",
        ));
    }
    Ok(())
}

const fn valid_transition(current: RequestState, next: RequestState) -> bool {
    matches!(
        (current, next),
        (RequestState::Assigned, RequestState::Connecting)
            | (RequestState::Assigned, RequestState::Sending)
            | (RequestState::Connecting, RequestState::Handshaking)
            | (RequestState::Handshaking, RequestState::Sending)
            | (RequestState::Sending, RequestState::WaitingResponse)
            | (RequestState::Retrying, RequestState::Connecting)
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use proptest::prelude::*;

    use super::{ActiveLease, Admission, AdmissionQueue, WaitingPermit};
    use crate::deadline::Deadline;
    use crate::error::RuntimeError;
    use crate::slot::{EngineEpoch, RequestId};

    fn queue(
        pool_size: usize,
        max_pending: usize,
    ) -> Result<(AdmissionQueue, EngineEpoch), RuntimeError> {
        let epoch = EngineEpoch::new(1)?;
        let mut queue = AdmissionQueue::new(pool_size, max_pending)?;
        queue.open(epoch)?;
        Ok((queue, epoch))
    }

    fn deadline(now: Instant, seconds: u64) -> Deadline {
        Deadline::at(now + Duration::from_secs(seconds))
    }

    fn active(admission: Admission) -> Result<ActiveLease, RuntimeError> {
        match admission {
            Admission::Active(lease) => Ok(lease),
            Admission::Waiting(_) => Err(RuntimeError::internal("expected active lease")),
            Admission::Pinned(_) => Err(RuntimeError::internal("expected active lease")),
        }
    }

    fn waiting(admission: Admission) -> Result<WaitingPermit, RuntimeError> {
        match admission {
            Admission::Waiting(permit) => Ok(permit),
            Admission::Active(_) => Err(RuntimeError::internal("expected waiting permit")),
            Admission::Pinned(_) => Err(RuntimeError::internal("expected waiting permit")),
        }
    }

    #[test]
    fn direct_active_admission_does_not_consume_waiting_capacity() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(2, 1)?;
        let first = active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let second = active(queue.submit(epoch, RequestId::new(2)?, deadline(now, 3), now)?)?;

        assert_ne!(first.slot_id, second.slot_id);
        assert_eq!(queue.active_count(), 2);
        assert_eq!(queue.waiting_count(), 0);
        assert_eq!(queue.total_owned(), 2);
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn release_atomically_promotes_fifo_and_returns_waiting_permit() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 2)?;
        let active = active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let first = waiting(queue.submit(epoch, RequestId::new(2)?, deadline(now, 3), now)?)?;
        let second = waiting(queue.submit(epoch, RequestId::new(3)?, deadline(now, 3), now)?)?;

        let released = queue.release_active(active, now)?;
        let promotion = released
            .promotion
            .ok_or_else(|| RuntimeError::internal("FIFO waiter was not promoted"))?;

        assert_eq!(promotion.returned_permit, first);
        assert_eq!(promotion.active_lease.request_id, first.request_id);
        assert_eq!(queue.waiting_count(), 1);
        assert!(queue.cancel_waiting(second));
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn waiting_capacity_is_exact_and_does_not_double_for_ingress() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 1)?;
        active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        waiting(queue.submit(epoch, RequestId::new(2)?, deadline(now, 3), now)?)?;

        let error = queue
            .submit(epoch, RequestId::new(3)?, deadline(now, 3), now)
            .err()
            .ok_or_else(|| RuntimeError::internal("queue overflow was accepted"))?;
        assert_eq!(error.kind(), "PoolBusy");
        assert_eq!(queue.total_owned(), 2);
        assert_eq!(queue.total_capacity(), 2);
        Ok(())
    }

    #[test]
    fn pin_local_waiters_share_the_exact_global_waiting_capacity() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 2)?;
        active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let pin_waiter =
            queue.reserve_pin_waiting(epoch, RequestId::new(2)?, deadline(now, 3), now)?;
        waiting(queue.submit(epoch, RequestId::new(3)?, deadline(now, 3), now)?)?;

        let error = queue
            .reserve_pin_waiting(epoch, RequestId::new(4)?, deadline(now, 3), now)
            .err()
            .ok_or_else(|| RuntimeError::internal("shared waiting capacity overflowed"))?;
        assert_eq!(error.kind(), "PoolBusy");
        assert_eq!(queue.waiting_count(), 2);
        assert!(queue.release_pin_waiting(pin_waiter));
        assert_eq!(queue.waiting_count(), 1);
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn heartbeat_claims_only_the_exact_idle_slot_and_never_queues() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(2, 1)?;
        let heartbeat = queue
            .claim_idle_for_heartbeat(
                epoch,
                RequestId::new(10)?,
                crate::slot::SlotId::new(1),
                deadline(now, 3),
                now,
            )?
            .ok_or_else(|| RuntimeError::internal("idle heartbeat Slot was not claimed"))?;
        assert_eq!(heartbeat.slot_id, crate::slot::SlotId::new(1));
        let business = active(queue.submit(epoch, RequestId::new(11)?, deadline(now, 3), now)?)?;
        assert_eq!(business.slot_id, crate::slot::SlotId::new(0));
        waiting(queue.submit(epoch, RequestId::new(12)?, deadline(now, 3), now)?)?;
        assert_eq!(
            queue.claim_idle_for_heartbeat(
                epoch,
                RequestId::new(13)?,
                crate::slot::SlotId::new(0),
                deadline(now, 3),
                now,
            )?,
            None
        );
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn stale_tokens_cannot_release_current_ownership() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 2)?;
        let active = active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let permit = waiting(queue.submit(epoch, RequestId::new(2)?, deadline(now, 3), now)?)?;
        let stale_permit = WaitingPermit {
            request_id: RequestId::new(99)?,
            ..permit
        };
        let stale_lease = ActiveLease {
            request_id: RequestId::new(99)?,
            ..active
        };

        assert!(!queue.cancel_waiting(stale_permit));
        assert!(!queue.release_active(stale_lease, now)?.released);
        assert_eq!(queue.active_count(), 1);
        assert_eq!(queue.waiting_count(), 1);
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn expired_fifo_head_is_removed_before_live_promotion() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 2)?;
        let active = active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let expired = waiting(queue.submit(epoch, RequestId::new(2)?, deadline(now, 1), now)?)?;
        let live = waiting(queue.submit(epoch, RequestId::new(3)?, deadline(now, 3), now)?)?;

        let outcome = queue.release_active(active, now + Duration::from_secs(2))?;
        assert_eq!(outcome.timed_out, vec![expired]);
        assert_eq!(
            outcome.promotion.map(|promotion| promotion.returned_permit),
            Some(live)
        );
        queue.check_invariants()?;
        Ok(())
    }

    #[test]
    fn close_seal_rejects_waiting_and_retains_active_until_terminal() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let (mut queue, epoch) = queue(1, 2)?;
        let active = active(queue.submit(epoch, RequestId::new(1)?, deadline(now, 3), now)?)?;
        let waiting = waiting(queue.submit(epoch, RequestId::new(2)?, deadline(now, 3), now)?)?;

        assert_eq!(queue.seal()?, vec![waiting]);
        assert!(queue.is_sealed());
        assert_eq!(queue.active_count(), 1);
        assert_eq!(queue.waiting_count(), 0);
        assert!(queue
            .submit(epoch, RequestId::new(3)?, deadline(now, 3), now)
            .is_err());
        assert!(queue.release_active(active, now)?.released);
        assert_eq!(queue.active_count(), 0);
        queue.check_invariants()?;
        Ok(())
    }

    proptest! {
        #[test]
        fn owned_capacity_never_exceeds_pool_plus_waiting_limit(
            operations in proptest::collection::vec(any::<bool>(), 0..256),
        ) {
            let now = Instant::now();
            let (mut queue, epoch) = match queue(4, 8) {
                Ok(value) => value,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let mut next_request = 1_u64;
            let mut active = VecDeque::new();
            for submit in operations {
                if submit {
                    let request_id = match RequestId::new(next_request) {
                        Ok(request_id) => request_id,
                        Err(error) => return Err(TestCaseError::fail(error.to_string())),
                    };
                    next_request = next_request.saturating_add(1);
                    if let Ok(Admission::Active(lease)) = queue.submit(
                        epoch,
                        request_id,
                        deadline(now, 30),
                        now,
                    ) {
                        active.push_back(lease);
                    }
                } else if let Some(lease) = active.pop_front() {
                    let outcome = match queue.release_active(lease, now) {
                        Ok(outcome) => outcome,
                        Err(error) => return Err(TestCaseError::fail(error.to_string())),
                    };
                    if let Some(promotion) = outcome.promotion {
                        active.push_back(promotion.active_lease);
                    }
                }
                if let Err(error) = queue.check_invariants() {
                    return Err(TestCaseError::fail(error.to_string()));
                }
                prop_assert!(queue.total_owned() <= queue.total_capacity());
            }
        }
    }
}
