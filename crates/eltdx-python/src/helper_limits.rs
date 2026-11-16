//! Legacy shortline price rules intentionally differ from the strict SDK's
//! evidence-based policy registry. Shared state decisions require known limits.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use crate::helper_numeric::callback;

fn get<'py>(value: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value.py().import("builtins")?.getattr("getattr")?.call1((value, name, value.py().None()))
}

#[pyfunction]
pub fn shortline_limit_ratio(py: Python<'_>, code: Bound<'_, PyAny>, name: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let name = if name.is_truthy()? { name } else { "".into_pyobject(py)?.into_any() };
    let name = name.str()?.call_method0("strip")?.call_method0("upper")?;
    if name.call_method1("startswith", (("N", "C"),))?.is_truthy()? { return Ok(py.None()); }
    if name.call_method1("startswith", (("ST", "*ST", "SST", "S*ST"),))?.is_truthy()? {
        return Ok(5.0.into_pyobject(py)?.into_any().unbind());
    }
    let symbol = code.get_item(pyo3::types::PySlice::new(py, 2, isize::MAX, 1))?;
    let value = if code.call_method1("startswith", ("bj",))?.is_truthy()? { 30.0 }
        else if code.call_method1("startswith", ("sh688",))?.is_truthy()?
            || (code.call_method1("startswith", ("sz",))?.is_truthy()?
                && symbol.call_method1("startswith", ("30",))?.is_truthy()?) { 20.0 }
        else { 10.0 };
    Ok(value.into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn shortline_price_compare(py: Python<'_>, left: Bound<'_, PyAny>, right: Bound<'_, PyAny>,
    tolerance: Bound<'_, PyAny>, above: bool) -> PyResult<bool> {
    if left.is_none() || right.is_none() { return Ok(false); }
    let builtins = py.import("builtins")?;
    let float = builtins.getattr("float")?;
    let op = py.import("operator")?;
    let left = float.call1((left,))?;
    if above {
        let sum = op.getattr("add")?.call1((left, tolerance))?;
        sum.ge(float.call1((right,))?)
    } else {
        let difference = op.getattr("sub")?.call1((left, float.call1((right,))?))?;
        builtins.getattr("abs")?.call1((difference,))?.le(tolerance)
    }
}

#[pyfunction]
pub fn shortline_limit_status(py: Python<'_>, code: Bound<'_, PyAny>, quote: Bound<'_, PyAny>,
    name: Bound<'_, PyAny>, scope: Bound<'_, PyDict>) -> PyResult<&'static str> {
    if quote.is_none() { return Ok("unknown"); }
    let ratio = callback(&scope, "_price_limit_ratio")?.call1((code, name))?;
    if ratio.is_none() { return Ok("none"); }
    let pre_close = callback(&scope, "_number")?.call1((&quote, "pre_close_price"))?;
    if !pre_close.is_truthy()? { return Ok("none"); }
    let op = py.import("operator")?;
    let ratio = op.getattr("truediv")?.call1((ratio, 100.0))?;
    let multiplier = op.getattr("add")?.call1((1.0, ratio))?;
    let price = op.getattr("mul")?.call1((pre_close, multiplier))?;
    let price = op.getattr("add")?.call1((price, 1e-9))?;
    let builtins = py.import("builtins")?;
    let limit = builtins.getattr("round")?.call1((price, 2))?;
    let last = callback(&scope, "_number")?.call1((&quote, "last_price"))?;
    let high = callback(&scope, "_number")?.call1((&quote, "high_price"))?;
    let levels = get(&quote, "buy_levels")?;
    let levels = if levels.is_truthy()? { builtins.getattr("tuple")?.call1((levels,))? }
        else { PyTuple::empty(py).into_any() };
    let bid = if levels.len()? > 0 { builtins.getattr("float")?.call1((levels.get_item(0)?.getattr("price")?,))? }
        else { py.None().into_bound(py) };
    let locked = callback(&scope, "_locked_amount")?.call1((quote,))?;
    let at_limit = callback(&scope, "_price_close")?.call1((&last, &limit))?.is_truthy()?;
    let result = eltdx::__python_compat::classify_limit_state(at_limit,
        || Ok::<_, PyErr>(callback(&scope, "_price_close")?.call1((bid, &limit))?.is_truthy()?
            || (!locked.is_none() && locked.gt(0)?)),
        || Ok(callback(&scope, "_price_at_or_above")?.call1((high, &limit))?.is_truthy()?
            || callback(&scope, "_price_at_or_above")?.call1((last, &limit))?.is_truthy()?))?;
    Ok(match result { eltdx::LimitStatus::Sealed => "sealed", eltdx::LimitStatus::Touched => "touched", _ => "none" })
}

#[pyfunction]
pub fn shortline_board_text(py: Python<'_>, days: Bound<'_, PyAny>, count: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if days.is_none() || count.is_none() { return Ok(py.None()); }
    let int = py.import("builtins")?.getattr("int")?;
    if int.call1((&days,))?.le(0)? || int.call1((&count,))?.le(0)? { return Ok(py.None()); }
    let days = int.call1((days,))?.str()?.extract::<String>()?;
    let count = int.call1((count,))?.str()?.extract::<String>()?;
    Ok(format!("{days}天{count}板").into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn shortline_ladder_level(py: Python<'_>, stat: Bound<'_, PyAny>, stats_date: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>, previous: Bound<'_, PyAny>, status: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if !status.eq("sealed")? || stat.is_none() { return Ok(py.None()); }
    let prior = stat.getattr("limit_up_streak_days")?;
    let prior = if prior.is_truthy()? { prior } else { 0.into_pyobject(py)?.into_any() };
    let prior = py.import("builtins")?.getattr("int")?.call1((prior,))?.unbind();
    let same = stats_date.eq(target)?;
    let previous = if same { false } else { stats_date.eq(previous)? };
    let result = eltdx::__python_compat::ladder_level_with(prior, same, previous,
        |value| Ok::<_, PyErr>(py.import("operator")?.getattr("add")?.call1((value.bind(py), 1))?.unbind()),
        |value| Ok(py.import("builtins")?.getattr("max")?.call1((1, value.bind(py)))?.unbind()))?;
    Ok(result.unwrap_or_else(|| py.None()))
}

#[pyfunction]
fn ladder_key(py: Python<'_>, row: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let level = row.getattr("ladder_level")?;
    let level = if level.is_truthy()? { level } else { 0.into_pyobject(py)?.into_any() };
    let seal = row.getattr("seal_amount")?;
    let seal = if seal.is_truthy()? { seal } else { 0.0.into_pyobject(py)?.into_any() };
    let negate = py.import("operator")?.getattr("neg")?;
    Ok((negate.call1((level,))?, negate.call1((seal,))?, row.getattr("full_code")?)
        .into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn helper_ladder(py: Python<'_>, table: Bound<'_, PyAny>, include_touched: Bound<'_, PyAny>, count: Bound<'_, PyAny>)
    -> PyResult<Py<PyAny>> {
    let rows = PyList::empty(py);
    for row in table.getattr("rows")?.try_iter()? {
        py.check_signals()?;
        let row = row?;
        let sealed = row.getattr("limit_status")?.eq("sealed")?;
        let include = if sealed { true } else { include_touched.is_truthy()? };
        let touched = if !sealed && include { row.getattr("limit_status")?.eq("touched")? } else { false };
        if eltdx::__python_compat::include_ladder_row(sealed, touched, include) { rows.append(row)?; }
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", pyo3::wrap_pyfunction!(ladder_key, py)?)?;
    rows.call_method("sort", (), Some(&kwargs))?;
    let rows = if count.is_none() { rows.into_any() } else {
        if count.le(0)? { return Err(pyo3::exceptions::PyValueError::new_err("count must be positive or None")); }
        let slice = py.import("builtins")?.getattr("slice")?.call1((py.None(), count))?;
        rows.get_item(slice)?
    };
    Ok(py.import("builtins")?.getattr("tuple")?.call1((rows,))?.unbind())
}
