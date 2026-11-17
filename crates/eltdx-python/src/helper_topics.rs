//! Legacy topic composition over live Python source/model adapters.
use crate::helper_f10::builtin;
use crate::helper_numeric::callback;
use eltdx::helpers::topics::{self, TopicMatch, TopicSource};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

fn pick<'py>(
    py: Python<'py>,
    scope: &Bound<'py, PyDict>,
    row: &Bound<'py, PyAny>,
    names: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let mut args = vec![row.clone()];
    for name in names {
        args.push(name.into_pyobject(py)?.into_any());
    }
    callback(scope, "_pick")?.call1(PyTuple::new(py, args)?)
}
fn convert_pick<'py>(
    py: Python<'py>,
    scope: &Bound<'py, PyDict>,
    kind: &str,
    row: &Bound<'py, PyAny>,
    names: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let convert = callback(scope, kind)?;
    convert.call1((pick(py, scope, row, names)?,))
}

#[pyfunction]
pub fn helper_topic_pick<'py>(
    py: Python<'py>,
    row: Bound<'py, PyAny>,
    names: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = eltdx::f10::collection::first_match(names.try_iter()?, |name| {
        py.check_signals()?;
        Ok::<_, PyErr>(if row.contains(&name)? {
            Some(row.get_item(name)?)
        } else {
            None
        })
    })?;
    Ok(value.map_or_else(|| py.None(), |value| value.unbind()))
}

#[pyfunction]
pub fn helper_topic_scalar<'py>(
    py: Python<'py>,
    value: Bound<'py, PyAny>,
    kind: &str,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    if value.is_none() {
        return Ok(py.None());
    }
    if kind == "text" {
        let text = builtin(&scope, "str")?
            .call1((value,))?
            .call_method0("strip")?;
        return Ok(if text.is_truthy()? {
            text.unbind()
        } else {
            py.None()
        });
    }
    if value.eq("")? {
        return Ok(py.None());
    }
    let result = builtin(&scope, kind)?.call1((value,));
    match result {
        Ok(value) => Ok(value.unbind()),
        Err(error) => {
            let classes = PyTuple::new(
                py,
                [
                    builtin(&scope, "TypeError")?,
                    builtin(&scope, "ValueError")?,
                ],
            )?;
            if builtin(&scope, "isinstance")?
                .call1((error.value(py), classes))?
                .is_truthy()?
            {
                Ok(py.None())
            } else {
                Err(error)
            }
        }
    }
}

#[pyfunction]
pub fn helper_topic_key<'py>(
    py: Python<'py>,
    id: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let value = if id.is_truthy()? {
        "id:{:}".into_pyobject(py)?.call_method1("format", (id,))?
    } else if name.is_truthy()? {
        "name:{:}"
            .into_pyobject(py)?
            .call_method1("format", (name,))?
    } else {
        "unknown".into_pyobject(py)?.into_any()
    };
    Ok(value.unbind())
}

#[pyclass]
struct HelperTopicRows {
    iterator: Py<PyIterator>,
    scope: Py<PyDict>,
    merged: Option<Py<PyAny>>,
}
#[pymethods]
impl HelperTopicRows {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut iterator = self.iterator.bind(py).clone();
        let Some(value) = iterator.next() else {
            return Ok(None);
        };
        let result: PyResult<Py<PyAny>> = (|| {
            py.check_signals()?;
            let (function, value) = match &self.merged {
                Some(merged) => ("_build_stock_topic", merged.bind(py).get_item(value?)?),
                None => ("_build_topic_stock", value?),
            };
            Ok(callback(self.scope.bind(py), function)?
                .call1((value,))?
                .unbind())
        })();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer =
                    pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error));
                Err(outer)
            }
            value => value.map(Some),
        }
    }
}

