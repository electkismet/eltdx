//! Legacy auction and market composition, preserving public Python hooks and
//! positional pseudo-index semantics while sharing pure numeric operations.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyByteArray, PyDict, PyList, PyString, PyTuple};
use crate::helper_numeric::callback;

fn get<'py>(value: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value.py().import("builtins")?.getattr("getattr")?.call1((value, name, value.py().None()))
}

fn product(py: Python<'_>, left: Bound<'_, PyAny>, right: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    eltdx::__python_compat::product_with(left.unbind(), right.unbind(),
        |left, right| Ok(py.import("operator")?.getattr("mul")?.call1((left.bind(py), right.bind(py)))?.unbind()))
}

#[pyfunction]
pub fn helper_percentage(py: Python<'_>, price: Bound<'_, PyAny>, base: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if price.is_none() || !base.is_truthy()? { return Ok(py.None()); }
    let op = py.import("operator")?;
    eltdx::__python_compat::percentage_change_with(price.unbind(), &base.unbind(),
        |price, base| Ok::<_, PyErr>(op.getattr("sub")?.call1((price.bind(py), base.bind(py)))?.unbind()),
        |difference, base| Ok(op.getattr("truediv")?.call1((difference.bind(py), base.bind(py)))?.unbind()),
        |ratio| Ok(op.getattr("mul")?.call1((ratio.bind(py), 100.0))?.unbind()))
}

#[pyfunction]
pub fn helper_market_date(py: Python<'_>, helper: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let handshake = helper.getattr("_client")?.getattr("session")?.call_method0("handshake")?;
    let first = get(&handshake, "server_date_1")?;
    let second = get(&handshake, "server_date_2")?;
    let dates = PyList::empty(py);
    for date in [first, second] { if !date.is_none() { dates.append(date)?; } }
    if dates.is_empty() { return Err(pyo3::exceptions::PyRuntimeError::new_err("server handshake did not provide a current market date")); }
    Ok(py.import("builtins")?.getattr("max")?.call1((dates,))?.unbind())
}

#[pyfunction]
pub fn helper_first_quote(py: Python<'_>, helper: Bound<'_, PyAny>, code: Bound<'_, PyAny>, sequence: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let quotes = helper.getattr("_client")?.getattr("quotes")?.call_method1("get_snapshots", (code,))?;
    if quotes.is_instance(&sequence)? && !quotes.is_instance_of::<PyString>()
        && !quotes.is_instance_of::<PyBytes>() && !quotes.is_instance_of::<PyByteArray>() {
        if quotes.is_truthy()? { Ok(quotes.get_item(0)?.unbind()) } else { Ok(py.None()) }
    } else { Ok(quotes.unbind()) }
}

