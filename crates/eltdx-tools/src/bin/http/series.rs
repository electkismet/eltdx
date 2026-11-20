use eltdx::types::{
    AdjustMode, AuctionQuery, AuxKind, AuxValues, BarTime, BatchItemState, BatchOutcome, KlineKind,
    KlinePeriod, KlineQuery, MinuteQuery, SparklineQuery, TradeEventKind, TradeQuery, TradeSide,
    TradeTick,
};
use eltdx::{
    CapitalQuery, FactorQuery, FinanceQuery, OperationOptions, PageOptions, Price, RawPolicy,
    TdxClient,
};
use serde_json::{json, Map, Value};

use super::super::quote::{hex, identity, raw, record};
use super::super::{date, securities, security, Arguments, Result, RpcError};
use super::{
    date_choice, date_number, date_value, event_time, missing_policy, optional_date, time_label,
};

pub(super) fn call(
    client: &TdxClient,
    service: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    match service {
        "bars" => bars(client, params),
        "minutes" => minutes(client, method, params),
        "trades" => trades(client, method, params),
        "auctions" => auctions(client, params),
        "corporate" => corporate(client, method, params),
        _ => Err(RpcError::internal("unregistered series service")),
    }
}

fn options(args: &Arguments) -> Result<OperationOptions> {
    let policy = if args.default("include_raw", false)? {
        RawPolicy::RetainPayload
    } else {
        RawPolicy::Omit
    };
    Ok(OperationOptions::default().with_raw(policy))
}

fn validate_batch(args: &Arguments) -> Result<()> {
    if args.optional::<usize>("batch_size")? == Some(0) {
        return Err(RpcError::value("batch_size must be positive"));
    }
    Ok(())
}

fn pages(args: &Arguments, size: usize, default: usize) -> Result<PageOptions> {
    let maximum = match args.value("max_pages") {
        Some(Value::Null) => (u16::MAX as usize / size.max(1)) + 2,
        Some(_) => args.get("max_pages")?,
        None => default,
    };
    Ok(PageOptions::new(
        size,
        maximum,
        size.saturating_mul(maximum),
    )?)
}

fn by_code<T>(
    single: bool,
    batch: &BatchOutcome<T>,
    project: impl Fn(&T) -> Result<Value>,
) -> Result<Value> {
    let mut rows = Map::new();
    for item in batch.items() {
        let BatchItemState::Present(row) = item.state() else {
            return Err(RpcError::value(format!(
                "missing code: {}",
                item.security()
            )));
        };
        let value = project(row)?;
        if single {
            return Ok(value);
        }
        rows.insert(format!("{}", item.security()), value);
    }
    Ok(Value::Object(rows))
}

fn period(text: &str) -> Result<(KlinePeriod, u16, u16, String)> {
    use KlinePeriod::*;
    let key = text.trim().to_ascii_lowercase();
    let (period, category, label) = match key.as_str() {
        "5m" | "5min" => (Min5, 0, "5m"),
        "15m" | "15min" => (Min15, 1, "15m"),
        "30m" | "30min" => (Min30, 2, "30m"),
        "60m" | "60min" | "1h" | "hour" => (Min60, 3, "60m"),
        "day" | "1d" | "d" | "daily" => (Day, 4, "day"),
        "week" | "1w" | "w" => (Week, 5, "week"),
        "month" | "1mo" | "mo" => (Month, 6, "month"),
        "1m" | "1min" | "minute" => (Min1, 7, "1m"),
        "quarter" | "1q" | "q" => (Quarter, 10, "quarter"),
        "year" | "1y" | "y" => (Year, 11, "year"),
        "flow" | "fund_flow" | "zjlx" => {
            return Err(RpcError::value(
                "period is not OHLC data; use money flow history",
            ))
        }
        _ => {
            let end = key.bytes().take_while(u8::is_ascii_digit).count();
            let invalid = || RpcError::value(format!("invalid period: {text:?}"));
            let value = key[..end].parse::<u16>().map_err(|_| invalid())?;
            return match &key[end..] {
                "m" | "min" => Ok((KlinePeriod::minutes(value)?, 8, value, format!("{value}m"))),
                "d" => Ok((KlinePeriod::days(value)?, 9, value, format!("{value}d"))),
                "s" => Ok((KlinePeriod::seconds(value)?, 13, value, format!("{value}s"))),
                _ => Err(invalid()),
            };
        }
    };
    Ok((period, category, 1, label.to_owned()))
}

