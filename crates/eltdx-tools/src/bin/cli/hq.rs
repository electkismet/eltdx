use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};

use eltdx::types::*;
use eltdx::{OperationOptions, TdxClient};

use super::args::*;
use super::output::{object, scalar, Value};
use super::{codes, finish, hex, invalid, operation, single_code, timeout, trade_date, Result};

fn with_client(
    args: Hq,
    raw: bool,
    call: impl FnOnce(&TdxClient, &OperationOptions) -> Result<Value>,
) -> Result<Value> {
    let duration = timeout(args.timeout)?;
    let mut builder = TdxClient::builder().request_timeout(duration);
    if let Some(host) = args.host.as_deref().filter(|host| !host.is_empty()) {
        let endpoint = if let Ok(ip) = host.parse::<IpAddr>() {
            SocketAddr::new(ip, 7709)
        } else {
            host.parse()?
        };
        builder = builder
            .endpoints(vec![endpoint])
            .money_flow_endpoints(vec![endpoint]);
    }
    let client = builder.build()?;
    finish(call(&client, &operation(duration, raw)), client.close())
}

pub(super) fn ids(values: &[String]) -> Result<Vec<SecurityId>> {
    codes(values)
        .into_iter()
        .map(|code| code.parse().map_err(Into::into))
        .collect()
}

fn unique_ids(values: &[String]) -> Result<Vec<SecurityId>> {
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids(values)?
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect();
    if ids.is_empty() {
        return Err(invalid("codes must not be empty"));
    }
    Ok(ids)
}

pub(super) fn market_id(market: HqMarket) -> u8 {
    match market {
        HqMarket::Shanghai => 1,
        HqMarket::Beijing => 2,
        _ => 0,
    }
}

fn date_text(date: Option<TradingDate>) -> Option<String> {
    date.map(|date| date.to_string())
}

fn datetime(time: EventTime, date_only: bool) -> Option<String> {
    let date = time.date()?;
    let (hour, minute, second) = time.clock().or(date_only.then_some((0, 0, 0)))?;
    Some(format!("{date}T{hour:02}:{minute:02}:{second:02}"))
}

fn payload(meta: &ResultMeta, enabled: bool) -> String {
    if !enabled {
        return String::new();
    }
    meta.raw()
        .and_then(|raw| raw.payloads().first())
        .map(|raw| hex(raw))
        .unwrap_or_default()
}

fn record(meta: &ResultMeta, index: usize) -> Option<Vec<u8>> {
    let raw = meta.raw()?;
    let &(page, start, end) = raw.row_ranges().get(index)?;
    let bytes = raw.payloads().get(page)?.get(start..end)?;
    Some(if raw.row_requires_xor93(index) == Some(true) {
        bytes.iter().map(|byte| byte ^ 0x93).collect()
    } else {
        bytes.to_vec()
    })
}

fn quantity(value: Quantity) -> i64 {
    match value.source_value() {
        NumericValue::ScaledInteger { value, scale: 0 } => value,
        other => other.as_f64() as i64,
    }
}

pub(super) fn ping(args: Hq) -> Result<Value> {
    with_client(args, false, |client, _| {
        client.health().ping()?;
        Ok(object! { "host" => "pong" })
    })
}

pub(super) fn workday(args: Workday) -> Result<Value> {
    let date = trade_date(args.date.as_deref())?;
    with_client(args.common, false, |client, options| {
        let calendar = client.calendar();
        let target = match date {
            Some(date) => date,
            None => calendar.today()?,
        };
        let snapshot = calendar.refresh(&CalendarQuery::new(), options)?;
        Ok(Value::Object(vec![
            (
                if date.is_some() { "date" } else { "today" }.to_owned(),
                target.to_string().into(),
            ),
            (
                "is_workday".to_owned(),
                snapshot.days().binary_search(&target).is_ok().into(),
            ),
        ]))
    })
}

