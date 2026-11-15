use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::{ClientConfig, ConfigDraft};
use crate::error::Result;

#[derive(Clone, Debug)]
pub struct ClientBuilder {
    draft: ConfigDraft,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self {
            draft: ConfigDraft::default(),
        }
    }

    pub fn pool_size(mut self, count: usize) -> Self {
        self.draft.pool_size = count;
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.draft.request_timeout = timeout;
        self
    }

    pub fn close_timeout(mut self, timeout: Duration) -> Self {
        self.draft.close_timeout = timeout;
        self
    }

    pub fn cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.draft.cleanup_timeout = timeout;
        self
    }

    pub fn heartbeat_interval(mut self, interval: Option<Duration>) -> Self {
        self.draft.heartbeat_interval = interval;
        self
    }

    pub fn endpoints(mut self, endpoints: Vec<SocketAddr>) -> Self {
        self.draft.endpoints = Some(endpoints);
        self
    }

    pub fn wire_attempt_limit(mut self, attempts: NonZeroU32) -> Self {
        self.draft.wire_attempt_limit = attempts;
        self
    }

    pub fn extra_attempt_budget(mut self, attempts: usize) -> Self {
        self.draft.extra_attempt_budget = attempts;
        self
    }

    pub fn build(self) -> Result<TdxClient> {
        let config = self.draft.validate()?;
        Ok(TdxClient {
            config,
            owner: Arc::new(ClientOwner {
                closed: AtomicBool::new(false),
            }),
        })
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct TdxClient {
    config: ClientConfig,
    owner: Arc<ClientOwner>,
}

#[derive(Debug)]
struct ClientOwner {
    closed: AtomicBool,
}

impl TdxClient {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Permanently closes this shared, lazily initialized client owner.
    pub fn close(&self) -> Result<()> {
        self.owner.closed.store(true, Ordering::Release);
        Ok(())
    }
}
