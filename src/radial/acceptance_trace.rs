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
    pub(crate) screen_x: i32,
    pub(crate) screen_y: i32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookDesktop {
    Default,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AcceptanceKey {
    F24,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookPriorityOwner {
    Launcher,
    ScreenDrawRecovery,
    ExclusiveTool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookDeadlineEdge {
    Scheduled,
    Fired,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootResultKind {
    RadialEdit,
    RadialSkins,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RadialActionStage {
    Activated,
    Parsed,
    ParseRejected,
    Dispatched,
    HostEntered,
    EditorModeApplied,
    HostCompleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DesignerSemanticTarget {
    Menus,
    Skins,
    Tree,
    Inspector,
    DefaultMenu,
    MenuName,
    MenuDefaultSkin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DesignerSemanticRole {
    SelectableLabel,
    Button,
    TextEdit,
    ComboBox,
}

impl DesignerSemanticRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::SelectableLabel => "SelectableLabel",
            Self::Button => "Button",
            Self::TextEdit => "TextEdit",
            Self::ComboBox => "ComboBox",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerCloseState {
    pub open: bool,
    pub close_prompt: bool,
    pub dirty: bool,
    pub pending_disposable: bool,
    pub pending_durable: bool,
    pub pending_native_preview: bool,
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
    DesignerPointerMoved {
        client_x: i32,
        client_y: i32,
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
    DesignerWidgetPointer {
        category: WidgetCategory,
        pressed: bool,
        released: bool,
        hovered: bool,
        button_down_on: bool,
        pointer_inside: bool,
        layer_is_topmost: bool,
        clicked: bool,
        has_position: bool,
        pointer_x: i32,
        pointer_y: i32,
        correlation: Correlation,
    },
    RootResultPointer {
        kind: RootResultKind,
        index: usize,
        pressed: bool,
        released: bool,
        hovered: bool,
        clicked: bool,
        has_position: bool,
        pointer_x: i32,
        pointer_y: i32,
    },
    RadialAction {
        stage: RadialActionStage,
        skins: bool,
        editor_open: Option<bool>,
        skins_selected: Option<bool>,
        panel_registered: Option<bool>,
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
    HookAdmission {
        transition: PrimaryTransition,
        provenance: crate::radial::invocation::InputProvenance,
        owner: HookPriorityOwner,
        global_exclusive_owners: u32,
        adapter_exclusive: bool,
        recovery: bool,
        deadline_scheduled: bool,
        radial_intent: bool,
    },
    HookDeadline {
        edge: HookDeadlineEdge,
        invocation_id: u64,
        timer_id: u64,
        delay_ms: u64,
        radial_intent: bool,
        global_exclusive_owners: u32,
    },
    HookServiceReady {
        thread_id: u32,
        desktop: HookDesktop,
        primary_vk: u32,
    },
    HookServiceExit {
        message_result: i32,
        shutdown_requested: bool,
        primary_down: bool,
        owned_input: bool,
        pending_deadlines: usize,
    },
    HookObserved {
        vk: u32,
        down: bool,
        injected: bool,
    },
    HookCallback {
        primary: bool,
        down: bool,
        injected: bool,
        elapsed_us: u64,
    },
    FrontendKey {
        key: AcceptanceKey,
        focused: bool,
        foreground_owner: NativeWindowOwner,
    },
    DesignerSemanticTarget {
        target: DesignerSemanticTarget,
        role: DesignerSemanticRole,
        viewport: ViewportClass,
        left_px: i32,
        top_px: i32,
        right_px: i32,
        bottom_px: i32,
        selected: bool,
        focused: bool,
        correlation: Correlation,
    },
    DesignerEditState {
        widget_changed: bool,
        model_changed: bool,
        input_matches_model: bool,
        draft_dirty: bool,
        correlation: Correlation,
    },
    DesignerClose {
        state: DesignerCloseState,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesignerSemanticSnapshot {
    target: DesignerSemanticTarget,
    role: DesignerSemanticRole,
    viewport: ViewportClass,
    bounds: [i32; 4],
    selected: bool,
    focused: bool,
    session_id: u64,
    generation: u64,
}

static DESIGNER_SEMANTIC_TARGETS: OnceLock<Mutex<[Option<DesignerSemanticSnapshot>; 7]>> =
    OnceLock::new();

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
            let (window_under_cursor_hwnd, window_under_cursor_owner, screen_x, screen_y) =
                window_under_cursor.map_or(
                    (0, NativeWindowOwner::Other, i32::MIN, i32::MIN),
                    |identity| {
                        (
                            identity.hwnd,
                            identity.owner,
                            identity.screen_x,
                            identity.screen_y,
                        )
                    },
                );
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_pointer",
                elapsed_ms,
                pointer_down = down,
                pointer_up = up,
                window_under_cursor_hwnd,
                window_under_cursor_owner = ?window_under_cursor_owner,
                cursor_screen_x = screen_x,
                cursor_screen_y = screen_y,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::DesignerPointerMoved { client_x, client_y } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_pointer_moved",
                elapsed_ms,
                client_x,
                client_y,
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
        Event::DesignerWidgetPointer {
            category,
            pressed,
            released,
            hovered,
            button_down_on,
            pointer_inside,
            layer_is_topmost,
            clicked,
            has_position,
            pointer_x,
            pointer_y,
            correlation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_widget_pointer",
                elapsed_ms,
                ?category,
                pointer_pressed = pressed,
                pointer_released = released,
                hovered,
                button_down_on,
                pointer_inside,
                layer_is_topmost,
                clicked,
                has_position,
                pointer_x,
                pointer_y,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                terminal = correlation.terminal,
                "radial acceptance trace"
            );
        }
        Event::RootResultPointer {
            kind,
            index,
            pressed,
            released,
            hovered,
            clicked,
            has_position,
            pointer_x,
            pointer_y,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "root_result_pointer",
                elapsed_ms,
                ?kind,
                result_index = index,
                pointer_pressed = pressed,
                pointer_released = released,
                hovered,
                clicked,
                has_position,
                pointer_x,
                pointer_y,
                "radial acceptance trace"
            );
        }
        Event::RadialAction {
            stage,
            skins,
            editor_open,
            skins_selected,
            panel_registered,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "radial_action",
                elapsed_ms,
                ?stage,
                skins,
                editor_open = ?editor_open,
                skins_selected = ?skins_selected,
                panel_registered = ?panel_registered,
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
        Event::HookAdmission {
            transition,
            provenance,
            owner,
            global_exclusive_owners,
            adapter_exclusive,
            recovery,
            deadline_scheduled,
            radial_intent,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_admission",
                elapsed_ms,
                ?transition,
                ?provenance,
                ?owner,
                global_exclusive_owners,
                adapter_exclusive,
                recovery,
                deadline_scheduled,
                radial_intent,
                "radial acceptance trace"
            );
        }
        Event::HookDeadline {
            edge,
            invocation_id,
            timer_id,
            delay_ms,
            radial_intent,
            global_exclusive_owners,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_deadline",
                elapsed_ms,
                ?edge,
                invocation_id,
                timer_id,
                delay_ms,
                radial_intent,
                global_exclusive_owners,
                "radial acceptance trace"
            );
        }
        Event::HookServiceReady {
            thread_id,
            desktop,
            primary_vk,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_service_ready",
                elapsed_ms,
                thread_id,
                ?desktop,
                primary_vk,
                "radial acceptance trace"
            );
        }
        Event::HookServiceExit {
            message_result,
            shutdown_requested,
            primary_down,
            owned_input,
            pending_deadlines,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_service_exit",
                elapsed_ms,
                message_result,
                shutdown_requested,
                primary_down,
                owned_input,
                pending_deadlines,
                "radial acceptance trace"
            );
        }
        Event::HookObserved { vk, down, injected } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_observed",
                elapsed_ms,
                vk,
                down,
                injected,
                "radial acceptance trace"
            );
        }
        Event::HookCallback {
            primary,
            down,
            injected,
            elapsed_us,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_callback",
                elapsed_ms,
                primary,
                down,
                injected,
                callback_elapsed_us = elapsed_us,
                "radial acceptance trace"
            );
        }
        Event::FrontendKey {
            key,
            focused,
            foreground_owner,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "frontend_key",
                elapsed_ms,
                ?key,
                focused,
                ?foreground_owner,
                "radial acceptance trace"
            );
        }
        Event::DesignerSemanticTarget {
            target,
            role,
            viewport,
            left_px,
            top_px,
            right_px,
            bottom_px,
            selected,
            focused,
            correlation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_semantic_target",
                elapsed_ms,
                ?target,
                role = role.as_str(),
                ?viewport,
                left_px,
                top_px,
                right_px,
                bottom_px,
                selected,
                focused,
                session_id = correlation.session_id,
                generation = correlation.generation,
                "radial acceptance trace"
            );
        }
        Event::DesignerEditState {
            widget_changed,
            model_changed,
            input_matches_model,
            draft_dirty,
            correlation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_edit_state",
                elapsed_ms,
                widget_changed,
                model_changed,
                input_matches_model,
                draft_dirty,
                session_id = correlation.session_id,
                generation = correlation.generation,
                "radial acceptance trace"
            );
        }
        Event::DesignerClose { state } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_close",
                elapsed_ms,
                open = state.open,
                close_prompt = state.close_prompt,
                dirty = state.dirty,
                pending_disposable = state.pending_disposable,
                pending_durable = state.pending_durable,
                pending_native_preview = state.pending_native_preview,
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

