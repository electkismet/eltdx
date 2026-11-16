//! Market/board queries and bounded typed collectors.
use std::num::NonZeroU32;
use std::time::Instant;
use crate::types::{PageOptions, ResultMeta, TradingDate};
use crate::operation::OperationContext;
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use crate::f10::{AshareCode, CallQuery, F10Client, F10MarketCode, JsonValue, Response, Row, SectionCode};
use crate::f10::values::{count, j, page, text};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketReportQuery { code: Option<AshareCode>, section: SectionCode, arg: String, date: Option<TradingDate>, page: u32, page_size: NonZeroU32 }
impl MarketReportQuery {
    pub fn new(code: Option<AshareCode>) -> Self { Self { code, section: SectionCode::default_for("market_report", "board_info"), arg: String::new(), date: None, page: 1, page_size: nz(20) } }
    pub fn code(&self) -> Option<&AshareCode> { self.code.as_ref() }
    pub fn section(&self) -> &SectionCode { &self.section }
    pub fn arg(&self) -> &str { &self.arg }
    pub fn date(&self) -> Option<TradingDate> { self.date }
    pub fn page(&self) -> u32 { self.page }
    pub fn page_size(&self) -> NonZeroU32 { self.page_size }
    pub fn with_code(mut self, value: Option<AshareCode>) -> Self { self.code = value; self }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> { value.checked_for("market_report")?; self.section = value; Ok(self) }
    pub fn with_arg(mut self, value: impl Into<String>) -> Result<Self> { let value = value.into(); text(&value, true)?; self.arg = value; Ok(self) }
    /// The source API accepts but does not apply date to market reports.
    pub fn with_date(mut self, value: Option<TradingDate>) -> Self { self.date = value; self }
    pub fn with_page(mut self, value: u32) -> Result<Self> { self.page = page(value)?; Ok(self) }
    pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> { self.page_size = count(value)?; Ok(self) }
    pub fn to_call(&self) -> Result<CallQuery> {
        let section = match self.section.as_str() { "index_members" => "index_flow_members", "index_etfs" => "index_tracking", "ipo_calendar" => "market_new_shares", value => value };
        let code = if matches!(section, "market_new_shares" | "market_capital") { self.code.as_ref().map_or("", AshareCode::as_str) }
            else { self.code.as_ref().ok_or_else(|| crate::f10::values::invalid("market report section requires code"))?.as_str() };
        let p = self.page.to_string(); let n = self.page_size.get().to_string(); let arg = self.arg.as_str();
        let default_arg = |default: &str| if arg.is_empty() { default.to_owned() } else { arg.to_owned() };
        let (entry, params) = match section {
            "board_info" => ("CWServ.skef10_bk_getinfo", vec![j(code)]),
            "board_basics" | "board_description" | "board_trading" => ("CWServ.skef10_bk_cpbd_jczl", vec![j(match section { "board_basics" => "001", "board_description" => "002", _ => "004" }), j(code), j("")]),
            "board_flow" => ("CWServ.skef10_bk_zjlx", vec![j(default_arg("001_1")), j(code), j("")]),
            "board_members" => ("CWServ.skef10_bk_scgl", vec![j(default_arg("001")), j(code), j(p), j(n)]),
            "board_analysis" => ("CWServ.skef10_bk_sdfx", vec![j("001"), j(code), j("")]),
            "board_finance_compare" => ("CWServ.skef10_bk_ggdb_cwdb", vec![j(code)]),
            "board_candidates" => ("CWServ.skef10_bk_cfgsj", vec![j(default_arg("001")), j(code)]),
            "board_trend" => ("CWServ.skef10_bk_ydfx", vec![j(code)]),
            "index_info" => ("CWServ.skef10_zs_getinfo", vec![j(code)]),
            "index_profile" => ("CWServ.skef10_zs_jbxx_zsgk", vec![j(code)]),
            "index_valuation" => ("CWServ.skef10_zs_jbxx_gzxx", vec![j("001"), j(code), j(""), j("")]),
            "index_tracking" => ("CWServ.skef10_zs_jbxx_gzbd", vec![j(code), j(p), j(n)]),
            "index_flow" => ("CWServ.skef10_zs_zjdx", vec![j(default_arg("004_1")), j(code), j(""), j(""), j(p), j(n)]),
            "index_flow_members" => ("CWServ.skef10_zs_cfgsj_cfgjcsj", vec![j(default_arg("001")), j(code), j(p), j(n)]),
            "index_notices" => ("CWServ.skef10_zs_jrts", vec![j(default_arg("001")), j(code), j(p), j(n)]),
            "market_basic" => ("CWServ.skef10_dpzx_jbxx", vec![j("002"), j(code)]),
            "market_margin" => ("CWServ.skef10_dpzx_rzrq", vec![j("001"), j(code)]),
            "market_margin_summary" => ("CWServ.skef10_dpzx_rzrq", vec![j("002"), j(code)]),
            "market_margin_investors" => ("CWServ.skef10_dpzx_rzrq", vec![j("003"), j(default_arg("1m"))]),
            "market_new_shares" => ("CWServ.skef10_dpzx_xgfx", vec![j("002")]),
            "market_capital" => ("CWServ.skef10_dpzx_scgl", vec![j(default_arg("004"))]),
            _ => unreachable!("closed market report registry"),
        };
        super::params(entry, params)
    }
}
fn nz(value: u32) -> NonZeroU32 { NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionCompletion { EndOfData, RequestedPage }
#[derive(Clone, Debug, Eq, PartialEq)]
struct CollectionQuery { code: AshareCode, all_pages: bool, page_size: NonZeroU32, max_pages: NonZeroU32, limits: PageOptions }
macro_rules! collection_query {
    ($name:ident, $maximum:literal, $records:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name { inner: CollectionQuery }
        impl $name {
            pub fn new(code: AshareCode) -> Self { Self { inner: CollectionQuery { code, all_pages: true, page_size: nz(100), max_pages: nz($maximum), limits: PageOptions::new(100, $maximum, $records).expect("positive built-in collection bounds") } } }
            pub fn code(&self) -> &AshareCode { &self.inner.code }
            pub fn all_pages(&self) -> bool { self.inner.all_pages }
            pub fn page_size(&self) -> NonZeroU32 { self.inner.page_size }
            pub fn max_pages(&self) -> NonZeroU32 { self.inner.max_pages }
            pub fn page_options(&self) -> &PageOptions { &self.inner.limits }
            pub fn with_all_pages(mut self, value: bool) -> Self { self.inner.all_pages = value; self }
            pub fn with_page_size(mut self, value: NonZeroU32) -> Result<Self> {
                if value.get() > 100 { return Err(crate::f10::values::invalid("F10 collection page size must be at most 100")); }
                self.inner.page_size = value; Ok(self)
            }
            pub fn with_max_pages(mut self, value: NonZeroU32) -> Result<Self> { self.inner.max_pages = count(value)?; Ok(self) }
            pub fn with_page_options(mut self, value: PageOptions) -> Self { self.inner.limits = value; self }
        }
    };
}
collection_query!(IndexMembersQuery, 100, 10_000);
collection_query!(IndexTrackingProductsQuery, 100, 10_000);
collection_query!(BoardMembersQuery, 50, 5_000);

pub(crate) fn value<'a>(row: &'a Row, name: &str) -> Option<&'a JsonValue> {
    row.cells().iter().find(|cell| cell.name() == name).map(|cell| cell.value())
}
pub(crate) fn string<'a>(row: &'a Row, name: &str) -> Option<&'a str> {
    value(row, name).and_then(JsonValue::as_str).map(str::trim).filter(|value| !value.is_empty())
}
pub(crate) fn number(row: &Row, name: &str) -> Option<f64> {
    match value(row, name)? { JsonValue::Number(value) => value.as_f64(), JsonValue::String(value) => value.trim().parse::<f64>().ok().filter(|value| value.is_finite()), _ => None }
}
pub(crate) fn integer(row: &Row, name: &str) -> Option<i64> {
    match value(row, name)? { JsonValue::Number(value) => value.as_i64(), JsonValue::String(value) => value.trim().parse().ok(), _ => None }
}
fn market(row: &Row, name: &str) -> Option<F10MarketCode> { integer(row, name).and_then(|value| u16::try_from(value).ok()).map(F10MarketCode::new) }
fn inferred_identity(row: &Row, code: &str, market_field: &str) -> Option<AshareCode> {
    let code = string(row, code)?;
    if let Some(market) = market(row, market_field).filter(|market| market.get() <= 2) { return AshareCode::new(market, code).ok(); }
    code.parse::<crate::types::SecurityId>().ok().map(AshareCode::from)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexMember { raw: Row }
impl IndexMember {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N001") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "N004") }
    pub fn join_date(&self) -> Option<&JsonValue> { value(&self.raw, "N003") }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "N004") }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexTrackingProduct { raw: Row }
