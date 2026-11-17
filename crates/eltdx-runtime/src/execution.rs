use std::sync::Arc;
use std::time::{Duration, Instant};

use eltdx_protocol::frame::ResponseFrame;
use eltdx_protocol::CommandRequest;

use crate::deadline::Deadline;
use crate::error::RuntimeError;
use crate::request::{RequestDeadlinePolicy, RequestWireIdentity, RetryPolicy};
use crate::slot::{EngineEpoch, GenerationId, RequestId, SlotId};

/// Submission policy for one raw execution using a caller-owned absolute deadline.
#[derive(Clone, Debug)]
pub struct SubmissionOptions {
    timing: RequestDeadlinePolicy,
    pub(crate) retry: Option<RetryPolicy>,
    pub(crate) authorization: Option<Arc<dyn AttemptAuthorization>>,
}

impl SubmissionOptions {
    /// Creates options with a deadline measured on the same monotonic clock as `Instant::now`.
    pub const fn new(absolute_deadline: Instant) -> Self {
        Self {
            timing: RequestDeadlinePolicy::Absolute(Deadline::from_instant(absolute_deadline)),
            retry: None,
            authorization: None,
        }
    }

    /// Returns the unchanged absolute deadline supplied at construction.
    pub fn absolute_deadline(&self) -> Option<Instant> {
        self.timing
            .absolute_deadline()
            .map(|deadline| deadline.instant())
    }

    /// Internal compatibility boundary: start this request's clock after runtime startup.
    #[doc(hidden)]
    pub fn legacy_after_startup(timeout: Duration) -> Self {
        Self {
            timing: RequestDeadlinePolicy::LegacyAfterStartup(timeout),
            retry: None,
            authorization: None,
        }
    }

    #[doc(hidden)]
    pub fn with_attempt_authorization(
        mut self,
        retry: RetryPolicy,
        authorization: Arc<dyn AttemptAuthorization>,
    ) -> Self {
        self.retry = Some(retry);
        self.authorization = Some(authorization);
        self
    }

    pub(crate) const fn timing(&self) -> RequestDeadlinePolicy {
        self.timing
    }
}

/// Language-neutral admission hook. Implementations must not retain an engine owner strongly.
/// No callback is invoked while a runtime lifecycle/state mutex is held.
#[doc(hidden)]
pub trait AttemptAuthorization: std::fmt::Debug + Send + Sync {
    fn check_boundary(&self, now: Instant) -> Result<(), RuntimeError>;

    fn reserve(
        &self,
        attempt: u32,
        last_failure: Option<&RuntimeError>,
    ) -> Result<Box<dyn ReservedAttempt>, RuntimeError>;

    /// Continue an existing legacy attempt without another logical recovery debit.
    fn reserve_endpoint_continuation(
        &self,
        attempt: u32,
        last_failure: Option<&RuntimeError>,
    ) -> Result<Box<dyn ReservedAttempt>, RuntimeError> {
        self.reserve(attempt, last_failure)
    }

    /// Old runtime endpoint cycling is retained only by the legacy policy.
    fn count_endpoint_failover(&self) -> bool;
}

/// Unique reservation, moved to a worker. Dropping before start refunds it exactly once.
#[doc(hidden)]
pub trait ReservedAttempt: std::fmt::Debug + Send {
    fn start(self: Box<Self>, now: Instant) -> Result<(), RuntimeError>;

    /// Authorizers with admission locks must recheck this bound after locking.
    fn start_before(self: Box<Self>, deadline: Instant) -> Result<(), RuntimeError> {
        let now = Instant::now();
        Deadline::at(deadline).require_remaining_at(now, crate::error::TimeoutPhase::Retry)?;
        self.start(now)
    }
}

/// An owned protocol frame and the session identity that received it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawReply {
    /// Receive order within this exact TCP generation, shared with push frames.
    pub receive_sequence: u64,
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
        receive_sequence: u64,
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
            receive_sequence,
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