pub(crate) fn emit_designer_semantic_target(
    target: DesignerSemanticTarget,
    role: DesignerSemanticRole,
    viewport: ViewportClass,
    bounds: [i32; 4],
    selected: bool,
    focused: bool,
    correlation: Correlation,
) {
    if !enabled() {
        return;
    }
    let index = match target {
        DesignerSemanticTarget::Menus => 0,
        DesignerSemanticTarget::Skins => 1,
        DesignerSemanticTarget::Tree => 2,
        DesignerSemanticTarget::Inspector => 3,
        DesignerSemanticTarget::DefaultMenu => 4,
        DesignerSemanticTarget::MenuName => 5,
        DesignerSemanticTarget::MenuDefaultSkin => 6,
    };
    let snapshot = DesignerSemanticSnapshot {
        target,
        role,
        viewport,
        bounds,
        selected,
        focused,
        session_id: correlation.session_id,
        generation: correlation.generation,
    };
    let changed = DESIGNER_SEMANTIC_TARGETS
        .get_or_init(|| Mutex::new([None; 7]))
        .lock()
        .map(|mut previous| {
            if previous[index] == Some(snapshot) {
                false
            } else {
                previous[index] = Some(snapshot);
                true
            }
        })
        .unwrap_or(false);
    if changed {
        emit(Event::DesignerSemanticTarget {
            target,
            role,
            viewport,
            left_px: bounds[0],
            top_px: bounds[1],
            right_px: bounds[2],
            bottom_px: bounds[3],
            selected,
            focused,
            correlation,
        });
    }
}

