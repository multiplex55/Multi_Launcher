//! Shared bounded XSD validation presentation and current-request checks.
use crate::xml_toolkit_engine::{XmlDiagnostic, XmlEngineError};
use crate::xml_toolkit_session::{
    XmlParseStatus, XmlSchemaSessionState, XmlSessionSnapshot, XmlTreeStatus,
};
const MAX_SCHEMA_PATH_CHARS: usize = 512;
pub const MAX_SCHEMA_DIAGNOSTIC_CHARS: usize = 4096;

pub fn request_is_current(
    schema: &XmlSchemaSessionState,
    snapshot: &XmlSessionSnapshot,
    request: &crate::xml_toolkit_session::XmlSchemaValidationRequest,
) -> bool {
    snapshot.revision == Some(request.source_revision)
        && snapshot.parse == XmlParseStatus::Ready
        && matches!(&snapshot.tree, XmlTreeStatus::Ready(tree) if tree.document == request.document)
        && schema.selected_path.as_ref() == Some(&request.schema_path)
        && schema.revision == request.schema_revision
}

pub fn schema_diagnostic_message(diagnostic: &XmlDiagnostic) -> String {
    let location = match (diagnostic.line, diagnostic.column) {
        (Some(line), Some(column)) => format!(" at XML line {line}, column {column}"),
        (Some(line), None) => format!(" at XML line {line}"),
        (None, Some(column)) => format!(" at XML column {column}"),
        (None, None) => String::new(),
    };
    let message = bounded_message(&diagnostic.message);
    if location.is_empty() {
        message
    } else {
        format!("{location}: {message}")
    }
}

pub fn schema_error_message(error: &XmlEngineError) -> String {
    match error {
        XmlEngineError::SchemaLoad(diagnostic) => {
            let location = match (diagnostic.line, diagnostic.column) {
                (Some(line), Some(column)) => {
                    format!(" (schema graph line {line}, column {column})")
                }
                (Some(line), None) => format!(" (schema graph line {line})"),
                (None, Some(column)) => format!(" (schema graph column {column})"),
                (None, None) => String::new(),
            };
            let message = bounded_message(&diagnostic.message);
            if location.is_empty() {
                message
            } else {
                format!("{location}: {message}")
            }
        }
        XmlEngineError::SchemaPolicy(message) => {
            format!("XSD resolution policy: {}", bounded_message(message))
        }
        XmlEngineError::Io(message) => {
            format!("local XSD file access: {}", bounded_message(message))
        }
        XmlEngineError::SchemaBackend(message) => {
            format!("MSXML backend: {}", bounded_message(message))
        }
        XmlEngineError::UnsupportedPlatform => "this platform is unsupported".into(),
        XmlEngineError::WorkerUnavailable => "XML worker is unavailable".into(),
        other => bounded_message(&format!("{other:?}")),
    }
}

pub fn bounded_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    bounded_message_with_limit(&text, MAX_SCHEMA_PATH_CHARS)
}

fn bounded_message(message: &str) -> String {
    bounded_message_with_limit(message, MAX_SCHEMA_DIAGNOSTIC_CHARS)
}

fn bounded_message_with_limit(message: &str, limit: usize) -> String {
    let mut chars = message.chars();
    let prefix: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{prefix}… (display clipped)")
    } else {
        prefix
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaStatusTone {
    Weak,
    Normal,
    Error,
}

/// Current validation status, with stale results reduced to NotRun.
pub fn status_message(snapshot: &XmlSessionSnapshot) -> Option<(String, SchemaStatusTone)> {
    use crate::xml_toolkit_session::XmlSchemaStatus;
    let schema = &snapshot.schema;
    let not_run = || {
        Some((
            "Validation has not been run for the current XML and XSD.".into(),
            SchemaStatusTone::Weak,
        ))
    };
    match &schema.status {
        XmlSchemaStatus::NotSelected => None,
        XmlSchemaStatus::NotRun => not_run(),
        XmlSchemaStatus::Pending(request) if request_is_current(schema, snapshot, request) => {
            Some((
                "Validating the current XML against the selected local XSD…".into(),
                SchemaStatusTone::Weak,
            ))
        }
        XmlSchemaStatus::Valid(request) if request_is_current(schema, snapshot, request) => Some((
            "XML is valid against the selected XSD.".into(),
            SchemaStatusTone::Normal,
        )),
        XmlSchemaStatus::SchemaViolation {
            request,
            diagnostic,
        } if request_is_current(schema, snapshot, request) => Some((
            format!(
                "XML instance violates the selected XSD: {}",
                schema_diagnostic_message(diagnostic)
            ),
            SchemaStatusTone::Error,
        )),
        XmlSchemaStatus::SchemaLoadParseResolveCompileError { request, error }
            if request_is_current(schema, snapshot, request) =>
        {
            Some((
                format!(
                    "The selected XSD schema graph could not be loaded, resolved, or compiled: {}",
                    schema_error_message(error)
                ),
                SchemaStatusTone::Error,
            ))
        }
        XmlSchemaStatus::Unavailable { request, error }
            if request_is_current(schema, snapshot, request) =>
        {
            Some((
                format!(
                    "XSD validation is unavailable: {}",
                    schema_error_message(error)
                ),
                SchemaStatusTone::Error,
            ))
        }
        _ => not_run(),
    }
}
