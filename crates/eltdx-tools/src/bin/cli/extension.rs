use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eltdx::ex::hosts::{parse_hosts, KNOWN_EX_HOSTS, MAC_EX_HOSTS};
use eltdx::mac::{self as mac_sdk, ex_query, MacAdjust, MacClient, MacExClient, MacPeriod};
use eltdx::{ex as ex_sdk, EventTime, OperationOptions, SecurityId};

use super::args::{Ex, ExAction, Fund, FundAction, Hq, Mac, MacAction, MacEx, MacExAction};
use super::output::{object, scalar, Value};
use super::{
    codes, finish, hex, invalid, operation, required, single_code, timeout, trade_date, Result,
};

fn endpoints(
    host: Option<&str>,
    port: i64,
    defaults: &[&str],
    extra: &[&str],
    wait: Duration,
) -> Result<Vec<SocketAddr>> {
    let port = u16::try_from(port)?;
    if port == 0 {
        return Err(invalid("port must be nonzero"));
    }
    if let Some(host) = host.map(str::trim).filter(|host| !host.is_empty()) {
        if let Ok(numeric) = parse_hosts([host], port) {
            return Ok(numeric);
        }
        let hostname = host.to_owned();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("eltdx-cli-dns".to_owned())
            .spawn(move || {
                let result = (hostname.as_str(), port)
                    .to_socket_addrs()
                    .map(|addresses| addresses.collect::<Vec<_>>());
                let _ = sender.send(result);
            })?;
        let addresses = receiver.recv_timeout(wait)??;
        if addresses.is_empty() {
            return Err(invalid(format!("no addresses resolved for {host}")));
        }
        return Ok(addresses);
    }
    Ok(parse_hosts(defaults.iter().chain(extra).copied(), port)?)
}

fn extended_endpoints(
    host: Option<&str>,
    port: i64,
    all_hosts: bool,
    wait: Duration,
) -> Result<Vec<SocketAddr>> {
    let extra = if all_hosts { KNOWN_EX_HOSTS } else { &[] };
    endpoints(host, port, MAC_EX_HOSTS, extra, wait)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_hostname_resolves_with_the_requested_port() -> Result<()> {
        let addresses = extended_endpoints(Some("localhost"), 7727, false, Duration::from_secs(2))?;
        assert!(!addresses.is_empty());
        assert!(addresses
            .iter()
            .all(|address| address.ip().is_loopback() && address.port() == 7727));
        Ok(())
    }
}

fn with_ex(
    common: &Hq,
    port: i64,
    all_hosts: bool,
    run: impl FnOnce(&ex_sdk::ExClient, &OperationOptions) -> Result<Value>,
) -> Result<Value> {
    let duration = timeout(common.timeout)?;
    let client = ex_sdk::ExClient::builder()
        .with_endpoints(extended_endpoints(
            common.host.as_deref(),
            port,
            all_hosts,
            duration,
        )?)
        .with_timeout(duration)
        .build()?;
    let result = run(&client, &operation(duration, true));
    finish(result, client.close())
}

fn with_mac_ex(
    common: &Hq,
    port: i64,
    all_hosts: bool,
    run: impl FnOnce(&MacExClient, &OperationOptions) -> Result<Value>,
) -> Result<Value> {
    let duration = timeout(common.timeout)?;
    let builder = MacExClient::builder()
        .with_endpoints(extended_endpoints(
            common.host.as_deref(),
            port,
            all_hosts,
            duration,
        )?)
        .with_timeout(duration);
    let client = eltdx_tools::mac_ex::configure_login(builder)?.build()?;
    let result = run(&client, &operation(duration, false));
    finish(result, client.close())
}

fn ex_market(value: i64) -> Result<ex_sdk::ExMarketId> {
    Ok(ex_sdk::ExMarketId::new(value.try_into()?)?)
}

fn ex_id(market: ex_sdk::ExMarketId, code: &str) -> Result<ex_sdk::ExInstrumentId> {
    Ok(ex_sdk::ExInstrumentId::new(
        market,
        ex_sdk::ExCode::new(code)?,
    ))
}

fn rows<T>(values: &[T], project: impl FnMut(&T) -> Value) -> Value {
    Value::Array(values.iter().map(project).collect())
}

