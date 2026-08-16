//! Conversion of protocol responses into the private, allocation-bounded Python DTO ABI.
//!
//! The outer DTO is always `(variant_name, payload)`. `payload` contains only Python
//! scalars, `bytes`, `None`, lists, and tuples. Python model construction remains in
//! `_native_models.py`; no Rust protocol type crosses the extension boundary.

use std::sync::Arc;

use eltdx_protocol::commands::{
    auctions::{AuctionPoint, AuctionSeries},
    corporate::{CapitalChangeBlock, CapitalChangeRecord, FinanceBatch, FinanceRecord},
    klines::{KlineBar, KlineSeries},
    limits::{SpecialLimitPage, SpecialLimitRecord},
    minutes::{MinuteAuxPoint, MinuteAuxSeries, MinutePoint, MinuteSeries, SparklineSeries},
    quotes::{
        CategoryQuotePage, CategoryQuoteRecord, LegacyQuote, QuoteLevel, QuoteRefreshPage,
        QuoteRefreshRecord, QuoteSnapshot,
    },
    resources::FileContentChunk,
    security::SecurityCode,
    session::{HandshakeInfo, HeartbeatAck},
    trades::{TradeEventKind, TradePage, TradeSide, TradeTick},
};
use eltdx_protocol::response::CommandResponse;
use eltdx_protocol::unit::{DateParts, DateTimeParts, Market};
use eltdx_runtime::push::PushFrame;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyString, PyTuple};
use pyo3::IntoPyObjectExt;

type Obj = Py<PyAny>;

const SNAPSHOT_STRIDE: usize = 27;
const TRADE_TICK_STRIDE: usize = 19;

struct TradeSemanticNames {
    buy: Obj,
    sell: Obj,
    neutral: Obj,
    trade: Obj,
    opening_match: Obj,
    auction_snapshot: Obj,
}

impl TradeSemanticNames {
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            buy: any(py, "buy")?,
            sell: any(py, "sell")?,
            neutral: any(py, "neutral")?,
            trade: any(py, "trade")?,
            opening_match: any(py, "opening_match")?,
            auction_snapshot: any(py, "auction_snapshot")?,
        })
    }

    fn side(&self, py: Python<'_>, value: &TradeSide) -> PyResult<Obj> {
        match value {
            TradeSide::Buy => Ok(self.buy.clone_ref(py)),
            TradeSide::Sell => Ok(self.sell.clone_ref(py)),
            TradeSide::Neutral => Ok(self.neutral.clone_ref(py)),
            TradeSide::Status(_) => any(py, value.canonical_name().as_ref()),
        }
    }

    fn event_kind(&self, py: Python<'_>, value: TradeEventKind) -> Obj {
        match value {
            TradeEventKind::Trade => self.trade.clone_ref(py),
            TradeEventKind::OpeningMatch => self.opening_match.clone_ref(py),
            TradeEventKind::AuctionSnapshot => self.auction_snapshot.clone_ref(py),
        }
    }
}

struct TradeTickObjects {
    semantic_names: TradeSemanticNames,
    last_time_minutes: Option<(u16, Obj)>,
    last_time_label: Option<(Arc<str>, Obj)>,
    last_record_hex: Option<(Arc<str>, Obj)>,
}

impl TradeTickObjects {
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            semantic_names: TradeSemanticNames::new(py)?,
            last_time_minutes: None,
            last_time_label: None,
            last_record_hex: None,
        })
    }

    fn time_minutes(&mut self, py: Python<'_>, value: u16) -> PyResult<Obj> {
        if let Some((previous, object)) = &self.last_time_minutes {
            if *previous == value {
                return Ok(object.clone_ref(py));
            }
        }
        let object = any(py, value)?;
        self.last_time_minutes = Some((value, object.clone_ref(py)));
        Ok(object)
    }

    fn time_label(&mut self, py: Python<'_>, value: &Arc<str>) -> Obj {
        if let Some((previous, object)) = &self.last_time_label {
            if Arc::ptr_eq(previous, value) {
                return object.clone_ref(py);
            }
        }
        let object = PyString::new(py, value.as_ref()).into_any().unbind();
        self.last_time_label = Some((Arc::clone(value), object.clone_ref(py)));
        object
    }

    fn record_hex(&mut self, py: Python<'_>, include_raw: bool, value: &Arc<str>) -> Obj {
        if let Some((previous, object)) = &self.last_record_hex {
            if Arc::ptr_eq(previous, value) {
                return object.clone_ref(py);
            }
        }
        let object = record_hex(py, include_raw, value.as_ref());
        self.last_record_hex = Some((Arc::clone(value), object.clone_ref(py)));
        object
    }
}

fn any<'py, T>(py: Python<'py>, value: T) -> PyResult<Obj>
where
    T: IntoPyObject<'py>,
{
    Ok(value.into_bound_py_any(py)?.unbind())
}

fn none<'py>(py: Python<'py>) -> Obj {
    py.None()
}

