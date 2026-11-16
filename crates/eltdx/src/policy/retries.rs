use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use eltdx_protocol::CommandRequest;
use eltdx_runtime::execution::{AttemptAuthorization, ReservedAttempt};
use eltdx_runtime::request::RetryPolicy;
use eltdx_runtime::RuntimeError;

use crate::error::{Error, ErrorKind, Result};
use crate::operation::{CancellationToken, ChildPurpose, RecoveryLimit};
use crate::session::SessionOwner;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WorkCounters {
    pub logical_requests: usize,
    pub pages: usize,
    pub records: usize,
    pub bytes: usize,
    pub wire_attempts: usize,
    pub extra_started: usize,
    pub extra_reserved: usize,
}

#[derive(Debug, Default)]
struct RecoveryCounter {
    configured: usize,
    reserved: usize,
    started: usize,
}

#[derive(Debug, Default)]
pub(crate) struct BudgetState {
    pub counters: WorkCounters,
    recoveries: BTreeMap<&'static str, RecoveryCounter>,
    probe_scopes: BTreeMap<usize, RecoveryCounter>,
    pub root: Option<Error>,
    pub last_recoverable: Option<Error>,
    pub cleanup_deadline: Option<Instant>,
    pub cleanup_claimed: bool,
    pub finished: bool,
}

/// Owner-free shared state crossing to runtime workers. Only operation guards own the session.
#[derive(Debug)]
pub(crate) struct OperationBudget {
    owner: Weak<SessionOwner>,
    token: CancellationToken,
    deadline: Option<Instant>,
    extra_limit: Option<usize>,
    pub state: Mutex<BudgetState>,
}

impl OperationBudget {
    pub(crate) fn new(
        owner: Weak<SessionOwner>, token: CancellationToken,
        deadline: Option<Instant>, extra_limit: Option<usize>,
    ) -> Arc<Self> {
        Arc::new(Self { owner, token, deadline, extra_limit, state: Mutex::new(BudgetState::default()) })
    }

    /// Owner close, user cancellation, deadline and budget commit share a strict lock order.
    /// The action is bounded arithmetic only; no lock survives return to the scheduler.
    pub(crate) fn gate<T>(&self, now: Instant, action: impl FnOnce(&mut BudgetState) -> Result<T>) -> Result<T> {
        self.gate_before(now, None, action)
    }

    fn gate_before<T>(&self, observed: Instant, attempt_deadline: Option<Instant>,
        action: impl FnOnce(&mut BudgetState) -> Result<T>) -> Result<T> {
        self.gate_with_clock(observed, attempt_deadline, Instant::now, action)
    }

    pub(crate) fn gate_with_clock<T>(&self, observed: Instant, attempt_deadline: Option<Instant>,
        clock: impl FnOnce() -> Instant, action: impl FnOnce(&mut BudgetState) -> Result<T>) -> Result<T> {
        let owner = self.owner.upgrade().ok_or_else(closed)?;
        owner.with_open_admission(|| self.token.with_active(|| {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            if let Some(root) = &state.root { return Err(root.clone()); }
            if state.finished { return Err(Error::new(ErrorKind::Internal, "operation already completed")); }
            // Include elapsed lock acquisition time; injected future observations
            // remain useful for deterministic deadline arithmetic cases.
            let now = observed.max(clock());
            if self.deadline.is_some_and(|deadline| now >= deadline) {
                return Err(Error::new(ErrorKind::Timeout, "operation deadline elapsed"));
            }
            if attempt_deadline.is_some_and(|deadline| now >= deadline) {
                return Err(Error::new(ErrorKind::Timeout, "wire attempt deadline elapsed")
                    .with_context("phase", "retry"));
            }
            action(&mut state)
        }))
    }

