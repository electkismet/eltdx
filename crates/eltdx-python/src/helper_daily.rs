//! Daily reference/limit composition with the original Python compatibility
//! policy. Strict SDK rule evidence and whole-operation budgets stay separate.
use crate::helper_numeric::callback;
use pyo3::exceptions::{PyAttributeError, PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn get<'py>(object: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    object
        .py()
        .import("builtins")?
        .getattr("getattr")?
        .call1((object, name, object.py().None()))
}

fn value_error(py: Python<'_>, error: &PyErr) -> bool {
    error.is_instance_of::<PyAttributeError>(py)
        || error.is_instance_of::<PyTypeError>(py)
        || error.is_instance_of::<PyValueError>(py)
}

#[pyfunction]
pub fn daily_limit_ratio(py: Python<'_>, code: Bound<'_, PyAny>) -> PyResult<f64> {
    if code.call_method1("startswith", ("bj",))?.is_truthy()? {
        return Ok(30.0);
    }
    if code.call_method1("startswith", ("sh688",))?.is_truthy()?
        || (code.call_method1("startswith", ("sz",))?.is_truthy()?
            && code
                .get_item(pyo3::types::PySlice::new(py, 2, isize::MAX, 1))?
                .call_method1("startswith", ("30",))?
                .is_truthy()?)
    {
        return Ok(20.0);
    }
    Ok(10.0)
}

#[pyfunction]
pub fn daily_limit_rule(
    py: Python<'_>,
    code: Bound<'_, PyAny>,
    name: Bound<'_, PyAny>,
) -> PyResult<&'static str> {
    let name = if name.is_truthy()? {
        name
    } else {
        "".into_pyobject(py)?.into_any()
    };
    let upper = name.str()?.call_method0("strip")?.call_method0("upper")?;
    if code.call_method1("startswith", ("bj",))?.is_truthy()? {
        return Ok("bse_30pct");
    }
    if code.call_method1("startswith", ("sh688",))?.is_truthy()? {
        return Ok("star_20pct");
    }
    if code.call_method1("startswith", ("sz",))?.is_truthy()?
        && code
            .get_item(pyo3::types::PySlice::new(py, 2, isize::MAX, 1))?
            .call_method1("startswith", ("30",))?
            .is_truthy()?
    {
        return Ok("chinext_20pct");
    }
    if upper
        .call_method1("startswith", (("ST", "*ST", "SST", "S*ST"),))?
        .is_truthy()?
    {
        Ok("st_main_10pct")
    } else {
        Ok("main_10pct")
    }
}

