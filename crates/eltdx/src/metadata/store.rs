use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use eltdx_protocol::CommandRequest;
use eltdx_runtime::execution::{AttemptAuthorization, ReservedAttempt};
use eltdx_runtime::request::RetryPolicy;
use eltdx_runtime::RuntimeError;

use crate::error::{Error, ErrorKind, Result};
use crate::metadata::{MetadataRecord, MetadataSnapshot, SnapshotKey};
use crate::operation::OperationSponsor;
use crate::policy::retries::to_runtime;
use crate::types::{Clock, HqMarket, SecurityId};

const MAX_MARKET_RECORDS: usize = u16::MAX as usize;
const MAX_MARKET_PAGES: usize = MAX_MARKET_RECORDS + 1;
const MAX_ACTIVE_FLIGHTS: usize = 16;
const MAX_FLIGHT_WAITERS: usize = 65_535;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageEvidence {
    More,
    TerminalEmpty,
    TerminalTotal(u32),
}

/// A candidate is invisible until terminal evidence and every record validate.
#[derive(Debug)]
pub(crate) struct MarketCandidate {
    key: SnapshotKey,
    next_offset: u32,
    pages: usize,
    terminal: bool,
    rejected: bool,
    records: HashMap<SecurityId, MetadataRecord>,
}

impl MarketCandidate {
    pub(crate) fn new(key: SnapshotKey) -> Result<Self> {
        if key.family() != super::ProtocolFamily::Hq || !matches!(key.market(), super::snapshot::MarketScope::Hq(_)) {
            return Err(Error::new(ErrorKind::InvalidArgument, "HQ market candidate requires an HQ snapshot key"));
        }
        Ok(Self { key, next_offset: 0, pages: 0, terminal: false, rejected: false, records: HashMap::new() })
    }

    pub(crate) fn push_page(
        &mut self,
        offset: u32,
        records: Vec<MetadataRecord>,
        evidence: PageEvidence,
    ) -> Result<()> {
        if self.rejected { return Err(candidate_error("metadata candidate was previously rejected")); }
        let result = self.push_page_inner(offset, records, evidence);
        if result.is_err() { self.rejected = true; }
        result
    }

    fn push_page_inner(
        &mut self, offset: u32, records: Vec<MetadataRecord>, evidence: PageEvidence,
    ) -> Result<()> {
        if self.terminal {
            return Err(candidate_error("page received after terminal market evidence"));
        }
        if offset != self.next_offset {
            return Err(candidate_error("market page offset is not contiguous")
                .with_context("expected_offset", format!("{}", self.next_offset))
                .with_context("actual_offset", format!("{offset}")));
        }
        if self.pages >= MAX_MARKET_PAGES || self.records.len().checked_add(records.len()).map_or(true, |count| count > MAX_MARKET_RECORDS) {
            return Err(Error::new(ErrorKind::LimitExceeded, "metadata market exceeds bounded catalog work")
                .with_context("limit_source", "HQ u16 catalog count"));
        }
        let count = u32::try_from(records.len()).map_err(|_| candidate_error("market page record count overflows u32"))?;
        match evidence {
            PageEvidence::More if count == 0 => return Err(candidate_error("empty page must be terminal evidence")),
            PageEvidence::TerminalEmpty if count != 0 => return Err(candidate_error("terminal empty evidence contains records")),
            PageEvidence::TerminalTotal(total) if offset.checked_add(count) != Some(total) => {
                return Err(candidate_error("terminal total does not match accumulated market records")
                    .with_context("declared_total", format!("{total}"))
                    .with_context("accumulated_total", format!("{}", offset.saturating_add(count))));
            }
            _ => {}
        }

        let market = match self.key.market() {
            super::snapshot::MarketScope::Hq(market) => *market,
            super::snapshot::MarketScope::Extension(_) => return Err(candidate_error("HQ candidate has a non-HQ market key")),
        };
        for record in records {
            if record.id().market() != market {
                return Err(candidate_error("security identity belongs to a different market")
                    .with_context("security", format!("{}", record.id())));
            }
            if self.records.contains_key(record.id()) {
                return Err(candidate_error("duplicate security identity in market candidate")
                    .with_context("security", format!("{}", record.id())));
            }
            self.records.insert(record.id().clone(), record);
        }
        self.pages = self.pages.checked_add(1).ok_or_else(|| candidate_error("market page count overflows usize"))?;
        self.next_offset = self.next_offset.checked_add(count).ok_or_else(|| candidate_error("market offset overflows u32"))?;
        self.terminal = matches!(evidence, PageEvidence::TerminalEmpty | PageEvidence::TerminalTotal(_));
        Ok(())
    }

