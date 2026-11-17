use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::session::HeartbeatRequest;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions};

use crate::config::{ClientConfig, ConfigDraft};
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationOptions};
use crate::services::{
    AuctionService, CodeService, KlineService, MinuteService, MoneyFlowService, QuoteService,
    TradeService,
};
use crate::session::SessionOwner;

#[derive(Clone, Debug)]
pub struct ClientBuilder {
    draft: ConfigDraft,
    #[cfg(feature = "f10")]
    f10: Option<crate::f10::F10ClientBuilder>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self {
            draft: ConfigDraft::default(),
            #[cfg(feature = "f10")]
            f10: None,
        }
    }

    pub fn pool_size(mut self, count: usize) -> Self {
        self.draft.pool_size = count;
        self
    }

    /// Supplies an explicit UTC-epoch clock for current Shanghai dates and freshness.
    pub fn clock(mut self, clock: Arc<dyn crate::types::Clock>) -> Self {
        self.draft.clock = crate::types::time::ClockHandle(clock);
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.draft.request_timeout = timeout;
        self
    }

    pub fn close_timeout(mut self, timeout: Duration) -> Self {
        self.draft.close_timeout = timeout;
        self
    }

    pub fn cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.draft.cleanup_timeout = timeout;
        self
    }

    pub fn heartbeat_interval(mut self, interval: Option<Duration>) -> Self {
        self.draft.heartbeat_interval = interval;
        self
    }

    pub fn endpoints(mut self, endpoints: Vec<SocketAddr>) -> Self {
        self.draft.endpoints = Some(endpoints);
        self
    }

    pub fn auction_endpoints(mut self, endpoints: Vec<SocketAddr>) -> Self {
        self.draft.auction_endpoints = Some(endpoints);
        self
    }
    pub fn money_flow_endpoints(mut self, endpoints: Vec<SocketAddr>) -> Self {
        self.draft.money_flow_endpoints = Some(endpoints);
        self
    }
    /// Physical connection cap across ordinary, dedicated and continuation pools.
    /// Defaults to four times pool_size, allowing three domains and a probe pool.
    pub fn max_connections(mut self, maximum: usize) -> Self {
        self.draft.max_connections = Some(maximum);
        self
    }

    pub fn wire_attempt_limit(mut self, attempts: NonZeroU32) -> Self {
        self.draft.wire_attempt_limit = attempts;
        self
    }

    pub fn extra_attempt_budget(mut self, attempts: usize) -> Self {
        self.draft.extra_attempt_budget = attempts;
        self
    }

    /// Configures HTTP routing without resolving DNS or starting HQ/F10 I/O.
    /// The resulting domain shares this client's close and operation owner.
    #[cfg(feature = "f10")]
    pub fn f10(mut self, builder: crate::f10::F10ClientBuilder) -> Self {
        self.f10 = Some(builder);
        self
    }

    pub fn build(self) -> Result<TdxClient> {
        let config = self.draft.validate()?;
        let hosts = configured_hosts(&config);
        let namespace = format!("hq:7709:{}", hosts.join(","));
        let engine = engine_with_hosts(&config, hosts)?;
        let owner = Arc::new(SessionOwner::new_with_clock(
            config.close_timeout,
            config.cleanup_timeout,
            config.clock.clone(),
        ));
        #[cfg(feature = "f10")]
        let f10 = self
            .f10
            .unwrap_or_else(|| {
                crate::f10::F10Client::builder()
                    .with_timeout(config.request_timeout())
                    .with_close_timeout(config.close_timeout())
                    .with_cleanup_timeout(config.cleanup_timeout())
            })
            .build_with_owner(Arc::clone(&owner))?;
        Ok(TdxClient {
            config,
            hq: HqExecutor::new(owner),
            engine,
            namespace,
            domains: Arc::new(crate::hq::domains::DomainPools::default()),
            #[cfg(feature = "f10")]
            f10,
        })
    }
}