impl IndexTrackingProduct {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N001") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "sc") }
    pub fn scale_yuan(&self) -> Option<f64> { number(&self.raw, "N003") }
    pub fn change_pct(&self) -> Option<f64> { number(&self.raw, "N005") }
    pub fn amount_yuan(&self) -> Option<f64> { number(&self.raw, "N006") }
    pub fn inception_date(&self) -> Option<&JsonValue> { value(&self.raw, "N008") }
    pub fn manager(&self) -> Option<&str> { string(&self.raw, "N009") }
    pub fn company(&self) -> Option<&str> { string(&self.raw, "N010") }
    pub fn nav(&self) -> Option<f64> { number(&self.raw, "N011") }
    pub fn fee_rate_pct(&self) -> Option<f64> { number(&self.raw, "N012") }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "sc") }
    /// Prefix heuristic only; does not establish the security's classification.
    pub fn matches_etf_prefix(&self) -> bool { self.code().is_some_and(|code| ["51", "56", "58", "15", "16"].iter().any(|prefix| code.starts_with(prefix))) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardMember { raw: Row }
impl BoardMember {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N001") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "sc") }
    pub fn dividend_plan(&self) -> Option<&str> { string(&self.raw, "N003") }
    pub fn progress(&self) -> Option<&str> { string(&self.raw, "N004") }
    pub fn announce_date(&self) -> Option<&JsonValue> { value(&self.raw, "N005") }
    pub fn record_date(&self) -> Option<&JsonValue> { value(&self.raw, "N006") }
    pub fn ex_date(&self) -> Option<&JsonValue> { value(&self.raw, "N007") }
    pub fn report_period(&self) -> Option<&JsonValue> { value(&self.raw, "N008") }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "sc") }
}
macro_rules! result_table {
    ($name:ident, $row:ident, $identity:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name { code: AshareCode, total: Option<u64>, rows: Vec<$row>, completion: CollectionCompletion, meta: ResultMeta }
        impl $name {
            pub fn $identity(&self) -> &AshareCode { &self.code }
            pub fn total(&self) -> Option<u64> { self.total }
            pub fn rows(&self) -> &[$row] { &self.rows }
            pub fn completion(&self) -> CollectionCompletion { self.completion }
            pub fn meta(&self) -> &ResultMeta { &self.meta }
        }
    };
}
result_table!(IndexMemberTable, IndexMember, index_code);
result_table!(IndexTrackingTable, IndexTrackingProduct, index_code);
result_table!(BoardMemberTable, BoardMember, board_code);
impl BoardMemberTable {
    pub fn stocks(&self) -> Vec<&BoardMember> {
        let mut result: Vec<&BoardMember> = Vec::new();
        for row in &self.rows { if row.code().is_some() && !result.iter().any(|item| item.code() == row.code()) { result.push(row); } }
        result
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpoEntry { raw: Row }
impl IpoEntry {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn apply_date(&self) -> Option<&JsonValue> { value(&self.raw, "N001") }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N003") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "sc") }
    pub fn issue_pe(&self) -> Option<f64> { number(&self.raw, "N004") }
    pub fn extra_pct(&self) -> Option<f64> { number(&self.raw, "N005") }
    pub fn issue_price(&self) -> Option<f64> { number(&self.raw, "N006") }
    pub fn apply_limit(&self) -> Option<f64> { number(&self.raw, "N007") }
    pub fn raise_amount(&self) -> Option<f64> { number(&self.raw, "N008") }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpoCalendar { rows: Vec<IpoEntry>, meta: ResultMeta }
impl IpoCalendar { pub fn rows(&self) -> &[IpoEntry] { &self.rows } pub fn meta(&self) -> &ResultMeta { &self.meta } }

fn total(response: &Response, context: &OperationContext) -> Result<Option<u64>> {
    for cell in response.result_sets().iter().skip(1).flat_map(|table| table.rows()).flat_map(|row| row.cells()) {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let value = match cell.value() { JsonValue::Number(value) => value.as_u64(), JsonValue::String(value) => value.trim().parse().ok(), _ => None };
        if value.is_some() { return Ok(value); }
    }
    Ok(None)
}
fn record_key(row: &Row, context: &OperationContext) -> Result<Vec<u8>> {
    let fields = ["N001", "N003", "N005", "N006", "N007", "N008"];
    let mut values = Vec::new();
    for name in fields {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let cell = if matches!(name, "N001" | "N003") {
            // Match the public text projection used in the record identity.
            match value(row, name) {
                Some(JsonValue::String(value)) if !value.trim().is_empty() => {
                    let mut output = Vec::new();
                    for chunk in value.trim().as_bytes().chunks(4096) {
                        context.check_boundary(TimeoutPhase::Aggregate)?;
                        output.try_reserve(chunk.len()).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 dividend key").with_source(error))?;
                        output.extend_from_slice(chunk);
                    }
                    JsonValue::String(String::from_utf8(output).map_err(|error| Error::new(ErrorKind::Internal, "invalid copied F10 dividend key").with_source(error))?)
                }
                _ => JsonValue::Null,
            }
        } else {
            match value(row, name) {
                Some(value) => crate::f10::json::clone_value(value, &mut || context.check_boundary(TimeoutPhase::Aggregate))?,
                None => JsonValue::Null,
            }
        };
        values.push(cell);
    }
    let key = crate::f10::json::encode(&JsonValue::Array(values), 16 * 1024 * 1024,
        &mut || context.check_boundary(TimeoutPhase::Aggregate))?;
    context.record_work(0, key.len())?;
    Ok(key)
}
struct Collected<T> { total: Option<u64>, rows: Vec<T>, completion: CollectionCompletion, meta: ResultMeta }
impl F10Client {
    pub fn market_report(&self, query: &MarketReportQuery, options: &OperationOptions) -> Result<Response> {
        self.endpoint("f10.market_report", Instant::now(), || query.to_call(), options)
    }
    fn collect_market<T>(&self, query: &CollectionQuery, section: &'static str, name: &'static str, options: &OperationOptions, project: impl Fn(Row) -> T) -> Result<Collected<T>> {
        let context = self.context(Instant::now(), name, options)?;
        let size = (query.page_size.get() as usize).min(query.limits.page_size());
        let maximum = (query.max_pages.get() as usize).min(query.limits.max_pages());
        let mut rows = Vec::new(); let mut metadata = super::super::response::MetaCollector::default();
        let mut reported_total = None;
        let mut keys = std::collections::HashSet::new();
        for index in 1..=maximum {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let call = MarketReportQuery::new(Some(query.code.clone())).with_section(SectionCode::new("market_report", section)?)?
                .with_page(u32::try_from(index).map_err(|_| crate::f10::values::invalid("collection page overflow"))?)?
                .with_page_size(nz(u32::try_from(size).map_err(|_| crate::f10::values::invalid("collection page size overflow"))?))?.to_call()?;
            let mut response = self.endpoint_call(&call, &context)?;
            if !response.ok() { return Err(Error::new(ErrorKind::MissingData, "F10 collection has a server business error").with_context("server_error_code", response.error_code().unwrap_or_default().to_string())); }
            if reported_total.is_none() { reported_total = total(&response, &context)?; }
            metadata.add(response.meta(), &context)?;
            let batch = response.take_first_rows(); let wire_count = batch.len();
            context.record_work(wire_count, 0)?;
            let before = rows.len();
            for row in batch {
                context.check_boundary(TimeoutPhase::Aggregate)?;
                if section == "board_members" {
                    keys.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 dividend identities").with_source(error))?;
                    if !keys.insert(record_key(&row, &context)?) { continue; }
                }
                if rows.len() >= query.limits.max_records() { return Err(Error::new(ErrorKind::LimitExceeded, "F10 collection record limit reached")); }
                rows.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 collection").with_source(error))?;
                rows.push(project(row));
            }
            let ended = wire_count < size || reported_total.is_some_and(|total| rows.len() as u64 >= total);
            if ended || !query.all_pages {
                context.finish()?;
                return Ok(Collected { total: reported_total, rows, completion: if ended { CollectionCompletion::EndOfData } else { CollectionCompletion::RequestedPage }, meta: metadata.finish() });
            }
            if section == "board_members" && rows.len() == before {
                return Err(Error::new(ErrorKind::MissingData, "F10 board pagination made no new-record progress")
                    .with_context("completion", "incomplete").with_context("page", index.to_string()));
            }
        }
        Err(Error::new(ErrorKind::LimitExceeded, "F10 collection page limit reached before end of data"))
    }
    pub fn index_members(&self, query: &IndexMembersQuery, options: &OperationOptions) -> Result<IndexMemberTable> {
        let result = self.collect_market(&query.inner, "index_members", "f10.index_members", options, |raw| IndexMember { raw })?;
        Ok(IndexMemberTable { code: query.inner.code.clone(), total: result.total, rows: result.rows, completion: result.completion, meta: result.meta })
    }
    pub fn index_tracking_products(&self, query: &IndexTrackingProductsQuery, options: &OperationOptions) -> Result<IndexTrackingTable> {
        let result = self.collect_market(&query.inner, "index_etfs", "f10.index_tracking_products", options, |raw| IndexTrackingProduct { raw })?;
        Ok(IndexTrackingTable { code: query.inner.code.clone(), total: result.total, rows: result.rows, completion: result.completion, meta: result.meta })
    }
    pub fn board_members(&self, query: &BoardMembersQuery, options: &OperationOptions) -> Result<BoardMemberTable> {
        let result = self.collect_market(&query.inner, "board_members", "f10.board_members", options, |raw| BoardMember { raw })?;
        Ok(BoardMemberTable { code: query.inner.code.clone(), total: result.total, rows: result.rows, completion: result.completion, meta: result.meta })
    }
    pub fn ipo_calendar(&self, options: &OperationOptions) -> Result<IpoCalendar> {
        let context = self.context(Instant::now(), "f10.ipo_calendar", options)?;
        let call = MarketReportQuery::new(None).with_section(SectionCode::new("market_report", "ipo_calendar")?)?.to_call()?;
        let mut response = self.endpoint_call(&call, &context)?;
        let mut rows = Vec::new();
        for raw in response.take_first_rows() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            context.record_work(1, 0)?;
            rows.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 IPO entries").with_source(error))?;
            rows.push(IpoEntry { raw });
        }
        let mut metadata = crate::f10::response::MetaCollector::default();
        metadata.add(response.meta(), &context)?;
        context.finish()?;
        Ok(IpoCalendar { rows, meta: metadata.finish() })
    }
}
