use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::num::NonZeroU32;
use std::time::Duration;

use eltdx::f10::fields::{describe_projected_field, FieldMetadata};
use eltdx::f10::*;
use eltdx::helpers::{StockTopicsQuery, TopicStocksQuery};
use eltdx::mac::ex_query::{MacroBarsQuery, MacroCode, MacroQuotesQuery};
use eltdx::mac::{bitmap, MacExClient, MacPeriod, Quote};
use eltdx::{OperationOptions, RawPolicy, SecurityId, TdxClient, TradingDate};
use serde_json::{json, Map, Value};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) fn call(name: &str, args: &Value, options: &OperationOptions) -> Result<Value> {
    match name {
        "stock_topics" | "topic_stocks" => return topics(name, args, options),
        "macro_indicators" | "macro_data" => return macro_tool(name, args, options),
        _ => {}
    }

    let client = F10Client::builder().build()?;
    let options = options.clone().with_raw(RawPolicy::RetainPayload);
    let result = f10_tool(&client, name, args, &options);
    let closed = client.close();
    let value = result?;
    closed?;
    Ok(value)
}

fn f10_tool(
    client: &F10Client,
    name: &str,
    args: &Value,
    options: &OperationOptions,
) -> Result<Value> {
    let stock = || ashare(required_text(args, "code")?);
    let response = match name {
        "news_channels" => {
            return Ok(Value::Array(
                client
                    .news_channels()
                    .iter()
                    .map(|channel| json!({"channel": channel.id(), "name": channel.label()}))
                    .collect(),
            ));
        }
        "limit_up_down_list" | "limit_board_ladder" => {
            let query = LimitUpDownListQuery::new()
                .with_dates(date_arg(args, "start_date")?, date_arg(args, "end_date")?)?
                .with_include_summary(args["include_summary"].as_bool().unwrap_or(false));
            return Ok(ladder_json(&client.limit_up_down_list(&query, options)?));
        }
        "company_profile" => client.company_profile(
            &CompanyProfileQuery::new(stock()?).with_section(section(
                args,
                "company_profile",
                "8",
            )?)?,
            options,
        )?,
        "hot_topics" => client.hot_topics(
            &HotTopicsQuery::new(stock()?).with_section(section(args, "hot_topics", "zttzbkz")?)?,
            options,
        )?,
        "finance_report" => client.finance_report(
            &FinanceReportQuery::new(stock()?).with_report_type(text(
                args,
                "report_type",
                "zcfzb",
            ))?,
            options,
        )?,
        "f10_digest" => {
            let (page, size) = pagination(args, 20)?;
            let query = DigestQuery::new(stock()?)
                .with_section(section(args, "digest", "tips")?)?
                .with_page(page)?
                .with_page_size(size)?;
            client.digest(&query, options)?
        }
        "company_news" => {
            let (page, size) = pagination(args, 20)?;
            let query = CompanyNewsQuery::new(stock()?)
                .with_section(section(args, "company_news", "gsyj")?)?
                .with_keyword(text(args, "keyword", ""))?
                .with_rating(scalar_arg(args, "rating")?.unwrap_or_else(|| "0".into()))?
                .with_page(page)?
                .with_page_size(size)?;
            client.company_news(&query, options)?
        }
        "market_news" => {
            let (page, size) = pagination(args, 50)?;
            let tab = selector(args, "tab", Some(3))?;
            let query = MarketNewsQuery::new()
                .with_tab(NewsHomeTab::from_wire(tab))
                .with_page(page)?
                .with_page_size(size)?;
            client.market_news(&query, options)?
        }
        "channel_news" => {
            let (page, size) = pagination(args, 50)?;
            let channel = selector(args, "channel", None)?;
            let query = ChannelNewsQuery::new(NewsChannel::from_wire(channel))
                .with_page(page)?
                .with_page_size(size)?;
            client.channel_news(&query, options)?
        }
        "us_stock_f10" => {
            let code = UsTicker::new(international_code(required_text(args, "code")?))?;
            match text(args, "section", "profile") {
                "profile" => client.mg_profile(&MgProfileQuery::new(code), options)?,
                "finance" => client.mg_finance(&MgFinanceQuery::new(code), options)?,
                "business" => client.mg_business(&MgBusinessQuery::new(code), options)?,
                "rating" => client.mg_rating(&MgRatingQuery::new(code), options)?,
                "announcements" => {
                    client.mg_announcements(&MgAnnouncementsQuery::new(code), options)?
                }
                unknown => return Err(format!("unknown section: {unknown:?}").into()),
            }
        }
        "hk_stock_f10" => {
            let code = HongKongCode::from_wire(international_code(required_text(args, "code")?))?;
            match text(args, "section", "profile") {
                "profile" => client.hk_profile(&HkProfileQuery::new(code), options)?,
                "finance" => client.hk_finance(&HkFinanceQuery::new(code), options)?,
                "capital" => client.hk_capital(&HkCapitalQuery::new(code), options)?,
                "news" => client.hk_news(&HkNewsQuery::new(code), options)?,
                "capital_flow" => {
                    client.hk_capital_flow(&HkCapitalFlowQuery::new(code), options)?
                }
                unknown => return Err(format!("unknown section: {unknown:?}").into()),
            }
        }
        "bond_f10" => bond(client, args, options)?,
        "industry_f10" => {
            let code = IndustryCode::new(required_text(args, "industry_code")?)?;
            match text(args, "section", "summary") {
                "summary" => client.hy_summary(&HySummaryQuery::new(code), options)?,
                "profile" => client.hy_profile(&HyProfileQuery::new(code), options)?,
                "news" => client.hy_news(&HyNewsQuery::new(code), options)?,
                "members" => client.hy_members_rank(&HyMembersRankQuery::new(code), options)?,
                "capital_flow" => {
                    client.hy_capital_flow(&HyCapitalFlowQuery::new(code), options)?
                }
                "index" => client.hy_index(&HyIndexQuery::new(code), options)?,
                unknown => return Err(format!("unknown section: {unknown:?}").into()),
            }
        }
        "stock_analysis" => {
            let code = stock()?;
            match text(args, "section", "industry_rank") {
                "industry_rank" => {
                    client.stock_industry_rank(&StockIndustryRankQuery::new(code), options)?
                }
                "stage_gain" => {
                    client.stock_stage_gain(&StockStageGainQuery::new(code), options)?
                }
                "main_funds" => {
                    client.stock_main_funds(&StockMainFundsQuery::new(code), options)?
                }
                "pepb_percentile" => {
                    client.stock_pepb_percentile(&StockPepbPercentileQuery::new(code), options)?
                }
                "valuation_percentile" => client.stock_valuation_percentile(
                    &StockValuationPercentileQuery::new(code),
                    options,
                )?,
                "history_trend" => {
                    client.stock_history_trend(&StockHistoryTrendQuery::new(code), options)?
                }
                unknown => return Err(format!("unknown section: {unknown:?}").into()),
            }
        }
        unknown => return Err(format!("unknown F10 tool: {unknown:?}").into()),
    };
    response_json(&response)
}

