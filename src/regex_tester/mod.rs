//! Local, Rust-compatible regular-expression tester domain.

pub mod engine;
pub mod examples;
pub mod explanation;
pub mod model;
pub mod persistence;
pub mod reference;

pub use engine::{evaluate, evaluate_substitution};
pub use examples::{BUILT_IN_EXAMPLES, RegexExample};
pub use explanation::{Explanation, ExplanationKind, ExplanationResult, explain};
pub use model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchId, RegexDraft, RegexFlags,
    RegexMatch, RegexValidationError, SourceIndex, SourceLocation, SubstitutionEvaluationResult,
    SubstitutionResult,
};
pub use persistence::{
    HistoryEntry, HistoryStore, MAX_RECENT_REGEXES, PresetId, PresetInput, PresetStore,
    RegexPreset, RegexStoreError, StoreDiagnostic, StoreLoadStatus,
};
pub use reference::{
    QUICK_REFERENCE, ReferenceCategory, ReferenceEntry, ReferenceExample, ReferenceSyntaxKind,
    search_reference,
};
