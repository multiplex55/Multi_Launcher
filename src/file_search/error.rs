use std::fmt;
use std::path::PathBuf;

/// User-facing categories for file search failures and warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSearchError {
    InvalidQuery {
        message: String,
    },
    InvalidDirectory {
        path: PathBuf,
        message: String,
    },
    BackendUnavailable {
        backend: String,
        message: String,
    },
    ProcessLaunchFailure {
        executable: PathBuf,
        message: String,
    },
    ProcessFatalStatus {
        executable: PathBuf,
        message: String,
    },
    ProcessOutputParseFailure {
        backend: String,
        message: String,
    },
    CancelledSearch,
    TraversalWarning {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for FileSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery { message } => write!(f, "Invalid search query: {message}"),
            Self::InvalidDirectory { path, message } => {
                write!(
                    f,
                    "Invalid search directory '{}': {message}",
                    path.display()
                )
            }
            Self::BackendUnavailable { backend, message } => {
                write!(f, "Search backend '{backend}' is unavailable: {message}")
            }
            Self::ProcessLaunchFailure {
                executable,
                message,
            } => {
                write!(f, "Failed to launch '{}': {message}", executable.display())
            }
            Self::ProcessFatalStatus {
                executable,
                message,
            } => {
                write!(
                    f,
                    "Search process '{}' returned a fatal status: {message}",
                    executable.display()
                )
            }
            Self::ProcessOutputParseFailure { backend, message } => {
                write!(f, "Failed to parse output from '{backend}': {message}")
            }
            Self::CancelledSearch => write!(f, "Search was cancelled"),
            Self::TraversalWarning { path, message } => {
                write!(
                    f,
                    "Could not fully traverse '{}': {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for FileSearchError {}
