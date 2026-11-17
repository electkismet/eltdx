//! Per-connection legacy generation over the shared SessionOwner. This is an
//! external-I/O admission boundary, not a second socket or cleanup state machine.
use std::sync::Arc;
use std::time::Duration;

use crate::session::{SessionOperation, SessionOwner};
use crate::{Error, ErrorKind, Result};

/// One irreversible generation. Reconnecting creates a new value; cloning it
/// never revives a retired connection. It starts no worker, socket or timer.
#[derive(Clone, Debug)]
pub struct LegacyExtensionSession { owner: Arc<SessionOwner> }

impl Default for LegacyExtensionSession {
    fn default() -> Self { Self::new() }
}

impl LegacyExtensionSession {
    pub fn new() -> Self {
        // External adapters own their original I/O timeouts and close policy.
        // Shared close runs only after external close and the final admission
        // have completed, with no wait budget imposed on Python network work.
        Self { owner: Arc::new(SessionOwner::new(Duration::ZERO, Duration::ZERO)) }
    }

    pub fn generation(&self) -> usize { self.owner.generation() }

    pub fn admit(&self) -> Result<LegacyExtensionOperation> {
        Ok(LegacyExtensionOperation { session: self.clone(), admission: self.owner.admit()? })
    }

    pub fn ensure_open(&self) -> Result<()> { self.owner.ensure_open() }

    /// Call only after the adapter has closed the captured physical resource.
    /// Existing admissions remain owned until they unwind, but cannot publish
    /// a result after this boundary. A same-thread reentrant close is safe.
    pub fn retire(&self) -> Result<()> { self.owner.retire_external() }
}

#[derive(Debug)]
pub struct LegacyExtensionOperation {
    session: LegacyExtensionSession,
    admission: SessionOperation,
}

impl LegacyExtensionOperation {
    pub fn generation(&self) -> usize { self.session.generation() }

    pub fn check(&self) -> Result<()> {
        if self.admission.is_cancelled() {
            return Err(Error::new(ErrorKind::Closed, "extension connection generation has retired"));
        }
        self.session.ensure_open()
    }

    /// Result delivery and retirement use the same owner admission mutex.
    pub fn commit<T>(&self, value: T) -> Result<T> {
        self.session.owner.with_open_admission(|| Ok(value))
    }
}

#[cfg(test)]
mod source_cases {
    use super::*;

    #[test]
    fn retirement_seals_old_handles_and_reopen_is_a_distinct_owner() -> Result<()> {
        let first = LegacyExtensionSession::new();
        let old = first.clone();
        let operation = first.admit()?;
        let id = operation.generation();
        first.retire()?;
        assert_eq!(operation.check().unwrap_err().kind(), ErrorKind::Closed);
        assert_eq!(operation.commit(1).unwrap_err().kind(), ErrorKind::Closed);
        assert_eq!(old.admit().unwrap_err().kind(), ErrorKind::Closed);
        drop(operation);
        let next = LegacyExtensionSession::new();
        assert_ne!(id, next.generation());
        assert_eq!(next.admit()?.commit(5)?, 5);
        assert!(old.ensure_open().is_err());
        Ok(())
    }

    #[test]
    fn admitted_work_keeps_the_generation_owned_until_after_reentrant_retire() -> Result<()> {
        let session = LegacyExtensionSession::new();
        let first = session.admit()?;
        let second = session.admit()?;
        session.retire()?;
        session.retire()?;
        drop(session);
        assert!(first.check().is_err());
        drop(first);
        assert!(second.check().is_err());
        drop(second);
        Ok(())
    }

    #[test]
    fn independent_protocol_owners_and_committed_values_are_not_invalidated() -> Result<()> {
        let ex = LegacyExtensionSession::new();
        let mac = LegacyExtensionSession::new();
        let committed = ex.admit()?.commit(vec![1, 2, 3])?;
        ex.retire()?;
        assert_eq!(committed, [1, 2, 3]);
        assert_eq!(mac.admit()?.commit(7)?, 7);
        Ok(())
    }
}
