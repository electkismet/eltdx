use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eltdx_protocol::CommandRequest;
use eltdx_runtime::execution::SubmissionOptions;
use eltdx_runtime::RuntimeError;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::metadata::MetadataSnapshot;
use crate::policy::retries::{
    checked_add, limit_error, rust_retry_policy, ChildAuthorization, OperationBudget, WorkCounters,
};
use crate::session::{SessionOperation, SessionOwner};
use crate::types::raw::RawPolicy;

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

/// Per-call options. Durations start at method entry, never at builder construction.
/// Timeout/cancellation may require up to the separate cleanup budget to confirm retirement.
#[derive(Clone, Debug, Default)]
pub struct OperationOptions {
    timeout: Option<Duration>,
    deadline: Option<Instant>,
    cancellation: Option<CancellationToken>,
    raw: RawPolicy,
    raw_explicit: bool,
}

impl OperationOptions {
    /// Explicit zero expires immediately. Arithmetic overflow is an invalid argument.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn with_deadline(mut self, at: Instant) -> Self {
        self.deadline = Some(at);
        self
    }
    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }
    pub fn with_raw(mut self, raw: RawPolicy) -> Self {
        self.raw = raw;
        self.raw_explicit = true;
        self
    }

    #[cfg(feature = "ex")]
    pub(crate) fn inheriting_raw(&self, default: RawPolicy) -> Self {
        let mut options = self.clone();
        if !options.raw_explicit {
            options.raw = default;
        }
        options
    }

    fn resolve_deadline(&self, entry: Instant, inherited: Duration) -> Result<Instant> {
        let duration = self
            .timeout
            .or_else(|| self.deadline.is_none().then_some(inherited));
        let relative = duration
            .map(|duration| {
                entry.checked_add(duration).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidArgument,
                        "operation timeout overflows the monotonic clock",
                    )
                })
            })
            .transpose()?;
        match (relative, self.deadline) {
            (Some(a), Some(b)) => Ok(a.min(b)),
            (Some(at), None) | (None, Some(at)) => Ok(at),
            (None, None) => Err(Error::new(ErrorKind::Internal, "operation has no deadline")),
        }
    }
}

/// Irreversible, owner-free cancellation state. Clones may be shared by independent calls.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<Mutex<bool>>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        let mut state = self
            .cancelled
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *state = true;
    }

    pub fn is_cancelled(&self) -> bool {
        *self
            .cancelled
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
    pub(crate) fn with_active<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self
            .cancelled
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "cancellation state is poisoned"))?;
        if *state {
            return Err(Error::new(ErrorKind::Cancelled, "operation cancelled"));
        }
        action()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChildPurpose {
    Required,
    Metadata,
    Page,
    MissingRecovery,
    EmptyProbe,
    Fallback,
    Renewal,
}