pub(super) fn ex(args: Ex) -> Result<Value> {
    if args.action == ExAction::Markets {
        return Ok(ex_markets(&args));
    }
    if args.action == ExAction::Count && args.market.is_some() {
        let market = args.market.map(ex_market).transpose()?;
        return with_mac_ex(
            &args.common,
            args.port,
            args.all_hosts,
            |client, options| mac_ex_count(client, options, market),
        );
    }

    let codes = codes(&args.code);
    let code = || single_code(&codes);
    let market = || ex_market(required(args.market)?);
    with_ex(
        &args.common,
        args.port,
        args.all_hosts,
        |client, options| {
            Ok(match args.action {
                ExAction::Markets => unreachable!(),
                ExAction::Count => object! { "count" => client.instrument_count(options)? },
                ExAction::Info => {
                    let query = ex_sdk::InstrumentsQuery::new(args.start.try_into()?)
                        .with_count(args.count.try_into()?)?;
                    rows(client.instruments(&query, options)?.rows(), instrument)
                }
                ExAction::Quote => {
                    let query = ex_sdk::QuoteQuery::new(ex_id(market()?, code()?)?);
                    client
                        .quote(&query, options)?
                        .as_ref()
                        .map_or(Value::Null, ex_quote)
                }
                ExAction::QuoteList => {
                    let category =
                        ex_sdk::QuoteCategory::new(required(args.category)?.try_into()?)?;
                    let query = ex_sdk::QuoteListQuery::new(market()?, category)
                        .with_start(args.start.try_into()?)?
                        .with_count(args.count.try_into()?)?;
                    rows(
                        client.quote_list(&query, options)?.rows(),
                        ex_quote_list_row,
                    )
                }
                ExAction::Bars => {
                    let category = required(args.category)?.try_into()?;
                    let period = ex_sdk::ExPeriod::known(category)
                        .unwrap_or(ex_sdk::ExPeriod::Wire(category));
                    let query = ex_sdk::BarsQuery::new(ex_id(market()?, code()?)?, period)
                        .with_start(args.start.try_into()?)
                        .with_count(args.count.try_into()?)?;
                    rows(client.bars(&query, options)?.rows(), ex_bar)
                }
                ExAction::Minute => {
                    let query = ex_sdk::MinutesQuery::new(ex_id(market()?, code()?)?);
                    rows(&client.minutes(&query, options)?, ex_minute)
                }
                ExAction::Txn => {
                    let query = ex_sdk::TransactionsQuery::new(ex_id(market()?, code()?)?)
                        .with_start(args.start.try_into()?)?
                        .with_count(args.count.try_into()?)?;
                    rows(client.transactions(&query, options)?.rows(), ex_transaction)
                }
            })
        },
    )
}

fn ex_markets(args: &Ex) -> Value {
    if args.probe_server {
        let result = with_ex(
            &args.common,
            args.port,
            args.all_hosts,
            |client, options| {
                Ok(rows(&client.markets(options)?, |market| {
                    object! {
                        "market" => market.market().get(), "category" => market.category(),
                        "name" => market.name(), "short_name" => market.short_name(),
                    }
                }))
            },
        );
        match result {
            Ok(Value::Array(markets)) if !markets.is_empty() => return Value::Array(markets),
            Ok(_) => eprintln!("提示: 服务器返回空市场表，已回退到本地 ExMarket 枚举。"),
            Err(error) => {
                eprintln!("提示: 服务器未返回市场表（{error}），已回退到本地 ExMarket 枚举。")
            }
        }
    }
    rows(EX_MARKETS, |&(market, name, alias)| {
        object! {
            "market" => market, "name" => name, "alias" => alias,
        }
    })
}

fn symbol_market(token: &str) -> Option<u16> {
    match token.to_ascii_lowercase().as_str() {
        "sz" | "0" => Some(0),
        "sh" | "1" => Some(1),
        "bj" | "2" => Some(2),
        _ => None,
    }
}

fn infer_mac_market(code: &str) -> Result<u16> {
    if code.len() == 6
        && code.bytes().all(|byte| byte.is_ascii_digit())
        && ["010", "019", "120", "122", "124", "126", "130"]
            .iter()
            .any(|prefix| code.starts_with(prefix))
    {
        return Ok(1);
    }
    let id: SecurityId = code.parse()?;
    required(symbol_market(id.market().code()))
}

