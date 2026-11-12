//! One error type for the whole crate. It serializes to the JSON error body that dcc prints.

use std::fmt;

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new("usage", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message)
    }
    pub fn external(message: impl Into<String>) -> Self {
        Self::new("external", message)
    }
    pub fn isolation(message: impl Into<String>) -> Self {
        Self::new("isolation", message)
    }
    pub fn io(message: impl Into<String>) -> Self {
        Self::new("io", message)
    }
    /// Process exit code for this kind of error.
    pub fn exit_code(&self) -> u8 {
        match self.kind {
            "usage" => 2,
            "not_found" => 3,
            _ => 1,
        }
    }
    pub fn to_json(&self) -> Value {
        json!({ "error": { "kind": self.kind, "message": self.message } })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::io(e.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("database", e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new("json", e.to_string())
    }
}