fn bond(client: &F10Client, args: &Value, options: &OperationOptions) -> Result<Response> {
    let kind = text(args, "kind", "convertible");
    if !matches!(
        kind,
        "convertible" | "treasury" | "corporate" | "repo" | "exchange" | "abs"
    ) {
        return Err(format!("unknown bond kind: {kind:?}").into());
    }
    let code = BondCode::new(required_text(args, "code")?)?;
    let section = text(args, "section", "profile");
    let response = match (kind, section) {
        ("convertible", "profile") => client.bond_profile(&BondProfileQuery::new(code), options),
        ("treasury", "profile") => client.gz_profile(&GzProfileQuery::new(code), options),
        ("corporate", "profile") => client.qyz_profile(&QyzProfileQuery::new(code), options),
        ("repo", "profile") => client.gzhg_info(&GzhgInfoQuery::new(code), options),
        ("exchange", "profile") => client.kjhz_profile(&KjhzProfileQuery::new(code), options),
        ("abs", "profile") => client.zczq_profile(&ZczqProfileQuery::new(code), options),
        ("convertible", "finance") => client.bond_finance(&BondFinanceQuery::new(code), options),
        ("treasury", "finance") => client.gz_finance(&GzFinanceQuery::new(code), options),
        ("corporate", "finance") => client.qyz_review(&QyzReviewQuery::new(code), options),
        ("exchange", "finance") => client.kjhz_finance(&KjhzFinanceQuery::new(code), options),
        ("abs", "finance") => client.zczq_finance(&ZczqFinanceQuery::new(code), options),
        ("convertible", "rating") => client.bond_rating(&BondRatingQuery::new(code), options),
        ("treasury", "rating") => client.gz_rating(&GzRatingQuery::new(code), options),
        ("corporate", "rating") => client.qyz_rating(&QyzRatingQuery::new(code), options),
        ("exchange", "rating") => client.kjhz_rating(&KjhzRatingQuery::new(code), options),
        ("abs", "rating") => client.zczq_rating(&ZczqRatingQuery::new(code), options),
        ("treasury", "interest") => client.gz_interest(&GzInterestQuery::new(code), options),
        ("corporate", "interest") => client.qyz_interest(&QyzInterestQuery::new(code), options),
        ("exchange", "interest") => client.kjhz_interest(&KjhzInterestQuery::new(code), options),
        (_, "terms") => client.bond_terms(&BondTermsQuery::new(code), options),
        (_, "announcements") => client.bond_bulletin(&BondBulletinQuery::new(code), options),
        _ => {
            return Err(format!("unsupported bond kind and section: {kind:?}, {section:?}").into());
        }
    }?;
    Ok(response)
}