    fn into_records(self) -> Result<(SnapshotKey, usize, HashMap<SecurityId, MetadataRecord>)> {
        if self.rejected || !self.terminal {
            return Err(candidate_error("market metadata candidate has no terminal evidence"));
        }
        if self.pages == 0 {
            return Err(candidate_error("market metadata candidate has no response pages"));
        }
        Ok((self.key, self.pages, self.records))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FlightPolicy {
    source_timeout: Option<Duration>,
    timeout_source: &'static str,
    cleanup_timeout: Duration,
}

impl FlightPolicy {
    /// Rust source-work cap; its clock begins when the shared flight is created.
    pub(crate) const fn rust_source_cap(source_timeout: Duration, cleanup_timeout: Duration) -> Self {
        Self { source_timeout: Some(source_timeout), timeout_source: "ClientConfig::request_timeout", cleanup_timeout }
    }

    /// Compatibility adapter retains Python's per-request timing without a new total prefetch cap.
    pub(crate) const fn legacy_per_request(cleanup_timeout: Duration) -> Self {
        Self { source_timeout: None, timeout_source: "legacy per-request policy", cleanup_timeout }
    }
}

#[derive(Clone, Debug)]
struct FlightWaiter {
    id: u64,
    sponsor: OperationSponsor,
}

#[derive(Debug, Default)]
struct FlightResult {
    result: Option<Result<Arc<MetadataSnapshot>>>,
}

#[derive(Debug)]
struct Flight {
    created_at: Instant,
    source_deadline: Option<Instant>,
    timeout_source: &'static str,
    cleanup_timeout: Duration,
    stop_requested: AtomicBool,
    cleanup_deadline: Mutex<Option<Instant>>,
    waiters: Mutex<Vec<FlightWaiter>>,
    result: Mutex<FlightResult>,
    changed: Condvar,
}

impl Flight {
    fn new(policy: FlightPolicy, first_waiter: FlightWaiter) -> Result<Self> {
        if policy.cleanup_timeout.is_zero() {
            return Err(Error::new(ErrorKind::InvalidArgument, "metadata cleanup timeout must be positive"));
        }
        let created_at = Instant::now();
        let source_deadline = policy.source_timeout.map(|timeout| created_at.checked_add(timeout)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "metadata source timeout overflows monotonic clock")))
            .transpose()?;
        Ok(Self {
            created_at,
            source_deadline,
            timeout_source: policy.timeout_source,
            cleanup_timeout: policy.cleanup_timeout,
            stop_requested: AtomicBool::new(false),
            cleanup_deadline: Mutex::new(None),
            waiters: Mutex::new(vec![first_waiter]),
            result: Mutex::new(FlightResult::default()),
            changed: Condvar::new(),
        })
    }

    fn register(&self, waiter: FlightWaiter) -> Result<()> {
        let mut waiters = self.waiters.lock().map_err(|_| flight_poisoned())?;
        if self.stop_requested.load(Ordering::Acquire) {
            return Err(cancelled_error("metadata flight no longer has eligible waiters"));
        }
        if waiters.len() >= MAX_FLIGHT_WAITERS {
            return Err(Error::new(ErrorKind::LimitExceeded, "metadata flight waiter capacity reached"));
        }
        waiters.push(waiter);
        self.changed.notify_all();
        Ok(())
    }

    fn deregister(&self, id: u64) {
        if let Ok(mut waiters) = self.waiters.lock() {
            waiters.retain(|waiter| waiter.id != id);
            if waiters.is_empty() {
                self.request_stop();
            }
        } else {
            self.request_stop();
        }
        self.changed.notify_all();
    }

    fn request_stop(&self) {
        if let Some(deadline) = Instant::now().checked_add(self.cleanup_timeout) {
            if let Ok(mut bound) = self.cleanup_deadline.lock() {
                *bound = Some(bound.map_or(deadline, |previous| previous.min(deadline)));
            }
        }
        self.stop_requested.store(true, Ordering::Release);
        self.changed.notify_all();
    }

    fn stop_before(&self, deadline: Instant) {
        if let Ok(mut bound) = self.cleanup_deadline.lock() {
            *bound = Some(bound.map_or(deadline, |previous| previous.min(deadline)));
        }
        self.request_stop();
    }

    fn active_sponsors(&self) -> Result<Vec<FlightWaiter>> {
        let current = self.waiters.lock().map_err(|_| flight_poisoned())?.clone();
        let sampled_ids: HashSet<u64> = current.iter().map(|waiter| waiter.id).collect();
        let mut active = Vec::with_capacity(current.len());
        let mut invalid_ids = HashSet::with_capacity(current.len());
        for waiter in current {
            if waiter.sponsor.check_boundary().is_ok() {
                active.push(waiter);
            } else {
                invalid_ids.insert(waiter.id);
            }
        }
        let mut waiters = self.waiters.lock().map_err(|_| flight_poisoned())?;
        // Do not erase waiters registered after the snapshot above.
        waiters.retain(|waiter| !invalid_ids.contains(&waiter.id));
        active.extend(waiters.iter().filter(|waiter| !sampled_ids.contains(&waiter.id)).cloned());
        if waiters.is_empty() {
            self.request_stop();
        }
        if let Some(deadline) = self.source_deadline {
            if Instant::now() >= deadline { self.request_stop(); }
        }
        Ok(active)
    }

    fn check_source(&self) -> Result<()> {
        if let Some(deadline) = self.source_deadline {
            if Instant::now() >= deadline {
                self.request_stop();
                return Err(self.source_timeout_error());
            }
        }
        if self.stop_requested.load(Ordering::Acquire) {
            if self.source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(self.source_timeout_error());
            }
            return Err(cancelled_error("metadata source work has no eligible waiters"));
        }
        if self.active_sponsors()?.is_empty() {
            return Err(cancelled_error("metadata source work has no eligible waiters"));
        }
        Ok(())
    }

    fn source_timeout_error(&self) -> Error {
        let limit = self.source_deadline.map_or(Duration::ZERO, |deadline| deadline.saturating_duration_since(self.created_at));
        Error::new(ErrorKind::Timeout, "metadata source-work cap elapsed")
            .with_context("limit_source", self.timeout_source)
            .with_context("limit_ms", format!("{}", limit.as_millis()))
    }

    fn complete(&self, result: Result<Arc<MetadataSnapshot>>) -> Result<()> {
        let mut state = self.result.lock().map_err(|_| flight_poisoned())?;
        if state.result.is_some() {
            return Err(Error::new(ErrorKind::Internal, "metadata flight completed more than once"));
        }
        state.result = Some(result);
        self.changed.notify_all();
        Ok(())
    }

    fn completed(&self) -> Result<Option<Result<Arc<MetadataSnapshot>>>> {
        Ok(self.result.lock().map_err(|_| flight_poisoned())?.result.clone())
    }
}

