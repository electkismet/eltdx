use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::deadline::Deadline;
use crate::error::{RuntimeError, TimeoutPhase};

const MAX_ENDPOINT_FAIR_SHARE_DIVISOR: usize = 8;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Endpoint {
    host: String,
    address: SocketAddr,
}

impl Endpoint {
    pub fn resolved(host: impl Into<String>, address: SocketAddr) -> Result<Self, RuntimeError> {
        let host = host.into();
        let normalized = host.trim();
        if normalized.is_empty() {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "endpoint host must not be empty",
            ));
        }
        if address.port() == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "endpoint port must be between 1 and 65535",
            ));
        }
        Ok(Self {
            host: normalized.to_owned(),
            address,
        })
    }

    pub fn numeric(host: &str) -> Result<Self, RuntimeError> {
        let address = host.trim().parse::<SocketAddr>().map_err(|_| {
            RuntimeError::invalid_argument(
                "ValueError",
                format!("numeric endpoint must be an IP address and port: {host:?}"),
            )
        })?;
        Self::resolved(host, address)
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub const fn address(&self) -> SocketAddr {
        self.address
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointAttempt {
    pub endpoint: Endpoint,
    pub endpoint_index: usize,
    pub endpoints_remaining: usize,
    pub deadline: Deadline,
}

#[derive(Clone, Debug)]
pub struct EndpointRotation {
    endpoints: Arc<[Endpoint]>,
    next_index: usize,
    remaining_in_attempt: usize,
}

impl EndpointRotation {
    pub fn new(endpoints: Vec<Endpoint>, start_index: usize) -> Result<Self, RuntimeError> {
        if endpoints.is_empty() {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "at least one resolved endpoint is required",
            ));
        }
        let next_index = start_index % endpoints.len();
        Ok(Self {
            endpoints: endpoints.into(),
            next_index,
            remaining_in_attempt: 0,
        })
    }

    pub fn from_numeric_hosts(
        hosts: impl IntoIterator<Item = String>,
        start_index: usize,
    ) -> Result<Self, RuntimeError> {
        let mut endpoints = Vec::new();
        for host in hosts {
            endpoints.push(Endpoint::numeric(&host)?);
        }
        Self::new(endpoints, start_index)
    }

    pub fn begin_attempt(&mut self) {
        self.remaining_in_attempt = self.endpoints.len();
    }

    pub fn next(
        &mut self,
        attempt_deadline: Deadline,
        now: Instant,
    ) -> Result<Option<EndpointAttempt>, RuntimeError> {
        if self.remaining_in_attempt == 0 {
            return Ok(None);
        }
        let endpoint_index = self.next_index;
        let endpoints_remaining = self.remaining_in_attempt;
        let fair_share_divisor = endpoints_remaining.min(MAX_ENDPOINT_FAIR_SHARE_DIVISOR);
        let deadline =
            attempt_deadline.fair_slice_at(now, fair_share_divisor, TimeoutPhase::Connect)?;
        let endpoint = self.endpoints[endpoint_index].clone();
        self.next_index = (endpoint_index + 1) % self.endpoints.len();
        self.remaining_in_attempt -= 1;
        Ok(Some(EndpointAttempt {
            endpoint,
            endpoint_index,
            endpoints_remaining,
            deadline,
        }))
    }

    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    pub const fn next_index(&self) -> usize {
        self.next_index
    }

    pub const fn remaining_in_attempt(&self) -> usize {
        self.remaining_in_attempt
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv6Addr, SocketAddr};
    use std::time::{Duration, Instant};

    use super::{Endpoint, EndpointRotation};
    use crate::deadline::Deadline;
    use crate::error::RuntimeError;

    fn endpoint(value: &str) -> Result<Endpoint, RuntimeError> {
        Endpoint::numeric(value)
    }

    #[test]
    fn numeric_endpoints_accept_ipv4_and_bracketed_ipv6() -> Result<(), RuntimeError> {
        let ipv4 = endpoint("127.0.0.1:7709")?;
        let ipv6 = endpoint("[::1]:7709")?;

        assert_eq!(ipv4.host(), "127.0.0.1:7709");
        assert_eq!(ipv6.address().ip(), IpAddr::V6(Ipv6Addr::LOCALHOST));
        Ok(())
    }

    #[test]
    fn hostname_requires_external_resolution() {
        assert!(matches!(
            Endpoint::numeric("quotes.example:7709"),
            Err(error) if error.kind() == "InvalidArgument"
        ));
    }

    #[test]
    fn resolved_endpoint_preserves_the_configured_host_identity() -> Result<(), RuntimeError> {
        let address = SocketAddr::from(([127, 0, 0, 1], 7709));
        let endpoint = Endpoint::resolved("quotes.example:7709", address)?;

        assert_eq!(endpoint.host(), "quotes.example:7709");
        assert_eq!(endpoint.address(), address);
        Ok(())
    }

    #[test]
    fn endpoint_budgets_redivide_the_attempts_remaining_time() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let attempt_deadline = Deadline::at(now + Duration::from_millis(900));
        let mut rotation = EndpointRotation::new(
            vec![
                endpoint("127.0.0.1:7709")?,
                endpoint("127.0.0.2:7709")?,
                endpoint("127.0.0.3:7709")?,
            ],
            0,
        )?;
        rotation.begin_attempt();

        let first = rotation.next(attempt_deadline, now)?.ok_or_else(|| {
            RuntimeError::internal("first endpoint is missing from a nonempty rotation")
        })?;
        let second_now = now + Duration::from_millis(100);
        let second = rotation
            .next(attempt_deadline, second_now)?
            .ok_or_else(|| RuntimeError::internal("second endpoint is missing"))?;

        assert_eq!(first.endpoint_index, 0);
        assert_eq!(first.deadline.instant(), now + Duration::from_millis(300));
        assert_eq!(second.endpoint_index, 1);
        assert_eq!(second.deadline.instant(), now + Duration::from_millis(500));
        Ok(())
    }

    #[test]
    fn large_ranked_pool_keeps_the_first_handshake_slice_practical() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let attempt_deadline = Deadline::at(now + Duration::from_secs(8));
        let endpoints = (1..=43)
            .map(|index| endpoint(&format!("127.0.0.{index}:7709")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut rotation = EndpointRotation::new(endpoints, 0)?;
        rotation.begin_attempt();

        let first = rotation
            .next(attempt_deadline, now)?
            .ok_or_else(|| RuntimeError::internal("first endpoint is missing"))?;

        assert_eq!(first.deadline.instant(), now + Duration::from_secs(1));
        assert_eq!(first.endpoints_remaining, 43);
        Ok(())
    }

    #[test]
    fn retry_starts_after_the_last_selected_endpoint() -> Result<(), RuntimeError> {
        let now = Instant::now();
        let attempt_deadline = Deadline::at(now + Duration::from_secs(3));
        let mut rotation = EndpointRotation::new(
            vec![
                endpoint("127.0.0.1:7709")?,
                endpoint("127.0.0.2:7709")?,
                endpoint("127.0.0.3:7709")?,
            ],
            1,
        )?;
        rotation.begin_attempt();
        let first = rotation
            .next(attempt_deadline, now)?
            .ok_or_else(|| RuntimeError::internal("first endpoint is missing"))?;
        rotation.begin_attempt();
        let retry = rotation
            .next(attempt_deadline, now)?
            .ok_or_else(|| RuntimeError::internal("retry endpoint is missing"))?;

        assert_eq!(first.endpoint_index, 1);
        assert_eq!(retry.endpoint_index, 2);
        assert_eq!(rotation.remaining_in_attempt(), 2);
        Ok(())
    }
}
