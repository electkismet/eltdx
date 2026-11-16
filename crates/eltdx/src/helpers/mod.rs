//! HQ business compositions and pure calculations. Network helpers share their
//! caller's operation context; pure calculations require neither Python nor I/O.
pub(crate) mod market;
pub(crate) mod service;
mod cache;
pub(crate) mod calculation;
mod profile;
mod market_service;
mod auction;
mod daily;
pub(crate) mod stats_alignment;
mod shortline;
mod board_decode;
mod boards;
#[cfg(feature = "f10")]
mod f10;
pub(crate) use cache::HelperCache;
pub use market::classify_fund_flow;
pub use service::HelperService;