#[derive(Debug)]
struct FlightWaiterGuard {
    flight: Arc<Flight>,
    id: u64,
    sponsor: OperationSponsor,
}

impl Drop for FlightWaiterGuard {
    fn drop(&mut self) { self.flight.deregister(self.id); }
}

#[derive(Debug, Default)]
struct StoreState {
    snapshots: HashMap<SnapshotKey, Arc<MetadataSnapshot>>,
    flights: HashMap<SnapshotKey, Arc<Flight>>,
}

struct StoreInner {
    state: Mutex<StoreState>,
    next_version: AtomicU64,
    next_waiter: AtomicU64,
    clock: Arc<dyn Clock>,
    sealed: AtomicBool,
}

pub(crate) struct MetadataStore {
    inner: Arc<StoreInner>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for MetadataStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("MetadataStore").finish_non_exhaustive()
    }
}

impl MetadataStore {
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(StoreInner {
                state: Mutex::new(StoreState::default()), next_version: AtomicU64::new(1),
                next_waiter: AtomicU64::new(1), clock,
                sealed: AtomicBool::new(false),
            }),
            workers: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn current(&self, key: &SnapshotKey) -> Result<Option<Arc<MetadataSnapshot>>> {
        let now = self.inner.clock.unix_seconds()?;
        Ok(self.inner.state.lock().map_err(|_| flight_poisoned())?.snapshots.get(key)
            .filter(|snapshot| snapshot.is_fresh_at(now)).cloned())
    }

    pub(crate) fn last_known_good(&self, key: &SnapshotKey) -> Result<Option<Arc<MetadataSnapshot>>> {
        Ok(self.inner.state.lock().map_err(|_| flight_poisoned())?.snapshots.get(key).cloned())
    }

    pub(crate) fn capture(&self, namespace: &str, snapshots: &[Arc<MetadataSnapshot>]) -> Result<Arc<MetadataSnapshot>> {
        if snapshots.len() == 1 && snapshots[0].key().namespace() == namespace {
            return Ok(Arc::clone(&snapshots[0]));
        }
        let version = if snapshots.is_empty() { 0 } else {
            self.inner.next_version.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
                .map_err(|_| Error::new(ErrorKind::Internal, "metadata capture version exhausted"))?
        };
        Ok(Arc::new(MetadataSnapshot::capture(namespace, version, snapshots)?))
    }

    /// The sole production loader owns only runtime state and cooperatively
    /// polls its source/cleanup bounds; arbitrary callbacks are test-only.
    pub(crate) fn load_hq_market(
        &self, engine: eltdx_runtime::engine::Engine, namespace: &str, market: HqMarket,
        sponsor: OperationSponsor, policy: FlightPolicy, request_timeout: Duration,
        ttl: Duration, refresh: bool,
    ) -> Result<Arc<MetadataSnapshot>> {
        let key = SnapshotKey::hq(namespace, market);
        self.load_or_refresh(key, sponsor, policy, ttl, refresh, move |control| {
            super::loader::load_market(&engine, market, request_timeout, control)
        })
    }

    #[cfg(test)]
    fn get_or_load<F>(
        &self,
        key: SnapshotKey,
        sponsor: OperationSponsor,
        policy: FlightPolicy,
        ttl: Duration,
        load: F,
    ) -> Result<Arc<MetadataSnapshot>>
    where
        F: FnOnce(&FlightControl) -> Result<MarketCandidate> + Send + 'static,
    {
        self.load_or_refresh(key, sponsor, policy, ttl, false, load)
    }

    #[cfg(test)]
    fn refresh<F>(
        &self, key: SnapshotKey, sponsor: OperationSponsor, policy: FlightPolicy, ttl: Duration, load: F,
    ) -> Result<Arc<MetadataSnapshot>>
    where
        F: FnOnce(&FlightControl) -> Result<MarketCandidate> + Send + 'static,
    {
        self.load_or_refresh(key, sponsor, policy, ttl, true, load)
    }

    fn load_or_refresh<F>(
        &self, key: SnapshotKey, sponsor: OperationSponsor, policy: FlightPolicy, ttl: Duration,
        refresh: bool, load: F,
    ) -> Result<Arc<MetadataSnapshot>>
    where
        // Loader captures only runtime data, never SessionOwner/OperationContext.
        F: FnOnce(&FlightControl) -> Result<MarketCandidate> + Send + 'static,
    {
        sponsor.check_boundary()?;
        if ttl.is_zero() {
            return Err(Error::new(ErrorKind::InvalidArgument, "metadata TTL must be positive"));
        }
        let now = self.inner.clock.unix_seconds()?;
        let waiter_id = self.inner.next_waiter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map_err(|_| Error::new(ErrorKind::Internal, "metadata waiter identity exhausted"))?;
        let waiter = FlightWaiter { id: waiter_id, sponsor: sponsor.clone() };
        let (flight, leader) = {
            let mut state = self.inner.state.lock().map_err(|_| flight_poisoned())?;
            if self.inner.sealed.load(Ordering::Acquire) {
                return Err(Error::new(ErrorKind::Closed, "metadata store is closed"));
            }
            if let Some(snapshot) = state.snapshots.get(&key).filter(|snapshot| !refresh && snapshot.is_fresh_at(now)) {
                let snapshot = Arc::clone(snapshot);
                drop(state);
                sponsor.check_boundary()?;
                return Ok(snapshot);
            }
            if let Some(flight) = state.flights.get(&key).filter(|flight| !flight.stop_requested.load(Ordering::Acquire)) {
                let flight = Arc::clone(flight);
                flight.register(waiter)?;
                (flight, false)
            } else {
                state.flights.remove(&key);
                if state.flights.len() >= MAX_ACTIVE_FLIGHTS {
                    return Err(Error::new(ErrorKind::LimitExceeded, "metadata concurrent flight capacity reached")
                        .with_context("limit_source", "owner metadata source-work cap"));
                }
                let flight = Arc::new(Flight::new(policy, waiter)?);
                state.flights.insert(key.clone(), Arc::clone(&flight));
                (flight, true)
            }
        };
        let waiter_guard = FlightWaiterGuard { flight: Arc::clone(&flight), id: waiter_id, sponsor };
        let control = FlightControl { flight: Arc::clone(&flight), key: key.clone() };

        if leader {
            self.start_worker(key, Arc::clone(&flight), control.clone(), ttl, load)?;
        }

        let result = wait_for_flight(&flight, &waiter_guard, &control)?;
        Ok(result)
    }