fn mac_ids(values: &[String], market: Option<u16>) -> Result<Vec<mac_sdk::MacInstrumentId>> {
    let tokens = codes(values);
    let mut tokens = tokens.into_iter().peekable();
    let mut ids = Vec::new();
    while let Some(token) = tokens.next() {
        let separate = symbol_market(token).zip(tokens.peek().copied());
        let prefixed = token.split_at_checked(2).and_then(|(prefix, code)| {
            if code.is_empty() {
                return None;
            }
            symbol_market(prefix).map(|market| (market, code))
        });
        let (hint, code) = if let Some((hint, code)) = separate {
            tokens.next();
            (Some(hint), code)
        } else if let Some((hint, code)) = prefixed {
            (Some(hint), code)
        } else {
            (None, token)
        };
        let market = match market.or(hint) {
            Some(market) => market,
            None => infer_mac_market(code)?,
        };
        ids.push(mac_sdk::MacInstrumentId::new(
            mac_sdk::MacMarketId::new(market),
            mac_sdk::MacCode::new(code)?,
        ));
    }
    Ok(ids)
}

fn mac_id(values: &[String], market: Option<u16>) -> Result<mac_sdk::MacInstrumentId> {
    let mut ids = mac_ids(values, market)?;
    if ids.len() != 1 {
        return Err(invalid(format!(
            "该命令只接受一个代码，收到 {} 个",
            ids.len()
        )));
    }
    required(ids.pop())
}

fn mac_period(value: &str) -> Result<MacPeriod> {
    let (_, raw, _) =
        super::hq::period(value).ok_or_else(|| invalid(format!("不支持的周期 {value:?}")))?;
    Ok(MacPeriod::known(raw)?)
}

fn mac_adjust(value: &str) -> Result<MacAdjust> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "none" | "bfq" => Ok(MacAdjust::None),
        "qfq" => Ok(MacAdjust::Qfq),
        "hfq" => Ok(MacAdjust::Hfq),
        _ => Err(invalid(format!(
            "不支持的复权方式 {value:?}；可用: none, qfq, hfq"
        ))),
    }
}

pub(super) fn mac(args: Mac) -> Result<Value> {
    let duration = timeout(args.common.timeout)?;
    let client = MacClient::builder()
        .with_endpoints(endpoints(
            args.common.host.as_deref(),
            args.port,
            mac_sdk::config::MAC_HOSTS,
            &[],
            duration,
        )?)
        .with_timeout(duration)
        .build()?;
    let result = run_mac(&client, &operation(duration, false), &args);
    finish(result, client.close())
}

fn run_mac(client: &MacClient, options: &OperationOptions, args: &Mac) -> Result<Value> {
    let market = || args.market.map(u16::try_from).transpose();
    let id = || mac_id(&args.code, market()?);
    let start = || u32::try_from(args.bars.start);
    let count = || u32::try_from(args.count);
    let date = || trade_date(args.bars.date.as_deref());
    Ok(match args.action {
        MacAction::Quote => {
            let query = mac_sdk::QuotesQuery::new(mac_ids(&args.code, market()?)?)?;
            rows(&client.quotes(&query, options)?, mac_quote)
        }
        MacAction::Kline => {
            let query = mac_sdk::BarsQuery::new(id()?)
                .with_period(mac_period(&args.bars.period)?)?
                .with_start(start()?)?
                .with_count(count()?)?
                .with_adjust(mac_adjust(&args.bars.adjust)?);
            rows(&client.bars(&query, options)?, mac_bar)
        }
        MacAction::Ranking => {
            let category = mac_sdk::Category::from_wire(required(args.category)?.try_into()?)?;
            let query = mac_sdk::QuoteListQuery::new(category)
                .with_start(start()?)?
                .with_count(count()?)?;
            rows(client.quote_list(&query, options)?.rows(), mac_quote)
        }
        MacAction::BoardList => {
            let query = mac_sdk::BoardsQuery::new().with_count(count()?)?;
            rows(&client.boards(&query, options)?, board)
        }
        MacAction::ServerInfo => server_info(&client.server_info(options)?),
        MacAction::SymbolInfo => {
            let query = mac_sdk::SymbolInfoQuery::new(id()?);
            symbol_info(&client.symbol_info(&query, options)?)
        }
        MacAction::Minute => {
            let query = mac_sdk::TickChartQuery::new(id()?).with_date(date()?)?;
            tick_chart(&client.tick_chart(&query, options)?)
        }
        MacAction::Txn => {
            let query = mac_sdk::TransactionsQuery::new(id()?)
                .with_date(date()?)?
                .with_start(start()?)?
                .with_count(count()?)?;
            rows(
                client.transactions(&query, options)?.rows(),
                mac_transaction,
            )
        }
        MacAction::Auction => {
            let query = mac_sdk::AuctionQuery::new(id()?);
            rows(&client.auction(&query, options)?, auction)
        }
        MacAction::Unusual => {
            let count = if args.count == 0 { 600 } else { count()? };
            let query =
                mac_sdk::UnusualQuery::new(mac_sdk::MacMarketId::new(market()?.unwrap_or(0)))
                    .with_start(start()?)?
                    .with_count(count)?;
            rows(client.unusual(&query, options)?.rows(), unusual)
        }
        MacAction::GoodsList => {
            let query = mac_sdk::GoodsQuery::new(mac_sdk::MacMarketId::new(required(market()?)?))
                .with_start(start()?)?
                .with_count(count()?)?;
            goods_rows(client.goods(&query, options)?.rows())
        }
    })
}