#[cfg(test)]
mod operation_policy_tests {
    use super::*;
    use crate::request::{
        AdmissionQueue, RequestState, RequestTracker, RequestWireIdentity, RetryDecision,
        RetryStopReason,
    };
    use crate::slot::{EngineEpoch, GenerationId, RequestId};
    use std::num::NonZeroU32;

    fn started_request(maximum: u32) -> (RequestTracker, RequestWireIdentity, Instant) {
        let now = Instant::now();
        let epoch = EngineEpoch::new(1).expect("epoch");
        let id = RequestId::new(1).expect("request");
        let mut queue = AdmissionQueue::new(1, 8).expect("queue");
        queue.open(epoch).expect("open");
        let admission = queue
            .submit(
                epoch,
                id,
                Deadline::after_at(now, Duration::from_secs(8)).expect("deadline"),
                now,
            )
            .expect("admission");
        let mut tracker = RequestTracker::default();
        tracker
            .admit_with_retry_policy(
                admission,
                RetryPolicy::rust_public(true, NonZeroU32::new(maximum).expect("positive")),
            )
            .expect("tracked");
        let attempt = tracker.begin_attempt(id, now).expect("attempt");
        let wire = RequestWireIdentity {
            engine_epoch: epoch,
            request_id: id,
            lease_id: attempt.lease_id,
            slot_id: attempt.slot_id,
            generation: Some(GenerationId::new(1).expect("generation")),
            message: None,
        };
        tracker
            .transition(id, RequestState::Connecting, Some(wire))
            .expect("connecting");
        (tracker, wire, now)
    }

    #[test]
    fn per_request_maximum_keeps_last_network_failure_instead_of_global_limit() {
        let (mut tracker, wire, now) = started_request(1);
        let network = RuntimeError::connection_closed("real connect failure")
            .with_context("host", "fixture:7709");
        match tracker
            .begin_retry(wire, network.clone(), true, now)
            .expect("decision")
        {
            RetryDecision::RetireThenTerminal(terminal) => {
                assert_eq!(terminal.reason, RetryStopReason::AttemptsExhausted);
                assert_eq!(terminal.error, network);
            }
            RetryDecision::Retire(_) => panic!("per-request maximum must prevent a retry"),
        }
    }

    #[test]
    fn rust_protocol_failure_is_not_retried_even_when_worker_marks_transport_retryable() {
        let (mut tracker, wire, now) = started_request(2);
        let protocol = RuntimeError::Protocol {
            code: "bad_frame".to_owned(),
            message: "bad frame".to_owned(),
            context: Vec::new(),
        };
        match tracker
            .begin_retry(wire, protocol.clone(), true, now)
            .expect("decision")
        {
            RetryDecision::RetireThenTerminal(terminal) => {
                assert_eq!(terminal.reason, RetryStopReason::NotRetryable);
                assert_eq!(terminal.error, protocol);
            }
            RetryDecision::Retire(_) => panic!("malformed protocol must remain protocol"),
        }
    }

    #[test]
    fn legacy_timing_remains_after_startup_and_rust_deadline_is_unchanged() {
        let entry = Instant::now();
        let public = SubmissionOptions::new(entry + Duration::from_secs(8));
        let legacy = SubmissionOptions::legacy_after_startup(Duration::from_secs(8));
        assert_eq!(
            public.absolute_deadline(),
            Some(entry + Duration::from_secs(8))
        );
        assert_eq!(legacy.absolute_deadline(), None);
        let after_startup = Instant::now();
        let legacy_deadline = legacy
            .timing()
            .resolve_after_startup()
            .expect("old boundary")
            .instant();
        assert!(legacy_deadline >= after_startup + Duration::from_secs(8));
        assert_eq!(
            public
                .timing()
                .resolve_after_startup()
                .expect("absolute")
                .instant(),
            entry + Duration::from_secs(8)
        );
    }
}
