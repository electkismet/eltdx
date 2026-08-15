use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use crate::deadline::Deadline;
use crate::error::{RuntimeError, TimeoutPhase};
use crate::request::{ActiveLease, LeaseId, RequestWireIdentity, WaitingPermit};
use crate::slot::{EngineEpoch, ReconnectAck, RequestId, SlotId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PinId(u64);

impl PinId {
    pub fn new(value: u64) -> Result<Self, RuntimeError> {
        if value == 0 {
            return Err(RuntimeError::internal("pin id must be nonzero"));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinIdentity {
    pub engine_epoch: EngineEpoch,
    pub pin_id: PinId,
    pub lease_id: LeaseId,
    pub slot_id: SlotId,
    pub reservation_request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedCallLease {
    pub pin: PinIdentity,
    pub request_id: RequestId,
    pub deadline: Deadline,
}

impl PinnedCallLease {
    pub const fn engine_epoch(self) -> EngineEpoch {
        self.pin.engine_epoch
    }

    pub const fn lease_id(self) -> LeaseId {
        self.pin.lease_id
    }

    pub const fn slot_id(self) -> SlotId {
        self.pin.slot_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PinState {
    Open,
    Closing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinTerminalPlan {
    pin: PinIdentity,
    call: PinnedCallLease,
    wire: RequestWireIdentity,
    sequence: u64,
    next_sequence: u64,
    expired: Vec<WaitingPermit>,
    promotion: Option<WaitingPermit>,
    released_lease: Option<ActiveLease>,
}

impl PinTerminalPlan {
    pub const fn pin(&self) -> PinIdentity {
        self.pin
    }

    pub const fn call(&self) -> PinnedCallLease {
        self.call
    }

    pub const fn wire(&self) -> RequestWireIdentity {
        self.wire
    }

    pub fn expired(&self) -> &[WaitingPermit] {
        &self.expired
    }

    pub const fn promotion(&self) -> Option<WaitingPermit> {
        self.promotion
    }

    pub const fn released_lease(&self) -> Option<ActiveLease> {
        self.released_lease
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinAdvanceOutcome {
    pub expired: Vec<WaitingPermit>,
    pub promotion: Option<(WaitingPermit, PinnedCallLease)>,
    pub released_lease: Option<ActiveLease>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinCloseOutcome {
    pub rejected: Vec<WaitingPermit>,
    pub withdrawn_unstarted: Option<PinnedCallLease>,
    pub released_lease: Option<ActiveLease>,
    pub waiting_for_request: Option<RequestId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActivePinCall {
    lease: PinnedCallLease,
    wire: Option<RequestWireIdentity>,
}

#[derive(Debug)]
struct PinRecord {
    identity: PinIdentity,
    lease: ActiveLease,
    state: PinState,
    active: Option<ActivePinCall>,
    waiting: VecDeque<WaitingPermit>,
    sequence: u64,
}

#[derive(Debug)]
pub struct PinRegistry {
    pool_size: usize,
    records: BTreeMap<PinId, PinRecord>,
}

impl PinRegistry {
    pub fn new(pool_size: usize) -> Result<Self, RuntimeError> {
        if pool_size == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "pool_size must be a positive integer",
            ));
        }
        Ok(Self {
            pool_size,
            records: BTreeMap::new(),
        })
    }

    pub fn register(
        &mut self,
        pin_id: PinId,
        lease: ActiveLease,
    ) -> Result<PinIdentity, RuntimeError> {
        self.validate_register(pin_id, lease)?;
        let identity = PinIdentity {
            engine_epoch: lease.engine_epoch,
            pin_id,
            lease_id: lease.lease_id,
            slot_id: lease.slot_id,
            reservation_request_id: lease.request_id,
        };
        self.records.insert(
            pin_id,
            PinRecord {
                identity,
                lease,
                state: PinState::Open,
                active: None,
                waiting: VecDeque::new(),
                sequence: 0,
            },
        );
        Ok(identity)
    }

    pub fn validate_register(&self, pin_id: PinId, lease: ActiveLease) -> Result<(), RuntimeError> {
        if lease.slot_id.get() >= self.pool_size {
            return Err(RuntimeError::internal(
                "pin lease references a Slot outside the configured pool",
            ));
        }
        if self.records.len() >= self.pool_size {
            return Err(RuntimeError::internal(
                "pin count cannot exceed configured pool size",
            ));
        }
        if self.records.contains_key(&pin_id) {
            return Err(RuntimeError::internal("pin id is already registered"));
        }
        if self.records.values().any(|record| {
            record.identity.slot_id == lease.slot_id
                || record.identity.lease_id == lease.lease_id
                || record.identity.reservation_request_id == lease.request_id
        }) {
            return Err(RuntimeError::internal(
                "pin lease aliases an existing pin owner",
            ));
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn identity(&self, pin_id: PinId) -> Option<PinIdentity> {
        self.records.get(&pin_id).map(|record| record.identity)
    }

    pub fn identities(&self) -> Vec<PinIdentity> {
        self.records
            .values()
            .map(|record| record.identity)
            .collect()
    }

    pub fn state(&self, identity: PinIdentity) -> Option<PinState> {
        self.exact_record(identity).map(|record| record.state)
    }

    pub fn active_call(&self, identity: PinIdentity) -> Option<PinnedCallLease> {
        self.exact_record(identity)
            .and_then(|record| record.active.map(|active| active.lease))
    }

    pub fn lease(&self, identity: PinIdentity) -> Option<ActiveLease> {
        self.exact_record(identity).map(|record| record.lease)
    }

    pub fn active_owner(
        &self,
        identity: PinIdentity,
    ) -> Option<Option<(PinnedCallLease, Option<RequestWireIdentity>)>> {
        self.exact_record(identity)
            .map(|record| record.active.map(|active| (active.lease, active.wire)))
    }

    pub fn waiting_for(&self, identity: PinIdentity) -> Option<Vec<WaitingPermit>> {
        self.exact_record(identity)
            .map(|record| record.waiting.iter().copied().collect())
    }

    pub fn validate_waiting(&self, identity: PinIdentity, permit: WaitingPermit) -> bool {
        self.exact_record(identity)
            .is_some_and(|record| record.waiting.contains(&permit))
    }

    pub fn expired_waiters(&self, now: Instant) -> Vec<(PinIdentity, WaitingPermit)> {
        self.records
            .values()
            .flat_map(|record| {
                record
                    .waiting
                    .iter()
                    .copied()
                    .filter(move |permit| permit.deadline.is_elapsed_at(now))
                    .map(move |permit| (record.identity, permit))
            })
            .collect()
    }

    pub fn call_for_request(&self, request_id: RequestId) -> Option<PinnedCallLease> {
        self.records.values().find_map(|record| {
            record
                .active
                .filter(|active| active.lease.request_id == request_id)
                .map(|active| active.lease)
        })
    }

    pub fn releases_on_close(&self, identity: PinIdentity) -> Option<bool> {
        self.exact_record(identity)
            .map(|record| record.active.is_none_or(|active| active.wire.is_none()))
    }

    pub fn waiting_count(&self, identity: PinIdentity) -> Option<usize> {
        self.exact_record(identity)
            .map(|record| record.waiting.len())
    }

    pub fn active_pin_leases(&self) -> Vec<ActiveLease> {
        self.records.values().map(|record| record.lease).collect()
    }

    pub fn waiting_permits(&self) -> Vec<WaitingPermit> {
        self.records
            .values()
            .flat_map(|record| record.waiting.iter().copied())
            .collect()
    }

    pub fn active_calls(&self) -> Vec<PinnedCallLease> {
        self.records
            .values()
            .filter_map(|record| record.active.map(|active| active.lease))
            .collect()
    }

    pub fn can_admit_direct(
        &self,
        identity: PinIdentity,
        request_id: RequestId,
        now: Instant,
        deadline: Deadline,
    ) -> Result<bool, RuntimeError> {
        deadline.require_remaining_at(now, TimeoutPhase::Pin)?;
        let record = self.require_exact_record(identity)?;
        self.require_open(record)?;
        self.require_unique_request(request_id)?;
        next_sequence(record)?;
        Ok(record.active.is_none() && record.waiting.is_empty())
    }

    pub fn admit_direct(
        &mut self,
        identity: PinIdentity,
        request_id: RequestId,
        deadline: Deadline,
        now: Instant,
    ) -> Result<PinnedCallLease, RuntimeError> {
        if !self.can_admit_direct(identity, request_id, now, deadline)? {
            return Err(RuntimeError::internal(
                "pin call cannot bypass its local FIFO",
            ));
        }
        let call = PinnedCallLease {
            pin: identity,
            request_id,
            deadline,
        };
        let record = self.require_exact_record_mut(identity)?;
        let sequence = next_sequence(record)?;
        record.active = Some(ActivePinCall {
            lease: call,
            wire: None,
        });
        record.sequence = sequence;
        Ok(call)
    }

    pub fn enqueue(
        &mut self,
        identity: PinIdentity,
        permit: WaitingPermit,
        now: Instant,
    ) -> Result<(), RuntimeError> {
        permit
            .deadline
            .require_remaining_at(now, TimeoutPhase::Pin)?;
        if permit.engine_epoch != identity.engine_epoch {
            return Err(RuntimeError::internal(
                "pin waiter epoch does not match pinned proxy",
            ));
        }
        self.require_unique_request(permit.request_id)?;
        let record = self.require_exact_record_mut(identity)?;
        if record.state != PinState::Open {
            return Err(RuntimeError::connection_closed("pinned proxy is closing"));
        }
        let sequence = next_sequence(record)?;
        record.waiting.push_back(permit);
        record.sequence = sequence;
        Ok(())
    }

    pub fn validate_bind_wire(
        &self,
        call: PinnedCallLease,
        wire: RequestWireIdentity,
    ) -> Result<bool, RuntimeError> {
        let Some(record) = self.exact_record(call.pin) else {
            return Ok(false);
        };
        let Some(active) = record.active else {
            return Ok(false);
        };
        if active.lease != call || !wire_matches_call(wire, call) {
            return Ok(false);
        }
        if active
            .wire
            .is_some_and(|current| current.generation != wire.generation)
        {
            return Err(RuntimeError::internal(
                "pin call generation change requires a reconnect acknowledgement",
            ));
        }
        next_sequence(record)?;
        Ok(true)
    }

    pub fn bind_wire(
        &mut self,
        call: PinnedCallLease,
        wire: RequestWireIdentity,
    ) -> Result<bool, RuntimeError> {
        if !self.validate_bind_wire(call, wire)? {
            return Ok(false);
        }
        let Some(record) = self.records.get_mut(&call.pin.pin_id) else {
            return Ok(false);
        };
        let sequence = next_sequence(record)?;
        let active = record.active.as_mut().ok_or_else(|| {
            RuntimeError::internal("validated pin call disappeared before wire binding")
        })?;
        active.wire = Some(wire);
        record.sequence = sequence;
        Ok(true)
    }

    pub fn validate_advance_generation(
        &self,
        call: PinnedCallLease,
        acknowledgement: ReconnectAck,
    ) -> Result<bool, RuntimeError> {
        let Some(record) = self.exact_record(call.pin) else {
            return Ok(false);
        };
        let Some(active) = record.active else {
            return Ok(false);
        };
        let Some(wire) = active.wire else {
            return Ok(false);
        };
        if active.lease != call
            || acknowledgement.engine_epoch != call.pin.engine_epoch
            || acknowledgement.slot_id != call.pin.slot_id
            || acknowledgement.request_id != call.request_id
            || wire.generation != Some(acknowledgement.retired_generation)
            || acknowledgement.next_generation == acknowledgement.retired_generation
        {
            return Ok(false);
        }
        next_sequence(record)?;
        Ok(true)
    }

    pub fn advance_generation(
        &mut self,
        call: PinnedCallLease,
        acknowledgement: ReconnectAck,
    ) -> Result<bool, RuntimeError> {
        if !self.validate_advance_generation(call, acknowledgement)? {
            return Ok(false);
        }
        let Some(record) = self.records.get_mut(&call.pin.pin_id) else {
            return Ok(false);
        };
        let sequence = next_sequence(record)?;
        let mut active = record.active.ok_or_else(|| {
            RuntimeError::internal("validated pin call disappeared before reconnect")
        })?;
        let wire = active.wire.ok_or_else(|| {
            RuntimeError::internal("validated pin wire disappeared before reconnect")
        })?;
        active.wire = Some(RequestWireIdentity {
            generation: Some(acknowledgement.next_generation),
            message: None,
            ..wire
        });
        record.active = Some(active);
        record.sequence = sequence;
        Ok(true)
    }

    pub fn plan_terminal(
        &self,
        wire: RequestWireIdentity,
        now: Instant,
    ) -> Result<Option<PinTerminalPlan>, RuntimeError> {
        let Some(record) = self.records.values().find(|record| {
            record.identity.engine_epoch == wire.engine_epoch
                && record.identity.slot_id == wire.slot_id
                && record.identity.lease_id == wire.lease_id
        }) else {
            return Ok(None);
        };
        let Some(active) = record.active else {
            return Ok(None);
        };
        if active.lease.request_id != wire.request_id || active.wire != Some(wire) {
            return Ok(None);
        }
        let mut expired = Vec::new();
        let mut promotion = None;
        if record.state == PinState::Open {
            for permit in &record.waiting {
                if permit.deadline.is_elapsed_at(now) {
                    expired.push(*permit);
                } else {
                    promotion = Some(*permit);
                    break;
                }
            }
        }
        Ok(Some(PinTerminalPlan {
            pin: record.identity,
            call: active.lease,
            wire,
            sequence: record.sequence,
            next_sequence: next_sequence(record)?,
            expired,
            promotion,
            released_lease: (record.state == PinState::Closing).then_some(record.lease),
        }))
    }

    pub fn commit_terminal(
        &mut self,
        plan: &PinTerminalPlan,
    ) -> Result<Option<PinAdvanceOutcome>, RuntimeError> {
        let mut remove_pin = false;
        let outcome = {
            let Some(record) = self.records.get_mut(&plan.pin.pin_id) else {
                return Ok(None);
            };
            if record.identity != plan.pin
                || record.sequence != plan.sequence
                || record.active.map(|active| active.lease) != Some(plan.call)
                || record.active.and_then(|active| active.wire) != Some(plan.wire)
            {
                return Ok(None);
            }
            if !record
                .waiting
                .iter()
                .take(plan.expired.len())
                .copied()
                .eq(plan.expired.iter().copied())
            {
                return Err(RuntimeError::internal(
                    "pin terminal plan no longer matches expired FIFO prefix",
                ));
            }
            let expected_live = record.waiting.get(plan.expired.len()).copied();
            if plan.promotion != expected_live
                || plan.released_lease
                    != (record.state == PinState::Closing).then_some(record.lease)
                || plan.next_sequence != next_sequence(record)?
            {
                return Err(RuntimeError::internal(
                    "pin terminal plan no longer matches the next FIFO action",
                ));
            }
            if record.state == PinState::Closing && !record.waiting.is_empty() {
                return Err(RuntimeError::internal(
                    "closing pin retained waiters before exact terminal",
                ));
            }
            for _ in 0..plan.expired.len() {
                record.waiting.pop_front();
            }
            let promotion = if let Some(expected) = plan.promotion {
                record.waiting.pop_front();
                let call = PinnedCallLease {
                    pin: record.identity,
                    request_id: expected.request_id,
                    deadline: expected.deadline,
                };
                record.active = Some(ActivePinCall {
                    lease: call,
                    wire: None,
                });
                Some((expected, call))
            } else {
                record.active = None;
                None
            };
            let released_lease = if record.state == PinState::Closing {
                remove_pin = true;
                plan.released_lease
            } else {
                None
            };
            record.sequence = plan.next_sequence;
            PinAdvanceOutcome {
                expired: plan.expired.clone(),
                promotion,
                released_lease,
            }
        };
        if remove_pin {
            self.records.remove(&plan.pin.pin_id);
        }
        Ok(Some(outcome))
    }

    pub fn cancel_waiting(
        &mut self,
        identity: PinIdentity,
        permit: WaitingPermit,
    ) -> Result<bool, RuntimeError> {
        let Some(record) = self.records.get_mut(&identity.pin_id) else {
            return Ok(false);
        };
        if record.identity != identity {
            return Ok(false);
        }
        let Some(index) = record
            .waiting
            .iter()
            .position(|candidate| *candidate == permit)
        else {
            return Ok(false);
        };
        let sequence = next_sequence(record)?;
        record.waiting.remove(index);
        record.sequence = sequence;
        Ok(true)
    }

    pub fn validate_begin_close(&self, identity: PinIdentity) -> Result<bool, RuntimeError> {
        let Some(record) = self.exact_record(identity) else {
            return Ok(false);
        };
        next_sequence(record)?;
        Ok(true)
    }

    pub fn begin_close(
        &mut self,
        identity: PinIdentity,
    ) -> Result<Option<PinCloseOutcome>, RuntimeError> {
        if !self.validate_begin_close(identity)? {
            return Ok(None);
        }
        let mut remove_pin = false;
        let outcome = {
            let Some(record) = self.records.get_mut(&identity.pin_id) else {
                return Ok(None);
            };
            if record.identity != identity {
                return Ok(None);
            }
            let sequence = next_sequence(record)?;
            record.state = PinState::Closing;
            let rejected = record.waiting.drain(..).collect();
            let withdrawn_unstarted = if record.active.is_some_and(|active| active.wire.is_none()) {
                record.active.take().map(|active| active.lease)
            } else {
                None
            };
            let waiting_for_request = record.active.map(|active| active.lease.request_id);
            let released_lease = if record.active.is_none() {
                remove_pin = true;
                Some(record.lease)
            } else {
                None
            };
            record.sequence = sequence;
            PinCloseOutcome {
                rejected,
                withdrawn_unstarted,
                released_lease,
                waiting_for_request,
            }
        };
        if remove_pin {
            self.records.remove(&identity.pin_id);
        }
        Ok(Some(outcome))
    }

    pub fn check_invariants(&self) -> Result<(), RuntimeError> {
        if self.records.len() > self.pool_size {
            return Err(RuntimeError::internal("pin count exceeds pool size"));
        }
        let mut request_owners = BTreeMap::new();
        for (pin_id, record) in &self.records {
            if *pin_id != record.identity.pin_id
                || record.identity.engine_epoch != record.lease.engine_epoch
                || record.identity.lease_id != record.lease.lease_id
                || record.identity.slot_id != record.lease.slot_id
                || record.identity.reservation_request_id != record.lease.request_id
            {
                return Err(RuntimeError::internal(
                    "pin identity does not match its lease",
                ));
            }
            record_request_owner(
                &mut request_owners,
                record.identity.reservation_request_id,
                *pin_id,
            )?;
            if record.state == PinState::Closing && !record.waiting.is_empty() {
                return Err(RuntimeError::internal(
                    "closing pin retained local FIFO waiters",
                ));
            }
            if let Some(active) = record.active {
                record_request_owner(&mut request_owners, active.lease.request_id, *pin_id)?;
                if active.lease.pin != record.identity {
                    return Err(RuntimeError::internal(
                        "active pin call references a different pin",
                    ));
                }
                if let Some(wire) = active.wire {
                    if !wire_matches_call(wire, active.lease) {
                        return Err(RuntimeError::internal(
                            "active pin wire identity does not match call owner",
                        ));
                    }
                }
            }
            for permit in &record.waiting {
                record_request_owner(&mut request_owners, permit.request_id, *pin_id)?;
                if permit.engine_epoch != record.identity.engine_epoch {
                    return Err(RuntimeError::internal(
                        "pin waiter epoch does not match pin owner",
                    ));
                }
            }
        }
        for first in self.records.values() {
            for second in self.records.values() {
                if first.identity.pin_id < second.identity.pin_id
                    && (first.identity.slot_id == second.identity.slot_id
                        || first.identity.lease_id == second.identity.lease_id)
                {
                    return Err(RuntimeError::internal("two pins alias the same Slot lease"));
                }
            }
        }
        Ok(())
    }

    fn exact_record(&self, identity: PinIdentity) -> Option<&PinRecord> {
        self.records
            .get(&identity.pin_id)
            .filter(|record| record.identity == identity)
    }

    fn require_exact_record(&self, identity: PinIdentity) -> Result<&PinRecord, RuntimeError> {
        self.exact_record(identity)
            .ok_or_else(|| RuntimeError::connection_closed("pinned proxy is no longer valid"))
    }

    fn require_exact_record_mut(
        &mut self,
        identity: PinIdentity,
    ) -> Result<&mut PinRecord, RuntimeError> {
        self.records
            .get_mut(&identity.pin_id)
            .filter(|record| record.identity == identity)
            .ok_or_else(|| RuntimeError::connection_closed("pinned proxy is no longer valid"))
    }

    fn require_open(&self, record: &PinRecord) -> Result<(), RuntimeError> {
        if record.state == PinState::Open {
            return Ok(());
        }
        Err(RuntimeError::connection_closed("pinned proxy is closing"))
    }

    fn require_unique_request(&self, request_id: RequestId) -> Result<(), RuntimeError> {
        if self.records.values().any(|record| {
            record.identity.reservation_request_id == request_id
                || record
                    .active
                    .is_some_and(|active| active.lease.request_id == request_id)
                || record
                    .waiting
                    .iter()
                    .any(|permit| permit.request_id == request_id)
        }) {
            return Err(RuntimeError::internal(
                "request already belongs to a pin owner",
            ));
        }
        Ok(())
    }
}

fn wire_matches_call(wire: RequestWireIdentity, call: PinnedCallLease) -> bool {
    wire.engine_epoch == call.pin.engine_epoch
        && wire.request_id == call.request_id
        && wire.lease_id == call.pin.lease_id
        && wire.slot_id == call.pin.slot_id
        && wire.generation.is_some()
}

fn record_request_owner(
    owners: &mut BTreeMap<RequestId, PinId>,
    request_id: RequestId,
    pin_id: PinId,
) -> Result<(), RuntimeError> {
    if owners.insert(request_id, pin_id).is_some() {
        return Err(
            RuntimeError::internal("request id aliases multiple pin ownership roles")
                .with_context("request_id", request_id.get().to_string()),
        );
    }
    Ok(())
}

fn next_sequence(record: &PinRecord) -> Result<u64, RuntimeError> {
    record
        .sequence
        .checked_add(1)
        .ok_or_else(|| RuntimeError::internal("pin mutation sequence exhausted"))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{PinId, PinRegistry, PinState};
    use crate::deadline::Deadline;
    use crate::error::RuntimeError;
    use crate::request::{
        ActiveLease, LeaseId, RequestWireIdentity, WaitingPermit, WaitingPermitId,
    };
    use crate::slot::{EngineEpoch, GenerationId, MessageIdentity, RequestId, SlotId};

    fn lease(request: u64) -> Result<ActiveLease, RuntimeError> {
        Ok(ActiveLease {
            engine_epoch: EngineEpoch::new(1)?,
            request_id: RequestId::new(request)?,
            lease_id: LeaseId::from_raw(1),
            slot_id: SlotId::new(0),
            deadline: Deadline::at(Instant::now() + Duration::from_secs(3)),
        })
    }

    fn permit(request: u64, id: u64, deadline: Deadline) -> Result<WaitingPermit, RuntimeError> {
        Ok(WaitingPermit {
            engine_epoch: EngineEpoch::new(1)?,
            request_id: RequestId::new(request)?,
            permit_id: WaitingPermitId::from_raw(id),
            deadline,
        })
    }

    fn wire(
        call: super::PinnedCallLease,
        generation: u64,
        message: u32,
    ) -> Result<RequestWireIdentity, RuntimeError> {
        Ok(RequestWireIdentity {
            engine_epoch: call.pin.engine_epoch,
            request_id: call.request_id,
            lease_id: call.pin.lease_id,
            slot_id: call.pin.slot_id,
            generation: Some(GenerationId::new(generation)?),
            message: Some(MessageIdentity::new(message, 0x044e)?),
        })
    }

    #[test]
    fn local_fifo_reuses_pin_lease_and_skips_expired_head() -> Result<(), RuntimeError> {
        let started = Instant::now();
        let terminal_now = started + Duration::from_millis(2);
        let mut pins = PinRegistry::new(1)?;
        let identity = pins.register(PinId::new(1)?, lease(1)?)?;
        let first = pins.admit_direct(
            identity,
            RequestId::new(2)?,
            Deadline::at(started + Duration::from_secs(2)),
            started,
        )?;
        let expired = permit(3, 1, Deadline::at(started + Duration::from_millis(1)))?;
        let live = permit(4, 2, Deadline::at(started + Duration::from_secs(2)))?;
        pins.enqueue(identity, expired, started)?;
        pins.enqueue(identity, live, started)?;
        let first_wire = wire(first, 1, 10)?;
        assert!(pins.bind_wire(first, first_wire)?);

        let plan = pins
            .plan_terminal(first_wire, terminal_now)?
            .ok_or_else(|| RuntimeError::internal("terminal plan is missing"))?;
        assert_eq!(plan.expired(), &[expired]);
        assert_eq!(plan.promotion(), Some(live));
        let outcome = pins
            .commit_terminal(&plan)?
            .ok_or_else(|| RuntimeError::internal("terminal plan was rejected"))?;
        let (_, promoted) = outcome
            .promotion
            .ok_or_else(|| RuntimeError::internal("live waiter was not promoted"))?;
        assert_eq!(promoted.request_id, live.request_id);
        assert_eq!(promoted.pin.lease_id, first.pin.lease_id);
        assert_eq!(pins.active_call(identity), Some(promoted));
        pins.check_invariants()?;
        Ok(())
    }

    #[test]
    fn close_keeps_started_call_until_exact_terminal_then_releases_once() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let mut pins = PinRegistry::new(1)?;
        let identity = pins.register(PinId::new(1)?, lease(1)?)?;
        let call = pins.admit_direct(
            identity,
            RequestId::new(2)?,
            Deadline::at(now + Duration::from_secs(2)),
            now,
        )?;
        let current = wire(call, 1, 10)?;
        assert!(pins.bind_wire(call, current)?);

        let close = pins
            .begin_close(identity)?
            .ok_or_else(|| RuntimeError::internal("pin close was rejected"))?;
        assert_eq!(close.waiting_for_request, Some(call.request_id));
        assert_eq!(close.released_lease, None);
        assert_eq!(pins.state(identity), Some(PinState::Closing));
        let stale = RequestWireIdentity {
            generation: Some(GenerationId::new(2)?),
            ..current
        };
        assert_eq!(pins.plan_terminal(stale, now)?, None);

        let plan = pins
            .plan_terminal(current, now)?
            .ok_or_else(|| RuntimeError::internal("exact terminal plan is missing"))?;
        let outcome = pins
            .commit_terminal(&plan)?
            .ok_or_else(|| RuntimeError::internal("exact terminal was rejected"))?;
        assert!(outcome.released_lease.is_some());
        assert_eq!(pins.identity(identity.pin_id), None);
        assert_eq!(pins.commit_terminal(&plan)?, None);
        Ok(())
    }

    #[test]
    fn old_proxy_identity_is_permanently_stale() -> Result<(), RuntimeError> {
        let mut pins = PinRegistry::new(1)?;
        let identity = pins.register(PinId::new(1)?, lease(1)?)?;
        let close = pins
            .begin_close(identity)?
            .ok_or_else(|| RuntimeError::internal("pin close was rejected"))?;
        assert!(close.released_lease.is_some());

        let stale = super::PinIdentity {
            engine_epoch: EngineEpoch::new(2)?,
            ..identity
        };
        assert!(pins
            .can_admit_direct(
                stale,
                RequestId::new(2)?,
                Deadline::at(Instant::now() + Duration::from_secs(1)),
                Instant::now(),
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn close_withdraws_unstarted_call_and_releases_pin_lease() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut pins = PinRegistry::new(1)?;
        let identity = pins.register(PinId::new(1)?, lease(1)?)?;
        pins.admit_direct(
            identity,
            RequestId::new(2)?,
            Deadline::at(now + Duration::from_secs(1)),
            now,
        )?;

        let close = pins
            .begin_close(identity)?
            .ok_or_else(|| RuntimeError::internal("pin close was rejected"))?;
        assert_eq!(close.waiting_for_request, None);
        assert!(close.withdrawn_unstarted.is_some());
        assert!(close.released_lease.is_some());
        assert_eq!(pins.identity(identity.pin_id), None);
        Ok(())
    }

    #[test]
    fn sequence_exhaustion_never_partially_cancels_a_waiter() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut pins = PinRegistry::new(1)?;
        let identity = pins.register(PinId::new(1)?, lease(1)?)?;
        pins.admit_direct(
            identity,
            RequestId::new(2)?,
            Deadline::at(now + Duration::from_secs(2)),
            now,
        )?;
        let waiting = permit(3, 1, Deadline::at(now + Duration::from_secs(2)))?;
        pins.enqueue(identity, waiting, now)?;
        let record = pins
            .records
            .get_mut(&identity.pin_id)
            .ok_or_else(|| RuntimeError::internal("pin record is missing"))?;
        record.sequence = u64::MAX;

        assert!(pins.cancel_waiting(identity, waiting).is_err());
        assert!(pins.validate_waiting(identity, waiting));
        assert_eq!(pins.waiting_count(identity), Some(1));
        assert!(pins.active_call(identity).is_some());
        Ok(())
    }
}
