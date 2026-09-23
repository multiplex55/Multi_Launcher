//! Bounded, opt-in tracing for one radial acceptance run.
//!
//! The trace deliberately exposes only typed control-flow facts.  In
//! particular, it must never carry menu names, notes, clipboard contents,
//! arbitrary key values, window titles, or other user payload.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use std::{sync::Mutex, sync::atomic::AtomicU64};

pub(crate) const ENVIRONMENT_VARIABLE: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
pub(crate) const EVENT_BUDGET: usize = 512;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeWindowOwner {
    Root,
    PreviewInput,
    PreviewVisual,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeWindowIdentity {
    pub(crate) hwnd: u64,
    pub(crate) owner: NativeWindowOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePointerTransition {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePointerButton {
    Primary,
    Secondary,
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
        window_under_cursor: Option<NativeWindowIdentity>,
        correlation: Correlation,
    },
    DesignerSubmitted {
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
    HookPrimary {
        transition: PrimaryTransition,
        provenance: crate::radial::invocation::InputProvenance,
        foreground_owner: NativeWindowOwner,
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
    WindowSampleTruncated {
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
    NativePointer {
        transition: NativePointerTransition,
        button: NativePointerButton,
        owner: NativeWindowOwner,
        hwnd: u64,
        generation: u64,
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
static NEXT_BOUNDARY_ID: AtomicU64 = AtomicU64::new(1);
const WINDOW_SAMPLE_CAPACITY: usize = 32;
const WINDOW_SAMPLE_DELAY_FRAMES: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingWindowSample {
    correlation: Correlation,
    ready_frame: u64,
}

#[derive(Debug, Default)]
struct WindowSampleQueue {
    frame: u64,
    pending: VecDeque<PendingWindowSample>,
}

impl WindowSampleQueue {
    fn enqueue(&mut self, correlation: Correlation) -> Option<Correlation> {
        let dropped = if self.pending.len() >= WINDOW_SAMPLE_CAPACITY {
            self.pending.pop_front().map(|sample| sample.correlation)
        } else {
            None
        };
        self.pending.push_back(PendingWindowSample {
            correlation,
            ready_frame: self.frame.saturating_add(WINDOW_SAMPLE_DELAY_FRAMES),
        });
        dropped
    }

    fn advance_frame(&mut self) {
        self.frame = self.frame.saturating_add(1);
    }

    fn pop_ready(&mut self) -> Option<Correlation> {
        self.pending
            .front()
            .filter(|sample| sample.ready_frame <= self.frame)
            .copied()
            .map(|sample| {
                self.pending.pop_front();
                sample.correlation
            })
    }

    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[derive(Debug, Default)]
struct NativeOwnerRegistry {
    root: Option<u64>,
    preview_inputs: [u64; 8],
    preview_visuals: [u64; 8],
}

impl NativeOwnerRegistry {
    fn register_preview(&mut self, input: u64, visual: u64) {
        register_bounded(&mut self.preview_inputs, input);
        register_bounded(&mut self.preview_visuals, visual);
    }

    fn unregister_preview(&mut self, input: u64, visual: u64) {
        unregister_bounded(&mut self.preview_inputs, input);
        unregister_bounded(&mut self.preview_visuals, visual);
    }

    fn classify(&self, hwnd: u64) -> NativeWindowOwner {
        if hwnd == 0 {
            NativeWindowOwner::Other
        } else if self.root == Some(hwnd) {
            NativeWindowOwner::Root
        } else if self.preview_inputs.contains(&hwnd) {
            NativeWindowOwner::PreviewInput
        } else if self.preview_visuals.contains(&hwnd) {
            NativeWindowOwner::PreviewVisual
        } else {
            NativeWindowOwner::Other
        }
    }
}

fn register_bounded(slots: &mut [u64], value: u64) {
    if value == 0 || slots.contains(&value) {
        return;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| **slot == 0) {
        *slot = value;
    }
}

fn unregister_bounded(slots: &mut [u64], value: u64) {
    if let Some(slot) = slots.iter_mut().find(|slot| **slot == value) {
        *slot = 0;
    }
}

static WINDOW_SAMPLE_QUEUE: OnceLock<Mutex<WindowSampleQueue>> = OnceLock::new();
static NATIVE_OWNER_REGISTRY: OnceLock<Mutex<NativeOwnerRegistry>> = OnceLock::new();

fn enabled_from_value(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "on")
    )
}

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime {
        enabled: {
            let enabled = enabled_from_value(std::env::var(ENVIRONMENT_VARIABLE).ok().as_deref());
            if enabled {
                tracing::warn!(target: TRACE_TARGET, trace_event = "trace_ready", "radial acceptance trace");
            }
            enabled
        },
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
    if !enabled() {
        return;
    }
    let dropped = WINDOW_SAMPLE_QUEUE
        .get_or_init(|| Mutex::new(WindowSampleQueue::default()))
        .lock()
        .ok()
        .and_then(|mut queue| queue.enqueue(correlation));
    if let Some(correlation) = dropped {
        emit(Event::WindowSampleTruncated { correlation });
    }
}

pub(crate) fn advance_window_sample_frame() -> bool {
    if !enabled() {
        return false;
    }
    WINDOW_SAMPLE_QUEUE
        .get_or_init(|| Mutex::new(WindowSampleQueue::default()))
        .lock()
        .ok()
        .map(|mut queue| {
            queue.advance_frame();
            queue.has_pending()
        })
        .unwrap_or(false)
}

pub(crate) fn take_window_sample_request() -> Option<Correlation> {
    if !enabled() {
        return None;
    }
    WINDOW_SAMPLE_QUEUE
        .get_or_init(|| Mutex::new(WindowSampleQueue::default()))
        .lock()
        .ok()
        .and_then(|mut queue| queue.pop_ready())
}

pub(crate) fn window_sample_pending() -> bool {
    enabled()
        && WINDOW_SAMPLE_QUEUE
            .get_or_init(|| Mutex::new(WindowSampleQueue::default()))
            .lock()
            .ok()
            .is_some_and(|queue| queue.has_pending())
}

pub(crate) fn register_root_hwnd(hwnd: u64) {
    if !enabled() || hwnd == 0 {
        return;
    }
    if let Ok(mut registry) = NATIVE_OWNER_REGISTRY
        .get_or_init(|| Mutex::new(NativeOwnerRegistry::default()))
        .lock()
    {
        registry.root = Some(hwnd);
    }
}

pub(crate) fn register_preview_hwnds(input: u64, visual: u64) {
    if !enabled() {
        return;
    }
    if let Ok(mut registry) = NATIVE_OWNER_REGISTRY
        .get_or_init(|| Mutex::new(NativeOwnerRegistry::default()))
        .lock()
    {
        registry.register_preview(input, visual);
    }
}

pub(crate) fn unregister_preview_hwnds(input: u64, visual: u64) {
    if !enabled() {
        return;
    }
    if let Ok(mut registry) = NATIVE_OWNER_REGISTRY
        .get_or_init(|| Mutex::new(NativeOwnerRegistry::default()))
        .lock()
    {
        registry.unregister_preview(input, visual);
    }
}

pub(crate) fn classify_window(hwnd: u64) -> NativeWindowOwner {
    if !enabled() {
        return NativeWindowOwner::Other;
    }
    NATIVE_OWNER_REGISTRY
        .get_or_init(|| Mutex::new(NativeOwnerRegistry::default()))
        .lock()
        .map(|registry| registry.classify(hwnd))
        .unwrap_or(NativeWindowOwner::Other)
}

#[cfg(target_os = "windows")]
pub(crate) fn foreground_owner() -> NativeWindowOwner {
    if !enabled() {
        return NativeWindowOwner::Other;
    }
    let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    classify_window(hwnd.0 as usize as u64)
}

pub(crate) fn enabled() -> bool {
    runtime().enabled
}

pub(crate) fn emit(event: Event) {
    let runtime = runtime();
    if !runtime.enabled {
        return;
    }
    // Acceptance runs commonly inherit a warn-only filter. Keep this
    // explicitly enabled, bounded trace visible without changing that filter.
    if !runtime.budget.reserve() {
        if runtime.budget.mark_exhausted_once() {
            tracing::warn!(
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
            tracing::warn!(
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
            tracing::warn!(
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
            let (window_under_cursor_hwnd, window_under_cursor_owner) = window_under_cursor
                .map_or((0, NativeWindowOwner::Other), |identity| {
                    (identity.hwnd, identity.owner)
                });
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_pointer",
                elapsed_ms,
                pointer_down = down,
                pointer_up = up,
                window_under_cursor_hwnd,
                window_under_cursor_owner = ?window_under_cursor_owner,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerSubmitted { correlation } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_submitted",
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
            tracing::warn!(
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
            tracing::warn!(
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
            tracing::warn!(
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
            tracing::warn!(
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
        Event::HookPrimary {
            transition,
            provenance,
            foreground_owner,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_primary",
                elapsed_ms,
                ?transition,
                ?provenance,
                ?foreground_owner,
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
            tracing::warn!(
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
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "short_tap",
                elapsed_ms,
                invocation_id,
                terminal,
                "radial acceptance trace"
            );
        }
        Event::DesiredVisibility { visible, source } => {
            tracing::warn!(
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
            RootCommandKind::Position { x, y } => tracing::warn!(
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
            RootCommandKind::Size { width, height } => tracing::warn!(
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
            command => tracing::warn!(
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
        Event::WindowSampleTruncated { correlation } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "window_sample_truncated",
                elapsed_ms,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::Restore { edge, correlation } => {
            tracing::warn!(
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
            tracing::warn!(
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
            tracing::warn!(
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
        Event::NativePointer {
            transition,
            button,
            owner,
            hwnd,
            generation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "native_pointer",
                elapsed_ms,
                ?transition,
                ?button,
                ?owner,
                hwnd,
                generation,
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
            Event::DesignerSubmitted { .. } => &["correlation"],
            Event::DesignerBody { .. } => &["state", "correlation"],
            Event::DesignerWidget { .. } => &["category", "response", "correlation"],
            Event::DesignerMutation { .. } => &["result", "correlation"],
            Event::Authoring { .. } => &["edge", "correlation"],
            Event::HookPrimary { .. } => &["transition", "provenance", "foreground_owner"],
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
            Event::WindowSampleTruncated { .. } => &["correlation"],
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
            Event::NativePointer { .. } => &["transition", "button", "owner", "hwnd", "generation"],
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
                window_under_cursor: Some(NativeWindowIdentity {
                    hwnd: 101,
                    owner: NativeWindowOwner::Root,
                }),
                correlation,
            },
            Event::DesignerSubmitted { correlation },
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
            Event::HookPrimary {
                transition: PrimaryTransition::Press,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                foreground_owner: NativeWindowOwner::Root,
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
            Event::WindowSampleTruncated { correlation },
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
            Event::NativePointer {
                transition: NativePointerTransition::Down,
                button: NativePointerButton::Primary,
                owner: NativeWindowOwner::PreviewInput,
                hwnd: 303,
                generation: 19,
            },
        ];
        let rendered = format!("{events:?}").to_ascii_lowercase();
        let forbidden = [
            "payload",
            "clipboard",
            "title",
            "class",
            "pid",
            "process",
            "path",
            "name",
            "text",
            "key",
            "value",
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

    fn correlation(request_id: u64) -> Correlation {
        Correlation {
            request_id,
            ..Correlation::default()
        }
    }

    #[test]
    fn window_sample_queue_preserves_fifo_after_two_boundaries() {
        let mut queue = WindowSampleQueue::default();
        assert!(queue.enqueue(correlation(1)).is_none());
        assert!(queue.enqueue(correlation(2)).is_none());
        assert!(queue.pop_ready().is_none());

        queue.advance_frame();
        assert!(queue.pop_ready().is_none());
        queue.advance_frame();
        assert_eq!(queue.pop_ready(), Some(correlation(1)));
        assert_eq!(queue.pop_ready(), Some(correlation(2)));
        assert!(queue.pop_ready().is_none());
    }

    #[test]
    fn window_sample_queue_reports_oldest_bounded_overflow() {
        let mut queue = WindowSampleQueue::default();
        for request_id in 0..WINDOW_SAMPLE_CAPACITY as u64 {
            assert!(queue.enqueue(correlation(request_id)).is_none());
        }
        assert_eq!(queue.enqueue(correlation(99)), Some(correlation(0)));

        queue.advance_frame();
        queue.advance_frame();
        assert_eq!(queue.pop_ready(), Some(correlation(1)));
        assert_eq!(queue.pop_ready(), Some(correlation(2)));
    }

    #[test]
    fn native_owner_registry_classifies_and_unregisters_known_windows() {
        let mut registry = NativeOwnerRegistry::default();
        registry.root = Some(10);
        registry.register_preview(20, 30);
        assert_eq!(registry.classify(10), NativeWindowOwner::Root);
        assert_eq!(registry.classify(20), NativeWindowOwner::PreviewInput);
        assert_eq!(registry.classify(30), NativeWindowOwner::PreviewVisual);
        assert_eq!(registry.classify(40), NativeWindowOwner::Other);

        registry.unregister_preview(20, 30);
        assert_eq!(registry.classify(20), NativeWindowOwner::Other);
        assert_eq!(registry.classify(30), NativeWindowOwner::Other);
    }
}
