//! Bounded server resources. An exhausted byte allowance is not EOF evidence.
use super::{Amount, Quantity, ResultMeta, SecurityId, TradingDate};
use crate::{Error, ErrorKind, Result};
use bytes::Bytes;
use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DownloadCompletion {
    Complete,
    LimitReached,
}

/// Shared bounded download policy. A server size is a hint, never EOF proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadOptions {
    max_bytes: u64,
    chunk_size: NonZeroU32,
    expected_size: Option<u64>,
    start_offset: u64,
    max_chunks: NonZeroU32,
}
impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            chunk_size: NonZeroU32::new(30_000).expect("positive chunk"),
            expected_size: None,
            start_offset: 0,
            max_chunks: NonZeroU32::new(4096).expect("positive chunk cap"),
        }
    }
}
impl DownloadOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }
    pub fn chunk_size(&self) -> NonZeroU32 {
        self.chunk_size
    }
    pub fn expected_size(&self) -> Option<u64> {
        self.expected_size
    }
    pub fn start_offset(&self) -> u64 {
        self.start_offset
    }
    pub fn max_chunks(&self) -> NonZeroU32 {
        self.max_chunks
    }
    pub fn with_max_bytes(mut self, value: u64) -> Result<Self> {
        self.max_bytes = value;
        self.validate_hq()?;
        Ok(self)
    }
    pub fn with_chunk_size(mut self, value: NonZeroU32) -> Result<Self> {
        self.chunk_size = value;
        self.validate_hq()?;
        Ok(self)
    }
    pub fn with_expected_size(mut self, value: Option<u64>) -> Result<Self> {
        self.expected_size = value;
        self.validate_hq()?;
        Ok(self)
    }
    pub fn with_start_offset(mut self, value: u64) -> Result<Self> {
        self.start_offset = value;
        self.validate_hq()?;
        Ok(self)
    }
    pub fn with_max_chunks(mut self, value: NonZeroU32) -> Result<Self> {
        self.max_chunks = value;
        self.validate_hq()?;
        Ok(self)
    }
    pub(crate) fn validate_hq(&self) -> Result<()> {
        if self.chunk_size.get() > 60_000
            || self.max_chunks.get() > 4096
            || self.start_offset > u64::from(u32::MAX)
            || self
                .expected_size
                .is_some_and(|value| value > u64::from(u32::MAX))
            || self
                .start_offset
                .checked_add(self.max_bytes)
                .is_none_or(|end| end > u64::from(u32::MAX))
        {
            return Err(invalid("download offsets/byte count must fit u32, chunk size <=60000 and max_chunks <=4096"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadResult {
    pub data: Bytes,
    pub completion: DownloadCompletion,
    pub next_offset: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockMeta {
    size: u32,
    md5: String,
    meta: ResultMeta,
}
impl BlockMeta {
    pub(crate) fn new(size: u32, md5: String, meta: ResultMeta) -> Self {
        Self { size, md5, meta }
    }
    pub const fn size(&self) -> u32 {
        self.size
    }
    /// Server-supplied checksum text; no content verification is implied.
    pub fn md5(&self) -> &str {
        &self.md5
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatsQuery {
    path: String,
    chunk_size: usize,
}
impl Default for StatsQuery {
    fn default() -> Self {
        Self {
            path: "zhb.zip".to_owned(),
            chunk_size: 30_000,
        }
    }
}
impl StatsQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_path(mut self, path: String) -> Result<Self> {
        self.path = normalize_path(&path)?;
        self.validate()?;
        Ok(self)
    }
    pub fn with_chunk_size(mut self, size: usize) -> Result<Self> {
        self.chunk_size = size;
        self.validate()?;
        Ok(self)
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub const fn chunk_size(&self) -> usize {
        self.chunk_size
    }
    pub(crate) fn validate(&self) -> Result<()> {
        normalize_path(&self.path)?;
        if !(1..=60_000).contains(&self.chunk_size) {
            return Err(invalid("chunk_size must be between 1 and 60000"));
        }
        Ok(())
    }
}

pub(crate) fn normalize_path(path: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() || path.len() > 300 || !path.is_ascii() || path.as_bytes().contains(&0) {
        return Err(invalid(
            "server path must contain 1..300 ASCII bytes without NUL",
        ));
    }
    Ok(path.replace('\\', "/"))
}

/// Source keys remain available when the market/code cannot form a SecurityId.
#[derive(Clone, Debug, PartialEq)]
pub struct StatRow {
    pub(crate) id: Option<SecurityId>,
    pub(crate) source_market: String,
    pub(crate) source_code: String,
    pub(crate) stats_date: Option<TradingDate>,
    pub(crate) stats_date_raw: Option<String>,
    pub(crate) beta_60d: Option<f64>,
    pub(crate) pe_ttm: Option<f64>,
    pub(crate) free_float_shares: Option<Quantity>,
    pub(crate) free_float_shares_source_10k: Option<f64>,
    pub(crate) year_limit_up_days: Option<u32>,
    pub(crate) limit_stat_days: Option<u32>,
    pub(crate) limit_up_count: Option<u32>,
    pub(crate) limit_streak: Option<u32>,
    pub(crate) source_line: usize,
    pub(crate) source_counts: [Option<String>; 4],
}
impl StatRow {
    pub fn id(&self) -> Option<&SecurityId> {
        self.id.as_ref()
    }
    pub fn source_market(&self) -> &str {
        &self.source_market
    }
    pub fn source_code(&self) -> &str {
        &self.source_code
    }
    pub const fn stats_date(&self) -> Option<TradingDate> {
        self.stats_date
    }
    pub fn stats_date_raw(&self) -> Option<&str> {
        self.stats_date_raw.as_deref()
    }
    pub const fn beta_60d(&self) -> Option<f64> {
        self.beta_60d
    }
    pub const fn pe_ttm(&self) -> Option<f64> {
        self.pe_ttm
    }
    pub const fn free_float_shares(&self) -> Option<Quantity> {
        self.free_float_shares
    }
    pub const fn free_float_shares_source_10k(&self) -> Option<f64> {
        self.free_float_shares_source_10k
    }
    pub const fn year_limit_up_days(&self) -> Option<u32> {
        self.year_limit_up_days
    }
    pub const fn limit_stat_days(&self) -> Option<u32> {
        self.limit_stat_days
    }
    pub const fn limit_up_count(&self) -> Option<u32> {
        self.limit_up_count
    }
    pub const fn limit_streak(&self) -> Option<u32> {
        self.limit_streak
    }
    pub const fn source_line(&self) -> usize {
        self.source_line
    }
    /// Exact decimal Python int(finite_float) values, before checked u32 projection.
    pub fn source_counts(&self) -> &[Option<String>; 4] {
        &self.source_counts
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stat2Row {
    pub(crate) id: Option<SecurityId>,
    pub(crate) source_market: String,
    pub(crate) source_code: String,
    pub(crate) stats_date: Option<TradingDate>,
    pub(crate) stats_date_raw: Option<String>,
    pub(crate) amount: Option<Amount>,
    pub(crate) seal: Option<Amount>,
    pub(crate) previous_amount: Option<Amount>,
    pub(crate) previous_seal: Option<Amount>,
    pub(crate) previous2_amount: Option<Amount>,
    pub(crate) previous2_seal: Option<Amount>,
    pub(crate) open_amount: Option<Amount>,
    pub(crate) previous_open_amount: Option<Amount>,
    pub(crate) open_volume: Option<Quantity>,
    pub(crate) previous_open_volume: Option<Quantity>,
    pub(crate) source_line: usize,
    pub(crate) source_values: [Option<f64>; 10],
}
impl Stat2Row {
    pub fn id(&self) -> Option<&SecurityId> {
        self.id.as_ref()
    }
    pub fn source_market(&self) -> &str {
        &self.source_market
    }
    pub fn source_code(&self) -> &str {
        &self.source_code
    }
    pub const fn stats_date(&self) -> Option<TradingDate> {
        self.stats_date
    }
    pub fn stats_date_raw(&self) -> Option<&str> {
        self.stats_date_raw.as_deref()
    }
    pub const fn amount(&self) -> Option<Amount> {
        self.amount
    }
    pub const fn seal(&self) -> Option<Amount> {
        self.seal
    }
    pub const fn previous_amount(&self) -> Option<Amount> {
        self.previous_amount
    }
    pub const fn previous_seal(&self) -> Option<Amount> {
        self.previous_seal
    }
    pub const fn previous2_amount(&self) -> Option<Amount> {
        self.previous2_amount
    }
    pub const fn previous2_seal(&self) -> Option<Amount> {
        self.previous2_seal
    }
    pub const fn open_amount(&self) -> Option<Amount> {
        self.open_amount
    }
    pub const fn previous_open_amount(&self) -> Option<Amount> {
        self.previous_open_amount
    }
    pub const fn open_volume(&self) -> Option<Quantity> {
        self.open_volume
    }
    pub const fn previous_open_volume(&self) -> Option<Quantity> {
        self.previous_open_volume
    }
    pub const fn source_line(&self) -> usize {
        self.source_line
    }
    /// Source order: amount/seal/previous amount/seal/previous2 amount/seal,
    /// opening/previous opening volume (hands), opening/previous opening amount.
    /// Monetary values retain the original ten-thousand-CNY unit.
    pub fn source_values(&self) -> &[Option<f64>; 10] {
        &self.source_values
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatsResource {
    pub(crate) stat: Vec<StatRow>,
    pub(crate) stat2: Vec<Stat2Row>,
    pub(crate) source_path: String,
    pub(crate) meta: ResultMeta,
}
impl StatsResource {
    pub fn stat(&self) -> &[StatRow] {
        &self.stat
    }
    pub fn stat2(&self) -> &[Stat2Row] {
        &self.stat2
    }
    pub fn source_path(&self) -> &str {
        &self.source_path
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
    pub fn stat_for(&self, id: &SecurityId) -> Option<&StatRow> {
        self.stat.iter().find(|row| row.id() == Some(id))
    }
    pub fn stat2_for(&self, id: &SecurityId) -> Option<&Stat2Row> {
        self.stat2.iter().find(|row| row.id() == Some(id))
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message)
}
