use super::ResultMeta;
use crate::{
    Error, ErrorKind, PageCursor, PageOptions, Price, Ratio, Result, SecurityId, TradingDate,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialLimitQuery {
    start_index: u16,
    pages: PageOptions,
    cursor: Option<PageCursor>,
}
impl Default for SpecialLimitQuery {
    fn default() -> Self {
        Self {
            start_index: 0,
            pages: PageOptions::new(800, 200, 10_000).unwrap_or_else(|_| unreachable!()),
            cursor: None,
        }
    }
}
impl SpecialLimitQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_start_index(mut self, value: u16) -> Result<Self> {
        self.start_index = value;
        self.cursor = None;
        Ok(self)
    }
    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        self.pages = value;
        self.validate()?;
        Ok(self)
    }
    pub fn with_cursor(mut self, value: &PageCursor) -> Result<Self> {
        self.start_index = u16::try_from(value.next_offset())
            .map_err(|_| invalid("cursor exceeds u16 wire range"))?;
        self.cursor = Some(value.clone());
        self.validate()?;
        Ok(self)
    }
    pub const fn start_index(&self) -> u16 {
        self.start_index
    }
    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.pages.max_pages() > 200 || self.pages.max_records() > 10_000 {
            return Err(invalid(
                "special limits support at most 200 pages and 10000 rows",
            ));
        }
        Ok(()) // page_size is only a workload hint, absent from wire0452.
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecialLimit {
    id: Option<SecurityId>,
    market_raw: u8,
    code_raw: u32,
    upper: Price,
    lower: Price,
    meta: ResultMeta,
}
impl SpecialLimit {
    pub(crate) fn new(
        id: Option<SecurityId>,
        market_raw: u8,
        code_raw: u32,
        upper: Price,
        lower: Price,
        meta: ResultMeta,
    ) -> Self {
        Self {
            id,
            market_raw,
            code_raw,
            upper,
            lower,
            meta,
        }
    }
    pub fn id(&self) -> Option<&SecurityId> {
        self.id.as_ref()
    }
    pub const fn market_raw(&self) -> u8 {
        self.market_raw
    }
    pub const fn code_raw(&self) -> u32 {
        self.code_raw
    }
    pub const fn upper(&self) -> Price {
        self.upper
    }
    pub const fn lower(&self) -> Price {
        self.lower
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LimitStatus {
    Normal,
    NoPriceLimit,
    MissingReference,
    UnknownRule,
    Sealed,
    Touched,
    NotAtLimit,
}
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum LimitRule {
    Approved {
        id: String,
        effective_from: TradingDate,
        effective_to: Option<TradingDate>,
        ratio: Option<Ratio>,
    },
    Unknown {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct LimitDecision {
    rule: LimitRule,
    status: LimitStatus,
    upper: Option<Price>,
    lower: Option<Price>,
    evidence: Vec<String>,
}
impl LimitDecision {
    pub(crate) fn new(
        rule: LimitRule,
        status: LimitStatus,
        upper: Option<Price>,
        lower: Option<Price>,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            rule,
            status,
            upper,
            lower,
            evidence,
        }
    }
    pub fn rule(&self) -> &LimitRule {
        &self.rule
    }
    pub const fn status(&self) -> LimitStatus {
        self.status
    }
    pub const fn upper(&self) -> Option<Price> {
        self.upper
    }
    pub const fn lower(&self) -> Option<Price> {
        self.lower
    }
    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("domain", "special limits")
}