fn topics(name: &str, args: &Value, options: &OperationOptions) -> Result<Value> {
    let client = TdxClient::builder().build()?;
    let helpers = client.helpers();
    let result = (|| -> Result<Value> {
        if name == "stock_topics" {
            let code = security(required_text(args, "code")?)?;
            let query = StockTopicsQuery::new(code.clone().into());
            let result = helpers.stock_topics(&query, options)?;
            let topics: Vec<_> = result
                .topics()
                .iter()
                .map(|topic| {
                    let sources: Map<_, _> = topic
                        .raw()
                        .iter()
                        .map(|(source, row)| (source.as_str().to_owned(), row_json(row)))
                        .collect();
                    let mut names: Vec<_> = sources.keys().map(String::as_str).collect();
                    names.sort_unstable();
                    json!({
                        "topic_id": topic.topic_id(),
                        "topic_name": topic.topic_name(),
                        "relation_level": topic.relation_level(),
                        "selected_date": optional_json(topic.selected_date()),
                        "topic_date": optional_json(topic.topic_date()),
                        "reason": topic.reason(),
                        "detail_id": topic.detail_id(),
                        "category_raw": optional_json(topic.category_raw()),
                        "source": names.join("+"),
                        "raw": sources,
                    })
                })
                .collect();
            return Ok(json!({"code": code.to_string(), "topics": topics}));
        }
        let seed = security(required_text(args, "seed_code")?)?;
        let query = match scalar_arg(args, "topic_id")? {
            Some(id) => TopicStocksQuery::new(seed.clone().into(), id)?,
            None => TopicStocksQuery::first(seed.clone().into()),
        }
        .with_topic_name(args["topic_name"].as_str().map(str::to_owned))?
        .with_sort_by(text(args, "sort_by", "zdf"))?
        .with_section(section(args, "topic_compare", "gndbzfsj")?)?;
        let result = helpers.topic_stocks(&query, options)?;
        let rows: Vec<_> = result
            .rows()
            .iter()
            .map(|row| {
                let raw = row_json(row.raw());
                let code = row.code();
                let supplied_market = raw
                    .get("sc")
                    .or_else(|| raw.get("market_id"))
                    .and_then(integer);
                let inferred = code.as_deref().and_then(|code| security(code).ok());
                let (market_id, exchange, full_code) =
                    match market_name(supplied_market).zip(code.as_deref()) {
                        Some((exchange, code)) => (
                            supplied_market,
                            Some(exchange),
                            Some(format!("{exchange}{code}")),
                        ),
                        None if inferred.is_some() => {
                            let id = inferred.as_ref();
                            (
                                id.cloned()
                                    .map(|id| i64::from(AshareCode::from(id).market().get())),
                                id.map(|id| id.market().code()),
                                id.map(ToString::to_string),
                            )
                        }
                        None => (supplied_market, market_name(supplied_market), None),
                    };
                json!({
                    "rank": row.rank(),
                    "full_code": full_code,
                    "exchange": exchange,
                    "market_id": market_id,
                    "code": code,
                    "name": row.name(),
                    "change_pct": row.change_pct(),
                    "change_pct_3d": row.change_pct_3d(),
                    "change_pct_5d": row.change_pct_5d(),
                    "change_pct_20d": row.change_pct_20d(),
                    "change_pct_60d": row.change_pct_60d(),
                    "change_pct_ytd": row.change_pct_ytd(),
                    "trading_date": optional_json(row.trading_date()),
                    "raw": raw,
                })
            })
            .collect();
        Ok(json!({
            "seed_code": seed.to_string(),
            "topic_id": result.topic_id(),
            "topic_name": result.topic_name(),
            "sort_by": result.sort_by(),
            "rows": rows,
        }))
    })();
    let closed = client.close();
    let value = result?;
    closed?;
    Ok(value)
}