#[pyfunction]
pub fn helper_snapshot_batches(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let rows = PyList::empty(py);
    let indices = (0..codes.len()?).collect::<Vec<_>>();
    for chunk in eltdx::__python_compat::full_quote_chunks(&indices) {
        py.check_signals()?;
        let start = chunk[0] as isize;
        let part = codes.get_item(pyo3::types::PySlice::new(py, start, start + chunk.len() as isize, 1))?;
        let page = helper.getattr("_client")?.getattr("quotes")?.call_method1("get_snapshots", (part,))?;
        if page.is_instance_of::<PyList>() || page.is_instance_of::<PyTuple>() { rows.call_method1("extend", (page,))?; }
    }
    Ok(rows.into_any().unbind())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn helper_auction(py: Python<'_>, helper: Bound<'_, PyAny>, code: Bound<'_, PyAny>, date: Bound<'_, PyAny>,
    include_series: Bound<'_, PyAny>, include_snapshot: Bound<'_, PyAny>, include_quote: Bound<'_, PyAny>,
    pre_close: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
    let code = callback(&scope, "normalize_code")?.call1((code,))?;
    let history = !date.is_none();
    let day = if history { helper.getattr("_client")?.getattr("workdays")?.call_method1("normalize", (date,))? }
        else { helper.call_method0("_current_market_date")? };
    let mut series = py.None();
    let mut snapshot = py.None();
    let mut historical_page = py.None();
    if include_series.is_truthy()? {
        series = helper.getattr("_client")?.getattr("auctions")?.call_method1("series",
            (&code, if history { day.clone() } else { py.None().into_bound(py) }))?.unbind();
    }
    if history {
        if include_snapshot.is_truthy()? || (include_quote.is_truthy()? && pre_close.is_none()) {
            historical_page = helper.getattr("_client")?.getattr("trades")?.call_method1("all_history", (&code, &day))?.unbind();
            if include_snapshot.is_truthy()? {
                let matches = py.import("builtins")?.getattr("getattr")?.call1((historical_page.bind(py), "opening_matches", PyTuple::empty(py)))?;
                let matches = py.import("builtins")?.getattr("tuple")?.call1((matches,))?;
                if matches.is_truthy()? { snapshot = matches.get_item(0)?.unbind(); }
            }
        }
    } else if include_snapshot.is_truthy()? {
        snapshot = helper.getattr("_client")?.getattr("trades")?.call_method1("opening_match_today", (&code,))?.unbind();
    }
    let mut quote = py.None();
    if !history && include_quote.is_truthy()? && pre_close.is_none() {
        quote = helper.call_method1("_first_quote", (&code,))?.unbind();
    }
    let mut pre_close = pre_close.unbind();
    if pre_close.is_none(py) && include_quote.is_truthy()? {
        if !historical_page.is_none(py) {
            let base = get(historical_page.bind(py), "price_base_raw_f32")?;
            if !base.is_none() && base.ne(0)? {
                let float = py.import("builtins")?.getattr("float")?.call1((base,))?;
                pre_close = py.import("builtins")?.getattr("round")?.call1((float, 3))?.unbind();
            }
        } else if !quote.is_none(py) { pre_close = get(quote.bind(py), "pre_close_price")?.unbind(); }
    }
    let mut price = py.None(); let mut volume = py.None(); let mut amount = py.None();
    if !snapshot.is_none(py) {
        price = get(snapshot.bind(py), "price")?.unbind();
        volume = get(snapshot.bind(py), "volume")?.unbind();
        amount = py.import("builtins")?.getattr("round")?.call1((snapshot.bind(py).getattr("trade_amount_yuan")?, 2))?.unbind();
    }
    let fields = PyDict::new(py);
    fields.set_item("code", code)?; fields.set_item("trading_date", day)?;
    fields.set_item("series", series)?; fields.set_item("snapshot_0925", snapshot)?;
    fields.set_item("pre_close_price", &pre_close)?; fields.set_item("open_price", &price)?;
    fields.set_item("open_volume", volume)?; fields.set_item("open_amount", amount)?;
    fields.set_item("open_change_pct", callback(&scope, "_pct")?.call1((price, pre_close))?)?;
    Ok(callback(&scope, "AuctionData")?.call((), Some(&fields))?.unbind())
}

#[pyfunction]
pub fn helper_market_stat(py: Python<'_>, client: Bound<'_, PyAny>, codes: Bound<'_, PyAny>, row_type: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let codes = py.import("builtins")?.getattr("list")?.call1((codes,))?;
    let quotes = client.getattr("quotes")?.call_method1("legacy", (codes,))?;
    if !quotes.is_truthy()? { return Err(pyo3::exceptions::PyRuntimeError::new_err("无法获取市场统计指数 880005/880001/880006 行情")); }
    // This positional association is part of the existing Python contract.
    let first = quotes.get_item(0)?;
    let count = |price: Bound<'_, PyAny>| -> PyResult<Py<PyAny>> {
        let value = product(py, price, 10_i32.into_pyobject(py)?.into_any())?;
        Ok(py.import("builtins")?.getattr("round")?.call1((value,))?.unbind())
    };
    let up = count(first.getattr("last_price")?)?;
    let down = count(first.getattr("open_price")?)?;
    let neutral = count(first.getattr("low_price")?)?;
    let total = count(first.getattr("high_price")?)?;
    let cap = if quotes.len()? > 1 { product(py, quotes.get_item(1)?.getattr("last_price")?, 1e10_f64.into_pyobject(py)?.into_any())? }
        else { 0.0_f64.into_pyobject(py)?.into_any().unbind() };
    let limit_down = if quotes.len()? > 2 { count(quotes.get_item(2)?.getattr("open_price")?)? }
        else { 0_i32.into_pyobject(py)?.into_any().unbind() };
    let limit_up = if quotes.len()? > 2 { count(quotes.get_item(2)?.getattr("last_price")?)? }
        else { 0_i32.into_pyobject(py)?.into_any().unbind() };
    let op = py.import("operator")?.getattr("sub")?;
    let remaining = op.call1((total.bind(py), up.bind(py)))?;
    let remaining = op.call1((remaining, down.bind(py)))?;
    let remaining = op.call1((remaining, neutral.bind(py)))?;
    let fields = PyDict::new(py);
    fields.set_item("up_count", up)?; fields.set_item("down_count", down)?; fields.set_item("neutral_count", neutral)?;
    fields.set_item("suspended_count", py.import("builtins")?.getattr("max")?.call1((0, remaining))?)?;
    fields.set_item("total_count", total)?; fields.set_item("total_amount", first.getattr("amount")?)?;
    fields.set_item("total_volume", py.import("builtins")?.getattr("float")?.call1((first.getattr("total_hand")?,))?)?;
    fields.set_item("total_market_cap", cap)?; fields.set_item("limit_up_count", limit_up)?;
    fields.set_item("limit_down_count", limit_down)?;
    Ok(row_type.call((), Some(&fields))?.unbind())
}

#[pyfunction]
pub fn helper_today_flow(py: Python<'_>, client: Bound<'_, PyAny>, code: Bound<'_, PyAny>, classify: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let page = client.getattr("trades")?.call_method1("all_today", (code,))?;
    let ticks = py.import("builtins")?.getattr("getattr")?.call1((&page, "ticks", &page))?;
    let ticks = if ticks.is_truthy()? { ticks } else { PyTuple::empty(py).into_any() };
    let ticks = py.import("builtins")?.getattr("tuple")?.call1((ticks,))?;
    Ok(classify.call1((ticks,))?.unbind())
}

#[pyfunction]
pub fn helper_latest_suspended(py: Python<'_>, helper: Bound<'_, PyAny>, market: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let values = helper.getattr("_client")?.getattr("codes")?.call_method1("latest_suspended", (&market,))?;
    let wanted = py.import("builtins")?.getattr("set")?.call1((values,))?;
    let stocks = helper.getattr("_client")?.getattr("codes")?.call_method1("latest_stock_list", (market,))?;
    let rows = PyList::empty(py);
    for row in stocks.try_iter()? { let row = row?;
        if wanted.contains(row.getattr("full_code")?)? { rows.append(row)?; }
    }
    Ok(rows.into_any().unbind())
}
