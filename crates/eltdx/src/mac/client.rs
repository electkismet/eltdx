//! Independent MAC owners with a shared, bounded extension transport.
//! Configuration assembly is local; only explicit operations start workers.
use super::config::{MacConfig, MacExConfig};
use super::handshake::{LoginReplyRules, MacSetupPolicy};
use super::profile::{MacExProfile, MacProfile};
use super::session::{self, Domain, MacOwner};
use super::{commands, decode, query::*, responses::*, WireFloat};
use crate::ex::frame::FrameLimits;
use crate::ex::hosts::{DiagnosticSnapshot, DiscoveryOptions, Endpoint, HostProbe};
use crate::types::{Diagnostic, EnrichmentStatus};
use crate::{ErrorKind, OperationOptions, RawPolicy, Result, TimeoutPhase};
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct MacClientBuilder {
    config: MacConfig,
}
#[derive(Clone, Debug, Default)]
pub struct MacExClientBuilder {
    config: MacExConfig,
}

macro_rules! builder_config {
    ($builder:ident, $config:ident) => {
        impl $builder {
            pub fn new() -> Self {
                Self::default()
            }
            pub fn with_config(mut self, value: $config) -> Self {
                self.config = value;
                self
            }
            pub fn with_endpoints(mut self, value: Vec<Endpoint>) -> Self {
                self.config.transport.endpoints = value;
                self
            }
            pub fn with_timeout(mut self, value: Duration) -> Self {
                self.config.transport.timeout = value;
                self
            }
            pub fn with_raw(mut self, value: RawPolicy) -> Self {
                self.config.transport.raw = value;
                self
            }
            pub fn with_auto_failover(mut self, value: bool) -> Self {
                self.config.transport.auto_failover = value;
                self
            }
            pub fn with_frame_limits(mut self, value: FrameLimits) -> Self {
                self.config.transport.frame_limits = value;
                self
            }
            pub fn with_close_timeout(mut self, value: Duration) -> Self {
                self.config.transport.close_timeout = value;
                self
            }
            pub fn with_cleanup_timeout(mut self, value: Duration) -> Self {
                self.config.transport.cleanup_timeout = value;
                self
            }
            pub fn with_wire_attempt_limit(mut self, value: NonZeroU32) -> Self {
                self.config.transport.wire_attempt_limit = value;
                self
            }
            pub fn with_extra_attempt_budget(mut self, value: usize) -> Self {
                self.config.transport.extra_attempt_budget = value;
                self
            }
            pub fn with_max_operation_bytes(mut self, value: NonZeroUsize) -> Self {
                self.config.transport.max_operation_bytes = value;
                self
            }
            pub fn with_xdxr_provider(mut self, value: Arc<dyn super::XdxrProvider>) -> Self {
                self.config.xdxr_provider = Some(value);
                self
            }
            pub fn without_xdxr_provider(mut self) -> Self {
                self.config.xdxr_provider = None;
                self
            }
        }
    };
}
builder_config!(MacClientBuilder, MacConfig);
builder_config!(MacExClientBuilder, MacExConfig);
impl MacClientBuilder {
    pub fn with_setup_policy(mut self, value: MacSetupPolicy) -> Self {
        self.config.setup = value;
        self
    }
    pub fn build(self) -> Result<MacClient> {
        let config = self.config.validate()?;
        let profile = Arc::new(MacProfile {
            policy: config.setup.clone(),
        });
        let inner = MacOwner::new(config.transport.clone(), profile, Domain::Mac)?;
        Ok(MacClient { inner, config })
    }
}
impl MacExClientBuilder {
    pub fn with_login_reply_rules(mut self, value: LoginReplyRules) -> Self {
        self.config.login = Some(value);
        self
    }
    pub fn build(self) -> Result<MacExClient> {
        let config = self.config.validate()?;
        let profile = Arc::new(MacExProfile {
            login: config.login.clone(),
        });
        let inner = MacOwner::new(config.transport.clone(), profile, Domain::MacEx)?;
        Ok(MacExClient { inner, config })
    }
}

/// Clones share this owner's irreversible close state. A separately built
/// client has a separate owner, generation, socket and setup policy.
#[derive(Clone, Debug)]
pub struct MacClient {
    pub(crate) inner: Arc<MacOwner>,
    config: MacConfig,
}
/// MAC-EX authenticates every new connection with its own login profile,
/// including requests that subsequently use ordinary EX command encoding.
/// Production login reply evidence must be configured before network use.
#[derive(Clone, Debug)]
pub struct MacExClient {
    pub(crate) inner: Arc<MacOwner>,
    config: MacExConfig,
}

