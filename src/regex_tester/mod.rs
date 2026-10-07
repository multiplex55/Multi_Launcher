//! Local, Rust-compatible regular-expression tester domain.

pub mod engine;
pub mod model;

pub use engine::{evaluate, evaluate_substitution};
pub use model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchId, RegexDraft, RegexFlags,
    RegexMatch, RegexValidationError, SourceIndex, SourceLocation, SubstitutionEvaluationResult,
    SubstitutionResult,
};
