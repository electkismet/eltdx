use std::collections::HashMap;

use eltdx_runtime::slot::GenerationIdentity;
use crate::error::{Error, ErrorKind, Result};
use crate::types::{QuoteUpdate, RefreshRequest, SecurityId};

/// One activation owns one immutable epoch/slot/TCP-generation binding.
/// Reconnection requires a new lease and book, never rebinding this book.
#[derive(Debug)]
pub(super) struct CursorBook {
    source: GenerationIdentity,
    order: Vec<SecurityId>,
    cursors: HashMap<SecurityId, u32>,
}

impl CursorBook {
    pub(super) fn new(source: GenerationIdentity, ids: &[SecurityId]) -> Result<Self> {
        let mut order = Vec::new(); let mut cursors = HashMap::new();
        order.try_reserve(ids.len()).map_err(allocation)?;
        cursors.try_reserve(ids.len()).map_err(allocation)?;
        for id in ids {
            if cursors.insert(id.clone(), 0).is_none() { order.push(id.clone()); }
        }
        Ok(Self { source, order, cursors })
    }

    pub(super) fn accepts(&self, source: GenerationIdentity) -> bool { self.source == source }

    /// Return false for stale source data before touching any cursor. Validate
    /// every identity first, so a corrupt frame cannot partly move this book.
    pub(super) fn remember(&mut self, source: GenerationIdentity, updates: &[QuoteUpdate]) -> Result<bool> {
        if !self.accepts(source) { return Ok(false); }
        for row in updates {
            if !self.cursors.contains_key(row.id()) {
                return Err(Error::new(ErrorKind::Protocol, "push contains an identity outside its activation"));
            }
        }
        for row in updates {
            // The most recently received value wins, including150000→93000.
            *self.cursors.get_mut(row.id()).expect("identity validated above") = row.cursor();
        }
        Ok(true)
    }

    pub(super) fn requests(&self) -> Result<Vec<RefreshRequest>> {
        let mut requests = Vec::new(); requests.try_reserve(self.order.len()).map_err(allocation)?;
        for id in &self.order {
            requests.push(RefreshRequest::new(id.clone()).with_cursor(self.cursors[id])?);
        }
        Ok(requests)
    }
}

fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "unable to reserve subscription cursor state").with_source(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::source_cases::update;
    use eltdx_runtime::slot::{EngineEpoch, GenerationId, SlotId};

    fn source(epoch: u64, slot: usize, generation: u64) -> GenerationIdentity {
        GenerationIdentity { engine_epoch: EngineEpoch::new(epoch).unwrap(), slot_id: SlotId::new(slot),
            generation: GenerationId::new(generation).unwrap() }
    }

    #[test]
    fn latest_source_cursor_can_move_backwards_into_the_next_morning() {
        let identity = source(1, 0, 1); let id: SecurityId = "sh600000".parse().unwrap();
        let mut cursors = CursorBook::new(identity, &[id.clone(), id]).unwrap();
        for cursor in [150000, 93000] { assert!(cursors.remember(identity, &[update("sh600000", cursor)]).unwrap()); }
        let requests = cursors.requests().unwrap();
        assert_eq!(requests.len(), 1); assert_eq!(requests[0].cursor(), 93000);
    }

    #[test]
    fn stale_epoch_slot_or_tcp_generation_cannot_change_a_cursor() {
        let identity = source(2, 3, 4); let id = "sh600000".parse().unwrap();
        let mut cursors = CursorBook::new(identity, &[id]).unwrap();
        cursors.remember(identity, &[update("sh600000", 93000)]).unwrap();
        for stale in [source(1, 3, 4), source(2, 2, 4), source(2, 3, 3)] {
            assert!(!cursors.remember(stale, &[update("sh600000", 150000)]).unwrap());
        }
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 93000);
    }

    #[test]
    fn invalid_identity_does_not_partly_commit_an_earlier_valid_row() {
        let identity = source(1, 0, 1);
        let mut cursors = CursorBook::new(identity, &["sh600000".parse().unwrap()]).unwrap();
        assert_eq!(cursors.remember(identity, &[update("sh600000", 150000), update("sh600001", 93000)])
            .unwrap_err().kind(), ErrorKind::Protocol);
        assert_eq!(cursors.requests().unwrap()[0].cursor(), 0);
    }
}
