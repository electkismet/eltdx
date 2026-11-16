use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Error>;
pub type ErrorContext = Vec<(String, String)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidArgument,
    Unsupported,
    Closed,
    ForkedProcess,
    Timeout,
    Cancelled,
    CancelUnconfirmed,
    CloseTimeout,
    PoolBusy,
    Connection,
    PinLost,
    Protocol,
    MissingData,
    MetadataUnavailable,
    LimitExceeded,
    PushGap,
    ResourceFormat,
    DataNotReady,
    Io,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TimeoutPhase {
    Startup,
    Admission,
    Queue,
    Resolve,
    Connect,
    Handshake,
    Send,
    Response,
    Decode,
    Aggregate,
    Retry,
    CancelConfirm,
    Close,
}

/// A language-neutral structured SDK error.
#[derive(Clone)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    context: ErrorContext,
    source: Option<Arc<dyn StdError + Send + Sync + 'static>>,
    cleanup_failure: Option<Arc<Error>>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: Vec::new(),
            source: None,
            cleanup_failure: None,
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
    }

    /// The cleanup failure accompanying a `CancelUnconfirmed` error.
    /// `source()` retains the original business cancellation/timeout cause.
    pub fn cleanup_failure(&self) -> Option<&Error> {
        self.cleanup_failure.as_deref()
    }

    pub(crate) fn with_cleanup_failure(mut self, failure: Error) -> Self {
        self.cleanup_failure = Some(Arc::new(failure));
        self
    }

    pub(crate) fn from_runtime(error: eltdx_runtime::RuntimeError) -> Self {
        use eltdx_runtime::RuntimeError as R;
        let context = error.context();
        let sdk_kind = context.iter().find(|(key, _)| key == "sdk_kind").map(|(_, value)| value.as_str());
        let kind = match sdk_kind {
            Some("Cancelled") => ErrorKind::Cancelled,
            Some("Closed") => ErrorKind::Closed,
            Some("LimitExceeded") => ErrorKind::LimitExceeded,
            _ => match &error {
                R::InvalidArgument { .. } => ErrorKind::InvalidArgument,
                R::Protocol { .. } => ErrorKind::Protocol,
                R::ConnectionClosed { .. } => ErrorKind::Connection,
                R::Timeout { .. } => ErrorKind::Timeout,
                R::PoolBusy { .. } => ErrorKind::PoolBusy,
                R::PushOverflow { .. } => ErrorKind::PushGap,
                R::CloseTimeout { .. } => ErrorKind::CloseTimeout,
                R::UnsupportedCommand { .. } => ErrorKind::Unsupported,
                R::Internal { .. } => ErrorKind::Internal,
            },
        };
        let mut mapped = Self::new(kind, error.to_string());
        mapped.context = context;
        mapped.with_source(error)
    }

    pub(crate) fn from_protocol(error: eltdx_protocol::ProtocolError) -> Self {
        let kind = match &error {
            eltdx_protocol::ProtocolError::InvalidArgument { .. } => ErrorKind::InvalidArgument,
            eltdx_protocol::ProtocolError::LimitExceeded { .. } => ErrorKind::LimitExceeded,
            eltdx_protocol::ProtocolError::UnsupportedCommand { .. } => ErrorKind::Unsupported,
            _ => ErrorKind::Protocol,
        };
        Self::new(kind, error.to_string()).with_source(error)
    }

    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn context(&self) -> &[(String, String)] {
        &self.context
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Error")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .field("context", &self.context)
            .field("source", &self.source.as_deref())
            .field("cleanup_failure", &self.cleanup_failure)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}
