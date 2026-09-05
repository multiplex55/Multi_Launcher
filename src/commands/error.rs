use std::fmt;

/// A failure to decode a claimed launcher command protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandError {
    pub domain: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn new(domain: &'static str, message: impl Into<String>) -> Self {
        Self {
            domain,
            message: message.into(),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} command: {}", self.domain, self.message)
    }
}

impl std::error::Error for CommandError {}
