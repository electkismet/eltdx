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

pub fn from_sdk(_py: Python<'_>, error: eltdx::Error) -> PyErr {
    use eltdx::ErrorKind as K;

    if error.kind() == K::LimitExceeded
        && error
            .context()
            .iter()
            .any(|(key, value)| key == "python_kind" && value == "RuntimeError")
    {
        return pyo3::exceptions::PyRuntimeError::new_err(error.message().to_owned());
    }

    let kind = match error.kind() {
        K::InvalidArgument => "InvalidArgument",
        K::Unsupported => "UnsupportedCommand",
        K::Closed | K::Connection => "ConnectionClosed",
        K::Timeout => "Timeout",
        K::PoolBusy => "PoolBusy",
        K::PushGap => "PushOverflow",
        K::CloseTimeout => "CloseTimeout",
        K::Protocol
        | K::MissingData
        | K::MetadataUnavailable
        | K::LimitExceeded
        | K::ResourceFormat
        | K::DataNotReady => "Protocol",
        K::ForkedProcess
        | K::Cancelled
        | K::CancelUnconfirmed
        | K::PinLost
        | K::Io
        | K::Internal => "Internal",
        _ => "Internal",
    };
    let mut context = error.context().to_vec();
    if error.kind() == K::InvalidArgument && !context.iter().any(|(key, _)| key == "python_kind") {
        context.push(("python_kind".to_owned(), "ValueError".to_owned()));
    }
    let message = if error.kind() == K::Protocol
        && context
            .iter()
            .any(|(key, value)| key == "limit" && value == "snapshot_missing")
        && error.message() == "legacy recovery request limit reached"
    {
        "snapshot batch recovery request budget exhausted (32)".to_owned()
    } else {
        error.message().to_owned()
    };
    NativeErrorPayload::new(kind, message, context).into_pyerr()
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("NativeError", module.py().get_type::<NativeError>())
}
