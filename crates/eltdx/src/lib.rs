//! Independent synchronous TDX SDK and shared Rust business implementation.
//!
//! Rust consumers depend only on this crate and its Rust dependencies. They do
//! not install Python, PyO3, maturin or a Python server. The separately shipped
//! Python wheel embeds bindings to the same business code. This unpublished
//! source candidate still requires the repository's full acceptance matrix.
//!
//! # Construction and ownership
//!
//! [`ClientBuilder::build`] validates configuration without DNS, sockets, user
//! files or background threads. Service getters are lazy. Network work begins
//! with an explicit connect or business call. A clone shares the same owner;
//! successful [`TdxClient::close`] permanently closes that owner and its service,
//! pin and subscription handles. Create another client for an independent
//! session. Explicit close reports cleanup failures; Drop is a fallback and is
//! not a substitute for observing a close result.
//!
//! ```
//! use eltdx::{Result, TdxClient};
//!
//! # fn main() -> Result<()> {
//! let client = TdxClient::builder().build()?;
//! let service = client.codes();
//! let shared_owner = client.clone();
//! shared_owner.close()?;
//! client.close()?; // idempotent; this does not reopen the owner
//! drop(service);
//! # Ok(())
//! # }
//! ```
//!
//! # Explicit network work and cleanup
//!
//! This example is compiled but must not query a server during documentation
//! tests. Supply an endpoint you are authorized to query when running it.
//!
//! ```no_run
//! use std::time::Duration;
//! use eltdx::{CodeQuery, HqMarket, OperationOptions, Result, TdxClient};
//!
//! # fn main() -> Result<()> {
//! let endpoint = "127.0.0.1:7709".parse().expect("numeric endpoint literal");
//! let client = TdxClient::builder().endpoints(vec![endpoint]).build()?;
//! let options = OperationOptions::default().with_timeout(Duration::from_secs(8));
//! let response = client.codes().list(HqMarket::Shanghai, &CodeQuery::new(), &options);
//! let cleanup = client.close();
//! match (response, cleanup) {
//!     (Err(primary), Err(cleanup)) => {
//!         eprintln!("close also failed: {cleanup}");
//!         return Err(primary);
//!     }
//!     (Err(primary), _) => return Err(primary),
//!     (Ok(_), Err(cleanup)) => return Err(cleanup),
//!     (Ok(page), Ok(())) => println!("{} securities", page.items().len()),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! | Selection | Public domain |
//! | --- | --- |
//! | Default, empty | HQ codes, quotes, bars, trades, company/resources and HQ helpers |
//! | `serde` | Serde implementations on the public types which explicitly provide them |
//! | `offline` | Local market files and professional financial DAT/ZIP/services |
//! | `f10` | TQLEX HTTP client, lossless tables, named endpoints and F10 helpers |
//! | `ex` | EX transport, configuration, queries and responses |
//! | `mac` | MAC and MAC-EX, also enabling `ex` |
//! | `full` | All selections above |
//!
//! There is no public async feature. Internal Tokio runtimes implement the
//! synchronous contract; calls can block the calling thread. Applications must
//! not assume cancelling a wrapper task cancels its underlying request.
//!
//! # Deadlines, cancellation and errors
//!
//! [`OperationOptions`] starts its relative timeout at method entry. Startup,
//! queueing, metadata, pages and retries share that deadline and their finite
//! budgets. [`CancellationToken`] is irreversible and clones share the signal.
//! Finishing cancellation can consume a separate cleanup allowance. Inspect
//! [`Error::kind`], [`Error::context`] and the standard error source chain;
//! [`ErrorKind::CancelUnconfirmed`] preserves the original cause and a separate
//! [`Error::cleanup_failure`]. It never certifies that all work has retired.
//! Python compatibility entry points retain the older per-request policy.
//!
//! # Data meaning and retention
//!
//! [`Price`] distinguishes scaled integers from wire floats. `as_f64` is a
//! convenience projection; use exact conversion or explicit rounding when
//! precision matters. [`Quantity`], [`Amount`] and [`Ratio`] carry unit and
//! provenance labels; unknown units are not guessed. [`EventTime`] distinguishes
//! dated Shanghai/local values, date-only/time-only values and unknown raw
//! scalars. Source dates and adjusted bar labels remain separate facts.
//!
//! [`RawPolicy::Omit`] omits optional raw retention; it does not promise zero
//! allocation or zero copying. Retained payload and wire frames differ by
//! transport, and an F10 wire capture is HTTP plaintext, not TLS ciphertext.
//! Preserve result diagnostics, completion, cursors and per-response sources.
//! Short pages do not universally prove EOF; batch absence is explicit rather
//! than a zero-valued row. Subscription gaps invalidate synchronization until a
//! documented recovery, and retained events remain owned values after close.
//!
//! Enabling `serde` does not make every DTO deserializable or define a universal
//! JSON schema. Validated identities/dates keep checked constructors. Nonfinite
//! wire floats require explicit handling and cannot silently become JSON null.
//! Hidden compatibility modules are binding internals, outside the Rust API
//! stability promise. MAC-EX production login rules and optional MAC setup
//! association require independent evidence; synthetic test ACKs are not defaults.
//!
#![forbid(unsafe_code)]

