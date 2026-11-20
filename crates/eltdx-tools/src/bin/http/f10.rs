use eltdx::f10::{self, CallQuery, EntryName, F10Client, JsonValue, RequestPayload, Response};
use eltdx::TdxClient;
use serde_json::{json, Map, Value};

use super::hq::{optional_date, with_payload};
use super::{security, Arguments, Result, RpcError};

const SIGNATURES: &[(&str, &str)] = &[
    ("call", "entry body=null ; params=null"),
    ("params", "entry"),
    ("limit_up_down_list limit_board_ladder", "start_date=null end_date=null ; include_summary=false"),
    ("stock_info business_periods topic_ids mg_name mg_profile mg_business mg_industry mg_industry_compare mg_announcements mg_topic hk_name hk_topic hk_price_chart sb_name bond_profile gz_finance qyz_guarantee qyz_rate qyz_review gzhg_info zczq_profile zczq_finance kjhz_profile kjhz_finance kjhz_guarantee bond_bulletin qh_inventory allotment_dates announcements news roadshows stock_history_trend", "code"),
    ("company_profile", "code section=:8"),
    ("business_composition", "code report_date=null"),
    ("shareholders", "code section=:count ; report_date=null kind=:000 filter1=: filter2=: page=1 page_size=20"),
    ("shareholder_change_plans", "code ; page=1 page_size=20 filter1=: filter2=:"),
    ("capital_structure", "code section=:structure ; date=null"),
    ("business_report", "code section=:business ; report_date=null"),
    ("industry_analysis", "code section=:peers ; arg=:"),
    ("trading_report", "code section=:margin ; arg=: date=null page=1 page_size=20"),
    ("fund_report", "code section=:basic ; arg=: date=null page=1 page_size=20"),
    ("mg_tips", "code section=:zxdt"),
    ("mg_finance hk_finance sb_finance bond_finance stock_finance_weights", "code section=:zcfzb"),
    ("mg_finance_indicator hk_finance_indicator", "code section=:jbzb"),
    ("mg_dividend sb_dividend dividend_financing", "code section=:fh"),
    ("mg_capital", "code section=:001 page=1"),
    ("mg_shareholders", "code section=:006 ; page=1 page_size=10"),
    ("mg_directors", "code section=:ds"),
    ("mg_market", "code section=:001 ; page=1 page_size=20"),
    ("mg_rating mg_deep_analysis hk_capital_flow sb_profile sb_tips sb_compliance sb_business sb_major_events sb_news", "code section=:001"),
    ("hk_tips hk_events hk_profile hk_executives hk_capital hk_shareholders hk_business hk_industry_compare bond_announcements", "code section=:1"),
    ("hk_related", "code section=:6"),
    ("hk_market hk_news", "code section=:3"),
    ("hk_pepb_band", "code ; sjfw=: tab_value=: hqzq=:"),
    ("hk_valuation", "code item_id=:"),
    ("hk_analyst_forecast", "code start=: end=:"),
    ("sb_events", "code section=:ydjy"),
    ("sb_capital", "code section=:gbjg"),
    ("sb_shareholders", "code section=:sdgd"),
    ("sb_industry_compare", "code section=:hyxx"),
    ("bond_rating zczq_rating kjhz_rating", "code section=:pjzst"),
    ("bond_conversion", "code section=:zgjg"),
    ("bond_rate kjhz_rate", "code section=:zqll"),
    ("gz_profile", "code section=:zqgk"),
    ("gz_interest qyz_interest kjhz_interest", "code section=:fxqk"),
    ("gz_market_data", "code section=:dzjy"),
    ("gz_rating", "code section=:dfzf"),
    ("qyz_profile", "code section=:zqzl"),
    ("qyz_rating", "code section=:zjpjzst"),
    ("gzhg_quotes", "code section=:bpzs"),
    ("kjhz_exchange", "code section=:jgtzzst"),
    ("bond_terms", "code section=:hstk"),
    ("hy_summary", "hy section=:hqtj"),
    ("hy_stage_rise", "hy flag=:e"),
    ("hy_profile hy_major_holders hy_index", "hy"),
    ("hy_commentary hy_capital_stats hy_capital_flow hy_analyst_index hy_rating_summary", "hy section=:001"),
    ("hy_news", "hy section=:hydt"),
    ("hy_members_rank", "hy section=:cwzkpm"),
    ("hy_fund_holdings", "hy section=:1"),
    ("qh_position_report", "code section=:cjcc ; date=: end_date=: index=false"),
    ("qh_basis_spread", "code section=:001 period=:1Y"),
    ("qh_warehouse_receipt", "code ; date=:"),
    ("qh_variety_info", "code section=:pzjj"),
    ("qh_index_info", "code section=:zsgk"),
    ("qh_index_valuation", "code=:000300 section=:dtpe ; start=: end=:"),
    ("qh_weights", "code=:399300 section=:zxcfg ; date=:"),
    ("market_report", "code=: section=:board_info ; arg=: date=null page=1 page_size=20"),
    ("index_members index_tracking_products", "code ; all_pages=true page_size=100 max_pages=100"),
    ("board_members", "code ; all_pages=true page_size=100 max_pages=50"),
    ("ipo_calendar news_channels", ""),
    ("allotment_details", "code date"),
    ("finance_report", "code report_type=:zcfzb"),
    ("finance_diagnosis", "code section=:zzyj scope=:"),
    ("stock_score", "code section=:pf arg=:"),
    ("profit_forecast", "code section=:ylyctj"),
    ("ranking_detail", "code section=:scpmdela"),
    ("governance", "code section=:gljy arg=:"),
    ("hot_topics", "code section=:zttzbkz"),
    ("topic_compare", "code topic_id section=:gndbzfsj sort_by=:zdf"),
    ("topic_compare_first", "code section=:gndbzfsj sort_by=:zdf"),
    ("company_news", "code section=:gsyj ; keyword=: rating=:0 page=1 page_size=20"),
    ("digest", "code section=:tips ; page=1 page_size=20"),
    ("northbound_holding", "code section=:bszj ; filter_value=: page=1 page_size=20"),
    ("detail", "detail_type record_id"),
    ("cache_list", "code kind=:gg"),
    ("market_news", "tab=3 ; page=1 page_size=50"),
    ("channel_news", "channel ; page=1 page_size=50 query1=:1 keyword=:"),
    ("theme_market", "code req_id=:200743 ; page=-1 page_size=10 zq_num=30 extra=null"),
    ("stock_stage_gain", "code ; days=60 page_size=10"),
    ("stock_main_funds stock_industry_rank stock_industry_pct_rank", "code ; days=30 page_size=10"),
    ("stock_board_member_quotes", "board_code ; board_market=1 page=-1 page_size=-1"),
    ("stock_valuation_percentile", "code kind=:0 ; page=-1 page_size=5"),
    ("stock_pepb_percentile", "code indicator=:0 ; window=:1Y period=:0 page=1 page_size=300"),
    ("valuation", "code req_id=:200191 ; page=0 page_size=20 extra=null"),
];