    pub(crate) fn check(&self, now: Instant) -> Result<()> {
        match self.gate(now, |_| Ok(())) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.stop(error)),
        }
    }

    pub(crate) fn stop(&self, error: Error) -> Error {
        match self.state.lock() {
            Ok(mut state) => {
                let mut first = state.root.take().unwrap_or_else(|| error.clone());
                for (key, value) in error.context() {
                    if !first.context().iter().any(|(existing, _)| existing == key) {
                        first = first.with_context(key, value);
                    }
                }
                state.root = Some(first.clone());
                first
            }
            Err(_) => error,
        }
    }

    pub(crate) fn counters(&self) -> Result<WorkCounters> {
        Ok(self.state.lock().map_err(|_| poisoned())?.counters)
    }

    pub(crate) fn last_recoverable(&self) -> Option<Error> {
        self.state.lock().ok().and_then(|state| state.last_recoverable.clone())
    }

    pub(crate) fn reserve(
        self: &Arc<Self>, extra: bool, recovery: Option<RecoveryLimit>,
        last_failure: Option<&RuntimeError>,
    ) -> Result<Reservation> {
        self.reserve_scoped(extra, recovery, None, last_failure)
    }

    pub(crate) fn recovery_started(&self, name: &'static str) -> Result<usize> {
        Ok(self.state.lock().map_err(|_| poisoned())?.recoveries.get(name).map_or(0, |counter| counter.started))
    }

    pub(crate) fn probe_started(&self, scope: usize) -> Result<usize> {
        Ok(self.state.lock().map_err(|_| poisoned())?.probe_scopes.get(&scope).map_or(0, |counter| counter.started))
    }

    pub(crate) fn reserve_scoped(
        self: &Arc<Self>, extra: bool, recovery: Option<RecoveryLimit>, probe_scope: Option<usize>,
        last_failure: Option<&RuntimeError>,
    ) -> Result<Reservation> {
        self.gate(Instant::now(), |state| {
            if let Some(failure) = last_failure {
                state.last_recoverable = Some(Error::from_runtime(failure.clone()));
            }
            if extra {
                if let Some(limit) = self.extra_limit {
                    let committed = checked_add(state.counters.extra_started, state.counters.extra_reserved)?;
                    if committed >= limit {
                        return Err(limit_error("extra_attempts", limit, state.counters.extra_started,
                            state.last_recoverable.as_ref()));
                    }
                }
            }
            // Validate every local/global debit before changing either counter.
            let global_reserved = if extra { checked_add(state.counters.extra_reserved, 1)? }
                else { state.counters.extra_reserved };
            let local_reserved = match recovery {
                Some(limit) => {
                    let counter = state.recoveries.get(limit.name);
                    if counter.is_some_and(|counter| counter.configured != limit.maximum) {
                        return Err(Error::new(ErrorKind::Internal, "recovery limit changed within operation"));
                    }
                    let (reserved, started) = counter.map_or((0, 0), |counter| (counter.reserved, counter.started));
                    if checked_add(reserved, started)? >= limit.maximum {
                        let kind = if self.extra_limit.is_none() { ErrorKind::Protocol } else { ErrorKind::LimitExceeded };
                        let mut error = limit_error(limit.name, limit.maximum, started, state.last_recoverable.as_ref());
                        if kind == ErrorKind::Protocol {
                            error = Error::new(kind, "legacy recovery request limit reached")
                                .with_context("limit", limit.name)
                                .with_context("configured", limit.maximum.to_string())
                                .with_context("started", started.to_string());
                            if let Some(source) = &state.last_recoverable { error = error.with_source(source.clone()); }
                        }
                        return Err(error);
                    }
                    Some(checked_add(reserved, 1)?)
                }
                None => None,
            };
            let probe_reserved = match probe_scope {
                Some(scope) => {
                    let counter = state.probe_scopes.get(&scope);
                    let (reserved, started) = counter.map_or((0, 0), |counter| (counter.reserved, counter.started));
                    if checked_add(reserved, started)? >= 3 {
                        return Err(limit_error("money_flow_code_probes", 3, started, state.last_recoverable.as_ref()));
                    }
                    Some(checked_add(reserved, 1)?)
                }
                None => None,
            };
            state.counters.extra_reserved = global_reserved;
            if let (Some(limit), Some(reserved)) = (recovery, local_reserved) {
                let counter = state.recoveries.entry(limit.name).or_default();
                counter.configured = limit.maximum;
                counter.reserved = reserved;
            }
            if let (Some(scope), Some(reserved)) = (probe_scope, probe_reserved) {
                state.probe_scopes.entry(scope).or_default().reserved = reserved;
            }
            Ok(Reservation { budget: Arc::clone(self), extra, recovery, probe_scope, count_wire: true, committed: false })
        })
    }
}

