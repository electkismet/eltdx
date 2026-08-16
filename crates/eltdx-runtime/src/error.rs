use std::fmt;

use eltdx_protocol::ProtocolError;
use thiserror::Error;

pub type ErrorContext = Vec<(String, String)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutPhase {
    Admission,
    Queue,
    Startup,
    Endpoint,
    Connect,
    Handshake,
    Send,
    Response,
    Retry,
    Pin,
    Heartbeat,
    PushPoll,
    CancelConfirmation,
    Close,
}

impl TimeoutPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Queue => "queue",
            Self::Startup => "startup",
            Self::Endpoint => "endpoint",
            Self::Connect => "connect",
            Self::Handshake => "handshake",
            Self::Send => "send",
            Self::Response => "response",
            Self::Retry => "retry",
            Self::Pin => "pin",
            Self::Heartbeat => "heartbeat",
            Self::PushPoll => "push_poll",
            Self::CancelConfirmation => "cancel_confirmation",
            Self::Close => "close",
        }
    }
}

impl fmt::Display for TimeoutPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RuntimeError {
    #[error("{message}")]
    InvalidArgument {
        python_kind: String,
        message: String,
        context: ErrorContext,
    },

    #[error("{message}")]
    Protocol {
        code: String,
        message: String,
        context: ErrorContext,
    },

    #[error("{message}")]
    ConnectionClosed {
        message: String,
        context: ErrorContext,
    },

    #[error("{message}")]
    Timeout {
        phase: TimeoutPhase,
        message: String,
        context: ErrorContext,
    },

    #[error("{message}")]
    PoolBusy {
        message: String,
        capacity: usize,
        context: ErrorContext,
    },

    #[error("7709 push buffer overflow; {dropped_total} frame(s) dropped")]
    PushOverflow {
        dropped_total: u64,
        context: ErrorContext,
    },

    #[error("{message}")]
    CloseTimeout {
        message: String,
        context: ErrorContext,
    },

    #[error("unsupported command: 0x{command:04x}")]
    UnsupportedCommand { command: u16, context: ErrorContext },

    #[error("{message}")]
    Internal {
        message: String,
        context: ErrorContext,
    },
}

