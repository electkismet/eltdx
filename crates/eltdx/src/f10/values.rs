//! Checked identities and selectors shared by concrete F10 queries.
use std::num::NonZeroU32;

use crate::types::{HqMarket, SecurityId, TradingDate};
use crate::{Error, ErrorKind, Result};

use super::JsonValue;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct F10MarketCode(u16);
impl F10MarketCode {
    pub const fn new(value: u16) -> Self { Self(value) }
    pub const fn get(self) -> u16 { self.0 }
}

/// The caller's purpose is descriptive; a six-digit identity alone cannot
/// establish whether an instrument is a stock, fund, index or board.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum CodeUse { #[default] Unspecified, Stock, Fund, Index, Board }

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AshareCode { number: String, market: F10MarketCode, usage: CodeUse }
impl AshareCode {
    pub fn new(market: F10MarketCode, number: impl Into<String>) -> Result<Self> {
        let number = number.into();
        digits(&number, 6)?;
        Ok(Self { number, market, usage: CodeUse::Unspecified })
    }
    pub fn as_str(&self) -> &str { &self.number }
    pub const fn market(&self) -> F10MarketCode { self.market }
    pub const fn usage(&self) -> CodeUse { self.usage }
    pub fn with_usage(mut self, usage: CodeUse) -> Self { self.usage = usage; self }
}
impl From<SecurityId> for AshareCode {
    fn from(value: SecurityId) -> Self {
        let market = match value.market() { HqMarket::Shenzhen => 0, HqMarket::Shanghai => 1, HqMarket::Beijing => 2 };
        Self { number: value.number().to_owned(), market: F10MarketCode(market), usage: CodeUse::Unspecified }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UsTicker(String);
impl UsTicker {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        // Only recognized market prefixes are stripped. In particular BRK.B
        // is a complete ticker, not a prefix followed by B.
        let bare = if value.get(..3).is_some_and(|prefix| prefix.eq_ignore_ascii_case("us."))
            || value.starts_with("74.") { &value[3..] } else { value };
        symbol(bare)?;
        Ok(Self(bare.to_ascii_uppercase()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HongKongCode { number: String, wire_six_digits: bool }
impl HongKongCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        let bare = if value.get(..3).is_some_and(|prefix| prefix.eq_ignore_ascii_case("hk."))
            || value.starts_with("31.") { &value[3..] } else { value };
        if bare.is_empty() || bare.len() > 5 || !bare.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid("Hong Kong code must contain one to five ASCII digits"));
        }
        Ok(Self { number: format!("{bare:0>5}"), wire_six_digits: false })
    }
    /// Explicit six-digit legacy wire identity, retained without truncation.
    pub fn from_wire(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.len() == 6 { digits(&value, 6)?; Ok(Self { number: value, wire_six_digits: true }) }
        else { Self::new(value) }
    }
    pub fn as_str(&self) -> &str { &self.number }
    pub const fn is_six_digit_wire(&self) -> bool { self.wire_six_digits }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ThirdBoardCode { number: String, market: F10MarketCode }
impl ThirdBoardCode {
    pub fn new(number: impl Into<String>) -> Result<Self> {
        let number = number.into(); digits(&number, 6)?;
        Ok(Self { number, market: F10MarketCode(2) })
    }
    pub fn with_market(mut self, market: F10MarketCode) -> Self { self.market = market; self }
    pub fn as_str(&self) -> &str { &self.number }
    pub const fn market(&self) -> F10MarketCode { self.market }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct BondCode { number: String, market: F10MarketCode, inferred_market: bool }
impl BondCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        let (number, market, inferred_market) = if value.len() == 8 && value.is_ascii() {
            match value[..2].to_ascii_lowercase().as_str() {
                "sh" => (&value[2..], 1, false), "sz" => (&value[2..], 0, false),
                _ => return Err(invalid("bond prefix must be sh or sz")),
            }
        } else { (value, 1, true) };
        digits(number, 6)?;
        Ok(Self { number: number.to_owned(), market: F10MarketCode(market), inferred_market })
    }
    pub fn with_market(mut self, market: F10MarketCode) -> Result<Self> {
        if market.get() > 1 { return Err(invalid("bond market must be 0 (SZ) or 1 (SH)")); }
        self.market = market; self.inferred_market = false; Ok(self)
    }
    pub fn as_str(&self) -> &str { &self.number }
    pub const fn market(&self) -> F10MarketCode { self.market }
    pub const fn market_is_inferred(&self) -> bool { self.inferred_market }
}

macro_rules! symbol_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into(); symbol(&value)?; Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
    };
}
symbol_type!(IndustryCode);
symbol_type!(FuturesCode);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RequestId(String);
impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) { return Err(invalid("F10 ReqId must be nonempty decimal text")); }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

