//! Legacy MAC date/codec/price rules. These rules are compatibility facts and
//! do not populate the strict SDK's approved, dated exchange-profile registry.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySlice, PyTuple};

fn prefix<'py>(py: Python<'py>, code: &Bound<'py, PyAny>, values: &[&str]) -> PyResult<bool> {
    code.call_method1("startswith", (PyTuple::new(py, values)?,))?
        .is_truthy()
}

#[pyfunction]
pub fn mac_index_like<'py>(
    py: Python<'py>,
    market: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<bool> {
    if market.eq(module.getattr("_MARKET_SH")?)?
        && prefix(
            py,
            &code,
            &["000", "880", "881", "882", "883", "884", "885", "999"],
        )?
    {
        return Ok(true);
    }
    if market.eq(module.getattr("_MARKET_SZ")?)? && prefix(py, &code, &["395", "399"])? {
        return Ok(true);
    }
    Ok(name.contains("指数")? || name.contains("板块")?)
}

#[pyfunction]
pub fn mac_no_limit_window<'py>(
    py: Python<'py>,
    market: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<u8> {
    if module
        .getattr("_is_index_like")?
        .call1((&market, &code, name))?
        .is_truthy()?
    {
        return Ok(0);
    }
    if prefix(py, &code, &["43", "83", "87", "92"])? {
        return Ok(1);
    }
    if market.eq(module.getattr("_MARKET_SH")?)? && prefix(py, &code, &["60", "68"])? {
        return Ok(5);
    }
    if market.eq(module.getattr("_MARKET_SZ")?)? && prefix(py, &code, &["00", "30"])? {
        return Ok(5);
    }
    Ok(0)
}

#[pyfunction]
pub fn mac_price_limits<'py>(
    py: Python<'py>,
    market: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
    reference: Bound<'py, PyAny>,
    listed_days: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let absent = || {
        Ok(PyTuple::new(py, [py.None(), py.None()])?
            .into_any()
            .unbind())
    };
    if reference.le(0)? {
        return absent();
    }
    let upper_name = name.call_method0("upper")?;
    if module
        .getattr("_is_index_like")?
        .call1((&market, &code, &name))?
        .is_truthy()?
    {
        return absent();
    }
    let window = module
        .getattr("get_no_limit_window_days")?
        .call1((market, &code, name))?;
    // Keep 0 < listed_days (and reflected custom comparison), once.
    let positive = if listed_days.is_none() {
        false
    } else {
        0_i32.into_pyobject(py)?.lt(&listed_days)?
    };
    if positive && listed_days.le(window)? {
        return absent();
    }
    let leading = code.get_item(py.get_type::<PySlice>().call1((py.None(), 3))?)?;
    let ratio: f64 = if module.getattr("_CHINEXT_CODES")?.contains(leading)? {
        0.20
    } else if code.call_method1("startswith", ("688",))?.is_truthy()? {
        0.20
    } else if prefix(py, &code, &["43", "83", "87", "92"])? {
        0.30
    } else if upper_name.contains("ST")? {
        0.05
    } else {
        0.10
    };
    let ratio = ratio.into_pyobject(py)?.into_any();
    let op = py.import("operator")?;
    let compute = |upper| -> PyResult<Bound<'py, PyAny>> {
        let price = eltdx::__python_compat::limit_price_with(
            &reference,
            &ratio,
            upper,
            |ratio| op.getattr("add")?.call1((1, ratio)),
            |ratio| op.getattr("sub")?.call1((1, ratio)),
            |reference, factor| op.getattr("mul")?.call1((reference, factor)),
        )?;
        let round = builtin(&module, "round")?;
        round.call1((op.getattr("add")?.call1((price, 0.00001))?, 2))
    };
    let upper = compute(true)?;
    let lower = compute(false)?;
    Ok(PyTuple::new(py, [upper, lower])?.into_any().unbind())
}

#[pyfunction]
pub fn mac_combine_datetime<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    seconds: Bound<'py, PyAny>,
    intraday: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let op = py.import("operator")?;
    let (year, month, day) = eltdx::__python_compat::ymd_parts_with(
        &value,
        |raw| op.getattr("floordiv")?.call1((raw, 10000)),
        |raw| {
            op.getattr("floordiv")?
                .call1((op.getattr("mod")?.call1((raw, 10000))?, 100))
        },
        |raw| op.getattr("mod")?.call1((raw, 100)),
    )?;
    if intraday.is_truthy()? && seconds.is_truthy()? {
        let (hour, minute, second) = eltdx::__python_compat::seconds_parts_with(
            &seconds,
            |raw| op.getattr("floordiv")?.call1((raw, 3600)),
            |raw| {
                op.getattr("floordiv")?
                    .call1((op.getattr("mod")?.call1((raw, 3600))?, 60))
            },
            |raw| op.getattr("mod")?.call1((raw, 60)),
        )?;
        return Ok(module
            .getattr("datetime")?
            .call1((year, month, day, hour, minute, second))?
            .unbind());
    }
    Ok(module
        .getattr("datetime")?
        .call1((year, month, day))?
        .unbind())
}

#[pyfunction]
pub fn mac_decode_gbk<'py>(py: Python<'py>, raw: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let method = raw.getattr("decode")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("errors", "ignore")?;
    Ok(method
        .call(("gbk",), Some(&kwargs))?
        .call_method1("replace", ("\0", ""))?
        .unbind())
}