fn bytes<'py>(py: Python<'py>, value: &[u8]) -> Obj {
    PyBytes::new(py, value).into_any().unbind()
}

fn tuple<'py>(py: Python<'py>, values: Vec<Obj>) -> PyResult<Obj> {
    Ok(PyTuple::new(py, values)?.into_any().unbind())
}

fn tuple_array<'py, const N: usize>(py: Python<'py>, values: [Obj; N]) -> PyResult<Obj> {
    Ok(PyTuple::new(py, values)?.into_any().unbind())
}

fn list<'py>(py: Python<'py>, values: Vec<Obj>) -> PyResult<Obj> {
    Ok(PyList::new(py, values)?.into_any().unbind())
}

fn date<'py>(py: Python<'py>, value: Option<DateParts>) -> PyResult<Obj> {
    match value {
        Some(value) => tuple(
            py,
            vec![
                any(py, value.year)?,
                any(py, value.month)?,
                any(py, value.day)?,
            ],
        ),
        None => Ok(none(py)),
    }
}

fn datetime<'py>(py: Python<'py>, value: Option<DateTimeParts>) -> PyResult<Obj> {
    match value {
        Some(value) => tuple(
            py,
            vec![
                any(py, value.date.year)?,
                any(py, value.date.month)?,
                any(py, value.date.day)?,
                any(py, value.hour)?,
                any(py, value.minute)?,
                any(py, value.second)?,
                value
                    .utc_offset_seconds
                    .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            ],
        ),
        None => Ok(none(py)),
    }
}

fn market(value: Option<Market>, raw: u8) -> &'static str {
    value.map(Market::as_str).unwrap_or_else(|| match raw {
        0 => "sz",
        1 => "sh",
        2 => "bj",
        _ => "unknown",
    })
}

fn code_parts<'py>(
    py: Python<'py>,
    exchange: &str,
    market_id: u8,
    code: &str,
) -> PyResult<Vec<Obj>> {
    Ok(vec![
        any(py, exchange)?,
        any(py, market_id)?,
        any(py, code)?,
    ])
}

fn tagged<'py>(py: Python<'py>, name: &'static str, payload: Obj) -> PyResult<Obj> {
    tuple(py, vec![any(py, name)?, payload])
}

fn raw_payload<'py>(py: Python<'py>, _include_raw: bool, payload: &[u8]) -> Obj {
    bytes(py, payload)
}

fn record_hex<'py>(py: Python<'py>, _include_raw: bool, value: &str) -> Obj {
    PyString::new(py, value).into_any().unbind()
}

fn level<'py>(py: Python<'py>, value: &QuoteLevel) -> PyResult<Obj> {
    tuple_array(
        py,
        [
            any(py, value.price)?,
            any(py, value.volume)?,
            any(py, value.price_delta_raw)?,
        ],
    )
}

fn levels<'py>(py: Python<'py>, values: &[QuoteLevel]) -> PyResult<Obj> {
    tuple(
        py,
        values
            .iter()
            .map(|value| level(py, value))
            .collect::<PyResult<Vec<_>>>()?,
    )
}

fn security<'py>(py: Python<'py>, value: &SecurityCode) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, value.market.as_str())?,
            any(py, value.market.id())?,
            any(py, value.code.as_str())?,
            any(py, value.name.as_str())?,
            any(py, value.multiple)?,
            any(py, value.decimal)?,
            any(py, f64::from(value.previous_close_price))?,
            any(py, f64::from(value.volume_ratio_base))?,
            bytes(py, &value.unknown0_raw),
            bytes(py, &value.previous_close_raw),
            bytes(py, &value.unknown3_raw),
            any(py, value.category.as_str())?,
            any(py, value.category_reason)?,
            any(py, value.board.as_str())?,
            any(py, value.board_reason())?,
        ],
    )
}

fn auction_point<'py>(py: Python<'py>, value: &AuctionPoint, include_raw: bool) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, value.index)?,
            any(py, value.minute_of_day_raw)?,
            any(py, value.second_raw)?,
            any(py, value.time_label.as_str())?,
            any(py, value.time_seconds)?,
            any(py, f64::from(value.price))?,
            any(py, value.price_milli)?,
            any(py, value.matched_volume)?,
            any(py, value.unmatched_signed_raw)?,
            any(py, value.unmatched_volume)?,
            any(py, value.unmatched_direction_raw)?,
            any(py, value.reserved_zero_0e)?,
            record_hex(py, include_raw, &value.record_hex),
        ],
    )
}

