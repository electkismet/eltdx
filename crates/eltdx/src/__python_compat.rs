//! Unstable bridge used only by the separately versioned Python binding.
//! No Python runtime, object or exception type crosses this boundary.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eltdx_protocol::CommandRequest;
use eltdx_runtime::engine::{Engine, PinHandle};
use eltdx_runtime::execution::RawReply;

use crate::error::{Error, ErrorKind, Result};
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, RecoveryLimit, WorkLimits};
use crate::session::SessionOwner;
use crate::{CancellationToken, RawPolicy};

#[derive(Clone, Copy, Debug)]
pub enum LegacyChildPurpose { Required, Metadata, Page, MissingRecovery, EmptyProbe, Fallback, Renewal }

impl From<LegacyChildPurpose> for ChildPurpose {
    fn from(purpose: LegacyChildPurpose) -> Self {
        match purpose {
            LegacyChildPurpose::Required => Self::Required,
            LegacyChildPurpose::Metadata => Self::Metadata,
            LegacyChildPurpose::Page => Self::Page,
            LegacyChildPurpose::MissingRecovery => Self::MissingRecovery,
            LegacyChildPurpose::EmptyProbe => Self::EmptyProbe,
            LegacyChildPurpose::Fallback => Self::Fallback,
            LegacyChildPurpose::Renewal => Self::Renewal,
        }
    }
}

/// Domain recovery counts preserve their old scope and error projection.
#[derive(Clone, Copy, Debug)]
pub struct LegacyRecoveryLimit { pub name: &'static str, pub maximum: usize }

#[derive(Clone, Debug)]
pub struct LegacyExecutor {
    executor: HqExecutor,
    engine: Engine,
    request_timeout: Duration,
    cleanup_timeout: Duration,
}

impl LegacyExecutor {
    /// Creates a separate legacy generation. Ordinary clients cannot be converted.
    /// Engine construction/registration is lazy and starts no network resources here.
    /// Bindings create separate executors for legacy close domains and close each at
    /// its original stage, without sealing still-open later domains.
    pub fn new(
        engine: Engine, request_timeout: Duration, close_timeout: Duration, cleanup_timeout: Duration,
    ) -> Result<Self> {
        if request_timeout.is_zero() || close_timeout.is_zero() || cleanup_timeout.is_zero() {
            return Err(Error::new(ErrorKind::InvalidArgument, "legacy timeouts must be positive"));
        }
        let owner = Arc::new(SessionOwner::new(close_timeout, cleanup_timeout));
        let executor = HqExecutor::new(owner);
        executor.owner().register_engine_lazy(engine.clone())?;
        Ok(Self { executor, engine, request_timeout, cleanup_timeout })
    }

    pub fn begin_operation(
        &self, name: &'static str, raw: RawPolicy, token: Option<CancellationToken>,
    ) -> Result<LegacyOperation> {
        let entry = Instant::now();
        let context = OperationContext::python_legacy(entry, name, Arc::clone(self.executor.owner()),
            self.request_timeout, self.cleanup_timeout, token, raw)?;
        Ok(LegacyOperation { executor: self.clone(), context })
    }

    pub fn close(&self) -> Result<()> { self.executor.close() }
}

#[derive(Debug)]
pub struct LegacyOperation { executor: LegacyExecutor, context: OperationContext }

impl LegacyOperation {
    /// `None` retains the legacy unlimited-page option; there is no fabricated cap.
    pub fn with_page_limit(mut self, maximum: Option<NonZeroUsize>) -> Result<Self> {
        self.context.set_work_limits(WorkLimits { pages: maximum, ..WorkLimits::default() })?;
        Ok(self)
    }

    pub fn execute_raw(
        &self, request: CommandRequest, purpose: LegacyChildPurpose, recovery: Option<LegacyRecoveryLimit>,
    ) -> Result<RawReply> {
        self.executor.executor.execute_raw(&self.executor.engine, &self.context, request, purpose.into(),
            recovery.map(|limit| RecoveryLimit { name: limit.name, maximum: limit.maximum }))
    }

    pub fn execute_pinned_raw(
        &self, pin: &PinHandle, request: CommandRequest, purpose: LegacyChildPurpose,
        recovery: Option<LegacyRecoveryLimit>,
    ) -> Result<RawReply> {
        self.executor.executor.execute_pinned_raw(&self.executor.engine, pin, &self.context, request,
            purpose.into(), recovery.map(|limit| RecoveryLimit { name: limit.name, maximum: limit.maximum }))
    }

    pub fn remember_recoverable(&self, error: Error) { self.context.remember_recoverable(error); }
    pub fn finish(self) -> Result<()> { self.context.finish() }
}