pub(super) fn methods() -> impl Iterator<Item = &'static str> {
    SIGNATURES
        .iter()
        .flat_map(|(names, _)| names.split_whitespace())
}

fn bind(method: &str, params: Value) -> Result<Arguments> {
    let signature = SIGNATURES
        .iter()
        .find(|(names, _)| names.split_whitespace().any(|name| name == method))
        .ok_or_else(|| RpcError::unknown(&format!("f10.{method}")))?
        .1;
    let entries: Vec<_> = signature
        .split_whitespace()
        .map(|item| {
            item.split_once('=')
                .map_or((item, None), |(key, value)| (key, Some(value)))
        })
        .collect();
    let names = entries.iter().map(|&(name, _)| name).collect::<Vec<_>>();
    let mut args = Arguments::bind(params, &names)?;
    for (name, default) in entries {
        if name == ";" || args.value(name).is_some() {
            continue;
        }
        let default = default
            .ok_or_else(|| RpcError::request(format!("missing required argument '{name}'")))?;
        let value = if let Some(text) = default.strip_prefix(':') {
            json!(text)
        } else {
            serde_json::from_str(default).map_err(|error| RpcError::internal(format!("{error}")))?
        };
        args.0.insert(name.to_owned(), value);
    }
    Ok(args)
}

