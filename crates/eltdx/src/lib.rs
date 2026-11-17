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
