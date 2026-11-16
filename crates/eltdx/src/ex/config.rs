//! Explicit local configuration loading. Construction never touches environment,
//! disk, DNS or the network. Calling load_hosts is the explicit read boundary.
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::num::{NonZeroU32, NonZeroUsize};
use crate::{Error, ErrorKind, RawPolicy, Result};
use super::{hosts::{self, DEFAULT_EX_PORT, EX_HOSTS_ENV}, frame::FrameLimits, invalid};

#[derive(Clone, Debug)]
pub struct ExConfig {
    pub(crate) endpoints: Vec<SocketAddr>, pub(crate) timeout: Duration,
    pub(crate) raw: RawPolicy, pub(crate) auto_failover: bool, pub(crate) frame_limits: FrameLimits,
    pub(crate) close_timeout: Duration, pub(crate) cleanup_timeout: Duration,
    pub(crate) wire_attempt_limit: NonZeroU32, pub(crate) extra_attempt_budget: usize,
    pub(crate) max_operation_bytes: NonZeroUsize,
}
impl Default for ExConfig {
    fn default() -> Self { Self {
        endpoints: hosts::default_endpoints(), timeout: Duration::from_secs(8), raw: RawPolicy::Omit,
        auto_failover: true, frame_limits: FrameLimits::default(),
        close_timeout: Duration::from_secs(1), cleanup_timeout: Duration::from_secs(1),
        wire_attempt_limit: NonZeroU32::new(2).unwrap_or(NonZeroU32::MIN), extra_attempt_budget: 32,
        max_operation_bytes: NonZeroUsize::new(64 * 1024 * 1024).unwrap_or(NonZeroUsize::MIN),
    } }
}
impl ExConfig {
    pub fn endpoints(&self) -> &[SocketAddr] { &self.endpoints }
    pub const fn timeout(&self) -> Duration { self.timeout }
    pub const fn raw(&self) -> RawPolicy { self.raw }
    pub const fn auto_failover(&self) -> bool { self.auto_failover }
    pub const fn frame_limits(&self) -> FrameLimits { self.frame_limits }
    pub const fn close_timeout(&self) -> Duration { self.close_timeout }
    pub const fn cleanup_timeout(&self) -> Duration { self.cleanup_timeout }
    pub const fn wire_attempt_limit(&self) -> NonZeroU32 { self.wire_attempt_limit }
    pub const fn extra_attempt_budget(&self) -> usize { self.extra_attempt_budget }
    pub const fn max_operation_bytes(&self) -> NonZeroUsize { self.max_operation_bytes }
    pub fn with_endpoints(mut self, value: Vec<SocketAddr>) -> Result<Self> { self.endpoints = validate_endpoints(value)?; Ok(self) }
    pub fn with_timeout(mut self, value: Duration) -> Result<Self> {
        if value.is_zero() { return Err(invalid("EX timeout must be positive")); } self.timeout = value; Ok(self)
    }
    pub fn with_raw(mut self, value: RawPolicy) -> Self { self.raw = value; self }
    pub fn with_auto_failover(mut self, value: bool) -> Self { self.auto_failover = value; self }
    pub fn with_frame_limits(mut self, value: FrameLimits) -> Self { self.frame_limits = value; self }
}
#[derive(Clone, Debug)]
pub enum EnvironmentSource {
    /// Do not read process environment.
    Disabled,
    /// Caller supplied snapshot, including absence.
    Value(Option<String>),
    /// Explicit opt-in to reading ELTDX_EX_HOSTS at load_hosts call time.
    Process,
}
#[derive(Clone, Debug)]
pub struct HostConfigSource { explicit: Option<Vec<SocketAddr>>, environment: EnvironmentSource, file: Option<PathBuf>, port: u16 }
impl Default for HostConfigSource {
    fn default() -> Self { Self { explicit: None, environment: EnvironmentSource::Disabled, file: None, port: DEFAULT_EX_PORT } }
}
impl HostConfigSource {
    pub fn explicit(&self) -> Option<&[SocketAddr]> { self.explicit.as_deref() }
    pub fn environment(&self) -> &EnvironmentSource { &self.environment }
    pub fn file(&self) -> Option<&Path> { self.file.as_deref() }
    pub const fn port(&self) -> u16 { self.port }
    pub fn with_explicit(mut self, value: Vec<SocketAddr>) -> Result<Self> { self.explicit = Some(validate_endpoints(value)?); Ok(self) }
    pub fn with_environment(mut self, value: EnvironmentSource) -> Self { self.environment = value; self }
    pub fn with_file(mut self, value: impl Into<PathBuf>) -> Self { self.file = Some(value.into()); self }
    pub fn with_port(mut self, value: u16) -> Result<Self> {
        if value == 0 { return Err(invalid("EX port must be nonzero")); } self.port = value; Ok(self)
    }
}
pub fn server_file_path(data_directory: impl AsRef<Path>) -> PathBuf { data_directory.as_ref().join(hosts::EX_SERVER_FILE) }
fn validate_endpoints(values: Vec<SocketAddr>) -> Result<Vec<SocketAddr>> {
    if values.is_empty() || values.iter().any(|value| value.port() == 0) { return Err(invalid("EX endpoints must be nonempty with nonzero ports")); }
    let mut endpoints = Vec::with_capacity(values.len());
    for endpoint in values { if !endpoints.contains(&endpoint) { endpoints.push(endpoint); } }
    Ok(endpoints)
}
fn io_error(error: std::io::Error) -> Error { Error::new(ErrorKind::Io, "EX config I/O failed").with_context("domain", "ex").with_source(error) }
fn config_error(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "ex").with_context("stage", "config") }
pub fn parse_hosts_json(text: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| config_error("invalid EX config JSON").with_source(error))?;
    let object = value.as_object().ok_or_else(|| config_error("EX config must be an object"))?;
    let values = object.get("hosts").and_then(serde_json::Value::as_array).ok_or_else(|| config_error("EX config requires hosts array"))?;
    let strings: Vec<&str> = values.iter().map(|value| value.as_str().ok_or_else(|| config_error("EX host must be a string"))).collect::<Result<_>>()?;
    hosts::parse_hosts(strings, port)
}
pub fn load_hosts(source: &HostConfigSource) -> Result<Vec<SocketAddr>> {
    if let Some(values) = &source.explicit { return validate_endpoints(values.clone()); }
    let process = if matches!(&source.environment, EnvironmentSource::Process) {
        match std::env::var(EX_HOSTS_ENV) {
            Ok(value) => Some(value), Err(std::env::VarError::NotPresent) => None,
            Err(error) => return Err(config_error("EX environment value is not Unicode").with_source(error)),
        }
    } else { None };
    let value = match &source.environment {
        EnvironmentSource::Disabled => None,
        EnvironmentSource::Value(value) => value.as_deref(),
        EnvironmentSource::Process => process.as_deref(),
    };
    if let Some(value) = value {
        // A present but empty configured source is invalid, not a request to
        // silently fall through to a lower-priority source.
        return hosts::parse_hosts(value.split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace()).filter(|part| !part.is_empty()), source.port);
    }
    if let Some(path) = &source.file {
        match fs::read_to_string(path) {
            Ok(text) => return parse_hosts_json(&text, source.port),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(io_error(error)),
        }
    }
    hosts::parse_hosts(hosts::DEFAULT_EX_HOSTS.iter().copied(), source.port)
}
static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
/// Explicit atomic replacement using a unique sibling file. Parent creation and
/// mutation happen only here; source loading never writes. No shell is used.
pub fn save_hosts(path: &Path, endpoints: &[SocketAddr]) -> Result<()> {
    let endpoints = validate_endpoints(endpoints.to_vec())?;
    let port = endpoints[0].port();
    if port != DEFAULT_EX_PORT || endpoints.iter().any(|endpoint| endpoint.port() != DEFAULT_EX_PORT) {
        return Err(invalid("legacy EX hosts file stores bare hosts only; saving requires port 7727"));
    }
    // Legacy file stores bare hosts, so the caller must load with the same port.
    let values: Vec<String> = endpoints.iter().map(|endpoint| match endpoint.ip() {
        std::net::IpAddr::V4(ip) => format!("{ip}"), std::net::IpAddr::V6(ip) => format!("[{ip}]"),
    }).collect();
    let mut bytes = serde_json::to_vec_pretty(&serde_json::json!({"schema_version": 1, "hosts": values}))
        .map_err(|error| config_error("cannot encode EX hosts JSON").with_source(error))?;
    bytes.push(b'\n');
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| invalid("EX config path requires a file name"))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let suffix = SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{}.{}.{}.tmp", name.to_str().unwrap_or("ex-servers"), std::process::id(), suffix));
    let mut created = false;
    let result = (|| {
        let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(io_error)?;
        created = true;
        output.write_all(&bytes).map_err(io_error)?; output.sync_all().map_err(io_error)?; drop(output);
        fs::rename(&temporary, path).map_err(io_error)
    })();
    // Only remove the file this invocation created; create_new failure must not
    // delete another writer's sibling temporary file.
    if result.is_err() && created { let _ = fs::remove_file(&temporary); }
    result
}
