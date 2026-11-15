#![forbid(unsafe_code)]

mod client;
mod config;
pub mod error;
pub mod types;

pub use client::{ClientBuilder, TdxClient};
pub use config::ClientConfig;
pub use error::{Error, ErrorContext, ErrorKind, Result, TimeoutPhase};
pub use types::batch::{
    BatchItem, BatchItemState, BatchOutcome, MissingPolicy, MissingReason, SnapshotBatch,
};
pub use types::numeric::{
    Amount, AmountUnit, NumericValue, Price, Quantity, QuantityUnit, Ratio, RatioUnit,
};
pub use types::raw::RawPolicy;
pub use types::security::{HqMarket, SecurityId};
pub use types::time::{EventTime, TradingDate};