pub(super) fn macex(args: MacEx) -> Result<Value> {
    let market = args.market.map(ex_market).transpose()?;
    if args.action == MacExAction::Count {
        return with_mac_ex(
            &args.common,
            args.port,
            args.all_hosts,
            |client, options| mac_ex_count(client, options, market),
        );
    }
    let market = required(market)?;
    with_mac_ex(
        &args.common,
        args.port,
        args.all_hosts,
        |client, options| {
            let id = || ex_id(market, single_code(&codes(&args.code))?);
            let start = || u32::try_from(args.bars.start);
            let count = || u32::try_from(args.count);
            let date = || trade_date(args.bars.date.as_deref());
            Ok(match args.action {
                MacExAction::Count => unreachable!(),
                MacExAction::Instruments => {
                    mac_ex_instruments(client, options, market, start()?, count()?)?
                }
                MacExAction::List => {
                    let query = ex_query::GoodsQuery::new(market)
                        .with_start(start()?)?
                        .with_count(count()?)?;
                    goods_rows(client.goods(&query, options)?.rows())
                }
                MacExAction::Quote => mac_ex_quotes(client, options, market, &codes(&args.code))?,
                MacExAction::QuoteList => {
                    let query = ex_query::QuoteListQuery::new(market)
                        .with_start(start()?)?
                        .with_count(count()?)?;
                    rows(client.quote_list(&query, options)?.rows(), mac_quote)
                }
                MacExAction::Kline => {
                    let query = ex_query::BarsQuery::new(id()?)?
                        .with_period(mac_period(&args.bars.period)?)?
                        .with_start(start()?)?
                        .with_count(count()?)?
                        .with_adjust(mac_adjust(&args.bars.adjust)?);
                    rows(&client.bars(&query, options)?, mac_bar)
                }
                MacExAction::Minute => {
                    let query = ex_query::TickChartQuery::new(id()?)?.with_date(date()?)?;
                    tick_chart(&client.tick_chart(&query, options)?)
                }
                MacExAction::Txn => {
                    let query = ex_query::TransactionsQuery::new(id()?)?
                        .with_date(date()?)?
                        .with_start(start()?)?
                        .with_count(count()?)?;
                    rows(
                        client.transactions(&query, options)?.rows(),
                        mac_transaction,
                    )
                }
            })
        },
    )
}

pub(super) fn fund(args: Fund) -> Result<Value> {
    if args.action == FundAction::Markets {
        return Ok(rows(FUND_MARKETS, |&(market, name)| {
            object! {
                "market" => market, "name" => name,
            }
        }));
    }
    let market = ex_market(args.market)?;
    let codes = codes(&args.code);
    if codes.is_empty() && args.action == FundAction::Nav {
        return Err(invalid(
            "请提供至少一个基金代码，例如: eltdx fund nav 016947",
        ));
    }
    with_mac_ex(&args.common, args.port, false, |client, options| {
        Ok(match args.action {
            FundAction::Markets => unreachable!(),
            FundAction::List => mac_ex_instruments(
                client,
                options,
                market,
                args.start.try_into()?,
                args.count.try_into()?,
            )?,
            FundAction::Nav | FundAction::Info => mac_ex_quotes(client, options, market, &codes)?,
            FundAction::Kline => {
                let id = ex_id(market, single_code(&codes)?)?;
                let query = ex_query::BarsQuery::new(id)?
                    .with_period(mac_period(&args.period)?)?
                    .with_start(args.start.try_into()?)?
                    .with_count(args.count.try_into()?)?;
                rows(&client.bars(&query, options)?, mac_bar)
            }
        })
    })
}

