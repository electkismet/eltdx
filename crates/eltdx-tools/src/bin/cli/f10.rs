use std::num::NonZeroU32;

use eltdx::f10::{self, F10Client, F10MarketCode, JsonValue};
use eltdx::services::MoneyFlowHistoryQuery;
use eltdx::{SecurityId, TdxClient};
use eltdx_tools::rpc;
use serde_json::{json, Value};

use super::args::{F10Action, F10Connection, Index, IndexAction, F10};
use super::{codes, finish, invalid, operation, single_code, timeout, Result};

fn with_client(
    common: F10Connection,
    run: impl FnOnce(&TdxClient, &eltdx::OperationOptions) -> Result<Value>,
) -> Result<Value> {
    let duration = timeout(common.timeout)?;
    let mut builder = F10Client::builder().with_timeout(duration);
    if let Some(url) = common.base_url.filter(|url| !url.is_empty()) {
        builder = builder.with_base_url(url);
    }
    let client = TdxClient::builder().f10(builder).build()?;
    finish(run(&client, &operation(duration, true)), client.close())
}

pub(super) fn run(args: F10) -> Result<Value> {
    use F10Action::*;
    let action = args.action;
    if action == Sections {
        return Ok(json!(include_str!("f10-sections.tsv").lines().flat_map(|line| {
            let mut parts = line.split('\t');
            let action = parts.next().unwrap_or_default();
            let default = parts.next().unwrap_or_default();
            parts.filter_map(move |pair| pair.split_once('=').map(|(section, label)| {
                json!({"action": action, "section": section, "default": section == default, "说明": label})
            }))
        }).collect::<Vec<_>>()));
    }
    let raw = args
        .params
        .as_deref()
        .filter(|text| !text.is_empty())
        .map(serde_json::from_str::<Value>)
        .transpose()?
        .unwrap_or_else(|| json!([]));
    let code_values = codes(&args.code);
    let section = args
        .section
        .as_deref()
        .filter(|section| !section.is_empty());
    let arg = args.arg.as_deref().unwrap_or_default();
    let option_number = |value: Option<i64>| -> Result<Option<i64>> {
        Ok(match value {
            Some(value) => Some(value),
            None if !arg.is_empty() => Some(arg.parse()?),
            None => None,
        })
    };
    if action == BoardMembers {
        let code = f10::AshareCode::from(single_code(&code_values)?.parse::<eltdx::SecurityId>()?);
        let size = page_size(args.page_size)?;
        return with_client(args.common, |client, options| {
            let query = f10::BoardMembersQuery::new(code).with_page_size(size)?;
            let table = client.f10().board_members(&query, options)?;
            Ok(json!(table.rows().iter().map(|row| Ok(json!({
                "code": row.code(), "full_code": row.full_code(), "name": row.name(),
                "dividend_plan": row.dividend_plan(), "progress": row.progress(),
                "announce_date": field(row.announce_date())?, "record_date": field(row.record_date())?,
                "ex_date": field(row.ex_date())?, "report_period": field(row.report_period())?,
            }))).collect::<Result<Vec<_>>>()?))
        });
    }
    let (method, mut params) = match action {
        Call => (
            "call",
            json!({"entry": args.entry.filter(|entry| !entry.is_empty()).ok_or_else(|| invalid("f10 call 需要 --entry"))?, "params": raw}),
        ),
        Detail => match code_values.as_slice() {
            [kind, id] => ("detail", json!({"detail_type": kind, "record_id": id})),
            _ => return Err(invalid("f10 detail 需要 detail_type 与 record_id")),
        },
        LimitBoard => (
            "limit_up_down_list",
            json!({"start_date": args.date.filter(|date| !date.is_empty())}),
        ),
        NewsChannels => ("news_channels", json!({})),
        MarketNews => (
            "market_news",
            json!({"tab": option_number(args.tab)?.unwrap_or(3), "page": args.page, "page_size": args.page_size}),
        ),
        ChannelNews => (
            "channel_news",
            json!({"channel": option_number(args.channel)?.ok_or_else(|| invalid("f10 channel-news 需要 --channel"))?, "page": args.page, "page_size": args.page_size}),
        ),
        Market => (
            "market_report",
            json!({"code": code_values.first().copied().unwrap_or_default(), "arg": arg, "date": args.date, "page": args.page, "page_size": args.page_size}),
        ),
        _ => {
            let code = single_code(&code_values)?;
            match action {
                StockInfo => ("stock_info", json!({"code": code})),
                Profile => ("company_profile", json!({"code": code})),
                Digest => ("digest", json!({"code": code, "page_size": args.page_size})),
                Shareholders => (
                    "shareholders",
                    json!({"code": code, "report_date": args.report_date, "kind": if arg.is_empty() { "000" } else { arg }, "page": args.page, "page_size": args.page_size}),
                ),
                Capital => (
                    "capital_structure",
                    json!({"code": code, "date": args.date}),
                ),
                Business => (
                    "business_report",
                    json!({"code": code, "report_date": args.report_date}),
                ),
                Industry => ("industry_analysis", json!({"code": code, "arg": arg})),
                Trading => (
                    "trading_report",
                    json!({"code": code, "arg": arg, "date": args.date, "page": args.page, "page_size": args.page_size}),
                ),
                Dividend => ("dividend_financing", json!({"code": code})),
                Finance => ("finance_report", json!({"code": code})),
                Governance => ("governance", json!({"code": code, "arg": arg})),
                Forecast => ("profit_forecast", json!({"code": code})),
                Score => ("stock_score", json!({"code": code, "arg": arg})),
                Topics => ("hot_topics", json!({"code": code})),
                TopicCompare => match args.topic.filter(|topic| !topic.is_empty()) {
                    Some(topic) => ("topic_compare", json!({"code": code, "topic_id": topic})),
                    None => ("topic_compare_first", json!({"code": code})),
                },
                News => (
                    "company_news",
                    json!({"code": code, "page": args.page, "page_size": args.page_size}),
                ),
                Northbound => (
                    "northbound_holding",
                    json!({"code": code, "page": args.page, "page_size": args.page_size}),
                ),
                Ranking => ("ranking_detail", json!({"code": code})),
                Valuation => ("valuation", json!({"code": code})),
                Fund => (
                    "fund_report",
                    json!({"code": code, "arg": arg, "date": args.date, "page": args.page, "page_size": args.page_size}),
                ),
                _ => unreachable!(),
            }
        }
    };
    if let Some(section) = section.or((action == Governance).then_some("wgcl")) {
        match action {
            Finance => params["report_type"] = json!(section),
            Profile | Digest | Shareholders | Capital | Business | Industry | Trading
            | Dividend | Governance | Forecast | Score | Topics | News | Northbound | Ranking
            | Fund | Market => params["section"] = json!(section),
            _ => {}
        }
    }
    with_client(args.common, |client, _| {
        let result = rpc::call(client, &format!("f10.{method}"), params)?;
        if action == NewsChannels {
            let rows = result
                .as_array()
                .ok_or_else(|| invalid("无效新闻频道列表"))?;
            return Ok(json!(rows
                .iter()
                .map(|row| json!({"channel": row[0], "name": row[1]}))
                .collect::<Vec<_>>()));
        }
        Ok(result)
    })
}

