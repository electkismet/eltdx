#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod commands;
pub mod context;
pub mod error;
pub mod frame;
pub mod limits;
pub mod request;
pub mod response;
pub mod unit;

#[cfg(test)]
mod round2_tests;

pub use context::{
    ParseOptions, ParseRawPolicy, PricePrecisionSource, RecordEncoding, RecordRange,
};
pub use error::ProtocolError;
pub use request::CommandRequest;
pub use response::CommandResponse;
