use std::sync::Arc;

use eltdx_runtime::engine::Engine;

use crate::error::Result;
use crate::session::{SessionOperation, SessionOwner};

/// Owner-bound entry point for HQ operations. It holds no runtime worker itself.
#[derive(Clone, Debug)]
pub(crate) struct HqExecutor {
    owner: Arc<SessionOwner>,
}

impl HqExecutor {
    pub(crate) fn new(owner: Arc<SessionOwner>) -> Self {
        Self { owner }
    }

    pub(crate) fn close(&self) -> Result<()> {
        self.owner.close()
    }

    pub(crate) fn begin_operation(&self) -> Result<SessionOperation> {
        self.owner.admit()
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        self.owner.ensure_open()
    }

    pub(crate) fn register_engine(&self, engine: Engine) -> Result<()> {
        self.owner.register_engine(engine)
    }
}