fn mac_ex_count(
    client: &MacExClient,
    options: &OperationOptions,
    market: Option<ex_sdk::ExMarketId>,
) -> Result<Value> {
    let count = client.goods_count(
        &ex_query::GoodsCountQuery::new().with_market(market),
        options,
    )?;
    Ok(match market {
        Some(market) => object! { "market" => market.get(), "count" => count },
        None => object! { "count" => count },
    })
}

fn mac_ex_instruments(
    client: &MacExClient,
    options: &OperationOptions,
    market: ex_sdk::ExMarketId,
    start: u32,
    count: u32,
) -> Result<Value> {
    let query = ex_query::InstrumentsQuery::new(market)
        .with_start(start)?
        .with_count(count)?;
    Ok(rows(
        client.instruments(&query, options)?.rows(),
        instrument,
    ))
}

fn mac_ex_quotes(
    client: &MacExClient,
    options: &OperationOptions,
    market: ex_sdk::ExMarketId,
    codes: &[&str],
) -> Result<Value> {
    if codes.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    let ids = codes
        .iter()
        .map(|code| ex_id(market, code))
        .collect::<Result<Vec<_>>>()?;
    let query = ex_query::QuotesQuery::new(ids)?;
    Ok(rows(&client.quotes(&query, options)?, mac_quote))
}

fn instrument(item: &ex_sdk::InstrumentInfo) -> Value {
    object! {
        "category" => item.category(), "market" => item.market().get(),
        "code" => item.code(), "name" => item.name(), "desc" => item.desc(),
        "_raw" => hex(item.record_raw()),
    }
}

fn ex_float(value: &ex_sdk::NativeNumber<ex_sdk::WireFloat>) -> f64 {
    value.value().as_f64()
}

fn ex_quote(quote: &ex_sdk::Quote) -> Value {
    let mut fields = vec![
        ("market".to_owned(), scalar(quote.id().market().get())),
        ("code".to_owned(), scalar(quote.id().code())),
        ("pre_close".to_owned(), scalar(ex_float(quote.pre_close()))),
        ("open".to_owned(), scalar(ex_float(quote.open()))),
        ("high".to_owned(), scalar(ex_float(quote.high()))),
        ("low".to_owned(), scalar(ex_float(quote.low()))),
        ("price".to_owned(), scalar(ex_float(quote.price()))),
        ("kaicang".to_owned(), scalar(quote.kaicang().value())),
        ("zongliang".to_owned(), scalar(quote.zongliang().value())),
        ("xianliang".to_owned(), scalar(quote.xianliang().value())),
        ("neipan".to_owned(), scalar(quote.neipan().value())),
        ("waipan".to_owned(), scalar(quote.waipan().value())),
        ("chicang".to_owned(), scalar(quote.chicang().value())),
    ];
    for (side, levels) in [("bid", quote.bid()), ("ask", quote.ask())] {
        for (index, level) in levels.iter().enumerate() {
            fields.push((
                format!("{side}{}", index + 1),
                scalar(ex_float(level.price())),
            ));
        }
        for (index, level) in levels.iter().enumerate() {
            fields.push((
                format!("{side}_vol{}", index + 1),
                scalar(level.volume().value()),
            ));
        }
    }
    fields.push((
        "_raw".to_owned(),
        scalar(
            quote
                .record_raw()
                .map_or_else(String::new, |bytes| hex(bytes)),
        ),
    ));
    Value::Object(fields)
}

fn ex_quote_list_row(quote: &ex_sdk::QuoteListRow) -> Value {
    let mut fields = vec![
        ("market".to_owned(), scalar(quote.id().market().get())),
        ("code".to_owned(), scalar(quote.id().code())),
    ];
    fields.extend(quote.fields().iter().map(|field| {
        let value = match field.value() {
            ex_sdk::NativeValue::Unsigned(value) => scalar(value),
            ex_sdk::NativeValue::Float(value) => scalar(value.as_f64()),
        };
        (field.name().to_owned(), value)
    }));
    Value::Object(fields)
}