pub(super) fn call(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    let sdk = client.f10();
    if method == "params" {
        if let Value::Array(mut values) = params {
            if values.is_empty() {
                return Err(RpcError::request("missing required argument 'entry'"));
            }
            let entry = values.remove(0);
            return execute(
                &sdk,
                entry
                    .as_str()
                    .ok_or_else(|| RpcError::request("entry must be a string"))?,
                json!({"Params": values}),
            );
        }
    }
    let args = bind(method, params)?;
    let options = with_payload();
    let code = || args.get::<String>("code");
    let section = args.optional::<String>("section")?.unwrap_or_default();
    let out = match method {
        "call" | "params" => {
            let entry = args.get::<String>("entry")?;
            let body = args.value("body").filter(|value| !value.is_null());
            let params = args.optional::<Vec<Value>>("params")?;
            if body.is_some() && params.is_some() {
                return Err(RpcError::value("body and params are mutually exclusive"));
            }
            return execute(
                &sdk,
                &entry,
                body.cloned().unwrap_or_else(|| {
                    params.map_or(json!({}), |params| json!({"Params": params}))
                }),
            );
        }
        "news_channels" => {
            return Ok(json!(sdk
                .news_channels()
                .iter()
                .map(|item| (item.id(), item.label()))
                .collect::<Vec<_>>()))
        }
        "ipo_calendar"
        | "index_members"
        | "index_tracking_products"
        | "board_members"
        | "limit_up_down_list"
        | "limit_board_ladder" => return collections(&sdk, method, &args),
        "business_composition" => sdk.business_composition(
            &f10::BusinessCompositionQuery::new(security(&code()?)?.into())
                .with_report_date(optional_date(&args, "report_date")?),
            &options,
        )?,
        "topic_compare_first" => sdk.topic_compare_first(
            &f10::TopicCompareFirstQuery::new(security(&code()?)?.into())
                .with_section(f10::SectionCode::new("topic_compare_first", &section)?)?
                .with_sort_by(args.get::<String>("sort_by")?)?,
            &options,
        )?,
        "cache_list" | "announcements" | "news" | "roadshows" => {
            let kind = f10::cache_alias(method)
                .map(str::to_owned)
                .map_or_else(|| args.get::<String>("kind"), Ok)?;
            sdk.cache_list(
                &f10::CacheListQuery::new(security(&code()?)?.into()).with_kind(kind)?,
                &options,
            )?
        }
        _ => {
            if let Some(response) = report(&sdk, method, &args, &section)? {
                return Ok(project(&response)?);
            }
            if let Some(body) = object_body(method, &args)? {
                return execute(
                    &sdk,
                    f10::object_entry(
                        method == "theme_market"
                            || method == "stock_board_member_quotes"
                            || f10::stage_request(method).is_some(),
                    ),
                    body,
                );
            }
            let template = f10::simple_endpoint(method)
                .or(f10::page_template(method, |name| {
                    Ok::<_, RpcError>(section == name)
                })?)
                .or(f10::input_template(method))
                .or_else(|| f10::sector_selection(method, &section))
                .or_else(|| f10::sector_dynamic(method));
            let template = match (method, template) {
                ("qh_position_report", _) => f10::SimpleEndpoint {
                    method: "qh_position_report",
                    entry: f10::position_prefix(args.get("index")?),
                    append_section: true,
                    arguments: f10::position_arguments(&section),
                    code_helper: None,
                    valid_sections: None,
                },
                ("qh_weights", _) if args.get::<String>("date")?.is_empty() => {
                    f10::sector_weights_without_date()
                }
                (_, Some(template)) => template,
                _ => return Err(RpcError::value(format!("unknown section: {section:?}"))),
            };
            return execute_template(&sdk, template, &args, &section);
        }
    };
    project(&out)
}

