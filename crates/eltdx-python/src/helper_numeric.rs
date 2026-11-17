//! Legacy rounded arithmetic uses Python numeric conversion at the boundary.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn scalar<'py>(py: Python<'py>, value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    py.import("builtins")?.getattr("float")?.call1((value,))
}

fn rounded(py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("builtins")?
        .getattr("round")?
        .call1((value, 6))?
        .unbind())
}

#[pyfunction]
pub fn shortline_scalar(
    py: Python<'_>,
    kind: &str,
    left: Bound<'_, PyAny>,
    right: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if left.is_none() {
        return Ok(py.None());
    }
    let op = py.import("operator")?;
    let float = py.import("builtins")?.getattr("float")?;
    let value = match kind {
        "round" => return rounded(py, float.call1((left,))?),
        "tenk" => op.getattr("mul")?.call1((float.call1((left,))?, 10000.0))?,
        "multiply" => {
            if right.is_none() {
                return Ok(py.None());
            }
            let value = eltdx::__python_compat::product_with(
                float.call1((left,))?.unbind(),
                float.call1((right,))?.unbind(),
                |a, b| Ok::<_, PyErr>(op.getattr("mul")?.call1((a.bind(py), b.bind(py)))?.unbind()),
            )?;
            value.into_bound(py)
        }
        "ratio" | "percent" => {
            if right.is_none() || float.call1((&right,))?.eq(0)? {
                return Ok(py.None());
            }
            let value = eltdx::__python_compat::ratio_with(
                float.call1((left,))?.unbind(),
                float.call1((right,))?.unbind(),
                |a, b| {
                    Ok::<_, PyErr>(
                        op.getattr("truediv")?
                            .call1((a.bind(py), b.bind(py)))?
                            .unbind(),
                    )
                },
            )?;
            if kind == "percent" {
                op.getattr("mul")?.call1((value.bind(py), 100.0))?
            } else {
                value.into_bound(py)
            }
        }
        "change" => {
            let excluded = PyTuple::new(
                py,
                [py.None(), 0_i32.into_pyobject(py)?.into_any().unbind()],
            )?;
            if excluded.contains(&right)? {
                return Ok(py.None());
            }
            let difference = op
                .getattr("sub")?
                .call1((float.call1((left,))?, float.call1((&right,))?))?;
            let value = op
                .getattr("truediv")?
                .call1((difference, float.call1((right,))?))?;
            op.getattr("mul")?.call1((value, 100.0))?
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown shortline scalar calculation",
            ))
        }
    };
    rounded(py, value)
}

#[pyfunction]
pub fn shortline_locked(py: Python<'_>, quote: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let builtins = py.import("builtins")?;
    let values = builtins
        .getattr("getattr")?
        .call1((quote, "buy_levels", PyTuple::empty(py)))?;
    let values = if values.is_truthy()? {
        builtins.getattr("tuple")?.call1((values,))?
    } else {
        PyTuple::empty(py).into_any()
    };
    if values.len()? == 0 {
        return Ok(py.None());
    }
    let first = values.get_item(0)?;
    let left = scalar(py, first.getattr("price")?)?;
    let right = scalar(py, first.getattr("volume")?)?;
    let mul = py.import("operator")?.getattr("mul")?;
    rounded(py, mul.call1((mul.call1((left, right))?, 100.0))?)
}

#[pyfunction]
pub fn shortline_volume_ratio(
    py: Python<'_>,
    volume: Bound<'_, PyAny>,
    bars: Bound<'_, PyAny>,
    ratio: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if volume.is_none() {
        return Ok(py.None());
    }
    let builtins = py.import("builtins")?;
    let bars = builtins.getattr("tuple")?.call1((bars,))?;
    let first = bars.get_item(pyo3::types::PySlice::new(py, 0, 5, 1))?;
    let volumes = PyList::empty(py);
    for bar in first.try_iter()? {
        let bar = bar?;
        if !builtins
            .getattr("getattr")?
            .call1((&bar, "volume_lots", py.None()))?
            .is_none()
        {
            volumes.append(scalar(py, bar.getattr("volume_lots")?)?)?;
        }
    }
    if volumes.len() < 5 {
        return Ok(py.None());
    }
    let sum = builtins.getattr("sum")?.call1((volumes,))?;
    let average = py
        .import("operator")?
        .getattr("truediv")?
        .call1((sum, 240.0 * 5.0))?;
    Ok(ratio.call1((volume, average))?.unbind())
}

pub(super) fn callback<'py>(scope: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    scope
        .get_item(name)?
        .ok_or_else(|| pyo3::exceptions::PyNameError::new_err(name.to_owned()))
}