impl ChildPurpose {
    pub(crate) const fn is_extra(self) -> bool {
        matches!(
            self,
            Self::MissingRecovery | Self::EmptyProbe | Self::Fallback
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecoveryLimit {
    pub name: &'static str,
    pub maximum: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WorkLimits {
    /// Legacy `None` explicitly means unlimited pages. Rust queries supply domain limits.
    pub pages: Option<NonZeroUsize>,
    pub records: Option<usize>,
    pub bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ExecutionProfile {
    RustPublic {
        deadline: Instant,
        wire_attempts: NonZeroU32,
    },
    PythonLegacy {
        request_timeout: Duration,
    },
}

/// A shared metadata flight can retain sponsorship without retaining the client owner.
/// The flight selects a live sponsor for each physical attempt; it owns its own clock.
#[derive(Clone, Debug)]
pub(crate) struct OperationSponsor {
    budget: Arc<OperationBudget>,
    profile: ExecutionProfile,
    limits: WorkLimits,
    request_limit: Option<NonZeroUsize>,
}

impl OperationSponsor {
    pub(crate) fn check_boundary(&self) -> Result<()> {
        self.budget.check(Instant::now())
    }

    /// Charge a worker's streamed bytes against its parent's existing budget.
    /// No second counter or separately renewed deadline is introduced.
    #[cfg(any(feature = "f10", feature = "ex"))]
    pub(crate) fn record_bytes(&self, bytes: usize) -> Result<()> {
        self.budget.gate(Instant::now(), |state| {
            let next = checked_add(state.counters.bytes, bytes)?;
            if let Some(maximum) = self.limits.bytes {
                if next > maximum {
                    return Err(limit_error(
                        "bytes",
                        maximum,
                        state.counters.bytes,
                        state.last_recoverable.as_ref(),
                    ));
                }
            }
            state.counters.bytes = next;
            Ok(())
        })
    }

    #[cfg(feature = "ex")]
    pub(crate) fn remaining_bytes(&self) -> Result<usize> {
        self.budget.gate(Instant::now(), |state| {
            Ok(self.limits.bytes.map_or(usize::MAX, |maximum| {
                maximum.saturating_sub(state.counters.bytes)
            }))
        })
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        match self.profile {
            ExecutionProfile::RustPublic { deadline, .. } => Some(deadline),
            _ => None,
        }
    }

    pub(crate) fn metadata_authorization(
        &self,
    ) -> Result<Arc<dyn eltdx_runtime::execution::AttemptAuthorization>> {
        self.source_authorization(ChildPurpose::Metadata)
    }

    /// An explicitly injected source uses the same reservation gate as the
    /// owning service. It does not create another operation or retry budget.
    #[cfg(feature = "mac")]
    pub(crate) fn fallback_authorization(
        &self,
    ) -> Result<Arc<dyn eltdx_runtime::execution::AttemptAuthorization>> {
        self.source_authorization(ChildPurpose::Fallback)
    }

    fn source_authorization(
        &self,
        purpose: ChildPurpose,
    ) -> Result<Arc<dyn eltdx_runtime::execution::AttemptAuthorization>> {
        self.budget.gate(Instant::now(), |state| {
            let next = checked_add(state.counters.logical_requests, 1)?;
            if let Some(limit) = self.request_limit {
                if next > limit.get() {
                    return Err(limit_error(
                        "logical_requests",
                        limit.get(),
                        state.counters.logical_requests,
                        state.last_recoverable.as_ref(),
                    ));
                }
            }
            state.counters.logical_requests = next;
            Ok(())
        })?;
        Ok(Arc::new(ChildAuthorization {
            budget: Arc::clone(&self.budget),
            purpose,
            recovery: None,
            probe_scope: None,
            rust_public: matches!(self.profile, ExecutionProfile::RustPublic { .. }),
        }))
    }

    pub(crate) fn retry_policy(
        &self,
        request: &CommandRequest,
    ) -> eltdx_runtime::request::RetryPolicy {
        match self.profile {
            ExecutionProfile::RustPublic { wire_attempts, .. } => {
                rust_retry_policy(request, wire_attempts)
            }
            ExecutionProfile::PythonLegacy { .. } => {
                eltdx_runtime::request::RetryPolicy::ordinary(request.retry_safe())
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct RequestPermit {
    pub child_id: usize,
    profile: ExecutionProfile,
    authorization: Arc<dyn eltdx_runtime::execution::AttemptAuthorization>,
}

impl RequestPermit {
    /// Move the same single authorization to a non-HQ transport. The caller
    /// reserves and starts it at physical connect/send, with no synthetic HQ
    /// command and no independent retry/recovery counters.
    pub(crate) fn into_transport_authorization(
        self,
    ) -> Arc<dyn eltdx_runtime::execution::AttemptAuthorization> {
        self.authorization
    }

    pub(crate) fn into_submission(self, request: &CommandRequest) -> SubmissionOptions {
        self.into_submission_policy(request, false)
    }

    pub(crate) fn into_pinned_submission(self, request: &CommandRequest) -> SubmissionOptions {
        self.into_submission_policy(request, true)
    }

    fn into_submission_policy(self, request: &CommandRequest, pinned: bool) -> SubmissionOptions {
        let (options, retry) = match self.profile {
            ExecutionProfile::RustPublic {
                deadline,
                wire_attempts,
            } => (
                SubmissionOptions::new(deadline),
                rust_retry_policy(
                    request,
                    if pinned {
                        NonZeroU32::MIN
                    } else {
                        wire_attempts
                    },
                ),
            ),
            ExecutionProfile::PythonLegacy { request_timeout } => (
                SubmissionOptions::legacy_after_startup(request_timeout),
                eltdx_runtime::request::RetryPolicy::ordinary(request.retry_safe()),
            ),
        };
        options.with_attempt_authorization(retry, self.authorization)
    }
}

#[derive(Debug, Default)]
struct DecodeMetadata {
    snapshot: Option<Arc<MetadataSnapshot>>,
    started: bool,
}

/// One public entry's identity, immutable options, policy and captured metadata.
#[derive(Debug)]
pub(crate) struct OperationContext {
    pub id: u64,
    pub session_id: usize,
    name: &'static str,
    owner: Arc<SessionOwner>,
    _admission: SessionOperation,
    options: OperationOptions,
    profile: ExecutionProfile,
    pub(crate) budget: Arc<OperationBudget>,
    limits: WorkLimits,
    request_limit: Option<NonZeroUsize>,
    cleanup_timeout: Duration,
    metadata: Mutex<DecodeMetadata>,
    market_date: Mutex<Option<crate::types::TradingDate>>,
}

impl OperationContext {
    pub(crate) fn rust_public(
        entry: Instant,
        name: &'static str,
        owner: Arc<SessionOwner>,
        config: &ClientConfig,
        options: &OperationOptions,
    ) -> Result<Self> {
        let options = options.clone();
        // Entry is supplied by the public method before its first validation.
        if options
            .cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(
                Error::new(ErrorKind::Cancelled, "operation cancelled before entry")
                    .with_context("operation", name)
                    .with_context("phase", "admission"),
            );
        }
        let deadline = options.resolve_deadline(entry, config.request_timeout())?;
        let profile = ExecutionProfile::RustPublic {
            deadline,
            wire_attempts: config.wire_attempt_limit(),
        };
        Self::create(
            entry,
            name,
            owner,
            options,
            profile,
            Some(config.extra_attempt_budget()),
            config.cleanup_timeout(),
        )
    }

    pub(crate) fn python_legacy(
        entry: Instant,
        name: &'static str,
        owner: Arc<SessionOwner>,
        request_timeout: Duration,
        cleanup_timeout: Duration,
        token: Option<CancellationToken>,
        raw: RawPolicy,
    ) -> Result<Self> {
        if request_timeout.is_zero() || cleanup_timeout.is_zero() {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "legacy request and cleanup timeout must be positive",
            ));
        }
        let options = OperationOptions {
            cancellation: token,
            raw,
            ..OperationOptions::default()
        };
        Self::create(
            entry,
            name,
            owner,
            options,
            ExecutionProfile::PythonLegacy { request_timeout },
            None,
            cleanup_timeout,
        )
    }

    fn create(
        _entry: Instant,
        name: &'static str,
        owner: Arc<SessionOwner>,
        options: OperationOptions,
        profile: ExecutionProfile,
        extras: Option<usize>,
        cleanup_timeout: Duration,
    ) -> Result<Self> {
        let id = NEXT_OPERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| Error::new(ErrorKind::Internal, "operation identity exhausted"))?;
        let deadline = match profile {
            ExecutionProfile::RustPublic { deadline, .. } => Some(deadline),
            _ => None,
        };
        let token = options.cancellation.clone().unwrap_or_default();
        let budget = OperationBudget::new(Arc::downgrade(&owner), token, deadline, extras);
        // Reject zero timeout/past deadline before obtaining admission or starting runtime.
        budget.check(Instant::now())?;
        let admission = owner.admit()?;
        let context = Self {
            id,
            session_id: owner.generation(),
            name,
            owner,
            _admission: admission,
            options,
            profile,
            budget,
            limits: WorkLimits::default(),
            request_limit: None,
            cleanup_timeout,
            metadata: Mutex::new(DecodeMetadata::default()),
            market_date: Mutex::new(None),
        };
        context.check_boundary(TimeoutPhase::Admission)?;
        Ok(context)
    }

    pub(crate) fn raw(&self) -> RawPolicy {
        self.options.raw
    }
    pub(crate) fn sponsor(&self) -> OperationSponsor {
        OperationSponsor {
            budget: Arc::clone(&self.budget),
            profile: self.profile,
            limits: self.limits,
            request_limit: self.request_limit,
        }
    }
    pub(crate) fn owner(&self) -> &Arc<SessionOwner> {
        &self.owner
    }
    /// Freeze the injected-clock Shanghai date once for this logical operation.
    pub(crate) fn current_market_date(&self) -> Result<crate::types::TradingDate> {
        self.check_boundary(TimeoutPhase::Admission)?;
        let mut date = self.market_date.lock().map_err(|_| {
            Error::new(
                ErrorKind::Internal,
                "operation market date state is poisoned",
            )
        })?;
        if let Some(date) = *date {
            return Ok(date);
        }
        let resolved = self.owner.current_shanghai_date()?;
        *date = Some(resolved);
        Ok(resolved)
    }
    pub(crate) fn is_legacy(&self) -> bool {
        matches!(self.profile, ExecutionProfile::PythonLegacy { .. })
    }
    pub(crate) fn counters(&self) -> Result<WorkCounters> {
        self.budget.counters()
    }

    pub(crate) fn metadata(&self) -> Result<Option<Arc<MetadataSnapshot>>> {
        Ok(self
            .metadata
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation metadata state is poisoned"))?
            .snapshot
            .clone())
    }

    pub(crate) fn install_metadata(&self, snapshot: Arc<MetadataSnapshot>) -> Result<()> {
        self.check_boundary(TimeoutPhase::Decode)?;
        let mut metadata = self
            .metadata
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation metadata state is poisoned"))?;
        if metadata.started {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "operation metadata cannot change after parsing begins",
            ));
        }
        metadata.snapshot = Some(snapshot);
        Ok(())
    }

    pub(crate) fn begin_decode(&self) -> Result<Arc<MetadataSnapshot>> {
        self.check_boundary(TimeoutPhase::Decode)?;
        let mut metadata = self
            .metadata
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation metadata state is poisoned"))?;
        let snapshot = metadata
            .snapshot
            .get_or_insert_with(|| Arc::new(MetadataSnapshot::empty("no-metadata")))
            .clone();
        metadata.started = true;
        Ok(snapshot)
    }

    pub(crate) fn set_work_limits(&mut self, limits: WorkLimits) -> Result<()> {
        if self.counters()?.logical_requests != 0 {
            return Err(Error::new(
                ErrorKind::Internal,
                "work limits must be frozen before the first request",
            ));
        }
        self.limits = limits;
        Ok(())
    }

    /// Opt-in composition limit over every logical child, including metadata,
    /// fallbacks and explicitly injected provider requests. Physical retries
    /// continue to use the existing attempt authorization and extra budget.
    #[cfg(feature = "mac")]
    pub(crate) fn set_request_limit(&mut self, maximum: NonZeroUsize) -> Result<()> {
        if self.counters()?.logical_requests != 0 {
            return Err(Error::new(
                ErrorKind::Internal,
                "logical request limit must be frozen before the first request",
            ));
        }
        self.request_limit = Some(maximum);
        Ok(())
    }

    pub(crate) fn check_boundary(&self, phase: TimeoutPhase) -> Result<()> {
        self.budget
            .check(Instant::now())
            .map_err(|error| self.stop(self.annotate(error, phase, None)))
    }

    pub(crate) fn begin_child(
        &self,
        purpose: ChildPurpose,
        recovery: Option<RecoveryLimit>,
    ) -> Result<RequestPermit> {
        self.begin_child_scoped(purpose, recovery, None)
    }

    pub(crate) fn begin_child_scoped(
        &self,
        purpose: ChildPurpose,
        recovery: Option<RecoveryLimit>,
        probe_scope: Option<usize>,
    ) -> Result<RequestPermit> {
        if probe_scope.is_some() && purpose != ChildPurpose::EmptyProbe {
            return Err(Error::new(
                ErrorKind::Internal,
                "per-code probe scope requires EmptyProbe purpose",
            ));
        }
        let child_id = self
            .budget
            .gate(Instant::now(), |state| {
                let logical = checked_add(state.counters.logical_requests, 1)?;
                if let Some(limit) = self.request_limit {
                    if logical > limit.get() {
                        return Err(limit_error(
                            "logical_requests",
                            limit.get(),
                            state.counters.logical_requests,
                            state.last_recoverable.as_ref(),
                        ));
                    }
                }
                let pages = checked_add(
                    state.counters.pages,
                    usize::from(purpose == ChildPurpose::Page),
                )?;
                if let Some(limit) = self.limits.pages {
                    if pages > limit.get() {
                        return Err(limit_error(
                            "pages",
                            limit.get(),
                            state.counters.pages,
                            state.last_recoverable.as_ref(),
                        ));
                    }
                }
                state.counters.logical_requests = logical;
                state.counters.pages = pages;
                Ok(logical)
            })
            .map_err(|error| self.annotate(error, TimeoutPhase::Admission, None))?;
        Ok(RequestPermit {
            child_id,
            profile: self.profile,
            authorization: Arc::new(ChildAuthorization {
                budget: Arc::clone(&self.budget),
                purpose,
                recovery,
                rust_public: !self.is_legacy(),
                probe_scope,
            }),
        })
    }

    pub(crate) fn record_work(&self, records: usize, bytes: usize) -> Result<()> {
        self.budget
            .gate(Instant::now(), |state| {
                let next_records = checked_add(state.counters.records, records)?;
                let next_bytes = checked_add(state.counters.bytes, bytes)?;
                for (name, next, started, maximum) in [
                    (
                        "records",
                        next_records,
                        state.counters.records,
                        self.limits.records,
                    ),
                    ("bytes", next_bytes, state.counters.bytes, self.limits.bytes),
                ] {
                    if let Some(maximum) = maximum {
                        if next > maximum {
                            return Err(limit_error(
                                name,
                                maximum,
                                started,
                                state.last_recoverable.as_ref(),
                            ));
                        }
                    }
                }
                state.counters.records = next_records;
                state.counters.bytes = next_bytes;
                Ok(())
            })
            .map_err(|error| self.annotate(error, TimeoutPhase::Aggregate, None))
    }

    pub(crate) fn remember_recoverable(&self, error: Error) {
        if let Ok(mut state) = self.budget.state.lock() {
            state.last_recoverable = Some(error);
        }
    }

    pub(crate) fn runtime_error(&self, runtime: RuntimeError, command: u16) -> Error {
        let recovery_limit = runtime
            .context()
            .iter()
            .any(|(key, value)| key == "code" && value == "legacy_recovery_limit");
        let mut error = Error::from_runtime(runtime);
        if error.kind() == ErrorKind::LimitExceeded || recovery_limit {
            if let Some(source) = self.budget.last_recoverable() {
                error = error.with_source(source);
            }
        }
        self.annotate(error, TimeoutPhase::Response, Some(command))
    }

    pub(crate) fn annotate(
        &self,
        mut error: Error,
        phase: TimeoutPhase,
        command: Option<u16>,
    ) -> Error {
        if !error.context().iter().any(|(key, _)| key == "phase") {
            error = error.with_context("phase", format!("{phase:?}").to_ascii_lowercase());
        }
        error = error
            .with_context("operation", self.name)
            .with_context("operation_id", self.id.to_string())
            .with_context("session_id", self.session_id.to_string());
        if let Some(command) = command {
            error = error.with_context("command", format!("0x{command:04x}"));
        }
        error
    }

    pub(crate) fn stop(&self, error: Error) -> Error {
        self.budget.stop(error)
    }

    pub(crate) fn enrich_stop(&self, root: Error, diagnostic: Error) -> Error {
        let mut root = self.budget.stop(root);
        for (key, value) in diagnostic.context() {
            if !root.context().iter().any(|(existing, _)| existing == key) {
                root = root.with_context(key, value);
            }
        }
        if std::error::Error::source(&root).is_none() {
            root = root.with_source(diagnostic);
        }
        if let Ok(mut state) = self.budget.state.lock() {
            state.root = Some(root.clone());
        }
        root
    }

    pub(crate) fn finish(&self) -> Result<()> {
        self.budget
            .gate(Instant::now(), |state| {
                state.finished = true;
                Ok(())
            })
            .map_err(|error| self.annotate(error, TimeoutPhase::Aggregate, None))
    }

    /// Claim retirement once for this operation; token cancellation does not gate cleanup.
    pub(crate) fn claim_cleanup(&self) -> Result<()> {
        let mut state = self
            .budget
            .state
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation budget poisoned"))?;
        if state.cleanup_claimed {
            return Err(Error::new(
                ErrorKind::Internal,
                "operation retirement confirmation already claimed",
            ));
        }
        let deadline = Instant::now()
            .checked_add(self.cleanup_timeout)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidArgument,
                    "cleanup timeout overflows monotonic clock",
                )
            })?;
        state.cleanup_claimed = true;
        state.cleanup_deadline = Some(deadline);
        Ok(())
    }

