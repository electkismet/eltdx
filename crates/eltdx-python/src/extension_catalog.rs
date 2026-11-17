//! Legacy MAC-EX directory and macro orchestration with explicit Python
//! DTO/command hooks. Strict snapshot caps, TTLs and numeric validation are
//! not applied to the original Python surface.
use crate::f10_request::builtin;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};

#[pyfunction]
pub fn mac_ex_goods_items<'py>(
    py: Python<'py>,
    infos: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let items = PyList::empty(py);
    for pair in builtin(&module, "enumerate")?.call1((infos,))?.try_iter()? {
        py.check_signals()?;
        let pair = crate::offline_financial::unpack(pair?, 2)?;
        let info = &pair[1];
        let numeric: PyResult<Bound<'py, PyAny>> = (|| {
            let convert = builtin(&module, "float")?;
            Ok(PyList::new(py, [convert.call1((info.getattr("code")?,))?])?.into_any())
        })();
        let numeric = match numeric {
            Ok(value) => value,
            Err(error) => {
                let classes = PyTuple::new(
                    py,
                    [
                        builtin(&module, "TypeError")?,
                        builtin(&module, "ValueError")?,
                    ],
                )?;
                if !error.matches(py, classes)? {
                    return Err(error);
                }
                PyList::empty(py).into_any()
            }
        };
        let append = items.getattr("append")?;
        let constructor = module.getattr("GoodsItem")?;
        let kwargs = PyDict::new(py);
        let mut name = info.getattr("code")?;
        if !name.is_truthy()? {
            name = info.getattr("name")?;
        }
        kwargs.set_item("name", name)?;
        kwargs.set_item("category", info.getattr("category")?)?;
        kwargs.set_item("u", 0)?;
        kwargs.set_item("index", &pair[0])?;
        kwargs.set_item("switch", 0)?;
        kwargs.set_item("code", numeric)?;
        kwargs.set_item("c1", 0)?;
        kwargs.set_item("c2", 0)?;
        append.call1((constructor.call((), Some(&kwargs))?,))?;
    }
    Ok(items.into_any().unbind())
}

#[pyfunction]
pub fn mac_ex_first_code<'py>(
    py: Python<'py>,
    item: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let mut label = item.getattr("name")?;
    if !label.is_truthy()? {
        label = "".into_pyobject(py)?.into_any();
    }
    let label = label.call_method0("strip")?;
    if label.is_truthy()? {
        return Ok(label.unbind());
    }
    let parts = item.getattr("code")?;
    let values = PyList::empty(py);
    for value in parts.try_iter()? {
        py.check_signals()?;
        let value = value?;
        values.append(builtin(&module, "int")?.call1((value,))?)?;
    }
    if values.is_empty() || values.get_item(0)?.le(0)? {
        return Ok("".into_pyobject(py)?.into_any().unbind());
    }
    Ok(builtin(&module, "str")?
        .call1((values.get_item(0)?,))?
        .unbind())
}

fn prefix<'py>(rows: Bound<'py, PyAny>, end: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = rows.py();
    let slice = py
        .import("builtins")?
        .getattr("slice")?
        .call1((py.None(), end, py.None()))?;
    rows.get_item(slice)
}

#[pyfunction]
pub fn mac_ex_goods_count<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    market: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if market.is_none() {
        let execute = client.getattr("_execute")?;
        return Ok(execute
            .call1((module.getattr("GetExInstrumentCountCmd")?.call0()?,))?
            .unbind());
    }
    let command = module.getattr("GoodsListCmd")?.call1((&market, 0, 1))?;
    client.call_method1("_execute", (&command,))?;
    if command.getattr("total")?.is_truthy()? {
        let convert = builtin(&module, "int")?;
        return Ok(convert.call1((command.getattr("total")?,))?.unbind());
    }
    let bounds =
        crate::offline_financial::unpack(client.call_method1("_market_bounds", (market,))?, 2)?;
    Ok(py
        .import("operator")?
        .getattr("sub")?
        .call1((&bounds[1], &bounds[0]))?
        .unbind())
}