#[pyfunction]
pub fn helper_stock_topics<'py>(
    py: Python<'py>,
    helper: Bound<'py, PyAny>,
    code: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let full = callback(&scope, "normalize_code")?.call1((code,))?;
    let rows = |method| -> PyResult<Bound<'py, PyAny>> {
        let list = builtin(&scope, "list")?;
        let get = builtin(&scope, "getattr")?;
        let response = helper
            .getattr("_client")?
            .getattr("f10")?
            .call_method1(method, (&full,))?;
        list.call1((get.call1((response, "rows", PyTuple::empty(py)))?,))
    };
    let ids = rows("topic_ids")?;
    let hot = rows("hot_topics")?;
    let merged = PyDict::new(py);
    let order = PyList::empty(py);
    for (source, rows) in [(TopicSource::TopicIds, ids), (TopicSource::HotTopics, hot)] {
        topics::visit_source(source, rows.try_iter()?, |source, row| {
            py.check_signals()?;
            let id = convert_pick(py, &scope, "_text", &row, source.id_fields())?;
            let name = convert_pick(py, &scope, "_text", &row, source.name_fields())?;
            let key = callback(&scope, "_topic_key")?.call1((&id, &name))?;
            if !merged.contains(&key)? {
                let item = PyDict::new(py);
                item.set_item("source", builtin(&scope, "set")?.call0()?)?;
                item.set_item("raw", PyDict::new(py))?;
                merged.set_item(&key, item)?;
                order.append(&key)?;
            }
            let item = merged
                .get_item(&key)?
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("topic key disappeared"))?;
            let values = PyDict::new(py);
            values.set_item("topic_id", id)?;
            values.set_item("topic_name", name)?;
            if source == TopicSource::HotTopics {
                for &(attribute, names) in topics::HOT_TOPIC_FIELDS {
                    let value = if attribute == "relation_level" {
                        convert_pick(py, &scope, "_float", &row, names)?
                    } else if matches!(attribute, "reason" | "detail_id") {
                        convert_pick(py, &scope, "_text", &row, names)?
                    } else {
                        pick(py, &scope, &row, names)?
                    };
                    values.set_item(attribute, value)?;
                }
            }
            item.call_method1("update", (values,))?;
            merged
                .get_item(&key)?
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("topic key disappeared"))?
                .get_item("source")?
                .call_method1("add", (source.as_str(),))?;
            let copied = builtin(&scope, "dict")?.call1((row,))?;
            merged
                .get_item(&key)?
                .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("topic key disappeared"))?
                .get_item("raw")?
                .set_item(source.as_str(), copied)
        })?;
    }
    let tuple = builtin(&scope, "tuple")?;
    let values = Py::new(
        py,
        HelperTopicRows {
            iterator: order.try_iter()?.unbind(),
            scope: scope.clone().unbind(),
            merged: Some(merged.into_any().unbind()),
        },
    )?;
    let topics = tuple.call1((values,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("code", full)?;
    kwargs.set_item("topics", topics)?;
    Ok(callback(&scope, "StockTopics")?
        .call((), Some(&kwargs))?
        .unbind())
}

#[pyfunction]
pub fn helper_resolve_topic<'py>(
    py: Python<'py>,
    helper: Bound<'py, PyAny>,
    full: Bound<'py, PyAny>,
    id: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let values = helper
        .call_method1("stock_topics", (&full,))?
        .getattr("topics")?;
    let result = topics::resolve_with(!id.is_none(), !name.is_none(), |kind| {
        if kind == TopicMatch::First {
            return Ok(if values.is_truthy()? {
                Some(values.get_item(0)?)
            } else {
                None
            });
        }
        for item in values.try_iter()? {
            py.check_signals()?;
            let item = item?;
            let found = match kind {
                TopicMatch::Id => item.getattr("topic_id")?.eq(&id)?,
                TopicMatch::Name => item.getattr("topic_name")?.eq(&name)?,
                TopicMatch::First => unreachable!(),
            };
            if found {
                return Ok(Some(item));
            }
        }
        Ok::<_, PyErr>(None)
    })?;
    if let Some(value) = result {
        return Ok(value.unbind());
    }
    if !id.is_none() || !name.is_none() {
        let _ = values.is_truthy()?;
    }
    let exception = builtin(&scope, "ValueError")?;
    let selector = if id.is_truthy()? { id } else { name };
    let message = "topic not found for {:}: {!r}"
        .into_pyobject(py)?
        .call_method1("format", (full, selector))?;
    // A replaced ValueError name is evaluated only when the original raises.
    let exception = exception.call1((message,))?;
    Err(PyErr::from_value(exception))
}

