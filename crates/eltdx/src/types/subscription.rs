use std::time::{Duration, Instant};

use crate::error::{Error, ErrorKind, Result};
use super::{QuoteUpdate, SecurityId, SnapshotBatch};

/// Long-lived subscription policy. Establishment deadlines and tokens belong
/// to OperationOptions and are deliberately absent from this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionOptions {
    max_events: usize,
    max_bytes: usize,
    renew_interval: Option<Duration>,
    gap_policy: GapPolicy,
}

impl Default for SubscriptionOptions {
    fn default() -> Self {
        Self { max_events: 1024, max_bytes: 64 * 1024 * 1024,
            renew_interval: Some(Duration::from_secs(7)), gap_policy: GapPolicy::ReportOnly }
    }
}

impl SubscriptionOptions {
    pub fn with_queue_limits(mut self, max_events: usize, max_bytes: usize) -> Self {
        self.max_events = max_events; self.max_bytes = max_bytes; self
    }
    pub fn with_renew_interval(mut self, interval: Option<Duration>) -> Self {
        self.renew_interval = interval; self
    }
    pub fn with_gap_policy(mut self, policy: GapPolicy) -> Self { self.gap_policy = policy; self }
    pub const fn max_events(&self) -> usize { self.max_events }
    pub const fn max_bytes(&self) -> usize { self.max_bytes }
    pub const fn renew_interval(&self) -> Option<Duration> { self.renew_interval }
    pub const fn gap_policy(&self) -> GapPolicy { self.gap_policy }

