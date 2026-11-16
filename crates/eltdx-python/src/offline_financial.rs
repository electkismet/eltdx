//! Python scalar/object projection around the shared financial row parser.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

#[pyfunction]
pub fn financial_file_list(py: Python<'_>, data: Vec<u8>, integer: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let result = PyList::empty(py);
    let text = String::from_utf8_lossy(&data);
    for (filename, hash, size) in eltdx::offline::financial::python_legacy::file_list_rows(&text) {
        py.check_signals()?;
        let size = match integer.call1((size,)) {
            Ok(size) => size,
            Err(error) if error.is_instance_of::<PyValueError>(py) => continue,
            Err(error) => return Err(error),
        };
        result.append((filename, hash, size))?;
    }
    Ok(result.into_any().unbind())
}
