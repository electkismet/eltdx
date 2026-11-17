//! Existing Python bar models over the shared fixed-record bit decoder.
use eltdx::offline::bars::python_legacy::{self as codec, Layout, Record, Slot};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn layout(value: &str) -> PyResult<Layout> {
    match value {
        "daily" => Ok(Layout::Daily),
        "lc" => Ok(Layout::MinuteFloat),
        "five" => Ok(Layout::MinuteInteger),
        "ex" => Ok(Layout::Extended),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "unknown private bar layout",
        )),
    }
}

#[pyfunction]
pub fn offline_model_date(py: Python<'_>, model: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let operator = py.import("operator")?;
    let value = codec::date_integer_with(
        |field| model.getattr(field),
        |left, right| operator.getattr("mul")?.call1((left, right)),
        |left, right| operator.getattr("add")?.call1((left, right)),
    )?;
    Ok(value.unbind())
}

#[pyfunction]
pub fn offline_model_datetime(py: Python<'_>, model: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let fields = codec::datetime_fields(|field| model.getattr(field))?;
    Ok(PyTuple::new(py, fields)?.into_any().unbind())
}

fn project(
    py: Python<'_>,
    row: Record,
    layout: Layout,
    module: &Bound<'_, PyAny>,
    coefficients: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let args = PyDict::new(py);
    let coefficient = || {
        coefficients
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("daily coefficients missing"))
    };
    let model = match layout {
        Layout::Daily => "DailyBar",
        Layout::Extended => "ExDailyBar",
        _ => "MinBar",
    };
    let constructor;
    if matches!(layout, Layout::Daily | Layout::Extended) {
        constructor = module.getattr(model)?;
        let (year, month, day) = codec::date_parts(row.date);
        args.set_item("year", year)?;
        args.set_item("month", month)?;
        args.set_item("day", day)?;
    } else {
        // These hooks remain callable, even when they return noncalendar values.
        let date = crate::offline_financial::unpack(
            module.getattr("_decode_tdx_date")?.call1((row.date,))?,
            3,
        )?;
        let time = crate::offline_financial::unpack(
            module.getattr("_decode_tdx_time")?.call1((row.time,))?,
            2,
        )?;
        constructor = module.getattr(model)?;
        for (index, field) in ["year", "month", "day"].iter().enumerate() {
            args.set_item(field, &date[index])?;
        }
        for (index, field) in ["hour", "minute"].iter().enumerate() {
            args.set_item(field, &time[index])?;
        }
    }
    let multiply = py.import("operator")?.getattr("mul")?;
    for (index, field) in ["open", "high", "low", "close"].iter().enumerate() {
        match layout {
            Layout::Daily => {
                let coefficient = coefficient()?.get_item(0)?;
                args.set_item(field, multiply.call1((row.prices[index], coefficient))?)?;
            }
            Layout::MinuteInteger => args.set_item(field, f64::from(row.prices[index]) / 100.0)?,
            _ => args.set_item(field, f64::from(f32::from_bits(row.prices[index])))?,
        }
    }
    match layout {
        Layout::Daily => {
            args.set_item(
                "volume",
                multiply.call1((row.volume, coefficient()?.get_item(1)?))?,
            )?;
            args.set_item("amount", f64::from(f32::from_bits(row.amount)))?;
            args.set_item("adjust_flag", row.tail)?;
        }
        Layout::Extended => {
            args.set_item("amount", row.amount)?;
            args.set_item("hk_stock_amount", f64::from(f32::from_bits(row.amount)))?;
            args.set_item("volume", row.volume)?;
            args.set_item("settlement", f64::from(f32::from_bits(row.tail)))?;
        }
        _ => {
            let builtin = py.import("builtins")?;
            let float =
                builtin
                    .getattr("getattr")?
                    .call1((module, "float", builtin.getattr("float")?))?;
            args.set_item("volume", float.call1((row.volume,))?)?;
            args.set_item("amount", f64::from(f32::from_bits(row.amount)))?;
            args.set_item("reserved", row.tail)?;
        }
    }
    args.set_item("raw", PyBytes::new(py, &row.raw))?;
    Ok(constructor.call((), Some(&args))?.unbind())
}

