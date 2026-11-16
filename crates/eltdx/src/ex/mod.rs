//! Independent EX client, checked identities and bounded wire codecs.
mod client;
pub(crate) mod control;
mod probe;
pub(crate) mod profile;
pub(crate) mod session;
pub(crate) mod transport;
pub(crate) mod worker;
pub mod commands;
pub mod config;
pub mod frame;
pub mod hosts;
pub mod models;
pub mod query;
pub mod time;

pub use models::*;
pub use client::{ExClient, ExClientBuilder};
pub use config::ExConfig;
pub use query::*;
pub use time::TimeOfDay;

use crate::{Error, ErrorKind};

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "ex")
}
pub(crate) fn protocol(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Protocol, message).with_context("domain", "ex")
}
