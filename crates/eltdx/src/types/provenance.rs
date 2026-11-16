use std::net::SocketAddr;

use bytes::Bytes;

use crate::error::ErrorKind;
use crate::types::TradingDate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EnrichmentStatus {
    Succeeded,
    NotRequested,
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    source: String,
    stage: String,
    status: EnrichmentStatus,
    error_kind: Option<ErrorKind>,
    message: Option<String>,
}

impl Diagnostic {
    pub fn new(
        source: impl Into<String>,
        stage: impl Into<String>,
        status: EnrichmentStatus,
        error_kind: Option<ErrorKind>,
        message: Option<String>,
    ) -> Self {
        Self { source: source.into(), stage: stage.into(), status, error_kind, message }
    }

    pub fn source(&self) -> &str { &self.source }
    pub fn stage(&self) -> &str { &self.stage }
    pub const fn status(&self) -> EnrichmentStatus { self.status }
    pub const fn error_kind(&self) -> Option<ErrorKind> { self.error_kind }
    pub fn message(&self) -> Option<&str> { self.message.as_deref() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ValueProvenance {
    SecurityTable,
    WireRule,
    PrefixFallback,
    UserOverride,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceStamp {
    command: Option<u16>,
    endpoint: Option<SocketAddr>,
    session_id: String,
    metadata_version: Option<u64>,
    trading_date: Option<TradingDate>,
}

impl SourceStamp {
    pub fn new(
        command: Option<u16>,
        endpoint: Option<SocketAddr>,
        session_id: impl Into<String>,
        metadata_version: Option<u64>,
        trading_date: Option<TradingDate>,
    ) -> Self {
        Self { command, endpoint, session_id: session_id.into(), metadata_version, trading_date }
    }

    pub const fn command(&self) -> Option<u16> { self.command }
    pub const fn endpoint(&self) -> Option<SocketAddr> { self.endpoint }
    pub fn session_id(&self) -> &str { &self.session_id }
    pub const fn metadata_version(&self) -> Option<u64> { self.metadata_version }
    pub const fn trading_date(&self) -> Option<TradingDate> { self.trading_date }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RawData {
    payloads: Vec<Bytes>,
    wire_frames: Option<Vec<Bytes>>,
    row_ranges: Vec<(usize, usize, usize)>,
}

impl RawData {
    pub fn new(
        payloads: Vec<Bytes>,
        wire_frames: Option<Vec<Bytes>>,
        row_ranges: Vec<(usize, usize, usize)>,
    ) -> Self {
        Self { payloads, wire_frames, row_ranges }
    }

    pub fn payloads(&self) -> &[Bytes] { &self.payloads }
    pub fn wire_frames(&self) -> Option<&[Bytes]> { self.wire_frames.as_deref() }
    pub fn row_ranges(&self) -> &[(usize, usize, usize)] { &self.row_ranges }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResultMeta {
    source: SourceStamp,
    diagnostics: Vec<Diagnostic>,
    raw: Option<RawData>,
}

impl ResultMeta {
    pub fn new(source: SourceStamp, diagnostics: Vec<Diagnostic>, raw: Option<RawData>) -> Self {
        Self { source, diagnostics, raw }
    }

    pub fn source(&self) -> &SourceStamp { &self.source }
    pub fn diagnostics(&self) -> &[Diagnostic] { &self.diagnostics }
    pub fn raw(&self) -> Option<&RawData> { self.raw.as_ref() }
}