fn capital_record<'py>(
    py: Python<'py>,
    value: &CapitalChangeRecord,
    include_raw: bool,
) -> PyResult<Obj> {
    let mut fields = code_parts(
        py,
        market(value.market, value.market_id),
        value.market_id,
        &value.code,
    )?;
    fields.extend([
        any(py, value.reserved_7)?,
        any(py, value.date_raw)?,
        date(py, value.date)?,
        any(py, value.category_raw)?,
        value
            .category_name
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        bytes(py, &value.c1_raw),
        bytes(py, &value.c2_raw),
        bytes(py, &value.c3_raw),
        bytes(py, &value.c4_raw),
        any(py, f64::from(value.c1_float))?,
        any(py, f64::from(value.c2_float))?,
        any(py, f64::from(value.c3_float))?,
        any(py, f64::from(value.c4_float))?,
        any(py, value.c1_value)?,
        any(py, value.c2_value)?,
        any(py, value.c3_value)?,
        any(py, value.c4_value)?,
        record_hex(py, include_raw, &value.record_hex),
    ]);
    tuple(py, fields)
}

fn finance_record<'py>(py: Python<'py>, value: &FinanceRecord, include_raw: bool) -> PyResult<Obj> {
    let mut fields = code_parts(
        py,
        market(value.market, value.market_id),
        value.market_id,
        &value.code,
    )?;
    fields.extend([
        bytes(py, &value.finance_info_raw),
        any(py, f64::from(value.liu_tong_gu_ben_raw_float))?,
        any(py, value.province_raw)?,
        any(py, value.industry_raw)?,
        any(py, value.updated_date_raw)?,
        date(py, value.updated_date)?,
        any(py, value.ipo_date_raw)?,
        date(py, value.ipo_date)?,
        any(py, f64::from(value.zong_gu_ben_raw_float))?,
        any(py, f64::from(value.guo_jia_gu_raw_float))?,
        any(py, f64::from(value.fa_qi_ren_fa_ren_gu_raw_float))?,
        any(py, f64::from(value.fa_ren_gu_raw_float))?,
        any(py, f64::from(value.b_gu_raw_float))?,
        any(py, f64::from(value.h_gu_raw_float))?,
        any(py, f64::from(value.eps_raw))?,
        any(py, f64::from(value.zong_zi_chan_raw_float))?,
        any(py, f64::from(value.liu_dong_zi_chan_raw_float))?,
        any(py, f64::from(value.gu_ding_zi_chan_raw_float))?,
        any(py, f64::from(value.wu_xing_zi_chan_raw_float))?,
        any(py, f64::from(value.gu_dong_ren_shu_raw_float))?,
        any(py, f64::from(value.liu_dong_fu_zhai_raw_float))?,
        any(py, f64::from(value.chang_qi_fu_zhai_raw_float))?,
        any(py, f64::from(value.zi_ben_gong_ji_jin_raw_float))?,
        any(py, f64::from(value.jing_zi_chan_raw_float))?,
        any(py, f64::from(value.zhu_ying_shou_ru_raw_float))?,
        any(py, f64::from(value.zhu_ying_li_run_raw_float))?,
        any(py, f64::from(value.ying_shou_zhang_kuan_raw_float))?,
        any(py, f64::from(value.ying_ye_li_run_raw_float))?,
        any(py, f64::from(value.tou_zi_shou_yu_raw_float))?,
        any(py, f64::from(value.jing_ying_xian_jin_liu_raw_float))?,
        any(py, f64::from(value.zong_xian_jin_liu_raw_float))?,
        any(py, f64::from(value.cun_huo_raw_float))?,
        any(py, f64::from(value.li_run_zong_he_raw_float))?,
        any(py, f64::from(value.shui_hou_li_run_raw_float))?,
        any(py, f64::from(value.jing_li_run_raw_float))?,
        any(py, f64::from(value.wei_fen_li_run_raw_float))?,
        any(py, f64::from(value.mei_gu_jing_zi_chan_raw_float))?,
        any(py, f64::from(value.bao_liu_2_raw_float))?,
        record_hex(py, include_raw, &value.record_hex),
    ]);
    tuple(py, fields)
}