fn page_size(size: i64) -> Result<NonZeroU32> {
    NonZeroU32::new(u32::try_from(size)?).ok_or_else(|| invalid("page-size 必须大于 0"))
}

fn field(value: Option<&JsonValue>) -> Result<Value> {
    Ok(match value {
        Some(value) => serde_json::from_slice(&value.to_json_bytes()?)?,
        None => Value::Null,
    })
}

fn market(value: Option<F10MarketCode>) -> Option<&'static str> {
    match value.map(F10MarketCode::get) {
        Some(0) => Some("sz"),
        Some(1) => Some("sh"),
        Some(2) => Some("bj"),
        _ => None,
    }
}

fn rounded(value: f64) -> f64 {
    (value * 10_000.0).round_ties_even() / 10_000.0
}

pub(super) fn index(args: Index) -> Result<Value> {
    let code =
        f10::AshareCode::from(single_code(&codes(&args.code))?.parse::<eltdx::SecurityId>()?);
    let size = page_size(args.page_size)?;
    with_client(args.common, |client, options| {
        let sdk = client.f10();
        let rows = match args.action {
            IndexAction::Members => {
                let query = f10::IndexMembersQuery::new(code)
                    .with_all_pages(!args.first_page)
                    .with_page_size(size)?;
                sdk.index_members(&query, options)?
                    .rows()
                    .iter()
                    .map(|row| {
                        Ok(json!({
                            "code": row.code(), "name": row.name(), "full_code": row.full_code(),
                            "market": market(row.market()), "join_date": field(row.join_date())?,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            IndexAction::Etfs => {
                let query = f10::IndexTrackingProductsQuery::new(code)
                    .with_all_pages(!args.first_page)
                    .with_page_size(size)?;
                sdk.index_tracking_products(&query, options)?.rows().iter().filter(|row| {
                    !args.etf_only || row.code().is_some_and(|code| ["51", "56", "58", "15", "16"].iter().any(|prefix| code.starts_with(prefix)))
                }).map(|row| Ok(json!({
                    "code": row.code(), "name": row.name(), "full_code": row.full_code(), "market": market(row.market()),
                    "nav": row.nav(), "change_pct": row.change_pct(), "scale_yi": row.scale_yuan().map(|value| rounded(value / 1e8)),
                    "amount_yuan": row.amount_yuan(), "inception_date": field(row.inception_date())?,
                    "manager": row.manager(), "company": row.company(), "fee_rate_pct": row.fee_rate_pct(),
                }))).collect::<Result<Vec<_>>>()?
            }
        };
        Ok(json!(rows))
    })
}

pub(super) fn ipo(common: F10Connection) -> Result<Value> {
    with_client(common, |client, options| {
        let calendar = client.f10().ipo_calendar(options)?;
        Ok(json!(calendar.rows().iter().map(|row| Ok(json!({
            "apply_date": field(row.apply_date())?, "code": row.code(), "name": row.name(),
            "full_code": row.full_code(), "market": market(row.market()), "issue_price": row.issue_price(),
            "raise_amount": row.raise_amount(), "raise_amount_yi": row.raise_amount_yi().map(rounded),
            "issue_pe": row.issue_pe(), "extra_pct": row.extra_pct(), "apply_limit": row.apply_limit(),
        }))).collect::<Result<Vec<_>>>()?))
    })
}

pub(super) fn money_flow_history(
    ids: &[SecurityId],
    days: u32,
    seconds: f64,
    include_raw: bool,
) -> Result<super::output::Value> {
    let duration = timeout(seconds)?;
    let client = TdxClient::builder()
        .f10(F10Client::builder().with_timeout(duration))
        .build()?;
    let result = (|| {
        let sdk = client.f10().money_flow_history();
        let options = operation(duration, include_raw);
        let mut output = serde_json::Map::new();
        for id in ids {
            let query = MoneyFlowHistoryQuery::new(id.clone().into()).with_days(days)?;
            let series = sdk.history(&query, &options)?;
            let rows = series
                .rows()
                .iter()
                .map(|row| {
                    row.raw()
                        .python_projection()
                        .into_iter()
                        .map(|(name, value)| Ok((name, field(Some(value))?)))
                        .collect::<Result<serde_json::Map<_, _>>>()
                })
                .collect::<Result<Vec<_>>>()?;
            let code = format!("{id}");
            let raw = series
                .meta()
                .raw()
                .and_then(|data| data.payloads().first())
                .map(|bytes| super::hex(bytes))
                .unwrap_or_default();
            let value = json!({"code": code, "days": days, "codes": [id.number()], "rows": rows, "raw_payload": raw});
            if ids.len() == 1 {
                return Ok(super::output::scalar(value));
            }
            output.insert(code, value);
        }
        Ok(super::output::scalar(Value::Object(output)))
    })();
    finish(result, client.close())
}