pub(crate) fn engine_with_hosts(config: &ClientConfig, hosts: Vec<String>) -> Result<Engine> {
    let engine_config = EngineConfig::new(
        hosts,
        config.request_timeout().as_secs_f64(),
        EngineConfigOptions {
            pool_size: config.pool_size(),
            runtime_workers: None,
            server_count: 1,
            max_connections_per_host: None,
            connect_concurrency: Some(1),
            connect_concurrency_per_host: Some(1),
            global_raw_bytes: None,
            global_decoded_bytes: None,
            heartbeat_interval: config.heartbeat_interval().map(|value| value.as_secs_f64()),
            max_pending_requests: 256,
            push_queue_size: 1_024,
            push_queue_bytes: 64 * 1024 * 1024,
        },
    )
    .map_err(Error::from_runtime)?;
    Engine::new(engine_config.with_connection_limit(config.connection_limit.clone()))
        .map_err(Error::from_runtime)
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct TdxClient {
    config: ClientConfig,
    hq: HqExecutor,
    engine: Engine,
    namespace: String,
    domains: Arc<crate::hq::domains::DomainPools>,
    #[cfg(feature = "f10")]
    f10: crate::f10::F10Client,
}

impl TdxClient {
    #[cfg(feature = "offline")]
    pub fn financial(&self) -> crate::financial::FinancialService {
        crate::financial::FinancialService::new(self.resources())
    }
    pub fn helpers(&self) -> crate::helpers::HelperService {
        crate::helpers::HelperService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
            self.domains.clone(),
            #[cfg(feature = "f10")]
            self.f10.clone(),
        )
    }
    /// HTTP domain of this client. It shares the same irreversible close;
    /// use F10Client::builder for a separately owned HTTP-only client.
    #[cfg(feature = "f10")]
    pub fn f10(&self) -> crate::f10::F10Client {
        self.f10.clone()
    }
    pub fn resources(&self) -> crate::services::ResourceService {
        crate::services::ResourceService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
    }
    pub fn company_info(&self) -> crate::services::CompanyInfoService {
        crate::services::CompanyInfoService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
    }
    pub fn session(&self) -> crate::services::SessionService {
        crate::services::SessionService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
    }
    pub fn health(&self) -> crate::services::HealthService {
        crate::services::HealthService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
    }
    pub fn corporate(&self) -> crate::services::CorporateService {
        crate::services::CorporateService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }
    pub fn limits(&self) -> crate::services::LimitService {
        crate::services::LimitService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
    }
    pub fn calendar(&self) -> crate::calendar::CalendarService {
        crate::calendar::CalendarService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Starts the lazy HQ runtime and confirms the protocol handshake with a heartbeat.
    pub fn connect(&self, options: &OperationOptions) -> Result<()> {
        let entry = Instant::now();
        let context = self
            .hq
            .begin_context_at(entry, "connect", &self.config, options)?;
        let request = CommandRequest::Heartbeat(HeartbeatRequest);
        let raw = self.hq.execute_raw(
            &self.engine,
            &context,
            request,
            ChildPurpose::Required,
            None,
        )?;
        match self.hq.decode_received(&context, raw)?.response {
            CommandResponse::Heartbeat(_) => {
                context.finish()?;
                Ok(())
            }
            _ => Err(context.annotate(
                Error::new(
                    ErrorKind::Protocol,
                    "HQ connection returned a non-heartbeat response",
                ),
                TimeoutPhase::Decode,
                None,
            )),
        }
    }

    pub fn codes(&self) -> CodeService {
        CodeService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }

    /// Reserves a fixed connection. The returned quote service borrows this
    /// unique guard and executes metadata and refresh on that same connection.
    pub fn pin(&self, options: &OperationOptions) -> Result<crate::PinnedSession> {
        let entry = Instant::now();
        let context = self
            .hq
            .begin_context_at(entry, "pin", &self.config, options)?;
        let mut pin = self.hq.acquire_pin(&self.engine, &context)?;
        pin.bind_quotes(
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        );
        if let Err(root) = context.finish() {
            return match self.hq.release_pin(&context, &mut pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(
                    ErrorKind::Internal,
                    "pin delivery lost its stop cause",
                )),
            };
        }
        Ok(pin)
    }

    pub fn quotes(&self) -> QuoteService {
        QuoteService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }

    pub fn klines(&self) -> KlineService {
        KlineService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }

    pub fn minutes(&self) -> MinuteService {
        MinuteService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }

    pub fn trades(&self) -> TradeService {
        TradeService::new(
            self.hq.clone(),
            self.engine.clone(),
            self.config.clone(),
            self.namespace.clone(),
        )
    }

    pub fn auctions(&self) -> AuctionService {
        AuctionService::new(self.hq.clone(), self.domains.clone(), self.config.clone())
    }

    pub fn money_flow(&self) -> MoneyFlowService {
        MoneyFlowService::new(self.hq.clone(), self.domains.clone(), self.config.clone())
    }

    /// Permanently closes this shared, lazily initialized client owner.
    pub fn close(&self) -> Result<()> {
        self.hq.close()
    }
}

pub(crate) fn configured_hosts(config: &ClientConfig) -> Vec<String> {
    config
        .endpoints()
        .map(|endpoints| endpoints.iter().map(ToString::to_string).collect())
        .unwrap_or_else(|| {
            crate::hosts::DEFAULT_7709_HOSTS
                .iter()
                .map(|host| (*host).to_owned())
                .collect()
        })
}

#[cfg(test)]
#[path = "stress_internal.rs"]
mod stress_internal;