#[pyfunction]
pub fn daily_new_stock(
    py: Python<'_>,
    ipo: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    workdays: Bound<'_, PyAny>,
) -> PyResult<bool> {
    if ipo.is_none() {
        return Ok(false);
    }
    let result = (|| {
        let ipo = workdays.call_method1("normalize", (ipo,))?;
        let target = workdays.call_method1("normalize", (target,))?;
        if target.lt(&ipo)? {
            return Ok(false);
        }
        let count = workdays.call_method1("range", (ipo, target))?.len()?;
        Ok(count > 0 && count <= 5)
    })();
    match result {
        Err(error) if value_error(py, &error) => Ok(false),
        other => other,
    }
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn daily_limit_row(
    py: Python<'_>,
    code: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    reference_date: Bound<'_, PyAny>,
    security: Bound<'_, PyAny>,
    reference: Bound<'_, PyAny>,
    events: Bound<'_, PyAny>,
    new_stock: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>,
) -> PyResult<Py<PyAny>> {
    let name = get(&security, "name")?;
    let mut reference = reference.unbind();
    let mut source = if reference.is_none(py) {
        py.None()
    } else {
        "kline_unadjusted".into_pyobject(py)?.into_any().unbind()
    };
    let builtins = py.import("builtins")?;
    let float = builtins.getattr("float")?;
    let op = py.import("operator")?;
    if !reference.is_none(py) && events.is_truthy()? {
        let result: PyResult<Py<PyAny>> = (|| {
            for event in events.try_iter()? {
                py.check_signals()?;
                let event = event?;
                // Preserve conversion order and the second read of c4.
                let bonus = float
                    .call1((event.getattr("c3_value")?,))?
                    .extract::<f64>()?;
                let rights = float
                    .call1((event.getattr("c4_value")?,))?
                    .extract::<f64>()?;
                let multiplier = (10.0 + bonus + rights) / 10.0;
                let cash = float
                    .call1((event.getattr("c1_value")?,))?
                    .extract::<f64>()?;
                let rights = float
                    .call1((event.getattr("c4_value")?,))?
                    .extract::<f64>()?;
                let price = float
                    .call1((event.getattr("c2_value")?,))?
                    .extract::<f64>()?;
                let offset = (cash - rights * price) / 10.0;
                if multiplier <= 0.0 {
                    return Err(PyValueError::new_err(
                        "non-positive capital-change multiplier",
                    ));
                }
                let multiplier = multiplier.into_pyobject(py)?.into_any().unbind();
                let offset = offset.into_pyobject(py)?.into_any().unbind();
                reference = eltdx::__python_compat::inverse_affine_with(
                    reference.clone_ref(py),
                    &multiplier,
                    &offset,
                    |value, offset| {
                        Ok::<_, PyErr>(
                            op.getattr("sub")?
                                .call1((value.bind(py), offset.bind(py)))?
                                .unbind(),
                        )
                    },
                    |value, multiplier| {
                        Ok(op
                            .getattr("truediv")?
                            .call1((value.bind(py), multiplier.bind(py)))?
                            .unbind())
                    },
                )?;
            }
            let text = source
                .bind(py)
                .call_method1("__format__", ("",))?
                .extract::<String>()?;
            Ok(format!("{text}+capital_changes")
                .into_pyobject(py)?
                .into_any()
                .unbind())
        })();
        match result {
            Ok(value) => source = value,
            Err(error) if value_error(py, &error) => {
                reference = py.None();
                source = py.None();
            }
            Err(error) => return Err(error),
        }
    }
    let ratio = if new_stock.is_truthy()? {
        py.None().into_bound(py)
    } else {
        callback(&scope, "_price_limit_ratio")?.call1((&code, &name))?
    };
    let rule = if new_stock.is_truthy()? {
        "ipo_first_5_days".into_pyobject(py)?.into_any()
    } else {
        callback(&scope, "_price_limit_rule")?.call1((&code, &name))?
    };
    let (status, upper, lower) = if ratio.is_none() {
        ("no_price_limit", py.None(), py.None())
    } else if reference.is_none(py) {
        ("missing_pre_close", py.None(), py.None())
    } else {
        let bound = |add: bool| -> PyResult<Py<PyAny>> {
            let percent = op.getattr("truediv")?.call1((&ratio, 100.0))?;
            let multiplier = op
                .getattr(if add { "add" } else { "sub" })?
                .call1((1.0, percent))?;
            let value = op.getattr("mul")?.call1((reference.bind(py), multiplier))?;
            let value = op.getattr("add")?.call1((value, 1e-9))?;
            Ok(builtins.getattr("round")?.call1((value, 2))?.unbind())
        };
        ("normal", bound(true)?, bound(false)?)
    };
    let fields = PyDict::new(py);
    fields.set_item("full_code", code)?;
    fields.set_item("trade_date", target)?;
    fields.set_item("name", name)?;
    fields.set_item("pre_close_trade_date", reference_date)?;
    fields.set_item("pre_close", reference)?;
    fields.set_item("limit_up_price", upper)?;
    fields.set_item("limit_down_price", lower)?;
    fields.set_item("limit_ratio_pct", ratio)?;
    fields.set_item("limit_rule", rule)?;
    fields.set_item(
        "limit_status",
        if new_stock.is_truthy()? {
            "no_price_limit"
        } else {
            status
        },
    )?;
    fields.set_item("pre_close_source", source)?;
    Ok(callback(&scope, "DailyPriceLimit")?
        .call((), Some(&fields))?
        .unbind())
}

#[pyfunction]
pub fn helper_daily_limits(
    py: Python<'_>,
    helper: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    scope: Bound<'_, PyDict>,
) -> PyResult<Py<PyAny>> {
    let normalizer = get(
        &helper.getattr("_client")?.getattr("workdays")?,
        "normalize",
    )?;
    let target = if normalizer.is_none() {
        target
    } else {
        normalizer.call1((target,))?
    };
    if !target.is_instance(&callback(&scope, "date")?)? {
        return Err(PyValueError::new_err(format!(
            "invalid trade_date: {}",
            target.repr()?.to_str()?
        )));
    }
    let is_workday = get(
        &helper.getattr("_client")?.getattr("workdays")?,
        "is_workday",
    )?;
    if !is_workday.is_none() && !is_workday.call1((&target,))?.is_truthy()? {
        return Err(PyValueError::new_err(format!(
            "trade_date is not a trading day: {}",
            target.call_method0("isoformat")?.str()?.to_str()?
        )));
    }
    let securities = helper.call_method1("_security_map", (&codes,))?;
    let previous = helper
        .getattr("_client")?
        .getattr("workdays")?
        .call_method1("previous_workday", (&target,))?;
    let bars = helper.call_method1("_kline_previous_close_map", (&codes, &target))?;
    let finance = helper.call_method1("_finance_map", (&codes,))?;
    let events = helper.call_method1("_today_capital_change_map", (&codes, &target))?;
    let phases = PyDict::new(py);
    for code in codes.try_iter()? {
        let code = code?;
        let ipo = get(&finance.call_method1("get", (&code,))?, "ipo_date")?;
        let workdays = helper.getattr("_client")?.getattr("workdays")?;
        phases.set_item(
            &code,
            callback(&scope, "_new_stock_phase")?.call1((ipo, &target, workdays))?,
        )?;
    }
    let rows = PyList::empty(py);
    let default_reference = (py.None(), py.None()).into_pyobject(py)?.into_any();
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        let reference_date = if bars.contains(&code)? {
            let date = bars
                .call_method1("get", (&code, &default_reference))?
                .get_item(0)?;
            if date.is_truthy()? {
                date
            } else {
                previous.clone()
            }
        } else {
            py.None().into_bound(py)
        };
        let security = securities.call_method1("get", (&code,))?;
        let reference = bars
            .call_method1("get", (&code, &default_reference))?
            .get_item(1)?;
        let actions = events.call_method1("get", (&code, PyTuple::empty(py)))?;
        let phase = phases.call_method1("get", (&code, false))?;
        rows.append(callback(&scope, "_build_daily_price_limit")?.call1((
            &code,
            &target,
            reference_date,
            security,
            reference,
            actions,
            phase,
        ))?)?;
    }
    Ok(PyTuple::new(py, rows.iter())?.into_any().unbind())
}

