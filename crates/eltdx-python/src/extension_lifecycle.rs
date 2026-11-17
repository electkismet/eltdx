//! Generation/admission bridge. Socket calls stay in explicit Python adapters;
//! the shared Rust owner controls stale handles and result publication.
use std::sync::Mutex;

use eltdx::__python_compat::{LegacyExtensionOperation, LegacyExtensionSession};
use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::{PyTraverseError, PyVisit};

#[pyclass(frozen, module = "eltdx._native")]
pub(crate) struct NativeExtensionSession {
    pub session: LegacyExtensionSession,
    resource: Mutex<Option<Py<PyAny>>>,
    resource_identity: usize,
}

#[pymethods]
impl NativeExtensionSession {
    #[getter]
    fn generation(&self) -> usize {
        self.session.generation()
    }

    #[getter]
    fn retired(&self) -> bool {
        self.session.ensure_open().is_err()
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Ok(resource) = self.resource.try_lock() {
            visit.call(&*resource)?;
        }
        Ok(())
    }

    fn __clear__(&self) {
        let _ = self.session.retire();
        self.release();
    }
}

impl NativeExtensionSession {
    fn release(&self) {
        // Drop Python references only after unlocking: __del__ may reenter.
        let resource = self
            .resource
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        drop(resource);
    }

    fn retire(&self) -> PyResult<()> {
        let result = self.session.retire().map_err(internal);
        self.release();
        result
    }
}

#[pyclass(frozen, module = "eltdx._native")]
pub(crate) struct NativeExtensionOperation {
    session: Py<NativeExtensionSession>,
    resource: Py<PyAny>,
    admission: Mutex<Option<LegacyExtensionOperation>>,
}

#[pymethods]
impl NativeExtensionOperation {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.session)?;
        visit.call(&self.resource)
    }
}

fn stale(module: &Bound<'_, PyAny>) -> PyResult<PyErr> {
    Ok(PyErr::from_value(
        module
            .getattr("TransportError")?
            .call1(("连接已关闭或已被新连接替换",))?,
    ))
}

fn internal(error: eltdx::Error) -> PyErr {
    // Core errors never carry a Python exception or callback reference.
    pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
}

