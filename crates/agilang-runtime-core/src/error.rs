use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RuntimeResult<T> = Result<T, AgilangError>;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    Ok = 0,
    InvalidArgument = 1,
    InvalidHandle = 2,
    TypeMismatch = 3,
    OutOfBounds = 4,
    Utf8 = 5,
    Io = 6,
    Serialization = 7,
    Timeout = 8,
    Cancelled = 9,
    PermissionDenied = 10,
    NotFound = 11,
    AlreadyExists = 12,
    Unsupported = 13,
    Overflow = 14,
    InvalidState = 15,
    Panic = 16,
    Internal = 255,
}

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[error("{message}")]
pub struct AgilangError {
    pub code: ErrorCode,
    pub message: String,
    pub context: Vec<(String, String)>,
}

impl AgilangError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: Vec::new(),
        }
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.push((key.into(), value.into()));
        self
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn invalid_handle(id: u64) -> Self {
        Self::new(
            ErrorCode::InvalidHandle,
            "invalid, stale, or released runtime handle",
        )
        .with_context("handle", id.to_string())
    }

    pub fn type_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::TypeMismatch,
            "runtime value has an incompatible type",
        )
        .with_context("expected", expected)
        .with_context("actual", actual)
    }

    pub fn out_of_bounds(index: usize, len: usize) -> Self {
        Self::new(ErrorCode::OutOfBounds, "collection index is out of bounds")
            .with_context("index", index.to_string())
            .with_context("length", len.to_string())
    }
}

impl From<std::io::Error> for AgilangError {
    fn from(value: std::io::Error) -> Self {
        let code = match value.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
            std::io::ErrorKind::TimedOut => ErrorCode::Timeout,
            _ => ErrorCode::Io,
        };
        Self::new(code, value.to_string())
    }
}
