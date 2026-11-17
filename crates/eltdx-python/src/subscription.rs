//! Legacy subscription traversal over the already-owned raw push consumer.
//! Python owns except/finally scopes so callback exception context and cleanup
//! ordering remain observable exactly as in the public facade.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn records<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = value.py();
    let empty = PyTuple::empty(py).into_any();
    let rows = py.import("builtins")?.getattr("getattr")?.call1((value, "records", &empty))?;
    if rows.is_truthy()? { Ok(rows) } else { Ok(empty) }
}

fn add<'py>(left: &Bound<'py, PyAny>, right: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    left.py().import("operator")?.getattr("add")?.call1((left, right))
}

#[pyfunction]
pub fn subscription_group_apis(py: Python<'_>, owner: Bound<'_, PyAny>,
    stack: Bound<'_, PyAny>, pin: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let apis = PyList::empty(py);
    for group in owner.getattr("_groups")?.try_iter()? {
        let _ = group?;
        py.check_signals()?;
        let api_type = owner.getattr("_api")?.get_type();
        let transport = stack.call_method1("enter_context", (pin.call0()?,))?;
        apis.append(api_type.call1((transport,))?)?;
    }
    Ok(apis.into_any().unbind())
}

#[pyfunction]
pub fn subscription_enable_native(apis: Bound<'_, PyAny>, proxy_type: Bound<'_, PyAny>) -> PyResult<()> {
    for api in apis.try_iter()? {
        let api = api?;
        api.setattr("_native_services", api.getattr("_transport")?.get_type().as_any().is(&proxy_type))?;
    }
    Ok(())
}

#[pyfunction]
pub fn subscription_activation_pages(py: Python<'_>, owner: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let pages = PyList::empty(py);
    for index in 0..owner.getattr("_groups")?.len()? {
        py.check_signals()?;
        pages.append(owner.call_method1("_refresh_group", (index,))?)?;
    }
    Ok(pages.into_any().unbind())
}

#[pyfunction]
pub fn subscription_combine(py: Python<'_>, owner: Bound<'_, PyAny>,
    pages: Bound<'_, PyAny>, page_type: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if pages.len()? == 1 { return Ok(pages.get_item(0)?.unbind()); }
    let codes = owner.getattr("_codes")?;
    let combined = PyList::empty(py);
    for page in pages.try_iter()? {
        // Unlike dispatch, activation requires .records even for custom pages.
        for row in page?.getattr("records")?.try_iter()? { combined.append(row?)?; }
    }
    Ok(page_type.call1((codes, PyTuple::new(py, combined.iter())?, PyBytes::new(py, b"")))?.unbind())
}

#[pyfunction]
pub fn subscription_remember(owner: Bound<'_, PyAny>, rows: Bound<'_, PyAny>,
    record_type: Bound<'_, PyAny>) -> PyResult<()> {
    for row in rows.try_iter()? {
        let row = row?;
        if row.is_instance(&record_type)? && owner.getattr("_cursors")?.contains(row.getattr("full_code")?)? {
            // A second property read is intentional. Time-of-day cursors may
            // decrease on the next session and must never be maximized.
            let code = row.getattr("full_code")?;
            let cursor = row.getattr("update_time_raw")?;
            owner.getattr("_cursors")?.set_item(code, cursor)?;
        }
    }
    Ok(())
}

#[pyfunction]
pub fn subscription_refresh(py: Python<'_>, owner: Bound<'_, PyAny>, index: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let api = owner.getattr("_group_apis")?.get_item(&index)?;
    let codes_list = py.import("builtins")?.getattr("list")?.call1((&codes,))?;
    let cursors = PyDict::new(py);
    for code in codes.try_iter()? {
        let code = code?;
        cursors.set_item(&code, owner.getattr("_cursors")?.get_item(&code)?)?;
    }
    let kwargs = PyDict::new(py); kwargs.set_item("cursors", cursors)?;
    let page = api.call_method("refresh", (codes_list,), Some(&kwargs))?;
    owner.call_method1("_remember", (records(&page)?,))?;
    Ok(page.unbind())
}

#[pyfunction]
pub fn subscription_schedule(py: Python<'_>, owner: Bound<'_, PyAny>, index: Bound<'_, PyAny>, clock: Bound<'_, PyAny>) -> PyResult<()> {
    let interval = owner.getattr("_reactivate_interval")?;
    let due = if interval.is_none() { py.None().into_bound(py) }
        else { add(&clock.call_method0("monotonic")?, &interval)? };
    owner.getattr("_next_renew")?.set_item(index, due)
}

