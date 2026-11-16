//! Lazy domain pools share owner policy, but never their wire connections.
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use eltdx_runtime::engine::Engine;
use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::executor::HqExecutor;
use crate::operation::OperationContext;

#[derive(Debug, Default)]
pub(crate) struct DomainPools {
    auction: Mutex<Option<Engine>>,
    money_flow: Mutex<Option<Engine>>,
    // Empty probing closes its endpoint-bound temporary engine before reuse.
    // Serial admission makes that close exclusive across concurrent operations.
    money_flow_operation: Mutex<()>,
}

impl DomainPools {
    pub(crate) fn auction(&self, config: &ClientConfig) -> Result<Engine> {
        lazy_engine(&self.auction, config, auction_hosts(config))
    }
    pub(crate) fn money_flow(&self, config: &ClientConfig) -> Result<Engine> {
        lazy_engine(&self.money_flow, config, money_flow_hosts(config))
    }
    pub(crate) fn money_flow_admission(&self, context: &OperationContext) -> Result<MutexGuard<'_, ()>> {
        loop {
            context.check_boundary(TimeoutPhase::Queue)?;
            match self.money_flow_operation.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(_)) => return Err(poisoned()),
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }
    pub(crate) fn money_probe(&self, executor: &HqExecutor, base: &Engine,
        config: &ClientConfig, endpoint: &str) -> Result<Engine> {
        if !money_flow_hosts(config).iter().any(|host| host == endpoint) {
            return Err(Error::new(ErrorKind::InvalidArgument, "money-flow probe endpoint is outside its catalog"));
        }
        executor.owner().endpoint_engine(base, endpoint, || {
            let mut single = config.clone(); single.pool_size = 1;
            crate::client::engine_with_hosts(&single, vec![endpoint.to_owned()])
        })
    }
}

fn lazy_engine(slot: &Mutex<Option<Engine>>, config: &ClientConfig, hosts: Vec<String>) -> Result<Engine> {
    let mut slot = slot.lock().map_err(|_| poisoned())?;
    if let Some(engine) = slot.as_ref() { return Ok(engine.clone()); }
    // Engine::new constructs memory only; runtime starts after normal owner registration.
    let engine = crate::client::engine_with_hosts(config, hosts)?;
    *slot = Some(engine.clone());
    Ok(engine)
}

pub(crate) fn auction_hosts(config: &ClientConfig) -> Vec<String> {
    config.auction_endpoints().map(|values| values.iter().map(ToString::to_string).collect())
        .unwrap_or_else(default_hosts)
}
pub(crate) fn money_flow_hosts(config: &ClientConfig) -> Vec<String> {
    config.money_flow_endpoints().map(|values| values.iter().map(ToString::to_string).collect())
        .unwrap_or_else(default_hosts)
}
fn default_hosts() -> Vec<String> {
    crate::hosts::DEDICATED_7709_HOSTS.iter().map(|value| (*value).to_owned()).collect()
}
fn poisoned() -> Error { Error::new(ErrorKind::Internal, "domain pool state is poisoned") }

/// Called while money_flow_admission is held. Only confirmed retirement permits
/// a later call to restart this cached endpoint Engine. Failed retirement stays
/// registered with the owner and is returned as an error.
pub(crate) fn retire_probe<T>(executor: &HqExecutor, context: &OperationContext,
    config: &ClientConfig, engine: &Engine, result: Result<T>) -> Result<T> {
    // Recoverable source errors retire under the original operation deadline.
    let mut root = context.check_boundary(TimeoutPhase::Close).err();
    let deadline = if root.is_some() {
        context.ensure_cleanup_budget()?;
        Instant::now().checked_add(context.cleanup_remaining()?)
            .ok_or_else(|| Error::new(ErrorKind::Internal, "probe retirement deadline overflows"))?
    } else {
        match context.sponsor().deadline() {
            Some(deadline) => deadline,
            None => Instant::now().checked_add(config.close_timeout())
                .ok_or_else(|| Error::new(ErrorKind::Internal, "probe retirement deadline overflows"))?,
        }
    };
    let deadline = executor.owner().shutdown_deadline()?.map_or(deadline, |owner| owner.min(deadline));
    let mut pending = match engine.begin_close_until(deadline) {
        Ok(pending) => pending,
        Err(error) => return Err(retirement_failure(root.or_else(|| result.as_ref().err().cloned()), Error::from_runtime(error))),
    };
    loop {
        if root.is_none() { root = context.check_boundary(TimeoutPhase::Close).err(); }
        if root.is_some() { context.ensure_cleanup_budget()?; }
        let wait = if root.is_some() { context.cleanup_remaining()?.min(eltdx_runtime::engine::SIGNAL_POLL_INTERVAL) }
            else { eltdx_runtime::engine::SIGNAL_POLL_INTERVAL };
        match pending.wait_timeout(wait) {
            Ok(eltdx_runtime::engine::PendingPoll::Ready(())) => return match root {
                Some(error) => Err(error), None => result,
            },
            Err(error) => {
                let cleanup = Error::from_runtime(error);
                return Err(retirement_failure(root.or_else(|| result.as_ref().err().cloned()), cleanup));
            }
            Ok(eltdx_runtime::engine::PendingPoll::Pending) if root.is_some() && context.cleanup_remaining()?.is_zero() => {
                return Err(Error::new(ErrorKind::CancelUnconfirmed, "money-flow probe retirement was not confirmed")
                    .with_source(root.take().expect("probe stop cause"))
                    .with_cleanup_failure(Error::new(ErrorKind::Timeout, "probe cleanup deadline elapsed")));
            }
            Ok(eltdx_runtime::engine::PendingPoll::Pending) => {}
        }
    }
}

fn retirement_failure(root: Option<Error>, cleanup: Error) -> Error {
    match root {
        Some(error) => Error::new(ErrorKind::CancelUnconfirmed, "money-flow probe retirement failed")
            .with_source(error).with_cleanup_failure(cleanup),
        None => cleanup,
    }
}