fn execute_template(
    sdk: &F10Client,
    template: f10::SimpleEndpoint,
    args: &Arguments,
    section: &str,
) -> Result<Value> {
    use f10::SimpleArgument::*;
    if template
        .valid_sections
        .is_some_and(|values| !values.contains(&section))
    {
        return Err(RpcError::value(format!("unknown section: {section:?}")));
    }
    let text = |name| args.get::<String>(name);
    let values = template.compose(|argument| {
        Ok::<_, RpcError>(match argument {
            Literal(value) => json!(value),
            Integer(value) => json!(value),
            Section => json!(section),
            Code => {
                let code = text(if args.value("hy").is_some() {
                    "hy"
                } else {
                    "code"
                })?;
                match template.code_helper {
                    Some("_code6") => json!(code6(&code)?),
                    Some("_us_code") => json!(f10::UsTicker::new(&code)?.as_str()),
                    Some("_hk_code") => json!(f10::HongKongCode::new(&code)?.as_str()),
                    Some("_bond_code") => json!(f10::BondCode::new(&code)?.as_str()),
                    _ => json!(code),
                }
            }
            Market => json!(format!(
                "{}",
                f10::BondCode::new(&text("code")?)?.market().get()
            )),
            MappedSection(map) => json!(map
                .value(section)
                .ok_or_else(|| RpcError::value(format!("unknown section: {section:?}")))?),
            SectionChoice(expected, yes, no) => json!(if section == expected { yes } else { no }),
            Input(name) => args.required(name)?.clone(),
            TextInput(name) => json!(python_text(args.required(name)?)),
            IntTextInput(name) => {
                let value = python_text(args.required(name)?)
                    .parse::<i64>()
                    .map_err(|_| RpcError::value(format!("invalid integer for {name}")))?;
                json!(format!("{value}"))
            }
            DateInput(name) => json!(text(name)?.trim().replace('-', "")),
        })
    })?;
    let entry = if template.append_section {
        format!("{}{section}", template.entry)
    } else {
        template.entry.to_owned()
    };
    execute(sdk, &entry, json!({"Params": values}))
}

fn code6(code: &str) -> Result<String> {
    let text = code.trim();
    if text.len() == 6 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(text.to_owned());
    }
    Ok(security(text)?.number().to_owned())
}

fn python_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        value => format!("{value}"),
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        _ => true,
    }
}

