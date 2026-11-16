use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Duration;
use url::Url;

use super::EntryName;
use crate::{Error, ErrorKind, Result};

pub const STATIC_URL: &str = "http://static.tdx.com.cn:7615/TQLEX";
pub const LADDER_URL: &str = "http://hot.icfqs.com:7615/TQLEX";
pub const INTERNATIONAL_URL: &str = "http://page2.tdx.com.cn:7615/TQLEX";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route { Static, Ladder, International }
impl Route {
    pub fn for_entry(entry: &EntryName) -> Self {
        let entry = entry.as_str();
        let route = super::python_legacy::route_with(
            || Ok::<_, std::convert::Infallible>(entry == "CWServ.cfg_fx_lbtt"),
            || Ok(["CWServ.skef10_mg_", "CWServ.skef10_hk_", "CWServ.skef10_sb_",
                "CWServ.skef10_bond_", "CWServ.skef10_hy_"].iter().any(|prefix| entry.starts_with(prefix))),
            || Ok(entry.starts_with("CWServ.qhf9_")));
        match route { Ok(route) => route, Err(impossible) => match impossible {} }
    }
    pub fn name(self) -> &'static str { match self { Self::Static => "static", Self::Ladder => "ladder", Self::International => "international" } }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header { name: String, value: String }
impl Header {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value.bytes().any(|byte| (byte < 32 && byte != b'\t') || byte == 127) {
            return Err(Error::new(ErrorKind::InvalidArgument, "invalid F10 HTTP header"));
        }
        Ok(Self { name, value })
    }
    pub fn name(&self) -> &str { &self.name }
    pub fn value(&self) -> &str { &self.value }
}

fn url(value: &str, proxy: bool) -> Result<Url> {
    if value.chars().any(char::is_control) { return Err(Error::new(ErrorKind::InvalidArgument, "HTTP URL contains a control character")); }
    let parsed = Url::parse(value).map_err(|error| Error::new(ErrorKind::InvalidArgument, "invalid HTTP URL").with_source(error))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none()
        || !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some()
        || parsed.query().is_some() || (proxy && !matches!(parsed.path(), "" | "/")) {
        return Err(Error::new(ErrorKind::InvalidArgument, "HTTP URL must have a host and no credentials, fragment or query"));
    }
    Ok(parsed)
}

