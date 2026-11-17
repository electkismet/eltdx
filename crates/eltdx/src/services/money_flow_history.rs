//! Historical daily fund flow from the independent F10 HTTP owner.
//!
//! This is not HQ's 0x0ffc daily response or the experimental category-22
//! K-line format. Only the F10 feature makes this service available.
use std::num::NonZeroU32;
use std::time::Instant;

use crate::f10::{
    AshareCode, CallQuery, EntryName, F10Client, JsonValue, RequestPayload, Response, Row,
};
use crate::operation::OperationContext;
use crate::types::{AmountUnit, Diagnostic, EnrichmentStatus, ResultMeta};
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase, TradingDate};

use crate::f10::collection::{
    history_parameters, reverse_then_window, MONEY_FLOW_HISTORY_ENTRY, MONEY_FLOW_HISTORY_SELECTOR,
};

/// The five history windows accepted by the F10 service.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum HistoryDays {
    Three,
    Five,
    Ten,
    Twenty,
    #[default]
    Sixty,
}
impl HistoryDays {
    pub const fn get(self) -> u32 {
        match self {
            Self::Three => 3,
            Self::Five => 5,
            Self::Ten => 10,
            Self::Twenty => 20,
            Self::Sixty => 60,
        }
    }
}
impl TryFrom<u32> for HistoryDays {
    type Error = Error;
    fn try_from(value: u32) -> Result<Self> {
        match value {
            3 => Ok(Self::Three),
            5 => Ok(Self::Five),
            10 => Ok(Self::Ten),
            20 => Ok(Self::Twenty),
            60 => Ok(Self::Sixty),
            _ => Err(Error::new(
                ErrorKind::InvalidArgument,
                "days must be one of: 3, 5, 10, 20, 60",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoneyFlowHistoryQuery {
    code: AshareCode,
    days: HistoryDays,
    start: usize,
    count: Option<NonZeroU32>,
}
impl MoneyFlowHistoryQuery {
    pub fn new(code: AshareCode) -> Self {
        Self {
            code,
            days: HistoryDays::default(),
            start: 0,
            count: None,
        }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn days(&self) -> HistoryDays {
        self.days
    }
    /// Offset in the reversed response, normally counting from the latest day.
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn count(&self) -> Option<NonZeroU32> {
        self.count
    }
    pub fn with_days(mut self, value: u32) -> Result<Self> {
        self.days = value.try_into()?;
        Ok(self)
    }
    pub fn with_start(mut self, value: usize) -> Self {
        self.start = value;
        self
    }
    pub fn with_count(mut self, value: Option<NonZeroU32>) -> Result<Self> {
        if value.is_some_and(|value| value.get() > 800) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "history count must be between 1 and 800",
            ));
        }
        self.count = value;
        Ok(self)
    }
    pub fn to_call(&self) -> Result<CallQuery> {
        Ok(CallQuery::new(
            EntryName::new(MONEY_FLOW_HISTORY_ENTRY)?,
            RequestPayload::Params(
                history_parameters(
                    MONEY_FLOW_HISTORY_SELECTOR.into(),
                    self.code.as_str().into(),
                    format!("{}", self.days.get()).into(),
                    self.code.as_str().into(),
                )
                .into(),
            ),
        ))
    }
}

/// A projected row retains all original cells, including unknown/duplicate
/// names and numeric lexemes. Numeric getters are explicit finite-f64 views.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundFlowHistoryRow {
    raw: Row,
    trade_date: Option<TradingDate>,
    listing_date: Option<TradingDate>,
}
impl FundFlowHistoryRow {
    pub fn raw(&self) -> &Row {
        &self.raw
    }
    pub fn trade_date(&self) -> Option<TradingDate> {
        self.trade_date
    }
    pub fn listing_date(&self) -> Option<TradingDate> {
        self.listing_date
    }
    pub fn code(&self) -> Option<&str> {
        field(&self.raw, "N002").and_then(JsonValue::as_str)
    }
    pub fn name(&self) -> Option<&str> {
        field(&self.raw, "N003").and_then(JsonValue::as_str)
    }
    pub fn main_net_amount(&self) -> Option<f64> {
        number(&self.raw, "N004")
    }
    pub fn extra_large_net_amount(&self) -> Option<f64> {
        number(&self.raw, "N005")
    }
    pub fn large_net_amount(&self) -> Option<f64> {
        number(&self.raw, "N006")
    }
    pub fn active_buy_net_amount(&self) -> Option<f64> {
        number(&self.raw, "N007")
    }
    /// Source code names an amount but does not establish its scale/currency.
    pub const fn amount_unit(&self) -> AmountUnit {
        AmountUnit::Unknown
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundFlowRangeSeries {
    code: AshareCode,
    days: HistoryDays,
    start: usize,
    count: Option<NonZeroU32>,
    source_row_count: usize,
    rows: Vec<FundFlowHistoryRow>,
    response: Response,
}
impl FundFlowRangeSeries {
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn days(&self) -> HistoryDays {
        self.days
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn count(&self) -> Option<NonZeroU32> {
        self.count
    }
    pub fn source_row_count(&self) -> usize {
        self.source_row_count
    }
    /// Original response reversed and then sliced; deliberately never sorted.
    pub fn rows(&self) -> &[FundFlowHistoryRow] {
        &self.rows
    }
    pub fn entry(&self) -> &str {
        self.response.entry()
    }
    pub fn request(&self) -> &RequestPayload {
        self.response.request()
    }
    pub fn ok(&self) -> bool {
        self.response.ok()
    }
    pub fn error_code(&self) -> Option<i64> {
        self.response.error_code()
    }
    pub fn attributes(&self) -> &[(String, JsonValue)] {
        self.response.attributes()
    }
    pub fn meta(&self) -> &ResultMeta {
        self.response.meta()
    }
    /// The first table's rows have moved to `rows()`. Its original columns,
    /// all additional tables, business code and retained raw remain available.
    pub fn response(&self) -> &Response {
        &self.response
    }
}

/// Owned handle; clones and the originating F10 client share one close state.
#[derive(Clone, Debug)]
pub struct MoneyFlowHistoryService {
    client: F10Client,
}
impl F10Client {
    /// Does not initialize a runtime, make a request, or construct an HQ client.
    pub fn money_flow_history(&self) -> MoneyFlowHistoryService {
        MoneyFlowHistoryService {
            client: self.clone(),
        }
    }
}
impl MoneyFlowHistoryService {
    pub fn history(
        &self,
        query: &MoneyFlowHistoryQuery,
        options: &OperationOptions,
    ) -> Result<FundFlowRangeSeries> {
        self.read(query, options, Instant::now(), "f10.money_flow_history")
    }
    /// Alias with the same window and reverse-then-slice semantics as history.
    pub fn range(
        &self,
        query: &MoneyFlowHistoryQuery,
        options: &OperationOptions,
    ) -> Result<FundFlowRangeSeries> {
        self.read(query, options, Instant::now(), "f10.money_flow_range")
    }
    fn read(
        &self,
        query: &MoneyFlowHistoryQuery,
        options: &OperationOptions,
        entry: Instant,
        name: &'static str,
    ) -> Result<FundFlowRangeSeries> {
        let context = self.client.context(entry, name, options)?;
        let mut response = self.client.call_in_context(&query.to_call()?, &context)?;
        let source = response.take_first_rows();
        let source_row_count = source.len();
        let selected_count = source_row_count
            .saturating_sub(query.start)
            .min(query.count.map_or(usize::MAX, |value| value.get() as usize));
        let mut rows = Vec::new();
        context.check_boundary(TimeoutPhase::Aggregate)?;
        context.record_work(source_row_count, 0)?;
        rows.try_reserve_exact(selected_count).map_err(|error| {
            Error::new(
                ErrorKind::LimitExceeded,
                "cannot reserve historical fund-flow rows",
            )
            .with_source(error)
        })?;
        // Traverse even skipped rows with cancellation points. Do not sort by
        // parsed dates, and do not compute start+count (which could overflow).
        reverse_then_window(
            || Ok::<_, Error>(source.into_iter().rev().enumerate()),
            |source| {
                for (index, raw) in source {
                    context.check_boundary(TimeoutPhase::Aggregate)?;
                    if index < query.start || rows.len() >= selected_count {
                        continue;
                    }
                    let trade_date = project_date(&raw, "N001", index, &mut response, &context)?;
                    let listing_date = project_date(&raw, "N009", index, &mut response, &context)?;
                    for name in ["N004", "N005", "N006", "N007"] {
                        if field(&raw, name).is_some_and(present) && number(&raw, name).is_none() {
                            response.push_diagnostic(projection_diagnostic(
                                index,
                                name,
                                "invalid finite number retained as raw",
                            ));
                        }
                    }
                    rows.push(FundFlowHistoryRow {
                        raw,
                        trade_date,
                        listing_date,
                    });
                }
                Ok(())
            },
        )?;
        let result = FundFlowRangeSeries {
            code: query.code.clone(),
            days: query.days,
            start: query.start,
            count: query.count,
            source_row_count,
            rows,
            response,
        };
        context.finish()?;
        Ok(result)
    }
}

// Match the established first-cell typed projection. `raw()` exposes all
// duplicates without applying Python's optional suffix/overwrite convention.
fn field<'a>(row: &'a Row, name: &str) -> Option<&'a JsonValue> {
    row.cells()
        .iter()
        .find(|cell| cell.name() == name)
        .map(|cell| cell.value())
}
fn present(value: &JsonValue) -> bool {
    !matches!(value, JsonValue::Null)
        && !matches!(value, JsonValue::String(text) if text.trim().is_empty())
}
fn number(row: &Row, name: &str) -> Option<f64> {
    match field(row, name)? {
        JsonValue::Number(value) => value.as_f64(),
        JsonValue::String(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}
fn projection_diagnostic(row: usize, field: &str, reason: &str) -> Diagnostic {
    Diagnostic::new(
        "f10.money_flow_history",
        "projection",
        EnrichmentStatus::Unavailable,
        None,
        Some(format!("reversed row {row}, {field}: {reason}")),
    )
}
fn project_date(
    raw: &Row,
    name: &str,
    index: usize,
    response: &mut Response,
    context: &OperationContext,
) -> Result<Option<TradingDate>> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    let Some(value) = field(raw, name).filter(|value| present(value)) else {
        return Ok(None);
    };
    let parsed = date(value);
    if parsed.is_none() {
        // Append to the same response metadata without cloning source bytes.
        response.push_diagnostic(projection_diagnostic(
            index,
            name,
            "invalid Gregorian date retained as raw",
        ));
    }
    Ok(parsed)
}
fn date(value: &JsonValue) -> Option<TradingDate> {
    let text = match value {
        JsonValue::String(value) => value.trim(),
        JsonValue::Number(value) => value.lexeme(),
        _ => return None,
    };
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
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let year = digits[..4]
        .iter()
        .fold(0_u16, |value, digit| value * 10 + u16::from(digit - b'0'));
    let month = (digits[4] - b'0') * 10 + digits[5] - b'0';
    let day = (digits[6] - b'0') * 10 + digits[7] - b'0';
    TradingDate::new(year, month, day).ok()
}
