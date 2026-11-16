//! Ordered object fields for the theme and valuation request families.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum ObjectPart { Theme, Window, Board, Range, Percentile, History, Series, ThemeModule, ValuationModule }

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum ObjectArgument { Literal(&'static str), Integer(i64), Input(&'static str) }

#[doc(hidden)]
pub fn object_entry(theme: bool) -> &'static str {
    if theme { "HQServ.hq_nlp_tcihq" } else { "HQServ.hq_nlp_gpsj" }
}

#[doc(hidden)]
pub fn object_fields(part: ObjectPart) -> &'static [(&'static str, ObjectArgument)] {
    use ObjectArgument::{Input as I, Integer as N, Literal as L};
    match part {
        ObjectPart::Theme => &[("ReqId", I("req_id")), ("setcode", I("market")), ("code", I("code")),
            ("Page", I("page")), ("PageSize", I("page_size")), ("modname", L("mod_tcihq.dll"))],
        ObjectPart::Window => &[("zq_num", I("zq_num"))],
        ObjectPart::Board => &[("ReqId", L("200744")), ("setcode", I("market")), ("code", I("code")),
            ("Page", I("page")), ("PageSize", I("page_size")), ("modname", L("mod_tcihq.dll"))],
        ObjectPart::Range => &[("ReqId", I("req_id")), ("Code", I("full_code")),
            ("BeginDate", L("0")), ("EndDate", L("0")), ("Page", I("page")), ("PageSize", I("page_size"))],
        ObjectPart::Percentile => &[("Type", I("kind")), ("Page", I("adjusted_page")), ("PageSize", I("adjusted_size"))],
        ObjectPart::History => &[("ReqId", I("req_id")), ("code", I("code")), ("setcode", I("market"))],
        ObjectPart::Series => &[("ReqId", I("req_id")), ("code", I("code")), ("setcode", I("market")),
            ("sjfw", I("window")), ("sdate", L("")), ("edate", L("")), ("zb", I("indicator")),
            ("hqzq", I("period")), ("cwppgz", N(1)), ("jglx", N(0)),
            ("Page", I("page")), ("PageSize", I("page_size"))],
        ObjectPart::ThemeModule => &[("modname", L("mod_tcihq.dll"))],
        ObjectPart::ValuationModule => &[("modname", L("mod_gpsj.dll"))],
    }
}

#[doc(hidden)]
pub fn object_part<T, E>(part: ObjectPart, mut value: impl FnMut(ObjectArgument) -> Result<T, E>)
    -> Result<Vec<(&'static str, T)>, E>
{
    object_fields(part).iter().map(|&(name, slot)| Ok((name, value(slot)?))).collect()
}

#[doc(hidden)]
pub const THEME_WINDOW_REQUESTS: &[&str] = &["200740", "200742", "200745", "200747"];

#[doc(hidden)]
pub fn stage_request(method: &str) -> Option<&'static str> {
    match method { "stock_stage_gain" => Some("200740"), "stock_main_funds" => Some("200742"),
        "stock_industry_rank" => Some("200745"), "stock_industry_pct_rank" => Some("200747"), _ => None }
}

#[doc(hidden)]
pub const VALUATION_RANGE_REQUESTS: &[&str] = &["200191", "200192"];

pub(super) fn native_part(part: ObjectPart, mut input: impl FnMut(&str) -> crate::f10::JsonValue)
    -> Vec<(String, crate::f10::JsonValue)>
{
    let result = object_part(part, |slot| Ok::<_, std::convert::Infallible>(match slot {
        ObjectArgument::Literal(value) => value.into(), ObjectArgument::Integer(value) => value.into(),
        ObjectArgument::Input(name) => input(name),
    }));
    match result { Ok(values) => values.into_iter().map(|(name, value)| (name.to_owned(), value)).collect(),
        Err(impossible) => match impossible {} }
}