pub(super) fn current<'py>(
    connection: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, NativeExtensionSession>>> {
    match connection.getattr("_native_session") {
        Ok(value) => Ok(value.cast_into::<NativeExtensionSession>().ok()),
        Err(error)
            if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(connection.py()) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Attach only after the old Python socket publication point. Old admissions
/// retain their old holder and cannot use the replacement's owner.
pub(super) fn publish(
    py: Python<'_>,
    connection: &Bound<'_, PyAny>,
    socket: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let prior = current(connection)?;
    let holder = Py::new(
        py,
        NativeExtensionSession {
            session: LegacyExtensionSession::new(),
            resource_identity: socket.as_ptr() as usize,
            resource: Mutex::new(Some(socket.clone().unbind())),
        },
    )?;
    connection.setattr("_native_session", holder)?;
    if let Some(prior) = prior {
        prior.get().retire()?;
    }
    Ok(())
}

/// A MAC connect owns its provisional socket before publication. It cannot
/// replace a working old generation until TCP connect has actually succeeded.
#[pyfunction]
pub fn extension_attempt_begin(
    py: Python<'_>,
    socket: Py<PyAny>,
) -> PyResult<Py<NativeExtensionOperation>> {
    let session = LegacyExtensionSession::new();
    let admission = session.admit().map_err(internal)?;
    let holder = Py::new(
        py,
        NativeExtensionSession {
            session,
            resource_identity: socket.as_ptr() as usize,
            resource: Mutex::new(Some(socket.clone_ref(py))),
        },
    )?;
    Py::new(
        py,
        NativeExtensionOperation {
            session: holder,
            resource: socket,
            admission: Mutex::new(Some(admission)),
        },
    )
}

#[pyfunction]
pub fn extension_attempt_publish<'py>(
    py: Python<'py>,
    connection: Bound<'py, PyAny>,
    operation: Bound<'py, NativeExtensionOperation>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    extension_operation_check(operation.clone(), module)?;
    let prior = current(&connection)?;
    let holder = operation.get().session.bind(py);
    connection.setattr("_sock", operation.get().resource.bind(py))?;
    connection.setattr("_native_session", holder)?;
    if let Some(prior) = prior {
        prior.get().retire()?;
    }
    Ok(())
}

pub(super) fn retire(connection: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Some(holder) = current(connection)? {
        holder.get().retire()?;
    }
    Ok(())
}

fn cleanup_queue<'py>(connection: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyList>>> {
    match connection.getattr("_native_cleanup_sockets") {
        Ok(value) => Ok(Some(value.cast_into::<PyList>()?)),
        Err(error)
            if error.is_instance_of::<pyo3::exceptions::PyAttributeError>(connection.py()) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn retain_failed_cleanup(connection: &Bound<'_, PyAny>, socket: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = connection.py();
    let queue = match cleanup_queue(connection)? {
        Some(queue) => queue,
        None => {
            let queue = PyList::empty(py);
            connection.setattr("_native_cleanup_sockets", &queue)?;
            queue
        }
    };
    for record in queue.iter() {
        if record.get_item(0)?.is(socket) {
            return Ok(());
        }
    }
    // Keep the resource independently of its old generation, which can be
    // released by a later successful replacement. The flag guards reentrant
    // close; no mutex is held across custom Python cleanup callbacks.
    queue.append(PyList::new(
        py,
        [
            socket.clone(),
            false.into_pyobject(py)?.to_owned().into_any(),
        ],
    )?)
}

fn remove_cleanup_record(queue: &Bound<'_, PyList>, record: &Bound<'_, PyAny>) -> PyResult<()> {
    for (index, candidate) in queue.iter().enumerate() {
        if candidate.is(record) {
            queue.del_item(index)?;
            break;
        }
    }
    Ok(())
}

/// Retry resources retained after an interrupted provisional/replaced socket
/// failed to close. A just-closed current socket is removed without closing it
/// twice. On failure the record remains owned and a later close retries it.
pub(super) fn retry_failed_cleanup(
    connection: &Bound<'_, PyAny>,
    module: &Bound<'_, PyAny>,
    closed: Option<&Bound<'_, PyAny>>,
    domain: &str,
) -> PyResult<()> {
    let Some(queue) = cleanup_queue(connection)? else {
        return Ok(());
    };
    let records: Vec<Py<PyAny>> = queue.iter().map(Bound::unbind).collect();
    for record in records {
        let record = record.bind(connection.py());
        let socket = record.get_item(0)?;
        if closed.is_some_and(|value| value.is(&socket)) {
            remove_cleanup_record(&queue, record)?;
            continue;
        }
        if record.get_item(1)?.is_truthy()? {
            continue;
        }
        record.set_item(1, true)?;
        let result = module
            .getattr(if domain == "mac" {
                "_safe_close"
            } else {
                "_native_safe_close"
            })
            .and_then(|close| close.call1((&socket,)));
        record.set_item(1, false)?;
        result?;
        remove_cleanup_record(&queue, record)?;
    }
    Ok(())
}

#[pyfunction]
pub fn extension_operation_begin<'py>(
    py: Python<'py>,
    connection: Bound<'py, PyAny>,
    socket: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<NativeExtensionOperation>> {
    let socket = if socket.is_none() {
        connection.getattr("_sock")?
    } else {
        socket
    };
    if socket.is_none() {
        return Err(PyErr::from_value(
            module
                .getattr("TransportError")?
                .call1(("未连接，请先调用 connect()",))?,
        ));
    }
    let holder = match current(&connection)? {
        Some(holder) if holder.get().resource_identity == socket.as_ptr() as usize => holder,
        _ => {
            // A direct custom _sock injection is an explicit adapter boundary.
            // It still receives its own generation, never the replaced one.
            publish(py, &connection, &socket)?;
            current(&connection)?.ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("missing extension generation")
            })?
        }
    };
    let admission = holder
        .get()
        .session
        .admit()
        .map_err(|_| stale(&module).unwrap_or_else(|error| error))?;
    Py::new(
        py,
        NativeExtensionOperation {
            session: holder.unbind(),
            resource: socket.unbind(),
            admission: Mutex::new(Some(admission)),
        },
    )
}

