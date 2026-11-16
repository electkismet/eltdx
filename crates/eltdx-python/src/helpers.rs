//! Python objects remain at this boundary; threshold and aggregation policy is
//! shared with the independent Rust SDK without exporting Python types there.
use pyo3::prelude::*;
use pyo3::exceptions::PyException;
use pyo3::types::{PyDict, PyFloat, PyList, PyTuple};

#[pyfunction]
pub fn helper_full_quotes(py: Python<'_>, helper: Bound<'_, PyAny>, codes: Bound<'_, PyList>)
    -> PyResult<Py<PyAny>> {
    let results = PyList::empty(py);
    let indices = (0..codes.len()).collect::<Vec<_>>();
    for chunk in eltdx::__python_compat::full_quote_chunks(&indices) {
        py.check_signals()?;
        let batch = PyList::new(py, chunk.iter().map(|&index| codes.get_item(index)).collect::<PyResult<Vec<_>>>()?)?;
        let page = helper.getattr("_client")?.getattr("quotes")?.call_method1("get_snapshots", (&batch,))?;
        if !page.is_instance_of::<PyList>() && !page.is_instance_of::<PyTuple>() {
            return Ok(page.unbind());
        }
        let snapshots = PyList::new(py, page.try_iter()?.collect::<PyResult<Vec<_>>>()?)?;
        let merged = helper.call_method1("_merge_quote_depths", (snapshots, batch))?;
        results.call_method1("extend", (merged,))?;
    }
    Ok(results.into_any().unbind())
}

#[pyfunction]
pub fn helper_merge_depths(py: Python<'_>, helper: Bound<'_, PyAny>, snapshots: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>, snapshot_type: Bound<'_, PyAny>, refresh_type: Bound<'_, PyAny>,
    replace: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if !snapshots.is_truthy()? { return Ok(snapshots.unbind()); }
    for snapshot in snapshots.try_iter()? {
        if !snapshot?.is_instance(&snapshot_type)? { return Ok(snapshots.unbind()); }
    }
    // Match the original try block exactly: only the optional get_depth call
    // downgrades Exception. Iteration/projection failures still propagate.
    let refresh = match helper.getattr("_client").and_then(|client| client.getattr("quotes"))
        .and_then(|quotes| quotes.call_method1("get_depth", (codes,))) {
        Ok(value) => value,
        Err(error) if error.is_instance_of::<PyException>(py) => return Ok(snapshots.unbind()),
        Err(error) => return Err(error),
    };
    let empty = PyTuple::empty(py).into_any();
    let records = py.import("builtins")?.getattr("getattr")?.call1((&refresh, "records", empty))?;
    let depth_by_code = PyDict::new(py);
    for record in records.try_iter()? {
        py.check_signals()?;
        let record = record?;
        if !record.is_instance(&refresh_type)? { continue; }
        // Retain short-circuit property access on incomplete buy depth.
        let bids = record.getattr("buy_levels")?.len()?;
        if bids < 5 { continue; }
        let asks = record.getattr("sell_levels")?.len()?;
        if eltdx::__python_compat::complete_quote_depth(bids, asks) {
            depth_by_code.set_item(record.getattr("full_code")?, record)?;
        }
    }
    if depth_by_code.is_empty() { return Ok(snapshots.unbind()); }
    let merged = PyList::empty(py);
    for snapshot in snapshots.try_iter()? {
        py.check_signals()?;
        let snapshot = snapshot?;
        match depth_by_code.get_item(snapshot.getattr("full_code")?)? {
            None => merged.append(snapshot)?,
            Some(depth) => {
                let kwargs = PyDict::new(py);
                for name in ["buy_levels", "sell_levels", "open_amount_raw", "open_amount_yuan"] {
                    kwargs.set_item(name, depth.getattr(name)?)?;
                }
                merged.append(replace.call((snapshot,), Some(&kwargs))?)?;
            }
        }
    }
    Ok(merged.into_any().unbind())
}

#[pyfunction]
pub fn helper_fund_flow(py: Python<'_>, ticks: Bound<'_, PyAny>,
    super_threshold: Bound<'_, PyAny>, large_threshold: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let operator = py.import("operator")?;
    let multiply = operator.getattr("mul")?;
    let add = operator.getattr("iadd")?;
    let medium_threshold = PyFloat::new(py, 40_000.0).into_any().unbind();
    let super_threshold = super_threshold.unbind();
    let large_threshold = large_threshold.unbind();
    let mut buckets: [Py<PyAny>; 8] = std::array::from_fn(|_| PyFloat::new(py, 0.0).into_any().unbind());
    for tick in ticks.try_iter()? {
        py.check_signals()?;
        let tick = tick?;
        if !tick.getattr("is_actual_trade")?.is_truthy()? { continue; }
        // Keep the original conditional's two independent side accesses.
        let direction = if tick.getattr("side")?.eq("buy")? { 0 }
            else if tick.getattr("side")?.eq("sell")? { 4 }
            else { continue; };
        let product = multiply.call1((tick.getattr("price")?, tick.getattr("volume")?))?;
        let amount = multiply.call1((product, 100.0))?.unbind();
        eltdx::__python_compat::accumulate_flow_amount(&mut buckets, direction, amount,
            [&super_threshold, &large_threshold, &medium_threshold],
            |left, right| left.bind(py).gt(right.bind(py)),
            |left, right| Ok(add.call1((left.bind(py), right.bind(py)))?.unbind()))?;
    }
    Ok(PyTuple::new(py, buckets)?.into_any().unbind())
}
