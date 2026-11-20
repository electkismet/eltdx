use std::num::NonZeroU32;
use std::time::Duration;

use eltdx::types::{BatchItemState, MarketDateChoice, ResultMeta};
use eltdx::{
    CalendarQuery, CategoryQuery, CodeQuery, CodeScanQuery, DownloadOptions, EventTime,
    EventTimeSource, MissingPolicy, OperationOptions, PageOptions, QuoteCategory, QuoteSort,
    RawPolicy, Security, SpecialLimitQuery, StatsQuery, TdxClient, TradingDate,
};
use serde_json::{json, Value};

use super::quote::{self, hex, identity, raw, record};
use super::{date, market, securities, Arguments, Result, RpcError};

#[path = "series.rs"]
mod series;

pub(super) const METHODS: &[&str] = &[
    "ping",
    "clear_cache",
    "session.ping",
    "session.handshake",
    "session.heartbeat",
    "codes.count",
    "codes.list",
    "codes.all",
    "codes.stock_count",
    "codes.a_share_count",
    "codes.stocks",
    "codes.a_shares",
    "codes.etfs",
    "codes.indices",
    "codes.all_markets",
    "codes.all_stocks",
    "codes.all_a_shares",
    "codes.all_etfs",
    "codes.all_indices",
    "codes.latest_stock_list",
    "codes.latest_st",
    "codes.st",
    "codes.latest_stocks",
    "codes.latest_suspended",
    "codes.suspended",
    "quotes.get_snapshots",
    "quotes.legacy",
    "quotes.get_depth",
    "quotes.list_by_category",
    "quotes.refresh",
    "quotes.poll_push",
    "quotes.drain_pushes",
    "quotes.subscribe_hq",
    "resources.read",
    "resources.block_meta",
    "resources.download_file",
    "resources.read_stats",
    "bars.get",
    "minutes.today",
    "minutes.history",
    "minutes.recent",
    "minutes.aux",
    "minutes.buy_sell_strength",
    "minutes.volume_comparison",
    "minutes.sparkline",
    "trades.today",
    "trades.history",
    "trades.all_today",
    "trades.all_history",
    "trades.history_batch",
    "trades.today_batch",
    "trades.all_today_batch",
    "trades.all_history_batch",
    "trades.opening_match_today",
    "trades.opening_match_history",
    "auctions.series",
    "corporate.capital_changes",
    "corporate.adjustment_factors",
    "corporate.finance_batch",
    "limits.special",
    "limits.scan_special",
    "workdays.today",
    "workdays.normalize",
    "workdays.text",
    "workdays.same_day",
    "workdays.refresh",
    "workdays.clear",
    "workdays.is_workday",
    "workdays.today_is_workday",
    "workdays.range",
    "workdays.iter_days",
    "workdays.next_workday",
    "workdays.previous_workday",
];

pub(super) fn call(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    let (service, name) = method.split_once('.').unwrap_or(("", method));
    match service {
        "codes" => codes(client, name, params),
        "quotes" => quotes(client, name, params),
        "resources" => resources(client, name, params),
        "bars" | "minutes" | "trades" | "auctions" | "corporate" => {
            series::call(client, service, name, params)
        }
        "limits" => limits(client, name, params),
        "workdays" => workdays(client, name, params),
        _ => {
            Arguments::bind(params, &[])?;
            let session = client.session();
            let options = with_payload();
            match name {
                "ping" => Ok(json!(session.ping()?.message())),
                "clear_cache" => {
                    client.calendar().clear_cache();
                    client.helpers().clear_cache();
                    Ok(Value::Null)
                }
                "handshake" => {
                    let reply = session.handshake(&options)?;
                    Ok(json!({
                        "server_datetime": reply.server_datetime().and_then(event_time),
                        "session_minutes_1": reply.session_minutes1(), "session_minutes_2": reply.session_minutes2(),
                        "server_date_1": date_value(reply.server_date1()), "server_date_2": date_value(reply.server_date2()),
                        "server_name": reply.server_name(), "product_tag": reply.product_tag(),
                        "unknown_time_1_raw": reply.unknown_time1(), "unknown_time_2_raw": reply.unknown_time2(),
                        "flags_raw": hex(reply.flags()), "tail_control_raw": hex(reply.control_tail()), "raw_payload": raw(reply.meta()),
                    }))
                }
                "heartbeat" => {
                    let reply = session.heartbeat(&options)?;
                    Ok(
                        json!({"reserved": hex(reply.reserved()), "server_date_raw": reply.server_date_raw(),
                        "server_date": date_value(reply.server_date()), "raw_payload": raw(reply.meta())}),
                    )
                }
                _ => Err(RpcError::internal("unregistered session method")),
            }
        }
    }
}

