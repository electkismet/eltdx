use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use hickory_resolver::config::{
    LookupIpStrategy, NameServerConfig, Protocol, ResolverConfig, ResolverOpts,
};
use hickory_resolver::TokioAsyncResolver;

use super::control::{connection, Control};
use super::ResolverPolicy;
use crate::{Error, ErrorKind, Result, TimeoutPhase};

/// Async DNS runs entirely inside the operation's owned Tokio runtime. Dropping
/// that runtime drops outstanding UDP/TCP DNS work; no blocking getaddrinfo or
/// detached DNS thread exists on this path.
pub(crate) async fn addresses(
    host: &str,
    port: u16,
    prefer_ipv4: bool,
    policy: &ResolverPolicy,
    control: &Control,
) -> Result<Vec<SocketAddr>> {
    control.check(TimeoutPhase::Resolve)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let (config, mut options) = match policy {
        ResolverPolicy::Bounded => hickory_resolver::system_conf::read_system_conf()
            .map_err(|error| connection(error, "cannot read system DNS configuration"))?,
        ResolverPolicy::Nameservers(servers) => {
            let mut config = ResolverConfig::new();
            for &address in servers {
                config.add_name_server(NameServerConfig::new(address, Protocol::Udp));
            }
            (config, ResolverOpts::default())
        }
    };
    control.check(TimeoutPhase::Resolve)?;
    options.timeout = Duration::from_secs(2).min(
        control
            .deadline
            .saturating_duration_since(std::time::Instant::now()),
    );
    options.attempts = 1;
    options.cache_size = 0;
    options.ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    let resolver = TokioAsyncResolver::tokio(config, options);
    let result = control
        .wait(TimeoutPhase::Resolve, async {
            resolver
                .lookup_ip(host)
                .await
                .map_err(|error| connection(error, "F10 DNS lookup failed"))
        })
        .await?;
    let mut addresses = Vec::new();
    for ip in result.iter() {
        control.check(TimeoutPhase::Resolve)?;
        let address = SocketAddr::new(ip, port);
        if !addresses.contains(&address) {
            if addresses.len() >= 64 {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    "F10 DNS returned too many addresses",
                ));
            }
            addresses.push(address);
        }
    }
    if prefer_ipv4 {
        addresses.sort_by_key(|address| address.is_ipv6());
    }
    if addresses.is_empty() {
        return Err(Error::new(
            ErrorKind::Connection,
            "F10 DNS lookup returned no addresses",
        ));
    }
    Ok(addresses)
}