#[derive(Debug)]
pub(crate) struct Reservation {
    budget: Arc<OperationBudget>,
    extra: bool,
    recovery: Option<RecoveryLimit>,
    probe_scope: Option<usize>,
    count_wire: bool,
    committed: bool,
}

impl Reservation {
    fn commit(&mut self, now: Instant, attempt_deadline: Option<Instant>) -> Result<()> {
        self.commit_with_clock(now, attempt_deadline, Instant::now)
    }

    pub(crate) fn commit_with_clock(&mut self, now: Instant, attempt_deadline: Option<Instant>,
        clock: impl FnOnce() -> Instant) -> Result<()> {
        self.budget.gate_with_clock(now, attempt_deadline, clock, |state| {
            let wire = checked_add(state.counters.wire_attempts, usize::from(self.count_wire))?;
            let extra = checked_add(state.counters.extra_started, usize::from(self.extra))?;
            let local_started = match self.recovery {
                Some(limit) => {
                    let counter = state.recoveries.get(limit.name).ok_or_else(poisoned)?;
                    Some(checked_add(counter.started, 1)?)
                }
                None => None,
            };
            let probe_started = self.probe_scope.map(|scope| state.probe_scopes.get(&scope)
                .ok_or_else(poisoned).and_then(|counter| checked_add(counter.started, 1))).transpose()?;
            state.counters.wire_attempts = wire;
            state.counters.extra_started = extra;
            if self.extra { state.counters.extra_reserved -= 1; }
            if let (Some(limit), Some(started)) = (self.recovery, local_started) {
                let counter = state.recoveries.get_mut(limit.name).ok_or_else(poisoned)?;
                counter.reserved -= 1;
                counter.started = started;
            }
            if let (Some(scope), Some(started)) = (self.probe_scope, probe_started) {
                let counter = state.probe_scopes.get_mut(&scope).ok_or_else(poisoned)?;
                counter.reserved -= 1;
                counter.started = started;
            }
            Ok(())
        })?;
        self.committed = true;
        Ok(())
    }
}

impl ReservedAttempt for Reservation {
    fn start(mut self: Box<Self>, now: Instant) -> std::result::Result<(), RuntimeError> {
        self.commit(now, None).map_err(to_runtime)
    }

