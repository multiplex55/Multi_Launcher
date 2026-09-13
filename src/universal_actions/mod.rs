//! Surface-independent semantic actions for launcher targets.
//!
//! This module deliberately contains no egui or input-trigger concepts. Rich
//! action sets are intended to be resolved lazily by later provider work.

mod model;
mod provider;
mod providers;
mod registry;
mod resolver;
mod target;

pub use model::{
    ActionAvailability, ActionGroup, ActionIconKey, ActionId, ActionPresentation,
    ActionPresentationOverride, ActionPriority, ActionSafety, ActionSurface,
    EffectiveActionPresentation, NoteExternalEditor, UniversalAction, UniversalActionOperation,
    UniversalUiIntent, action_ids,
};
pub use provider::{ActionResolutionContext, PinCapability, UniversalActionProvider};
pub use registry::UniversalActionRegistry;
pub use resolver::{ActionTargetResolver, ActionTargetResolverContext, ResolvedActionTarget};
pub use target::{ActionTarget, PersistableActionTargetRef, PersistedUniversalActionRef};

#[cfg(test)]
mod provider_tests;