fn macro_tool(name: &str, args: &Value, options: &OperationOptions) -> Result<Value> {
    if name == "macro_indicators" {
        return with_macro_client(|client| {
            let rows = client.macro_indicators(options)?;
            let indicators: Vec<_> = rows
                .iter()
                .map(|row| {
                    let code = row.name().as_str();
                    let category = code.split_once('_').map_or(code, |(prefix, _)| prefix);
                    json!({"code": code, "category": category})
                })
                .collect();
            Ok(json!({"count": indicators.len(), "indicators": indicators}))
        });
    }

    let period = match text(args, "period", "monthly") {
        "daily" => MacPeriod::Daily,
        "weekly" => MacPeriod::Weekly,
        "monthly" => MacPeriod::Monthly,
        "quarterly" => MacPeriod::Quarterly,
        "yearly" => MacPeriod::Yearly,
        unknown => return Err(format!("unknown period: {unknown:?}").into()),
    };
    let codes = required_text(args, "codes")?;
    let selectors = codes
        .split(',')
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(MacroCode::new)
        .collect::<eltdx::Result<Vec<_>>>()?;
    if selectors.is_empty() {
        return Err("codes 不能为空".into());
    }
    let query = MacroQuotesQuery::new(selectors)?;
    with_macro_client(|client| {
        let quote_options = options.clone().with_raw(RawPolicy::RetainPayload);
        let quotes = client.macro_quotes(&query, &quote_options)?;
        if quotes.is_empty() {
            return Err(format!("未取到任何宏观指标: {codes:?}").into());
        }
        let quotes: Vec<_> = quotes
            .iter()
            .enumerate()
            .map(|(index, quote)| {
                json!({
                    "code": quote.id().code().as_str(),
                    "name": quote.name(),
                    "latest": macro_quote_value(quote, index, 4),
                    "previous": macro_quote_value(quote, index, 0),
                })
            })
            .collect();
        let mut payload = json!({"quotes": quotes});
        if args["history"].as_bool().unwrap_or(false) {
            let mut history = Map::new();
            for code in query.codes() {
                let bars = MacroBarsQuery::new(code.clone())
                    .and_then(|query| query.with_period(period))
                    .and_then(|query| query.with_count(800))
                    .and_then(|query| client.macro_bars(&query, options))
                    .unwrap_or_default();
                let bars = bars
                    .iter()
                    .map(|bar| {
                        json!({
                            "date": bar.datetime().date().map(|date| date.to_string()),
                            "value": bar.close().value().as_f64(),
                        })
                    })
                    .collect();
                history.insert(code.as_str().to_owned(), Value::Array(bars));
            }
            payload["history"] = Value::Object(history);
        }
        Ok(payload)
    })
}

fn with_macro_client(operation: impl FnOnce(&MacExClient) -> Result<Value>) -> Result<Value> {
    let builder = MacExClient::builder().with_timeout(Duration::from_secs(10));
    let client = eltdx_tools::mac_ex::configure_login(builder)?.build()?;
    let result = operation(&client);
    let closed = client.close();
    let value = result?;
    closed?;
    Ok(value)
}

