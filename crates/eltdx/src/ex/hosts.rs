//! Source snapshots, not live health evidence. No network work in this module.
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use std::num::NonZeroUsize;
use crate::Result;
use super::invalid;

pub const DEFAULT_EX_PORT: u16 = 7727;
/// Numeric socket endpoint; builders never resolve hostnames implicitly.
pub type Endpoint = std::net::SocketAddr;
pub const EX_HOSTS_ENV: &str = "ELTDX_EX_HOSTS";
pub const EX_SERVER_FILE: &str = "ex_servers.json";
pub const KNOWN_EX_HOSTS: &[&str] = &[
    "112.74.214.43","120.25.218.6","43.139.173.246","159.75.90.107",
    "106.52.170.195","139.9.191.175","175.24.47.69","150.158.9.199",
    "150.158.20.127","49.235.119.116","49.234.13.160","116.205.143.214",
    "124.71.223.19","113.45.175.47","123.60.173.210","118.89.69.202",
];
pub const PROBE_VERIFIED_EX_HOSTS: &[&str] = &["150.158.20.127"];
pub const DEFAULT_EX_HOSTS: &[&str] = &[
    "150.158.20.127","112.74.214.43","120.25.218.6","43.139.173.246",
    "159.75.90.107","106.52.170.195","139.9.191.175","175.24.47.69",
    "150.158.9.199","49.235.119.116","49.234.13.160","116.205.143.214",
    "124.71.223.19","113.45.175.47","123.60.173.210","118.89.69.202",
];
pub const MAC_EX_HOSTS: &[&str] = &["116.205.135.205","121.37.232.167"];

