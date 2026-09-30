//! Bounded, opt-in tracing for one radial acceptance run.
//!
//! The trace deliberately exposes only typed control-flow facts.  In
//! particular, it must never carry menu names, notes, clipboard contents,
//! arbitrary key values, window titles, or other user payload.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use std::{sync::Mutex, sync::atomic::AtomicU64};

pub(crate) const ENVIRONMENT_VARIABLE: &str = "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE";
pub(crate) const TRACE_BUDGET_PROFILE_ENVIRONMENT_VARIABLE: &str =
    "MULTI_LAUNCHER_RADIAL_ACCEPTANCE_TRACE_PROFILE";
// Existing hotkey/query acceptance keeps its original bound. Gate C observes seven
// independent authoring cases; measured distinct rendered controls make broad
// deduplication lose needed proof. Reserve one legacy budget per case plus one
// legacy-budget margin (eight budgets total), and keep a separate bounded terminal
// lane. This profile is selected only by the Gate C acceptance child.
pub(crate) const EVENT_BUDGET: usize = 8_192;
pub(crate) const GATE_C_EVENT_BUDGET: usize = EVENT_BUDGET * 8;
pub(crate) const GATE_C_TERMINAL_RESERVE: usize = 256;
const GATE_C_TRACE_PROFILE: &str = "gate_c_v1";
pub(crate) const GATE_D_EVENT_BUDGET: usize = EVENT_BUDGET * 6;
pub(crate) const GATE_D_TERMINAL_RESERVE: usize = 256;
const GATE_D_TRACE_PROFILE: &str = "gate_d_v1";
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
    /// Visibility ordering metadata is trace-only. It lets the acceptance
    /// runner join a ROOT boundary command to the hotkey decision that caused
    /// it without inferring from nearby log lines.
    pub(crate) visibility_revision: u64,
    pub(crate) invocation_id: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct VisibilityTraceLink {
    revision: u64,
    invocation_id: u64,
}

thread_local! {
    static VISIBILITY_TRACE_LINK: RefCell<Option<VisibilityTraceLink>> = const { RefCell::new(None) };
}

/// Attach the current visibility decision to ROOT commands issued while the
/// operation runs. This scoped trace context does not participate in product
/// state or change command ordering.
pub(crate) fn with_visibility_trace_link<T>(
    revision: u64,
    invocation_id: Option<u64>,
    operation: impl FnOnce() -> T,
) -> T {
    struct RestorePrevious(Option<VisibilityTraceLink>);

    impl Drop for RestorePrevious {
        fn drop(&mut self) {
            VISIBILITY_TRACE_LINK.with(|slot| *slot.borrow_mut() = self.0);
        }
    }

    let previous = VISIBILITY_TRACE_LINK.with(|slot| {
        slot.replace(Some(VisibilityTraceLink {
            revision,
            invocation_id: invocation_id.unwrap_or(0),
        }))
    });
    let _restore = RestorePrevious(previous);
    operation()
}