pub(crate) fn emit_designer_edit_state(
    widget_changed: bool,
    model_changed: bool,
    input_matches_model: bool,
    draft_dirty: bool,
    correlation: Correlation,
) {
    emit(Event::DesignerEditState {
        widget_changed,
        model_changed,
        input_matches_model,
        draft_dirty,
        correlation,
    });
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
            Event::DesignerPointer { .. } => &[
                "down",
                "up",
                "window_under_cursor",
                "screen_x",
                "screen_y",
                "correlation",
            ],
            Event::DesignerPointerMoved { .. } => &["client_x", "client_y"],
            Event::DesignerSubmitted { .. } => &["correlation"],
            Event::DesignerBody { .. } => &["state", "correlation"],
            Event::DesignerWidget { .. } => &["category", "response", "correlation"],
            Event::DesignerWidgetPointer { .. } => &[
                "category",
                "pressed",
                "released",
                "hovered",
                "button_down_on",
                "pointer_inside",
                "layer_is_topmost",
                "clicked",
                "has_position",
                "pointer_x",
                "pointer_y",
                "correlation",
            ],
            Event::RootResultPointer { .. } => &[
                "kind",
                "index",
                "pressed",
                "released",
                "hovered",
                "clicked",
                "has_position",
                "pointer_x",
                "pointer_y",
            ],
            Event::RadialAction { .. } => &[
                "stage",
                "skins",
                "editor_open",
                "skins_selected",
                "panel_registered",
            ],
            Event::DesignerMutation { .. } => &["result", "correlation"],
            Event::Authoring { .. } => &["edge", "correlation"],
            Event::HookPrimary { .. } => &["transition", "provenance", "foreground_owner"],
            Event::HookAdmission { .. } => &[
                "transition",
                "provenance",
                "owner",
                "global_exclusive_owners",
                "adapter_exclusive",
                "recovery",
                "deadline_scheduled",
                "radial_intent",
            ],
            Event::HookDeadline { .. } => &[
                "edge",
                "invocation_id",
                "timer_id",
                "delay_ms",
                "radial_intent",
                "global_exclusive_owners",
            ],
            Event::HookServiceReady { .. } => &["thread_id", "desktop", "primary_vk"],
            Event::HookServiceExit { .. } => &[
                "message_result",
                "shutdown_requested",
                "primary_down",
                "owned_input",
                "pending_deadlines",
            ],
            Event::HookObserved { .. } => &["vk", "down", "injected"],
            Event::HookCallback { .. } => &["primary", "down", "injected", "elapsed_us"],
            Event::FrontendKey { .. } => &["key", "focused", "foreground_owner"],
            Event::DesignerSemanticTarget { .. } => &[
                "target",
                "role",
                "viewport",
                "left_px",
                "top_px",
                "right_px",
                "bottom_px",
                "selected",
                "focused",
                "correlation",
            ],
            Event::DesignerEditState { .. } => &[
                "widget_changed",
                "model_changed",
                "input_matches_model",
                "draft_dirty",
                "correlation",
            ],
            Event::DesignerClose { .. } => &[
                "open",
                "close_prompt",
                "dirty",
                "pending_disposable",
                "pending_durable",
                "pending_native_preview",
            ],
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
                    screen_x: 101,
                    screen_y: 102,
                }),
                correlation,
            },
            Event::DesignerPointerMoved {
                client_x: 101,
                client_y: 102,
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
            Event::DesignerWidgetPointer {
                category: WidgetCategory::Inspector,
                pressed: true,
                released: false,
                hovered: true,
                button_down_on: true,
                pointer_inside: true,
                layer_is_topmost: true,
                clicked: false,
                has_position: true,
                pointer_x: 10,
                pointer_y: 20,
                correlation,
            },
            Event::RootResultPointer {
                kind: RootResultKind::RadialSkins,
                index: 0,
                pressed: true,
                released: false,
                hovered: true,
                clicked: false,
                has_position: true,
                pointer_x: 12,
                pointer_y: 24,
            },
            Event::RadialAction {
                stage: RadialActionStage::EditorModeApplied,
                skins: true,
                editor_open: Some(true),
                skins_selected: Some(true),
                panel_registered: None,
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
            Event::HookAdmission {
                transition: PrimaryTransition::Press,
                provenance: crate::radial::invocation::InputProvenance::ExternalInjected,
                owner: HookPriorityOwner::Launcher,
                global_exclusive_owners: 0,
                adapter_exclusive: false,
                recovery: false,
                deadline_scheduled: true,
                radial_intent: false,
            },
            Event::HookDeadline {
                edge: HookDeadlineEdge::Scheduled,
                invocation_id: 17,
                timer_id: 31,
                delay_ms: 350,
                radial_intent: false,
                global_exclusive_owners: 0,
            },
            Event::HookServiceReady {
                thread_id: 31,
                desktop: HookDesktop::Default,
                primary_vk: 0x7A,
            },
            Event::HookServiceExit {
                message_result: 0,
                shutdown_requested: false,
                primary_down: true,
                owned_input: true,
                pending_deadlines: 1,
            },
            Event::HookObserved {
                vk: 0x87,
                down: true,
                injected: true,
            },
            Event::HookCallback {
                primary: true,
                down: true,
                injected: true,
                elapsed_us: 10,
            },
            Event::DesignerSemanticTarget {
                target: DesignerSemanticTarget::Tree,
                role: DesignerSemanticRole::SelectableLabel,
                viewport: ViewportClass::Deferred,
                left_px: 10,
                top_px: 20,
                right_px: 40,
                bottom_px: 44,
                selected: false,
                focused: true,
                correlation,
            },
            Event::DesignerEditState {
                widget_changed: true,
                model_changed: true,
                input_matches_model: true,
                draft_dirty: true,
                correlation,
            },
            Event::DesignerClose {
                state: DesignerCloseState {
                    open: true,
                    close_prompt: false,
                    dirty: false,
                    pending_disposable: false,
                    pending_durable: false,
                    pending_native_preview: false,
                },
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
