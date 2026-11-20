use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use eltdx::types::{
    AdjustMode, AuctionQuery, KlinePeriod, KlineQuery, MarketDateChoice, ResultMeta, TradeQuery,
};
use eltdx::{
    AuctionDataQuery, BatchItemState, BatchOutcome, CapitalQuery, DailyLimitsQuery, FactorQuery,
    OperationOptions, Price, ProfileQuery, SecurityId, SecuritySelection, ShortlineQuery, Snapshot,
    StatsQuery, TdxClient, TradingDate,
};
use serde_json::{json, Map, Value};

use super::ToolResult;

#[derive(Default)]
pub(super) struct Clients {
    entries: VecDeque<(Option<String>, TdxClient)>,
}

impl Clients {
    fn client(&mut self, host: Option<&str>) -> Result<&TdxClient, Box<dyn std::error::Error>> {
        let host = host.map(str::trim);
        if let Some(index) = self
            .entries
            .iter()
            .position(|(key, _)| key.as_deref() == host)
        {
            if let Some(entry) = self.entries.remove(index) {
                self.entries.push_front(entry);
            }
        } else {
            let mut builder = TdxClient::builder().pool_size(4);
            if let Some(host) = host {
                let endpoints = endpoints(host)?;
                builder = builder
                    .endpoints(endpoints.clone())
                    .auction_endpoints(endpoints.clone())
                    .money_flow_endpoints(endpoints);
            }
            let client = builder.build()?;
            if self.entries.len() == 16 {
                if let Some((_, evicted)) = self.entries.pop_back() {
                    evicted.close()?;
                }
            }
            self.entries.push_front((host.map(str::to_owned), client));
        }
        self.entries
            .front()
            .map(|(_, client)| client)
            .ok_or_else(|| "Client unavailable".into())
    }

