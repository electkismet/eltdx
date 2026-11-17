#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod deadline;
pub mod diagnostics;
pub mod endpoint;
pub mod engine;
pub mod error;
pub mod execution;
pub mod memory;
pub mod pin;
pub mod push;
pub mod request;
pub mod slot;
pub mod supervisor;

#[cfg(all(test, feature = "loom"))]
mod loom_tests;

pub use engine::{Engine, EngineConfig, EngineConfigOptions, PendingRawExecution};
pub use error::RuntimeError;
pub use execution::{RawReply, SubmissionOptions};
