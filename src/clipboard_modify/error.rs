use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardModifyError {
    MissingArgument {
        operation: String,
        argument: &'static str,
    },
    UnexpectedArgument {
        operation: String,
        argument: &'static str,
    },
    MissingPlaceholder {
        template: String,
    },
    ReservedName {
        name: String,
    },
    DuplicateName {
        name: String,
    },
    UnknownOperation {
        operation: String,
    },
    UnknownTemplate {
        name: String,
    },
    NestedPipeline {
        name: String,
    },
    Transform(String),
    Cancelled,
}

impl fmt::Display for ClipboardModifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument {
                operation,
                argument,
            } => write!(f, "{operation} requires {argument}"),
            Self::UnexpectedArgument {
                operation,
                argument,
            } => write!(f, "{operation} does not accept {argument}"),
            Self::MissingPlaceholder { template } => {
                write!(f, "template {template} must contain {{{{clipboard}}}}")
            }
            Self::ReservedName { name } => write!(f, "{name} is reserved"),
            Self::DuplicateName { name } => write!(f, "{name} is duplicated"),
            Self::UnknownOperation { operation } => write!(f, "unknown operation {operation}"),
            Self::UnknownTemplate { name } => write!(f, "unknown template {name}"),
            Self::NestedPipeline { name } => {
                write!(f, "nested saved pipeline {name} is not supported")
            }
            Self::Transform(reason) => write!(f, "{reason}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for ClipboardModifyError {}
