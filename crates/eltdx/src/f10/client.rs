use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::config::{checked_url, INTERNATIONAL_URL, LADDER_URL, STATIC_URL};
use super::{F10Config, Header, ProxyUrl, ResolverPolicy, TlsPolicy};
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
    pub fn generation(&self) -> usize { self.generation }
    pub fn active_endpoint(&self) -> Option<&str> { self.active_endpoint.as_deref() }
    pub fn completed_requests(&self) -> u64 { self.completed_requests }
}

/// Configuration assembly only. Build does not create a runtime, resolve DNS,
/// discover roots/proxy settings, open sockets or read user files.
#[derive(Clone, Debug)]
pub struct F10ClientBuilder {
    base_url: String, ladder_url: String, international_url: String,
    headers: Option<Vec<Header>>, prefer_ipv4: bool, timeout: Duration,
    max_response_bytes: NonZeroU64, max_encoded_bytes: NonZeroU64,
    max_header_bytes: NonZeroUsize, max_redirects: u8,
    proxy: Option<ProxyUrl>, tls: TlsPolicy, resolver: ResolverPolicy,
    close_timeout: Duration, cleanup_timeout: Duration,
}
impl Default for F10ClientBuilder { fn default() -> Self { Self::new() } }
impl F10ClientBuilder {
    pub fn new() -> Self {
        Self {
            base_url: STATIC_URL.to_owned(), ladder_url: LADDER_URL.to_owned(),
            international_url: INTERNATIONAL_URL.to_owned(), headers: None,
            prefer_ipv4: true, timeout: Duration::from_secs(8),
            max_response_bytes: NonZeroU64::new(16 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
            max_encoded_bytes: NonZeroU64::new(16 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
            max_header_bytes: NonZeroUsize::new(64 * 1024).unwrap_or(NonZeroUsize::MIN),
            max_redirects: 0, proxy: None, tls: TlsPolicy::SystemRoots,
            resolver: ResolverPolicy::Bounded, close_timeout: Duration::from_secs(1),
            cleanup_timeout: Duration::from_secs(1),
        }
    }
    pub fn with_base_url(mut self, value: impl Into<String>) -> Self { self.base_url = value.into(); self }
    pub fn with_ladder_url(mut self, value: impl Into<String>) -> Self { self.ladder_url = value.into(); self }
    pub fn with_international_url(mut self, value: impl Into<String>) -> Self { self.international_url = value.into(); self }
    /// Entries replace matching default names case-insensitively in place; new
    /// names are appended. Duplicate user names preserve their relative order.
    pub fn with_headers(mut self, value: Vec<Header>) -> Self { self.headers = Some(value); self }
    pub fn with_prefer_ipv4(mut self, value: bool) -> Self { self.prefer_ipv4 = value; self }
    pub fn with_timeout(mut self, value: Duration) -> Self { self.timeout = value; self }
    pub fn with_max_response_bytes(mut self, value: NonZeroU64) -> Self { self.max_response_bytes = value; self }
    pub fn with_max_encoded_bytes(mut self, value: NonZeroU64) -> Self { self.max_encoded_bytes = value; self }
    pub fn with_max_header_bytes(mut self, value: NonZeroUsize) -> Self { self.max_header_bytes = value; self }
    pub fn with_max_redirects(mut self, value: u8) -> Self { self.max_redirects = value; self }
    pub fn with_proxy(mut self, value: Option<ProxyUrl>) -> Self { self.proxy = value; self }
    pub fn with_tls(mut self, value: TlsPolicy) -> Self { self.tls = value; self }
    pub fn with_resolver(mut self, value: ResolverPolicy) -> Self { self.resolver = value; self }
    pub fn with_close_timeout(mut self, value: Duration) -> Self { self.close_timeout = value; self }
    pub fn with_cleanup_timeout(mut self, value: Duration) -> Self { self.cleanup_timeout = value; self }

    pub fn build(self) -> Result<F10Client> {
        let mut config = F10Config::defaults()?;
        config.base_url = checked_url(&self.base_url)?;
        config.ladder_url = checked_url(&self.ladder_url)?;
        config.international_url = checked_url(&self.international_url)?;
        for duration in [self.timeout, self.close_timeout, self.cleanup_timeout] {
            if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
                return Err(Error::new(ErrorKind::InvalidArgument, "F10 timeouts must be positive and fit the monotonic clock"));
            }
        }
        for size in [self.max_response_bytes.get(), self.max_encoded_bytes.get()] {
            if usize::try_from(size).is_err() {
                return Err(Error::new(ErrorKind::InvalidArgument, "F10 byte limit exceeds this platform's address space"));
            }
        }
        if let ResolverPolicy::Nameservers(servers) = &self.resolver {
            if servers.is_empty() || servers.iter().any(|server| server.port() == 0) {
                return Err(Error::new(ErrorKind::InvalidArgument, "F10 nameservers must contain nonzero-port endpoints"));
            }
        }
        if let Some(headers) = self.headers {
            for default in &mut config.headers {
                if let Some(replacement) = headers.iter().find(|header| header.name().eq_ignore_ascii_case(default.name())) {
                    *default = replacement.clone();
                }
            }
            let mut replaced_defaults = Vec::<String>::new();
            for header in headers {
                if ["Content-Type", "Accept"].iter().any(|name| name.eq_ignore_ascii_case(header.name()))
                    && !replaced_defaults.iter().any(|name| name.eq_ignore_ascii_case(header.name())) {
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
        config.max_header_bytes = self.max_header_bytes;
        config.max_redirects = self.max_redirects;
        config.proxy = self.proxy;
        config.tls = self.tls;
        config.resolver = self.resolver;
        let common = ConfigDraft {
            request_timeout: self.timeout, close_timeout: self.close_timeout,
            cleanup_timeout: self.cleanup_timeout, heartbeat_interval: None,
            ..ConfigDraft::default()
        }.validate()?;
        let owner = Arc::new(SessionOwner::new(self.close_timeout, self.cleanup_timeout));
        let diagnostics = Mutex::new(F10Diagnostics { generation: owner.generation(), ..F10Diagnostics::default() });
        Ok(F10Client { inner: Arc::new(F10Inner { config, common, owner, diagnostics }) })
    }
}

#[derive(Debug)]
pub(crate) struct F10Inner {
    pub(crate) config: F10Config,
    pub(crate) common: ClientConfig,
    pub(crate) owner: Arc<SessionOwner>,
    pub(crate) diagnostics: Mutex<F10Diagnostics>,
}

/// An independent F10 owner. Clones share its lifetime and irreversible close.
#[derive(Clone, Debug)]
pub struct F10Client { pub(crate) inner: Arc<F10Inner> }
impl F10Client {
    pub fn builder() -> F10ClientBuilder { F10ClientBuilder::new() }
    pub fn config(&self) -> &F10Config { &self.inner.config }
    pub fn diagnostics(&self) -> Result<F10Diagnostics> {
        self.inner.diagnostics.lock().map(|state| state.clone())
            .map_err(|_| Error::new(ErrorKind::Internal, "F10 diagnostics lock poisoned"))
    }
    pub fn close(&self) -> Result<()> { self.inner.owner.close() }
    pub(crate) fn context(&self, entry: Instant, name: &'static str, options: &OperationOptions) -> Result<OperationContext> {
        OperationContext::rust_public(entry, name, Arc::clone(&self.inner.owner), &self.inner.common, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_build_is_lazy_and_close_is_shared_without_reopening() -> Result<()> {
        let client = F10Client::builder().with_base_url("https://unresolved.invalid:65535/TQLEX").build()?;
        assert_eq!(client.diagnostics()?.active_endpoint(), None);
        let clone = client.clone();
        assert_eq!(client.diagnostics()?.generation(), clone.diagnostics()?.generation());
        clone.close()?;
        assert_eq!(client.context(Instant::now(), "closed", &OperationOptions::default()).unwrap_err().kind(), ErrorKind::Closed);
        client.close()?;
        Ok(())
    }
    #[test]
    fn configuration_keeps_route_overrides_and_replaces_default_header_in_place() -> Result<()> {
        let client = F10Client::builder().with_base_url("http://localhost:1/base")
            .with_ladder_url("http://localhost:2/ladder").with_international_url("http://localhost:3/international")
            .with_headers(vec![Header::new("accept", "application/custom")?, Header::new("X-Test", "yes")?]).build()?;
        assert_eq!(client.config().headers()[1].name(), "accept");
        assert_eq!(client.config().headers()[1].value(), "application/custom");
        assert_eq!(client.config().headers()[2].name(), "X-Test");
        assert_eq!(client.config().ladder_url(), "http://localhost:2/ladder");
        client.close()
    }
}