pub fn default_endpoints() -> Vec<SocketAddr> {
    // Numeric bytes mirror DEFAULT_EX_HOSTS in order; no parser, DNS or silent
    // filter can remove a configured default candidate.
    [
        [150,158,20,127],[112,74,214,43],[120,25,218,6],[43,139,173,246],
        [159,75,90,107],[106,52,170,195],[139,9,191,175],[175,24,47,69],
        [150,158,9,199],[49,235,119,116],[49,234,13,160],[116,205,143,214],
        [124,71,223,19],[113,45,175,47],[123,60,173,210],[118,89,69,202],
    ].into_iter().map(|address| SocketAddr::from((address, DEFAULT_EX_PORT))).collect()
}
/// Config hosts are bare numeric addresses; port is supplied separately. Unlike
/// the legacy Python hostname path, Rust has no implicit unbounded resolver.
pub fn parse_hosts<'a>(values: impl IntoIterator<Item = &'a str>, port: u16) -> Result<Vec<SocketAddr>> {
    if port == 0 { return Err(invalid("EX port must be nonzero")); }
    let mut endpoints = Vec::new();
    for value in values {
        let host = value.trim();
        let address = if host.starts_with('[') && host.ends_with(']') {
            host[1..host.len()-1].parse::<IpAddr>()
        } else { host.parse::<IpAddr>() }.map_err(|_| invalid("EX host must be a bare numeric IP; configure a bounded resolver separately"))?;
        let endpoint = SocketAddr::new(address, port);
        if !endpoints.contains(&endpoint) { endpoints.push(endpoint); }
    }
    if endpoints.is_empty() { return Err(invalid("configured EX host list is empty")); }
    Ok(endpoints)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeMode { TcpOnly, DataReady }
#[derive(Clone, Copy, Debug)]
pub struct ProbeOptions { timeout: Duration, mode: ProbeMode }
impl Default for ProbeOptions { fn default() -> Self { Self { timeout: Duration::from_secs(5), mode: ProbeMode::DataReady } } }
impl ProbeOptions {
    pub const fn timeout(self) -> Duration { self.timeout }
    pub const fn mode(self) -> ProbeMode { self.mode }
    pub fn with_timeout(mut self, value: Duration) -> Result<Self> {
        if value.is_zero() { return Err(invalid("EX probe timeout must be positive")); } self.timeout = value; Ok(self)
    }
    pub fn with_mode(mut self, value: ProbeMode) -> Self { self.mode = value; self }
}
#[derive(Clone, Copy, Debug)]
pub struct DiscoveryOptions { max_workers: NonZeroUsize, probe: ProbeOptions }
impl Default for DiscoveryOptions {
    fn default() -> Self { Self { max_workers: NonZeroUsize::new(8).unwrap_or(NonZeroUsize::MIN), probe: ProbeOptions::default() } }
}
impl DiscoveryOptions {
    pub const fn max_workers(self) -> NonZeroUsize { self.max_workers }
    pub const fn probe(self) -> ProbeOptions { self.probe }
    pub fn with_max_workers(mut self, value: NonZeroUsize) -> Result<Self> {
        if value.get() > 8 { return Err(invalid("EX discovery max_workers exceeds 8")); } self.max_workers = value; Ok(self)
    }
    pub fn with_probe(mut self, value: ProbeOptions) -> Self { self.probe = value; self }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeStatus { Ready, Failed }
#[derive(Clone, Debug)]
pub struct HostProbe { endpoint: SocketAddr, latency: Option<Duration>, status: ProbeStatus, error_kind: Option<crate::ErrorKind>, mode: ProbeMode }
impl HostProbe {
    /// A successful DataReady result may be constructed only after the selected
    /// profile handshake and markets response complete in Stage B.
    pub(crate) fn new(endpoint: SocketAddr, latency: Option<Duration>, status: ProbeStatus, error_kind: Option<crate::ErrorKind>) -> Self {
        Self { endpoint, latency, status, error_kind, mode: ProbeMode::DataReady }
    }
    pub const fn endpoint(&self) -> SocketAddr { self.endpoint }
    pub const fn latency(&self) -> Option<Duration> { self.latency }
    pub const fn status(&self) -> ProbeStatus { self.status }
    pub const fn error_kind(&self) -> Option<crate::ErrorKind> { self.error_kind }
    pub const fn mode(&self) -> ProbeMode { self.mode }
    pub const fn data_ready(&self) -> bool { matches!(self.mode, ProbeMode::DataReady) && matches!(self.status, ProbeStatus::Ready) }
    pub(crate) fn with_mode(mut self, mode: ProbeMode) -> Self { self.mode = mode; self }
}
#[derive(Clone, Debug)]
pub struct DiagnosticSnapshot { active_endpoint: Option<SocketAddr>, candidates: Vec<SocketAddr>, probes: Vec<HostProbe>, latest_meta: Option<crate::types::ResultMeta> }
impl DiagnosticSnapshot {
    pub(crate) fn new(active_endpoint: Option<SocketAddr>, candidates: Vec<SocketAddr>, probes: Vec<HostProbe>) -> Self {
        Self { active_endpoint, candidates, probes, latest_meta: None }
    }
    pub const fn active_endpoint(&self) -> Option<SocketAddr> { self.active_endpoint }
    pub fn candidates(&self) -> &[SocketAddr] { &self.candidates }
    pub fn probes(&self) -> &[HostProbe] { &self.probes }
    pub fn latest_meta(&self) -> Option<&crate::types::ResultMeta> { self.latest_meta.as_ref() }
    pub(crate) fn with_latest_meta(mut self, meta: Option<crate::types::ResultMeta>) -> Self { self.latest_meta = meta; self }
}
/// Stable latency ordering; failed/unprobed candidates remain in original order.
/// This is pure ranking and cannot promote unprobed endpoints to Ready.
pub fn rank_endpoints(candidates: &[SocketAddr], probes: &[HostProbe]) -> Vec<SocketAddr> {
    let mut ready: Vec<&HostProbe> = probes.iter().filter(|probe|
        probe.data_ready() && probe.latency.is_some() && candidates.contains(&probe.endpoint)).collect();
    ready.sort_by_key(|probe| probe.latency);
    let mut result = Vec::with_capacity(candidates.len());
    for probe in ready { if !result.contains(&probe.endpoint) { result.push(probe.endpoint); } }
    for &endpoint in candidates { if !result.contains(&endpoint) { result.push(endpoint); } }
    result
}

#[cfg(test)]
mod source_cases {
    use super::*;
    #[test]
    fn ranking_is_stable_and_does_not_promote_failed_or_unprobed_candidates() -> Result<()> {
        let candidates = parse_hosts(["127.0.0.1","127.0.0.2","127.0.0.3","127.0.0.4"],7727)?;
        let probes = vec![
            HostProbe::new(candidates[1],Some(Duration::from_millis(9)),ProbeStatus::Ready,None),
            HostProbe::new(candidates[2],Some(Duration::from_millis(9)),ProbeStatus::Ready,None),
            HostProbe::new(candidates[0],None,ProbeStatus::Failed,Some(crate::ErrorKind::Connection)),
        ];
        assert_eq!(rank_endpoints(&candidates,&probes),vec![candidates[1],candidates[2],candidates[0],candidates[3]]);
        Ok(())
    }
}