    /// Several resources of one business call share one retirement deadline.
    /// This does not grant a second deadline after a request already stopped.
    pub(crate) fn ensure_cleanup_budget(&self) -> Result<()> {
        let mut state = self
            .budget
            .state
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation budget poisoned"))?;
        if !state.cleanup_claimed {
            state.cleanup_deadline = Some(
                Instant::now()
                    .checked_add(self.cleanup_timeout)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidArgument,
                            "cleanup timeout overflows monotonic clock",
                        )
                    })?,
            );
            state.cleanup_claimed = true;
        }
        Ok(())
    }

    pub(crate) fn cleanup_remaining(&self) -> Result<Duration> {
        // Do not nest budget -> owner; admission uses the opposite order.
        let owner_deadline = self.owner.shutdown_deadline()?;
        let state = self
            .budget
            .state
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "operation budget poisoned"))?;
        let local = state
            .cleanup_deadline
            .ok_or_else(|| Error::new(ErrorKind::Internal, "cleanup not claimed"))?;
        let deadline = owner_deadline.map_or(local, |deadline| deadline.min(local));
        Ok(deadline.saturating_duration_since(Instant::now()))
    }
}

impl Drop for OperationContext {
    fn drop(&mut self) {
        if let Ok(mut state) = self.budget.state.lock() {
            if !state.finished && state.root.is_none() {
                state.root = Some(Error::new(
                    ErrorKind::Cancelled,
                    "operation context retired",
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
