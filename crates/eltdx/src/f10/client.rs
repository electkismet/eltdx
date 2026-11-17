use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::config::{checked_url, INTERNATIONAL_URL, LADDER_URL, STATIC_URL};
use super::{
    CallQuery, F10Config, Header, ParamsQuery, ProxyUrl, ResolverPolicy, Response, Route, TlsPolicy,
};
use crate::config::{ClientConfig, ConfigDraft};
use crate::operation::{OperationContext, OperationOptions};
use crate::session::SessionOwner;
use crate::{Error, ErrorKind, Result};

/// Read-only owner snapshot. Reading diagnostics never probes a server.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct F10Diagnostics {
    pub(crate) generation: usize,
    pub(crate) active_endpoint: Option<String>,
    pub(crate) completed_requests: u64,
}
impl F10Diagnostics {
    pub fn generation(&self) -> usize {
        self.generation
    }
    pub fn active_endpoint(&self) -> Option<&str> {
        self.active_endpoint.as_deref()
    }
    pub fn completed_requests(&self) -> u64 {
        self.completed_requests
    }
}

/// Configuration assembly only. Build does not create a runtime, resolve DNS,
/// discover roots/proxy settings, open sockets or read user files.
#[derive(Clone, Debug)]
pub struct F10ClientBuilder {
    base_url: String,
    ladder_url: String,
    international_url: String,
    headers: Option<Vec<Header>>,
    prefer_ipv4: bool,
    timeout: Duration,
    max_response_bytes: NonZeroU64,
    max_encoded_bytes: NonZeroU64,
    max_operation_bytes: NonZeroU64,
    max_header_bytes: NonZeroUsize,
    max_redirects: u8,
    proxy: Option<ProxyUrl>,
    tls: TlsPolicy,
    resolver: ResolverPolicy,
    close_timeout: Duration,
    cleanup_timeout: Duration,
    local_date_clock: super::clock::LocalDateClockHandle,
}
impl Default for F10ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl F10ClientBuilder {
    pub fn new() -> Self {
        Self {
            base_url: STATIC_URL.to_owned(),
            ladder_url: LADDER_URL.to_owned(),
            international_url: INTERNATIONAL_URL.to_owned(),
            headers: None,
            prefer_ipv4: true,
            timeout: Duration::from_secs(8),
            max_response_bytes: NonZeroU64::new(16 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
            max_encoded_bytes: NonZeroU64::new(16 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
            max_operation_bytes: NonZeroU64::new(64 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
            max_header_bytes: NonZeroUsize::new(64 * 1024).unwrap_or(NonZeroUsize::MIN),
            max_redirects: 0,
            proxy: None,
            tls: TlsPolicy::SystemRoots,
            resolver: ResolverPolicy::Bounded,
            close_timeout: Duration::from_secs(1),
            cleanup_timeout: Duration::from_secs(1),
            local_date_clock: super::clock::LocalDateClockHandle::default(),
        }
    }
    pub fn with_base_url(mut self, value: impl Into<String>) -> Self {
        self.base_url = value.into();
        self
    }
    pub fn with_ladder_url(mut self, value: impl Into<String>) -> Self {
        self.ladder_url = value.into();
        self
    }
    pub fn with_international_url(mut self, value: impl Into<String>) -> Self {
        self.international_url = value.into();
        self
    }
    /// Entries replace matching default names case-insensitively in place; new
    /// names are appended. Duplicate user names preserve their relative order.
    pub fn with_headers(mut self, value: Vec<Header>) -> Self {
        self.headers = Some(value);
        self
    }
    pub fn with_prefer_ipv4(mut self, value: bool) -> Self {
        self.prefer_ipv4 = value;
        self
    }
    pub fn with_timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }
    pub fn with_max_response_bytes(mut self, value: NonZeroU64) -> Self {
        self.max_response_bytes = value;
        self
    }
    pub fn with_max_encoded_bytes(mut self, value: NonZeroU64) -> Self {
        self.max_encoded_bytes = value;
        self
    }
    pub fn with_max_operation_bytes(mut self, value: NonZeroU64) -> Self {
        self.max_operation_bytes = value;
        self
    }
    pub fn with_max_header_bytes(mut self, value: NonZeroUsize) -> Self {
        self.max_header_bytes = value;
        self
    }
    pub fn with_max_redirects(mut self, value: u8) -> Self {
        self.max_redirects = value;
        self
    }
    pub fn with_proxy(mut self, value: Option<ProxyUrl>) -> Self {
        self.proxy = value;
        self
    }
    pub fn with_tls(mut self, value: TlsPolicy) -> Self {
        self.tls = value;
        self
    }
    pub fn with_resolver(mut self, value: ResolverPolicy) -> Self {
        self.resolver = value;
        self
    }
    pub fn with_close_timeout(mut self, value: Duration) -> Self {
        self.close_timeout = value;
        self
    }
    pub fn with_cleanup_timeout(mut self, value: Duration) -> Self {
        self.cleanup_timeout = value;
        self
    }
    /// Used only when a ladder query omits both dates; never called by build.
    pub fn with_local_date_clock(mut self, value: Arc<dyn super::LocalDateClock>) -> Self {
        self.local_date_clock = super::clock::LocalDateClockHandle(value);
        self
    }

    pub fn build(self) -> Result<F10Client> {
        self.build_inner(None)
    }

    pub(crate) fn build_with_owner(self, owner: Arc<SessionOwner>) -> Result<F10Client> {
        self.build_inner(Some(owner))
    }

    fn build_inner(self, owner: Option<Arc<SessionOwner>>) -> Result<F10Client> {
        let mut config = F10Config::defaults()?;
        config.base_url = checked_url(&self.base_url)?;
        config.ladder_url = checked_url(&self.ladder_url)?;
        config.international_url = checked_url(&self.international_url)?;
        for duration in [self.timeout, self.close_timeout, self.cleanup_timeout] {
            if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "F10 timeouts must be positive and fit the monotonic clock",
                ));
            }
        }
        for size in [
            self.max_response_bytes.get(),
            self.max_encoded_bytes.get(),
            self.max_operation_bytes.get(),
        ] {
            if usize::try_from(size).is_err() {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "F10 byte limit exceeds this platform's address space",
                ));
            }
        }
        if let ResolverPolicy::Nameservers(servers) = &self.resolver {
            if servers.is_empty() || servers.iter().any(|server| server.port() == 0) {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "F10 nameservers must contain nonzero-port endpoints",
                ));
            }
        }
        if let Some(headers) = self.headers {
            for default in &mut config.headers {
                if let Some(replacement) = headers
                    .iter()
                    .find(|header| header.name().eq_ignore_ascii_case(default.name()))
                {
                    *default = replacement.clone();
                }
            }
            let mut replaced_defaults = Vec::<String>::new();
            for header in headers {
                if ["Content-Type", "Accept"]
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(header.name()))
                    && !replaced_defaults
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(header.name()))
                {
                    replaced_defaults.push(header.name().to_owned());
                } else {
                    config.headers.push(header);
                }
            }
        }
        config.prefer_ipv4 = self.prefer_ipv4;
        config.timeout = self.timeout;
        config.max_response_bytes = self.max_response_bytes;
        config.max_encoded_bytes = self.max_encoded_bytes;
        config.max_operation_bytes = self.max_operation_bytes;
        config.max_header_bytes = self.max_header_bytes;
        config.max_redirects = self.max_redirects;
        config.proxy = self.proxy;
        config.tls = self.tls;
        config.resolver = self.resolver;
        for header in &config.headers {
            hyper::header::HeaderName::from_bytes(header.name().as_bytes()).map_err(|error| {
                Error::new(ErrorKind::InvalidArgument, "invalid HTTP header name")
                    .with_source(error)
            })?;
            hyper::header::HeaderValue::from_str(header.value()).map_err(|error| {
                Error::new(ErrorKind::InvalidArgument, "invalid HTTP header value")
                    .with_source(error)
            })?;
            if ["Host", "Content-Length", "Transfer-Encoding", "Connection"]
                .iter()
                .any(|name| header.name().eq_ignore_ascii_case(name))
            {
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    "F10 framing headers are transport-owned",
                )
                .with_context("header", header.name()));
            }
        }
        let common = ConfigDraft {
            request_timeout: self.timeout,
            close_timeout: self.close_timeout,
            cleanup_timeout: self.cleanup_timeout,
            heartbeat_interval: None,
            ..ConfigDraft::default()
        }
        .validate()?;
        let owner = owner.unwrap_or_else(|| {
            Arc::new(SessionOwner::new(self.close_timeout, self.cleanup_timeout))
        });
        let diagnostics = Mutex::new(F10Diagnostics {
            generation: owner.generation(),
            ..F10Diagnostics::default()
        });
        Ok(F10Client {
            inner: Arc::new(F10Inner {
                config,
                common,
                owner,
                diagnostics,
                local_date_clock: self.local_date_clock,
                connected: AtomicBool::new(false),
                gate: Arc::new(super::worker::Concurrency::new(4)),
                connect_gate: Arc::new(super::worker::Concurrency::new(1)),
            }),
        })
    }
}

