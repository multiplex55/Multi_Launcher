//! Pure bounded evaluation with explicit app-owned editing sessions and document storage.
//!
//! The expression grammar and evaluation limits are documented in this module's
//! `README.md`.

mod document;
mod examples;
mod operators;
mod persistence;
mod session;

pub use document::{
    CalculationDocument, CalculationEvaluation, CalculationLineResult, CalculationValue,
    Diagnostic, DiagnosticKind, DocumentDiagnostic, EditError, LineOutcome, MAX_DEPENDENCIES,
    MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_NESTING,
    MAX_NAME_BYTES, MAX_SOURCE_LINES,
};
pub(crate) use examples::{CalculationExampleGroup, EXAMPLE_GROUPS};

pub use persistence::{
    CURRENT_DOCUMENT_FORMAT_VERSION, DocumentId, LineId, MAX_SAVED_DOCUMENTS,
    MAX_STORED_DOCUMENT_BYTES, PadDocumentStore, PadDocumentSummary, PadStoreError,
};
pub use session::{
    CalculationPadSession, PendingTransition, SessionError, SessionEvaluation, SessionLineResult,
    TransitionDecision, TransitionOutcome,
};
