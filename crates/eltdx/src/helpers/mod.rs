//! HQ business compositions and pure calculations. Network helpers share their
//! caller's operation context; pure calculations require neither Python nor I/O.
pub(crate) mod market;
pub(crate) mod service;
mod cache;
mod profile;
mod market_service;
mod auction;
mod daily;
mod stats_alignment;
mod shortline;
mod board_decode;
mod boards;
pub(crate) use cache::HelperCache;
pub use market::classify_fund_flow;
pub use service::HelperService;
