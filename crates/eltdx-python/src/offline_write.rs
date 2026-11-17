//! Legacy append preparation; Python owns file context managers and OS hooks.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

fn formatted(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value
        .py()
        .import("builtins")?
        .getattr("format")?
        .call1((value, ""))?
        .extract()
}

#[pyfunction]
pub fn offline_alignment(
    py: Python<'_>,
    filepath: Bound<'_, PyAny>,
    record_size: Bound<'_, PyAny>,
    kind: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
) -> PyResult<()> {
    if record_size.le(0)? {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "record_size must be positive, got {}",
            formatted(&record_size)?
        )));
    }
    let path = module.getattr("Path")?.call1((filepath,))?;
    if !path.call_method0("exists")?.is_truthy()? {
        return Ok(());
    }
    let size = path.call_method0("stat")?.getattr("st_size")?;
    let operator = py.import("operator")?;
    let remainder = eltdx::offline::alignment::python_legacy::remainder_with(
        &size,
        &record_size,
        |left, right| operator.getattr("mod")?.call1((left, right)),
    )?;
    if remainder.eq(0)? {
        return Ok(());
    }
    let prefix = format!(
        "{}文件存在残尾记录，拒绝追加: {} ",
        formatted(&kind)?,
        formatted(&path)?
    );
    let detail = format!(
        "(大小 {} 字节，模 {} 余 {} 字节)。",
        formatted(&size)?,
        formatted(&record_size)?,
        formatted(&remainder)?
    );
    let aligned = operator.getattr("sub")?.call1((&size, &remainder))?;
    let message = format!("{prefix}{detail}追加会让此后每条记录错位且不报错。请先把文件截断到 {} 字节（即最近一条完整记录的结尾）再写入。", formatted(&aligned)?);
    let kind = module.getattr("ResourceFormatError")?;
    Err(match kind.call1((message,)) {
        Ok(value) => PyErr::from_value(value),
        Err(error) => error,
    })
}

#[pyfunction]
pub fn offline_select_new_bars<'py>(
    py: Python<'py>,
    bars: Bound<'py, PyAny>,
    last: Bound<'py, PyAny>,
    key: &str,
) -> PyResult<Py<PyAny>> {
    if last.is_none() {
        return Ok(py
            .import("builtins")?
            .getattr("list")?
            .call1((bars,))?
            .unbind());
    }
    let rows = eltdx::offline::bars::python_legacy::select_newer(
        bars.try_iter()?,
        |bar| bar.getattr(key)?.gt(&last),
        &mut || py.check_signals(),
    )?;
    Ok(PyList::new(py, rows)?.into_any().unbind())
}

#[pyfunction]
pub fn offline_encode_batch(
    py: Python<'_>,
    bars: Bound<'_, PyAny>,
    encode: Bound<'_, PyAny>,
    coefficients: Option<Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let values = PyList::empty(py);
    for bar in bars.try_iter()? {
        py.check_signals()?;
        let bar = bar?;
        let encoded = if let Some(coefficients) = &coefficients {
            encode.call1((bar, coefficients.get_item(0)?, coefficients.get_item(1)?))?
        } else {
            encode.call1((bar,))?
        };
        values.append(encoded)?;
    }
    Ok(PyBytes::new(py, b"")
        .call_method1("join", (values,))?
        .unbind())
}

#[pyfunction]
pub fn offline_last_record_plan(
    py: Python<'_>,
    filepath: Bound<'_, PyAny>,
    format: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
    minute: bool,
) -> PyResult<Py<PyAny>> {
    let path = module.getattr("Path")?.call1((filepath,))?;
    if !path.call_method0("is_file")?.is_truthy()? {
        return Ok(py.None());
    }
    let length = path.call_method0("stat")?.getattr("st_size")?;
    let size = if minute {
        format
    } else {
        format.getattr("size")?
    };
    // Daily/EX check short files before modulo; minute historically did modulo
    // first. This matters for explicit custom numeric/Struct adapters.
    if !minute && length.lt(&size)? {
        return Ok(py.None());
    }
    let operator = py.import("operator")?;
    let tail =
        eltdx::offline::alignment::python_legacy::remainder_with(&length, &size, |left, right| {
            operator.getattr("mod")?.call1((left, right))
        })?;
    let aligned = operator.getattr("sub")?.call1((length, tail))?;
    if minute && aligned.lt(&size)? {
        return Ok(py.None());
    }
    let offset = operator.getattr("sub")?.call1((aligned, &size))?;
    Ok((path, offset, size).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn offline_prepare_append(
    py: Python<'_>,
    filepath: Bound<'_, PyAny>,
    bars: Bound<'_, PyAny>,
    last_reader: Bound<'_, PyAny>,
    record_size: Bound<'_, PyAny>,
    kind: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
    minute: bool,
) -> PyResult<Py<PyAny>> {
    let path = filepath;
    module
        .getattr("ensure_aligned_for_append")?
        .call1((&path, record_size, kind))?;
    let last = if minute {
        last_reader.call0()?
    } else {
        last_reader.call1((&path,))?
    };
    let selected = offline_select_new_bars(
        py,
        bars,
        last,
        if minute { "datetime_key" } else { "date_int" },
    )?;
    Ok((path, selected).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn offline_last_record_value(
    py: Python<'_>,
    data: Bound<'_, PyAny>,
    format: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
    minute: bool,
) -> PyResult<Py<PyAny>> {
    if minute {
        let values = module
            .getattr("struct")?
            .call_method1("unpack_from", ("<HH", data, 0))?;
        let pair = crate::offline_financial::unpack(values, 2)?;
        let mut date = crate::offline_financial::unpack(
            module.getattr("_decode_tdx_date")?.call1((&pair[0],))?,
            3,
        )?;
        let time = crate::offline_financial::unpack(
            module.getattr("_decode_tdx_time")?.call1((&pair[1],))?,
            2,
        )?;
        date.extend(time);
        return Ok(pyo3::types::PyTuple::new(py, date)?.into_any().unbind());
    }
    let values = format.call_method1("unpack", (data,))?;
    let mut values = values.try_iter()?;
    let Some(date) = values.next() else {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "not enough values to unpack (expected at least 1, got 0)",
        ));
    };
    let date = date?;
    // Star-unpacking consumes the remaining iterator before conversion.
    for value in values {
        value?;
    }
    let builtins = py.import("builtins")?;
    let integer = builtins
        .getattr("getattr")?
        .call1((&module, "int", builtins.getattr("int")?))?;
    Ok(integer.call1((date,))?.unbind())
}
