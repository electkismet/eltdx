//! A-share and fund reports, including prerequisite report-period queries.
use std::num::NonZeroU32;
use std::time::Instant;
use crate::operation::OperationContext;
use crate::types::TradingDate;
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use crate::f10::{AshareCode, CallQuery, DateOrWindow, F10Client, JsonValue, Response, SectionCode};
use crate::f10::values::{count, date_text, j, page, text};

code_endpoint!(StockInfoQuery, stock_info, AshareCode, code, "CWServ.tdxf10_gg_comreq", [j("gpquery"), j(code.as_str())]);
code_endpoint!(BusinessPeriodsQuery, business_periods, AshareCode, code, "CWServ.tdxf10_gg_comreq", [j("zygcfx"), j(code.as_str())]);
code_endpoint!(TopicIdsQuery, topic_ids, AshareCode, code, "CWServ.tdxf10_gg_comreq", [j("rdtcgn"), j(code.as_str())]);
section_endpoint!(CompanyProfileQuery, company_profile, AshareCode, "8", code, section, "CWServ.tdxf10_gg_gsgk", [j(section), j(code.as_str()), j("")]);
section_endpoint!(DividendFinancingQuery, dividend_financing, AshareCode, "fh", code, section, "CWServ.tdxf10_gg_fhrz", [j(code.as_str()), j(section)]);
code_endpoint!(AllotmentDatesQuery, allotment_dates, AshareCode, code, "CWServ.tdxf10_gg_fhrz_zfhpmx", [j("zfpg_bgq"), j(code.as_str()), j("")]);
section_endpoint!(ProfitForecastQuery, profit_forecast, AshareCode, "ylyctj", code, section, "CWServ.tdxf10_gg_ybpj", [j(code.as_str()), j(section)]);
section_endpoint!(RankingDetailQuery, ranking_detail, AshareCode, "scpmdela", code, section, "CWServ.tdxf10_gg_zxts_rqpm", [j(code.as_str()), j(section)]);
section_endpoint!(HotTopicsQuery, hot_topics, AshareCode, "zttzbkz", code, section, "CWServ.tdxf10_gg_rdtc", [j(code.as_str()), j(section)]);
section_endpoint!(StockFinanceWeightsQuery, stock_finance_weights, AshareCode, "zcfzb", code, section, "CWServ.tdxf10_gg_zxtscwbq", [j(code.as_str()), j(section)]);

macro_rules! section_base {
    ($name:ident, $method:literal, $default:literal, {$($field:ident: $ty:ty = $value:expr),* $(,)?}) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name { code: AshareCode, section: SectionCode, $($field: $ty),* }
        impl $name {
            pub fn new(code: AshareCode) -> Self { Self { code, section: SectionCode::default_for($method, $default), $($field: $value),* } }
            pub fn code(&self) -> &AshareCode { &self.code }
            pub fn section(&self) -> &SectionCode { &self.section }
            pub fn with_section(mut self, section: SectionCode) -> Result<Self> { section.checked_for($method)?; self.section = section; Ok(self) }
        }
    };
}
macro_rules! string_field {
    ($name:ident, $field:ident, $setter:ident) => {
        impl $name {
            pub fn $field(&self) -> &str { &self.$field }
            pub fn $setter(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.$field = value; Ok(self) }
        }
    };
}
macro_rules! page_fields {
    ($name:ident) => {
        impl $name {
            pub fn page(&self) -> u32 { self.page }
            pub fn page_size(&self) -> NonZeroU32 { self.page_size }
            pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
            pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
        }
    };
}
macro_rules! date_field {
    ($name:ident, $field:ident, $setter:ident) => {
        impl $name {
            pub fn $field(&self) -> Option<TradingDate> { self.$field }
            pub fn $setter(mut self, value: Option<TradingDate>) -> Self { self.$field = value; self }
        }
    };
}
fn twenty() -> NonZeroU32 { NonZeroU32::new(20).unwrap_or(NonZeroU32::MIN) }
fn nonempty<'a>(value: &'a str, default: &'a str) -> &'a str { if value.is_empty() { default } else { value } }

