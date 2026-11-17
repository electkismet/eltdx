//! Independent Rust MAC and MAC-EX clients and source-preserving codecs.
//! MAC-EX production login reply evidence is an explicit configuration input;
//! the current source-only defaults do not establish production host support.
mod client;
mod profile;
mod session;
mod quote;
pub mod config;
pub mod bitmap;
pub mod commands;
pub mod models;
pub mod routing;
pub mod responses;
pub mod decode;
pub mod enums;
pub mod query;
pub mod ex_query;
pub mod handshake;
pub(crate) mod pagination;

pub use client::{MacClient, MacClientBuilder, MacExClient, MacExClientBuilder};
pub use config::{MacConfig, MacExConfig};
pub use quote::Quote;
pub use crate::ex::hosts::{DiagnosticSnapshot, DiscoveryOptions, Endpoint, HostProbe, ProbeMode, ProbeOptions, ProbeStatus};
pub use crate::ex::frame::FrameLimits;
pub use bitmap::{FieldId, FieldSelection, PresetField};
pub use models::{MacAdjust, MacCode, MacInstrumentId, MacMarketId, MacPeriod, RemoteFileName, WireFloat};
pub use responses::*;
pub use enums::{BoardRankKey, BoardSymbol, BoardType, Category, ExBoardType, FilterMask, FilterType, QuoteListOrder, SortOrder, SortType};
pub use crate::ex::models::{PageCompletion, PageResult};
pub use query::*;
