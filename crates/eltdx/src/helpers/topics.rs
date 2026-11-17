//! Topic compositions preserve physical source order and original row cells.
use super::HelperService;
use crate::f10::{
    AshareCode, HotTopicsQuery, JsonValue, Response, Row, SectionCode, TopicCompareQuery,
    TopicIdsQuery,
};
use crate::operation::{OperationContext, WorkLimits};
use crate::services::{aggregate_meta, allocation_error};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta, SecurityId};
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopicSource {
    TopicIds,
    HotTopics,
}
impl TopicSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopicIds => "topic_ids",
            Self::HotTopics => "hot_topics",
        }
    }
    pub const fn id_fields(self) -> &'static [&'static str] {
        match self {
            Self::TopicIds => &["t001", "id", "topic_id"],
            Self::HotTopics => &["id", "t001", "topic_id"],
        }
    }
    pub const fn name_fields(self) -> &'static [&'static str] {
        match self {
            Self::TopicIds => &["t002", "ztmc", "topic_name"],
            Self::HotTopics => &["ztmc", "t002", "topic_name"],
        }
    }
}
/// First matching column wins, including a null value. Shared by the native
/// legacy bridge; callers retain their own scalar-conversion policy.
pub const HOT_TOPIC_FIELDS: &[(&str, &[&str])] = &[
    ("relation_level", &["gld", "relation_level"]),
    ("selected_date", &["rxsj", "selected_date"]),
    ("topic_date", &["ztrq", "topic_date"]),
    ("reason", &["ztnr", "reason"]),
    ("detail_id", &["arec", "detail_id"]),
    ("category_raw", &["sslb", "category_raw"]),
];
pub const TOPIC_STOCK_FIELDS: &[(&str, &[&str])] = &[
    ("rank", &["pm", "rank"]),
    ("name", &["zqjc", "name"]),
    ("change_pct", &["zdf", "change_pct"]),
    ("change_pct_3d", &["zdf_3d", "change_pct_3d"]),
    ("change_pct_5d", &["zdf_5d", "change_pct_5d"]),
    ("change_pct_20d", &["zdf_20d", "change_pct_20d"]),
    ("change_pct_60d", &["zdf_60d", "change_pct_60d"]),
    ("change_pct_ytd", &["zdf_ys", "change_pct_ytd"]),
    ("trading_date", &["tjdate", "trading_date"]),
];
pub fn visit_sources<T, E>(
    ids: impl IntoIterator<Item = std::result::Result<T, E>>,
    hot: impl IntoIterator<Item = std::result::Result<T, E>>,
    mut visit: impl FnMut(TopicSource, T) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    visit_source(TopicSource::TopicIds, ids, &mut visit)?;
    visit_source(TopicSource::HotTopics, hot, visit)
}
pub fn visit_source<T, E>(
    source: TopicSource,
    rows: impl IntoIterator<Item = std::result::Result<T, E>>,
    mut visit: impl FnMut(TopicSource, T) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    for row in rows {
        visit(source, row?)?;
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopicMatch {
    Id,
    Name,
    First,
}
pub fn resolve_with<T, E>(
    has_id: bool,
    has_name: bool,
    mut search: impl FnMut(TopicMatch) -> std::result::Result<Option<T>, E>,
) -> std::result::Result<Option<T>, E> {
    if has_id {
        if let Some(value) = search(TopicMatch::Id)? {
            return Ok(Some(value));
        }
    }
    if has_name {
        if let Some(value) = search(TopicMatch::Name)? {
            return Ok(Some(value));
        }
    }
    if !has_id && !has_name {
        return search(TopicMatch::First);
    }
    Ok(None)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockTopicsQuery {
    code: AshareCode,
    max_records: NonZeroUsize,
}
impl StockTopicsQuery {
    pub fn new(code: AshareCode) -> Self {
        Self {
            code,
            max_records: NonZeroUsize::new(20_000).unwrap_or(NonZeroUsize::MIN),
        }
    }
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn max_records(&self) -> NonZeroUsize {
        self.max_records
    }
    pub fn with_max_records(mut self, value: NonZeroUsize) -> Self {
        self.max_records = value;
        self
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StockTopic {
    topic_id: Option<String>,
    topic_name: Option<String>,
    relation_level: Option<f64>,
    reason: Option<String>,
    detail_id: Option<String>,
    raw: Vec<(TopicSource, Row)>,
}
impl StockTopic {
    pub fn topic_id(&self) -> Option<&str> {
        self.topic_id.as_deref()
    }
    pub fn topic_name(&self) -> Option<&str> {
        self.topic_name.as_deref()
    }
    pub fn relation_level(&self) -> Option<f64> {
        self.relation_level
    }
    pub fn selected_date(&self) -> Option<&JsonValue> {
        self.hot_value("selected_date")
    }
    pub fn topic_date(&self) -> Option<&JsonValue> {
        self.hot_value("topic_date")
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    pub fn detail_id(&self) -> Option<&str> {
        self.detail_id.as_deref()
    }
    pub fn category_raw(&self) -> Option<&JsonValue> {
        self.hot_value("category_raw")
    }
    pub fn raw(&self) -> &[(TopicSource, Row)] {
        &self.raw
    }
    pub fn sources(&self) -> impl Iterator<Item = TopicSource> + '_ {
        self.raw.iter().map(|(source, _)| *source)
    }
    fn hot_value(&self, name: &str) -> Option<&JsonValue> {
        self.raw
            .iter()
            .find(|(source, _)| *source == TopicSource::HotTopics)
            .and_then(|(_, row)| field(row, name))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StockTopics {
    code: AshareCode,
    topics: Vec<StockTopic>,
    responses: Vec<Response>,
    meta: ResultMeta,
}
impl StockTopics {
    pub fn code(&self) -> &AshareCode {
        &self.code
    }
    pub fn topics(&self) -> &[StockTopic] {
        &self.topics
    }
    pub fn count(&self) -> usize {
        self.topics.len()
    }
    pub fn responses(&self) -> &[Response] {
        &self.responses
    }
    pub fn ok(&self) -> bool {
        self.responses.iter().all(Response::ok)
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopicStocksQuery {
    source: StockTopicsQuery,
    topic_id: Option<String>,
    topic_name: Option<String>,
    section: SectionCode,
    sort_by: String,
}
impl TopicStocksQuery {
    /// Explicitly request the first discovered topic when no selector is set.
    pub fn first(code: AshareCode) -> Self {
        Self {
            source: StockTopicsQuery::new(code),
            topic_id: None,
            topic_name: None,
            section: SectionCode::default_for("topic_compare", "gndbzfsj"),
            sort_by: "zdf".to_owned(),
        }
    }
    pub fn new(code: AshareCode, id: impl Into<String>) -> Result<Self> {
        let mut query = Self::first(code);
        let id = id.into();
        TopicCompareQuery::new(query.source.code.clone(), id.clone())?;
        query.topic_id = Some(id);
        Ok(query)
    }
    pub fn named(code: AshareCode, name: impl Into<String>) -> Result<Self> {
        Self::first(code).with_topic_name(Some(name.into()))
    }
    pub fn with_topic_name(mut self, value: Option<String>) -> Result<Self> {
        if value
            .as_ref()
            .is_some_and(|value| value.trim().is_empty() || value.contains('\0'))
        {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "topic name must not be blank or contain NUL",
            ));
        }
        self.topic_name = value;
        Ok(self)
    }
    pub fn with_sort_by(mut self, value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        TopicCompareQuery::new(self.source.code.clone(), "validation")?
            .with_sort_by(value.clone())?;
        self.sort_by = value;
        Ok(self)
    }
    pub fn with_section(mut self, value: SectionCode) -> Result<Self> {
        value.checked_for("topic_compare")?;
        self.section = value;
        Ok(self)
    }
    pub fn with_max_records(mut self, value: NonZeroUsize) -> Self {
        self.source.max_records = value;
        self
    }
    pub fn seed_code(&self) -> &AshareCode {
        &self.source.code
    }
    pub fn topic_id(&self) -> Option<&str> {
        self.topic_id.as_deref()
    }
    pub fn topic_name(&self) -> Option<&str> {
        self.topic_name.as_deref()
    }
    pub fn section(&self) -> &SectionCode {
        &self.section
    }
    pub fn sort_by(&self) -> &str {
        &self.sort_by
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TopicStock {
    raw: Row,
    id: Option<SecurityId>,
}
impl TopicStock {
    pub fn raw(&self) -> &Row {
        &self.raw
    }
    pub fn id(&self) -> Option<&SecurityId> {
        self.id.as_ref()
    }
    pub fn rank(&self) -> Option<i64> {
        integer(pick(&self.raw, &["pm", "rank"]))
    }
    pub fn code(&self) -> Option<String> {
        scalar_text(pick(&self.raw, &["zqdm", "code"]))
    }
    pub fn name(&self) -> Option<String> {
        scalar_text(pick(&self.raw, &["zqjc", "name"]))
    }
    /// Percentage points, not an ordinary fraction. Other change windows are
    /// available by their original column names through raw().
    pub fn change_pct(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf", "change_pct"]))
    }
    pub fn change_pct_3d(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf_3d", "change_pct_3d"]))
    }
    pub fn change_pct_5d(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf_5d", "change_pct_5d"]))
    }
    pub fn change_pct_20d(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf_20d", "change_pct_20d"]))
    }
    pub fn change_pct_60d(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf_60d", "change_pct_60d"]))
    }
    pub fn change_pct_ytd(&self) -> Option<f64> {
        number(pick(&self.raw, &["zdf_ys", "change_pct_ytd"]))
    }
    pub fn trading_date(&self) -> Option<&JsonValue> {
        pick(&self.raw, &["tjdate", "trading_date"])
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TopicStockTable {
    seed: AshareCode,
    topic_id: String,
    topic_name: Option<String>,
    sort_by: String,
    rows: Vec<TopicStock>,
    responses: Vec<Response>,
    meta: ResultMeta,
}
impl TopicStockTable {
    pub fn seed_code(&self) -> &AshareCode {
        &self.seed
    }
    pub fn topic_id(&self) -> &str {
        &self.topic_id
    }
    pub fn topic_name(&self) -> Option<&str> {
        self.topic_name.as_deref()
    }
    pub fn sort_by(&self) -> &str {
        &self.sort_by
    }
    pub fn rows(&self) -> &[TopicStock] {
        &self.rows
    }
    pub fn count(&self) -> usize {
        self.rows.len()
    }
    pub fn responses(&self) -> &[Response] {
        &self.responses
    }
    pub fn ok(&self) -> bool {
        self.responses.iter().all(Response::ok)
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

fn pick<'a>(row: &'a Row, names: &[&str]) -> Option<&'a JsonValue> {
    names.iter().find_map(|name| {
        row.cells()
            .iter()
            .find(|cell| cell.name() == *name)
            .map(|cell| cell.value())
    })
}
fn scalar_text(value: Option<&JsonValue>) -> Option<String> {
    match value? {
        JsonValue::String(value) => (!value.trim().is_empty()).then(|| value.trim().to_owned()),
        JsonValue::Number(value) => Some(value.lexeme().to_owned()),
        JsonValue::Bool(value) => Some(if *value { "True" } else { "False" }.to_owned()),
        _ => None,
    }
}
fn number(value: Option<&JsonValue>) -> Option<f64> {
    match value? {
        JsonValue::Number(value) => value.as_f64(),
        JsonValue::String(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}
fn integer(value: Option<&JsonValue>) -> Option<i64> {
    match value? {
        JsonValue::Number(value) => value.as_i64(),
        JsonValue::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}
fn key(id: Option<&str>, name: Option<&str>) -> String {
    if let Some(id) = id {
        format!("id:{id}")
    } else if let Some(name) = name {
        format!("name:{name}")
    } else {
        "unknown".to_owned()
    }
}
fn projected_identity(row: &Row) -> Option<SecurityId> {
    let code = scalar_text(pick(row, &["zqdm", "code"]))?;
    let market = integer(pick(row, &["sc", "market_id"]));
    let prefix = match market {
        Some(0) => Some("sz"),
        Some(1) => Some("sh"),
        Some(2) => Some("bj"),
        _ => None,
    };
    if let Some(prefix) = prefix {
        format!("{prefix}{code}").parse().ok()
    } else {
        code.parse().ok()
    }
}
fn field<'a>(row: &'a Row, name: &str) -> Option<&'a JsonValue> {
    HOT_TOPIC_FIELDS
        .iter()
        .find(|(field, _)| *field == name)
        .and_then(|(_, names)| pick(row, names))
}

impl HelperService {
    /// Both HTTP sources are required. Nonzero business codes remain explicit
    /// in responses()/ok(); transport/owner/budget failures terminate the call.
    pub fn stock_topics(
        &self,
        query: &StockTopicsQuery,
        options: &OperationOptions,
    ) -> Result<StockTopics> {
        let mut context = self
            .f10
            .context(Instant::now(), "helpers.stock_topics", options)?;
        context.set_work_limits(WorkLimits {
            records: Some(query.max_records.get()),
            bytes: Some(self.f10.config().max_operation_bytes().get() as usize),
            pages: None,
        })?;
        let result = self.stock_topics_in_context(query, &context)?;
        context.finish()?;
        Ok(result)
    }
    pub(super) fn stock_topics_in_context(
        &self,
        query: &StockTopicsQuery,
        context: &OperationContext,
    ) -> Result<StockTopics> {
        let mut ids = self
            .f10
            .endpoint_call(&TopicIdsQuery::new(query.code.clone()).to_call()?, context)?;
        let mut hot = self
            .f10
            .endpoint_call(&HotTopicsQuery::new(query.code.clone()).to_call()?, context)?;
        let ids_rows = ids.take_first_rows();
        let hot_rows = hot.take_first_rows();
        let count = ids_rows
            .len()
            .checked_add(hot_rows.len())
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "topic row count overflow"))?;
        if count > query.max_records.get() {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "topic source row limit exceeded",
            ));
        }
        context.record_work(count, 0)?;
        let mut topics: Vec<StockTopic> = Vec::new();
        let mut positions = HashMap::new();
        topics.try_reserve(count).map_err(allocation_error)?;
        positions.try_reserve(count).map_err(allocation_error)?;
        visit_sources(
            ids_rows.into_iter().map(Ok),
            hot_rows.into_iter().map(Ok),
            |source, row| {
                context.check_boundary(TimeoutPhase::Aggregate)?;
                let topic_id = scalar_text(pick(&row, source.id_fields()));
                let topic_name = scalar_text(pick(&row, source.name_fields()));
                let identity = key(topic_id.as_deref(), topic_name.as_deref());
                let index = *positions.entry(identity).or_insert_with(|| {
                    let index = topics.len();
                    topics.push(StockTopic {
                        topic_id: None,
                        topic_name: None,
                        relation_level: None,
                        reason: None,
                        detail_id: None,
                        raw: Vec::new(),
                    });
                    index
                });
                let item = &mut topics[index];
                item.topic_id = topic_id;
                item.topic_name = topic_name;
                if source == TopicSource::HotTopics {
                    item.relation_level = number(field(&row, "relation_level"));
                    item.reason = scalar_text(field(&row, "reason"));
                    item.detail_id = scalar_text(field(&row, "detail_id"));
                }
                if let Some((_, raw)) = item.raw.iter_mut().find(|(kind, _)| *kind == source) {
                    *raw = row;
                } else {
                    item.raw.try_reserve(1).map_err(allocation_error)?;
                    item.raw.push((source, row));
                }
                Ok::<_, Error>(())
            },
        )?;
        let meta = aggregate_meta(context, &[ids.meta().clone(), hot.meta().clone()], None)?;
        Ok(StockTopics {
            code: query.code.clone(),
            topics,
            responses: vec![ids, hot],
            meta,
        })
    }
    pub fn topic_stocks(
        &self,
        query: &TopicStocksQuery,
        options: &OperationOptions,
    ) -> Result<TopicStockTable> {
        let mut context = self
            .f10
            .context(Instant::now(), "helpers.topic_stocks", options)?;
        context.set_work_limits(WorkLimits {
            records: Some(query.source.max_records.get()),
            bytes: Some(self.f10.config().max_operation_bytes().get() as usize),
            pages: None,
        })?;
        let mut id = query.topic_id.clone();
        let mut name = query.topic_name.clone();
        let mut responses = Vec::new();
        if id.is_none() || name.is_some() {
            let source = self.stock_topics_in_context(&query.source, &context)?;
            if !source.ok() {
                return Err(Error::new(
                    ErrorKind::MissingData,
                    "topic resolution source reported a business error",
                )
                .with_context("source", "topic_ids+hot_topics")
                .with_context("completion", "business_error"));
            }
            let matched = resolve_with(id.is_some(), name.is_some(), |kind| {
                context.check_boundary(TimeoutPhase::Aggregate)?;
                let found = match kind {
                    TopicMatch::Id => source.topics.iter().find(|item| item.topic_id == id),
                    TopicMatch::Name => source.topics.iter().find(|item| item.topic_name == name),
                    TopicMatch::First => source.topics.first(),
                };
                Ok::<_, Error>(found)
            })?
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::MissingData,
                    "topic selector did not match the seed stock",
                )
            })?;
            if id.is_none() {
                id = matched.topic_id.clone();
            }
            if name.is_none() {
                name = matched.topic_name.clone();
            }
            responses = source.responses;
        }
        let id =
            id.ok_or_else(|| Error::new(ErrorKind::MissingData, "resolved topic has no id"))?;
        let call = TopicCompareQuery::new(query.source.code.clone(), id.clone())?
            .with_section(query.section.clone())?
            .with_sort_by(query.sort_by.clone())?
            .to_call()?;
        let mut response = self.f10.endpoint_call(&call, &context)?;
        let raw = response.take_first_rows();
        context.record_work(raw.len(), 0)?;
        if raw.len() > query.source.max_records.get() {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "topic stock row limit exceeded",
            ));
        }
        let mut rows = Vec::new();
        rows.try_reserve(raw.len()).map_err(allocation_error)?;
        for raw in raw {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let id = projected_identity(&raw);
            if id.is_none() {
                response.push_diagnostic(Diagnostic::new(
                    "f10.topic_compare",
                    "identity",
                    EnrichmentStatus::Unavailable,
                    None,
                    Some("invalid or absent identity retained in original row".to_owned()),
                ));
            }
            rows.push(TopicStock { raw, id });
        }
        responses.push(response);
        let metas = responses
            .iter()
            .map(|response| response.meta().clone())
            .collect::<Vec<_>>();
        let meta = aggregate_meta(&context, &metas, None)?;
        context.finish()?;
        Ok(TopicStockTable {
            seed: query.source.code.clone(),
            topic_id: id,
            topic_name: name,
            sort_by: query.sort_by.clone(),
            rows,
            responses,
            meta,
        })
    }
}
