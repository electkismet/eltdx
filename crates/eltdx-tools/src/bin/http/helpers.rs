use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use eltdx::f10::{
    AshareCode, BoardMembersQuery, CodeUse, IndexMembersQuery, IndexTrackingProductsQuery,
    JsonValue, SectionCode,
};
use eltdx::types::{BatchItemState, TradeSide};
use eltdx::{
    Amount, AuctionDataQuery, BatchItem, BatchOutcome, BoardCategory, BoardId, BoardMember,
    BoardQuery, BoardQuote, CategoryQuote, DailyLimitsQuery, DailySharesQuery, FinanceRecord,
    HelperService, LadderQuery, LimitRule, LimitStatus, Price, ProfileQuery, Quantity, QuoteSort,
    RankQuery, Ratio, Security, SecurityId, SecuritySelection, ShortlineIndicator, ShortlineQuery,
    StatsAlignment, StatsQuery, StockTopicsQuery, TdxClient, ThemeStrengthQuery, TopicStocksQuery,
};
use serde_json::{json, Value};

use super::hq::{self, date_choice, date_value, with_payload};
use super::quote::{self, identity, market_id, snapshot};
use super::{date, f10, finance, market, securities, security, Arguments, Result, RpcError};

pub(super) const METHODS: &[&str] = &[
    "helpers.clear_cache",
    "helpers.index_members",
    "helpers.index_etfs",
    "helpers.ipo_calendar",
    "helpers.board_members",
    "helpers.board_quotes",
    "helpers.board_member_quotes",
    "helpers.full_quotes",
    "helpers.latest_stock_list",
    "helpers.latest_st",
    "helpers.latest_suspended",
    "helpers.buy_sell_strength",
    "helpers.volume_comparison",
    "helpers.realtime_rank",
    "helpers.stock_realtime_rank",
    "helpers.daily_share_capital",
    "helpers.daily_shares",
    "helpers.daily_price_limits",
    "helpers.stock_daily_price_limits",
    "helpers.limit_ladder",
    "helpers.stock_limit_ladder",
    "helpers.theme_strength_rank",
    "helpers.stock_theme_strength_rank",
    "helpers.shortline_indicators",
    "helpers.stock_profile_table",
    "helpers.stock_topics",
    "helpers.topic_stocks",
    "helpers.auction_data",
    "helpers.market_stat",
    "helpers.today_fund_flow",
];

type CollectionKey = (String, String);

#[derive(Clone, Default)]
pub struct CollectionCache(Arc<Mutex<VecDeque<(CollectionKey, Value)>>>);

impl CollectionCache {
    pub(super) fn clear(&self) -> Result<()> {
        self.entries()?.clear();
        Ok(())
    }

    fn entries(&self) -> Result<std::sync::MutexGuard<'_, VecDeque<(CollectionKey, Value)>>> {
        self.0
            .lock()
            .map_err(|_| RpcError::internal("F10 helper cache unavailable"))
    }

    fn get_or_load(
        &self,
        name: &str,
        code: &str,
        refresh: bool,
        load: impl FnOnce() -> Result<Value>,
    ) -> Result<Value> {
        const MAX_ENTRIES: usize = 32;
        let key = (name.to_owned(), code.trim().to_ascii_lowercase());
        if !refresh {
            let mut entries = self.entries()?;
            if let Some(index) = entries.iter().position(|(cached, _)| cached == &key) {
                if let Some(entry) = entries.remove(index) {
                    let value = entry.1.clone();
                    entries.push_back(entry);
                    return Ok(value);
                }
            }
        }
        let value = load()?;
        let mut entries = self.entries()?;
        entries.retain(|(cached, _)| cached != &key);
        if entries.len() >= MAX_ENTRIES {
            entries.pop_front();
        }
        entries.push_back((key, value.clone()));
        Ok(value)
    }
}

