//! Buffer/struct/calendar object adapters over native legacy arithmetic. Python
//! keeps an explicit except shell so hooks see the original active exception.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PySlice, PyTuple};

#[pyfunction]
pub fn extension_require_bytes<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
    pos: Bound<'py, PyAny>,
    size: Bound<'py, PyAny>,
    context: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    if pos.lt(0)? {
        let constructor = module.getattr("ProtocolError")?;
        let message = "{:}: 非法偏移 {:}"
            .into_pyobject(py)?
            .call_method1("format", (context, pos))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let op = py.import("operator")?;
    let end = op.getattr("add")?.call1((&pos, &size))?;
    if end.gt(builtin(&module, "len")?.call1((&data,))?)? {
        let maximum = builtin(&module, "max")?;
        let rest = op
            .getattr("sub")?
            .call1((builtin(&module, "len")?.call1((data,))?, &pos))?;
        let remaining = maximum.call1((rest, 0))?;
        let constructor = module.getattr("ProtocolError")?;
        let message = "{:}: 数据不足，需要 {:} 字节，偏移 {:}，实际剩余 {:} 字节"
            .into_pyobject(py)?
            .call_method1("format", (context, size, pos, remaining))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    Ok(())
}

#[pyfunction]
pub fn extension_prepare_unpack<'py>(
    _py: Python<'py>,
    fmt: Bound<'py, PyAny>,
    data: Bound<'py, PyAny>,
    pos: Bound<'py, PyAny>,
    context: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let require = module.getattr("require_bytes")?;
    let size = module.getattr("struct")?.call_method1("calcsize", (fmt,))?;
    require.call1((data, pos, size, context))?;
    Ok(())
}

#[pyfunction]
pub fn extension_unpack_error<'py>(
    py: Python<'py>,
    context: Bound<'py, PyAny>,
    error: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constructor = module.getattr("ProtocolError")?;
    let message = "{:}: 解析失败: {:}"
        .into_pyobject(py)?
        .call_method1("format", (context, error))?;
    Ok(constructor.call1((message,))?.unbind())
}

#[pyfunction]
pub fn extension_slice_bytes<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
    pos: Bound<'py, PyAny>,
    size: Bound<'py, PyAny>,
    context: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    module
        .getattr("require_bytes")?
        .call1((&data, &pos, &size, context))?;
    let constructor = builtin(&module, "bytes")?;
    let end = py.import("operator")?.getattr("add")?.call1((&pos, size))?;
    // Slice syntax uses the intrinsic slice type, not a replaced builtins.slice.
    let slice = py.get_type::<PySlice>().call1((pos, end))?;
    Ok(constructor.call1((data.get_item(slice)?,))?.unbind())
}

fn packed_parts<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    shift: bool,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let op = py.import("operator")?;
    eltdx::__python_compat::packed_date_parts_with(
        value,
        |raw| {
            op.getattr("add")?.call1((
                op.getattr(if shift { "rshift" } else { "floordiv" })?
                    .call1((raw, if shift { 11 } else { 2048 }))?,
                2004,
            ))
        },
        |raw| {
            op.getattr("floordiv")?
                .call1((op.getattr("mod")?.call1((raw, 2048))?, 100))
        },
        |raw| {
            op.getattr("mod")?
                .call1((op.getattr("mod")?.call1((raw, 2048))?, 100))
        },
    )
}
fn clock_parts<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let op = py.import("operator")?;
    eltdx::__python_compat::clock_parts_with(
        value,
        |raw| op.getattr("floordiv")?.call1((raw, 60)),
        |raw| op.getattr("mod")?.call1((raw, 60)),
    )
}

#[pyfunction]
pub fn ex_date_parts<'py>(
    py: Python<'py>,
    kind: &str,
    value: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if kind == "clock" {
        let (hour, minute) = clock_parts(py, &value)?;
        return Ok(PyTuple::new(py, [hour, minute])?.into_any().unbind());
    }
    let (year, month, day) = packed_parts(py, &value, kind == "minute")?;
    Ok(PyTuple::new(py, [year, month, day])?.into_any().unbind())
}

#[pyfunction]
pub fn ex_datetime_decode<'py>(
    py: Python<'py>,
    kind: &str,
    category: Bound<'py, PyAny>,
    data: Bound<'py, PyAny>,
    pos: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if kind == "auto" {
        if category.lt(4)? || PyTuple::new(py, [7, 8])?.contains(&category)? {
            return Ok(module
                .getattr("get_datetime_minute")?
                .call1((data, pos))?
                .unbind());
        }
        let values = module.getattr("get_datetime_day")?.call1((data, pos))?;
        let mut iter = values.try_iter()?;
        let mut items = Vec::new();
        for _ in 0..4 {
            items.push(iter.next().transpose()?.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "not enough values to unpack (expected 4, got {})",
                    items.len()
                ))
            })?);
        }
        if iter.next().transpose()?.is_some() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "too many values to unpack (expected 4)",
            ));
        }
        let result = [
            items[0].clone(),
            items[1].clone(),
            items[2].clone(),
            15_i32.into_pyobject(py)?.into_any(),
            0_i32.into_pyobject(py)?.into_any(),
            items[3].clone(),
        ];
        return Ok(PyTuple::new(py, result)?.into_any().unbind());
    }
    let (fmt, context, count) = if kind == "minute" {
        ("<HH", "minute datetime", 2)
    } else {
        ("<I", "day datetime", 1)
    };
    let values = module
        .getattr("unpack_from")?
        .call1((fmt, data, &pos, context))?;
    let mut iter = values.try_iter()?;
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(iter.next().transpose()?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "not enough values to unpack (expected {count}, got {})",
                items.len()
            ))
        })?);
    }
    if iter.next().transpose()?.is_some() {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "too many values to unpack (expected {count})"
        )));
    }
    let op = py.import("operator")?;
    let (year, month, day) = if kind == "minute" {
        packed_parts(py, &items[0], true)?
    } else {
        eltdx::__python_compat::ymd_parts_with(
            &items[0],
            |raw| op.getattr("floordiv")?.call1((raw, 10000)),
            |raw| {
                op.getattr("floordiv")?
                    .call1((op.getattr("mod")?.call1((raw, 10000))?, 100))
            },
            |raw| op.getattr("mod")?.call1((raw, 100)),
        )?
    };
    let mut result = vec![year, month, day];
    if kind == "minute" {
        let (hour, minute) = clock_parts(py, &items[1])?;
        result.push(hour);
        result.push(minute);
    }
    result.push(op.getattr("add")?.call1((pos, 4))?);
    Ok(PyTuple::new(py, result)?.into_any().unbind())
}