fn minute_point<'py>(py: Python<'py>, value: &MinutePoint, include_raw: bool) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, value.index)?,
            any(py, value.time_label.as_str())?,
            datetime(py, value.time)?,
            any(py, value.price)?,
            any(py, value.price_milli)?,
            any(py, value.volume)?,
            value
                .price_field
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .avg_field
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .avg_price
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .price_raw
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value.avg_raw.map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .price_delta_raw
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .aux_delta_raw
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            record_hex(py, include_raw, &value.record_hex),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn minute_series<'py, R>(
    py: Python<'py>,
    value: &MinuteSeries<R>,
    include_raw: bool,
    code: &str,
    exchange: &str,
    market_id: u8,
    trading_date: Option<DateParts>,
    date_selector_raw: Option<u32>,
) -> PyResult<Obj> {
    let points = tuple(
        py,
        value
            .points
            .iter()
            .map(|p| minute_point(py, p, include_raw))
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    tuple(
        py,
        vec![
            any(py, exchange)?,
            any(py, market_id)?,
            any(py, code)?,
            date(py, trading_date)?,
            points,
            value
                .reserved_zero
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .prev_close
                .map_or_else(|| Ok(none(py)), |v| any(py, f64::from(v)))?,
            value
                .open_price
                .map_or_else(|| Ok(none(py)), |v| any(py, f64::from(v)))?,
            value
                .date_selector_raw
                .or(date_selector_raw)
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            raw_payload(py, include_raw, &value.raw_payload),
        ],
    )
}

fn extend_trade_tick<'py>(
    py: Python<'py>,
    fields: &mut Vec<Obj>,
    value: &TradeTick,
    include_raw: bool,
    objects: &mut TradeTickObjects,
) -> PyResult<()> {
    let index = any(py, value.index)?;
    let absolute_index = if u32::from(value.index) == value.absolute_index {
        index.clone_ref(py)
    } else {
        any(py, value.absolute_index)?
    };
    let time_minutes = objects.time_minutes(py, value.time_minutes)?;
    let time_label = objects.time_label(py, &value.time_label);
    let record_hex = objects.record_hex(py, include_raw, &value.record_hex);
    fields.extend([
        index,
        absolute_index,
        time_minutes,
        time_label,
        datetime(py, value.trade_datetime)?,
        any(py, value.price)?,
        any(py, value.price_milli)?,
        any(py, value.volume)?,
        any(py, value.order_count)?,
        any(py, value.status_raw)?,
        objects.semantic_names.side(py, &value.side)?,
        any(py, value.price_delta_raw)?,
        any(py, value.price_acc_raw)?,
        value
            .unknown_tail_raw
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        value
            .reserved_zero
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        record_hex,
        objects.semantic_names.event_kind(py, value.event_kind),
        value
            .auction_matched_volume
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        value
            .auction_unmatched_signed_volume
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
    ]);
    Ok(())
}

fn trade_ticks<'py>(py: Python<'py>, values: &[TradeTick], include_raw: bool) -> PyResult<Obj> {
    let capacity = values
        .len()
        .checked_mul(TRADE_TICK_STRIDE)
        .ok_or_else(|| PyValueError::new_err("native trade tick DTO length overflow"))?;
    let mut fields = Vec::with_capacity(capacity);
    let mut objects = TradeTickObjects::new(py)?;
    for value in values {
        extend_trade_tick(py, &mut fields, value, include_raw, &mut objects)?;
    }
    tuple(py, fields)
}

fn extend_quote_snapshot<'py>(
    py: Python<'py>,
    fields: &mut Vec<Obj>,
    value: &QuoteSnapshot,
) -> PyResult<()> {
    let [buy_level] = value.buy_levels.as_slice() else {
        return Err(PyValueError::new_err(
            "native snapshot must contain exactly one buy level",
        ));
    };
    let [sell_level] = value.sell_levels.as_slice() else {
        return Err(PyValueError::new_err(
            "native snapshot must contain exactly one sell level",
        ));
    };
    fields.extend([
        any(py, market_id_name(value.market_id))?,
        any(py, value.market_id)?,
        any(py, value.code.as_str())?,
        any(py, value.active1)?,
        any(py, value.last_price)?,
        any(py, value.pre_close_price)?,
        any(py, value.open_price)?,
        any(py, value.high_price)?,
        any(py, value.low_price)?,
        any(py, value.time_raw)?,
        any(py, value.unknown_after_time_raw)?,
        any(py, value.total_hand)?,
        any(py, value.current_hand)?,
        any(py, value.amount)?,
        any(py, value.amount_raw)?,
        any(py, value.inside_dish)?,
        any(py, value.outer_disc)?,
        any(py, value.unknown_after_outer_raw)?,
        any(py, value.open_amount_raw)?,
        any(py, value.open_amount_yuan)?,
        any(py, buy_level.price)?,
        any(py, buy_level.volume)?,
        any(py, buy_level.price_delta_raw)?,
        any(py, sell_level.price)?,
        any(py, sell_level.volume)?,
        any(py, sell_level.price_delta_raw)?,
        bytes(py, &value.tail_raw),
    ]);
    Ok(())
}

fn quote_snapshots<'py>(py: Python<'py>, values: &[QuoteSnapshot]) -> PyResult<Obj> {
    let capacity = values
        .len()
        .checked_mul(SNAPSHOT_STRIDE)
        .ok_or_else(|| PyValueError::new_err("native snapshot DTO length overflow"))?;
    let mut fields = Vec::with_capacity(capacity);
    for value in values {
        extend_quote_snapshot(py, &mut fields, value)?;
    }
    tuple(py, fields)
}

fn market_id_name(value: u8) -> &'static str {
    match value {
        0 => "sz",
        1 => "sh",
        2 => "bj",
        _ => "unknown",
    }
}

