use super::SessionService;
use crate::config::ClientConfig;
use crate::hq::executor::HqExecutor;
use crate::operation::OperationOptions;
use crate::types::{HealthReport, PingReply};
use crate::{Error, ErrorKind, Result};
use eltdx_runtime::engine::Engine;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct HealthService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
}
impl HealthService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig) -> Self {
        Self {
            executor,
            engine,
            config,
        }
    }
    pub fn ping(&self) -> Result<PingReply> {
        Ok(PingReply)
    }
    /// Measure this explicit heartbeat operation, including lazy admission and
    /// startup. A successful result describes its endpoint, not every pool slot.
    pub fn probe(&self, options: &OperationOptions) -> Result<HealthReport> {
        let entry = Instant::now();
        let context =
            self.executor
                .begin_context_at(entry, "health_probe", &self.config, options)?;
        let heartbeat = SessionService::new(
            self.executor.clone(),
            self.engine.clone(),
            self.config.clone(),
        )
        .heartbeat_in_context(&context)?;
        let endpoint = heartbeat.meta().source().endpoint().ok_or_else(|| {
            Error::new(
                ErrorKind::Protocol,
                "heartbeat source endpoint is unavailable",
            )
        })?;
        let meta = heartbeat.meta().clone();
        context.finish()?;
        Ok(HealthReport {
            heartbeat,
            elapsed: entry.elapsed(),
            endpoint,
            meta,
        })
    }
}
