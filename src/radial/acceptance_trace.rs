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
// The native acceptance pass opens Designer twice and drives geometry proposals
// after the ROOT/Designer recovery matrix. Keep the full trace bounded while
// leaving room for both windows' semantic transitions and native input edges.
pub(crate) const EVENT_BUDGET: usize = 8_192;
const TRACE_TARGET: &str = "multi_launcher.radial_acceptance";
const AUTHORING_CONTROL_REFRESH_MS: u128 = 500;

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
pub(crate) enum AcceptancePrepareGateEdge {
    Held,
    Released,
    TimedOut,
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
    RearmedEarly,
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
pub(crate) enum RootMenuControl {
    File,
    Apps,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RootMenuInteractionState {
    hovered: bool,
    clicked: bool,
    open: bool,
}

impl RootMenuInteractionState {
    fn is_active(self) -> bool {
        self.hovered || self.clicked || self.open
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DesignerAuthoringTarget {
    NewMenu,
    MenuAfterAction,
    AfterActionOption,
    AddRing,
    MenuRow,
    RingSelector,
    RingOption,
    Slots,
    PreviewProposal,
    ApplyProposal,
    CancelProposal,
    MoveToOverflow,
    CancelResolution,
    DiscardCells,
    Canvas,
    CanvasCell,
    DiscardDraft,
    CellType,
    ActionTypeOption,
    ActionSearch,
    ActionRow,
    PopupApply,
    PopupCancel,
    PopupOpenInspector,
    PopupApplyAndOpen,
    PopupDiscardAndOpen,
    PopupKeepEditing,
    InspectorCell,
    SkinRow,
    SkinGlowEnabled,
    OpenDesktopPreview,
    StopDesktopPreview,
    Undo,
    Redo,
    Save,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesignerAuthoringRole {
    Button,
    Selectable,
    ComboBox,
    DragValue,
    Region,
    TextEdit,
    Checkbox,
}

impl DesignerAuthoringRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Selectable => "Selectable",
            Self::ComboBox => "ComboBox",
            Self::DragValue => "DragValue",
            Self::Region => "Region",
            Self::TextEdit => "TextEdit",
            Self::Checkbox => "Checkbox",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesignerProposalKind {
    None,
    NewRing,
    Resize,
    ResolvedResize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerGeometryState {
    pub session_id: u64,
    pub menu_count: usize,
    pub selected_menu_index: Option<usize>,
    pub selected_menu_after_action: Option<crate::radial::model::AfterActionPolicy>,
    pub ring_count: usize,
    pub selected_ring_index: Option<usize>,
    pub selected_cell_index: Option<usize>,
    pub selected_cell_id_digest: Option<u64>,
    pub selected_cell_custom_action_index: Option<usize>,
    pub selected_cell_custom_action_index_known: bool,
    pub selected_ring_slots: usize,
    pub requested_slots: usize,
    pub selected_ring_populated: usize,
    pub menu_populated: usize,
    pub draft_cell_ids_digest: u64,
    pub proposal_cell_ids_digest: u64,
    pub proposal_cell_ids_digest_available: bool,
    pub proposal_kind: DesignerProposalKind,
    pub proposal_active: bool,
    pub proposal_ready: bool,
    pub proposal_slots: usize,
    pub proposal_candidate_rings: usize,
    pub proposal_resolution_populated: usize,
    pub proposal_cell_ids_preserved: bool,
    pub resize_prompt_open: bool,
    pub resize_prompt_populated: usize,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesignerAuthoringControlSnapshot {
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    viewport: ViewportClass,
    session_id: u64,
    index: Option<usize>,
    bounds: [i32; 4],
    client_size: [i32; 2],
    enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    generation: u64,
    canvas_scope: Option<DesignerCanvasCellScope>,
    last_emitted_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerCanvasCellScope {
    pub menu_cell_ids_digest: u64,
    pub ring_index: usize,
    pub slot_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerCloseState {
    pub session_id: u64,
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
    RootPointerMoved {
        screen_x: i32,
        screen_y: i32,
    },
    RootPointerButton {
        pressed: bool,
        released: bool,
        screen_x: i32,
        screen_y: i32,
    },
    RootMenuInteraction {
        menu: RootMenuControl,
        hovered: bool,
        clicked: bool,
        open: bool,
    },
    RootMenuBody {
        menu: RootMenuControl,
        entered: bool,
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
    AcceptancePrepareGate {
        edge: AcceptancePrepareGateEdge,
        correlation: Correlation,
    },
    DesignerPreviewRendered {
        session_id: u64,
        generation: u64,
        menu_cell_ids_digest: u64,
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
    HookPumpProbe {
        probe_id: u64,
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
    DesignerAuthoringControl {
        target: DesignerAuthoringTarget,
        role: DesignerAuthoringRole,
        viewport: ViewportClass,
        index: Option<usize>,
        left_px: i32,
        top_px: i32,
        right_px: i32,
        bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
        enabled: bool,
        selected: bool,
        focused: bool,
        clicked: bool,
        session_id: u64,
        generation: u64,
        canvas_scope: Option<DesignerCanvasCellScope>,
    },
    DesignerCanvasAllocation {
        allocated_rect_px: [i32; 4],
        clip_rect_px: [i32; 4],
        requested_size_px: [i32; 2],
        session_id: u64,
        generation: u64,
    },
    DesignerActionCatalogRank {
        custom_action_index: usize,
        rank: usize,
        catalog_len: usize,
        session_id: u64,
        generation: u64,
    },
    NativePreviewDispatchCount {
        editor_session: u64,
        count: usize,
    },
    DesignerGeometryState {
        state: DesignerGeometryState,
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
    DisposableRequestCancelled {
        request_kind: RequestKind,
        request_id: u64,
        session_id: u64,
        generation: u64,
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
static ACTION_CATALOG_RANKS: OnceLock<Mutex<Vec<(u64, usize, usize, usize)>>> = OnceLock::new();
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
static ROOT_MENU_INTERACTIONS: OnceLock<Mutex<[Option<RootMenuInteractionState>; 2]>> =
    OnceLock::new();
static ROOT_MENU_BODIES: OnceLock<Mutex<[Option<bool>; 2]>> = OnceLock::new();
static DESIGNER_AUTHORING_CONTROLS: OnceLock<Mutex<Vec<DesignerAuthoringControlSnapshot>>> =
    OnceLock::new();
static DESIGNER_GEOMETRY_STATE: OnceLock<Mutex<Option<DesignerGeometryState>>> = OnceLock::new();

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
        Event::RootPointerMoved { screen_x, screen_y } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "root_pointer_moved",
                elapsed_ms,
                screen_x,
                screen_y,
                "radial acceptance trace"
            );
        }
        Event::RootPointerButton {
            pressed,
            released,
            screen_x,
            screen_y,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "root_pointer_button",
                elapsed_ms,
                pressed,
                released,
                screen_x,
                screen_y,
                "radial acceptance trace"
            );
        }
        Event::RootMenuInteraction {
            menu,
            hovered,
            clicked,
            open,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "root_menu_interaction",
                elapsed_ms,
                ?menu,
                hovered,
                clicked,
                open,
                "radial acceptance trace"
            );
        }
        Event::RootMenuBody { menu, entered } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "root_menu_body",
                elapsed_ms,
                ?menu,
                entered,
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
        Event::AcceptancePrepareGate { edge, correlation } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "acceptance_prepare_gate",
                elapsed_ms,
                ?edge,
                request_id = correlation.request_id,
                request_kind = ?correlation.request_kind,
                session_id = correlation.session_id,
                generation = correlation.generation,
                "radial acceptance trace"
            );
        }
        Event::DesignerPreviewRendered {
            session_id,
            generation,
            menu_cell_ids_digest,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_preview_rendered",
                elapsed_ms,
                session_id,
                generation,
                menu_cell_ids_digest,
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
        Event::HookPumpProbe { probe_id } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "hook_pump_probe",
                elapsed_ms,
                probe_id,
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
        Event::DesignerAuthoringControl {
            target,
            role,
            viewport,
            index,
            left_px,
            top_px,
            right_px,
            bottom_px,
            client_width_px,
            client_height_px,
            enabled,
            selected,
            focused,
            clicked,
            session_id,
            generation,
            canvas_scope,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_authoring_control",
                elapsed_ms,
                ?target,
                role = role.as_str(),
                ?viewport,
                control_index = index.map_or(-1, |index| index as i64),
                left_px,
                top_px,
                right_px,
                bottom_px,
                client_width_px,
                client_height_px,
                enabled,
                selected,
                focused,
                clicked,
                session_id,
                generation,
                menu_cell_ids_digest = canvas_scope.map_or(0, |scope| scope.menu_cell_ids_digest),
                cell_ring_index = canvas_scope.map_or(-1, |scope| scope.ring_index as i64),
                cell_slot_index = canvas_scope.map_or(-1, |scope| scope.slot_index as i64),
                "radial acceptance trace"
            );
        }
        Event::DesignerCanvasAllocation {
            allocated_rect_px,
            clip_rect_px,
            requested_size_px,
            session_id,
            generation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_canvas_allocation",
                elapsed_ms,
                allocated_left_px = allocated_rect_px[0],
                allocated_top_px = allocated_rect_px[1],
                allocated_right_px = allocated_rect_px[2],
                allocated_bottom_px = allocated_rect_px[3],
                clip_left_px = clip_rect_px[0],
                clip_top_px = clip_rect_px[1],
                clip_right_px = clip_rect_px[2],
                clip_bottom_px = clip_rect_px[3],
                requested_width_px = requested_size_px[0],
                requested_height_px = requested_size_px[1],
                session_id,
                generation,
                "radial acceptance trace"
            );
        }
        Event::DesignerActionCatalogRank {
            custom_action_index,
            rank,
            catalog_len,
            session_id,
            generation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_action_catalog_rank",
                elapsed_ms,
                custom_action_index,
                rank,
                catalog_len,
                session_id,
                generation,
                "radial acceptance trace"
            );
        }
        Event::NativePreviewDispatchCount {
            editor_session,
            count,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "native_preview_dispatch_count",
                elapsed_ms,
                editor_session,
                count,
                "radial acceptance trace"
            );
        }
        Event::DesignerGeometryState { state } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "designer_geometry_state",
                elapsed_ms,
                session_id = state.session_id,
                menu_count = state.menu_count,
                selected_menu_index = state.selected_menu_index.map_or(-1, |index| index as i64),
                selected_menu_after_action = ?state.selected_menu_after_action,
                ring_count = state.ring_count,
                selected_ring_index = state.selected_ring_index.map_or(-1, |index| index as i64),
                selected_cell_index = state.selected_cell_index.map_or(-1, |index| index as i64),
                selected_cell_id_digest = state
                    .selected_cell_id_digest
                    .map_or(-1, |digest| (digest & i64::MAX as u64) as i64),
                selected_cell_custom_action_index = state
                    .selected_cell_custom_action_index
                    .map_or(-1, |index| index as i64),
                selected_cell_custom_action_index_known = state
                    .selected_cell_custom_action_index_known,
                selected_ring_slots = state.selected_ring_slots,
                requested_slots = state.requested_slots,
                selected_ring_populated = state.selected_ring_populated,
                menu_populated = state.menu_populated,
                draft_cell_ids_digest = state.draft_cell_ids_digest,
                proposal_cell_ids_digest = state.proposal_cell_ids_digest,
                proposal_cell_ids_digest_available = state.proposal_cell_ids_digest_available,
                proposal_kind = ?state.proposal_kind,
                proposal_active = state.proposal_active,
                proposal_ready = state.proposal_ready,
                proposal_slots = state.proposal_slots,
                proposal_candidate_rings = state.proposal_candidate_rings,
                proposal_resolution_populated = state.proposal_resolution_populated,
                proposal_cell_ids_preserved = state.proposal_cell_ids_preserved,
                resize_prompt_open = state.resize_prompt_open,
                resize_prompt_populated = state.resize_prompt_populated,
                generation = state.generation,
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
                session_id = state.session_id,
                open = state.open,
                close_prompt = state.close_prompt,
                dirty = state.dirty,
                pending_disposable = state.pending_disposable,
                pending_durable = state.pending_durable,
                pending_native_preview = state.pending_native_preview,
                "radial acceptance trace"
            );
        }
        Event::DisposableRequestCancelled {
            request_kind,
            request_id,
            session_id,
            generation,
        } => {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "disposable_request_cancelled",
                elapsed_ms,
                ?request_kind,
                request_id,
                session_id,
                generation,
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

pub(crate) fn trace_root_menu_interaction(
    menu: RootMenuControl,
    hovered: bool,
    clicked: bool,
    open: bool,
) {
    if !enabled() {
        return;
    }

    let current = RootMenuInteractionState {
        hovered,
        clicked,
        open,
    };
    let index = match menu {
        RootMenuControl::File => 0,
        RootMenuControl::Apps => 1,
    };
    let should_emit = ROOT_MENU_INTERACTIONS
        .get_or_init(|| Mutex::new([None; 2]))
        .lock()
        .map(|mut states| should_emit_root_menu_state(&mut states[index], current))
        .unwrap_or(false);

    if should_emit {
        emit(Event::RootMenuInteraction {
            menu,
            hovered,
            clicked,
            open,
        });
    }
}

pub(crate) fn trace_root_menu_body(menu: RootMenuControl, entered: bool) {
    if !enabled() {
        return;
    }

    let index = match menu {
        RootMenuControl::File => 0,
        RootMenuControl::Apps => 1,
    };
    let should_emit = ROOT_MENU_BODIES
        .get_or_init(|| Mutex::new([None; 2]))
        .lock()
        .map(|mut states| {
            let changed = states[index] != Some(entered);
            states[index] = Some(entered);
            changed
        })
        .unwrap_or(false);

    if should_emit {
        emit(Event::RootMenuBody { menu, entered });
    }
}

fn should_emit_root_menu_state(
    previous: &mut Option<RootMenuInteractionState>,
    current: RootMenuInteractionState,
) -> bool {
    let was_active = previous.is_some_and(RootMenuInteractionState::is_active);
    let changed = *previous != Some(current);
    *previous = Some(current);
    // Emit the terminal inactive edge when a menu closes, but avoid flooding
    // the trace with unchanged idle state. Native acceptance uses the close
    // edge to distinguish a real open popup from an AccessKit/UIA subtree that
    // remains published while its parent menu is closed.
    changed && (was_active || current.is_active())
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

pub(crate) fn emit_designer_authoring_control(
    target: DesignerAuthoringTarget,
    role: DesignerAuthoringRole,
    viewport: ViewportClass,
    index: Option<usize>,
    bounds: [i32; 4],
    client_size: [i32; 2],
    is_enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    session_id: u64,
    generation: u64,
    canvas_scope: Option<DesignerCanvasCellScope>,
) {
    if !enabled()
        || bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || client_size[0] <= 0
        || client_size[1] <= 0
    {
        return;
    }
    let now_ms = elapsed_ms();
    let mut snapshot = DesignerAuthoringControlSnapshot {
        target,
        role,
        viewport,
        session_id,
        index,
        bounds,
        client_size,
        enabled: is_enabled,
        selected,
        focused,
        clicked,
        generation,
        canvas_scope,
        last_emitted_ms: now_ms,
    };
    let changed = DESIGNER_AUTHORING_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut previous| {
            if let Some(existing) = previous.iter_mut().find(|item| {
                item.target == target
                    && item.role == role
                    && item.viewport == viewport
                    && item.session_id == session_id
                    && item.index == index
            }) {
                if !authoring_control_snapshot_should_emit(existing, &snapshot, now_ms) {
                    false
                } else {
                    snapshot.last_emitted_ms = now_ms;
                    *existing = snapshot;
                    true
                }
            } else if previous.len() < 256 {
                previous.push(snapshot);
                true
            } else {
                false
            }
        })
        .unwrap_or(false);
    if changed {
        emit(Event::DesignerAuthoringControl {
            target,
            role,
            viewport,
            index,
            left_px: bounds[0],
            top_px: bounds[1],
            right_px: bounds[2],
            bottom_px: bounds[3],
            client_width_px: client_size[0],
            client_height_px: client_size[1],
            enabled: is_enabled,
            selected,
            focused,
            clicked,
            session_id,
            generation,
            canvas_scope,
        });
    }
}

pub(crate) fn emit_designer_action_catalog_rank(
    custom_action_index: usize,
    rank: usize,
    catalog_len: usize,
    session_id: u64,
    generation: u64,
) {
    if !enabled() || rank >= catalog_len {
        return;
    }
    let emitted = ACTION_CATALOG_RANKS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut previous| {
            let key = (session_id, custom_action_index, rank, catalog_len);
            if previous.contains(&key) || previous.len() >= 1_024 {
                false
            } else {
                previous.push(key);
                true
            }
        })
        .unwrap_or(false);
    if emitted {
        emit(Event::DesignerActionCatalogRank {
            custom_action_index,
            rank,
            catalog_len,
            session_id,
            generation,
        });
    }
}

fn authoring_control_snapshot_should_emit(
    left: &DesignerAuthoringControlSnapshot,
    right: &DesignerAuthoringControlSnapshot,
    now_ms: u128,
) -> bool {
    let unchanged = left.target == right.target
        && left.role == right.role
        && left.viewport == right.viewport
        && left.session_id == right.session_id
        && left.index == right.index
        && left.bounds == right.bounds
        && left.client_size == right.client_size
        && left.enabled == right.enabled
        && left.selected == right.selected
        && left.focused == right.focused
        && left.clicked == right.clicked
        && left.generation == right.generation
        && left.canvas_scope == right.canvas_scope;
    !unchanged || now_ms.saturating_sub(left.last_emitted_ms) >= AUTHORING_CONTROL_REFRESH_MS
}

pub(crate) fn emit_designer_geometry_state(state: DesignerGeometryState) {
    if !enabled() {
        return;
    }
    let changed = DESIGNER_GEOMETRY_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map(|mut previous| {
            if *previous == Some(state) {
                false
            } else {
                *previous = Some(state);
                true
            }
        })
        .unwrap_or(false);
    if changed {
        emit(Event::DesignerGeometryState { state });
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

    fn authoring_control_snapshot(last_emitted_ms: u128) -> DesignerAuthoringControlSnapshot {
        DesignerAuthoringControlSnapshot {
            target: DesignerAuthoringTarget::Canvas,
            role: DesignerAuthoringRole::Region,
            viewport: ViewportClass::Deferred,
            session_id: 9,
            index: None,
            bounds: [10, 20, 520, 390],
            client_size: [624, 441],
            enabled: true,
            selected: false,
            focused: false,
            clicked: false,
            generation: 4,
            canvas_scope: None,
            last_emitted_ms,
        }
    }

    #[test]
    fn unchanged_authoring_controls_refresh_for_post_resize_render_proof() {
        let prior = authoring_control_snapshot(1_000);
        let unchanged = authoring_control_snapshot(1_000);
        assert!(!authoring_control_snapshot_should_emit(
            &prior, &unchanged, 1_499
        ));
        assert!(authoring_control_snapshot_should_emit(
            &prior, &unchanged, 1_500
        ));

        let mut moved = unchanged;
        moved.bounds[2] += 1;
        assert!(authoring_control_snapshot_should_emit(
            &prior, &moved, 1_001
        ));

        let mut resized = unchanged;
        resized.client_size = [520, 380];
        assert!(authoring_control_snapshot_should_emit(
            &prior, &resized, 1_001
        ));

        let mut focused = unchanged;
        focused.focused = true;
        assert!(authoring_control_snapshot_should_emit(
            &prior, &focused, 1_001
        ));

        let mut different_canvas_epoch = unchanged;
        different_canvas_epoch.canvas_scope = Some(DesignerCanvasCellScope {
            menu_cell_ids_digest: 41,
            ring_index: 1,
            slot_index: 0,
        });
        assert!(authoring_control_snapshot_should_emit(
            &prior,
            &different_canvas_epoch,
            1_001
        ));
    }

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
            Event::RootPointerMoved { .. } => &["screen_x", "screen_y"],
            Event::RootPointerButton { .. } => &["pressed", "released", "screen_x", "screen_y"],
            Event::RootMenuInteraction { .. } => &["menu", "hovered", "clicked", "open"],
            Event::RootMenuBody { .. } => &["menu", "entered"],
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
            Event::AcceptancePrepareGate { .. } => &["edge", "correlation"],
            Event::DesignerPreviewRendered { .. } => {
                &["session_id", "generation", "menu_cell_ids_digest"]
            }
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
            Event::HookPumpProbe { .. } => &["probe_id"],
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
            Event::DesignerAuthoringControl { .. } => &[
                "target",
                "role",
                "viewport",
                "control_index",
                "left_px",
                "top_px",
                "right_px",
                "bottom_px",
                "client_width_px",
                "client_height_px",
                "enabled",
                "selected",
                "focused",
                "clicked",
                "session_id",
                "generation",
                "menu_cell_ids_digest",
                "cell_ring_index",
                "cell_slot_index",
            ],
            Event::DesignerCanvasAllocation { .. } => &[
                "allocated_left_px",
                "allocated_top_px",
                "allocated_right_px",
                "allocated_bottom_px",
                "clip_left_px",
                "clip_top_px",
                "clip_right_px",
                "clip_bottom_px",
                "requested_width_px",
                "requested_height_px",
                "session_id",
                "generation",
            ],
            Event::DesignerActionCatalogRank { .. } => &[
                "custom_action_index",
                "rank",
                "catalog_len",
                "session_id",
                "generation",
            ],
            Event::NativePreviewDispatchCount { .. } => &["editor_session", "count"],
            Event::DesignerGeometryState { .. } => &[
                "session_id",
                "menu_count",
                "selected_menu_index",
                "selected_menu_after_action",
                "ring_count",
                "selected_ring_index",
                "selected_cell_index",
                "selected_cell_id_digest",
                "selected_cell_custom_action_index",
                "selected_cell_custom_action_index_known",
                "selected_ring_slots",
                "requested_slots",
                "selected_ring_populated",
                "menu_populated",
                "draft_cell_ids_digest",
                "proposal_cell_ids_digest",
                "proposal_cell_ids_digest_available",
                "proposal_kind",
                "proposal_active",
                "proposal_ready",
                "proposal_slots",
                "proposal_candidate_rings",
                "proposal_resolution_populated",
                "proposal_cell_ids_preserved",
                "resize_prompt_open",
                "resize_prompt_populated",
                "generation",
            ],
            Event::DesignerEditState { .. } => &[
                "widget_changed",
                "model_changed",
                "input_matches_model",
                "draft_dirty",
                "correlation",
            ],
            Event::DesignerClose { .. } => &[
                "session_id",
                "open",
                "close_prompt",
                "dirty",
                "pending_disposable",
                "pending_durable",
                "pending_native_preview",
            ],
            Event::DisposableRequestCancelled { .. } => {
                &["request_kind", "request_id", "session_id", "generation"]
            }
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
        assert_eq!(EVENT_BUDGET, 8_192);
        let budget = EventBudget::new(EVENT_BUDGET);
        for _ in 0..EVENT_BUDGET {
            assert!(budget.reserve());
        }
        assert!(!budget.reserve());
        assert!(budget.mark_exhausted_once());
        assert!(!budget.mark_exhausted_once());
        assert_eq!(budget.emitted(), EVENT_BUDGET);
    }

    #[test]
    fn elapsed_time_is_monotonic() {
        let first = elapsed_ms();
        let second = elapsed_ms();
        assert!(second >= first);
    }

    #[test]
    fn root_menu_trace_emits_state_changes_and_resets_on_close() {
        let mut previous = None;
        let inactive = RootMenuInteractionState::default();
        let hovered = RootMenuInteractionState {
            hovered: true,
            ..inactive
        };
        let open = RootMenuInteractionState {
            open: true,
            ..hovered
        };
        let clicked = RootMenuInteractionState {
            clicked: true,
            ..open
        };

        assert!(!should_emit_root_menu_state(&mut previous, inactive));
        assert!(should_emit_root_menu_state(&mut previous, hovered));
        assert!(!should_emit_root_menu_state(&mut previous, hovered));
        assert!(should_emit_root_menu_state(&mut previous, open));
        assert!(should_emit_root_menu_state(&mut previous, clicked));
        assert!(should_emit_root_menu_state(&mut previous, open));
        assert!(should_emit_root_menu_state(&mut previous, inactive));
        assert!(!should_emit_root_menu_state(&mut previous, inactive));
        assert!(should_emit_root_menu_state(&mut previous, open));
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
            Event::RootPointerMoved {
                screen_x: 201,
                screen_y: 202,
            },
            Event::RootMenuInteraction {
                menu: RootMenuControl::File,
                hovered: true,
                clicked: true,
                open: true,
            },
            Event::RootMenuBody {
                menu: RootMenuControl::File,
                entered: true,
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
            Event::HookPumpProbe { probe_id: 41 },
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
            Event::DesignerAuthoringControl {
                target: DesignerAuthoringTarget::RingOption,
                role: DesignerAuthoringRole::Selectable,
                viewport: ViewportClass::Deferred,
                index: Some(1),
                left_px: 10,
                top_px: 20,
                right_px: 40,
                bottom_px: 44,
                client_width_px: 624,
                client_height_px: 441,
                enabled: true,
                selected: false,
                focused: true,
                clicked: true,
                session_id: 77,
                generation: 19,
                canvas_scope: Some(DesignerCanvasCellScope {
                    menu_cell_ids_digest: 123,
                    ring_index: 1,
                    slot_index: 0,
                }),
            },
            Event::DesignerCanvasAllocation {
                allocated_rect_px: [191, 157, 331, 442],
                clip_rect_px: [0, 0, 624, 441],
                requested_size_px: [140, 285],
                session_id: 77,
                generation: 19,
            },
            Event::DesignerActionCatalogRank {
                custom_action_index: 67,
                rank: 71,
                catalog_len: 140,
                session_id: 77,
                generation: 19,
            },
            Event::NativePreviewDispatchCount {
                editor_session: 77,
                count: 0,
            },
            Event::DesignerGeometryState {
                state: DesignerGeometryState {
                    session_id: 77,
                    menu_count: 10,
                    selected_menu_index: Some(9),
                    selected_menu_after_action: Some(
                        crate::radial::model::AfterActionPolicy::Inherit,
                    ),
                    ring_count: 2,
                    selected_ring_index: Some(1),
                    selected_cell_index: Some(4),
                    selected_cell_id_digest: Some(303),
                    selected_cell_custom_action_index: Some(67),
                    selected_cell_custom_action_index_known: true,
                    selected_ring_slots: 10,
                    requested_slots: 10,
                    selected_ring_populated: 0,
                    menu_populated: 0,
                    draft_cell_ids_digest: 101,
                    proposal_cell_ids_digest: 202,
                    proposal_cell_ids_digest_available: true,
                    proposal_kind: DesignerProposalKind::Resize,
                    proposal_active: true,
                    proposal_ready: true,
                    proposal_slots: 10,
                    proposal_candidate_rings: 2,
                    proposal_resolution_populated: 0,
                    proposal_cell_ids_preserved: true,
                    resize_prompt_open: false,
                    resize_prompt_populated: 0,
                    generation: 19,
                },
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
                    session_id: 77,
                    open: true,
                    close_prompt: false,
                    dirty: false,
                    pending_disposable: false,
                    pending_durable: false,
                    pending_native_preview: false,
                },
            },
            Event::DisposableRequestCancelled {
                request_kind: RequestKind::PrepareEmbeddedPreview,
                request_id: 88,
                session_id: 77,
                generation: 19,
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
