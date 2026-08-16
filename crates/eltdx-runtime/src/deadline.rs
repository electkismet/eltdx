use std::time::{Duration, Instant};

use tokio::time::Instant as TokioInstant;

use crate::error::{RuntimeError, TimeoutPhase};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Deadline {
    at: Instant,
}

impl Deadline {
    pub const fn at(at: Instant) -> Self {
        Self { at }
    }

    pub fn after(timeout: Duration) -> Result<Self, RuntimeError> {
        Self::after_at(Instant::now(), timeout)
    }

    pub fn after_at(now: Instant, timeout: Duration) -> Result<Self, RuntimeError> {
        let at = now.checked_add(timeout).ok_or_else(|| {
            RuntimeError::invalid_argument("OverflowError", "timeout is too large")
        })?;
        Ok(Self { at })
    }

    pub const fn instant(self) -> Instant {
        self.at
    }

    pub fn tokio_instant(self) -> TokioInstant {
        TokioInstant::from_std(self.at)
    }

    pub fn remaining(self) -> Duration {
        self.remaining_at(Instant::now())
    }

    pub fn remaining_at(self, now: Instant) -> Duration {
        self.at.saturating_duration_since(now)
    }

    pub fn is_elapsed(self) -> bool {
        self.is_elapsed_at(Instant::now())
    }

    pub fn is_elapsed_at(self, now: Instant) -> bool {
        now >= self.at
    }

    pub fn require_remaining(self, phase: TimeoutPhase) -> Result<Duration, RuntimeError> {
        self.require_remaining_at(Instant::now(), phase)
    }

    pub fn require_remaining_at(
        self,
        now: Instant,
        phase: TimeoutPhase,
    ) -> Result<Duration, RuntimeError> {
        if self.is_elapsed_at(now) {
            return Err(RuntimeError::timeout(phase));
        }
        Ok(self.remaining_at(now))
    }

    pub fn fair_slice_at(
        self,
        now: Instant,
        parts_remaining: usize,
        phase: TimeoutPhase,
    ) -> Result<Self, RuntimeError> {
        if parts_remaining == 0 {
            return Err(RuntimeError::internal(
                "deadline budget cannot be divided into zero parts",
            ));
        }
        let remaining = self.require_remaining_at(now, phase)?;
        let divisor = u32::try_from(parts_remaining)
            .map_err(|_| RuntimeError::internal("deadline budget part count exceeds u32"))?;
        let slice = remaining / divisor;
        let at = now.checked_add(slice).ok_or_else(|| {
            RuntimeError::internal("deadline budget overflow while slicing remaining time")
        })?;
        Ok(Self {
            at: at.min(self.at),
        })
    }

    pub fn earlier(self, other: Self) -> Self {
        Self {
            at: self.at.min(other.at),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::Deadline;
    use crate::error::{RuntimeError, TimeoutPhase};

    #[test]
    fn one_absolute_deadline_only_loses_elapsed_time() -> Result<(), RuntimeError> {
        let started = Instant::now();
        let deadline = Deadline::after_at(started, Duration::from_secs(8))?;

        assert_eq!(
            deadline.remaining_at(started + Duration::from_secs(3)),
            Duration::from_secs(5)
        );
        assert!(deadline.is_elapsed_at(started + Duration::from_secs(8)));
        Ok(())
    }

    #[test]
    fn fair_slice_divides_only_the_remaining_budget() -> Result<(), RuntimeError> {
        let started = Instant::now();
        let deadline = Deadline::after_at(started, Duration::from_millis(900))?;
        let now = started + Duration::from_millis(300);

        let slice = deadline.fair_slice_at(now, 2, TimeoutPhase::Retry)?;

        assert_eq!(slice.instant(), started + Duration::from_millis(600));
        assert_eq!(deadline.instant(), started + Duration::from_millis(900));
        Ok(())
    }

    #[test]
    fn elapsed_deadline_reports_the_exact_phase() {
        let now = Instant::now();
        assert!(matches!(
            Deadline::at(now).require_remaining_at(now, TimeoutPhase::Connect),
            Err(RuntimeError::Timeout {
                phase: TimeoutPhase::Connect,
                ..
            })
        ));
    }

    #[test]
    fn earlier_never_extends_an_existing_deadline() {
        let now = Instant::now();
        let first = Deadline::at(now + Duration::from_secs(1));
        let second = Deadline::at(now + Duration::from_secs(2));

        assert_eq!(first.earlier(second), first);
        assert_eq!(second.earlier(first), first);
    }
}
