use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use pyo3::exceptions::PyOverflowError;

fn exception(class: &Bound<'_, PyAny>, message: String) -> PyErr {
    match class.call1((message,)) { Ok(value) => PyErr::from_value(value), Err(error) => error }
}

#[pyfunction]
pub fn download_resource<'py>(py: Python<'py>, reader: Bound<'py, PyAny>, path: Bound<'py, PyAny>,
    chunk_size: usize, maximum: Option<usize>, chunk_type: Bound<'py, PyAny>,
    protocol_error: Bound<'py, PyAny>, transport_error: Bound<'py, PyAny>) -> PyResult<Py<PyAny>> {
    let mut expected_host: Option<Py<PyAny>> = None;
    let chunks = eltdx::__python_compat::legacy_resource_download(chunk_size, maximum, |offset, size| {
        py.check_signals()?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("offset", offset)?;
        kwargs.set_item("size", size)?;
        let chunk = reader.call_method("read", (&path,), Some(&kwargs))?;
        if !chunk.is_instance(&chunk_type)? {
            return Err(exception(&protocol_error, "file content request did not return FileContentChunk".into()));
        }
        let count = chunk.getattr("chunk_len")?;
        let content = chunk.getattr("content")?;
        if !count.eq(content.len()?)? {
            return Err(exception(&protocol_error, "file content chunk length does not match the returned content".into()));
        }
        if count.gt(size)? {
            return Err(exception(&protocol_error, format!("file content chunk exceeds requested size: {} > {size}", count.str()?.to_str()?)));
        }
        let count = count.extract::<usize>()?;
        let transport = reader.getattr("_transport")?;
        let host = py.import("builtins")?.getattr("getattr")?.call1((transport, "connected_host", py.None()))?;
        if expected_host.is_none() {
            if !host.is_none() { expected_host = Some(host.unbind()); }
        } else if !host.is_none() && !host.eq(expected_host.as_ref().unwrap().bind(py))? {
            return Err(exception(&transport_error, "TDX server changed while downloading a multi-chunk file".into()));
        }
        Ok((content.unbind(), count))
    }, || PyOverflowError::new_err("file content offset overflow"))?;
    // Keep bytes-like acceptance and join error behavior at the Python boundary.
    Ok(PyBytes::new(py, b"").call_method1("join", (pyo3::types::PyList::new(py, chunks)?,))?.unbind())
}