pub(super) fn quote(args: Quote) -> Result<Value> {
    let ids = ids(&args.code)?;
    with_client(args.common, true, |client, options| {
        if args.depth {
            let quotes = client.quotes().depth(&ids, options)?;
            let records = quotes
                .updates()
                .iter()
                .enumerate()
                .map(|(index, update)| {
                    quote_row(
                        update.snapshot(),
                        Some(update),
                        record(quotes.meta(), index).as_deref(),
                    )
                })
                .collect();
            let raw = quotes.meta().raw().and_then(|raw| raw.payloads().first());
            let decoded =
                raw.map(|raw| hex(&raw.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>()));
            Ok(Value::Object(vec![
                (
                    "requested_codes".to_owned(),
                    scalar(ids.iter().map(ToString::to_string).collect::<Vec<_>>()),
                ),
                ("records".to_owned(), Value::Array(records)),
                (
                    "decoded_payload".to_owned(),
                    decoded.unwrap_or_default().into(),
                ),
                ("raw_payload".to_owned(), "".into()),
            ]))
        } else {
            let quotes = client.quotes().snapshots(&ids, options)?;
            Ok(Value::Array(
                quotes
                    .items()
                    .iter()
                    .filter_map(|item| match item.state() {
                        BatchItemState::Present(row) => {
                            Some(quote_row(row, None, record(row.meta(), 0).as_deref()))
                        }
                        _ => None,
                    })
                    .collect(),
            ))
        }
    })
}

fn quote_row(snapshot: &Snapshot, update: Option<&QuoteUpdate>, raw: Option<&[u8]>) -> Value {
    let id = snapshot.id();
    let levels = |levels: &[QuoteLevel]| {
        Value::Array(levels.iter().map(|level| {
        let delta = match (level.price().value(), snapshot.last().value()) {
            (NumericValue::ScaledInteger { value: level, .. }, NumericValue::ScaledInteger { value: last, .. }) => Some(level.saturating_sub(last)),
            _ => None,
        };
        object! { "price" => level.price().as_f64(), "volume" => quantity(level.volume()), "price_delta_raw" => delta }
    }).collect())
    };
    let mut fields = vec![
        ("exchange".to_owned(), id.market().code().into()),
        ("market_id".to_owned(), scalar(market_id(id.market()))),
        ("code".to_owned(), id.number().into()),
    ];
    if let Some(update) = update {
        fields.extend([
            ("active".to_owned(), scalar(update.active())),
            ("update_time_raw".to_owned(), scalar(update.cursor())),
        ]);
    } else {
        fields.push(("active1".to_owned(), scalar(snapshot.active())));
    }
    fields.extend([
        ("last_price".to_owned(), scalar(snapshot.last().as_f64())),
        (
            if update.is_some() {
                "last_close_price"
            } else {
                "pre_close_price"
            }
            .to_owned(),
            scalar(snapshot.pre_close().as_f64()),
        ),
        ("open_price".to_owned(), scalar(snapshot.open().as_f64())),
        ("high_price".to_owned(), scalar(snapshot.high().as_f64())),
        ("low_price".to_owned(), scalar(snapshot.low().as_f64())),
    ]);
    if let Some(update) = update {
        fields.push((
            "status_or_reserved_raw".to_owned(),
            scalar(update.status_raw()),
        ));
    } else {
        fields.extend([
            ("time_raw".to_owned(), scalar(snapshot.server_time_raw())),
            (
                "unknown_after_time_raw".to_owned(),
                scalar(snapshot.unknown_after_time_raw()),
            ),
        ]);
    }
    fields.extend([
        ("total_hand".to_owned(), scalar(quantity(snapshot.total()))),
        (
            "current_hand".to_owned(),
            scalar(quantity(snapshot.current())),
        ),
        (
            "amount".to_owned(),
            scalar(snapshot.amount().value().as_f64()),
        ),
        ("amount_raw".to_owned(), scalar(snapshot.amount_wire_word())),
        (
            "inside_dish".to_owned(),
            scalar(quantity(snapshot.inside())),
        ),
        (
            "outer_disc".to_owned(),
            scalar(quantity(snapshot.outside())),
        ),
        (
            "unknown_after_outer_raw".to_owned(),
            scalar(snapshot.unknown_after_outer_raw()),
        ),
        (
            "open_amount_raw".to_owned(),
            scalar(snapshot.open_amount_raw()),
        ),
        (
            "open_amount_yuan".to_owned(),
            scalar(snapshot.open_amount().value().as_f64()),
        ),
        ("buy_levels".to_owned(), levels(snapshot.bids())),
        ("sell_levels".to_owned(), levels(snapshot.asks())),
        ("tail_raw".to_owned(), "".into()),
    ]);
    if update.is_some() {
        fields.push((
            "record_hex".to_owned(),
            raw.map(hex).unwrap_or_default().into(),
        ));
    }
    Value::Object(fields)
}

