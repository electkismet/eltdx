use crate::{Error, ErrorKind, PageOptions, Result, SecurityId, TradingDate};
use super::ResultMeta;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarQuery { benchmark: SecurityId, pages: PageOptions, refresh: bool }
impl Default for CalendarQuery {
    fn default() -> Self { Self { benchmark: "sh000001".parse().unwrap_or_else(|_| unreachable!()),
        pages: PageOptions::new(800, 200, 160_000).unwrap_or_else(|_| unreachable!()), refresh: false } }
}
impl CalendarQuery {
    pub fn new() -> Self { Self::default() }
    pub fn with_benchmark(mut self, value: SecurityId) -> Result<Self> { self.benchmark = value; self.validate()?; Ok(self) }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> { self.pages = value; self.validate()?; Ok(self) }
    pub fn with_refresh(mut self, value: bool) -> Result<Self> { self.refresh = value; self.validate()?; Ok(self) }
    pub fn benchmark(&self) -> &SecurityId { &self.benchmark }
    pub fn pages(&self) -> &PageOptions { &self.pages }
    pub const fn refresh(&self) -> bool { self.refresh }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.pages.page_size() > 800 { return Err(Error::new(ErrorKind::InvalidArgument, "calendar K-line page size exceeds 800")); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalendarSnapshot {
    benchmark: SecurityId, days: Vec<TradingDate>, coverage_start: Option<TradingDate>, coverage_end: Option<TradingDate>,
    complete: bool, loaded_on: TradingDate, meta: ResultMeta,
}
impl CalendarSnapshot {
    pub(crate) fn new(benchmark: SecurityId, days: Vec<TradingDate>, loaded_on: TradingDate, meta: ResultMeta) -> Self {
        let days = crate::calendar::index::calendar_days(days);
        Self { benchmark, coverage_start: days.first().copied(), coverage_end: days.last().copied(), days,
            complete: true, loaded_on, meta }
    }
    pub fn benchmark(&self) -> &SecurityId { &self.benchmark }
    pub fn days(&self) -> &[TradingDate] { &self.days }
    pub const fn coverage_start(&self) -> Option<TradingDate> { self.coverage_start }
    pub const fn coverage_end(&self) -> Option<TradingDate> { self.coverage_end }
    pub const fn complete(&self) -> bool { self.complete }
    pub const fn loaded_on(&self) -> TradingDate { self.loaded_on }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub(crate) fn require_coverage(&self, date: TradingDate) -> Result<()> {
        if self.complete && self.coverage_start.is_some_and(|start| date >= start) && self.coverage_end.is_some_and(|end| date <= end) { return Ok(()); }
        Err(Error::new(ErrorKind::MetadataUnavailable, "date is outside the observed trading-calendar coverage")
            .with_context("benchmark", format!("{}", self.benchmark)).with_context("requested", format!("{date}"))
            .with_context("coverage_start", format!("{:?}", self.coverage_start)).with_context("coverage_end", format!("{:?}", self.coverage_end)))
    }
}
