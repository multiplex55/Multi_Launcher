//! Egui compatibility adapter for the shared Jump Mode model.

pub(crate) mod config;
pub(crate) mod paint;
pub(crate) mod ui;

pub use crate::launcher::jump_mode::{
    DuplicateTargetId, Hint, HintAssignment, HintMap, HintSession, MatchOutcome,
    PopulationChange, SemanticTargetId, SessionLifecycle, SurfaceId, TargetGeneration,
    TargetId, TargetKind, TargetMetadata,
};
