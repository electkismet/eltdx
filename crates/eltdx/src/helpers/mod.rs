//! HQ business compositions and pure calculations. Network helpers share their
//! caller's operation context; pure calculations require neither Python nor I/O.
mod market;
mod service;
pub use market::classify_fund_flow;
pub use service::HelperService;
