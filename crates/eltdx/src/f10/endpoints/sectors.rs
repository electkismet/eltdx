//! Industry and futures endpoints retain native symbols and request dates.
use std::time::Instant;
use super::SimpleArgument::{Code, Literal, Section};
use crate::{OperationOptions, Result};
use crate::types::TradingDate;
use crate::f10::{CallQuery, F10Client, FuturesCode, IndustryCode, Response, SectionCode};
use crate::f10::values::{date_pair, date_text, j, text};

section_endpoint!(HySummaryQuery, hy_summary, IndustryCode, "hqtj", code, section, &format!("CWServ.skef10_hy_{section}"), [j(code.as_str())]);
code_endpoint!(HyProfileQuery, hy_profile, IndustryCode, code, "CWServ.skef10_hy_hyzl_hyjs", wire [Code]);
section_endpoint!(HyCommentaryQuery, hy_commentary, IndustryCode, "001", code, section, "CWServ.skef10_hy_hyzl_hyplk", wire [Section, Code]);
section_endpoint!(HyNewsQuery, hy_news, IndustryCode, "hydt", code, section, "CWServ.skef10_hy_zxdt", wire [Section, Code]);
section_endpoint!(HyCapitalStatsQuery, hy_capital_stats, IndustryCode, "001", code, section, "CWServ.skef10_hy_hysj_bxzjtj", [j(section), j(code.as_str()), j(if section == "001" { "n" } else { "" })]);
section_endpoint!(HyFundHoldingsQuery, hy_fund_holdings, IndustryCode, "1", code, section, "CWServ.skef10_hy_hysj_jjzb", wire [Section, Code, Literal("")]);
section_endpoint!(HyCapitalFlowQuery, hy_capital_flow, IndustryCode, "001", code, section, "CWServ.skef10_hy_hysj_zjlx", wire [Section, Code]);
section_endpoint!(HyAnalystIndexQuery, hy_analyst_index, IndustryCode, "001", code, section, "CWServ.skef10_hy_hyyj_fxszs", wire [Section, Code, Literal(""), Literal("")]);
code_endpoint!(HyMajorHoldersQuery, hy_major_holders, IndustryCode, code, "CWServ.skef10_hy_hyyj_hymzgd", wire [Code]);
section_endpoint!(HyRatingSummaryQuery, hy_rating_summary, IndustryCode, "001", code, section, "CWServ.skef10_hy_hyyj_hypjhz", wire [Section, Code, Literal("")]);
code_endpoint!(HyIndexQuery, hy_index, IndustryCode, code, "CWServ.skef10_hy_idsy", wire [Literal("002"), Code]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HyStageRiseQuery { code: IndustryCode, flag: String }
impl HyStageRiseQuery {
    pub fn new(code: IndustryCode) -> Self { Self { code, flag: "e".to_owned() } }
    pub fn code(&self) -> &IndustryCode { &self.code }
    pub fn flag(&self) -> &str { &self.flag }
    pub fn with_flag(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.flag = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.skef10_hy_jdzf", vec![j(self.code.as_str()), j(self.flag.as_str())]) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HyMembersRankQuery { code: IndustryCode, section: SectionCode }
impl HyMembersRankQuery {
    pub fn new(code: IndustryCode) -> Self { Self { code, section: SectionCode::default_for("hy_members_rank", "cwzkpm") } }
    pub fn code(&self) -> &IndustryCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn with_section(mut self, section: SectionCode) -> Result<Self> { section.checked_for("hy_members_rank")?; self.section = section; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let code = self.code.as_str();
        let (suffix, params) = match self.section.as_str() {
            "cwzkpm" => ("cwzkpm", vec![j("002"), j(code), j(""), j("e"), j("")]),
            "gsgmpm" => ("gsgmpm", vec![j(code)]),
            "gzsppm" => ("gzsppm", vec![j("01"), j(code), j("o")]),
            "scbxpm" => ("scbxpm", vec![j("scpm"), j(""), j(code)]),
            "zysrpm" => ("zysrpm", vec![j("001"), j(code), j("")]),
            _ => unreachable!("closed industry rank registry"),
        };
        super::params(&format!("CWServ.skef10_hy_hydw_{suffix}"), params)
    }
}

code_endpoint!(QhInventoryQuery, qh_inventory, FuturesCode, code, "CWServ.qhf9_sp_kcbg_kcbg", wire [Literal("1"), Code]);
section_endpoint!(QhIndexInfoQuery, qh_index_info, FuturesCode, "zsgk", code, section, &format!("CWServ.qhf9_gz_zszl_{section}"), [j(if section == "zsgk" { "1" } else { "2" }), j(code.as_str())]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhPositionReportQuery { code: FuturesCode, section: SectionCode, date: Option<TradingDate>, end_date: Option<TradingDate>, index: bool }
impl QhPositionReportQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, section: SectionCode::default_for("qh_position_report", "cjcc"), date: None, end_date: None, index: false } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn date(&self) -> Option<TradingDate> { self.date }
    pub fn end_date(&self) -> Option<TradingDate> { self.end_date }
    pub fn index(&self) -> bool { self.index }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("qh_position_report")?; self.section = value; Ok(self) }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Result<Self> { date_pair(value, self.end_date)?; self.date = value; Ok(self) }
    pub fn with_end_date(mut self, value: Option<TradingDate>) -> Result<Self> { date_pair(self.date, value)?; self.end_date = value; Ok(self) }
    pub fn with_index(mut self, value: bool) -> Self { self.index = value; self }
    pub fn to_call(&self) -> Result<CallQuery> {
        let section = self.section.as_str();
        let mut params = vec![j("1"), j(self.code.as_str()), j(date_text(self.date))];
        if section == "cjjg" { params.push(j("")); }
        else if section != "cjcc" { params.push(j(date_text(self.end_date))); params.push(j("")); }
        super::params(&format!("CWServ.qhf9_{}_ccbg_{section}", if self.index { "gz" } else { "sp" }), params)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhBasisSpreadQuery { code: FuturesCode, section: SectionCode, period: String }
impl QhBasisSpreadQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, section: SectionCode::default_for("qh_basis_spread", "001"), period: "1Y".to_owned() } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn period(&self) -> &str { &self.period }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("qh_basis_spread")?; self.section = value; Ok(self) }
    pub fn with_period(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.period = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.qhf9_sp_jcjcfx", vec![j(self.section.as_str()), j(self.code.as_str()), j(self.period.as_str()), j(""), j("")]) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhWarehouseReceiptQuery { code: FuturesCode, date: Option<TradingDate> }
impl QhWarehouseReceiptQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, date: None } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn date(&self) -> Option<TradingDate> { self.date }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Self { self.date = value; self }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.qhf9_sp_ckrb_ckrb", vec![j("1"), j(self.code.as_str()), j(date_text(self.date))]) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhVarietyInfoQuery { code: FuturesCode, section: SectionCode }
impl QhVarietyInfoQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, section: SectionCode::default_for("qh_variety_info", "pzjj") } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("qh_variety_info")?; self.section = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let mut params = vec![j(self.code.as_str())];
        if self.section.as_str() == "pzjj" { params.push(j("1")); }
        super::params(&format!("CWServ.qhf9_sp_pzzl_{}", self.section.as_str()), params)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhIndexValuationQuery { code: FuturesCode, section: SectionCode, start: Option<TradingDate>, end: Option<TradingDate> }
