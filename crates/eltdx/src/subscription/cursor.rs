use std::collections::HashMap;

use crate::error::{Error, ErrorKind, Result};
use crate::types::{QuoteUpdate, RefreshRequest, SecurityId};
use eltdx_runtime::slot::GenerationIdentity;

/// One activation owns one immutable epoch/slot/TCP-generation binding.
/// Reconnection requires a new lease and book, never rebinding this book.
#[derive(Debug)]
pub(super) struct CursorBook {
    source: GenerationIdentity,
    order: Vec<SecurityId>,
    cursors: HashMap<SecurityId, u32>,
    sequences: HashMap<SecurityId, u64>,
}

impl CursorBook {
    pub(super) fn new(source: GenerationIdentity, ids: &[SecurityId]) -> Result<Self> {
        let mut order = Vec::new();
        let mut cursors = HashMap::new();
        order.try_reserve(ids.len()).map_err(allocation)?;
        cursors.try_reserve(ids.len()).map_err(allocation)?;
        for id in ids {
            if cursors.insert(id.clone(), 0).is_none() {
                order.push(id.clone());
            }
        }
        let mut sequences = HashMap::new();
        sequences.try_reserve(order.len()).map_err(allocation)?;
        for id in &order {
            sequences.insert(id.clone(), 0);
        }
        Ok(Self {
            source,
            order,
            cursors,
            sequences,
        })
    }

    pub(super) fn accepts(&self, source: GenerationIdentity) -> bool {
        self.source == source
    }

    pub(super) fn reset(&mut self) {
        for cursor in self.cursors.values_mut() {
            *cursor = 0;
        }
    }

    pub(super) fn snapshot_floors(&mut self, floors: &[(SecurityId, u64)]) -> Result<()> {
        for (id, _) in floors {
            if !self.sequences.contains_key(id) {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    "snapshot floor has an unrequested identity",
                ));
            }
        }
        for (id, sequence) in floors {
            let last = self.sequences.get_mut(id).ok_or_else(|| {
                Error::new(ErrorKind::Internal, "validated floor identity disappeared")
            })?;
            *last = (*last).max(*sequence);
        }
        Ok(())
    }

    /// Receive sequences are monotonic only within the exact TCP generation.
    /// Cursor values remain time-of-day and are never compared as a maximum.
    pub(super) fn remember_ordered(
        &mut self,
        source: GenerationIdentity,
        sequence: u64,
        updates: &mut Vec<QuoteUpdate>,
    ) -> Result<()> {
        if !self.accepts(source) {
            updates.clear();
            return Ok(());
        }
        for update in updates.iter() {
            if !self.cursors.contains_key(update.id()) {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    "push contains an identity outside its activation",
                ));
            }
        }
        updates.retain(|update| {
            self.sequences
                .get(update.id())
                .is_some_and(|last| sequence > *last)
        });
        self.remember(source, updates)?;
        for update in updates {
            *self.sequences.get_mut(update.id()).ok_or_else(|| {
                Error::new(
                    ErrorKind::Internal,
                    "validated sequence identity disappeared",
                )
            })? = sequence;
        }
        Ok(())
    }

    /// Return false for stale source data before touching any cursor. Validate
    /// every identity first, so a corrupt frame cannot partly move this book.
    pub(super) fn remember(
        &mut self,
        source: GenerationIdentity,
        updates: &[QuoteUpdate],
    ) -> Result<bool> {
        if !self.accepts(source) {
            return Ok(false);
        }
        for row in updates {
            if !self.cursors.contains_key(row.id()) {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    "push contains an identity outside its activation",
                ));
            }
        }
        for row in updates {
            // The most recently received value wins, including150000→93000.
            *self.cursors.get_mut(row.id()).ok_or_else(|| {
                Error::new(ErrorKind::Internal, "validated cursor identity disappeared")
            })? = row.cursor();
        }
        Ok(true)
    }

    pub(super) fn requests(&self) -> Result<Vec<RefreshRequest>> {
        let mut requests = Vec::new();
        requests.try_reserve(self.order.len()).map_err(allocation)?;
        for id in &self.order {
            requests.push(RefreshRequest::new(id.clone()).with_cursor(self.cursors[id])?);
        }
        Ok(requests)
    }
}

fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "unable to reserve subscription cursor state",
    )
    .with_source(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::source_cases::update;
    use eltdx_runtime::slot::{EngineEpoch, GenerationId, SlotId};

    fn source(epoch: u64, slot: usize, generation: u64) -> GenerationIdentity {
        GenerationIdentity {
            engine_epoch: EngineEpoch::new(epoch).unwrap(),
            slot_id: SlotId::new(slot),
            generation: GenerationId::new(generation).unwrap(),
        }
    }

    #[test]
    fn latest_source_cursor_can_move_backwards_into_the_next_morning() {
        let identity = source(1, 0, 1);
        let id: SecurityId = "sh600000".parse().unwrap();
        let mut cursors = CursorBook::new(identity, &[id.clone(), id]).unwrap();
        for cursor in [150000, 93000] {
            assert!(cursors
                .remember(identity, &[update("sh600000", cursor)])
                .unwrap());
        }
        let requests = cursors.requests().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cursor(), 93000);
    }

    #[test]
    fn stale_epoch_slot_or_tcp_generation_cannot_change_a_cursor() {
        let identity = source(2, 3, 4);
        let id = "sh600000".parse().unwrap();
        let mut cursors = CursorBook::new(identity, &[id]).unwrap();
        cursors
            .remember(identity, &[update("sh600000", 93000)])
            .unwrap();
        for stale in [source(1, 3, 4), source(2, 2, 4), source(2, 3, 3)] {
            assert!(!cursors
                .remember(stale, &[update("sh600000", 150000)])
                .unwrap());
        }
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 93000);
    }

    #[test]
    fn invalid_identity_does_not_partly_commit_an_earlier_valid_row() {
        let identity = source(1, 0, 1);
        let mut cursors = CursorBook::new(identity, &["sh600000".parse().unwrap()]).unwrap();
        assert_eq!(
            cursors
                .remember(
                    identity,
                    &[update("sh600000", 150000), update("sh600001", 93000)]
                )
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 0);
    }

    #[test]
    fn receive_order_filters_stale_updates_and_preserves_time_of_day_rollover() {
        let identity = source(1, 0, 1);
        let mut cursors = CursorBook::new(identity, &["sh600000".parse().unwrap()]).unwrap();
        let mut afternoon = vec![update("sh600000", 150000)];
        cursors
            .remember_ordered(identity, 10, &mut afternoon)
            .unwrap();
        let mut morning = vec![update("sh600000", 93000)];
        cursors
            .remember_ordered(identity, 12, &mut morning)
            .unwrap();
        assert_eq!(morning.len(), 1);
        for sequence in [10, 11, 12] {
            let mut stale = vec![update("sh600000", 150001)];
            cursors
                .remember_ordered(identity, sequence, &mut stale)
                .unwrap();
            assert!(stale.is_empty());
        }
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 93000);
    }

    #[test]
    fn snapshot_fences_are_per_security_and_do_not_replace_server_cursors() {
        let identity = source(1, 0, 1);
        let first: SecurityId = "sh600000".parse().unwrap();
        let second: SecurityId = "sh600001".parse().unwrap();
        let mut cursors = CursorBook::new(identity, &[first.clone(), second.clone()]).unwrap();
        let mut activation = vec![update("sh600000", 93000), update("sh600001", 93001)];
        cursors
            .remember_ordered(identity, 2, &mut activation)
            .unwrap();
        cursors
            .snapshot_floors(&[(first, 10), (second, 20)])
            .unwrap();
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 93000);
        let mut between = vec![update("sh600000", 93002), update("sh600001", 93003)];
        cursors
            .remember_ordered(identity, 15, &mut between)
            .unwrap();
        assert_eq!(between.len(), 1);
        assert_eq!(between[0].id().number(), "600000");
        let requests = cursors.requests().unwrap();
        assert_eq!(requests[0].cursor(), 93002);
        assert_eq!(requests[1].cursor(), 93001);
    }

    #[test]
    fn snapshot_floor_identity_validation_is_atomic() {
        let identity = source(1, 0, 1);
        let id: SecurityId = "sh600000".parse().unwrap();
        let mut cursors = CursorBook::new(identity, std::slice::from_ref(&id)).unwrap();
        assert_eq!(
            cursors
                .snapshot_floors(&[(id, 10), ("sh600001".parse().unwrap(), 20)])
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol
        );
        let mut update = vec![update("sh600000", 93000)];
        cursors.remember_ordered(identity, 5, &mut update).unwrap();
        assert_eq!(
            update.len(),
            1,
            "an invalid floor set must not partly fence the first security"
        );
    }
}