#[pyfunction]
pub fn extension_operation_check(
    operation: Bound<'_, NativeExtensionOperation>,
    module: Bound<'_, PyAny>,
) -> PyResult<()> {
    let active = {
        let state = operation.get().admission.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("extension operation state unavailable")
        })?;
        state
            .as_ref()
            .is_some_and(|admission| admission.check().is_ok())
    };
    if active {
        Ok(())
    } else {
        Err(stale(&module)?)
    }
}

#[pyfunction]
pub fn extension_operation_finish<'py>(
    operation: Bound<'py, NativeExtensionOperation>,
    value: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<Py<PyAny>> {
    let admission = operation
        .get()
        .admission
        .lock()
        .map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("extension operation state unavailable")
        })?
        .take();
    let Some(admission) = admission else {
        return Err(stale(&module)?);
    };
    // A Python value must never be dropped under the owner's admission mutex;
    // arbitrary __del__ callbacks can reenter the same connection.
    admission
        .commit(())
        .map_err(|_| stale(&module).unwrap_or_else(|error| error))?;
    Ok(value.unbind())
}

#[pyfunction]
pub fn extension_operation_failed<'py>(
    py: Python<'py>,
    operation: Bound<'py, NativeExtensionOperation>,
    connection: Bound<'py, PyAny>,
    cause: Bound<'py, PyAny>,
    module: Bound<'py, PyAny>,
) -> PyResult<()> {
    let holder = operation.get().session.bind(py);
    if cause.is_instance_of::<pyo3::exceptions::PyKeyboardInterrupt>() {
        // A Python signal has unwound the synchronous I/O call. Retire only
        // the captured socket; a reentrant replacement remains untouched.
        let resource = operation.get().resource.bind(py);
        let close_result = module
            .getattr("_native_signal_close")
            .and_then(|close| close.call1((resource,)));
        // Even a failed custom close seals admission. A queue on the connection
        // retains provisional/replaced resources independently of this holder.
        // If a custom connection refuses the private attribute, the raised
        // signal still exposes the resource and the retention error as cause.
        if let Err(error) = &close_result {
            if let Err(retention) = retain_failed_cleanup(&connection, resource) {
                let fallback = cause.setattr("_eltdx_cleanup_socket", resource);
                error.set_cause(py, Some(retention));
                if let Err(fallback) = fallback {
                    error.set_cause(py, Some(fallback));
                }
            }
        }
        let retire_result = holder.get().session.retire().map_err(internal);
        if close_result.is_ok() {
            holder.get().release();
        }
        match current(&connection) {
            Ok(Some(current)) if current.is(holder) && close_result.is_ok() => {
                if let Err(error) = connection.setattr("_sock", py.None()) {
                    cause.setattr("__cause__", error.value(py))?;
                }
            }
            Err(error) => {
                cause.setattr("__cause__", error.value(py))?;
            }
            _ => {}
        }
        if let Err(error) = close_result {
            cause.setattr("__cause__", error.value(py))?;
        } else if let Err(error) = retire_result {
            cause.setattr("__cause__", error.value(py))?;
        }
    }
    operation
        .get()
        .admission
        .lock()
        .map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("extension operation state unavailable")
        })?
        .take();
    Ok(())
}

#[pyfunction]
pub fn extension_generation_retire(connection: Bound<'_, PyAny>) -> PyResult<()> {
    retire(&connection)
}
