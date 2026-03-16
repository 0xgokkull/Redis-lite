use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    EmptyInput,
    UnknownCommand,
    InvalidArgs,
    Io,
    Json,
    Config,
    FormatVersion,
}

#[derive(Debug)]
pub enum AppError {
    EmptyInput,
    UnknownCommand(String),
    InvalidArgs {
        command: String,
        expected: &'static str,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    Config(String),
    FormatVersion(String),
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::EmptyInput => ErrorCode::EmptyInput,
            Self::UnknownCommand(_) => ErrorCode::UnknownCommand,
            Self::InvalidArgs { .. } => ErrorCode::InvalidArgs,
            Self::Io(_) => ErrorCode::Io,
            Self::Json(_) => ErrorCode::Json,
            Self::Config(_) => ErrorCode::Config,
            Self::FormatVersion(_) => ErrorCode::FormatVersion,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "error: command cannot be empty"),
            Self::UnknownCommand(cmd) => write!(f, "error: unknown command '{cmd}'"),
            Self::InvalidArgs { command, expected } => {
                write!(f, "error: {command} expects {expected}")
            }
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Json(e) => write!(f, "json error: {e}"),
            Self::Config(message) => write!(f, "config error: {message}"),
            Self::FormatVersion(message) => write!(f, "format error: {message}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
