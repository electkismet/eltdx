//! Dedicated F10 limit-board route. Server classifications are observations,
//! distinct from HQ's price-limit rule evaluation and live auction ladder.
use std::time::Instant;
use crate::operation::OperationContext;
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase, TradingDate};
use crate::f10::{CallQuery, F10Client, F10MarketCode, JsonValue, RequestPayload, Response, Row};
use crate::f10::values::{date_pair, date_text, j};
use super::market::{integer, number, string, value};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LimitUpDownListQuery {
    start_date: Option<TradingDate>, end_date: Option<TradingDate>, include_summary: bool,
}
impl LimitUpDownListQuery {
    pub fn new() -> Self { Self::default() }
    pub fn start_date(&self) -> Option<TradingDate> { self.start_date }
    pub fn end_date(&self) -> Option<TradingDate> { self.end_date }
    pub fn include_summary(&self) -> bool { self.include_summary }
    pub fn with_start_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_pair(value, self.end_date)?; self.start_date = value; Ok(self)
    }
    pub fn with_end_date(mut self, value: Option<TradingDate>) -> Result<Self> {
        date_pair(self.start_date, value)?; self.end_date = value; Ok(self)
    }
    pub fn with_dates(mut self, start: Option<TradingDate>, end: Option<TradingDate>) -> Result<Self> {
        date_pair(start, end)?; self.start_date = start; self.end_date = end; Ok(self)
    }
    pub fn with_include_summary(mut self, value: bool) -> Self { self.include_summary = value; self }
    /// Construct the detail request for an explicit range. When both dates
    /// are absent, the client must first capture its local-date clock.
    pub fn to_call(&self) -> Result<CallQuery> {
        let (start, end) = self.resolve(None)?;
        request("1", start, end)
    }
    fn resolve(&self, today: Option<TradingDate>) -> Result<(TradingDate, TradingDate)> {
        date_pair(self.start_date, self.end_date)?;
        crate::f10::collection::resolve_dates(self.start_date, self.end_date,
            || today.ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "ladder query needs the client's local date clock")))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LimitBoardLadderQuery { inner: LimitUpDownListQuery }
impl LimitBoardLadderQuery {
    pub fn new() -> Self { Self::default() }
    pub fn start_date(&self) -> Option<TradingDate> { self.inner.start_date() }
    pub fn end_date(&self) -> Option<TradingDate> { self.inner.end_date() }
    pub fn include_summary(&self) -> bool { self.inner.include_summary() }
    pub fn with_start_date(mut self, value: Option<TradingDate>) -> Result<Self> { self.inner = self.inner.with_start_date(value)?; Ok(self) }
    pub fn with_end_date(mut self, value: Option<TradingDate>) -> Result<Self> { self.inner = self.inner.with_end_date(value)?; Ok(self) }
    pub fn with_dates(mut self, start: Option<TradingDate>, end: Option<TradingDate>) -> Result<Self> { self.inner = self.inner.with_dates(start, end)?; Ok(self) }
    pub fn with_include_summary(mut self, value: bool) -> Self { self.inner = self.inner.with_include_summary(value); self }
    pub fn to_call(&self) -> Result<CallQuery> { self.inner.to_call() }
}

