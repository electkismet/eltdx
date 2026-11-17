//! A-share and fund reports, including prerequisite report-period queries.
use super::SimpleArgument::{Code, Literal, Section};
use crate::f10::values::{count, date_text, j, page, text};
use crate::f10::{
    AshareCode, CallQuery, DateOrWindow, F10Client, JsonValue, Response, SectionCode,
};
use crate::operation::OperationContext;
use crate::types::TradingDate;
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use std::num::NonZeroU32;
use std::time::Instant;

code_endpoint!(StockInfoQuery, stock_info, AshareCode, code, "CWServ.tdxf10_gg_comreq", wire [Literal("gpquery"), Code]);
code_endpoint!(BusinessPeriodsQuery, business_periods, AshareCode, code, "CWServ.tdxf10_gg_comreq", wire [Literal("zygcfx"), Code]);
code_endpoint!(TopicIdsQuery, topic_ids, AshareCode, code, "CWServ.tdxf10_gg_comreq", wire [Literal("rdtcgn"), Code]);
section_endpoint!(CompanyProfileQuery, company_profile, AshareCode, "8", code, section, "CWServ.tdxf10_gg_gsgk", wire [Section, Code, Literal("")]);
section_endpoint!(DividendFinancingQuery, dividend_financing, AshareCode, "fh", code, section, "CWServ.tdxf10_gg_fhrz", wire [Code, Section]);
code_endpoint!(AllotmentDatesQuery, allotment_dates, AshareCode, code, "CWServ.tdxf10_gg_fhrz_zfhpmx", wire [Literal("zfpg_bgq"), Code, Literal("")]);
section_endpoint!(ProfitForecastQuery, profit_forecast, AshareCode, "ylyctj", code, section, "CWServ.tdxf10_gg_ybpj", wire [Code, Section]);
section_endpoint!(RankingDetailQuery, ranking_detail, AshareCode, "scpmdela", code, section, "CWServ.tdxf10_gg_zxts_rqpm", wire [Code, Section]);
section_endpoint!(HotTopicsQuery, hot_topics, AshareCode, "zttzbkz", code, section, "CWServ.tdxf10_gg_rdtc", wire [Code, Section]);
section_endpoint!(StockFinanceWeightsQuery, stock_finance_weights, AshareCode, "zcfzb", code, section, "CWServ.tdxf10_gg_zxtscwbq", wire [Code, Section]);

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
            pub fn $field(&self) -> &str {
                &self.$field
            }
            pub fn $setter(mut self, value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                text(&value, true)?;
                self.$field = value;
                Ok(self)
            }
        }
    };
}
macro_rules! page_fields {
    ($name:ident) => {
        impl $name {
            pub fn page(&self) -> u32 {
                self.page
            }
            pub fn page_size(&self) -> NonZeroU32 {
                self.page_size
            }
            pub fn with_page(mut self, value: u32) -> Result<Self> {
                self.page = page(value)?;
                Ok(self)
            }
            pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> {
                self.page_size = count(value)?;
                Ok(self)
            }
        }
    };
}
macro_rules! date_field {
    ($name:ident, $field:ident, $setter:ident) => {
        impl $name {
            pub fn $field(&self) -> Option<TradingDate> {
                self.$field
            }
            pub fn $setter(mut self, value: Option<TradingDate>) -> Self {
                self.$field = value;
                self
            }
        }
    };
}
fn twenty() -> NonZeroU32 {
    NonZeroU32::new(20).unwrap_or(NonZeroU32::MIN)
}

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
        super::report_templates::native_report(
            "shareholders",
            self.code.as_str(),
            self.section.as_str(),
            |name| {
                Ok(match name {
                    "filter1" => j(self.filter1.as_str()),
                    "filter2" => j(self.filter2.as_str()),
                    "kind" => j(self.kind.as_str()),
                    "date" => j(date_text(self.report_date)),
                    "page" => j(format!("{}", self.page)),
                    "page_size" => j(format!("{}", self.page_size.get())),
                    _ => unreachable!("built-in shareholder input"),
                })
            },
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareholderChangePlansQuery {
    inner: ShareholdersQuery,
}
impl ShareholderChangePlansQuery {
    pub fn new(code: AshareCode) -> Self {
        let mut inner = ShareholdersQuery::new(code);
        inner.section = SectionCode::default_for("shareholders", super::SHAREHOLDER_PLAN_SECTION);
        Self { inner }
    }
    pub fn code(&self) -> &AshareCode {
        self.inner.code()
    }
    pub fn page(&self) -> u32 {
        self.inner.page()
    }
    pub fn page_size(&self) -> NonZeroU32 {
        self.inner.page_size()
    }
    pub fn filter1(&self) -> &str {
        self.inner.filter1()
    }
    pub fn filter2(&self) -> &str {
        self.inner.filter2()
    }
    pub fn with_page(mut self, value: u32) -> Result<Self> {
        self.inner = self.inner.with_page(value)?;
        Ok(self)
    }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> {
        self.inner = self.inner.with_page_size(value)?;
        Ok(self)
    }
    pub fn with_filter1(mut self, value: impl Into<String>) -> Result<Self> {
        self.inner = self.inner.with_filter1(value)?;
        Ok(self)
    }
    pub fn with_filter2(mut self, value: impl Into<String>) -> Result<Self> {
        self.inner = self.inner.with_filter2(value)?;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        self.inner.to_call()
    }
}

section_base!(CapitalStructureQuery, "capital_structure", "structure", { date: Option<TradingDate> = None });
date_field!(CapitalStructureQuery, date, with_date);
impl CapitalStructureQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        if self.section.as_str() == "restricted_detail" && self.date.is_none() {
            return Err(crate::f10::values::invalid(
                "restricted_detail requires date",
            ));
        }
        super::report_templates::native_report(
            "capital_structure",
            self.code.as_str(),
            self.section.as_str(),
            |_| Ok(j(date_text(self.date))),
        )
    }
}
section_base!(BusinessReportQuery, "business_report", "business", { report_date: Option<TradingDate> = None });
date_field!(BusinessReportQuery, report_date, with_report_date);
impl BusinessReportQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::report_templates::native_report(
            "business_report",
            self.code.as_str(),
            self.section.as_str(),
            |_| Ok(j(date_text(self.report_date))),
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessCompositionQuery {
    code: AshareCode,
    report_date: Option<TradingDate>,
}
impl BusinessCompositionQuery {
    pub fn new(code: AshareCode) -> Self {
        Self {
            code,
            report_date: None,
        }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn report_date(&self) -> Option<TradingDate> {
        self.report_date
    }
    pub fn with_report_date(mut self, value: Option<TradingDate>) -> Self {
        self.report_date = value;
        self
    }
}

section_base!(IndustryAnalysisQuery, "industry_analysis", "peers", { arg: String = String::new() });
string_field!(IndustryAnalysisQuery, arg, with_arg);
impl IndustryAnalysisQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::report_templates::native_report(
            "industry_analysis",
            self.code.as_str(),
            self.section.as_str(),
            |_| Ok(j(self.arg.as_str())),
        )
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
        super::report_templates::native_report(
            "trading_report",
            self.code.as_str(),
            self.section.as_str(),
            |_| Ok(j(self.arg.as_str())),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundReportQuery {
    code: AshareCode,
    section: SectionCode,
    arg: String,
    date: Option<DateOrWindow>,
    page: u32,
    page_size: NonZeroU32,
}
string_field!(FundReportQuery, arg, with_arg);
page_fields!(FundReportQuery);
impl FundReportQuery {
    pub fn new(code: AshareCode) -> Self {
        Self {
            code,
            section: SectionCode::default_for("fund_report", "basic"),
            arg: String::new(),
            date: None,
            page: 1,
            page_size: twenty(),
        }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn section(&self) -> &SectionCode {
        &self.section
    }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> {
        value.checked_for("fund_report")?;
        valid_fund_date(value.as_str(), self.date.as_ref())?;
        self.section = value;
        Ok(self)
    }
    pub fn date(&self) -> Option<&DateOrWindow> {
        self.date.as_ref()
    }
    pub fn with_date(mut self, value: Option<DateOrWindow>) -> Result<Self> {
        valid_fund_date(self.section.as_str(), value.as_ref())?;
        self.date = value;
        Ok(self)
    }
    pub(crate) fn prerequisite(&self) -> Result<Option<CallQuery>> {
        if self.date.is_some() {
            return Ok(None);
        }
        let Some(section) = super::fund_period_section(self.section.as_str()) else {
            return Ok(None);
        };
        Ok(Some(super::params(
            "CWServ.skef10_jj_tzzh",
            vec![j(section), j(self.code.as_str()), j("")],
        )?))
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        self.resolved(self.date.as_ref().map(DateOrWindow::wire).as_deref())
    }
    fn resolved(&self, date: Option<&str>) -> Result<CallQuery> {
        let section = self.section.as_str();
        valid_fund_date(section, self.date.as_ref())?;
        if date.is_none() && self.prerequisite()?.is_some() {
            return Err(Error::new(
                ErrorKind::MetadataUnavailable,
                "fund report requires a report-period prerequisite",
            ));
        }
        let date = date.unwrap_or("");
        super::report_templates::native_report("fund_report", self.code.as_str(), section, |name| {
            Ok(match name {
                "arg" => j(self.arg.as_str()),
                "date" => j(date),
                "page" => j(format!("{}", self.page)),
                "page_size" => j(format!("{}", self.page_size.get())),
                _ => unreachable!("built-in fund input"),
            })
        })
    }
}

fn valid_fund_date(section: &str, date: Option<&DateOrWindow>) -> Result<()> {
    if matches!(date, Some(DateOrWindow::Window(_)))
        && !matches!(section, "etf" | "margin" | "scale_history")
    {
        return Err(crate::f10::values::invalid(
            "this fund section requires a date, not a window",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllotmentDetailsQuery {
    code: AshareCode,
    date: TradingDate,
}
impl AllotmentDetailsQuery {
    pub fn new(code: AshareCode, date: TradingDate) -> Self {
        Self { code, date }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn date(&self) -> TradingDate {
        self.date
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("allotment_details", &self.code, "", |_| {
            Ok(j(date_text(Some(self.date))))
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinanceReportQuery {
    code: AshareCode,
    report_type: String,
}
impl FinanceReportQuery {
    pub fn new(code: AshareCode) -> Self {
        Self {
            code,
            report_type: "zcfzb".to_owned(),
        }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn report_type(&self) -> &str {
        &self.report_type
    }
    pub fn with_report_type(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        text(&value, false)?;
        self.report_type = value;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("finance_report", &self.code, "", |_| {
            Ok(j(self.report_type.as_str()))
        })
    }
}
section_base!(FinanceDiagnosisQuery, "finance_diagnosis", "zzyj", { scope: String = String::new() });
string_field!(FinanceDiagnosisQuery, scope, with_scope);
impl FinanceDiagnosisQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named(
            "finance_diagnosis",
            &self.code,
            self.section.as_str(),
            |_| Ok(j(self.scope.as_str())),
        )
    }
}
section_base!(StockScoreQuery, "stock_score", "pf", { arg: String = String::new() });
string_field!(StockScoreQuery, arg, with_arg);
impl StockScoreQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("stock_score", &self.code, self.section.as_str(), |_| {
            Ok(j(self.arg.as_str()))
        })
    }
}
section_base!(GovernanceQuery, "governance", "gljy", { arg: String = String::new() });
string_field!(GovernanceQuery, arg, with_arg);
impl GovernanceQuery {
    pub fn to_call(&self) -> Result<CallQuery> {
        super::template::native_named("governance", &self.code, self.section.as_str(), |_| {
            Ok(j(self.arg.as_str()))
        })
    }
}

fn first_scalar(
    response: &Response,
    field: &str,
    context: &OperationContext,
) -> Result<Option<String>> {
    if !response.ok() {
        return Ok(None);
    }
    if let Some(row) = response
        .first_table()
        .and_then(|table| table.rows().first())
    {
        let selected =
            crate::f10::collection::first_match(row.cells().iter().map(Ok::<_, Error>), |cell| {
                context.check_boundary(TimeoutPhase::Decode)?;
                if cell.name() != field {
                    return Ok(None);
                }
                let value = match cell.value() {
                    JsonValue::String(value) => value.as_str(),
                    JsonValue::Number(value) => value.lexeme(),
                    _ => return Ok(Some(None)),
                };
                // Prerequisites here are dates, never arbitrary display strings.
                // Bound the copy before validating Gregorian syntax below.
                Ok(Some(
                    (!value.is_empty() && value.len() <= 10).then(|| value.to_owned()),
                ))
            })?;
        return Ok(selected.flatten());
    }
    Ok(None)
}
fn canonical_period(value: &str) -> Option<String> {
    if value.len() != 8 && value.len() != 10 {
        return None;
    }
    let digits = if value.len() == 10 {
        let bytes = value.as_bytes();
        if !matches!((bytes[4], bytes[7]), (b'-', b'-') | (b'/', b'/')) {
            return None;
        }
        value
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (!matches!(index, 4 | 7)).then_some(byte))
            .collect::<Vec<_>>()
    } else {
        value.as_bytes().to_vec()
    };
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let digits = String::from_utf8(digits).ok()?;
    let year = digits[..4].parse().ok()?;
    let month = digits[4..6].parse().ok()?;
    let day = digits[6..].parse().ok()?;
    TradingDate::new(year, month, day).ok()?;
    Some(digits)
}
fn latest_period(response: &Response, context: &OperationContext) -> Result<String> {
    if let Some(period) = first_scalar(response, "rq", context)?
        .as_deref()
        .and_then(canonical_period)
    {
        return Ok(period);
    }
    Err(missing_metadata(response, "no valid fund report period"))
}
fn missing_metadata(response: &Response, message: &str) -> Error {
    Error::new(ErrorKind::MetadataUnavailable, message)
        .with_context("entry", response.entry())
        .with_context(
            "server_error_code",
            response
                .error_code()
                .map_or_else(|| "absent".to_owned(), |value| value.to_string()),
        )
}
impl F10Client {
    pub fn business_composition(
        &self,
        query: &BusinessCompositionQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.business_composition", options)?;
        let (date, prerequisite) = if let Some(date) = query.report_date {
            (date_text(Some(date)), None)
        } else {
            let response = self.endpoint_call(
                &BusinessPeriodsQuery::new(query.code.clone()).to_call()?,
                &context,
            )?;
            let date = first_scalar(&response, "T002", &context)?
                .as_deref()
                .and_then(canonical_period)
                .ok_or_else(|| {
                    missing_metadata(&response, "no valid business composition report period")
                })?;
            (date, Some(response))
        };
        let call = super::report_templates::native_report(
            "business_report",
            query.code.as_str(),
            "composition",
            |_| Ok(j(date.as_str())),
        )?;
        let mut response = self.endpoint_call(&call, &context)?;
        if let Some(prerequisite) = prerequisite {
            response = response.with_prerequisite(&prerequisite, &context)?;
        }
        context.finish()?;
        Ok(response)
    }
    pub fn fund_report(
        &self,
        query: &FundReportQuery,
        options: &OperationOptions,
    ) -> Result<Response> {
        let context = self.context(Instant::now(), "f10.fund_report", options)?;
        let prerequisite = query
            .prerequisite()?
            .map(|call| self.endpoint_call(&call, &context))
            .transpose()?;
        let call = if let Some(response) = &prerequisite {
            query.resolved(Some(&latest_period(response, &context)?))?
        } else {
            query.to_call()?
        };
        let mut response = self.endpoint_call(&call, &context)?;
        if let Some(prerequisite) = prerequisite {
            response = response.with_prerequisite(&prerequisite, &context)?;
        }
        context.finish()?;
        Ok(response)
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