pub(super) fn with_payload() -> OperationOptions {
    OperationOptions::default().with_raw(RawPolicy::RetainPayload)
}

pub(super) fn date_value(date: Option<TradingDate>) -> Value {
    json!(date.map(|day| format!("{day}")))
}

pub(super) fn date_number(date: TradingDate) -> u32 {
    u32::from(date.year()) * 10000 + u32::from(date.month()) * 100 + u32::from(date.day())
}

pub(super) fn event_time(time: EventTime) -> Option<String> {
    let date = time.date()?;
    let (hour, minute, second) = time.clock().unwrap_or((0, 0, 0));
    let zone = if time.source_kind() == EventTimeSource::Shanghai {
        "+08:00"
    } else {
        ""
    };
    Some(format!("{date}T{hour:02}:{minute:02}:{second:02}{zone}"))
}

pub(super) fn time_label(time: EventTime) -> String {
    time.clock().map_or_else(String::new, |(hour, minute, _)| {
        format!("{hour:02}:{minute:02}")
    })
}

pub(super) fn optional_date(args: &Arguments, name: &str) -> Result<Option<TradingDate>> {
    args.value(name)
        .filter(|value| !value.is_null())
        .map(date)
        .transpose()
}

pub(super) fn date_choice(args: &Arguments, name: &str) -> Result<MarketDateChoice> {
    Ok(optional_date(args, name)?.map_or(MarketDateChoice::Current, MarketDateChoice::Date))
}

pub(super) fn missing_policy(args: &Arguments) -> Result<MissingPolicy> {
    match args
        .default::<String>("on_missing", "raise".to_owned())?
        .as_str()
    {
        "raise" => Ok(MissingPolicy::Error),
        "skip" => Ok(MissingPolicy::AllowPartial),
        _ => Err(RpcError::value("on_missing must be 'raise' or 'skip'")),
    }
}

fn payload_bytes(meta: &ResultMeta) -> &[u8] {
    meta.raw()
        .and_then(|data| data.payloads().first())
        .map_or(&[], AsRef::as_ref)
}

fn security_row(row: &Security) -> Value {
    let category = match row.category() {
        eltdx::SecurityCategory::AShare => "a_share",
        eltdx::SecurityCategory::BShare => "b_share",
        eltdx::SecurityCategory::Etf => "etf",
        eltdx::SecurityCategory::Index => "index",
        eltdx::SecurityCategory::Bond => "bond",
        eltdx::SecurityCategory::PrivateConvertibleBond => "private_convertible_bond",
        eltdx::SecurityCategory::Other(name) => name,
        _ => "unknown",
    };
    let board = match row.board() {
        eltdx::SecurityBoard::SseMain => "sse_main",
        eltdx::SecurityBoard::SzseMain => "szse_main",
        eltdx::SecurityBoard::ChiNext => "chinext",
        eltdx::SecurityBoard::Star => "star",
        eltdx::SecurityBoard::Beijing => "beijing",
        eltdx::SecurityBoard::Other(name) => name,
        _ => "",
    };
    identity(
        row.id(),
        json!({
            "name": row.name(), "multiple": row.multiple(), "decimal": row.decimal(),
            "previous_close_price": row.previous_close().as_f64(), "volume_ratio_base": row.volume_ratio_base(),
            "unknown0_raw": "", "previous_close_raw": "", "unknown3_raw": "",
            "category": category, "category_reason": row.category_reason(), "board": board, "board_reason": row.board_reason(),
        }),
    )
}

