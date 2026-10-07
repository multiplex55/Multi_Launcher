//! Local, Rust-compatible regular-expression tester domain.

pub mod engine;
pub mod examples;
pub mod explanation;
pub mod model;
pub mod persistence;
pub mod policy;
pub mod reference;
pub mod session;

pub use engine::{
    evaluate, evaluate_substitution, evaluate_substitution_with_policy, evaluate_with_policy,
};
pub use examples::{BUILT_IN_EXAMPLES, RegexExample};
pub use explanation::{Explanation, ExplanationKind, ExplanationResult, explain};
pub use model::{
    ByteSpan, CaptureGroup, CaptureValue, EvaluationResult, MatchCompleteness, MatchId, RegexDraft,
    RegexFlags, RegexMatch, RegexValidationError, SourceIndex, SourceLocation,
    SubstitutionEvaluationResult, SubstitutionResult,
};
pub use persistence::{
    HistoryEntry, HistoryStore, MAX_RECENT_REGEXES, PresetId, PresetInput, PresetStore,
    RegexPreset, RegexStoreError, StoreDiagnostic, StoreLoadStatus,
};
pub use policy::{EvaluationLimit, EvaluationPolicy, EvaluationSuspension};
pub use reference::{
    QUICK_REFERENCE, ReferenceCategory, ReferenceEntry, ReferenceExample, ReferenceSyntaxKind,
    search_reference,
};
pub use session::RegexSession;