#[pyfunction]
pub fn subscription_dispatch(py: Python<'_>, owner: Bound<'_, PyAny>, rows: Bound<'_, PyAny>,
    record_type: Bound<'_, PyAny>) -> PyResult<usize> {
    owner.call_method1("_remember", (&rows,))?;
    let mut count = 0;
    for row in rows.try_iter()? {
        py.check_signals()?;
        let row = row?;
        if !row.is_instance(&record_type)? { continue; }
        if owner.getattr("_filter_codes")?.is_truthy()?
            && !owner.getattr("_code_set")?.contains(row.getattr("full_code")?)? { continue; }
        let total = add(&owner.getattr("_dispatched")?, &1_i32.into_pyobject(py)?.into_any())?;
        owner.setattr("_dispatched", total)?;
        owner.call_method1("_invoke_callback", (row,))?;
        count += 1;
    }
    Ok(count)
}

#[pyfunction]
pub fn subscription_run_once(py: Python<'_>, owner: Bound<'_, PyAny>, timeout: Bound<'_, PyAny>,
    clock: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let zero = 0_i32.into_pyobject(py)?.into_any();
    if owner.getattr("_stopped")?.is_truthy()? { return Ok(zero.unbind()); }
    let mut effective = if timeout.is_none() { owner.getattr("_timeout")? } else { timeout };
    if !effective.is_none() && effective.lt(0)? {
        return Err(pyo3::exceptions::PyValueError::new_err("timeout 必须 >= 0"));
    }
    let mut dispatched = zero;
    for (index, due) in owner.getattr("_next_renew")?.try_iter()?.enumerate() {
        let due = due?;
        if owner.getattr("_stopped")?.is_truthy()? { return Ok(dispatched.unbind()); }
        if !due.is_none() && clock.call_method0("monotonic")?.ge(due)? {
            // Exception policy lives in a Python except scope so sys.exception
            // inside the user's on_error sees the original exception.
            let outcome = owner.call_method1("_renew_once", (index,))?;
            if outcome.get_item(0)?.is_truthy()? {
                let count = owner.call_method1("_dispatch", (records(&outcome.get_item(1)?)?,))?;
                dispatched = add(&dispatched, &count)?;
            }
        }
    }
    if owner.getattr("_stopped")?.is_truthy()? { return Ok(dispatched.unbind()); }
    let due_times = PyList::empty(py);
    for due in owner.getattr("_next_renew")?.try_iter()? {
        let due = due?;
        if !due.is_none() { due_times.append(due)?; }
    }
    let builtins = py.import("builtins")?;
    if !due_times.is_empty() {
        let due = builtins.getattr("min")?.call1((due_times,))?;
        let delta = py.import("operator")?.getattr("sub")?.call1((due, clock.call_method0("monotonic")?))?;
        let until = builtins.getattr("max")?.call1((0.0, delta))?;
        effective = if effective.is_none() { until }
            else { builtins.getattr("min")?.call1((effective, until))? };
    }
    let outcome = owner.call_method1("_poll_once", (effective,))?;
    if !outcome.get_item(0)?.is_truthy()? { return Ok(dispatched.unbind()); }
    let total = add(&owner.getattr("_frames")?, &1_i32.into_pyobject(py)?.into_any())?;
    owner.setattr("_frames", total)?;
    let frame = outcome.get_item(1)?;
    if frame.is_none() { return Ok(dispatched.unbind()); }
    let count = owner.call_method1("_dispatch", (records(&frame)?,))?;
    Ok(add(&dispatched, &count)?.unbind())
}

#[pyfunction]
pub fn subscription_run(py: Python<'_>, owner: Bound<'_, PyAny>, duration: Bound<'_, PyAny>, clock: Bound<'_, PyAny>) -> PyResult<()> {
    let page = owner.call_method0("activate")?;
    owner.call_method1("_dispatch", (records(&page)?,))?;
    let deadline = if duration.is_none() { py.None().into_bound(py) }
        else { add(&clock.call_method0("monotonic")?, &duration)? };
    let builtins = py.import("builtins")?;
    while !owner.getattr("_stopped")?.is_truthy()? {
        py.check_signals()?;
        let mut timeout = owner.getattr("_timeout")?;
        if !deadline.is_none() {
            let delta = py.import("operator")?.getattr("sub")?.call1((&deadline, clock.call_method0("monotonic")?))?;
            let remaining = builtins.getattr("max")?.call1((0.0, delta))?;
            timeout = if timeout.is_none() { remaining }
                else { builtins.getattr("min")?.call1((timeout, remaining))? };
        }
        let kwargs = PyDict::new(py); kwargs.set_item("timeout", timeout)?;
        owner.call_method("run_once", (), Some(&kwargs))?;
        if !deadline.is_none() && clock.call_method0("monotonic")?.ge(&deadline)? { break; }
    }
    Ok(())
}