    fn start_before(mut self: Box<Self>, deadline: Instant) -> std::result::Result<(), RuntimeError> {
        self.commit(Instant::now(), Some(deadline)).map_err(to_runtime)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.committed { return; }
        if let Ok(mut state) = self.budget.state.lock() {
            if self.extra { state.counters.extra_reserved -= 1; }
            if let Some(limit) = self.recovery {
                if let Some(counter) = state.recoveries.get_mut(limit.name) { counter.reserved -= 1; }
            }
            if let Some(scope) = self.probe_scope {
                if let Some(counter) = state.probe_scopes.get_mut(&scope) { counter.reserved -= 1; }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ChildAuthorization {
    pub budget: Arc<OperationBudget>,
    pub purpose: ChildPurpose,
    pub recovery: Option<RecoveryLimit>,
    pub probe_scope: Option<usize>,
    pub rust_public: bool,
}

impl AttemptAuthorization for ChildAuthorization {
    fn check_boundary(&self, now: Instant) -> std::result::Result<(), RuntimeError> {
        self.budget.check(now).map_err(to_runtime)
    }

    fn reserve(&self, attempt: u32, last_failure: Option<&RuntimeError>)
        -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
        // A recovery's first request consumes a global extra, but its wire retry is not
        // a second logical recovery. Each wire retry still consumes another global extra.
        self.budget.reserve_scoped(attempt > 1 || self.purpose.is_extra(),
            if attempt == 1 { self.recovery } else { None },
            if attempt == 1 { self.probe_scope } else { None }, last_failure)
            .map(|reservation| Box::new(reservation) as Box<dyn ReservedAttempt>)
            .map_err(to_runtime)
    }

    fn count_endpoint_failover(&self) -> bool { self.rust_public }

    fn reserve_endpoint_continuation(&self, _: u32, _: Option<&RuntimeError>)
        -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
        self.budget.gate(Instant::now(), |_| Ok(())).map_err(to_runtime)?;
        Ok(Box::new(Reservation {
            budget: Arc::clone(&self.budget), extra: false, recovery: None,
            probe_scope: None,
            count_wire: false, committed: false,
        }))
    }
}

pub(crate) fn rust_retry_policy(request: &CommandRequest, maximum: std::num::NonZeroU32) -> RetryPolicy {
    // Explicit read whitelist. Session/stream state changes cannot be replayed after a send.
    let safe = matches!(request,
        CommandRequest::CapitalChanges(_) | CommandRequest::FinanceBatch(_)
        | CommandRequest::CompanyInfoCategory(_) | CommandRequest::CompanyInfoContent(_)
        | CommandRequest::BlockMeta(_) | CommandRequest::SecurityList(_) | CommandRequest::SecurityCount(_)
        | CommandRequest::SpecialLimits(_) | CommandRequest::IntradayAux(_) | CommandRequest::DelistedKlines(_)
        | CommandRequest::Klines(_) | CommandRequest::TodayIntraday(_) | CommandRequest::LegacyQuotes(_)
        | CommandRequest::CategoryQuotes(_) | CommandRequest::Snapshots(_) | CommandRequest::AuctionSeries(_)
        | CommandRequest::FileContent(_) | CommandRequest::HistoricalIntraday(_) | CommandRequest::TodayTicks(_)
        | CommandRequest::HistoricalTicks(_) | CommandRequest::Sparkline(_) | CommandRequest::RecentIntraday(_)
        | CommandRequest::MoneyFlow(_));
    RetryPolicy::rust_public(safe, maximum)
}

pub(crate) fn to_runtime(error: Error) -> RuntimeError {
    let kind = error.kind();
    let mut runtime = if kind == ErrorKind::Timeout {
        RuntimeError::timeout(eltdx_runtime::error::TimeoutPhase::Admission)
    } else if kind == ErrorKind::Protocol {
        RuntimeError::Protocol { code: "legacy_recovery_limit".to_owned(), message: error.to_string(), context: Vec::new() }
    } else {
        RuntimeError::internal(error.to_string())
    };
    runtime = runtime.with_context("sdk_kind", format!("{kind:?}"));
    for (key, value) in error.context() { runtime = runtime.with_context(key, value); }
    runtime
}

pub(crate) fn checked_add(value: usize, increment: usize) -> Result<usize> {
    value.checked_add(increment).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "operation counter overflow"))
}

pub(crate) fn limit_error(name: &str, configured: usize, started: usize, source: Option<&Error>) -> Error {
    let error = Error::new(ErrorKind::LimitExceeded, "operation work limit reached")
        .with_context("limit", name).with_context("configured", configured.to_string())
        .with_context("started", started.to_string());
    match source { Some(source) => error.with_source(source.clone()), None => error }
}

fn poisoned() -> Error { Error::new(ErrorKind::Internal, "operation budget state is unavailable") }
fn closed() -> Error { Error::new(ErrorKind::Closed, "operation owner is closed") }