fn macro_quote_value(quote: &Quote, index: usize, field: u8) -> Option<f32> {
    let raw = quote.meta().raw()?;
    let &(payload_index, start, end) = raw.row_ranges().get(index)?;
    let payload = raw.payloads().get(payload_index)?;
    let bitmap = bitmap::decode_bitmap(payload.get(..20)?).ok()?;
    let slot = bitmap.slots.iter().find(|slot| slot.bit.get() == field)?;
    let row = payload.get(start..end)?;
    let offset = 68 + slot.slot * 4;
    let bytes = row.get(offset..offset + 4)?.try_into().ok()?;
    Some(f32::from_le_bytes(bytes))
}

fn response_json(response: &Response) -> Result<Value> {
    let mut metadata = FieldMetadata::new();
    let metadata_value = |name| {
        response
            .result_sets()
            .iter()
            .flat_map(ResultSet::rows)
            .find_map(|row| row.get_all(name).next().map(|cell| cell.value()))
    };
    if let Some(value) = metadata_value("nhytype") {
        metadata = metadata.with_industry_template(integer(&native_json(value)));
    }
    if let Some(value) = metadata_value("rtype").and_then(JsonValue::as_str) {
        metadata = metadata.with_report_type(Some(value.to_owned()))?;
    }
    let context = response.field_context(metadata)?;
    let tables: Vec<_> = response.result_sets().iter().enumerate().map(|(index, table)| {
        let rows: Vec<_> = table.rows().iter().map(row_json).collect();
        let row_cells: Vec<Vec<_>> = table.rows().iter().map(|row| {
            row.cells().iter().map(|cell| {
                json!({"name": cell.name(), "index": cell.index(), "value": native_json(cell.value())})
            }).collect()
        }).collect();
        let mut counts = HashMap::new();
        let mut seen = HashSet::new();
        let mut fields = Vec::new();
        for column in table.columns() {
            let count = counts.entry(column).or_insert(0);
            *count += 1;
            let key = if *count == 1 { column.clone() } else { format!("{column}__{count}") };
            if seen.insert(key.clone()) {
                fields.push(key);
            }
        }
        for row in table.rows() {
            for (key, _) in row.python_projection() {
                if seen.insert(key.clone()) {
                    fields.push(key);
                }
            }
        }
        let mut labels = Map::new();
        let mut notes = Map::new();
        let mut unmapped = Vec::new();
        for key in fields {
            let field = describe_projected_field(&context, &key);
            if let Some(label) = field.label() {
                labels.insert(key.clone(), json!(label));
            } else {
                unmapped.push(key.clone());
            }
            if let Some(note) = field.note() {
                notes.insert(key, json!(note));
            }
        }
        let key = table.key().filter(|key| !key.is_empty()).map(str::to_owned)
            .unwrap_or_else(|| format!("table{index}"));
        json!({
            "key": key,
            "columns": table.columns(),
            "rows": rows,
            "row_cells": row_cells,
            "raw": optional_json(table.raw()),
            "field_labels": labels,
            "unmapped_fields": unmapped,
            "field_notes": notes,
        })
    }).collect();
    Ok(json!({
        "entry": response.entry(),
        "request_body": request_json(response.request()),
        "error_code": response.error_code(),
        "result_sets": tables,
        "raw": response_raw(response),
    }))
}