pub(super) fn period(value: &str) -> Option<(KlinePeriod, u16, &'static str)> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "5m" | "5min" => (KlinePeriod::Min5, 0, "5m"),
        "15m" | "15min" => (KlinePeriod::Min15, 1, "15m"),
        "30m" | "30min" => (KlinePeriod::Min30, 2, "30m"),
        "60m" | "60min" | "1h" | "hour" => (KlinePeriod::Min60, 3, "60m"),
        "day" | "1d" | "d" | "daily" => (KlinePeriod::Day, 4, "day"),
        "week" | "1w" | "w" => (KlinePeriod::Week, 5, "week"),
        "month" | "1mo" | "mo" => (KlinePeriod::Month, 6, "month"),
        "1m" | "1min" | "minute" => (KlinePeriod::Min1, 7, "1m"),
        "quarter" | "1q" | "q" => (KlinePeriod::Quarter, 10, "quarter"),
        "year" | "1y" | "y" => (KlinePeriod::Year, 11, "year"),
        _ => return None,
    })
}

fn hq_period(value: &str) -> Result<(KlinePeriod, u16, u16, String)> {
    if let Some((period, raw, name)) = period(value) {
        return Ok((period, raw, 1, name.to_owned()));
    }
    let key = value.trim().to_ascii_lowercase();
    if matches!(key.as_str(), "flow" | "fund_flow" | "zjlx") {
        return Err(invalid(format!("period={value:?} is not OHLC data; use client.money_flow.history(code, days=...) instead")));
    }
    let digits = key.bytes().take_while(u8::is_ascii_digit).count();
    let error = || invalid(format!("invalid period: {value:?}"));
    let parameter: u16 = key[..digits].parse().map_err(|_| error())?;
    match &key[digits..] {
        "min" | "m" => {
            let name = format!("{parameter}m");
            if let Some((period, raw, _)) = period(&name) {
                Ok((period, raw, 1, name))
            } else {
                Ok((KlinePeriod::minutes(parameter)?, 8, parameter, name))
            }
        }
        "d" if parameter == 1 => Ok((KlinePeriod::Day, 4, 1, "day".to_owned())),
        "d" => Ok((
            KlinePeriod::days(parameter)?,
            9,
            parameter,
            format!("{parameter}d"),
        )),
        "s" => Ok((
            KlinePeriod::seconds(parameter)?,
            13,
            parameter,
            format!("{parameter}s"),
        )),
        _ => Err(error()),
    }
}

fn milli(price: Price) -> i64 {
    (price.as_f64() * 1_000.0).round() as i64
}

