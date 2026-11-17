//! Legacy shortline date strings and values stay at the Python boundary.
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PySlice, PyTuple};

fn get<'py>(value: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value
        .py()
        .import("builtins")?
        .getattr("getattr")?
        .call1((value, name, value.py().None()))
}

fn hook<'py>(scope: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    scope
        .get_item(name)?
        .ok_or_else(|| pyo3::exceptions::PyNameError::new_err(name.to_owned()))
}

fn records<'py>(page: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let raw = get(page, name)?;
    let py = page.py();
    if raw.is_truthy()? {
        py.import("builtins")?.getattr("tuple")?.call1((raw,))
    } else {
        Ok(PyTuple::empty(py).into_any())
    }
}

#[pyfunction]
pub fn helper_shortline_by_code(py: Python<'_>, items: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let items = if items.is_truthy()? {
        items
    } else {
        PyTuple::empty(py).into_any()
    };
    for item in items.try_iter()? {
        py.check_signals()?;
        let item = item?;
        let code = get(&item, "full_code")?;
        if !code.is_none() {
            result.set_item(code.str()?, item)?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_shortline_row_date(
    py: Python<'_>,
    stat: Bound<'_, PyAny>,
    stat2: Bound<'_, PyAny>,
    parse: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let first = get(&stat2, "stats_date")?;
    let value = if first.is_truthy()? {
        first
    } else {
        get(&stat, "stats_date")?
    };
    if value.is_truthy()? {
        Ok(parse.call1((value,))?.unbind())
    } else {
        Ok(py.None())
    }
}

#[pyfunction]
pub fn helper_handshake(py: Python<'_>, client: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let transport = get(&client, "transport")?;
    let raw = get(&transport, "_transports")?;
    let candidates = if raw.is_truthy()? {
        py.import("builtins")?.getattr("list")?.call1((raw,))?
    } else {
        PyList::empty(py).into_any()
    };
    if !transport.is_none() && !candidates.is_truthy()? {
        candidates.call_method1("append", (transport,))?;
    }
    for candidate in candidates.try_iter()? {
        let value = get(&candidate?, "last_handshake")?;
        if !value.is_none() {
            return Ok(value.unbind());
        }
    }
    let request = get(&get(&client, "session")?, "handshake")?;
    if request.is_callable() {
        Ok(request.call0()?.unbind())
    } else {
        Ok(py.None())
    }
}

#[pyfunction]
pub fn helper_market_context(
    py: Python<'_>,
    client: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>,
) -> PyResult<Py<PyAny>> {
    let handshake = hook(&scope, "_client_handshake_info")?.call1((&client,))?;
    let error = hook(&scope, "TdxStatsDateError")?;
    if handshake.is_none() {
        return Err(raised(
            &error,
            "unable to resolve the target trading day without a TDX handshake",
        ));
    }
    let datetime = get(&handshake, "server_datetime")?;
    if !datetime.is_instance(&hook(&scope, "datetime")?)? {
        return Err(raised(
            &error,
            "TDX handshake does not contain a usable server datetime",
        ));
    }
    let dates = PyList::empty(py);
    // Both getattr calls precede filtering and sorting in the original tuple.
    let first = get(&handshake, "server_date_1")?;
    let second = get(&handshake, "server_date_2")?;
    for date in [first, second] {
        if date.is_instance(&hook(&scope, "date")?)? {
            dates.append(date)?;
        }
    }
    let dates = py.import("builtins")?.getattr("sorted")?.call1((dates,))?;
    if dates.len()? == 0 {
        return Err(raised(
            &error,
            "TDX handshake does not contain a usable target trading day",
        ));
    }
    let target = dates.get_item(dates.len()? - 1)?;
    let previous = client
        .getattr("workdays")?
        .call_method1("previous_workday", (&target,))?;
    if previous.is_none() {
        return Err(raised(
            &error,
            &format!(
                "unable to resolve the previous trading day for {}",
                target.call_method0("isoformat")?.str()?.to_str()?
            ),
        ));
    }
    let same = datetime.call_method0("date")?.eq(&target)?;
    let before = same
        && datetime
            .call_method0("time")?
            .lt(hook(&scope, "AUCTION_READY_TIME")?)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("target_trade_date", target)?;
    kwargs.set_item("previous_trade_date", previous)?;
    kwargs.set_item("ready", eltdx::__python_compat::auction_ready(same, before))?;
    Ok(hook(&scope, "_MarketDateContext")?
        .call((), Some(&kwargs))?
        .unbind())
}

#[pyfunction]
pub fn helper_shortline_get(
    py: Python<'_>,
    service: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    path: Bound<'_, PyAny>,
    refresh: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>,
) -> PyResult<Py<PyAny>> {
    let context =
        hook(&scope, "_resolve_market_date_context")?.call1((service.getattr("_client")?,))?;
    if !context.getattr("ready")?.is_truthy()? {
        return Err(raised(
            &hook(&scope, "ShortlineIndicatorsNotReadyError")?,
            "shortline indicators are not ready before the 09:25 auction completes",
        ));
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("refresh", refresh)?;
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    let (stats, refreshed) =
        pair(service.call_method("_stats_resource", (path,), Some(&kwargs))?)?;
    kwargs.del_item("refresh")?;
    kwargs.set_item("target", context.getattr("target_trade_date")?)?;
    kwargs.set_item("previous", context.getattr("previous_trade_date")?)?;
    let resource_date =
        hook(&scope, "_validate_stats_resource_dates")?.call((&stats,), Some(&kwargs))?;
    let quote_map = hook(&scope, "_by_full_code")?.call1((service
        .getattr("_client")?
        .getattr("helpers")?
        .call_method1("full_quotes", (&codes,))?,))?;
    let security_map =
        hook(&scope, "_security_map")?.call1((service.getattr("_client")?, &codes))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("before", context.getattr("target_trade_date")?)?;
    let bars_map = hook(&scope, "_recent_daily_bars_map")?
        .call((service.getattr("_client")?, &codes), Some(&kwargs))?;
    let finance_map = hook(&scope, "_finance_map")?.call1((service.getattr("_client")?, &codes))?;
    let rush_map =
        hook(&scope, "_opening_rush_map")?.call1((service.getattr("_client")?, &codes))?;
    let rows = PyList::empty(py);
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("quote", quote_map.call_method1("get", (&code,))?)?;
        kwargs.set_item("security", security_map.call_method1("get", (&code,))?)?;
        kwargs.set_item(
            "recent_daily_bars",
            bars_map.call_method1("get", (&code, PyTuple::empty(py)))?,
        )?;
        kwargs.set_item("finance", finance_map.call_method1("get", (&code,))?)?;
        kwargs.set_item("opening_rush", rush_map.call_method1("get", (&code,))?)?;
        kwargs.set_item("stats", &stats)?;
        kwargs.set_item("context", &context)?;
        rows.append(hook(&scope, "_build_indicator")?.call((&code,), Some(&kwargs))?)?;
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        "codes",
        py.import("builtins")?.getattr("tuple")?.call1((codes,))?,
    )?;
    kwargs.set_item("target_trade_date", context.getattr("target_trade_date")?)?;
    kwargs.set_item(
        "previous_trade_date",
        context.getattr("previous_trade_date")?,
    )?;
    kwargs.set_item("stats_date", resource_date)?;
    kwargs.set_item("stats_source_path", stats.getattr("source_path")?)?;
    kwargs.set_item("stats_refreshed", refreshed)?;
    kwargs.set_item("rows", PyTuple::new(py, rows.iter())?)?;
    Ok(hook(&scope, "ShortlineIndicatorTable")?
        .call((), Some(&kwargs))?
        .unbind())
}

#[pyfunction]
pub fn helper_shortline_security(
    py: Python<'_>,
    client: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let markets = PySet::empty(py)?;
    for code in codes.try_iter()? {
        markets.add(code?.get_item(PySlice::new(py, 0, 2, 1))?)?;
    }
    let markets = py
        .import("builtins")?
        .getattr("sorted")?
        .call1((markets,))?;
    let result = PyDict::new(py);
    for market in markets.try_iter()? {
        py.check_signals()?;
        let page = client.getattr("codes")?.call_method1("all", (market?,))?;
        for item in page.try_iter()? {
            let item = item?;
            let code = get(&item, "full_code")?;
            if codes.contains(&code)? {
                result.set_item(code.str()?, item)?;
            }
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_shortline_finance(
    py: Python<'_>,
    client: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let getter = get(&get(&client, "corporate")?, "finance_batch")?;
    if getter.is_none() {
        return Ok(result.into_any().unbind());
    }
    let indices = (0..codes.len()?).collect::<Vec<_>>();
    for chunk in eltdx::__python_compat::full_quote_chunks(&indices) {
        py.check_signals()?;
        let start = chunk[0] as isize;
        let page = match codes
            .get_item(PySlice::new(py, start, start + chunk.len() as isize, 1))
            .and_then(|part| getter.call1((part,)))
        {
            Ok(page) => page,
            Err(error) if error.is_instance_of::<PyException>(py) => continue,
            Err(error) => return Err(error),
        };
        let raw = get(&page, "records")?;
        if !raw.is_truthy()? {
            continue;
        }
        for item in raw.try_iter()? {
            let item = item?;
            let code = get(&item, "full_code")?;
            if !code.is_none() {
                result.set_item(code.str()?, item)?;
            }
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_shortline_bars(
    py: Python<'_>,
    client: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    before: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let getter = get(&get(&client, "bars")?, "get")?;
    if getter.is_none() {
        return Ok(result.into_any().unbind());
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("period", "day")?;
    kwargs.set_item("start", 0)?;
    kwargs.set_item("count", 6)?;
    kwargs.set_item("adjust", "none")?;
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        let page = match getter.call((&code,), Some(&kwargs)) {
            Ok(page) => page,
            Err(error) if error.is_instance_of::<PyException>(py) => continue,
            Err(error) => return Err(error),
        };
        let values = records(&page, "bars")?;
        let values = if before.is_none() {
            values
        } else {
            let selected = PyList::empty(py);
            for bar in values.try_iter()? {
                let bar = bar?;
                let time = get(&bar, "time")?;
                let day = match time.getattr("date") {
                    Ok(method) => method.call0()?,
                    Err(error)
                        if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) =>
                    {
                        py.None().into_bound(py)
                    }
                    Err(error) => return Err(error),
                };
                if day.is_none() || bar.getattr("time")?.call_method0("date")?.lt(&before)? {
                    selected.append(bar)?;
                }
            }
            PyTuple::new(py, selected.iter())?.into_any()
        };
        if values.is_truthy()? {
            result.set_item(code, values.get_item(PySlice::new(py, 0, 5, 1))?)?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn helper_shortline_rush(
    py: Python<'_>,
    client: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    if codes.len()? < 80 {
        return Ok(result.into_any().unbind());
    }
    let getter = get(&get(&client, "quotes")?, "list_by_category")?;
    if getter.is_none() {
        return Ok(result.into_any().unbind());
    }
    let wanted = py.import("builtins")?.getattr("set")?.call1((codes,))?;
    let float = py.import("builtins")?.getattr("float")?;
    let round = py.import("builtins")?.getattr("round")?;
    let mut offset = 0_usize;
    while wanted.is_truthy()? && offset < 200 * 80 {
        py.check_signals()?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("sort_by", "代码")?;
        kwargs.set_item("start", offset)?;
        kwargs.set_item("count", 80)?;
        kwargs.set_item("ascending", true)?;
        let page = match getter.call(("沪深A股",), Some(&kwargs)) {
            Ok(page) => page,
            Err(error) if error.is_instance_of::<PyException>(py) => break,
            Err(error) => return Err(error),
        };
        let values = records(&page, "records")?;
        let count = values.len()?;
        if count == 0 {
            break;
        }
        for row in values.try_iter()? {
            let row = row?;
            let code = get(&row, "full_code")?;
            let value = get(&row, "opening_rush")?;
            if wanted.contains(&code)? && !value.is_none() {
                result.set_item(code.str()?, round.call1((float.call1((value,))?, 6))?)?;
                wanted.call_method1("discard", (code,))?;
            }
        }
        offset += count;
        if count < 80 {
            break;
        }
    }
    Ok(result.into_any().unbind())
}

fn raised(kind: &Bound<'_, PyAny>, message: &str) -> PyErr {
    match kind.call1((message,)) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    }
}

fn pair<'py>(value: Bound<'py, PyAny>) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let tuple = value
        .py()
        .import("builtins")?
        .getattr("tuple")?
        .call1((value,))?;
    let count = tuple.len()?;
    if count < 2 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "not enough values to unpack (expected 2, got {count})"
        )));
    }
    if count > 2 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "too many values to unpack (expected 2)",
        ));
    }
    Ok((tuple.get_item(0)?, tuple.get_item(1)?))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn helper_validate_stats(
    py: Python<'_>,
    resource: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    previous: Bound<'_, PyAny>,
    dominant: Bound<'_, PyAny>,
    parse: Bound<'_, PyAny>,
    minimum: Bound<'_, PyAny>,
    format_error: Bound<'_, PyAny>,
    date_error: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let (date, coverage) =
        pair(dominant.call1((resource.getattr("stat")?.call_method0("values")?,))?)?;
    let (date2, coverage2) =
        pair(dominant.call1((resource.getattr("stat2")?.call_method0("values")?,))?)?;
    if date.is_none() || date2.is_none() {
        return Err(raised(
            &format_error,
            "TDX statistics resource has no dominant date in tdxstat.cfg or tdxstat2.cfg",
        ));
    }
    if !date.eq(&date2)? {
        return Err(raised(
            &format_error,
            &format!(
                "TDX statistics resource dates disagree: tdxstat.cfg={}, tdxstat2.cfg={}",
                date.str()?.to_str()?,
                date2.str()?.to_str()?
            ),
        ));
    }
    if coverage.lt(&minimum)? || coverage2.lt(&minimum)? {
        let format = py.import("builtins")?.getattr("format")?;
        let text = format.call1((coverage, ".2%"))?.extract::<String>()?;
        let text2 = format.call1((coverage2, ".2%"))?.extract::<String>()?;
        return Err(raised(&format_error, &format!("TDX statistics resource dominant-date coverage is too low: tdxstat.cfg={text}, tdxstat2.cfg={text2}")));
    }
    let parsed = parse.call1((date,))?;
    let accepted = PySet::new(py, [&target, &previous])?;
    if !accepted.contains(&parsed)? {
        let day = parsed.call_method0("isoformat")?.extract::<String>()?;
        let today = target.call_method0("isoformat")?.extract::<String>()?;
        let previous = previous.call_method0("isoformat")?.extract::<String>()?;
        return Err(raised(&date_error, &format!("TDX statistics resource is not usable for the target session: stats_date={day}, target={today}, previous={previous}")));
    }
    Ok(parsed.unbind())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn helper_stats_resource(
    py: Python<'_>,
    service: Bound<'_, PyAny>,
    path: Bound<'_, PyAny>,
    refresh: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    previous: Bound<'_, PyAny>,
    usable: Bound<'_, PyAny>,
    validate: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cache = service.getattr("_stats_cache")?;
    let cached = cache.call_method1("get", (&path,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("target", target)?;
    kwargs.set_item("previous", previous)?;
    if !refresh.is_truthy()?
        && !cached.is_none()
        && usable.call((&cached,), Some(&kwargs))?.is_truthy()?
    {
        return Ok((cached, false).into_pyobject(py)?.into_any().unbind());
    }
    let resource = service
        .getattr("_client")?
        .getattr("resources")?
        .call_method1("read_stats", (&path,))?;
    validate.call((&resource,), Some(&kwargs))?;
    cache.set_item(path, &resource)?;
    Ok((resource, true).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn helper_dominant_date(
    py: Python<'_>,
    rows: Bound<'_, PyAny>,
) -> PyResult<(Option<String>, f64)> {
    // Materialize before property evaluation, matching the original list(rows).
    let list_type = py.import("builtins")?.getattr("list")?;
    let rows = list_type.call1((rows,))?;
    let dates = rows.try_iter()?.map(|row| {
        let row = row?;
        if row.getattr("stats_date")?.is_truthy()? {
            Ok(Some(row.getattr("stats_date")?.str()?.to_str()?.to_owned()))
        } else {
            Ok(None)
        }
    });
    let (date, count, total) = eltdx::__python_compat::dominant_date(
        dates,
        || py.check_signals(),
        || pyo3::exceptions::PyOverflowError::new_err("statistics row count overflow"),
    )?;
    Ok((
        date,
        if count == 0 {
            0.0
        } else {
            count as f64 / total.max(1) as f64
        },
    ))
}

#[pyfunction]
pub fn helper_aligned_stat2(
    py: Python<'_>,
    row: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    previous: Bound<'_, PyAny>,
    empty: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if row.is_none() || !row.getattr("stats_date")?.is_truthy()? {
        return Ok(empty.call1(("stats_row_missing",))?.unbind());
    }
    let target = target.call_method1("strftime", ("%Y%m%d",))?;
    let previous = previous.call_method1("strftime", ("%Y%m%d",))?;
    let same = row.getattr("stats_date")?.eq(&target)?;
    let prior = if same {
        false
    } else {
        row.getattr("stats_date")?.eq(&previous)?
    };
    if !same && !prior {
        return Ok(empty.call1(("stats_date_unaligned",))?.unbind());
    }
    let result = PyDict::new(py);
    result.set_item(
        "status",
        if prior {
            "previous_trading_day"
        } else {
            "same_day"
        },
    )?;
    let keys = [
        "prev_amount_10k",
        "prev_seal_amount_10k",
        "prev2_seal_amount_10k",
        "prev_open_volume_hand",
        "prev_open_amount_10k",
    ];
    for (key, field) in keys
        .into_iter()
        .zip(eltdx::__python_compat::stat2_columns(prior))
    {
        use eltdx::__python_compat::Stat2Column::*;
        let name = match field {
            Amount => "amount_10k",
            Seal => "seal_amount_10k",
            PreviousAmount => "prev_amount_10k",
            PreviousSeal => "prev_seal_amount_10k",
            Previous2Seal => "prev2_seal_amount_10k",
            OpenVolume => "open_volume_hand",
            OpenAmount => "open_amount_10k",
            PreviousOpenVolume => "prev_open_volume_hand",
            PreviousOpenAmount => "prev_open_amount_10k",
        };
        result.set_item(key, row.getattr(name)?)?;
    }
    Ok(result.into_any().unbind())
}