fn current_visibility_trace_link() -> VisibilityTraceLink {
    VISIBILITY_TRACE_LINK.with(|slot| slot.borrow().unwrap_or_default())
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
pub(crate) enum RuntimePreparationEdge {
    GateHeld,
    GateReleased,
    GateTimedOut,
    ReplyQueued,
    ReplyRejected,
    CancelledByLauncherTap,
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
    /// A Screen Draw exact restore commits a new revision for an already
    /// visible ROOT state; this is not a launcher-toggle decision.
    ScreenDrawRestore,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RadialInsertionTraceIdentity {
    pub(crate) request_id: u64,
    pub(crate) source_target_digest: u64,
    pub(crate) source_action_digest: u64,
    pub(crate) source_binding_digest: u64,
    pub(crate) source_query_digest: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RadialInsertionControl {
    SourceAdd,
    DestinationMenu,
    DestinationRing,
    DestinationCell,
    InsertSelectedSpacer,
    AppendToRing,
    ReplaceToggle,
    ReplaceConfirm,
    Cancel,
    CloseTreeConfirm,
    Save,
    Reopen,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RadialInsertionWidgetPart {
    None,
    Selector,
    Option,
    Button,
}

impl RadialInsertionWidgetPart {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Selector => "selector",
            Self::Option => "option",
            Self::Button => "button",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RadialInsertionWidgetGeometry {
    pub(crate) bounds: [i32; 4],
    pub(crate) full_bounds: [i32; 4],
    pub(crate) client_size: [i32; 2],
    pub(crate) visible: bool,
    pub(crate) fully_visible: bool,
}

impl RadialInsertionControl {
    fn as_str(self) -> &'static str {
        match self {
            Self::SourceAdd => "source_add",
            Self::DestinationMenu => "destination_menu",
            Self::DestinationRing => "destination_ring",
            Self::DestinationCell => "destination_cell",
            Self::InsertSelectedSpacer => "insert_selected_spacer",
            Self::AppendToRing => "append_to_ring",
            Self::ReplaceToggle => "replace_toggle",
            Self::ReplaceConfirm => "replace_confirm",
            Self::Cancel => "cancel",
            Self::CloseTreeConfirm => "close_tree_confirm",
            Self::Save => "save",
            Self::Reopen => "reopen",
            Self::Undo => "undo",
            Self::Redo => "redo",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RadialInsertionControlSnapshot {
    identity: RadialInsertionTraceIdentity,
    control: RadialInsertionControl,
    widget_part: RadialInsertionWidgetPart,
    widget: RadialInsertionWidgetGeometry,
    destination_menu_digest: u64,
    destination_ring_digest: u64,
    destination_cell_digest: u64,
    session_id: u64,
    generation: u64,
    enabled: bool,
    selected: bool,
    clicked: bool,
    document_digest_after: u64,
    binding_digest_after: u64,
    last_emitted_ms: u128,
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
    ProjectedCell,
    TreeSearch,
    TreeSearchClear,
    TreeSearchResult,
    BulkLabel,
    BulkSetLabel,
    DesignerBack,
    DesignerBreadcrumb,
    EditDynamicSource,
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
    InspectorDiscardAndContinue,
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
    clip_bounds: [i32; 4],
    client_size: [i32; 2],
    enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    generation: u64,
    scope: Option<DesignerAuthoringControlScope>,
    last_emitted_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActionEditorControlSnapshot {
    surface: &'static str,
    control: &'static str,
    editor_session_id: u64,
    draft_generation: u64,
    stable_target_digest: u64,
    editor_epoch: u64,
    edit_generation: u64,
    query_generation: u64,
    query_request_generation: u64,
    search_request_generation: u64,
    test_request_generation: u64,
    control_index: Option<usize>,
    target_digest: u64,
    title_digest: u64,
    type_digest: u64,
    disambiguator_digest: u64,
    action_digest: u64,
    binding_digest: u64,
    query_digest: u64,
    value_digest: u64,
    displayed_text_digest: u64,
    editor_assigned_binding_digest: u64,
    bounds: [i32; 4],
    full_bounds: [i32; 4],
    client_size: [i32; 2],
    fully_visible: bool,
    enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    changed: bool,
    enter_pressed: bool,
    last_emitted_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActionEditorScrollSnapshot {
    surface: &'static str,
    editor_session_id: u64,
    draft_generation: u64,
    stable_target_digest: u64,
    editor_epoch: u64,
    edit_generation: u64,
    query_generation: u64,
    query_request_generation: u64,
    search_request_generation: u64,
    test_request_generation: u64,
    query_digest: u64,
    editor_assigned_binding_digest: u64,
    scroll_id: u64,
    frame_nr: u64,
    offset_y_milli: i64,
    velocity_y_milli: i64,
    content_height_milli: i64,
    inner_height_milli: i64,
    pixels_per_point_milli: i64,
    handle_min_length_milli: i64,
    inner_bounds: [i32; 4],
    inner_visible_bounds: [i32; 4],
    track_bounds: [i32; 4],
    track_visible_bounds: [i32; 4],
    thumb_bounds: [i32; 4],
    thumb_visible_bounds: [i32; 4],
    painted_thumb_bounds: [i32; 4],
    painted_thumb_visible_bounds: [i32; 4],
    paint_clip_bounds: [i32; 4],
    client_size: [i32; 2],
    last_emitted_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActionEditorScrollObservation {
    pub query_digest: u64,
    pub scroll_id: u64,
    pub frame_nr: u64,
    pub offset_y_milli: i64,
    pub velocity_y_milli: i64,
    pub content_height_milli: i64,
    pub inner_height_milli: i64,
    pub pixels_per_point_milli: i64,
    pub handle_min_length_milli: i64,
    pub inner_bounds: [i32; 4],
    pub inner_visible_bounds: [i32; 4],
    pub track_bounds: [i32; 4],
    pub track_visible_bounds: [i32; 4],
    pub thumb_bounds: [i32; 4],
    pub thumb_visible_bounds: [i32; 4],
    pub painted_thumb_bounds: [i32; 4],
    pub painted_thumb_visible_bounds: [i32; 4],
    pub paint_clip_bounds: [i32; 4],
    pub client_size: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InspectorCellTextEditSnapshot {
    target_digest: u64,
    session_id: u64,
    generation: u64,
    value_digest: u64,
    bounds: [i32; 4],
    clip_bounds: [i32; 4],
    client_size: [i32; 2],
    visible: bool,
    fully_visible: bool,
    focused: bool,
    clicked: bool,
    changed: bool,
    last_emitted_ms: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerCanvasCellScope {
    pub menu_cell_ids_digest: u64,
    pub menu_id_digest: u64,
    pub ring_id_digest: u64,
    pub cell_id_digest: u64,
    pub authored_target_digest: u64,
    pub label_digest: u64,
    pub ring_index: usize,
    pub slot_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerProjectedCellScope {
    pub menu_id_digest: u64,
    pub source_target_digest: u64,
    pub result_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerDynamicSourceEditScope {
    pub source_target_digest: u64,
    pub result_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DesignerRenderedText {
    pub digest: u64,
    pub bounds: [i32; 4],
    pub fully_visible: bool,
    pub elided: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesignerAuthoringControlScope {
    CanvasCell(DesignerCanvasCellScope),
    TreeSearchUndo {
        field_id_digest: u64,
        value_digest: u64,
        in_flux: bool,
    },
    AuthoredSearchResult {
        menu_id_digest: u64,
        ring_id_digest: u64,
        cell_id_digest: u64,
        authored_target_digest: u64,
        rendered_text: DesignerRenderedText,
    },
    ProjectedCell(DesignerProjectedCellScope),
    DynamicSourceEdit(DesignerDynamicSourceEditScope),
    Breadcrumb {
        menu_id_digest: u64,
    },
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
    UniversalActionExecution {
        action_id_digest: u64,
        action_surface: &'static str,
        activation_source: &'static str,
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
    RuntimePreparation {
        edge: RuntimePreparationEdge,
        invocation_id: u64,
        generation: u64,
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
        clip_left_px: i32,
        clip_top_px: i32,
        clip_right_px: i32,
        clip_bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
        enabled: bool,
        selected: bool,
        focused: bool,
        clicked: bool,
        session_id: u64,
        generation: u64,
        scope: Option<DesignerAuthoringControlScope>,
    },
    DesignerActionEditorControl {
        surface: &'static str,
        control: &'static str,
        control_index: Option<usize>,
        target_digest: u64,
        title_digest: u64,
        type_digest: u64,
        disambiguator_digest: u64,
        action_digest: u64,
        binding_digest: u64,
        query_digest: u64,
        value_digest: u64,
        displayed_text_digest: u64,
        editor_assigned_binding_digest: u64,
        editor_session_id: u64,
        draft_generation: u64,
        stable_target_digest: u64,
        editor_epoch: u64,
        edit_generation: u64,
        query_generation: u64,
        query_request_generation: u64,
        search_request_generation: u64,
        test_request_generation: u64,
        left_px: i32,
        top_px: i32,
        right_px: i32,
        bottom_px: i32,
        full_left_px: i32,
        full_top_px: i32,
        full_right_px: i32,
        full_bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
        fully_visible: bool,
        enabled: bool,
        selected: bool,
        focused: bool,
        clicked: bool,
        changed: bool,
        enter_pressed: bool,
    },
    DesignerActionEditorScroll {
        surface: &'static str,
        editor_session_id: u64,
        draft_generation: u64,
        stable_target_digest: u64,
        editor_epoch: u64,
        edit_generation: u64,
        query_generation: u64,
        query_request_generation: u64,
        search_request_generation: u64,
        test_request_generation: u64,
        query_digest: u64,
        editor_assigned_binding_digest: u64,
        scroll_id: u64,
        frame_nr: u64,
        offset_y_milli: i64,
        velocity_y_milli: i64,
        content_height_milli: i64,
        inner_height_milli: i64,
        pixels_per_point_milli: i64,
        handle_min_length_milli: i64,
        inner_left_px: i32,
        inner_top_px: i32,
        inner_right_px: i32,
        inner_bottom_px: i32,
        inner_visible_left_px: i32,
        inner_visible_top_px: i32,
        inner_visible_right_px: i32,
        inner_visible_bottom_px: i32,
        track_left_px: i32,
        track_top_px: i32,
        track_right_px: i32,
        track_bottom_px: i32,
        track_visible_left_px: i32,
        track_visible_top_px: i32,
        track_visible_right_px: i32,
        track_visible_bottom_px: i32,
        thumb_left_px: i32,
        thumb_top_px: i32,
        thumb_right_px: i32,
        thumb_bottom_px: i32,
        thumb_visible_left_px: i32,
        thumb_visible_top_px: i32,
        thumb_visible_right_px: i32,
        thumb_visible_bottom_px: i32,
        painted_thumb_left_px: i32,
        painted_thumb_top_px: i32,
        painted_thumb_right_px: i32,
        painted_thumb_bottom_px: i32,
        painted_thumb_visible_left_px: i32,
        painted_thumb_visible_top_px: i32,
        painted_thumb_visible_right_px: i32,
        painted_thumb_visible_bottom_px: i32,
        paint_clip_left_px: i32,
        paint_clip_top_px: i32,
        paint_clip_right_px: i32,
        paint_clip_bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
    },
    DesignerInspectorCellTextEdit {
        target_digest: u64,
        session_id: u64,
        generation: u64,
        value_digest: u64,
        left_px: i32,
        top_px: i32,
        right_px: i32,
        bottom_px: i32,
        clip_left_px: i32,
        clip_top_px: i32,
        clip_right_px: i32,
        clip_bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
        visible: bool,
        fully_visible: bool,
        focused: bool,
        clicked: bool,
        changed: bool,
    },
    RadialInsertionControl {
        control: &'static str,
        widget_part: &'static str,
        request_id: u64,
        source_target_digest: u64,
        source_action_digest: u64,
        source_binding_digest: u64,
        source_query_digest: u64,
        destination_menu_digest: u64,
        destination_ring_digest: u64,
        destination_cell_digest: u64,
        session_id: u64,
        generation: u64,
        enabled: bool,
        selected: bool,
        clicked: bool,
        left_px: i32,
        top_px: i32,
        right_px: i32,
        bottom_px: i32,
        full_left_px: i32,
        full_top_px: i32,
        full_right_px: i32,
        full_bottom_px: i32,
        client_width_px: i32,
        client_height_px: i32,
        visible: bool,
        fully_visible: bool,
        document_digest_after: u64,
        binding_digest_after: u64,
    },
    AuthoringProviderSearch {
        edge: &'static str,
        kind: &'static str,
        editor_surface: &'static str,
        editor_session_id: u64,
        draft_generation: u64,
        stable_target_digest: u64,
        editor_epoch: u64,
        edit_generation: u64,
        query_generation: u64,
        query_request_generation: u64,
        search_request_generation: u64,
        test_request_generation: u64,
        query_digest: u64,
        binding_digest: u64,
        editor_assigned_binding_digest: u64,
        provider_revision: Option<u64>,
    },
    AuthoringObservationBoundary {
        phase: &'static str,
        request_id: u64,
        baseline_request_id: Option<u64>,
        captured_trace_sequence: u64,
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
    RadialDispatchRequested {
        invocation_id: u64,
        session_generation: u64,
    },
    RuntimeRadialHover {
        session_digest: u64,
        cell_digest: u64,
        layout_generation: u64,
        role: &'static str,
        executable: bool,
    },
    RadialQueryResolution {
        invocation_id: u64,
        session_digest: u64,
        cell_digest: u64,
        session_generation: u64,
        config_revision: u64,
        preparation_generation: u64,
        query_digest: u64,
        mode: &'static str,
        state: &'static str,
        provider_revision: Option<u64>,
        result_count: usize,
        result_digest: u64,
        selected_digest: Option<u64>,
        interaction_requirement: &'static str,
    },
    RadialQueryDispatch {
        invocation_id: u64,
        session_digest: u64,
        cell_digest: u64,
        session_generation: u64,
        config_revision: u64,
        mode: &'static str,
        query_digest: u64,
        selected_digest: u64,
        interaction_requirement: &'static str,
        root_policy: &'static str,
        outcome: &'static str,
    },
    RadialRootSnapshot {
        phase: &'static str,
        invocation_id: u64,
        session_digest: u64,
        cell_digest: u64,
        query_digest: u64,
        action_digest: u64,
        source: &'static str,
        state_digest: u64,
        ordinary_query_digest: u64,
        results_digest: u64,
        results_count: usize,
        selected_index: i64,
        grid_layout: bool,
        visible: bool,
        restore: bool,
        visibility_revision: u64,
        focus_query: bool,
        move_cursor_end: bool,
        last_results_valid: bool,
        last_search_query_digest: u64,
        suggestions_digest: u64,
        autocomplete_index: usize,
        query_history_digest: u64,
        matching_history_count: usize,
        radial_source_history_count: usize,
        usage_count: u32,
    },
    DesiredVisibility {
        visible: bool,
        revision: u64,
        source: VisibilitySource,
        invocation_id: Option<u64>,
    },
    ScreenDrawRestoreFocusIntent {
        revision: u64,
        invocation_id: Option<u64>,
        focus_intent: crate::visibility::RootFocusIntent,
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
        process_id: u32,
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
    terminal_reserve_limit: usize,
    emitted: AtomicUsize,
    terminal_reserved: AtomicUsize,
    exhausted: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventBudgetAdmission {
    Normal,
    TerminalReserve,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BudgetTraceEntry {
    Exhausted {
        trace_sequence: u64,
        elapsed_ms: u64,
        event_budget: usize,
        reserved_event_budget: usize,
    },
    Event {
        event: Event,
        captured_trace_sequence: u64,
        trace_sequence: u64,
        elapsed_ms: u64,
    },
}

impl EventBudget {
    const fn new(limit: usize) -> Self {
        Self::with_terminal_reserve(limit, 0)
    }

    const fn with_terminal_reserve(limit: usize, terminal_reserve_limit: usize) -> Self {
        Self {
            limit,
            terminal_reserve_limit,
            emitted: AtomicUsize::new(0),
            terminal_reserved: AtomicUsize::new(0),
            exhausted: AtomicBool::new(false),
        }
    }

    fn reserve(&self) -> bool {
        self.reserve_with_terminal_policy(false) == EventBudgetAdmission::Normal
    }

    fn reserve_with_terminal_policy(&self, terminal_event: bool) -> EventBudgetAdmission {
        if reserve_atomic_slot(&self.emitted, self.limit) {
            return EventBudgetAdmission::Normal;
        }
        if terminal_event
            && reserve_atomic_slot(&self.terminal_reserved, self.terminal_reserve_limit)
        {
            return EventBudgetAdmission::TerminalReserve;
        }
        EventBudgetAdmission::Rejected
    }

    fn mark_exhausted_once(&self) -> bool {
        !self.exhausted.swap(true, Ordering::Relaxed)
    }

    #[cfg(test)]
    fn emitted(&self) -> usize {
        self.emitted.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn terminal_reserved(&self) -> usize {
        self.terminal_reserved.load(Ordering::Relaxed)
    }
}

fn reserve_atomic_slot(counter: &AtomicUsize, limit: usize) -> bool {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current >= limit {
            return false;
        }
        match counter.compare_exchange_weak(
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TraceBudgetProfile {
    name: &'static str,
    event_limit: usize,
    terminal_reserve: usize,
}

impl TraceBudgetProfile {
    const DEFAULT: Self = Self {
        name: "default",
        event_limit: EVENT_BUDGET,
        terminal_reserve: 0,
    };

    const GATE_C: Self = Self {
        name: GATE_C_TRACE_PROFILE,
        event_limit: GATE_C_EVENT_BUDGET,
        terminal_reserve: GATE_C_TERMINAL_RESERVE,
    };

    const GATE_D: Self = Self {
        name: GATE_D_TRACE_PROFILE,
        event_limit: GATE_D_EVENT_BUDGET,
        terminal_reserve: GATE_D_TERMINAL_RESERVE,
    };

    fn from_environment(value: Option<&str>) -> Self {
        match value {
            Some(GATE_C_TRACE_PROFILE) => Self::GATE_C,
            Some(GATE_D_TRACE_PROFILE) => Self::GATE_D,
            _ => Self::DEFAULT,
        }
    }
}

// This reserve is unavailable to ordinary controls. It preserves real terminal
// lifecycle, GUI-owner boundary, and cancellation/close receipts needed to explain
// and clean up a run that has already failed its primary event budget.
fn event_uses_terminal_reserve(event: &Event) -> bool {
    match event {
        Event::AuthoringObservationBoundary { .. }
        | Event::DesignerClose { .. }
        | Event::DisposableRequestCancelled { .. }
        | Event::DesignerPointerMoved { .. } => true,
        Event::DesignerPointer { down, up, .. } if *down || *up => true,
        Event::AuthoringProviderSearch { edge, .. } => matches!(
            *edge,
            "worker_completed" | "worker_failed" | "cancelled" | "applied" | "rejected" | "retired"
        ),
        Event::DesignerAuthoringControl {
            target: DesignerAuthoringTarget::DiscardDraft,
            enabled: true,
            ..
        } => true,
        Event::DesignerAuthoringControl {
            target:
                DesignerAuthoringTarget::PopupDiscardAndOpen
                | DesignerAuthoringTarget::PopupKeepEditing
                | DesignerAuthoringTarget::KeepEditing,
            clicked: true,
            ..
        } => true,
        Event::RadialInsertionControl {
            control: "cancel",
            clicked: true,
            ..
        } => true,
        _ => false,
    }
}

struct Runtime {
    enabled: bool,
    budget: EventBudget,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static TRACE_STARTED_AT: OnceLock<Instant> = OnceLock::new();
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_RADIAL_INSERTION_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static RADIAL_INSERTION_CONTROLS: OnceLock<Mutex<Vec<RadialInsertionControlSnapshot>>> =
    OnceLock::new();
static ACTION_CATALOG_RANKS: OnceLock<Mutex<Vec<(u64, usize, usize, usize)>>> = OnceLock::new();
static ACTION_EDITOR_SCROLLS: OnceLock<Mutex<Vec<ActionEditorScrollSnapshot>>> = OnceLock::new();
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
static ACTION_EDITOR_CONTROLS: OnceLock<Mutex<Vec<ActionEditorControlSnapshot>>> = OnceLock::new();
static INSPECTOR_CELL_TEXT_EDITS: OnceLock<Mutex<Vec<InspectorCellTextEditSnapshot>>> =
    OnceLock::new();
static DESIGNER_GEOMETRY_STATE: OnceLock<Mutex<Option<DesignerGeometryState>>> = OnceLock::new();
struct TracePublicationFence {
    gate: Mutex<()>,
    sequence: AtomicU64,
}

impl TracePublicationFence {
    const fn new() -> Self {
        Self {
            gate: Mutex::new(()),
            sequence: AtomicU64::new(0),
        }
    }

    fn publish_with_elapsed<T>(
        &self,
        sample_elapsed: impl FnOnce() -> u64,
        publish: impl FnOnce(u64, u64, u64) -> T,
    ) -> T {
        let _guard = self
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let captured = self.sequence.load(Ordering::Relaxed);
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let elapsed_ms = sample_elapsed();
        publish(captured, sequence, elapsed_ms)
    }

    fn cursor(&self) -> u64 {
        let _guard = self
            .gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.sequence.load(Ordering::Relaxed)
    }
}

static TRACE_PUBLICATION_FENCE: TracePublicationFence = TracePublicationFence::new();

/// Current acceptance-trace cursor for GUI-owner observation boundaries.
/// The cursor advances only for events accepted by the bounded trace, so a
/// captured value brackets the same event stream that the native runner reads.
pub(crate) fn trace_sequence() -> u64 {
    TRACE_PUBLICATION_FENCE.cursor()
}

/// Extracts the publication sequence from a sanitized acceptance-trace record.
///
/// This is shared with the native reader so producer-output tests exercise the same field
/// decoding that the runner uses for freshness checks.
#[doc(hidden)]
pub fn trace_line_sequence(line: &str) -> Option<u64> {
    line.split_ascii_whitespace().find_map(|field| {
        field
            .strip_prefix("trace_sequence=")
            .and_then(|value| value.trim_end_matches(',').parse::<u64>().ok())
    })
}

/// Returns whether a sanitized trace record belongs to `(after_exclusive, through_inclusive]`.
/// The acceptance runner uses the same predicate as producer tests so event records and GUI
/// observation fences share one sequence-window contract.
#[doc(hidden)]
pub fn trace_line_is_in_sequence_window(
    line: &str,
    after_exclusive: u64,
    through_inclusive: u64,
) -> bool {
    if after_exclusive >= through_inclusive {
        return false;
    }
    trace_line_sequence(line)
        .is_some_and(|sequence| sequence > after_exclusive && sequence <= through_inclusive)
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthoringProviderTraceSnapshot {
    pub(crate) identity: crate::gui::AuthoringBindingEditorIdentity,
    pub(crate) edge: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) editor_surface: &'static str,
    pub(crate) query_digest: u64,
    pub(crate) binding_digest: u64,
}

#[cfg(test)]
thread_local! {
    static TEST_AUTHORING_PROVIDER_TRACE: RefCell<Vec<AuthoringProviderTraceSnapshot>> = const { RefCell::new(Vec::new()) };
    static TEST_ACTION_EDITOR_CONTROL_EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static TEST_ACTION_EDITOR_SCROLL_EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static TEST_INSPECTOR_TEXT_EDIT_EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static TEST_RADIAL_INSERTION_CONTROL_EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn take_authoring_provider_trace_test_events() -> Vec<AuthoringProviderTraceSnapshot> {
    TEST_AUTHORING_PROVIDER_TRACE.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn take_action_editor_control_test_events() -> Vec<Event> {
    TEST_ACTION_EDITOR_CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn take_action_editor_scroll_test_events() -> Vec<Event> {
    TEST_ACTION_EDITOR_SCROLL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn reset_action_editor_scroll_test_state() {
    TEST_ACTION_EDITOR_SCROLL_EVENTS.with(|events| events.borrow_mut().clear());
    if let Some(cache) = ACTION_EDITOR_SCROLLS.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.clear();
        }
    }
}

#[cfg(test)]
pub(crate) fn take_inspector_text_edit_test_events() -> Vec<Event> {
    TEST_INSPECTOR_TEXT_EDIT_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn take_radial_insertion_control_test_events() -> Vec<Event> {
    TEST_RADIAL_INSERTION_CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[cfg(test)]
pub(crate) fn reset_radial_insertion_control_test_state() {
    TEST_RADIAL_INSERTION_CONTROL_EVENTS.with(|events| events.borrow_mut().clear());
    if let Some(cache) = RADIAL_INSERTION_CONTROLS.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.clear();
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_inspector_text_edit_test_state() {
    TEST_INSPECTOR_TEXT_EDIT_EVENTS.with(|events| events.borrow_mut().clear());
    if let Some(cache) = INSPECTOR_CELL_TEXT_EDITS.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.clear();
        }
    }
}

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
    RUNTIME.get_or_init(|| {
        let enabled = enabled_from_value(std::env::var(ENVIRONMENT_VARIABLE).ok().as_deref());
        let budget_profile = TraceBudgetProfile::from_environment(
            std::env::var(TRACE_BUDGET_PROFILE_ENVIRONMENT_VARIABLE)
                .ok()
                .as_deref(),
        );
        if enabled {
            tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "trace_ready",
                trace_budget_profile = budget_profile.name,
                event_budget = budget_profile.event_limit as u64,
                reserved_event_budget = budget_profile.terminal_reserve as u64,
                "radial acceptance trace"
            );
        }
        Runtime {
            enabled,
            budget: EventBudget::with_terminal_reserve(
                budget_profile.event_limit,
                budget_profile.terminal_reserve,
            ),
        }
    })
}

fn elapsed_ms() -> u128 {
    TRACE_STARTED_AT
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
}

pub(crate) fn root_command_correlation() -> Correlation {
    let id = next_request_id();
    let visibility = current_visibility_trace_link();
    Correlation {
        request_id: id,
        generation: id,
        visibility_revision: visibility.revision,
        invocation_id: visibility.invocation_id,
        ..Correlation::default()
    }
}

/// Allocate an identifier shared by ROOT boundary and native restore traces.
/// Both event families can describe one visibility request, so they must not
/// draw from independent namespaces that can collide in the acceptance log.
pub(crate) fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
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

pub(crate) fn next_radial_insertion_request_id() -> u64 {
    if !enabled() {
        return 0;
    }
    NEXT_RADIAL_INSERTION_REQUEST_ID
        .fetch_add(1, Ordering::Relaxed)
        .max(1)
}

pub(crate) fn emit_radial_insertion_control(
    identity: RadialInsertionTraceIdentity,
    control: RadialInsertionControl,
    destination_menu_digest: u64,
    destination_ring_digest: u64,
    destination_cell_digest: u64,
    session_id: u64,
    generation: u64,
    enabled: bool,
    selected: bool,
    clicked: bool,
    document_digest_after: u64,
    binding_digest_after: u64,
) {
    emit_radial_insertion_control_with_widget(
        identity,
        control,
        RadialInsertionWidgetPart::None,
        RadialInsertionWidgetGeometry::default(),
        destination_menu_digest,
        destination_ring_digest,
        destination_cell_digest,
        session_id,
        generation,
        enabled,
        selected,
        clicked,
        document_digest_after,
        binding_digest_after,
    );
}

pub(crate) fn emit_radial_insertion_control_with_widget(
    identity: RadialInsertionTraceIdentity,
    control: RadialInsertionControl,
    widget_part: RadialInsertionWidgetPart,
    widget: RadialInsertionWidgetGeometry,
    destination_menu_digest: u64,
    destination_ring_digest: u64,
    destination_cell_digest: u64,
    session_id: u64,
    generation: u64,
    enabled: bool,
    selected: bool,
    clicked: bool,
    document_digest_after: u64,
    binding_digest_after: u64,
) {
    if (!self::enabled() && !cfg!(test)) || identity.request_id == 0 {
        return;
    }
    let snapshot = RadialInsertionControlSnapshot {
        identity,
        control,
        widget_part,
        widget,
        destination_menu_digest,
        destination_ring_digest,
        destination_cell_digest,
        session_id,
        generation,
        enabled,
        selected,
        clicked,
        document_digest_after,
        binding_digest_after,
        last_emitted_ms: elapsed_ms(),
    };
    const CONTROL_CACHE_CAPACITY: usize = 512;
    let now_ms = snapshot.last_emitted_ms;
    let changed = RADIAL_INSERTION_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut previous| {
            let same_state = |old: &RadialInsertionControlSnapshot| {
                let mut old_without_time = *old;
                let mut current_without_time = snapshot;
                old_without_time.last_emitted_ms = 0;
                current_without_time.last_emitted_ms = 0;
                old_without_time == current_without_time
            };
            if let Some(existing) = previous.iter_mut().find(|old| same_state(old)) {
                if now_ms.saturating_sub(existing.last_emitted_ms) < AUTHORING_CONTROL_REFRESH_MS {
                    false
                } else {
                    *existing = snapshot;
                    true
                }
            } else {
                if previous.len() >= CONTROL_CACHE_CAPACITY {
                    previous.remove(0);
                }
                previous.push(snapshot);
                true
            }
        })
        .unwrap_or(false);
    if changed {
        let event = Event::RadialInsertionControl {
            control: control.as_str(),
            widget_part: widget_part.as_str(),
            request_id: identity.request_id,
            source_target_digest: identity.source_target_digest,
            source_action_digest: identity.source_action_digest,
            source_binding_digest: identity.source_binding_digest,
            source_query_digest: identity.source_query_digest,
            destination_menu_digest,
            destination_ring_digest,
            destination_cell_digest,
            session_id,
            generation,
            enabled,
            selected,
            clicked,
            left_px: widget.bounds[0],
            top_px: widget.bounds[1],
            right_px: widget.bounds[2],
            bottom_px: widget.bounds[3],
            full_left_px: widget.full_bounds[0],
            full_top_px: widget.full_bounds[1],
            full_right_px: widget.full_bounds[2],
            full_bottom_px: widget.full_bounds[3],
            client_width_px: widget.client_size[0],
            client_height_px: widget.client_size[1],
            visible: widget.visible,
            fully_visible: widget.fully_visible,
            document_digest_after,
            binding_digest_after,
        };
        #[cfg(test)]
        TEST_RADIAL_INSERTION_CONTROL_EVENTS.with(|events| events.borrow_mut().push(event.clone()));
        emit(event);
    }
}

pub(crate) fn emit(event: Event) {
    let runtime = runtime();
    if !runtime.enabled {
        return;
    }
    emit_with_budget(event, &runtime.budget);
}

fn publish_budgeted_event(
    event: Event,
    budget: &EventBudget,
    fence: &TracePublicationFence,
    mut publish: impl FnMut(BudgetTraceEntry),
) -> EventBudgetAdmission {
    let admission = budget.reserve_with_terminal_policy(event_uses_terminal_reserve(&event));
    if admission != EventBudgetAdmission::Normal && budget.mark_exhausted_once() {
        fence.publish_with_elapsed(
            || elapsed_ms() as u64,
            |_, trace_sequence, elapsed_ms| {
                publish(BudgetTraceEntry::Exhausted {
                    trace_sequence,
                    elapsed_ms,
                    event_budget: budget.limit,
                    reserved_event_budget: budget.terminal_reserve_limit,
                });
            },
        );
    }
    if admission == EventBudgetAdmission::Rejected {
        return admission;
    }
    fence.publish_with_elapsed(
        || elapsed_ms() as u64,
        |captured_trace_sequence, trace_sequence, elapsed_ms| {
            publish(BudgetTraceEntry::Event {
                event,
                captured_trace_sequence,
                trace_sequence,
                elapsed_ms,
            });
        },
    );
    admission
}

fn emit_with_budget(event: Event, budget: &EventBudget) {
    publish_budgeted_event(
        event,
        budget,
        &TRACE_PUBLICATION_FENCE,
        |entry| match entry {
            BudgetTraceEntry::Exhausted {
                trace_sequence,
                elapsed_ms,
                event_budget,
                reserved_event_budget,
            } => tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "budget_exhausted",
                trace_sequence,
                elapsed_ms,
                event_budget = event_budget as u64,
                reserved_event_budget = reserved_event_budget as u64,
                "radial acceptance trace"
            ),
            BudgetTraceEntry::Event {
                event,
                captured_trace_sequence,
                trace_sequence,
                elapsed_ms,
            } => match event {
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                        trace_sequence,
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
                        trace_sequence,
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
                        trace_sequence,
                        "radial acceptance trace"
                    );
                }
                Event::UniversalActionExecution {
                    action_id_digest,
                    action_surface,
                    activation_source,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "universal_action_execution",
                        elapsed_ms,
                        action_id_digest,
                        action_surface,
                        activation_source,
                        trace_sequence,
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
                        trace_sequence,
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
                        trace_sequence,
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
                Event::RuntimePreparation {
                    edge,
                    invocation_id,
                    generation,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "runtime_preparation",
                        elapsed_ms,
                        ?edge,
                        invocation_id,
                        generation,
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
                    clip_left_px,
                    clip_top_px,
                    clip_right_px,
                    clip_bottom_px,
                    client_width_px,
                    client_height_px,
                    enabled,
                    selected,
                    focused,
                    clicked,
                    session_id,
                    generation,
                    scope,
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
                        clip_left_px,
                        clip_top_px,
                        clip_right_px,
                        clip_bottom_px,
                        client_width_px,
                        client_height_px,
                        trace_sequence,
                        enabled,
                        selected,
                        focused,
                        clicked,
                        session_id,
                        generation,
                        menu_cell_ids_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.menu_cell_ids_digest, _ => 0 }),
                        menu_id_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.menu_id_digest, DesignerAuthoringControlScope::ProjectedCell(cell) => cell.menu_id_digest, DesignerAuthoringControlScope::Breadcrumb { menu_id_digest } | DesignerAuthoringControlScope::AuthoredSearchResult { menu_id_digest, .. } => menu_id_digest, DesignerAuthoringControlScope::DynamicSourceEdit(_) | DesignerAuthoringControlScope::TreeSearchUndo { .. } => 0 }),
                        ring_id_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.ring_id_digest, DesignerAuthoringControlScope::AuthoredSearchResult { ring_id_digest, .. } => ring_id_digest, _ => 0 }),
                        cell_id_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.cell_id_digest, DesignerAuthoringControlScope::AuthoredSearchResult { cell_id_digest, .. } => cell_id_digest, _ => 0 }),
                        authored_target_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.authored_target_digest, DesignerAuthoringControlScope::AuthoredSearchResult { authored_target_digest, .. } => authored_target_digest, _ => 0 }),
                        cell_label_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.label_digest, _ => 0 }),
                        cell_ring_index = scope.map_or(-1, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.ring_index as i64, _ => -1 }),
                        cell_slot_index = scope.map_or(-1, |scope| match scope { DesignerAuthoringControlScope::CanvasCell(cell) => cell.slot_index as i64, _ => -1 }),
                        projected_source_target_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::ProjectedCell(cell) => cell.source_target_digest, _ => 0 }),
                        projected_result_index = scope.map_or(-1, |scope| match scope { DesignerAuthoringControlScope::ProjectedCell(cell) => cell.result_index as i64, _ => -1 }),
                        edit_source_target_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::DynamicSourceEdit(edit) => edit.source_target_digest, _ => 0 }),
                        edit_source_result_index = scope.map_or(-1, |scope| match scope { DesignerAuthoringControlScope::DynamicSourceEdit(edit) => edit.result_index as i64, _ => -1 }),
                        breadcrumb_menu_id_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::Breadcrumb { menu_id_digest } => menu_id_digest, _ => 0 }),
                        rendered_text_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } => rendered_text.digest, _ => 0 }),
                        rendered_text_left_px = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } => rendered_text.bounds[0], _ => 0 }),
                        rendered_text_top_px = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } => rendered_text.bounds[1], _ => 0 }),
                        rendered_text_right_px = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } => rendered_text.bounds[2], _ => 0 }),
                        rendered_text_bottom_px = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } => rendered_text.bounds[3], _ => 0 }),
                        rendered_text_fully_visible = scope.is_some_and(|scope| matches!(scope, DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } if rendered_text.fully_visible)),
                        rendered_text_elided = scope.is_some_and(|scope| matches!(scope, DesignerAuthoringControlScope::AuthoredSearchResult { rendered_text, .. } if rendered_text.elided)),
                        text_edit_field_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::TreeSearchUndo { field_id_digest, .. } => field_id_digest, _ => 0 }),
                        text_edit_value_digest = scope.map_or(0, |scope| match scope { DesignerAuthoringControlScope::TreeSearchUndo { value_digest, .. } => value_digest, _ => 0 }),
                        text_edit_undo_in_flux = scope.map_or(-1i32, |scope| match scope { DesignerAuthoringControlScope::TreeSearchUndo { in_flux, .. } => i32::from(in_flux), _ => -1 }),
                        "radial acceptance trace"
                    );
                }
                Event::DesignerActionEditorControl {
                    surface,
                    control,
                    control_index,
                    target_digest,
                    title_digest,
                    type_digest,
                    disambiguator_digest,
                    action_digest,
                    binding_digest,
                    query_digest,
                    value_digest,
                    displayed_text_digest,
                    editor_assigned_binding_digest,
                    editor_session_id,
                    draft_generation,
                    stable_target_digest,
                    editor_epoch,
                    edit_generation,
                    query_generation,
                    query_request_generation,
                    search_request_generation,
                    test_request_generation,
                    left_px,
                    top_px,
                    right_px,
                    bottom_px,
                    full_left_px,
                    full_top_px,
                    full_right_px,
                    full_bottom_px,
                    client_width_px,
                    client_height_px,
                    fully_visible,
                    enabled,
                    selected,
                    focused,
                    clicked,
                    changed,
                    enter_pressed,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "designer_action_editor_control",
                        elapsed_ms,
                        editor_surface = surface,
                        editor_control = control,
                        control_index = control_index.map_or(-1, |index| index as i64),
                        target_digest,
                        title_digest,
                        type_digest,
                        disambiguator_digest,
                        action_digest,
                        binding_digest,
                        query_digest,
                        value_digest,
                        displayed_text_digest,
                        editor_assigned_binding_digest,
                        editor_session_id,
                        draft_generation,
                        stable_target_digest,
                        editor_epoch,
                        edit_generation,
                        query_generation,
                        query_request_generation,
                        search_request_generation,
                        test_request_generation,
                        trace_sequence,
                        left_px,
                        top_px,
                        right_px,
                        bottom_px,
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        client_width_px,
                        client_height_px,
                        fully_visible,
                        enabled,
                        selected,
                        focused,
                        clicked,
                        changed,
                        enter_pressed,
                        visible = true,
                        "radial acceptance trace"
                    );
                }
                Event::DesignerActionEditorScroll {
                    surface,
                    editor_session_id,
                    draft_generation,
                    stable_target_digest,
                    editor_epoch,
                    edit_generation,
                    query_generation,
                    query_request_generation,
                    search_request_generation,
                    test_request_generation,
                    query_digest,
                    editor_assigned_binding_digest,
                    scroll_id,
                    frame_nr,
                    offset_y_milli,
                    velocity_y_milli,
                    content_height_milli,
                    inner_height_milli,
                    pixels_per_point_milli,
                    handle_min_length_milli,
                    inner_left_px,
                    inner_top_px,
                    inner_right_px,
                    inner_bottom_px,
                    inner_visible_left_px,
                    inner_visible_top_px,
                    inner_visible_right_px,
                    inner_visible_bottom_px,
                    track_left_px,
                    track_top_px,
                    track_right_px,
                    track_bottom_px,
                    track_visible_left_px,
                    track_visible_top_px,
                    track_visible_right_px,
                    track_visible_bottom_px,
                    thumb_left_px,
                    thumb_top_px,
                    thumb_right_px,
                    thumb_bottom_px,
                    thumb_visible_left_px,
                    thumb_visible_top_px,
                    thumb_visible_right_px,
                    thumb_visible_bottom_px,
                    painted_thumb_left_px,
                    painted_thumb_top_px,
                    painted_thumb_right_px,
                    painted_thumb_bottom_px,
                    painted_thumb_visible_left_px,
                    painted_thumb_visible_top_px,
                    painted_thumb_visible_right_px,
                    painted_thumb_visible_bottom_px,
                    paint_clip_left_px,
                    paint_clip_top_px,
                    paint_clip_right_px,
                    paint_clip_bottom_px,
                    client_width_px,
                    client_height_px,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "designer_action_editor_scroll",
                        elapsed_ms,
                        trace_sequence,
                        editor_surface = surface,
                        editor_session_id,
                        draft_generation,
                        stable_target_digest,
                        editor_epoch,
                        edit_generation,
                        query_generation,
                        query_request_generation,
                        search_request_generation,
                        test_request_generation,
                        query_digest,
                        editor_assigned_binding_digest,
                        scroll_id,
                        frame_nr,
                        offset_y_milli,
                        velocity_y_milli,
                        content_height_milli,
                        inner_height_milli,
                        pixels_per_point_milli,
                        handle_min_length_milli,
                        inner_left_px,
                        inner_top_px,
                        inner_right_px,
                        inner_bottom_px,
                        inner_visible_left_px,
                        inner_visible_top_px,
                        inner_visible_right_px,
                        inner_visible_bottom_px,
                        track_left_px,
                        track_top_px,
                        track_right_px,
                        track_bottom_px,
                        track_visible_left_px,
                        track_visible_top_px,
                        track_visible_right_px,
                        track_visible_bottom_px,
                        thumb_left_px,
                        thumb_top_px,
                        thumb_right_px,
                        thumb_bottom_px,
                        thumb_visible_left_px,
                        thumb_visible_top_px,
                        thumb_visible_right_px,
                        thumb_visible_bottom_px,
                        painted_thumb_left_px,
                        painted_thumb_top_px,
                        painted_thumb_right_px,
                        painted_thumb_bottom_px,
                        painted_thumb_visible_left_px,
                        painted_thumb_visible_top_px,
                        painted_thumb_visible_right_px,
                        painted_thumb_visible_bottom_px,
                        paint_clip_left_px,
                        paint_clip_top_px,
                        paint_clip_right_px,
                        paint_clip_bottom_px,
                        client_width_px,
                        client_height_px,
                        "radial acceptance trace"
                    );
                }
                Event::DesignerInspectorCellTextEdit {
                    target_digest,
                    session_id,
                    generation,
                    value_digest,
                    left_px,
                    top_px,
                    right_px,
                    bottom_px,
                    clip_left_px,
                    clip_top_px,
                    clip_right_px,
                    clip_bottom_px,
                    client_width_px,
                    client_height_px,
                    visible,
                    fully_visible,
                    focused,
                    clicked,
                    changed,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "designer_inspector_cell_text_edit",
                        elapsed_ms,
                        target_digest,
                        session_id,
                        generation,
                        value_digest,
                        trace_sequence,
                        left_px,
                        top_px,
                        right_px,
                        bottom_px,
                        clip_left_px,
                        clip_top_px,
                        clip_right_px,
                        clip_bottom_px,
                        client_width_px,
                        client_height_px,
                        visible,
                        fully_visible,
                        focused,
                        clicked,
                        changed,
                        "radial acceptance trace"
                    );
                }
                Event::RadialInsertionControl {
                    control,
                    widget_part,
                    request_id,
                    source_target_digest,
                    source_action_digest,
                    source_binding_digest,
                    source_query_digest,
                    destination_menu_digest,
                    destination_ring_digest,
                    destination_cell_digest,
                    session_id,
                    generation,
                    enabled,
                    selected,
                    clicked,
                    left_px,
                    top_px,
                    right_px,
                    bottom_px,
                    full_left_px,
                    full_top_px,
                    full_right_px,
                    full_bottom_px,
                    client_width_px,
                    client_height_px,
                    visible,
                    fully_visible,
                    document_digest_after,
                    binding_digest_after,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "radial_insertion_control",
                        elapsed_ms,
                        insertion_control = control,
                        widget_part,
                        request_id,
                        source_target_digest,
                        source_action_digest,
                        source_binding_digest,
                        source_query_digest,
                        destination_menu_digest,
                        destination_ring_digest,
                        destination_cell_digest,
                        session_id,
                        generation,
                        enabled,
                        selected,
                        clicked,
                        left_px,
                        top_px,
                        right_px,
                        bottom_px,
                        full_left_px,
                        full_top_px,
                        full_right_px,
                        full_bottom_px,
                        client_width_px,
                        client_height_px,
                        visible,
                        fully_visible,
                        document_digest_after,
                        binding_digest_after,
                        trace_sequence,
                        "radial acceptance trace"
                    );
                }
                Event::AuthoringProviderSearch {
                    edge,
                    kind,
                    editor_surface,
                    editor_session_id,
                    draft_generation,
                    stable_target_digest,
                    editor_epoch,
                    edit_generation,
                    query_generation,
                    query_request_generation,
                    search_request_generation,
                    test_request_generation,
                    query_digest,
                    binding_digest,
                    editor_assigned_binding_digest,
                    provider_revision,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "authoring_provider_search",
                        elapsed_ms,
                        authoring_request_edge = edge,
                        authoring_search_kind = kind,
                        editor_surface,
                        editor_session_id,
                        draft_generation,
                        stable_target_digest,
                        editor_epoch,
                        edit_generation,
                        query_generation,
                        query_request_generation,
                        search_request_generation,
                        test_request_generation,
                        query_digest,
                        binding_digest,
                        editor_assigned_binding_digest,
                        provider_revision = provider_revision.map_or(-1, |revision| revision as i64),
                        trace_sequence,
                        "radial acceptance trace"
                    );
                }
                Event::AuthoringObservationBoundary {
                    phase,
                    request_id,
                    baseline_request_id,
                    captured_trace_sequence: _,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "authoring_observation_boundary",
                        elapsed_ms,
                        phase,
                        request_id,
                        baseline_request_id = baseline_request_id.map_or(0, |id| id),
                        captured_trace_sequence,
                        trace_sequence,
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
                        trace_sequence,
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
                Event::RadialDispatchRequested {
                    invocation_id,
                    session_generation,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "radial_dispatch_requested",
                        elapsed_ms,
                        invocation_id,
                        session_generation,
                        "radial acceptance trace"
                    );
                }
                Event::RuntimeRadialHover {
                    session_digest,
                    cell_digest,
                    layout_generation,
                    role,
                    executable,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "runtime_radial_hover",
                        elapsed_ms,
                        session_digest,
                        cell_digest,
                        layout_generation,
                        role,
                        executable,
                        "radial acceptance trace"
                    );
                }
                Event::RadialQueryResolution {
                    invocation_id,
                    session_digest,
                    cell_digest,
                    session_generation,
                    config_revision,
                    preparation_generation,
                    query_digest,
                    mode,
                    state,
                    provider_revision,
                    result_count,
                    result_digest,
                    selected_digest,
                    interaction_requirement,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "radial_query_resolution",
                        elapsed_ms,
                        invocation_id,
                        session_digest,
                        cell_digest,
                        session_generation,
                        config_revision,
                        preparation_generation,
                        query_digest,
                        mode,
                        state,
                        provider_revision,
                        result_count,
                        result_digest,
                        selected_digest = selected_digest.unwrap_or(0),
                        interaction_requirement,
                        "radial acceptance trace"
                    );
                }
                Event::RadialQueryDispatch {
                    invocation_id,
                    session_digest,
                    cell_digest,
                    session_generation,
                    config_revision,
                    mode,
                    query_digest,
                    selected_digest,
                    interaction_requirement,
                    root_policy,
                    outcome,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "radial_query_dispatch",
                        elapsed_ms,
                        invocation_id,
                        session_digest,
                        cell_digest,
                        session_generation,
                        config_revision,
                        mode,
                        query_digest,
                        selected_digest,
                        interaction_requirement,
                        root_policy,
                        outcome,
                        "radial acceptance trace"
                    );
                }
                Event::RadialRootSnapshot {
                    phase,
                    invocation_id,
                    session_digest,
                    cell_digest,
                    query_digest,
                    action_digest,
                    source,
                    state_digest,
                    ordinary_query_digest,
                    results_digest,
                    results_count,
                    selected_index,
                    grid_layout,
                    visible,
                    restore,
                    visibility_revision,
                    focus_query,
                    move_cursor_end,
                    last_results_valid,
                    last_search_query_digest,
                    suggestions_digest,
                    autocomplete_index,
                    query_history_digest,
                    matching_history_count,
                    radial_source_history_count,
                    usage_count,
                } => {
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "radial_root_snapshot",
                        elapsed_ms,
                        phase,
                        invocation_id,
                        session_digest,
                        cell_digest,
                        query_digest,
                        action_digest,
                        source,
                        state_digest,
                        ordinary_query_digest,
                        results_digest,
                        results_count,
                        selected_index,
                        grid_layout,
                        visible,
                        restore,
                        visibility_revision,
                        focus_query,
                        move_cursor_end,
                        last_results_valid,
                        last_search_query_digest,
                        suggestions_digest,
                        autocomplete_index,
                        query_history_digest,
                        matching_history_count,
                        radial_source_history_count,
                        usage_count,
                        "radial acceptance trace"
                    );
                }
                Event::DesiredVisibility {
                    visible,
                    revision,
                    source,
                    invocation_id,
                } => {
                    let invocation_id = invocation_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "desired_visibility",
                        elapsed_ms,
                        visible,
                        revision,
                        ?source,
                        invocation_id,
                        "radial acceptance trace"
                    );
                }
                Event::ScreenDrawRestoreFocusIntent {
                    revision,
                    invocation_id,
                    focus_intent,
                } => {
                    let invocation_id = invocation_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    tracing::warn!(
                        target: TRACE_TARGET,
                        trace_event = "screen_draw_restore_focus_intent",
                        elapsed_ms,
                        revision,
                        invocation_id,
                        ?focus_intent,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                    process_id,
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
                        process_id,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
                        visibility_revision = correlation.visibility_revision,
                        invocation_id = correlation.invocation_id,
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
            },
        },
    );
}

