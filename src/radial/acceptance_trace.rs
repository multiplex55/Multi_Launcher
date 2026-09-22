//! Bounded, opt-in tracing for one radial acceptance run.
//!
//! The trace deliberately exposes only typed control-flow facts.  In
//! particular, it must never carry menu names, notes, clipboard contents,
//! arbitrary key values, window titles, or other user payload.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use std::{sync::Mutex, sync::atomic::AtomicU64};

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
    ReplyEnqueued,
    ReplyAccepted,
    ReplyRejected,
    PendingRetired,
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
    Position { x: i32, y: i32 },
    Size { width: i32, height: i32 },
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
pub(crate) enum NativeActivationEdge {
    RestoreRequested,
    RestoreCompleted,
    RestoreFailed,
}

// The closed, typed `Event` enum is the privacy boundary for this diagnostic.
// The exhaustive schema test below covers every variant and rejects sensitive
// field labels and representative content.

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
        window_under_cursor: Option<u64>,
        correlation: Correlation,
    },
    DesignerPresented {
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
        correlation: Correlation,
    },
    Restore {
        edge: RestoreEdge,
        correlation: Correlation,
    },
    NativeWindowSnapshot {
        hwnd: u64,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        visible: bool,
        minimized: bool,
        correlation: Correlation,
    },
    NativeActivation {
        edge: NativeActivationEdge,
        hwnd: u64,
        correlation: Correlation,
    },
}

struct EventBudget {
    limit: usize,
    emitted: AtomicUsize,
    exhausted: AtomicBool,
}

