//! Independent, optional TQLEX/F10 client and lossless JSON values.
//!
//! This module is available with `features = ["f10"]`. Public JSON values do
//! not depend on enabling the SDK's separate model-serialization feature.

mod config;
mod json;
mod request;
mod response;

pub use config::{F10Config, Header, ProxyUrl, ResolverPolicy, Route, TlsPolicy};
pub use json::{JsonNumber, JsonValue, OrderedJsonObject};
pub use request::{CallQuery, EntryName, ParamsQuery, RequestPayload};
pub use response::{Cell, Response, ResultSet, Row};
