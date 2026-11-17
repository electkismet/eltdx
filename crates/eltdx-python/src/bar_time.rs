//! Python calendar objects over native legacy bar-time policy. Native Rust
//! dated rollover remains the strict API policy; facade wrapping is explicit.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

#[pyfunction]
pub fn bar_time_check<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    if !module.getattr("VALID_BAR_TIMES")?.contains(&value)? {
        let constructor = builtin(&module, "ValueError")?;
        let start = module.getattr("BAR_TIME_START")?.repr()?;
        let end = module.getattr("BAR_TIME_END")?.repr()?;
        let actual = value.repr()?;
        let message = "bar_time 必须是 {} 或 {}，得到: {}"
            .into_pyobject(py)?
            .call_method1("format", (start, end, actual))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    Ok(())
}

#[pyfunction]
pub fn bar_time_minutes<'py>(
    py: Python<'py>,
    kind: &str,
    value: Bound<'py, PyAny>,
    parameter: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    match kind {
        "ex" => {
            let get = module.getattr("EX_CATEGORY_MINUTES")?.getattr("get")?;
            Ok(get
                .call1((builtin(&module, "int")?.call1((value,))?,))?
                .unbind())
        }
        "mac" => {
            let get = module.getattr("MAC_PERIOD_MINUTES")?.getattr("get")?;
            let base = get.call1((builtin(&module, "int")?.call1((&value,))?,))?;
            if base.is_none() {
                return Ok(base.unbind());
            }
            if builtin(&module, "int")?.call1((value,))?.eq(8)? {
                let max = builtin(&module, "max")?;
                let multiple = max.call1((builtin(&module, "int")?.call1((parameter,))?, 1))?;
                return Ok(op.getattr("mul")?.call1((base, multiple))?.unbind());
            }
            Ok(base.unbind())
        }
        "hq" => {
            if builtin(&module, "int")?.call1((&value,))?.eq(8)? {
                let max = builtin(&module, "max")?;
                return Ok(max
                    .call1((builtin(&module, "int")?.call1((parameter,))?, 1))?
                    .unbind());
            }
            let get = module.getattr("KLINES_RAW_MINUTES")?.getattr("get")?;
            Ok(get
                .call1((builtin(&module, "int")?.call1((value,))?,))?
                .unbind())
        }
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "unknown private bar-time domain",
        )),
    }
}

#[pyfunction]
pub fn bar_time_needed<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    delta: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if value.eq(module.getattr("BAR_TIME_START")?)? {
        return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
    }
    module.getattr("check_bar_time")?.call1((value,))?;
    if delta.is_none() {
        return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
    }
    // Keep the actual rich-comparison result, as Python's final `and` operand.
    Ok(py
        .import("operator")?
        .getattr("gt")?
        .call1((delta, 0))?
        .unbind())
}

fn total_minutes<'py>(
    py: Python<'py>,
    clock: &Bound<'py, PyAny>,
    delta: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let op = py.import("operator")?;
    let hours = op.getattr("mul")?.call1((clock.getattr("hour")?, 60))?;
    let minutes = op
        .getattr("add")?
        .call1((hours, clock.getattr("minute")?))?;
    op.getattr("add")?.call1((minutes, delta))
}
fn clock_kwargs<'py>(py: Python<'py>, total: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let op = py.import("operator")?;
    let kwargs = PyDict::new(py);
    let hour = op
        .getattr("mod")?
        .call1((op.getattr("floordiv")?.call1((total, 60))?, 24))?;
    kwargs.set_item("hour", hour)?;
    kwargs.set_item("minute", op.getattr("mod")?.call1((total, 60))?)?;
    Ok(kwargs)
}

#[pyfunction]
pub fn bar_time_align<'py>(
    py: Python<'py>,
    kind: &str,
    bars: Bound<'py, PyAny>,
    delta: Bound<'py, PyAny>,
    value: Bound<'py, PyAny>,
    field: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if !module
        .getattr("_needs_align")?
        .call1((value, &delta))?
        .is_truthy()?
    {
        return Ok(bars.unbind());
    }
    // Match the facade assertion when enabled; optimized Python removes it.
    if delta.is_none()
        && py
            .import("sys")?
            .getattr("flags")?
            .getattr("optimize")?
            .extract::<usize>()?
            == 0
    {
        return Err(pyo3::exceptions::PyAssertionError::new_err(()));
    }
    let elapsed = if kind == "datetime" {
        let constructor = module.getattr("timedelta")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("minutes", &delta)?;
        Some(constructor.call((), Some(&kwargs))?)
    } else {
        None
    };
    let output = PyList::empty(py);
    for bar in bars.try_iter()? {
        py.check_signals()?;
        let bar = bar?;
        match kind {
            "hour" => {
                let total = total_minutes(py, &bar, &delta)?;
                let append = output.getattr("append")?;
                let replace = module.getattr("replace")?;
                append.call1((replace.call((bar,), Some(&clock_kwargs(py, &total)?))?,))?;
                continue;
            }
            "wrapped" => {
                let when = builtin(&module, "getattr")?.call1((&bar, &field))?;
                let total = total_minutes(py, &when, &delta)?;
                let append = output.getattr("append")?;
                let replace = module.getattr("replace")?;
                let replace_clock = when.getattr("replace")?;
                let when = replace_clock.call((), Some(&clock_kwargs(py, &total)?))?;
                let kwargs = PyDict::new(py);
                kwargs.set_item(&field, when)?;
                append.call1((replace.call((bar,), Some(&kwargs))?,))?;
                continue;
            }
            "datetime" => {
                let append = output.getattr("append")?;
                let replace = module.getattr("replace")?;
                let when = builtin(&module, "getattr")?.call1((&bar, &field))?;
                let when = py
                    .import("operator")?
                    .getattr("add")?
                    .call1((when, elapsed.as_ref().expect("prepared timedelta")))?;
                let kwargs = PyDict::new(py);
                kwargs.set_item(&field, when)?;
                append.call1((replace.call((bar,), Some(&kwargs))?,))?;
                continue;
            }
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "unknown private alignment policy",
                ))
            }
        }
    }
    Ok(output.into_any().unbind())
}

#[pyfunction]
pub fn mac_ymd_date<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if value.is_none() {
        return Ok(value.unbind());
    }
    let constructor = module.getattr("_date")?;
    let op = py.import("operator")?;
    let (year, month, day) = eltdx::__python_compat::ymd_parts_with(
        &value,
        |value| op.getattr("floordiv")?.call1((value, 10000)),
        |value| {
            op.getattr("floordiv")?
                .call1((op.getattr("mod")?.call1((value, 10000))?, 100))
        },
        |value| op.getattr("mod")?.call1((value, 100)),
    )?;
    Ok(constructor.call1((year, month, day))?.unbind())
}