fn milli(price: Price) -> i64 {
    (price.as_f64() * 1000.0).round() as i64
}

fn bars(client: &TdxClient, params: Value) -> Result<Value> {
    let args = Arguments::bind(
        params,
        &[
            "code",
            ";",
            "period",
            "start",
            "count",
            "adjust",
            "anchor_date",
            "kind",
            "include_raw",
            "all_pages",
            "page_size",
            "max_pages",
            "batch_size",
            "bar_time",
        ],
    )?;
    validate_batch(&args)?;
    let code = args.required("code")?;
    let ids = securities(code)?;
    let (period, category, parameter, label) =
        period(&args.default::<String>("period", "day".into())?)?;
    let (adjust, adjust_raw, adjust_name) = match args
        .optional::<String>("adjust")?
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "none" => (AdjustMode::None, 0, "none"),
        "qfq" | "front" => (AdjustMode::Qfq, 1, "qfq"),
        "hfq" | "back" => (AdjustMode::Hfq, 2, "hfq"),
        "fixed_qfq" | "fixed_front" => (AdjustMode::FixedQfq, 3, "fixed_qfq"),
        "fixed_hfq" | "fixed_back" => (AdjustMode::FixedHfq, 4, "fixed_hfq"),
        value => return Err(RpcError::value(format!("invalid adjust mode: {value}"))),
    };
    let kind = match args.optional::<String>("kind")?.as_deref() {
        None | Some("auto") => KlineKind::Auto,
        Some("stock") => KlineKind::Stock,
        Some("index") => KlineKind::Index,
        Some(value) => return Err(RpcError::value(format!("invalid K-line kind: {value}"))),
    };
    let bar_time = match args.default::<String>("bar_time", "start".into())?.as_str() {
        "start" => BarTime::Start,
        "end" => BarTime::End,
        value => return Err(RpcError::value(format!("invalid bar_time: {value}"))),
    };
    let all = args.default("all_pages", false)?;
    let count = args.default(if all { "page_size" } else { "count" }, 800usize)?;
    let start = args.default("start", 0)?;
    let mut query = KlineQuery::new()
        .with_period(period)?
        .with_start(start)?
        .with_count(count)?
        .with_adjust(adjust)?
        .with_anchor_date(optional_date(&args, "anchor_date")?)?
        .with_kind(kind)?
        .with_bar_time(bar_time)?;
    if all {
        query = query.with_pages(pages(&args, count, 200)?)?;
    }
    let options = options(&args)?;
    let service = client.klines();
    let batch = if all {
        service.collect_many(&ids, &query, &options)?
    } else {
        service.page_many(&ids, &query, &options)?
    };
    by_code(code.is_string(), &batch, |series| {
        let rows = series.bars().items().iter().enumerate().map(|(index, bar)| {
            let (open, close, high, low) = (milli(bar.open()), milli(bar.close()), milli(bar.high()), milli(bar.low()));
            json!({"time": event_time(bar.time()), "open": bar.open().as_f64(), "close": bar.close().as_f64(),
                "high": bar.high().as_f64(), "low": bar.low().as_f64(), "open_price_milli": open, "close_price_milli": close,
                "high_price_milli": high, "low_price_milli": low, "last_close_price_milli": bar.last_close().map(milli),
                "volume_raw": bar.volume_word(), "amount_raw": bar.amount_word(), "volume_wire_value": bar.volume().source_value().as_f64(),
                "volume_lots": bar.volume().value().as_f64(), "amount": bar.amount().value().as_f64(),
                "open_delta_raw": open - bar.last_close().map_or(0, milli), "close_delta_raw": close - open,
                "high_delta_raw": high - open, "low_delta_raw": low - open, "up_count": bar.index_up(),
                "down_count": bar.index_down(), "record_hex": record(series.meta(), index)})
        }).collect::<Vec<_>>();
        Ok(identity(
            series.id(),
            json!({"period_raw": category, "period_param_raw": parameter, "period_name": label,
            "adjust_mode": adjust_name, "adjust_mode_raw": adjust_raw, "start": start,
            "request_count": if all { rows.len() } else { count }, "anchor_date": date_value(series.anchor_date()),
            "anchor_date_raw": series.anchor_date().map_or(0, date_number), "bars": rows,
            "raw_payload": if all { String::new() } else { raw(series.meta()) }}),
        ))
    })
}