#[pyfunction]
pub fn offline_decode_bars(
    py: Python<'_>,
    data: Bound<'_, PyBytes>,
    kind: &str,
    module: Bound<'_, PyAny>,
    coefficients: Option<Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let selected = layout(kind)?;
    let result = PyList::empty(py);
    codec::visit(
        data.as_bytes(),
        selected,
        |row| result.append(project(py, row, selected, &module, coefficients.as_ref())?),
        &mut || py.check_signals(),
    )?;
    Ok(result.into_any().unbind())
}

fn builtin<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyAny>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let builtins = py.import("builtins")?;
    builtins
        .getattr("getattr")?
        .call1((module, name, builtins.getattr(name)?))
}

#[pyfunction]
pub fn offline_encode_bar<'py>(
    py: Python<'py>,
    bar: Bound<'py, PyAny>,
    kind: &str,
    module: Bound<'py, PyAny>,
    price_coefficient: Option<Bound<'py, PyAny>>,
    volume_coefficient: Option<Bound<'py, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let selected = layout(kind)?;
    let fmt = match selected {
        Layout::Daily => "_DAILY_FMT",
        Layout::Extended => "_EX_DAILY_FMT",
        Layout::MinuteFloat => "_LC_MIN_FMT",
        Layout::MinuteInteger => "_MIN_FMT",
    };
    // Capture pack before argument expressions, matching normal Python calls.
    let pack = module.getattr(fmt)?.getattr("pack")?;
    let operator = py.import("operator")?;
    let fields = codec::encode_fields(selected, |slot| {
        let value = match slot {
            Slot::Date if matches!(selected, Layout::Daily | Layout::Extended) => {
                bar.getattr("date_int")?
            }
            Slot::Date => module.getattr("_encode_tdx_date")?.call1((
                bar.getattr("year")?,
                bar.getattr("month")?,
                bar.getattr("day")?,
            ))?,
            Slot::Time => module
                .getattr("_encode_tdx_time")?
                .call1((bar.getattr("hour")?, bar.getattr("minute")?))?,
            Slot::Price(index) if matches!(selected, Layout::Daily | Layout::MinuteInteger) => {
                let integer = builtin(py, &module, "int")?;
                let round = builtin(py, &module, "round")?;
                let price = bar.getattr(["open", "high", "low", "close"][index])?;
                let scaled = if matches!(selected, Layout::Daily) {
                    operator.getattr("truediv")?.call1((
                        price,
                        price_coefficient.as_ref().ok_or_else(|| {
                            pyo3::exceptions::PyValueError::new_err(
                                "daily price coefficient missing",
                            )
                        })?,
                    ))?
                } else {
                    operator.getattr("mul")?.call1((price, 100))?
                };
                integer.call1((round.call1((scaled,))?,))?
            }
            Slot::Price(index) => bar.getattr(["open", "high", "low", "close"][index])?,
            Slot::Amount => bar.getattr("amount")?,
            Slot::Volume if !matches!(selected, Layout::Extended) => {
                let integer = builtin(py, &module, "int")?;
                let round = builtin(py, &module, "round")?;
                let volume = bar.getattr("volume")?;
                let scaled = if matches!(selected, Layout::Daily) {
                    operator.getattr("truediv")?.call1((
                        volume,
                        volume_coefficient.as_ref().ok_or_else(|| {
                            pyo3::exceptions::PyValueError::new_err(
                                "daily volume coefficient missing",
                            )
                        })?,
                    ))?
                } else {
                    volume
                };
                integer.call1((round.call1((scaled,))?,))?
            }
            Slot::Volume => bar.getattr("volume")?,
            Slot::Tail => bar.getattr(if matches!(selected, Layout::Daily) {
                "adjust_flag"
            } else if matches!(selected, Layout::Extended) {
                "settlement"
            } else {
                "reserved"
            })?,
        };
        Ok::<_, PyErr>(value)
    })?;
    Ok(pack.call1(PyTuple::new(py, fields)?)?.unbind())
}