mod client;
mod config;
mod encoding;
mod archive;
mod stats;
mod hq;
mod hosts;
pub mod services;
mod session;
mod subscription;
mod operation;
mod policy;
pub mod adjustment;
pub mod calendar;
pub mod helpers;
/// Approved limit-price policy evaluation with explicit unknown results.
pub mod price_limits { pub use crate::policy::limits::evaluate; }
/// Pure corporate-action factor calculation; usable without a client or Python.
pub mod equity { pub use crate::adjustment::build_adjustment_factors; }
mod metadata;
#[doc(hidden)]
pub mod __python_compat;
pub mod error;
pub mod types;
pub mod advanced;
#[cfg(feature = "f10")]
pub mod f10;
#[cfg(feature = "ex")]
pub mod ex;
#[cfg(feature = "mac")]
pub mod mac;
#[cfg(feature = "offline")]
pub mod offline;
#[cfg(feature = "offline")]
pub mod financial;

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
pub use types::corporate::{AdjustmentFactor, AdjustmentFactors, CapitalChangeBlock, CapitalEvent, CapitalQuery,
    FactorQuery, FinanceField, FinanceProjection, FinanceQuery, FinanceRecord, FinanceValue};
pub use types::limits::{LimitDecision, LimitRule, LimitStatus, SpecialLimit, SpecialLimitQuery};
pub use types::calendar::{CalendarQuery, CalendarSnapshot};
pub use helpers::HelperService;
#[cfg(feature = "f10")]
pub use helpers::{StockTopic, StockTopics, StockTopicsQuery, TopicSource, TopicStock, TopicStockTable, TopicStocksQuery};
#[cfg(feature = "f10")]
pub use helpers::{ThemeStrengthQuery, ThemeStrengthRow, ThemeStrengthTable, ThemeTopicSource};
pub use types::helpers::{AuctionData, AuctionDataQuery, DailyLimitsQuery, DailyPriceLimit, DailyShareCapital,
    DailySharesQuery, FullQuote, FundFlow, MarketStat, ProfileQuery, RankQuery, RealtimeRankRow, SecuritySelection, StockProfile};
pub use types::shortline::{LadderQuery, ShortlineIndicator, ShortlineQuery, ShortlineTable, StatsAlignment};
pub use types::boards::{BoardCategory, BoardDefinition, BoardFile, BoardId, BoardMember, BoardMemberQuotes, BoardQuery, BoardQuote};
pub use types::resources::{BlockMeta, DownloadCompletion, DownloadOptions, DownloadResult, StatRow, Stat2Row, StatsQuery, StatsResource};
pub use types::company_info::{CompanySection, CompanySectionQuery, CompanyText, Table};
pub use types::session::{HandshakeInfo, HeartbeatAck, HealthReport, PingReply};
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