section_base!(ShareholdersQuery, "shareholders", "count", {
    report_date: Option<TradingDate> = None, kind: String = "000".to_owned(),
    filter1: String = String::new(), filter2: String = String::new(), page: u32 = 1, page_size: NonZeroU32 = twenty()
});
date_field!(ShareholdersQuery, report_date, with_report_date);
string_field!(ShareholdersQuery, kind, with_kind);
string_field!(ShareholdersQuery, filter1, with_filter1);
string_field!(ShareholdersQuery, filter2, with_filter2);
page_fields!(ShareholdersQuery);
impl ShareholdersQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        let section = self.section.as_str();
        let code = self.code.as_str(); let p = self.page.to_string(); let n = self.page_size.get().to_string();
        if section == "institution_holdings" {
            return super::params("CWServ.tdxf10_gg_gdyj_jgcgmx", vec![j(code), j(self.kind.as_str()), j(date_text(self.report_date)), j("99"), j(p.clone()), j(p), j(n)]);
        }
        let wire = match section {
            "count" => "gdrs", "ranking" => "thygdrs", "controller" => "kggd", "top_circulating" => "ltgd",
            "top" => "sdgdbgq", "bonds" => "sdzqcyr", "changes" => "cgbd", "institutions" => "jgcg",
            "institution_types" => "jgcgz", "plan" => "gdzjcjh", _ => unreachable!("closed shareholder registry"),
        };
        let filter2 = if section == "institution_types" { date_text(self.report_date) } else { self.filter2.clone() };
        super::params("CWServ.tdxf10_gg_gdyj", vec![j(code), j(wire), j(self.filter1.as_str()), j(filter2), j(p.clone()), j(p), j(n)])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareholderChangePlansQuery { inner: ShareholdersQuery }
impl ShareholderChangePlansQuery {
    pub fn new(code: AshareCode) -> Self {
        let mut inner = ShareholdersQuery::new(code); inner.section = SectionCode::default_for("shareholders", "plan"); Self { inner }
    }
    pub fn code(&self) -> &AshareCode { self.inner.code() }
    pub fn page(&self) -> u32 { self.inner.page() }
    pub fn page_size(&self) -> NonZeroU32 { self.inner.page_size() }
    pub fn filter1(&self) -> &str { self.inner.filter1() }
    pub fn filter2(&self) -> &str { self.inner.filter2() }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.inner = self.inner.with_page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.inner = self.inner.with_page_size(value)?; Ok(self) }
    pub fn with_filter1(mut self, value: impl Into<String>) -> Result<Self> { self.inner = self.inner.with_filter1(value)?; Ok(self) }
    pub fn with_filter2(mut self, value: impl Into<String>) -> Result<Self> { self.inner = self.inner.with_filter2(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { self.inner.to_call() }
}

section_base!(CapitalStructureQuery, "capital_structure", "structure", { date: Option<TradingDate> = None });
date_field!(CapitalStructureQuery, date, with_date);
impl CapitalStructureQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        if self.section.as_str() == "restricted_detail" {
            let date = self.date.ok_or_else(|| crate::f10::values::invalid("restricted_detail requires date"))?;
            return super::params("CWServ.tdxf10_gg_gbjg_xsjjmx", vec![j(self.code.as_str()), j(date_text(Some(date)))]);
        }
        let section = match self.section.as_str() { "structure" => "gbjg", "changes" => "gbbd", "buyback" => "gphg", "restricted" => "xslt", _ => unreachable!("closed capital registry") };
        super::params("CWServ.tdxf10_gg_gbjg", vec![j(self.code.as_str()), j(section)])
    }
}
section_base!(BusinessReportQuery, "business_report", "business", { report_date: Option<TradingDate> = None });
date_field!(BusinessReportQuery, report_date, with_report_date);
impl BusinessReportQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        let (section, use_date) = match self.section.as_str() {
            "business" => ("zyyw", false), "composition" => ("zygc", true), "customers" => ("qwm", true),
            "suppliers" => ("qwmgys", true), "price" => ("cpjg", false), "changes" => ("0", true), _ => unreachable!("closed business registry"),
        };
        super::params("CWServ.tdxf10_gg_jyfx", vec![j(self.code.as_str()), j(section), j(if use_date { date_text(self.report_date) } else { String::new() })])
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessCompositionQuery { code: AshareCode, report_date: Option<TradingDate> }
impl BusinessCompositionQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, report_date: None } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn report_date(&self) -> Option<TradingDate> { self.report_date }
    pub fn with_report_date(mut self, value: Option<TradingDate>) -> Self { self.report_date = value; self }
}

