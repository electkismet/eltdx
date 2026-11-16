//! Explicit HQ board sources; constructing these types performs no discovery,
//! filesystem access, downloads or writes.
use std::path::{Path, PathBuf};
use std::str::FromStr;
use bytes::Bytes;
use crate::{Error, ErrorKind, Result};
use super::{Amount, HqMarket, Price, Quantity, Ratio, ResultMeta, SecurityId, Snapshot, TradingDate};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoardCategory { #[default] Concept, Style, Region, Industry1, Industry2, Industry3, Industry880, All }

/// A board lookup key, distinct from a security identity. A bare six-digit key
/// accepts the source's market; an explicit prefix restricts the lookup.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoardId { number: String, market: Option<HqMarket> }
impl BoardId {
    pub fn new(number: &str, market: Option<HqMarket>) -> Result<Self> {
        if number.len() != 6 || !number.bytes().all(|byte| byte.is_ascii_digit()) { return Err(invalid("board number must contain six ASCII digits")); }
        Ok(Self { number: number.to_owned(), market })
    }
    pub fn number(&self) -> &str { &self.number }
    pub const fn market(&self) -> Option<HqMarket> { self.market }
}
impl FromStr for BoardId {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.len() == 6 { Self::new(value, None) }
        else { let id: SecurityId = value.parse()?; Self::new(id.number(), Some(id.market())) }
    }
}

