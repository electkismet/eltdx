//! A transport worker borrows the existing operation clock and weak owner.
use crate::operation::OperationSponsor;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct Control {
    pub sponsor: OperationSponsor,
    pub stopped: Arc<AtomicBool>,
    pub owner_stopped: Arc<AtomicBool>,
    pub deadline: Instant,
}
impl Control {
    pub fn check(&self, phase: TimeoutPhase) -> Result<()> {
        self.sponsor
            .check_boundary()
            .map_err(|error| phase_error(error, phase))?;
        if self.owner_stopped.load(Ordering::Acquire) {
            return Err(phase_error(
                Error::new(ErrorKind::Closed, "extension owner closed"),
                phase,
            ));
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(phase_error(
                Error::new(ErrorKind::Cancelled, "extension worker stopped"),
                phase,
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(phase_error(
                Error::new(ErrorKind::Timeout, "extension operation deadline elapsed"),
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
                result = &mut future => {
                    self.check(phase)?;
                    return result.map_err(|error| phase_error(error, phase));
                },
            }
        }
    }
    pub async fn connect<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        // Bound a silent endpoint without renewing the caller's total clock.
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let allowance = (remaining / 2).min(Duration::from_secs(1));
        self.wait(TimeoutPhase::Connect, async {
            tokio::time::timeout(allowance, future).await.map_err(|_| {
                Error::new(
                    ErrorKind::Connection,
                    "extension endpoint connect timed out",
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
pub(crate) fn connection(error: std::io::Error) -> Error {
    Error::new(ErrorKind::Connection, "extension socket I/O failed").with_source(error)
}
