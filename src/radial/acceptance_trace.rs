//! Bounded, opt-in tracing for one radial acceptance run.
//!
//! The trace deliberately exposes only typed control-flow facts.  In
//! particular, it must never carry menu names, notes, clipboard contents,
//! arbitrary key values, window titles, or other user payload.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const ENVIRONMENT_VARIABLE: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
pub(crate) const EVENT_BUDGET: usize = 256;
const TRACE_TARGET: &str = "multi_launcher.radial_acceptance";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RequestKind {
    #[default]
    None,
    Snapshot,
    CommitApply,
    CommitSave,
    CommitRevertAndClose,
    LivePreview,
    CancelPreview,
    ExportPackage,
    ExportSkin,
    AuditionManagedAsset,
    FontCatalog,
    PrepareEmbeddedPreview,
    ReplacePackage,
    StartNativePreview,
    UpdateNativePreview,
    StopNativePreview,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Correlation {
    pub(crate) request_id: u64,
    pub(crate) request_kind: RequestKind,
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewportClass {
    Root,
    Deferred,
    Immediate,
    Embedded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallbackPhase {
    Enter,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusEdge {
    Requested,
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BodyBlock {
    Enabled,
    InitialSnapshot,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WidgetCategory {
    Canvas,
    DesignerBody,
    Menus,
    Skins,
    Tree,
    Inspector,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WidgetResponse {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutationResult {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthoringEdge {
    RequestSent,
    ReplyTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimaryTransition {
    Press,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VisibilitySource {
    ToggleBatch,
    LegacyTrigger,
    Queued,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootCommandKind {
    Position,
    Size,
    Show,
    Minimize,
    Focus,
    ParkingBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestoreEdge {
    RestoreFlag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    DesignerCallback {
        phase: CallbackPhase,
        viewport: ViewportClass,
    },
    DesignerFocus {
        edge: FocusEdge,
        viewport: ViewportClass,
        correlation: Correlation,
    },
    DesignerPointer {
        down: bool,
        up: bool,
        correlation: Correlation,
    },
    DesignerBody {
        state: BodyBlock,
        correlation: Correlation,
    },
    DesignerWidget {
        category: WidgetCategory,
        response: WidgetResponse,
        correlation: Correlation,
    },
    DesignerMutation {
        result: MutationResult,
        correlation: Correlation,
    },
    Authoring {
        edge: AuthoringEdge,
        correlation: Correlation,
    },
    InvocationPrimary {
        transition: PrimaryTransition,
        provenance: crate::radial::invocation::InputProvenance,
        modifiers_match: bool,
        invocation_id: u64,
        generation: u64,
    },
    ShortTap {
        invocation_id: u64,
        terminal: bool,
    },
    DesiredVisibility {
        visible: bool,
        source: VisibilitySource,
    },
    RootCommand {
        command: RootCommandKind,
    },
    Restore {
        edge: RestoreEdge,
    },
}

struct EventBudget {
    limit: usize,
    emitted: AtomicUsize,
}

impl EventBudget {
    const fn new(limit: usize) -> Self {
        Self {
            limit,
            emitted: AtomicUsize::new(0),
        }
    }

    fn reserve(&self) -> bool {
        let mut current = self.emitted.load(Ordering::Relaxed);
        loop {
            if current >= self.limit {
                return false;
            }
            match self.emitted.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }

    #[cfg(test)]
    fn emitted(&self) -> usize {
        self.emitted.load(Ordering::Relaxed)
    }
}

struct Runtime {
    enabled: bool,
    budget: EventBudget,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn enabled_from_value(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "on")
    )
}

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime {
        enabled: enabled_from_value(std::env::var(ENVIRONMENT_VARIABLE).ok().as_deref()),
        budget: EventBudget::new(EVENT_BUDGET),
    })
}

pub(crate) fn enabled() -> bool {
    runtime().enabled
}

pub(crate) fn emit(event: Event) {
    let runtime = runtime();
    if !runtime.enabled || !runtime.budget.reserve() {
        return;
    }

    match event {
        Event::DesignerCallback { phase, viewport } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_callback",
                ?phase,
                ?viewport,
                "radial acceptance trace"
            );
        }
        Event::DesignerFocus {
            edge,
            viewport,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_focus",
                ?edge,
                ?viewport,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerPointer {
            down,
            up,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_pointer",
                pointer_down = down,
                pointer_up = up,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerBody { state, correlation } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_body",
                ?state,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerWidget {
            category,
            response,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_widget",
                ?category,
                ?response,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerMutation {
            result,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_mutation",
                ?result,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::Authoring { edge, correlation } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "authoring",
                ?edge,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::InvocationPrimary {
            transition,
            provenance,
            modifiers_match,
            invocation_id,
            generation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "configured_primary",
                ?transition,
                ?provenance,
                modifiers_match,
                invocation_id,
                generation,
                "radial acceptance trace"
            );
        }
        Event::ShortTap {
            invocation_id,
            terminal,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "short_tap",
                invocation_id,
                terminal,
                "radial acceptance trace"
            );
        }
        Event::DesiredVisibility { visible, source } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "desired_visibility",
                visible,
                ?source,
                "radial acceptance trace"
            );
        }
        Event::RootCommand { command } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "root_command",
                ?command,
                "radial acceptance trace"
            );
        }
        Event::Restore { edge } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "restore",
                ?edge,
                "radial acceptance trace"
            );
        }
    }
}

pub(crate) struct DesignerCallbackGuard {
    viewport: ViewportClass,
}

impl DesignerCallbackGuard {
    pub(crate) fn enter(viewport: ViewportClass) -> Self {
        emit(Event::DesignerCallback {
            phase: CallbackPhase::Enter,
            viewport,
        });
        Self { viewport }
    }
}

impl Drop for DesignerCallbackGuard {
    fn drop(&mut self) {
        emit(Event::DesignerCallback {
            phase: CallbackPhase::Exit,
            viewport: self.viewport,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_switch_is_disabled_when_unset_or_false() {
        assert!(!enabled_from_value(None));
        assert!(!enabled_from_value(Some("0")));
        assert!(!enabled_from_value(Some("false")));
        assert!(enabled_from_value(Some("1")));
        assert!(enabled_from_value(Some("true")));
    }

    #[test]
    fn event_budget_is_hard_bounded() {
        let budget = EventBudget::new(2);
        assert!(budget.reserve());
        assert!(budget.reserve());
        assert!(!budget.reserve());
        assert_eq!(budget.emitted(), 2);
    }

    #[test]
    fn event_schema_has_no_payload_surface() {
        let event = Event::DesignerPointer {
            down: true,
            up: false,
            correlation: Correlation::default(),
        };
        assert!(!format!("{event:?}").contains("payload"));
    }
}