fn report(
    sdk: &F10Client,
    method: &str,
    args: &Arguments,
    section: &str,
) -> Result<Option<Response>> {
    let lookup = if method == "shareholder_change_plans" {
        "shareholders"
    } else {
        method
    };
    let definitions = f10::report_table(lookup);
    if definitions.is_empty() {
        return Ok(None);
    }
    let section = match method {
        "market_report" => f10::market_section(section),
        "shareholder_change_plans" => f10::SHAREHOLDER_PLAN_SECTION,
        _ => section,
    };
    let definition = definitions
        .iter()
        .find(|entry| entry.section == section)
        .ok_or_else(|| RpcError::value(format!("unknown section: {section:?}")))?;
    let code = args.get::<String>("code")?;
    let code = if method == "market_report" && code.is_empty() {
        code
    } else {
        code6(&code)?
    };
    let mut report_date = args
        .value("date")
        .or_else(|| args.value("report_date"))
        .cloned()
        .unwrap_or(Value::Null);
    if method == "fund_report" && !truthy(&report_date) {
        if let Some(section) = f10::fund_period_section(section) {
            let periods = execute(
                sdk,
                "CWServ.skef10_jj_tzzh",
                json!({"Params": [section, code, ""]}),
            )?;
            report_date = periods
                .pointer("/result_sets/0/row_cells/0/0/value")
                .cloned()
                .unwrap_or_default();
        }
    }
    let values = definition.compose(|argument| {
        use f10::ReportArgument::*;
        let input = |name| -> Result<Value> {
            match name {
                "date" => Ok(if report_date.is_null() {
                    json!("")
                } else if method == "business_report" {
                    json!(python_text(&report_date))
                } else {
                    report_date.clone()
                }),
                "page" | "page_size" => Ok(json!(python_text(args.required(name)?))),
                _ => Ok(args.required(name)?.clone()),
            }
        };
        Ok::<_, RpcError>(match argument {
            Code => json!(code),
            Literal(value) => json!(value),
            Input(name) => input(name)?,
            OrInput(name, default) => {
                let value = input(name)?;
                if truthy(&value) {
                    value
                } else {
                    json!(default)
                }
            }
        })
    })?;
    Ok(Some(query(
        sdk,
        definition.entry,
        json!({"Params": values}),
    )?))
}

fn object_body(method: &str, args: &Arguments) -> Result<Option<Value>> {
    use f10::{ObjectArgument as Slot, ObjectPart as Part};
    let stage = f10::stage_request(method);
    let parts: &[Part] = match method {
        "theme_market"
            if f10::THEME_WINDOW_REQUESTS
                .contains(&python_text(args.required("req_id")?).as_str()) =>
        {
            &[Part::Theme, Part::Window]
        }
        "theme_market" => &[Part::Theme],
        "stock_board_member_quotes" => &[Part::Board],
        "stock_valuation_percentile" => &[Part::Range, Part::ValuationModule, Part::Percentile],
        "stock_pepb_percentile" => &[Part::Series, Part::ValuationModule],
        "stock_history_trend" => &[Part::History, Part::ValuationModule],
        "valuation" => &[Part::Range, Part::ValuationModule],
        _ if stage.is_some() => &[Part::Theme, Part::Window],
        _ => return Ok(None),
    };
    let board = method == "stock_board_member_quotes";
    let code = if board {
        args.get::<String>("board_code")?
    } else {
        args.get::<String>("code")?
    };
    let id = if board { None } else { Some(security(&code)?) };
    let market = if board {
        args.required("board_market")?.clone()
    } else {
        json!(id.as_ref().map(|id| super::quote::market_id(id.market())))
    };
    let req_id = match method {
        "stock_valuation_percentile" => json!("200191"),
        "stock_pepb_percentile" => json!("200191"),
        "stock_history_trend" => json!("200192"),
        _ if stage.is_some() => json!(stage),
        _ => json!(python_text(args.required("req_id").unwrap_or(&Value::Null))),
    };
    let mut body = Map::new();
    for &part in parts {
        for (name, value) in f10::object_part(part, |slot| {
            Ok::<_, RpcError>(match slot {
                Slot::Literal(value) => json!(value),
                Slot::Integer(value) => json!(value),
                Slot::Input(name) => match name {
                    "req_id" => req_id.clone(),
                    "market" => market.clone(),
                    "code" => json!(id.as_ref().map_or(code.as_str(), |id| id.number())),
                    "full_code" => json!(id.as_ref().map(|id| format!("{id}"))),
                    "page" if stage.is_some() => json!(-1),
                    "zq_num" if stage.is_some() => args.required("days")?.clone(),
                    "adjusted_page" => args.required("page")?.clone(),
                    "adjusted_size" => args.required("page_size")?.clone(),
                    name => args.required(name)?.clone(),
                },
            })
        })? {
            body.insert(name.to_owned(), value);
        }
    }
    if let Some(extra) = args.optional::<Map<String, Value>>("extra")? {
        body.extend(extra);
    }
    Ok(Some(Value::Object(body)))
}