fn codes(client: &TdxClient, name: &str, params: Value) -> Result<Value> {
    let sdk = client.codes();
    let options = OperationOptions::default();
    let scan = CodeScanQuery::new();
    let signature: &[&str] = match name {
        "list" => &["market", ";", "start", "limit"],
        "all" => &["market", ";", "page_size"],
        name if name.starts_with("all_") => &[],
        _ => &["market"],
    };
    let args = Arguments::bind(params, signature)?;
    if name.starts_with("all_") {
        return Ok(match name {
            "all_markets" => json!(sdk
                .all_markets(&scan, &options)?
                .items()
                .iter()
                .map(security_row)
                .collect::<Vec<_>>()),
            _ => {
                let rows = match name {
                    "all_stocks" => sdk.all_stocks(&scan, &options)?,
                    "all_a_shares" => sdk.all_a_shares(&scan, &options)?,
                    "all_etfs" => sdk.all_etfs(&scan, &options)?,
                    "all_indices" => sdk.all_indices(&scan, &options)?,
                    _ => return Err(RpcError::internal("unregistered codes method")),
                };
                json!(rows
                    .items()
                    .iter()
                    .map(|id| format!("{id}"))
                    .collect::<Vec<_>>())
            }
        });
    }
    if matches!(
        name,
        "latest_stock_list"
            | "latest_stocks"
            | "latest_st"
            | "st"
            | "latest_suspended"
            | "suspended"
    ) {
        let market = args
            .value("market")
            .filter(|value| !value.is_null())
            .map(market)
            .transpose()?;
        return Ok(match name {
            "latest_st" | "st" => json!(sdk
                .latest_st(market, &options)?
                .items()
                .iter()
                .map(security_row)
                .collect::<Vec<_>>()),
            "latest_suspended" | "suspended" => json!(sdk
                .latest_suspended(market, &options)?
                .items()
                .iter()
                .map(|id| format!("{id}"))
                .collect::<Vec<_>>()),
            _ => json!(sdk
                .latest_stock_list(market, &options)?
                .items()
                .iter()
                .map(security_row)
                .collect::<Vec<_>>()),
        });
    }
    let market = market(args.required("market")?)?;
    Ok(match name {
        "count" => json!(sdk.count(market, &options)?),
        "stock_count" => json!(sdk.stock_count(market, &scan, &options)?),
        "a_share_count" => json!(sdk.a_share_count(market, &scan, &options)?),
        "list" => {
            let query = CodeQuery::new()
                .with_start(args.default("start", 0)?)?
                .with_limit(args.default("limit", 1600)?)?;
            json!(sdk
                .list(market, &query, &options)?
                .items()
                .iter()
                .map(security_row)
                .collect::<Vec<_>>())
        }
        _ => {
            let rows = match name {
                "all" => sdk.all(
                    market,
                    &scan.with_pages(PageOptions::new(
                        args.default("page_size", 1600)?,
                        200,
                        160_000,
                    )?)?,
                    &options,
                )?,
                "stocks" => sdk.stocks(market, &scan, &options)?,
                "a_shares" => sdk.a_shares(market, &scan, &options)?,
                "etfs" => sdk.etfs(market, &scan, &options)?,
                "indices" => sdk.indices(market, &scan, &options)?,
                _ => return Err(RpcError::internal("unregistered codes method")),
            };
            json!(rows.items().iter().map(security_row).collect::<Vec<_>>())
        }
    })
}

