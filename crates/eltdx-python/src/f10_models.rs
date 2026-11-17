//! Existing Python dataclasses are public DTOs. Their derived business views
//! use shared Rust traversal and retain dynamic Python object operations.
use eltdx::f10::collection::{self, visit_classified, visit_promotion};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};
use crate::f10_request::builtin;

#[pyfunction]
pub fn f10_infer_market<'py>(py: Python<'py>, code: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let text = builtin(&module, "str")?.call1((if code.is_truthy()? { code } else { "".into_pyobject(py)?.into_any() },))?.call_method0("strip")?;
    if !text.is_truthy()? { return Ok(py.None()); }
    let result = (|| module.getattr("normalize_code")?.call1((text,))?
        .get_item(pyo3::types::PySlice::new(py, 0, 2, 1)))();
    match result {
        Ok(value) => Ok(value.unbind()),
        Err(error) => {
            if builtin(&module, "isinstance")?.call1((error.value(py), builtin(&module, "Exception")?))?.is_truthy()? { Ok(py.None()) }
            else { Err(error) }
        }
    }
}

#[pyfunction]
pub fn f10_model_identity<'py>(py: Python<'py>, model: Bound<'py, PyAny>, policy: &str,
    module: Bound<'py, PyAny>) -> PyResult<()> {
    let mut market = py.None().into_bound(py);
    if policy == "tracking" {
        let isinstance = builtin(&module, "isinstance")?;
        if isinstance.call1((model.getattr("market_id")?, builtin(&module, "int")?))?.is_truthy()?
            && module.getattr("ID_TO_MARKET")?.contains(model.getattr("market_id")?)? {
            market = module.getattr("ID_TO_MARKET")?.get_item(model.getattr("market_id")?)?;
        }
    } else if !model.getattr("market_id")?.is_none() {
        market = module.getattr("ID_TO_MARKET")?.call_method1("get", (model.getattr("market_id")?,))?;
    }
    if policy != "ladder" && market.is_none() { market = module.getattr("_infer_market")?.call1((model.getattr("code")?,))?; }
    builtin(&module, "object")?.call_method1("__setattr__", (&model, "market", &market))?;
    let setter = builtin(&module, "object")?.getattr("__setattr__")?;
    let full = if market.is_truthy()? && model.getattr("code")?.is_truthy()? {
        "{}{}".into_pyobject(py)?.call_method1("format", (market, model.getattr("code")?))?
    } else { model.getattr("code")? };
    setter.call1((model, "full_code", full))?;
    Ok(())
}

#[pyclass]
struct ModelValues { iterator: Py<PyIterator>, mode: String }

#[pymethods]
impl ModelValues {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let result: PyResult<Option<Py<PyAny>>> = (|| {
            let mut iterator = self.iterator.bind(py).clone();
            loop {
                let Some(row) = iterator.next() else { return Ok(None); };
                let row = row?;
                py.check_signals()?;
                if self.mode == "etfs" {
                    let code = row.getattr("code")?;
                    if code.is_truthy()? && code.call_method1("startswith", (PyTuple::new(py, collection::ETF_PREFIXES.iter().copied())?,))?.is_truthy()? {
                        return Ok(Some(row.unbind()));
                    }
                } else if row.getattr(&self.mode)?.is_truthy()? {
                    return Ok(Some(row.getattr(&self.mode)?.unbind()));
                }
            }
        })();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error)); Err(outer)
            }
            result => result,
        }
    }
}

#[pyfunction]
pub fn f10_model_sequence<'py>(py: Python<'py>, model: Bound<'py, PyAny>, attribute: &str,
    unique: bool, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    if unique {
        let seen = PyDict::new(py);
        for row in model.getattr("rows")?.try_iter()? {
            py.check_signals()?;
            let row = row?;
            if row.getattr(attribute)?.is_truthy()? && !seen.contains(row.getattr(attribute)?)? {
                seen.set_item(row.getattr(attribute)?, py.None())?;
            }
        }
        return Ok(builtin(&module, "tuple")?.call1((seen,))?.unbind());
    }
    let tuple = builtin(&module, "tuple")?;
    let iterator = model.getattr("rows")?.try_iter()?.unbind();
    let values = Py::new(py, ModelValues { iterator, mode: attribute.to_owned() })?;
    Ok(tuple.call1((values,))?.unbind())
}

#[pyfunction]
pub fn f10_board_record_key<'py>(py: Python<'py>, model: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let values: PyResult<Vec<_>> = collection::BOARD_RECORD_ATTRIBUTES.iter().map(|&name| model.getattr(name)).collect();
    Ok(PyTuple::new(py, values?)?.into_any().unbind())
}

fn ordered_counts<'py>(py: Python<'py>, counts: &Bound<'py, PyAny>, module: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let dict = builtin(module, "dict")?;
    let sorted = builtin(module, "sorted")?;
    let kwargs = PyDict::new(py); kwargs.set_item("reverse", true)?;
    dict.call1((sorted.call((counts.call_method0("items")?,), Some(&kwargs))?,))
}

