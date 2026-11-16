//! Explicit request-ID families used by themes, stage gain and valuation.
use std::num::NonZeroU32;
use std::time::Instant;
use crate::{OperationOptions, Result};
use crate::f10::{AshareCode, CallQuery, EntryName, F10Client, F10MarketCode, JsonValue, OrderedJsonObject,
    PageSelector, PageSizeSelector, RequestId, RequestPayload, Response};
use crate::f10::values::{count, j, text};

fn nz(value: u32) -> NonZeroU32 { NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN) }
fn id(value: &'static str) -> RequestId { RequestId::new(value).expect("built-in decimal request id") }
fn object_call(entry: &str, payload: OrderedJsonObject) -> Result<CallQuery> {
    Ok(CallQuery::new(EntryName::new(entry)?, RequestPayload::Body(JsonValue::Array(vec![JsonValue::Object(payload)]))))
}
fn checked_extra(extra: &OrderedJsonObject) -> Result<()> {
    let mut empty = Vec::new();
    super::super::request::merge_extra(&mut empty, extra, &["ReqId", "Code", "code", "setcode", "Page", "PageSize", "modname"])
}
fn apply_extra(payload: &mut OrderedJsonObject, extra: &OrderedJsonObject) -> Result<()> {
    checked_extra(extra)?;
    // Non-identity defaults such as Type/sjfw/zb may be selected by the caller.
    // Remove them before strict merge, preserving the caller's order exactly.
    payload.retain(|(name, _)| !extra.iter().any(|(key, _)| key == name));
    super::super::request::merge_extra(payload, extra, &["ReqId", "Code", "code", "setcode", "Page", "PageSize", "modname"])
}
fn page_number(value: PageSelector) -> JsonValue { match value { PageSelector::All => j(-1_i64), PageSelector::Number(value) => j(u64::from(value)) } }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeMarketQuery { code: AshareCode, req_id: RequestId, page: PageSelector, page_size: NonZeroU32, zq_num: NonZeroU32, extra: OrderedJsonObject }
impl ThemeMarketQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, req_id: id("200743"), page: PageSelector::All, page_size: nz(10), zq_num: nz(30), extra: Vec::new() } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn req_id(&self) -> &RequestId { &self.req_id }
    pub fn page(&self) -> PageSelector { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn zq_num(&self) -> NonZeroU32 { self.zq_num }
    pub fn extra(&self) -> &[(String, JsonValue)] { &self.extra }
    pub fn with_req_id(mut self, value: RequestId) -> Self { self.req_id = value; self }
    pub fn with_page(mut self, value: PageSelector) -> Result<Self> { self.page = value.checked(false, true)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn with_zq_num(mut self, value: NonZeroU32) -> Result<Self> { self.zq_num = count(value)?; Ok(self) }
    pub fn with_extra(mut self, value: OrderedJsonObject) -> Result<Self> { checked_extra(&value)?; self.extra = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let mut payload = vec![
            ("ReqId".to_owned(), j(self.req_id.as_str())), ("setcode".to_owned(), j(u64::from(self.code.market().get()))),
            ("code".to_owned(), j(self.code.as_str())), ("Page".to_owned(), page_number(self.page)),
            ("PageSize".to_owned(), j(self.page_size.get().to_string())), ("modname".to_owned(), j("mod_tcihq.dll")),
        ];
        if matches!(self.req_id.as_str(), "200740" | "200742" | "200745" | "200747") { payload.push(("zq_num".to_owned(), j(self.zq_num.get().to_string()))); }
        apply_extra(&mut payload, &self.extra)?;
        object_call("HQServ.hq_nlp_tcihq", payload)
    }
}

macro_rules! stage_query {
    ($name:ident, $request:literal, $days:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name { code: AshareCode, days: NonZeroU32, page_size: NonZeroU32 }
        impl $name {
            pub fn new(code: AshareCode) -> Self { Self { code, days: nz($days), page_size: nz(10) } }
            pub fn code(&self) -> &AshareCode { &self.code }
            pub fn days(&self) -> NonZeroU32 { self.days }
            pub fn page_size(&self) -> NonZeroU32 { self.page_size }
            pub fn with_days(mut self, value: NonZeroU32) -> Result<Self> { self.days = count(value)?; Ok(self) }
            pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
            pub fn to_call(&self) -> Result<CallQuery> {
                ThemeMarketQuery::new(self.code.clone()).with_req_id(id($request)).with_zq_num(self.days)?.with_page_size(self.page_size)?.to_call()
            }
        }
    };
}
stage_query!(StockStageGainQuery, "200740", 60);
stage_query!(StockMainFundsQuery, "200742", 30);
stage_query!(StockIndustryRankQuery, "200745", 30);
stage_query!(StockIndustryPctRankQuery, "200747", 30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockBoardMemberQuotesQuery { board_code: AshareCode, board_market: F10MarketCode, page: PageSelector, page_size: PageSizeSelector }
impl StockBoardMemberQuotesQuery {
    pub fn new(board_code: AshareCode) -> Self { Self { board_code, board_market: F10MarketCode::new(1), page: PageSelector::All, page_size: PageSizeSelector::All } }
    pub fn board_code(&self) -> &AshareCode { &self.board_code }
    pub fn board_market(&self) -> F10MarketCode { self.board_market }
    pub fn page(&self) -> PageSelector { self.page }
    pub fn page_size(&self) -> PageSizeSelector { self.page_size }
    pub fn with_board_market(mut self, value: F10MarketCode) -> Self { self.board_market = value; self }
    pub fn with_page(mut self, value: PageSelector) -> Result<Self> { self.page = value.checked(false, true)?; Ok(self) }
    pub fn with_page_size(mut self, value: PageSizeSelector) -> Result<Self> { self.page_size = value.checked(true)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        object_call("HQServ.hq_nlp_tcihq", vec![("ReqId".to_owned(), j("200744")), ("setcode".to_owned(), j(self.board_market.get().to_string())),
            ("code".to_owned(), j(self.board_code.as_str())), ("Page".to_owned(), page_number(self.page)),
            ("PageSize".to_owned(), j(self.page_size.wire())), ("modname".to_owned(), j("mod_tcihq.dll"))])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValuationQuery { code: AshareCode, req_id: RequestId, page: PageSelector, page_size: NonZeroU32, extra: OrderedJsonObject }
impl ValuationQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, req_id: id("200191"), page: PageSelector::Number(0), page_size: nz(20), extra: Vec::new() } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn req_id(&self) -> &RequestId { &self.req_id }
    pub fn page(&self) -> PageSelector { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn extra(&self) -> &[(String, JsonValue)] { &self.extra }
    pub fn with_req_id(mut self, value: RequestId) -> Self { self.req_id = value; self }
    pub fn with_page(mut self, value: PageSelector) -> Result<Self> { self.page = value.checked(true, true)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn with_extra(mut self, value: OrderedJsonObject) -> Result<Self> { checked_extra(&value)?; self.extra = value; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let request = self.req_id.as_str();
        let mut payload = vec![("ReqId".to_owned(), j(request))];
        if matches!(request, "200191" | "200192") {
            let page = if request == "200192" && self.page == PageSelector::Number(0) { PageSelector::All } else { self.page };
            let size = if request == "200192" && self.page_size.get() == 20 { 5 } else { self.page_size.get() };
            payload.extend([
                ("Code".to_owned(), j(format!("{}|{}", self.code.as_str(), self.code.market().get()))),
                ("BeginDate".to_owned(), j("0")), ("EndDate".to_owned(), j("0")),
                ("Page".to_owned(), j(page.wire())), ("PageSize".to_owned(), j(size.to_string())),
            ]);
            if request == "200192" { payload.push(("Type".to_owned(), j("0"))); }
        } else {
            payload.extend([("code".to_owned(), j(self.code.as_str())), ("setcode".to_owned(), j(self.code.market().get().to_string()))]);
            if request == "200124" {
                let page = if self.page == PageSelector::Number(0) { PageSelector::Number(1) } else { self.page };
                if page == PageSelector::All { return Err(crate::f10::values::invalid("PE/PB series requires a numbered page")); }
                let size = if self.page_size.get() == 20 { 300 } else { self.page_size.get() };
                payload.extend([
                    ("sjfw".to_owned(), j("1Y")), ("sdate".to_owned(), j("")), ("edate".to_owned(), j("")),
                    ("zb".to_owned(), j("0")), ("hqzq".to_owned(), j("0")), ("cwppgz".to_owned(), j(1_i64)), ("jglx".to_owned(), j(0_i64)),
                    ("Page".to_owned(), page_number(page)), ("PageSize".to_owned(), j(size.to_string())),
                ]);
            }
        }
        payload.push(("modname".to_owned(), j("mod_gpsj.dll")));
        apply_extra(&mut payload, &self.extra)?;
        object_call("HQServ.hq_nlp_gpsj", payload)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockValuationPercentileQuery { code: AshareCode, kind: String, page: PageSelector, page_size: NonZeroU32 }
impl StockValuationPercentileQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, kind: "0".to_owned(), page: PageSelector::All, page_size: nz(5) } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn kind(&self) -> &str { &self.kind }
    pub fn page(&self) -> PageSelector { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_kind(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.kind = value; Ok(self) }
    pub fn with_page(mut self, value: PageSelector) -> Result<Self> { self.page = value.checked(false, true)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        // These values are explicit: a requested page_size=20 is really 20,
        // rather than the legacy generic method's 20->5 default substitution.
        object_call("HQServ.hq_nlp_gpsj", vec![("ReqId".to_owned(), j("200192")),
            ("Code".to_owned(), j(format!("{}|{}", self.code.as_str(), self.code.market().get()))),
            ("BeginDate".to_owned(), j("0")), ("EndDate".to_owned(), j("0")), ("Page".to_owned(), j(self.page.wire())),
            ("PageSize".to_owned(), j(self.page_size.get().to_string())), ("Type".to_owned(), j(self.kind.as_str())), ("modname".to_owned(), j("mod_gpsj.dll"))])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockPepbPercentileQuery { code: AshareCode, indicator: String, window: String, period: String, page: u32, page_size: NonZeroU32 }
impl StockPepbPercentileQuery {
    pub fn new(code: AshareCode) -> Self { Self { code, indicator: "0".to_owned(), window: "1Y".to_owned(), period: "0".to_owned(), page: 1, page_size: nz(300) } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn indicator(&self) -> &str { &self.indicator }
    pub fn window(&self) -> &str { &self.window }
    pub fn period(&self) -> &str { &self.period }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_indicator(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.indicator = value; Ok(self) }
    pub fn with_window(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.window = value; Ok(self) }
    pub fn with_period(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, false)?; self.period = value; Ok(self) }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = crate::f10::values::page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        object_call("HQServ.hq_nlp_gpsj", vec![("ReqId".to_owned(), j("200124")), ("code".to_owned(), j(self.code.as_str())),
            ("setcode".to_owned(), j(self.code.market().get().to_string())), ("sjfw".to_owned(), j(self.window.as_str())),
            ("sdate".to_owned(), j("")), ("edate".to_owned(), j("")), ("zb".to_owned(), j(self.indicator.as_str())), ("hqzq".to_owned(), j(self.period.as_str())),
            ("cwppgz".to_owned(), j(1_i64)), ("jglx".to_owned(), j(0_i64)), ("Page".to_owned(), j(u64::from(self.page))),
            ("PageSize".to_owned(), j(self.page_size.get().to_string())), ("modname".to_owned(), j("mod_gpsj.dll"))])
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockHistoryTrendQuery { code: AshareCode }
impl StockHistoryTrendQuery {
    pub fn new(code: AshareCode) -> Self { Self { code } }
    pub fn code(&self) -> &AshareCode { &self.code }
    pub fn to_call(&self) -> Result<CallQuery> { ValuationQuery::new(self.code.clone()).with_req_id(id("200135")).to_call() }
}
macro_rules! execute_queries { ($($method:ident: $query:ident),* $(,)?) => { impl F10Client { $(
    pub fn $method(&self, query: &$query, options: &OperationOptions) -> Result<Response> {
        self.endpoint(concat!("f10.", stringify!($method)), Instant::now(), || query.to_call(), options)
    }
)* } }; }
execute_queries!(theme_market: ThemeMarketQuery, stock_stage_gain: StockStageGainQuery, stock_main_funds: StockMainFundsQuery,
    stock_industry_rank: StockIndustryRankQuery, stock_industry_pct_rank: StockIndustryPctRankQuery,
    stock_board_member_quotes: StockBoardMemberQuotesQuery, valuation: ValuationQuery,
    stock_valuation_percentile: StockValuationPercentileQuery, stock_pepb_percentile: StockPepbPercentileQuery, stock_history_trend: StockHistoryTrendQuery);
