use std::fmt;

/// A failure to decode or execute a launcher command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandError {
    pub domain: &'static str,
    pub message: String,
    pub toast: bool,
    pub refocus: bool,
    pub favorite: Option<String>,
}

impl CommandError {
    pub fn new(domain: &'static str, message: impl Into<String>) -> Self {
        Self {
            domain,
            message: message.into(),
            toast: false,
            refocus: false,
            favorite: None,
        }
    }

    pub fn with_refocus_policy(mut self) -> Self {
        self.refocus = true;
        self
    }

    pub fn with_gui_failure_policy(mut self) -> Self {
        self.toast = true;
        self.refocus = true;
        self
    }
    pub fn with_favorite(mut self, label: String) -> Self {
        self.favorite = Some(label);
        self
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} command: {}", self.domain, self.message)
    }
}

impl std::error::Error for CommandError {}