fn ex_bar(bar: &ex_sdk::Bar) -> Value {
    let date = bar.time().date();
    let clock = bar.time().clock();
    object! {
        "open" => ex_float(bar.open()), "high" => ex_float(bar.high()),
        "low" => ex_float(bar.low()), "close" => ex_float(bar.close()),
        "position" => bar.position_bits(), "trade" => bar.trade().value(),
        "amount" => bar.amount_view().as_ref().map(ex_float),
        "year" => date.map(|date| date.year()), "month" => date.map(|date| date.month()),
        "day" => date.map(|date| date.day()), "hour" => clock.map(|clock| clock.0),
        "minute" => clock.map(|clock| clock.1),
        "_raw" => bar.record_raw().map_or_else(String::new, |bytes| hex(bytes)),
    }
}

fn ex_minute(minute: &ex_sdk::Minute) -> Value {
    let clock = minute.time().clock();
    object! {
        "hour" => clock.map(|clock| clock.0), "minute" => clock.map(|clock| clock.1),
        "price" => ex_float(minute.price()), "avg_price" => ex_float(minute.avg_price()),
        "volume" => minute.volume().value(), "open_interest" => minute.open_interest().value(),
        "_raw" => minute.record_raw().map_or_else(String::new, |bytes| hex(bytes)),
    }
}

fn ex_transaction(transaction: &ex_sdk::Transaction) -> Value {
    let clock = transaction.time().clock();
    object! {
        "hour" => clock.map(|clock| clock.0), "minute" => clock.map(|clock| clock.1),
        "second" => clock.map(|clock| clock.2), "price" => transaction.price_raw().value(),
        "volume" => transaction.volume().value(), "zengcang" => transaction.zengcang().value(),
        "nature" => transaction.nature(),
        "_raw" => transaction.record_raw().map_or_else(String::new, |bytes| hex(bytes)),
    }
}

fn mac_float(value: &mac_sdk::NativeNumber<mac_sdk::WireFloat>) -> f64 {
    value.value().as_f64()
}

fn mac_quote(quote: &mac_sdk::Quote) -> Value {
    use mac_sdk::models::SlotValue;

    let fields = quote
        .fields()
        .iter()
        .filter_map(|field| {
            let key = field.name?;
            let value = if let Some(text) = &field.derived_text {
                scalar(text)
            } else {
                match field.value {
                    SlotValue::F32(value) => scalar(value.as_f64()),
                    SlotValue::U32(0) if field.bit.get() == 74 => scalar(""),
                    SlotValue::U32(value) => scalar(value),
                    SlotValue::I32(value) => scalar(value),
                    SlotValue::Unknown(_) => return None,
                }
            };
            Some((key.to_owned(), value))
        })
        .collect();
    Value::Object(vec![
        ("market".to_owned(), scalar(quote.id().market().get())),
        ("code".to_owned(), scalar(quote.id().code().as_str())),
        ("name".to_owned(), scalar(quote.name())),
        ("fields".to_owned(), Value::Object(fields)),
    ])
}

fn clock_text(time: EventTime) -> Option<String> {
    let (hour, minute, second) = time.clock()?;
    Some(format!("{hour:02}:{minute:02}:{second:02}"))
}

fn datetime_text(time: EventTime) -> Option<String> {
    Some(format!("{}T{}", time.date()?, clock_text(time)?))
}

fn mac_bar(bar: &mac_sdk::Bar) -> Value {
    object! {
        "datetime" => datetime_text(bar.datetime()),
        "open" => bar.open().value().as_f64(), "high" => bar.high().value().as_f64(),
        "low" => bar.low().value().as_f64(), "close" => bar.close().value().as_f64(),
        "vol" => mac_float(bar.volume()), "amount" => mac_float(bar.amount()),
        "float_shares" => bar.float_shares().map_or(0.0, mac_float),
    }
}

fn mac_transaction(transaction: &mac_sdk::Transaction) -> Value {
    object! {
        "time" => clock_text(transaction.time()), "price" => transaction.price().as_f64(),
        "vol" => transaction.volume().value(), "trade_count" => transaction.trade_count(),
        "bs_flag" => transaction.nature_raw(),
    }
}

