//! Independent EX client, checked identities and bounded wire codecs.
#[cfg(feature = "mac")]
pub(crate) mod catalog;
mod client;
pub mod commands;
pub mod config;
pub(crate) mod control;
pub mod frame;
pub mod hosts;
pub mod layout;
#[doc(hidden)]
pub mod legacy;
pub mod models;
pub(crate) mod probe;
pub(crate) mod profile;
pub mod query;
pub(crate) mod session;
pub mod time;
pub(crate) mod transport;
pub(crate) mod worker;

pub use client::{ExClient, ExClientBuilder};
pub use config::ExConfig;
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
