use std::time::Instant;

use eltdx_protocol::frame::ResponseFrame;
use eltdx_protocol::CommandRequest;

use crate::error::RuntimeError;
use crate::request::RequestWireIdentity;
use crate::slot::{EngineEpoch, GenerationId, RequestId, SlotId};

/// Submission policy for one raw execution using a caller-owned absolute deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionOptions {
    absolute_deadline: Instant,
}

impl SubmissionOptions {
    /// Creates options with a deadline measured on the same monotonic clock as `Instant::now`.
    pub const fn new(absolute_deadline: Instant) -> Self {
        Self { absolute_deadline }
    }

    /// Returns the unchanged absolute deadline supplied at construction.
    pub const fn absolute_deadline(self) -> Instant {
        self.absolute_deadline
    }
}

/// An owned protocol frame and the session identity that received it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawReply {
    /// The request used to produce this reply.
    pub request: CommandRequest,
    /// The decoded response frame, including both payload and original wire bytes.
    pub response: ResponseFrame,
    /// Runtime request identity.
    pub request_id: RequestId,
    /// Runtime epoch that owned the request.
    pub engine_epoch: EngineEpoch,
    /// Runtime Slot that received the frame.
    pub slot_id: SlotId,
    /// TCP generation that received the frame.
    pub generation: GenerationId,
    /// Configured endpoint host for the receiving connection.
    pub host: String,
}

impl RawReply {
    pub(crate) fn from_runtime(
        request: CommandRequest,
        response: ResponseFrame,
        wire: RequestWireIdentity,
        host: String,
    ) -> Result<Self, RuntimeError> {
        let generation = wire
            .generation
            .ok_or_else(|| RuntimeError::internal("raw reply has no TCP generation"))?;
        let message = wire
            .message
            .ok_or_else(|| RuntimeError::internal("raw reply has no wire message identity"))?;
        if message.msg_id() != response.msg_id || message.msg_type() != response.msg_type {
            return Err(RuntimeError::internal(
                "raw reply frame does not match its wire message identity",
            ));
        }
        if host.is_empty() {
            return Err(RuntimeError::internal("raw reply has no endpoint host"));
        }
        Ok(Self {
            request,
            response,
            request_id: wire.request_id,
            engine_epoch: wire.engine_epoch,
            slot_id: wire.slot_id,
            generation,
            host,
        })
    }
}