fn legacy_quote<'py>(py: Python<'py>, value: &LegacyQuote, include_raw: bool) -> PyResult<Obj> {
    let mut fields = code_parts(py, value.market.as_str(), value.market.id(), &value.code)?;
    fields.extend([
        any(py, value.active1)?,
        any(py, value.last_price)?,
        any(py, value.pre_close_price)?,
        any(py, value.open_price)?,
        any(py, value.high_price)?,
        any(py, value.low_price)?,
        any(py, value.server_time_raw)?,
        any(py, value.unknown_after_time_raw)?,
        any(py, value.total_hand)?,
        any(py, value.current_hand)?,
        any(py, value.amount)?,
        any(py, value.amount_raw)?,
        any(py, value.inside_dish)?,
        any(py, value.outer_disc)?,
        any(py, value.unknown_after_outer_raw)?,
        any(py, value.open_amount_raw)?,
        any(py, value.open_amount_yuan)?,
        levels(py, &value.buy_levels)?,
        levels(py, &value.sell_levels)?,
        any(py, value.trading_status_raw)?,
        tuple(
            py,
            value
                .tail_metrics_raw
                .iter()
                .map(|v| any(py, *v))
                .collect::<PyResult<Vec<_>>>()?,
        )?,
        value
            .rise_speed_raw
            .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        value.active2.map_or_else(|| Ok(none(py)), |v| any(py, v))?,
        record_hex(py, include_raw, &value.record_hex),
    ]);
    tuple(py, fields)
}

fn category_quote<'py>(
    py: Python<'py>,
    value: &CategoryQuoteRecord,
    include_raw: bool,
) -> PyResult<Obj> {
    let mut fields = code_parts(
        py,
        market_id_name(value.market_id),
        value.market_id,
        &value.code,
    )?;
    fields.extend([
        any(py, value.active1)?,
        any(py, value.active2)?,
        any(py, value.last_price)?,
        any(py, value.pre_close_price)?,
        any(py, value.open_price)?,
        any(py, value.high_price)?,
        any(py, value.low_price)?,
        any(py, value.server_time_raw)?,
        any(py, value.neg_price_raw)?,
        any(py, value.total_hand)?,
        any(py, value.current_hand)?,
        any(py, value.amount)?,
        any(py, value.amount_raw)?,
        any(py, value.inside_dish)?,
        any(py, value.outer_disc)?,
        any(py, value.after_outer_raw)?,
        any(py, value.open_amount_raw)?,
        any(py, value.open_amount)?,
        any(py, value.bid1)?,
        any(py, value.ask1)?,
        any(py, value.bid_vol1)?,
        any(py, value.ask_vol1)?,
        any(py, value.status_or_sort_raw)?,
        any(py, value.rise_speed_raw)?,
        any(py, value.rise_speed)?,
        any(py, value.short_turnover_raw)?,
        any(py, value.short_turnover)?,
        any(py, f64::from(value.min2_amount))?,
        any(py, value.opening_rush_raw)?,
        any(py, value.opening_rush)?,
        bytes(py, &value.extra_pair_raw),
        any(py, f64::from(value.vol_rise_speed))?,
        any(py, f64::from(value.depth))?,
        bytes(py, &value.extra_meta_raw),
        bytes(py, &value.tail_raw),
        record_hex(py, include_raw, &value.record_hex),
    ]);
    tuple(py, fields)
}

fn refresh_quote<'py>(
    py: Python<'py>,
    value: &QuoteRefreshRecord,
    include_raw: bool,
) -> PyResult<Obj> {
    let mut fields = code_parts(
        py,
        market_id_name(value.market_id),
        value.market_id,
        &value.code,
    )?;
    fields.extend([
        any(py, value.active)?,
        any(py, value.update_time_raw)?,
        any(py, value.last_price)?,
        any(py, value.last_close_price)?,
        any(py, value.open_price)?,
        any(py, value.high_price)?,
        any(py, value.low_price)?,
        any(py, value.status_or_reserved_raw)?,
        any(py, value.total_hand)?,
        any(py, value.current_hand)?,
        any(py, value.amount)?,
        any(py, value.amount_raw)?,
        any(py, value.inside_dish)?,
        any(py, value.outer_disc)?,
        any(py, value.unknown_after_outer_raw)?,
        any(py, value.open_amount_raw)?,
        any(py, value.open_amount_yuan)?,
        levels(py, &value.buy_levels)?,
        levels(py, &value.sell_levels)?,
        bytes(py, &value.tail_raw),
        record_hex(py, include_raw, &value.record_hex),
    ]);
    tuple(py, fields)
}

fn kline_bar<'py>(py: Python<'py>, value: &KlineBar, include_raw: bool) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            datetime(py, Some(value.time))?,
            any(py, value.open)?,
            any(py, value.close)?,
            any(py, value.high)?,
            any(py, value.low)?,
            any(py, value.open_price_milli)?,
            any(py, value.close_price_milli)?,
            any(py, value.high_price_milli)?,
            any(py, value.low_price_milli)?,
            value
                .last_close_price_milli
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            any(py, value.volume_raw)?,
            any(py, value.amount_raw)?,
            any(py, value.volume_wire_value)?,
            any(py, value.volume_lots)?,
            any(py, value.amount)?,
            any(py, value.open_delta_raw)?,
            any(py, value.close_delta_raw)?,
            any(py, value.high_delta_raw)?,
            any(py, value.low_delta_raw)?,
            value
                .up_count
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            value
                .down_count
                .map_or_else(|| Ok(none(py)), |v| any(py, v))?,
            record_hex(py, include_raw, &value.record_hex),
        ],
    )
}

