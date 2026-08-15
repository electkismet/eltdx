use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

create_exception!(
    _native,
    NativeError,
    PyException,
    "Private structured error raised by the eltdx native engine."
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeErrorPayload {
    pub kind: String,
    pub message: String,
    pub context: Vec<(String, String)>,
}

impl NativeErrorPayload {
    pub fn new(
        kind: impl Into<String>,
        message: impl Into<String>,
        context: Vec<(String, String)>,
    ) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            context,
        }
    }

    pub fn into_pyerr(self) -> PyErr {
        NativeError::new_err((self.kind, self.message, self.context))
    }
}

pub fn from_runtime(error: eltdx_runtime::RuntimeError) -> PyErr {
    let payload = NativeErrorPayload::new(error.kind(), error.to_string(), error.context());
    payload.into_pyerr()
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("NativeError", module.py().get_type::<NativeError>())
}
