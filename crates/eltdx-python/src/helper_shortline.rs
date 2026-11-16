//! Legacy shortline date strings and values stay at the Python boundary.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PyTuple};

fn raised(kind: &Bound<'_, PyAny>, message: &str) -> PyErr {
    match kind.call1((message,)) { Ok(value) => PyErr::from_value(value), Err(error) => error }
}

fn pair<'py>(value: Bound<'py, PyAny>) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let tuple = value.py().import("builtins")?.getattr("tuple")?.call1((value,))?;
    let count = tuple.len()?;
    if count < 2 { return Err(pyo3::exceptions::PyValueError::new_err(format!("not enough values to unpack (expected 2, got {count})"))); }
    if count > 2 { return Err(pyo3::exceptions::PyValueError::new_err("too many values to unpack (expected 2)")); }
    Ok((tuple.get_item(0)?, tuple.get_item(1)?))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn helper_validate_stats(py: Python<'_>, resource: Bound<'_, PyAny>, target: Bound<'_, PyAny>,
    previous: Bound<'_, PyAny>, dominant: Bound<'_, PyAny>, parse: Bound<'_, PyAny>,
    minimum: Bound<'_, PyAny>, format_error: Bound<'_, PyAny>, date_error: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (date, coverage) = pair(dominant.call1((resource.getattr("stat")?.call_method0("values")?,))?)?;
    let (date2, coverage2) = pair(dominant.call1((resource.getattr("stat2")?.call_method0("values")?,))?)?;
    if date.is_none() || date2.is_none() {
        return Err(raised(&format_error, "TDX statistics resource has no dominant date in tdxstat.cfg or tdxstat2.cfg"));
    }
    if !date.eq(&date2)? {
        return Err(raised(&format_error, &format!("TDX statistics resource dates disagree: tdxstat.cfg={}, tdxstat2.cfg={}",
            date.str()?.to_str()?, date2.str()?.to_str()?)));
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
pub fn helper_stats_resource(py: Python<'_>, service: Bound<'_, PyAny>, path: Bound<'_, PyAny>,
    refresh: Bound<'_, PyAny>, target: Bound<'_, PyAny>, previous: Bound<'_, PyAny>,
    usable: Bound<'_, PyAny>, validate: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let cache = service.getattr("_stats_cache")?;
    let cached = cache.call_method1("get", (&path,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("target", target)?; kwargs.set_item("previous", previous)?;
    if !refresh.is_truthy()? && !cached.is_none() && usable.call((&cached,), Some(&kwargs))?.is_truthy()? {
        return Ok((cached, false).into_pyobject(py)?.into_any().unbind());
    }
    let resource = service.getattr("_client")?.getattr("resources")?.call_method1("read_stats", (&path,))?;
    validate.call((&resource,), Some(&kwargs))?;
    cache.set_item(path, &resource)?;
    Ok((resource, true).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn helper_dominant_date(py: Python<'_>, rows: Bound<'_, PyAny>) -> PyResult<(Option<String>, f64)> {
    // Materialize before property evaluation, matching the original list(rows).
    let list_type = py.import("builtins")?.getattr("list")?;
    let rows = list_type.call1((rows,))?;
    let dates = rows.try_iter()?.map(|row| {
        let row = row?;
        if row.getattr("stats_date")?.is_truthy()? {
            Ok(Some(row.getattr("stats_date")?.str()?.to_str()?.to_owned()))
        } else { Ok(None) }
    });
    let (date, count, total) = eltdx::__python_compat::dominant_date(dates, || py.check_signals(),
        || pyo3::exceptions::PyOverflowError::new_err("statistics row count overflow"))?;
    Ok((date, if count == 0 { 0.0 } else { count as f64 / total.max(1) as f64 }))
}

#[pyfunction]
pub fn helper_aligned_stat2(py: Python<'_>, row: Bound<'_, PyAny>, target: Bound<'_, PyAny>,
    previous: Bound<'_, PyAny>, empty: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if row.is_none() || !row.getattr("stats_date")?.is_truthy()? {
        return Ok(empty.call1(("stats_row_missing",))?.unbind());
    }
    let target = target.call_method1("strftime", ("%Y%m%d",))?;
    let previous = previous.call_method1("strftime", ("%Y%m%d",))?;
    let same = row.getattr("stats_date")?.eq(&target)?;
    let prior = if same { false } else { row.getattr("stats_date")?.eq(&previous)? };
    if !same && !prior { return Ok(empty.call1(("stats_date_unaligned",))?.unbind()); }
    let result = PyDict::new(py);
    result.set_item("status", if prior { "previous_trading_day" } else { "same_day" })?;
    let keys = ["prev_amount_10k", "prev_seal_amount_10k", "prev2_seal_amount_10k", "prev_open_volume_hand", "prev_open_amount_10k"];
    for (key, field) in keys.into_iter().zip(eltdx::__python_compat::stat2_columns(prior)) {
        use eltdx::__python_compat::Stat2Column::*;
        let name = match field { Amount => "amount_10k", Seal => "seal_amount_10k", PreviousAmount => "prev_amount_10k",
            PreviousSeal => "prev_seal_amount_10k", Previous2Seal => "prev2_seal_amount_10k", OpenVolume => "open_volume_hand",
            OpenAmount => "open_amount_10k", PreviousOpenVolume => "prev_open_volume_hand", PreviousOpenAmount => "prev_open_amount_10k" };
        result.set_item(key, row.getattr(name)?)?;
    }
    Ok(result.into_any().unbind())
}