impl EventBudget {
    const fn new(limit: usize) -> Self {
        Self {
            limit,
            emitted: AtomicUsize::new(0),
            exhausted: AtomicBool::new(false),
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

    fn mark_exhausted_once(&self) -> bool {
        !self.exhausted.swap(true, Ordering::Relaxed)
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
static TRACE_STARTED_AT: OnceLock<Instant> = OnceLock::new();
static WINDOW_SAMPLE_PENDING: AtomicBool = AtomicBool::new(false);
static WINDOW_SAMPLE_CORRELATION: OnceLock<Mutex<Correlation>> = OnceLock::new();
static NEXT_BOUNDARY_ID: AtomicU64 = AtomicU64::new(1);

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

fn elapsed_ms() -> u128 {
    TRACE_STARTED_AT
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
}

pub(crate) fn root_command_correlation() -> Correlation {
    let id = NEXT_BOUNDARY_ID.fetch_add(1, Ordering::Relaxed);
    Correlation {
        request_id: id,
        generation: id,
        ..Correlation::default()
    }
}

pub(crate) fn request_window_sample(correlation: Correlation) {
    if enabled() {
        if let Ok(mut pending) = WINDOW_SAMPLE_CORRELATION
            .get_or_init(|| Mutex::new(Correlation::default()))
            .lock()
        {
            *pending = correlation;
        }
        WINDOW_SAMPLE_PENDING.store(true, Ordering::Release);
    }
}

pub(crate) fn take_window_sample_request() -> Option<Correlation> {
    if !enabled() || !WINDOW_SAMPLE_PENDING.swap(false, Ordering::AcqRel) {
        return None;
    }
    WINDOW_SAMPLE_CORRELATION
        .get_or_init(|| Mutex::new(Correlation::default()))
        .lock()
        .ok()
        .map(|mut pending| std::mem::take(&mut *pending))
}

pub(crate) fn enabled() -> bool {
    runtime().enabled
}

pub(crate) fn emit(event: Event) {
    let runtime = runtime();
    if !runtime.enabled {
        return;
    }
    if !runtime.budget.reserve() {
        if runtime.budget.mark_exhausted_once() {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "budget_exhausted",
                elapsed_ms = elapsed_ms() as u64,
                event_budget = EVENT_BUDGET as u64,
                "radial acceptance trace"
            );
        }
        return;
    }
    let elapsed_ms = elapsed_ms() as u64;

    match event {
        Event::DesignerCallback { phase, viewport } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_callback",
                elapsed_ms,
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
                elapsed_ms,
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
            window_under_cursor,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_pointer",
                elapsed_ms,
                pointer_down = down,
                pointer_up = up,
                window_under_cursor,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerPresented { correlation } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "designer_presented",
                elapsed_ms,
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
                elapsed_ms,
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
                elapsed_ms,
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
                elapsed_ms,
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
                elapsed_ms,
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
                elapsed_ms,
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
                elapsed_ms,
                invocation_id,
                terminal,
                "radial acceptance trace"
            );
        }
        Event::DesiredVisibility { visible, source } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "desired_visibility",
                elapsed_ms,
                visible,
                ?source,
                "radial acceptance trace"
            );
        }
        Event::RootCommand {
            command,
            correlation,
        } => match command {
            RootCommandKind::Position { x, y } => tracing::info!(
                target: TRACE_TARGET,
                trace_event = "root_command",
                elapsed_ms,
                command = "position",
                requested_x = x,
                requested_y = y,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            ),
            RootCommandKind::Size { width, height } => tracing::info!(
                target: TRACE_TARGET,
                trace_event = "root_command",
                elapsed_ms,
                command = "size",
                requested_width = width,
                requested_height = height,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            ),
            command => tracing::info!(
                target: TRACE_TARGET,
                trace_event = "root_command",
                elapsed_ms,
                ?command,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            ),
        },
        Event::Restore { edge, correlation } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "restore",
                elapsed_ms,
                ?edge,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::NativeWindowSnapshot {
            hwnd,
            left,
            top,
            right,
            bottom,
            visible,
            minimized,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "native_window_snapshot",
                elapsed_ms,
                hwnd,
                left,
                top,
                right,
                bottom,
                visible,
                minimized,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::NativeActivation {
            edge,
            hwnd,
            correlation,
        } => {
            tracing::info!(
                target: TRACE_TARGET,
                trace_event = "native_activation",
                elapsed_ms,
                ?edge,
                hwnd,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
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

    fn schema_labels(event: Event) -> &'static [&'static str] {
        match event {
            Event::DesignerCallback { .. } => &["phase", "viewport"],
            Event::DesignerFocus { .. } => &["edge", "viewport", "correlation"],
            Event::DesignerPointer { .. } => &["down", "up", "window_under_cursor", "correlation"],
            Event::DesignerPresented { .. } => &["correlation"],
            Event::DesignerBody { .. } => &["state", "correlation"],
            Event::DesignerWidget { .. } => &["category", "response", "correlation"],
            Event::DesignerMutation { .. } => &["result", "correlation"],
            Event::Authoring { .. } => &["edge", "correlation"],
            Event::InvocationPrimary { .. } => &[
                "transition",
                "provenance",
                "modifiers_match",
                "invocation_id",
                "generation",
            ],
            Event::ShortTap { .. } => &["invocation_id", "terminal"],
            Event::DesiredVisibility { .. } => &["visible", "source"],
            Event::RootCommand { .. } => &["command", "correlation"],
            Event::Restore { .. } => &["edge", "correlation"],
            Event::NativeWindowSnapshot { .. } => &[
                "hwnd",
                "left",
                "top",
                "right",
                "bottom",
                "visible",
                "minimized",
                "correlation",
            ],
            Event::NativeActivation { .. } => &["edge", "hwnd", "correlation"],
        }
    }

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
        assert!(budget.mark_exhausted_once());
        assert!(!budget.mark_exhausted_once());
        assert_eq!(budget.emitted(), 2);
    }

    #[test]
    fn elapsed_time_is_monotonic() {
        let first = elapsed_ms();
        let second = elapsed_ms();
        assert!(second >= first);
    }

    #[test]
    fn event_schema_has_no_payload_surface() {
        let correlation = Correlation {
            request_id: 7,
            request_kind: RequestKind::CommitApply,
            session_id: 11,
            generation: 13,
            terminal: true,
        };
        let events = [
            Event::DesignerCallback {
                phase: CallbackPhase::Enter,
                viewport: ViewportClass::Deferred,
            },
            Event::DesignerFocus {
                edge: FocusEdge::Requested,
                viewport: ViewportClass::Root,
                correlation,
            },
            Event::DesignerPointer {
                down: true,
                up: true,
                window_under_cursor: Some(101),
                correlation,
            },
            Event::DesignerPresented { correlation },
            Event::DesignerBody {
                state: BodyBlock::Conflict,
                correlation,
            },
            Event::DesignerWidget {
                category: WidgetCategory::Inspector,
                response: WidgetResponse::Rejected,
                correlation,
            },
            Event::DesignerMutation {
                result: MutationResult::Accepted,
                correlation,
            },
            Event::Authoring {
                edge: AuthoringEdge::PendingRetired,
                correlation,
            },
            Event::InvocationPrimary {
                transition: PrimaryTransition::Press,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                modifiers_match: true,
                invocation_id: 17,
                generation: 19,
            },
            Event::ShortTap {
                invocation_id: 17,
                terminal: true,
            },
            Event::DesiredVisibility {
                visible: false,
                source: VisibilitySource::Queued,
            },
            Event::RootCommand {
                command: RootCommandKind::Position { x: -101, y: 202 },
                correlation,
            },
            Event::Restore {
                edge: RestoreEdge::RestoreFlag,
                correlation,
            },
            Event::NativeWindowSnapshot {
                hwnd: 303,
                left: -1,
                top: 2,
                right: 3,
                bottom: 4,
                visible: true,
                minimized: false,
                correlation,
            },
            Event::NativeActivation {
                edge: NativeActivationEdge::RestoreFailed,
                hwnd: 303,
                correlation,
            },
        ];
        let rendered = format!("{events:?}").to_ascii_lowercase();
        let forbidden = [
            "payload",
            "clipboard",
            "title",
            "notes",
            "content",
            "entered_name",
            "arbitrary_key",
            "window_title",
            "secret-menu",
            "user note",
        ];
        for labels in events.iter().copied().map(schema_labels) {
            for label in labels {
                assert!(
                    !forbidden.contains(label),
                    "diagnostic schema contains forbidden field label: {label}"
                );
            }
        }
        for forbidden in forbidden {
            assert!(
                !rendered.contains(forbidden),
                "diagnostic schema contains forbidden field/content: {forbidden}"
            );
        }
    }
}