impl Default for QhIndexValuationQuery { fn default() -> Self { Self { code: FuturesCode::new("000300").expect("built-in numeric futures symbol"), section: SectionCode::default_for("qh_index_valuation", "dtpe"), start: None, end: None } } }
impl QhIndexValuationQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, ..Self::default() } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn start(&self) -> Option<TradingDate> { self.start }
    pub fn end(&self) -> Option<TradingDate> { self.end }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("qh_index_valuation")?; self.section = value; Ok(self) }
    pub fn with_start(mut self, value: Option<TradingDate>) -> Result<Self> { date_pair(value, self.end)?; self.start = value; Ok(self) }
    pub fn with_end(mut self, value: Option<TradingDate>) -> Result<Self> { date_pair(self.start, value)?; self.end = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params(&format!("CWServ.qhf9_gz_zsgz_{}", self.section.as_str()), vec![j("1"), j(self.code.as_str()), j(date_text(self.start)), j(date_text(self.end))]) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QhWeightsQuery { code: FuturesCode, section: SectionCode, date: Option<TradingDate> }
impl Default for QhWeightsQuery { fn default() -> Self { Self { code: FuturesCode::new("399300").expect("built-in numeric futures symbol"), section: SectionCode::default_for("qh_weights", "zxcfg"), date: None } } }
impl QhWeightsQuery {
    pub fn new(code: FuturesCode) -> Self { Self { code, ..Self::default() } }
    pub fn code(&self) -> &FuturesCode { &self.code }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn date(&self) -> Option<TradingDate> { self.date }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("qh_weights")?; self.section = value; Ok(self) }
    pub fn with_date(mut self, value: Option<TradingDate>) -> Self { self.date = value; self }
    pub fn to_call(&self) -> Result<CallQuery> {
        let mut params = vec![j("1"), j(self.code.as_str())];
        if self.section.as_str() != "cfghyfb" { params.push(j(date_text(self.date))); }
        super::params(&format!("CWServ.qhf9_gz_cfg_{}", self.section.as_str()), params)
    }
}

macro_rules! execute_query {
    ($($method:ident: $query:ident),* $(,)?) => { impl F10Client { $(
        pub fn $method(&self, query: &$query, options: &OperationOptions) -> Result<Response> {
            self.endpoint(concat!("f10.", stringify!($method)), Instant::now(), || query.to_call(), options)
        }
    )* } };
}
execute_query!(hy_stage_rise: HyStageRiseQuery, hy_members_rank: HyMembersRankQuery,
    qh_position_report: QhPositionReportQuery, qh_basis_spread: QhBasisSpreadQuery,
    qh_warehouse_receipt: QhWarehouseReceiptQuery, qh_variety_info: QhVarietyInfoQuery,
    qh_index_valuation: QhIndexValuationQuery, qh_weights: QhWeightsQuery);
