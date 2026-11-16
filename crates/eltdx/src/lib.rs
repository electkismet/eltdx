#![forbid(unsafe_code)]

mod client;
mod config;
mod encoding;
mod hq;
mod hosts;
pub mod services;
mod session;
mod subscription;
mod operation;
mod policy;
mod metadata;
#[doc(hidden)]
pub mod __python_compat;
pub mod error;
pub mod types;
#[cfg(feature = "offline")]
pub mod offline;

pub use client::{ClientBuilder, TdxClient};
pub use config::ClientConfig;
pub use operation::{CancellationToken, OperationOptions};
pub use session::PinnedSession;
pub use subscription::Subscription;
pub use types::subscription::{GapPolicy, PushEvent, PushGap, SubscriptionOptions};
pub use error::{Error, ErrorContext, ErrorKind, Result, TimeoutPhase};
pub use types::batch::{
    BatchItem, BatchItemState, BatchOutcome, Collected, DataPage, EndEvidence, MissingPolicy,
    MissingReason, PageCursor, PageOptions, SnapshotBatch,
};
pub use types::codes::{CodeQuery, CodeScanQuery, Security, SecurityBoard, SecurityCategory};
pub use types::quotes::{
    CategoryQuery, CategoryQuote, LegacyQuote, QuoteCategory, QuoteLevel, QuoteSort, QuoteUpdate,
    QuoteUpdates, Snapshot,
};
pub use types::numeric::{
    Amount, AmountUnit, NumericValue, Price, PriceSource, Quantity, QuantityUnit, Ratio,
    RatioUnit, RoundingMode,
};
pub use types::raw::RawPolicy;
pub use types::security::{HqMarket, SecurityId};
pub use types::time::{EventTime, EventTimeSource, TradingDate};
pub use types::time::{Clock, SystemClock};