/// Publishes a GUI-owner observation receipt after all trace lines through the
/// captured cursor have been submitted to the same tracing sink. The native
/// runner waits for this line before reading the mailbox response, so an ACK
/// alone never stands in for buffered trace output.
pub(crate) fn emit_authoring_observation_boundary(
    phase: &'static str,
    request_id: u64,
    baseline_request_id: Option<u64>,
) -> Option<(u64, u64)> {
    let runtime = runtime();
    if !runtime.enabled {
        #[cfg(test)]
        {
            return Some(TRACE_PUBLICATION_FENCE.publish_with_elapsed(
                || elapsed_ms() as u64,
                |captured, sequence, _| (captured, sequence),
            ));
        }
        #[cfg(not(test))]
        return None;
    }
    publish_authoring_observation_boundary_with_budget(
        phase,
        request_id,
        baseline_request_id,
        &runtime.budget,
        &TRACE_PUBLICATION_FENCE,
        |entry| match entry {
            BudgetTraceEntry::Exhausted {
                trace_sequence,
                elapsed_ms,
                event_budget,
                reserved_event_budget,
            } => tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "budget_exhausted",
                trace_sequence,
                elapsed_ms,
                event_budget = event_budget as u64,
                reserved_event_budget = reserved_event_budget as u64,
                "radial acceptance trace"
            ),
            BudgetTraceEntry::Event {
                event:
                    Event::AuthoringObservationBoundary {
                        phase,
                        request_id,
                        baseline_request_id,
                        captured_trace_sequence: _,
                    },
                captured_trace_sequence,
                trace_sequence,
                elapsed_ms,
            } => tracing::warn!(
                target: TRACE_TARGET,
                trace_event = "authoring_observation_boundary",
                elapsed_ms,
                phase,
                request_id,
                baseline_request_id = baseline_request_id.map_or(0, |id| id),
                captured_trace_sequence,
                trace_sequence,
                "radial acceptance trace"
            ),
            BudgetTraceEntry::Event { .. } => {}
        },
    )
}

fn publish_authoring_observation_boundary_with_budget(
    phase: &'static str,
    request_id: u64,
    baseline_request_id: Option<u64>,
    budget: &EventBudget,
    fence: &TracePublicationFence,
    mut publish: impl FnMut(BudgetTraceEntry),
) -> Option<(u64, u64)> {
    let mut receipt = None;
    let admission = publish_budgeted_event(
        Event::AuthoringObservationBoundary {
            phase,
            request_id,
            baseline_request_id,
            captured_trace_sequence: 0,
        },
        budget,
        fence,
        |entry| {
            if let BudgetTraceEntry::Event {
                event: Event::AuthoringObservationBoundary { .. },
                captured_trace_sequence,
                trace_sequence,
                ..
            } = entry
            {
                receipt = Some((captured_trace_sequence, trace_sequence));
            }
            publish(entry);
        },
    );
    (admission != EventBudgetAdmission::Rejected)
        .then_some(receipt)
        .flatten()
}

pub fn emit_radial_dispatch_requested(invocation_id: u64, session_generation: u64) {
    emit(Event::RadialDispatchRequested {
        invocation_id,
        session_generation,
    });
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
    clip_bounds: [i32; 4],
    client_size: [i32; 2],
    is_enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    session_id: u64,
    generation: u64,
    scope: Option<DesignerAuthoringControlScope>,
) {
    if !enabled()
        || bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || client_size[0] <= 0
        || client_size[1] <= 0
        || clip_bounds[0] < 0
        || clip_bounds[1] < 0
        || clip_bounds[2] > client_size[0]
        || clip_bounds[3] > client_size[1]
        || clip_bounds[2] <= clip_bounds[0]
        || clip_bounds[3] <= clip_bounds[1]
    {
        return;
    }
    let now_ms = elapsed_ms();
    let snapshot = DesignerAuthoringControlSnapshot {
        target,
        role,
        viewport,
        session_id,
        index,
        bounds,
        clip_bounds,
        client_size,
        enabled: is_enabled,
        selected,
        focused,
        clicked,
        generation,
        scope,
        last_emitted_ms: now_ms,
    };
    let changed = DESIGNER_AUTHORING_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut previous| {
            insert_designer_authoring_control_snapshot(&mut previous, snapshot, now_ms)
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
            clip_left_px: clip_bounds[0],
            clip_top_px: clip_bounds[1],
            clip_right_px: clip_bounds[2],
            clip_bottom_px: clip_bounds[3],
            client_width_px: client_size[0],
            client_height_px: client_size[1],
            enabled: is_enabled,
            selected,
            focused,
            clicked,
            session_id,
            generation,
            scope,
        });
    }
}

