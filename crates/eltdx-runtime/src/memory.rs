use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

use crate::error::RuntimeError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryBudgetSnapshot {
    pub raw_bytes: usize,
    pub raw_max_bytes: usize,
    pub raw_peak_bytes: usize,
    pub decoded_bytes: usize,
    pub decoded_max_bytes: usize,
    pub decoded_peak_bytes: usize,
}

#[derive(Debug)]
pub struct MemoryBudget {
    raw_bytes: AtomicUsize,
    raw_max_bytes: usize,
    raw_peak_bytes: AtomicUsize,
    raw_released: Notify,
    decoded_bytes: AtomicUsize,
    decoded_max_bytes: usize,
    decoded_peak_bytes: AtomicUsize,
    decoded_released: Notify,
}

impl MemoryBudget {
    pub fn new(raw_max_bytes: usize, decoded_max_bytes: usize) -> Result<Self, RuntimeError> {
        if raw_max_bytes == 0 || decoded_max_bytes == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "Engine memory budgets must be positive",
            ));
        }
        Ok(Self {
            raw_bytes: AtomicUsize::new(0),
            raw_max_bytes,
            raw_peak_bytes: AtomicUsize::new(0),
            raw_released: Notify::new(),
            decoded_bytes: AtomicUsize::new(0),
            decoded_max_bytes,
            decoded_peak_bytes: AtomicUsize::new(0),
            decoded_released: Notify::new(),
        })
    }

    pub fn raw_available(&self) -> usize {
        self.raw_max_bytes
            .saturating_sub(self.raw_bytes.load(Ordering::Acquire))
    }

    pub fn try_reserve_raw(&self, requested: usize) -> bool {
        try_reserve(
            &self.raw_bytes,
            self.raw_max_bytes,
            &self.raw_peak_bytes,
            requested,
        )
    }

    pub fn release_raw(&self, bytes: usize) {
        if release(&self.raw_bytes, bytes) {
            self.raw_released.notify_waiters();
            self.raw_released.notify_one();
        }
    }

    pub async fn wait_for_raw_release(&self) {
        self.raw_released.notified().await;
    }

    pub fn decoded_available(&self) -> usize {
        self.decoded_max_bytes
            .saturating_sub(self.decoded_bytes.load(Ordering::Acquire))
    }

    pub fn try_reserve_decoded(&self, requested: usize) -> bool {
        try_reserve(
            &self.decoded_bytes,
            self.decoded_max_bytes,
            &self.decoded_peak_bytes,
            requested,
        )
    }

    pub fn release_decoded(&self, bytes: usize) {
        if release(&self.decoded_bytes, bytes) {
            self.decoded_released.notify_waiters();
            self.decoded_released.notify_one();
        }
    }

    pub async fn wait_for_decoded_release(&self) {
        self.decoded_released.notified().await;
    }

    pub fn snapshot(&self) -> MemoryBudgetSnapshot {
        MemoryBudgetSnapshot {
            raw_bytes: self.raw_bytes.load(Ordering::Acquire),
            raw_max_bytes: self.raw_max_bytes,
            raw_peak_bytes: self.raw_peak_bytes.load(Ordering::Acquire),
            decoded_bytes: self.decoded_bytes.load(Ordering::Acquire),
            decoded_max_bytes: self.decoded_max_bytes,
            decoded_peak_bytes: self.decoded_peak_bytes.load(Ordering::Acquire),
        }
    }

    pub fn check_empty(&self) -> Result<(), RuntimeError> {
        let snapshot = self.snapshot();
        if snapshot.raw_bytes != 0 || snapshot.decoded_bytes != 0 {
            return Err(RuntimeError::internal(
                "Engine memory budget retained bytes after Slot cleanup",
            )
            .with_context("raw_bytes", snapshot.raw_bytes.to_string())
            .with_context("decoded_bytes", snapshot.decoded_bytes.to_string()));
        }
        Ok(())
    }
}