fn ladder_json(ladder: &LimitBoardLadder) -> Value {
    let rows: Vec<_> = ladder
        .rows()
        .iter()
        .map(|row| {
            let raw = row_json(row.raw());
            let code = raw.get("ZQDM").and_then(|value| match value {
                Value::Null => None,
                Value::String(text) => Some(text.clone()),
                other => Some(other.to_string()),
            });
            let market_id = raw.get("SC").and_then(integer);
            let market = market_name(market_id);
            let full_code = match (market, code.as_deref()) {
                (Some(market), Some(code)) if !code.is_empty() => Some(format!("{market}{code}")),
                _ => code.clone(),
            };
            let limit_type = raw.get("ztlb").and_then(integer);
            let level = raw
                .get("lbts")
                .and_then(integer)
                .filter(|level| limit_type == Some(1) && *level > 0);
            let seal_amount = match &raw["fde"] {
                value @ Value::Number(_) => value.clone(),
                Value::Bool(value) => json!(u8::from(*value)),
                Value::String(value) => value
                    .trim()
                    .parse::<f64>()
                    .map_or(Value::Null, |value| json!(value)),
                _ => Value::Null,
            };
            json!({
                "trading_date": raw["rq"],
                "trading_date_value": raw["rqex"],
                "board_level": level,
                "code": code,
                "market_id": market_id,
                "limit_reason": raw["ztyy"],
                "seal_amount": seal_amount,
                "name": raw["ZQJC"],
                "limit_reason_extra": raw["ztyy2"],
                "limit_time": raw["ztsj"],
                "broken_count": raw.get("kbcs").and_then(integer),
                "industry": raw["sshy"],
                "limit_type": limit_type,
                "success_rate": raw["cgl"],
                "highest_board_level": raw.get("zglb").and_then(integer),
                "raw": raw,
                "market": market,
                "full_code": full_code,
            })
        })
        .collect();
    let summary: Vec<_> = ladder
        .summary()
        .iter()
        .map(|row| row_json(row.raw()))
        .collect();
    let date =
        |date: TradingDate| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day());
    json!({
        "entry": ladder.entry(),
        "request_body": request_json(ladder.request()),
        "error_code": ladder.error_code(),
        "start_date": date(ladder.start_date()),
        "end_date": date(ladder.end_date()),
        "rows": rows,
        "summary": summary,
        "raw": response_raw(ladder.detail_response()),
    })
}

fn response_raw(response: &Response) -> Value {
    if response.result_sets().is_empty() {
        let payload = response.meta().raw().and_then(|raw| raw.payloads().last());
        if let Some(value @ Value::Object(_)) =
            payload.and_then(|payload| serde_json::from_slice(payload).ok())
        {
            return value;
        }
    }
    let mut raw: Map<_, _> = response
        .attributes()
        .iter()
        .map(|(key, value)| (key.clone(), native_json(value)))
        .collect();
    if !response.result_sets().is_empty() {
        let tables = response
            .result_sets()
            .iter()
            .map(|table| optional_json(table.raw()))
            .collect();
        raw.insert("ResultSets".to_owned(), Value::Array(tables));
    }
    Value::Object(raw)
}

fn request_json(request: &RequestPayload) -> Value {
    match request {
        RequestPayload::Body(body) => native_json(body),
        RequestPayload::Params(params) => {
            json!({"Params": params.iter().map(native_json).collect::<Vec<_>>()})
        }
    }
}

fn row_json(row: &Row) -> Value {
    Value::Object(
        row.python_projection()
            .into_iter()
            .map(|(key, value)| (key, native_json(value)))
            .collect(),
    )
}

fn optional_json(value: Option<&JsonValue>) -> Value {
    value.map_or(Value::Null, native_json)
}

fn native_json(value: &JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(value) => Value::Bool(*value),
        JsonValue::Number(value) => serde_json::from_str(value.lexeme()).unwrap_or(Value::Null),
        JsonValue::String(value) => Value::String(value.clone()),
        JsonValue::Array(values) => Value::Array(values.iter().map(native_json).collect()),
        JsonValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), native_json(value)))
                .collect(),
        ),
    }
}

fn text<'a>(args: &'a Value, key: &str, default: &'a str) -> &'a str {
    args[key].as_str().unwrap_or(default)
}

fn required_text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key]
        .as_str()
        .ok_or_else(|| format!("{key} must be a string").into())
}

fn scalar_arg(args: &Value, key: &str) -> Result<Option<String>> {
    match &args[key] {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        value if value.is_i64() || value.is_u64() => Ok(Some(value.to_string())),
        _ => Err(format!("{key} must be a string or integer").into()),
    }
}

fn section(args: &Value, method: &'static str, default: &str) -> Result<SectionCode> {
    Ok(SectionCode::new(method, text(args, "section", default))?)
}

fn selector(args: &Value, key: &str, default: Option<i32>) -> Result<i32> {
    if args.get(key).is_none() {
        return default.ok_or_else(|| format!("{key} is required").into());
    }
    args[key]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| format!("{key} must be a 32-bit integer").into())
}