    pub(super) fn call(
        &mut self,
        name: &str,
        args: &Value,
        options: &OperationOptions,
    ) -> ToolResult {
        let client = self.client(args["host"].as_str())?;
        match name {
            "quote" => {
                let ids = codes(&args["codes"], 200)?;
                batch_array(&client.quotes().snapshots(&ids, options)?, |row| {
                    Ok(snapshot(row))
                })
            }
            "quote_depth" => {
                let ids = codes(&args["codes"], 100)?;
                let depth_options = options.clone().with_raw(eltdx::RawPolicy::RetainPayload);
                let result = client.quotes().depth(&ids, &depth_options)?;
                let records = result
                    .updates()
                    .iter()
                    .map(|update| {
                        let mut record = snapshot(update.snapshot());
                        if let Some(fields) = record.as_object_mut() {
                            fields.remove("active1");
                            fields.remove("time_raw");
                            fields.remove("unknown_after_time_raw");
                            if let Some(last_close) = fields.remove("pre_close_price") {
                                fields.insert("last_close_price".into(), last_close);
                            }
                            fields.insert("active".into(), json!(update.active()));
                            fields.insert("update_time_raw".into(), json!(update.cursor()));
                            fields.insert(
                                "status_or_reserved_raw".into(),
                                json!(update.status_raw()),
                            );
                            fields.insert("record_hex".into(), json!(""));
                        }
                        record
                    })
                    .collect::<Vec<_>>();
                Ok(json!({
                    "requested_codes": result.requested().iter().map(|id| format!("{id}")).collect::<Vec<_>>(),
                    "records": records, "decoded_payload": "", "raw_payload": raw_payload(result.meta()),
                }))
            }
            "kline" => {
                let (ids, single) = code_argument(args)?;
                let query = KlineQuery::new()
                    .with_period(period(args["period"].as_str().unwrap_or("day"))?)?
                    .with_start(integer(args, "start", 0, 0, u16::MAX.into())? as u16)?
                    .with_count(integer(args, "count", 120, 1, 800)?)?
                    .with_adjust(adjust(args["adjust"].as_str())?)?
                    .with_anchor_date(date(&args["anchor_date"])?)?;
                let result = client.klines().page_many(&ids, &query, options)?;
                per_code(&result, single, |row| {
                    let (exchange, market_id, code) = identity(row.id());
                    let (period_raw, period_param_raw) = period_fields(query.period());
                    Ok(json!({
                        "exchange": exchange, "market_id": market_id, "code": code,
                        "period_raw": period_raw, "period_param_raw": period_param_raw,
                        "period_name": args["period"].as_str().unwrap_or("day"),
                        "start": query.start(), "request_count": query.count(), "bars": row.bars().items(),
                        "adjust_mode": args["adjust"].as_str().unwrap_or("none"),
                        "anchor_date": row.anchor_date().map(|date| format!("{date}")),
                        "raw_payload": raw_payload(row.meta()),
                    }))
                })
            }
            "capital_changes" | "adjustment_factors" => {
                let ids = codes(&args["codes"], 200)?;
                let capital = CapitalQuery::new().with_batch_size(integer(
                    args,
                    "batch_size",
                    75,
                    1,
                    200,
                )?)?;
                if name == "capital_changes" {
                    let outcome = client
                        .corporate()
                        .capital_changes(&ids, &capital, options)?;
                    let blocks = batch_array(&outcome, |row| {
                        let (exchange, market_id, code) = identity(row.id());
                        Ok(json!({
                            "exchange": exchange, "market_id": market_id, "code": code,
                            "block_count": row.declared_blocks(),
                            "records": row.records().iter().map(|event| {
                                let [first, second, third, fourth] = *event.coefficients();
                                let [raw_first, raw_second, raw_third, raw_fourth] = *event.coefficient_bits();
                                json!({
                                    "exchange": exchange, "market_id": market_id, "code": code,
                                    "reserved_7": event.reserved(), "date_raw": event.date_raw(),
                                    "date": event.date().map(|date| format!("{date}")),
                                    "category_raw": event.category(), "category_name": event.category_name(),
                                    "c1_raw": hex(&raw_first), "c2_raw": hex(&raw_second),
                                    "c3_raw": hex(&raw_third), "c4_raw": hex(&raw_fourth),
                                    "c1_float": f32::from_le_bytes(raw_first), "c2_float": f32::from_le_bytes(raw_second),
                                    "c3_float": f32::from_le_bytes(raw_third), "c4_float": f32::from_le_bytes(raw_fourth),
                                    "c1_value": first, "c2_value": second, "c3_value": third, "c4_value": fourth,
                                    "record_hex": "",
                                })
                            }).collect::<Vec<_>>(),
                            "raw_payload": raw_payload(row.meta()),
                        }))
                    })?;
                    let payloads = outcome
                        .meta()
                        .raw()
                        .into_iter()
                        .flat_map(|raw| raw.payloads())
                        .map(|payload| hex(payload))
                        .collect::<Vec<_>>();
                    Ok(json!({"blocks": blocks, "raw_payloads": payloads}))
                } else {
                    let query = FactorQuery::new()
                        .with_capital(capital)?
                        .with_anchor_date(date(&args["anchor_date"])?)?
                        .with_start_date(date(&args["start_date"])?)?;
                    let outcome = client
                        .corporate()
                        .adjustment_factors(&ids, &query, options)?;
                    let responses = batch_array(&outcome, |row| {
                        let (exchange, market_id, code) = identity(row.id());
                        Ok(json!({
                            "exchange": exchange, "market_id": market_id, "code": code,
                            "anchor_date": row.anchor_date().map(|date| format!("{date}")),
                            "start_date": row.start_date().map(|date| format!("{date}")),
                            "items": row.rows().iter().map(|factor| json!({
                                "date": format!("{}", factor.date()),
                                "qfq_scale": factor.qfq_scale(), "qfq_offset": factor.qfq_offset(),
                                "hfq_scale": factor.hfq_scale(), "hfq_offset": factor.hfq_offset(),
                            })).collect::<Vec<_>>(),
                        }))
                    })?;
                    Ok(json!({"responses": responses}))
                }
            }
            "money_flow" => {
                let ids = codes(&args["codes"], 200)?;
                let batch_size = integer(args, "batch_size", 75, 1, 200)?;
                let mut blocks = Vec::new();
                for chunk in ids.chunks(batch_size) {
                    let outcome = client.money_flow().daily_many(chunk, options)?;
                    if let Value::Array(items) = batch_array(&outcome, |row| {
                        let (exchange, market_id, code) = identity(row.id());
                        Ok(json!({
                            "exchange": exchange, "market_id": market_id, "code": code,
                            "records": row.records().iter().map(|record| {
                                let [super_large, large, medium, small] = record.main_size_net();
                                let [buy_super_large, buy_large, buy_medium, buy_small] = record.main_buy_size_net();
                                json!({
                                    "date_raw": record.date_raw(),
                                    "date": record.date().map(|date| format!("{date}")),
                                    "total_amount": record.total_amount().value().as_f64(),
                                    "buckets": record.buckets(), "main_net": record.main_net().value().as_f64(),
                                    "main_ratio": record.main_ratio().value().as_f64(), "raw": record.raw_values(),
                                    "main_buy_net": record.main_buy_net().value().as_f64(),
                                    "main_buy_ratio": record.main_buy_ratio().value().as_f64(),
                                    "main_buy_super_large_net": buy_super_large.value().as_f64(),
                                    "main_buy_large_net": buy_large.value().as_f64(),
                                    "main_buy_medium_net": buy_medium.value().as_f64(),
                                    "main_buy_small_net": buy_small.value().as_f64(),
                                    "main_super_large_net": super_large.value().as_f64(),
                                    "main_large_net": large.value().as_f64(),
                                    "main_medium_net": medium.value().as_f64(),
                                    "main_small_net": small.value().as_f64(),
                                    "record_hex": "",
                                })
                            }).collect::<Vec<_>>(),
                        }))
                    })? {
                        blocks.extend(items);
                    }
                }
                Ok(json!({"blocks": blocks}))
            }
            "daily_price_limits" => {
                let mut query = DailyLimitsQuery::new(
                    date(&args["trade_date"])?.ok_or("trade_date is required")?,
                );
                if !args["codes"].is_null() {
                    query = query
                        .with_selection(SecuritySelection::Explicit(codes(&args["codes"], 200)?))?;
                }
                let table = client.helpers().daily_price_limits(&query, options)?;
                Ok(json!(table.rows().iter().map(|row| json!({
                    "code": format!("{}", row.id()), "trade_date": format!("{}", row.trade_date()),
                    "name": row.name(), "reference_date": row.reference_date().map(|date| format!("{date}")),
                    "reference_price": row.reference().map(Price::as_f64),
                    "limit_up": row.upper().map(Price::as_f64), "limit_down": row.lower().map(Price::as_f64),
                    "status": format!("{:?}", row.status()), "reference_source": row.reference_source(),
                })).collect::<Vec<_>>()))
            }
            "minute" => {
                let (ids, single) = code_argument(args)?;
                let result = match date(&args["trading_date"])? {
                    Some(date) => client.minutes().history_many(&ids, date, options)?,
                    None => client.minutes().today_many(&ids, options)?,
                };
                per_code(&result, single, |row| {
                    let (exchange, market_id, code) = identity(row.id());
                    Ok(json!({
                        "exchange": exchange, "market_id": market_id, "code": code,
                        "trading_date": row.trading_date().map(|date| format!("{date}")),
                        "prev_close": row.previous_close().map(Price::as_f64),
                        "open_price": row.open().map(Price::as_f64),
                        "points": row.points(),
                        "raw_payload": raw_payload(row.meta()),
                    }))
                })
            }
            "trades" => {
                let (ids, single) = code_argument(args)?;
                let query = TradeQuery::new()
                    .with_date(date_choice(&args["trading_date"])?)?
                    .with_start(integer(args, "start", 0, 0, u16::MAX.into())? as u16)?
                    .with_count(integer(args, "count", 500, 1, 1800)?)?;
                let result = client.trades().page_many(&ids, &query, options)?;
                per_code(&result, single, |row| {
                    let (exchange, market_id, code) = identity(row.id());
                    Ok(json!({
                        "exchange": exchange, "market_id": market_id, "code": code,
                        "trading_date": row.trading_date().map(|date| format!("{date}")),
                        "start": query.start(), "request_count": query.count(), "ticks": row.ticks().items(),
                        "price_base_raw_f32": row.price_base().map(Price::as_f64),
                        "raw_payload": raw_payload(row.meta()),
                    }))
                })
            }
            "call_auction" => {
                let query = AuctionQuery::new().with_date(date_choice(&args["trading_date"])?)?;
                serialize(
                    &client
                        .auctions()
                        .series(&code(&args["code"])?, &query, options)?,
                )
            }
            "opening_match_today" | "opening_match_history" => {
                let query = TradeQuery::new().with_date(date_choice(&args["trading_date"])?)?;
                serialize(
                    &client
                        .trades()
                        .opening_match(&code(&args["code"])?, &query, options)?,
                )
            }
            "auction_data" => {
                let query = AuctionDataQuery::new()
                    .with_date(date_choice(&args["trading_date"])?)?
                    .with_series(args["include_series"].as_bool().unwrap_or(true))?
                    .with_snapshot(args["include_snapshot"].as_bool().unwrap_or(true))?
                    .with_quote(args["include_quote"].as_bool().unwrap_or(true))?
                    .with_pre_close(args["pre_close_price"].as_f64().map(Price::from_f64))?;
                let row = client
                    .helpers()
                    .auction_data(&code(&args["code"])?, &query, options)?;
                Ok(json!({
                    "code": format!("{}", row.id()), "trading_date": format!("{}", row.date()),
                    "series": row.series(), "opening_match": row.opening_match(),
                    "pre_close": row.pre_close().map(Price::as_f64), "open": row.open().map(Price::as_f64),
                    "open_volume": row.open_volume(), "open_amount": row.open_amount(),
                    "open_change": row.open_change(),
                }))
            }
            "stock_profile" => {
                let query = ProfileQuery::new()
                    .with_security(args["include_security"].as_bool().unwrap_or(true))?
                    .with_finance(args["include_finance"].as_bool().unwrap_or(true))?;
                let table = client.helpers().stock_profile_table(
                    &codes(&args["codes"], 200)?,
                    &query,
                    options,
                )?;
                Ok(json!(table.rows().iter().map(|row| json!({
                    "code": format!("{}", row.id()), "name": row.name(),
                    "last": row.last().map(Price::as_f64), "pre_close": row.pre_close().map(Price::as_f64),
                    "open": row.open().map(Price::as_f64), "high": row.high().map(Price::as_f64),
                    "low": row.low().map(Price::as_f64), "change": row.change().map(Price::as_f64),
                    "change_pct": row.change_pct(), "volume": row.volume(), "amount": row.amount(),
                    "open_amount": row.open_amount(), "circulating_shares": row.circulating_shares(),
                    "total_shares": row.total_shares(), "turnover": row.turnover(),
                    "circulating_market_value": row.circulating_market_value(),
                    "total_market_value": row.total_market_value(), "eps": row.eps().map(Price::as_f64),
                })).collect::<Vec<_>>()))
            }
            "shortline_indicators" => {
                let stats = StatsQuery::new()
                    .with_path(args["stats_path"].as_str().unwrap_or("zhb.zip").to_owned())?;
                let query = ShortlineQuery::new(codes(&args["codes"], 200)?)?
                    .with_stats(stats)?
                    .with_refresh_stats(args["refresh_stats"].as_bool().unwrap_or(false))?;
                let table = client.helpers().shortline_indicators(&query, options)?;
                Ok(json!({
                    "trade_date": format!("{}", table.target()), "previous_date": format!("{}", table.previous()),
                    "stats_date": format!("{}", table.stats_date()), "stats_path": table.stats_path(),
                    "stats_refreshed": table.stats_refreshed(), "rows": table.rows().iter().map(|row| json!({
                        "code": format!("{}", row.id()), "beta_60d": row.beta_60d(), "pe_ttm": row.pe_ttm(),
                        "free_float_shares": row.free_float_shares(), "float_shares": row.float_shares(),
                        "previous_amount": row.previous_amount(), "previous_seal": row.previous_seal(),
                        "previous2_seal": row.previous2_seal(), "previous_open_amount": row.previous_open_amount(),
                        "previous_open_volume": row.previous_open_volume(), "limit_stat_days": row.limit_stat_days(),
                        "limit_up_count": row.limit_up_count(), "limit_streak": row.limit_streak(),
                        "year_limit_up_days": row.year_limit_up_days(), "free_float_market_value": row.free_float_market_value(),
                        "float_market_value": row.float_market_value(), "open_turnover": row.open_turnover(),
                        "open_previous_amount": row.open_previous_amount(), "open_previous_seal": row.open_previous_seal(),
                        "seal_to_float": row.seal_to_float(), "open_change": row.open_change(),
                        "auction_previous_volume": row.auction_previous_volume(), "seal_previous": row.seal_previous(),
                        "open_volume_ratio": row.open_volume_ratio(), "seal_to_amount": row.seal_to_amount(),
                        "board_text": row.board_text(), "ladder_level": row.ladder_level(),
                        "open": row.open().map(Price::as_f64), "pre_close": row.pre_close().map(Price::as_f64),
                        "open_amount": row.open_amount(), "seal_amount": row.seal_amount(),
                        "open_volume": row.open_volume(), "opening_rush": row.opening_rush(),
                    })).collect::<Vec<_>>(),
                }))
            }
            _ => Err("Unknown HQ tool".into()),
        }
    }
}