pub(crate) fn emit_action_editor_control(
    identity: &crate::gui::AuthoringBindingEditorIdentity,
    control: &'static str,
    control_index: Option<usize>,
    target_digest: u64,
    title_digest: u64,
    type_digest: u64,
    disambiguator_digest: u64,
    action_digest: u64,
    binding_digest: u64,
    query_digest: u64,
    value_digest: u64,
    bounds: [i32; 4],
    full_bounds: [i32; 4],
    client_size: [i32; 2],
    displayed_text_digest: u64,
    fully_visible: bool,
    is_enabled: bool,
    selected: bool,
    focused: bool,
    clicked: bool,
    changed: bool,
    enter_pressed: bool,
) {
    if (!enabled() && !cfg!(test))
        || bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || client_size[0] <= 0
        || client_size[1] <= 0
    {
        return;
    }
    let surface = match identity.scope.surface {
        crate::gui::BindingEditorSurface::Properties => "properties",
        crate::gui::BindingEditorSurface::Inspector => "inspector",
    };
    let mut target_hasher = std::collections::hash_map::DefaultHasher::new();
    identity.scope.target.hash(&mut target_hasher);
    let stable_target_digest = target_hasher.finish();
    let now_ms = elapsed_ms();
    let snapshot = ActionEditorControlSnapshot {
        surface,
        control,
        editor_session_id: identity.scope.editor_session.0,
        draft_generation: identity.scope.draft_generation.0,
        stable_target_digest,
        editor_epoch: identity.editor_epoch,
        edit_generation: identity.edit_generation,
        query_generation: identity.query_generation,
        query_request_generation: identity.query_request_generation,
        search_request_generation: identity.search_request_generation,
        test_request_generation: identity.test_request_generation,
        control_index,
        target_digest,
        title_digest,
        type_digest,
        disambiguator_digest,
        action_digest,
        binding_digest,
        query_digest,
        value_digest,
        displayed_text_digest,
        editor_assigned_binding_digest: identity.assigned_binding_digest,
        bounds,
        full_bounds,
        client_size,
        fully_visible,
        enabled: is_enabled,
        selected,
        focused,
        clicked,
        changed,
        enter_pressed,
        last_emitted_ms: now_ms,
    };
    #[cfg(test)]
    TEST_ACTION_EDITOR_CONTROL_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(action_editor_control_event(&snapshot, changed));
    });
    if !enabled() {
        return;
    }
    let event = ACTION_EDITOR_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .ok()
        .and_then(|mut previous| {
            let mut published = None;
            publish_cached_action_editor_control(
                &mut previous,
                snapshot,
                changed,
                now_ms,
                |event| published = Some(event),
            );
            published
        });
    if let Some(event) = event {
        emit(event);
    }
}

pub(crate) fn emit_action_editor_scroll(
    identity: &crate::gui::AuthoringBindingEditorIdentity,
    observation: ActionEditorScrollObservation,
) {
    if (!enabled() && !cfg!(test)) || !action_editor_scroll_observation_is_valid(&observation) {
        return;
    }
    let surface = match identity.scope.surface {
        crate::gui::BindingEditorSurface::Properties => "properties",
        crate::gui::BindingEditorSurface::Inspector => "inspector",
    };
    let mut target_hasher = std::collections::hash_map::DefaultHasher::new();
    identity.scope.target.hash(&mut target_hasher);
    let now_ms = elapsed_ms();
    let snapshot = ActionEditorScrollSnapshot {
        surface,
        editor_session_id: identity.scope.editor_session.0,
        draft_generation: identity.scope.draft_generation.0,
        stable_target_digest: target_hasher.finish(),
        editor_epoch: identity.editor_epoch,
        edit_generation: identity.edit_generation,
        query_generation: identity.query_generation,
        query_request_generation: identity.query_request_generation,
        search_request_generation: identity.search_request_generation,
        test_request_generation: identity.test_request_generation,
        query_digest: observation.query_digest,
        editor_assigned_binding_digest: identity.assigned_binding_digest,
        scroll_id: observation.scroll_id,
        frame_nr: observation.frame_nr,
        offset_y_milli: observation.offset_y_milli,
        velocity_y_milli: observation.velocity_y_milli,
        content_height_milli: observation.content_height_milli,
        inner_height_milli: observation.inner_height_milli,
        pixels_per_point_milli: observation.pixels_per_point_milli,
        handle_min_length_milli: observation.handle_min_length_milli,
        inner_bounds: observation.inner_bounds,
        inner_visible_bounds: observation.inner_visible_bounds,
        track_bounds: observation.track_bounds,
        track_visible_bounds: observation.track_visible_bounds,
        thumb_bounds: observation.thumb_bounds,
        thumb_visible_bounds: observation.thumb_visible_bounds,
        painted_thumb_bounds: observation.painted_thumb_bounds,
        painted_thumb_visible_bounds: observation.painted_thumb_visible_bounds,
        paint_clip_bounds: observation.paint_clip_bounds,
        client_size: observation.client_size,
        last_emitted_ms: now_ms,
    };
    let event = ACTION_EDITOR_SCROLLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .ok()
        .and_then(|mut previous| {
            cached_action_editor_scroll_event(&mut previous, snapshot, now_ms)
        });
    if let Some(event) = event {
        #[cfg(test)]
        TEST_ACTION_EDITOR_SCROLL_EVENTS.with(|events| events.borrow_mut().push(event));
        if enabled() {
            emit(event);
        }
    }
}

fn action_editor_scroll_observation_is_valid(observation: &ActionEditorScrollObservation) -> bool {
    let rect_is_positive = |bounds: [i32; 4]| bounds[2] > bounds[0] && bounds[3] > bounds[1];
    let rect_contains = |outer: [i32; 4], inner: [i32; 4]| {
        outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
    };
    let client = [0, 0, observation.client_size[0], observation.client_size[1]];
    if observation.client_size[0] <= 0
        || observation.client_size[1] <= 0
        || observation.content_height_milli <= observation.inner_height_milli
        || observation.inner_height_milli <= 0
        || observation.pixels_per_point_milli <= 0
        || observation.handle_min_length_milli <= 0
        || observation.offset_y_milli < 0
        || observation.offset_y_milli
            > observation.content_height_milli - observation.inner_height_milli
        || !rect_is_positive(observation.inner_bounds)
        || !rect_is_positive(observation.inner_visible_bounds)
        || !rect_is_positive(observation.track_bounds)
        || !rect_is_positive(observation.track_visible_bounds)
        || !rect_is_positive(observation.thumb_bounds)
        || !rect_is_positive(observation.thumb_visible_bounds)
        || !rect_is_positive(observation.painted_thumb_bounds)
        || !rect_is_positive(observation.painted_thumb_visible_bounds)
        || !rect_is_positive(observation.paint_clip_bounds)
        || !rect_contains(observation.inner_bounds, observation.inner_visible_bounds)
        || !rect_contains(observation.track_bounds, observation.track_visible_bounds)
        || !rect_contains(
            observation.track_visible_bounds,
            observation.thumb_visible_bounds,
        )
        || !rect_contains(observation.thumb_bounds, observation.thumb_visible_bounds)
        || !rect_contains(
            observation.painted_thumb_bounds,
            observation.painted_thumb_visible_bounds,
        )
        || !rect_contains(client, observation.inner_visible_bounds)
        || !rect_contains(client, observation.track_visible_bounds)
        || !rect_contains(client, observation.thumb_visible_bounds)
        || !rect_contains(client, observation.painted_thumb_visible_bounds)
        || !rect_contains(client, observation.paint_clip_bounds)
        || !rect_contains(
            observation.paint_clip_bounds,
            observation.painted_thumb_visible_bounds,
        )
        || observation.track_bounds[1] != observation.inner_bounds[1]
        || observation.track_bounds[3] != observation.inner_bounds[3]
        || observation.thumb_bounds[0] != observation.track_bounds[0]
        || observation.thumb_bounds[2] != observation.track_bounds[2]
        || observation.painted_thumb_bounds[0] != observation.track_bounds[0]
        || observation.painted_thumb_bounds[2] != observation.track_bounds[2]
    {
        return false;
    }
    let inner_height_px = i128::from(observation.inner_bounds[3] - observation.inner_bounds[1]);
    let content_height = i128::from(observation.content_height_milli);
    let expected_top = i128::from(observation.inner_bounds[1])
        + (i128::from(observation.offset_y_milli) * inner_height_px + content_height / 2)
            / content_height;
    let expected_bottom = i128::from(observation.inner_bounds[1])
        + ((i128::from(observation.offset_y_milli) + i128::from(observation.inner_height_milli))
            * inner_height_px
            + content_height / 2)
            / content_height;
    let expected_painted_visible = [
        observation.painted_thumb_bounds[0].max(observation.paint_clip_bounds[0]),
        observation.painted_thumb_bounds[1].max(observation.paint_clip_bounds[1]),
        observation.painted_thumb_bounds[2].min(observation.paint_clip_bounds[2]),
        observation.painted_thumb_bounds[3].min(observation.paint_clip_bounds[3]),
    ];
    let raw_height = i128::from(observation.thumb_bounds[3] - observation.thumb_bounds[1]);
    let min_height_px = (i128::from(observation.handle_min_length_milli)
        * i128::from(observation.pixels_per_point_milli)
        + 500_000)
        / 1_000_000;
    let expected_painted_height = raw_height.max(min_height_px);
    let painted_height =
        i128::from(observation.painted_thumb_bounds[3] - observation.painted_thumb_bounds[1]);
    (i128::from(observation.thumb_bounds[1]) - expected_top).abs() <= 2
        && (i128::from(observation.thumb_bounds[3]) - expected_bottom).abs() <= 2
        && (i128::from(observation.painted_thumb_bounds[1])
            + i128::from(observation.painted_thumb_bounds[3])
            - i128::from(observation.thumb_bounds[1])
            - i128::from(observation.thumb_bounds[3]))
        .abs()
            <= 2
        && (painted_height - expected_painted_height).abs() <= 2
        && observation.painted_thumb_visible_bounds == expected_painted_visible
}

fn action_editor_scroll_snapshot_changed(
    previous: &ActionEditorScrollSnapshot,
    next: &ActionEditorScrollSnapshot,
) -> bool {
    let mut comparable = *next;
    comparable.frame_nr = previous.frame_nr;
    comparable.last_emitted_ms = previous.last_emitted_ms;
    *previous != comparable
}

fn action_editor_scroll_event(snapshot: &ActionEditorScrollSnapshot) -> Event {
    Event::DesignerActionEditorScroll {
        surface: snapshot.surface,
        editor_session_id: snapshot.editor_session_id,
        draft_generation: snapshot.draft_generation,
        stable_target_digest: snapshot.stable_target_digest,
        editor_epoch: snapshot.editor_epoch,
        edit_generation: snapshot.edit_generation,
        query_generation: snapshot.query_generation,
        query_request_generation: snapshot.query_request_generation,
        search_request_generation: snapshot.search_request_generation,
        test_request_generation: snapshot.test_request_generation,
        query_digest: snapshot.query_digest,
        editor_assigned_binding_digest: snapshot.editor_assigned_binding_digest,
        scroll_id: snapshot.scroll_id,
        frame_nr: snapshot.frame_nr,
        offset_y_milli: snapshot.offset_y_milli,
        velocity_y_milli: snapshot.velocity_y_milli,
        content_height_milli: snapshot.content_height_milli,
        inner_height_milli: snapshot.inner_height_milli,
        pixels_per_point_milli: snapshot.pixels_per_point_milli,
        handle_min_length_milli: snapshot.handle_min_length_milli,
        inner_left_px: snapshot.inner_bounds[0],
        inner_top_px: snapshot.inner_bounds[1],
        inner_right_px: snapshot.inner_bounds[2],
        inner_bottom_px: snapshot.inner_bounds[3],
        inner_visible_left_px: snapshot.inner_visible_bounds[0],
        inner_visible_top_px: snapshot.inner_visible_bounds[1],
        inner_visible_right_px: snapshot.inner_visible_bounds[2],
        inner_visible_bottom_px: snapshot.inner_visible_bounds[3],
        track_left_px: snapshot.track_bounds[0],
        track_top_px: snapshot.track_bounds[1],
        track_right_px: snapshot.track_bounds[2],
        track_bottom_px: snapshot.track_bounds[3],
        track_visible_left_px: snapshot.track_visible_bounds[0],
        track_visible_top_px: snapshot.track_visible_bounds[1],
        track_visible_right_px: snapshot.track_visible_bounds[2],
        track_visible_bottom_px: snapshot.track_visible_bounds[3],
        thumb_left_px: snapshot.thumb_bounds[0],
        thumb_top_px: snapshot.thumb_bounds[1],
        thumb_right_px: snapshot.thumb_bounds[2],
        thumb_bottom_px: snapshot.thumb_bounds[3],
        thumb_visible_left_px: snapshot.thumb_visible_bounds[0],
        thumb_visible_top_px: snapshot.thumb_visible_bounds[1],
        thumb_visible_right_px: snapshot.thumb_visible_bounds[2],
        thumb_visible_bottom_px: snapshot.thumb_visible_bounds[3],
        painted_thumb_left_px: snapshot.painted_thumb_bounds[0],
        painted_thumb_top_px: snapshot.painted_thumb_bounds[1],
        painted_thumb_right_px: snapshot.painted_thumb_bounds[2],
        painted_thumb_bottom_px: snapshot.painted_thumb_bounds[3],
        painted_thumb_visible_left_px: snapshot.painted_thumb_visible_bounds[0],
        painted_thumb_visible_top_px: snapshot.painted_thumb_visible_bounds[1],
        painted_thumb_visible_right_px: snapshot.painted_thumb_visible_bounds[2],
        painted_thumb_visible_bottom_px: snapshot.painted_thumb_visible_bounds[3],
        paint_clip_left_px: snapshot.paint_clip_bounds[0],
        paint_clip_top_px: snapshot.paint_clip_bounds[1],
        paint_clip_right_px: snapshot.paint_clip_bounds[2],
        paint_clip_bottom_px: snapshot.paint_clip_bounds[3],
        client_width_px: snapshot.client_size[0],
        client_height_px: snapshot.client_size[1],
    }
}

fn cached_action_editor_scroll_event(
    previous: &mut Vec<ActionEditorScrollSnapshot>,
    snapshot: ActionEditorScrollSnapshot,
    now_ms: u128,
) -> Option<Event> {
    let key_matches = |old: &ActionEditorScrollSnapshot| {
        old.surface == snapshot.surface
            && old.editor_session_id == snapshot.editor_session_id
            && old.stable_target_digest == snapshot.stable_target_digest
            && old.editor_epoch == snapshot.editor_epoch
            && old.scroll_id == snapshot.scroll_id
    };
    if let Some(index) = previous.iter().position(key_matches) {
        let old = previous[index];
        let changed = action_editor_scroll_snapshot_changed(&old, &snapshot)
            || now_ms.saturating_sub(old.last_emitted_ms) >= AUTHORING_CONTROL_REFRESH_MS;
        if !changed {
            return None;
        }
        previous[index] = snapshot;
        return Some(action_editor_scroll_event(&snapshot));
    }
    if previous.len() >= 128
        && let Some(oldest) = previous
            .iter()
            .enumerate()
            .min_by_key(|(_, item)| item.last_emitted_ms)
            .map(|(index, _)| index)
    {
        previous.remove(oldest);
    }
    previous.push(snapshot);
    Some(action_editor_scroll_event(&snapshot))
}

pub(crate) fn emit_inspector_cell_text_edit(
    menu_id: &str,
    ring_id: &str,
    cell_id: &str,
    session_id: u64,
    generation: u64,
    value: &str,
    bounds: [i32; 4],
    clip_bounds: [i32; 4],
    client_size: [i32; 2],
    focused: bool,
    clicked: bool,
    changed: bool,
) {
    if (!enabled() && !cfg!(test))
        || session_id == 0
        || generation == 0
        || client_size[0] <= 0
        || client_size[1] <= 0
    {
        return;
    }
    let visible = clip_bounds[2] > clip_bounds[0] && clip_bounds[3] > clip_bounds[1];
    if !visible {
        return;
    }
    let fully_visible = bounds == clip_bounds
        && bounds[0] >= 0
        && bounds[1] >= 0
        && bounds[2] <= client_size[0]
        && bounds[3] <= client_size[1];
    let now_ms = elapsed_ms();
    let snapshot = InspectorCellTextEditSnapshot {
        target_digest: private_trace_parts_digest(&[menu_id, ring_id, cell_id]),
        session_id,
        generation,
        value_digest: private_trace_text_digest(value),
        bounds,
        clip_bounds,
        client_size,
        visible,
        fully_visible,
        focused,
        clicked,
        changed,
        last_emitted_ms: now_ms,
    };
    let event = INSPECTOR_CELL_TEXT_EDITS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .ok()
        .and_then(|mut previous| {
            let key = (snapshot.target_digest, snapshot.session_id);
            let changed = previous
                .iter()
                .find(|old| (old.target_digest, old.session_id) == key)
                .is_none_or(|old| {
                    old.generation != snapshot.generation
                        || old.value_digest != snapshot.value_digest
                        || old.bounds != snapshot.bounds
                        || old.clip_bounds != snapshot.clip_bounds
                        || old.client_size != snapshot.client_size
                        || old.visible != snapshot.visible
                        || old.fully_visible != snapshot.fully_visible
                        || old.focused != snapshot.focused
                        || old.clicked != snapshot.clicked
                        || old.changed != snapshot.changed
                        || now_ms.saturating_sub(old.last_emitted_ms)
                            >= AUTHORING_CONTROL_REFRESH_MS
                });
            if !changed {
                return None;
            }
            if let Some(old) = previous
                .iter_mut()
                .find(|old| (old.target_digest, old.session_id) == key)
            {
                *old = snapshot;
            } else {
                if previous.len() >= 128 {
                    previous.remove(0);
                }
                previous.push(snapshot);
            }
            Some(inspector_cell_text_edit_event(snapshot))
        });
    if let Some(event) = event {
        #[cfg(test)]
        TEST_INSPECTOR_TEXT_EDIT_EVENTS.with(|events| events.borrow_mut().push(event));
        if !enabled() {
            return;
        }
        emit(event);
    }
}

fn inspector_cell_text_edit_event(snapshot: InspectorCellTextEditSnapshot) -> Event {
    Event::DesignerInspectorCellTextEdit {
        target_digest: snapshot.target_digest,
        session_id: snapshot.session_id,
        generation: snapshot.generation,
        value_digest: snapshot.value_digest,
        left_px: snapshot.bounds[0],
        top_px: snapshot.bounds[1],
        right_px: snapshot.bounds[2],
        bottom_px: snapshot.bounds[3],
        clip_left_px: snapshot.clip_bounds[0],
        clip_top_px: snapshot.clip_bounds[1],
        clip_right_px: snapshot.clip_bounds[2],
        clip_bottom_px: snapshot.clip_bounds[3],
        client_width_px: snapshot.client_size[0],
        client_height_px: snapshot.client_size[1],
        visible: snapshot.visible,
        fully_visible: snapshot.fully_visible,
        focused: snapshot.focused,
        clicked: snapshot.clicked,
        changed: snapshot.changed,
    }
}

fn cached_action_editor_control_event(
    previous: &mut Vec<ActionEditorControlSnapshot>,
    snapshot: ActionEditorControlSnapshot,
    widget_changed: bool,
    now_ms: u128,
) -> Option<Event> {
    let event = action_editor_control_event(&snapshot, widget_changed);
    insert_action_editor_control_snapshot(previous, snapshot, now_ms).then_some(event)
}

fn publish_cached_action_editor_control(
    previous: &mut Vec<ActionEditorControlSnapshot>,
    snapshot: ActionEditorControlSnapshot,
    widget_changed: bool,
    now_ms: u128,
    mut publish: impl FnMut(Event),
) {
    if let Some(event) =
        cached_action_editor_control_event(previous, snapshot, widget_changed, now_ms)
    {
        publish(event);
    }
}