/// A selector is scoped to the method which validated it. Passing a selector
/// made for another endpoint is rejected even if the text happens to match.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SectionCode { method: &'static str, value: String }
impl SectionCode {
    pub fn new(method: &'static str, value: impl Into<String>) -> Result<Self> {
        let value = value.into(); text(&value, false)?;
        if let Some(choices) = closed_sections(method) {
            if !choices.contains(&value.as_str()) { return Err(invalid("unknown F10 section").with_context("method", method).with_context("section", value)); }
        }
        Ok(Self { method, value })
    }
    pub fn as_str(&self) -> &str { &self.value }
    pub const fn method(&self) -> &'static str { self.method }
    pub(crate) fn checked_for(&self, method: &'static str) -> Result<()> {
        if self.method != method { return Err(invalid("F10 section belongs to another method")); }
        Ok(())
    }
    pub(crate) fn default_for(method: &'static str, value: &'static str) -> Self { Self { method, value: value.to_owned() } }
}

pub(crate) fn closed_sections(method: &str) -> Option<&'static [&'static str]> {
    Some(match method {
        "shareholders" => &["count", "ranking", "controller", "top_circulating", "top", "bonds", "changes", "institutions", "institution_types", "institution_holdings", "plan"],
        "capital_structure" => &["structure", "changes", "buyback", "restricted", "restricted_detail"],
        "business_report" => &["business", "composition", "customers", "suppliers", "price", "changes"],
        "industry_analysis" => &["peers", "dividend_rank", "scale", "valuation", "news", "reports", "market_share", "summary", "comparable", "comparable_profile"],
        "trading_report" => &["margin", "margin_history", "block_trades", "dragon_board", "northbound", "limit_down", "limit_up", "bonds", "main_force", "reports", "kcb"],
        "fund_report" => &["name", "managers", "company", "basic", "seats", "scale", "scale_history", "nav", "nav_period", "nav_rank", "dividends", "asset_allocation", "industry_allocation", "holdings_compare", "holdings_rank", "holding_summary", "bond_allocation", "futures_allocation", "holdings", "holdings_detail", "holdings_changes", "fund_of_funds", "finance", "sales_info", "sales_announcements", "sales_news", "etf", "margin", "ratings", "categories"],
        "mg_tips" => &["zxdt", "sypl", "gsgy", "zysj"],
        "mg_finance" => &["zcfzb", "lyb", "xjllb"],
        "mg_finance_indicator" | "hk_finance_indicator" => &["jbzb", "ylnl", "cznl", "yynl", "ccznl"],
        "mg_dividend" => &["fh", "cf"], "mg_capital" => &["001", "002"],
        "mg_directors" => &["ds", "gg", "lrdjg"], "mg_market" => &["001", "003"],
        "hk_finance" => &["zhsyb", "zcfzb", "xjllb"],
        "hy_summary" => &["hqtj", "hyzq", "hygy", "zszfb"],
        "hy_members_rank" => &["cwzkpm", "gsgmpm", "gzsppm", "scbxpm", "zysrpm"],
        "qh_position_report" => &["cjcc", "cjjg", "jcgc", "ccjj", "ykfx"],
        "qh_variety_info" => &["pzjj", "hyjj", "xgzq"],
        "qh_index_info" => &["zsgk", "lsjg"],
        "qh_index_valuation" => &["dtpe", "lspe", "xjfh"],
        "qh_weights" => &["zxcfg", "lscfg", "cfgqz", "cfghyfb", "xwpl"],
        "market_report" => &["board_info", "board_basics", "board_description", "board_trading", "board_flow", "board_members", "board_analysis", "board_finance_compare", "board_candidates", "board_trend", "index_info", "index_profile", "index_valuation", "index_tracking", "index_flow", "index_flow_members", "index_notices", "market_basic", "market_margin", "market_margin_summary", "market_margin_investors", "market_new_shares", "market_capital", "index_members", "index_etfs", "ipo_calendar"],
        "profit_forecast" => &["ylyctj", "ylycmx", "yzyq", "tzpjtj", "ycpjyjbg"],
        "digest" => &["tips", "profile", "events", "news", "announcements", "reports", "roadshows", "interaction"],
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSelector { All, Number(u32) }
impl PageSelector {
    pub(crate) fn checked(self, allow_zero: bool, allow_all: bool) -> Result<Self> {
        match self {
            Self::All if allow_all => Ok(self),
            Self::Number(value) if value <= i32::MAX as u32 && (allow_zero || value != 0) => Ok(self),
            _ => Err(invalid("page selector is outside this endpoint's range")),
        }
    }
    pub(crate) fn wire(self) -> String { match self { Self::All => "-1".to_owned(), Self::Number(value) => value.to_string() } }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSizeSelector { All, Count(NonZeroU32) }
impl PageSizeSelector {
    pub(crate) fn checked(self, allow_all: bool) -> Result<Self> {
        match self { Self::All if allow_all => Ok(self), Self::Count(value) => { count(value)?; Ok(self) }, _ => Err(invalid("unlimited page size is not supported by this endpoint")) }
    }
    pub(crate) fn wire(self) -> String { match self { Self::All => "-1".to_owned(), Self::Count(value) => value.get().to_string() } }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundWindow(String);
impl FundWindow {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, false)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn three_months() -> Self { Self("3M".to_owned()) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DateOrWindow { Date(TradingDate), Window(FundWindow) }
impl DateOrWindow { pub(crate) fn wire(&self) -> String { match self { Self::Date(date) => date_text(Some(*date)), Self::Window(window) => window.as_str().to_owned() } } }

pub(crate) fn count(value: NonZeroU32) -> Result<NonZeroU32> {
    if value.get() > i32::MAX as u32 { return Err(invalid("F10 count exceeds signed 32-bit wire range")); } Ok(value)
}
pub(crate) fn page(value: u32) -> Result<u32> { PageSelector::Number(value).checked(false, false)?; Ok(value) }
pub(crate) fn text(value: &str, allow_empty: bool) -> Result<()> {
    if (!allow_empty && value.is_empty()) || value.chars().any(char::is_control) { return Err(invalid("F10 text must have no control characters")); } Ok(())
}
fn digits(value: &str, size: usize) -> Result<()> {
    if value.len() != size || !value.bytes().all(|byte| byte.is_ascii_digit()) { return Err(invalid("F10 code has invalid ASCII digits or length")); } Ok(())
}
fn symbol(value: &str) -> Result<()> {
    let allowed = super::python_legacy::allowed_symbols(value.bytes().map(Ok),
        |byte| Ok::<_, std::convert::Infallible>(byte.is_ascii_alphanumeric()),
        |byte| Ok(b".-_".contains(byte)));
    let allowed = match allowed { Ok(value) => value, Err(impossible) => match impossible {} };
    if !value.bytes().any(|byte| byte.is_ascii_alphanumeric()) || !allowed {
        return Err(invalid("invalid F10 symbol"));
    } Ok(())
}
pub(crate) fn invalid(message: &str) -> Error { Error::new(ErrorKind::InvalidArgument, message) }
pub(crate) fn date_text(date: Option<TradingDate>) -> String { date.map_or_else(String::new, |date| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day())) }
pub(crate) fn date_pair(start: Option<TradingDate>, end: Option<TradingDate>) -> Result<()> {
    if start.zip(end).is_some_and(|(start, end)| start > end) { return Err(invalid("F10 date range is reversed")); } Ok(())
}
pub(crate) fn j(value: impl Into<JsonValue>) -> JsonValue { value.into() }

pub(crate) trait IdentityDiagnostic { fn inferred_bond_market(&self) -> bool { false } }
impl IdentityDiagnostic for AshareCode {}
impl IdentityDiagnostic for UsTicker {}
impl IdentityDiagnostic for HongKongCode {}
impl IdentityDiagnostic for ThirdBoardCode {}
impl IdentityDiagnostic for IndustryCode {}
impl IdentityDiagnostic for FuturesCode {}
impl IdentityDiagnostic for BondCode { fn inferred_bond_market(&self) -> bool { self.market_is_inferred() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_preserve_market_and_punctuation() -> Result<()> {
        assert_eq!(UsTicker::new("us.BRK.B")?.as_str(), "BRK.B");
        assert_eq!(HongKongCode::new("700")?.as_str(), "00700");
        assert!(HongKongCode::new("123456").is_err());
        assert!(HongKongCode::from_wire("123456")?.is_six_digit_wire());
        assert!(BondCode::new("113043")?.market_is_inferred());
        assert_eq!(BondCode::new("sz113043")?.market().get(), 0);
        assert_eq!(AshareCode::new(F10MarketCode::new(1), "000001")?.market().get(), 1);
        assert!(ThirdBoardCode::new("１２３４５６").is_err());
        Ok(())
    }
    #[test]
    fn selectors_cannot_cross_methods_or_exceed_wire_range() -> Result<()> {
        assert!(SectionCode::new("mg_finance", "unknown").is_err());
        assert!(SectionCode::new("mg_tips", "zxdt")?.checked_for("hk_tips").is_err());
        assert!(PageSelector::Number(0).checked(false, false).is_err());
        assert!(PageSelector::All.checked(false, false).is_err());
        assert!(count(NonZeroU32::new(u32::MAX).unwrap()).is_err());
        Ok(())
    }
}