impl Drop for Clients {
    fn drop(&mut self) {
        for (_, client) in self.entries.drain(..) {
            if let Err(error) = client.close() {
                eprintln!("MCP client cleanup failed: {error}");
            }
        }
    }
}

fn endpoints(host: &str) -> Result<Vec<SocketAddr>, Box<dyn std::error::Error>> {
    if let Ok(endpoint) = host.parse::<SocketAddr>() {
        return Ok(vec![endpoint]);
    }
    if let Ok(ip) = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
    {
        return Ok(vec![SocketAddr::new(ip, 7709)]);
    }
    let endpoints: Vec<_> = if host.contains(':') {
        host.to_socket_addrs()?.collect()
    } else {
        (host, 7709).to_socket_addrs()?.collect()
    };
    if endpoints.is_empty() || endpoints.iter().any(|address| address.port() == 0) {
        return Err("host must resolve to a valid endpoint".into());
    }
    Ok(endpoints)
}

fn serialize<T: serde::Serialize + ?Sized>(value: &T) -> ToolResult {
    Ok(serde_json::to_value(value)?)
}

fn raw_payload(meta: &ResultMeta) -> String {
    meta.raw()
        .and_then(|raw| raw.payloads().first())
        .map(|bytes| hex(bytes))
        .unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut hex = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn identity(id: &SecurityId) -> (&str, u8, &str) {
    use eltdx::HqMarket;

    let market_id = match id.market() {
        HqMarket::Shenzhen => 0,
        HqMarket::Shanghai => 1,
        HqMarket::Beijing => 2,
        _ => 0,
    };
    (id.market().code(), market_id, id.number())
}

fn snapshot(row: &Snapshot) -> Value {
    let (exchange, market_id, code) = identity(row.id());
    let levels = |levels: &[eltdx::types::QuoteLevel]| {
        levels
            .iter()
            .map(|level| {
                json!({
                    "price": level.price().as_f64(), "volume": level.volume().value().as_f64(),
                })
            })
            .collect::<Vec<_>>()
    };
    json!({
        "exchange": exchange, "market_id": market_id, "code": code,
        "active1": row.active(), "last_price": row.last().as_f64(),
        "pre_close_price": row.pre_close().as_f64(), "open_price": row.open().as_f64(),
        "high_price": row.high().as_f64(), "low_price": row.low().as_f64(),
        "time_raw": row.server_time_raw(), "unknown_after_time_raw": row.unknown_after_time_raw(),
        "total_hand": row.total().value().as_f64(), "current_hand": row.current().value().as_f64(),
        "amount": row.amount().value().as_f64(), "amount_raw": row.amount_wire_word(),
        "inside_dish": row.inside().value().as_f64(), "outer_disc": row.outside().value().as_f64(),
        "unknown_after_outer_raw": row.unknown_after_outer_raw(), "open_amount_raw": row.open_amount_raw(),
        "open_amount_yuan": row.open_amount().value().as_f64(),
        "buy_levels": levels(row.bids()), "sell_levels": levels(row.asks()), "tail_raw": "",
    })
}

fn batch_array<T>(outcome: &BatchOutcome<T>, encode: impl Fn(&T) -> ToolResult) -> ToolResult {
    outcome
        .items()
        .iter()
        .map(|item| match item.state() {
            BatchItemState::Present(value) => encode(value),
            _ => Ok(Value::Null),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn per_code<T>(
    outcome: &BatchOutcome<T>,
    single: bool,
    encode: impl Fn(&T) -> ToolResult,
) -> ToolResult {
    let values = batch_array(outcome, encode)?;
    let Value::Array(values) = values else {
        return Err("Expected batch rows".into());
    };
    if single {
        return Ok(values.into_iter().next().unwrap_or_default());
    }
    Ok(outcome
        .items()
        .iter()
        .map(|item| format!("{}", item.security()))
        .zip(values)
        .collect::<Map<_, _>>()
        .into())
}

fn integer(
    args: &Value,
    name: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let value = if args[name].is_null() {
        default
    } else {
        args[name]
            .as_u64()
            .and_then(|value| value.try_into().ok())
            .unwrap_or(0)
    };
    if !(min..=max).contains(&value) || args[name].as_i64().is_some_and(|value| value < 0) {
        return Err(format!("{name} must be between {min} and {max}").into());
    }
    Ok(value)
}

fn code(value: &Value) -> Result<SecurityId, Box<dyn std::error::Error>> {
    Ok(value
        .as_str()
        .ok_or("Expected a security code")?
        .trim()
        .parse()?)
}

fn codes(value: &Value, maximum: usize) -> Result<Vec<SecurityId>, Box<dyn std::error::Error>> {
    if let Some(values) = value.as_array() {
        if values.is_empty() || values.len() > maximum {
            return Err(format!("Expected 1–{maximum} security codes").into());
        }
        return values.iter().map(code).collect();
    }
    Ok(vec![code(value)?])
}

fn code_argument(args: &Value) -> Result<(Vec<SecurityId>, bool), Box<dyn std::error::Error>> {
    integer(args, "batch_size", 200, 1, 200)?;
    Ok((codes(&args["code"], 200)?, args["code"].is_string()))
}

fn date(value: &Value) -> Result<Option<TradingDate>, Box<dyn std::error::Error>> {
    if value.is_null() {
        return Ok(None);
    }
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{value}"));
    let compact = text.replace('-', "");
    if compact.len() != 8 || !compact.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Expected YYYY-MM-DD or YYYYMMDD".into());
    }
    Ok(Some(TradingDate::new(
        compact[..4].parse()?,
        compact[4..6].parse()?,
        compact[6..].parse()?,
    )?))
}

fn date_choice(value: &Value) -> Result<MarketDateChoice, Box<dyn std::error::Error>> {
    Ok(date(value)?.map_or(MarketDateChoice::Current, MarketDateChoice::Date))
}

fn period(value: &str) -> Result<KlinePeriod, Box<dyn std::error::Error>> {
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "1m" | "1min" | "min1" => KlinePeriod::Min1,
        "5m" | "5min" | "min5" => KlinePeriod::Min5,
        "15m" | "15min" | "min15" => KlinePeriod::Min15,
        "30m" | "30min" | "min30" => KlinePeriod::Min30,
        "60m" | "60min" | "min60" => KlinePeriod::Min60,
        "d" | "day" | "daily" => KlinePeriod::Day,
        "w" | "week" | "weekly" => KlinePeriod::Week,
        "mon" | "month" | "monthly" => KlinePeriod::Month,
        "q" | "quarter" => KlinePeriod::Quarter,
        "y" | "year" => KlinePeriod::Year,
        other => {
            let unit_start = other
                .find(|character: char| !character.is_ascii_digit())
                .ok_or_else(|| format!("Unsupported K-line period: {value}"))?;
            let (width, unit) = other.split_at(unit_start);
            match unit {
                "m" | "min" => KlinePeriod::minutes(width.parse()?)?,
                "d" | "day" => KlinePeriod::days(width.parse()?)?,
                "s" | "sec" => KlinePeriod::seconds(width.parse()?)?,
                _ => return Err(format!("Unsupported K-line period: {value}").into()),
            }
        }
    })
}

fn period_fields(period: KlinePeriod) -> (u16, u16) {
    match period {
        KlinePeriod::Min5 => (0, 1),
        KlinePeriod::Min15 => (1, 1),
        KlinePeriod::Min30 => (2, 1),
        KlinePeriod::Min60 => (3, 1),
        KlinePeriod::Day => (4, 1),
        KlinePeriod::Week => (5, 1),
        KlinePeriod::Month => (6, 1),
        KlinePeriod::Min1 => (7, 1),
        KlinePeriod::Minutes(width) => (8, width),
        KlinePeriod::Days(width) => (9, width),
        KlinePeriod::Quarter => (10, 1),
        KlinePeriod::Year => (11, 1),
        KlinePeriod::Seconds(width) => (13, width),
        _ => (4, 1),
    }
}

fn adjust(value: Option<&str>) -> Result<AdjustMode, Box<dyn std::error::Error>> {
    Ok(
        match value.unwrap_or("none").to_ascii_lowercase().as_str() {
            "none" => AdjustMode::None,
            "qfq" => AdjustMode::Qfq,
            "hfq" => AdjustMode::Hfq,
            "fixed_qfq" => AdjustMode::FixedQfq,
            "fixed_hfq" => AdjustMode::FixedHfq,
            _ => return Err("Unsupported adjustment mode".into()),
        },
    )
}