#[pyfunction]
pub fn mac_ex_goods_list<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    market: Bound<'py, PyAny>,
    mut offset: Bound<'py, PyAny>,
    count: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let start = offset.clone();
    let collected = PyList::empty(py);
    let op = py.import("operator")?;
    while builtin(&module, "len")?.call1((&collected,))?.lt(&count)? {
        py.check_signals()?;
        let execute = client.getattr("_execute")?;
        let constructor = module.getattr("GoodsListCmd")?;
        let size = builtin(&module, "min")?.call1((600, 1000))?;
        let batch = execute.call1((constructor.call1((&market, &offset, size))?,))?;
        if !batch.is_truthy()? {
            break;
        }
        collected.call_method1("extend", (&batch,))?;
        offset = op
            .getattr("iadd")?
            .call1((offset, builtin(&module, "len")?.call1((&batch,))?))?;
        if builtin(&module, "len")?.call1((batch,))?.lt(600)? {
            break;
        }
    }
    if !collected.is_empty() {
        return Ok(prefix(collected.into_any(), &count)?.unbind());
    }
    let convert = module.getattr("_to_goods_items")?;
    let instruments = client.getattr("instruments")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("start", start)?;
    kwargs.set_item("count", count)?;
    Ok(convert
        .call1((instruments.call((market,), Some(&kwargs))?,))?
        .unbind())
}

fn lower_bound<'py>(
    py: Python<'py>,
    client: &Bound<'py, PyAny>,
    market: &Bound<'py, PyAny>,
    total: &Bound<'py, PyAny>,
    greater: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let op = py.import("operator")?;
    let mut lo = 0_i32.into_pyobject(py)?.into_any();
    let mut hi = total.clone();
    while lo.lt(&hi)? {
        py.check_signals()?;
        let mid = op
            .getattr("floordiv")?
            .call1((op.getattr("add")?.call1((&lo, &hi))?, 2))?;
        let items = client.call_method1("_info_page", (&mid, 1))?;
        if !items.is_truthy()? {
            hi = mid;
            continue;
        }
        let value = items.get_item(0)?.getattr("market")?;
        let before = if greater {
            value.gt(market)?
        } else {
            value.ge(market)?
        };
        if before {
            hi = mid;
        } else {
            lo = op.getattr("add")?.call1((mid, 1))?;
        }
    }
    Ok(lo)
}

#[pyfunction]
pub fn mac_ex_market_bounds<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    market: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let cached = client
        .getattr("_bounds_cache")?
        .call_method1("get", (&market,))?;
    if !cached.is_none() {
        return Ok(cached.unbind());
    }
    let execute = client.getattr("_execute")?;
    let total = execute.call1((module.getattr("GetExInstrumentCountCmd")?.call0()?,))?;
    if total.le(0)? {
        return Ok((0, 0).into_pyobject(py)?.into_any().unbind());
    }
    let bounds = PyTuple::new(
        py,
        [
            lower_bound(py, &client, &market, &total, false)?,
            lower_bound(py, &client, &market, &total, true)?,
        ],
    )?;
    client.getattr("_bounds_cache")?.set_item(market, &bounds)?;
    Ok(bounds.into_any().unbind())
}

#[pyfunction]
pub fn mac_ex_instruments<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    market: Bound<'py, PyAny>,
    start: Bound<'py, PyAny>,
    count: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if count.le(0)? {
        return Ok(PyList::empty(py).into_any().unbind());
    }
    let bounds =
        crate::offline_financial::unpack(client.call_method1("_market_bounds", (&market,))?, 2)?;
    let execute = client.getattr("_execute")?;
    let total = execute.call1((module.getattr("GetExInstrumentCountCmd")?.call0()?,))?;
    let minimum = builtin(&module, "min")?;
    let page = minimum.call1((builtin(&module, "max")?.call1((&count, 1))?, 1000))?;
    let collected = PyList::empty(py);
    let mut skipped = 0_i32.into_pyobject(py)?.into_any();
    let mut pos = bounds[0].clone();
    let op = py.import("operator")?;
    while pos.lt(&total)? && builtin(&module, "len")?.call1((&collected,))?.lt(&count)? {
        py.check_signals()?;
        let batch = client.call_method1("_info_page", (&pos, &page))?;
        if !batch.is_truthy()? {
            break;
        }
        for item in batch.try_iter()? {
            py.check_signals()?;
            let item = item?;
            if item.getattr("market")?.eq(&market)? {
                if skipped.lt(&start)? {
                    skipped = op.getattr("iadd")?.call1((skipped, 1))?;
                    continue;
                }
                collected.append(item)?;
                if builtin(&module, "len")?.call1((&collected,))?.ge(&count)? {
                    break;
                }
            } else if item.getattr("market")?.gt(&market)? {
                return Ok(collected.into_any().unbind());
            }
        }
        pos = op
            .getattr("iadd")?
            .call1((pos, builtin(&module, "len")?.call1((batch,))?))?;
    }
    Ok(collected.into_any().unbind())
}