    fn start_worker<F>(
        &self, key: SnapshotKey, flight: Arc<Flight>, control: FlightControl, ttl: Duration, load: F,
    ) -> Result<()>
    where F: FnOnce(&FlightControl) -> Result<MarketCandidate> + Send + 'static {
        // Registration and shutdown serialize on this mutex. Once shutdown can
        // observe an empty worker set, no late worker can appear behind it.
        let mut workers = self.workers.lock().map_err(|_| flight_poisoned())?;
        let mut active = Vec::with_capacity(workers.len() + 1);
        for previous in workers.drain(..) {
            if previous.is_finished() { let _ = previous.join(); } else { active.push(previous); }
        }
        *workers = active;
        if self.inner.sealed.load(Ordering::Acquire) || workers.len() >= MAX_ACTIVE_FLIGHTS {
            flight.request_stop();
            let error = if self.inner.sealed.load(Ordering::Acquire) {
                Error::new(ErrorKind::Closed, "metadata store is closed")
            } else {
                Error::new(ErrorKind::LimitExceeded, "metadata worker capacity reached")
            };
            let _ = flight.complete(Err(error.clone()));
            return Err(error);
        }
        // The worker owns no strong store/owner reference. Completion always lives in flight.
        let weak_inner = Arc::downgrade(&self.inner);
        let failure_flight = Arc::clone(&flight);
        let worker = thread::Builder::new().name("eltdx-metadata".to_owned()).spawn(move || {
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let candidate = load(&control)?;
                control.check_active()?;
                let inner = weak_inner.upgrade().ok_or_else(|| Error::new(ErrorKind::Closed, "metadata store is closed"))?;
                inner.publish(candidate, &key, ttl, &flight)
            })).unwrap_or_else(|_| Err(Error::new(ErrorKind::Internal, "metadata loader panicked")));
            let _ = flight.complete(loaded);
            if let Some(inner) = weak_inner.upgrade() {
                if let Ok(mut state) = inner.state.lock() {
                    if state.flights.get(&key).is_some_and(|current| Arc::ptr_eq(current, &flight)) {
                        state.flights.remove(&key);
                    }
                }
            }
        }).map_err(|error| {
            failure_flight.request_stop();
            let error = Error::new(ErrorKind::Internal, "unable to start metadata worker").with_source(error);
            let _ = failure_flight.complete(Err(error.clone()));
            error
        })?;
        workers.push(worker);
        Ok(())
    }

    pub(crate) fn stop_all(&self) {
        self.inner.sealed.store(true, Ordering::Release);
        if let Ok(state) = self.inner.state.lock() {
            for flight in state.flights.values() { flight.request_stop(); }
        }
    }

    /// The owner's Stage B shutdown calls this outside its state lock.
    pub(crate) fn close_until(&self, deadline: Instant) -> Result<()> {
        self.stop_before(deadline);
        loop {
            let mut workers = self.workers.lock().map_err(|_| flight_poisoned())?;
            let mut active = Vec::with_capacity(workers.len());
            for worker in workers.drain(..) {
                if worker.is_finished() { let _ = worker.join(); } else { active.push(worker); }
            }
            *workers = active;
            if workers.is_empty() { return Ok(()); }
            if Instant::now() >= deadline {
                return Err(Error::new(ErrorKind::CloseTimeout, "metadata workers did not retire before owner close deadline"));
            }
            drop(workers);
            thread::sleep(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(2)));
        }
    }

    fn stop_before(&self, deadline: Instant) {
        self.inner.sealed.store(true, Ordering::Release);
        if let Ok(state) = self.inner.state.lock() {
            for flight in state.flights.values() { flight.stop_before(deadline); }
        }
    }
}

impl StoreInner {
    fn publish(&self, candidate: MarketCandidate, expected_key: &SnapshotKey, ttl: Duration, flight: &Arc<Flight>) -> Result<Arc<MetadataSnapshot>> {
        let (key, pages, records) = candidate.into_records()?;
        if &key != expected_key {
            return Err(candidate_error("metadata candidate key does not match requested market"));
        }
        if pages == 0 { return Err(candidate_error("metadata market completed without any response page")); }
        let fetched_at = self.clock.unix_seconds()?;
        let ttl_seconds = ttl.as_secs().checked_add(u64::from(ttl.subsec_nanos() != 0))
            .and_then(|seconds| i64::try_from(seconds).ok())
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "metadata TTL exceeds epoch range"))?;
        let expires_at = fetched_at.checked_add(ttl_seconds)
            .ok_or_else(|| Error::new(ErrorKind::InvalidArgument, "metadata expiry overflows epoch range"))?;
        flight.check_source()?;
        let mut state = self.state.lock().map_err(|_| flight_poisoned())?;
        if self.sealed.load(Ordering::Acquire) || flight.stop_requested.load(Ordering::Acquire)
            || !state.flights.get(&key).is_some_and(|current| Arc::ptr_eq(current, flight)) {
            return Err(cancelled_error("superseded metadata flight cannot publish"));
        }
        if flight.source_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(flight.source_timeout_error());
        }
        let version = self.next_version.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map_err(|_| Error::new(ErrorKind::Internal, "metadata snapshot version exhausted"))?;
        let snapshot = Arc::new(MetadataSnapshot::new(key.clone(), version, fetched_at, Some(expires_at), records));
        state.snapshots.insert(key, Arc::clone(&snapshot));
        Ok(snapshot)
    }
}

