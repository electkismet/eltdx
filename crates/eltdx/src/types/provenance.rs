use std::net::SocketAddr;

use bytes::Bytes;

use crate::error::ErrorKind;
use crate::types::TradingDate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum EnrichmentStatus {
    Succeeded,
    NotRequested,
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Diagnostic {
    source: String,
    stage: String,
    status: EnrichmentStatus,
    error_kind: Option<ErrorKind>,
    message: Option<String>,
}

impl Diagnostic {
    fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> crate::Result<()> {
        charge.string(&self.source)?;
        charge.string(&self.stage)?;
        if let Some(message) = &self.message { charge.string(message)?; }
        Ok(())
    }
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceStamp {
    command: Option<u16>,
    endpoint: Option<SocketAddr>,
    session_id: String,
    metadata_version: Option<u64>,
    trading_date: Option<TradingDate>,
}

impl SourceStamp {
    fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> crate::Result<()> {
        charge.string(&self.session_id)
    }
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RawData {
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_payloads"))]
    payloads: Vec<Bytes>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_wire_frames"))]
    wire_frames: Option<Vec<Bytes>>,
    row_ranges: Vec<(usize, usize, usize)>,
    xor93_rows: Vec<bool>,
}

impl RawData {
    fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> crate::Result<()> {
        charge.array::<Bytes>(self.payloads.capacity())?;
        for payload in &self.payloads { charge.buffer(payload)?; }
        if let Some(frames) = &self.wire_frames {
            charge.array::<Bytes>(frames.capacity())?;
            for frame in frames { charge.buffer(frame)?; }
        }
        charge.array::<(usize, usize, usize)>(self.row_ranges.capacity())?;
        charge.array::<bool>(self.xor93_rows.capacity())?;
        Ok(())
    }
    pub fn new(
        payloads: Vec<Bytes>,
        wire_frames: Option<Vec<Bytes>>,
        row_ranges: Vec<(usize, usize, usize)>,
    ) -> Self {
        let xor93_rows = vec![false; row_ranges.len()];
        Self { payloads, wire_frames, row_ranges, xor93_rows }
    }

    pub(crate) fn with_encodings(mut self, xor93_rows: Vec<bool>) -> Self {
        self.xor93_rows = xor93_rows;
        self
    }

    pub fn payloads(&self) -> &[Bytes] { &self.payloads }
    pub fn wire_frames(&self) -> Option<&[Bytes]> { self.wire_frames.as_deref() }
    pub fn row_ranges(&self) -> &[(usize, usize, usize)] { &self.row_ranges }
    /// Refresh ranges address XOR-encoded bytes in the retained original payload.
    pub fn row_requires_xor93(&self, index: usize) -> Option<bool> { self.xor93_rows.get(index).copied() }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ResultMeta {
    source: SourceStamp,
    sources: Vec<SourceStamp>,
    diagnostics: Vec<Diagnostic>,
    raw: Option<RawData>,
}

#[cfg(feature = "serde")]
fn serialize_payloads<S: serde::Serializer>(values: &[Bytes], serializer: S) -> std::result::Result<S::Ok, S::Error> {
    use serde::Serialize;
    values.iter().map(Bytes::as_ref).collect::<Vec<_>>().serialize(serializer)
}

#[cfg(feature = "serde")]
fn serialize_wire_frames<S: serde::Serializer>(values: &Option<Vec<Bytes>>, serializer: S) -> std::result::Result<S::Ok, S::Error> {
    use serde::Serialize;
    values.as_ref().map(|frames| frames.iter().map(Bytes::as_ref).collect::<Vec<_>>()).serialize(serializer)
}

impl ResultMeta {
    pub(crate) fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> crate::Result<()> {
        self.source.charge_heap(charge)?;
        charge.array::<SourceStamp>(self.sources.capacity())?;
        for source in &self.sources { source.charge_heap(charge)?; }
        charge.array::<Diagnostic>(self.diagnostics.capacity())?;
        for diagnostic in &self.diagnostics { diagnostic.charge_heap(charge)?; }
        if let Some(raw) = &self.raw { raw.charge_heap(charge)?; }
        Ok(())
    }
    pub fn new(source: SourceStamp, diagnostics: Vec<Diagnostic>, raw: Option<RawData>) -> Self {
        Self { sources: vec![source.clone()], source, diagnostics, raw }
    }

    pub fn source(&self) -> &SourceStamp { &self.source }
    pub fn sources(&self) -> &[SourceStamp] { &self.sources }
    pub fn diagnostics(&self) -> &[Diagnostic] { &self.diagnostics }
    pub fn raw(&self) -> Option<&RawData> { self.raw.as_ref() }

    pub(crate) fn with_sources(mut self, sources: Vec<SourceStamp>) -> Self { self.sources = sources; self }

    pub(crate) fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic); self
    }

    pub(crate) fn with_trading_date(mut self, date: TradingDate) -> Self {
        self.source.trading_date = Some(date);
        for source in &mut self.sources { source.trading_date = Some(date); }
        self
    }

    /// Reorder only row references after a service orders its result records.
    /// Payloads, frames and source stamps keep their original response order.
    pub(crate) fn with_raw_row_order(mut self, order: &[usize]) -> crate::error::Result<Self> {
        if let Some(raw) = self.raw.as_mut() {
            if order.len() != raw.row_ranges.len() {
                return Err(crate::error::Error::new(ErrorKind::Internal,
                    "result row order differs from retained raw row count"));
            }
            let mut ranges = Vec::new();
            let mut encodings = Vec::new();
            ranges.try_reserve(order.len()).map_err(raw_allocation_error)?;
            encodings.try_reserve(order.len()).map_err(raw_allocation_error)?;
            for &index in order {
                ranges.push(*raw.row_ranges.get(index).ok_or_else(|| crate::error::Error::new(
                    ErrorKind::Internal, "result row order contains an invalid source index"))?);
                encodings.push(raw.row_requires_xor93(index).unwrap_or(false));
            }
            raw.row_ranges = ranges;
            raw.xor93_rows = encodings;
        }
        Ok(self)
    }
}

fn raw_allocation_error(error: std::collections::TryReserveError) -> crate::error::Error {
    crate::error::Error::new(ErrorKind::LimitExceeded, "unable to reserve raw row mapping storage")
        .with_source(error)
}

impl Default for ResultMeta {
    fn default() -> Self {
        Self::new(SourceStamp::new(None, None, String::new(), None, None), Vec::new(), None)
    }
}