impl RuntimeError {
    pub fn invalid_argument(python_kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            python_kind: python_kind.into(),
            message: message.into(),
            context: Vec::new(),
        }
    }

    pub fn timeout(phase: TimeoutPhase) -> Self {
        Self::Timeout {
            phase,
            message: format!("7709 response timed out during {phase}"),
            context: Vec::new(),
        }
    }

    pub fn connection_closed(message: impl Into<String>) -> Self {
        Self::ConnectionClosed {
            message: message.into(),
            context: Vec::new(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            context: Vec::new(),
        }
    }

    pub fn unsupported_command(command: u16) -> Self {
        Self::UnsupportedCommand {
            command,
            context: Vec::new(),
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::InvalidArgument { .. } => "InvalidArgument",
            Self::Protocol { .. } => "Protocol",
            Self::ConnectionClosed { .. } => "ConnectionClosed",
            Self::Timeout { .. } => "Timeout",
            Self::PoolBusy { .. } => "PoolBusy",
            Self::PushOverflow { .. } => "PushOverflow",
            Self::CloseTimeout { .. } => "CloseTimeout",
            Self::UnsupportedCommand { .. } => "UnsupportedCommand",
            Self::Internal { .. } => "Internal",
        }
    }

    pub fn context(&self) -> ErrorContext {
        match self {
            Self::InvalidArgument {
                python_kind,
                context,
                ..
            } => with_field(context, "python_kind", python_kind.clone()),
            Self::Protocol { code, context, .. } => with_field(context, "code", code.clone()),
            Self::ConnectionClosed { context, .. }
            | Self::CloseTimeout { context, .. }
            | Self::Internal { context, .. } => context.clone(),
            Self::Timeout { phase, context, .. } => with_field(context, "phase", phase.to_string()),
            Self::PoolBusy {
                capacity, context, ..
            } => with_field(context, "capacity", capacity.to_string()),
            Self::PushOverflow {
                dropped_total,
                context,
            } => with_field(context, "dropped_total", dropped_total.to_string()),
            Self::UnsupportedCommand {
                command, context, ..
            } => with_field(context, "command", format!("0x{command:04x}")),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if let Some(context) = self.context_mut() {
            context.push((key.into(), value.into()));
        }
        self
    }

    fn context_mut(&mut self) -> Option<&mut ErrorContext> {
        match self {
            Self::InvalidArgument { context, .. }
            | Self::Protocol { context, .. }
            | Self::ConnectionClosed { context, .. }
            | Self::Timeout { context, .. }
            | Self::PoolBusy { context, .. }
            | Self::PushOverflow { context, .. }
            | Self::CloseTimeout { context, .. }
            | Self::UnsupportedCommand { context, .. }
            | Self::Internal { context, .. } => Some(context),
        }
    }
}

impl From<ProtocolError> for RuntimeError {
    fn from(error: ProtocolError) -> Self {
        let message = error.to_string();
        match error {
            ProtocolError::InvalidArgument { name, .. } => Self::InvalidArgument {
                python_kind: "ValueError".to_owned(),
                message,
                context: vec![("name".to_owned(), name.to_owned())],
            },
            ProtocolError::UnsupportedCommand { command } => Self::unsupported_command(command),
            ProtocolError::UnexpectedEof {
                context,
                offset,
                needed,
                remaining,
            } => Self::Protocol {
                code: "unexpected_eof".to_owned(),
                message,
                context: vec![
                    ("context".to_owned(), context.to_owned()),
                    ("offset".to_owned(), offset.to_string()),
                    ("needed".to_owned(), needed.to_string()),
                    ("remaining".to_owned(), remaining.to_string()),
                ],
            },
            ProtocolError::InvalidData { context, .. } => Self::Protocol {
                code: "invalid_data".to_owned(),
                message,
                context: vec![("context".to_owned(), context.to_owned())],
            },
            ProtocolError::LimitExceeded {
                resource,
                actual,
                limit,
            } => Self::Protocol {
                code: "limit_exceeded".to_owned(),
                message,
                context: vec![
                    ("resource".to_owned(), resource.to_owned()),
                    ("actual".to_owned(), actual.to_string()),
                    ("limit".to_owned(), limit.to_string()),
                ],
            },
            ProtocolError::LengthMismatch {
                field,
                expected,
                actual,
            } => Self::Protocol {
                code: "length_mismatch".to_owned(),
                message,
                context: vec![
                    ("field".to_owned(), field.to_owned()),
                    ("expected".to_owned(), expected.to_string()),
                    ("actual".to_owned(), actual.to_string()),
                ],
            },
            ProtocolError::Compression { .. } => Self::Protocol {
                code: "compression".to_owned(),
                message,
                context: Vec::new(),
            },
        }
    }
}

fn with_field(context: &ErrorContext, key: &str, value: String) -> ErrorContext {
    let mut result = Vec::with_capacity(context.len().saturating_add(1));
    result.extend(context.iter().cloned());
    result.push((key.to_owned(), value));
    result
}

#[cfg(test)]
mod tests {
    use eltdx_protocol::ProtocolError;

    use super::{RuntimeError, TimeoutPhase};

    #[test]
    fn timeout_exposes_stable_kind_phase_and_message() {
        let error = RuntimeError::timeout(TimeoutPhase::Handshake).with_context("slot", "2");

        assert_eq!(error.kind(), "Timeout");
        assert_eq!(
            error.to_string(),
            "7709 response timed out during handshake"
        );
        assert_eq!(
            error.context(),
            vec![
                ("slot".to_owned(), "2".to_owned()),
                ("phase".to_owned(), "handshake".to_owned()),
            ]
        );
    }

    #[test]
    fn protocol_context_survives_runtime_mapping() {
        let error = RuntimeError::from(ProtocolError::UnexpectedEof {
            context: "frame header",
            offset: 4,
            needed: 12,
            remaining: 3,
        });

        assert_eq!(error.kind(), "Protocol");
        assert_eq!(
            error.context(),
            vec![
                ("context".to_owned(), "frame header".to_owned()),
                ("offset".to_owned(), "4".to_owned()),
                ("needed".to_owned(), "12".to_owned()),
                ("remaining".to_owned(), "3".to_owned()),
                ("code".to_owned(), "unexpected_eof".to_owned()),
            ]
        );
    }

    #[test]
    fn unsupported_command_remains_a_distinct_runtime_error() {
        let error = RuntimeError::from(ProtocolError::UnsupportedCommand { command: 0x9999 })
            .with_context("registry", "missing");

        assert_eq!(error.kind(), "UnsupportedCommand");
        assert_eq!(error.to_string(), "unsupported command: 0x9999");
        assert_eq!(
            error.context(),
            vec![
                ("registry".to_owned(), "missing".to_owned()),
                ("command".to_owned(), "0x9999".to_owned()),
            ]
        );
    }
}