impl Drop for MetadataStore {
    fn drop(&mut self) { self.stop_all(); }
}

impl crate::session::OwnerTaskGroup for MetadataStore {
    fn stop(&self) { self.stop_all(); }
    fn stop_before(&self, deadline: Instant) { MetadataStore::stop_before(self, deadline); }
    fn close_until(&self, deadline: Instant) -> Result<()> {
        MetadataStore::close_until(self, deadline)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FlightControl {
    flight: Arc<Flight>,
    key: SnapshotKey,
}

impl FlightControl {
    pub(crate) fn key(&self) -> &SnapshotKey { &self.key }
    pub(crate) fn source_deadline(&self) -> Option<Instant> { self.flight.source_deadline }
    pub(crate) fn cleanup_timeout(&self) -> Duration { self.flight.cleanup_timeout }
    pub(crate) fn cleanup_deadline(&self, local: Instant) -> Result<Instant> {
        let bound = *self.flight.cleanup_deadline.lock().map_err(|_| flight_poisoned())?;
        Ok(bound.map_or(local, |owner| local.min(owner)))
    }
    pub(crate) fn timeout_source(&self) -> &'static str { self.flight.timeout_source }

    pub(crate) fn check_active(&self) -> Result<()> { self.flight.check_source() }

    pub(crate) fn retry_policy(&self, request: &CommandRequest) -> Result<RetryPolicy> {
        self.check_active()?;
        self.flight.active_sponsors()?.first()
            .map(|waiter| waiter.sponsor.retry_policy(request))
            .ok_or_else(|| cancelled_error("metadata request has no eligible sponsor"))
    }

    /// One proxy per logical request caches one metadata authorizer per sponsor.
    pub(crate) fn attempt_authorization(&self) -> Arc<dyn AttemptAuthorization> {
        Arc::new(MetadataAttemptAuthorization {
            state: Arc::new(MetadataAttemptState {
                flight: Arc::clone(&self.flight),
                authorizers: Mutex::new(HashMap::new()),
            }),
        })
    }
}

#[derive(Debug)]
struct CachedAuthorizer {
    authorizer: Arc<dyn AttemptAuthorization>,
}

#[derive(Debug)]
struct MetadataAttemptAuthorization {
    state: Arc<MetadataAttemptState>,
}

#[derive(Debug)]
struct MetadataAttemptState {
    flight: Arc<Flight>,
    authorizers: Mutex<HashMap<u64, CachedAuthorizer>>,
}

impl MetadataAttemptState {
    fn authorizers_for_live_sponsors(&self) -> std::result::Result<Vec<(u64, OperationSponsor, Arc<dyn AttemptAuthorization>)>, RuntimeError> {
        let sponsors = self.flight.active_sponsors().map_err(to_runtime)?;
        let mut output = Vec::with_capacity(sponsors.len());
        for waiter in sponsors {
            let authorizer = {
                let mut cache = self.authorizers.lock().map_err(|_| RuntimeError::internal("metadata authorizer cache is poisoned"))?;
                if let Some(cached) = cache.get(&waiter.id) {
                    Arc::clone(&cached.authorizer)
                } else {
                    let authorizer = match waiter.sponsor.metadata_authorization() {
                        Ok(authorizer) => authorizer,
                        Err(_) => continue,
                    };
                    cache.insert(waiter.id, CachedAuthorizer { authorizer: Arc::clone(&authorizer) });
                    authorizer
                }
            };
            output.push((waiter.id, waiter.sponsor, authorizer));
        }
        if output.is_empty() { return Err(to_runtime(cancelled_error("metadata request has no eligible sponsor"))); }
        Ok(output)
    }

    fn reserve_for(
        &self,
        attempt: u32,
        last_failure: Option<&RuntimeError>,
        excluded: &HashSet<u64>,
        endpoint_continuation: bool,
    ) -> std::result::Result<(u64, Box<dyn ReservedAttempt>), RuntimeError> {
        self.flight.check_source().map_err(to_runtime)?;
        let mut last_error = None;
        for (id, _sponsor, authorizer) in self.authorizers_for_live_sponsors()? {
            if excluded.contains(&id) { continue; }
            let reservation = if endpoint_continuation {
                authorizer.reserve_endpoint_continuation(attempt, last_failure)
            } else {
                authorizer.reserve(attempt, last_failure)
            };
            match reservation {
                Ok(reservation) => return Ok((id, reservation)),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| to_runtime(cancelled_error("metadata attempt has no eligible sponsor"))))
    }
}

impl AttemptAuthorization for MetadataAttemptAuthorization {
    fn check_boundary(&self, now: Instant) -> std::result::Result<(), RuntimeError> {
        self.state.flight.check_source().map_err(to_runtime)?;
        let sponsors = self.state.authorizers_for_live_sponsors()?;
        let mut last_error = None;
        for (_, _, authorizer) in sponsors {
            match authorizer.check_boundary(now) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| to_runtime(cancelled_error("metadata request has no eligible sponsor"))))
    }

    fn reserve(
        &self,
        attempt: u32,
        last_failure: Option<&RuntimeError>,
    ) -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
        self.reserve_kind(attempt, last_failure, false)
    }

