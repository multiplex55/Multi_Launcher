//! Local, Rust-compatible regular-expression tester domain.

pub mod engine;
pub mod explanation;
pub mod model;
pub mod reference;

pub use engine::{evaluate, evaluate_substitution};
pub use explanation::{Explanation, ExplanationKind, ExplanationResult, explain};
pub use model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchId, RegexDraft, RegexFlags,
    RegexMatch, RegexValidationError, SourceIndex, SourceLocation, SubstitutionEvaluationResult,
    SubstitutionResult,
};
pub use reference::{
    QUICK_REFERENCE, ReferenceCategory, ReferenceEntry, ReferenceExample, ReferenceSyntaxKind,
    search_reference,
};