pub(super) fn call(
    client: &TdxClient,
    collections: &CollectionCache,
    method: &str,
    params: Value,
) -> Result<Value> {
    let name = method
        .strip_prefix("helpers.")
        .ok_or_else(|| RpcError::unknown(method))?;
    let signature: &[&str] = match name {
        "clear_cache" | "market_stat" => &[],
        "index_members" | "index_etfs" | "board_members" => &["code", ";", "refresh"],
        "ipo_calendar" => &[";", "refresh"],
        "board_quotes" => &[";", "category", "refresh"],
        "board_member_quotes" => &["board_code", ";", "refresh"],
        "full_quotes" => &["codes"],
        "latest_stock_list" | "latest_st" => {
            return hq::call(client, &format!("codes.{name}"), params)
        }
        "latest_suspended" => &["market"],
        "buy_sell_strength" | "volume_comparison" => {
            return hq::call(client, &format!("minutes.{name}"), params);
        }
        "realtime_rank" | "stock_realtime_rank" => {
            &[";", "category", "sort_by", "count", "ascending"]
        }
        "daily_share_capital" | "daily_shares" | "shortline_indicators" => {
            &["codes", ";", "stats_path", "refresh_stats"]
        }
        "daily_price_limits" | "stock_daily_price_limits" => &["codes", ";", "trade_date"],
        "limit_ladder" | "stock_limit_ladder" => &["codes", ";", "include_touched", "count"],
        "theme_strength_rank" | "stock_theme_strength_rank" => &["codes", ";", "count"],
        "stock_profile_table" => &["codes", ";", "include_security", "include_finance"],
        "stock_topics" | "today_fund_flow" => &["code"],
        "topic_stocks" => &[
            "seed_code",
            ";",
            "topic_id",
            "topic_name",
            "sort_by",
            "section",
        ],
        "auction_data" => &[
            "code",
            "date",
            ";",
            "include_series",
            "include_snapshot",
            "include_quote",
            "pre_close_price",
        ],
        _ => return Err(RpcError::unknown(method)),
    };
    let args = Arguments::bind(params, signature)?;
    let sdk = client.helpers();
    let options = with_payload();
    match name {
        "clear_cache" => {
            sdk.clear_cache();
            collections.clear()?;
            Ok(Value::Null)
        }
        "index_members" | "index_etfs" | "board_members" | "ipo_calendar" => {
            let code = if name == "ipo_calendar" {
                String::new()
            } else {
                args.get::<String>("code")?
            };
            collections.get_or_load(name, &code, args.default("refresh", false)?, || {
                f10_collection(&sdk, name, &code)
            })
        }
        "board_quotes" | "board_member_quotes" => boards(&sdk, name, &args),
        "full_quotes" => {
            let ids = securities(args.required("codes")?)?;
            let batch = sdk.full_quotes(&ids, &options)?;
            Ok(json!(batch
                .items()
                .iter()
                .filter_map(|item| match item.state() {
                    BatchItemState::Present(row) => Some(snapshot(row.snapshot())),
                    _ => None,
                })
                .collect::<Vec<_>>()))
        }
        "latest_suspended" => {
            let market = args
                .value("market")
                .filter(|value| !value.is_null())
                .map(market)
                .transpose()?;
            let table = sdk.latest_suspended(market, &options)?;
            Ok(json!(table
                .items()
                .iter()
                .map(security_row)
                .collect::<Vec<_>>()))
        }
        "realtime_rank" | "stock_realtime_rank" => realtime_rank(&sdk, &args),
        "daily_share_capital" | "daily_shares" => daily_shares(&sdk, &args),
        "daily_price_limits" | "stock_daily_price_limits" => daily_limits(&sdk, &args),
        "shortline_indicators" => shortline(&sdk, &args),
        "limit_ladder" | "stock_limit_ladder" => {
            let query = LadderQuery::new()
                .with_selection(selection(&args)?)?
                .with_include_touched(args.default("include_touched", false)?)?
                .with_count(args.optional("count")?)?;
            let table = sdk.limit_ladder(&query, &options)?;
            let rows = table.rows();
            Ok(
                json!({"trade_date": date_value(rows.first().map(ShortlineIndicator::target)
                .or_else(|| table.meta().source().trading_date())),
                "rows": rows.iter().map(shortline_row).collect::<Vec<_>>()}),
            )
        }
        "theme_strength_rank" | "stock_theme_strength_rank" => {
            let ladder = LadderQuery::new().with_selection(selection(&args)?)?;
            let query = ThemeStrengthQuery::new()
                .with_ladder(ladder)?
                .with_count(args.optional("count")?)?;
            let table = sdk.theme_strength_rank(&query, &options)?;
            let rows = table.rows().iter().map(|row| json!({
                "rank": row.rank(), "topic_id": row.topic_id(), "topic_name": row.topic_name(),
                "limit_up_count": row.limit_up_count(), "highest_ladder_level": row.highest_ladder_level(),
                "lianban_count": row.lianban_count(), "total_seal_amount": amount(row.total_seal_amount()),
                "leader_code": row.leader().to_string(), "leader_ladder_level": row.leader_ladder_level(),
            })).collect::<Vec<_>>();
            Ok(json!({"trade_date": date_value(table.trade_date()), "rows": rows}))
        }
        "stock_profile_table" => profile(&sdk, &args),
        "stock_topics" | "topic_stocks" => topics(&sdk, name, &args),
        "auction_data" => auction(&sdk, &args),
        "market_stat" => {
            let row = sdk.market_stat(&options)?;
            Ok(
                json!({"up_count": row.up(), "down_count": row.down(), "neutral_count": row.neutral(),
                "suspended_count": row.suspended_or_unclassified(), "total_count": row.total(),
                "total_amount": amount(row.total_amount()), "total_volume": quantity(row.total_volume()),
                "total_market_cap": amount(row.market_cap()), "limit_up_count": row.limit_up(),
                "limit_down_count": row.limit_down()}),
            )
        }
        "today_fund_flow" => {
            let id = security(&args.get::<String>("code")?)?;
            let row = sdk.today_fund_flow(&id, &options)?;
            Ok(
                json!({"super_in": amount(row.super_in()), "large_in": amount(row.large_in()),
                "medium_in": amount(row.medium_in()), "small_in": amount(row.small_in()),
                "super_out": amount(row.super_out()), "large_out": amount(row.large_out()),
                "medium_out": amount(row.medium_out()), "small_out": amount(row.small_out())}),
            )
        }
        _ => Err(RpcError::unknown(method)),
    }
}

