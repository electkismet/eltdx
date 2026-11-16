//! Legacy topic-strength assembly with original optional-source error policy.
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyTuple};
use eltdx::helpers::theme::{update_group, ThemeUpdate, RANK_FIELDS};
use crate::helper_f10::builtin;
use crate::helper_numeric::callback;

#[pyclass]
struct ThemeSortKey;
#[pymethods]
impl ThemeSortKey {
    fn __call__<'py>(&self, py: Python<'py>, item: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyTuple>> {
        let negative = py.import("operator")?.getattr("neg")?;
        let mut values = Vec::new();
        for &field in RANK_FIELDS { values.push(negative.call1((item.get_item(field)?,))?); }
        PyTuple::new(py, values)
    }
}

#[pyclass]
struct ThemeRows { iterator: Py<PyIterator>, scope: Py<PyDict> }
#[pymethods]
impl ThemeRows {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        let mut iterator = self.iterator.bind(py).clone();
        let Some(pair) = iterator.next() else { return Ok(None); };
        let result: PyResult<Py<PyAny>> = (|| {
            py.check_signals()?;
            let pair = crate::offline_financial::unpack(pair?, 2)?;
            let item = &pair[1]; let scope = self.scope.bind(py);
            let constructor = callback(scope, "ThemeStrengthRow")?;
            let kwargs = PyDict::new(py); kwargs.set_item("rank", &pair[0])?;
            for name in ["topic_id", "topic_name", "limit_up_count", "highest_ladder_level", "lianban_count"] {
                kwargs.set_item(name, item.get_item(name)?)?;
            }
            let round = builtin(scope, "round")?;
            kwargs.set_item("total_seal_amount", round.call1((item.get_item("total_seal_amount")?, 6))?)?;
            let get = builtin(scope, "getattr")?;
            kwargs.set_item("leader_code", get.call1((item.get_item("leader")?, "full_code", py.None()))?)?;
            let get = builtin(scope, "getattr")?;
            kwargs.set_item("leader_ladder_level", get.call1((item.get_item("leader")?, "ladder_level", py.None()))?)?;
            Ok(constructor.call((), Some(&kwargs))?.unbind())
        })();
        match result {
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                let outer = pyo3::exceptions::PyRuntimeError::new_err("generator raised StopIteration");
                outer.set_cause(py, Some(error)); Err(outer)
            }
            value => value.map(Some),
        }
    }
}

fn observe<'py>(py: Python<'py>, item: &Bound<'py, PyAny>, row: &Bound<'py, PyAny>, scope: &Bound<'py, PyDict>) -> PyResult<()> {
    let op = py.import("operator")?;
    let mut level = py.None().into_bound(py);
    update_group(|step| {
        match step {
            ThemeUpdate::Count => {
                let value = op.getattr("iadd")?.call1((item.get_item("limit_up_count")?, 1))?;
                item.set_item("limit_up_count", value)?;
            }
            ThemeUpdate::Level => {
                level = row.getattr("ladder_level")?;
                if !level.is_truthy()? { level = 0.into_pyobject(py)?.into_any(); }
            }
            ThemeUpdate::Highest => {
                let maximum = builtin(scope, "max")?;
                item.set_item("highest_ladder_level", maximum.call1((item.get_item("highest_ladder_level")?, &level))?)?;
            }
            ThemeUpdate::Streak => {
                let previous = item.get_item("lianban_count")?;
                let convert = builtin(scope, "int")?;
                let value = convert.call1((op.getattr("ge")?.call1((&level, 2))?,))?;
                item.set_item("lianban_count", op.getattr("iadd")?.call1((previous, value))?)?;
            }
            ThemeUpdate::Seal => {
                let previous = item.get_item("total_seal_amount")?;
                let mut value = row.getattr("seal_amount")?;
                if !value.is_truthy()? { value = 0.0.into_pyobject(py)?.into_any(); }
                item.set_item("total_seal_amount", op.getattr("iadd")?.call1((previous, value))?)?;
            }
            ThemeUpdate::Leader => {
                let leader = item.get_item("leader")?;
                let replace = if leader.is_none() { true } else {
                    let mut previous = leader.getattr("ladder_level")?;
                    if !previous.is_truthy()? { previous = 0.into_pyobject(py)?.into_any(); }
                    op.getattr("gt")?.call1((&level, previous))?.is_truthy()?
                };
                if replace { item.set_item("leader", row)?; }
            }
        }
        Ok(())
    })
}

