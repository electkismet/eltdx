#![forbid(unsafe_code)]

mod client;
mod config;
mod hq;
mod session;
mod operation;
mod policy;
#[doc(hidden)]
pub mod __python_compat;
pub mod error;
pub mod types;

pub use client::{ClientBuilder, TdxClient};
pub use config::ClientConfig;
pub use operation::{CancellationToken, OperationOptions};
pub use session::PinnedSession;
pub use error::{Error, ErrorContext, ErrorKind, Result, TimeoutPhase};
pub use types::batch::{
    BatchItem, BatchItemState, BatchOutcome, MissingPolicy, MissingReason, SnapshotBatch,
};
pub use types::numeric::{
    Amount, AmountUnit, NumericValue, Price, PriceSource, Quantity, QuantityUnit, Ratio,
    RatioUnit, RoundingMode,
};
pub use types::raw::RawPolicy;
pub use types::security::{HqMarket, SecurityId};
pub use types::time::{EventTime, EventTimeSource, TradingDate};
