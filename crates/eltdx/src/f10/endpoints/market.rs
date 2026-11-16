//! Market/board queries and bounded typed collectors.
use std::num::NonZeroU32;
use std::time::Instant;
use crate::types::{Diagnostic, EnrichmentStatus, PageOptions, ResultMeta, TradingDate};
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
        let section = super::market_section(self.section.as_str());
        let code = if matches!(section, "market_new_shares" | "market_capital") { self.code.as_ref().map_or("", AshareCode::as_str) }
            else { self.code.as_ref().ok_or_else(|| crate::f10::values::invalid("market report section requires code"))?.as_str() };
        super::report_templates::native_report("market_report", code, section, |name| Ok(match name {
            "arg" => j(self.arg.as_str()), "page" => j(format!("{}", self.page)),
            "page_size" => j(format!("{}", self.page_size.get())), _ => unreachable!("built-in market input"),
        }))
    }
}
fn nz(value: u32) -> NonZeroU32 { NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionCompletion {
    EndOfData,
    RequestedPage,
    /// A page reported a nonzero business code. Its data is retained, but
    /// neither that page nor earlier pages establish a complete collection.
    BusinessError,
}
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

/// Evidence attached to the typed interpretation, independent of whether
/// an individual row contains a value. SourceMapping cites the repository's
/// documented response schema, not an independent live-server verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketFieldEvidence { SourceMapping, Inferred, Unknown }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketFieldUnit { Yuan, PercentagePoints, NetAssetValuePerUnit, Unknown }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarketFieldInfo { unit: MarketFieldUnit, evidence: MarketFieldEvidence }
impl MarketFieldInfo {
    pub const fn unit(self) -> MarketFieldUnit { self.unit }
    pub const fn evidence(self) -> MarketFieldEvidence { self.evidence }
    const fn new(unit: MarketFieldUnit, evidence: MarketFieldEvidence) -> Self { Self { unit, evidence } }
    const fn unknown() -> Self { Self::new(MarketFieldUnit::Unknown, MarketFieldEvidence::Unknown) }
    const fn mapped() -> Self { Self::new(MarketFieldUnit::Unknown, MarketFieldEvidence::SourceMapping) }
}

fn full_code(identity: Option<AshareCode>) -> Option<String> {
    let identity = identity?;
    let prefix = match identity.market().get() { 0 => "sz", 1 => "sh", 2 => "bj", _ => return None };
    Some(format!("{prefix}{}", identity.as_str()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexMember { raw: Row, join_date: Option<TradingDate> }
impl IndexMember {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N001") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "N004") }
    pub fn join_date(&self) -> Option<&JsonValue> { value(&self.raw, "N003") }
    pub fn parsed_join_date(&self) -> Option<TradingDate> { self.join_date }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "N004") }
    pub fn full_code(&self) -> Option<String> { full_code(self.inferred_identity()) }
    pub fn field_info(&self, name: &str) -> MarketFieldInfo {
        match name { "N001" | "N002" | "N003" | "N004" => MarketFieldInfo::mapped(), _ => MarketFieldInfo::unknown() }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexTrackingProduct { raw: Row, inception_date: Option<TradingDate> }
impl IndexTrackingProduct {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N001") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "sc") }
    pub fn scale_yuan(&self) -> Option<f64> { number(&self.raw, "N003") }
    pub fn change_pct(&self) -> Option<f64> { number(&self.raw, "N005") }
    pub fn amount_yuan(&self) -> Option<f64> { number(&self.raw, "N006") }
    pub fn inception_date(&self) -> Option<&JsonValue> { value(&self.raw, "N008") }
    pub fn parsed_inception_date(&self) -> Option<TradingDate> { self.inception_date }
    pub fn manager(&self) -> Option<&str> { string(&self.raw, "N009") }
    pub fn company(&self) -> Option<&str> { string(&self.raw, "N010") }
    pub fn nav(&self) -> Option<f64> { number(&self.raw, "N011") }
    pub fn fee_rate_pct(&self) -> Option<f64> { number(&self.raw, "N012") }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "sc") }
    pub fn full_code(&self) -> Option<String> { full_code(self.inferred_identity()) }
    pub fn field_info(&self, name: &str) -> MarketFieldInfo {
        use MarketFieldEvidence::SourceMapping;
        use MarketFieldUnit::*;
        match name {
            "N003" | "N006" => MarketFieldInfo::new(Yuan, SourceMapping),
            "N005" | "N012" => MarketFieldInfo::new(PercentagePoints, SourceMapping),
            "N011" => MarketFieldInfo::new(NetAssetValuePerUnit, SourceMapping),
            "N001" | "N002" | "N008" | "N009" | "N010" | "sc" => MarketFieldInfo::mapped(),
            _ => MarketFieldInfo::unknown(),
        }
    }
    /// Prefix heuristic only; does not establish the security's classification.
    pub fn matches_etf_prefix(&self) -> bool { self.code().is_some_and(|code| ["51", "56", "58", "15", "16"].iter().any(|prefix| code.starts_with(prefix))) }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardMember {
    raw: Row, announce_date: Option<TradingDate>, record_date: Option<TradingDate>,
    ex_date: Option<TradingDate>, report_period: Option<TradingDate>,
}
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
    /// These three semantic names are inferred from dividend workflow. The
    /// parsed values validate only calendar syntax, not those interpretations.
    pub fn parsed_announce_date(&self) -> Option<TradingDate> { self.announce_date }
    pub fn parsed_record_date(&self) -> Option<TradingDate> { self.record_date }
    pub fn parsed_ex_date(&self) -> Option<TradingDate> { self.ex_date }
    pub fn parsed_report_period(&self) -> Option<TradingDate> { self.report_period }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N001", "sc") }
    pub fn full_code(&self) -> Option<String> { full_code(self.inferred_identity()) }
    pub fn field_info(&self, name: &str) -> MarketFieldInfo {
        match name {
            "N005" | "N006" | "N007" => MarketFieldInfo::new(MarketFieldUnit::Unknown, MarketFieldEvidence::Inferred),
            "N001" | "N002" | "N003" | "N004" | "N008" | "sc" => MarketFieldInfo::mapped(),
            _ => MarketFieldInfo::unknown(),
        }
    }
}
macro_rules! result_table {
    ($name:ident, $row:ident, $identity:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            code: AshareCode, total: Option<u64>, rows: Vec<$row>, completion: CollectionCompletion,
            responses: Vec<Response>, meta: ResultMeta,
        }
        impl $name {
            pub fn $identity(&self) -> &AshareCode { &self.code }
            pub fn total(&self) -> Option<u64> { self.total }
            pub fn rows(&self) -> &[$row] { &self.rows }
            pub fn count(&self) -> usize { self.rows.len() }
            pub fn completion(&self) -> CollectionCompletion { self.completion }
            pub fn ok(&self) -> bool { self.responses.iter().all(Response::ok) }
            pub fn error_code(&self) -> Option<i64> { self.responses.iter().find(|response| !response.ok()).and_then(Response::error_code) }
            /// Physical response order, with first-table rows moved to rows().
            /// Columns, extra tables, attributes and page status remain intact.
            pub fn page_responses(&self) -> &[Response] { &self.responses }
            pub fn meta(&self) -> &ResultMeta { &self.meta }
        }
    };
}
result_table!(IndexMemberTable, IndexMember, index_code);
result_table!(IndexTrackingTable, IndexTrackingProduct, index_code);
result_table!(BoardMemberTable, BoardMember, board_code);
impl IndexMemberTable {
    pub fn codes(&self) -> Vec<&str> { self.rows.iter().filter_map(IndexMember::code).collect() }
    pub fn full_codes(&self) -> Vec<String> { self.rows.iter().filter_map(IndexMember::full_code).collect() }
}
impl IndexTrackingTable {
    /// Prefix heuristic only, not a confirmed instrument classification.
    pub fn etfs(&self) -> Vec<&IndexTrackingProduct> { self.rows.iter().filter(|row| row.matches_etf_prefix()).collect() }
}
impl BoardMemberTable {
    pub fn stocks(&self) -> Vec<&BoardMember> {
        let mut result: Vec<&BoardMember> = Vec::new();
        for row in &self.rows { if row.code().is_some() && !result.iter().any(|item| item.code() == row.code()) { result.push(row); } }
        result
    }
    pub fn codes(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        self.rows.iter().filter_map(BoardMember::code).filter(|code| seen.insert(*code)).collect()
    }
    pub fn full_codes(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.rows.iter().filter_map(BoardMember::full_code).filter(|code| seen.insert(code.clone())).collect()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpoEntry { raw: Row, apply_date: Option<TradingDate> }
impl IpoEntry {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn apply_date(&self) -> Option<&JsonValue> { value(&self.raw, "N001") }
    pub fn parsed_apply_date(&self) -> Option<TradingDate> { self.apply_date }
    pub fn code(&self) -> Option<&str> { string(&self.raw, "N002") }
    pub fn name(&self) -> Option<&str> { string(&self.raw, "N003") }
    pub fn market(&self) -> Option<F10MarketCode> { market(&self.raw, "sc") }
    pub fn issue_pe(&self) -> Option<f64> { number(&self.raw, "N004") }
    pub fn extra_pct(&self) -> Option<f64> { number(&self.raw, "N005") }
    pub fn issue_price(&self) -> Option<f64> { number(&self.raw, "N006") }
    pub fn apply_limit(&self) -> Option<f64> { number(&self.raw, "N007") }
    pub fn raise_amount(&self) -> Option<f64> { number(&self.raw, "N008") }
    pub fn raise_amount_yi(&self) -> Option<f64> { self.raise_amount().map(|value| value / 1e8) }
    pub fn inferred_identity(&self) -> Option<AshareCode> { inferred_identity(&self.raw, "N002", "sc") }
    pub fn full_code(&self) -> Option<String> { full_code(self.inferred_identity()) }
    /// N005 retains its old `extra_pct` getter, but its meaning and unit are
    /// unknown. N007's suspected ten-thousand-share scale is not asserted.
    pub fn field_info(&self, name: &str) -> MarketFieldInfo {
        match name {
            "N006" | "N008" => MarketFieldInfo::new(MarketFieldUnit::Yuan, MarketFieldEvidence::SourceMapping),
            "N004" | "N007" => MarketFieldInfo::new(MarketFieldUnit::Unknown, MarketFieldEvidence::Inferred),
            "N001" | "N002" | "N003" | "sc" => MarketFieldInfo::mapped(),
            _ => MarketFieldInfo::unknown(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpoCalendar { rows: Vec<IpoEntry>, response: Response }
impl IpoCalendar {
    pub fn rows(&self) -> &[IpoEntry] { &self.rows }
    pub fn count(&self) -> usize { self.rows.len() }
    pub fn full_codes(&self) -> Vec<String> { self.rows.iter().filter_map(IpoEntry::full_code).collect() }
    pub fn ok(&self) -> bool { self.response.ok() }
    pub fn error_code(&self) -> Option<i64> { self.response.error_code() }
    pub fn meta(&self) -> &ResultMeta { self.response.meta() }
    /// First-table rows have moved to rows(); all other data remains here.
    pub fn response(&self) -> &Response { &self.response }
}

fn total(response: &mut Response, context: &OperationContext) -> Result<Option<u64>> {
    let mut selected = None;
    let mut invalid = false;
    let mut conflict = false;
    // The schema names the total `zs`. An unrelated numeric column must not
    // truncate collection. Python's historical scan is retained by its adapter.
    for cell in response.result_sets().iter().skip(1).flat_map(|table| table.rows()).flat_map(|row| row.cells()) {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if cell.name() != "zs" || !present(cell.value()) { continue; }
        let value = match cell.value() { JsonValue::Number(value) => value.as_u64(), JsonValue::String(value) => value.trim().parse::<u64>().ok(), _ => None };
        if let Some(value) = value {
            if selected.is_some_and(|selected| selected != value) { conflict = true; }
            selected = Some(value);
        } else { invalid = true; }
    }
    if invalid { response.push_diagnostic(projection_diagnostic("zs", "invalid nonnegative total retained as raw")); }
    if conflict { response.push_diagnostic(projection_diagnostic("zs", "conflicting totals ignored; all values retained as raw")); }
    Ok(if conflict { None } else { selected })
}
fn present(value: &JsonValue) -> bool {
    !matches!(value, JsonValue::Null) && !matches!(value, JsonValue::String(value) if value.trim().is_empty())
}
fn projection_diagnostic(field: &str, reason: &str) -> Diagnostic {
    Diagnostic::new("f10.market", "projection", EnrichmentStatus::Unavailable,
        None, Some(format!("{field}: {reason}")))
}
fn date_value(value: &JsonValue) -> Option<TradingDate> {
    let text = match value { JsonValue::String(value) => value.trim(), JsonValue::Number(value) => value.lexeme(), _ => return None };
    let bytes = text.as_bytes();
    let mut digits = [0_u8; 8];
    match bytes.len() {
        8 => digits.copy_from_slice(bytes),
        10 if matches!((bytes[4], bytes[7]), (b'-', b'-') | (b'/', b'/')) => {
            digits[..4].copy_from_slice(&bytes[..4]);
            digits[4..6].copy_from_slice(&bytes[5..7]);
            digits[6..].copy_from_slice(&bytes[8..]);
        }
        _ => return None,
    }
    if !digits.iter().all(u8::is_ascii_digit) { return None; }
    let year = digits[..4].iter().fold(0_u16, |value, digit| value * 10 + u16::from(digit - b'0'));
    let month = (digits[4] - b'0') * 10 + digits[5] - b'0';
    let day = (digits[6] - b'0') * 10 + digits[7] - b'0';
    TradingDate::new(year, month, day).ok()
}
fn project_date(row: &Row, field: &str, response: &mut Response, context: &OperationContext) -> Result<Option<TradingDate>> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    let Some(value) = value(row, field).filter(|value| present(value)) else { return Ok(None); };
    let date = date_value(value);
    if date.is_none() { response.push_diagnostic(projection_diagnostic(field, "invalid Gregorian date retained as raw")); }
    Ok(date)
}
fn check_numbers(row: &Row, fields: &[&str], response: &mut Response, context: &OperationContext) -> Result<()> {
    for field in fields {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if value(row, field).is_some_and(present) && number(row, field).is_none() {
            response.push_diagnostic(projection_diagnostic(field, "invalid finite number retained as raw"));
        }
    }
    Ok(())
}
fn check_market(row: &Row, field: &str, response: &mut Response, context: &OperationContext) -> Result<()> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    if value(row, field).is_some_and(present) && market(row, field).is_none() {
        response.push_diagnostic(projection_diagnostic(field, "invalid market number retained as raw"));
    }
    Ok(())
}
fn index_member(raw: Row, response: &mut Response, context: &OperationContext) -> Result<IndexMember> {
    let join_date = project_date(&raw, "N003", response, context)?;
    check_market(&raw, "N004", response, context)?;
    Ok(IndexMember { raw, join_date })
}
fn tracking_product(raw: Row, response: &mut Response, context: &OperationContext) -> Result<IndexTrackingProduct> {
    let inception_date = project_date(&raw, "N008", response, context)?;
    check_numbers(&raw, &["N003", "N005", "N006", "N011", "N012"], response, context)?;
    check_market(&raw, "sc", response, context)?;
    Ok(IndexTrackingProduct { raw, inception_date })
}
fn board_member(raw: Row, response: &mut Response, context: &OperationContext) -> Result<BoardMember> {
    let announce_date = project_date(&raw, "N005", response, context)?;
    let record_date = project_date(&raw, "N006", response, context)?;
    let ex_date = project_date(&raw, "N007", response, context)?;
    let report_period = project_date(&raw, "N008", response, context)?;
    check_market(&raw, "sc", response, context)?;
    Ok(BoardMember { raw, announce_date, record_date, ex_date, report_period })
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
struct Collected<T> {
    code: AshareCode, total: Option<u64>, rows: Vec<T>, completion: CollectionCompletion,
    responses: Vec<Response>, meta: ResultMeta,
}
impl F10Client {
    pub fn market_report(&self, query: &MarketReportQuery, options: &OperationOptions) -> Result<Response> {
        self.endpoint("f10.market_report", Instant::now(), || query.to_call(), options)
    }
    fn collect_market<T>(&self, query: &CollectionQuery, section: &'static str, name: &'static str,
        options: &OperationOptions, project: impl Fn(Row, &mut Response, &OperationContext) -> Result<T>) -> Result<Collected<T>> {
        let context = self.context(Instant::now(), name, options)?;
        let size = (query.page_size.get() as usize).min(query.limits.page_size());
        let maximum = (query.max_pages.get() as usize).min(query.limits.max_pages());
        let mut rows = Vec::new(); let mut responses = Vec::new();
        let mut metadata = super::super::response::MetaCollector::default();
        let mut reported_total = None;
        let mut keys = std::collections::HashSet::new();
        for index in 1..=maximum {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let call = MarketReportQuery::new(Some(query.code.clone())).with_section(SectionCode::new("market_report", section)?)?
                .with_page(u32::try_from(index).map_err(|_| crate::f10::values::invalid("collection page overflow"))?)?
                .with_page_size(nz(u32::try_from(size).map_err(|_| crate::f10::values::invalid("collection page size overflow"))?))?.to_call()?;
            let mut response = self.endpoint_call(&call, &context)?;
            let business_error = !response.ok();
            if reported_total.is_none() { reported_total = total(&mut response, &context)?; }
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
                rows.push(project(row, &mut response, &context)?);
            }
            // Add diagnostics after projection, and retain the remaining
            // response tree even when raw HTTP retention is disabled.
            metadata.add(response.meta(), &context)?;
            responses.try_reserve(1).map_err(|error| Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 page responses").with_source(error))?;
            responses.push(response);
            let ended = wire_count < size || reported_total.is_some_and(|total| rows.len() as u64 >= total);
            if business_error || ended || !query.all_pages {
                let completion = if business_error { CollectionCompletion::BusinessError }
                    else if ended { CollectionCompletion::EndOfData } else { CollectionCompletion::RequestedPage };
                let result = Collected { code: query.code.clone(), total: reported_total, rows, completion,
                    responses, meta: metadata.finish() };
                context.finish()?;
                return Ok(result);
            }
            if section == "board_members" && rows.len() == before {
                return Err(Error::new(ErrorKind::MissingData, "F10 board pagination made no new-record progress")
                    .with_context("completion", "incomplete").with_context("page", index.to_string()));
            }
        }
        Err(Error::new(ErrorKind::LimitExceeded, "F10 collection page limit reached before end of data"))
    }
    pub fn index_members(&self, query: &IndexMembersQuery, options: &OperationOptions) -> Result<IndexMemberTable> {
        let result = self.collect_market(&query.inner, "index_members", "f10.index_members", options, index_member)?;
        Ok(IndexMemberTable { code: result.code, total: result.total, rows: result.rows, completion: result.completion,
            responses: result.responses, meta: result.meta })
    }
    pub fn index_tracking_products(&self, query: &IndexTrackingProductsQuery, options: &OperationOptions) -> Result<IndexTrackingTable> {
        let result = self.collect_market(&query.inner, "index_etfs", "f10.index_tracking_products", options, tracking_product)?;
        Ok(IndexTrackingTable { code: result.code, total: result.total, rows: result.rows, completion: result.completion,
            responses: result.responses, meta: result.meta })
    }
    pub fn board_members(&self, query: &BoardMembersQuery, options: &OperationOptions) -> Result<BoardMemberTable> {
        let result = self.collect_market(&query.inner, "board_members", "f10.board_members", options, board_member)?;
        Ok(BoardMemberTable { code: result.code, total: result.total, rows: result.rows, completion: result.completion,
            responses: result.responses, meta: result.meta })
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
            let apply_date = project_date(&raw, "N001", &mut response, &context)?;
            check_numbers(&raw, &["N004", "N005", "N006", "N007", "N008"], &mut response, &context)?;
            check_market(&raw, "sc", &mut response, &context)?;
            rows.push(IpoEntry { raw, apply_date });
        }
        let result = IpoCalendar { rows, response };
        context.finish()?;
        Ok(result)
    }
}