fn response_frame<'py>(
    py: Python<'py>,
    frame: &eltdx_protocol::frame::ResponseFrame,
) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, frame.control)?,
            any(py, frame.msg_id)?,
            any(py, frame.msg_type)?,
            any(py, frame.zip_length)?,
            any(py, frame.length)?,
            bytes(py, &frame.data),
            bytes(py, &frame.raw),
            any(py, frame.response_header_reserved)?,
        ],
    )
}

pub fn to_python(py: Python<'_>, response: CommandResponse) -> PyResult<Py<PyAny>> {
    let value = match response {
        CommandResponse::Heartbeat(value) => tagged(py, "heartbeat", heartbeat(py, &value)?)?,
        CommandResponse::Handshake(value) => tagged(py, "handshake", handshake(py, &value)?)?,
        CommandResponse::CapitalChanges(value) => {
            tagged(py, "capital_changes", capital_changes(py, &value)?)?
        }
        CommandResponse::FinanceBatch(value) => {
            tagged(py, "finance_batch", finance_batch(py, &value)?)?
        }
        CommandResponse::SecurityList(values) => tagged(
            py,
            "security_list",
            list(
                py,
                values
                    .iter()
                    .map(|v| security(py, v))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
        )?,
        CommandResponse::SecurityCount(value) => tagged(py, "security_count", any(py, value)?)?,
        CommandResponse::SpecialLimits(value) => {
            tagged(py, "special_limits", special_limits(py, &value)?)?
        }
        CommandResponse::IntradayAux(value) => {
            tagged(py, "intraday_aux", intraday_aux(py, &value)?)?
        }
        CommandResponse::Klines(value) => tagged(py, "klines", klines(py, &value)?)?,
        CommandResponse::TodayIntraday(value) => {
            tagged(py, "today_intraday", today_intraday(py, &value)?)?
        }
        CommandResponse::LegacyQuotes(values) => tagged(
            py,
            "legacy_quotes",
            list(
                py,
                values
                    .iter()
                    .map(|v| legacy_quote(py, v, true))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
        )?,
        CommandResponse::RefreshStream(value) => {
            tagged(py, "refresh_stream", refresh_stream(py, &value)?)?
        }
        CommandResponse::CategoryQuotes(value) => {
            tagged(py, "category_quotes", category_quotes(py, &value)?)?
        }
        CommandResponse::Snapshots(values) => {
            tagged(py, "snapshots", quote_snapshots(py, &values)?)?
        }
        CommandResponse::AuctionSeries(value) => {
            tagged(py, "auction_series", auction_series(py, &value)?)?
        }
        CommandResponse::FileContent(value) => {
            tagged(py, "file_content", file_content(py, &value)?)?
        }
        CommandResponse::HistoricalIntraday(value) => {
            tagged(py, "historical_intraday", historical_intraday(py, &value)?)?
        }
        CommandResponse::TodayTicks(value) => tagged(py, "today_ticks", today_ticks(py, &value)?)?,
        CommandResponse::HistoricalTicks(value) => {
            tagged(py, "historical_ticks", historical_ticks(py, &value)?)?
        }
        CommandResponse::Sparkline(value) => tagged(py, "sparkline", sparkline(py, &value)?)?,
        CommandResponse::RecentIntraday(value) => {
            tagged(py, "recent_intraday", recent_intraday(py, &value)?)?
        }
    };
    Ok(value)
}

fn heartbeat<'py>(py: Python<'py>, value: &HeartbeatAck) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            bytes(py, &value.reserved),
            any(py, value.server_date_raw)?,
            date(py, value.server_date)?,
            bytes(py, &value.raw_payload),
        ],
    )
}
fn handshake<'py>(py: Python<'py>, value: &HandshakeInfo) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            datetime(py, value.server_datetime)?,
            tuple(
                py,
                value
                    .session_minutes_1
                    .iter()
                    .map(|v| any(py, v))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            tuple(
                py,
                value
                    .session_minutes_2
                    .iter()
                    .map(|v| any(py, v))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            date(py, value.server_date_1)?,
            date(py, value.server_date_2)?,
            any(py, value.server_name.as_str())?,
            any(py, value.product_tag.as_str())?,
            any(py, value.unknown_time_1_raw)?,
            any(py, value.unknown_time_2_raw)?,
            bytes(py, &value.flags_raw),
            bytes(py, &value.tail_control_raw),
            bytes(py, &value.raw_payload),
        ],
    )
}

