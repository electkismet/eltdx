//! Legacy helper orchestration retains Python identity, cache keys and hooks.
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PySlice, PyTuple};

fn get<'py>(object: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let py = object.py();
    py.import("builtins")?.getattr("getattr")?.call1((object, name, py.None()))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn helper_daily_shares(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyAny>,
    stats_path: Bound<'_, PyAny>, refresh: Bound<'_, PyAny>, resolve_context: Bound<'_, PyAny>,
    validate_dates: Bound<'_, PyAny>, as_float: Bound<'_, PyAny>, row_type: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    let context = resolve_context.call1((helper.getattr("_client")?,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("refresh", refresh)?;
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    let response = helper.getattr("_shortline")?.call_method("_stats_resource", (stats_path,), Some(&kwargs))?;
    let response = py.import("builtins")?.getattr("tuple")?.call1((response,))?;
    if response.len()? != 2 { return Err(pyo3::exceptions::PyValueError::new_err("expected stats resource and refreshed flag")); }
    let stats = response.get_item(0)?;
    kwargs.del_item("refresh")?;
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    let stats_date = validate_dates.call((&stats,), Some(&kwargs))?;
    let finance_map = helper.call_method1("_finance_map", (&codes,))?;
    let markets = PyDict::new(py);
    markets.set_item("sz", 0)?; markets.set_item("sh", 1)?; markets.set_item("bj", 2)?;
    let builtins = py.import("builtins")?;
    let float = builtins.getattr("float")?;
    let round = builtins.getattr("round")?;
    let multiply = py.import("operator")?.getattr("mul")?;
    let rows = PyList::empty(py);
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        let market = markets.get_item(code.get_item(PySlice::new(py, 0, 2, 1))?)?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("unknown market"))?;
        let pair = stats.call_method1("row", (market, code.get_item(PySlice::new(py, 2, isize::MAX, 1))?))?;
        let pair = builtins.getattr("tuple")?.call1((pair,))?;
        if pair.len()? != 2 { return Err(pyo3::exceptions::PyValueError::new_err("expected two statistics rows")); }
        let stat = pair.get_item(0)?;
        let finance = finance_map.call_method1("get", (&code,))?;
        let total = as_float.call1((get(&finance, "total_shares")?,))?;
        let circulating = as_float.call1((get(&finance, "circulating_shares")?,))?;
        let free_float = if stat.is_none() || stat.getattr("free_float_shares_10k")?.is_none() { py.None() }
            else {
                let value = float.call1((stat.getattr("free_float_shares_10k")?,))?.unbind();
                let value = eltdx::__python_compat::product_with(value, 10000.0_f64.into_pyobject(py)?.into_any().unbind(),
                    |left, right| Ok::<_, PyErr>(multiply.call1((left.bind(py), right.bind(py)))?.unbind()))?;
                round.call1((value.bind(py), 6))?.unbind()
            };
        let sources = eltdx::__python_compat::share_source_names(!total.is_none(), !circulating.is_none(), !free_float.is_none());
        let kwargs = PyDict::new(py);
        kwargs.set_item("full_code", code)?; kwargs.set_item("trade_date", &stats_date)?;
        kwargs.set_item("total_shares", total)?; kwargs.set_item("circulating_shares", circulating)?;
        kwargs.set_item("free_float_shares", free_float)?;
        kwargs.set_item("finance_updated_date", get(&finance, "updated_date")?)?;
        kwargs.set_item("share_source", if sources.is_empty() { "missing".to_owned() } else { sources.join("+") })?;
        rows.append(row_type.call((), Some(&kwargs))?)?;
    }
    Ok(PyTuple::new(py, rows.iter())?.into_any().unbind())
}

