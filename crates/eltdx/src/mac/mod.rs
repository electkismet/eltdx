//! Independent Rust MAC and MAC-EX clients and source-preserving codecs.
//! MAC-EX production login reply evidence is an explicit configuration input;
//! the current source-only defaults do not establish production host support.
pub mod bitmap;
mod boards;
mod catalog;
mod client;
pub(crate) mod collect;
pub mod commands;
pub mod config;
pub mod decode;
mod download;
pub mod enums;
pub mod ex_query;
mod ex_services;
pub mod fields;
pub mod handshake;
pub mod layout;
mod market_data;
pub mod models;
pub(crate) mod pagination;
mod profile;
#[doc(hidden)]
pub mod python_legacy;
pub mod query;
mod quote;
pub mod responses;
pub mod routing;
pub mod services;
mod session;
pub mod util;

pub use crate::ex::catalog::{CatalogKind, CatalogSnapshot, CATALOG_SCHEMA_VERSION};
pub use crate::ex::frame::FrameLimits;
pub use crate::ex::hosts::{
    DiagnosticSnapshot, DiscoveryOptions, Endpoint, HostProbe, ProbeMode, ProbeOptions, ProbeStatus,
};
pub use crate::ex::models::{PageCompletion, PageResult};
pub use bitmap::{FieldId, FieldSelection, PresetField};
pub use boards::{BoardChangeRank, BoardRank, BoardSummary, NamedNativeField, NativeValue};
pub use catalog::{GoodsCatalog, InstrumentCatalog};
pub use client::{MacClient, MacClientBuilder, MacExClient, MacExClientBuilder};
pub use config::{MacConfig, MacExConfig};
pub use enums::{
    BoardRankKey, BoardSymbol, BoardType, Category, ExBoardType, FilterMask, FilterType,
    QuoteListOrder, SortOrder, SortType,
};
pub use fields::KnownField;
pub use market_data::{
    Bar, BarPrice, BarPriceSource, BarTimeSource, CountPresence, Transaction, TransactionSide,
    TransactionSource,
};
pub use models::{
    MacAdjust, MacCode, MacInstrumentId, MacMarketId, MacPeriod, RemoteFileName, WireFloat,
};
pub use query::*;
pub use quote::Quote;
pub use responses::*;
pub use services::{XdxrBatch, XdxrContext, XdxrProvider, XdxrSourceRequest};
pub use util::{AdjustmentEvent, XdxrEvent};
