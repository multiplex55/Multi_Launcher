//! Pure bounded evaluation with explicit app-owned editing sessions and document storage.
//!
//! The expression grammar and evaluation limits are documented in this module's
//! `README.md`.

mod bit_labels;
mod comparison;
mod dependencies;
mod document;
mod examples;
mod inspector;
mod integer;
mod operators;
mod persistence;
mod programmer;
mod quantity;
mod session;
mod vector;

pub(crate) use bit_labels::{read_bit_labels, write_bit_labels};
pub use comparison::{
    ComparisonContext, ComparisonRelation, ComparisonRequest, ComparisonResult,
    ComparisonResultRef, ComparisonRow, ComparisonSide, ComparisonSlot,
};
pub use dependencies::{DependencyLocation, DependencyRow, DependencySnapshot};
pub use document::{
    CalculationDocument, CalculationEvaluation, CalculationLineResult, CalculationValue,
    Diagnostic, DiagnosticKind, DocumentDiagnostic, EditError, LineOutcome, MAX_DEPENDENCIES,
    MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_NESTING,
    MAX_NAME_BYTES, MAX_SOURCE_LINES,
};
pub(crate) use examples::{CalculationExampleGroup, EXAMPLE_GROUPS};
pub use inspector::{
    DocumentIncarnation, InspectorAnnotation, InspectorError, InspectorFormatOptions,
    InspectorInsertRequest, InspectorSourceOrigin, InspectorSourceSnapshot, ProgrammerInspector,
};
pub use integer::{IntegerError, IntegerMode, IntegerSignedness, IntegerWidth, TypedInteger};
pub use programmer::ProgrammerError;
pub use quantity::{CalculatedQuantity, QuantityError};
pub use vector::CalculationVector;

pub use persistence::{
    CURRENT_DOCUMENT_FORMAT_VERSION, CURRENT_RECOVERY_FORMAT_VERSION, DocumentId, LineId,
    MAX_SAVED_DOCUMENTS, MAX_STORED_DOCUMENT_BYTES, MAX_STORED_RECOVERY_BYTES, PadDocumentStore,
    PadDocumentSummary, PadRecoverySnapshot, PadRecoveryStore, PadRecoveryWriteFailure,
    PadRecoveryWriter, PadStoreError,
};
pub use session::{
    CalculationPadSession, InspectorInsertError, PendingTransition, SessionError,
    SessionEvaluation, SessionLineResult, TransitionDecision, TransitionOutcome,
};