fn pagination(args: &Value, default_size: u32) -> Result<(u32, NonZeroU32)> {
    let bounded = |key, default, max| -> Result<u32> {
        match args.get(key) {
            None => Ok(default),
            Some(&Value::Null) => Err(format!("{key} must be an integer").into()),
            Some(value) => value
                .as_u64()
                .and_then(|value: u64| u32::try_from(value).ok())
                .filter(|value| (1..=max).contains(value))
                .ok_or_else(|| format!("{key} must be an integer between 1 and {max}").into()),
        }
    };
    let page = bounded("page", 1, 10_000)?;
    let size = bounded("page_size", default_size, 100)?;
    Ok((
        page,
        NonZeroU32::new(size).ok_or("page_size must be positive")?,
    ))
}

fn date_arg(args: &Value, key: &str) -> Result<Option<TradingDate>> {
    let Some(value) = scalar_arg(args, key)? else {
        return Ok(None);
    };
    let digits = value.trim().replace(['-', '/'], "");
    if digits.len() != 8 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid {key}: {value:?}").into());
    }
    Ok(Some(TradingDate::new(
        digits[..4].parse()?,
        digits[4..6].parse()?,
        digits[6..].parse()?,
    )?))
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::String(value) => value.trim().parse().ok(),
        Value::Bool(value) => Some(i64::from(*value)),
        _ => value.as_i64().or_else(|| {
            value
                .as_f64()
                .filter(|value| *value >= i64::MIN as f64 && *value < -(i64::MIN as f64))
                .map(|value| value as i64)
        }),
    }
}

fn market_name(id: Option<i64>) -> Option<&'static str> {
    match id {
        Some(0) => Some("sz"),
        Some(1) => Some("sh"),
        Some(2) => Some("bj"),
        _ => None,
    }
}

fn security(code: &str) -> Result<SecurityId> {
    let code = code.trim();
    match code.get(..3).filter(|_| code.len() == 6) {
        Some("010" | "019" | "120" | "122" | "124" | "126" | "130") => {
            Ok(SecurityId::new(eltdx::HqMarket::Shanghai, code)?)
        }
        Some("127") => Ok(SecurityId::new(eltdx::HqMarket::Shenzhen, code)?),
        _ => Ok(code.parse()?),
    }
}

fn ashare(code: &str) -> Result<AshareCode> {
    Ok(security(code)?.into())
}

fn international_code(code: &str) -> &str {
    let code = code.trim();
    code.rsplit_once('.').map_or(code, |(_, number)| number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_preserves_duplicate_cells_and_scoped_field_labels() -> Result<()> {
        let raw = br#"{
            "ErrorCode": 0,
            "ResultSets": [
                {"ColName": ["rq", "T039", "T039", "T999"], "Content": [["2026-06-30", 3, 4, 1]]},
                {"ColName": ["rtype", "nhytype"], "Content": [["zcfzb", 1]]}
            ]
        }"#;
        let request = RequestPayload::Params(
            ["000001", "zcfzb", ""]
                .into_iter()
                .map(JsonValue::from)
                .collect(),
        );
        let response = parse_response(
            &EntryName::new("CWServ.tdxf10_gg_cwfx")?,
            &request,
            JsonValue::parse(raw)?,
            RawPolicy::RetainPayload,
        )?;
        let payload = response_json(&response)?;
        let first = &payload["result_sets"][0];
        assert_eq!(
            payload["request_body"],
            json!({"Params": ["000001", "zcfzb", ""]})
        );
        assert_eq!(payload["raw"], serde_json::from_slice::<Value>(raw)?);
        assert_eq!(first["key"], "table0");
        assert_eq!(first["rows"][0]["T039__2"], 4);
        assert_eq!(
            first["row_cells"][0][2],
            json!({"name": "T039", "index": 2, "value": 4})
        );
        assert_eq!(first["field_labels"]["T039"], "固定资产");
        assert_eq!(first["field_labels"]["T039__2"], "固定资产（2）");
        assert_eq!(first["unmapped_fields"], json!(["T999"]));
        Ok(())
    }
}
