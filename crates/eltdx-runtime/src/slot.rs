use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eltdx_protocol::frame::{ResponseFrame, ResponseFrameDecoder};
use eltdx_protocol::limits::{
    MAX_DECODED_QUEUE_BYTES, MAX_DECODED_QUEUE_FRAMES, MAX_RAW_STAGING_BUFFER_SIZE,
    MAX_RESPONSE_PAYLOAD_SIZE, MAX_RESPONSE_RESYNC_BYTES, SLOT_DECODED_BUDGET_BYTES,
    SLOT_FRAME_BUDGET, SLOT_WIRE_BUDGET_BYTES,
};
use eltdx_protocol::ProtocolError;

use crate::deadline::Deadline;
use crate::endpoint::{EndpointAttempt, EndpointRotation};
use crate::error::RuntimeError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EngineEpoch(u64);

impl EngineEpoch {
    pub fn new(value: u64) -> Result<Self, RuntimeError> {
        nonzero_identity("engine epoch", value).map(Self)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SlotId(usize);

impl SlotId {
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenerationId(u64);

impl GenerationId {
    pub fn new(value: u64) -> Result<Self, RuntimeError> {
        nonzero_identity("TCP generation", value).map(Self)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(u64);

impl RequestId {
    pub fn new(value: u64) -> Result<Self, RuntimeError> {
        nonzero_identity("request id", value).map(Self)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageIdentity {
    msg_id: u32,
    msg_type: u16,
}

impl MessageIdentity {
    pub fn new(msg_id: u32, msg_type: u16) -> Result<Self, RuntimeError> {
        if msg_id == 0 {
            return Err(RuntimeError::internal("message id must be nonzero"));
        }
        Ok(Self { msg_id, msg_type })
    }

    pub const fn msg_id(self) -> u32 {
        self.msg_id
    }

    pub const fn msg_type(self) -> u16 {
        self.msg_type
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationIdentity {
    pub engine_epoch: EngineEpoch,
    pub slot_id: SlotId,
    pub generation: GenerationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameIdentity {
    pub generation: GenerationIdentity,
    pub message: MessageIdentity,
    pub receive_sequence: u64,
    pub send_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExchangeKind {
    Handshake { terminal: bool },
    Business,
    Heartbeat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchedExchange {
    pub request_id: RequestId,
    pub message: MessageIdentity,
    pub kind: ExchangeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDisposition {
    Matched(MatchedExchange),
    Push,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotState {
    Disconnected,
    Connecting,
    ConnectedUnhandshaken,
    Handshaking,
    Idle,
    Busy,
    Retiring,
}

impl SlotState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::ConnectedUnhandshaken => "connected_unhandshaken",
            Self::Handshaking => "handshaking",
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Retiring => "retiring",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectStart {
    pub identity: GenerationIdentity,
    pub attempt: EndpointAttempt,
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetiredGeneration {
    pub request_id: Option<RequestId>,
    pub next_generation: GenerationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconnectAck {
    pub(crate) engine_epoch: EngineEpoch,
    pub(crate) slot_id: SlotId,
    pub(crate) request_id: RequestId,
    pub(crate) retired_generation: GenerationId,
    pub(crate) next_generation: GenerationId,
    pub(crate) next_endpoint_index: usize,
    pub(crate) endpoints_remaining_in_attempt: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeartbeatCandidate {
    pub generation: GenerationIdentity,
    pub observed_last_activity: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueuedDecodedFrame {
    response: ResponseFrame,
    receive_sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotDecodeTurn {
    pub frames_added: usize,
    pub decoded_bytes_added: usize,
    pub queue_frames: usize,
    pub queue_bytes: usize,
    pub budget_exhausted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutedResponse {
    pub frame: FrameIdentity,
    pub disposition: FrameDisposition,
    pub response: ResponseFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExchangeIdentity {
    request_id: RequestId,
    message: MessageIdentity,
    receive_boundary: u64,
    kind: ExchangeKind,
}

#[derive(Debug)]
struct SlotGeneration {
    id: GenerationId,
    endpoint_index: usize,
    endpoint_host: String,
    connect_deadline: Deadline,
    decoder: ResponseFrameDecoder,
    decoded_frames: VecDeque<QueuedDecodedFrame>,
    decoded_bytes: usize,
    receive_sequence: u64,
    exchange: Option<ExchangeIdentity>,
    last_activity_at: Instant,
}

#[derive(Debug)]
pub struct Slot {
    engine_epoch: EngineEpoch,
    slot_id: SlotId,
    state: SlotState,
    next_generation: GenerationId,
    generation: Option<SlotGeneration>,
    endpoint_rotation: EndpointRotation,
    active_request: Option<RequestId>,
    connected_host: Option<String>,
    reconnect_count: u64,
    stale_event_count: u64,
    last_error: Option<String>,
}

impl Slot {
    pub fn new(
        engine_epoch: EngineEpoch,
        slot_id: SlotId,
        endpoint_rotation: EndpointRotation,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            engine_epoch,
            slot_id,
            state: SlotState::Disconnected,
            next_generation: GenerationId::new(1)?,
            generation: None,
            endpoint_rotation,
            active_request: None,
            connected_host: None,
            reconnect_count: 0,
            stale_event_count: 0,
            last_error: None,
        })
    }

    pub const fn engine_epoch(&self) -> EngineEpoch {
        self.engine_epoch
    }

    pub const fn slot_id(&self) -> SlotId {
        self.slot_id
    }

    pub const fn state(&self) -> SlotState {
        self.state
    }

    pub const fn tcp_generation(&self) -> GenerationId {
        match &self.generation {
            Some(generation) => generation.id,
            None => self.next_generation,
        }
    }

    pub const fn active_request(&self) -> Option<RequestId> {
        self.active_request
    }

    pub fn connected_host(&self) -> Option<&str> {
        self.connected_host.as_deref()
    }

    pub fn pending_depth(&self) -> usize {
        if self.active_request.is_some() {
            1
        } else {
            0
        }
    }

    pub const fn reconnect_count(&self) -> u64 {
        self.reconnect_count
    }

    pub const fn stale_event_count(&self) -> u64 {
        self.stale_event_count
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn begin_endpoint_attempt(&mut self) -> Result<(), RuntimeError> {
        self.require_state(SlotState::Disconnected, "begin endpoint attempt")?;
        if self.generation.is_some() || self.active_request.is_some() {
            return Err(
                self.invariant_error("disconnected Slot retained generation or request ownership")
            );
        }
        self.endpoint_rotation.begin_attempt();
        Ok(())
    }

    pub fn start_connect(
        &mut self,
        request_id: RequestId,
        attempt_deadline: Deadline,
        now: Instant,
    ) -> Result<Option<ConnectStart>, RuntimeError> {
        self.require_state(SlotState::Disconnected, "start connect")?;
        if self.generation.is_some() || self.active_request.is_some() {
            return Err(
                self.invariant_error("disconnected Slot retained generation or request ownership")
            );
        }
        let decoder = ResponseFrameDecoder::with_limits(
            MAX_RESPONSE_PAYLOAD_SIZE,
            MAX_RAW_STAGING_BUFFER_SIZE,
            MAX_RESPONSE_RESYNC_BYTES,
        )?;
        let Some(attempt) = self.endpoint_rotation.next(attempt_deadline, now)? else {
            return Ok(None);
        };
        let identity = GenerationIdentity {
            engine_epoch: self.engine_epoch,
            slot_id: self.slot_id,
            generation: self.next_generation,
        };
        self.generation = Some(SlotGeneration {
            id: self.next_generation,
            endpoint_index: attempt.endpoint_index,
            endpoint_host: attempt.endpoint.host().to_owned(),
            connect_deadline: attempt.deadline,
            decoder,
            decoded_frames: VecDeque::new(),
            decoded_bytes: 0,
            receive_sequence: 0,
            exchange: None,
            last_activity_at: now,
        });
        self.active_request = Some(request_id);
        self.state = SlotState::Connecting;
        Ok(Some(ConnectStart {
            identity,
            attempt,
            request_id,
        }))
    }

    pub fn on_connected(
        &mut self,
        identity: GenerationIdentity,
        now: Instant,
    ) -> Result<bool, RuntimeError> {
        if !self.accept_generation_event(identity, SlotState::Connecting) {
            return Ok(false);
        }
        let missing_generation = self.invariant_error("connecting Slot has no generation");
        let generation = self.generation.as_mut().ok_or(missing_generation)?;
        generation.last_activity_at = now;
        self.connected_host = Some(generation.endpoint_host.clone());
        self.state = SlotState::ConnectedUnhandshaken;
        Ok(true)
    }

    pub fn assign_ready(&mut self, request_id: RequestId) -> Result<(), RuntimeError> {
        self.require_state(SlotState::Idle, "assign ready request")?;
        if self.active_request.is_some() {
            return Err(self.invariant_error("idle Slot already owns a request"));
        }
        self.active_request = Some(request_id);
        Ok(())
    }

    pub fn begin_handshake(
        &mut self,
        message: MessageIdentity,
        receive_boundary: u64,
        terminal: bool,
    ) -> Result<(), RuntimeError> {
        self.require_state(SlotState::ConnectedUnhandshaken, "begin handshake")?;
        self.install_exchange(
            message,
            receive_boundary,
            ExchangeKind::Handshake { terminal },
        )?;
        self.state = SlotState::Handshaking;
        Ok(())
    }

    pub fn begin_business(
        &mut self,
        message: MessageIdentity,
        receive_boundary: u64,
    ) -> Result<(), RuntimeError> {
        self.require_state(SlotState::Idle, "begin business exchange")?;
        self.install_exchange(message, receive_boundary, ExchangeKind::Business)?;
        self.state = SlotState::Busy;
        Ok(())
    }

    pub fn begin_heartbeat(
        &mut self,
        message: MessageIdentity,
        receive_boundary: u64,
    ) -> Result<(), RuntimeError> {
        self.require_state(SlotState::Idle, "begin heartbeat exchange")?;
        self.install_exchange(message, receive_boundary, ExchangeKind::Heartbeat)?;
        self.state = SlotState::Busy;
        Ok(())
    }

    pub fn on_frame(&mut self, frame: FrameIdentity, now: Instant) -> FrameDisposition {
        if !self.matches_generation(frame.generation) {
            self.record_stale();
            return FrameDisposition::Stale;
        }
        let Some(generation) = self.generation.as_mut() else {
            self.record_stale();
            return FrameDisposition::Stale;
        };
        generation.last_activity_at = now;
        let Some(exchange) = generation.exchange else {
            return FrameDisposition::Push;
        };
        let expected_state = match exchange.kind {
            ExchangeKind::Handshake { .. } => SlotState::Handshaking,
            ExchangeKind::Business | ExchangeKind::Heartbeat => SlotState::Busy,
        };
        if self.state != expected_state
            || frame.message != exchange.message
            || !frame.send_complete
            || frame.receive_sequence <= exchange.receive_boundary
        {
            return FrameDisposition::Push;
        }

        generation.exchange = None;
        self.state = SlotState::Idle;
        if matches!(exchange.kind, ExchangeKind::Handshake { terminal: true }) {
            self.active_request = None;
        }
        FrameDisposition::Matched(MatchedExchange {
            request_id: exchange.request_id,
            message: exchange.message,
            kind: exchange.kind,
        })
    }

    pub fn finish_heartbeat(&mut self, request_id: RequestId) -> Result<bool, RuntimeError> {
        self.release_unstarted_request(request_id)
    }

    pub fn finish_business(&mut self, request_id: RequestId) -> Result<bool, RuntimeError> {
        self.release_unstarted_request(request_id)
    }

    pub fn release_unstarted_request(
        &mut self,
        request_id: RequestId,
    ) -> Result<bool, RuntimeError> {
        self.require_state(SlotState::Idle, "release unstarted request")?;
        if self.active_request != Some(request_id) {
            self.record_stale();
            return Ok(false);
        }
        let has_exchange = self
            .generation
            .as_ref()
            .is_some_and(|generation| generation.exchange.is_some());
        if has_exchange {
            return Err(
                self.invariant_error("idle Slot cannot release a request with an active exchange")
            );
        }
        self.active_request = None;
        Ok(true)
    }

    pub fn begin_retire(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<Option<GenerationIdentity>, RuntimeError> {
        if self.state == SlotState::Disconnected {
            return Ok(None);
        }
        if self.state == SlotState::Retiring {
            return self
                .generation_identity()
                .map(Some)
                .ok_or_else(|| self.invariant_error("retiring Slot has no generation"));
        }
        let Some(generation) = self.generation.as_ref() else {
            return Err(self.invariant_error("connected Slot has no generation to retire"));
        };
        self.state = SlotState::Retiring;
        self.last_error = Some(reason.into());
        Ok(Some(GenerationIdentity {
            engine_epoch: self.engine_epoch,
            slot_id: self.slot_id,
            generation: generation.id,
        }))
    }

    pub fn begin_reconnect_retire(
        &mut self,
        request_id: RequestId,
        identity: GenerationIdentity,
        reason: impl Into<String>,
    ) -> Result<bool, RuntimeError> {
        if self.active_request != Some(request_id) || !self.matches_generation(identity) {
            self.record_stale();
            return Ok(false);
        }
        let retiring = self.begin_retire(reason)?;
        if retiring != Some(identity) {
            return Err(
                self.invariant_error("retry retirement did not retain the exact failed generation")
            );
        }
        Ok(true)
    }

    pub fn finish_retire(
        &mut self,
        identity: GenerationIdentity,
    ) -> Result<Option<RetiredGeneration>, RuntimeError> {
        if !self.accept_generation_event(identity, SlotState::Retiring) {
            return Ok(None);
        }
        let missing_generation = self.invariant_error("retiring Slot has no generation");
        let generation_id = self
            .generation
            .as_ref()
            .map(|generation| generation.id)
            .ok_or(missing_generation)?;
        let next_value = generation_id
            .get()
            .checked_add(1)
            .ok_or_else(|| self.invariant_error("TCP generation identity space exhausted"))?;
        let next_generation = GenerationId::new(next_value)?;
        let missing_generation = self.invariant_error("retiring Slot generation disappeared");
        let mut generation = self.generation.take().ok_or(missing_generation)?;
        generation.decoder.clear_generation();
        generation.exchange = None;
        let request_id = self.active_request.take();
        self.next_generation = next_generation;
        self.connected_host = None;
        self.reconnect_count = self.reconnect_count.saturating_add(1);
        self.state = SlotState::Disconnected;
        Ok(Some(RetiredGeneration {
            request_id,
            next_generation,
        }))
    }

    pub fn finish_reconnect_retire(
        &mut self,
        request_id: RequestId,
        identity: GenerationIdentity,
    ) -> Result<Option<ReconnectAck>, RuntimeError> {
        if self.state != SlotState::Retiring
            || self.active_request != Some(request_id)
            || !self.matches_generation(identity)
        {
            self.record_stale();
            return Ok(None);
        }
        let next_endpoint_index = self.endpoint_rotation.next_index();
        let endpoints_remaining_in_attempt = self.endpoint_rotation.remaining_in_attempt();
        let retired = self
            .finish_retire(identity)?
            .ok_or_else(|| self.invariant_error("exact retry retirement ack was rejected"))?;
        if retired.request_id != Some(request_id) {
            return Err(self.invariant_error("retry retirement released a different request owner"));
        }
        Ok(Some(ReconnectAck {
            engine_epoch: identity.engine_epoch,
            slot_id: identity.slot_id,
            request_id,
            retired_generation: identity.generation,
            next_generation: retired.next_generation,
            next_endpoint_index,
            endpoints_remaining_in_attempt,
        }))
    }

    pub fn generation_identity(&self) -> Option<GenerationIdentity> {
        self.generation
            .as_ref()
            .map(|generation| GenerationIdentity {
                engine_epoch: self.engine_epoch,
                slot_id: self.slot_id,
                generation: generation.id,
            })
    }

    pub fn connect_deadline(&self) -> Option<Deadline> {
        self.generation
            .as_ref()
            .map(|generation| generation.connect_deadline)
    }

    pub fn endpoint_index(&self) -> Option<usize> {
        self.generation
            .as_ref()
            .map(|generation| generation.endpoint_index)
    }

    pub fn last_activity_at(&self) -> Option<Instant> {
        self.generation
            .as_ref()
            .map(|generation| generation.last_activity_at)
    }

    pub(crate) fn receive_sequence(&self) -> u64 {
        self.generation
            .as_ref()
            .map_or(0, |generation| generation.receive_sequence)
    }

    pub fn wire_read_capacity(&self, identity: GenerationIdentity) -> usize {
        if !self.matches_generation(identity) || self.state == SlotState::Retiring {
            return 0;
        }
        self.generation.as_ref().map_or(0, |generation| {
            MAX_RAW_STAGING_BUFFER_SIZE
                .saturating_sub(generation.decoder.buffered_bytes())
                .min(SLOT_WIRE_BUDGET_BYTES)
        })
    }

    pub fn push_wire_bytes(&mut self, identity: GenerationIdentity, data: &[u8]) -> usize {
        let capacity = self.wire_read_capacity(identity);
        if capacity == 0 {
            if !data.is_empty() && !self.matches_generation(identity) {
                self.record_stale();
            }
            return 0;
        }
        let offered = data.len().min(capacity);
        self.generation
            .as_mut()
            .map_or(0, |generation| generation.decoder.push(&data[..offered]))
    }

    pub fn decode_turn(
        &mut self,
        identity: GenerationIdentity,
    ) -> Result<Option<SlotDecodeTurn>, RuntimeError> {
        if !self.matches_generation(identity) || self.state == SlotState::Retiring {
            self.record_stale();
            return Ok(None);
        }
        let (remaining_frames, remaining_bytes) = self
            .generation
            .as_ref()
            .map(|generation| {
                (
                    MAX_DECODED_QUEUE_FRAMES.saturating_sub(generation.decoded_frames.len()),
                    MAX_DECODED_QUEUE_BYTES.saturating_sub(generation.decoded_bytes),
                )
            })
            .ok_or_else(|| self.invariant_error("decode turn lost its generation"))?;
        let frame_budget = remaining_frames.min(SLOT_FRAME_BUDGET);
        let byte_budget = remaining_bytes.min(SLOT_DECODED_BUDGET_BYTES);
        let decoded = {
            let generation = self
                .generation
                .as_mut()
                .ok_or_else(|| RuntimeError::internal("decode generation disappeared"))?;
            generation
                .decoder
                .decode_available(frame_budget, byte_budget)
        };
        let batch = match decoded {
            Ok(batch) => batch,
            Err(error) => return Err(self.retire_decode_failure(identity, error)),
        };
        let frame_budget_hit = batch.frames.len() >= frame_budget;
        let queue_overflow = batch.budget_exhausted
            && ((frame_budget_hit && frame_budget == remaining_frames)
                || (!frame_budget_hit && byte_budget == remaining_bytes));
        if queue_overflow {
            let error = if frame_budget_hit && frame_budget == remaining_frames {
                ProtocolError::LimitExceeded {
                    resource: "decoded frame queue",
                    actual: MAX_DECODED_QUEUE_FRAMES.saturating_add(1),
                    limit: MAX_DECODED_QUEUE_FRAMES,
                }
            } else {
                ProtocolError::LimitExceeded {
                    resource: "decoded byte queue",
                    actual: MAX_DECODED_QUEUE_BYTES.saturating_add(1),
                    limit: MAX_DECODED_QUEUE_BYTES,
                }
            };
            return Err(self.retire_decode_failure(identity, error));
        }
        let frames_added = batch.frames.len();
        let decoded_bytes_added = batch.decoded_bytes;
        let (base_sequence, base_bytes) = self
            .generation
            .as_ref()
            .map(|generation| (generation.receive_sequence, generation.decoded_bytes))
            .ok_or_else(|| RuntimeError::internal("decoded generation disappeared"))?;
        let frame_increment = u64::try_from(frames_added)
            .map_err(|_| RuntimeError::internal("decoded frame count exceeds u64"))?;
        let final_sequence = base_sequence
            .checked_add(frame_increment)
            .ok_or_else(|| RuntimeError::internal("receive sequence exhausted"))?;
        let final_bytes = base_bytes
            .checked_add(decoded_bytes_added)
            .ok_or_else(|| RuntimeError::internal("decoded queue byte count overflow"))?;
        let mut queued_batch = Vec::with_capacity(frames_added);
        for (offset, response) in batch.frames.into_iter().enumerate() {
            let sequence_offset = u64::try_from(offset)
                .map_err(|_| RuntimeError::internal("decoded frame offset exceeds u64"))?;
            let receive_sequence = base_sequence
                .checked_add(sequence_offset)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| RuntimeError::internal("receive sequence exhausted"))?;
            queued_batch.push(QueuedDecodedFrame {
                response,
                receive_sequence,
            });
        }
        let generation = self
            .generation
            .as_mut()
            .ok_or_else(|| RuntimeError::internal("decoded generation disappeared"))?;
        if generation.receive_sequence != base_sequence || generation.decoded_bytes != base_bytes {
            return Err(RuntimeError::internal(
                "decoded queue changed before transactional enqueue",
            ));
        }
        generation.decoded_frames.extend(queued_batch);
        generation.receive_sequence = final_sequence;
        generation.decoded_bytes = final_bytes;
        Ok(Some(SlotDecodeTurn {
            frames_added,
            decoded_bytes_added,
            queue_frames: generation.decoded_frames.len(),
            queue_bytes: generation.decoded_bytes,
            budget_exhausted: batch.budget_exhausted,
        }))
    }

    pub fn route_decoded_turn(
        &mut self,
        identity: GenerationIdentity,
        send_complete: bool,
        now: Instant,
    ) -> Result<Option<Vec<RoutedResponse>>, RuntimeError> {
        if !self.matches_generation(identity) || self.state == SlotState::Retiring {
            self.record_stale();
            return Ok(None);
        }
        let route_count = self
            .generation
            .as_ref()
            .map(|generation| generation.decoded_frames.len().min(SLOT_FRAME_BUDGET))
            .ok_or_else(|| self.invariant_error("route turn lost its generation"))?;
        let zero_message = self.generation.as_ref().is_some_and(|generation| {
            generation
                .decoded_frames
                .iter()
                .take(route_count)
                .any(|queued| queued.response.msg_id == 0)
        });
        if zero_message {
            return Err(self.retire_decode_failure(
                identity,
                ProtocolError::invalid_data("response frame", "message id must be nonzero"),
            ));
        }
        let messages = self
            .generation
            .as_ref()
            .ok_or_else(|| self.invariant_error("route generation disappeared"))?
            .decoded_frames
            .iter()
            .take(route_count)
            .map(|queued| MessageIdentity::new(queued.response.msg_id, queued.response.msg_type))
            .collect::<Result<Vec<_>, _>>();
        let messages = match messages {
            Ok(messages) => messages,
            Err(error) => {
                if self.matches_generation(identity) && self.state != SlotState::Retiring {
                    self.state = SlotState::Retiring;
                    self.last_error = Some(error.to_string());
                }
                return Err(error);
            }
        };
        let (queued_bytes, remaining_bytes) = self
            .generation
            .as_ref()
            .ok_or_else(|| RuntimeError::internal("route generation disappeared"))?
            .decoded_frames
            .iter()
            .take(route_count)
            .try_fold(0_usize, |total, frame| {
                total.checked_add(frame.response.data.len())
            })
            .and_then(|routed_bytes| {
                self.generation
                    .as_ref()
                    .and_then(|generation| generation.decoded_bytes.checked_sub(routed_bytes))
                    .map(|remaining| (routed_bytes, remaining))
            })
            .ok_or_else(|| RuntimeError::internal("decoded route byte accounting failed"))?;
        let queued;
        {
            let generation = self
                .generation
                .as_mut()
                .ok_or_else(|| RuntimeError::internal("route generation disappeared"))?;
            if generation.decoded_frames.len() < route_count
                || generation.decoded_bytes.checked_sub(queued_bytes) != Some(remaining_bytes)
            {
                return Err(RuntimeError::internal(
                    "decoded queue changed before transactional routing",
                ));
            }
            queued = generation
                .decoded_frames
                .drain(..route_count)
                .collect::<Vec<_>>();
            generation.decoded_bytes = remaining_bytes;
        }
        let mut routed = Vec::with_capacity(route_count);
        for (frame, message) in queued.into_iter().zip(messages) {
            let identity = FrameIdentity {
                generation: identity,
                message,
                receive_sequence: frame.receive_sequence,
                send_complete,
            };
            let disposition = self.on_frame(identity, now);
            routed.push(RoutedResponse {
                frame: identity,
                disposition,
                response: frame.response,
            });
        }
        Ok(Some(routed))
    }

    pub fn decoded_queue_usage(&self) -> (usize, usize) {
        self.generation.as_ref().map_or((0, 0), |generation| {
            (generation.decoded_frames.len(), generation.decoded_bytes)
        })
    }

    pub fn check_decode_invariants(&self) -> Result<(), RuntimeError> {
        let Some(generation) = &self.generation else {
            return Ok(());
        };
        if generation.decoder.buffered_bytes() > MAX_RAW_STAGING_BUFFER_SIZE
            || generation.decoded_frames.len() > MAX_DECODED_QUEUE_FRAMES
            || generation.decoded_bytes > MAX_DECODED_QUEUE_BYTES
        {
            return Err(self.invariant_error("Slot decode capacity invariant failed"));
        }
        let actual_decoded_bytes = generation
            .decoded_frames
            .iter()
            .try_fold(0_usize, |total, frame| {
                total.checked_add(frame.response.data.len())
            })
            .ok_or_else(|| self.invariant_error("decoded queue byte accounting overflow"))?;
        if actual_decoded_bytes != generation.decoded_bytes {
            return Err(
                self.invariant_error("decoded queue byte count does not match resident frames")
            );
        }
        Ok(())
    }

    pub fn heartbeat_candidate(
        &self,
        interval: Option<Duration>,
        now: Instant,
    ) -> Option<HeartbeatCandidate> {
        let interval = interval.filter(|value| !value.is_zero())?;
        if self.state != SlotState::Idle || self.active_request.is_some() {
            return None;
        }
        let generation = self.generation.as_ref()?;
        if generation.exchange.is_some()
            || now < generation.last_activity_at.checked_add(interval)?
        {
            return None;
        }
        Some(HeartbeatCandidate {
            generation: GenerationIdentity {
                engine_epoch: self.engine_epoch,
                slot_id: self.slot_id,
                generation: generation.id,
            },
            observed_last_activity: generation.last_activity_at,
        })
    }

    fn retire_decode_failure(
        &mut self,
        identity: GenerationIdentity,
        error: ProtocolError,
    ) -> RuntimeError {
        let runtime_error = RuntimeError::from(error);
        if self.matches_generation(identity) && self.state != SlotState::Retiring {
            self.state = SlotState::Retiring;
            self.last_error = Some(runtime_error.to_string());
        }
        runtime_error
    }

    pub fn defer_heartbeat(&mut self, candidate: HeartbeatCandidate, now: Instant) -> bool {
        if self.state != SlotState::Idle
            || self.active_request.is_some()
            || !self.matches_generation(candidate.generation)
        {
            self.record_stale();
            return false;
        }
        let Some(generation) = self.generation.as_mut() else {
            self.record_stale();
            return false;
        };
        if generation.exchange.is_some()
            || generation.last_activity_at != candidate.observed_last_activity
        {
            self.record_stale();
            return false;
        }
        generation.last_activity_at = generation.last_activity_at.max(now);
        true
    }

    pub fn assign_heartbeat(
        &mut self,
        candidate: HeartbeatCandidate,
        request_id: RequestId,
    ) -> Result<bool, RuntimeError> {
        if self.state != SlotState::Idle
            || self.active_request.is_some()
            || !self.matches_generation(candidate.generation)
        {
            self.record_stale();
            return Ok(false);
        }
        let generation = self
            .generation
            .as_ref()
            .ok_or_else(|| self.invariant_error("heartbeat candidate lost its generation"))?;
        if generation.exchange.is_some()
            || generation.last_activity_at != candidate.observed_last_activity
        {
            self.record_stale();
            return Ok(false);
        }
        self.active_request = Some(request_id);
        Ok(true)
    }

    fn install_exchange(
        &mut self,
        message: MessageIdentity,
        receive_boundary: u64,
        kind: ExchangeKind,
    ) -> Result<(), RuntimeError> {
        let request_id = self.active_request.ok_or_else(|| {
            self.invariant_error("cannot begin exchange without request ownership")
        })?;
        if self
            .generation
            .as_ref()
            .is_some_and(|generation| generation.exchange.is_some())
        {
            return Err(self.invariant_error("Slot already has an active exchange"));
        }
        let missing_generation = self.invariant_error("cannot begin exchange without a generation");
        let generation = self.generation.as_mut().ok_or(missing_generation)?;
        generation.exchange = Some(ExchangeIdentity {
            request_id,
            message,
            receive_boundary,
            kind,
        });
        Ok(())
    }

    fn accept_generation_event(
        &mut self,
        identity: GenerationIdentity,
        expected_state: SlotState,
    ) -> bool {
        let accepted = self.state == expected_state && self.matches_generation(identity);
        if !accepted {
            self.record_stale();
        }
        accepted
    }

    fn matches_generation(&self, identity: GenerationIdentity) -> bool {
        identity.engine_epoch == self.engine_epoch
            && identity.slot_id == self.slot_id
            && self
                .generation
                .as_ref()
                .is_some_and(|generation| identity.generation == generation.id)
    }

    fn record_stale(&mut self) {
        self.stale_event_count = self.stale_event_count.saturating_add(1);
    }

    fn require_state(
        &self,
        expected: SlotState,
        operation: &'static str,
    ) -> Result<(), RuntimeError> {
        if self.state == expected {
            return Ok(());
        }
        Err(self
            .invariant_error(format!(
                "cannot {operation} while Slot is {}",
                self.state.as_str()
            ))
            .with_context("expected_state", expected.as_str()))
    }

    fn invariant_error(&self, message: impl Into<String>) -> RuntimeError {
        RuntimeError::internal(message)
            .with_context("engine_epoch", self.engine_epoch.get().to_string())
            .with_context("slot_id", self.slot_id.get().to_string())
            .with_context("slot_state", self.state.as_str())
    }
}

fn nonzero_identity(name: &'static str, value: u64) -> Result<u64, RuntimeError> {
    if value == 0 {
        return Err(RuntimeError::internal(format!("{name} must be nonzero")));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use eltdx_protocol::frame::RESPONSE_PREFIX;
    use proptest::prelude::*;

    use super::{
        EngineEpoch, ExchangeKind, FrameDisposition, FrameIdentity, GenerationId,
        GenerationIdentity, MessageIdentity, RequestId, Slot, SlotId, SlotState,
    };
    use crate::deadline::Deadline;
    use crate::endpoint::{Endpoint, EndpointRotation};
    use crate::error::RuntimeError;

    fn slot() -> Result<Slot, RuntimeError> {
        let endpoints = vec![
            Endpoint::numeric("127.0.0.1:7709")?,
            Endpoint::numeric("127.0.0.2:7709")?,
        ];
        Slot::new(
            EngineEpoch::new(1)?,
            SlotId::new(0),
            EndpointRotation::new(endpoints, 0)?,
        )
    }

    fn start_connect(
        slot: &mut Slot,
        request_id: RequestId,
        now: Instant,
    ) -> Result<GenerationIdentity, RuntimeError> {
        slot.begin_endpoint_attempt()?;
        let started = slot
            .start_connect(request_id, Deadline::at(now + Duration::from_secs(2)), now)?
            .ok_or_else(|| RuntimeError::internal("test endpoint is missing"))?;
        Ok(started.identity)
    }

    fn response_bytes(
        message: u32,
        message_type: u16,
        payload: &[u8],
    ) -> Result<Vec<u8>, RuntimeError> {
        let length = u16::try_from(payload.len())
            .map_err(|_| RuntimeError::internal("test response payload is too large"))?;
        let mut raw = vec![0_u8; 16 + payload.len()];
        raw[..4].copy_from_slice(&RESPONSE_PREFIX);
        raw[5..9].copy_from_slice(&message.to_le_bytes());
        raw[10..12].copy_from_slice(&message_type.to_le_bytes());
        raw[12..14].copy_from_slice(&length.to_le_bytes());
        raw[14..16].copy_from_slice(&length.to_le_bytes());
        raw[16..].copy_from_slice(payload);
        Ok(raw)
    }

    #[test]
    fn full_handshake_and_business_lifecycle_is_explicit() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(7)?;
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, request_id, now)?;

        assert_eq!(slot.state(), SlotState::Connecting);
        assert!(slot.on_connected(generation, now)?);
        assert_eq!(slot.state(), SlotState::ConnectedUnhandshaken);

        let handshake = MessageIdentity::new(11, 0x000d)?;
        slot.begin_handshake(handshake, 3, false)?;
        let handshake_result = slot.on_frame(
            FrameIdentity {
                generation,
                message: handshake,
                receive_sequence: 4,
                send_complete: true,
            },
            now,
        );
        assert_eq!(
            handshake_result,
            FrameDisposition::Matched(super::MatchedExchange {
                request_id,
                message: handshake,
                kind: ExchangeKind::Handshake { terminal: false },
            })
        );
        assert_eq!(slot.state(), SlotState::Idle);
        assert_eq!(slot.active_request(), Some(request_id));

        let business = MessageIdentity::new(12, 0x044e)?;
        slot.begin_business(business, 4)?;
        let business_result = slot.on_frame(
            FrameIdentity {
                generation,
                message: business,
                receive_sequence: 5,
                send_complete: true,
            },
            now,
        );
        assert!(matches!(
            business_result,
            FrameDisposition::Matched(super::MatchedExchange {
                kind: ExchangeKind::Business,
                ..
            })
        ));
        assert_eq!(slot.state(), SlotState::Idle);
        assert_eq!(slot.active_request(), Some(request_id));
        assert!(slot.finish_business(request_id)?);
        assert_eq!(slot.active_request(), None);
        Ok(())
    }

    #[test]
    fn stale_generation_cannot_advance_the_slot() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut slot = slot()?;
        let current = start_connect(&mut slot, RequestId::new(1)?, now)?;
        let stale = GenerationIdentity {
            generation: super::GenerationId::new(current.generation.get() + 1)?,
            ..current
        };

        assert!(!slot.on_connected(stale, now)?);
        assert_eq!(slot.state(), SlotState::Connecting);
        assert_eq!(slot.stale_event_count(), 1);
        Ok(())
    }

    #[test]
    fn unmatched_same_generation_frame_is_push_not_terminal() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(2)?;
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, request_id, now)?;
        assert!(slot.on_connected(generation, now)?);
        let handshake = MessageIdentity::new(20, 0x000d)?;
        slot.begin_handshake(handshake, 0, false)?;
        assert!(matches!(
            slot.on_frame(
                FrameIdentity {
                    generation,
                    message: handshake,
                    receive_sequence: 1,
                    send_complete: true,
                },
                now,
            ),
            FrameDisposition::Matched(_)
        ));
        let business = MessageIdentity::new(21, 0x044e)?;
        slot.begin_business(business, 1)?;

        let disposition = slot.on_frame(
            FrameIdentity {
                generation,
                message: MessageIdentity::new(99, 0x0547)?,
                receive_sequence: 2,
                send_complete: true,
            },
            now,
        );

        assert_eq!(disposition, FrameDisposition::Push);
        assert_eq!(slot.state(), SlotState::Busy);
        assert_eq!(slot.active_request(), Some(request_id));
        assert_eq!(slot.stale_event_count(), 0);
        Ok(())
    }

    #[test]
    fn retirement_is_the_only_path_back_to_disconnected() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(3)?;
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, request_id, now)?;
        let retiring = slot
            .begin_retire("connect cancelled")?
            .ok_or_else(|| RuntimeError::internal("active generation was not retired"))?;
        let repeated = slot
            .begin_retire("later cleanup observation")?
            .ok_or_else(|| RuntimeError::internal("retiring generation disappeared"))?;

        assert_eq!(retiring, generation);
        assert_eq!(repeated, generation);
        assert_eq!(slot.last_error(), Some("connect cancelled"));
        assert_eq!(slot.state(), SlotState::Retiring);
        let retired = slot
            .finish_retire(retiring)?
            .ok_or_else(|| RuntimeError::internal("retirement ack was rejected"))?;

        assert_eq!(retired.request_id, Some(request_id));
        assert_eq!(
            retired.next_generation.get(),
            generation.generation.get() + 1
        );
        assert_eq!(slot.state(), SlotState::Disconnected);
        assert_eq!(slot.reconnect_count(), 1);
        assert_eq!(slot.connected_host(), None);
        Ok(())
    }

    #[test]
    fn stale_retirement_ack_does_not_release_request_ownership() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(4)?;
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, request_id, now)?;
        let retiring = slot
            .begin_retire("timeout")?
            .ok_or_else(|| RuntimeError::internal("active generation was not retired"))?;
        let stale = GenerationIdentity {
            engine_epoch: EngineEpoch::new(2)?,
            ..generation
        };

        assert_eq!(slot.finish_retire(stale)?, None);
        assert_eq!(slot.state(), SlotState::Retiring);
        assert_eq!(slot.active_request(), Some(request_id));
        assert_eq!(slot.stale_event_count(), 1);
        assert!(slot.finish_retire(retiring)?.is_some());
        Ok(())
    }

    #[test]
    fn reconnect_ack_advances_generation_and_preserves_endpoint_cursor() -> Result<(), RuntimeError>
    {
        let now = Instant::now();
        let request_id = RequestId::new(40)?;
        let mut slot = slot()?;
        let first = start_connect(&mut slot, request_id, now)?;

        assert_eq!(slot.endpoint_index(), Some(0));
        assert!(slot.begin_reconnect_retire(request_id, first, "first endpoint failed")?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, first)?
            .ok_or_else(|| RuntimeError::internal("retry retirement ack was rejected"))?;

        assert_eq!(acknowledgement.retired_generation, first.generation);
        assert_eq!(
            acknowledgement.next_generation.get(),
            first.generation.get() + 1
        );
        assert_eq!(acknowledgement.next_endpoint_index, 1);
        assert_eq!(acknowledgement.endpoints_remaining_in_attempt, 1);
        assert_eq!(slot.state(), SlotState::Disconnected);

        let next = slot
            .start_connect(request_id, Deadline::at(now + Duration::from_secs(2)), now)?
            .ok_or_else(|| RuntimeError::internal("retry endpoint is missing"))?;
        assert_eq!(next.identity.generation, acknowledgement.next_generation);
        assert_eq!(
            next.attempt.endpoint_index,
            acknowledgement.next_endpoint_index
        );
        Ok(())
    }

    #[test]
    fn heartbeat_candidate_requires_idle_due_generation_and_defers_exactly(
    ) -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(50)?;
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, request_id, now)?;
        assert!(slot.on_connected(generation, now)?);
        let handshake = MessageIdentity::new(200, 0x000d)?;
        slot.begin_handshake(handshake, 0, false)?;
        assert!(matches!(
            slot.on_frame(
                FrameIdentity {
                    generation,
                    message: handshake,
                    receive_sequence: 1,
                    send_complete: true,
                },
                now,
            ),
            FrameDisposition::Matched(_)
        ));
        assert!(slot.release_unstarted_request(request_id)?);
        assert_eq!(slot.state(), SlotState::Idle);
        assert_eq!(
            slot.heartbeat_candidate(Some(Duration::from_secs(30)), now),
            None
        );

        let due = now + Duration::from_secs(30);
        let candidate = slot
            .heartbeat_candidate(Some(Duration::from_secs(30)), due)
            .ok_or_else(|| RuntimeError::internal("due heartbeat candidate is missing"))?;
        assert_eq!(candidate.generation, generation);
        assert!(slot.defer_heartbeat(candidate, due));
        assert_eq!(
            slot.heartbeat_candidate(Some(Duration::from_secs(30)), due),
            None
        );
        let next = slot
            .heartbeat_candidate(Some(Duration::from_secs(30)), due + Duration::from_secs(30))
            .ok_or_else(|| RuntimeError::internal("deferred heartbeat did not become due"))?;
        assert!(slot.assign_heartbeat(next, RequestId::new(51)?)?);
        assert_eq!(slot.active_request(), Some(RequestId::new(51)?));
        assert!(!slot.defer_heartbeat(candidate, due + Duration::from_secs(31)));
        assert_eq!(slot.stale_event_count(), 1);
        assert_eq!(slot.heartbeat_candidate(Some(Duration::ZERO), due), None);
        let heartbeat = MessageIdentity::new(201, 0x0004)?;
        slot.begin_heartbeat(heartbeat, 1)?;
        assert!(matches!(
            slot.on_frame(
                FrameIdentity {
                    generation,
                    message: heartbeat,
                    receive_sequence: 2,
                    send_complete: true,
                },
                due + Duration::from_secs(31),
            ),
            FrameDisposition::Matched(exchange)
                if exchange.kind == ExchangeKind::Heartbeat
        ));
        assert_eq!(slot.active_request(), Some(RequestId::new(51)?));
        assert!(slot.finish_heartbeat(RequestId::new(51)?)?);
        assert_eq!(slot.active_request(), None);
        Ok(())
    }

    #[test]
    fn decode_and_route_turns_stop_at_sixty_four_frames() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, RequestId::new(60)?, now)?;
        let mut raw = Vec::new();
        for message in 1_u32..=65 {
            raw.extend_from_slice(&response_bytes(message, 0x0547, &[])?);
        }
        assert_eq!(slot.push_wire_bytes(generation, &raw), raw.len());
        let first = slot
            .decode_turn(generation)?
            .ok_or_else(|| RuntimeError::internal("first decode turn was rejected"))?;
        assert_eq!(first.frames_added, 64);
        assert!(first.budget_exhausted);
        let routed = slot
            .route_decoded_turn(generation, true, now)?
            .ok_or_else(|| RuntimeError::internal("route turn was rejected"))?;
        assert_eq!(routed.len(), 64);
        assert!(routed
            .iter()
            .all(|item| item.disposition == FrameDisposition::Push));
        let second = slot
            .decode_turn(generation)?
            .ok_or_else(|| RuntimeError::internal("second decode turn was rejected"))?;
        assert_eq!(second.frames_added, 1);
        assert!(!second.budget_exhausted);
        assert_eq!(slot.decoded_queue_usage(), (1, 0));
        slot.check_decode_invariants()?;
        Ok(())
    }

    #[test]
    fn decoded_queue_overflow_retires_only_the_current_generation() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let mut slot = slot()?;
        let generation = start_connect(&mut slot, RequestId::new(61)?, now)?;
        let mut raw = Vec::new();
        for message in 1_u32..=1_024 {
            raw.extend_from_slice(&response_bytes(message, 0x0547, &[])?);
        }
        assert_eq!(slot.push_wire_bytes(generation, &raw), raw.len());
        for _ in 0..16 {
            let turn = slot
                .decode_turn(generation)?
                .ok_or_else(|| RuntimeError::internal("decode turn was rejected"))?;
            assert_eq!(turn.frames_added, 64);
        }
        assert_eq!(slot.decoded_queue_usage(), (1_024, 0));
        let overflow = response_bytes(1_025, 0x0547, &[])?;
        assert_eq!(slot.push_wire_bytes(generation, &overflow), overflow.len());
        let error = slot
            .decode_turn(generation)
            .err()
            .ok_or_else(|| RuntimeError::internal("decoded queue overflow was accepted"))?;
        assert_eq!(error.kind(), "Protocol");
        assert_eq!(slot.state(), SlotState::Retiring);
        assert_eq!(slot.active_request(), Some(RequestId::new(61)?));
        slot.check_decode_invariants()?;
        Ok(())
    }

    #[test]
    fn retry_cannot_start_before_exact_retirement_ack() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(41)?;
        let mut slot = slot()?;
        let first = start_connect(&mut slot, request_id, now)?;
        assert!(slot.begin_reconnect_retire(request_id, first, "retryable failure")?);

        assert!(slot.begin_endpoint_attempt().is_err());
        let stale = GenerationIdentity {
            generation: GenerationId::new(first.generation.get() + 1)?,
            ..first
        };
        assert_eq!(slot.finish_reconnect_retire(request_id, stale)?, None);
        assert_eq!(slot.state(), SlotState::Retiring);
        assert!(slot.finish_reconnect_retire(request_id, first)?.is_some());
        assert_eq!(slot.state(), SlotState::Disconnected);
        Ok(())
    }

    #[test]
    fn old_generation_frame_is_stale_after_retry_connect_starts() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let request_id = RequestId::new(42)?;
        let mut slot = slot()?;
        let first = start_connect(&mut slot, request_id, now)?;
        assert!(slot.begin_reconnect_retire(request_id, first, "retryable failure")?);
        let acknowledgement = slot
            .finish_reconnect_retire(request_id, first)?
            .ok_or_else(|| RuntimeError::internal("retirement ack was rejected"))?;
        slot.begin_endpoint_attempt()?;
        let second = slot
            .start_connect(request_id, Deadline::at(now + Duration::from_secs(2)), now)?
            .ok_or_else(|| RuntimeError::internal("retry endpoint is missing"))?;
        assert_eq!(second.identity.generation, acknowledgement.next_generation);

        let stale = slot.on_frame(
            FrameIdentity {
                generation: first,
                message: MessageIdentity::new(120, 0x044e)?,
                receive_sequence: 1,
                send_complete: true,
            },
            now,
        );
        assert_eq!(stale, FrameDisposition::Stale);
        assert_eq!(slot.state(), SlotState::Connecting);
        assert_eq!(slot.active_request(), Some(request_id));
        Ok(())
    }

    #[test]
    fn invalid_internal_transition_is_structured() -> Result<(), RuntimeError> {
        let mut slot = slot()?;
        let error = slot
            .assign_ready(RequestId::new(5)?)
            .err()
            .ok_or_else(|| RuntimeError::internal("invalid transition unexpectedly succeeded"))?;

        assert_eq!(error.kind(), "Internal");
        assert!(error
            .context()
            .iter()
            .any(|(key, value)| key == "slot_state" && value == "disconnected"));
        Ok(())
    }

    proptest! {
        #[test]
        fn arbitrary_stale_generation_identity_cannot_advance_connecting_slot(
            mismatched_field in 0_u8..3,
            delta in 1_u64..10_000,
        ) {
            let now = Instant::now();
            let mut slot = match slot() {
                Ok(slot) => slot,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let request_id = match RequestId::new(1) {
                Ok(request_id) => request_id,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let current = match start_connect(&mut slot, request_id, now) {
                Ok(identity) => identity,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            let mut stale = current;
            match mismatched_field {
                0 => {
                    stale.engine_epoch = match EngineEpoch::new(
                        current.engine_epoch.get().saturating_add(delta),
                    ) {
                        Ok(epoch) => epoch,
                        Err(error) => return Err(TestCaseError::fail(error.to_string())),
                    };
                }
                1 => {
                    let slot_delta = usize::try_from(delta).unwrap_or(usize::MAX);
                    stale.slot_id = SlotId::new(
                        current.slot_id.get().saturating_add(slot_delta),
                    );
                }
                _ => {
                    stale.generation = match super::GenerationId::new(
                        current.generation.get().saturating_add(delta),
                    ) {
                        Ok(generation) => generation,
                        Err(error) => return Err(TestCaseError::fail(error.to_string())),
                    };
                }
            }

            let accepted = match slot.on_connected(stale, now) {
                Ok(accepted) => accepted,
                Err(error) => return Err(TestCaseError::fail(error.to_string())),
            };
            prop_assert!(!accepted);
            prop_assert_eq!(slot.state(), SlotState::Connecting);
            prop_assert_eq!(slot.active_request(), Some(request_id));
            prop_assert_eq!(slot.stale_event_count(), 1);
        }
    }
}