/// Explicit HTTP(S) proxy URL; no environment proxy variables are consulted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyUrl(Url);
impl ProxyUrl {
    pub fn new(value: &str) -> Result<Self> { Ok(Self(url(value, true)?)) }
    pub fn as_str(&self) -> &str { self.0.as_str() }
    pub(crate) fn url(&self) -> &Url { &self.0 }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TlsPolicy {
    SystemRoots,
    /// DER certificates are validated when network work first needs TLS.
    CustomRoots(Vec<Vec<u8>>),
    SystemAndCustomRoots(Vec<Vec<u8>>),
}

/// The production policy must use a cancellable bounded resolver. An explicit
/// nameserver list avoids environment discovery in deployments which need it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverPolicy { Bounded, Nameservers(Vec<std::net::SocketAddr>) }

#[derive(Clone, Debug)]
pub struct F10Config {
    pub(crate) base_url: Url,
    pub(crate) ladder_url: Url,
    pub(crate) international_url: Url,
    pub(crate) headers: Vec<Header>,
    pub(crate) prefer_ipv4: bool,
    pub(crate) timeout: Duration,
    pub(crate) max_response_bytes: NonZeroU64,
    pub(crate) max_encoded_bytes: NonZeroU64,
    pub(crate) max_operation_bytes: NonZeroU64,
    pub(crate) max_header_bytes: NonZeroUsize,
    pub(crate) max_redirects: u8,
    pub(crate) proxy: Option<ProxyUrl>,
    pub(crate) tls: TlsPolicy,
    pub(crate) resolver: ResolverPolicy,
}
impl F10Config {
    pub(crate) fn defaults() -> Result<Self> {
        Ok(Self {
            base_url: url(STATIC_URL, false)?, ladder_url: url(LADDER_URL, false)?,
            international_url: url(INTERNATIONAL_URL, false)?,
            headers: vec![Header::new("Content-Type", "application/json")?, Header::new("Accept", "application/json")?],
            prefer_ipv4: true, timeout: Duration::from_secs(8),
            max_response_bytes: NonZeroU64::new(16 * 1024 * 1024).ok_or_else(internal_limit)?,
            max_encoded_bytes: NonZeroU64::new(16 * 1024 * 1024).ok_or_else(internal_limit)?,
            max_operation_bytes: NonZeroU64::new(64 * 1024 * 1024).ok_or_else(internal_limit)?,
            max_header_bytes: NonZeroUsize::new(64 * 1024).ok_or_else(internal_limit)?,
            max_redirects: 0, proxy: None, tls: TlsPolicy::SystemRoots, resolver: ResolverPolicy::Bounded,
        })
    }
    pub fn base_url(&self) -> &str { self.base_url.as_str() }
    pub fn ladder_url(&self) -> &str { self.ladder_url.as_str() }
    pub fn international_url(&self) -> &str { self.international_url.as_str() }
    pub fn headers(&self) -> &[Header] { &self.headers }
    pub fn prefer_ipv4(&self) -> bool { self.prefer_ipv4 }
    pub fn timeout(&self) -> Duration { self.timeout }
    pub fn max_response_bytes(&self) -> NonZeroU64 { self.max_response_bytes }
    pub fn max_encoded_bytes(&self) -> NonZeroU64 { self.max_encoded_bytes }
    /// Combined decoded and retained HTTP bytes across all requests made by
    /// one public operation. This is a logical byte budget, not a heap metric.
    pub fn max_operation_bytes(&self) -> NonZeroU64 { self.max_operation_bytes }
    pub fn max_header_bytes(&self) -> NonZeroUsize { self.max_header_bytes }
    pub fn max_redirects(&self) -> u8 { self.max_redirects }
    pub fn proxy(&self) -> Option<&ProxyUrl> { self.proxy.as_ref() }
    pub fn tls(&self) -> &TlsPolicy { &self.tls }
    pub fn resolver(&self) -> &ResolverPolicy { &self.resolver }
    pub(crate) fn endpoint(&self, route: Route) -> &Url {
        match route { Route::Static => &self.base_url, Route::Ladder => &self.ladder_url, Route::International => &self.international_url }
    }
    pub(crate) fn request_url(&self, entry: &EntryName) -> Url {
        let mut output = self.endpoint(Route::for_entry(entry)).clone();
        output.query_pairs_mut().append_pair("Entry", entry.as_str());
        output
    }
}
pub(crate) fn checked_url(value: &str) -> Result<Url> { url(value, false) }
fn internal_limit() -> Error { Error::new(ErrorKind::Internal, "invalid built-in F10 limit") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_routes_limits_and_no_implicit_proxy() -> Result<()> {
        let config = F10Config::defaults()?;
        assert_eq!(config.timeout(), Duration::from_secs(8));
        assert_eq!(config.max_response_bytes().get(), 16 * 1024 * 1024);
        assert_eq!(config.max_encoded_bytes().get(), 16 * 1024 * 1024);
        assert_eq!(config.max_header_bytes().get(), 64 * 1024);
        assert_eq!(config.max_redirects(), 0);
        assert!(config.prefer_ipv4());
        assert!(config.proxy().is_none());
        assert_eq!(config.tls(), &TlsPolicy::SystemRoots);
        assert_eq!(config.resolver(), &ResolverPolicy::Bounded);
        assert_eq!(config.headers()[0].name(), "Content-Type");
        assert_eq!(config.headers()[1].name(), "Accept");
        let request = config.request_url(&EntryName::new("name with +&中文")?);
        let parameters: Vec<_> = request.query_pairs().collect();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].0, "Entry");
        assert_eq!(parameters[0].1, "name with +&中文");
        Ok(())
    }

    #[test]
    fn shared_route_priority_is_lazy() -> Result<()> {
        let value = super::super::python_legacy::route_with(|| Ok::<_, Error>(true),
            || panic!("ladder must not inspect international prefixes"),
            || panic!("ladder must not inspect futures prefixes"))?;
        assert_eq!(value, Route::Ladder);
        let value = super::super::python_legacy::route_with(|| Ok::<_, Error>(false),
            || Ok(true), || panic!("international must not inspect futures prefixes"))?;
        assert_eq!(value, Route::International);
        assert_eq!(Route::for_entry(&EntryName::new("CWServ.qhf9_example")?), Route::International);
        Ok(())
    }
}