    fn reserve_endpoint_continuation(
        &self, attempt: u32, last_failure: Option<&RuntimeError>,
    ) -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
        self.reserve_kind(attempt, last_failure, true)
    }

    fn count_endpoint_failover(&self) -> bool {
        self.state.authorizers_for_live_sponsors().ok().and_then(|sponsors| sponsors.first().map(|(_, _, auth)| auth.count_endpoint_failover())).unwrap_or(false)
    }
}

impl MetadataAttemptAuthorization {
    fn reserve_kind(
        &self, attempt: u32, last_failure: Option<&RuntimeError>, endpoint_continuation: bool,
    ) -> std::result::Result<Box<dyn ReservedAttempt>, RuntimeError> {
        let (sponsor_id, reservation) = self.state.reserve_for(attempt, last_failure, &HashSet::new(), endpoint_continuation)?;
        Ok(Box::new(SponsoredReservation {
            state: Arc::clone(&self.state),
            attempt,
            sponsor_id,
            last_failure: last_failure.cloned(),
            reservation: Some(reservation),
            endpoint_continuation,
        }))
    }
}

#[derive(Debug)]
struct SponsoredReservation {
    state: Arc<MetadataAttemptState>,
    attempt: u32,
    sponsor_id: u64,
    last_failure: Option<RuntimeError>,
    reservation: Option<Box<dyn ReservedAttempt>>,
    endpoint_continuation: bool,
}

impl ReservedAttempt for SponsoredReservation {
    fn start(self: Box<Self>, now: Instant) -> std::result::Result<(), RuntimeError> {
        self.start_with_bound(now, None)
    }

    fn start_before(self: Box<Self>, deadline: Instant) -> std::result::Result<(), RuntimeError> {
        self.start_with_bound(Instant::now(), Some(deadline))
    }
}

impl SponsoredReservation {
    fn start_with_bound(mut self: Box<Self>, now: Instant, bound: Option<Instant>) -> std::result::Result<(), RuntimeError> {
        let mut excluded = HashSet::new();
        let mut candidate = Some((self.sponsor_id, self.reservation.take().ok_or_else(|| RuntimeError::internal("metadata reservation was already consumed"))?));
        let last_error = self.last_failure.clone();
        loop {
            let (id, reservation) = match candidate.take() {
                Some(candidate) => candidate,
                None => {
                    let failure = last_error.as_ref();
                    match self.state.reserve_for(self.attempt, failure, &excluded, self.endpoint_continuation) {
                        Ok((id, reservation)) => (id, reservation),
                        Err(error) => return Err(error),
                    }
                }
            };
            excluded.insert(id);
            let live = self.state.authorizers_for_live_sponsors()?.into_iter().any(|(current, _, _)| current == id);
            if !live {
                drop(reservation);
                continue;
            }
            self.state.flight.check_source().map_err(to_runtime)?;
            let started = match bound {
                Some(deadline) => reservation.start_before(deadline),
                None => reservation.start(now.max(Instant::now())),
            };
            match started {
                Ok(()) => return Ok(()),
                Err(_) => {}
            }
        }
    }
}

fn wait_for_flight(
    flight: &Arc<Flight>,
    waiter: &FlightWaiterGuard,
    control: &FlightControl,
) -> Result<Arc<MetadataSnapshot>> {
    loop {
        if let Err(root) = waiter.sponsor.check_boundary() {
            return Err(retire_waiter(flight, waiter.id, control, root));
        }
        if let Some(result) = flight.completed()? {
            if let Err(root) = waiter.sponsor.check_boundary() {
                return Err(retire_waiter(flight, waiter.id, control, root));
            }
            return result;
        }
        if let Err(root) = flight.check_source() {
            return Err(retire_waiter(flight, waiter.id, control, root));
        }
        if let Some(deadline) = control.source_deadline() {
            if Instant::now() >= deadline {
                return Err(retire_waiter(flight, waiter.id, control, flight.source_timeout_error()));
            }
        }
        let guard = flight.result.lock().map_err(|_| flight_poisoned())?;
        let remaining = control.source_deadline().map_or(Duration::from_millis(20), |deadline| {
            deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(20))
        });
        let _ = flight.changed.wait_timeout(guard, remaining).map_err(|_| flight_poisoned())?;
    }
}

fn retire_waiter(flight: &Arc<Flight>, id: u64, control: &FlightControl, root: Error) -> Error {
    flight.deregister(id);
    // A departing sponsor never waits for or cancels surviving sponsors' work.
    if !flight.stop_requested.load(Ordering::Acquire) { return root; }
    let local = Instant::now().checked_add(control.cleanup_timeout()).unwrap_or_else(Instant::now);
    loop {
        match flight.completed() {
            Ok(Some(Err(error))) if error.kind() == ErrorKind::CancelUnconfirmed => {
                let cleanup = error.cleanup_failure().cloned().unwrap_or(error);
                return Error::new(ErrorKind::CancelUnconfirmed, "metadata waiter stopped but source retirement was not confirmed")
                    .with_context("root_kind", format!("{:?}", root.kind())).with_source(root).with_cleanup_failure(cleanup);
            }
            Ok(Some(_)) => return root,
            Err(error) => return error,
            Ok(None) => {}
        }
        let deadline = match control.cleanup_deadline(local) { Ok(deadline) => deadline, Err(error) => return error };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Error::new(ErrorKind::CancelUnconfirmed, "metadata source worker did not retire before cleanup deadline")
                .with_context("root_kind", format!("{:?}", root.kind())).with_source(root)
                .with_cleanup_failure(Error::new(ErrorKind::Timeout, "metadata worker cleanup deadline elapsed"));
        }
        let guard = match flight.result.lock() { Ok(guard) => guard, Err(_) => return flight_poisoned() };
        if flight.changed.wait_timeout(guard, remaining.min(Duration::from_millis(20))).is_err() { return flight_poisoned(); }
    }
}

