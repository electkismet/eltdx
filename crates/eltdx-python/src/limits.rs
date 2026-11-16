use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3::exceptions::PyOverflowError;

#[pyfunction]
pub fn scan_special(py: Python<'_>, api: Bound<'_, PyAny>, start: u64, maximum: u64) -> PyResult<Py<PyAny>> {
    let records = PyList::empty(py);
    eltdx::__python_compat::legacy_special_scan(start, maximum, |index| {
        py.check_signals()?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("start_index", index)?;
        let page = api.call_method("special", (), Some(&kwargs))?;
        let rows = page.getattr("records")?;
        if !rows.is_truthy()? { return Ok(None); }
        records.call_method1("extend", (page.getattr("records")?,))?;
        Ok(Some(page.getattr("count")?.extract::<u64>()?))
    }, || PyOverflowError::new_err("special-limit index overflow"))?;
    Ok(records.unbind().into_any())
}
