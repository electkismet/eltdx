use crate::error::RuntimeError;
use crate::slot::{EngineEpoch, Slot, SlotId, SlotState};
use crate::supervisor::{EngineState, Supervisor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeState {
    Starting,
    Running,
    Closing,
    Stopped,
    Failed,
    FailedClosing,
    FailedClosed,
}

impl RuntimeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Closing => "CLOSING",
            Self::Failed => "FAILED",
            Self::FailedClosing => "FAILED_CLOSING",
            Self::FailedClosed => "FAILED_CLOSED",
        }
    }
}

impl From<EngineState> for RuntimeState {
    fn from(value: EngineState) -> Self {
        match value {
            EngineState::Stopped => Self::Stopped,
            EngineState::Starting => Self::Starting,
            EngineState::Running => Self::Running,
            EngineState::Closing => Self::Closing,
            EngineState::Failed => Self::Failed,
            EngineState::FailedClosing => Self::FailedClosing,
            EngineState::FailedClosed => Self::FailedClosed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolState {
    Starting,
    Running,
    Closing,
    Stopped,
    Failed,
    FailedClosing,
    FailedClosed,
}

impl PoolState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Closing => "CLOSING",
            Self::Failed => "FAILED",
            Self::FailedClosing => "FAILED_CLOSING",
            Self::FailedClosed => "FAILED_CLOSED",
        }
    }
}

impl From<EngineState> for PoolState {
    fn from(value: EngineState) -> Self {
        match value {
            EngineState::Stopped => Self::Stopped,
            EngineState::Starting => Self::Starting,
            EngineState::Running => Self::Running,
            EngineState::Closing => Self::Closing,
            EngineState::Failed => Self::Failed,
            EngineState::FailedClosing => Self::FailedClosing,
            EngineState::FailedClosed => Self::FailedClosed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpState {
    Down,
    Connecting,
    ConnectedUnhandshaken,
    Handshaking,
    Ready,
    Retiring,
}

impl TcpState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Down => "DOWN",
            Self::Connecting => "CONNECTING",
            Self::ConnectedUnhandshaken => "CONNECTED_UNHANDSHAKEN",
            Self::Handshaking => "HANDSHAKING",
            Self::Ready => "READY",
            Self::Retiring => "RETIRING",
        }
    }
}

