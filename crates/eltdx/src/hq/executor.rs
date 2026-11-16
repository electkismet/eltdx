use std::sync::Arc;
use std::time::{Duration, Instant};

use eltdx_protocol::CommandRequest;
use eltdx_runtime::engine::{Engine, PendingPoll, PendingRawExecution, PinHandle, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::execution::RawReply;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, RecoveryLimit};
use crate::session::{PinnedSession, SessionOperation, SessionOwner};
use crate::metadata::{FlightPolicy, MetadataSnapshot, hq_market};
use crate::types::{HqMarket, SecurityId};

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

    pub(crate) fn acquire_pin(&self, engine: &Engine, context: &OperationContext) -> Result<PinnedSession> {
        self.check_context(context)?;
        context.check_boundary(TimeoutPhase::Admission)?;
        self.register_engine(engine.clone())?;
        let request = CommandRequest::Heartbeat(eltdx_protocol::commands::session::HeartbeatRequest);
        let permit = context.begin_child(ChildPurpose::Required, None)?;
        let command = request.command_code();
        let mut pending = engine.begin_pin_with_options(permit.into_submission(&request))
            .map_err(|runtime| context.runtime_error(runtime, command))?;
        loop {
            if let Err(root) = context.check_boundary(TimeoutPhase::Admission) {
                context.ensure_cleanup_budget()?;
                let cleanup = pending.cancel_and_confirm(context.cleanup_remaining()?).map_err(Error::from_runtime);
                return Err(Self::complete_retirement(context, root, command, cleanup));
            }
            match pending.wait_timeout(SIGNAL_POLL_INTERVAL) {
                Ok(PendingPoll::Pending) => {}
                Ok(PendingPoll::Ready(pin)) => {
                    let mut pin = PinnedSession::new(Arc::clone(&self.owner), pin)?;
                    if let Err(root) = context.check_boundary(TimeoutPhase::Admission) {
                        return match self.release_pin(context, &mut pin, Some(root)) {
                            Err(error) => Err(error),
                            Ok(()) => Err(Error::new(ErrorKind::Internal, "cancelled pin delivery lost its root cause")),
                        };
                    }
                    return Ok(pin);
                }
                Err(runtime) => return Err(context.runtime_error(runtime, command)),
            }
        }
    }

    /// Normal release consumes the operation's remaining time. After a stop,
    /// retirement uses the same separate cleanup budget as cancelled requests.
    pub(crate) fn release_pin(&self, context: &OperationContext, pin: &mut PinnedSession, mut root: Option<Error>) -> Result<()> {
        let mut pending = pin.handle()?.begin_close().map_err(Error::from_runtime)?;
        loop {
            if root.is_none() { root = context.check_boundary(TimeoutPhase::Close).err(); }
            if root.is_some() { context.ensure_cleanup_budget()?; }
            let wait = if root.is_some() { context.cleanup_remaining()?.min(SIGNAL_POLL_INTERVAL) } else { SIGNAL_POLL_INTERVAL };
            match pending.wait_timeout(wait) {
                Ok(PendingPoll::Ready(())) => {
                    pin.mark_released();
                    return root.map_or(Ok(()), Err);
                }
                Err(runtime) => {
                    let cleanup = Error::from_runtime(runtime);
                    return Err(match root {
                        Some(root) => Error::new(ErrorKind::CancelUnconfirmed, "temporary pin retirement failed")
                            .with_source(root).with_cleanup_failure(cleanup),
                        None => cleanup,
                    });
                }
                Ok(PendingPoll::Pending) if root.is_some() && context.cleanup_remaining()?.is_zero() => {
                    return Err(Error::new(ErrorKind::CancelUnconfirmed, "temporary pin retirement was not confirmed")
                        .with_source(root.take().expect("stop cause exists"))
                        .with_cleanup_failure(Error::new(ErrorKind::Timeout, "pin release cleanup budget elapsed")));
                }
                Ok(PendingPoll::Pending) => {}
            }
        }
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

    /// Resolve every required market for the whole operation before its first
    /// decode. The caller supplies the source/catalog namespace of this Engine.
    /// Passing all batch chunks here preserves one immutable capture across them.
    pub(crate) fn prepare_metadata(
        &self, engine: &Engine, context: &OperationContext, config: &ClientConfig,
        namespace: &str, requests: &[CommandRequest],
    ) -> Result<Arc<MetadataSnapshot>> {
        self.check_context(context)?;
        context.check_boundary(TimeoutPhase::Admission)?;
        if namespace.is_empty() {
            return Err(Error::new(ErrorKind::InvalidArgument, "metadata source namespace must not be empty"));
        }
        // Legacy adapters explicitly choose their historical preload scope and
        // request order; they do not enter normal Rust request inference.
        if context.is_legacy() {
            return Err(Error::new(ErrorKind::InvalidArgument, "legacy metadata must use its explicit compatibility preload"));
        }
        let markets = required_markets(requests);
        if let Some(snapshot) = context.metadata()? {
            if snapshot.key().namespace() != namespace {
                return Err(Error::new(ErrorKind::InvalidArgument, "operation metadata belongs to a different source"));
            }
            validate_request_precision(&snapshot, requests)?;
            return Ok(snapshot);
        }
        let mut snapshots = Vec::with_capacity(markets.len());
        if !markets.is_empty() { self.register_engine(engine.clone())?; }
        for market in markets {
            context.check_boundary(TimeoutPhase::Admission)?;
            let loaded = self.owner.metadata().load_hq_market(
                engine.clone(), namespace, market, context.sponsor(),
                FlightPolicy::rust_source_cap(config.request_timeout(), config.cleanup_timeout()),
                config.request_timeout(), Duration::from_secs(24 * 60 * 60), false,
            ).map_err(|error| context.annotate(error, TimeoutPhase::Decode, None))?;
            snapshots.push(loaded);
        }
        let snapshot = self.owner.metadata().capture(namespace, &snapshots)?;
        validate_request_precision(&snapshot, requests)?;
        context.install_metadata(Arc::clone(&snapshot))?;
        Ok(snapshot)
    }

    /// Compatibility callers pass exactly the historical selected market order.
    /// No Rust source total cap or market inference is imposed on that path.
    pub(crate) fn prepare_legacy_metadata(
        &self, engine: &Engine, context: &OperationContext, namespace: &str,
        markets: &[HqMarket], request_timeout: Duration, cleanup_timeout: Duration,
    ) -> Result<Arc<MetadataSnapshot>> {
        self.check_context(context)?;
        if !context.is_legacy() || namespace.is_empty() {
            return Err(Error::new(ErrorKind::InvalidArgument, "explicit legacy metadata policy required"));
        }
        if let Some(snapshot) = context.metadata()? {
            if snapshot.key().namespace() != namespace {
                return Err(Error::new(ErrorKind::InvalidArgument, "operation metadata belongs to a different source"));
            }
            return Ok(snapshot);
        }
        let mut snapshots = Vec::with_capacity(markets.len());
        if !markets.is_empty() { self.register_engine(engine.clone())?; }
        for &market in markets {
            context.check_boundary(TimeoutPhase::Admission)?;
            snapshots.push(self.owner.metadata().load_hq_market(
                engine.clone(), namespace, market, context.sponsor(),
                FlightPolicy::legacy_per_request(cleanup_timeout), request_timeout,
                Duration::from_secs(24 * 60 * 60), false,
            )?);
        }
        let snapshot = self.owner.metadata().capture(namespace, &snapshots)?;
        context.install_metadata(Arc::clone(&snapshot))?;
        Ok(snapshot)
    }

    pub(crate) fn execute_decoded(
        &self, engine: &Engine, context: &OperationContext, config: &ClientConfig,
        namespace: &str, request: CommandRequest, purpose: ChildPurpose,
        recovery: Option<RecoveryLimit>,
    ) -> Result<super::decode::DecodedReply> {
        self.prepare_metadata(engine, context, config, namespace, std::slice::from_ref(&request))?;
        let raw = self.execute_raw(engine, context, request, purpose, recovery)?;
        self.decode_received(context, raw)
    }

    pub(crate) fn decode_received(
        &self, context: &OperationContext, raw: RawReply,
    ) -> Result<super::decode::DecodedReply> {
        self.check_context(context)?;
        let command = raw.request.command_code();
        let snapshot = context.begin_decode()?;
        if !context.is_legacy() { validate_request_precision(&snapshot, std::slice::from_ref(&raw.request))?; }
        let decoded = super::decode::decode_reply_profile(raw, context.raw(), snapshot.as_ref(), context.is_legacy()).map_err(|error| {
            let kind = match &error {
                eltdx_protocol::ProtocolError::InvalidData { context: "price precision", .. } => ErrorKind::MetadataUnavailable,
                _ => ErrorKind::Protocol,
            };
            context.annotate(Error::new(kind, "HQ response decoding failed").with_source(error), TimeoutPhase::Decode, Some(command))
        })?;
        context.check_boundary(TimeoutPhase::Decode)?;
        Ok(decoded)
    }

    /// Directory rows carry their own decimals. Decoding them must not freeze
    /// an empty operation snapshot before later quote precision is prepared.
    pub(crate) fn decode_unpriced(
        &self, context: &OperationContext, raw: RawReply,
    ) -> Result<super::decode::DecodedReply> {
        struct NoPrecision;
        impl eltdx_protocol::PricePrecisionSource for NoPrecision {
            fn security_decimal(&self, _: &eltdx_protocol::unit::NormalizedCode) -> Option<u8> { None }
        }
        self.check_context(context)?;
        context.check_boundary(TimeoutPhase::Decode)?;
        if !matches!(&raw.request, CommandRequest::SecurityList(_) | CommandRequest::SecurityCount(_)
            | CommandRequest::Heartbeat(_) | CommandRequest::Handshake(_)) {
            return Err(Error::new(ErrorKind::InvalidArgument, "unpriced decoder requires an unpriced command"));
        }
        let command = raw.request.command_code();
        let decoded = super::decode::decode_reply(raw, context.raw(), &NoPrecision)
            .map_err(|error| context.annotate(Error::from_protocol(error), TimeoutPhase::Decode, Some(command)))?;
        context.check_boundary(TimeoutPhase::Decode)?;
        Ok(decoded)
    }

    fn check_context(&self, context: &OperationContext) -> Result<()> {
        if !Arc::ptr_eq(context.owner(), &self.owner) {
            return Err(Error::new(ErrorKind::InvalidArgument, "operation belongs to a different session"));
        }
        Ok(())
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
        let root = context.stop(context.annotate(root, TimeoutPhase::Response, Some(command)));
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
        if result.is_err() { let _ = pending.quarantine(); }
        Self::complete_retirement(context, root, command, result)
    }

    fn complete_retirement(context: &OperationContext, root: Error, command: u16, result: Result<()>) -> Error {
        match result {
            Ok(()) => root,
            Err(cleanup) => {
                context.annotate(Error::new(ErrorKind::CancelUnconfirmed, "operation stopped but retirement was not confirmed")
                    .with_context("root_kind", format!("{:?}", root.kind()))
                    .with_source(root).with_cleanup_failure(cleanup),
                TimeoutPhase::CancelConfirm, Some(command),
                )
            }
        }
    }
}