pub(super) fn kline(args: Kline) -> Result<Value> {
    let id: SecurityId = single_code(&codes(&args.code))?.parse()?;
    let (period, period_raw, period_param, period_name) = hq_period(&args.period)?;
    let (adjust, adjust_raw, adjust_name) = match args
        .adjust
        .as_deref()
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
        _ => return Err(invalid(format!("invalid adjust mode: {:?}", args.adjust))),
    };
    let query = KlineQuery::new()
        .with_period(period)?
        .with_adjust(adjust)?
        .with_start(args.start.try_into()?)?
        .with_count(args.count.try_into()?)?;
    with_client(args.common, args.raw, |client, options| {
        let series = client.klines().page(&id, &query, options)?;
        let bars = series.bars().items().iter().enumerate().map(|(index, bar)| {
            let (open, close, high, low) = (milli(bar.open()), milli(bar.close()), milli(bar.high()), milli(bar.low()));
            object! {
                "time" => datetime(bar.time(), true), "open" => bar.open().as_f64(),
                "close" => bar.close().as_f64(), "high" => bar.high().as_f64(), "low" => bar.low().as_f64(),
                "open_price_milli" => open, "close_price_milli" => close,
                "high_price_milli" => high, "low_price_milli" => low,
                "last_close_price_milli" => bar.last_close().map(milli),
                "volume_raw" => bar.volume_word(), "amount_raw" => bar.amount_word(),
                "volume_wire_value" => bar.volume().source_value().as_f64(),
                "volume_lots" => bar.volume().value().as_f64(), "amount" => bar.amount().value().as_f64(),
                "open_delta_raw" => open - bar.last_close().map_or(0, milli),
                "close_delta_raw" => close - open, "high_delta_raw" => high - open, "low_delta_raw" => low - open,
                "up_count" => bar.index_up(), "down_count" => bar.index_down(),
                "record_hex" => record(series.meta(), index).as_deref().map(hex).unwrap_or_default(),
            }
        }).collect();
        let anchor = series.anchor_date();
        Ok(Value::Object(vec![
            ("exchange".to_owned(), id.market().code().into()),
            ("market_id".to_owned(), scalar(market_id(id.market()))),
            ("code".to_owned(), id.number().into()),
            ("period_raw".to_owned(), scalar(period_raw)),
            ("period_param_raw".to_owned(), scalar(period_param)),
            ("period_name".to_owned(), period_name.into()),
            ("start".to_owned(), scalar(args.start)),
            ("request_count".to_owned(), scalar(args.count)),
            ("adjust_mode_raw".to_owned(), scalar(adjust_raw)),
            ("adjust_mode".to_owned(), adjust_name.into()),
            ("anchor_date_raw".to_owned(), scalar(anchor.map_or(0, ymd))),
            ("anchor_date".to_owned(), scalar(date_text(anchor))),
            ("bars".to_owned(), Value::Array(bars)),
            (
                "raw_payload".to_owned(),
                payload(series.meta(), args.raw).into(),
            ),
        ]))
    })
}

fn ymd(date: TradingDate) -> u32 {
    u32::from(date.year()) * 10000 + u32::from(date.month()) * 100 + u32::from(date.day())
}

pub(super) fn minute(args: Minute) -> Result<Value> {
    let ids = unique_ids(&args.code)?;
    let date = trade_date(args.date.as_deref())?;
    with_client(args.common, true, |client, options| {
        let service = client.minutes();
        let batch = match date {
            Some(date) => service.history_many(&ids, date, options)?,
            None => service.today_many(&ids, options)?,
        };
        let mut entries = Vec::new();
        for item in batch.items() {
            let id = item.security();
            let BatchItemState::Present(series) = item.state() else {
                return Err(format!("MissingCodesError: {id}").into());
            };
            let bytes = series.meta().raw().and_then(|raw| raw.payloads().first());
            let reserved = bytes
                .and_then(|raw| raw.get(2..4))
                .and_then(|raw| <[u8; 2]>::try_from(raw).ok())
                .map(u16::from_le_bytes);
            let points = series.points().iter().map(|point| {
                let label = point.time().clock().map(|(hour, minute, _)| format!("{hour:02}:{minute:02}"));
                object! {
                    "index" => point.index(), "time_label" => label,
                    "time" => datetime(point.time(), false), "price" => point.price().as_f64(),
                    "price_milli" => milli(point.price()), "volume" => quantity(point.volume()),
                    "price_field" => Option::<i64>::None, "avg_field" => Option::<i64>::None,
                    "avg_price" => point.average().map(Price::as_f64),
                    "price_raw" => Option::<i64>::None, "avg_raw" => Option::<i64>::None,
                    "price_delta_raw" => Option::<i64>::None, "aux_delta_raw" => Option::<i64>::None,
                    "record_hex" => "",
                }
            }).collect();
            let fields = vec![
                ("exchange".to_owned(), id.market().code().into()),
                ("market_id".to_owned(), scalar(market_id(id.market()))),
                ("code".to_owned(), id.number().into()),
                (
                    "trading_date".to_owned(),
                    scalar(date_text(series.trading_date())),
                ),
                ("points".to_owned(), Value::Array(points)),
                (
                    "reserved_zero".to_owned(),
                    scalar(reserved.filter(|_| date.is_none())),
                ),
                (
                    "prev_close".to_owned(),
                    scalar(series.previous_close().map(Price::as_f64)),
                ),
                (
                    "open_price".to_owned(),
                    scalar(series.open().map(Price::as_f64)),
                ),
                ("date_selector_raw".to_owned(), Value::Null),
                ("raw_payload".to_owned(), "".into()),
            ];
            entries.push((id.to_string(), Value::Object(fields)));
        }
        Ok(Value::Object(entries))
    })
}