fn amount(value: Amount) -> f64 {
    value.value().as_f64()
}

fn quantity(value: Quantity) -> f64 {
    value.value().as_f64()
}

fn lots(value: Quantity) -> i64 {
    value.source_value().as_f64() as i64
}

fn ratio(value: Ratio) -> f64 {
    value.value().as_f64()
}

fn selection(args: &Arguments) -> Result<SecuritySelection> {
    match args.value("codes").filter(|value| !value.is_null()) {
        Some(value) => Ok(SecuritySelection::Explicit(securities(value)?)),
        None => Ok(SecuritySelection::AllAShares),
    }
}

fn stats(args: &Arguments) -> Result<StatsQuery> {
    Ok(StatsQuery::new().with_path(args.default("stats_path", "zhb.zip".to_owned())?)?)
}

fn f10_collection(sdk: &HelperService, name: &str, code: &str) -> Result<Value> {
    let options = with_payload();
    let field = |value: Option<&JsonValue>| value.map(f10::json_value).transpose();
    if name == "ipo_calendar" {
        let table = sdk.ipo_calendar(&options)?;
        let rows = table.rows().iter().map(|row| Ok(json!({
            "apply_date": field(row.apply_date())?, "code": row.code(), "name": row.name(),
            "market_id": row.market().map(|market| market.get()), "issue_pe": row.issue_pe(),
            "extra_pct": row.extra_pct(), "issue_price": row.issue_price(), "apply_limit": row.apply_limit(),
            "raise_amount": row.raise_amount(), "raw": f10::project_row(row.raw())?,
            "market": f10_market(row.inferred_identity()), "full_code": row.full_code(),
        }))).collect::<Result<Vec<_>>>()?;
        return Ok(json!({"rows": rows}));
    }
    let id = security(code)?;
    let code = AshareCode::from(id).with_usage(if name == "board_members" {
        CodeUse::Board
    } else {
        CodeUse::Index
    });
    match name {
        "index_members" => {
            let table = sdk.index_members(&IndexMembersQuery::new(code), &options)?;
            let rows = table.rows().iter().map(|row| Ok(json!({
                "code": row.code(), "name": row.name(), "market_id": row.market().map(|market| market.get()),
                "join_date": field(row.join_date())?, "raw": f10::project_row(row.raw())?,
                "market": f10_market(row.inferred_identity()), "full_code": row.full_code(),
            }))).collect::<Result<Vec<_>>>()?;
            Ok(
                json!({"index_code": table.index_code().as_str(), "total": table.total(), "rows": rows}),
            )
        }
        "index_etfs" => {
            let table = sdk.index_etfs(&IndexTrackingProductsQuery::new(code), &options)?;
            let rows = table.rows().iter().map(|row| Ok(json!({
                "code": row.code(), "name": row.name(), "market_id": row.market().map(|market| market.get()),
                "scale_yuan": row.scale_yuan(), "change_pct": row.change_pct(), "amount_yuan": row.amount_yuan(),
                "inception_date": field(row.inception_date())?, "manager": row.manager(), "company": row.company(),
                "nav": row.nav(), "fee_rate_pct": row.fee_rate_pct(), "raw": f10::project_row(row.raw())?,
                "market": f10_market(row.inferred_identity()), "full_code": row.full_code(),
            }))).collect::<Result<Vec<_>>>()?;
            Ok(
                json!({"index_code": table.index_code().as_str(), "total": table.total(), "rows": rows}),
            )
        }
        "board_members" => {
            let table = sdk.board_members(&BoardMembersQuery::new(code), &options)?;
            let rows = table.rows().iter().map(|row| Ok(json!({
                "code": row.code(), "name": row.name(), "market_id": row.market().map(|market| market.get()),
                "dividend_plan": row.dividend_plan(), "progress": row.progress(), "announce_date": field(row.announce_date())?,
                "record_date": field(row.record_date())?, "ex_date": field(row.ex_date())?, "report_period": field(row.report_period())?,
                "raw": f10::project_row(row.raw())?, "market": f10_market(row.inferred_identity()), "full_code": row.full_code(),
            }))).collect::<Result<Vec<_>>>()?;
            Ok(
                json!({"board_code": table.board_code().as_str(), "total": table.total(), "rows": rows}),
            )
        }
        _ => Err(RpcError::internal("unregistered helper F10 collection")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_cache_normalizes_keys_and_respects_refresh_and_clear() -> Result<()> {
        let cache = CollectionCache::default();
        assert_eq!(
            cache.get_or_load("index_etfs", " SH000300 ", false, || Ok(json!(1)))?,
            json!(1)
        );
        let unavailable = || Err(RpcError::internal("upstream unavailable"));
        assert_eq!(
            cache.get_or_load("index_etfs", "sh000300", false, unavailable)?,
            json!(1)
        );
        assert_eq!(
            cache.get_or_load("index_members", "sh000300", false, || Ok(json!(2)))?,
            json!(2)
        );
        assert_eq!(
            cache.get_or_load("index_etfs", "sh000300", true, || Ok(json!(3)))?,
            json!(3)
        );
        assert_eq!(
            cache.get_or_load("index_etfs", "sh000300", false, unavailable)?,
            json!(3)
        );
        cache.clear()?;
        assert!(cache
            .get_or_load("index_etfs", "sh000300", false, unavailable)
            .is_err());
        Ok(())
    }
}

fn f10_market(id: Option<AshareCode>) -> Option<&'static str> {
    match id?.market().get() {
        0 => Some("sz"),
        1 => Some("sh"),
        2 => Some("bj"),
        _ => None,
    }
}

fn security_row(row: &Security) -> Value {
    let category = match row.category() {
        eltdx::SecurityCategory::AShare => "a_share",
        eltdx::SecurityCategory::BShare => "b_share",
        eltdx::SecurityCategory::Etf => "etf",
        eltdx::SecurityCategory::Index => "index",
        eltdx::SecurityCategory::Bond => "bond",
        eltdx::SecurityCategory::PrivateConvertibleBond => "private_convertible_bond",
        eltdx::SecurityCategory::Other(category) => category,
        _ => "unknown",
    };
    let board = match row.board() {
        eltdx::SecurityBoard::SseMain => "sse_main",
        eltdx::SecurityBoard::SzseMain => "szse_main",
        eltdx::SecurityBoard::ChiNext => "chinext",
        eltdx::SecurityBoard::Star => "star",
        eltdx::SecurityBoard::Beijing => "beijing",
        eltdx::SecurityBoard::Other(board) => board,
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

fn realtime_rank(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let category = args.default("category", json!("沪深A股"))?;
    let sort = args.default("sort_by", json!("涨幅"))?;
    let sort_key = match sort.as_str().map(str::trim) {
        Some("代码") => QuoteSort::Code,
        Some("现价") => QuoteSort::Last,
        Some("成交额") => QuoteSort::Amount,
        Some("涨幅") => QuoteSort::ChangePct,
        Some("封单额") => QuoteSort::SealAmount,
        Some("开盘金额") => QuoteSort::OpenAmount,
        Some("涨速") => QuoteSort::RiseSpeed,
        Some("短换手") => QuoteSort::ShortTurnover,
        Some("量涨速") => QuoteSort::VolumeRiseSpeed,
        Some("开盘抢筹") => QuoteSort::OpeningRush,
        Some("2分钟金额") => QuoteSort::TwoMinuteAmount,
        Some("开盘涨幅") => QuoteSort::OpenChange,
        Some("最高涨幅") => QuoteSort::HighChange,
        Some("最低涨幅") => QuoteSort::LowChange,
        Some("回撤") => QuoteSort::Drawdown,
        Some("攻击") => QuoteSort::Attack,
        _ if sort.is_null() => QuoteSort::default(),
        _ => hq::sort(&sort)?,
    };
    let category_key = match category.as_str().map(str::trim) {
        Some("a股" | "A股") => eltdx::QuoteCategory::HqAShares,
        _ => hq::category(&category)?,
    };
    let query = RankQuery::new()
        .with_category(category_key)?
        .with_sort(sort_key)?
        .with_count(args.default("count", Some(80))?)?
        .with_ascending(args.default("ascending", false)?)?;
    let table = sdk.realtime_rank(&query, &with_payload())?;
    let rows = table
        .rows()
        .iter()
        .map(|row| {
            json!({
                "rank": row.rank(), "full_code": row.id().to_string(), "name": row.name(),
                "last_price": row.last().as_f64(), "pre_close": row.pre_close().as_f64(),
                "change_pct": row.change_pct().map(ratio), "amount": amount(row.amount()),
                "volume_hand": lots(row.volume()), "opening_rush": ratio(row.opening_rush()),
                "seal_amount": amount(row.seal_amount()), "raw": rank_quote(row.quote()),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"category": category, "sort_by": sort, "rows": rows}))
}

fn rank_quote(row: &CategoryQuote) -> Value {
    let snap = row.snapshot();
    identity(
        snap.id(),
        json!({
            "active1": snap.active(), "active2": null, "last_price": snap.last().as_f64(),
            "pre_close_price": snap.pre_close().as_f64(), "open_price": snap.open().as_f64(),
            "high_price": snap.high().as_f64(), "low_price": snap.low().as_f64(),
            "server_time_raw": snap.server_time_raw(), "neg_price_raw": snap.unknown_after_time_raw(),
            "total_hand": lots(snap.total()), "current_hand": lots(snap.current()),
            "amount": amount(snap.amount()), "amount_raw": snap.amount_wire_word(),
            "inside_dish": lots(snap.inside()), "outer_disc": lots(snap.outside()),
            "after_outer_raw": snap.unknown_after_outer_raw(), "open_amount_raw": snap.open_amount_raw(),
            "open_amount": amount(snap.open_amount()), "bid1": row.bid1().price().as_f64(),
            "ask1": row.ask1().price().as_f64(), "bid_vol1": lots(row.bid1().volume()),
            "ask_vol1": lots(row.ask1().volume()), "status_or_sort_raw": row.status_sort_raw(),
            "rise_speed_raw": row.rise_speed().source_value().as_f64(), "rise_speed": ratio(row.rise_speed()),
            "short_turnover_raw": row.short_turnover().source_value().as_f64(), "short_turnover": ratio(row.short_turnover()),
            "opening_rush_raw": row.opening_rush().source_value().as_f64(), "opening_rush": ratio(row.opening_rush()),
            "min2_amount": amount(row.two_minute_amount()), "vol_rise_speed": ratio(row.volume_rise_speed()),
            "depth": row.depth(), "extra_pair_raw": "", "extra_meta_raw": "", "tail_raw": "",
            "record_hex": quote::record(row.meta(), 0),
        }),
    )
}

fn daily_shares(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let query = DailySharesQuery::new()
        .with_selection(selection(args)?)?
        .with_stats(stats(args)?)?
        .with_refresh_stats(args.default("refresh_stats", false)?)?;
    let table = sdk.daily_share_capital(&query, &with_payload())?;
    let codes = table
        .rows()
        .iter()
        .map(|row| row.id().to_string())
        .collect::<Vec<_>>();
    let rows = table.rows().iter().map(|row| json!({
        "full_code": row.id().to_string(), "trade_date": date_value(Some(row.trade_date())),
        "total_shares": row.total().map(quantity), "circulating_shares": row.circulating().map(quantity),
        "free_float_shares": row.free_float().map(quantity), "finance_updated_date": date_value(row.finance_updated()),
        "share_source": if row.sources().is_empty() { "missing".to_owned() } else { row.sources().join("+") },
    })).collect::<Vec<_>>();
    Ok(json!({"codes": codes, "rows": rows}))
}

fn daily_limits(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let query = DailyLimitsQuery::new(date(args.required("trade_date")?)?)
        .with_selection(selection(args)?)?;
    let table = sdk.daily_price_limits(&query, &with_payload())?;
    let codes = table
        .rows()
        .iter()
        .map(|row| row.id().to_string())
        .collect::<Vec<_>>();
    let rows = table.rows().iter().map(|row| {
        let (rule, limit_ratio) = match row.rule() {
            LimitRule::Approved { id, ratio: limit, .. } => (id.as_str(), limit.map(ratio)),
            _ => ("unknown", None),
        };
        json!({"full_code": row.id().to_string(), "trade_date": date_value(Some(row.trade_date())),
            "name": row.name(), "pre_close_trade_date": date_value(row.reference_date()),
            "pre_close": row.reference().map(Price::as_f64), "limit_up_price": row.upper().map(Price::as_f64),
            "limit_down_price": row.lower().map(Price::as_f64), "limit_ratio_pct": limit_ratio,
            "limit_rule": rule, "limit_status": limit_status(row.status()), "pre_close_source": row.reference_source()})
    }).collect::<Vec<_>>();
    Ok(json!({"codes": codes, "rows": rows}))
}

fn limit_status(value: LimitStatus) -> &'static str {
    match value {
        LimitStatus::Normal => "normal",
        LimitStatus::NoPriceLimit => "no_price_limit",
        LimitStatus::MissingReference => "missing_pre_close",
        LimitStatus::UnknownRule => "unknown_rule",
        LimitStatus::Sealed => "sealed",
        LimitStatus::Touched => "touched",
        LimitStatus::NotAtLimit => "not_limit_up",
        _ => "unknown",
    }
}

fn shortline(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let query = ShortlineQuery::new(securities(args.required("codes")?)?)?
        .with_stats(stats(args)?)?
        .with_refresh_stats(args.default("refresh_stats", false)?)?;
    let table = sdk.shortline_indicators(&query, &with_payload())?;
    Ok(
        json!({"codes": table.ids().iter().map(SecurityId::to_string).collect::<Vec<_>>(),
        "target_trade_date": date_value(Some(table.target())), "previous_trade_date": date_value(Some(table.previous())),
        "stats_date": date_value(Some(table.stats_date())), "stats_source_path": table.stats_path(),
        "stats_refreshed": table.stats_refreshed(), "rows": table.rows().iter().map(shortline_row).collect::<Vec<_>>() }),
    )
}

fn shortline_row(row: &ShortlineIndicator) -> Value {
    let alignment = match row.alignment() {
        StatsAlignment::SameDay => "same_day",
        StatsAlignment::PreviousTradingDay => "previous_trading_day",
        StatsAlignment::RowMissing => "stats_row_missing",
        StatsAlignment::DateUnaligned => "stats_date_unaligned",
    };
    let status = match row.limit_status() {
        LimitStatus::Sealed => "sealed",
        LimitStatus::Touched => "touched",
        _ => "none",
    };
    identity(
        row.id(),
        json!({
            "full_code": row.id().to_string(), "target_trade_date": date_value(Some(row.target())),
            "previous_trade_date": date_value(Some(row.previous())), "stats_date": date_value(row.stats_date()),
            "alignment_status": alignment, "limit_status": status,
            "beta_60d": row.beta_60d(), "pe_ttm": row.pe_ttm(), "free_float_shares": row.free_float_shares().map(quantity),
            "prev_amount": row.previous_amount().map(amount), "prev_seal_amount": row.previous_seal().map(amount),
            "prev2_seal_amount": row.previous2_seal().map(amount), "prev_open_volume_hand": row.previous_open_volume().map(quantity),
            "prev_open_amount": row.previous_open_amount().map(amount), "limit_stat_days": row.limit_stat_days(),
            "limit_up_count_in_stat_days": row.limit_up_count(), "limit_up_streak_days": row.limit_streak(),
            "year_limit_up_days": row.year_limit_up_days(), "free_float_market_value": row.free_float_market_value().map(amount),
            "open_turnover_z": row.open_turnover().map(ratio), "open_prev_amount_ratio": row.open_previous_amount().map(ratio),
            "auction_prev_volume_ratio": row.auction_previous_volume().map(ratio), "open_prev_seal_ratio": row.open_previous_seal().map(ratio),
            "seal_to_float_ratio": row.seal_to_float().map(ratio), "seal_prev_ratio": row.seal_previous().map(ratio),
            "limit_board_text": row.board_text(), "ladder_level": row.ladder_level(), "open_price": row.open().map(Price::as_f64),
            "pre_close": row.pre_close().map(Price::as_f64), "open_change_pct": row.open_change().map(ratio),
            "open_amount": row.open_amount().map(amount), "open_volume_hand": row.open_volume().map(quantity),
            "open_volume_ratio": row.open_volume_ratio().map(ratio), "opening_rush": row.opening_rush().map(ratio),
            "float_shares": row.float_shares().map(quantity), "float_market_value": row.float_market_value().map(amount),
            "seal_amount": row.seal_amount().map(amount), "seal_to_amount_ratio": row.seal_to_amount().map(ratio),
        }),
    )
}

fn boards(sdk: &HelperService, name: &str, args: &Arguments) -> Result<Value> {
    let query = BoardQuery::new().with_refresh(args.default("refresh", false)?)?;
    let options = with_payload();
    if name == "board_member_quotes" {
        let board: BoardId = args.get::<String>("board_code")?.parse()?;
        let table = sdk.board_member_quotes(&board, &query, &options)?;
        return Ok(
            json!({"board_code": table.board().id().number(), "board_name": table.board().name(),
            "prepared_date": date_value(Some(table.prepared_date())),
            "raw_members": table.raw_members().iter().map(|row| board_member(row, false)).collect::<Vec<_>>(),
            "display_members": table.display_members().iter().map(|row| board_member(row, false)).collect::<Vec<_>>(),
            "excluded_members": table.excluded_members().iter().map(|row| board_member(row, true)).collect::<Vec<_>>(),
            "rows": table.rows().iter().map(board_quote).collect::<Vec<_>>()}),
        );
    }
    let category = args.default("category", "概念".to_owned())?;
    let category = match category.trim() {
        "概念" => BoardCategory::Concept,
        "风格" => BoardCategory::Style,
        "地区" => BoardCategory::Region,
        "一级行业" => BoardCategory::Industry1,
        "二级行业" => BoardCategory::Industry2,
        "三级行业" => BoardCategory::Industry3,
        "880行业" => BoardCategory::Industry880,
        "全部" => BoardCategory::All,
        _ => {
            return Err(RpcError::value(format!(
                "unknown board category: {category}"
            )))
        }
    };
    let table = sdk.board_quotes(&query.with_category(category)?, &options)?;
    let prepared = table.meta().source().trading_date().or_else(|| {
        table
            .meta()
            .sources()
            .iter()
            .find_map(|source| source.trading_date())
    });
    Ok(
        json!({"prepared_date": date_value(prepared), "rows": table.rows().iter().map(board_quote).collect::<Vec<_>>()}),
    )
}

fn board_member(row: &BoardMember, excluded: bool) -> Value {
    let mut value = json!({"market": market_id(row.id().market()), "code": row.id().number()});
    if excluded {
        value["reason"] = json!(row.excluded_reason());
    }
    value
}

fn board_quote(row: &BoardQuote) -> Value {
    let id = row.security_id();
    json!({"board_code": row.board().id().number(), "board_name": row.board().name(),
        "full_code": id.to_string(), "exchange": id.market().code(), "market_id": market_id(id.market()),
        "last_price": row.last().map(Price::as_f64), "pre_close_price": row.pre_close().map(Price::as_f64),
        "open_price": row.open().map(Price::as_f64), "high_price": row.high().map(Price::as_f64),
        "low_price": row.low().map(Price::as_f64), "change": row.change().map(Price::as_f64),
        "change_pct": row.change_pct().map(ratio), "amount": row.amount().map(amount),
        "volume_hand": row.volume().map(lots), "quote": row.quote().map(snapshot)})
}

fn finance_row(row: &FinanceRecord) -> Result<Value> {
    let item = BatchItem::new(0, row.id().clone(), BatchItemState::Present(row.clone()));
    let mut projected = finance::batch(&BatchOutcome::new(vec![item]), None)?;
    Ok(projected["records"][0].take())
}

fn profile(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let ids = securities(args.required("codes")?)?;
    let query = ProfileQuery::new()
        .with_security(args.default("include_security", true)?)?
        .with_finance(args.default("include_finance", true)?)?;
    let table = sdk.stock_profile_table(&ids, &query, &with_payload())?;
    let rows = table.rows().iter().map(|row| {
        let security = row.security().map(security_row);
        let category = security.as_ref().map(|value| &value["category"]);
        let board = security.as_ref().map(|value| &value["board"]);
        Ok(identity(row.id(), json!({"full_code": row.id().to_string(), "name": row.name(),
            "category": category, "board": board, "last_price": row.last().map(Price::as_f64),
            "pre_close_price": row.pre_close().map(Price::as_f64), "open_price": row.open().map(Price::as_f64),
            "high_price": row.high().map(Price::as_f64), "low_price": row.low().map(Price::as_f64),
            "change": row.change().map(Price::as_f64), "change_pct": row.change_pct().map(ratio),
            "volume_hand": row.volume().map(lots), "amount": row.amount().map(amount), "open_amount_yuan": row.open_amount().map(amount),
            "circulating_shares": row.circulating_shares().map(quantity), "total_shares": row.total_shares().map(quantity),
            "turnover_rate": row.turnover().map(ratio), "circulating_market_value": row.circulating_market_value().map(amount),
            "total_market_value": row.total_market_value().map(amount), "eps": row.eps().map(Price::as_f64),
            "ipo_date": date_value(row.ipo()), "updated_date": date_value(row.updated()),
            "security": security, "quote": snapshot(row.quote().snapshot()),
            "finance": row.finance().map(finance_row).transpose()?,
        })))
    }).collect::<Result<Vec<_>>>()?;
    Ok(json!({"codes": ids.iter().map(SecurityId::to_string).collect::<Vec<_>>(), "rows": rows}))
}

fn topics(sdk: &HelperService, name: &str, args: &Arguments) -> Result<Value> {
    let options = with_payload();
    if name == "stock_topics" {
        let id = security(&args.get::<String>("code")?)?;
        let query = StockTopicsQuery::new(AshareCode::from(id.clone()));
        let result = sdk.stock_topics(&query, &options)?;
        let topics = result.topics().iter().map(|topic| {
            let raw = topic.raw().iter().map(|(source, row)| {
                Ok((source.as_str().to_owned(), f10::project_row(row)?))
            }).collect::<Result<serde_json::Map<String, Value>>>()?;
            let mut sources = topic.sources().map(|source| source.as_str()).collect::<Vec<_>>();
            sources.sort_unstable();
            Ok(json!({"topic_id": topic.topic_id(), "topic_name": topic.topic_name(),
                "relation_level": topic.relation_level(), "selected_date": topic.selected_date().map(f10::json_value).transpose()?,
                "topic_date": topic.topic_date().map(f10::json_value).transpose()?, "reason": topic.reason(),
                "detail_id": topic.detail_id(), "category_raw": topic.category_raw().map(f10::json_value).transpose()?,
                "source": sources.join("+"), "raw": raw}))
        }).collect::<Result<Vec<_>>>()?;
        return Ok(json!({"code": id.to_string(), "topics": topics}));
    }
    let id = security(&args.get::<String>("seed_code")?)?;
    let code = AshareCode::from(id.clone());
    let topic_id = match args.value("topic_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Some(number.to_string())
        }
        _ => return Err(RpcError::request("topic_id must be a string or an integer")),
    };
    let topic_name = args.optional::<String>("topic_name")?;
    let query = match topic_id {
        Some(topic_id) => TopicStocksQuery::new(code, topic_id)?.with_topic_name(topic_name)?,
        None => match topic_name {
            Some(name) => TopicStocksQuery::named(code, name)?,
            None => TopicStocksQuery::first(code),
        },
    }
    .with_sort_by(args.default("sort_by", "zdf".to_owned())?)?
    .with_section(SectionCode::new(
        "topic_compare",
        args.default("section", "gndbzfsj".to_owned())?,
    )?)?;
    let table = sdk.topic_stocks(&query, &options)?;
    let rows = table.rows().iter().map(|row| Ok(json!({
        "rank": row.rank(), "full_code": row.id().map(SecurityId::to_string),
        "exchange": row.id().map(|id| id.market().code()), "market_id": row.id().map(|id| market_id(id.market())),
        "code": row.code(), "name": row.name(), "change_pct": row.change_pct(), "change_pct_3d": row.change_pct_3d(),
        "change_pct_5d": row.change_pct_5d(), "change_pct_20d": row.change_pct_20d(),
        "change_pct_60d": row.change_pct_60d(), "change_pct_ytd": row.change_pct_ytd(),
        "trading_date": row.trading_date().map(f10::json_value).transpose()?, "raw": f10::project_row(row.raw())?,
    }))).collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"seed_code": id.to_string(), "topic_id": table.topic_id(), "topic_name": table.topic_name(),
        "sort_by": table.sort_by(), "rows": rows}),
    )
}