section_base!(IndustryAnalysisQuery, "industry_analysis", "peers", { arg: String = String::new() });
string_field!(IndustryAnalysisQuery, arg, with_arg);
impl IndustryAnalysisQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        let section = self.section.as_str(); let code = self.code.as_str();
        if matches!(section, "comparable" | "comparable_profile") {
            return super::params("CWServ.tdxf10_gg_kbgs", vec![j(if section == "comparable" { "1" } else { "0" }), j(code)]);
        }
        let (wire, default) = match section {
            "peers" => ("cwgz", "000"), "dividend_rank" => ("fhrzb", "100"), "scale" => ("gsgm", "000"),
            "valuation" => ("gzsp", "011"), "news" => ("hyxw", ""), "reports" => ("hyyb", ""),
            "market_share" => ("scbx", "000"), "summary" => ("tot", ""), _ => unreachable!("closed industry registry"),
        };
        super::params("CWServ.tdxf10_gg_hyfx", vec![j(wire), j(code), j(nonempty(&self.arg, default))])
    }
}
section_base!(TradingReportQuery, "trading_report", "margin", {
    arg: String = String::new(), date: Option<TradingDate> = None, page: u32 = 1, page_size: NonZeroU32 = twenty()
});
string_field!(TradingReportQuery, arg, with_arg);
date_field!(TradingReportQuery, date, with_date);
page_fields!(TradingReportQuery);
impl TradingReportQuery {
    /// Date/page/page_size are retained as compatibility inputs; the source
    /// endpoint does not support those filters and no fabricated fields emit.
    pub fn to_call(&self) -> Result<CallQuery> {
        let code = self.code.as_str(); let section = self.section.as_str();
        if section == "margin_history" { return super::params("CWServ.tdxf10_gg_sj", vec![j("rzrq_sj"), j(code), j("0"), j(code)]); }
        let wire = match section {
            "margin" => "rzrq", "block_trades" => "dzjy", "dragon_board" => "lgt", "northbound" => "islgt",
            "limit_down" => "dtfx", "limit_up" => "ztfx", "bonds" => "zrq", "main_force" => "zlpskcj", "reports" => "yxsbxx", "kcb" => "checkkcb",
            _ => unreachable!("closed trading registry"),
        };
        super::params("CWServ.tdxf10_gg_jyds", vec![j(code), j(wire), j(if section == "dragon_board" { nonempty(&self.arg, "2") } else { "" })])
    }
}

