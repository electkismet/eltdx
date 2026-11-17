//! Shortline source alignment and derived indicators. Percentage points and
//! ordinary fractions have distinct RatioUnit values; source counters stay raw.
use super::{
    Amount, LimitStatus, Price, Quantity, Ratio, ResultMeta, SecurityId, SecuritySelection,
    StatsQuery, TradingDate,
};
use crate::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatsAlignment {
    SameDay,
    PreviousTradingDay,
    RowMissing,
    DateUnaligned,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortlineQuery {
    ids: Vec<SecurityId>,
    stats: StatsQuery,
    refresh_stats: bool,
}
impl ShortlineQuery {
    pub fn new(ids: Vec<SecurityId>) -> Result<Self> {
        let query = Self {
            ids,
            stats: StatsQuery::default(),
            refresh_stats: false,
        };
        query.validate()?;
        Ok(query)
    }
    pub fn with_stats(mut self, value: StatsQuery) -> Result<Self> {
        value.validate()?;
        self.stats = value;
        Ok(self)
    }
    pub fn with_refresh_stats(mut self, value: bool) -> Result<Self> {
        self.refresh_stats = value;
        Ok(self)
    }
    pub fn ids(&self) -> &[SecurityId] {
        &self.ids
    }
    pub fn stats(&self) -> &StatsQuery {
        &self.stats
    }
    pub const fn refresh_stats(&self) -> bool {
        self.refresh_stats
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.ids.is_empty() {
            return Err(invalid("shortline ids must not be empty"));
        }
        crate::services::checked_ids(&self.ids)?;
        self.stats.validate()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LadderQuery {
    selection: SecuritySelection,
    include_touched: bool,
    count: Option<usize>,
    stats: StatsQuery,
    refresh_stats: bool,
}
impl LadderQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_selection(mut self, value: SecuritySelection) -> Result<Self> {
        value.validate()?;
        self.selection = value;
        Ok(self)
    }
    pub fn with_include_touched(mut self, value: bool) -> Result<Self> {
        self.include_touched = value;
        Ok(self)
    }
    pub fn with_count(mut self, value: Option<usize>) -> Result<Self> {
        if value == Some(0) {
            return Err(invalid("ladder count must be positive or None"));
        }
        self.count = value;
        Ok(self)
    }
    pub fn with_stats(mut self, value: StatsQuery) -> Result<Self> {
        value.validate()?;
        self.stats = value;
        Ok(self)
    }
    pub fn with_refresh_stats(mut self, value: bool) -> Result<Self> {
        self.refresh_stats = value;
        Ok(self)
    }
    pub fn selection(&self) -> &SecuritySelection {
        &self.selection
    }
    pub const fn include_touched(&self) -> bool {
        self.include_touched
    }
    pub const fn count(&self) -> Option<usize> {
        self.count
    }
    pub fn stats(&self) -> &StatsQuery {
        &self.stats
    }
    pub const fn refresh_stats(&self) -> bool {
        self.refresh_stats
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.selection.validate()?;
        self.stats.validate()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShortlineTable {
    pub(crate) ids: Vec<SecurityId>,
    pub(crate) target: TradingDate,
    pub(crate) previous: TradingDate,
    pub(crate) stats_date: TradingDate,
    pub(crate) stats_path: String,
    pub(crate) stats_refreshed: bool,
    pub(crate) rows: Vec<ShortlineIndicator>,
    pub(crate) meta: ResultMeta,
}
impl ShortlineTable {
    pub fn ids(&self) -> &[SecurityId] {
        &self.ids
    }
    pub const fn target(&self) -> TradingDate {
        self.target
    }
    pub const fn previous(&self) -> TradingDate {
        self.previous
    }
    pub const fn stats_date(&self) -> TradingDate {
        self.stats_date
    }
    pub fn stats_path(&self) -> &str {
        &self.stats_path
    }
    pub const fn stats_refreshed(&self) -> bool {
        self.stats_refreshed
    }
    pub fn rows(&self) -> &[ShortlineIndicator] {
        &self.rows
    }
    pub fn into_rows(self) -> Vec<ShortlineIndicator> {
        self.rows
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShortlineIndicator {
    pub(crate) id: SecurityId,
    pub(crate) target: TradingDate,
    pub(crate) previous: TradingDate,
    pub(crate) stats_date: Option<TradingDate>,
    pub(crate) alignment: StatsAlignment,
    pub(crate) limit_status: LimitStatus,
    pub(crate) beta_60d: Option<f64>,
    pub(crate) pe_ttm: Option<f64>,
    pub(crate) free_float_shares: Option<Quantity>,
    pub(crate) float_shares: Option<Quantity>,
    pub(crate) previous_amount: Option<Amount>,
    pub(crate) previous_seal: Option<Amount>,
    pub(crate) previous2_seal: Option<Amount>,
    pub(crate) previous_open_amount: Option<Amount>,
    pub(crate) previous_open_volume: Option<Quantity>,
    pub(crate) limit_stat_days: Option<u32>,
    pub(crate) limit_up_count: Option<u32>,
    pub(crate) limit_streak: Option<u32>,
    pub(crate) year_limit_up_days: Option<u32>,
    pub(crate) free_float_market_value: Option<Amount>,
    pub(crate) float_market_value: Option<Amount>,
    pub(crate) open_turnover: Option<Ratio>,
    pub(crate) open_previous_amount: Option<Ratio>,
    pub(crate) open_previous_seal: Option<Ratio>,
    pub(crate) seal_to_float: Option<Ratio>,
    pub(crate) open_change: Option<Ratio>,
    pub(crate) auction_previous_volume: Option<Ratio>,
    pub(crate) seal_previous: Option<Ratio>,
    pub(crate) open_volume_ratio: Option<Ratio>,
    pub(crate) seal_to_amount: Option<Ratio>,
    pub(crate) board_text: Option<String>,
    pub(crate) ladder_level: Option<u32>,
    pub(crate) open: Option<Price>,
    pub(crate) pre_close: Option<Price>,
    pub(crate) open_amount: Option<Amount>,
    pub(crate) seal_amount: Option<Amount>,
    pub(crate) open_volume: Option<Quantity>,
    pub(crate) opening_rush: Option<Ratio>,
    pub(crate) meta: ResultMeta,
}
impl ShortlineIndicator {
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub const fn target(&self) -> TradingDate {
        self.target
    }
    pub const fn previous(&self) -> TradingDate {
        self.previous
    }
    pub const fn stats_date(&self) -> Option<TradingDate> {
        self.stats_date
    }
    pub const fn alignment(&self) -> StatsAlignment {
        self.alignment
    }
    pub const fn limit_status(&self) -> LimitStatus {
        self.limit_status
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
    pub const fn float_shares(&self) -> Option<Quantity> {
        self.float_shares
    }
    pub const fn previous_amount(&self) -> Option<Amount> {
        self.previous_amount
    }
    pub const fn previous_seal(&self) -> Option<Amount> {
        self.previous_seal
    }
    pub const fn previous2_seal(&self) -> Option<Amount> {
        self.previous2_seal
    }
    pub const fn previous_open_amount(&self) -> Option<Amount> {
        self.previous_open_amount
    }
    pub const fn previous_open_volume(&self) -> Option<Quantity> {
        self.previous_open_volume
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
    pub const fn year_limit_up_days(&self) -> Option<u32> {
        self.year_limit_up_days
    }
    pub const fn free_float_market_value(&self) -> Option<Amount> {
        self.free_float_market_value
    }
    pub const fn float_market_value(&self) -> Option<Amount> {
        self.float_market_value
    }
    /// Percentage points: 10 means ten percent.
    pub const fn open_turnover(&self) -> Option<Ratio> {
        self.open_turnover
    }
    pub const fn open_previous_amount(&self) -> Option<Ratio> {
        self.open_previous_amount
    }
    pub const fn open_previous_seal(&self) -> Option<Ratio> {
        self.open_previous_seal
    }
    pub const fn seal_to_float(&self) -> Option<Ratio> {
        self.seal_to_float
    }
    pub const fn open_change(&self) -> Option<Ratio> {
        self.open_change
    }
    /// Ordinary fractions: 1 means equal amounts or volumes.
    pub const fn auction_previous_volume(&self) -> Option<Ratio> {
        self.auction_previous_volume
    }
    pub const fn seal_previous(&self) -> Option<Ratio> {
        self.seal_previous
    }
    pub const fn open_volume_ratio(&self) -> Option<Ratio> {
        self.open_volume_ratio
    }
    pub const fn seal_to_amount(&self) -> Option<Ratio> {
        self.seal_to_amount
    }
    pub fn board_text(&self) -> Option<&str> {
        self.board_text.as_deref()
    }
    pub const fn ladder_level(&self) -> Option<u32> {
        self.ladder_level
    }
    pub const fn open(&self) -> Option<Price> {
        self.open
    }
    pub const fn pre_close(&self) -> Option<Price> {
        self.pre_close
    }
    pub const fn open_amount(&self) -> Option<Amount> {
        self.open_amount
    }
    pub const fn seal_amount(&self) -> Option<Amount> {
        self.seal_amount
    }
    pub const fn open_volume(&self) -> Option<Quantity> {
        self.open_volume
    }
    pub const fn opening_rush(&self) -> Option<Ratio> {
        self.opening_rush
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message)
}