macro_rules! management {
    ($client:ident, $builder:ident, $config:ident, $prefix:literal) => {
        impl $client {
            pub fn builder() -> $builder {
                $builder::new()
            }
            pub fn config(&self) -> &$config {
                &self.config
            }
            pub fn diagnostics(&self) -> Result<DiagnosticSnapshot> {
                self.inner.diagnostics()
            }
            pub fn host(&self) -> Result<Option<Endpoint>> {
                Ok(self.diagnostics()?.active_endpoint())
            }
            pub fn hosts(&self) -> Result<Vec<Endpoint>> {
                Ok(self.diagnostics()?.candidates().to_vec())
            }
            pub fn healthy_hosts(&self) -> Result<Vec<Endpoint>> {
                self.inner.healthy_hosts()
            }
            pub fn unhealthy_hosts(&self) -> Result<Vec<Endpoint>> {
                self.inner.session.unhealthy()
            }
            pub fn close(&self) -> Result<()> {
                self.inner.close()
            }
            pub fn disconnect(&self) -> Result<()> {
                self.close()
            }
            pub fn connect(&self, options: &OperationOptions) -> Result<()> {
                let context =
                    self.inner
                        .context(Instant::now(), concat!($prefix, ".connect"), options)?;
                self.inner.connect(&context)?;
                context.finish()
            }
            pub fn ensure_connected(&self, options: &OperationOptions) -> Result<()> {
                self.connect(options)
            }
            pub fn probe_endpoints(
                &self,
                endpoints: &[Endpoint],
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Vec<HostProbe>> {
                let context = self.inner.context(
                    Instant::now(),
                    concat!($prefix, ".probe_endpoints"),
                    options,
                )?;
                let probes =
                    self.inner
                        .probe(&context, endpoints.to_vec(), *discovery, false, false)?;
                context.finish()?;
                Ok(probes)
            }
            pub fn discover(
                &self,
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Vec<HostProbe>> {
                let context =
                    self.inner
                        .context(Instant::now(), concat!($prefix, ".discover"), options)?;
                let probes = self
                    .inner
                    .probe(&context, self.hosts()?, *discovery, false, true)?;
                context.finish()?;
                Ok(probes)
            }
            pub fn ping_all(
                &self,
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Vec<HostProbe>> {
                self.discover(discovery, options)
            }
            pub fn refresh_hosts(
                &self,
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Vec<HostProbe>> {
                self.discover(discovery, options)
            }
            pub fn refresh_endpoints(
                &self,
                endpoints: Vec<Endpoint>,
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Vec<HostProbe>> {
                let context = self.inner.context(
                    Instant::now(),
                    concat!($prefix, ".refresh_endpoints"),
                    options,
                )?;
                let endpoints = self
                    .config
                    .transport
                    .clone()
                    .with_endpoints(endpoints)?
                    .endpoints()
                    .to_vec();
                let probes = self
                    .inner
                    .probe(&context, endpoints, *discovery, true, true)?;
                context.finish()?;
                Ok(probes)
            }
            /// Discovery and connect share one public deadline and attempt/byte
            /// sponsor. DataReady proves only the profile's documented probe.
            pub fn from_best_host(
                builder: $builder,
                discovery: &DiscoveryOptions,
                options: &OperationOptions,
            ) -> Result<Self> {
                let entry = Instant::now();
                let client = builder.build()?;
                let context =
                    client
                        .inner
                        .context(entry, concat!($prefix, ".from_best_host"), options)?;
                client.inner.best(&context, *discovery)?;
                context.finish()?;
                Ok(client)
            }
        }
    };
}
management!(MacClient, MacClientBuilder, MacConfig, "mac");
management!(MacExClient, MacExClientBuilder, MacExConfig, "mac-ex");

impl MacClient {
    pub fn download_file(
        &self,
        query: &DownloadFileQuery,
        options: &OperationOptions,
    ) -> Result<crate::types::DownloadResult> {
        self.download_entry(query, options, false)
    }
    /// Returns a complete file only after an observed EOF and consistent size.
    /// The starting offset must be zero; download_file supports resumed suffixes.
    pub fn download_complete(
        &self,
        query: &DownloadFileQuery,
        options: &OperationOptions,
    ) -> Result<bytes::Bytes> {
        self.download_entry(query, options, true)
            .map(|result| result.data)
    }
    fn download_entry(
        &self,
        query: &DownloadFileQuery,
        options: &OperationOptions,
        strict: bool,
    ) -> Result<crate::types::DownloadResult> {
        let context = self.inner.context(
            Instant::now(),
            if strict {
                "mac.download_complete"
            } else {
                "mac.download_file"
            },
            options,
        )?;
        if strict && query.options().start_offset() != 0 {
            return Err(context.annotate(
                super::query::invalid("MAC complete-file download requires starting offset zero"),
                TimeoutPhase::Admission,
                None,
            ));
        }
        let result = super::download::file(&self.inner, &context, query).map_err(|error| {
            context
                .annotate(error, TimeoutPhase::Aggregate, None)
                .with_context("domain", "mac")
        })?;
        if strict && result.completion != crate::types::DownloadCompletion::Complete {
            return Err(context.annotate(
                crate::Error::new(
                    ErrorKind::LimitExceeded,
                    "MAC file download reached a limit before EOF",
                )
                .with_context("reason", "LimitReached")
                .with_context(
                    "next_offset",
                    format!(
                        "{}",
                        result.next_offset.unwrap_or(query.options().start_offset())
                    ),
                ),
                TimeoutPhase::Aggregate,
                None,
            ));
        }
        context.finish()?;
        Ok(result)
    }
    /// Every member page contributes to one total deadline and shared limits.
    pub fn board_summary(
        &self,
        query: &BoardSummaryQuery,
        options: &OperationOptions,
    ) -> Result<super::BoardSummary> {
        let mut context = self
            .inner
            .context(Instant::now(), "mac.board_summary", options)?;
        board_limits(
            &mut context,
            query.pages(),
            self.config.transport.max_operation_bytes().get(),
        )?;
        let result = super::boards::summary(&self.inner, &context, query)?;
        context.finish()?;
        Ok(result)
    }
    /// Money/volume keys aggregate all candidates before a stable top-N. None
    /// retains the full bounded ranking. Missing reference values sort last.
    pub fn board_ranking(
        &self,
        query: &BoardRankingQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::BoardRank>> {
        let mut context = self
            .inner
            .context(Instant::now(), "mac.board_ranking", options)?;
        board_limits(
            &mut context,
            query.pages(),
            self.config.transport.max_operation_bytes().get(),
        )?;
        let result = super::boards::ranking(&self.inner, &context, query)?;
        context.finish()?;
        Ok(result)
    }
    /// Fetches each board's required history on this same operation. Missing
    /// intervals are diagnosed; required I/O, cancellation and limits propagate.
    pub fn board_change_ranking(
        &self,
        query: &BoardChangeRankingQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::BoardChangeRank>> {
        let mut context =
            self.inner
                .context(Instant::now(), "mac.board_change_ranking", options)?;
        board_limits(
            &mut context,
            query.pages(),
            self.config.transport.max_operation_bytes().get(),
        )?;
        let result = super::boards::change_ranking(&self.inner, &context, query)?;
        context.finish()?;
        Ok(result)
    }
    /// Collects a checked target, retaining response order and duplicate rows.
    /// Board categories use board aliases; constituent lists use instrument
    /// meanings even though both commands share the same bitmap layout.
    pub fn quote_list(
        &self,
        query: &QuoteListQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<super::Quote>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.quote_list", options)?;
        let code = u32::from(query.category().wire());
        let field_context = if (10000..=10010).contains(&code) {
            super::models::FieldContext::Board
        } else {
            super::models::FieldContext::Instrument
        };
        let page = super::collect::quote_list(
            &self.inner,
            &context,
            super::collect::ListPlan {
                code,
                start: query.start(),
                target: super::pagination::Target::Count(query.count()),
                fields: query.fields(),
                sort: query.sort(),
                order: query.order(),
                exclude: query.exclude(),
                pages: query.pages().clone(),
                field_context,
            },
        )?
        .page()?;
        context.finish()?;
        Ok(page)
    }
    pub fn board_members(
        &self,
        query: &BoardMembersQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<super::Quote>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.board_members", options)?;
        let page = super::collect::quote_list(
            &self.inner,
            &context,
            super::collect::ListPlan {
                code: query.board().wire_code(),
                start: query.start(),
                target: super::pagination::Target::Count(query.count()),
                fields: query.fields(),
                sort: query.sort(),
                order: query.order(),
                exclude: query.exclude(),
                pages: query.pages().clone(),
                field_context: super::models::FieldContext::Instrument,
            },
        )?
        .page()?;
        context.finish()?;
        Ok(page)
    }
    /// Each response is ordered oldest to newest; older pages are prepended
    /// intact. Unknown native units and all seven source float bits survive.
    pub fn bars(&self, query: &BarsQuery, options: &OperationOptions) -> Result<Vec<super::Bar>> {
        let context = self.inner.context(Instant::now(), "mac.bars", options)?;
        let result = super::services::bars(
            &self.inner,
            &context,
            query,
            self.config.xdxr_provider.clone(),
        )?;
        context.finish()?;
        Ok(result.rows)
    }
    pub fn transactions(
        &self,
        query: &TransactionsQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<super::Transaction>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.transactions", options)?;
        let page = super::collect::transactions(&self.inner, &context, query)?.page()?;
        context.finish()?;
        Ok(page)
    }
    pub fn boards(
        &self,
        query: &BoardsQuery,
        options: &OperationOptions,
    ) -> Result<Vec<BoardInfo>> {
        let context = self.inner.context(Instant::now(), "mac.boards", options)?;
        let result = super::collect::boards(
            &self.inner,
            &context,
            query,
            super::pagination::Target::Count(query.count()),
        )?;
        context.finish()?;
        Ok(result.rows)
    }
    /// A single bounded wire page. There is no verified empty/short-page EOF
    /// rule for this command, so an unfulfilled target remains Unknown. Resume
    /// with next_offset explicitly rather than assuming a complete event log.
    pub fn unusual(
        &self,
        query: &UnusualQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<Unusual>> {
        let context = self.inner.context(Instant::now(), "mac.unusual", options)?;
        let target = usize::try_from(query.count().get())
            .map_err(|_| super::query::invalid("MAC unusual target exceeds platform capacity"))?;
        let requested = u16::try_from(
            target
                .min(query.pages().page_size())
                .min(query.pages().max_records())
                .min(600),
        )
        .map_err(|_| super::query::invalid("MAC unusual page exceeds wire range"))?;
        let start = u16::try_from(query.start())
            .map_err(|_| super::query::invalid("MAC unusual offset exceeds wire range"))?;
        let request = commands::unusual_request(query.market(), start, requested)?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x1237,
            move |body, _| session::rows(decode::unusual(&body)?, requested, 2, 32),
        )?;
        let count = reply.wire_count.ok_or_else(|| {
            crate::Error::new(ErrorKind::Internal, "MAC unusual page lacks wire count")
        })?;
        let next = query
            .start()
            .checked_add(u32::from(count))
            .ok_or_else(|| super::query::invalid("MAC unusual next offset overflows"))?;
        let completion = if u32::from(count) == query.count().get() {
            super::PageCompletion::RequestedCount
        } else {
            super::PageCompletion::Unknown
        };
        let shared_meta = Arc::new(reply.meta.clone());
        let mut rows = reply.value;
        for row in &mut rows {
            row.set_meta(Arc::clone(&shared_meta));
        }
        let page = super::PageResult::new(rows, None, Some(next), completion, reply.meta)?;
        context.finish()?;
        Ok(page)
    }
    pub fn quotes(
        &self,
        query: &QuotesQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::Quote>> {
        let context = self.inner.context(Instant::now(), "mac.quotes", options)?;
        let reply = quotes(&self.inner, &context, query.ids(), query.fields())?;
        context.finish()?;
        Ok(reply)
    }
    pub fn tick_chart(
        &self,
        query: &TickChartQuery,
        options: &OperationOptions,
    ) -> Result<TickChart> {
        let context = self
            .inner
            .context(Instant::now(), "mac.tick_chart", options)?;
        let reply = tick_chart(&self.inner, &context, query)?;
        context.finish()?;
        Ok(reply)
    }
    pub fn tick_charts(
        &self,
        query: &TickChartsQuery,
        options: &OperationOptions,
    ) -> Result<MultiTickChart> {
        let context = self
            .inner
            .context(Instant::now(), "mac.tick_charts", options)?;
        let request = commands::tick_charts_request(
            query.id(),
            super::query::date_wire(query.date())?,
            u16::from(query.days()),
        )?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x123e,
            |body, _| session::decoded(decode::tick_charts(&body)?, None, Vec::new()),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn chart_sampling(
        &self,
        query: &ChartSamplingQuery,
        options: &OperationOptions,
    ) -> Result<Vec<WireFloat>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.chart_sampling", options)?;
        let reply = chart_sampling(&self.inner, &context, query)?;
        context.finish()?;
        Ok(reply)
    }
    pub fn symbol_info(
        &self,
        query: &SymbolInfoQuery,
        options: &OperationOptions,
    ) -> Result<SymbolInfo> {
        let context = self
            .inner
            .context(Instant::now(), "mac.symbol_info", options)?;
        let request = commands::symbol_info_request(query.id())?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x122a,
            |body, _| session::decoded(decode::symbol_info(&body)?, None, Vec::new()),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn belonging_boards(
        &self,
        query: &BelongingBoardsQuery,
        options: &OperationOptions,
    ) -> Result<Vec<BelongBoard>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.belonging_boards", options)?;
        let request = commands::belonging_board_request(query.id().market(), query.id().code())?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x1218,
            |body, _| {
                // JSON source rows have no fixed byte width. Retain the complete
                // payload when requested without inventing binary row ranges.
                session::decoded(decode::belonging_boards(&body)?, None, Vec::new())
            },
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn capital_flow(
        &self,
        query: &CapitalFlowQuery,
        options: &OperationOptions,
    ) -> Result<Option<CapitalFlow>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.capital_flow", options)?;
        let request = commands::capital_flow_request(query.id().market(), query.id().code())?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x1218,
            |body, _| session::decoded(decode::capital_flow(&body)?, None, Vec::new()),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn auction(
        &self,
        query: &AuctionQuery,
        options: &OperationOptions,
    ) -> Result<Vec<Auction>> {
        let context = self.inner.context(Instant::now(), "mac.auction", options)?;
        let request = commands::auction_request(query.id(), 0, 500)?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x123d,
            |body, _| session::rows(decode::auction(&body)?, 500, 36, 16),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn server_info(&self, options: &OperationOptions) -> Result<ServerSession> {
        let context = self
            .inner
            .context(Instant::now(), "mac.server_info", options)?;
        let request = commands::server_info_request()?;
        let reply = self.inner.exchange(&context,self.inner.mac_work(request)?,0x120f,|body,_| {
            let value = decode::server_info(&body)?;
            let invalid = value.today().parsed().is_none() || value.last_trading_day().parsed().is_none();
            let mut decoded = session::decoded(value,None,Vec::new())?;
            if invalid { decoded.diagnostics.push(Diagnostic::new("mac","server-date",EnrichmentStatus::Unavailable,
                Some(ErrorKind::Protocol),Some("server date is retained as observed; calendar interpretation is unavailable".into()))); }
            Ok(decoded)
        })?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn kline_offsets(
        &self,
        query: &KlineOffsetsQuery,
        options: &OperationOptions,
    ) -> Result<KlineOffsetInfo> {
        let context = self
            .inner
            .context(Instant::now(), "mac.kline_offsets", options)?;
        let request = commands::kline_offsets_request(query.offset(), query.count().get())?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x124a,
            |body, _| {
                let (total, returned) = commands::decode_kline_offsets(&body)?;
                session::decoded(KlineOffsetInfo { total, returned }, None, Vec::new())
            },
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn file_meta(&self, query: &FileMetaQuery, options: &OperationOptions) -> Result<FileMeta> {
        let context = self
            .inner
            .context(Instant::now(), "mac.file_meta", options)?;
        let request = commands::file_meta_request(query.filename().as_str(), query.offset())?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x1215,
            |body, _| session::decoded(decode::file_meta(&body)?, None, Vec::new()),
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn download_chunk(
        &self,
        query: &DownloadChunkQuery,
        options: &OperationOptions,
    ) -> Result<Vec<u8>> {
        let context = self
            .inner
            .context(Instant::now(), "mac.download_chunk", options)?;
        let size = query.size().get();
        let request = commands::download_chunk_request(
            query.filename().as_str(),
            query.index().get(),
            query.offset(),
            size,
        )?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x1217,
            move |body, _| {
                let chunk = decode::download_chunk(&body, size)?;
                session::decoded(chunk.into_bytes(), None, Vec::new())
            },
        )?;
        context.finish()?;
        Ok(reply.value)
    }
    pub fn goods(
        &self,
        query: &GoodsQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<GoodsItem>> {
        let context = self.inner.context(Instant::now(), "mac.goods", options)?;
        let requested = u16::try_from(query.count().get())
            .map_err(|_| super::query::invalid("MAC goods count exceeds wire range"))?;
        let request = commands::goods_request(query.market(), query.start(), u32::from(requested))?;
        let reply = self.inner.exchange(
            &context,
            self.inner.mac_work(request)?,
            0x2562,
            move |body, _| session::rows(decode::goods(&body)?, requested, 2, 48),
        )?;
        let count = reply.wire_count.ok_or_else(|| {
            crate::Error::new(ErrorKind::Internal, "MAC goods page lacks consumed rows")
        })?;
        let next = query
            .start()
            .checked_add(u32::from(count))
            .ok_or_else(|| super::query::invalid("MAC goods next offset overflows"))?;
        // The leading count is the decoded page count, not an established
        // global market total. This single-page API has no verified EOF rule.
        let completion = if count == requested {
            super::PageCompletion::RequestedCount
        } else {
            super::PageCompletion::Unknown
        };
        let page = super::PageResult::new(reply.value, None, Some(next), completion, reply.meta)?;
        context.finish()?;
        Ok(page)
    }
}

impl MacExClient {
    /// Explicitly collects at most 700 rows per MAC-wire response under one
    /// operation. This does not inherit Python's historical single-request
    /// goods_kline behavior; T27 preserves that in its compatibility profile.
    pub fn bars(
        &self,
        query: &super::ex_query::BarsQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::Bar>> {
        let context = self.inner.context(Instant::now(), "mac-ex.bars", options)?;
        let result = super::services::bars(
            &self.inner,
            &context,
            query.common(),
            self.config.xdxr_provider.clone(),
        )?;
        context.finish()?;
        Ok(result.rows)
    }
    /// HK stock markets use ordinary EX transaction bytes on this same logged
    /// in owner. Futures and other markets retain MAC0x122F encoding.
    pub fn transactions(
        &self,
        query: &super::ex_query::TransactionsQuery,
        options: &OperationOptions,
    ) -> Result<super::PageResult<super::Transaction>> {
        let context = self
            .inner
            .context(Instant::now(), "mac-ex.transactions", options)?;
        let result = super::collect::ex_transactions(
            &self.inner,
            &context,
            super::collect::ExTransactionPlan {
                id: query.id(),
                date: query.date(),
                start: query.start(),
                target: super::pagination::Target::Count(query.count()),
                pages: query.pages().clone(),
            },
        )?
        .page()?;
        context.finish()?;
        Ok(result)
    }
    /// A full cap of 50 pages/90000 rows without EOF is LimitExceeded. The
    /// method cannot return success while claiming an unproven complete day.
    pub fn all_transactions(
        &self,
        query: &super::ex_query::AllTransactionsQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::Transaction>> {
        let context = self
            .inner
            .context(Instant::now(), "mac-ex.all_transactions", options)?;
        let result = super::collect::ex_transactions(
            &self.inner,
            &context,
            super::collect::ExTransactionPlan {
                id: query.id(),
                date: query.date(),
                start: query.start(),
                target: super::pagination::Target::All,
                pages: query.pages().clone(),
            },
        )?;
        context.finish()?;
        Ok(result.rows)
    }
    pub fn quotes(
        &self,
        query: &super::ex_query::QuotesQuery,
        options: &OperationOptions,
    ) -> Result<Vec<super::Quote>> {
        let context = self
            .inner
            .context(Instant::now(), "mac-ex.quotes", options)?;
        let reply = quotes(&self.inner, &context, query.wire_ids(), query.fields())?;
        context.finish()?;
        Ok(reply)
    }
    pub fn tick_chart(
        &self,
        query: &super::ex_query::TickChartQuery,
        options: &OperationOptions,
    ) -> Result<TickChart> {
        let context = self
            .inner
            .context(Instant::now(), "mac-ex.tick_chart", options)?;
        let reply = tick_chart(&self.inner, &context, query.common())?;
        context.finish()?;
        Ok(reply)
    }
    pub fn chart_sampling(
        &self,
        query: &super::ex_query::ChartSamplingQuery,
        options: &OperationOptions,
    ) -> Result<Vec<WireFloat>> {
        let context = self
            .inner
            .context(Instant::now(), "mac-ex.chart_sampling", options)?;
        let reply = chart_sampling(&self.inner, &context, query.common())?;
        context.finish()?;
        Ok(reply)
    }
}

fn board_limits(
    context: &mut crate::operation::OperationContext,
    pages: &crate::PageOptions,
    bytes: usize,
) -> Result<()> {
    context.set_work_limits(crate::operation::WorkLimits {
        pages: NonZeroUsize::new(pages.max_pages()),
        records: Some(pages.max_records()),
        bytes: Some(bytes),
    })
}
fn quotes(
    owner: &MacOwner,
    context: &crate::operation::OperationContext,
    ids: &[super::MacInstrumentId],
    fields: &super::FieldSelection,
) -> Result<Vec<super::Quote>> {
    let lease = owner.session.lease(context)?;
    let rows = quotes_leased(owner, context, &lease, ids, fields, None)?;
    lease.accept();
    Ok(rows)
}
pub(super) fn quotes_leased(
    owner: &MacOwner,
    context: &crate::operation::OperationContext,
    lease: &crate::ex::session::Lease,
    ids: &[super::MacInstrumentId],
    fields: &super::FieldSelection,
    source: Option<(Endpoint, u64)>,
) -> Result<Vec<super::Quote>> {
    let requested = u16::try_from(ids.len())
        .map_err(|_| super::query::invalid("MAC quote occurrences exceed wire range"))?;
    let request = commands::quote_request(ids, fields)?;
    let work = match source {
        Some(source) => owner.pinned_mac_work(request, source)?,
        None => owner.mac_work(request)?,
    };
    let sponsor = context.sponsor();
    let reply = owner.exchange_leased_with_purpose(
        context,
        lease,
        work,
        0x122b,
        crate::operation::ChildPurpose::Page,
        move |body, _| {
            let prefix = body.get(..26).ok_or_else(|| {
                crate::Error::new(ErrorKind::Protocol, "MAC quote response lacks its prefix")
            })?;
            let count = u16::from_le_bytes([prefix[24], prefix[25]]);
            if count > requested {
                return Err(crate::Error::new(
                    ErrorKind::Protocol,
                    "MAC quote response exceeds its request",
                ));
            }
            let bitmap = super::bitmap::decode_bitmap(&prefix[..20])?;
            let width = super::bitmap::quote_row_width(&bitmap)?;
            let slots = width.saturating_sub(68) / 4;
            let per_row = std::mem::size_of::<super::Quote>()
                + std::mem::size_of::<super::models::QuoteRow>()
                + 66 * 6
                + slots * (std::mem::size_of::<super::models::QuoteField>() + 128)
                + std::mem::size_of::<(usize, usize)>();
            sponsor.record_bytes(usize::from(count).checked_mul(per_row).ok_or_else(|| {
                crate::Error::new(ErrorKind::LimitExceeded, "MAC quote allocation overflows")
            })?)?;
            let (_, rows, bitmap) =
                commands::decode_quote_rows(&body, super::models::FieldContext::Instrument)?;
            let width = super::bitmap::quote_row_width(&bitmap)?;
            // Identity projection is inside strict decoding: a malformed identity
            // never permits this socket to be published for another operation.
            let values = rows
                .into_iter()
                .map(super::Quote::from_row)
                .collect::<Result<Vec<_>>>()?;
            session::rows(values, requested, 26, width)
        },
    )?;
    let meta = Arc::new(reply.meta);
    let values = reply
        .value
        .into_iter()
        .map(|value| value.with_meta(Arc::clone(&meta)))
        .collect();
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(values)
}
fn tick_chart(
    owner: &MacOwner,
    context: &crate::operation::OperationContext,
    query: &TickChartQuery,
) -> Result<TickChart> {
    let request = commands::tick_chart_request(query.id(), super::query::date_wire(query.date())?)?;
    owner
        .exchange(context, owner.mac_work(request)?, 0x122d, |body, _| {
            session::decoded(decode::tick_chart(&body)?, None, Vec::new())
        })
        .map(|reply| reply.value)
}
fn chart_sampling(
    owner: &MacOwner,
    context: &crate::operation::OperationContext,
    query: &ChartSamplingQuery,
) -> Result<Vec<WireFloat>> {
    let request = commands::chart_sampling_request(query.id())?;
    owner
        .exchange(context, owner.mac_work(request)?, 0x254d, |body, _| {
            session::rows(decode::chart_sampling(&body)?, u16::MAX, 42, 4)
        })
        .map(|reply| reply.value)
}