fn quotes(client: &TdxClient, name: &str, params: Value) -> Result<Value> {
    let sdk = client.quotes();
    let options = with_payload();
    let signature: &[&str] = match name {
        "get_snapshots" => &["codes", ";", "on_missing"],
        "refresh" => &["codes", "cursors"],
        "list_by_category" => &["category", ";", "sort_by", "start", "count", "ascending"],
        "poll_push" => &[";", "timeout", "parse"],
        "drain_pushes" => &[";", "parse"],
        _ => &["codes"],
    };
    let args = Arguments::bind(params, signature)?;
    if matches!(name, "poll_push" | "drain_pushes") {
        let wait = if name == "poll_push" {
            let seconds = args.optional::<f64>("timeout")?.unwrap_or_default();
            Some(
                Duration::try_from_secs_f64(seconds)
                    .map_err(|_| RpcError::value("timeout must be finite and non-negative"))?,
            )
        } else {
            None
        };
        let parse = args.default("parse", false)?;
        let frames = sdk.read_push_frames(wait, &options)?;
        let mut values = frames.iter().map(|frame| {
            let response = frame.response();
            if parse {
                let mut value = quote::refresh_page(&sdk.decode_push_frame(frame, &options)?);
                value["requested_codes"] = json!([]);
                return Ok(value);
            }
            Ok(json!({"control": response.control, "msg_id": response.msg_id, "msg_type": response.msg_type,
                "zip_length": response.zip_length, "length": response.length, "data": hex(&response.data),
                "raw": hex(&response.raw), "response_header_reserved": response.response_header_reserved}))
        }).collect::<Result<Vec<_>>>()?;
        return Ok(if wait.is_some() {
            values.pop().unwrap_or_default()
        } else {
            json!(values)
        });
    }
    if name == "list_by_category" {
        let category = category(args.required("category")?)?;
        let sort = args
            .value("sort_by")
            .filter(|value| !value.is_null())
            .map(sort)
            .transpose()?
            .unwrap_or_default();
        let query = CategoryQuery::new()
            .with_category(category)?
            .with_sort(sort)?
            .with_start(args.default("start", 0)?)?
            .with_count(args.default("count", 80)?)?
            .with_ascending(args.default("ascending", false)?)?;
        let page = sdk.category(&query, &options)?;
        let records = page.items().iter().enumerate().map(|(index, row)| {
            let snapshot = row.snapshot();
            identity(snapshot.id(), json!({
                "active1": snapshot.active(), "active2": null, "last_price": snapshot.last().as_f64(),
                "pre_close_price": snapshot.pre_close().as_f64(), "open_price": snapshot.open().as_f64(),
                "high_price": snapshot.high().as_f64(), "low_price": snapshot.low().as_f64(),
                "server_time_raw": snapshot.server_time_raw(), "neg_price_raw": snapshot.unknown_after_time_raw(),
                "total_hand": snapshot.total().source_value().as_f64() as i64,
                "current_hand": snapshot.current().source_value().as_f64() as i64,
                "amount": snapshot.amount().value().as_f64(), "amount_raw": snapshot.amount_wire_word(),
                "inside_dish": snapshot.inside().source_value().as_f64() as i64,
                "outer_disc": snapshot.outside().source_value().as_f64() as i64,
                "after_outer_raw": snapshot.unknown_after_outer_raw(), "open_amount_raw": snapshot.open_amount_raw(),
                "open_amount": snapshot.open_amount().value().as_f64(), "bid1": row.bid1().price().as_f64(),
                "ask1": row.ask1().price().as_f64(), "bid_vol1": row.bid1().volume().source_value().as_f64() as i64,
                "ask_vol1": row.ask1().volume().source_value().as_f64() as i64, "status_or_sort_raw": row.status_sort_raw(),
                "rise_speed_raw": row.rise_speed().source_value().as_f64(), "rise_speed": row.rise_speed().value().as_f64(),
                "short_turnover_raw": row.short_turnover().source_value().as_f64(), "short_turnover": row.short_turnover().value().as_f64(),
                "opening_rush_raw": row.opening_rush().source_value().as_f64(), "opening_rush": row.opening_rush().value().as_f64(),
                "min2_amount": row.two_minute_amount().value().as_f64(), "vol_rise_speed": row.volume_rise_speed().value().as_f64(),
                "depth": row.depth(), "extra_pair_raw": "", "extra_meta_raw": "", "tail_raw": "", "record_hex": record(page.meta(), index),
            }))
        }).collect::<Vec<_>>();
        let header = payload_bytes(page.meta())
            .get(..2)
            .and_then(|value| <[u8; 2]>::try_from(value).ok())
            .map(u16::from_le_bytes);
        return Ok(json!({
            "category": category.raw(), "sort_type": sort.raw(), "start": query.start(),
            "request_count": query.count(), "sort_reverse": u8::from(!query.ascending()),
            "filter_raw": 0, "header": header, "records": records, "raw_payload": raw(page.meta()),
        }));
    }
    let ids = if name == "refresh" {
        args.value("codes")
            .filter(|value| !value.is_null())
            .map(securities)
            .transpose()?
            .unwrap_or_default()
    } else {
        securities(args.required("codes")?)?
    };
    match name {
        "get_snapshots" => {
            let batch = sdk.snapshots_with_policy(&ids, missing_policy(&args)?, &options)?;
            Ok(json!(batch
                .items()
                .iter()
                .filter_map(|item| match item.state() {
                    BatchItemState::Present(row) => Some(quote::snapshot(row)),
                    _ => None,
                })
                .collect::<Vec<_>>()))
        }
        "legacy" => {
            let batch = sdk.legacy(&ids, &options)?;
            Ok(json!(batch
                .items()
                .iter()
                .filter_map(|item| match item.state() {
                    BatchItemState::Present(row) => {
                        let mut value = quote::snapshot(row.snapshot());
                        let object = value.as_object_mut()?;
                        object.remove("tail_raw");
                        let time = object.remove("time_raw")?;
                        value["server_time_raw"] = time;
                        value["trading_status_raw"] = json!(row.trading_status());
                        value["tail_metrics_raw"] = json!(row.tail_metrics());
                        value["rise_speed_raw"] =
                            json!(row.rise_speed().map(|speed| speed.source_value().as_f64()));
                        value["active2"] = json!(row.active2());
                        value["record_hex"] = json!(record(row.snapshot().meta(), 0));
                        Some(value)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()))
        }
        "get_depth" | "refresh" => {
            if ids.len() > 100 {
                return Err(RpcError::value("refresh accepts at most 100 securities"));
            }
            let cursors = args
                .optional::<std::collections::HashMap<String, u32>>("cursors")?
                .unwrap_or_default();
            if name == "refresh" && (ids.is_empty() || !cursors.is_empty()) {
                let requests = ids
                    .into_iter()
                    .map(|id| {
                        let cursor = cursors.get(&format!("{id}")).copied().unwrap_or_default();
                        eltdx::types::RefreshRequest::new(id).with_cursor(cursor)
                    })
                    .collect::<eltdx::Result<Vec<_>>>()?;
                let pin = client.pin(&options)?;
                return Ok(quote::refresh_page(
                    &pin.quotes().refresh(&requests, &options)?,
                ));
            }
            Ok(quote::refresh_page(&sdk.depth(&ids, &options)?))
        }
        _ => Err(RpcError::internal("unregistered quote method")),
    }
}

pub(super) fn category(value: &Value) -> Result<QuoteCategory> {
    if let Some(number) = value.as_u64().and_then(|value| u16::try_from(value).ok()) {
        return Ok(QuoteCategory::Wire(number));
    }
    match value
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .as_str()
    {
        "a_share" | "a_shares" | "ashare" | "a" | "沪深a股" | "6" => {
            Ok(QuoteCategory::HqAShares)
        }
        name => name
            .parse::<u16>()
            .map(QuoteCategory::Wire)
            .map_err(|_| RpcError::value(format!("unknown quote category: {value}"))),
    }
}

pub(super) fn sort(value: &Value) -> Result<QuoteSort> {
    if let Some(number) = value.as_u64().and_then(|value| u16::try_from(value).ok()) {
        return Ok(QuoteSort::Wire(number));
    }
    Ok(
        match value
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str()
        {
            "code" => QuoteSort::Code,
            "price" | "last" => QuoteSort::Last,
            "amount" => QuoteSort::Amount,
            "change_pct" | "zdf" => QuoteSort::ChangePct,
            "seal_amount" => QuoteSort::SealAmount,
            "open_amount" => QuoteSort::OpenAmount,
            "rise_speed" => QuoteSort::RiseSpeed,
            "short_turnover" => QuoteSort::ShortTurnover,
            "vol_rise_speed" => QuoteSort::VolumeRiseSpeed,
            "opening_rush" => QuoteSort::OpeningRush,
            "min2_amount" => QuoteSort::TwoMinuteAmount,
            "open_change" => QuoteSort::OpenChange,
            "high_change" => QuoteSort::HighChange,
            "low_change" => QuoteSort::LowChange,
            "drawdown" => QuoteSort::Drawdown,
            "attack" => QuoteSort::Attack,
            name => QuoteSort::Wire(
                name.parse::<u16>()
                    .map_err(|_| RpcError::value(format!("unknown quote sort: {value}")))?,
            ),
        },
    )
}

fn resources(client: &TdxClient, name: &str, params: Value) -> Result<Value> {
    let sdk = client.resources();
    let options = with_payload();
    let args = Arguments::bind(
        params,
        match name {
            "read" => &["path", ";", "offset", "size"],
            "block_meta" => &["filename"],
            "download_file" => &["path", ";", "chunk_size", "max_bytes"],
            _ => &["path", ";", "chunk_size"],
        },
    )?;
    Ok(match name {
        "read" => {
            let path: String = args.get("path")?;
            let offset = args.default("offset", 0)?;
            let size = args.default("size", 30_000)?;
            let chunk = sdk.read(&path, offset, size, &options)?;
            let mut payload = (chunk.data.len() as u32).to_le_bytes().to_vec();
            payload.extend_from_slice(&chunk.data);
            json!({"path": path, "offset": offset, "request_size": size, "chunk_len": chunk.data.len(),
                "content": hex(&chunk.data), "raw_payload": hex(&payload)})
        }
        "block_meta" => {
            let value = sdk.block_meta(&args.get::<String>("filename")?, &options)?;
            json!({"size": value.size(), "md5": value.md5()})
        }
        "download_file" => {
            let path: String = args.get("path")?;
            let size = NonZeroU32::new(args.default("chunk_size", 30_000)?)
                .ok_or_else(|| RpcError::value("chunk_size must be positive"))?;
            let maximum = args.optional::<u64>("max_bytes")?;
            if maximum == Some(0) {
                return Ok(json!(""));
            }
            let mut query = DownloadOptions::new().with_chunk_size(size)?;
            if let Some(maximum) = maximum {
                query = query.with_max_bytes(maximum)?;
            }
            json!(hex(&sdk
                .download_with_options(&path, &query, &options)?
                .data))
        }
        "read_stats" => {
            let query = StatsQuery::new()
                .with_path(args.default("path", "zhb.zip".to_owned())?)?
                .with_chunk_size(args.default("chunk_size", 30_000)?)?;
            let value = sdk.read_stats(&query, &options)?;
            let stat = value.stat().iter().map(|row| {
                let market = row.source_market().parse::<u8>().ok().or_else(|| row.id().map(|id| quote::market_id(id.market()))).unwrap_or_default();
                let code = row.source_code();
                (format!("[{market}, '{code}']"), json!({"market_id": market, "code": code, "stats_date": row.stats_date_raw(),
                    "beta_60d": row.beta_60d(), "pe_ttm": row.pe_ttm(), "free_float_shares_10k": row.free_float_shares_source_10k(),
                    "year_limit_up_days": row.source_counts()[0].as_deref().and_then(|value| value.parse::<i64>().ok()),
                    "limit_stat_days": row.source_counts()[1].as_deref().and_then(|value| value.parse::<i64>().ok()),
                    "limit_up_count_in_stat_days": row.source_counts()[2].as_deref().and_then(|value| value.parse::<i64>().ok()),
                    "limit_up_streak_days": row.source_counts()[3].as_deref().and_then(|value| value.parse::<i64>().ok())}))
            }).collect::<serde_json::Map<_, _>>();
            let stat2 = value.stat2().iter().map(|row| {
                let market = row.source_market().parse::<u8>().ok().or_else(|| row.id().map(|id| quote::market_id(id.market()))).unwrap_or_default();
                let code = row.source_code();
                let values = row.source_values();
                (format!("[{market}, '{code}']"), json!({"market_id": market, "code": code, "stats_date": row.stats_date_raw(),
                    "amount_10k": values[0], "seal_amount_10k": values[1], "prev_amount_10k": values[2], "prev_seal_amount_10k": values[3],
                    "prev2_amount_10k": values[4], "prev2_seal_amount_10k": values[5], "open_volume_hand": values[6],
                    "prev_open_volume_hand": values[7], "open_amount_10k": values[8], "prev_open_amount_10k": values[9]}))
            }).collect::<serde_json::Map<_, _>>();
            json!({"stat": stat, "stat2": stat2, "source_path": value.source_path()})
        }
        _ => return Err(RpcError::internal("unregistered resources method")),
    })
}

fn limits(client: &TdxClient, name: &str, params: Value) -> Result<Value> {
    let args = Arguments::bind(
        params,
        if name == "special" {
            &[";", "start_index"]
        } else {
            &[";", "start_index", "max_rows"]
        },
    )?;
    let sdk = client.limits();
    let mut query = SpecialLimitQuery::new().with_start_index(args.default("start_index", 0)?)?;
    let options = with_payload();
    let row = |value: &eltdx::SpecialLimit| {
        let market = value.market_raw();
        json!({"exchange": value.id().map(|id| id.market().code()).unwrap_or(""), "market_id": market,
            "code": format!("{:06}", value.code_raw()), "code_num": value.code_raw(),
            "upper_price_raw_f32": value.upper().as_f64(), "lower_price_raw_f32": value.lower().as_f64(),
            "record_hex": record(value.meta(), 0)})
    };
    if name == "special" {
        let page = sdk.special(&query, &options)?;
        return Ok(
            json!({"start_index": query.start_index(), "records": page.items().iter().map(row).collect::<Vec<_>>(),
            "raw_payload": raw(page.meta())}),
        );
    }
    let maximum = args.default("max_rows", 10_000)?;
    if maximum == 0 {
        return Ok(json!([]));
    }
    let pages = query.pages().clone().with_max_records(maximum)?;
    query = query.with_pages(pages)?;
    Ok(json!(sdk
        .scan_special(&query, &options)?
        .items()
        .iter()
        .map(row)
        .collect::<Vec<_>>()))
}

fn workdays(client: &TdxClient, name: &str, params: Value) -> Result<Value> {
    let args = Arguments::bind(
        params,
        match name {
            "today" | "today_is_workday" | "refresh" | "clear" => &[],
            "same_day" => &["left", "right"],
            "range" | "iter_days" => &["start", "end", ";", "descending"],
            "next_workday" | "previous_workday" => &["value", ";", "include_self"],
            _ => &["value"],
        },
    )?;
    let sdk = client.calendar();
    let query = CalendarQuery::new();
    let options = OperationOptions::default();
    let day =
        || optional_date(&args, "value")?.map_or_else(|| sdk.today().map_err(RpcError::from), Ok);
    let required = |name| {
        let value = args.required(name)?;
        if value.is_null() {
            Ok(sdk.today()?)
        } else {
            date(value)
        }
    };
    Ok(match name {
        "today" => date_value(Some(sdk.today()?)),
        "normalize" | "text" => date_value(Some(day()?)),
        "same_day" => json!(required("left")? == required("right")?),
        "refresh" => json!(sdk.refresh(&query, &options)?.days().len()),
        "clear" => {
            sdk.clear_cache();
            Value::Null
        }
        "is_workday" | "today_is_workday" => json!(sdk.is_workday(day()?, &query, &options)?),
        "range" | "iter_days" => json!(sdk
            .range(
                required("start")?,
                required("end")?,
                args.default("descending", false)?,
                &query,
                &options
            )?
            .into_iter()
            .map(|day| format!("{day}"))
            .collect::<Vec<_>>()),
        "next_workday" => date_value(sdk.next(
            day()?,
            args.default("include_self", false)?,
            &query,
            &options,
        )?),
        "previous_workday" => date_value(sdk.previous(
            day()?,
            args.default("include_self", false)?,
            &query,
            &options,
        )?),
        _ => return Err(RpcError::internal("unregistered calendar method")),
    })
}
