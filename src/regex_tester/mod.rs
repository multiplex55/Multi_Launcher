//! Local, Rust-compatible regular-expression tester domain.

pub mod model;

pub use model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchId, RegexDraft, RegexFlags,
    RegexMatch, RegexValidationError, SourceIndex, SourceLocation, SubstitutionEvaluationResult,
    SubstitutionResult,
};