fn try_reserve(
    current: &AtomicUsize,
    maximum: usize,
    peak: &AtomicUsize,
    requested: usize,
) -> bool {
    if requested == 0 {
        return true;
    }
    let mut observed = current.load(Ordering::Acquire);
    loop {
        let Some(next) = observed.checked_add(requested) else {
            return false;
        };
        if next > maximum {
            return false;
        }
        match current.compare_exchange_weak(observed, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                peak.fetch_max(next, Ordering::AcqRel);
                return true;
            }
            Err(actual) => observed = actual,
        }
    }
}

fn release(current: &AtomicUsize, bytes: usize) -> bool {
    if bytes == 0 {
        return false;
    }
    let result = current.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        value.checked_sub(bytes)
    });
    debug_assert!(result.is_ok(), "memory budget release underflow");
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use proptest::prelude::*;

    use super::MemoryBudget;

    #[test]
    fn reservations_are_exact_and_peak_usage_is_retained() -> Result<(), crate::error::RuntimeError>
    {
        let budget = MemoryBudget::new(10, 20)?;

        assert!(budget.try_reserve_raw(7));
        assert!(!budget.try_reserve_raw(4));
        assert_eq!(budget.raw_available(), 3);
        budget.release_raw(7);

        assert!(budget.try_reserve_decoded(20));
        assert!(!budget.try_reserve_decoded(1));
        budget.release_decoded(20);

        let snapshot = budget.snapshot();
        assert_eq!(snapshot.raw_bytes, 0);
        assert_eq!(snapshot.raw_peak_bytes, 7);
        assert_eq!(snapshot.decoded_bytes, 0);
        assert_eq!(snapshot.decoded_peak_bytes, 20);
        assert!(budget.check_empty().is_ok());
        Ok(())
    }

    proptest! {
        #[test]
        fn sequential_raw_accounting_matches_a_bounded_model(
            operations in prop::collection::vec((any::<bool>(), 0_usize..=256), 0..512)
        ) {
            const MAXIMUM: usize = 1_024;
            let created = MemoryBudget::new(MAXIMUM, MAXIMUM);
            prop_assert!(created.is_ok());
            let budget = match created {
                Ok(budget) => budget,
                Err(_) => return Ok(()),
            };
            let mut model = 0_usize;
            let mut peak = 0_usize;

            for (reserve, amount) in operations {
                if reserve {
                    let expected = model.checked_add(amount).is_some_and(|next| next <= MAXIMUM);
                    prop_assert_eq!(budget.try_reserve_raw(amount), expected);
                    if expected {
                        model += amount;
                        peak = peak.max(model);
                    }
                } else {
                    let released = amount.min(model);
                    budget.release_raw(released);
                    model -= released;
                }
                let snapshot = budget.snapshot();
                prop_assert_eq!(snapshot.raw_bytes, model);
                prop_assert_eq!(snapshot.raw_peak_bytes, peak);
                prop_assert!(snapshot.raw_bytes <= snapshot.raw_max_bytes);
            }
        }
    }

    #[test]
    fn concurrent_reservations_never_exceed_the_global_limit(
    ) -> Result<(), crate::error::RuntimeError> {
        const UNIT: usize = 4 * 1024;
        const CAPACITY: usize = 8 * UNIT;
        let budget = Arc::new(MemoryBudget::new(CAPACITY, CAPACITY)?);
        let mut workers = Vec::new();
        for _ in 0..32 {
            let budget = Arc::clone(&budget);
            workers.push(thread::spawn(move || {
                for _ in 0..256 {
                    if budget.try_reserve_decoded(UNIT) {
                        assert!(budget.snapshot().decoded_bytes <= CAPACITY);
                        budget.release_decoded(UNIT);
                    }
                }
            }));
        }
        for worker in workers {
            assert!(worker.join().is_ok(), "memory budget worker panicked");
        }

        assert!(budget.snapshot().decoded_peak_bytes <= CAPACITY);
        assert!(budget.check_empty().is_ok());
        Ok(())
    }
}
