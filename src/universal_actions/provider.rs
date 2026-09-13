use crate::universal_actions::{ActionSurface, ResolvedActionTarget, UniversalAction};

/// Pin state already resolved by the caller from its in-memory/persisted pin
/// boundary. Providers never load or mutate pin storage themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PinCapability {
    #[default]
    Unsupported,
    Writable {
        is_pinned: bool,
    },
    ReadOnly {
        is_pinned: bool,
        reason: String,
    },
}

/// Narrow, read-only data used while discovering actions for one target.
///
/// Constructing this value performs no I/O. Runtime catalogs and mutable
/// application state deliberately remain outside the provider boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionResolutionContext<'a> {
    pub surface: ActionSurface,
    pub query: &'a str,
    pub pin: PinCapability,
    pub can_add_favorite: bool,
    /// Snapshot of the selected live timer's pause state. `None` means the
    /// timer no longer exists (or this is not a timer target).
    pub timer_paused: Option<bool>,
    /// Snapshot of the selected live stopwatch's pause state. `None` means
    /// the stopwatch no longer exists (or this is not a stopwatch target).
    pub stopwatch_paused: Option<bool>,
}

impl<'a> ActionResolutionContext<'a> {
    pub fn new(surface: ActionSurface, query: &'a str) -> Self {
        Self {
            surface,
            query,
            pin: PinCapability::Unsupported,
            can_add_favorite: false,
            timer_paused: None,
            stopwatch_paused: None,
        }
    }
}

/// Pure capability discovery for a resolved launcher target.
pub trait UniversalActionProvider: Sync {
    fn actions(
        &self,
        resolved: &ResolvedActionTarget,
        context: &ActionResolutionContext<'_>,
    ) -> Vec<UniversalAction>;
}