pub(super) fn trades(args: Trades) -> Result<Value> {
    let ids = unique_ids(&args.code)?;
    let date = trade_date(args.date.as_deref())?;
    let (start, count) = if args.all {
        (0, 1800)
    } else {
        (args.start, args.count)
    };
    let mut query = TradeQuery::new()
        .with_start(start.try_into()?)?
        .with_count(count.try_into()?)?;
    if args.all {
        query = query.with_pages(PageOptions::new(1800, 100, 180_000)?)?;
    }
    if let Some(date) = date {
        query = query.with_date(MarketDateChoice::Date(date))?;
    }
    with_client(args.common, args.raw, |client, options| {
        let service = client.trades();
        let batch = if args.all {
            service.collect_many(&ids, &query, options)?
        } else {
            service.page_many(&ids, &query, options)?
        };
        let mut rows = Vec::new();
        for item in batch.items() {
            let id = item.security();
            let BatchItemState::Present(page) = item.state() else {
                return Err(format!("MissingCodesError: {id}").into());
            };
            let count = if args.all {
                page.ticks().items().len() as i64
            } else {
                count
            };
            let ticks = page.ticks().items().iter().enumerate().map(|(index, tick)| {
                let clock = tick.time().clock();
                let side = match tick.side() { TradeSide::Buy => "buy", TradeSide::Sell => "sell", TradeSide::Neutral => "neutral", _ => "unknown" };
                let kind = match tick.kind() { TradeEventKind::OpeningMatch => "opening_match", TradeEventKind::AuctionSnapshot => "auction_snapshot", _ => "trade" };
                object! {
                    "index" => tick.index(), "absolute_index" => tick.absolute_index(),
                    "time_minutes" => clock.map(|(hour, minute, _)| u16::from(hour) * 60 + u16::from(minute)),
                    "time_label" => clock.map(|(hour, minute, _)| format!("{hour:02}:{minute:02}")),
                    "trade_datetime" => datetime(tick.time(), false), "price" => tick.price().as_f64(),
                    "price_milli" => milli(tick.price()), "volume" => quantity(tick.volume()),
                    "order_count" => tick.order_count(), "status_raw" => tick.status_raw(), "side" => side,
                    "price_delta_raw" => tick.price_delta_raw(), "price_acc_raw" => tick.price_acc_raw(),
                    "unknown_tail_raw" => tick.unknown_tail(), "reserved_zero" => tick.reserved_zero(),
                    "record_hex" => record(page.meta(), index).as_deref().map(hex).unwrap_or_default(),
                    "event_kind" => kind, "auction_matched_volume" => tick.auction_matched().map(quantity),
                    "auction_unmatched_signed_volume" => tick.auction_unmatched_signed(),
                }
            }).collect();
            rows.push((
                id.to_string(),
                Value::Object(vec![
                    ("exchange".to_owned(), id.market().code().into()),
                    ("market_id".to_owned(), scalar(market_id(id.market()))),
                    ("code".to_owned(), id.number().into()),
                    ("start".to_owned(), scalar(start)),
                    ("request_count".to_owned(), scalar(count)),
                    ("ticks".to_owned(), Value::Array(ticks)),
                    (
                        "trading_date".to_owned(),
                        scalar(date_text(page.trading_date())),
                    ),
                    (
                        "price_base_raw_f32".to_owned(),
                        scalar(page.price_base().map(Price::as_f64)),
                    ),
                    (
                        "raw_payload".to_owned(),
                        payload(page.meta(), args.raw).into(),
                    ),
                ]),
            ));
        }
        Ok(Value::Object(rows))
    })
}

