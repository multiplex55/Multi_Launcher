//! Pure, bounded evaluation for the Calculation Pad document.
//!
//! The expression grammar and evaluation limits are documented in this module's
//! `README.md`.

mod document;
mod operators;

pub use document::{
    CalculationDocument, CalculationEvaluation, CalculationLineResult, CalculationValue,
    Diagnostic, DiagnosticKind, DocumentDiagnostic, EditError, LineOutcome, MAX_DEPENDENCIES,
    MAX_DEPENDENCIES_PER_LINE, MAX_DOCUMENT_BYTES, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_NESTING,
    MAX_NAME_BYTES, MAX_SOURCE_LINES,
};
