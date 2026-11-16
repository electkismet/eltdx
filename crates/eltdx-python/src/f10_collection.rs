//! Public Python collection models with original per-page request policy.
use eltdx::f10::collection::{collect_pages, collection_route, row_fields, FieldConversion};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PySet, PyTuple};
use crate::f10_request::builtin;

fn project<'py>(py: Python<'py>, method: &str, row: Bound<'py, PyAny>,
    constructor: &str, module: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>>
{
    let constructor = module.getattr(constructor)?;
    let kwargs = PyDict::new(py);
    for field in row_fields(method) {
        let value = match field.conversion {
            FieldConversion::Raw => row.call_method1("get", (field.column,))?,
            conversion => {
                // The live converter is resolved before row.get, as in the
                // original keyword expression _text(row.get(...)).
                let converter = module.getattr(match conversion {
                    FieldConversion::Text => "_text", FieldConversion::Integer => "_int",
                    FieldConversion::Number => "_float", FieldConversion::MarketFlag => "_market_flag",
                    FieldConversion::Raw => unreachable!(),
                })?;
                converter.call1((row.call_method1("get", (field.column,))?,))?
            }
        };
        kwargs.set_item(field.attribute, value)?;
    }
    kwargs.set_item("raw", builtin(module, "dict")?.call1((row,))?)?;
    constructor.call((), Some(&kwargs))
}

#[pyfunction]
pub fn f10_collect<'py>(py: Python<'py>, client: Bound<'py, PyAny>, method: &str,
    code: Bound<'py, PyAny>, all_pages: Bound<'py, PyAny>, page_size: Bound<'py, PyAny>,
    max_pages: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let (section, constructor, result_type) = collection_route(method)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("unknown F10 collection"))?;
    let rows = PyList::empty(py);
    if method == "ipo_calendar" {
        let kwargs = PyDict::new(py); kwargs.set_item("section", section)?;
        let response = client.call_method("market_report", (), Some(&kwargs))?;
        let table = response.getattr("first_table")?;
        if !table.is_none() {
            for row in table.getattr("rows")?.try_iter()? {
                py.check_signals()?;
                rows.append(project(py, method, row?, constructor, &module)?)?;
            }
        }
        let result = module.getattr(result_type)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("rows", builtin(&module, "tuple")?.call1((rows,))?)?;
        return Ok(result.call((), Some(&kwargs))?.unbind());
    }

    // Preserve max(1, min(int(page_size), live_limit)) evaluation order.
    let max = builtin(&module, "max")?;
    let min = builtin(&module, "min")?;
    let converted = builtin(&module, "int")?.call1((page_size,))?;
    let size = max.call1((1, min.call1((converted, module.getattr("_MAX_PAGE_SIZE")?))?))?;
    let seen = PySet::empty(py)?;
    let mut total = py.None().into_bound(py);
    collect_pages(1_i64.into_pyobject(py)?.into_any(), |page| -> PyResult<Option<()>> {
        py.check_signals()?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("page", page)?; kwargs.set_item("page_size", &size)?;
        let response = client.call_method("market_report", (&code, section), Some(&kwargs))?;
        let table = response.getattr("first_table")?;
        let batch = if table.is_none() { PyTuple::empty(py).into_any() } else { table.getattr("rows")? };
        if total.is_none() { total = module.getattr("_result_total")?.call1((response,))?; }
        let mut added = 0_usize;
        for row in batch.try_iter()? {
            py.check_signals()?;
            let item = project(py, method, row?, constructor, &module)?;
            if method == "board_members" {
                let key = item.getattr("record_key")?;
                if seen.contains(&key)? { continue; }
                seen.add(key)?;
            }
            rows.append(item)?;
            added += 1;
        }
        // Keep short-circuit order and live len calls, including the board
        // batch truth test and the original soft max_pages stopping policy.
        if !all_pages.is_truthy()? { return Ok(Some(())); }
        if method == "board_members" && !batch.is_truthy()? { return Ok(Some(())); }
        if builtin(&module, "len")?.call1((&batch,))?.lt(&size)? || page.ge(&max_pages)? {
            return Ok(Some(()));
        }
        if !total.is_none() && builtin(&module, "len")?.call1((&rows,))?.ge(&total)? { return Ok(Some(())); }
        if method == "board_members" && added == 0 { return Ok(Some(())); }
        Ok(None)
    }, |page| py.import("operator")?.getattr("iadd")?.call1((page, 1)))?;
    let result = module.getattr(result_type)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item(if method == "board_members" { "board_code" } else { "index_code" },
        module.getattr("_code6")?.call1((code,))?)?;
    kwargs.set_item("total", total)?;
    kwargs.set_item("rows", builtin(&module, "tuple")?.call1((rows,))?)?;
    Ok(result.call((), Some(&kwargs))?.unbind())
}

