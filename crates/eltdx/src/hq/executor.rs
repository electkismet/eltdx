use std::sync::Arc;
use std::time::Instant;

use eltdx_protocol::CommandRequest;
use eltdx_runtime::engine::{Engine, PendingPoll, PendingRawExecution, PinHandle, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::execution::RawReply;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, RecoveryLimit};
use crate::session::{SessionOperation, SessionOwner};

/// Owner-bound entry point for HQ operations. It holds no runtime worker itself.
#[derive(Clone, Debug)]
pub(crate) struct HqExecutor {
    owner: Arc<SessionOwner>,
}

impl HqExecutor {
    pub(crate) fn new(owner: Arc<SessionOwner>) -> Self {
        Self { owner }
    }

    pub(crate) fn close(&self) -> Result<()> {
        self.owner.close()
    }

    pub(crate) fn begin_operation(&self) -> Result<SessionOperation> {
        self.owner.admit()
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        self.owner.ensure_open()
    }

    pub(crate) fn register_engine(&self, engine: Engine) -> Result<()> {
        self.owner.register_engine(engine)
    }

    pub(crate) fn owner(&self) -> &Arc<SessionOwner> { &self.owner }

    pub(crate) fn begin_context(
        &self, operation: &'static str, config: &ClientConfig, options: &OperationOptions,
    ) -> Result<OperationContext> {
        let entry = Instant::now();
        self.begin_context_at(entry, operation, config, options)
    }

    /// Public business methods capture `entry` as their first action, before validation.
    pub(crate) fn begin_context_at(
        &self, entry: Instant, operation: &'static str, config: &ClientConfig, options: &OperationOptions,
    ) -> Result<OperationContext> {
        OperationContext::rust_public(entry, operation, Arc::clone(&self.owner), config, options)
    }

    pub(crate) fn execute_raw(
        &self, engine: &Engine, context: &OperationContext, request: CommandRequest,
        purpose: ChildPurpose, recovery: Option<RecoveryLimit>,
    ) -> Result<RawReply> {
        self.execute_raw_inner(engine, None, context, request, purpose, recovery)
    }

    pub(crate) fn execute_pinned_raw(
        &self, engine: &Engine, pin: &PinHandle, context: &OperationContext,
        request: CommandRequest, purpose: ChildPurpose, recovery: Option<RecoveryLimit>,
    ) -> Result<RawReply> {
        if !pin.belongs_to_engine(engine) {
            return Err(Error::new(ErrorKind::InvalidArgument, "pin belongs to a different engine"));
        }
        self.execute_raw_inner(engine, Some(pin), context, request, purpose, recovery)
    }

    fn execute_raw_inner(
        &self, engine: &Engine, pin: Option<&PinHandle>, context: &OperationContext,
        request: CommandRequest, purpose: ChildPurpose, recovery: Option<RecoveryLimit>,
    ) -> Result<RawReply> {
        if !Arc::ptr_eq(context.owner(), &self.owner) {
            return Err(Error::new(ErrorKind::InvalidArgument, "operation belongs to a different session"));
        }
        context.check_boundary(TimeoutPhase::Admission)?;
        self.register_engine(engine.clone())?;
        let command = request.command_code();
        let permit = context.begin_child(purpose, recovery)?;
        let child_id = permit.child_id;
        let options = permit.into_submission(&request);
        let pending = match pin {
            Some(pin) => pin.begin_execute_raw_with_options(request, options),
            None => engine.begin_execute_raw_with_options(request, options),
        };
        let mut pending = pending.map_err(|runtime| context.runtime_error(runtime, command)
            .with_context("child_id", child_id.to_string()))?;
        loop {
            if let Err(root) = context.check_boundary(TimeoutPhase::Response) {
                return Err(self.retire_cancelled(context, &pending, root, command));
            }
            match pending.wait_timeout(SIGNAL_POLL_INTERVAL) {
                Ok(PendingPoll::Pending) => {}
                Ok(PendingPoll::Ready(reply)) => {
                    // Delivery arbitration happens after receiving, so token/close cannot
                    // win and then be overwritten by a buffered successful result.
                    if let Err(root) = context.check_boundary(TimeoutPhase::Response) {
                        return Err(self.retire_cancelled(context, &pending, root, command));
                    }
                    context.record_work(0, reply.response.data.len())?;
                    return Ok(reply);
                }
                Err(runtime) => {
                    let runtime_error = context.runtime_error(runtime, command)
                        .with_context("child_id", child_id.to_string());
                    if let Err(root) = context.check_boundary(TimeoutPhase::Response) {
                        let root = context.enrich_stop(root, runtime_error);
                        return Err(self.retire_cancelled(context, &pending, root, command));
                    }
                    // A legacy request-level timeout remains recoverable by its business
                    // policy. It does not create an operation-wide deadline/root cause.
                    return Err(runtime_error);
                }
            }
        }
    }

    fn retire_cancelled(
        &self, context: &OperationContext, pending: &PendingRawExecution,
        root: Error, command: u16,
    ) -> Error {
        let root = context.stop(root);
        let result = (|| {
            context.claim_cleanup()?;
            let confirmation = pending.begin_cancel().map_err(Error::from_runtime)?;
            loop {
                match confirmation.wait_timeout(std::time::Duration::ZERO).map_err(Error::from_runtime)? {
                    PendingPoll::Ready(()) => return Ok(()),
                    PendingPoll::Pending => {}
                }
                let remaining = context.cleanup_remaining()?;
                if remaining.is_zero() {
                    let cleanup = Error::new(ErrorKind::Timeout, "operation retirement was not confirmed")
                        .with_context("phase", "cancel_confirm");
                    let _ = confirmation.quarantine();
                    return Err(cleanup);
                }
                match confirmation.wait_timeout(remaining.min(SIGNAL_POLL_INTERVAL)) {
                    Ok(PendingPoll::Ready(())) => return Ok(()),
                    Ok(PendingPoll::Pending) => {}
                    Err(error) => {
                        let _ = confirmation.quarantine();
                        return Err(Error::from_runtime(error));
                    }
                }
            }
        })();
        match result {
            Ok(()) => root,
            Err(cleanup) => {
                let _ = pending.quarantine();
                context.annotate(Error::new(ErrorKind::CancelUnconfirmed, "operation stopped but retirement was not confirmed")
                    .with_context("root_kind", format!("{:?}", root.kind()))
                    .with_source(root).with_cleanup_failure(cleanup),
                TimeoutPhase::CancelConfirm, Some(command),
                )
            }
        }
    }
}
