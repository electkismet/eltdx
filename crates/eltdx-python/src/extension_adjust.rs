//! Python objects and legacy invalid-factor policy over shared MAC arithmetic.
use crate::f10_request::builtin;
use eltdx::mac::util::{
    forward_denominator_with, forward_numerator_with, price_invalid_with, scale_prices_with,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

fn nan(module: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Ok(builtin(module, "float")?.call1(("nan",))?.unbind())
}

#[pyfunction]
pub fn mac_forward_factor<'py>(
    py: Python<'py>,
    close: Bound<'py, PyAny>,
    cash: Bound<'py, PyAny>,
    price: Bound<'py, PyAny>,
    stock: Bound<'py, PyAny>,
    rights: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if close.le(0)? {
        return nan(&module);
    }
    let op = py.import("operator")?;
    let denominator = forward_denominator_with(
        &close,
        &stock,
        &rights,
        1.0_f64.into_pyobject(py)?.into_any(),
        |left, right| op.getattr("add")?.call1((left, right)),
        |left, right| op.getattr("mul")?.call1((left, right)),
    )?;
    if denominator.eq(0)? {
        return nan(&module);
    }
    let numerator = forward_numerator_with(
        &close,
        &cash,
        &price,
        &rights,
        |left, right| op.getattr("sub")?.call1((left, right)),
        |left, right| op.getattr("mul")?.call1((left, right)),
        |left, right| op.getattr("add")?.call1((left, right)),
    )?;
    let factor = op.getattr("truediv")?.call1((numerator, denominator))?;
    if !module
        .getattr("math")?
        .call_method1("isfinite", (&factor,))?
        .is_truthy()?
    {
        return nan(&module);
    }
    Ok(builtin(&module, "float")?.call1((factor,))?.unbind())
}

#[pyfunction]
pub fn mac_bad_prices(
    py: Python<'_>,
    bars: Bound<'_, PyAny>,
    module: Bound<'_, PyAny>,
) -> PyResult<bool> {
    for bar in bars.try_iter()? {
        py.check_signals()?;
        let bar = bar?;
        // The source constructs the whole OHLC tuple before examining open.
        let prices = [
            bar.getattr("open")?,
            bar.getattr("high")?,
            bar.getattr("low")?,
            bar.getattr("close")?,
        ];
        for value in prices {
            if price_invalid_with(
                || {
                    module
                        .getattr("math")?
                        .call_method1("isfinite", (&value,))?
                        .is_truthy()
                },
                || value.le(0),
            )? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[pyfunction]
pub fn mac_apply_forward<'py>(
    py: Python<'py>,
    bars: Bound<'py, PyAny>,
    events: Bound<'py, PyAny>,
    bar_key: Bound<'py, PyAny>,
    event_key: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let sorted = builtin(&module, "sorted")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", bar_key)?;
    let ordered = sorted.call((bars,), Some(&kwargs))?;
    if !ordered.is_truthy()? {
        return Ok(builtin(&module, "list")?.call1((ordered,))?.unbind());
    }
    let selected = PyList::empty(py);
    for event in events.try_iter()? {
        py.check_signals()?;
        let event = event?;
        if event.getattr("category")?.eq(1)? {
            let any = builtin(&module, "any")?;
            let quantities = PyTuple::new(
                py,
                [
                    event.getattr("fenhong")?,
                    event.getattr("peigujia")?,
                    event.getattr("songzhuangu")?,
                    event.getattr("peigu")?,
                ],
            )?;
            if any.call1((quantities,))?.is_truthy()? {
                selected.append(event)?;
            }
        }
    }
    if selected.is_empty() {
        return Ok(builtin(&module, "list")?.call1((ordered,))?.unbind());
    }
    let dates = PyList::empty(py);
    for bar in ordered.try_iter()? {
        py.check_signals()?;
        dates.append(bar?.getattr("datetime")?.call_method0("date")?)?;
    }
    let opens = PyList::empty(py);
    let highs = PyList::empty(py);
    let lows = PyList::empty(py);
    let closes = PyList::empty(py);
    for (list, name) in [
        (&opens, "open"),
        (&highs, "high"),
        (&lows, "low"),
        (&closes, "close"),
    ] {
        for bar in ordered.try_iter()? {
            py.check_signals()?;
            list.append(bar?.getattr(name)?)?;
        }
    }
    let sorted = builtin(&module, "sorted")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("key", event_key)?;
    let events = sorted.call((selected,), Some(&kwargs))?;
    let op = py.import("operator")?;
    for event in events.try_iter()? {
        py.check_signals()?;
        let event = event?;
        let idx = module
            .getattr("bisect_left")?
            .call1((&dates, event.getattr("date")?))?;
        let cum_idx = op.getattr("sub")?.call1((idx, 1))?;
        if cum_idx.lt(0)? || cum_idx.ge(builtin(&module, "len")?.call1((&ordered,))?)? {
            continue;
        }
        let factor = module.getattr("compute_forward_factor")?.call1((
            closes.as_any().get_item(&cum_idx)?,
            event.getattr("fenhong")?,
            event.getattr("peigujia")?,
            event.getattr("songzhuangu")?,
            event.getattr("peigu")?,
        ))?;
        if !module
            .getattr("math")?
            .call_method1("isfinite", (&factor,))?
            .is_truthy()?
        {
            continue;
        }
        let range = builtin(&module, "range")?;
        let indices = range.call1((op.getattr("add")?.call1((cum_idx, 1))?,))?;
        let prices = [&opens, &highs, &lows, &closes];
        scale_prices_with(
            indices.try_iter()?,
            |index, slot| {
                let list = prices[slot].as_any();
                let value = op
                    .getattr("imul")?
                    .call1((list.get_item(index)?, &factor))?;
                list.set_item(index, value)
            },
            |_| py.check_signals(),
        )?;
    }
    let output = PyList::empty(py);
    for pair in builtin(&module, "enumerate")?
        .call1((ordered,))?
        .try_iter()?
    {
        py.check_signals()?;
        let pair = crate::offline_financial::unpack(pair?, 2)?;
        let (index, bar) = (&pair[0], &pair[1]);
        let constructor = module.getattr("MacBar")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("datetime", bar.getattr("datetime")?)?;
        for (list, name) in [
            (&opens, "open"),
            (&highs, "high"),
            (&lows, "low"),
            (&closes, "close"),
        ] {
            kwargs.set_item(name, list.as_any().get_item(index)?)?;
        }
        for name in ["vol", "amount", "float_shares"] {
            kwargs.set_item(name, bar.getattr(name)?)?;
        }
        output.append(constructor.call((), Some(&kwargs))?)?;
    }
    Ok(output.into_any().unbind())
}
