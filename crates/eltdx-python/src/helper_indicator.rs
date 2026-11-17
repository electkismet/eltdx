//! Python shortline record construction, with the original source/hook order.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PySlice};
use crate::helper_numeric::callback;

fn get<'py>(value: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value.py().import("builtins")?.getattr("getattr")?.call1((value, name, value.py().None()))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn shortline_indicator(py: Python<'_>, code: Bound<'_, PyAny>, quote: Bound<'_, PyAny>,
    security: Bound<'_, PyAny>, bars: Bound<'_, PyAny>, finance: Bound<'_, PyAny>,
    rush: Bound<'_, PyAny>, stats: Bound<'_, PyAny>, context: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let exchange = code.get_item(PySlice::new(py, 0, 2, 1))?;
    let symbol = code.get_item(PySlice::new(py, 2, isize::MAX, 1))?;
    let market = callback(&scope, "MARKET_TO_ID")?.get_item(&exchange)?;
    let pair = stats.call_method1("row", (&market, &symbol))?;
    let pair = py.import("builtins")?.getattr("tuple")?.call1((pair,))?;
    if pair.len()? != 2 { return Err(pyo3::exceptions::PyValueError::new_err("expected two statistics rows")); }
    let mut stat = pair.get_item(0)?;
    let stat2 = pair.get_item(1)?;
    let target = context.getattr("target_trade_date")?;
    let previous = context.getattr("previous_trade_date")?;
    let accepted = PySet::new(py, [target.call_method1("strftime", ("%Y%m%d",))?, previous.call_method1("strftime", ("%Y%m%d",))?])?;
    if !stat.is_none() && !accepted.contains(stat.getattr("stats_date")?)? { stat = py.None().into_bound(py); }
    let kwargs = PyDict::new(py);
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    let aligned = callback(&scope, "_aligned_stat2")?.call((&stat2,), Some(&kwargs))?;
    let number = callback(&scope, "_number")?;
    let last = number.call1((&quote, "last_price"))?;
    let open = number.call1((&quote, "open_price"))?;
    let open_amount = number.call1((&quote, "open_amount_yuan"))?;
    let op = py.import("operator")?;
    let denominator = if open.is_truthy()? { op.getattr("mul")?.call1((&open, 100.0))? }
        else { py.None().into_bound(py) };
    let open_volume = callback(&scope, "_safe_ratio")?.call1((&open_amount, denominator))?;
    let pre_close = number.call1((&quote, "pre_close_price"))?;
    let float_shares = callback(&scope, "_round")?.call1((get(&finance, "circulating_shares")?,))?;
    let free_shares = callback(&scope, "_tenk")?.call1((get(&stat, "free_float_shares_10k")?,))?;
    let free_value = callback(&scope, "_multiply")?.call1((&free_shares, &last))?;
    let float_value = callback(&scope, "_multiply")?.call1((&float_shares, &last))?;
    let locked = callback(&scope, "_locked_amount")?.call1((&quote,))?;
    let prior_amount = callback(&scope, "_tenk")?.call1((aligned.get_item("prev_amount_10k")?,))?;
    let prior_seal = callback(&scope, "_tenk")?.call1((aligned.get_item("prev_seal_amount_10k")?,))?;
    let prior2_seal = callback(&scope, "_tenk")?.call1((aligned.get_item("prev2_seal_amount_10k")?,))?;
    let prior_open = callback(&scope, "_tenk")?.call1((aligned.get_item("prev_open_amount_10k")?,))?;
    let prior_volume = callback(&scope, "_round")?.call1((aligned.get_item("prev_open_volume_hand")?,))?;
    let limit_status = callback(&scope, "_limit_status")?.call1((&code, &quote, get(&security, "name")?))?;
    let mut days = get(&stat, "limit_stat_days")?;
    let mut count = get(&stat, "limit_up_count_in_stat_days")?;
    let stat_date = callback(&scope, "_row_stats_date")?.call1((&stat, &stat2))?;
    if limit_status.eq("sealed")? && stat_date.eq(context.getattr("previous_trade_date")?)? {
        if !days.is_truthy()? { days = 0_i32.into_pyobject(py)?.into_any(); }
        if !count.is_truthy()? { count = 0_i32.into_pyobject(py)?.into_any(); }
        days = op.getattr("add")?.call1((days, 1))?;
        count = op.getattr("add")?.call1((count, 1))?;
    }
    let row = PyDict::new(py);
    row.set_item("full_code", code)?; row.set_item("exchange", exchange)?;
    row.set_item("market_id", market)?; row.set_item("code", symbol)?;
    row.set_item("target_trade_date", context.getattr("target_trade_date")?)?;
    row.set_item("previous_trade_date", context.getattr("previous_trade_date")?)?;
    row.set_item("stats_date", &stat_date)?; row.set_item("alignment_status", aligned.get_item("status")?.str()?)?;
    row.set_item("limit_status", &limit_status)?;
    row.set_item("beta_60d", callback(&scope, "_round")?.call1((get(&stat, "beta_60d")?,))?)?;
    row.set_item("pe_ttm", callback(&scope, "_round")?.call1((get(&stat, "pe_ttm")?,))?)?;
    row.set_item("free_float_shares", &free_shares)?;
    row.set_item("prev_amount", &prior_amount)?; row.set_item("prev_seal_amount", &prior_seal)?;
    row.set_item("prev2_seal_amount", &prior2_seal)?; row.set_item("prev_open_volume_hand", &prior_volume)?;
    row.set_item("prev_open_amount", prior_open)?;
    for field in ["limit_stat_days", "limit_up_count_in_stat_days", "limit_up_streak_days", "year_limit_up_days"] {
        row.set_item(field, get(&stat, field)?)?;
    }
    row.set_item("free_float_market_value", &free_value)?;
    let float_lots = if free_shares.is_truthy()? { op.getattr("truediv")?.call1((&free_shares, 100.0))? }
        else { py.None().into_bound(py) };
    row.set_item("open_turnover_z", callback(&scope, "_safe_ratio_pct")?.call1((&open_volume, float_lots))?)?;
    row.set_item("open_prev_amount_ratio", callback(&scope, "_safe_ratio_pct")?.call1((&open_amount, prior_amount))?)?;
    row.set_item("auction_prev_volume_ratio", callback(&scope, "_safe_ratio")?.call1((&open_volume, prior_volume))?)?;
    row.set_item("open_prev_seal_ratio", callback(&scope, "_safe_ratio_pct")?.call1((&open_amount, &prior_seal))?)?;
    row.set_item("seal_to_float_ratio", callback(&scope, "_safe_ratio_pct")?.call1((&locked, free_value))?)?;
    row.set_item("seal_prev_ratio", callback(&scope, "_safe_ratio")?.call1((&locked, prior_seal))?)?;
    row.set_item("limit_board_text", callback(&scope, "_limit_board_text")?.call1((days, count))?)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("stats_date", stat_date)?;
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    kwargs.set_item("limit_status", limit_status)?;
    row.set_item("ladder_level", callback(&scope, "_ladder_level")?.call((stat,), Some(&kwargs))?)?;
    row.set_item("open_price", &open)?; row.set_item("pre_close", &pre_close)?;
    row.set_item("open_change_pct", callback(&scope, "_change_pct")?.call1((open, pre_close))?)?;
    row.set_item("open_amount", open_amount)?; row.set_item("open_volume_hand", &open_volume)?;
    row.set_item("open_volume_ratio", callback(&scope, "_open_volume_ratio")?.call1((open_volume, bars))?)?;
    row.set_item("opening_rush", if rush.is_none() { number.call1((&quote, "opening_rush"))? } else { rush })?;
    row.set_item("float_shares", float_shares)?; row.set_item("float_market_value", float_value)?;
    row.set_item("seal_amount", &locked)?;
    row.set_item("seal_to_amount_ratio", callback(&scope, "_safe_ratio")?.call1((locked, number.call1((quote, "amount"))?))?)?;
    Ok(callback(&scope, "ShortlineIndicator")?.call((), Some(&row))?.unbind())
}