fn request(selector: &str, start: TradingDate, end: TradingDate) -> Result<CallQuery> {
    super::params("CWServ.cfg_fx_lbtt", super::super::collection::ladder_parameters(
        j(selector), j(date_text(Some(start))), j(date_text(Some(end)))).into())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LimitBoardType { Up, Broken, Down, Unknown(Option<i64>) }
impl LimitBoardType {
    fn from_wire(value: Option<i64>) -> Self {
        match value { Some(1) => Self::Up, Some(3) => Self::Broken, Some(0) => Self::Down, value => Self::Unknown(value) }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LadderDateSource { ExplicitRange, CopiedStart, CopiedEnd, LocalClock }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitBoardLadderRow {
    raw: Row, trade_date: Option<TradingDate>, board_level: Option<u32>, status: LimitBoardType,
}
impl LimitBoardLadderRow {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn trading_date(&self) -> Option<&JsonValue> { value(&self.raw, crate::f10::collection::ladder_column("trading_date")) }
    pub fn trading_date_value(&self) -> Option<&JsonValue> { value(&self.raw, crate::f10::collection::ladder_column("trading_date_value")) }
    pub fn trade_date(&self) -> Option<TradingDate> { self.trade_date }
    pub fn board_level(&self) -> Option<u32> { self.board_level }
    pub fn consecutive_limit_days(&self) -> Option<u32> { self.board_level }
    pub fn status(&self) -> LimitBoardType { self.status }
    pub fn code(&self) -> Option<&str> { string(&self.raw, crate::f10::collection::ladder_column("code")) }
    pub fn market(&self) -> Option<F10MarketCode> {
        integer(&self.raw, "SC").and_then(|value| u16::try_from(value).ok()).map(F10MarketCode::new)
    }
    pub fn limit_reason(&self) -> Option<&JsonValue> { value(&self.raw, "ztyy") }
    pub fn limit_reason_extra(&self) -> Option<&JsonValue> { value(&self.raw, "ztyy2") }
    pub fn reason(&self) -> Option<&JsonValue> {
        self.limit_reason().filter(|value| present(value)).or_else(|| self.limit_reason_extra())
    }
    /// Native numeric value. Its monetary unit is not established.
    pub fn seal_amount(&self) -> Option<f64> { number(&self.raw, "fde") }
    pub fn name(&self) -> Option<&JsonValue> { value(&self.raw, "ZQJC") }
    pub fn limit_time(&self) -> Option<&JsonValue> { value(&self.raw, "ztsj") }
    pub fn broken_count(&self) -> Option<i64> { integer(&self.raw, "kbcs") }
    pub fn industry(&self) -> Option<&JsonValue> { value(&self.raw, "sshy") }
    pub fn limit_type(&self) -> Option<i64> { integer(&self.raw, "ztlb") }
    /// Uninterpreted value; the gateway's scale is not asserted as a ratio.
    pub fn success_rate(&self) -> Option<&JsonValue> { value(&self.raw, "cgl") }
    pub fn highest_board_level(&self) -> Option<i64> { integer(&self.raw, "zglb") }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitMarketOverview { raw: Row, trade_date: Option<TradingDate> }
impl LimitMarketOverview {
    pub fn raw(&self) -> &Row { &self.raw }
    pub fn trade_date(&self) -> Option<TradingDate> { self.trade_date }
    pub fn advancing_count(&self) -> Option<i64> { integer(&self.raw, "N002") }
    pub fn declining_count(&self) -> Option<i64> { integer(&self.raw, "N003") }
    pub fn flat_count(&self) -> Option<i64> { integer(&self.raw, "N004") }
    pub fn limit_up_count(&self) -> Option<i64> { integer(&self.raw, "N005") }
    pub fn limit_down_count(&self) -> Option<i64> { integer(&self.raw, "N006") }
    /// Native amount with unknown unit.
    pub fn today_amount(&self) -> Option<f64> { number(&self.raw, "N007") }
    /// Native amount with unknown unit.
    pub fn previous_amount(&self) -> Option<f64> { number(&self.raw, "N008") }
    pub fn hot_topics(&self) -> Vec<(Option<&JsonValue>, Option<&JsonValue>)> {
        [("N009", "N010"), ("N011", "N012"), ("N013", "N014")].into_iter()
            .filter_map(|(name, count)| value(&self.raw, name).filter(|value| present(value))
                .map(|name| (Some(name), value(&self.raw, count)))).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LadderDayStats { date: TradingDate, counts: Vec<(u32, usize)> }
impl LadderDayStats {
    pub fn trade_date(&self) -> TradingDate { self.date }
    /// Descending positive board level, with only gateway type=1 included.
    pub fn counts(&self) -> &[(u32, usize)] { &self.counts }
    /// A ratio is present only for an adjacent nonzero denominator and a
    /// numerator no greater than it. Contradictory counts remain observable.
    pub fn promotion_rates(&self) -> Vec<(u32, f64)> {
        self.counts.iter().filter_map(|&(level, count)| {
            let lower = level.checked_sub(1)?;
            let denominator = self.counts.iter().find(|&&(value, _)| value == lower)?.1;
            (denominator > 0 && count <= denominator).then_some((level, count as f64 / denominator as f64))
        }).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LimitBoardLadder {
    detail: Response, summary_response: Option<Response>, start_date: TradingDate, end_date: TradingDate,
    date_source: LadderDateSource, rows: Vec<LimitBoardLadderRow>, summary: Vec<LimitMarketOverview>,
    statistics: Vec<LadderDayStats>, meta: ResultMeta,
}
impl LimitBoardLadder {
    pub fn entry(&self) -> &str { self.detail.entry() }
    pub fn request(&self) -> &RequestPayload { self.detail.request() }
    pub fn error_code(&self) -> Option<i64> { self.detail.error_code() }
    pub fn ok(&self) -> bool { self.detail.ok() }
    pub fn summary_error_code(&self) -> Option<i64> { self.summary_response.as_ref().and_then(Response::error_code) }
    pub fn summary_ok(&self) -> Option<bool> { self.summary_response.as_ref().map(Response::ok) }
    pub fn attributes(&self) -> &[(String, JsonValue)] { self.detail.attributes() }
    /// First-table rows have moved to `rows()`; all other result sets and
    /// original retained bytes remain reachable through this response.
    pub fn detail_response(&self) -> &Response { &self.detail }
    /// First-table rows have moved to `summary()`.
    pub fn summary_response(&self) -> Option<&Response> { self.summary_response.as_ref() }
    pub fn start_date(&self) -> TradingDate { self.start_date }
    pub fn end_date(&self) -> TradingDate { self.end_date }
    pub fn date_source(&self) -> LadderDateSource { self.date_source }
    pub fn trade_date(&self) -> Option<TradingDate> { (self.start_date == self.end_date).then_some(self.start_date) }
    pub fn rows(&self) -> &[LimitBoardLadderRow] { &self.rows }
    pub fn summary(&self) -> &[LimitMarketOverview] { &self.summary }
    pub fn market_overview(&self) -> &[LimitMarketOverview] { &self.summary }
    pub fn ladder_counts_by_date(&self) -> &[LadderDayStats] { &self.statistics }
    /// A range deliberately has no merged multi-day count.
    pub fn ladder_counts(&self) -> Option<&[(u32, usize)]> {
        let date = self.trade_date()?;
        Some(self.statistics.iter().find(|row| row.date == date).map_or(&[], |row| row.counts.as_slice()))
    }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

fn present(value: &JsonValue) -> bool {
    !matches!(value, JsonValue::Null) && !matches!(value, JsonValue::String(value) if value.trim().is_empty())
}
fn date(value: &JsonValue) -> Option<TradingDate> {
    let text = match value { JsonValue::String(value) => value.trim(), JsonValue::Number(value) => value.lexeme(), _ => return None };
    if !matches!(text.len(), 8 | 10) { return None; }
    let mut digits = [0_u8; 8];
    if text.len() == 10 {
        let bytes = text.as_bytes();
        if !matches!((bytes[4], bytes[7]), (b'-', b'-') | (b'/', b'/')) { return None; }
        digits[..4].copy_from_slice(&bytes[..4]); digits[4..6].copy_from_slice(&bytes[5..7]); digits[6..].copy_from_slice(&bytes[8..]);
    } else { digits.copy_from_slice(text.as_bytes()); }
    if !digits.iter().all(u8::is_ascii_digit) { return None; }
    let year = digits[..4].iter().fold(0_u16, |value, digit| value * 10 + u16::from(digit - b'0'));
    let month = (digits[4] - b'0') * 10 + digits[5] - b'0';
    let day = (digits[6] - b'0') * 10 + digits[7] - b'0';
    TradingDate::new(year, month, day).ok()
}
fn diagnostic(context: &OperationContext, meta: &mut ResultMeta, row: usize, field: &str, message: &str) -> Result<()> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    *meta = std::mem::take(meta).with_diagnostic(Diagnostic::new("f10.ladder", "projection", EnrichmentStatus::Unavailable,
        None, Some(format!("row {row}, {field}: {message}"))));
    Ok(())
}
fn check_fields(raw: &Row, row: usize, integers: &[&str], numbers: &[&str], context: &OperationContext, meta: &mut ResultMeta) -> Result<()> {
    for field in integers {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if value(raw, field).is_some_and(present) && integer(raw, field).is_none() { diagnostic(context, meta, row, field, "invalid integer retained as raw")?; }
    }
    for field in numbers {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if value(raw, field).is_some_and(present) && number(raw, field).is_none() { diagnostic(context, meta, row, field, "invalid finite numeric value retained as raw")?; }
    }
    Ok(())
}
fn selected_date(raw: &Row, primary: &str, fallback: &str, row: usize, context: &OperationContext, meta: &mut ResultMeta) -> Result<Option<TradingDate>> {
    let mut chosen = None;
    for field in [primary, fallback] {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if let Some(value) = value(raw, field).filter(|value| present(value)) {
            match date(value) {
                Some(date) if chosen.is_none() => chosen = Some(date),
                Some(_) => {},
                None => diagnostic(context, meta, row, field, "invalid Gregorian date retained as raw")?,
            }
        }
    }
    Ok(chosen)
}
fn statistics(rows: &[LimitBoardLadderRow], context: &OperationContext, meta: &mut ResultMeta) -> Result<Vec<LadderDayStats>> {
    let mut days: Vec<LadderDayStats> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let (Some(date), Some(level)) = (row.trade_date, row.board_level) else {
            if row.status == LimitBoardType::Up && row.trade_date.is_none() { diagnostic(context, meta, index, "rqex/rq", "row omitted from date statistics")?; }
            continue;
        };
        let day_index = match days.binary_search_by_key(&date, |day| day.date) {
            Ok(index) => index,
            Err(index) => {
                days.try_reserve(1).map_err(allocation)?;
                days.insert(index, LadderDayStats { date, counts: Vec::new() });
                index
            }
        };
        let counts = &mut days[day_index].counts;
        match counts.binary_search_by(|&(value, _)| level.cmp(&value)) {
            Ok(index) => counts[index].1 = counts[index].1.checked_add(1).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "ladder count overflow"))?,
            Err(index) => { counts.try_reserve(1).map_err(allocation)?; counts.insert(index, (level, 1)); }
        }
    }
    for day in &days {
        for &(level, count) in &day.counts {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            if let Some(denominator) = level.checked_sub(1).and_then(|lower| day.counts.iter().find(|&&(value, _)| value == lower)).map(|&(_, count)| count) {
                if count > denominator { diagnostic(context, meta, 0, "promotion_rate", "adjacent board counts imply ratio greater than one; rate omitted")?; }
            }
        }
    }
    Ok(days)
}
fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 ladder result").with_source(error)
}

impl F10Client {
    pub fn limit_up_down_list(&self, query: &LimitUpDownListQuery, options: &OperationOptions) -> Result<LimitBoardLadder> {
        self.ladder(query, options, Instant::now(), "f10.limit_up_down_list")
    }
    pub fn limit_board_ladder(&self, query: &LimitBoardLadderQuery, options: &OperationOptions) -> Result<LimitBoardLadder> {
        self.ladder(&query.inner, options, Instant::now(), "f10.limit_board_ladder")
    }
    fn ladder(&self, query: &LimitUpDownListQuery, options: &OperationOptions, entry: Instant, name: &'static str) -> Result<LimitBoardLadder> {
        let context = self.context(entry, name, options)?;
        let date_source = match (query.start_date, query.end_date) {
            (Some(_), Some(_)) => LadderDateSource::ExplicitRange,
            (Some(_), None) => LadderDateSource::CopiedStart,
            (None, Some(_)) => LadderDateSource::CopiedEnd,
            (None, None) => LadderDateSource::LocalClock,
        };
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let today = if date_source == LadderDateSource::LocalClock { Some(self.inner.local_date_clock.0.today()?) } else { None };
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let (start_date, end_date) = query.resolve(today)?;
        let mut detail = self.endpoint_call(&request("1", start_date, end_date)?, &context)?;
        let mut summary_response = if query.include_summary { Some(self.endpoint_call(&request("2", start_date, end_date)?, &context)?) } else { None };
        let mut metadata = crate::f10::response::MetaCollector::default();
        metadata.add(detail.meta(), &context)?;
        if let Some(response) = &summary_response { metadata.add(response.meta(), &context)?; }
        let mut meta = metadata.finish().with_diagnostic(Diagnostic::new("f10.ladder", "query_date", EnrichmentStatus::Succeeded,
            None, Some(format!("date source: {date_source:?}"))));
        let mut rows = Vec::new();
        for (index, raw) in detail.take_first_rows().into_iter().enumerate() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            context.record_work(1, 0)?;
            check_fields(&raw, index, &["lbts", "SC", "kbcs", "ztlb", "zglb"], &["fde"], &context, &mut meta)?;
            let status = LimitBoardType::from_wire(integer(&raw, "ztlb"));
            let candidate = (status == LimitBoardType::Up).then(|| integer(&raw, "lbts")).flatten()
                .and_then(|level| u32::try_from(level).ok());
            let keep = crate::f10::collection::keep_ladder_level(|| Ok::<_, Error>(status == LimitBoardType::Up),
                || Ok(candidate.is_some()), || Ok(candidate.is_some_and(|level| level > 0)))?;
            let board_level = if keep { candidate } else { None };
            if status == LimitBoardType::Up && board_level.is_none() {
                diagnostic(&context, &mut meta, index, "lbts", "positive u32 board level required for ladder statistics")?;
            }
            if integer(&raw, "SC").is_some_and(|market| u16::try_from(market).is_err()) {
                diagnostic(&context, &mut meta, index, "SC", "market is outside u16 range; raw value retained")?;
            }
            let trade_date = selected_date(&raw, "rqex", "rq", index, &context, &mut meta)?;
            if trade_date.is_some_and(|date| date < start_date || date > end_date) { diagnostic(&context, &mut meta, index, "rqex/rq", "server date is outside requested range")?; }
            if matches!(status, LimitBoardType::Unknown(_)) { diagnostic(&context, &mut meta, index, "ztlb", "unknown gateway classification retained")?; }
            rows.try_reserve(1).map_err(allocation)?;
            rows.push(LimitBoardLadderRow { raw, trade_date, board_level, status });
        }
        let mut summary = Vec::new();
        if let Some(response) = &mut summary_response {
            for (index, raw) in response.take_first_rows().into_iter().enumerate() {
                context.check_boundary(TimeoutPhase::Aggregate)?;
                context.record_work(1, 0)?;
                check_fields(&raw, index, &["N002", "N003", "N004", "N005", "N006", "N010", "N012", "N014"], &["N007", "N008"], &context, &mut meta)?;
                let trade_date = selected_date(&raw, "N001", "t001", index, &context, &mut meta)?;
                summary.try_reserve(1).map_err(allocation)?;
                summary.push(LimitMarketOverview { raw, trade_date });
            }
            if !response.ok() { diagnostic(&context, &mut meta, 0, "summary.ErrorCode", "summary business error preserved separately")?; }
        }
        let statistics = statistics(&rows, &context, &mut meta)?;
        context.finish()?;
        Ok(LimitBoardLadder { detail, summary_response, start_date, end_date, date_source, rows, summary, statistics, meta })
    }
}