section_base!(FundReportQuery, "fund_report", "basic", {
    arg: String = String::new(), date: Option<DateOrWindow> = None, page: u32 = 1, page_size: NonZeroU32 = twenty()
});
string_field!(FundReportQuery, arg, with_arg);
page_fields!(FundReportQuery);
impl FundReportQuery {
    pub fn date(&self) -> Option<&DateOrWindow> { self.date.as_ref() }
    pub fn with_date(mut self, value: Option<DateOrWindow>) -> Result<Self> {
        if matches!(&value, Some(DateOrWindow::Window(_))) && !matches!(self.section.as_str(), "etf" | "margin" | "scale_history") {
            return Err(crate::f10::values::invalid("this fund section requires a date, not a window"));
        }
        self.date = value; Ok(self)
    }
    pub(crate) fn prerequisite(&self) -> Result<Option<CallQuery>> {
        if self.date.is_some() { return Ok(None); }
        let section = match self.section.as_str() { "industry_allocation" => "hyfbbgq", "bond_allocation" => "zczqbgq", "futures_allocation" => "ccqhmxbgq", _ => return Ok(None) };
        Ok(Some(super::params("CWServ.skef10_jj_tzzh", vec![j(section), j(self.code.as_str()), j("")])?))
    }
    pub fn to_call(&self) -> Result<CallQuery> { self.resolved(self.date.as_ref().map(DateOrWindow::wire).as_deref()) }
    fn resolved(&self, date: Option<&str>) -> Result<CallQuery> {
        let section = self.section.as_str();
        if matches!(&self.date, Some(DateOrWindow::Window(_))) && !matches!(section, "etf" | "margin" | "scale_history") {
            return Err(crate::f10::values::invalid("this fund section requires a date, not a window"));
        }
        if date.is_none() && self.prerequisite()?.is_some() {
            return Err(Error::new(ErrorKind::MetadataUnavailable, "fund report requires a report-period prerequisite"));
        }
        let date = date.unwrap_or(""); let code = self.code.as_str(); let arg = self.arg.as_str();
        let p = self.page.to_string(); let n = self.page_size.get().to_string();
        let (suffix, params) = match section {
            "name" => ("idreq", vec![j("name"), j(code)]),
            "managers" | "company" | "basic" | "seats" => {
                let tag = match section { "managers" => "1", "company" => "2", "basic" => "4", _ => "5" };
                ("jjzl", vec![j(tag), j(code), j(if section == "seats" { arg } else { "" }), j(p.clone()), j(p), j(n)])
            }
            "scale" => ("gmfe", vec![j("0"), j(code), j(""), j("")]),
            "scale_history" => ("gmfe", vec![j(nonempty(arg, "0")), j(code), j(""), j(date)]),
            "nav" | "nav_period" | "nav_rank" | "dividends" => {
                let tag = match section { "nav" => "0", "nav_period" => "1", "nav_rank" => "3", _ => "5" };
                ("jzhb", vec![j(tag), j(code), j("0"), j("")])
            }
            "asset_allocation" | "industry_allocation" | "holdings_compare" | "holdings_rank" | "holding_summary" | "bond_allocation" | "futures_allocation" => {
                let tag = match section { "asset_allocation" => "zcpz", "industry_allocation" => "hyfb", "holdings_compare" => "cgjzd", "holdings_rank" => "zjpz", "holding_summary" => "jjxtzzh", "bond_allocation" => "zczq", _ => "ccqhmx" };
                ("tzzh", vec![j(tag), j(code), j(if matches!(section, "industry_allocation" | "bond_allocation" | "futures_allocation") { date } else { "" })])
            }
            "holdings" | "holdings_detail" | "holdings_changes" | "fund_of_funds" => {
                let tag = match section { "holdings" => "cgqkbgq", "holdings_detail" => "cgmxbgq", "holdings_changes" => "cyjjmxbgq", _ => "sxjjzcbgq" };
                ("cgqk", vec![j(tag), j(code), j(""), j(""), j("1"), j("1"), j("1")])
            }
            "finance" => ("cwzk", vec![j(nonempty(arg, "zcfzb")), j(code)]),
            "sales_info" | "sales_announcements" | "sales_news" => ("xsxx", vec![j(match section { "sales_info" => "0", "sales_announcements" => "4", _ => "5" }), j(code)]),
            "etf" | "margin" => ("etfzj", vec![j(if section == "etf" { "etfzjlx" } else { "etfrzrq" }), j(code), j(nonempty(date, "3M"))]),
            "ratings" => ("jjpj", vec![j(nonempty(arg, "001")), j(code)]),
            "categories" => ("fxsy", vec![j(nonempty(arg, "bgqzf")), j(code), j("")]),
            _ => unreachable!("closed fund report registry"),
        };
        super::params(&format!("CWServ.skef10_jj_{suffix}"), params)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllotmentDetailsQuery { code: AshareCode, date: TradingDate }
impl AllotmentDetailsQuery {
    pub fn new(code: AshareCode, date: TradingDate) -> Self { Self { code, date } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn date(&self) -> TradingDate { self.date }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_fhrz_zfhpmx", vec![j("zfpg"), j(self.code.as_str()), j(date_text(Some(self.date)))]) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinanceReportQuery { code: AshareCode, report_type: String }
impl FinanceReportQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, report_type: "zcfzb".to_owned() } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn report_type(&self) -> &str { &self.report_type }
    pub fn with_report_type(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.report_type = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_cwfx", vec![j(self.code.as_str()), j(self.report_type.as_str()), j("")]) }
}
section_base!(FinanceDiagnosisQuery, "finance_diagnosis", "zzyj", { scope: String = String::new() });
string_field!(FinanceDiagnosisQuery, scope, with_scope);
impl FinanceDiagnosisQuery { pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_cwzd", vec![j(self.section.as_str()), j(self.code.as_str()), j(self.scope.as_str())]) } }
section_base!(StockScoreQuery, "stock_score", "pf", { arg: String = String::new() });
string_field!(StockScoreQuery, arg, with_arg);
impl StockScoreQuery { pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_ggzp", vec![j(self.section.as_str()), j(self.code.as_str()), j(self.arg.as_str()), j("")]) } }
section_base!(GovernanceQuery, "governance", "gljy", { arg: String = String::new() });
string_field!(GovernanceQuery, arg, with_arg);
impl GovernanceQuery { pub fn to_call(&self) -> Result<CallQuery> { super::params("CWServ.tdxf10_gg_zbyz", vec![j(self.section.as_str()), j(self.code.as_str()), j(self.arg.as_str())]) } }

fn first_scalar(response: &Response, field: &str, context: &OperationContext) -> Result<Option<String>> {
    if let Some(table) = response.first_table() {
        for row in table.rows() {
            context.check_boundary(TimeoutPhase::Decode)?;
            if let Some(cell) = row.get_all(field).next() {
                return Ok(match cell.value() { JsonValue::String(value) if !value.is_empty() => Some(value.clone()), JsonValue::Number(value) => Some(value.lexeme().to_owned()), _ => None });
            }
        }
    }
    Ok(None)
}
fn latest_period(response: &Response, context: &OperationContext) -> Result<String> {
    if let Some(row) = response.first_table().and_then(|table| table.rows().first()) {
        context.check_boundary(TimeoutPhase::Decode)?;
        if let Some(cell) = row.get_all("rq").next() {
                let value = match cell.value() { JsonValue::String(value) => Some(value.clone()), JsonValue::Number(value) => Some(value.lexeme().to_owned()), _ => None };
                if let Some(value) = value {
                let digits: String = value.chars().filter(|ch| *ch != '-').collect();
                if digits.len() == 8 && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    let year = digits[..4].parse().ok(); let month = digits[4..6].parse().ok(); let day = digits[6..].parse().ok();
                    if let (Some(year), Some(month), Some(day)) = (year, month, day) {
                        if TradingDate::new(year, month, day).is_ok() { return Ok(digits); }
                    }
                }
                }
        }
    }
    Err(missing_metadata(response, "no valid fund report period"))
}
fn missing_metadata(response: &Response, message: &str) -> Error {
    Error::new(ErrorKind::MetadataUnavailable, message).with_context("entry", response.entry())
        .with_context("server_error_code", response.error_code().map_or_else(|| "absent".to_owned(), |value| value.to_string()))
}
impl F10Client {
    pub fn business_composition(&self, query: &BusinessCompositionQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.business_composition", options)?;
        let (date, prerequisite) = if let Some(date) = query.report_date { (date_text(Some(date)), None) } else {
            let response = self.endpoint_call(&BusinessPeriodsQuery::new(query.code.clone()).to_call()?, &context)?;
            let date = first_scalar(&response, "T002", &context)?.ok_or_else(|| missing_metadata(&response, "no business composition report period"))?;
            (date, Some(response))
        };
        let call = super::params("CWServ.tdxf10_gg_jyfx", vec![j(query.code.as_str()), j("zygc"), j(date)])?;
        let mut response = self.endpoint_call(&call, &context)?;
        if let Some(prerequisite) = prerequisite { response = response.with_prerequisite(&prerequisite); }
        context.finish()?; Ok(response)
    }
    pub fn fund_report(&self, query: &FundReportQuery, options: &OperationOptions) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.fund_report", options)?;
        let prerequisite = query.prerequisite()?.map(|call| self.endpoint_call(&call, &context)).transpose()?;
        let call = if let Some(response) = &prerequisite { query.resolved(Some(&latest_period(response, &context)?))? } else { query.to_call()? };
        let mut response = self.endpoint_call(&call, &context)?;
        if let Some(prerequisite) = prerequisite { response = response.with_prerequisite(&prerequisite); }
        context.finish()?; Ok(response)
    }
}

macro_rules! execute_queries { ($($method:ident: $query:ident),* $(,)?) => { impl F10Client { $(
    pub fn $method(&self, query: &$query, options: &OperationOptions) -> Result<Response> {
        self.endpoint(concat!("f10.", stringify!($method)), Instant::now(), || query.to_call(), options)
    }
)* } }; }
execute_queries!(shareholders: ShareholdersQuery, shareholder_change_plans: ShareholderChangePlansQuery,
    capital_structure: CapitalStructureQuery, business_report: BusinessReportQuery, industry_analysis: IndustryAnalysisQuery,
    trading_report: TradingReportQuery, allotment_details: AllotmentDetailsQuery, finance_report: FinanceReportQuery,
    finance_diagnosis: FinanceDiagnosisQuery, stock_score: StockScoreQuery, governance: GovernanceQuery);