fn ladder_body<'py>(py: Python<'py>, selector: &str, dates: &[Bound<'py, PyAny>]) -> PyResult<Bound<'py, PyDict>> {
    let values = eltdx::f10::collection::ladder_parameters(selector.into_pyobject(py)?.into_any(),
        dates[0].clone(), dates[1].clone());
    let body = PyDict::new(py);
    body.set_item("Params", PyList::new(py, values)?)?;
    Ok(body)
}

#[pyclass]
struct LadderRows {
    iterator: Py<PyIterator>,
    module: Py<PyAny>,
}

#[pymethods]
impl LadderRows {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        py.check_signals()?;
        let mut iterator = self.iterator.bind(py).clone();
        let Some(row) = iterator.next() else { return Ok(None); };
        match self.module.bind(py).getattr("_parse_limit_board_ladder_row")?.call1((row?,)) {
            Ok(value) => Ok(Some(value.unbind())),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error));
                Err(outer)
            }
            Err(error) => Err(error),
        }
    }
}

#[pyfunction]
pub fn f10_ladder<'py>(py: Python<'py>, client: Bound<'py, PyAny>, start: Bound<'py, PyAny>,
    end: Bound<'py, PyAny>, include_summary: Bound<'py, PyAny>, module: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let dates = crate::offline_financial::unpack(module.getattr("_normalize_limit_board_dates")?.call1((start, end))?, 2)?;
    let body = ladder_body(py, "1", &dates)?;
    let response = client.call_method1("_post", (module.getattr("LIMIT_BOARD_LADDER_ENTRY")?, &body))?;
    let summary = if include_summary.is_truthy()? {
        let post = client.getattr("_post")?;
        let entry = module.getattr("LIMIT_BOARD_LADDER_ENTRY")?;
        post.call1((entry, ladder_body(py, "2", &dates)?))?.getattr("rows")?
    } else { PyTuple::empty(py).into_any() };
    let tuple = builtin(&module, "tuple")?;
    // A replaced tuple callable may consume no items or only some items. Keep
    // the original generator's lazy row parsing and exception behavior.
    let values = Py::new(py, LadderRows { iterator: response.getattr("rows")?.try_iter()?.unbind(),
        module: module.clone().unbind() })?;
    let rows = tuple.call1((values,))?;
    let constructor = module.getattr("LimitBoardLadder")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("entry", response.getattr("entry")?)?;
    kwargs.set_item("request_body", body)?;
    kwargs.set_item("error_code", response.getattr("error_code")?)?;
    kwargs.set_item("start_date", &dates[0])?;
    kwargs.set_item("end_date", &dates[1])?;
    kwargs.set_item("rows", rows)?;
    kwargs.set_item("summary", summary)?;
    kwargs.set_item("raw", response.getattr("raw")?)?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn f10_ladder_alias<'py>(py: Python<'py>, client: Bound<'py, PyAny>, start: Bound<'py, PyAny>,
    end: Bound<'py, PyAny>, include_summary: Bound<'py, PyAny>) -> PyResult<Py<PyAny>>
{
    let kwargs = PyDict::new(py); kwargs.set_item("include_summary", include_summary)?;
    Ok(client.call_method("limit_up_down_list", (start, end), Some(&kwargs))?.unbind())
}
