use crate::{Error, ErrorKind, Result};
use super::{Amount, CategoryQuote, Diagnostic, EnrichmentStatus, PageOptions, Price,
    Quantity, QuoteCategory, QuoteSort, Ratio, ResultMeta, SecurityId, Snapshot, SourceStamp,
    AuctionQuery, AuctionSeries, FinanceRecord, MarketDateChoice, Security, SecurityBoard,
    SecurityCategory, TradeTick, TradingDate, LimitRule, LimitStatus, StatsQuery};

/// AllAShares scans the current directory. Explicit preserves occurrences,
/// including an empty list; it never silently becomes a whole-market request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum SecuritySelection { #[default] AllAShares, Explicit(Vec<SecurityId>) }
impl SecuritySelection {
    pub(crate) fn validate(&self) -> Result<()> {
        if let Self::Explicit(ids) = self { crate::services::checked_ids(ids)?; }
        Ok(())
    }
    pub(crate) fn is_empty(&self) -> bool { matches!(self, Self::Explicit(ids) if ids.is_empty()) }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DailySharesQuery { selection: SecuritySelection, stats: StatsQuery, refresh_stats: bool }
impl DailySharesQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_selection(mut self, value: SecuritySelection) -> Result<Self> { value.validate()?; self.selection = value; Ok(self) }
    pub fn with_stats(mut self, value: StatsQuery) -> Result<Self> { value.validate()?; self.stats = value; Ok(self) }
    pub fn with_refresh_stats(mut self, value: bool) -> Result<Self> { self.refresh_stats = value; Ok(self) }
    pub fn selection(&self) -> &SecuritySelection { &self.selection }
    pub fn stats(&self) -> &StatsQuery { &self.stats }
    pub const fn refresh_stats(&self) -> bool { self.refresh_stats }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyLimitsQuery { trade_date: TradingDate, selection: SecuritySelection }
impl DailyLimitsQuery {
    pub fn new(trade_date: TradingDate) -> Self { Self { trade_date, selection: SecuritySelection::AllAShares } }
    pub fn with_selection(mut self, value: SecuritySelection) -> Result<Self> { value.validate()?; self.selection = value; Ok(self) }
    pub const fn trade_date(&self) -> TradingDate { self.trade_date }
    pub fn selection(&self) -> &SecuritySelection { &self.selection }
}

/// The target day is supplied by the current source. Each contributing row's
/// actual date and units remain in meta; free float never substitutes circulation.
#[derive(Clone, Debug, PartialEq)]
pub struct DailyShareCapital {
    pub(crate) id: SecurityId, pub(crate) trade_date: TradingDate,
    pub(crate) total: Option<Quantity>, pub(crate) circulating: Option<Quantity>,
    pub(crate) free_float: Option<Quantity>, pub(crate) finance_updated: Option<TradingDate>,
    pub(crate) sources: Vec<String>, pub(crate) meta: ResultMeta,
}
impl DailyShareCapital {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn trade_date(&self) -> TradingDate { self.trade_date }
    pub const fn total(&self) -> Option<Quantity> { self.total }
    pub const fn circulating(&self) -> Option<Quantity> { self.circulating }
    pub const fn free_float(&self) -> Option<Quantity> { self.free_float }
    pub const fn finance_updated(&self) -> Option<TradingDate> { self.finance_updated }
    pub fn sources(&self) -> &[String] { &self.sources }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DailyPriceLimit {
    pub(crate) id: SecurityId, pub(crate) trade_date: TradingDate, pub(crate) name: Option<String>,
    pub(crate) reference_date: Option<TradingDate>, pub(crate) reference: Option<Price>,
    pub(crate) upper: Option<Price>, pub(crate) lower: Option<Price>,
    pub(crate) rule: LimitRule, pub(crate) status: LimitStatus,
    pub(crate) reference_source: Option<String>, pub(crate) meta: ResultMeta,
}
impl DailyPriceLimit {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn trade_date(&self) -> TradingDate { self.trade_date }
    pub fn name(&self) -> Option<&str> { self.name.as_deref() }
    pub const fn reference_date(&self) -> Option<TradingDate> { self.reference_date }
    pub const fn reference(&self) -> Option<Price> { self.reference }
    pub const fn upper(&self) -> Option<Price> { self.upper }
    pub const fn lower(&self) -> Option<Price> { self.lower }
    pub fn rule(&self) -> &LimitRule { &self.rule }
    pub const fn status(&self) -> LimitStatus { self.status }
    pub fn reference_source(&self) -> Option<&str> { self.reference_source.as_deref() }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuctionDataQuery {
    date: MarketDateChoice, include_series: bool, include_snapshot: bool,
    include_quote: bool, pre_close: Option<Price>,
}
impl Default for AuctionDataQuery {
    fn default() -> Self { Self { date: MarketDateChoice::Current, include_series: true,
        include_snapshot: true, include_quote: true, pre_close: None } }
}
impl AuctionDataQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_date(mut self, date: MarketDateChoice) -> Result<Self> {
        AuctionQuery::new().with_date(date)?; self.date = date; Ok(self)
    }
    pub fn with_series(mut self, value: bool) -> Result<Self> { self.include_series = value; Ok(self) }
    pub fn with_snapshot(mut self, value: bool) -> Result<Self> { self.include_snapshot = value; Ok(self) }
    pub fn with_quote(mut self, value: bool) -> Result<Self> { self.include_quote = value; Ok(self) }
    pub fn with_pre_close(mut self, value: Option<Price>) -> Result<Self> {
        if value.is_some_and(|value| !value.as_f64().is_finite() || value.as_f64() < 0.0) {
            return Err(Error::new(ErrorKind::InvalidArgument, "auction reference must be finite and nonnegative"));
        }
        self.pre_close = value; Ok(self)
    }
    pub const fn date(&self) -> MarketDateChoice { self.date }
    pub const fn include_series(&self) -> bool { self.include_series }
    pub const fn include_snapshot(&self) -> bool { self.include_snapshot }
    pub const fn include_quote(&self) -> bool { self.include_quote }
    pub const fn pre_close(&self) -> Option<Price> { self.pre_close }
}

/// Opening amounts come only from formal executed OpeningMatch ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct AuctionData {
    pub(crate) id: SecurityId, pub(crate) date: TradingDate,
    pub(crate) series: Option<AuctionSeries>, pub(crate) opening_match: Option<TradeTick>,
    pub(crate) pre_close: Option<Price>, pub(crate) open: Option<Price>,
    pub(crate) open_volume: Option<Quantity>, pub(crate) open_amount: Option<Amount>,
    pub(crate) open_change: Option<Ratio>, pub(crate) meta: ResultMeta,
}
impl AuctionData {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub const fn date(&self) -> TradingDate { self.date }
    pub fn series(&self) -> Option<&AuctionSeries> { self.series.as_ref() }
    pub fn opening_match(&self) -> Option<&TradeTick> { self.opening_match.as_ref() }
    pub const fn pre_close(&self) -> Option<Price> { self.pre_close }
    pub const fn open(&self) -> Option<Price> { self.open }
    pub const fn open_volume(&self) -> Option<Quantity> { self.open_volume }
    pub const fn open_amount(&self) -> Option<Amount> { self.open_amount }
    /// Percentage points, e.g. 10 for a ten-percent opening change.
    pub const fn open_change(&self) -> Option<Ratio> { self.open_change }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

/// Breadth pseudo-indices. The residual count is not confirmed suspensions.
#[derive(Clone, Debug, PartialEq)]
pub struct MarketStat {
    pub(crate) up: u64, pub(crate) down: u64, pub(crate) neutral: u64,
    pub(crate) suspended_or_unclassified: u64, pub(crate) total: u64,
    pub(crate) limit_up: u64, pub(crate) limit_down: u64,
    pub(crate) total_amount: Amount, pub(crate) total_volume: Quantity,
    pub(crate) market_cap: Amount, pub(crate) meta: ResultMeta,
}
impl MarketStat {
    pub const fn up(&self) -> u64 { self.up }
    pub const fn down(&self) -> u64 { self.down }
    pub const fn neutral(&self) -> u64 { self.neutral }
    pub const fn suspended_or_unclassified(&self) -> u64 { self.suspended_or_unclassified }
    pub const fn total(&self) -> u64 { self.total }
    pub const fn limit_up(&self) -> u64 { self.limit_up }
    pub const fn limit_down(&self) -> u64 { self.limit_down }
    pub const fn total_amount(&self) -> Amount { self.total_amount }
    pub const fn total_volume(&self) -> Quantity { self.total_volume }
    pub const fn market_cap(&self) -> Amount { self.market_cap }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileQuery { include_security: bool, include_finance: bool }
impl Default for ProfileQuery {
    fn default() -> Self { Self { include_security: true, include_finance: true } }
}
impl ProfileQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_security(mut self, value: bool) -> Result<Self> { self.include_security = value; Ok(self) }
    pub fn with_finance(mut self, value: bool) -> Result<Self> { self.include_finance = value; Ok(self) }
    pub const fn include_security(&self) -> bool { self.include_security }
    pub const fn include_finance(&self) -> bool { self.include_finance }
}

/// Quote is required; directory and finance are independent optional sources.
/// Original DTOs and source units remain accessible alongside derived fields.
#[derive(Clone, Debug, PartialEq)]
pub struct StockProfile {
    pub(crate) quote: FullQuote, pub(crate) security: Option<Security>,
    pub(crate) finance: Option<FinanceRecord>, pub(crate) circulating_shares: Option<Quantity>,
    pub(crate) total_shares: Option<Quantity>, pub(crate) turnover: Option<Ratio>,
    pub(crate) circulating_market_value: Option<Amount>, pub(crate) total_market_value: Option<Amount>,
    pub(crate) eps: Option<Price>, pub(crate) security_status: EnrichmentStatus,
    pub(crate) finance_status: EnrichmentStatus, pub(crate) meta: ResultMeta,
}
impl StockProfile {
    pub fn id(&self) -> &SecurityId { self.quote.snapshot().id() }
    pub fn name(&self) -> Option<&str> { self.security.as_ref().map(Security::name) }
    pub fn category(&self) -> Option<&SecurityCategory> { self.security.as_ref().map(Security::category) }
    pub fn board(&self) -> Option<&SecurityBoard> { self.security.as_ref().map(Security::board) }
    pub fn last(&self) -> Option<Price> { Some(self.quote.snapshot().last()) }
    pub fn pre_close(&self) -> Option<Price> { Some(self.quote.snapshot().pre_close()) }
    pub fn open(&self) -> Option<Price> { Some(self.quote.snapshot().open()) }
    pub fn high(&self) -> Option<Price> { Some(self.quote.snapshot().high()) }
    pub fn low(&self) -> Option<Price> { Some(self.quote.snapshot().low()) }
    pub fn change(&self) -> Option<Price> { Some(self.quote.snapshot().change()) }
    pub fn change_pct(&self) -> Option<Ratio> { self.quote.snapshot().change_pct() }
    pub fn volume(&self) -> Option<Quantity> { Some(self.quote.snapshot().total()) }
    pub fn amount(&self) -> Option<Amount> { Some(self.quote.snapshot().amount()) }
    pub fn open_amount(&self) -> Option<Amount> { Some(self.quote.snapshot().open_amount()) }
    pub const fn circulating_shares(&self) -> Option<Quantity> { self.circulating_shares }
    pub const fn total_shares(&self) -> Option<Quantity> { self.total_shares }
    /// Turnover is expressed in percentage points, not a fraction.
    pub const fn turnover(&self) -> Option<Ratio> { self.turnover }
    pub const fn circulating_market_value(&self) -> Option<Amount> { self.circulating_market_value }
    pub const fn total_market_value(&self) -> Option<Amount> { self.total_market_value }
    pub const fn eps(&self) -> Option<Price> { self.eps }
    pub fn ipo(&self) -> Option<TradingDate> { self.finance.as_ref().and_then(FinanceRecord::ipo) }
    pub fn updated(&self) -> Option<TradingDate> { self.finance.as_ref().and_then(FinanceRecord::updated) }
    pub fn security(&self) -> Option<&Security> { self.security.as_ref() }
    pub fn quote(&self) -> &FullQuote { &self.quote }
    pub fn finance(&self) -> Option<&FinanceRecord> { self.finance.as_ref() }
    pub const fn security_status(&self) -> EnrichmentStatus { self.security_status }
    pub const fn finance_status(&self) -> EnrichmentStatus { self.finance_status }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

/// A primary snapshot with an explicitly reported optional depth supplement.
#[derive(Clone, Debug, PartialEq)]
pub struct FullQuote {
    pub(crate) snapshot: Snapshot,
    pub(crate) depth_status: EnrichmentStatus,
    pub(crate) depth_source: Option<SourceStamp>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}
impl FullQuote {
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub const fn depth_status(&self) -> EnrichmentStatus { self.depth_status }
    pub fn depth_source(&self) -> Option<&SourceStamp> { self.depth_source.as_ref() }
    pub fn diagnostics(&self) -> &[Diagnostic] { &self.diagnostics }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RankQuery {
    category: QuoteCategory, sort: QuoteSort, ascending: bool,
    count: Option<usize>, pages: PageOptions,
}
impl Default for RankQuery {
    fn default() -> Self {
        Self { category: QuoteCategory::HqAShares, sort: QuoteSort::ChangePct, ascending: false,
            count: Some(80), pages: PageOptions::new(80, 200, 16_000).unwrap_or_else(|_| unreachable!()) }
    }
}
impl RankQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_category(mut self, value: QuoteCategory) -> Result<Self> { self.category = value; Ok(self) }
    pub fn with_sort(mut self, value: QuoteSort) -> Result<Self> { self.sort = value; Ok(self) }
    pub fn with_ascending(mut self, value: bool) -> Result<Self> { self.ascending = value; Ok(self) }
    pub fn with_count(mut self, value: Option<usize>) -> Result<Self> {
        if value == Some(0) { return Err(rank_argument("count must be positive or None")); }
        self.count = value; Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        if value.page_size() > 80 { return Err(rank_argument("rank page size must be between 1 and 80")); }
        self.pages = value; Ok(self)
    }
    pub const fn category(&self) -> QuoteCategory { self.category }
    pub const fn sort(&self) -> QuoteSort { self.sort }
    pub const fn ascending(&self) -> bool { self.ascending }
    pub const fn count(&self) -> Option<usize> { self.count }
    pub fn pages(&self) -> &PageOptions { &self.pages }
}
fn rank_argument(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message) }

/// Server order is preserved, including repeated identities across pages.
#[derive(Clone, Debug, PartialEq)]
pub struct RealtimeRankRow {
    pub(crate) rank: usize, pub(crate) name: Option<String>, pub(crate) quote: CategoryQuote,
    pub(crate) seal_amount: Amount,
}
impl RealtimeRankRow {
    pub const fn rank(&self) -> usize { self.rank }
    pub fn id(&self) -> &SecurityId { self.quote.snapshot().id() }
    pub fn name(&self) -> Option<&str> { self.name.as_deref() }
    pub fn last(&self) -> Price { self.quote.snapshot().last() }
    pub fn pre_close(&self) -> Price { self.quote.snapshot().pre_close() }
    pub fn change_pct(&self) -> Option<Ratio> { self.quote.snapshot().change_pct() }
    pub fn amount(&self) -> Amount { self.quote.snapshot().amount() }
    pub fn volume(&self) -> Quantity { self.quote.snapshot().total() }
    pub fn opening_rush(&self) -> Ratio { self.quote.opening_rush() }
    pub const fn seal_amount(&self) -> Amount { self.seal_amount }
    pub fn quote(&self) -> &CategoryQuote { &self.quote }
}

/// L1 trade-size classification in CNY. The buckets describe trade amounts,
/// and do not identify actual institutional orders or investment intent.
#[derive(Clone, Debug, PartialEq)]
pub struct FundFlow {
    pub(crate) buckets: [Amount; 8], pub(crate) main_net: Amount, pub(crate) total_net: Amount,
    pub(crate) classified_count: usize, pub(crate) skipped_count: usize, pub(crate) meta: ResultMeta,
}
impl FundFlow {
    pub const fn super_in(&self) -> Amount { self.buckets[0] }
    pub const fn large_in(&self) -> Amount { self.buckets[1] }
    pub const fn medium_in(&self) -> Amount { self.buckets[2] }
    pub const fn small_in(&self) -> Amount { self.buckets[3] }
    pub const fn super_out(&self) -> Amount { self.buckets[4] }
    pub const fn large_out(&self) -> Amount { self.buckets[5] }
    pub const fn medium_out(&self) -> Amount { self.buckets[6] }
    pub const fn small_out(&self) -> Amount { self.buckets[7] }
    pub const fn main_net(&self) -> Amount { self.main_net }
    pub const fn total_net(&self) -> Amount { self.total_net }
    pub const fn classified_count(&self) -> usize { self.classified_count }
    pub const fn skipped_count(&self) -> usize { self.skipped_count }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
