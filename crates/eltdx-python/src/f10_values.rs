//! Python scalar/date/row projection with original permissive conversions.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

fn conversion_error(py: Python<'_>, module: &Bound<'_, PyAny>, error: &PyErr) -> PyResult<bool> {
    let isinstance = builtin(module, "isinstance")?;
    let classes = PyTuple::new(
        py,
        [
            builtin(module, "TypeError")?,
            builtin(module, "ValueError")?,
        ],
    )?;
    isinstance.call1((error.value(py), classes))?.is_truthy()
}

#[pyfunction]
pub fn f10_scalar<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    kind: &str,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if kind == "market" {
        let text = module.getattr("_text")?.call1((value,))?;
        if text.is_none() {
            return Ok(py.None());
        }
        let map = PyDict::new(py);
        map.set_item("1", 1)?;
        map.set_item("0", 0)?;
        return Ok(map.call_method1("get", (text,))?.unbind());
    }
    if value.is_none() {
        return Ok(py.None());
    }
    if kind == "text" || kind == "as_text" {
        let mut text = builtin(&module, "str")?.call1((value,))?;
        if kind == "text" {
            text = text.call_method0("strip")?;
            if !text.is_truthy()? {
                return Ok(py.None());
            }
        }
        return Ok(text.unbind());
    }
    if value.eq("")? {
        return Ok(py.None());
    }
    if kind == "as_number" {
        let isinstance = builtin(&module, "isinstance")?;
        if isinstance
            .call1((&value, builtin(&module, "bool")?))?
            .is_truthy()?
        {
            return Ok(builtin(&module, "int")?.call1((value,))?.unbind());
        }
        let isinstance = builtin(&module, "isinstance")?;
        let types = PyTuple::new(py, [builtin(&module, "int")?, builtin(&module, "float")?])?;
        if isinstance.call1((&value, types))?.is_truthy()? {
            return Ok(value.unbind());
        }
    }
    let converted: PyResult<Bound<'py, PyAny>> = (|| match kind {
        "int" => {
            let convert = builtin(&module, "int")?;
            convert.call1((builtin(&module, "str")?
                .call1((value,))?
                .call_method0("strip")?,))
        }
        "as_int" => builtin(&module, "int")?.call1((value,)),
        "float" | "as_number" => builtin(&module, "float")?.call1((value,)),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "unknown F10 scalar policy",
        )),
    })();
    let number = match converted {
        Ok(number) => number,
        Err(error) => {
            if conversion_error(py, &module, &error)? {
                return Ok(py.None());
            } else {
                return Err(error);
            }
        }
    };
    if kind == "as_number" && number.call_method0("is_integer")?.is_truthy()? {
        return Ok(builtin(&module, "int")?.call1((number,))?.unbind());
    }
    Ok(number.unbind())
}

#[pyfunction]
pub fn f10_first_value<'py>(
    py: Python<'py>,
    response: Bound<'py, PyAny>,
    field: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let result =
        eltdx::f10::collection::first_match(response.getattr("rows")?.try_iter()?, |row| {
            py.check_signals()?;
            Ok::<_, PyErr>(if row.contains(&field)? {
                Some(row.get_item(&field)?)
            } else {
                None
            })
        })?;
    Ok(result.map_or_else(|| py.None(), |value| value.unbind()))
}

#[pyfunction]
pub fn f10_result_total<'py>(
    py: Python<'py>,
    response: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    for table in response.getattr("result_sets")?.try_iter()? {
        let table = table?;
        if table.getattr("key")?.eq("table0")? {
            continue;
        }
        for row in table.getattr("rows")?.try_iter()? {
            for value in row?.call_method0("values")?.try_iter()? {
                py.check_signals()?;
                let value = value?;
                let isinstance = builtin(&module, "isinstance")?;
                if isinstance
                    .call1((&value, builtin(&module, "int")?))?
                    .is_truthy()?
                {
                    return Ok(value.unbind());
                }
                let parsed = module.getattr("_int")?.call1((value,))?;
                if !parsed.is_none() {
                    return Ok(parsed.unbind());
                }
            }
        }
    }
    Ok(py.None())
}