fn minutes(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    let service = client.minutes();
    let signature: &[&str] = match method {
        "today" => &["code", ";", "include_raw", "batch_size"],
        "history" | "recent" => &["code", "trading_date", ";", "include_raw", "batch_size"],
        "aux" => &["code", "kind", ";", "include_raw"],
        "sparkline" => &["code", ";", "selector", "window", "include_raw"],
        _ => &["code", ";", "include_raw"],
    };
    let args = Arguments::bind(params, signature)?;
    let options = options(&args)?;
    let code = args.required("code")?;
    if matches!(method, "today" | "history" | "recent") {
        validate_batch(&args)?;
        let ids = securities(code)?;
        let batch = match method {
            "today" => service.today_many(&ids, &options)?,
            "history" => {
                service.history_many(&ids, date(args.required("trading_date")?)?, &options)?
            }
            _ => service.recent_many(
                &ids,
                &MinuteQuery::new().with_date(date_choice(&args, "trading_date")?)?,
                &options,
            )?,
        };
        return by_code(code.is_string(), &batch, |series| {
            let points = series.points().iter().enumerate().map(|(index, point)| json!({
                "index": point.index(), "time_label": time_label(point.time()), "time": event_time(point.time()),
                "price": point.price().as_f64(), "price_milli": milli(point.price()), "volume": point.volume().value().as_f64(),
                "avg_price": point.average().map(Price::as_f64), "price_field": null, "avg_field": null,
                "price_raw": null, "avg_raw": null, "price_delta_raw": null, "aux_delta_raw": null,
                "record_hex": record(series.meta(), index),
            })).collect::<Vec<_>>();
            Ok(identity(
                series.id(),
                json!({"trading_date": date_value(series.trading_date()), "points": points,
                "reserved_zero": (method == "today").then_some(0), "prev_close": series.previous_close().map(Price::as_f64),
                "open_price": series.open().map(Price::as_f64), "date_selector_raw": null, "raw_payload": raw(series.meta())}),
            ))
        });
    }
    let id = security(&args.get::<String>("code")?)?;
    if method == "sparkline" {
        let query = SparklineQuery::new()
            .with_selector(args.default("selector", 1)?)?
            .with_window(args.default("window", 20)?)?;
        let row = service.sparkline(&id, &query, &options)?;
        return Ok(identity(
            &id,
            json!({"selector_raw": row.selector(), "selector_echo": row.selector_echo(),
            "window_or_count_raw": row.requested_window(), "max_count_raw": row.max_count(), "base_price": row.base().as_f64(),
            "prices": row.prices().iter().map(|price| price.as_f64()).collect::<Vec<_>>(),
            "reserved_param_u32": row.reserved(), "raw_payload": raw(row.meta())}),
        ));
    }
    let (kind, selector, label) = match (method, args.value("kind")) {
        ("volume_comparison", _) => (AuxKind::VolumeComparison, 11, "volume_comparison"),
        ("buy_sell_strength", _) | ("aux", None | Some(Value::Null)) => {
            (AuxKind::BuySellStrength, 0, "buy_sell_strength")
        }
        (_, Some(value)) if value == "volume_comparison" || value == 11 => {
            (AuxKind::VolumeComparison, 11, "volume_comparison")
        }
        (_, Some(value)) if value == "buy_sell_strength" || value == 0 => {
            (AuxKind::BuySellStrength, 0, "buy_sell_strength")
        }
        _ => return Err(RpcError::value("unknown minute auxiliary kind")),
    };
    let row = service.aux(&id, kind, &options)?;
    let points = row.points().iter().enumerate().map(|(index, point)| {
        let mut fields = match point.values() {
            AuxValues::BuySell { buy_commission, sell_commission } => json!({"series_a": buy_commission, "series_b": sell_commission,
                "buy_commission": buy_commission, "sell_commission": sell_commission, "previous_day_cumulative_volume": null,
                "current_day_cumulative_volume": null, "cumulative_volume": null}),
            AuxValues::Volumes { previous, current, cumulative } => json!({"series_a": previous, "series_b": current,
                "buy_commission": null, "sell_commission": null, "previous_day_cumulative_volume": previous,
                "current_day_cumulative_volume": current, "cumulative_volume": cumulative}),
            _ => json!({}),
        };
        fields["index"] = json!(point.index());
        fields["time_label"] = json!(time_label(point.time()));
        fields["record_hex"] = json!(record(row.meta(), index));
        fields
    }).collect::<Vec<_>>();
    Ok(identity(
        &id,
        json!({"selector_raw": selector, "kind": label, "points": points, "raw_payload": raw(row.meta())}),
    ))
}