fn action_editor_control_event(
    snapshot: &ActionEditorControlSnapshot,
    widget_changed: bool,
) -> Event {
    Event::DesignerActionEditorControl {
        surface: snapshot.surface,
        control: snapshot.control,
        control_index: snapshot.control_index,
        target_digest: snapshot.target_digest,
        title_digest: snapshot.title_digest,
        type_digest: snapshot.type_digest,
        disambiguator_digest: snapshot.disambiguator_digest,
        action_digest: snapshot.action_digest,
        binding_digest: snapshot.binding_digest,
        query_digest: snapshot.query_digest,
        value_digest: snapshot.value_digest,
        displayed_text_digest: snapshot.displayed_text_digest,
        editor_assigned_binding_digest: snapshot.editor_assigned_binding_digest,
        editor_session_id: snapshot.editor_session_id,
        draft_generation: snapshot.draft_generation,
        stable_target_digest: snapshot.stable_target_digest,
        editor_epoch: snapshot.editor_epoch,
        edit_generation: snapshot.edit_generation,
        query_generation: snapshot.query_generation,
        query_request_generation: snapshot.query_request_generation,
        search_request_generation: snapshot.search_request_generation,
        test_request_generation: snapshot.test_request_generation,
        left_px: snapshot.bounds[0],
        top_px: snapshot.bounds[1],
        right_px: snapshot.bounds[2],
        bottom_px: snapshot.bounds[3],
        full_left_px: snapshot.full_bounds[0],
        full_top_px: snapshot.full_bounds[1],
        full_right_px: snapshot.full_bounds[2],
        full_bottom_px: snapshot.full_bounds[3],
        client_width_px: snapshot.client_size[0],
        client_height_px: snapshot.client_size[1],
        fully_visible: snapshot.fully_visible,
        enabled: snapshot.enabled,
        selected: snapshot.selected,
        focused: snapshot.focused,
        clicked: snapshot.clicked,
        changed: widget_changed,
        enter_pressed: snapshot.enter_pressed,
    }
}

fn action_editor_control_snapshot_should_emit(
    left: &ActionEditorControlSnapshot,
    right: &ActionEditorControlSnapshot,
    now_ms: u128,
) -> bool {
    let unchanged = left.surface == right.surface
        && left.control == right.control
        && left.editor_session_id == right.editor_session_id
        && left.draft_generation == right.draft_generation
        && left.stable_target_digest == right.stable_target_digest
        && left.editor_epoch == right.editor_epoch
        && left.edit_generation == right.edit_generation
        && left.query_generation == right.query_generation
        && left.query_request_generation == right.query_request_generation
        && left.search_request_generation == right.search_request_generation
        && left.test_request_generation == right.test_request_generation
        && left.control_index == right.control_index
        && left.target_digest == right.target_digest
        && left.title_digest == right.title_digest
        && left.type_digest == right.type_digest
        && left.disambiguator_digest == right.disambiguator_digest
        && left.action_digest == right.action_digest
        && left.binding_digest == right.binding_digest
        && left.query_digest == right.query_digest
        && left.value_digest == right.value_digest
        && left.displayed_text_digest == right.displayed_text_digest
        && left.editor_assigned_binding_digest == right.editor_assigned_binding_digest
        && left.bounds == right.bounds
        && left.full_bounds == right.full_bounds
        && left.client_size == right.client_size
        && left.fully_visible == right.fully_visible
        && left.enabled == right.enabled
        && left.selected == right.selected
        && left.focused == right.focused
        && left.clicked == right.clicked
        && left.changed == right.changed
        && left.enter_pressed == right.enter_pressed;
    !unchanged || now_ms.saturating_sub(left.last_emitted_ms) >= AUTHORING_CONTROL_REFRESH_MS
}

fn insert_action_editor_control_snapshot(
    previous: &mut Vec<ActionEditorControlSnapshot>,
    snapshot: ActionEditorControlSnapshot,
    now_ms: u128,
) -> bool {
    if let Some(existing) = previous.iter_mut().find(|item| {
        item.surface == snapshot.surface
            && item.control == snapshot.control
            && item.editor_session_id == snapshot.editor_session_id
            && item.stable_target_digest == snapshot.stable_target_digest
            && item.editor_epoch == snapshot.editor_epoch
            && item.control_index == snapshot.control_index
    }) {
        if !action_editor_control_snapshot_should_emit(existing, &snapshot, now_ms) {
            return false;
        }
        *existing = snapshot;
        return true;
    }
    if previous.len() >= 512
        && let Some(oldest) = previous
            .iter()
            .enumerate()
            .min_by_key(|(_, item)| item.last_emitted_ms)
            .map(|(index, _)| index)
    {
        previous.remove(oldest);
    }
    previous.push(snapshot);
    true
}

pub(crate) fn emit_authoring_provider_search(
    identity: &crate::gui::AuthoringBindingEditorIdentity,
    edge: &'static str,
    kind: &'static str,
    query: &str,
    binding: Option<&crate::radial::model::ActionBinding>,
    provider_revision: Option<u64>,
) {
    #[cfg(not(test))]
    if !enabled() {
        return;
    }
    #[cfg(test)]
    let query_digest = private_trace_text_digest(query);
    #[cfg(not(test))]
    let query_digest = private_trace_text_digest(query);
    #[cfg(test)]
    let binding_digest = binding
        .and_then(|binding| serde_json::to_vec(binding).ok())
        .as_deref()
        .map_or(0, private_trace_digest);
    #[cfg(not(test))]
    let binding_digest = binding
        .and_then(|binding| serde_json::to_vec(binding).ok())
        .as_deref()
        .map_or(0, private_trace_digest);
    #[cfg(test)]
    TEST_AUTHORING_PROVIDER_TRACE.with(|events| {
        events.borrow_mut().push(AuthoringProviderTraceSnapshot {
            identity: identity.clone(),
            edge,
            kind,
            editor_surface: binding_editor_surface_label(identity.scope.surface),
            query_digest,
            binding_digest,
        });
    });
    if !enabled() {
        return;
    }
    emit(authoring_provider_search_event(
        identity,
        edge,
        kind,
        query_digest,
        binding_digest,
        provider_revision,
    ));
}

fn binding_editor_surface_label(surface: crate::gui::BindingEditorSurface) -> &'static str {
    match surface {
        crate::gui::BindingEditorSurface::Properties => "properties",
        crate::gui::BindingEditorSurface::Inspector => "inspector",
    }
}

fn authoring_provider_search_event(
    identity: &crate::gui::AuthoringBindingEditorIdentity,
    edge: &'static str,
    kind: &'static str,
    query_digest: u64,
    binding_digest: u64,
    provider_revision: Option<u64>,
) -> Event {
    let mut target_hasher = std::collections::hash_map::DefaultHasher::new();
    identity.scope.target.hash(&mut target_hasher);
    Event::AuthoringProviderSearch {
        edge,
        kind,
        editor_surface: binding_editor_surface_label(identity.scope.surface),
        editor_session_id: identity.scope.editor_session.0,
        draft_generation: identity.scope.draft_generation.0,
        stable_target_digest: target_hasher.finish(),
        editor_epoch: identity.editor_epoch,
        edit_generation: identity.edit_generation,
        query_generation: identity.query_generation,
        query_request_generation: identity.query_request_generation,
        search_request_generation: identity.search_request_generation,
        test_request_generation: identity.test_request_generation,
        query_digest,
        binding_digest,
        editor_assigned_binding_digest: identity.assigned_binding_digest,
        provider_revision,
    }
}

fn private_trace_digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn private_trace_parts_digest(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        hash = private_trace_digest_from(hash, part.as_bytes());
        hash = private_trace_digest_from(hash, &[0]);
    }
    hash
}

pub(crate) fn private_trace_serialized_digest<T: serde::Serialize>(value: &T) -> u64 {
    serde_json::to_vec(value)
        .ok()
        .map_or(0, |bytes| private_trace_digest(&bytes))
}

