//! Independent, optional TQLEX/F10 client and lossless JSON values.
//!
//! This module is available with `features = ["f10"]`. Public JSON values do
//! not depend on enabling the SDK's separate model-serialization feature.

mod client;
mod clock;
#[doc(hidden)]
pub mod collection;
mod config;
mod control;
mod endpoints;
pub mod fields;
mod http;
mod io;
mod json;
pub mod news;
#[doc(hidden)]
pub mod python_legacy;
mod request;
mod resolver;
mod response;
mod values;
mod worker;

pub use crate::services::{
    FundFlowHistoryRow, FundFlowRangeSeries, HistoryDays, MoneyFlowHistoryQuery,
    MoneyFlowHistoryService,
};
pub use client::{F10Client, F10ClientBuilder, F10Diagnostics};
pub use clock::{LocalDateClock, SystemLocalDateClock};
pub use config::{F10Config, Header, ProxyUrl, ResolverPolicy, Route, TlsPolicy};
pub use endpoints::*;
pub use json::{JsonNumber, JsonValue, OrderedJsonObject};
pub use news::{NewsChannel, NewsChannelInfo, NewsHomeTab};
pub use request::{CallQuery, EntryName, ParamsQuery, RequestPayload};
pub use response::{Cell, Response, ResultSet, Row};
pub use values::{
    AshareCode, BondCode, CodeUse, DateOrWindow, F10MarketCode, FundWindow, FuturesCode,
    HongKongCode, IndustryCode, PageSelector, PageSizeSelector, RequestId, SectionCode,
    ThirdBoardCode, UsTicker,
};

/// Parse an already obtained TQLEX object without network work. Original HTTP
/// bytes are only available on client responses; this parser retains table
/// JSON according to the requested policy.
pub fn parse_response(
    entry: &EntryName,
    request: &RequestPayload,
    json: JsonValue,
    raw: crate::RawPolicy,
) -> crate::Result<Response> {
    Response::from_json(
        entry.as_str().to_owned(),
        request.retained(&mut || Ok(()))?,
        json,
        raw != crate::RawPolicy::Omit,
        crate::types::ResultMeta::default(),
        &mut || Ok(()),
    )
}