fn tick(row: &TradeTick, record_hex: String) -> Value {
    let side = match row.side() {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
        TradeSide::Neutral => "neutral",
        _ => "unknown",
    };
    let kind = match row.kind() {
        TradeEventKind::OpeningMatch => "opening_match",
        TradeEventKind::AuctionSnapshot => "auction_snapshot",
        _ => "trade",
    };
    json!({"index": row.index(), "absolute_index": row.absolute_index(), "time_label": time_label(row.time()),
        "time_minutes": row.time().clock().map(|(hour, minute, _)| u16::from(hour) * 60 + u16::from(minute)),
        "trade_datetime": event_time(row.time()), "price": row.price().as_f64(), "price_milli": milli(row.price()),
        "volume": row.volume().value().as_f64(), "order_count": row.order_count(), "status_raw": row.status_raw(),
        "side": side, "price_delta_raw": row.price_delta_raw(), "price_acc_raw": row.price_acc_raw(),
        "unknown_tail_raw": row.unknown_tail(), "reserved_zero": row.reserved_zero(), "record_hex": record_hex,
        "event_kind": kind, "auction_matched_volume": row.auction_matched().map(|value| value.value().as_f64()),
        "auction_unmatched_signed_volume": row.auction_unmatched_signed()})
}

fn trades(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    let opening = method.starts_with("opening_match");
    let all = method.starts_with("all_");
    let bulk = method.ends_with("_batch");
    let history = method.contains("history");
    let mut signature = vec!["code"];
    if history {
        signature.push("trading_date");
    }
    signature.extend([";", "include_raw", "batch_size"]);
    signature.extend(if opening || all {
        ["page_size", "max_pages"]
    } else {
        ["start", "count"]
    });
    let args = Arguments::bind(params, &signature)?;
    validate_batch(&args)?;
    let options = options(&args)?;
    let code = args.required("code")?;
    let ids = securities(code)?;
    let count = args.default(
        if all || opening { "page_size" } else { "count" },
        1800usize,
    )?;
    let start = args.default("start", 0u16)?;
    let mut query = TradeQuery::new().with_start(start)?.with_count(count)?;
    if history {
        query = query.with_date(eltdx::types::MarketDateChoice::Date(date(
            args.required("trading_date")?,
        )?))?;
    }
    if all || opening {
        query = query.with_pages(pages(&args, count, 100)?)?;
    }
    let service = client.trades();
    if opening {
        let batch = service.opening_match_many(&ids, &query, &options)?;
        return by_code(code.is_string(), &batch, |row| {
            Ok(row
                .as_ref()
                .map_or(Value::Null, |row| tick(row, String::new())))
        });
    }
    let project = |page: &eltdx::types::TradePage| {
        let ticks = page
            .ticks()
            .items()
            .iter()
            .enumerate()
            .map(|(index, row)| tick(row, record(page.meta(), index)))
            .collect::<Vec<_>>();
        Ok(identity(
            page.id(),
            json!({"start": start, "request_count": if all { ticks.len() } else { count }, "ticks": ticks,
            "trading_date": date_value(page.trading_date()), "price_base_raw_f32": page.price_base().map(Price::as_f64),
            "raw_payload": if all { String::new() } else { raw(page.meta()) }}),
        ))
    };
    if bulk {
        let batch = if all {
            service.collect_columns_many(&ids, &query, &options)?
        } else {
            service.columns_many(&ids, &query, &options)?
        };
        by_code(code.is_string(), &batch, |columns| {
            project(&columns.to_page())
        })
    } else {
        let batch = if all {
            service.collect_many(&ids, &query, &options)?
        } else {
            service.page_many(&ids, &query, &options)?
        };
        by_code(code.is_string(), &batch, project)
    }
}

fn auctions(client: &TdxClient, params: Value) -> Result<Value> {
    let args = Arguments::bind(params, &["code", "date", ";", "include_raw"])?;
    let id = security(&args.get::<String>("code")?)?;
    let query = AuctionQuery::new().with_date(date_choice(&args, "date")?)?;
    let row = client.auctions().series(&id, &query, &options(&args)?)?;
    let points = row.points().iter().enumerate().map(|(index, point)| json!({
        "index": point.index(), "minute_of_day_raw": point.minute_of_day_raw(), "second_raw": point.second_raw(),
        "time_label": format!("{}:{:02}", time_label(point.time()), point.second_raw()),
        "time_seconds": u32::from(point.minute_of_day_raw()) * 60 + u32::from(point.second_raw()),
        "price": point.price().as_f64(), "price_milli": milli(point.price()), "matched_volume": point.matched().value().as_f64(),
        "unmatched_signed_raw": point.unmatched_signed(), "unmatched_volume": point.unmatched().value().as_f64(),
        "unmatched_direction_raw": point.direction_raw(), "reserved_zero_0e": point.reserved(), "record_hex": record(row.meta(), index),
    })).collect::<Vec<_>>();
    Ok(identity(
        &id,
        json!({"trading_date": date_value(row.trading_date()), "mode_or_selector_raw": row.mode_selector(),
        "start_raw": row.start(), "limit_or_count_raw": row.requested(), "points": points, "raw_payload": raw(row.meta())}),
    ))
}