    /// The caller supplies the supported maximum event charge for its shape.
    /// Validation happens before metadata I/O or reserving the first pin.
    pub(crate) fn validate(&self, maximum_event_bytes: usize, now: Instant) -> Result<()> {
        if self.max_events == 0 || self.max_bytes == 0 {
            return Err(invalid("subscription queue capacities must be positive"));
        }
        if self.max_bytes < maximum_event_bytes {
            return Err(invalid("subscription byte capacity cannot hold a supported maximum event")
                .with_context("required", maximum_event_bytes.to_string())
                .with_context("configured", self.max_bytes.to_string()));
        }
        if let Some(interval) = self.renew_interval {
            if interval.is_zero() || now.checked_add(interval).is_none() {
                return Err(invalid("subscription renewal interval must be positive and finite"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum GapPolicy { #[default] ReportOnly, Resynchronize, Close }

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RefreshRequest { id: SecurityId, cursor: u32 }

impl RefreshRequest {
    pub fn new(id: SecurityId) -> Self { Self { id, cursor: 0 } }
    pub fn with_id(mut self, id: SecurityId) -> Result<Self> { self.id = id; Ok(self) }
    /// Source-local time-of-day cursor; a lower value can begin a new session.
    pub fn with_cursor(mut self, cursor: u32) -> Result<Self> { self.cursor = cursor; Ok(self) }
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn cursor(&self) -> u32 { self.cursor }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum GapReason { RuntimeOverflow, SubscriptionOverflow, SourceLost, OversizedEvent, Multiple }

/// Loss evidence uses optional source/cursor bounds: unknown bounds remain
/// unknown instead of being inferred from a monotonic sequence assumption.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PushGap {
    reason: GapReason,
    dropped: u64,
    first_cursor: Option<u32>,
    last_cursor: Option<u32>,
    engine_epoch: Option<u64>,
    slot: Option<usize>,
    generation: Option<u64>,
}

impl PushGap {
    pub(crate) fn new(reason: GapReason, dropped: u64, first: Option<u32>, last: Option<u32>) -> Self {
        Self { reason, dropped, first_cursor: first, last_cursor: last,
            engine_epoch: None, slot: None, generation: None }
    }
    pub(crate) fn with_source(mut self, epoch: u64, slot: usize, generation: u64) -> Self {
        self.engine_epoch = Some(epoch); self.slot = Some(slot); self.generation = Some(generation); self
    }
    pub(crate) fn merge(&mut self, other: Self) {
        if self.reason != other.reason { self.reason = GapReason::Multiple; }
        self.dropped = self.dropped.saturating_add(other.dropped);
        // Unknown or mixed ranges cannot be made precise by merging.
        self.first_cursor = self.first_cursor.zip(other.first_cursor).map(|(first, _)| first);
        self.last_cursor = self.last_cursor.zip(other.last_cursor).map(|(_, last)| last);
        if self.engine_epoch != other.engine_epoch || self.slot != other.slot || self.generation != other.generation {
            self.engine_epoch = None; self.slot = None; self.generation = None;
        }
    }
    pub const fn reason(&self) -> GapReason { self.reason }
    pub const fn dropped(&self) -> u64 { self.dropped }
    pub const fn first_cursor(&self) -> Option<u32> { self.first_cursor }
    pub const fn last_cursor(&self) -> Option<u32> { self.last_cursor }
    pub const fn engine_epoch(&self) -> Option<u64> { self.engine_epoch }
    pub const fn slot(&self) -> Option<usize> { self.slot }
    pub const fn generation(&self) -> Option<u64> { self.generation }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum PushEvent {
    Initial(SnapshotBatch),
    Update(QuoteUpdate),
    Gap(PushGap),
    Resynchronized(SnapshotBatch),
}

impl PushEvent {
    pub(crate) fn canonical_heap(&mut self) {
        match self {
            Self::Initial(batch) | Self::Resynchronized(batch) => batch.canonical_heap(),
            Self::Update(update) => update.canonical_heap(),
            Self::Gap(_) => {}
        }
    }
    pub(crate) fn retained_bytes(&self) -> Result<usize> {
        let mut charge = super::memory::MemoryCharge::default();
        charge.add(std::mem::size_of::<Self>())?;
        match self {
            Self::Initial(batch) | Self::Resynchronized(batch) => batch.charge_heap(&mut charge)?,
            Self::Update(update) => update.charge_heap(&mut charge)?,
            Self::Gap(_) => {}
        }
        Ok(charge.total())
    }
    pub(crate) fn cursor(&self) -> Option<u32> {
        match self { Self::Update(update) => Some(update.cursor()), _ => None }
    }
    pub(crate) fn mark_desynchronized(&mut self) {
        if let Self::Update(update) = self { update.mark_desynchronized(); }
    }
}

fn invalid(message: &'static str) -> Error { Error::new(ErrorKind::InvalidArgument, message) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_subscription_configuration_is_rejected_before_resource_admission() {
        let now = Instant::now();
        for options in [SubscriptionOptions::default().with_queue_limits(0, 1024),
            SubscriptionOptions::default().with_queue_limits(1, 0),
            SubscriptionOptions::default().with_queue_limits(1, 1023),
            SubscriptionOptions::default().with_renew_interval(Some(Duration::ZERO)),
            SubscriptionOptions::default().with_renew_interval(Some(Duration::MAX))] {
            assert_eq!(options.validate(1024, now).unwrap_err().kind(), ErrorKind::InvalidArgument);
        }
        SubscriptionOptions::default().with_queue_limits(1, 1024).with_renew_interval(None)
            .validate(1024, now).unwrap();
    }

    #[test]
    fn merged_loss_ranges_preserve_receive_order_and_leave_mixed_sources_unknown() {
        let mut gap = PushGap::new(GapReason::SubscriptionOverflow, 1, Some(150000), Some(150000))
            .with_source(1, 0, 1);
        gap.merge(PushGap::new(GapReason::SubscriptionOverflow, 1, Some(93000), Some(93000))
            .with_source(1, 0, 1));
        assert_eq!(gap.first_cursor(), Some(150000)); assert_eq!(gap.last_cursor(), Some(93000));
        gap.merge(PushGap::new(GapReason::RuntimeOverflow, 2, None, None));
        assert_eq!(gap.reason(), GapReason::Multiple); assert_eq!(gap.dropped(), 4);
        assert_eq!(gap.first_cursor(), None); assert_eq!(gap.last_cursor(), None);
        assert_eq!(gap.engine_epoch(), None); assert_eq!(gap.slot(), None); assert_eq!(gap.generation(), None);
    }
}