/// User-supplied bytes take precedence over explicitly selected directories
/// and HQ downloads. Names are a fixed allowlist, never arbitrary paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardFile { name: String, data: Bytes }
impl BoardFile {
    pub fn new(name: &str, data: Bytes) -> Result<Self> {
        if !matches!(name, "tdxzs.cfg" | "tdxzs3.cfg" | "infoharbor_block.dat" | "tdxhy.cfg" | "base.dbf") {
            return Err(invalid("unsupported board source filename"));
        }
        if data.len() > MAX_BOARD_FILE_BYTES { return Err(limit("board source exceeds 32 MiB")); }
        Ok(Self { name: name.to_owned(), data })
    }
    pub fn name(&self) -> &str { &self.name }
    pub fn data(&self) -> &Bytes { &self.data }
}
pub(crate) const MAX_BOARD_FILE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_BOARD_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BoardQuery {
    category: BoardCategory, refresh: bool, data_dir: Option<PathBuf>, definitions_dir: Option<PathBuf>, files: Vec<BoardFile>,
}
impl BoardQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_category(mut self, value: BoardCategory) -> Result<Self> { self.category = value; Ok(self) }
    pub fn with_refresh(mut self, value: bool) -> Result<Self> { self.refresh = value; Ok(self) }
    pub fn with_data_dir(mut self, value: Option<PathBuf>) -> Result<Self> {
        validate_dir(value.as_deref())?; self.data_dir = value; Ok(self)
    }
    pub fn with_definitions_dir(mut self, value: Option<PathBuf>) -> Result<Self> {
        validate_dir(value.as_deref())?; self.definitions_dir = value; Ok(self)
    }
    pub fn with_file(mut self, file: BoardFile) -> Result<Self> {
        if let Some(existing) = self.files.iter_mut().find(|existing| existing.name == file.name) { *existing = file; }
        else { self.files.try_reserve(1).map_err(crate::services::allocation_error)?; self.files.push(file); }
        self.files.sort_by(|left, right| left.name.cmp(&right.name)); self.validate()?; Ok(self)
    }
    pub const fn category(&self) -> BoardCategory { self.category }
    pub const fn refresh(&self) -> bool { self.refresh }
    pub fn data_dir(&self) -> Option<&Path> { self.data_dir.as_deref() }
    pub fn definitions_dir(&self) -> Option<&Path> { self.definitions_dir.as_deref() }
    pub fn files(&self) -> &[BoardFile] { &self.files }
    pub(crate) fn validate(&self) -> Result<()> {
        validate_dir(self.data_dir())?; validate_dir(self.definitions_dir())?;
        let mut size = 0_usize;
        for file in &self.files { size = size.checked_add(file.data.len()).ok_or_else(|| limit("board source aggregate overflows"))?; }
        if size > MAX_BOARD_TOTAL_BYTES { return Err(limit("board sources exceed 64 MiB aggregate")); }
        Ok(())
    }
    pub(crate) fn source_key(&self) -> Self { Self { category: BoardCategory::All, refresh: false, ..self.clone() } }
}
fn validate_dir(value: Option<&Path>) -> Result<()> {
    if value.is_some_and(|path| path.as_os_str().is_empty()) { return Err(invalid("explicit board directory must not be empty")); }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardDefinition {
    pub(crate) id: BoardId, pub(crate) name: String, pub(crate) market: HqMarket,
    pub(crate) category: BoardCategory, pub(crate) category_raw: u16, pub(crate) membership_key: String,
}
impl BoardDefinition {
    pub fn id(&self) -> &BoardId { &self.id }
    pub fn name(&self) -> &str { &self.name }
    pub const fn market(&self) -> HqMarket { self.market }
    pub const fn category(&self) -> BoardCategory { self.category }
    pub const fn category_raw(&self) -> u16 { self.category_raw }
    pub fn membership_key(&self) -> &str { &self.membership_key }
    pub(crate) fn security_id(&self) -> Result<SecurityId> { SecurityId::new(self.market, self.id.number()) }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardQuote {
    pub(crate) board: BoardDefinition, pub(crate) security_id: SecurityId, pub(crate) quote: Option<Snapshot>, pub(crate) meta: ResultMeta,
}
impl BoardQuote {
    pub fn board(&self) -> &BoardDefinition { &self.board }
    pub fn security_id(&self) -> &SecurityId { &self.security_id }
    pub fn last(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::last) }
    pub fn pre_close(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::pre_close) }
    pub fn open(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::open) }
    pub fn high(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::high) }
    pub fn low(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::low) }
    pub fn change(&self) -> Option<Price> { self.quote.as_ref().map(Snapshot::change) }
    pub fn change_pct(&self) -> Option<Ratio> { self.quote.as_ref().and_then(Snapshot::change_pct) }
    pub fn amount(&self) -> Option<Amount> { self.quote.as_ref().map(Snapshot::amount) }
    pub fn volume(&self) -> Option<Quantity> { self.quote.as_ref().map(Snapshot::total) }
    pub fn quote(&self) -> Option<&Snapshot> { self.quote.as_ref() }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardMember { pub(crate) id: SecurityId, pub(crate) excluded_reason: Option<String> }
impl BoardMember {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub fn excluded_reason(&self) -> Option<&str> { self.excluded_reason.as_deref() }
}
#[derive(Clone, Debug, PartialEq)]
pub struct BoardMemberQuotes {
    pub(crate) board: BoardDefinition, pub(crate) prepared_date: TradingDate,
    pub(crate) raw_members: Vec<BoardMember>, pub(crate) display_members: Vec<BoardMember>, pub(crate) excluded_members: Vec<BoardMember>,
    pub(crate) rows: Vec<BoardQuote>, pub(crate) meta: ResultMeta,
}
impl BoardMemberQuotes {
    pub fn board(&self) -> &BoardDefinition { &self.board }
    /// Preparation context date. Local/in-memory file freshness is not proven
    /// by this value; source diagnostics retain that distinction.
    pub const fn prepared_date(&self) -> TradingDate { self.prepared_date }
    pub fn raw_members(&self) -> &[BoardMember] { &self.raw_members }
    pub fn display_members(&self) -> &[BoardMember] { &self.display_members }
    pub fn excluded_members(&self) -> &[BoardMember] { &self.excluded_members }
    pub fn rows(&self) -> &[BoardQuote] { &self.rows }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message) }
fn limit(message: &str) -> Error { Error::new(ErrorKind::LimitExceeded, message) }