fn symbol_info(info: &mac_sdk::SymbolInfo) -> Value {
    object! {
        "market" => info.id().market().get(), "code" => info.id().code().as_str(),
        "name" => info.name().as_str(), "time" => info.time().and_then(datetime_text),
        "activity" => info.activity(), "pre_close" => mac_float(info.pre_close()),
        "open" => mac_float(info.open()), "high" => mac_float(info.high()),
        "low" => mac_float(info.low()), "close" => mac_float(info.close()),
        "momentum" => mac_float(info.momentum()), "vol" => info.vol().value(),
        "amount" => mac_float(info.amount()), "inside_volume" => info.inside_volume().value(),
        "outside_volume" => info.outside_volume().value(), "turnover" => mac_float(info.turnover()),
        "avg" => mac_float(info.avg()),
    }
}

fn tick_chart(chart: &mac_sdk::TickChart) -> Value {
    let snapshot = chart.snapshot();
    let ticks = rows(chart.ticks(), |tick| {
        object! {
            "time" => clock_text(tick.time()), "price" => mac_float(tick.price()),
            "avg" => mac_float(tick.avg()), "vol" => tick.vol().value(),
            "momentum" => tick.momentum().map_or(0.0, mac_float),
        }
    });
    let Value::Object(mut fields) = (object! {
        "market" => chart.id().market().get(), "code" => chart.id().code().as_str(),
        "name" => snapshot.name().as_str(), "pre_close" => mac_float(snapshot.pre_close()),
        "open" => mac_float(snapshot.open()), "high" => mac_float(snapshot.high()),
        "low" => mac_float(snapshot.low()), "close" => mac_float(snapshot.close()),
        "vol" => snapshot.vol().value(), "amount" => mac_float(snapshot.amount()),
        "turnover" => mac_float(snapshot.turnover()), "avg" => mac_float(snapshot.avg()),
    }) else {
        unreachable!()
    };
    fields.push(("charts".to_owned(), ticks));
    Value::Object(fields)
}

fn board(info: &mac_sdk::BoardInfo) -> Value {
    let board = info.board();
    let leader = info.leader();
    object! {
        "market" => board.id().market().get(), "code" => board.id().code().as_str(),
        "name" => board.name().as_str(), "price" => mac_float(board.price()),
        "rise_speed" => mac_float(board.rise_speed()), "pre_close" => mac_float(board.pre_close()),
        "symbol_market" => leader.id().market().get(), "symbol_code" => leader.id().code().as_str(),
        "symbol_name" => leader.name().as_str(), "symbol_price" => mac_float(leader.price()),
        "symbol_rise_speed" => mac_float(leader.rise_speed()), "symbol_pre_close" => mac_float(leader.pre_close()),
    }
}

fn auction(auction: &mac_sdk::Auction) -> Value {
    object! {
        "time" => clock_text(auction.time()), "price" => mac_float(auction.price()),
        "matched" => auction.matched().value(), "unmatched" => auction.unmatched().value(),
    }
}

fn unusual(event: &mac_sdk::Unusual) -> Value {
    object! {
        "index" => event.index(), "market" => event.id().market().get(),
        "code" => event.id().code().as_str(), "name" => event.name(), "time" => clock_text(event.time()),
        "desc" => event.description(), "value" => event.value(), "unusual_type" => event.unusual_type(),
    }
}

fn server_info(session: &mac_sdk::ServerSession) -> Value {
    let windows = |windows: &[mac_sdk::SessionWindow]| {
        windows
            .iter()
            .map(|window| {
                let time = |minutes: u16| format!("{}:{:02}", minutes / 60, minutes % 60);
                [
                    ("open", time(window.open_minutes())),
                    ("close", time(window.close_minutes())),
                ]
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>()
            })
            .collect::<Vec<_>>()
    };
    object! {
        "today" => session.today().source_text(), "last_trading_day" => session.last_trading_day().source_text(),
        "sessions_1" => windows(session.sessions_1()), "sessions_2" => windows(session.sessions_2()),
        "market_param_1" => session.market_param_1().unwrap_or_default(),
        "market_param_2" => session.market_param_2().unwrap_or_default(),
    }
}

