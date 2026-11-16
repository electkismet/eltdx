//! Legacy helper cache dispatch keeps the public Python F10 overrides and
//! per-request timeouts. The called F10 entries already use shared Rust code.
use pyo3::prelude::*;
use pyo3::types::PyDict;

// Python LOAD_GLOBAL resolves the supplied module dictionary before builtins.
// Actual helper callbacks deliberately keep their stricter module lookup.
fn builtin<'py>(scope: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    if let Some(value) = scope.get_item(name)? { return Ok(value); }
    if let Some(builtins) = scope.get_item("__builtins__")? {
        if let Ok(values) = builtins.downcast::<PyDict>() {
            return values.get_item(name)?.ok_or_else(|| pyo3::exceptions::PyNameError::new_err(name.to_owned()));
        }
        return builtins.getattr(name);
    }
    scope.py().import("builtins")?.getattr(name)
}

#[pyfunction]
pub fn helper_f10_cached<'py>(py: Python<'py>, helper: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, refresh: Bound<'py, PyAny>, scope: Bound<'py, PyDict>) -> PyResult<Py<PyAny>> {
    if matches!(method, "index_members" | "index_etfs") {
        let cached = helper.getattr("_index_cached")?;
        let getter = helper.getattr("_client")?.getattr("f10")?
            .getattr(if method == "index_members" { "index_members" } else { "index_tracking_products" })?;
        return Ok(cached.call1((if method == "index_members" { "members" } else { "etfs" }, code, refresh, getter))?.unbind());
    }
    let key = if method == "ipo_calendar" { "".into_pyobject(py)?.into_any() }
        else { builtin(&scope, "str")?.call1((&code,))?.call_method0("strip")?.call_method0("lower")? };
    let kind = if method == "ipo_calendar" { "ipo" } else { "board_members" };
    if !refresh.is_truthy()? {
        let cached = helper.getattr("_index_cache")?.call_method1("get", ((kind, &key),))?;
        if !cached.is_none() { return Ok(cached.unbind()); }
    }
    let service = helper.getattr("_client")?.getattr("f10")?;
    let result = if method == "ipo_calendar" { service.call_method0("ipo_calendar")? }
        else { service.call_method1("board_members", (code,))? };
    helper.getattr("_index_cache")?.set_item((kind, key), &result)?;
    Ok(result.unbind())
}

#[pyfunction]
pub fn helper_index_cached<'py>(helper: Bound<'py, PyAny>, kind: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>, refresh: Bound<'py, PyAny>, getter: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>) -> PyResult<Py<PyAny>> {
    let key = builtin(&scope, "str")?.call1((&code,))?.call_method0("strip")?.call_method0("lower")?;
    if !refresh.is_truthy()? {
        let cached = helper.getattr("_index_cache")?.call_method1("get", ((&kind, &key),))?;
        if !cached.is_none() { return Ok(cached.unbind()); }
    }
    let result = getter.call1((code,))?;
    helper.getattr("_index_cache")?.set_item((kind, key), &result)?;
    Ok(result.unbind())
}
