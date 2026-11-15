use std::error::Error as StdError;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;
pub type ErrorContext = Vec<(String, String)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
pub struct Error {
    kind: ErrorKind,
    message: String,
    context: ErrorContext,
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: Vec::new(),
            source: None,
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
        self.source = Some(Box::new(source));
        self
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