#[pyfunction]
pub fn helper_capital_events(
    py: Python<'_>,
    helper: Bound<'_, PyAny>,
    codes: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let getter = get(
        &get(&helper.getattr("_client")?, "corporate")?,
        "capital_changes",
    )?;
    let result = PyDict::new(py);
    if getter.is_none() || !codes.is_truthy()? {
        return Ok(result.into_any().unbind());
    }
    let response = match getter.call1((codes,)) {
        Ok(response) => response,
        Err(error) if error.is_instance_of::<PyException>(py) => {
            return Ok(result.into_any().unbind())
        }
        Err(error) => return Err(error),
    };
    let mut blocks = get(&response, "blocks")?;
    if blocks.is_none() && response.hasattr("full_code")? {
        blocks = PyTuple::new(py, [&response])?.into_any();
    }
    if blocks.is_truthy()? {
        for block in blocks.try_iter()? {
            py.check_signals()?;
            let block = block?;
            let records = get(&block, "records")?;
            let selected = PyList::empty(py);
            if records.is_truthy()? {
                for record in records.try_iter()? {
                    let record = record?;
                    if get(&record, "category_raw")?.eq(1)? && get(&record, "date")?.eq(&target)? {
                        selected.append(record)?;
                    }
                }
            }
            if !selected.is_empty() {
                result.set_item(
                    block.getattr("full_code")?.str()?,
                    PyTuple::new(py, selected.iter())?,
                )?;
            }
        }
    }
    Ok(result.into_any().unbind())
}

#[pyfunction]
pub fn daily_prior_close(
    py: Python<'_>,
    getter: Bound<'_, PyAny>,
    target: Bound<'_, PyAny>,
    code: Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value: PyResult<Py<PyAny>> = (|| {
        let kwargs = PyDict::new(py);
        kwargs.set_item("period", "day")?;
        kwargs.set_item("start", 0)?;
        kwargs.set_item("count", 800)?;
        kwargs.set_item("adjust", "none")?;
        let series = getter.call((&code,), Some(&kwargs))?;
        let raw = get(&series, "bars")?;
        let bars = if raw.is_truthy()? {
            py.import("builtins")?.getattr("tuple")?.call1((raw,))?
        } else {
            PyTuple::empty(py).into_any()
        };
        let mut latest = None;
        for bar in bars.try_iter()? {
            py.check_signals()?;
            let bar = bar?;
            let time = get(&bar, "time")?;
            let date = match time.getattr("date") {
                Ok(method) => method.call0()?,
                Err(error) if error.is_instance_of::<PyAttributeError>(py) => {
                    py.None().into_bound(py)
                }
                Err(error) => return Err(error),
            };
            // Legacy takes the last qualifying source row, not the maximum date.
            if !date.is_none() && date.lt(&target)? {
                latest = Some((date, bar));
            }
        }
        match latest {
            Some((date, row)) => {
                let close = py
                    .import("builtins")?
                    .getattr("float")?
                    .call1((row.getattr("close")?,))?;
                Ok((date, close).into_pyobject(py)?.into_any().unbind())
            }
            None => Ok(py.None()),
        }
    })();
    let value = match value {
        Ok(value) => value,
        Err(error) if error.is_instance_of::<PyException>(py) => py.None(),
        Err(error) => return Err(error),
    };
    Ok((code, value).into_pyobject(py)?.into_any().unbind())
}

#[pyfunction]
pub fn daily_reference_map(py: Python<'_>, pairs: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyDict::new(py);
    for pair in pairs.try_iter()? {
        let pair = pair?;
        let tuple = py.import("builtins")?.getattr("tuple")?.call1((pair,))?;
        let count = tuple.len()?;
        if count < 2 {
            return Err(PyValueError::new_err(format!(
                "not enough values to unpack (expected 2, got {count})"
            )));
        }
        if count > 2 {
            return Err(PyValueError::new_err(
                "too many values to unpack (expected 2)",
            ));
        }
        let code = tuple.get_item(0)?;
        let value = tuple.get_item(1)?;
        if !value.is_none() {
            result.set_item(code, value)?;
        }
    }
    Ok(result.into_any().unbind())
}