fn candidate_error(message: &str) -> Error { Error::new(ErrorKind::Protocol, message).with_context("stage", "metadata candidate") }
fn flight_poisoned() -> Error { Error::new(ErrorKind::Internal, "metadata store state is poisoned") }
fn cancelled_error(message: &str) -> Error { Error::new(ErrorKind::Cancelled, message).with_context("stage", "metadata flight") }

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::atomic::AtomicI64;
    use crate::config::ConfigDraft;
    use crate::metadata::PrecisionMetadata;
    use crate::operation::{CancellationToken, OperationContext, OperationOptions};
    use crate::session::SessionOwner;
    use crate::types::ValueProvenance;

    struct TestClock(AtomicI64);
    impl Clock for TestClock {
        fn unix_seconds(&self) -> Result<i64> { Ok(self.0.load(Ordering::Acquire)) }
    }

    fn operation(token: CancellationToken) -> OperationContext {
        let config = ConfigDraft::default().validate().unwrap();
        let owner = Arc::new(SessionOwner::new(config.close_timeout(), config.cleanup_timeout()));
        OperationContext::rust_public(Instant::now(), "metadata-test", owner, &config,
            &OperationOptions::default().with_cancellation(token)).unwrap()
    }

    fn record(decimal: u8) -> MetadataRecord {
        MetadataRecord::new(SecurityId::new(HqMarket::Shanghai, "600000").unwrap(),
            Some(PrecisionMetadata::new(decimal, ValueProvenance::SecurityTable).unwrap()), None, None)
    }

    fn candidate(key: SnapshotKey, decimal: u8) -> MarketCandidate {
        let mut candidate = MarketCandidate::new(key).unwrap();
        candidate.push_page(0, vec![record(decimal)], PageEvidence::More).unwrap();
        candidate.push_page(1, vec![], PageEvidence::TerminalEmpty).unwrap();
        candidate
    }

    #[test]
    fn candidate_needs_terminal_evidence_and_rejects_duplicate_or_wrong_identity() {
        let key = SnapshotKey::hq("source-a", HqMarket::Shanghai);
        let mut partial = MarketCandidate::new(key.clone()).unwrap();
        partial.push_page(0, vec![record(2)], PageEvidence::More).unwrap();
        assert!(partial.into_records().is_err());
        let mut duplicate = MarketCandidate::new(key.clone()).unwrap();
        duplicate.push_page(0, vec![record(2)], PageEvidence::More).unwrap();
        assert!(duplicate.push_page(1, vec![record(3)], PageEvidence::More).is_err());
        let mut wrong = MarketCandidate::new(key).unwrap();
        let wrong_id = MetadataRecord::new(SecurityId::new(HqMarket::Beijing, "430047").unwrap(), None, None, None);
        assert!(wrong.push_page(0, vec![wrong_id], PageEvidence::TerminalTotal(1)).is_err());
    }

    #[test]
    fn half_load_failure_keeps_old_snapshot_and_successful_refresh_freezes_prior_arc() {
        let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
        let store = MetadataStore::new(clock.clone());
        let key = SnapshotKey::hq("source-a", HqMarket::Shanghai);
        let context = operation(CancellationToken::default());
        let policy = FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1));
        let old = store.get_or_load(key.clone(), context.sponsor(), policy, Duration::from_secs(60),
            { let key = key.clone(); move |_| Ok(candidate(key, 2)) }).unwrap();
        let failed = store.refresh(key.clone(), context.sponsor(), policy, Duration::from_secs(60), move |_| {
            Err(Error::new(ErrorKind::Connection, "second market page failed"))
        });
        assert_eq!(failed.unwrap_err().kind(), ErrorKind::Connection);
        assert_eq!(store.last_known_good(&key).unwrap().unwrap().version(), old.version());
        let fresh = store.refresh(key.clone(), context.sponsor(), policy, Duration::from_secs(60),
            { let key = key.clone(); move |_| Ok(candidate(key, 4)) }).unwrap();
        let id = SecurityId::new(HqMarket::Shanghai, "600000").unwrap();
        assert_eq!(old.require_precision(&id).unwrap().decimals(), 2);
        assert_eq!(fresh.require_precision(&id).unwrap().decimals(), 4);
        assert!(fresh.version() > old.version());
        clock.0.store(1_060, Ordering::Release);
        assert!(store.current(&key).unwrap().is_none());
        assert!(store.last_known_good(&key).unwrap().is_some());
        store.close_until(Instant::now() + Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn independent_owner_stores_can_hold_conflicting_security_precision() {
        let context_a = operation(CancellationToken::default());
        let context_b = operation(CancellationToken::default());
        let policy = FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1));
        let key = SnapshotKey::hq("same-catalog", HqMarket::Shanghai);
        let store_a = context_a.owner().metadata();
        let store_b = context_b.owner().metadata();
        let a = store_a.get_or_load(key.clone(), context_a.sponsor(), policy, Duration::from_secs(60),
            { let key = key.clone(); move |_| Ok(candidate(key, 2)) }).unwrap();
        let b = store_b.get_or_load(key.clone(), context_b.sponsor(), policy, Duration::from_secs(60),
            move |_| Ok(candidate(key, 4))).unwrap();
        let id = SecurityId::new(HqMarket::Shanghai, "600000").unwrap();
        assert_eq!(a.require_precision(&id).unwrap().decimals(), 2);
        assert_eq!(b.require_precision(&id).unwrap().decimals(), 4);
    }

    #[test]
    fn operation_captures_multiple_markets_and_refuses_replacement_after_first_decode() {
        let context = operation(CancellationToken::default());
        let store = context.owner().metadata();
        let policy = FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1));
        let sh_key = SnapshotKey::hq("capture", HqMarket::Shanghai);
        let sh = store.get_or_load(sh_key.clone(), context.sponsor(), policy, Duration::from_secs(60),
            move |_| Ok(candidate(sh_key, 4))).unwrap();
        let bj_key = SnapshotKey::hq("capture", HqMarket::Beijing);
        let bj = store.get_or_load(bj_key.clone(), context.sponsor(), policy, Duration::from_secs(60), move |_| {
            let mut candidate = MarketCandidate::new(bj_key).unwrap();
            let record = MetadataRecord::new(SecurityId::new(HqMarket::Beijing, "430047").unwrap(),
                Some(PrecisionMetadata::new(2, ValueProvenance::SecurityTable).unwrap()), None, None);
            candidate.push_page(0, vec![record], PageEvidence::More).unwrap();
            candidate.push_page(1, vec![], PageEvidence::TerminalEmpty).unwrap();
            Ok(candidate)
        }).unwrap();
        let frozen = store.capture("capture", &[sh, bj]).unwrap();
        assert!(frozen.covers_market(HqMarket::Shanghai));
        assert!(frozen.covers_market(HqMarket::Beijing));
        assert!(!frozen.covers_market(HqMarket::Shenzhen));
        context.install_metadata(Arc::clone(&frozen)).unwrap();
        assert!(Arc::ptr_eq(&context.begin_decode().unwrap(), &frozen));
        assert_eq!(context.install_metadata(Arc::new(MetadataSnapshot::empty("replacement"))).unwrap_err().kind(),
            ErrorKind::InvalidArgument);
        assert!(Arc::ptr_eq(&context.begin_decode().unwrap(), &frozen));
    }

    #[test]
    fn closing_owner_stops_and_joins_its_metadata_worker() {
        let context = operation(CancellationToken::default());
        let owner = Arc::clone(context.owner());
        let store = Arc::clone(owner.metadata());
        let observed_store = Arc::clone(&store);
        let (started_tx, started_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = store.get_or_load(SnapshotKey::hq("owner-close", HqMarket::Shanghai),
                context.sponsor(), FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1)),
                Duration::from_secs(60), move |control| {
                    started_tx.send(()).unwrap();
                    loop { control.check_active()?; thread::yield_now(); }
                });
            assert!(result.is_err());
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        owner.close().unwrap();
        worker.join().unwrap();
        assert!(observed_store.workers.lock().unwrap().is_empty());
        assert!(observed_store.inner.sealed.load(Ordering::Acquire));
    }

    #[test]
    fn last_owner_drop_stops_its_independently_owned_task_group() {
        #[derive(Debug)]
        struct Probe(Arc<AtomicBool>, Arc<AtomicBool>);
        impl crate::session::OwnerTaskGroup for Probe {
            fn stop(&self) { self.0.store(true, Ordering::Release); }
            fn close_until(&self, _: Instant) -> Result<()> { self.1.store(true, Ordering::Release); Ok(()) }
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let joined = Arc::new(AtomicBool::new(false));
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        let weak = Arc::downgrade(&owner);
        owner.register_task_group(Arc::new(Probe(Arc::clone(&stopped), Arc::clone(&joined)))).unwrap();
        drop(owner);
        assert!(weak.upgrade().is_none());
        assert!(stopped.load(Ordering::Acquire));
        assert!(joined.load(Ordering::Acquire));
    }

    #[test]
    fn cancelling_original_waiter_refunds_its_unstarted_attempt_and_survivor_finishes() {
        let token = CancellationToken::default();
        let original = operation(token.clone());
        let survivor = operation(CancellationToken::default());
        let store = Arc::new(MetadataStore::new(Arc::new(TestClock(AtomicI64::new(0)))));
        let key = SnapshotKey::hq("source-a", HqMarket::Shanghai);
        let policy = FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1));
        let (began_tx, began_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = mpsc::channel();
        let first_store = Arc::clone(&store);
        let first_key = key.clone();
        let first = thread::spawn(move || {
            let result = first_store.get_or_load(first_key, original.sponsor(), policy, Duration::from_secs(60), move |control| {
                let authorization = control.attempt_authorization();
                let reserved = authorization.reserve(2, None).unwrap();
                began_tx.send(()).unwrap();
                continue_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                reserved.start(Instant::now()).unwrap();
                authorization.check_boundary(Instant::now()).unwrap();
                Ok(candidate(control.key().clone(), 3))
            });
            (result, original.counters().unwrap())
        });
        began_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let second_store = Arc::clone(&store);
        let second_key = key.clone();
        let second = thread::spawn(move || {
            let result = second_store.get_or_load(second_key, survivor.sponsor(), policy, Duration::from_secs(60), |_| {
                panic!("same-key follower must not invoke its loader");
            });
            (result, survivor.counters().unwrap())
        });
        let join_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let joined = store.inner.state.lock().unwrap().flights.get(&key)
                .is_some_and(|flight| flight.waiters.lock().unwrap().len() == 2);
            if joined { break; }
            assert!(Instant::now() < join_deadline, "surviving waiter did not register");
            thread::yield_now();
        }
        token.cancel();
        let (first_result, first_counters) = first.join().unwrap();
        assert_eq!(first_result.unwrap_err().kind(), ErrorKind::Cancelled);
        continue_tx.send(()).unwrap();
        let (second_result, second_counters) = second.join().unwrap();
        assert!(second_result.is_ok());
        assert_eq!(first_counters.extra_started, 0);
        assert_eq!(second_counters.extra_started, 1);
        // The first reservation is refunded when replacement starts, after the first result.
        store.close_until(Instant::now() + Duration::from_secs(1)).unwrap();
    }
}
