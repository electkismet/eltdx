//! Independent, optional TQLEX/F10 client and lossless JSON values.
//!
//! This module is available with `features = ["f10"]`. Public JSON values do
//! not depend on enabling the SDK's separate model-serialization feature.

mod config;
mod client;
mod json;
mod request;
mod response;
mod control;
mod io;
mod resolver;
mod http;
mod worker;
mod values;
mod clock;
mod endpoints;
pub mod news;
pub mod fields;

pub use config::{F10Config, Header, ProxyUrl, ResolverPolicy, Route, TlsPolicy};
pub use client::{F10Client, F10ClientBuilder, F10Diagnostics};
pub use json::{JsonNumber, JsonValue, OrderedJsonObject};
pub use request::{CallQuery, EntryName, ParamsQuery, RequestPayload};
pub use response::{Cell, Response, ResultSet, Row};
pub use values::{AshareCode, BondCode, CodeUse, DateOrWindow, F10MarketCode, FundWindow, FuturesCode,
    HongKongCode, IndustryCode, PageSelector, PageSizeSelector, RequestId, SectionCode, ThirdBoardCode, UsTicker};
pub use endpoints::*;
pub use news::{NewsChannel, NewsChannelInfo, NewsHomeTab};
pub use clock::{LocalDateClock, SystemLocalDateClock};

/// Parse an already obtained TQLEX object without network work. Original HTTP
/// bytes are only available on client responses; this parser retains table
/// JSON according to the requested policy.
pub fn parse_response(entry: &EntryName, request: &RequestPayload, json: JsonValue, raw: crate::RawPolicy) -> crate::Result<Response> {
    Response::from_json(entry.as_str().to_owned(), request.retained(&mut || Ok(()))?, json,
        raw != crate::RawPolicy::Omit, crate::types::ResultMeta::default(), &mut || Ok(()))
}