fn private_trace_digest_from(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn private_trace_text_digest(text: &str) -> u64 {
    let mut hash = private_trace_digest(text.as_bytes());
    hash ^= 0;
    hash.wrapping_mul(0x100000001b3)
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
        && left.clip_bounds == right.clip_bounds
        && left.client_size == right.client_size
        && left.enabled == right.enabled
        && left.selected == right.selected
        && left.focused == right.focused
        && left.clicked == right.clicked
        && left.generation == right.generation
        && left.scope == right.scope;
    !unchanged || now_ms.saturating_sub(left.last_emitted_ms) >= AUTHORING_CONTROL_REFRESH_MS
}

fn insert_designer_authoring_control_snapshot(
    previous: &mut Vec<DesignerAuthoringControlSnapshot>,
    mut snapshot: DesignerAuthoringControlSnapshot,
    now_ms: u128,
) -> bool {
    const MAX_SNAPSHOTS: usize = 256;
    if let Some(existing) = previous.iter_mut().find(|item| {
        item.target == snapshot.target
            && item.role == snapshot.role
            && item.viewport == snapshot.viewport
            && item.session_id == snapshot.session_id
            && item.index == snapshot.index
    }) {
        if !authoring_control_snapshot_should_emit(existing, &snapshot, now_ms) {
            return false;
        }
        snapshot.last_emitted_ms = now_ms;
        *existing = snapshot;
        return true;
    }

    if previous.len() >= MAX_SNAPSHOTS {
        if let Some(oldest_index) = previous
            .iter()
            .enumerate()
            .min_by_key(|(_, item)| item.last_emitted_ms)
            .map(|(index, _)| index)
        {
            previous.remove(oldest_index);
        }
    }
    snapshot.last_emitted_ms = now_ms;
    previous.push(snapshot);
    true
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

    #[derive(Clone)]
    struct CapturedTraceWriter(std::sync::Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CapturedTraceWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn trace_cursor_waits_for_publication_to_finish() {
        use std::sync::Arc;
        use std::sync::mpsc;
        use std::time::Duration;

        let fence = Arc::new(TracePublicationFence::new());
        let published = Arc::new(Mutex::new(Vec::new()));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let producer_fence = Arc::clone(&fence);
        let producer_published = Arc::clone(&published);
        let producer = std::thread::spawn(move || {
            producer_fence.publish_with_elapsed(
                || 0,
                |_, sequence, _| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    producer_published.lock().unwrap().push(sequence);
                },
            );
        });
        entered_rx.recv().unwrap();

        let (cursor_ready_tx, cursor_ready_rx) = mpsc::channel();
        let (cursor_tx, cursor_rx) = mpsc::channel();
        let cursor_fence = Arc::clone(&fence);
        let cursor_published = Arc::clone(&published);
        let reader = std::thread::spawn(move || {
            cursor_ready_tx.send(()).unwrap();
            let cursor = cursor_fence.cursor();
            let published = cursor_published.lock().unwrap().clone();
            cursor_tx.send((cursor, published)).unwrap();
        });
        cursor_ready_rx.recv().unwrap();
        assert!(cursor_rx.recv_timeout(Duration::from_millis(20)).is_err());

        release_tx.send(()).unwrap();
        producer.join().unwrap();
        let (cursor, published) = cursor_rx.recv().unwrap();
        reader.join().unwrap();
        assert_eq!(cursor, 1);
        assert_eq!(published, vec![cursor]);
    }

    #[test]
    fn authoring_provider_emitter_keeps_both_editor_surfaces_in_reader_records() {
        use crate::radial::authoring::{AuthoringSessionId, DraftGeneration, StableSelection};
        use crate::radial::model::{CellId, MenuId, RingId};

        let bytes = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_bytes = std::sync::Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedTraceWriter(std::sync::Arc::clone(&writer_bytes)))
            .finish();
        let budget = EventBudget::new(4);
        let query = "private title value path content";
        let mut emitted_surfaces = Vec::new();

        tracing::subscriber::with_default(subscriber, || {
            for surface in [
                crate::gui::BindingEditorSurface::Properties,
                crate::gui::BindingEditorSurface::Inspector,
            ] {
                let identity = crate::gui::AuthoringBindingEditorIdentity {
                    scope: crate::gui::BindingEditorScope {
                        surface,
                        editor_session: AuthoringSessionId(7),
                        draft_generation: DraftGeneration(9),
                        target: StableSelection::Cell {
                            menu_id: MenuId::new("menu-private"),
                            ring_id: RingId::new("ring-private"),
                            cell_id: CellId::new("cell-private"),
                        },
                        slot: crate::gui::BindingEditorSlot::CellPrimary,
                    },
                    assigned_binding_digest: 40,
                    editor_epoch: 10,
                    edit_generation: 11,
                    query_generation: 12,
                    query_request_generation: 13,
                    search_request_generation: 14,
                    test_request_generation: 15,
                };

                emit_authoring_provider_search(
                    &identity,
                    "worker_started",
                    "search",
                    query,
                    None,
                    Some(16),
                );
                let emitted = take_authoring_provider_trace_test_events()
                    .into_iter()
                    .next()
                    .expect("the production emitter records its typed identity");
                let label = binding_editor_surface_label(emitted.identity.scope.surface);
                assert_eq!(emitted.editor_surface, label);
                emitted_surfaces.push(label);

                // Use the same event constructor as the production emitter, then
                // serialize it through the real tracing arm consumed by the runner.
                emit_with_budget(
                    authoring_provider_search_event(
                        &emitted.identity,
                        emitted.edge,
                        emitted.kind,
                        emitted.query_digest,
                        emitted.binding_digest,
                        Some(16),
                    ),
                    &budget,
                );
            }
        });

        assert_eq!(emitted_surfaces, ["properties", "inspector"]);
        let output = String::from_utf8(
            bytes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
        .expect("trace formatter emits UTF-8");
        let lines = output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        for (line, surface) in lines.iter().zip(["properties", "inspector"]) {
            assert!(line.contains("trace_event=\"authoring_provider_search\""));
            assert!(line.contains(&format!("editor_surface=\"{surface}\"")));
            assert!(!line.contains(query));
            assert!(line.contains("query_digest="));
        }
    }

    #[test]
    fn deferred_editor_receipts_serialize_full_semantic_identity_without_text() {
        let bytes = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_bytes = std::sync::Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedTraceWriter(std::sync::Arc::clone(&writer_bytes)))
            .finish();
        let budget = EventBudget::new(3);

        tracing::subscriber::with_default(subscriber, || {
            for surface in ["properties", "inspector"] {
                emit_with_budget(
                    Event::DesignerActionEditorControl {
                        surface,
                        control: "result_target",
                        control_index: Some(2),
                        target_digest: 31,
                        title_digest: 32,
                        type_digest: 33,
                        disambiguator_digest: 34,
                        action_digest: 35,
                        binding_digest: 36,
                        query_digest: 37,
                        value_digest: 0,
                        displayed_text_digest: 38,
                        editor_assigned_binding_digest: 39,
                        editor_session_id: 7,
                        draft_generation: 9,
                        stable_target_digest: 40,
                        editor_epoch: 10,
                        edit_generation: 11,
                        query_generation: 12,
                        query_request_generation: 13,
                        search_request_generation: 14,
                        test_request_generation: 15,
                        left_px: 1,
                        top_px: 2,
                        right_px: 90,
                        bottom_px: 30,
                        full_left_px: 1,
                        full_top_px: 2,
                        full_right_px: 90,
                        full_bottom_px: 30,
                        client_width_px: 800,
                        client_height_px: 600,
                        fully_visible: true,
                        enabled: true,
                        selected: false,
                        focused: false,
                        clicked: false,
                        changed: false,
                        enter_pressed: false,
                    },
                    &budget,
                );
            }
            emit_with_budget(
                Event::DesignerInspectorCellTextEdit {
                    target_digest: 41,
                    session_id: 7,
                    generation: 10,
                    value_digest: 42,
                    left_px: 11,
                    top_px: 21,
                    right_px: 101,
                    bottom_px: 43,
                    clip_left_px: 11,
                    clip_top_px: 21,
                    clip_right_px: 101,
                    clip_bottom_px: 43,
                    client_width_px: 800,
                    client_height_px: 600,
                    visible: true,
                    fully_visible: true,
                    focused: true,
                    clicked: true,
                    changed: true,
                },
                &budget,
            );
        });

        let output = String::from_utf8(
            bytes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
        .expect("trace output is UTF-8");
        let lines = output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        for (line, surface) in lines[..2].iter().zip(["properties", "inspector"]) {
            assert!(line.contains(&format!("editor_surface=\"{surface}\"")));
            assert!(line.contains("editor_control=\"result_target\""));
            assert!(line.contains("displayed_text_digest=38"));
            assert!(line.contains("full_left_px=1"));
            assert!(line.contains("fully_visible=true"));
            assert!(line.contains("trace_sequence="));
            assert!(!line.contains("private"));
        }
        let inspector = lines[2];
        assert!(inspector.contains("trace_event=\"designer_inspector_cell_text_edit\""));
        assert!(inspector.contains("target_digest=41"));
        assert!(inspector.contains("value_digest=42"));
        assert!(inspector.contains("focused=true clicked=true changed=true"));
        assert!(inspector.contains("trace_sequence="));
        assert!(!inspector.contains("Pinned label"));
    }

    #[test]
    fn action_editor_scroll_production_log_publishes_native_sequence() {
        let bytes = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_bytes = std::sync::Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedTraceWriter(std::sync::Arc::clone(&writer_bytes)))
            .finish();
        let budget = EventBudget::new(1);
        let before = trace_sequence();

        tracing::subscriber::with_default(subscriber, || {
            emit_with_budget(
                action_editor_scroll_event(&action_editor_scroll_snapshot(23, 210_000, 1)),
                &budget,
            );
        });

        let output = String::from_utf8(
            bytes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
        .expect("trace output is UTF-8");
        let lines = output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let line = lines[0];
        assert!(line.contains("trace_event=\"designer_action_editor_scroll\""));
        assert!(line.contains("trace_sequence="));
        let sequence = trace_line_sequence(line)
            .expect("production scroll trace includes its publication sequence");
        assert!(trace_line_is_in_sequence_window(line, before, sequence));
        assert!(!line.contains("private"));
    }

    #[test]
    fn radial_insertion_producer_serializes_every_finite_control_as_a_quoted_enum() {
        let bytes = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_bytes = std::sync::Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedTraceWriter(std::sync::Arc::clone(&writer_bytes)))
            .finish();
        let controls = [
            (RadialInsertionControl::SourceAdd, "source_add"),
            (RadialInsertionControl::DestinationMenu, "destination_menu"),
            (RadialInsertionControl::DestinationRing, "destination_ring"),
            (RadialInsertionControl::DestinationCell, "destination_cell"),
            (
                RadialInsertionControl::InsertSelectedSpacer,
                "insert_selected_spacer",
            ),
            (RadialInsertionControl::AppendToRing, "append_to_ring"),
            (RadialInsertionControl::ReplaceToggle, "replace_toggle"),
            (RadialInsertionControl::ReplaceConfirm, "replace_confirm"),
            (RadialInsertionControl::Cancel, "cancel"),
            (
                RadialInsertionControl::CloseTreeConfirm,
                "close_tree_confirm",
            ),
            (RadialInsertionControl::Save, "save"),
            (RadialInsertionControl::Reopen, "reopen"),
            (RadialInsertionControl::Undo, "undo"),
            (RadialInsertionControl::Redo, "redo"),
        ];
        let budget = EventBudget::new(controls.len());

        tracing::subscriber::with_default(subscriber, || {
            for (index, (control, expected_name)) in controls.iter().enumerate() {
                assert_eq!(control.as_str(), *expected_name);
                emit_with_budget(
                    Event::RadialInsertionControl {
                        control: control.as_str(),
                        widget_part: "none",
                        request_id: 17,
                        source_target_digest: 31,
                        source_action_digest: 32,
                        source_binding_digest: 33,
                        source_query_digest: 34,
                        destination_menu_digest: 41,
                        destination_ring_digest: 42,
                        destination_cell_digest: 43,
                        session_id: 7,
                        generation: 11,
                        enabled: true,
                        selected: false,
                        clicked: index == 0,
                        left_px: 0,
                        top_px: 0,
                        right_px: 0,
                        bottom_px: 0,
                        full_left_px: 0,
                        full_top_px: 0,
                        full_right_px: 0,
                        full_bottom_px: 0,
                        client_width_px: 0,
                        client_height_px: 0,
                        visible: false,
                        fully_visible: false,
                        document_digest_after: 51,
                        binding_digest_after: 33,
                    },
                    &budget,
                );
            }
        });

        let output = String::from_utf8(
            bytes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
        .expect("trace output is UTF-8");
        let lines = output.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), controls.len());
        for (line, (_, expected_name)) in lines.iter().zip(controls) {
            assert!(line.contains("trace_event=\"radial_insertion_control\""));
            assert!(line.contains(&format!("insertion_control=\"{expected_name}\"")));
            assert!(line.contains("request_id=17"));
            assert!(line.contains("source_target_digest=31"));
            assert!(line.contains("destination_cell_digest=43"));
            assert!(line.contains("trace_sequence="));
        }
    }

    #[test]
    fn fenced_producer_records_include_sequences_consumed_by_gate_c_windows() {
        let bytes = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_bytes = std::sync::Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_target(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedTraceWriter(std::sync::Arc::clone(&writer_bytes)))
            .finish();
        let budget = EventBudget::new(8);
        tracing::subscriber::with_default(subscriber, || {
            emit_with_budget(
                Event::RootResultPointer {
                    kind: RootResultKind::RadialSkins,
                    index: 0,
                    pressed: false,
                    released: false,
                    hovered: true,
                    clicked: false,
                    has_position: true,
                    pointer_x: 10,
                    pointer_y: 20,
                },
                &budget,
            );
            emit_with_budget(
                Event::UniversalActionExecution {
                    action_id_digest: 47,
                    action_surface: "LauncherList",
                    activation_source: "click",
                },
                &budget,
            );
            emit_with_budget(
                Event::RadialAction {
                    stage: RadialActionStage::EditorModeApplied,
                    skins: false,
                    editor_open: Some(true),
                    skins_selected: None,
                    panel_registered: None,
                },
                &budget,
            );
            emit_with_budget(
                Event::Authoring {
                    edge: AuthoringEdge::PendingRetired,
                    correlation: Correlation::default(),
                },
                &budget,
            );
        });

        let output = String::from_utf8(
            bytes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        )
        .expect("trace formatter emits UTF-8");
        for event_name in [
            "root_result_pointer",
            "universal_action_execution",
            "radial_action",
            "authoring",
        ] {
            let line = output
                .lines()
                .find(|line| line.contains(&format!("trace_event=\"{event_name}\"")))
                .unwrap_or_else(|| panic!("missing emitted {event_name} record: {output}"));
            let sequence = line
                .split_ascii_whitespace()
                .find_map(|field| {
                    field
                        .strip_prefix("trace_sequence=")
                        .and_then(|value| value.trim_end_matches(',').parse::<u64>().ok())
                })
                .unwrap_or_else(|| panic!("{event_name} omitted its publication sequence: {line}"));
            assert!(sequence > 0);
            assert!(trace_line_is_in_sequence_window(
                line,
                sequence - 1,
                sequence
            ));
            assert!(!trace_line_is_in_sequence_window(
                line,
                sequence,
                sequence + 1
            ));
        }
        assert_eq!(budget.emitted(), 4);
    }

    fn authoring_control_snapshot(last_emitted_ms: u128) -> DesignerAuthoringControlSnapshot {
        DesignerAuthoringControlSnapshot {
            target: DesignerAuthoringTarget::Canvas,
            role: DesignerAuthoringRole::Region,
            viewport: ViewportClass::Deferred,
            session_id: 9,
            index: None,
            bounds: [10, 20, 520, 390],
            clip_bounds: [0, 0, 624, 441],
            client_size: [624, 441],
            enabled: true,
            selected: false,
            focused: false,
            clicked: false,
            generation: 4,
            scope: None,
            last_emitted_ms,
        }
    }

    fn action_editor_control_snapshot(
        session_id: u64,
        generation: u64,
        last_emitted_ms: u128,
    ) -> ActionEditorControlSnapshot {
        ActionEditorControlSnapshot {
            surface: "properties",
            control: "query_field",
            editor_session_id: session_id,
            draft_generation: 1,
            stable_target_digest: 11,
            editor_epoch: 2,
            edit_generation: generation,
            query_generation: 3,
            query_request_generation: 4,
            search_request_generation: 5,
            test_request_generation: 6,
            control_index: None,
            target_digest: 7,
            title_digest: 71,
            type_digest: 72,
            disambiguator_digest: 73,
            action_digest: 8,
            binding_digest: 9,
            query_digest: 10,
            value_digest: 10,
            displayed_text_digest: 0,
            editor_assigned_binding_digest: 9,
            bounds: [1, 2, 30, 40],
            full_bounds: [1, 2, 30, 40],
            client_size: [640, 480],
            fully_visible: true,
            enabled: true,
            selected: false,
            focused: true,
            clicked: false,
            changed: false,
            enter_pressed: false,
            last_emitted_ms,
        }
    }

    fn action_editor_scroll_snapshot(
        frame_nr: u64,
        offset_y_milli: i64,
        last_emitted_ms: u128,
    ) -> ActionEditorScrollSnapshot {
        let offset_px = (offset_y_milli * 190 + 500_000) / 1_000_000;
        let bottom_px = ((offset_y_milli + 190_000) * 190 + 500_000) / 1_000_000;
        ActionEditorScrollSnapshot {
            surface: "inspector",
            editor_session_id: 7,
            draft_generation: 4,
            stable_target_digest: 11,
            editor_epoch: 2,
            edit_generation: 3,
            query_generation: 5,
            query_request_generation: 6,
            search_request_generation: 9,
            test_request_generation: 0,
            query_digest: 13,
            editor_assigned_binding_digest: 17,
            scroll_id: 19,
            frame_nr,
            offset_y_milli,
            velocity_y_milli: 0,
            content_height_milli: 1_000_000,
            inner_height_milli: 190_000,
            pixels_per_point_milli: 1_000,
            handle_min_length_milli: 12_000,
            inner_bounds: [20, 40, 300, 230],
            inner_visible_bounds: [20, 40, 300, 230],
            track_bounds: [285, 40, 300, 230],
            track_visible_bounds: [285, 40, 300, 230],
            thumb_bounds: [285, 40 + offset_px as i32, 300, 40 + bottom_px as i32],
            thumb_visible_bounds: [285, 40 + offset_px as i32, 300, 40 + bottom_px as i32],
            painted_thumb_bounds: [285, 40 + offset_px as i32, 300, 40 + bottom_px as i32],
            painted_thumb_visible_bounds: [285, 40 + offset_px as i32, 300, 40 + bottom_px as i32],
            paint_clip_bounds: [0, 0, 640, 480],
            client_size: [640, 480],
            last_emitted_ms,
        }
    }

    #[test]
    fn action_editor_controls_reemit_for_every_request_identity_generation() {
        let prior = action_editor_control_snapshot(1, 7, 1_000);
        let mut changed = prior;
        changed.draft_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.editor_epoch += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.edit_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.query_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.query_request_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.search_request_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
        let mut changed = prior;
        changed.test_request_generation += 1;
        assert!(action_editor_control_snapshot_should_emit(
            &prior, &changed, 1_001
        ));
    }

    #[test]
    fn action_editor_control_event_reports_widget_change_not_cache_refresh() {
        let mut cache = Vec::new();
        let mut published = Vec::new();
        let first = action_editor_control_snapshot(1, 7, 1_000);
        publish_cached_action_editor_control(&mut cache, first, false, 1_000, |event| {
            published.push(event)
        });
        let emitted_for_first_paint = published.pop().unwrap();
        assert!(matches!(
            emitted_for_first_paint,
            Event::DesignerActionEditorControl { changed: false, .. }
        ));
        let idle_refresh = action_editor_control_snapshot(1, 7, 1_000);
        publish_cached_action_editor_control(&mut cache, idle_refresh, false, 1_499, |event| {
            published.push(event)
        });
        assert!(published.is_empty());
        let periodic_refresh = action_editor_control_snapshot(1, 7, 1_500);
        publish_cached_action_editor_control(&mut cache, periodic_refresh, false, 1_500, |event| {
            published.push(event)
        });
        let emitted_for_periodic_refresh = published.pop().unwrap();
        assert!(matches!(
            emitted_for_periodic_refresh,
            Event::DesignerActionEditorControl { changed: false, .. }
        ));

        let mut edited = action_editor_control_snapshot(1, 7, 1_501);
        edited.changed = true;
        edited.edit_generation += 1;
        publish_cached_action_editor_control(&mut cache, edited, true, 1_501, |event| {
            published.push(event)
        });
        let emitted_for_text_edit = published.pop().unwrap();
        assert!(matches!(
            emitted_for_text_edit,
            Event::DesignerActionEditorControl { changed: true, .. }
        ));
    }

    #[test]
    fn action_editor_scroll_cache_emits_identity_and_measured_state_changes() {
        let mut cache = Vec::new();
        let first = action_editor_scroll_snapshot(10, 210_000, 1_000);
        assert!(action_editor_scroll_observation_is_valid(
            &ActionEditorScrollObservation {
                query_digest: first.query_digest,
                scroll_id: first.scroll_id,
                frame_nr: first.frame_nr,
                offset_y_milli: first.offset_y_milli,
                velocity_y_milli: first.velocity_y_milli,
                content_height_milli: first.content_height_milli,
                inner_height_milli: first.inner_height_milli,
                pixels_per_point_milli: first.pixels_per_point_milli,
                handle_min_length_milli: first.handle_min_length_milli,
                inner_bounds: first.inner_bounds,
                inner_visible_bounds: first.inner_visible_bounds,
                track_bounds: first.track_bounds,
                track_visible_bounds: first.track_visible_bounds,
                thumb_bounds: first.thumb_bounds,
                thumb_visible_bounds: first.thumb_visible_bounds,
                painted_thumb_bounds: first.painted_thumb_bounds,
                painted_thumb_visible_bounds: first.painted_thumb_visible_bounds,
                paint_clip_bounds: first.paint_clip_bounds,
                client_size: first.client_size,
            }
        ));
        let mut events = Vec::new();
        events.push(cached_action_editor_scroll_event(&mut cache, first, 1_000).unwrap());
        let duplicate_frame = action_editor_scroll_snapshot(11, 210_000, 1_001);
        assert!(cached_action_editor_scroll_event(&mut cache, duplicate_frame, 1_001).is_none());

        let moved = action_editor_scroll_snapshot(12, 220_000, 1_002);
        events.push(cached_action_editor_scroll_event(&mut cache, moved, 1_002).unwrap());
        let new_request = ActionEditorScrollSnapshot {
            search_request_generation: 10,
            frame_nr: 13,
            last_emitted_ms: 1_003,
            ..moved
        };
        events.push(cached_action_editor_scroll_event(&mut cache, new_request, 1_003).unwrap());
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            Event::DesignerActionEditorScroll {
                surface: "inspector",
                scroll_id: 19,
                frame_nr: 10,
                query_digest: 13,
                ..
            }
        ));
        assert!(matches!(
            events[1],
            Event::DesignerActionEditorScroll {
                offset_y_milli: 220_000,
                frame_nr: 12,
                ..
            }
        ));
        assert!(matches!(
            events[2],
            Event::DesignerActionEditorScroll {
                search_request_generation: 10,
                frame_nr: 13,
                ..
            }
        ));
    }

    #[test]
    fn action_editor_scroll_geometry_rejects_no_thumb_and_inconsistent_offset() {
        let valid = action_editor_scroll_snapshot(1, 210_000, 1);
        let to_observation = |snapshot: ActionEditorScrollSnapshot| ActionEditorScrollObservation {
            query_digest: snapshot.query_digest,
            scroll_id: snapshot.scroll_id,
            frame_nr: snapshot.frame_nr,
            offset_y_milli: snapshot.offset_y_milli,
            velocity_y_milli: snapshot.velocity_y_milli,
            content_height_milli: snapshot.content_height_milli,
            inner_height_milli: snapshot.inner_height_milli,
            pixels_per_point_milli: snapshot.pixels_per_point_milli,
            handle_min_length_milli: snapshot.handle_min_length_milli,
            inner_bounds: snapshot.inner_bounds,
            inner_visible_bounds: snapshot.inner_visible_bounds,
            track_bounds: snapshot.track_bounds,
            track_visible_bounds: snapshot.track_visible_bounds,
            thumb_bounds: snapshot.thumb_bounds,
            thumb_visible_bounds: snapshot.thumb_visible_bounds,
            painted_thumb_bounds: snapshot.painted_thumb_bounds,
            painted_thumb_visible_bounds: snapshot.painted_thumb_visible_bounds,
            paint_clip_bounds: snapshot.paint_clip_bounds,
            client_size: snapshot.client_size,
        };
        let mut missing_thumb = to_observation(valid);
        missing_thumb.thumb_visible_bounds = [0; 4];
        assert!(!action_editor_scroll_observation_is_valid(&missing_thumb));
        let mut wrong_offset = to_observation(valid);
        wrong_offset.offset_y_milli += 100_000;
        assert!(!action_editor_scroll_observation_is_valid(&wrong_offset));
        let mut outside_client = to_observation(valid);
        outside_client.thumb_visible_bounds[2] = outside_client.client_size[0] + 1;
        assert!(!action_editor_scroll_observation_is_valid(&outside_client));

        let mut minimum_sized = to_observation(valid);
        minimum_sized.handle_min_length_milli = 40_000;
        minimum_sized.painted_thumb_bounds = [285, 78, 300, 118];
        minimum_sized.painted_thumb_visible_bounds = minimum_sized.painted_thumb_bounds;
        assert!(action_editor_scroll_observation_is_valid(&minimum_sized));
        minimum_sized.painted_thumb_bounds = minimum_sized.thumb_bounds;
        minimum_sized.painted_thumb_visible_bounds = minimum_sized.thumb_visible_bounds;
        assert!(!action_editor_scroll_observation_is_valid(&minimum_sized));
    }

    #[test]
    fn action_editor_control_cache_evicts_old_keys_and_accepts_new_sessions() {
        let mut cache = Vec::new();
        for session_id in 1..=512 {
            assert!(insert_action_editor_control_snapshot(
                &mut cache,
                action_editor_control_snapshot(session_id, 1, session_id as u128),
                session_id as u128,
            ));
        }
        assert_eq!(cache.len(), 512);
        let newest = action_editor_control_snapshot(513, 1, 513);
        assert!(insert_action_editor_control_snapshot(
            &mut cache, newest, 513
        ));
        assert_eq!(cache.len(), 512);
        assert!(cache.iter().any(|item| item.editor_session_id == 513));
        assert!(!cache.iter().any(|item| item.editor_session_id == 1));
        assert!(insert_action_editor_control_snapshot(
            &mut cache,
            action_editor_control_snapshot(514, 1, 514),
            514,
        ));
        assert_eq!(cache.len(), 512);
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

        let mut pane_clipped = unchanged;
        pane_clipped.clip_bounds[1] = 28;
        assert!(authoring_control_snapshot_should_emit(
            &prior,
            &pane_clipped,
            1_001
        ));
        let mut cache = vec![prior];
        assert!(insert_designer_authoring_control_snapshot(
            &mut cache,
            pane_clipped,
            1_001
        ));
        assert_eq!(cache[0].bounds, prior.bounds);
        assert_eq!(cache[0].clip_bounds, pane_clipped.clip_bounds);

        let mut focused = unchanged;
        focused.focused = true;
        assert!(authoring_control_snapshot_should_emit(
            &prior, &focused, 1_001
        ));

        let mut different_canvas_epoch = unchanged;
        different_canvas_epoch.scope = Some(DesignerAuthoringControlScope::CanvasCell(
            DesignerCanvasCellScope {
                menu_cell_ids_digest: 41,
                menu_id_digest: 42,
                ring_id_digest: 43,
                cell_id_digest: 44,
                authored_target_digest: 45,
                label_digest: 46,
                ring_index: 1,
                slot_index: 0,
            },
        ));
        assert!(authoring_control_snapshot_should_emit(
            &prior,
            &different_canvas_epoch,
            1_001
        ));
    }

    #[test]
    fn tree_search_undo_observation_deduplicates_and_emits_settled_checkpoint_changes() {
        let mut changing = authoring_control_snapshot(1_000);
        changing.target = DesignerAuthoringTarget::TreeSearch;
        changing.role = DesignerAuthoringRole::TextEdit;
        changing.focused = true;
        changing.scope = Some(DesignerAuthoringControlScope::TreeSearchUndo {
            field_id_digest: 41,
            value_digest: 42,
            in_flux: true,
        });
        let mut cache = vec![changing];
        assert!(!insert_designer_authoring_control_snapshot(
            &mut cache, changing, 1_001
        ));
        let mut settled = changing;
        settled.scope = Some(DesignerAuthoringControlScope::TreeSearchUndo {
            field_id_digest: 41,
            value_digest: 42,
            in_flux: false,
        });
        assert!(insert_designer_authoring_control_snapshot(
            &mut cache, settled, 1_002
        ));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].scope, settled.scope);
        assert!(!insert_designer_authoring_control_snapshot(
            &mut cache, settled, 1_003
        ));
        assert!(insert_designer_authoring_control_snapshot(
            &mut cache, settled, 1_502
        ));
        for scope in [
            None,
            Some(DesignerAuthoringControlScope::TreeSearchUndo {
                field_id_digest: 43,
                value_digest: 42,
                in_flux: false,
            }),
            Some(DesignerAuthoringControlScope::TreeSearchUndo {
                field_id_digest: 41,
                value_digest: 44,
                in_flux: false,
            }),
        ] {
            let mut changed = settled;
            changed.scope = scope;
            assert!(authoring_control_snapshot_should_emit(
                &settled, &changed, 1_003
            ));
        }
    }

    #[test]
    fn designer_authoring_cache_evicts_old_sessions_for_later_canvas_cells() {
        let mut cache = Vec::new();
        let mut now_ms = 1;
        for session_id in 1..=9 {
            for cell_index in 0..40 {
                let mut snapshot = authoring_control_snapshot(now_ms);
                snapshot.target = DesignerAuthoringTarget::CanvasCell;
                snapshot.session_id = session_id;
                snapshot.index = Some(cell_index);
                snapshot.scope = Some(DesignerAuthoringControlScope::CanvasCell(
                    DesignerCanvasCellScope {
                        menu_cell_ids_digest: session_id * 100,
                        menu_id_digest: session_id * 101,
                        ring_id_digest: session_id * 102,
                        cell_id_digest: session_id * 103,
                        authored_target_digest: session_id * 104,
                        label_digest: session_id * 105,
                        ring_index: 0,
                        slot_index: cell_index,
                    },
                ));
                assert!(insert_designer_authoring_control_snapshot(
                    &mut cache, snapshot, now_ms,
                ));
                now_ms += 1;
            }
        }

        assert_eq!(cache.len(), 256, "the cache remains strictly bounded");
        assert!(
            !cache.iter().any(|snapshot| snapshot.session_id == 1),
            "oldest session observations are evicted"
        );
        let newest = cache
            .iter()
            .find(|snapshot| {
                snapshot.session_id == 9
                    && snapshot.target == DesignerAuthoringTarget::CanvasCell
                    && snapshot.index == Some(39)
            })
            .copied()
            .expect("current-session CanvasCell observation was retained");

        let mut clicked = newest;
        clicked.clicked = true;
        clicked.selected = true;
        clicked.generation += 1;
        assert!(insert_designer_authoring_control_snapshot(
            &mut cache, clicked, now_ms,
        ));
        assert_eq!(cache.len(), 256);
        let refreshed = cache
            .iter()
            .find(|snapshot| {
                snapshot.session_id == 9
                    && snapshot.target == DesignerAuthoringTarget::CanvasCell
                    && snapshot.index == Some(39)
            })
            .expect("current-session clicked CanvasCell remains cached");
        assert!(refreshed.clicked && refreshed.selected);
        assert_eq!(refreshed.generation, newest.generation + 1);
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
                "trace_sequence",
            ],
            Event::DesignerPointerMoved { .. } => &["client_x", "client_y", "trace_sequence"],
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
                "trace_sequence",
            ],
            Event::UniversalActionExecution { .. } => &[
                "action_id_digest",
                "action_surface",
                "activation_source",
                "trace_sequence",
            ],
            Event::RadialAction { .. } => &[
                "stage",
                "skins",
                "editor_open",
                "skins_selected",
                "panel_registered",
                "trace_sequence",
            ],
            Event::DesignerMutation { .. } => &["result", "correlation"],
            Event::Authoring { .. } => &["edge", "correlation", "trace_sequence"],
            Event::AcceptancePrepareGate { .. } => &["edge", "correlation"],
            Event::RuntimePreparation { .. } => &["edge", "invocation_id", "generation"],
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
                "clip_left_px",
                "clip_top_px",
                "clip_right_px",
                "clip_bottom_px",
                "client_width_px",
                "client_height_px",
                "trace_sequence",
                "enabled",
                "selected",
                "focused",
                "clicked",
                "session_id",
                "generation",
                "menu_cell_ids_digest",
                "menu_id_digest",
                "ring_id_digest",
                "cell_id_digest",
                "authored_target_digest",
                "cell_label_digest",
                "cell_ring_index",
                "cell_slot_index",
                "projected_source_target_digest",
                "projected_result_index",
                "edit_source_target_digest",
                "edit_source_result_index",
                "breadcrumb_menu_id_digest",
                "rendered_text_digest",
                "rendered_text_left_px",
                "rendered_text_top_px",
                "rendered_text_right_px",
                "rendered_text_bottom_px",
                "rendered_text_fully_visible",
                "rendered_text_elided",
                "text_edit_field_digest",
                "text_edit_value_digest",
                "text_edit_undo_in_flux",
            ],
            Event::DesignerActionEditorControl { .. } => &[
                "editor_surface",
                "editor_control",
                "control_index",
                "target_digest",
                "title_digest",
                "type_digest",
                "disambiguator_digest",
                "action_digest",
                "binding_digest",
                "query_digest",
                "value_digest",
                "displayed_text_digest",
                "editor_assigned_binding_digest",
                "editor_session_id",
                "draft_generation",
                "stable_target_digest",
                "editor_epoch",
                "edit_generation",
                "query_generation",
                "query_request_generation",
                "search_request_generation",
                "test_request_generation",
                "trace_sequence",
                "left_px",
                "top_px",
                "right_px",
                "bottom_px",
                "full_left_px",
                "full_top_px",
                "full_right_px",
                "full_bottom_px",
                "client_width_px",
                "client_height_px",
                "fully_visible",
                "enabled",
                "selected",
                "focused",
                "clicked",
                "changed",
                "enter_pressed",
                "visible",
            ],
            Event::DesignerActionEditorScroll { .. } => &[
                "editor_surface",
                "editor_session_id",
                "draft_generation",
                "stable_target_digest",
                "editor_epoch",
                "edit_generation",
                "query_generation",
                "query_request_generation",
                "search_request_generation",
                "test_request_generation",
                "trace_sequence",
                "query_digest",
                "editor_assigned_binding_digest",
                "scroll_id",
                "frame_nr",
                "offset_y_milli",
                "velocity_y_milli",
                "content_height_milli",
                "inner_height_milli",
                "pixels_per_point_milli",
                "handle_min_length_milli",
                "inner_left_px",
                "inner_top_px",
                "inner_right_px",
                "inner_bottom_px",
                "inner_visible_left_px",
                "inner_visible_top_px",
                "inner_visible_right_px",
                "inner_visible_bottom_px",
                "track_left_px",
                "track_top_px",
                "track_right_px",
                "track_bottom_px",
                "track_visible_left_px",
                "track_visible_top_px",
                "track_visible_right_px",
                "track_visible_bottom_px",
                "thumb_left_px",
                "thumb_top_px",
                "thumb_right_px",
                "thumb_bottom_px",
                "thumb_visible_left_px",
                "thumb_visible_top_px",
                "thumb_visible_right_px",
                "thumb_visible_bottom_px",
                "painted_thumb_left_px",
                "painted_thumb_top_px",
                "painted_thumb_right_px",
                "painted_thumb_bottom_px",
                "painted_thumb_visible_left_px",
                "painted_thumb_visible_top_px",
                "painted_thumb_visible_right_px",
                "painted_thumb_visible_bottom_px",
                "paint_clip_left_px",
                "paint_clip_top_px",
                "paint_clip_right_px",
                "paint_clip_bottom_px",
                "client_width_px",
                "client_height_px",
            ],
            Event::DesignerInspectorCellTextEdit { .. } => &[
                "target_digest",
                "session_id",
                "generation",
                "value_digest",
                "trace_sequence",
                "left_px",
                "top_px",
                "right_px",
                "bottom_px",
                "clip_left_px",
                "clip_top_px",
                "clip_right_px",
                "clip_bottom_px",
                "client_width_px",
                "client_height_px",
                "visible",
                "fully_visible",
                "focused",
                "clicked",
                "changed",
            ],
            Event::RadialInsertionControl { .. } => &[
                "insertion_control",
                "widget_part",
                "request_id",
                "source_target_digest",
                "source_action_digest",
                "source_binding_digest",
                "source_query_digest",
                "destination_menu_digest",
                "destination_ring_digest",
                "destination_cell_digest",
                "session_id",
                "generation",
                "enabled",
                "selected",
                "clicked",
                "left_px",
                "top_px",
                "right_px",
                "bottom_px",
                "full_left_px",
                "full_top_px",
                "full_right_px",
                "full_bottom_px",
                "client_width_px",
                "client_height_px",
                "visible",
                "fully_visible",
                "document_digest_after",
                "binding_digest_after",
                "trace_sequence",
            ],
            Event::AuthoringProviderSearch { .. } => &[
                "authoring_request_edge",
                "authoring_search_kind",
                "editor_surface",
                "editor_session_id",
                "draft_generation",
                "stable_target_digest",
                "editor_epoch",
                "edit_generation",
                "query_generation",
                "query_request_generation",
                "search_request_generation",
                "test_request_generation",
                "query_digest",
                "binding_digest",
                "editor_assigned_binding_digest",
                "provider_revision",
                "trace_sequence",
            ],
            Event::AuthoringObservationBoundary { .. } => &[
                "phase",
                "request_id",
                "baseline_request_id",
                "captured_trace_sequence",
                "trace_sequence",
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
            Event::RadialDispatchRequested { .. } => &["invocation_id", "session_generation"],
            Event::RuntimeRadialHover { .. } => &[
                "session_digest",
                "cell_digest",
                "layout_generation",
                "role",
                "executable",
            ],
            Event::RadialQueryResolution { .. } => &[
                "invocation_id",
                "session_digest",
                "cell_digest",
                "session_generation",
                "config_revision",
                "preparation_generation",
                "query_digest",
                "mode",
                "state",
                "provider_revision",
                "result_count",
                "result_digest",
                "selected_digest",
                "interaction_requirement",
            ],
            Event::RadialQueryDispatch { .. } => &[
                "invocation_id",
                "session_digest",
                "cell_digest",
                "session_generation",
                "config_revision",
                "mode",
                "query_digest",
                "selected_digest",
                "interaction_requirement",
                "root_policy",
                "outcome",
            ],
            Event::RadialRootSnapshot { .. } => &[
                "phase",
                "invocation_id",
                "session_digest",
                "cell_digest",
                "query_digest",
                "action_digest",
                "source",
                "state_digest",
                "ordinary_query_digest",
                "results_digest",
                "results_count",
                "selected_index",
                "grid_layout",
                "visible",
                "restore",
                "visibility_revision",
                "focus_query",
                "move_cursor_end",
                "last_results_valid",
                "last_search_query_digest",
                "suggestions_digest",
                "autocomplete_index",
                "query_history_digest",
                "matching_history_count",
                "radial_source_history_count",
                "usage_count",
            ],
            Event::DesiredVisibility { .. } => &["visible", "revision", "source", "invocation_id"],
            Event::ScreenDrawRestoreFocusIntent { .. } => {
                &["revision", "invocation_id", "focus_intent"]
            }
            Event::RootCommand { .. } => &["command", "correlation"],
            Event::WindowSampleTruncated { .. } => &["correlation"],
            Event::Restore { .. } => &["edge", "correlation"],
            Event::NativeWindowSnapshot { .. } => &[
                "hwnd",
                "process_id",
                "left",
                "top",
                "right",
                "bottom",
                "visible",
                "minimized",
                "correlation",
            ],
            Event::NativeActivation { .. } => &[
                "edge",
                "hwnd",
                "correlation",
                "visibility_revision",
                "invocation_id",
            ],
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
    fn visibility_scope_links_root_command_and_native_activation_schema() {
        let correlation = with_visibility_trace_link(73, Some(41), root_command_correlation);
        assert_eq!(correlation.visibility_revision, 73);
        assert_eq!(correlation.invocation_id, 41);

        let labels = schema_labels(Event::NativeActivation {
            edge: NativeActivationEdge::RestoreRequested,
            hwnd: 303,
            correlation,
        });
        assert!(labels.contains(&"visibility_revision"));
        assert!(labels.contains(&"invocation_id"));
    }

    #[test]
    fn boundary_and_restore_requests_share_one_unique_id_sequence() {
        let before_restore = next_request_id();
        let boundary = root_command_correlation();
        let restore = next_request_id();

        assert!(before_restore < boundary.request_id);
        assert!(boundary.request_id < restore);
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
    fn gate_c_profile_is_explicitly_bounded_and_other_profiles_keep_legacy_limit() {
        assert_eq!(
            TraceBudgetProfile::from_environment(None),
            TraceBudgetProfile::DEFAULT
        );
        assert_eq!(
            TraceBudgetProfile::from_environment(Some("gate_c_v1")),
            TraceBudgetProfile::GATE_C
        );
        assert_eq!(
            TraceBudgetProfile::from_environment(Some("gate-c-v1")),
            TraceBudgetProfile::DEFAULT
        );
        assert_eq!(
            TraceBudgetProfile::from_environment(Some("gate_d_v1")),
            TraceBudgetProfile::GATE_D
        );
        assert_eq!(
            TraceBudgetProfile::from_environment(Some("gate-d-v1")),
            TraceBudgetProfile::DEFAULT
        );
        assert_eq!(TraceBudgetProfile::DEFAULT.event_limit, 8_192);
        assert_eq!(TraceBudgetProfile::DEFAULT.terminal_reserve, 0);
        assert_eq!(TraceBudgetProfile::GATE_C.event_limit, 65_536);
        assert_eq!(TraceBudgetProfile::GATE_C.event_limit, EVENT_BUDGET * 8);
        assert_eq!(TraceBudgetProfile::GATE_C.terminal_reserve, 256);
        assert_eq!(TraceBudgetProfile::GATE_D.event_limit, 49_152);
        assert_eq!(TraceBudgetProfile::GATE_D.event_limit, EVENT_BUDGET * 6);
        assert_eq!(TraceBudgetProfile::GATE_D.terminal_reserve, 256);
    }

    #[test]
    fn gate_c_budget_reserve_is_hard_bounded_and_only_accepts_terminal_evidence() {
        let boundary = Event::AuthoringObservationBoundary {
            phase: "terminal",
            request_id: 2,
            baseline_request_id: Some(1),
            captured_trace_sequence: 19,
        };
        let close = Event::DesignerClose {
            state: DesignerCloseState {
                session_id: 7,
                open: true,
                close_prompt: true,
                dirty: true,
                pending_disposable: false,
                pending_durable: false,
                pending_native_preview: false,
            },
        };
        let cancelled_request = Event::DisposableRequestCancelled {
            request_kind: RequestKind::LivePreview,
            request_id: 2,
            session_id: 7,
            generation: 8,
        };
        let terminal_provider = Event::AuthoringProviderSearch {
            edge: "applied",
            kind: "search",
            editor_surface: "inspector",
            editor_session_id: 1,
            draft_generation: 1,
            stable_target_digest: 2,
            editor_epoch: 1,
            edit_generation: 1,
            query_generation: 1,
            query_request_generation: 1,
            search_request_generation: 2,
            test_request_generation: 0,
            query_digest: 3,
            binding_digest: 0,
            editor_assigned_binding_digest: 4,
            provider_revision: Some(5),
        };
        let queued_provider = Event::AuthoringProviderSearch {
            edge: "queued",
            kind: "search",
            editor_surface: "inspector",
            editor_session_id: 1,
            draft_generation: 1,
            stable_target_digest: 2,
            editor_epoch: 1,
            edit_generation: 1,
            query_generation: 1,
            query_request_generation: 1,
            search_request_generation: 2,
            test_request_generation: 0,
            query_digest: 3,
            binding_digest: 0,
            editor_assigned_binding_digest: 4,
            provider_revision: Some(5),
        };
        let leaf_dispatch = Event::RadialAction {
            stage: RadialActionStage::Dispatched,
            skins: false,
            editor_open: None,
            skins_selected: None,
            panel_registered: None,
        };
        let discard = |clicked| Event::DesignerAuthoringControl {
            target: DesignerAuthoringTarget::DiscardDraft,
            role: DesignerAuthoringRole::Button,
            viewport: ViewportClass::Deferred,
            index: None,
            left_px: 1,
            top_px: 1,
            right_px: 20,
            bottom_px: 20,
            clip_left_px: 0,
            clip_top_px: 0,
            clip_right_px: 800,
            clip_bottom_px: 600,
            client_width_px: 800,
            client_height_px: 600,
            enabled: true,
            selected: false,
            focused: true,
            clicked,
            session_id: 1,
            generation: 2,
            scope: None,
        };
        let clicked_discard = discard(true);
        let unclicked_discard = discard(false);
        let mut disabled_discard = unclicked_discard;
        if let Event::DesignerAuthoringControl { enabled, .. } = &mut disabled_discard {
            *enabled = false;
        }
        let insertion_cancel = |clicked| Event::RadialInsertionControl {
            control: "cancel",
            widget_part: "none",
            request_id: 1,
            source_target_digest: 2,
            source_action_digest: 3,
            source_binding_digest: 4,
            source_query_digest: 5,
            destination_menu_digest: 6,
            destination_ring_digest: 7,
            destination_cell_digest: 8,
            session_id: 9,
            generation: 10,
            enabled: true,
            selected: false,
            clicked,
            left_px: 0,
            top_px: 0,
            right_px: 0,
            bottom_px: 0,
            full_left_px: 0,
            full_top_px: 0,
            full_right_px: 0,
            full_bottom_px: 0,
            client_width_px: 0,
            client_height_px: 0,
            visible: false,
            fully_visible: false,
            document_digest_after: 11,
            binding_digest_after: 12,
        };
        let clicked_insertion_cancel = insertion_cancel(true);
        let insertion_not_cancelled = insertion_cancel(false);
        let pointer = Event::NativePointer {
            transition: NativePointerTransition::Up,
            button: NativePointerButton::Primary,
            owner: NativeWindowOwner::Root,
            hwnd: 1,
            generation: 1,
        };
        let pointer_move = Event::DesignerPointerMoved {
            client_x: 14,
            client_y: 18,
        };
        let designer_pointer = Event::DesignerPointer {
            down: false,
            up: true,
            window_under_cursor: None,
            correlation: Correlation {
                session_id: 7,
                generation: 8,
                ..Correlation::default()
            },
        };
        assert!(event_uses_terminal_reserve(&boundary));
        assert!(event_uses_terminal_reserve(&close));
        assert!(event_uses_terminal_reserve(&cancelled_request));
        assert!(event_uses_terminal_reserve(&terminal_provider));
        assert!(event_uses_terminal_reserve(&clicked_discard));
        assert!(event_uses_terminal_reserve(&clicked_insertion_cancel));
        assert!(event_uses_terminal_reserve(&unclicked_discard));
        assert!(event_uses_terminal_reserve(&pointer_move));
        assert!(event_uses_terminal_reserve(&designer_pointer));
        assert!(!event_uses_terminal_reserve(&queued_provider));
        assert!(!event_uses_terminal_reserve(&leaf_dispatch));
        assert!(!event_uses_terminal_reserve(&disabled_discard));
        assert!(!event_uses_terminal_reserve(&insertion_not_cancelled));
        assert!(!event_uses_terminal_reserve(&pointer));

        let budget = EventBudget::with_terminal_reserve(3, 2);
        for _ in 0..3 {
            assert_eq!(
                budget.reserve_with_terminal_policy(false),
                EventBudgetAdmission::Normal
            );
        }
        assert_eq!(
            budget.reserve_with_terminal_policy(false),
            EventBudgetAdmission::Rejected,
            "ordinary events cannot borrow terminal capacity"
        );
        assert!(budget.mark_exhausted_once());
        assert!(!budget.mark_exhausted_once());
        assert_eq!(
            budget.reserve_with_terminal_policy(event_uses_terminal_reserve(&boundary)),
            EventBudgetAdmission::TerminalReserve
        );
        assert_eq!(
            budget.reserve_with_terminal_policy(event_uses_terminal_reserve(&terminal_provider)),
            EventBudgetAdmission::TerminalReserve
        );
        assert_eq!(
            budget.reserve_with_terminal_policy(event_uses_terminal_reserve(&boundary)),
            EventBudgetAdmission::Rejected,
            "terminal capacity is also a hard bound"
        );
        assert_eq!(budget.emitted(), 3);
        assert_eq!(budget.terminal_reserved(), 2);

        let no_dispatch_bypass = EventBudget::with_terminal_reserve(1, 1);
        assert_eq!(
            no_dispatch_bypass
                .reserve_with_terminal_policy(event_uses_terminal_reserve(&leaf_dispatch)),
            EventBudgetAdmission::Normal
        );
        assert_eq!(
            no_dispatch_bypass
                .reserve_with_terminal_policy(event_uses_terminal_reserve(&leaf_dispatch)),
            EventBudgetAdmission::Rejected,
            "ordinary action dispatches cannot consume the cleanup reserve"
        );
    }

    #[test]
    fn gate_c_budget_exhaustion_marker_and_reserved_receipts_keep_one_ordered_sequence() {
        let budget = EventBudget::with_terminal_reserve(1, 1);
        let fence = TracePublicationFence::new();
        let first = budget.reserve_with_terminal_policy(false);
        assert_eq!(first, EventBudgetAdmission::Normal);
        let first_sequence = fence.publish_with_elapsed(|| 1, |_, sequence, _| sequence);

        assert_eq!(
            budget.reserve_with_terminal_policy(false),
            EventBudgetAdmission::Rejected
        );
        assert!(budget.mark_exhausted_once());
        let marker_sequence = fence.publish_with_elapsed(|| 2, |_, sequence, _| sequence);
        assert!(!budget.mark_exhausted_once());

        let boundary = Event::AuthoringObservationBoundary {
            phase: "terminal",
            request_id: 3,
            baseline_request_id: Some(2),
            captured_trace_sequence: marker_sequence,
        };
        let terminal = budget.reserve_with_terminal_policy(event_uses_terminal_reserve(&boundary));
        assert_eq!(terminal, EventBudgetAdmission::TerminalReserve);
        let terminal_sequence = fence.publish_with_elapsed(|| 3, |_, sequence, _| sequence);
        assert_eq!(
            [first_sequence, marker_sequence, terminal_sequence],
            [1, 2, 3]
        );
        assert_eq!(budget.emitted(), 1);
        assert_eq!(budget.terminal_reserved(), 1);
        assert_eq!(
            budget.reserve_with_terminal_policy(false),
            EventBudgetAdmission::Rejected,
            "exhaustion does not suppress the normal dispatch budget"
        );
    }

    #[test]
    fn gate_c_exhausted_budget_retains_real_dirty_cleanup_event_chain() {
        let budget = EventBudget::with_terminal_reserve(1, 16);
        let fence = TracePublicationFence::new();
        let entries = RefCell::new(Vec::new());
        let mut publish = |entry| entries.borrow_mut().push(entry);

        assert_eq!(
            publish_budgeted_event(
                Event::RootMenuBody {
                    menu: RootMenuControl::File,
                    entered: false,
                },
                &budget,
                &fence,
                &mut publish,
            ),
            EventBudgetAdmission::Normal
        );
        let baseline = publish_authoring_observation_boundary_with_budget(
            "baseline",
            41,
            None,
            &budget,
            &fence,
            &mut publish,
        )
        .expect("the reserved baseline boundary is published after exhaustion");
        assert_eq!(baseline.0, 2, "the boundary includes the exhaustion marker");

        let close_prompt = Event::DesignerClose {
            state: DesignerCloseState {
                session_id: 7,
                open: true,
                close_prompt: true,
                dirty: true,
                pending_disposable: false,
                pending_durable: false,
                pending_native_preview: false,
            },
        };
        let discard = Event::DesignerAuthoringControl {
            target: DesignerAuthoringTarget::DiscardDraft,
            role: DesignerAuthoringRole::Button,
            viewport: ViewportClass::Deferred,
            index: None,
            left_px: 4,
            top_px: 5,
            right_px: 48,
            bottom_px: 27,
            clip_left_px: 0,
            clip_top_px: 0,
            clip_right_px: 800,
            clip_bottom_px: 600,
            client_width_px: 800,
            client_height_px: 600,
            enabled: true,
            selected: false,
            focused: true,
            clicked: false,
            session_id: 7,
            generation: 9,
            scope: None,
        };
        let mut clicked_discard = discard;
        if let Event::DesignerAuthoringControl { clicked, .. } = &mut clicked_discard {
            *clicked = true;
        }
        let pointer_move = Event::DesignerPointerMoved {
            client_x: 26,
            client_y: 16,
        };
        let pointer_up = Event::DesignerPointer {
            down: false,
            up: true,
            window_under_cursor: None,
            correlation: Correlation {
                session_id: 7,
                generation: 9,
                ..Correlation::default()
            },
        };
        let closed = Event::DesignerClose {
            state: DesignerCloseState {
                session_id: 0,
                open: false,
                close_prompt: false,
                dirty: false,
                pending_disposable: false,
                pending_durable: false,
                pending_native_preview: false,
            },
        };
        for event in [
            close_prompt,
            discard,
            pointer_move,
            pointer_up,
            clicked_discard,
            closed,
        ] {
            assert_eq!(
                publish_budgeted_event(event, &budget, &fence, &mut publish),
                EventBudgetAdmission::TerminalReserve
            );
        }
        let terminal = publish_authoring_observation_boundary_with_budget(
            "terminal",
            42,
            Some(41),
            &budget,
            &fence,
            &mut publish,
        )
        .expect("the reserved terminal boundary follows the actual cleanup receipts");
        assert_eq!(terminal.0 + 1, terminal.1);

        let before_dispatch = entries.borrow().len();
        assert_eq!(
            publish_budgeted_event(
                Event::RadialAction {
                    stage: RadialActionStage::Dispatched,
                    skins: false,
                    editor_open: None,
                    skins_selected: None,
                    panel_registered: None,
                },
                &budget,
                &fence,
                &mut publish,
            ),
            EventBudgetAdmission::Rejected,
            "a leaf dispatch cannot use the cleanup reserve"
        );
        assert_eq!(entries.borrow().len(), before_dispatch);
        let entries = entries.into_inner();

        let marker_count = entries
            .iter()
            .filter(|entry| matches!(entry, BudgetTraceEntry::Exhausted { .. }))
            .count();
        assert_eq!(marker_count, 1);
        let published_sequences = entries
            .iter()
            .map(|entry| match entry {
                BudgetTraceEntry::Exhausted { trace_sequence, .. }
                | BudgetTraceEntry::Event { trace_sequence, .. } => *trace_sequence,
            })
            .collect::<Vec<_>>();
        assert!(published_sequences.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(entries.iter().any(|entry| matches!(
            entry,
            BudgetTraceEntry::Event {
                event: Event::DesignerAuthoringControl {
                    target: DesignerAuthoringTarget::DiscardDraft,
                    clicked: false,
                    enabled: true,
                    session_id: 7,
                    ..
                },
                ..
            }
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry,
            BudgetTraceEntry::Event {
                event: Event::DesignerPointerMoved { .. },
                ..
            }
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry,
            BudgetTraceEntry::Event {
                event: Event::DesignerAuthoringControl {
                    target: DesignerAuthoringTarget::DiscardDraft,
                    clicked: true,
                    session_id: 7,
                    ..
                },
                ..
            }
        )));
        assert!(entries.iter().any(|entry| matches!(
            entry,
            BudgetTraceEntry::Event {
                event: Event::DesignerClose {
                    state: DesignerCloseState {
                        session_id: 0,
                        open: false,
                        ..
                    }
                },
                ..
            }
        )));
        assert_eq!(budget.emitted(), 1);
        assert_eq!(budget.terminal_reserved(), 8);
    }

    #[test]
    fn gate_c_observation_boundary_uses_terminal_budget_after_normal_exhaustion() {
        let budget = EventBudget::with_terminal_reserve(1, 1);
        let fence = TracePublicationFence::new();
        let mut entries = Vec::new();
        assert_eq!(
            publish_budgeted_event(
                Event::RootMenuBody {
                    menu: RootMenuControl::Apps,
                    entered: false,
                },
                &budget,
                &fence,
                |entry| entries.push(entry),
            ),
            EventBudgetAdmission::Normal
        );
        let receipt = publish_authoring_observation_boundary_with_budget(
            "terminal",
            12,
            Some(11),
            &budget,
            &fence,
            |entry| entries.push(entry),
        )
        .expect("the actual boundary publisher must consume eligible reserve");
        assert_eq!(receipt.0 + 1, receipt.1);
        assert_eq!(budget.emitted(), 1);
        assert_eq!(budget.terminal_reserved(), 1);
        assert!(entries.iter().any(|entry| matches!(
            entry,
            BudgetTraceEntry::Event {
                event: Event::AuthoringObservationBoundary {
                    phase: "terminal",
                    request_id: 12,
                    baseline_request_id: Some(11),
                    ..
                },
                ..
            }
        )));
    }

    #[test]
    fn elapsed_time_is_monotonic() {
        let first = elapsed_ms();
        let second = elapsed_ms();
        assert!(second >= first);
    }

    #[test]
    fn trace_elapsed_sampling_waits_for_prior_publication_to_finish() {
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc;

        let fence = std::sync::Arc::new(TracePublicationFence::new());
        let first_published = std::sync::Arc::new(AtomicBool::new(false));
        let (first_sampled_tx, first_sampled_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let first_fence = std::sync::Arc::clone(&fence);
        let first_published_flag = std::sync::Arc::clone(&first_published);
        let first = std::thread::spawn(move || {
            first_fence.publish_with_elapsed(
                || {
                    let _ = first_sampled_tx.send(());
                    let _ = release_first_rx.recv();
                    20
                },
                |_, sequence, elapsed| {
                    first_published_flag.store(true, Ordering::Release);
                    (sequence, elapsed)
                },
            )
        });
        first_sampled_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("first publisher should hold the publication gate while sampling time");

        let (second_calling_tx, second_calling_rx) = mpsc::channel();
        let (second_sampled_tx, second_sampled_rx) = mpsc::channel();
        let second_fence = std::sync::Arc::clone(&fence);
        let second_published_flag = std::sync::Arc::clone(&first_published);
        let second = std::thread::spawn(move || {
            let _ = second_calling_tx.send(());
            second_fence.publish_with_elapsed(
                || {
                    let _ = second_sampled_tx.send(());
                    if second_published_flag.load(Ordering::Acquire) {
                        21
                    } else {
                        10
                    }
                },
                |_, sequence, elapsed| (sequence, elapsed),
            )
        });
        second_calling_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("second publisher should start while the first still holds the gate");
        let sampled_while_blocked = second_sampled_rx
            .recv_timeout(std::time::Duration::from_millis(40))
            .is_ok();
        let _ = release_first_tx.send(());

        let first = first.join().expect("first publisher should finish");
        let second = second.join().expect("second publisher should finish");
        assert!(!sampled_while_blocked);
        assert_eq!(first, (1, 20));
        assert_eq!(second, (2, 21));
        assert!(first.1 <= second.1);
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
            ..Correlation::default()
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
            Event::UniversalActionExecution {
                action_id_digest: 47,
                action_surface: "ContextMenu",
                activation_source: "click",
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
                clip_left_px: 0,
                clip_top_px: 0,
                clip_right_px: 624,
                clip_bottom_px: 441,
                client_width_px: 624,
                client_height_px: 441,
                enabled: true,
                selected: false,
                focused: true,
                clicked: true,
                session_id: 77,
                generation: 19,
                scope: Some(DesignerAuthoringControlScope::CanvasCell(
                    DesignerCanvasCellScope {
                        menu_cell_ids_digest: 123,
                        menu_id_digest: 124,
                        ring_id_digest: 125,
                        cell_id_digest: 126,
                        authored_target_digest: 127,
                        label_digest: 128,
                        ring_index: 1,
                        slot_index: 0,
                    },
                )),
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
            Event::DesignerActionEditorControl {
                surface: "properties",
                control: "query_field",
                control_index: None,
                target_digest: 101,
                title_digest: 0,
                type_digest: 0,
                disambiguator_digest: 0,
                action_digest: 102,
                binding_digest: 103,
                query_digest: 104,
                value_digest: 104,
                displayed_text_digest: 0,
                editor_assigned_binding_digest: 114,
                editor_session_id: 77,
                draft_generation: 19,
                stable_target_digest: 105,
                editor_epoch: 106,
                edit_generation: 107,
                query_generation: 108,
                query_request_generation: 109,
                search_request_generation: 110,
                test_request_generation: 111,
                left_px: 10,
                top_px: 20,
                right_px: 40,
                bottom_px: 44,
                full_left_px: 10,
                full_top_px: 20,
                full_right_px: 40,
                full_bottom_px: 44,
                client_width_px: 624,
                client_height_px: 441,
                fully_visible: true,
                enabled: true,
                selected: false,
                focused: true,
                clicked: false,
                changed: true,
                enter_pressed: false,
            },
            Event::DesignerActionEditorScroll {
                surface: "inspector",
                editor_session_id: 77,
                draft_generation: 19,
                stable_target_digest: 105,
                editor_epoch: 106,
                edit_generation: 107,
                query_generation: 108,
                query_request_generation: 109,
                search_request_generation: 110,
                test_request_generation: 111,
                query_digest: 104,
                editor_assigned_binding_digest: 114,
                scroll_id: 115,
                frame_nr: 116,
                offset_y_milli: 210_000,
                velocity_y_milli: -4_000,
                content_height_milli: 1_000_000,
                inner_height_milli: 190_000,
                pixels_per_point_milli: 1_000,
                handle_min_length_milli: 12_000,
                inner_left_px: 100,
                inner_top_px: 40,
                inner_right_px: 300,
                inner_bottom_px: 230,
                inner_visible_left_px: 100,
                inner_visible_top_px: 40,
                inner_visible_right_px: 300,
                inner_visible_bottom_px: 230,
                track_left_px: 285,
                track_top_px: 40,
                track_right_px: 300,
                track_bottom_px: 230,
                track_visible_left_px: 285,
                track_visible_top_px: 40,
                track_visible_right_px: 300,
                track_visible_bottom_px: 230,
                thumb_left_px: 285,
                thumb_top_px: 80,
                thumb_right_px: 300,
                thumb_bottom_px: 116,
                thumb_visible_left_px: 285,
                thumb_visible_top_px: 80,
                thumb_visible_right_px: 300,
                thumb_visible_bottom_px: 116,
                painted_thumb_left_px: 285,
                painted_thumb_top_px: 80,
                painted_thumb_right_px: 300,
                painted_thumb_bottom_px: 116,
                painted_thumb_visible_left_px: 285,
                painted_thumb_visible_top_px: 80,
                painted_thumb_visible_right_px: 300,
                painted_thumb_visible_bottom_px: 116,
                paint_clip_left_px: 0,
                paint_clip_top_px: 0,
                paint_clip_right_px: 624,
                paint_clip_bottom_px: 441,
                client_width_px: 624,
                client_height_px: 441,
            },
            Event::DesignerInspectorCellTextEdit {
                target_digest: 125,
                session_id: 77,
                generation: 20,
                value_digest: 126,
                left_px: 11,
                top_px: 21,
                right_px: 101,
                bottom_px: 43,
                clip_left_px: 11,
                clip_top_px: 21,
                clip_right_px: 101,
                clip_bottom_px: 43,
                client_width_px: 624,
                client_height_px: 441,
                visible: true,
                fully_visible: true,
                focused: true,
                clicked: true,
                changed: true,
            },
            Event::RadialInsertionControl {
                control: "destination_cell",
                widget_part: "none",
                request_id: 115,
                source_target_digest: 116,
                source_action_digest: 117,
                source_binding_digest: 118,
                source_query_digest: 119,
                destination_menu_digest: 120,
                destination_ring_digest: 121,
                destination_cell_digest: 122,
                session_id: 77,
                generation: 19,
                enabled: true,
                selected: true,
                clicked: true,
                left_px: 0,
                top_px: 0,
                right_px: 0,
                bottom_px: 0,
                full_left_px: 0,
                full_top_px: 0,
                full_right_px: 0,
                full_bottom_px: 0,
                client_width_px: 0,
                client_height_px: 0,
                visible: false,
                fully_visible: false,
                document_digest_after: 123,
                binding_digest_after: 124,
            },
            Event::AuthoringProviderSearch {
                edge: "worker_completed",
                kind: "search",
                editor_surface: "properties",
                editor_session_id: 77,
                draft_generation: 19,
                stable_target_digest: 105,
                editor_epoch: 106,
                edit_generation: 107,
                query_generation: 108,
                query_request_generation: 109,
                search_request_generation: 110,
                test_request_generation: 111,
                query_digest: 104,
                binding_digest: 103,
                editor_assigned_binding_digest: 114,
                provider_revision: Some(112),
            },
            Event::AuthoringObservationBoundary {
                phase: "terminal",
                request_id: 113,
                baseline_request_id: Some(109),
                captured_trace_sequence: 112,
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
            Event::RadialDispatchRequested {
                invocation_id: 17,
                session_generation: 19,
            },
            Event::RuntimeRadialHover {
                session_digest: 23,
                cell_digest: 29,
                layout_generation: 19,
                role: "Action",
                executable: true,
            },
            Event::RadialQueryResolution {
                invocation_id: 17,
                session_digest: 23,
                cell_digest: 29,
                session_generation: 19,
                config_revision: 5,
                preparation_generation: 3,
                query_digest: 31,
                mode: "execute_first",
                state: "ready",
                provider_revision: Some(7),
                result_count: 2,
                result_digest: 37,
                selected_digest: Some(41),
                interaction_requirement: "none",
            },
            Event::RadialQueryDispatch {
                invocation_id: 17,
                session_digest: 23,
                cell_digest: 29,
                session_generation: 19,
                config_revision: 5,
                mode: "execute_first",
                query_digest: 31,
                selected_digest: 41,
                interaction_requirement: "none",
                root_policy: "preserve",
                outcome: "executed",
            },
            Event::RadialRootSnapshot {
                phase: "before",
                invocation_id: 17,
                session_digest: 23,
                cell_digest: 29,
                query_digest: 31,
                action_digest: 41,
                source: "click",
                state_digest: 43,
                ordinary_query_digest: 47,
                results_digest: 53,
                results_count: 2,
                selected_index: 0,
                grid_layout: true,
                visible: false,
                restore: false,
                visibility_revision: 3,
                focus_query: false,
                move_cursor_end: false,
                last_results_valid: true,
                last_search_query_digest: 59,
                suggestions_digest: 61,
                autocomplete_index: 0,
                query_history_digest: 67,
                matching_history_count: 4,
                radial_source_history_count: 1,
                usage_count: 7,
            },
            Event::DesiredVisibility {
                visible: false,
                revision: 3,
                source: VisibilitySource::Queued,
                invocation_id: None,
            },
            Event::ScreenDrawRestoreFocusIntent {
                revision: 8,
                invocation_id: Some(41),
                focus_intent: crate::visibility::RootFocusIntent::PreserveForeground,
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
                process_id: 404,
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
        assert!(rendered.contains("title_digest:"));
        assert!(rendered.contains("value_digest:"));
        assert!(rendered.contains("content_height_milli: 1000000"));
        assert!(rendered.contains("contextmenu"));
        // Allow only the typed hash-field labels and the two exact enum names
        // whose type names contain `text`, plus this typed numeric scroll
        // measurement. Exact schema-label checks below still reject raw
        // title/value/text/content fields, and the substring scan still checks
        // the measurement's value.
        let rendered_without_typed_field_labels = rendered
            .replace("title_digest:", "")
            .replace("value_digest:", "")
            .replace("displayed_text_digest:", "")
            .replace("content_height_milli:", "")
            .replace("designerinspectorcelltextedit", "")
            .replace("contextmenu", "");
        let forbidden = [
            "payload",
            "clipboard",
            "title",
            "class",
            "pid",
            "process_name",
            "process_path",
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
                !rendered_without_typed_field_labels.contains(forbidden),
                "diagnostic schema contains forbidden field/content: {forbidden}; rendered={rendered_without_typed_field_labels}"
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
