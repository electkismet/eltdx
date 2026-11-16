//! Pure EX identities, query validation and wire codecs.
//!
//! This module contains no socket, owner, automatic configuration loading or
//! retries. Registration and operation-aware transport belong to T24 Stage B.
pub mod commands;
pub mod config;
pub mod frame;
pub mod hosts;
pub mod models;
pub mod query;
pub mod time;

pub use models::*;
pub use query::*;
pub use time::TimeOfDay;

use crate::{Error, ErrorKind};

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "ex")
}
pub(crate) fn protocol(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Protocol, message).with_context("domain", "ex")
}