#[pyfunction]
pub fn f10_latest_period<'py>(
    py: Python<'py>,
    response: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let rows = response.getattr("rows")?;
    if !rows.is_truthy()? {
        return Ok("".into_pyobject(py)?.into_any().unbind());
    }
    let text = builtin(&module, "str")?;
    let value = rows.get_item(0)?.call_method1("get", ("rq",))?;
    Ok(text
        .call1((if value.is_truthy()? {
            value
        } else {
            "".into_pyobject(py)?.into_any()
        },))?
        .unbind())
}

fn invalid_date(module: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<PyErr> {
    let class = builtin(module, "ValueError")?;
    let message = "invalid date: {!r}"
        .into_pyobject(module.py())?
        .call_method1("format", (value,))?;
    Ok(PyErr::from_value(class.call1((message,))?))
}

#[pyfunction]
pub fn f10_date_text<'py>(
    py: Python<'py>,
    mut value: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if value.is_none() {
        return Ok(py.None());
    }
    if builtin(&module, "isinstance")?
        .call1((&value, module.getattr("datetime")?))?
        .is_truthy()?
    {
        value = value.call_method0("date")?;
    }
    if builtin(&module, "isinstance")?
        .call1((&value, module.getattr("date")?))?
        .is_truthy()?
    {
        return Ok(value.call_method1("strftime", ("%Y%m%d",))?.unbind());
    }
    let text = builtin(&module, "str")?
        .call1((&value,))?
        .call_method0("strip")?
        .call_method1("replace", ("-", ""))?
        .call_method1("replace", ("/", ""))?;
    if !builtin(&module, "len")?.call1((&text,))?.eq(8)?
        || !text.call_method0("isdigit")?.is_truthy()?
    {
        return Err(invalid_date(&module, &value)?);
    }
    if let Err(error) = module
        .getattr("datetime")?
        .call_method1("strptime", (&text, "%Y%m%d"))
    {
        let class = builtin(&module, "ValueError")?;
        if builtin(&module, "isinstance")?
            .call1((error.value(py), class))?
            .is_truthy()?
        {
            let outer = invalid_date(&module, &value)?;
            outer.set_cause(py, Some(error));
            return Err(outer);
        }
        return Err(error);
    }
    Ok(text.unbind())
}

#[pyfunction]
pub fn f10_ladder_dates<'py>(
    py: Python<'py>,
    start: Bound<'py, PyAny>,
    end: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let start = module.getattr("_date_text")?.call1((start,))?;
    let end = module.getattr("_date_text")?.call1((end,))?;
    let both_missing = start.is_none() && end.is_none();
    let (start, end) = eltdx::f10::collection::resolve_dates(
        if start.is_none() { None } else { Some(start) },
        if end.is_none() { None } else { Some(end) },
        || {
            module
                .getattr("date")?
                .call_method0("today")?
                .call_method1("strftime", ("%Y%m%d",))
        },
    )?;
    // The original default-today branch returns before ordering comparisons.
    if !both_missing && start.gt(&end)? {
        return Err(PyErr::from_value(
            builtin(&module, "ValueError")?.call1(("start_date must be on or before end_date",))?,
        ));
    }
    Ok((start, end).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn f10_ladder_row<'py>(
    py: Python<'py>,
    row: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let kind = module
        .getattr("_as_int")?
        .call1((row.call_method1("get", ("ztlb",))?,))?;
    let level = module
        .getattr("_as_int")?
        .call1((row.call_method1("get", ("lbts",))?,))?;
    let constructor = module.getattr("LimitBoardLadderRow")?;
    let kwargs = PyDict::new(py);
    for &(attribute, column, convert) in eltdx::f10::collection::LADDER_ROW_FIELDS {
        if attribute == "code" {
            let keep = eltdx::f10::collection::keep_ladder_level(
                || kind.eq(1),
                || Ok::<_, PyErr>(!level.is_none()),
                || level.gt(0),
            )?;
            kwargs.set_item(
                "board_level",
                if keep {
                    level.clone()
                } else {
                    py.None().into_bound(py)
                },
            )?;
        }
        let value = if attribute == "limit_type" {
            kind.clone()
        } else if let Some(convert) = convert {
            let convert = module.getattr(convert)?;
            convert.call1((row.call_method1("get", (column,))?,))?
        } else {
            row.call_method1("get", (column,))?
        };
        kwargs.set_item(attribute, value)?;
    }
    kwargs.set_item("raw", builtin(&module, "dict")?.call1((row,))?)?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}