fn cache_items<'py>(py: Python<'py>, items: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let cache = PyDict::new(py);
    for item in items.try_iter()? {
        py.check_signals()?;
        let item = item?;
        cache.set_item(item.getattr("name")?, item)?;
    }
    Ok(cache)
}

#[pyfunction]
pub fn mac_ex_macro_code<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let text = builtin(&module, "str")?
        .call1((&code,))?
        .call_method0("strip")?;
    if text.contains("_")? {
        return Ok(text.unbind());
    }
    if !client.getattr("_macro_cache")?.is_truthy()? {
        let items = client.call_method0("macro_indicators")?;
        client.setattr("_macro_cache", cache_items(py, &items)?)?;
    }
    let candidates = PyList::empty(py);
    for name in client.getattr("_macro_cache")?.try_iter()? {
        py.check_signals()?;
        let name = name?;
        let matches =
            if name.is_exact_instance_of::<PyString>() && text.is_exact_instance_of::<PyString>() {
                let name = name.extract::<String>()?;
                let text = text.extract::<String>()?;
                eltdx::mac::python_legacy::macro_suffix(&name).unwrap_or("") == text
            } else {
                name.call_method1("partition", ("_",))?
                    .get_item(2)?
                    .eq(&text)?
            };
        if matches {
            candidates.append(name)?;
        }
    }
    if builtin(&module, "len")?.call1((&candidates,))?.eq(1)? {
        return Ok(candidates.get_item(0)?.unbind());
    }
    if builtin(&module, "len")?.call1((&candidates,))?.gt(1)? {
        let constructor = builtin(&module, "ValueError")?;
        let original = code.repr()?;
        let ordered = builtin(&module, "sorted")?.call1((candidates,))?;
        let joined = ", ".into_pyobject(py)?.call_method1("join", (ordered,))?;
        let message = "ambiguous macro code {}; candidates: {}"
            .into_pyobject(py)?
            .call_method1("format", (original, joined))?;
        return Err(PyErr::from_value(constructor.call1((message,))?));
    }
    let constructor = module.getattr("ProtocolError")?;
    let message = "unknown macro indicator: {!r}"
        .into_pyobject(py)?
        .call_method1("format", (code,))?;
    Err(PyErr::from_value(constructor.call1((message,))?))
}

#[pyfunction]
pub fn mac_ex_macro_indicators<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    if client.getattr("_macro_cache")?.is_truthy()? {
        let list = builtin(&module, "list")?;
        return Ok(list
            .call1((client.getattr("_macro_cache")?.call_method0("values")?,))?
            .unbind());
    }
    let get = client.getattr("goods_list")?;
    let items = get.call1((
        module.getattr("ExMarket")?.getattr("MACRO_INDICATOR")?,
        0,
        600,
    ))?;
    client.setattr("_macro_cache", cache_items(py, &items)?)?;
    Ok(items.unbind())
}

#[pyfunction]
pub fn mac_ex_macro_quotes<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    mut codes: Bound<'py, PyAny>,
    fields: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let isinstance = builtin(&module, "isinstance")?;
    if isinstance
        .call1((&codes, builtin(&module, "str")?))?
        .is_truthy()?
    {
        codes = PyList::new(py, [codes])?.into_any();
    }
    if !codes.is_truthy()? {
        return Ok(PyList::empty(py).into_any().unbind());
    }
    let resolved = PyList::empty(py);
    for code in codes.try_iter()? {
        py.check_signals()?;
        let code = code?;
        let market = module.getattr("ExMarket")?.getattr("MACRO_INDICATOR")?;
        resolved.append(PyTuple::new(
            py,
            [market, client.call_method1("_macro_code", (code,))?],
        )?)?;
    }
    Ok(client
        .call_method1("goods_quotes", (resolved, fields))?
        .unbind())
}

#[pyfunction]
pub fn mac_ex_macro_kline<'py>(
    py: Python<'py>,
    client: Bound<'py, PyAny>,
    args: Bound<'py, PyTuple>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let get = client.getattr("goods_kline")?;
    let market = module.getattr("ExMarket")?.getattr("MACRO_INDICATOR")?;
    let code = client.call_method1("_macro_code", (args.get_item(0)?,))?;
    let kwargs = PyDict::new(py);
    for (index, name) in [(1, "period"), (2, "start"), (3, "count"), (4, "adjust")] {
        kwargs.set_item(name, args.get_item(index)?)?;
    }
    Ok(get.call((market, code), Some(&kwargs))?.unbind())
}
