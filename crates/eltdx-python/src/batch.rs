//! Legacy outer scheduling traversals. The Python executor owns its context,
//! shutdown and user overrides; these functions only submit ordered chunks.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn part<'py>(
    codes: &Bound<'py, PyAny>,
    start: &Bound<'py, PyAny>,
    size: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = codes.py();
    let end = py
        .import("operator")?
        .getattr("add")?
        .call1((start, size))?;
    let slice = py
        .import("builtins")?
        .getattr("slice")?
        .call1((start, end))?;
    codes.get_item(slice)
}

#[pyfunction]
pub fn batch_chunks(
    py: Python<'_>,
    codes: Bound<'_, PyAny>,
    size: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for start in py
        .import("builtins")?
        .getattr("range")?
        .call1((0, codes.len()?, &size))?
        .try_iter()?
    {
        result.append(part(&codes, &start?, &size)?)?;
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn map_batches(
    py: Python<'_>,
    executor: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    query: Bound<'_, PyAny>,
    size: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    let zip = py.import("builtins")?.getattr("zip")?;
    if size.is_none() {
        let responses = executor.call_method1("map", (&query, &codes))?;
        result.call_method1("update", (zip.call1((&codes, responses))?,))?;
    } else {
        for start in py
            .import("builtins")?
            .getattr("range")?
            .call1((0, codes.len()?, &size))?
            .try_iter()?
        {
            py.check_signals()?;
            let codes = part(&codes, &start?, &size)?;
            let responses = executor.call_method1("map", (&query, &codes))?;
            result.call_method1("update", (zip.call1((codes, responses))?,))?;
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn assemble_capital(
    py: Python<'_>,
    responses: Bound<'_, PyAny>,
    block_type: Bound<'_, PyAny>,
    batch_type: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let blocks = PyList::empty(py);
    let raw = PyList::empty(py);
    for response in responses.try_iter()? {
        let response = response?;
        if response.is_instance(&block_type)? {
            blocks.append(&response)?;
            if response.getattr("raw_payload")?.is_truthy()? {
                raw.append(response.getattr("raw_payload")?)?;
            }
        } else if response.is_instance(&batch_type)? {
            blocks.call_method1("extend", (response.getattr("blocks")?,))?;
            raw.call_method1("extend", (response.getattr("raw_payloads")?,))?;
        } else {
            return Ok(response.unbind());
        }
    }
    Ok(batch_type
        .call1((
            PyTuple::new(py, blocks.iter())?,
            PyTuple::new(py, raw.iter())?,
        ))?
        .unbind())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn map_money_flow(
    py: Python<'_>,
    executor: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    query: Bound<'_, PyAny>,
    size: Bound<'_, PyAny>,
    block_type: Bound<'_, PyAny>,
    batch_type: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let blocks = PyList::empty(py);
    for start in py
        .import("builtins")?
        .getattr("range")?
        .call1((0, codes.len()?, &size))?
        .try_iter()?
    {
        py.check_signals()?;
        let codes = part(&codes, &start?, &size)?;
        for response in executor.call_method1("map", (&query, codes))?.try_iter()? {
            let response = response?;
            if response.is_instance(&block_type)? {
                blocks.append(response)?;
            } else if response.is_instance(&batch_type)? {
                blocks.call_method1("extend", (response.getattr("blocks")?,))?;
            } else {
                return Ok((true, response).into_pyobject(py)?.into_any().unbind());
            }
        }
    }
    // The public Python adapter constructs its batch after executor.__exit__,
    // as before. A custom response returns from inside that context instead.
    Ok((false, blocks).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn adjustment_batch(
    py: Python<'_>,
    changes: Bound<'_, PyAny>,
    anchor: Bound<'_, PyAny>,
    start: Bound<'_, PyAny>,
    builder: Bound<'_, PyAny>,
    block_type: Bound<'_, PyAny>,
    batch_type: Bound<'_, PyAny>,
    output_type: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("anchor_date", anchor)?;
    kwargs.set_item("start_date", start)?;
    if changes.is_instance(&batch_type)? {
        let result = PyList::empty(py);
        for block in changes.getattr("blocks")?.try_iter()? {
            result.append(builder.call((block?,), Some(&kwargs))?)?;
        }
        return Ok(output_type
            .call1((PyTuple::new(py, result.iter())?,))?
            .unbind());
    }
    if !changes.is_instance(&block_type)? {
        return Ok(changes.unbind());
    }
    Ok(builder.call((changes,), Some(&kwargs))?.unbind())
}