fn security(row: &Security) -> Value {
    let category = match row.category() {
        SecurityCategory::AShare => "a_share",
        SecurityCategory::BShare => "b_share",
        SecurityCategory::Etf => "etf",
        SecurityCategory::Index => "index",
        SecurityCategory::PrivateConvertibleBond => "private_convertible_bond",
        SecurityCategory::Bond => "bond",
        SecurityCategory::Other(name) => name,
        _ => "unknown",
    };
    let board = match row.board() {
        SecurityBoard::SseMain => "sse_main",
        SecurityBoard::SzseMain => "szse_main",
        SecurityBoard::Star => "star",
        SecurityBoard::ChiNext => "chinext",
        SecurityBoard::Beijing => "beijing",
        SecurityBoard::Other(name) => name,
        _ => "",
    };
    let id = row.id();
    object! {
        "exchange" => id.market().code(), "market_id" => market_id(id.market()), "code" => id.number(),
        "name" => row.name(), "multiple" => row.multiple(), "decimal" => row.decimal(),
        "previous_close_price" => row.previous_close().as_f64(), "volume_ratio_base" => row.volume_ratio_base(),
        "unknown0_raw" => "", "previous_close_raw" => row.previous_close().value().wire_f32_bits().map(|bits| hex(&bits.to_le_bytes())).unwrap_or_default(),
        "unknown3_raw" => "", "category" => category, "category_reason" => row.category_reason(),
        "board" => board, "board_reason" => row.board_reason(),
    }
}

pub(super) fn directory(args: Directory) -> Result<Value> {
    with_client(args.common, false, |client, options| {
        let service = client.codes();
        let scan = CodeScanQuery::new();
        if matches!(args.kind, DirectoryKind::Markets) {
            return Ok(Value::Array(
                service
                    .all_markets(&scan, options)?
                    .items()
                    .iter()
                    .map(security)
                    .collect(),
            ));
        }
        let market = match args.market.trim().to_ascii_lowercase().as_str() {
            "sh" => HqMarket::Shanghai,
            "sz" => HqMarket::Shenzhen,
            "bj" => HqMarket::Beijing,
            _ => return Err(invalid(format!("unsupported market: {:?}", args.market))),
        };
        let data = match args.kind {
            DirectoryKind::List => {
                let query = CodeQuery::new()
                    .with_start(args.start.try_into()?)?
                    .with_limit(args.limit.try_into()?)?;
                service.list(market, &query, options)?.into_items()
            }
            DirectoryKind::All => {
                let scan =
                    scan.with_pages(PageOptions::new(args.limit.try_into()?, 200, 160_000)?)?;
                service.all(market, &scan, options)?.into_items()
            }
            DirectoryKind::Stocks => service.stocks(market, &scan, options)?.into_items(),
            DirectoryKind::Etfs => service.etfs(market, &scan, options)?.into_items(),
            DirectoryKind::Indices => service.indices(market, &scan, options)?.into_items(),
            DirectoryKind::Count => return Ok(scalar(service.count(market, options)?)),
            DirectoryKind::Markets => unreachable!("handled before market parsing"),
        };
        Ok(Value::Array(data.iter().map(security).collect()))
    })
}

