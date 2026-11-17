use super::session::Session;
use super::transport::Work;
use super::{
    commands, config::ExConfig, frame::FrameLimits, hosts::DiagnosticSnapshot, models::*, query::*,
};
use crate::config::{ClientConfig, ConfigDraft};
use crate::operation::{ChildPurpose, OperationContext, WorkLimits};
use crate::session::SessionOwner;
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta, SourceStamp};
use crate::{Error, ErrorKind, OperationOptions, RawPolicy, Result, TimeoutPhase};
use bytes::Bytes;
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pure configuration assembly. No implicit environment/file reads, discovery,
/// runtime, resolver, or network work occurs until an explicit operation.
#[derive(Clone, Debug, Default)]
pub struct ExClientBuilder {
    config: ExConfig,
}
impl ExClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_config(mut self, config: ExConfig) -> Self {
        self.config = config;
        self
    }
    pub fn with_endpoints(mut self, endpoints: Vec<SocketAddr>) -> Self {
        self.config.endpoints = endpoints;
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }
    pub fn with_raw(mut self, raw: RawPolicy) -> Self {
        self.config.raw = raw;
        self
    }
    pub fn with_auto_failover(mut self, enabled: bool) -> Self {
        self.config.auto_failover = enabled;
        self
    }
    pub fn with_frame_limits(mut self, limits: FrameLimits) -> Self {
        self.config.frame_limits = limits;
        self
    }
    pub fn with_close_timeout(mut self, timeout: Duration) -> Self {
        self.config.close_timeout = timeout;
        self
    }
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.config.cleanup_timeout = timeout;
        self
    }
    pub fn with_wire_attempt_limit(mut self, maximum: NonZeroU32) -> Self {
        self.config.wire_attempt_limit = maximum;
        self
    }
    pub fn with_extra_attempt_budget(mut self, maximum: usize) -> Self {
        self.config.extra_attempt_budget = maximum;
        self
    }
    pub fn with_max_operation_bytes(mut self, maximum: NonZeroUsize) -> Self {
        self.config.max_operation_bytes = maximum;
        self
    }
    pub fn build(self) -> Result<ExClient> {
        let config = self
            .config
            .clone()
            .with_endpoints(self.config.endpoints.clone())?;
        for timeout in [
            config.timeout(),
            config.close_timeout(),
            config.cleanup_timeout(),
        ] {
            if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
                return Err(super::invalid(
                    "EX timeouts must be positive and fit the monotonic clock",
                ));
            }
        }
        let common = ConfigDraft {
            request_timeout: config.timeout(),
            close_timeout: config.close_timeout(),
            cleanup_timeout: config.cleanup_timeout(),
            heartbeat_interval: None,
            wire_attempt_limit: config.wire_attempt_limit(),
            extra_attempt_budget: config.extra_attempt_budget(),
            ..ConfigDraft::default()
        }
        .validate()?;
        let owner = Arc::new(SessionOwner::new(
            config.close_timeout(),
            config.cleanup_timeout(),
        ));
        let session = Session::new(config.endpoints().to_vec());
        Ok(ExClient {
            inner: Arc::new(Inner {
                config,
                common,
                owner,
                session,
            }),
        })
    }
}
#[derive(Debug)]
struct Inner {
    config: ExConfig,
    common: ClientConfig,
    owner: Arc<SessionOwner>,
    session: Arc<Session>,
}
/// Independent Rust EX entry. Clones retain the owner and its irreversible
/// close state. No Python interpreter, process global connection or HQ pool.
#[derive(Clone, Debug)]
pub struct ExClient {
    inner: Arc<Inner>,
}
struct Reply<T> {
    value: T,
    meta: ResultMeta,
    wire_count: Option<u16>,
    response_offset: Option<u32>,
}
impl ExClient {
    pub fn builder() -> ExClientBuilder {
        ExClientBuilder::new()
    }
    pub fn config(&self) -> &ExConfig {
        &self.inner.config
    }
    pub fn diagnostics(&self) -> Result<DiagnosticSnapshot> {
        self.inner.session.snapshot()
    }
    pub fn host(&self) -> Result<Option<SocketAddr>> {
        Ok(self.diagnostics()?.active_endpoint())
    }
    pub fn hosts(&self) -> Result<Vec<SocketAddr>> {
        Ok(self.diagnostics()?.candidates().to_vec())
    }
    pub fn unhealthy_hosts(&self) -> Result<Vec<SocketAddr>> {
        self.inner.session.unhealthy()
    }
    /// Only a successful explicit DataReady probe supplies health evidence.
    pub fn healthy_hosts(&self) -> Result<Vec<super::hosts::Endpoint>> {
        let snapshot = self.diagnostics()?;
        let excluded = self.unhealthy_hosts()?;
        Ok(snapshot
            .probes()
            .iter()
            .filter(|probe| {
                probe.data_ready()
                    && snapshot.candidates().contains(&probe.endpoint())
                    && !excluded.contains(&probe.endpoint())
            })
            .map(|probe| probe.endpoint())
            .collect())
    }
    pub fn close(&self) -> Result<()> {
        self.inner.owner.close()
    }
    pub fn disconnect(&self) -> Result<()> {
        self.close()
    }
    pub fn ensure_connected(&self, options: &OperationOptions) -> Result<()> {
        self.connect(options)
    }
    pub fn connect(&self, options: &OperationOptions) -> Result<()> {
        let context = self.context(Instant::now(), "ex.connect", options)?;
        self.connect_in_context(&context)?;
        context.finish()
    }
    fn connect_in_context(&self, context: &OperationContext) -> Result<()> {
        let lease = self.inner.session.lease(context)?;
        if self.inner.session.snapshot()?.active_endpoint().is_some() {
            context.check_boundary(TimeoutPhase::Connect)?;
            lease.accept();
            return Ok(());
        }
        let authorization = context
            .begin_child(ChildPurpose::Required, None)?
            .into_transport_authorization();
        super::worker::execute(
            Arc::clone(&self.inner.session),
            self.inner.config.clone(),
            Work::Connect,
            self.inner.config.wire_attempt_limit().get(),
            authorization,
            context,
            lease.worker_slot(),
            |_| Ok(()),
        )
        .map_err(|error| {
            context
                .annotate(error, TimeoutPhase::Connect, None)
                .with_context("domain", "ex")
        })?;
        context.check_boundary(TimeoutPhase::Connect)?;
        lease.accept();
        Ok(())
    }
    pub fn probe_endpoints(
        &self,
        endpoints: &[super::hosts::Endpoint],
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        let context = self.context(Instant::now(), "ex.probe_endpoints", options)?;
        let probes =
            self.probe_in_context(&context, endpoints.to_vec(), *discovery, false, false)?;
        context.finish()?;
        Ok(probes)
    }
    pub fn discover(
        &self,
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        let context = self.context(Instant::now(), "ex.discover", options)?;
        let probes = self.probe_in_context(&context, self.hosts()?, *discovery, false, true)?;
        context.finish()?;
        Ok(probes)
    }
    pub fn ping_all(
        &self,
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        self.discover(discovery, options)
    }
    pub fn refresh_hosts(
        &self,
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        self.discover(discovery, options)
    }
    pub fn refresh_endpoints(
        &self,
        endpoints: Vec<super::hosts::Endpoint>,
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        let context = self.context(Instant::now(), "ex.refresh_endpoints", options)?;
        let endpoints = self
            .inner
            .config
            .clone()
            .with_endpoints(endpoints)?
            .endpoints()
            .to_vec();
        let probes = self.probe_in_context(&context, endpoints, *discovery, true, true)?;
        context.finish()?;
        Ok(probes)
    }
    /// Discover and establish the selected connection under one operation's
    /// admission, total deadline, streamed-byte and physical-attempt budget.
    pub fn from_best_host(
        builder: ExClientBuilder,
        discovery: &super::hosts::DiscoveryOptions,
        options: &OperationOptions,
    ) -> Result<Self> {
        let entry = Instant::now();
        let client = builder.build()?;
        let context = client.context(entry, "ex.from_best_host", options)?;
        if discovery.probe().mode() != super::hosts::ProbeMode::DataReady {
            return Err(super::invalid(
                "EX best host selection requires a DataReady probe",
            ));
        }
        let probes = client.probe_in_context(&context, client.hosts()?, *discovery, false, true)?;
        if !probes.iter().any(super::hosts::HostProbe::data_ready) {
            return Err(Error::new(
                ErrorKind::DataNotReady,
                "no EX endpoint completed setup and markets",
            ));
        }
        client.connect_in_context(&context)?;
        context.finish()?;
        Ok(client)
    }
    fn probe_in_context(
        &self,
        context: &OperationContext,
        endpoints: Vec<super::hosts::Endpoint>,
        discovery: super::hosts::DiscoveryOptions,
        replace: bool,
        rank: bool,
    ) -> Result<Vec<super::hosts::HostProbe>> {
        if endpoints.is_empty() {
            context.check_boundary(TimeoutPhase::Admission)?;
            return Ok(Vec::new());
        }
        let endpoints = self
            .inner
            .config
            .clone()
            .with_endpoints(endpoints)?
            .endpoints()
            .to_vec();
        let lease = self.inner.session.lease(context)?;
        let requests = endpoints
            .iter()
            .map(|endpoint| {
                let authorization = context
                    .begin_child(ChildPurpose::Required, None)?
                    .into_transport_authorization();
                Ok((*endpoint, authorization))
            })
            .collect::<Result<Vec<_>>>()?;
        let config = self.inner.config.clone();
        let probes = super::worker::run(
            Arc::clone(&self.inner.session.stopped),
            context,
            lease.worker_slot(),
            move |control| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| {
                        Error::new(ErrorKind::Internal, "cannot start EX discovery runtime")
                            .with_source(error)
                    })?;
                let result =
                    runtime.block_on(super::probe::run(config, requests, discovery, control));
                // JoinSet tasks and every probe socket have been destroyed before
                // the registered job can announce completion to its caller.
                drop(runtime);
                result
            },
        )
        .map_err(|error| {
            context
                .annotate(error, TimeoutPhase::Response, None)
                .with_context("domain", "ex")
        })?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        self.inner
            .session
            .update_probes(probes.clone(), replace.then_some(endpoints), rank)?;
        lease.accept();
        Ok(probes)
    }
    pub fn markets(&self, options: &OperationOptions) -> Result<Vec<MarketInfo>> {
        let context = self.context(Instant::now(), "ex.markets", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::markets_request()),
            0x23f4,
            |body, _| commands::decode_markets(body),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn instrument_count(&self, options: &OperationOptions) -> Result<u32> {
        let context = self.context(Instant::now(), "ex.instrument_count", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::instrument_count_request()),
            0x23f0,
            |body, _| commands::decode_instrument_count(body),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn instruments(
        &self,
        query: &InstrumentsQuery,
        options: &OperationOptions,
    ) -> Result<PageResult<InstrumentInfo>> {
        let context = self.context(Instant::now(), "ex.instruments", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::instruments_request(query)),
            0x23f5,
            |body, _| commands::decode_instruments(body),
        )?;
        let start = reply.response_offset.unwrap_or(query.start());
        let page = Self::page(reply, start, query.count().get())?;
        context.finish()?;
        Ok(page)
    }
    pub fn quote(&self, query: &QuoteQuery, options: &OperationOptions) -> Result<Option<Quote>> {
        let context = self.context(Instant::now(), "ex.quote", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::quote_request(query)),
            0x23fa,
            commands::decode_quote,
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn quote_list(
        &self,
        query: &QuoteListQuery,
        options: &OperationOptions,
    ) -> Result<PageResult<QuoteListRow>> {
        let context = self.context(Instant::now(), "ex.quote_list", options)?;
        let category = query.category();
        let reply = self.exchange(
            &context,
            Work::Request(commands::quote_list_request(query)),
            0x2400,
            move |body, _| commands::decode_quote_list(body, category),
        )?;
        let page = Self::page(reply, u32::from(query.start()), query.count().get())?;
        context.finish()?;
        Ok(page)
    }
    pub fn bars(&self, query: &BarsQuery, options: &OperationOptions) -> Result<PageResult<Bar>> {
        let context = self.context(Instant::now(), "ex.bars", options)?;
        let decode_query = query.clone();
        let reply = self.exchange(
            &context,
            Work::Request(commands::bars_request(query)),
            0x23ff,
            move |body, raw| commands::decode_bars(body, &decode_query, raw),
        )?;
        let page = Self::page(reply, query.start(), query.count().get())?;
        context.finish()?;
        Ok(page)
    }
    pub fn bars_range(
        &self,
        query: &BarsRangeQuery,
        options: &OperationOptions,
    ) -> Result<Vec<Bar>> {
        let context = self.context(Instant::now(), "ex.bars_range", options)?;
        let reply = self.exchange(
            &context,
            Work::Range(query.clone()),
            0x240d,
            commands::decode_bars_range,
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn minutes(&self, query: &MinutesQuery, options: &OperationOptions) -> Result<Vec<Minute>> {
        let context = self.context(Instant::now(), "ex.minutes", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::minutes_request(query)),
            0x240b,
            commands::decode_minutes,
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn history_minutes(
        &self,
        query: &HistoryMinutesQuery,
        options: &OperationOptions,
    ) -> Result<Vec<Minute>> {
        let context = self.context(Instant::now(), "ex.history_minutes", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::history_minutes_request(query)),
            0x240c,
            commands::decode_history_minutes,
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn transactions(
        &self,
        query: &TransactionsQuery,
        options: &OperationOptions,
    ) -> Result<PageResult<Transaction>> {
        let context = self.context(Instant::now(), "ex.transactions", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::transactions_request(query)),
            0x23fc,
            commands::decode_transactions,
        )?;
        let page = Self::page(reply, query.start(), query.count().get())?;
        context.finish()?;
        Ok(page)
    }
    pub fn history_transactions(
        &self,
        query: &HistoryTransactionsQuery,
        options: &OperationOptions,
    ) -> Result<PageResult<Transaction>> {
        let context = self.context(Instant::now(), "ex.history_transactions", options)?;
        let reply = self.exchange(
            &context,
            Work::Request(commands::history_transactions_request(query)),
            0x2406,
            commands::decode_history_transactions,
        )?;
        let page = Self::page(reply, query.start(), query.count().get())?;
        context.finish()?;
        Ok(page)
    }
    pub(crate) fn context(
        &self,
        entry: Instant,
        name: &'static str,
        options: &OperationOptions,
    ) -> Result<OperationContext> {
        let options = options.inheriting_raw(self.inner.config.raw());
        let mut context = OperationContext::rust_public(
            entry,
            name,
            Arc::clone(&self.inner.owner),
            &self.inner.common,
            &options,
        )?;
        context.set_work_limits(WorkLimits {
            bytes: Some(self.inner.config.max_operation_bytes().get()),
            ..WorkLimits::default()
        })?;
        Ok(context)
    }
    fn exchange<T: Send + 'static>(
        &self,
        context: &OperationContext,
        work: Work,
        command: u16,
        decode: impl FnOnce(Bytes, RawPolicy) -> Result<Decoded<T>> + Send + 'static,
    ) -> Result<Reply<T>> {
        let lease = self.inner.session.lease(context)?;
        let authorization = context
            .begin_child(ChildPurpose::Required, None)?
            .into_transport_authorization();
        let raw = context.raw();
        let owner_generation = context.session_id;
        let maximum_bytes = self.inner.config.max_operation_bytes().get();
        let diagnostics = Arc::clone(&self.inner.session);
        let reply = super::worker::execute(
            Arc::clone(&self.inner.session),
            self.inner.config.clone(),
            work,
            self.inner.config.wire_attempt_limit().get(),
            authorization,
            context,
            lease.worker_slot(),
            move |reply| {
                let frame = reply.frame.ok_or_else(|| {
                    Error::new(ErrorKind::Internal, "EX business reply lacks a frame")
                })?;
                let mut decoded = decode(frame.payload().clone(), raw)?;
                if !frame.association_verified() {
                    decoded.diagnostics.push(Diagnostic::new(
                        "ex",
                        "association",
                        EnrichmentStatus::Unavailable,
                        None,
                        Some(
                            "production response magic/method/sequence rules lack capture evidence"
                                .to_owned(),
                        ),
                    ));
                }
                let retained = project_raw(
                    frame.payload().clone(),
                    Some(frame.wire().clone()),
                    &decoded.ranges,
                    raw,
                    maximum_bytes,
                )?;
                let wire_count = decoded.wire_row_count;
                let response_offset = decoded.response_offset;
                let source = SourceStamp::new(
                    Some(command),
                    Some(reply.endpoint),
                    format!("{owner_generation}:{}", reply.generation),
                    None,
                    None,
                );
                let (value, meta) = decoded.with_meta(source, retained);
                diagnostics.record_meta(meta.clone())?;
                Ok(Reply {
                    value,
                    meta,
                    wire_count,
                    response_offset,
                })
            },
        )
        .map_err(|error| {
            context
                .annotate(error, TimeoutPhase::Response, Some(command))
                .with_context("domain", "ex")
        })?;
        context.record_work(usize::from(reply.wire_count.unwrap_or(0)), 0)?;
        context.check_boundary(TimeoutPhase::Decode)?;
        lease.accept();
        Ok(reply)
    }
    fn page<T>(reply: Reply<Vec<T>>, start: u32, requested: u16) -> Result<PageResult<T>> {
        let consumed = reply
            .wire_count
            .ok_or_else(|| super::protocol("EX page lacks consumed wire count"))?;
        let next = start.checked_add(u32::from(consumed)).ok_or_else(|| {
            Error::new(ErrorKind::LimitExceeded, "EX next page offset exceeds u32")
        })?;
        let completion = if consumed >= requested {
            PageCompletion::RequestedCount
        } else {
            PageCompletion::Unknown
        };
        PageResult::new(reply.value, None, Some(next), completion, reply.meta)
    }
}
