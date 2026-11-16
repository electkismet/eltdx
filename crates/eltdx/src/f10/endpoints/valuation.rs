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

fn range_payload(code: &AshareCode, request: &str, page: PageSelector, size: u32) -> OrderedJsonObject {
    super::object_templates::native_part(super::ObjectPart::Range, |name| match name {
        "req_id" => j(request), "full_code" => j(format!("{}|{}", code.as_str(), code.market().get())),
        "page" => j(page.wire()), "page_size" => j(format!("{size}")), _ => unreachable!(),
    })
}

fn series_payload(code: &AshareCode, page: PageSelector, size: u32,
    window: &str, indicator: &str, period: &str) -> OrderedJsonObject
{
    super::object_templates::native_part(super::ObjectPart::Series, |name| match name {
        "req_id" => j("200124"), "code" => j(code.as_str()), "market" => j(format!("{}", code.market().get())),
        "page" => page_number(page), "page_size" => j(format!("{size}")),
        "window" => j(window), "indicator" => j(indicator), "period" => j(period), _ => unreachable!(),
    })
}

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
        let mut payload = super::object_templates::native_part(super::ObjectPart::Theme, |name| match name {
            "req_id" => j(self.req_id.as_str()), "market" => j(u64::from(self.code.market().get())),
            "code" => j(self.code.as_str()), "page" => page_number(self.page),
            "page_size" => j(format!("{}", self.page_size.get())), _ => unreachable!(),
        });
        if super::THEME_WINDOW_REQUESTS.contains(&self.req_id.as_str()) {
            payload.extend(super::object_templates::native_part(super::ObjectPart::Window, |_| j(format!("{}", self.zq_num.get()))));
        }
        apply_extra(&mut payload, &self.extra)?;
        object_call(super::object_entry(true), payload)
    }
}

macro_rules! stage_query {
    ($name:ident, $method:literal, $days:literal) => {
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
                ThemeMarketQuery::new(self.code.clone()).with_req_id(id(super::stage_request($method).expect("built-in stage request"))).with_zq_num(self.days)?.with_page_size(self.page_size)?.to_call()
            }
        }
    };
}
stage_query!(StockStageGainQuery, "stock_stage_gain", 60);
stage_query!(StockMainFundsQuery, "stock_main_funds", 30);
stage_query!(StockIndustryRankQuery, "stock_industry_rank", 30);
stage_query!(StockIndustryPctRankQuery, "stock_industry_pct_rank", 30);

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
        let payload = super::object_templates::native_part(super::ObjectPart::Board, |name| match name {
            "market" => j(format!("{}", self.board_market.get())), "code" => j(self.board_code.as_str()),
            "page" => page_number(self.page), "page_size" => j(self.page_size.wire()), _ => unreachable!(),
        });
        object_call(super::object_entry(true), payload)
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
        let mut payload = if super::VALUATION_RANGE_REQUESTS.contains(&request) {
            let page = if request == "200192" && self.page == PageSelector::Number(0) { PageSelector::All } else { self.page };
            let size = if request == "200192" && self.page_size.get() == 20 { 5 } else { self.page_size.get() };
            let mut payload = range_payload(&self.code, request, page, size);
            if request == "200192" { payload.push(("Type".to_owned(), j("0"))); }
            payload
        } else if request == "200124" {
            let page = if self.page == PageSelector::Number(0) { PageSelector::Number(1) } else { self.page };
            if page == PageSelector::All { return Err(crate::f10::values::invalid("PE/PB series requires a numbered page")); }
            let size = if self.page_size.get() == 20 { 300 } else { self.page_size.get() };
            series_payload(&self.code, page, size, "1Y", "0", "0")
        } else {
            super::object_templates::native_part(super::ObjectPart::History, |name| match name {
                "req_id" => j(request), "code" => j(self.code.as_str()),
                "market" => j(format!("{}", self.code.market().get())), _ => unreachable!(),
            })
        };
        payload.extend(super::object_templates::native_part(super::ObjectPart::ValuationModule, |_| unreachable!()));
        apply_extra(&mut payload, &self.extra)?;
        object_call(super::object_entry(false), payload)
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
        let mut payload = range_payload(&self.code, "200192", self.page, self.page_size.get());
        payload.push(("Type".to_owned(), j(self.kind.as_str())));
        payload.extend(super::object_templates::native_part(super::ObjectPart::ValuationModule, |_| unreachable!()));
        object_call(super::object_entry(false), payload)
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
        let mut payload = series_payload(&self.code, PageSelector::Number(self.page), self.page_size.get(),
            &self.window, &self.indicator, &self.period);
        payload.extend(super::object_templates::native_part(super::ObjectPart::ValuationModule, |_| unreachable!()));
        object_call(super::object_entry(false), payload)
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
