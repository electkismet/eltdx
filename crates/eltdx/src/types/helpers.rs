use crate::{Error, ErrorKind, Result};
use super::{Amount, CategoryQuote, Diagnostic, EnrichmentStatus, PageOptions, Price,
    Quantity, QuoteCategory, QuoteSort, Ratio, ResultMeta, SecurityId, Snapshot, SourceStamp};

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