#[pyfunction]
pub fn helper_topic_stocks<'py>(
    py: Python<'py>,
    helper: Bound<'py, PyAny>,
    seed: Bound<'py, PyAny>,
    id: Bound<'py, PyAny>,
    name: Bound<'py, PyAny>,
    sort_by: Bound<'py, PyAny>,
    section: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let full = callback(&scope, "normalize_code")?.call1((seed,))?;
    let mut id = if id.is_none() {
        id
    } else {
        builtin(&scope, "str")?.call1((id,))?
    };
    let mut name = name;
    if id.is_none() || !name.is_none() {
        let matched = helper.call_method1("_resolve_topic", (&full, &id, &name))?;
        if !id.is_truthy()? {
            id = matched.getattr("topic_id")?;
        }
        if !name.is_truthy()? {
            name = matched.getattr("topic_name")?;
        }
    }
    if id.is_none() {
        return Err(PyErr::from_value(builtin(&scope, "ValueError")?.call1(
            ("topic_id or topic_name is required when the seed stock has no topic id",),
        )?));
    }
    let kwargs = PyDict::new(py);
    kwargs.set_item("section", section)?;
    kwargs.set_item("sort_by", &sort_by)?;
    let response = helper.getattr("_client")?.getattr("f10")?.call_method(
        "topic_compare",
        (&full, &id),
        Some(&kwargs),
    )?;
    let tuple = builtin(&scope, "tuple")?;
    let rows = builtin(&scope, "getattr")?.call1((response, "rows", PyTuple::empty(py)))?;
    let values = Py::new(
        py,
        HelperTopicRows {
            iterator: rows.try_iter()?.unbind(),
            scope: scope.clone().unbind(),
            merged: None,
        },
    )?;
    let rows = tuple.call1((values,))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("seed_code", full)?;
    kwargs.set_item("topic_id", id)?;
    kwargs.set_item("topic_name", name)?;
    kwargs.set_item("sort_by", sort_by)?;
    kwargs.set_item("rows", rows)?;
    Ok(callback(&scope, "TopicStockTable")?
        .call((), Some(&kwargs))?
        .unbind())
}

#[pyfunction]
pub fn helper_build_stock_topic<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let constructor = callback(&scope, "StockTopic")?;
    let kwargs = PyDict::new(py);
    for name in [
        "topic_id",
        "topic_name",
        "relation_level",
        "selected_date",
        "topic_date",
        "reason",
        "detail_id",
        "category_raw",
    ] {
        kwargs.set_item(name, data.call_method1("get", (name,))?)?;
    }
    let sorted = builtin(&scope, "sorted")?;
    let sources = data.call_method1("get", ("source", PyTuple::empty(py)))?;
    kwargs.set_item(
        "source",
        "+".into_pyobject(py)?
            .call_method1("join", (sorted.call1((sources,))?,))?,
    )?;
    let dict = builtin(&scope, "dict")?;
    kwargs.set_item(
        "raw",
        dict.call1((data.call_method1("get", ("raw", PyDict::new(py)))?,))?,
    )?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}

#[pyfunction]
pub fn helper_build_topic_stock<'py>(
    py: Python<'py>,
    row: Bound<'py, PyAny>,
    scope: Bound<'py, PyDict>,
) -> PyResult<Py<PyAny>> {
    let mut market = convert_pick(py, &scope, "_int", &row, &["sc", "market_id"])?;
    let mut exchange = if market.is_none() {
        py.None().into_bound(py)
    } else {
        callback(&scope, "ID_TO_MARKET")?.call_method1("get", (&market,))?
    };
    let code = convert_pick(py, &scope, "_text", &row, &["zqdm", "code"])?;
    let mut full = if exchange.is_truthy()? && code.is_truthy()? {
        "{:}{:}"
            .into_pyobject(py)?
            .call_method1("format", (&exchange, &code))?
    } else {
        py.None().into_bound(py)
    };
    if full.is_none() && code.is_truthy()? {
        let resolved: PyResult<()> = (|| {
            full = callback(&scope, "normalize_code")?.call1((&code,))?;
            exchange = full.get_item(pyo3::types::PySlice::new(py, 0, 2, 1))?;
            let ids = PyDict::new(py);
            ids.set_item("sz", 0)?;
            ids.set_item("sh", 1)?;
            ids.set_item("bj", 2)?;
            market = ids.call_method1("get", (&exchange,))?;
            Ok(())
        })();
        if let Err(error) = resolved {
            if !builtin(&scope, "isinstance")?
                .call1((error.value(py), builtin(&scope, "Exception")?))?
                .is_truthy()?
            {
                return Err(error);
            }
            full = py.None().into_bound(py);
        }
    }
    let constructor = callback(&scope, "TopicStock")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item(
        "rank",
        convert_pick(py, &scope, "_int", &row, topics::TOPIC_STOCK_FIELDS[0].1)?,
    )?;
    kwargs.set_item("full_code", full)?;
    kwargs.set_item("exchange", exchange)?;
    kwargs.set_item("market_id", market)?;
    kwargs.set_item("code", code)?;
    for &(attribute, names) in &topics::TOPIC_STOCK_FIELDS[1..] {
        let value = if attribute == "name" {
            convert_pick(py, &scope, "_text", &row, names)?
        } else if attribute != "trading_date" {
            convert_pick(py, &scope, "_float", &row, names)?
        } else {
            pick(py, &scope, &row, names)?
        };
        kwargs.set_item(attribute, value)?;
    }
    kwargs.set_item("raw", builtin(&scope, "dict")?.call1((row,))?)?;
    Ok(constructor.call((), Some(&kwargs))?.unbind())
}
