use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub(crate) clock: crate::types::time::ClockHandle,
    pub(crate) pool_size: usize,
    pub(crate) request_timeout: Duration,
    pub(crate) close_timeout: Duration,
    pub(crate) cleanup_timeout: Duration,
    pub(crate) heartbeat_interval: Option<Duration>,
    pub(crate) endpoints: Option<Vec<SocketAddr>>,
    pub(crate) wire_attempt_limit: NonZeroU32,
    pub(crate) extra_attempt_budget: usize,
}

impl ClientConfig {
    pub fn clock(&self) -> &dyn crate::types::Clock { self.clock.0.as_ref() }
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub fn close_timeout(&self) -> Duration {
        self.close_timeout
    }

    pub fn cleanup_timeout(&self) -> Duration {
        self.cleanup_timeout
    }

    pub fn heartbeat_interval(&self) -> Option<Duration> {
        self.heartbeat_interval
    }

    pub fn endpoints(&self) -> Option<&[SocketAddr]> {
        self.endpoints.as_deref()
    }

    pub fn wire_attempt_limit(&self) -> NonZeroU32 {
        self.wire_attempt_limit
    }

    pub fn extra_attempt_budget(&self) -> usize {
        self.extra_attempt_budget
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConfigDraft {
    pub(crate) clock: crate::types::time::ClockHandle,
    pub(crate) pool_size: usize,
    pub(crate) request_timeout: Duration,
    pub(crate) close_timeout: Duration,
    pub(crate) cleanup_timeout: Duration,
    pub(crate) heartbeat_interval: Option<Duration>,
    pub(crate) endpoints: Option<Vec<SocketAddr>>,
    pub(crate) wire_attempt_limit: NonZeroU32,
    pub(crate) extra_attempt_budget: usize,
}

impl Default for ConfigDraft {
    fn default() -> Self {
        Self {
            clock: crate::types::time::ClockHandle::default(),
            pool_size: 1,
            request_timeout: Duration::from_secs(8),
            close_timeout: Duration::from_secs(1),
            cleanup_timeout: Duration::from_secs(1),
            heartbeat_interval: Some(Duration::from_secs(30)),
            endpoints: None,
            wire_attempt_limit: NonZeroU32::new(2).unwrap_or(NonZeroU32::MIN),
            extra_attempt_budget: 32,
        }
    }
}

impl ConfigDraft {
    pub(crate) fn validate(self) -> Result<ClientConfig> {
        if self.pool_size == 0 {
            return Err(invalid("pool_size", "must be greater than zero"));
        }
        for (name, duration) in [
            ("request_timeout", self.request_timeout),
            ("close_timeout", self.close_timeout),
            ("cleanup_timeout", self.cleanup_timeout),
        ] {
            if duration.is_zero() {
                return Err(invalid(name, "must be greater than zero"));
            }
        }
        if self.heartbeat_interval.is_some_and(|interval| interval.is_zero()) {
            return Err(invalid(
                "heartbeat_interval",
                "must be greater than zero when enabled",
            ));
        }
        if let Some(endpoints) = &self.endpoints {
            if endpoints.is_empty() {
                return Err(invalid("endpoints", "must contain at least one socket address"));
            }
            if endpoints.iter().any(|endpoint| endpoint.port() == 0) {
                return Err(invalid("endpoints", "socket ports must be greater than zero"));
            }
        }

        Ok(ClientConfig {
            clock: self.clock,
            pool_size: self.pool_size,
            request_timeout: self.request_timeout,
            close_timeout: self.close_timeout,
            cleanup_timeout: self.cleanup_timeout,
            heartbeat_interval: self.heartbeat_interval,
            endpoints: self.endpoints,
            wire_attempt_limit: self.wire_attempt_limit,
            extra_attempt_budget: self.extra_attempt_budget,
        })
    }
}

fn invalid(field: &str, message: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("field", field)
}