impl From<SlotState> for TcpState {
    fn from(value: SlotState) -> Self {
        match value {
            SlotState::Disconnected => Self::Down,
            SlotState::Connecting => Self::Connecting,
            SlotState::ConnectedUnhandshaken => Self::ConnectedUnhandshaken,
            SlotState::Handshaking => Self::Handshaking,
            SlotState::Idle | SlotState::Busy => Self::Ready,
            SlotState::Retiring => Self::Retiring,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActorSnapshot {
    slot_id: SlotId,
    pub runtime_epoch: u64,
    pub state: RuntimeState,
    pub tcp_state: TcpState,
    pub tcp_generation: u64,
    pub connected_host: Option<String>,
    pub actor_alive: bool,
    pub pending_depth: usize,
    pub reconnect_count: u64,
    pub stale_event_count: u64,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SlotSnapshot {
    pub slot_id: SlotId,
    pub engine_epoch: EngineEpoch,
    pub tcp_state: TcpState,
    pub tcp_generation: u64,
    pub connected_host: Option<String>,
    pub actor_alive: bool,
    pub pending_depth: usize,
    pub reconnect_count: u64,
    pub stale_event_count: u64,
    pub last_error: Option<String>,
}

impl SlotSnapshot {
    pub fn capture(slot: &Slot, actor_alive: bool) -> Self {
        Self {
            slot_id: slot.slot_id(),
            engine_epoch: slot.engine_epoch(),
            tcp_state: slot.state().into(),
            tcp_generation: slot.tcp_generation().get(),
            connected_host: slot.connected_host().map(str::to_owned),
            actor_alive,
            pending_depth: slot.pending_depth(),
            reconnect_count: slot.reconnect_count(),
            stale_event_count: slot.stale_event_count(),
            last_error: slot.last_error().map(str::to_owned),
        }
    }
}

impl ActorSnapshot {
    pub fn capture(engine_state: EngineState, slot: &Slot, actor_alive: bool) -> Self {
        Self::from_slot_snapshot(engine_state, SlotSnapshot::capture(slot, actor_alive))
    }

    fn from_slot_snapshot(engine_state: EngineState, slot: SlotSnapshot) -> Self {
        Self {
            slot_id: slot.slot_id,
            runtime_epoch: slot.engine_epoch.get(),
            state: engine_state.into(),
            tcp_state: slot.tcp_state,
            tcp_generation: slot.tcp_generation,
            connected_host: slot.connected_host,
            actor_alive: slot.actor_alive,
            pending_depth: slot.pending_depth,
            reconnect_count: slot.reconnect_count,
            stale_event_count: slot.stale_event_count,
            last_error: slot.last_error,
        }
    }

    pub const fn slot_id(&self) -> SlotId {
        self.slot_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokerSnapshot {
    pub pool_epoch: u64,
    pub idle_slots: usize,
    pub waiter_count: usize,
    pub pin_waiter_count: usize,
    pub active_leases: usize,
    pub closed: bool,
}

impl BrokerSnapshot {
    fn capture(supervisor: &Supervisor) -> Option<Self> {
        supervisor.broker_epoch().map(|epoch| Self {
            pool_epoch: epoch.get(),
            idle_slots: supervisor.idle_slot_count(),
            waiter_count: supervisor.ordinary_waiter_count(),
            pin_waiter_count: supervisor.pin_waiter_count(),
            active_leases: supervisor.active_count(),
            closed: supervisor.broker_closed(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolDiagnostics {
    pub epoch: u64,
    pub state: PoolState,
    pub broker: Option<BrokerSnapshot>,
    pub actors: Vec<ActorSnapshot>,
    pub push_frames: usize,
    pub push_bytes: usize,
    pub push_dropped: u64,
}

impl PoolDiagnostics {
    pub fn capture<'a>(
        supervisor: &Supervisor,
        slots: impl IntoIterator<Item = (&'a Slot, bool)>,
    ) -> Result<Self, RuntimeError> {
        Self::capture_snapshots(
            supervisor,
            slots
                .into_iter()
                .map(|(slot, alive)| SlotSnapshot::capture(slot, alive)),
        )
    }

    pub(crate) fn capture_snapshots(
        supervisor: &Supervisor,
        slots: impl IntoIterator<Item = SlotSnapshot>,
    ) -> Result<Self, RuntimeError> {
        let actors = capture_actor_snapshots(supervisor, slots)?;
        let push = supervisor.push_snapshot();
        Ok(Self {
            epoch: supervisor.diagnostic_epoch(),
            state: supervisor.state().into(),
            broker: BrokerSnapshot::capture(supervisor),
            actors,
            push_frames: push.map_or(0, |snapshot| snapshot.frame_count),
            push_bytes: push.map_or(0, |snapshot| snapshot.byte_count),
            push_dropped: push.map_or(0, |snapshot| snapshot.dropped_total),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportDiagnostics {
    pub epoch: u64,
    pub actor: Option<ActorSnapshot>,
    pub push_frames: usize,
    pub push_bytes: usize,
    pub push_dropped: u64,
    pub push_max_frames: usize,
    pub push_max_bytes: usize,
}

impl TransportDiagnostics {
    pub fn capture<'a>(
        supervisor: &Supervisor,
        slots: impl IntoIterator<Item = (&'a Slot, bool)>,
    ) -> Result<Self, RuntimeError> {
        Self::capture_snapshots(
            supervisor,
            slots
                .into_iter()
                .map(|(slot, alive)| SlotSnapshot::capture(slot, alive)),
        )
    }

    pub(crate) fn capture_snapshots(
        supervisor: &Supervisor,
        slots: impl IntoIterator<Item = SlotSnapshot>,
    ) -> Result<Self, RuntimeError> {
        if supervisor.pool_size() != 1 {
            return Err(RuntimeError::internal(
                "standalone transport diagnostics require exactly one configured Slot",
            ));
        }
        let mut actors = capture_actor_snapshots(supervisor, slots)?;
        let actor = actors.pop();
        let push = supervisor.push_snapshot();
        Ok(Self {
            epoch: supervisor.diagnostic_epoch(),
            actor,
            push_frames: push.map_or(0, |snapshot| snapshot.frame_count),
            push_bytes: push.map_or(0, |snapshot| snapshot.byte_count),
            push_dropped: push.map_or(0, |snapshot| snapshot.dropped_total),
            push_max_frames: push.map_or(0, |snapshot| snapshot.max_frames_observed),
            push_max_bytes: push.map_or(0, |snapshot| snapshot.max_bytes_observed),
        })
    }
}

fn capture_actor_snapshots(
    supervisor: &Supervisor,
    slots: impl IntoIterator<Item = SlotSnapshot>,
) -> Result<Vec<ActorSnapshot>, RuntimeError> {
    let expected_epoch = supervisor.diagnostic_owner_epoch();
    let expected_slots = supervisor.instantiated_slots();
    let mut actors = Vec::with_capacity(expected_slots.len());
    for slot in slots {
        if expected_epoch != Some(slot.engine_epoch)
            || expected_slots.binary_search(&slot.slot_id).is_err()
        {
            return Err(RuntimeError::internal(
                "diagnostics received a Slot outside the current engine owner",
            ));
        }
        actors.push(ActorSnapshot::from_slot_snapshot(supervisor.state(), slot));
    }
    actors.sort_by_key(ActorSnapshot::slot_id);
    if actors.len() != expected_slots.len()
        || actors
            .iter()
            .map(ActorSnapshot::slot_id)
            .ne(expected_slots.iter().copied())
    {
        return Err(RuntimeError::internal(
            "diagnostics must include every instantiated Slot exactly once",
        ));
    }
    Ok(actors)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use bytes::Bytes;
    use eltdx_protocol::frame::{ResponseFrame, ResponseHeader};

    use super::{ActorSnapshot, PoolDiagnostics, PoolState, RuntimeState, TcpState};
    use crate::deadline::Deadline;
    use crate::endpoint::{Endpoint, EndpointRotation};
    use crate::error::RuntimeError;
    use crate::push::PushFrame;
    use crate::request::{Admission, RetryPolicy};
    use crate::slot::{
        EngineEpoch, GenerationId, GenerationIdentity, HeartbeatCandidate, RequestId, Slot, SlotId,
        SlotState,
    };
    use crate::supervisor::{CloseClaim, EngineState, StartClaim, Supervisor};

    fn start(supervisor: &mut Supervisor) -> Result<EngineEpoch, RuntimeError> {
        let attempt = match supervisor.begin_start()? {
            StartClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected start owner, got {other:?}"
                )))
            }
        };
        let epoch = attempt.candidate_epoch();
        if !supervisor.publish_start(attempt)? {
            return Err(RuntimeError::internal("start publication was rejected"));
        }
        Ok(epoch)
    }

    fn slot(epoch: EngineEpoch, slot_id: usize) -> Result<Slot, RuntimeError> {
        Slot::new(
            epoch,
            SlotId::new(slot_id),
            EndpointRotation::new(vec![Endpoint::numeric("127.0.0.1:7709")?], 0)?,
        )
    }

    fn active(admission: Admission) -> Result<crate::request::ActiveLease, RuntimeError> {
        match admission {
            Admission::Active(lease) => Ok(lease),
            Admission::Waiting(_) | Admission::Pinned(_) => {
                Err(RuntimeError::internal("expected active admission"))
            }
        }
    }

    fn push_frame(epoch: EngineEpoch) -> Result<PushFrame, RuntimeError> {
        let header = ResponseHeader {
            control: 0,
            msg_id: 1,
            reserved: 0,
            msg_type: 0x0547,
            zip_length: 1,
            length: 1,
        };
        let mut raw = vec![0_u8; 17];
        raw[..4].copy_from_slice(&[0xb1, 0x49, 0x53, 0x68]);
        raw[5..9].copy_from_slice(&1_u32.to_le_bytes());
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

    #[test]
    fn public_state_names_map_exhaustively() {
        let engine_states = [
            EngineState::Stopped,
            EngineState::Starting,
            EngineState::Running,
            EngineState::Closing,
            EngineState::Failed,
            EngineState::FailedClosing,
            EngineState::FailedClosed,
        ];
        let expected = [
            "STOPPED",
            "STARTING",
            "RUNNING",
            "CLOSING",
            "FAILED",
            "FAILED_CLOSING",
            "FAILED_CLOSED",
        ];
        for (state, name) in engine_states.into_iter().zip(expected) {
            assert_eq!(RuntimeState::from(state).as_str(), name);
            assert_eq!(PoolState::from(state).as_str(), name);
        }

        let tcp_states = [
            SlotState::Disconnected,
            SlotState::Connecting,
            SlotState::ConnectedUnhandshaken,
            SlotState::Handshaking,
            SlotState::Idle,
            SlotState::Busy,
            SlotState::Retiring,
        ];
        let tcp_expected = [
            "DOWN",
            "CONNECTING",
            "CONNECTED_UNHANDSHAKEN",
            "HANDSHAKING",
            "READY",
            "READY",
            "RETIRING",
        ];
        for (state, name) in tcp_states.into_iter().zip(tcp_expected) {
            assert_eq!(TcpState::from(state).as_str(), name);
        }
    }

    #[test]
    fn actors_are_exactly_instantiated_and_sorted_by_slot() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::with_admission(2, 2)?;
        let epoch = start(&mut supervisor)?;
        assert!(supervisor.register_slot(epoch, SlotId::new(1))?);
        assert!(supervisor.register_slot(epoch, SlotId::new(0))?);
        let first = slot(epoch, 0)?;
        let second = slot(epoch, 1)?;

        let diagnostics =
            PoolDiagnostics::capture(&supervisor, [(&second, false), (&first, true)])?;
        assert_eq!(
            diagnostics
                .actors
                .iter()
                .map(ActorSnapshot::slot_id)
                .collect::<Vec<_>>(),
            vec![SlotId::new(0), SlotId::new(1)]
        );
        assert!(diagnostics.actors[0].actor_alive);
        assert!(!diagnostics.actors[1].actor_alive);
        assert_eq!(diagnostics.actors[0].runtime_epoch, epoch.get());
        assert_eq!(diagnostics.actors[0].tcp_state, TcpState::Down);
        assert_eq!(diagnostics.actors[0].tcp_generation, 1);
        assert!(PoolDiagnostics::capture(&supervisor, [(&first, true)]).is_err());
        Ok(())
    }

    #[test]
    fn public_epoch_tracks_runtime_candidates_not_close_invalidation() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::with_admission(1, 1)?;
        let first = start(&mut supervisor)?;
        let close = match supervisor.begin_close()? {
            CloseClaim::Owner(attempt) => attempt,
            other => {
                return Err(RuntimeError::internal(format!(
                    "expected close owner, got {other:?}"
                )))
            }
        };
        assert!(supervisor.finish_close(close, Ok(())));
        let stopped = PoolDiagnostics::capture(&supervisor, [])?;
        assert_eq!(stopped.epoch, first.get());
        assert_eq!(stopped.state, PoolState::Stopped);
        assert!(stopped.broker.is_none());
        assert!(supervisor.epoch() > stopped.epoch);

        let second = start(&mut supervisor)?;
        let running = PoolDiagnostics::capture(&supervisor, [])?;
        assert_eq!(running.epoch, second.get());
        assert!(second.get() > first.get());
        Ok(())
    }

    #[test]
    fn broker_snapshot_splits_waiters_and_excludes_heartbeat_leases() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let deadline = Deadline::at(now + Duration::from_secs(3));
        let mut supervisor = Supervisor::with_admission(2, 2)?;
        let epoch = start(&mut supervisor)?;
        let reservation = active(supervisor.submit(RequestId::new(1)?, deadline, now)?)?;
        let pin = supervisor.open_pin(reservation.request_id)?;
        supervisor.submit_pin(
            pin,
            RequestId::new(2)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?;
        supervisor.submit(RequestId::new(3)?, deadline, now)?;
        supervisor.submit(RequestId::new(4)?, deadline, now)?;
        supervisor.submit_pin(
            pin,
            RequestId::new(5)?,
            deadline,
            RetryPolicy::ordinary(true),
            now,
        )?;
        let diagnostics = PoolDiagnostics::capture(&supervisor, [])?;
        let broker = diagnostics
            .broker
            .ok_or_else(|| RuntimeError::internal("running broker snapshot is missing"))?;
        assert_eq!(broker.pool_epoch, epoch.get());
        assert_eq!(broker.idle_slots, 0);
        assert_eq!(broker.waiter_count, 1);
        assert_eq!(broker.pin_waiter_count, 1);
        assert_eq!(broker.active_leases, 2);
        assert!(!broker.closed);

        let mut heartbeat_supervisor = Supervisor::with_admission(1, 1)?;
        let heartbeat_epoch = start(&mut heartbeat_supervisor)?;
        let claim = heartbeat_supervisor.claim_heartbeat(
            RequestId::new(10)?,
            HeartbeatCandidate {
                generation: GenerationIdentity {
                    engine_epoch: heartbeat_epoch,
                    slot_id: SlotId::new(0),
                    generation: GenerationId::new(1)?,
                },
                observed_last_activity: now,
            },
            Duration::from_secs(1),
            now,
        )?;
        assert!(claim.is_some());
        let heartbeat = PoolDiagnostics::capture(&heartbeat_supervisor, [])?
            .broker
            .ok_or_else(|| RuntimeError::internal("heartbeat broker snapshot is missing"))?;
        assert_eq!(heartbeat.idle_slots, 0);
        assert_eq!(heartbeat.active_leases, 0);
        Ok(())
    }

    #[test]
    fn transport_diagnostics_report_observed_push_peaks() -> Result<(), RuntimeError> {
        let mut supervisor = Supervisor::with_limits(1, 1, 4, 256)?;
        let epoch = start(&mut supervisor)?;
        assert!(supervisor.offer_push(push_frame(epoch)?));
        let diagnostics = super::TransportDiagnostics::capture(&supervisor, [])?;
        assert_eq!(diagnostics.epoch, epoch.get());
        assert_eq!(diagnostics.push_frames, 1);
        assert_eq!(diagnostics.push_bytes, 17);
        assert_eq!(diagnostics.push_dropped, 0);
        assert_eq!(diagnostics.push_max_frames, 1);
        assert_eq!(diagnostics.push_max_bytes, 17);
        assert!(diagnostics.actor.is_none());
        Ok(())
    }
}
