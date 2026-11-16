use crate::error::{Error, ErrorKind, Result};
use crate::types::{GapPolicy, GapReason, PushEvent, PushGap, SubscriptionOptions};

#[derive(Debug)]
struct ChargedEvent { event: PushEvent, bytes: usize }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QueueAction { Continue, Resynchronize, Stop }

/// The typed event queue has its own charge, separate from runtime wire bytes.
/// Production publication computes owned and retained DTO storage itself.
/// The fixed ring allocation is charged even when no data is pending.
/// Loss control occupies its own slot and is visible even with a full data queue.
#[derive(Debug)]
pub(super) struct EventQueue {
    max_events: usize,
    max_bytes: usize,
    policy: GapPolicy,
    events: Box<[Option<ChargedEvent>]>,
    head: usize,
    len: usize,
    storage_bytes: usize,
    bytes: usize,
    gap: Option<PushGap>,
    desynchronized: bool,
    recovery_requested: bool,
    terminal: Option<Error>,
    ended: bool,
}

impl EventQueue {
    pub(super) fn storage_bytes(count: usize) -> Result<usize> {
        count.checked_mul(std::mem::size_of::<Option<ChargedEvent>>())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Self>())).ok_or_else(||
            Error::new(ErrorKind::InvalidArgument, "subscription queue storage size overflows"))
    }

    pub(super) fn new(options: &SubscriptionOptions) -> Result<Self> {
        let storage_bytes = Self::storage_bytes(options.max_events())?;
        if options.max_events() == 0 || storage_bytes >= options.max_bytes() {
            return Err(Error::new(ErrorKind::InvalidArgument, "subscription byte capacity cannot hold its queue storage"));
        }
        let mut events = Vec::new();
        events.try_reserve_exact(options.max_events()).map_err(|error| Error::new(ErrorKind::LimitExceeded,
            "unable to reserve subscription queue slots").with_source(error))?;
        events.resize_with(options.max_events(), || None);
        Ok(Self { max_events: options.max_events(), max_bytes: options.max_bytes(), policy: options.gap_policy(),
            events: events.into_boxed_slice(), head: 0, len: 0, storage_bytes, bytes: storage_bytes,
            gap: None, desynchronized: false, recovery_requested: false, terminal: None, ended: false })
    }

    pub(super) fn publish(&mut self, mut event: PushEvent) -> Result<QueueAction> {
        event.canonical_heap();
        let bytes = event.retained_bytes()?;
        self.offer(event, bytes)
    }

    fn pop_data(&mut self) -> Option<ChargedEvent> {
        if self.len == 0 { return None; }
        let value = self.events[self.head].take();
        self.head = (self.head + 1) % self.max_events;
        self.len -= 1;
        value
    }

    fn clear_data(&mut self) {
        while self.pop_data().is_some() {}
        self.bytes = self.storage_bytes;
    }

    fn offer(&mut self, mut event: PushEvent, bytes: usize) -> Result<QueueAction> {
        if self.ended || self.terminal.is_some() { return Ok(QueueAction::Stop); }
        if bytes > self.max_bytes - self.storage_bytes {
            self.clear_data();
            self.loss(PushGap::new(GapReason::OversizedEvent, 1, event.cursor(), event.cursor()));
            self.terminal = Some(Error::new(ErrorKind::LimitExceeded, "typed push event exceeds subscription byte capacity")
                .with_context("event_bytes", bytes.to_string())
                .with_context("configured", self.max_bytes.to_string()));
            return Ok(QueueAction::Stop);
        }
        while self.len >= self.max_events || self.bytes > self.max_bytes - bytes {
            let Some(lost) = self.pop_data() else {
                return Err(Error::new(ErrorKind::Internal, "subscription queue capacity is invalid"));
            };
            self.bytes -= lost.bytes;
            let cursor = lost.event.cursor();
            let action = self.loss(PushGap::new(GapReason::SubscriptionOverflow, 1, cursor, cursor));
            if action == QueueAction::Stop { return Ok(action); }
        }
        if self.desynchronized { event.mark_desynchronized(); }
        self.bytes += bytes;
        let tail = (self.head + self.len) % self.max_events;
        self.events[tail] = Some(ChargedEvent { event, bytes });
        self.len += 1;
        Ok(if self.recovery_requested { QueueAction::Resynchronize } else { QueueAction::Continue })
    }

    pub(super) fn loss(&mut self, gap: PushGap) -> QueueAction {
        self.desynchronized = true;
        match self.gap.as_mut() { Some(pending) => pending.merge(gap), None => self.gap = Some(gap) }
        match self.policy {
            GapPolicy::ReportOnly => QueueAction::Continue,
            GapPolicy::Resynchronize => { self.recovery_requested = true; QueueAction::Resynchronize }
            GapPolicy::Close => {
                self.clear_data();
                self.terminal = Some(Error::new(ErrorKind::PushGap, "subscription stopped after a push gap"));
                QueueAction::Stop
            }
        }
    }

    pub(super) fn pop(&mut self) -> Result<Option<PushEvent>> {
        if let Some(gap) = self.gap.take() { return Ok(Some(PushEvent::Gap(gap))); }
        if let Some(mut value) = self.pop_data() {
            self.bytes -= value.bytes;
            if self.desynchronized { value.event.mark_desynchronized(); }
            return Ok(Some(value.event));
        }
        if let Some(error) = &self.terminal { return Err(error.clone()); }
        Ok(None)
    }

    /// Called only after a finite, successful snapshot refresh on the same
    /// activation. Old pending data is superseded; the independent Gap stays
    /// ahead of Resynchronized even if the consumer has not read it yet.
    fn restore(&mut self, batch: crate::types::SnapshotBatch, bytes: usize) -> Result<QueueAction> {
        if self.ended || self.terminal.is_some() { return Ok(QueueAction::Stop); }
        if !self.recovery_requested {
            return Err(Error::new(ErrorKind::Internal, "subscription resynchronization was not requested"));
        }
        self.clear_data();
        let action = self.offer(PushEvent::Resynchronized(batch), bytes)?;
        if action != QueueAction::Stop {
            self.desynchronized = false; self.recovery_requested = false;
            return Ok(QueueAction::Continue);
        }
        Ok(action)
    }

    /// Normal user close clears pending data; parent/fatal close uses fail so
    /// its error survives reads. Public next_event distinguishes idle from end.
    pub(super) fn end(&mut self) {
        self.clear_data(); self.gap = None;
        self.terminal = None; self.ended = true; self.recovery_requested = false;
    }
    pub(super) fn fail(&mut self, error: Error) {
        self.clear_data();
        if self.terminal.is_none() { self.terminal = Some(error); }
    }
    pub(super) fn ended(&self) -> bool { self.ended }
    pub(super) fn ready(&self) -> bool { self.gap.is_some() || self.len != 0 || self.terminal.is_some() }
    pub(super) fn recovery_requested(&self) -> bool { self.recovery_requested }
    pub(super) fn restore_event(&mut self, batch: crate::types::SnapshotBatch) -> Result<QueueAction> {
        let mut event = PushEvent::Resynchronized(batch);
        event.canonical_heap();
        let bytes = event.retained_bytes()?;
        let PushEvent::Resynchronized(batch) = event else { unreachable!() };
        self.restore(batch, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::source_cases::update;
    use crate::types::SnapshotBatch;

    fn options(count: usize, data_bytes: usize) -> SubscriptionOptions {
        SubscriptionOptions::default().with_queue_limits(count, data_bytes + EventQueue::storage_bytes(count).unwrap())
    }

    #[test]
    fn independent_gap_survives_count_overflow_and_marks_updates_until_restore() {
        let mut queue = EventQueue::new(&options(2, 100)).unwrap();
        for cursor in [150000, 93000, 93001] {
            queue.offer(PushEvent::Update(update("sh600000", cursor)), 40).unwrap();
        }
        match queue.pop().unwrap().unwrap() {
            PushEvent::Gap(gap) => {
                assert_eq!(gap.dropped(), 1); assert_eq!(gap.first_cursor(), Some(150000));
                assert_eq!(gap.last_cursor(), Some(150000));
            }
            _ => panic!("gap control must be reachable before full data queue"),
        }
        for cursor in [93000, 93001] {
            match queue.pop().unwrap().unwrap() {
                PushEvent::Update(row) => { assert_eq!(row.cursor(), cursor); assert!(row.desynchronized()); }
                _ => panic!("retained updates keep their source order"),
            }
        }
        queue.offer(PushEvent::Update(update("sh600000", 93002)), 40).unwrap();
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Update(row)) if row.desynchronized()));
        assert!(!queue.ended(), "an idle internal queue is not normal public stream completion");
    }

    #[test]
    fn byte_budget_coalesces_losses_even_when_event_count_has_room() {
        let mut queue = EventQueue::new(&options(10, 50)).unwrap();
        for cursor in [1, 2, 3] { queue.offer(PushEvent::Update(update("sh600000", cursor)), 30).unwrap(); }
        match queue.pop().unwrap().unwrap() {
            PushEvent::Gap(gap) => { assert_eq!(gap.dropped(), 2); assert_eq!(gap.first_cursor(), Some(1)); assert_eq!(gap.last_cursor(), Some(2)); }
            _ => panic!("independent loss slot"),
        }
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Update(row)) if row.cursor() == 3 && row.desynchronized()));
    }

    #[test]
    fn oversized_event_is_an_explicit_limit_failure_after_loss_notification() {
        let mut queue = EventQueue::new(&options(2, 10)).unwrap();
        assert_eq!(queue.offer(PushEvent::Update(update("sh600000", 1)), 11).unwrap(), QueueAction::Stop);
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Gap(gap)) if gap.reason() == GapReason::OversizedEvent));
        assert_eq!(queue.pop().unwrap_err().kind(), ErrorKind::LimitExceeded);
    }

    #[test]
    fn resynchronization_reports_gap_then_snapshot_before_continuous_updates() {
        let mut queue = EventQueue::new(&options(2, 100).with_gap_policy(GapPolicy::Resynchronize)).unwrap();
        assert_eq!(queue.loss(PushGap::new(GapReason::RuntimeOverflow, 3, None, None)), QueueAction::Resynchronize);
        queue.offer(PushEvent::Update(update("sh600000", 93000)), 20).unwrap();
        assert_eq!(queue.restore(SnapshotBatch::new(Vec::new()), 20).unwrap(), QueueAction::Continue);
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Gap(gap)) if gap.dropped() == 3));
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Resynchronized(_))));
        queue.offer(PushEvent::Update(update("sh600000", 93001)), 20).unwrap();
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Update(row)) if !row.desynchronized()));
    }

    #[test]
    fn close_policy_and_failed_recovery_do_not_leave_an_active_looking_stream() {
        let mut queue = EventQueue::new(&SubscriptionOptions::default().with_gap_policy(GapPolicy::Close)).unwrap();
        assert_eq!(queue.loss(PushGap::new(GapReason::SourceLost, 1, None, None)), QueueAction::Stop);
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Gap(_))));
        assert_eq!(queue.pop().unwrap_err().kind(), ErrorKind::PushGap);
        let mut queue = EventQueue::new(&SubscriptionOptions::default().with_gap_policy(GapPolicy::Resynchronize)).unwrap();
        queue.loss(PushGap::new(GapReason::RuntimeOverflow, 1, None, None));
        queue.fail(Error::new(ErrorKind::Connection, "resynchronization failed"));
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Gap(_))));
        assert_eq!(queue.pop().unwrap_err().kind(), ErrorKind::Connection);
        queue.end(); assert!(queue.ended()); assert!(queue.pop().unwrap().is_none());
    }

    #[test]
    fn production_publication_charges_owned_dtos_and_fixed_ring_storage() {
        let row = update("sh600000", 93000);
        let event = PushEvent::Update(row);
        let charge = event.retained_bytes().unwrap();
        assert!(charge > std::mem::size_of::<PushEvent>());
        let storage = EventQueue::storage_bytes(2).unwrap();
        let mut queue = EventQueue::new(&SubscriptionOptions::default().with_queue_limits(2, storage + charge)).unwrap();
        queue.publish(event.clone()).unwrap();
        assert_eq!(queue.bytes, storage + charge);
        queue.publish(event).unwrap();
        assert_eq!(queue.len, 1, "the byte budget can evict before the count limit");
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Gap(gap)) if gap.dropped() == 1));
        assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Update(_))));
        assert_eq!(queue.bytes, storage, "empty queue still retains its fixed slot allocation");
        queue.end(); assert_eq!(queue.bytes, storage);
    }

    #[test]
    fn ring_wraps_without_growing_and_huge_slot_storage_is_rejected() {
        assert!(EventQueue::storage_bytes(usize::MAX).is_err());
        assert!(EventQueue::new(&SubscriptionOptions::default().with_queue_limits(2, 1)).is_err());
        let mut queue = EventQueue::new(&options(2, 100)).unwrap();
        let address = queue.events.as_ptr();
        for cursor in 0..10 {
            queue.offer(PushEvent::Update(update("sh600000", cursor)), 20).unwrap();
            assert!(matches!(queue.pop().unwrap(), Some(PushEvent::Update(row)) if row.cursor() == cursor));
            assert_eq!(queue.events.as_ptr(), address);
        }
    }
}
