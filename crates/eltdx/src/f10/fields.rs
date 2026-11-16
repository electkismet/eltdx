//! Context-scoped field labels copied from the two Python label dictionaries.
//! Labels identify source semantics; they do not establish numeric units.
//! Resolve each original cell independently to preserve duplicate columns.
use crate::{Error, ErrorKind, Result};
use super::{EntryName, JsonValue, RequestPayload, Response};

mod data;
use data::*;
enum Label { Pair(&'static str, &'static str), Include(&'static [Label]) }

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FieldMetadata { industry_template: Option<i64>, report_type: Option<String> }
impl FieldMetadata {
    pub fn new() -> Self { Self::default() }
    pub fn industry_template(&self) -> Option<i64> { self.industry_template }
    pub fn report_type(&self) -> Option<&str> { self.report_type.as_deref() }
    /// Unknown template numbers remain explicit; they select no known schema.
    pub fn with_industry_template(mut self, value: Option<i64>) -> Self { self.industry_template = value; self }
    pub fn with_report_type(mut self, value: Option<String>) -> Result<Self> {
        if let Some(value) = &value { super::values::text(value, false)?; }
        self.report_type = value; Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldContext {
    entry: EntryName, scope: Option<String>, metadata: FieldMetadata,
}
impl FieldContext {
    pub fn new(entry: EntryName, scope: Option<String>, metadata: FieldMetadata) -> Result<Self> {
        if let Some(scope) = &scope { super::values::text(scope, false)?; }
        Ok(Self { entry, scope, metadata })
    }
    pub fn from_request(entry: &EntryName, request: &RequestPayload, metadata: FieldMetadata) -> Result<Self> {
        let index = scope_index(entry.as_str());
        let params = match request {
            RequestPayload::Params(values) => Some(values.as_slice()),
            RequestPayload::Body(body) => body.get("Params").and_then(JsonValue::as_array),
        };
        let scope = index.and_then(|index| params.and_then(|params| params.get(index))).and_then(scalar);
        Self::new(entry.clone(), scope, metadata)
    }
    pub fn entry(&self) -> &EntryName { &self.entry }
    pub fn scope(&self) -> Option<&str> { self.scope.as_deref() }
    pub fn metadata(&self) -> &FieldMetadata { &self.metadata }
    /// A known report family in metadata must not contradict the request.
    /// Other report-type tokens remain in the context without invented maps.
    fn report_matches(&self) -> bool {
        match self.metadata.report_type.as_deref() {
            Some(report @ ("zcfzb" | "lyb" | "xjllb")) => self.scope() == Some(report),
            _ => true,
        }
    }
    fn labels(&self) -> &'static [Label] {
        let Some(scope) = self.scope() else { return &[]; };
        match self.entry.as_str() {
            "CWServ.tdxf10_gg_gsgk" => match scope { "8" => COMPANY_PROFILE_LISTING, "9" => COMPANY_PROFILE_INDEX_CHANGES, _ => &[] },
            "CWServ.tdxf10_gg_rdtc" if scope == "zttzbkz" => HOT_TOPICS,
            "CWServ.tdxf10_gg_gszx" => match scope { "gsyj" => COMPANY_RESEARCH, "jgcs" => SUPERVISION, _ => &[] },
            "CWServ.tdxf10_gg_cwfx" if self.report_matches() => match (scope, self.metadata.industry_template) {
                ("zcfzb", Some(0)) => BALANCE_SHEET_GENERAL, ("zcfzb", Some(1)) => BALANCE_SHEET_BANK,
                ("zcfzb", Some(2)) => BALANCE_SHEET_SECURITIES, ("zcfzb", Some(3)) => BALANCE_SHEET_INSURANCE,
                ("lyb", Some(0)) => INCOME_GENERAL, ("lyb", Some(1)) => INCOME_BANK,
                ("lyb", Some(2)) => INCOME_SECURITIES, ("lyb", Some(3)) => INCOME_INSURANCE,
                ("xjllb", Some(0)) => CASHFLOW_GENERAL, ("xjllb", Some(1)) => CASHFLOW_BANK,
                ("xjllb", Some(2)) => CASHFLOW_SECURITIES, ("xjllb", Some(3)) => CASHFLOW_INSURANCE,
                _ => &[],
            },
            "CWServ.skef10_mg_idreq" if scope == "name" => MG_NAME,
            "CWServ.skef10_mg_gsgk" if scope == "001" => MG_PROFILE,
            "CWServ.skef10_mg_zxdt" if matches!(scope, "zxdt" | "sypl") => MG_QUOTE_SNAP,
            "CWServ.skef10_mg_cwfx" if self.report_matches() => match scope {
                "zcfzb" => MG_BALANCE_WITH_COMMON, "lyb" => MG_INCOME_WITH_COMMON, "xjllb" => MG_CASHFLOW_WITH_COMMON, _ => &[],
            },
            "CWServ.skef10_mg_jyfx" => MG_BUSINESS,
            "CWServ.skef10_mg_hyfx" if scope == "001" => MG_INDUSTRY,
            "CWServ.skef10_mg_hydb" if scope == "001" => MG_INDUSTRY_COMPARE,
            "CWServ.skef10_mg_fhrz" => match scope { "fh" => MG_DIVIDEND_FH, "cf" => MG_DIVIDEND_CF, _ => &[] },
            "CWServ.skef10_mg_gbjg" => match scope { "001" => MG_CAPITAL_001, "002" => MG_CAPITAL_002, _ => &[] },
            "CWServ.skef10_mg_gbgd" if matches!(scope, "006" | "003") => MG_SHAREHOLDERS,
            "CWServ.skef10_mg_dsgg" if matches!(scope, "ds" | "gg" | "lrdjg") => MG_DIRECTORS,
            "CWServ.skef10_mg_schq" if scope == "001" => MG_MARKET,
            "CWServ.skef10_mg_gsbd" if scope == "gsgg" => MG_ANNOUNCEMENTS,
            "CWServ.skef10_mg_gspj" if scope == "001" => MG_RATING,
            "CWServ.skef10_mg_rdtc" if scope == "001" => MG_TOPIC,
            "CWServ.skef10_mg_sdfx" if matches!(scope, "001" | "002") => MG_DEEP_ANALYSIS,
            "CWServ.skef10_hk_idreq" if scope == "name" => HK_NAME,
            "CWServ.skef10_hk_zxts" if scope == "1" => HK_TIPS,
            "CWServ.skef10_hk_gsds" => match scope { "1" => HK_EVENTS_DIVIDEND, "2" => HK_EVENTS_CAPITAL, _ => &[] },
            "CWServ.skef10_hk_gsgk" => match scope { "1" => HK_PROFILE_1, "2" => HK_PROFILE_2, _ => &[] },
            "CWServ.skef10_hk_ggzl" if matches!(scope, "1" | "2" | "3") => HK_EXECUTIVES,
            "CWServ.skef10_hk_gbjg" if matches!(scope, "1" | "2" | "3") => HK_CAPITAL,
            "CWServ.skef10_hk_gdcg" if scope == "1" => HK_SHAREHOLDERS,
            "CWServ.skef10_hk_cwfx" if matches!(scope, "1" | "2" | "3") => HK_FINANCE,
            "CWServ.skef10_hk_cwzb" if matches!(scope, "1" | "2" | "3" | "4" | "5") => HK_FINANCE_INDICATOR,
            "CWServ.skef10_hk_jyfx" if scope == "1" => HK_BUSINESS,
            "CWServ.skef10_hk_hybj" if matches!(scope, "1" | "3") => HK_INDUSTRY_COMPARE,
            "CWServ.skef10_hk_schq" => match scope { "1" => HK_MARKET_SUSPEND, "2" => HK_MARKET_AB, _ => &[] },
            "CWServ.skef10_hk_glzq" if scope == "6" => HK_RELATED,
            "CWServ.skef10_hk_sdfx_gzfx" => HK_VALUATION,
            "CWServ.skef10_hk_sdfx_hqzft" => HK_PRICE_CHART,
            "CWServ.skef10_hk_sdfx_fxysyfx" => HK_ANALYST_FORECAST,
            "CWServ.skef10_hk_rdtc" => HK_TOPIC,
            _ => &[],
        }
    }
    fn notes(&self) -> &'static [Label] {
        if self.entry.as_str() != "CWServ.tdxf10_gg_cwfx" || self.scope() != Some("xjllb") || !self.report_matches() { return &[]; }
        match self.metadata.industry_template { Some(0) => CASHFLOW_GENERAL_NOTES, Some(1) => CASHFLOW_BANK_NOTES, _ => &[] }
    }
}
static MG_BALANCE_WITH_COMMON: &[Label] = &[Label::Include(MG_FINANCE_COMMON), Label::Include(MG_BALANCE_SHEET)];
static MG_INCOME_WITH_COMMON: &[Label] = &[Label::Include(MG_FINANCE_COMMON), Label::Include(MG_INCOME)];
static MG_CASHFLOW_WITH_COMMON: &[Label] = &[Label::Include(MG_FINANCE_COMMON), Label::Include(MG_CASHFLOW)];
static CASHFLOW_BANK_NOTES: &[Label] = &[Label::Pair("T088", "银行历史报表的动态扩展调节项，含义随证券及报告期变化；不能使用统一字段名。")];

fn scope_index(entry: &str) -> Option<usize> {
    match entry {
        "CWServ.tdxf10_gg_gsgk" => Some(0),
        "CWServ.tdxf10_gg_rdtc" | "CWServ.tdxf10_gg_cwfx" | "CWServ.tdxf10_gg_gszx" => Some(1),
        "CWServ.skef10_mg_fhrz" | "CWServ.skef10_mg_dsgg" => Some(1),
        value if value.starts_with("CWServ.skef10_sb_") => Some(1),
        value if value.starts_with("CWServ.skef10_mg_") || value.starts_with("CWServ.skef10_hk_") => Some(0),
        "CWServ.skef10_hk_sdfx_fxysyfx" => Some(0),
        _ => None,
    }
}
fn scalar(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) if !value.is_empty() => Some(value.clone()),
        JsonValue::Number(value) => Some(value.lexeme().to_owned()),
        JsonValue::Bool(value) => Some(if *value { "True" } else { "False" }.to_owned()),
        JsonValue::Null => Some("None".to_owned()),
        _ => None,
    }
}
fn lookup(labels: &'static [Label], field: &str) -> Option<&'static str> {
    labels.iter().rev().find_map(|item| match item {
        Label::Pair(name, label) if *name == field => Some(*label),
        Label::Include(values) => lookup(values, field),
        _ => None,
    })
}
fn append_labels(labels: &'static [Label], output: &mut Vec<(&'static str, &'static str)>) -> Result<()> {
    for item in labels {
        match item {
            Label::Include(values) => append_labels(values, output)?,
            Label::Pair(name, label) => match output.iter_mut().find(|(field, _)| field == name) {
                Some((_, existing)) => *existing = label,
                None => { output.try_reserve(1).map_err(allocation)?; output.push((name, label)); },
            },
        }
    }
    Ok(())
}
fn allocation(error: std::collections::TryReserveError) -> Error {
    Error::new(ErrorKind::LimitExceeded, "cannot reserve F10 field descriptions").with_source(error)
}
fn duplicate_suffix(field: &str) -> (&str, Option<&str>) {
    if let Some((base, suffix)) = field.rsplit_once("__") {
        let number = suffix.trim_start_matches('0');
        if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            && (number.len() > 1 || number.as_bytes().first().is_some_and(|&byte| byte >= b'2')) {
            return (base, Some(number));
        }
    }
    (field, None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldEvidence { LegacyDictionary, Unknown }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldUnit { Unknown }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDescription {
    field: String, label: Option<String>, note: Option<&'static str>, evidence: FieldEvidence,
}
impl FieldDescription {
    pub fn field(&self) -> &str { &self.field }
    pub fn label(&self) -> Option<&str> { self.label.as_deref() }
    pub fn note(&self) -> Option<&'static str> { self.note }
    pub fn evidence(&self) -> FieldEvidence { self.evidence }
    /// Field label dictionaries alone establish no numeric scale/currency.
    pub fn unit(&self) -> FieldUnit { FieldUnit::Unknown }
}
pub fn field_labels_for(context: &FieldContext) -> Result<Vec<(&'static str, &'static str)>> {
    let mut labels = Vec::new(); append_labels(context.labels(), &mut labels)?; Ok(labels)
}
pub fn field_notes_for(context: &FieldContext) -> Result<Vec<(&'static str, &'static str)>> {
    let mut notes = Vec::new(); append_labels(context.notes(), &mut notes)?; Ok(notes)
}
/// Describes an actual raw name, without treating a literal `__2` as a suffix.
pub fn describe_field(context: &FieldContext, field: &str) -> FieldDescription {
    let label = lookup(context.labels(), field);
    FieldDescription { field: field.to_owned(), label: label.map(str::to_owned), note: lookup(context.notes(), field),
        evidence: if label.is_some() { FieldEvidence::LegacyDictionary } else { FieldEvidence::Unknown } }
}
/// Explicit Python suffix projection. Source cells should use describe_field.
pub fn describe_projected_field(context: &FieldContext, field: &str) -> FieldDescription {
    let (base, duplicate) = duplicate_suffix(field);
    let label = lookup(context.labels(), field).or_else(|| lookup(context.labels(), base));
    FieldDescription { field: field.to_owned(), label: label.map(|label| match duplicate { Some(index) => format!("{label}（{index}）"), None => label.to_owned() }),
        note: lookup(context.notes(), field).or_else(|| lookup(context.notes(), base)),
        evidence: if label.is_some() { FieldEvidence::LegacyDictionary } else { FieldEvidence::Unknown } }
}
pub fn describe_fields(context: &FieldContext, fields: &[&str]) -> Result<Vec<FieldDescription>> {
    let mut output = Vec::new(); output.try_reserve(fields.len()).map_err(allocation)?;
    for field in fields { output.push(describe_field(context, field)); }
    Ok(output)
}
pub fn describe_field_notes(context: &FieldContext, fields: &[&str]) -> Result<Vec<(String, &'static str)>> {
    let mut output = Vec::new();
    for field in fields {
        let (base, _) = duplicate_suffix(field);
        if let Some(note) = lookup(context.notes(), field).or_else(|| lookup(context.notes(), base)) {
            output.try_reserve(1).map_err(allocation)?;
            output.push(((*field).to_owned(), note));
        }
    }
    Ok(output)
}
impl Response {
    /// Metadata is explicit because a report's template cannot be inferred
    /// from its ticker or the meaning of one raw field.
    pub fn field_context(&self, metadata: FieldMetadata) -> Result<FieldContext> {
        FieldContext::from_request(&EntryName::new(self.entry())?, self.request(), metadata)
    }
}