fn requested_securities(request: &CommandRequest) -> Vec<&eltdx_protocol::unit::NormalizedCode> {
    match request {
        CommandRequest::Snapshots(request) => request.codes.iter().collect(),
        CommandRequest::LegacyQuotes(request) => request.codes.iter().collect(),
        CommandRequest::RefreshStream(request) => request.items.iter().map(|item| &item.code).collect(),
        CommandRequest::TodayIntraday(request) => vec![&request.code],
        CommandRequest::HistoricalIntraday(request) => vec![&request.code],
        CommandRequest::RecentIntraday(request) => vec![&request.code],
        _ => Vec::new(),
    }
}

fn required_markets(requests: &[CommandRequest]) -> Vec<HqMarket> {
    let mut markets = Vec::new();
    for request in requests {
        // Category pages identify securities only in the response. Capture all
        // possible HQ markets before decoding any row of such a page.
        if matches!(request, CommandRequest::CategoryQuotes(_)) {
            return vec![HqMarket::Shenzhen, HqMarket::Shanghai, HqMarket::Beijing];
        }
        for code in requested_securities(request) {
            let market = hq_market(code.market());
            if !markets.contains(&market) { markets.push(market); }
        }
    }
    markets
}

fn validate_request_precision(snapshot: &MetadataSnapshot, requests: &[CommandRequest]) -> Result<()> {
    for market in required_markets(requests) {
        if !snapshot.covers_market(market) {
            return Err(Error::new(ErrorKind::MetadataUnavailable, "operation snapshot does not contain the required market")
                .with_context("market", market.code()));
        }
    }
    for request in requests {
        for code in requested_securities(request) {
            snapshot.require_precision(&SecurityId::new(hq_market(code.market()), code.number())?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod operation_retirement_tests {
    use std::error::Error as _;
    use std::sync::Arc;
    use std::time::Duration;

    use super::HqExecutor;
    use crate::client::ClientBuilder;
    use crate::error::{Error, ErrorKind, TimeoutPhase};
    use crate::operation::OperationOptions;
    use crate::session::SessionOwner;

    #[test]
    fn irrelevant_command_preparation_leaves_engine_unstarted_and_snapshot_empty() {
        use eltdx_protocol::commands::security::SecurityCountRequest;
        use eltdx_protocol::unit::Market;
        use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions};
        let config = ClientBuilder::new().build().unwrap().config().clone();
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        let executor = HqExecutor::new(Arc::clone(&owner));
        let context = executor.begin_context("no-prefetch", &config, &OperationOptions::default()).unwrap();
        // Port 1 is deliberately never used; prepare only inspects request shape.
        let engine = Engine::new(EngineConfig::new(vec!["127.0.0.1:1".into()], 1.0, EngineConfigOptions {
            pool_size: 1, runtime_workers: Some(1), server_count: 1,
            max_connections_per_host: None, connect_concurrency: None, connect_concurrency_per_host: None,
            global_raw_bytes: None, global_decoded_bytes: None, heartbeat_interval: None,
            max_pending_requests: 1, push_queue_size: 1, push_queue_bytes: 1024,
        }).unwrap()).unwrap();
        let request = eltdx_protocol::CommandRequest::SecurityCount(SecurityCountRequest { market: Market::Shanghai, client_date: 0 });
        let snapshot = executor.prepare_metadata(&engine, &context, &config, "no-network", &[request]).unwrap();
        assert_eq!(snapshot.version(), 0);
        assert_eq!(context.counters().unwrap().logical_requests, 0);
        assert!(!snapshot.covers_market(crate::types::HqMarket::Shanghai));
        drop(context);
        owner.close().unwrap();
    }

    #[test]
    fn strict_request_identity_selects_only_its_market() {
        use eltdx_protocol::commands::quotes::SnapshotsRequest;
        use eltdx_protocol::unit::NormalizedCode;
        let request = eltdx_protocol::CommandRequest::Snapshots(SnapshotsRequest {
            codes: vec![NormalizedCode::parse("sh510300").unwrap()],
        });
        assert_eq!(super::required_markets(&[request]), vec![crate::types::HqMarket::Shanghai]);
    }

    #[test]
    fn confirmed_retirement_returns_first_annotated_stop_cause_with_runtime_source() {
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        let config = ClientBuilder::new().build().expect("lazy config").config().clone();
        let context = HqExecutor::new(Arc::clone(&owner))
            .begin_context("cancelled_lookup", &config, &OperationOptions::default())
            .expect("operation");
        let runtime = eltdx_runtime::RuntimeError::timeout(eltdx_runtime::error::TimeoutPhase::Response)
            .with_context("attempt", "2").with_context("host", "fixture:7709");
        let cause = context.annotate(Error::from_runtime(runtime.clone()), TimeoutPhase::Response,
            Some(0x044e));
        let cause = context.stop(cause);
        let returned = HqExecutor::complete_retirement(&context, cause, 0x044e, Ok(()));
        assert_eq!(returned.kind(), ErrorKind::Timeout);
        assert_eq!(returned.source().and_then(|source| source.downcast_ref::<eltdx_runtime::RuntimeError>()), Some(&runtime));
        for key in ["operation", "operation_id", "session_id", "phase", "command", "attempt", "host"] {
            assert!(returned.context().iter().any(|(actual, _)| actual == key), "missing {key}");
        }
        assert_eq!(returned.context().iter().find(|(key, _)| key == "command").map(|(_, value)| value.as_str()), Some("0x044e"));
        owner.close().expect("no runtime started");
    }
}
