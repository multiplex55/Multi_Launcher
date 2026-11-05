//! Pure bounded evaluation with explicit app-owned editing sessions and document storage.
//!
//! The expression grammar and evaluation limits are documented in this module's
//! `README.md`.

mod document;
mod examples;
mod integer;
mod operators;
mod persistence;
mod programmer;
mod session;

pub use document::{
    CalculationDocument, CalculationEvaluation, CalculationLineResult, CalculationValue,
    Diagnostic, DiagnosticKind, DocumentDiagnostic, EditError, LineOutcome, MAX_DEPENDENCIES,
    MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_NESTING,
    MAX_NAME_BYTES, MAX_SOURCE_LINES,
};
pub(crate) use examples::{CalculationExampleGroup, EXAMPLE_GROUPS};
pub use integer::{IntegerError, IntegerMode, IntegerSignedness, IntegerWidth, TypedInteger};
pub use programmer::ProgrammerError;

pub use persistence::{
    CURRENT_DOCUMENT_FORMAT_VERSION, CURRENT_RECOVERY_FORMAT_VERSION, DocumentId, LineId,
    MAX_SAVED_DOCUMENTS, MAX_STORED_DOCUMENT_BYTES, MAX_STORED_RECOVERY_BYTES, PadDocumentStore,
    PadDocumentSummary, PadRecoverySnapshot, PadRecoveryStore, PadRecoveryWriteFailure,
    PadRecoveryWriter, PadStoreError,
};
pub use session::{
    CalculationPadSession, PendingTransition, SessionError, SessionEvaluation, SessionLineResult,
    TransitionDecision, TransitionOutcome,
};
