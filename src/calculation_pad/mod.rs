//! Pure bounded evaluation with explicit app-owned editing sessions and document storage.
//!
//! The expression grammar and evaluation limits are documented in this module's
//! `README.md`.

mod bit_labels;
mod calendar;
mod chart;
pub(crate) use chart::{ChartDefinition, ChartSource};
mod comparison;
mod dependencies;
mod document;
mod exact;
mod examples;
mod inspector;
mod input;
mod integer;
mod limits;
mod operators;
mod persistence;
mod programmer;
mod quantity;
mod session;
mod sensitivity;
mod vector;
mod worker;
pub use worker::EvaluationBasis;
pub use limits::{ABSOLUTE_BYTES as MAX_ADMITTED_DOCUMENT_BYTES, ABSOLUTE_LINES as MAX_ADMITTED_SOURCE_LINES};
pub(crate) use limits::profile_limits;
pub(crate) use document::{MAX_WORK, MAX_SEQUENCE_ELEMENTS, MAX_LOOP_ITERATIONS, MAX_LOOP_DEPTH, MAX_BLOCK_DEPTH};
pub(crate) use vector::{MAX_VECTOR_COMPONENTS, MAX_VECTOR_PARSE_WORK};
pub(crate) use exact::{MAX_BITS as MAX_EXACT_BITS, MAX_SCALE as MAX_EXACT_SCALE};
pub(crate) use session::{MAX_UNDO_BYTES, MAX_UNDO_ENTRIES};
pub(crate) use session::RenamePreview;
pub(crate) use limits::source_admitted;
pub use calendar::CalendarValue;

pub(crate) use document::syntax::{ProtectedKind, SourceAnalysis};

pub(crate) use bit_labels::{read_bit_labels, write_bit_labels};
pub use comparison::{
    ComparisonContext, ComparisonRelation, ComparisonRequest, ComparisonResult,
    ComparisonResultRef, ComparisonRow, ComparisonSide, ComparisonSlot,
};
pub use dependencies::{DependencyLocation, DependencyRow, DependencySnapshot};
pub use document::{
    CalculationDocument, CalculationEvaluation, CalculationLineResult, CalculationValue, EvaluationUsage,
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
pub use document::CalculationSequence;
pub(crate) use document::{literal_inputs, LiteralInput, watch_name, editor_bindings, editor_loop_header, editor_control_keyword, editor_function_scope, function_signature_help, FUNCTIONS};
pub(crate) use document::rename::{RenamePlan, RenameScope};

pub(crate) use persistence::{PadViewMetadata, PadInputReference, SensitivitySettings};
pub(crate) use input::{InputConfig, InputControl, InputKind, SampleNumber};
pub(crate) use sensitivity::{Sampling, SensitivityBasis, SensitivityDefinition, SensitivityRow, SensitivityStatus, SensitivityTable};
pub(crate) use chart::{ChartDataset, ChartHit, ChartKind, ChartPoint, ChartScene, ChartSeries, ChartView, ScenePrimitive};
pub(crate) use sensitivity::{TableExport, export_table, table_header, table_row};
pub(crate) use input::{step_value, slider_values, exact_grid, exact_grid_value, project as project_input};
pub use persistence::{
    CURRENT_DOCUMENT_FORMAT_VERSION, CURRENT_RECOVERY_FORMAT_VERSION, CalculationPolicy, DocumentOptions, DocumentProfile, DocumentId, LineId,
    MAX_SAVED_DOCUMENTS, MAX_STORED_DOCUMENT_BYTES, MAX_STORED_RECOVERY_BYTES, PadDocumentStore,
    PadDocumentSummary, PadRecoverySnapshot, PadRecoveryStore, PadRecoveryWriteFailure,
    PadRecoveryWriter, PadStoreError,
};
pub use session::{
    CalculationPadSession, EvaluationStatus, InspectorInsertError, PendingTransition, SessionError,
    SessionEvaluation, SessionLineResult, TransitionDecision, TransitionOutcome,
};