fn corporate(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    let service = client.corporate();
    if method == "finance_batch" {
        let args = Arguments::bind(
            params,
            &["codes", "fields", ";", "include_raw", "on_missing"],
        )?;
        let ids = securities(args.required("codes")?)?;
        let fields = args.optional::<Vec<String>>("fields")?;
        let query = FinanceQuery::new().with_missing(missing_policy(&args)?)?;
        let batch = service.finance(&ids, &query, &options(&args)?)?;
        return super::super::finance::batch(&batch, fields.as_deref());
    }
    let signature: &[&str] = if method == "capital_changes" {
        &["code", ";", "include_raw", "batch_size"]
    } else {
        &["code", "anchor_date", ";", "start_date", "batch_size"]
    };
    let args = Arguments::bind(params, signature)?;
    let code = args.required("code")?;
    let ids = securities(code)?;
    let query = CapitalQuery::new().with_batch_size(args.default("batch_size", 75)?)?;
    let options = options(&args)?;
    if method == "capital_changes" {
        let batch = service.capital_changes(&ids, &query, &options)?;
        let raw_payloads = batch
            .meta()
            .raw()
            .into_iter()
            .flat_map(|data| data.payloads())
            .map(|bytes| hex(bytes))
            .collect::<Vec<_>>();
        return corporate_rows(code.is_string(), &batch, "blocks", |block| {
            let records = block.records().iter().enumerate().map(|(index, row)| {
                let value = row.coefficients();
                let bits = row.coefficient_bits();
                let wire = bits.map(f32::from_le_bytes);
                identity(block.id(), json!({"reserved_7": row.reserved(), "date_raw": row.date_raw(), "date": date_value(row.date()),
                    "category_raw": row.category(), "category_name": row.category_name(), "c1_raw": hex(&bits[0]), "c2_raw": hex(&bits[1]),
                    "c3_raw": hex(&bits[2]), "c4_raw": hex(&bits[3]), "c1_float": wire[0], "c2_float": wire[1], "c3_float": wire[2],
                    "c4_float": wire[3], "c1_value": value[0], "c2_value": value[1], "c3_value": value[2], "c4_value": value[3],
                    "record_hex": record(block.meta(), index)}))
            }).collect::<Vec<_>>();
            identity(block.id(), json!({"block_count": block.declared_blocks(), "records": records, "raw_payload": raw(block.meta())}))
        }).map(|mut result| {
            if !code.is_string() { result["raw_payloads"] = json!(raw_payloads); }
            result
        });
    }
    let query = FactorQuery::new()
        .with_anchor_date(optional_date(&args, "anchor_date")?)?
        .with_start_date(optional_date(&args, "start_date")?)?
        .with_capital(query)?;
    let batch = service.adjustment_factors(&ids, &query, &options)?;
    corporate_rows(code.is_string(), &batch, "responses", |row| {
        let items = row.rows().iter().map(|factor| json!({"date": date_value(Some(factor.date())),
            "qfq_scale": factor.qfq_scale(), "qfq_offset": factor.qfq_offset(), "hfq_scale": factor.hfq_scale(), "hfq_offset": factor.hfq_offset()})).collect::<Vec<_>>();
        identity(
            row.id(),
            json!({"anchor_date": date_value(row.anchor_date()), "start_date": date_value(row.start_date()), "items": items}),
        )
    })
}

fn corporate_rows<T>(
    single: bool,
    batch: &BatchOutcome<T>,
    name: &str,
    project: impl Fn(&T) -> Value,
) -> Result<Value> {
    if single {
        return by_code(true, batch, |row| Ok(project(row)));
    }
    let rows = batch
        .items()
        .iter()
        .filter_map(|item| match item.state() {
            BatchItemState::Present(row) => Some(project(row)),
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(json!({(name): rows}))
}