const FINANCE_KEYS: [&str; 31] = [
    "liu_tong_gu_ben_raw_float",
    "zong_gu_ben_raw_float",
    "guo_jia_gu_raw_float",
    "fa_qi_ren_fa_ren_gu_raw_float",
    "fa_ren_gu_raw_float",
    "b_gu_raw_float",
    "h_gu_raw_float",
    "eps_raw",
    "zong_zi_chan_raw_float",
    "liu_dong_zi_chan_raw_float",
    "gu_ding_zi_chan_raw_float",
    "wu_xing_zi_chan_raw_float",
    "gu_dong_ren_shu_raw_float",
    "liu_dong_fu_zhai_raw_float",
    "chang_qi_fu_zhai_raw_float",
    "zi_ben_gong_ji_jin_raw_float",
    "jing_zi_chan_raw_float",
    "zhu_ying_shou_ru_raw_float",
    "zhu_ying_li_run_raw_float",
    "ying_shou_zhang_kuan_raw_float",
    "ying_ye_li_run_raw_float",
    "tou_zi_shou_yu_raw_float",
    "jing_ying_xian_jin_liu_raw_float",
    "zong_xian_jin_liu_raw_float",
    "cun_huo_raw_float",
    "li_run_zong_he_raw_float",
    "shui_hou_li_run_raw_float",
    "jing_li_run_raw_float",
    "wei_fen_li_run_raw_float",
    "mei_gu_jing_zi_chan_raw_float",
    "bao_liu_2_raw_float",
];

fn finance_value(value: FinanceValue) -> Option<f32> {
    match value {
        FinanceValue::Shares(value) => Some(value.source_value().as_f64() as f32),
        FinanceValue::Money(value) => Some(value.source_value().as_f64() as f32),
        FinanceValue::Unclassified { value, .. } => Some(value),
        _ => None,
    }
}

fn finance_row(row: &FinanceRecord, raw: bool) -> Value {
    let id = row.id();
    let values: Vec<_> = FinanceField::ALL
        .into_iter()
        .map(|field| row.value(field).and_then(finance_value))
        .collect();
    let mut info = Vec::new();
    info.extend(values[0].unwrap_or_default().to_le_bytes());
    info.extend(row.province().to_le_bytes());
    info.extend(row.industry().to_le_bytes());
    info.extend(row.updated_raw().to_le_bytes());
    info.extend(row.ipo_raw().to_le_bytes());
    for value in &values[1..] {
        info.extend(value.unwrap_or_default().to_le_bytes());
    }
    let mut fields = vec![
        ("exchange".to_owned(), id.market().code().into()),
        ("market_id".to_owned(), scalar(market_id(id.market()))),
        ("code".to_owned(), id.number().into()),
        ("finance_info_raw".to_owned(), hex(&info).into()),
        (FINANCE_KEYS[0].to_owned(), scalar(values[0].map(f64::from))),
        ("province_raw".to_owned(), scalar(row.province())),
        ("industry_raw".to_owned(), scalar(row.industry())),
        ("updated_date_raw".to_owned(), scalar(row.updated_raw())),
        ("updated_date".to_owned(), scalar(date_text(row.updated()))),
        ("ipo_date_raw".to_owned(), scalar(row.ipo_raw())),
        ("ipo_date".to_owned(), scalar(date_text(row.ipo()))),
    ];
    fields.extend(
        FINANCE_KEYS[1..]
            .iter()
            .zip(&values[1..])
            .map(|(key, value)| ((*key).to_owned(), scalar(value.map(f64::from)))),
    );
    fields.push((
        "record_hex".to_owned(),
        if raw {
            record(row.meta(), 0)
                .as_deref()
                .map(hex)
                .unwrap_or_default()
        } else {
            String::new()
        }
        .into(),
    ));
    Value::Object(fields)
}

