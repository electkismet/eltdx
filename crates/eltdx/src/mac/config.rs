//! Local-only MAC configuration. Shared transport limits are the same checked
//! values used by EX; endpoints and immutable setup profiles remain separate.
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::{RawPolicy, Result};
use crate::ex::{config::ExConfig, frame::FrameLimits};
use super::handshake::{LoginReplyRules, MacSetupPolicy};
use super::services::XdxrProvider;

pub const MAC_PORT: u16 = 7709;
pub const MAC_EX_PORT: u16 = 7727;
pub const MAC_HOSTS: &[&str] = &["121.36.248.138","123.60.47.136","121.37.207.165"];
pub const MAC_EX_HOSTS: &[&str] = &["116.205.135.205","121.37.232.167"];
pub fn default_endpoints() -> Vec<SocketAddr> {
    [[121,36,248,138],[123,60,47,136],[121,37,207,165]].into_iter()
        .map(|address| SocketAddr::from((address,MAC_PORT))).collect()
}
pub fn default_ex_endpoints() -> Vec<SocketAddr> {
    [[116,205,135,205],[121,37,232,167]].into_iter()
        .map(|address| SocketAddr::from((address,MAC_EX_PORT))).collect()
}
#[derive(Clone, Debug)]
pub struct MacConfig { pub(crate) transport: ExConfig, pub(crate) setup: MacSetupPolicy, pub(crate) xdxr_provider: Option<Arc<dyn XdxrProvider>> }
impl Default for MacConfig {
    fn default() -> Self {
        let mut transport = ExConfig::default(); transport.endpoints = default_endpoints();
        Self { transport, setup: MacSetupPolicy::default(), xdxr_provider: None }
    }
}
#[derive(Clone, Debug)]
pub struct MacExConfig { pub(crate) transport: ExConfig, pub(crate) login: Option<LoginReplyRules>, pub(crate) xdxr_provider: Option<Arc<dyn XdxrProvider>> }
impl Default for MacExConfig {
    fn default() -> Self {
        let mut transport = ExConfig::default(); transport.endpoints = default_ex_endpoints();
        Self { transport, login: None, xdxr_provider: None }
    }
}
macro_rules! shared_config {
    ($($name:ident),+ $(,)?) => { $(
        impl $name {
            pub fn new() -> Self { Self::default() }
            pub fn endpoints(&self) -> &[SocketAddr] { self.transport.endpoints() }
            pub const fn timeout(&self) -> Duration { self.transport.timeout() }
            pub const fn raw(&self) -> RawPolicy { self.transport.raw() }
            pub const fn auto_failover(&self) -> bool { self.transport.auto_failover() }
            pub const fn frame_limits(&self) -> FrameLimits { self.transport.frame_limits() }
            pub const fn close_timeout(&self) -> Duration { self.transport.close_timeout() }
            pub const fn cleanup_timeout(&self) -> Duration { self.transport.cleanup_timeout() }
            pub const fn wire_attempt_limit(&self) -> NonZeroU32 { self.transport.wire_attempt_limit() }
            pub const fn extra_attempt_budget(&self) -> usize { self.transport.extra_attempt_budget() }
            pub const fn max_operation_bytes(&self) -> NonZeroUsize { self.transport.max_operation_bytes() }
            pub fn with_endpoints(mut self, value: Vec<SocketAddr>) -> Result<Self> { self.transport = self.transport.with_endpoints(value)?; Ok(self) }
            pub fn with_timeout(mut self, value: Duration) -> Result<Self> { self.transport = self.transport.with_timeout(value)?; Ok(self) }
            pub fn with_raw(mut self, value: RawPolicy) -> Self { self.transport = self.transport.with_raw(value); self }
            pub fn with_auto_failover(mut self, value: bool) -> Self { self.transport = self.transport.with_auto_failover(value); self }
            pub fn with_frame_limits(mut self, value: FrameLimits) -> Self { self.transport = self.transport.with_frame_limits(value); self }
            pub fn with_close_timeout(mut self, value: Duration) -> Result<Self> { duration(value)?; self.transport.close_timeout = value; Ok(self) }
            pub fn with_cleanup_timeout(mut self, value: Duration) -> Result<Self> { duration(value)?; self.transport.cleanup_timeout = value; Ok(self) }
            pub fn with_wire_attempt_limit(mut self, value: NonZeroU32) -> Self { self.transport.wire_attempt_limit = value; self }
            pub fn with_extra_attempt_budget(mut self, value: usize) -> Self { self.transport.extra_attempt_budget = value; self }
            pub fn with_max_operation_bytes(mut self, value: NonZeroUsize) -> Self { self.transport.max_operation_bytes = value; self }
            pub fn xdxr_provider(&self) -> Option<&dyn XdxrProvider> { self.xdxr_provider.as_deref() }
            pub fn with_xdxr_provider(mut self, value: Arc<dyn XdxrProvider>) -> Self { self.xdxr_provider = Some(value); self }
            pub fn without_xdxr_provider(mut self) -> Self { self.xdxr_provider = None; self }
            pub(crate) fn validate(mut self) -> Result<Self> {
                self.transport = self.transport.clone().with_endpoints(self.transport.endpoints().to_vec())?;
                for value in [self.timeout(),self.close_timeout(),self.cleanup_timeout()] { duration(value)?; }
                Ok(self)
            }
        }
    )+ };
}
shared_config!(MacConfig, MacExConfig);
impl MacConfig {
    pub fn setup_policy(&self) -> &MacSetupPolicy { &self.setup }
    pub fn with_setup_policy(mut self, value: MacSetupPolicy) -> Self { self.setup = value; self }
}
impl MacExConfig {
    /// No production positive/negative response capture is bundled yet.
    /// Missing evidence never becomes successful authentication by byte length.
    pub fn login_reply_rules(&self) -> Option<&LoginReplyRules> { self.login.as_ref() }
    pub fn with_login_reply_rules(mut self, value: LoginReplyRules) -> Self { self.login = Some(value); self }
}
fn duration(value: Duration) -> Result<()> {
    if value.is_zero() || Instant::now().checked_add(value).is_none() {
        return Err(super::query::invalid("MAC timeout must be positive and fit the monotonic clock"));
    }
    Ok(())
}