fn execute(sdk: &F10Client, entry: &str, body: Value) -> Result<Value> {
    project(&query(sdk, entry, body)?)
}

fn query(sdk: &F10Client, entry: &str, body: Value) -> Result<Response> {
    let body = serde_json::to_vec(&body).map_err(|error| RpcError::internal(format!("{error}")))?;
    let query = CallQuery::new(
        EntryName::new(entry)?,
        RequestPayload::Body(JsonValue::parse(&body)?),
    );
    Ok(sdk.call(&query, &with_payload())?)
}

pub(super) fn json_value(value: &JsonValue) -> Result<Value> {
    serde_json::from_slice(&value.to_json_bytes()?)
        .map_err(|error| RpcError::internal(format!("{error}")))
}

pub(super) fn project_row(row: &f10::Row) -> Result<Value> {
    row.python_projection()
        .into_iter()
        .map(|(name, value)| Ok((name, json_value(value)?)))
        .collect::<Result<Map<_, _>>>()
        .map(Value::Object)
}

pub(super) fn project(response: &Response) -> Result<Value> {
    let body = match response.request() {
        RequestPayload::Body(body) => json_value(body)?,
        RequestPayload::Params(values) => {
            json!({"Params": values.iter().map(json_value).collect::<Result<Vec<_>>>()?})
        }
    };
    let mut raw = response
        .meta()
        .raw()
        .and_then(|data| data.payloads().first())
        .and_then(|payload| serde_json::from_slice::<Value>(payload).ok());
    let mut tables = Vec::new();
    let mut raw_tables = Vec::new();
    for (index, table) in response.result_sets().iter().enumerate() {
        let rows = table
            .rows()
            .iter()
            .map(project_row)
            .collect::<Result<Vec<_>>>()?;
        let cells = table.rows().iter().map(|row| row.cells().iter().map(|cell| {
            Ok(json!({"name": cell.name(), "value": json_value(cell.value())?, "index": cell.index()}))
        }).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
        let page = table.raw().map(json_value).transpose()?.unwrap_or_default();
        tables.push(json!({"key": table.key().filter(|key| !key.is_empty()).map_or_else(|| format!("table{index}"), str::to_owned),
            "columns": table.columns(), "rows": rows, "row_cells": cells, "raw": page}));
        raw_tables.push(page);
    }
    if raw.is_none() {
        let mut attributes = response
            .attributes()
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_value(value)?)))
            .collect::<Result<Map<_, _>>>()?;
        if let Some(error) = response.error_code() {
            attributes.insert("ErrorCode".to_owned(), json!(error));
        }
        if !raw_tables.is_empty() {
            attributes.insert("ResultSets".to_owned(), json!(raw_tables));
        }
        raw = Some(Value::Object(attributes));
    }
    Ok(
        json!({"entry": response.entry(), "request_body": body, "error_code": response.error_code(),
        "result_sets": tables, "raw": raw}),
    )
}