fn auction(sdk: &HelperService, args: &Arguments) -> Result<Value> {
    let id = security(&args.get::<String>("code")?)?;
    let date = date_choice(args, "date")?;
    let query = AuctionDataQuery::new()
        .with_date(date)?
        .with_series(args.default("include_series", true)?)?
        .with_snapshot(args.default("include_snapshot", true)?)?
        .with_quote(args.default("include_quote", true)?)?
        .with_pre_close(
            args.optional::<f64>("pre_close_price")?
                .map(Price::from_f64),
        )?;
    let data = sdk.auction_data(&id, &query, &with_payload())?;
    let series = data.series().map(|series| {
        let points = series.points().iter().enumerate().map(|(index, point)| json!({
            "index": point.index(), "minute_of_day_raw": point.minute_of_day_raw(), "second_raw": point.second_raw(),
            "time_label": format!("{}:{:02}", hq::time_label(point.time()), point.second_raw()),
            "time_seconds": u32::from(point.minute_of_day_raw()) * 60 + u32::from(point.second_raw()),
            "price": point.price().as_f64(), "price_milli": (point.price().as_f64() * 1000.0).round() as i64,
            "matched_volume": lots(point.matched()), "unmatched_signed_raw": point.unmatched_signed(),
            "unmatched_volume": lots(point.unmatched()), "unmatched_direction_raw": point.direction_raw(),
            "reserved_zero_0e": point.reserved(), "record_hex": quote::record(series.meta(), index),
        })).collect::<Vec<_>>();
        identity(series.id(), json!({"trading_date": date_value(series.trading_date()),
            "mode_or_selector_raw": series.mode_selector(), "start_raw": series.start(),
            "limit_or_count_raw": series.requested(), "points": points, "raw_payload": quote::raw(series.meta())}))
    });
    let opening = data.opening_match().map(|tick| {
        let time = tick.time();
        let minutes = time.clock().map(|(hour, minute, _)| u32::from(hour) * 60 + u32::from(minute));
        let side = match tick.side() {
            TradeSide::Buy => "buy", TradeSide::Sell => "sell", TradeSide::Neutral => "neutral", _ => "unknown",
        };
        json!({"index": tick.index(), "absolute_index": tick.absolute_index(),
            "time_minutes": minutes, "time_label": hq::time_label(time), "trade_datetime": hq::event_time(time),
            "price": tick.price().as_f64(), "price_milli": (tick.price().as_f64() * 1000.0).round() as i64,
            "volume": lots(tick.volume()), "order_count": tick.order_count(), "status_raw": tick.status_raw(),
            "side": side, "price_delta_raw": tick.price_delta_raw(), "price_acc_raw": tick.price_acc_raw(),
            "unknown_tail_raw": tick.unknown_tail(), "reserved_zero": tick.reserved_zero(), "record_hex": "",
            "event_kind": "opening_match", "auction_matched_volume": tick.auction_matched().map(lots),
            "auction_unmatched_signed_volume": tick.auction_unmatched_signed()})
    });
    Ok(
        json!({"code": id.to_string(), "trading_date": date_value(Some(data.date())), "series": series,
        "snapshot_0925": opening, "pre_close_price": data.pre_close().map(Price::as_f64), "open_price": data.open().map(Price::as_f64),
        "open_volume": data.open_volume().map(lots), "open_amount": data.open_amount().map(amount),
        "open_change_pct": data.open_change().map(ratio)}),
    )
}