fn capital_changes<'py>(py: Python<'py>, value: &CapitalChangeBlock) -> PyResult<Obj> {
    let include_raw = value.request.include_raw;
    tuple(
        py,
        vec![
            any(py, market(value.market, value.market_id))?,
            any(py, value.market_id)?,
            any(py, value.code.as_str())?,
            any(py, value.block_count)?,
            tuple(
                py,
                value
                    .records
                    .iter()
                    .map(|v| capital_record(py, v, include_raw))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            raw_payload(py, include_raw, &value.raw_payload),
        ],
    )
}
fn finance_batch<'py>(py: Python<'py>, value: &FinanceBatch) -> PyResult<Obj> {
    let include_raw = value.request.include_raw();
    tuple(
        py,
        vec![
            tuple(
                py,
                value
                    .records
                    .iter()
                    .map(|v| finance_record(py, v, include_raw))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            raw_payload(py, include_raw, &value.raw_payload),
        ],
    )
}
fn special_limits<'py>(py: Python<'py>, value: &SpecialLimitPage) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, value.request.start_index)?,
            tuple(
                py,
                value
                    .records
                    .iter()
                    .map(|v| special_limit_record(py, v))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            bytes(py, &value.raw_payload),
        ],
    )
}
fn special_limit_record<'py>(py: Python<'py>, value: &SpecialLimitRecord) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, market(value.market, value.market_id))?,
            any(py, value.market_id)?,
            any(py, value.code_num)?,
            any(py, value.code.as_str())?,
            any(py, f64::from(value.upper_price_raw_f32))?,
            any(py, f64::from(value.lower_price_raw_f32))?,
            record_hex(py, true, &value.record_hex),
        ],
    )
}

fn intraday_aux<'py>(py: Python<'py>, value: &MinuteAuxSeries) -> PyResult<Obj> {
    let req = &value.request;
    let points = tuple(
        py,
        value
            .points
            .iter()
            .map(|p| minute_aux_point(py, p, req.include_raw))
            .collect::<PyResult<Vec<_>>>()?,
    )?;
    tuple(
        py,
        vec![
            any(py, req.code.market().as_str())?,
            any(py, req.code.market().id())?,
            any(py, req.code.number())?,
            any(py, req.kind.raw())?,
            any(py, req.kind.canonical_name())?,
            points,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}
fn minute_aux_point<'py>(
    py: Python<'py>,
    value: &MinuteAuxPoint,
    include_raw: bool,
) -> PyResult<Obj> {
    match value {
        MinuteAuxPoint::BuySellStrength {
            index,
            time_label,
            series_a,
            series_b,
            record_hex: raw_hex,
        } => tuple(
            py,
            vec![
                any(py, *index)?,
                any(py, time_label.as_str())?,
                any(py, *series_a)?,
                any(py, *series_b)?,
                any(py, *series_a)?,
                any(py, *series_b)?,
                none(py),
                none(py),
                none(py),
                record_hex(py, include_raw, raw_hex),
            ],
        ),
        MinuteAuxPoint::VolumeComparison {
            index,
            time_label,
            previous_day_cumulative_volume,
            current_day_cumulative_volume,
            record_hex: raw_hex,
            ..
        } => tuple(
            py,
            vec![
                any(py, *index)?,
                any(py, time_label.as_str())?,
                any(py, f64::from(*previous_day_cumulative_volume))?,
                any(py, f64::from(*current_day_cumulative_volume))?,
                none(py),
                none(py),
                any(py, f64::from(*previous_day_cumulative_volume))?,
                any(py, f64::from(*current_day_cumulative_volume))?,
                any(
                    py,
                    f64::from(*previous_day_cumulative_volume + *current_day_cumulative_volume),
                )?,
                record_hex(py, include_raw, raw_hex),
            ],
        ),
    }
}

fn klines<'py>(py: Python<'py>, value: &KlineSeries) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, req.code.market().as_str())?,
            any(py, req.code.market().id())?,
            any(py, req.code.number())?,
            any(py, req.period.raw)?,
            any(py, req.period.parameter)?,
            any(py, req.period.name())?,
            any(py, req.start)?,
            any(py, req.count)?,
            any(py, req.adjust as u16)?,
            any(py, req.adjust.as_str())?,
            any(py, req.anchor_date_raw)?,
            date(py, value.anchor_date)?,
            tuple(
                py,
                value
                    .bars
                    .iter()
                    .map(|v| kline_bar(py, v, req.include_raw))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}