#[pyfunction]
pub fn helper_by_code(py: Python<'_>, items: Bound<'_, PyAny>, mapping_type: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let iterable = if items.is_instance(&mapping_type)? { items.call_method0("values")? }
        else if items.is_truthy()? { items } else { PyTuple::empty(py).into_any() };
    for item in iterable.try_iter()? {
        py.check_signals()?;
        let item = item?;
        let key = get(&item, "full_code")?;
        if !key.is_none() { result.set_item(key.str()?, item)?; }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_security_map(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    let markets = PySet::empty(py)?;
    for code in codes.try_iter()? { markets.add(code?.get_item(PySlice::new(py, 0, 2, 1))?)?; }
    let builtins = py.import("builtins")?;
    let sorted = builtins.getattr("sorted")?.call1((markets,))?;
    let cache = helper.getattr("_security_cache")?;
    let result = PyDict::new(py);
    for market in sorted.try_iter()? {
        py.check_signals()?;
        let market = market?;
        let mut rows = cache.call_method1("get", (&market,))?;
        if rows.is_none() {
            let source = helper.getattr("_client")?.getattr("codes")?.call_method1("all", (&market,))?;
            rows = builtins.getattr("tuple")?.call1((source,))?;
            cache.set_item(&market, &rows)?;
        }
        for item in rows.try_iter()? {
            let item = item?;
            // Python evaluates an eager default expression even if full_code exists.
            let exchange = item.getattr("exchange")?.call_method1("__format__", ("",))?;
            let code = item.getattr("code")?.call_method1("__format__", ("",))?;
            let fallback = format!("{}{}", exchange.extract::<String>()?, code.extract::<String>()?);
            let key = builtins.getattr("getattr")?.call1((&item, "full_code", fallback))?;
            result.set_item(key, item)?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_finance_map(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyAny>,
    by_code: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let corporate = get(&helper.getattr("_client")?, "corporate")?;
    let getter = get(&corporate, "finance_batch")?;
    if getter.is_none() { return Ok(PyDict::new(py).into_any().unbind()); }
    let tuple_type = py.import("builtins")?.getattr("tuple")?;
    let key = tuple_type.call1((&codes,))?;
    let cache = helper.getattr("_finance_cache")?;
    let mut batch = cache.call_method1("get", (&key,))?;
    if batch.is_none() {
        let records = PyList::empty(py);
        let indices = (0..codes.len()?).collect::<Vec<_>>();
        for chunk in eltdx::__python_compat::full_quote_chunks(&indices) {
            py.check_signals()?;
            let start = chunk[0] as isize;
            let page = match codes.get_item(PySlice::new(py, start, start + chunk.len() as isize, 1))
                .and_then(|part| getter.call1((part,))) {
                Ok(page) => page,
                Err(error) if error.is_instance_of::<PyException>(py) => continue,
                Err(error) => return Err(error),
            };
            // Only the request is optional: malformed records still fail and
            // must not turn into a successfully cached partial result.
            let values = get(&page, "records")?;
            if values.is_truthy()? { records.call_method1("extend", (values,))?; }
        }
        batch = tuple_type.call1((records,))?;
        cache.set_item(key, &batch)?;
    }
    let fallback = if batch.is_instance_of::<PyTuple>() { batch.clone() }
        else { PyTuple::empty(py).into_any() };
    let rows = py.import("builtins")?.getattr("getattr")?.call1((batch, "records", fallback))?;
    Ok(by_code.call1((rows,))?.unbind())
}

#[pyfunction]
pub fn helper_profile(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyAny>,
    include_security: Bound<'_, PyAny>, include_finance: Bound<'_, PyAny>,
    by_code: Bound<'_, PyAny>, builder: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let quotes = by_code.call1((helper.call_method1("full_quotes", (&codes,))?,))?;
    let securities = if include_security.is_truthy()? { helper.call_method1("_security_map", (&codes,))? }
        else { PyDict::new(py).into_any() };
    let finances = if include_finance.is_truthy()? { helper.call_method1("_finance_map", (&codes,))? }
        else { PyDict::new(py).into_any() };
    let rows = PyList::empty(py);
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        rows.append(builder.call1((&code, quotes.call_method1("get", (&code,))?,
            securities.call_method1("get", (&code,))?, finances.call_method1("get", (&code,))?))?)?;
    }
    Ok(PyTuple::new(py, rows.iter())?.into_any().unbind())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn build_profile(py: Python<'_>, code: Bound<'_, PyAny>, quote: Bound<'_, PyAny>,
    security: Bound<'_, PyAny>, finance: Bound<'_, PyAny>, row_type: Bound<'_, PyAny>,
    turnover: Bound<'_, PyAny>, market_value: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let exchange = code.get_item(PySlice::new(py, 0, 2, 1))?;
    let symbol = code.get_item(PySlice::new(py, 2, isize::MAX, 1))?;
    let mut market = get(&quote, "market_id")?;
    if market.is_none() { market = get(&security, "market_id")?; }
    if market.is_none() { market = get(&finance, "market_id")?; }
    let price = get(&quote, "last_price")?;
    let circulating = get(&finance, "circulating_shares")?;
    let total = get(&finance, "total_shares")?;
    let volume = get(&quote, "total_hand")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("full_code", code)?; kwargs.set_item("exchange", exchange)?;
    kwargs.set_item("market_id", market)?; kwargs.set_item("code", symbol)?;
    for name in ["name", "category", "board"] { kwargs.set_item(name, get(&security, name)?)?; }
    kwargs.set_item("last_price", &price)?;
    for name in ["pre_close_price", "open_price", "high_price", "low_price", "change", "change_pct"] {
        kwargs.set_item(name, get(&quote, name)?)?;
    }
    kwargs.set_item("volume_hand", &volume)?;
    for name in ["amount", "open_amount_yuan"] { kwargs.set_item(name, get(&quote, name)?)?; }
    kwargs.set_item("circulating_shares", &circulating)?; kwargs.set_item("total_shares", &total)?;
    kwargs.set_item("turnover_rate", turnover.call1((volume, &circulating))?)?;
    kwargs.set_item("circulating_market_value", market_value.call1((&price, circulating))?)?;
    kwargs.set_item("total_market_value", market_value.call1((price, total))?)?;
    kwargs.set_item("eps", get(&finance, "eps_raw")?)?;
    for name in ["ipo_date", "updated_date"] { kwargs.set_item(name, get(&finance, name)?)?; }
    kwargs.set_item("security", security)?; kwargs.set_item("quote", quote)?; kwargs.set_item("finance", finance)?;
    Ok(row_type.call((), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn helper_turnover(py: Python<'_>, volume: Bound<'_, PyAny>, shares: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    if volume.is_none() || !shares.is_truthy()? { return Ok(py.None()); }
    let operator = py.import("operator")?;
    let multiply = operator.getattr("mul")?;
    let divide = operator.getattr("truediv")?;
    eltdx::__python_compat::turnover_with(volume.unbind(), &shares.unbind(),
        |value| Ok(multiply.call1((value.bind(py), 100.0))?.unbind()),
        |value, denominator| Ok(divide.call1((value.bind(py), denominator.bind(py)))?.unbind()))
}

#[pyfunction]
pub fn helper_market_value(py: Python<'_>, price: Bound<'_, PyAny>, shares: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    if price.is_none() || shares.is_none() { return Ok(py.None()); }
    let multiply = py.import("operator")?.getattr("mul")?;
    eltdx::__python_compat::product_with(price.unbind(), shares.unbind(),
        |left, right| Ok(multiply.call1((left.bind(py), right.bind(py)))?.unbind()))
}
