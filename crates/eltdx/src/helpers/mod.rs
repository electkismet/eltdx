//! HQ business compositions and pure calculations. Network helpers share their
//! caller's operation context; pure calculations require neither Python nor I/O.
mod auction;
mod board_decode;
mod boards;
mod cache;
pub(crate) mod calculation;
mod daily;
#[cfg(feature = "f10")]
mod f10;
pub(crate) mod market;
mod market_service;
mod profile;
pub(crate) mod service;
mod shortline;
pub(crate) mod stats_alignment;
#[cfg(feature = "f10")]
pub mod theme;
#[cfg(feature = "f10")]
pub mod topics;
pub(crate) use cache::HelperCache;
pub use market::classify_fund_flow;
pub use service::HelperService;
#[cfg(feature = "f10")]
pub use theme::{ThemeStrengthQuery, ThemeStrengthRow, ThemeStrengthTable, ThemeTopicSource};
#[cfg(feature = "f10")]
pub use topics::{
    StockTopic, StockTopics, StockTopicsQuery, TopicSource, TopicStock, TopicStockTable,
    TopicStocksQuery,
};