fn goods_rows(items: &[mac_sdk::GoodsItem]) -> Value {
    Value::Array(
        items
            .iter()
            .enumerate()
            .map(|(index, item)| goods(item, index))
            .collect(),
    )
}

fn goods(item: &mac_sdk::GoodsItem, page_index: usize) -> Value {
    if let Some(instrument) = item.source_instrument() {
        let code = instrument.code();
        let name = if code.is_empty() {
            instrument.name()
        } else {
            code
        };
        return object! {
            "name" => name, "category" => item.category(), "u" => item.u(),
            "index" => page_index, "switch" => item.switch(),
            "code" => code.parse::<f64>().into_iter().collect::<Vec<_>>(),
            "c1" => item.c1(), "c2" => item.c2(),
        };
    }
    object! {
        "name" => item.name().as_str(), "category" => item.category(), "u" => item.u(),
        "index" => item.index(), "switch" => item.switch(),
        "code" => item.code().map(mac_sdk::WireFloat::as_f64), "c1" => item.c1(), "c2" => item.c2(),
    }
}

const FUND_MARKETS: &[(u8, &str)] = &[
    (33, "开放式基金"),
    (34, "货币基金"),
    (49, "香港基金"),
    (56, "阳光私募基金"),
    (57, "券商集合理财"),
    (58, "券商货币理财"),
    (91, "货币市场"),
    (93, "基金估值"),
];

const EX_MARKETS: &[(u8, &str, &str)] = &[
    (1, "TEMP_STOCK", "上海"),
    (4, "ZZ_FUTURES_OPTION", ""),
    (5, "DL_FUTURES_OPTION", ""),
    (6, "SH_FUTURES_OPTION", ""),
    (7, "CFFEX_OPTION", ""),
    (8, "SH_STOCK_OPTION", ""),
    (9, "SZ_STOCK_OPTION", ""),
    (10, "BASIC_FX", ""),
    (11, "CROSS_FX", ""),
    (12, "INTL_INDEX", ""),
    (16, "COMEX_FUTURES", ""),
    (17, "NYMEX_FUTURES", ""),
    (18, "CBOT_FUTURES", ""),
    (23, "HK_FINANCIAL_FUTURES", ""),
    (24, "HK_FINANCIAL_OPTIONS", ""),
    (25, "HK_STOCK_FUTURES", ""),
    (26, "HK_STOCK_OPTIONS", ""),
    (27, "HK_INDEX", ""),
    (28, "ZZ_FUTURES", "郑州商品"),
    (29, "DL_FUTURES", "大连商品"),
    (30, "SH_FUTURES", "上海期货"),
    (31, "HK_MAIN_BOARD", "香港主板"),
    (33, "OPEN_END_FUND", ""),
    (34, "MONETARY_FUND", ""),
    (38, "MACRO_INDICATOR", ""),
    (42, "FUTURES_INDEX", ""),
    (43, "B_TO_H", ""),
    (44, "NEEQ", ""),
    (46, "SH_GOLD", ""),
    (47, "CFFEX_FUTURES", "中金所"),
    (48, "HK_GEM", "香港创业板"),
    (49, "HK_FUND", "香港基金"),
    (54, "TREASURY_VALUATION", ""),
    (56, "SUNSHINE_PRIVATE_FUND", ""),
    (57, "BROKER_COLLECTIVE_FINANCE", ""),
    (58, "BROKER_MONETARY_FINANCE", ""),
    (60, "MAIN_FUTURES_CONTRACT", ""),
    (62, "CSI_INDEX", ""),
    (65, "GZ_ARBITRAGE_FUTURES", ""),
    (66, "GZ_FUTURES", ""),
    (67, "GZ_OPTIONS", ""),
    (68, "RISK_CONTROL_INDEX", ""),
    (69, "HUAZHENG_INDEX", ""),
    (70, "EXTENDED_SECTOR_INDEX", ""),
    (71, "HK_STOCK_GGT", "沪港通"),
    (73, "GE_STOCK", ""),
    (74, "US_STOCK", "外盘"),
    (78, "SG_STOCK", ""),
    (91, "MONEY_MARKET", ""),
    (93, "FUND_VALUATION", ""),
    (98, "HK_DARK_POOL", ""),
    (100, "CODE_MIRROR", ""),
    (102, "SZSE_INDEX", ""),
];