#[pyfunction]
pub fn f10_ladder_counts<'py>(py: Python<'py>, model: Bound<'py, PyAny>, by_date: bool,
    module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    visit_classified(model.getattr("rows")?.try_iter()?, |row| {
        py.check_signals()?;
        let other = if by_date { row.getattr("status")?.ne("limit_up")? } else { !row.getattr("status")?.eq("limit_up")? };
        if other || row.getattr("board_level")?.is_none() { return Ok::<_, PyErr>(None); }
        let day = if by_date {
            let text = builtin(&module, "str")?;
            let primary = row.getattr("trading_date_value")?;
            let day = if primary.is_truthy()? { primary } else {
                let fallback = row.getattr("trading_date")?;
                if fallback.is_truthy()? { fallback } else { "".into_pyobject(py)?.into_any() }
            };
            Some(text.call1((day,))?)
        } else { None };
        Ok(Some((row, day)))
    }, |(row, day)| {
        let counts = match day {
            Some(day) => result.call_method1("setdefault", (day, PyDict::new(py)))?,
            None => result.clone().into_any(),
        };
        // Assignment RHS is evaluated before the subscript target expression.
        let value = counts.call_method1("get", (row.getattr("board_level")?, 0))?;
        let value = py.import("operator")?.getattr("add")?.call1((value, 1))?;
        counts.set_item(row.getattr("board_level")?, value)
    })?;
    if !by_date { return Ok(ordered_counts(py, result.as_any(), &module)?.unbind()); }
    let sorted = PyDict::new(py);
    for pair in result.call_method0("items")?.try_iter()? {
        let pair = crate::offline_financial::unpack(pair?, 2)?;
        sorted.set_item(&pair[0], ordered_counts(py, &pair[1], &module)?)?;
    }
    Ok(sorted.into_any().unbind())
}

fn rates<'py>(py: Python<'py>, counts: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    visit_promotion(counts.try_iter()?, |level| {
        let get = counts.getattr("get")?;
        let lower = py.import("operator")?.getattr("sub")?.call1((level, 1))?;
        let denominator = get.call1((lower,))?;
        Ok::<_, PyErr>(if denominator.is_truthy()? { Some(()) } else { None })
    }, |level, ()| {
        let numerator = counts.get_item(&level)?;
        let lower = py.import("operator")?.getattr("sub")?.call1((&level, 1))?;
        let denominator = counts.get_item(lower)?;
        result.set_item(level, py.import("operator")?.getattr("truediv")?.call1((numerator, denominator))?)
    })?;
    Ok(result)
}

#[pyfunction]
pub fn f10_ladder_rates<'py>(py: Python<'py>, model: Bound<'py, PyAny>, by_date: bool) -> PyResult<Py<PyAny>> {
    let counts = model.getattr(if by_date { "ladder_counts_by_date" } else { "ladder_counts" })?;
    if !by_date { return Ok(rates(py, counts)?.into_any().unbind()); }
    let result = PyDict::new(py);
    for pair in counts.call_method0("items")?.try_iter()? {
        let pair = crate::offline_financial::unpack(pair?, 2)?;
        result.set_item(&pair[0], rates(py, pair[1].clone())?)?;
    }
    Ok(result.into_any().unbind())
}

#[pyclass]
struct OverviewTopics { row: Py<PyAny>, index: usize }
#[pymethods]
impl OverviewTopics {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let result: PyResult<Option<Py<PyAny>>> = (|| {
        while let Some(&(name, count)) = collection::OVERVIEW_TOPICS.get(self.index) {
            self.index += 1;
            let row = self.row.bind(py);
            let value = row.call_method1("get", (name,))?;
            if PyTuple::new(py, [py.None().into_bound(py), "".into_pyobject(py)?.into_any()])?.contains(&value)? { continue; }
            let topic = PyDict::new(py);
            topic.set_item("name", row.call_method1("get", (name,))?)?;
            topic.set_item("count", row.call_method1("get", (count,))?)?;
            return Ok(Some(topic.into_any().unbind()));
        }
        Ok(None)
        })();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error)); Err(outer)
            }
            result => result,
        }
    }
}

#[pyfunction]
pub fn f10_market_overview<'py>(py: Python<'py>, model: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    for row in model.getattr("summary")?.try_iter()? {
        py.check_signals()?;
        let row = row?;
        let item = PyDict::new(py);
        let date = row.call_method1("get", ("N001",))?;
        item.set_item("trade_date", if date.is_truthy()? { date } else { row.call_method1("get", ("t001",))? })?;
        for &(name, column) in collection::OVERVIEW_FIELDS { item.set_item(name, row.call_method1("get", (column,))?)?; }
        let tuple = builtin(&module, "tuple")?;
        let topics = Py::new(py, OverviewTopics { row: row.clone().unbind(), index: 0 })?;
        item.set_item("hot_topics", tuple.call1((topics,))?)?;
        item.set_item("raw", builtin(&module, "dict")?.call1((row,))?)?;
        result.append(item)?;
    }
    Ok(builtin(&module, "tuple")?.call1((result,))?.unbind())
}

#[pyfunction]
pub fn f10_raise_amount<'py>(py: Python<'py>, model: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    if model.getattr("raise_amount")?.is_none() { return Ok(py.None()); }
    Ok(py.import("operator")?.getattr("truediv")?.call1((model.getattr("raise_amount")?, 1e8))?.unbind())
}

#[pyfunction]
pub fn f10_ladder_status<'py>(py: Python<'py>, model: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let map = PyDict::new(py); map.set_item(1, "limit_up")?; map.set_item(3, "broken")?; map.set_item(0, "limit_down")?;
    Ok(map.call_method1("get", (model.getattr("limit_type")?, "unknown"))?.unbind())
}
