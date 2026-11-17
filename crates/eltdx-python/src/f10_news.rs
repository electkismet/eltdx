//! Static news helpers preserve mutable Python registries and object hooks.
use crate::f10_request::builtin;
use eltdx::f10::news::{lookup_name_with, visit_rows, NEWS_RESULT_SET_INDEX};
use pyo3::prelude::*;
use pyo3::types::PyList;

#[pyfunction]
pub fn f10_news_name<'py>(
    value: Bound<'py, PyAny>,
    home: bool,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let lookup = module
        .getattr(if home {
            "HOME_TAB_LABELS"
        } else {
            "NEWS_CHANNEL_LABELS"
        })?
        .getattr("get")?;
    Ok(lookup_name_with(
        || builtin(&module, "int")?.call1((&value,)),
        || {
            let text = builtin(&module, "str")?;
            text.call1((builtin(&module, "int")?.call1((&value,))?,))
        },
        |key, fallback| lookup.call1((key, fallback)),
    )?
    .unbind())
}

#[pyfunction]
pub fn f10_news_rows<'py>(
    py: Python<'py>,
    response: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let sets = match response.getattr("result_sets") {
        Ok(sets) => sets,
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) => {
            return Ok(PyList::empty(py).into_any().unbind());
        }
        Err(error) => return Err(error),
    };
    if builtin(&module, "len")?
        .call1((&sets,))?
        .lt(NEWS_RESULT_SET_INDEX + 1)?
    {
        return Ok(PyList::empty(py).into_any().unbind());
    }
    let rows = sets.get_item(NEWS_RESULT_SET_INDEX)?.getattr("rows")?;
    let result = PyList::empty(py);
    visit_rows(rows.try_iter()?, |row| {
        py.check_signals()?;
        result.append(builtin(&module, "dict")?.call1((row,))?)
    })?;
    Ok(result.into_any().unbind())
}
