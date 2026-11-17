use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::operation::OperationSponsor;
use crate::{Error, ErrorKind, Result, TimeoutPhase};

/// Owner-free worker control. A job never owns its parent client/session.
#[derive(Clone, Debug)]
pub(crate) struct Control {
    pub sponsor: OperationSponsor,
    pub stop: Arc<AtomicBool>,
    pub deadline: Instant,
}
impl Control {
    pub fn check(&self, phase: TimeoutPhase) -> Result<()> {
        self.sponsor
            .check_boundary()
            .map_err(|error| phase_error(error, phase))?;
        if self.stop.load(Ordering::Acquire) {
            return Err(phase_error(
                Error::new(ErrorKind::Cancelled, "F10 task stopped"),
                phase,
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(phase_error(
                Error::new(ErrorKind::Timeout, "F10 operation deadline elapsed"),
                phase,
            ));
        }
        Ok(())
    }
    pub async fn wait<T>(
        &self,
        phase: TimeoutPhase,
        future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        tokio::pin!(future);
        loop {
            self.check(phase)?;
            tokio::select! {
                biased;
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
                value = &mut future => {
                    self.check(phase)?;
                    return value.map_err(|error| phase_error(error, phase));
                },
            }
        }
    }
    pub async fn connect_wait<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        // Leave budget for another address family instead of allowing one
        // unreachable IPv4 address to consume the entire operation deadline.
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let allowance = (remaining / 2).min(Duration::from_secs(1));
        self.wait(TimeoutPhase::Connect, async {
            tokio::time::timeout(allowance, future).await.map_err(|_| {
                Error::new(
                    ErrorKind::Connection,
                    "F10 address connect attempt timed out",
                )
            })?
        })
        .await
    }
}
pub(crate) fn phase_error(error: Error, phase: TimeoutPhase) -> Error {
    if error.context().iter().any(|(key, _)| key == "phase") {
        error
    } else {
        error.with_context("phase", format!("{phase:?}").to_ascii_lowercase())
    }
}
pub(crate) fn connection(
    error: impl std::error::Error + Send + Sync + 'static,
    message: &str,
) -> Error {
    Error::new(ErrorKind::Connection, message).with_source(error)
}