#[pyfunction]
pub fn helper_theme_strength<'py>(py: Python<'py>, helper: Bound<'py, PyAny>, codes: Bound<'py, PyAny>,
    count: Bound<'py, PyAny>, scope: Bound<'py, PyDict>) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py); kwargs.set_item("count", py.None())?;
    let ladder = helper.call_method("limit_ladder", (codes,), Some(&kwargs))?;
    let grouped = PyDict::new(py);
    for row in ladder.getattr("rows")?.try_iter()? {
        py.check_signals()?; let row = row?;
        let topics: PyResult<Bound<'py, PyAny>> = (|| {
            let get = helper.getattr("stock_topics")?;
            get.call1((row.getattr("full_code")?,))?.getattr("topics")
        })();
        let topics = match topics {
            Ok(topics) => topics,
            Err(error) => {
                if !builtin(&scope, "isinstance")?.call1((error.value(py), builtin(&scope, "Exception")?))?.is_truthy()? { return Err(error); }
                PyTuple::empty(py).into_any()
            }
        };
        for topic in topics.try_iter()? {
            py.check_signals()?; let topic = topic?;
            let mut key = topic.getattr("topic_id")?;
            if !key.is_truthy()? { key = topic.getattr("topic_name")?; }
            if !key.is_truthy()? || !topic.getattr("topic_name")?.is_truthy()? { continue; }
            let insert = grouped.getattr("setdefault")?;
            let key = builtin(&scope, "str")?.call1((key,))?;
            // Python evaluates this default even when the key already exists.
            let defaults = PyDict::new(py); defaults.set_item("topic_id", topic.getattr("topic_id")?)?;
            defaults.set_item("topic_name", topic.getattr("topic_name")?)?;
            defaults.set_item("limit_up_count", 0)?; defaults.set_item("highest_ladder_level", 0)?;
            defaults.set_item("lianban_count", 0)?; defaults.set_item("total_seal_amount", 0.0)?; defaults.set_item("leader", py.None())?;
            let item = insert.call1((key, defaults))?;
            observe(py, &item, &row, &scope)?;
        }
    }
    let sorted = builtin(&scope, "sorted")?;
    let values = grouped.call_method0("values")?;
    let kwargs = PyDict::new(py); kwargs.set_item("key", Py::new(py, ThemeSortKey)?)?;
    let mut ranked = sorted.call((values,), Some(&kwargs))?;
    if !count.is_none() {
        if count.le(0)? { return Err(PyErr::from_value(builtin(&scope, "ValueError")?.call1(("count must be positive or None",))?)); }
        let slice = py.import("builtins")?.getattr("slice")?.call1((py.None(), count, py.None()))?;
        ranked = ranked.get_item(slice)?;
    }
    let tuple = builtin(&scope, "tuple")?;
    let enumerate = builtin(&scope, "enumerate")?.call1((ranked, 1))?;
    let values = Py::new(py, ThemeRows { iterator: enumerate.try_iter()?.unbind(), scope: scope.clone().unbind() })?;
    let rows = tuple.call1((values,))?;
    let trade_date = ladder.getattr("trade_date")?;
    let kwargs = PyDict::new(py); kwargs.set_item("trade_date", trade_date)?; kwargs.set_item("rows", rows)?;
    Ok(callback(&scope, "ThemeStrengthTable")?.call((), Some(&kwargs))?.unbind())
}