fn collections(sdk: &F10Client, method: &str, args: &Arguments) -> Result<Value> {
    use std::num::NonZeroU32;
    let options = with_payload();
    if matches!(method, "limit_up_down_list" | "limit_board_ladder") {
        let query = f10::LimitBoardLadderQuery::new()
            .with_start_date(optional_date(args, "start_date")?)?
            .with_end_date(optional_date(args, "end_date")?)?
            .with_include_summary(args.get("include_summary")?);
        let ladder = sdk.limit_board_ladder(&query, &options)?;
        let detail = project(ladder.detail_response())?;
        let rows = ladder.rows().iter().map(|row| {
            let field = |value: Option<&JsonValue>| value.map(json_value).transpose();
            Ok(json!({"trading_date": field(row.trading_date())?, "trading_date_value": field(row.trading_date_value())?,
                "board_level": row.board_level(), "code": row.code(), "market_id": row.market().map(|market| market.get()),
                "limit_reason": field(row.limit_reason())?, "seal_amount": row.seal_amount(), "name": field(row.name())?,
                "limit_reason_extra": field(row.limit_reason_extra())?, "limit_time": field(row.limit_time())?,
                "broken_count": row.broken_count(), "industry": field(row.industry())?, "limit_type": row.limit_type(),
                "success_rate": field(row.success_rate())?, "highest_board_level": row.highest_board_level(), "raw": project_row(row.raw())?}))
        }).collect::<Result<Vec<_>>>()?;
        let summary = ladder
            .summary()
            .iter()
            .map(|row| project_row(row.raw()))
            .collect::<Result<Vec<_>>>()?;
        return Ok(
            json!({"entry": ladder.entry(), "request_body": detail["request_body"], "error_code": ladder.error_code(),
            "start_date": format!("{}", super::hq::date_number(ladder.start_date())),
            "end_date": format!("{}", super::hq::date_number(ladder.end_date())),
            "rows": rows, "summary": summary, "raw": detail["raw"]}),
        );
    }
    if method == "ipo_calendar" {
        let calendar = sdk.ipo_calendar(&options)?;
        return Ok(
            json!({"rows": calendar.rows().iter().map(|row| project_row(row.raw())).collect::<Result<Vec<_>>>()?}),
        );
    }
    let code = f10::AshareCode::from(security(&args.get::<String>("code")?)?);
    let page_size = NonZeroU32::new(args.get("page_size")?)
        .ok_or_else(|| RpcError::value("page_size must be positive"))?;
    let max_pages = NonZeroU32::new(args.get("max_pages")?)
        .ok_or_else(|| RpcError::value("max_pages must be positive"))?;
    let all = args.get("all_pages")?;
    let code_value = json!(code.as_str());
    Ok(match method {
        "index_members" => {
            let query = f10::IndexMembersQuery::new(code)
                .with_all_pages(all)
                .with_page_size(page_size)?
                .with_max_pages(max_pages)?;
            let table = sdk.index_members(&query, &options)?;
            let rows = table.rows().iter().map(|row| {
                let market = row.market().map(|market| market.get());
                Ok(json!({"code": row.code(), "name": row.name(), "market_id": market,
                    "join_date": row.join_date().map(json_value).transpose()?, "raw": project_row(row.raw())?,
                    "market": market_name(market), "full_code": row.full_code()}))
            }).collect::<Result<Vec<_>>>()?;
            json!({"index_code": code_value, "total": table.total(), "rows": rows})
        }
        "index_tracking_products" => {
            let query = f10::IndexTrackingProductsQuery::new(code)
                .with_all_pages(all)
                .with_page_size(page_size)?
                .with_max_pages(max_pages)?;
            let table = sdk.index_tracking_products(&query, &options)?;
            json!({"index_code": code_value, "total": table.total(),
                "rows": table.rows().iter().map(|row| project_row(row.raw())).collect::<Result<Vec<_>>>()?})
        }
        "board_members" => {
            let query = f10::BoardMembersQuery::new(code)
                .with_all_pages(all)
                .with_page_size(page_size)?
                .with_max_pages(max_pages)?;
            let table = sdk.board_members(&query, &options)?;
            json!({"board_code": code_value, "total": table.total(),
                "rows": table.rows().iter().map(|row| project_row(row.raw())).collect::<Result<Vec<_>>>()?})
        }
        _ => return Err(RpcError::internal("unregistered F10 collection")),
    })
}

fn market_name(market: Option<u16>) -> Option<&'static str> {
    match market {
        Some(0) => Some("sz"),
        Some(1) => Some("sh"),
        Some(2) => Some("bj"),
        _ => None,
    }
}