#[derive(Debug)]
pub(crate) struct F10Inner {
    pub(crate) config: F10Config,
    pub(crate) common: ClientConfig,
    pub(crate) owner: Arc<SessionOwner>,
    pub(crate) diagnostics: Mutex<F10Diagnostics>,
    pub(crate) local_date_clock: super::clock::LocalDateClockHandle,
    connected: AtomicBool,
    gate: Arc<super::worker::Concurrency>,
    connect_gate: Arc<super::worker::Concurrency>,
}

/// An independent F10 owner. Clones share its lifetime and irreversible close.
#[derive(Clone, Debug)]
pub struct F10Client {
    pub(crate) inner: Arc<F10Inner>,
}
impl F10Client {
    pub fn builder() -> F10ClientBuilder {
        F10ClientBuilder::new()
    }
    pub fn config(&self) -> &F10Config {
        &self.inner.config
    }
    pub fn diagnostics(&self) -> Result<F10Diagnostics> {
        self.inner
            .diagnostics
            .lock()
            .map(|state| state.clone())
            .map_err(|_| Error::new(ErrorKind::Internal, "F10 diagnostics lock poisoned"))
    }
    pub fn close(&self) -> Result<()> {
        self.inner.owner.close()
    }
    /// Explicitly resolve and negotiate the base endpoint. F10 has no protocol
    /// login; this probe closes its socket after transport negotiation.
    pub fn connect(&self, options: &OperationOptions) -> Result<()> {
        let context = self.context(Instant::now(), "f10.connect", options)?;
        if self.inner.connected.load(Ordering::Acquire) {
            return context.finish();
        }
        // Only one explicit transport probe may run at a time. Waiting callers
        // retain their own deadline/cancellation and recheck successful state.
        let connect_slot = self.inner.connect_gate.acquire(&context)?;
        if self.inner.connected.load(Ordering::Acquire) {
            return context.finish();
        }
        let slot = self
            .inner
            .gate
            .acquire(&context)?
            .holding(connect_slot.clone());
        if self.inner.connected.load(Ordering::Acquire) {
            return context.finish();
        }
        let authorization = context
            .begin_child(crate::operation::ChildPurpose::Required, None)?
            .into_transport_authorization();
        let reply = super::worker::Job::execute(
            self.inner.config.clone(),
            super::worker::Work::Connect(self.inner.config.base_url.clone()),
            authorization,
            &context,
            slot,
        )
        .map_err(|error| {
            context
                .annotate(error, crate::TimeoutPhase::Connect, None)
                .with_context("domain", "f10")
        })?;
        self.record_endpoint(reply.url)?;
        self.inner.connected.store(true, Ordering::Release);
        context.finish()
    }
    pub fn call(&self, query: &CallQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.call", options)?;
        let response = self.call_in_context(query, &context)?;
        context.finish()?;
        Ok(response)
    }
    pub fn params(&self, query: &ParamsQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.params", options)?;
        let response = self.call_in_context(query.call(), &context)?;
        context.finish()?;
        Ok(response)
    }
    /// T20 convenience endpoints use this seam so prerequisites and all pages
    /// share the original entry clock, owner admission and request counters.
    pub(crate) fn call_in_context(
        &self,
        query: &CallQuery,
        context: &OperationContext,
    ) -> Result<Response> {
        use crate::types::{
            Diagnostic, EnrichmentStatus, RawData, RawPolicy, ResultMeta, SourceStamp,
        };
        use crate::{operation::ChildPurpose, TimeoutPhase};
        use bytes::Bytes;
        let result = (|| {
            context.check_boundary(TimeoutPhase::Send)?;
            let slot = self.inner.gate.acquire(context)?;
            let body = query.payload().encoded(
                self.inner.config.max_encoded_bytes.get() as usize,
                &mut || context.check_boundary(TimeoutPhase::Send),
            )?;
            let authorization = context
                .begin_child(ChildPurpose::Required, None)?
                .into_transport_authorization();
            let reply = super::worker::Job::execute(
                self.inner.config.clone(),
                super::worker::Work::Post {
                    url: self.inner.config.request_url(query.entry()),
                    body: Bytes::from(body),
                    retain_payload: context.raw() != RawPolicy::Omit,
                    retain_wire: context.raw() == RawPolicy::RetainWire,
                },
                authorization,
                context,
                slot.clone(),
            )?;
            let value = super::json::decode(
                &reply.body,
                self.inner.config.max_response_bytes.get() as usize,
                &mut || context.check_boundary(TimeoutPhase::Decode),
            )?;
            context.check_boundary(TimeoutPhase::Decode)?;
            let source = SourceStamp::new(
                None,
                Some(reply.endpoint),
                context.session_id.to_string(),
                None,
                None,
            );
            let diagnostic = Diagnostic::new(
                Route::for_entry(query.entry()).name(),
                "http",
                EnrichmentStatus::Succeeded,
                None,
                Some(reply.url.clone()),
            );
            let raw = (context.raw() != RawPolicy::Omit).then(|| {
                RawData::new(
                    vec![Bytes::from(reply.encoded)],
                    (context.raw() == RawPolicy::RetainWire).then(|| vec![Bytes::from(reply.wire)]),
                    Vec::new(),
                )
            });
            let request = query
                .payload()
                .retained(&mut || context.check_boundary(TimeoutPhase::Decode))?;
            let response = Response::from_json(
                query.entry().as_str().to_owned(),
                request,
                value,
                context.raw() != RawPolicy::Omit,
                ResultMeta::new(source, vec![diagnostic], raw),
                &mut || context.check_boundary(TimeoutPhase::Decode),
            )?;
            self.record_endpoint(reply.url)?;
            Ok(response)
        })();
        result.map_err(|error| {
            context
                .annotate(error, TimeoutPhase::Response, None)
                .with_context("domain", "f10")
                .with_context("entry", query.entry().as_str())
        })
    }
    fn record_endpoint(&self, url: String) -> Result<()> {
        let mut state = self
            .inner
            .diagnostics
            .lock()
            .map_err(|_| Error::new(ErrorKind::Internal, "F10 diagnostics lock poisoned"))?;
        state.active_endpoint = Some(url);
        state.completed_requests = state.completed_requests.checked_add(1).ok_or_else(|| {
            Error::new(ErrorKind::LimitExceeded, "F10 diagnostic counter overflow")
        })?;
        Ok(())
    }
    pub(crate) fn context(
        &self,
        entry: Instant,
        name: &'static str,
        options: &OperationOptions,
    ) -> Result<OperationContext> {
        let mut context = OperationContext::rust_public(
            entry,
            name,
            Arc::clone(&self.inner.owner),
            &self.inner.common,
            options,
        )?;
        context.set_work_limits(crate::operation::WorkLimits {
            bytes: Some(self.inner.config.max_operation_bytes.get() as usize),
            ..crate::operation::WorkLimits::default()
        })?;
        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_build_is_lazy_and_close_is_shared_without_reopening() -> Result<()> {
        let client = F10Client::builder()
            .with_base_url("https://unresolved.invalid:65535/TQLEX")
            .build()?;
        assert_eq!(client.diagnostics()?.active_endpoint(), None);
        let clone = client.clone();
        assert_eq!(
            client.diagnostics()?.generation(),
            clone.diagnostics()?.generation()
        );
        clone.close()?;
        assert_eq!(
            client
                .context(Instant::now(), "closed", &OperationOptions::default())
                .unwrap_err()
                .kind(),
            ErrorKind::Closed
        );
        client.close()?;
        Ok(())
    }
    #[test]
    fn configuration_keeps_route_overrides_and_replaces_default_header_in_place() -> Result<()> {
        let client = F10Client::builder()
            .with_base_url("http://localhost:1/base")
            .with_ladder_url("http://localhost:2/ladder")
            .with_international_url("http://localhost:3/international")
            .with_headers(vec![
                Header::new("accept", "application/custom")?,
                Header::new("X-Test", "yes")?,
            ])
            .build()?;
        assert_eq!(client.config().headers()[1].name(), "accept");
        assert_eq!(client.config().headers()[1].value(), "application/custom");
        assert_eq!(client.config().headers()[2].name(), "X-Test");
        assert_eq!(client.config().ladder_url(), "http://localhost:2/ladder");
        client.close()
    }
}