pub(super) fn finance(args: Finance) -> Result<Value> {
    let ids = ids(&args.code)?;
    with_client(args.common, args.raw, |client, options| {
        let result = client
            .corporate()
            .finance(&ids, &FinanceQuery::new(), options)?;
        let records = result
            .items()
            .iter()
            .map(|item| match item.state() {
                BatchItemState::Present(row) => Ok(finance_row(row, args.raw)),
                _ => Err(format!("MissingCodesError: {}", item.security()).into()),
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Object(vec![
            ("records".to_owned(), Value::Array(records)),
            (
                "raw_payload".to_owned(),
                payload(result.meta(), args.raw).into(),
            ),
        ]))
    })
}

pub(super) fn money_flow(args: MoneyFlow) -> Result<Value> {
    let ids = ids(&args.code)?;
    if ids.is_empty() {
        return Err(invalid("codes must not be empty"));
    }
    if let Some(days) = args.days {
        let days = days.parse()?;
        return super::f10::money_flow_history(&ids, days, args.common.timeout, args.raw);
    }
    with_client(args.common, args.raw, |client, options| {
        let result = client.money_flow().daily_many(&ids, options)?;
        let mut blocks = Vec::new();
        for item in result.items() {
            let BatchItemState::Present(block) = item.state() else {
                continue;
            };
            if block.records().is_empty() {
                continue;
            }
            let id = block.id();
            let records = block.records().iter().enumerate().map(|(index, row)| {
                let main = row.main_size_net().map(|value| value.value().as_f64());
                let buy = row.main_buy_size_net().map(|value| value.value().as_f64());
                object! {
                    "date_raw" => row.date_raw(), "date" => date_text(row.date()),
                    "total_amount" => row.total_amount().value().as_f64(), "buckets" => row.buckets(),
                    "main_net" => row.main_net().value().as_f64(), "main_ratio" => row.main_ratio().value().as_f64(),
                    "raw" => row.raw_values(), "record_hex" => record(block.meta(), index).as_deref().map(hex).unwrap_or_default(),
                    "main_buy_net" => row.main_buy_net().value().as_f64(), "main_buy_ratio" => row.main_buy_ratio().value().as_f64(),
                    "main_buy_super_large_net" => buy[0], "main_buy_large_net" => buy[1],
                    "main_buy_medium_net" => buy[2], "main_buy_small_net" => buy[3],
                    "main_super_large_net" => main[0], "main_large_net" => main[1],
                    "main_medium_net" => main[2], "main_small_net" => main[3],
                }
            }).collect();
            blocks.push(Value::Object(vec![
                ("exchange".to_owned(), id.market().code().into()),
                ("market_id".to_owned(), scalar(market_id(id.market()))),
                ("code".to_owned(), id.number().into()),
                ("records".to_owned(), Value::Array(records)),
            ]));
        }
        Ok(Value::Object(vec![(
            "blocks".to_owned(),
            Value::Array(blocks),
        )]))
    })
}

pub(super) fn limits(args: Limits) -> Result<Value> {
    let ids = ids(&args.code)?;
    let target = trade_date(args.date.as_deref())?;
    with_client(args.common, false, |client, options| {
        let target = match target {
            Some(date) => date,
            None => client.calendar().today()?,
        };
        let mut query = DailyLimitsQuery::new(target);
        if !ids.is_empty() {
            query = query.with_selection(SecuritySelection::Explicit(ids.clone()))?;
        }
        let result = client.helpers().daily_price_limits(&query, options)?;
        let rows = result.rows().iter().map(|row| {
            let (rule, ratio) = match row.rule() {
                LimitRule::Approved { id, ratio, .. } => (id.as_str(), ratio.map(|ratio| ratio.value().as_f64())),
                LimitRule::Unknown { reason } => (reason.as_str(), None),
                _ => ("unknown", None),
            };
            let status = match row.status() {
                LimitStatus::Normal => "normal",
                LimitStatus::NoPriceLimit => "no_price_limit",
                LimitStatus::MissingReference => "missing_reference",
                _ => "unknown_rule",
            };
            object! {
                "full_code" => row.id().to_string(), "trade_date" => row.trade_date().to_string(), "name" => row.name(),
                "pre_close_trade_date" => date_text(row.reference_date()), "pre_close" => row.reference().map(Price::as_f64),
                "limit_up_price" => row.upper().map(Price::as_f64), "limit_down_price" => row.lower().map(Price::as_f64),
                "limit_ratio_pct" => ratio, "limit_rule" => rule, "limit_status" => status, "pre_close_source" => row.reference_source(),
            }
        }).collect();
        let codes: Vec<_> = if ids.is_empty() {
            result
                .rows()
                .iter()
                .map(|row| row.id().to_string())
                .collect()
        } else {
            ids.iter().map(ToString::to_string).collect()
        };
        Ok(Value::Object(vec![
            ("codes".to_owned(), scalar(codes)),
            ("rows".to_owned(), Value::Array(rows)),
        ]))
    })
}