fn today_intraday<'py>(
    py: Python<'py>,
    value: &MinuteSeries<eltdx_protocol::commands::minutes::TodayIntradayRequest>,
) -> PyResult<Obj> {
    let req = &value.request;
    minute_series(
        py,
        value,
        req.include_raw,
        req.code.number(),
        req.code.market().as_str(),
        req.code.market().id(),
        None,
        None,
    )
}
fn historical_intraday<'py>(
    py: Python<'py>,
    value: &MinuteSeries<eltdx_protocol::commands::minutes::HistoricalIntradayRequest>,
) -> PyResult<Obj> {
    let req = &value.request;
    minute_series(
        py,
        value,
        req.include_raw,
        req.code.number(),
        req.code.market().as_str(),
        req.code.market().id(),
        Some(req.trading_date),
        None,
    )
}
fn recent_intraday<'py>(
    py: Python<'py>,
    value: &MinuteSeries<eltdx_protocol::commands::minutes::RecentIntradayRequest>,
) -> PyResult<Obj> {
    let req = &value.request;
    minute_series(
        py,
        value,
        req.include_raw,
        req.code.number(),
        req.code.market().as_str(),
        req.code.market().id(),
        Some(req.trading_date),
        Some(req.date_selector_raw),
    )
}
fn refresh_stream<'py>(py: Python<'py>, value: &QuoteRefreshPage) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            tuple(
                py,
                value
                    .requested_codes
                    .iter()
                    .map(|v| any(py, v.full_code()))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            tuple(
                py,
                value
                    .records
                    .iter()
                    .map(|v| refresh_quote(py, v, true))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            bytes(py, &value.decoded_payload),
            bytes(py, &value.raw_payload),
        ],
    )
}
fn category_quotes<'py>(py: Python<'py>, value: &CategoryQuotePage) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, req.category)?,
            any(py, req.sort_type)?,
            any(py, req.start)?,
            any(py, req.count)?,
            any(py, req.sort_reverse)?,
            any(py, req.filter_raw)?,
            any(py, value.header)?,
            tuple(
                py,
                value
                    .records
                    .iter()
                    .map(|v| category_quote(py, v, true))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            bytes(py, &value.raw_payload),
        ],
    )
}
fn auction_series<'py>(py: Python<'py>, value: &AuctionSeries) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, req.code.market().as_str())?,
            any(py, req.code.market().id())?,
            any(py, req.code.number())?,
            date(py, req.trading_date)?,
            any(py, req.mode_or_selector_raw)?,
            any(py, req.start_raw)?,
            any(py, req.limit_or_count_raw)?,
            tuple(
                py,
                value
                    .points
                    .iter()
                    .map(|v| auction_point(py, v, req.include_raw))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}
fn file_content<'py>(py: Python<'py>, value: &FileContentChunk) -> PyResult<Obj> {
    tuple(
        py,
        vec![
            any(py, value.request.path())?,
            any(py, value.request.offset())?,
            any(py, value.request.size())?,
            any(py, value.chunk_len)?,
            bytes(py, &value.content),
            bytes(py, &value.raw_payload),
        ],
    )
}
fn today_ticks<'py>(
    py: Python<'py>,
    value: &TradePage<eltdx_protocol::commands::trades::TodayTicksRequest>,
) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, req.code.market().as_str())?,
            any(py, req.code.market().id())?,
            any(py, req.code.number())?,
            any(py, req.start)?,
            any(py, req.count)?,
            trade_ticks(py, &value.ticks, req.include_raw)?,
            none(py),
            value
                .price_base_raw_f32
                .map_or_else(|| Ok(none(py)), |v| any(py, f64::from(v)))?,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}
fn historical_ticks<'py>(
    py: Python<'py>,
    value: &TradePage<eltdx_protocol::commands::trades::HistoricalTicksRequest>,
) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, req.code.market().as_str())?,
            any(py, req.code.market().id())?,
            any(py, req.code.number())?,
            any(py, req.start)?,
            any(py, req.count)?,
            trade_ticks(py, &value.ticks, req.include_raw)?,
            date(py, Some(req.trading_date))?,
            value
                .price_base_raw_f32
                .map_or_else(|| Ok(none(py)), |v| any(py, f64::from(v)))?,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}
fn sparkline<'py>(py: Python<'py>, value: &SparklineSeries) -> PyResult<Obj> {
    let req = &value.request;
    tuple(
        py,
        vec![
            any(py, value.response_market.as_str())?,
            any(py, value.response_market_id)?,
            any(py, value.response_code.as_str())?,
            any(py, req.selector)?,
            any(py, value.selector_echo)?,
            any(py, req.window_or_count_raw)?,
            any(py, value.max_count_raw)?,
            any(py, f64::from(value.base_price))?,
            tuple(
                py,
                value
                    .prices
                    .iter()
                    .map(|v| any(py, f64::from(v.value)))
                    .collect::<PyResult<Vec<_>>>()?,
            )?,
            any(py, value.reserved_param_u32)?,
            raw_payload(py, req.include_raw, &value.raw_payload),
        ],
    )
}

pub fn push_to_python(py: Python<'_>, frame: PushFrame, parse: bool) -> PyResult<Py<PyAny>> {
    let payload = tuple(
        py,
        vec![
            any(py, frame.engine_epoch.get())?,
            any(py, frame.slot_id.get())?,
            any(py, frame.generation.get())?,
            any(py, frame.connected_host.as_ref())?,
            response_frame(py, &frame.response)?,
            any(py, parse)?,
        ],
    )?;
    tagged(py, "push", payload)
}
