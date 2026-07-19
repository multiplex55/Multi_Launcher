pub mod case;
pub mod development;
pub mod lines;
pub mod newlines;
pub mod whitespace;
pub mod words;
pub mod wrappers;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextTransformError {
    InvalidJson(String),
    InvalidJsonEscape(String),
    InvalidPercentEncoding(String),
    InvalidUtf8(String),
    InvalidBase64(String),
    InvalidCsvLikeInput(String),
    InvalidLanguageIdentifier(String),
    InvalidWrapperArguments(String),
    Cancelled,
    Validation { operation: String, message: String },
}

impl fmt::Display for TextTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(s) => write!(f, "invalid JSON: {s}"),
            Self::InvalidJsonEscape(s) => write!(f, "invalid JSON escape: {s}"),
            Self::InvalidPercentEncoding(s) => write!(f, "invalid percent encoding: {s}"),
            Self::InvalidUtf8(s) => write!(f, "invalid UTF-8 after decoding: {s}"),
            Self::InvalidBase64(s) => write!(f, "invalid Base64: {s}"),
            Self::InvalidCsvLikeInput(s) => write!(f, "invalid CSV-like input: {s}"),
            Self::InvalidLanguageIdentifier(s) => write!(f, "invalid language identifier: {s}"),
            Self::InvalidWrapperArguments(s) => write!(f, "invalid wrapper arguments: {s}"),
            Self::Cancelled => write!(f, "operation cancelled"),
            Self::Validation { operation, message } => {
                write!(f, "{operation} validation failed: {message}")
            }
        }
    }
}

impl std::error::Error for TextTransformError {}
