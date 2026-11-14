use super::utility_host::{
    HostIdentity as PadHostIdentity, UtilityHostState, UtilityPresentation as PadHost,
    ViewportPassProvenance,
};
use crate::calculation_pad::{
    CalculationPadSession, DocumentId, DocumentIncarnation, EXAMPLE_GROUPS,
    InspectorSourceSnapshot, LineId, LineOutcome, PadDocumentSummary, PadRecoverySnapshot,
    PadRecoveryStore, PadRecoveryWriteFailure, PadRecoveryWriter, PadStoreError, PendingTransition,
    SessionError, TransitionDecision, TransitionOutcome,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::platform::app_data::AppDataRoot;
use eframe::egui;

#[path = "calculation_pad_format.rs"]
mod result_format;

#[path = "calculation_pad_editor.rs"]
mod editor;
use editor::{CompletionState, EditorTools, FoldRegion, Projection, SourcePositions, SyntaxCache, SyntaxKind};

const PAD_VIEWPORT_SIZE: [f32; 2] = [1080.0, 760.0];
const MIN_PAD_VIEWPORT_SIZE: [f32; 2] = [500.0, 380.0];
const REFERENCE_SIZE: egui::Vec2 = egui::vec2(620.0, 540.0);
const MIN_REFERENCE_SIZE: egui::Vec2 = egui::vec2(360.0, 280.0);
const SPLITTER_SIZE: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitOrientation {
    Wide,
    Stacked,
}

#[derive(Clone, Copy)]
struct SplitPreview {
    host: PadHostIdentity,
    orientation: SplitOrientation,
    ratio: f32,
    start_ratio: f32,
    press_position: f32,
}

#[derive(Clone, Copy)]
enum SplitAction {
    Preview(SplitPreview),
    Finish(SplitPreview),
    Cancel,
}

fn split_dimensions(total: f32, ratio: f32, minimum: f32, fallback: f32) -> (f32, f32, f32) {
    let content = (total - SPLITTER_SIZE).max(minimum * 2.0);
    let ratio = if ratio.is_finite() { ratio } else { fallback };
    let lower = (minimum / content).max(0.05);
    let ratio = ratio.clamp(lower, 1.0 - lower);
    (content * ratio, content * (1.0 - ratio), ratio)
}

fn splitter(ui: &mut egui::Ui, orientation: SplitOrientation, length: f32) -> egui::Response {
    let size = match orientation {
        SplitOrientation::Wide => egui::vec2(SPLITTER_SIZE, length),
        SplitOrientation::Stacked => egui::vec2(length, SPLITTER_SIZE),
    };
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let fill = if response.dragged() {
        ui.visuals().widgets.active.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().rect_filled(rect, 2.0, fill);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(match orientation {
            SplitOrientation::Wide => egui::CursorIcon::ResizeHorizontal,
            SplitOrientation::Stacked => egui::CursorIcon::ResizeVertical,
        });
    }
    response
}

fn physical_line_positions(
    galley: &egui::Galley,
    top: f32,
) -> impl Iterator<Item = (usize, f32)> + '_ {
    let mut line_number = 0;
    let mut physical_start = true;
    galley.rows.iter().filter_map(move |row| {
        let position = if physical_start {
            line_number += 1;
            Some((line_number, top + row.pos.y))
        } else {
            None
        };
        physical_start = row.ends_with_newline;
        position
    })
}

fn reference_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("calculation_pad_reference_native_viewport")
}

fn reference_tool_key() -> crate::settings::UtilityToolKey {
    crate::settings::UtilityToolKey::parse("calculation-pad-reference")
        .expect("Calculation Pad reference is a registered utility window key")
}

fn pad_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("calculation_pad_native_viewport")
}

fn calculation_pad_tool_key() -> crate::settings::UtilityToolKey {
    crate::settings::UtilityToolKey::parse("calculation-pad")
        .expect("Calculation Pad is a registered utility window key")
}

fn pad_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Calculation Pad")
        .with_inner_size(PAD_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_PAD_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

#[derive(Clone)]
struct InputCache {
    basis: (DocumentIncarnation, DocumentId, u64),
    inputs: Vec<crate::calculation_pad::LiteralInput>,
}
#[derive(Clone, Default, PartialEq)]
struct PadView {
    show_inputs: bool,
    show_watches: bool,
    inputs: Vec<InputSelection>,
    watches: Vec<String>,
    watch_entry: String,
}
#[derive(Clone, PartialEq)]
struct InputSelection {
    id: LineId,
    name: String,
    unavailable: bool,
    draft: Option<InputDraft>,
}
#[derive(Clone, PartialEq)]
struct InputDraft {
    value: String,
    original: String,
    revision: u64,
    error: Option<String>,
    dirty: bool,
}
#[derive(Clone)]
struct InputCommit { id: LineId, original: String, revision: u64, value: String }

pub(crate) struct CalculationPadDialog {
    pub(crate) open: bool,
    focus_source: bool,
    session: CalculationPadSession,
    inspector_selection: Option<InspectorSelection>,
    inspector_caret: Option<InspectorCaret>,
    inspector_selection_generation: u64,
    inspector_caret_generation: u64,
    inspector_context: std::sync::Arc<InspectorPadContext>,
    inspector_context_key: Option<(DocumentIncarnation, DocumentId, u64, Option<LineId>, u64, bool)>,
    recovery_writer: PadRecoveryWriter,
    remember_source: bool,
    display_preferences: crate::settings::CalculationPadDisplayPreferences,
    split_preview: Option<SplitPreview>,
    recovery_attempted: bool,
    last_recovery_identity: Option<(DocumentId, u64)>,
    host_state: UtilityHostState,
    presentation_prepared: bool,
    deferred_source_focus: Option<(egui::ViewportId, egui::Id)>,
    dependency_navigation: Option<SourceNavigation>,
    source_highlight: Option<SourceHighlight>,
    editor_tools: EditorTools,
    pad_view: PadView,
    completion: CompletionState,
    folds: FoldState,
    editor_tool_focus_pending: bool,
    selected_result: Option<(DocumentIncarnation, DocumentId, u64, LineId)>,
    active_source_line: Option<(DocumentIncarnation, DocumentId, u64, LineId)>,
    pending_result_scroll: Option<(DocumentIncarnation, DocumentId, u64, LineId)>,
    deferred_projected_input: Option<DeferredProjectedInput>,
    open_chooser: bool,
    saved_documents: Vec<PadDocumentSummary>,
    examples_open: bool,
    examples_host: UtilityHostState,
    examples_presentation_prepared: bool,
    examples_epoch: u64,
    examples_owner: Option<DocumentIncarnation>,
    examples_search: String,
    examples_scroll: f32,
    save_as_open: bool,
    save_as_name: String,
    error: Option<String>,
    conflict_document: Option<DocumentId>,
    notice: Option<String>,
    pending_root_effects: Option<RootEffects>,
    host_close_requested: bool,
    #[cfg(test)]
    test_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_editor_scroll_id: Option<egui::Id>,
    #[cfg(test)]
    test_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_editor_clip: Option<egui::Rect>,
    #[cfg(test)]
    test_display_controls: Option<(egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_editor_geometry: Option<(f32, f32, f32, Vec<(usize, f32)>)>,
    #[cfg(test)]
    test_splitter: Option<(SplitOrientation, egui::Rect)>,
    #[cfg(test)]
    test_copy_all_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_save_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_line_copy_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_inspect_rects: Vec<(usize, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_result_rows: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_rendered_result_rows: Vec<(usize, LineId)>,
    #[cfg(test)]
    test_select_comparison: Option<egui::Rect>,
    #[cfg(test)]
    test_go_to_source: Option<egui::Rect>,
    #[cfg(test)]
    test_result_bodies: Vec<(usize, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_highlight_rows: Vec<egui::Rect>,
    #[cfg(test)]
    test_dependency_open: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_inspector_reopen: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_discard_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_keep_editing_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_button_ownership: Option<(egui::Rect, bool)>,
    #[cfg(test)]
    test_examples_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_copy_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_copy_state: Option<(bool, egui::Rect)>,
    #[cfg(test)]
    test_examples_controls: Option<(egui::Rect, egui::Rect, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_examples_group_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_rendered_sources: Vec<&'static str>,
    #[cfg(test)]
    test_host_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_callback_viewport_id: Option<egui::ViewportId>,
    #[cfg(test)]
    test_child_discard_requested: bool,
    #[cfg(test)]
    test_request_child_discard: bool,
}

#[derive(Debug, PartialEq)]
pub(super) enum CalculationPadDialogEffect {
    CopyText(String),
    OpenInspector(PadOpenReceipt),
    OpenDependencies(PadOpenReceipt),
    ShowRoot,
    SaveRecoveryPreference(bool),
    SaveDisplayPreferences(crate::settings::CalculationPadDisplayPreferences),
}

pub(super) type PadOpenReceipt = super::utility_host::AcceptedParentOpen<(DocumentIncarnation, DocumentId)>;

struct RootEffects {
    pass: ViewportPassProvenance,
    help: Option<HelpEffects>,
    host: Option<HostEffects>,
}

struct HelpEffects {
    identity: PadHostIdentity,
    owner: Option<DocumentIncarnation>,
    epoch: u64,
    viewport_id: egui::ViewportId,
    viewport_class: egui::ViewportClass,
    viewport_info: Option<egui::ViewportInfo>,
    pass: Option<ViewportPassProvenance>,
    accepted: bool,
    search: Option<String>,
    scroll: Option<f32>,
    copy: Option<String>,
    close: bool,
    native_close: bool,
    host_request: Option<PadHost>,
    focus_viewport: bool,
    embedded_layer: Option<egui::LayerId>,
    embedded_focus: Option<egui::Id>,
}

struct DependencyEditorUpdate {
    navigation: super::calculation_dependency_dialog::DependencyNavigation,
    revision: u64,
    editor_id: egui::Id,
    editor_state: egui::widgets::text_edit::TextEditState,
    scroll_id: egui::Id,
    scroll_state: egui::scroll_area::State,
    result_navigation: bool,
    source_byte_offset: usize,
    selection: Option<(usize, usize)>,
    focus_source: bool,
}

struct ResultScrollUpdate {
    target: (DocumentIncarnation, DocumentId, u64, LineId),
    scroll_id: egui::Id,
    state: egui::scroll_area::State,
}

struct DeferredProjectedInput {
    host: PadHostIdentity,
    basis: (DocumentIncarnation, DocumentId, u64),
    events: Vec<egui::Event>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceNavigation {
    host: PadHostIdentity,
    navigation: super::calculation_dependency_dialog::DependencyNavigation,
    revision: u64,
    result_navigation: bool,
    selection: Option<(usize, usize)>,
    focus_source: bool,
}

#[derive(Clone, Copy)]
struct SourceHighlight {
    host: PadHostIdentity,
    navigation: super::calculation_dependency_dialog::DependencyNavigation,
    revision: u64,
    expires_at: f64,
}

struct HostEffects {
    identity: PadHostIdentity,
    viewport_id: egui::ViewportId,
    viewport_class: egui::ViewportClass,
    viewport_info: Option<egui::ViewportInfo>,
    pass: Option<ViewportPassProvenance>,
    pass_accepted: bool,
    edited_source: Option<String>,
    pad_view: Option<PadView>,
    input_commit: Option<InputCommit>,
    watch_copy: Option<(String, bool)>,
    document_basis: Option<(DocumentIncarnation, DocumentId, u64)>,
    inspector_selection: Option<LineId>,
    inspector_caret: Option<std::ops::Range<usize>>,
    open_inspector: bool,
    open_dependencies: bool,
    dependency_applied: Option<DependencyEditorUpdate>,
    result_navigation: Option<(super::calculation_dependency_dialog::DependencyNavigation, u64)>,
    save_as_name: Option<String>,
    source_focus_requested: bool,
    source_editor_id: Option<egui::Id>,
    display_preferences: Option<crate::settings::CalculationPadDisplayPreferences>,
    display_selection: Option<egui::text::CCursorRange>,
    editor_tools: Option<EditorTools>,
    completion: Option<CompletionState>,
    completion_cursor: Option<egui::text::CCursorRange>,
    folds: Option<FoldState>,
    projection_state: Option<egui::widgets::text_edit::TextEditState>,
    source_clipboard: Option<String>,
    deferred_projected_input: Option<Vec<egui::Event>>,
    projected_input_replayed: bool,
    editor_replacement: Option<String>,
    editor_navigation: Option<std::ops::Range<usize>>,
    editor_tool_focus: Option<egui::Id>,
    editor_tool_focus_acknowledged: bool,
    selected_result: Option<LineId>,
    editor_primary_caret: Option<usize>,
    result_scroll: Option<ResultScrollUpdate>,
    split_action: Option<SplitAction>,
    editor_history_action: Option<bool>,
    intent: Option<DialogIntent>,
    host_request: Option<PadHost>,
    result_copy: Option<ResultCopyIntent>,
    close_requested: bool,
    native_close_requested: bool,
    focus_viewport: bool,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct FoldState {
    collapsed: Vec<LineId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InspectorSelection {
    incarnation: DocumentIncarnation,
    document: DocumentId,
    line: LineId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InspectorCaret {
    pub(super) incarnation: DocumentIncarnation,
    pub(super) document: DocumentId,
    pub(super) revision: u64,
    pub(super) range: std::ops::Range<usize>,
    pub(super) generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct InspectorPadContext {
    pub(super) candidate: Option<InspectorSourceSnapshot>,
    pub(super) comparison: Option<std::sync::Arc<crate::calculation_pad::ComparisonContext>>,
    pub(super) selected_line: Option<LineId>,
    pub(super) caret: Option<InspectorCaret>,
    pub(super) selection_generation: u64,
    pub(super) editable: bool,
}

#[derive(Clone, Copy)]
enum ResultCopyTarget {
    All,
    Line(LineId),
    FormattedLine(LineId),
}

#[derive(Clone, Copy)]
struct ResultCopyIntent {
    host: PadHostIdentity,
    viewport_id: egui::ViewportId,
    document_id: DocumentId,
    target: ResultCopyTarget,
}

#[derive(Debug)]
enum DialogIntent {
    New,
    OpenChooser,
    RefreshDocuments,
    Open(DocumentId),
    OpenSaveAs,
    HideChooser,
    CancelSaveAs,
    Save,
    SaveAs(String),
    OpenExamples,
    Decision(TransitionDecision),
    SetRememberSource(bool),
}

impl CalculationPadDialog {
    fn examples_window_id() -> egui::Id {
        egui::Id::new("calculation-pad-examples-window")
    }

    fn root_host_layer_id() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("calculation-pad-window"))
    }

    fn host_identity(&self) -> PadHostIdentity {
        self.host_state.identity()
    }

    fn empty_host_effects(&self, viewport_id: egui::ViewportId) -> HostEffects {
        HostEffects {
            identity: self.host_identity(),
            viewport_id,
            viewport_class: egui::ViewportClass::Root,
            viewport_info: None,
            pass: None,
            pass_accepted: false,
            edited_source: None,
            pad_view: None,
            input_commit: None,
            watch_copy: None,
            document_basis: self
                .session
                .document_incarnation()
                .zip(self.session.current_id())
                .zip(self.session.revision())
                .map(|((incarnation, document), revision)| (incarnation, document, revision)),
            inspector_selection: None,
            inspector_caret: None,
            open_inspector: false,
            open_dependencies: false,
            dependency_applied: None,
            result_navigation: None,
            save_as_name: None,
            source_focus_requested: false,
            source_editor_id: None,
            display_preferences: None,
            display_selection: None,
            editor_tools: None,
            completion: None,
            completion_cursor: None,
            folds: None,
            projection_state: None,
            source_clipboard: None,
            deferred_projected_input: None,
            projected_input_replayed: false,
            editor_replacement: None,
            editor_navigation: None,
            editor_tool_focus: None,
            editor_tool_focus_acknowledged: false,
            selected_result: None,
            editor_primary_caret: None,
            result_scroll: None,
            split_action: None,
            editor_history_action: None,
            intent: None,
            host_request: None,
            result_copy: None,
            close_requested: false,
            native_close_requested: false,
            focus_viewport: false,
            embedded_root_layer: None,
            embedded_root_focused_id: None,
        }
    }

    pub(super) fn host_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        match self.host_state.presentation() {
            PadHost::Docked => Some(Self::root_host_layer_id()),
            PadHost::Popout => self.host_state.embedded_root_layer(),
        }
    }

    pub(super) fn embedded_host_owns_root_input(&self, ctx: &egui::Context) -> bool {
        if !self.open
            || self.host_state.presentation() != PadHost::Popout
            || ctx.viewport_id() != egui::ViewportId::ROOT
        {
            return false;
        }
        let Some(focused_id) = self.host_state.embedded_root_focused_id() else {
            return false;
        };
        let Some(response) = ctx.read_response(focused_id) else {
            return false;
        };
        let Some(layer) = self.host_state.embedded_root_layer() else {
            return false;
        };
        if response.layer_id != layer
            || !ctx.memory(|memory| {
                memory.focused() == Some(focused_id)
                    && memory.layer_ids().any(|memory_layer| memory_layer == layer)
            })
        {
            return false;
        }
        ctx.input(|input| input.focused)
            && !ctx.input(|input| {
                (input.pointer.any_pressed() || input.pointer.any_released())
                    && input.pointer.interact_pos().is_some_and(|position| {
                        ctx.layer_id_at(position)
                            .is_some_and(|hit_layer| hit_layer != layer)
                    })
            })
    }

    pub(super) fn editor_tools_own_root_escape(&self, ctx: &egui::Context) -> bool {
        self.open
            && (self.editor_tools.open || self.completion.visible)
            && self.host_state.presentation() == PadHost::Docked
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && ctx.input(|input| input.focused)
            && (ctx.memory(|memory| [
                egui::Id::new("calculation-pad-source"),
                egui::Id::new("calculation-pad-find"),
                egui::Id::new("calculation-pad-replace"),
                egui::Id::new("calculation-pad-goto"),
            ].into_iter().any(|id| memory.has_focus(id)))
                || ctx.memory(|memory| memory.focused())
                    .and_then(|id| ctx.read_response(id))
                    .is_some_and(|response| response.layer_id == Self::root_host_layer_id())
                || (self.editor_tool_focus_pending && ctx.memory(|memory| memory.focused().is_none())))
    }

    pub(super) fn examples_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open || !self.examples_open {
            return None;
        }
        match self.examples_host.presentation() {
            PadHost::Docked => Some(egui::LayerId::new(
                egui::Order::Middle,
                Self::examples_window_id(),
            )),
            PadHost::Popout => self.examples_host.embedded_root_layer(),
        }
    }

    pub(crate) fn new(root: &AppDataRoot) -> Self {
        Self {
            open: false,
            focus_source: false,
            session: CalculationPadSession::new(root),
            inspector_selection: None,
            inspector_caret: None,
            inspector_selection_generation: 0,
            inspector_caret_generation: 0,
            inspector_context: std::sync::Arc::new(InspectorPadContext::default()),
            inspector_context_key: None,
            recovery_writer: PadRecoveryWriter::new(PadRecoveryStore::new(root)),
            remember_source: false,
            display_preferences: Default::default(),
            split_preview: None,
            recovery_attempted: false,
            last_recovery_identity: None,
            host_state: UtilityHostState::docked(),
            presentation_prepared: false,
            deferred_source_focus: None,
            dependency_navigation: None,
            source_highlight: None,
            editor_tools: EditorTools::default(),
            pad_view: PadView::default(),
            completion: CompletionState::default(),
            folds: FoldState::default(),
            editor_tool_focus_pending: false,
            selected_result: None,
            active_source_line: None,
            pending_result_scroll: None,
            deferred_projected_input: None,
            open_chooser: false,
            saved_documents: Vec::new(),
            examples_open: false,
            examples_host: UtilityHostState::docked(),
            examples_presentation_prepared: false,
            examples_epoch: 0,
            examples_owner: None,
            examples_search: String::new(),
            examples_scroll: 0.0,
            save_as_open: false,
            save_as_name: String::new(),
            error: None,
            conflict_document: None,
            notice: None,
            pending_root_effects: None,
            host_close_requested: false,
            #[cfg(test)]
            test_editor_id: None,
            #[cfg(test)]
            test_editor_scroll_id: None,
            #[cfg(test)]
            test_editor_rect: None,
            #[cfg(test)]
            test_editor_clip: None,
            #[cfg(test)]
            test_display_controls: None,
            #[cfg(test)]
            test_editor_geometry: None,
            #[cfg(test)]
            test_splitter: None,
            #[cfg(test)]
            test_copy_all_rect: None,
            #[cfg(test)]
            test_save_rect: None,
            #[cfg(test)]
            test_line_copy_rects: Vec::new(),
            #[cfg(test)]
            test_inspect_rects: Vec::new(),
            #[cfg(test)]
            test_result_rows: Vec::new(),
            #[cfg(test)]
            test_rendered_result_rows: Vec::new(),
            #[cfg(test)]
            test_select_comparison: None,
            #[cfg(test)]
            test_go_to_source: None,
            #[cfg(test)]
            test_result_bodies: Vec::new(),
            #[cfg(test)]
            test_highlight_rows: Vec::new(),
            #[cfg(test)]
            test_dependency_open: None,
            #[cfg(test)]
            test_inspector_reopen: None,
            #[cfg(test)]
            test_window_rect: None,
            #[cfg(test)]
            test_discard_rect: None,
            #[cfg(test)]
            test_keep_editing_rect: None,
            #[cfg(test)]
            test_examples_button_rect: None,
            #[cfg(test)]
            test_examples_button_ownership: None,
            #[cfg(test)]
            test_examples_window_rect: None,
            #[cfg(test)]
            test_examples_copy_rect: None,
            #[cfg(test)]
            test_examples_copy_state: None,
            #[cfg(test)]
            test_examples_controls: None,
            #[cfg(test)]
            test_examples_group_rect: None,
            #[cfg(test)]
            test_examples_rendered_sources: Vec::new(),
            #[cfg(test)]
            test_host_button_rect: None,
            #[cfg(test)]
            test_callback_viewport_id: None,
            #[cfg(test)]
            test_child_discard_requested: false,
            #[cfg(test)]
            test_request_child_discard: false,
        }
    }

    pub(crate) fn open(&mut self) {
        if !self.open {
            self.reset_host_to_docked();
        }
        let previous = self.session.document_incarnation();
        self.ensure_session_open();
        self.restore_recovery_on_explicit_open();
        if self.session.document_incarnation() != previous { self.restore_view_from_session(); }
        self.open = true;
        self.host_close_requested = false;
        self.request_source_focus();
    }

    pub(crate) fn ensure_open(&mut self) {
        let was_open = self.open;
        if !was_open {
            self.reset_host_to_docked();
        }
        let previous = self.session.document_incarnation();
        self.ensure_session_open();
        if self.session.document_incarnation() != previous { self.restore_view_from_session(); }
        self.open = true;
        if !was_open {
            self.request_source_focus();
        }
    }

    fn request_source_focus(&mut self) {
        self.focus_source = true;
        self.host_state.request_focus();
    }

    fn reset_host_to_docked(&mut self) {
        self.host_state.retire_to_docked();
        self.presentation_prepared = false;
        self.deferred_source_focus = None;
    }

    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            super::utility_host::restore_remembered_presentation(
                ctx,
                calculation_pad_tool_key(),
                &mut self.host_state,
            );
            self.presentation_prepared = true;
        }
        if self.examples_open && !self.examples_presentation_prepared {
            super::utility_host::restore_remembered_presentation(
                ctx,
                reference_tool_key(),
                &mut self.examples_host,
            );
            self.examples_presentation_prepared = true;
        }
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.host_state.bootstrap_pending()
            || self.examples_host.bootstrap_pending()
            || self.examples_open
            || self.examples_owner.is_some()
            || self.examples_host.presentation() == PadHost::Popout
            || (self.open && self.host_state.presentation() == PadHost::Popout)
    }

    fn examples_owner(&self) -> Option<DocumentIncarnation> {
        self.session.document_incarnation()
    }

    fn open_examples(&mut self) {
        if !self.examples_open && self.examples_owner().is_some() {
            self.examples_epoch = self.examples_epoch.wrapping_add(1);
            self.examples_owner = self.examples_owner();
            self.examples_host.retire_to_docked();
            self.examples_presentation_prepared = false;
            self.examples_open = true;
        }
    }

    fn empty_examples_effects(&self, viewport_id: egui::ViewportId) -> HelpEffects {
        HelpEffects {
            identity: self.examples_host.identity(),
            owner: self.examples_owner,
            epoch: self.examples_epoch,
            viewport_id,
            viewport_class: egui::ViewportClass::Root,
            viewport_info: None,
            pass: None,
            accepted: false,
            search: None,
            scroll: None,
            copy: None,
            close: false,
            native_close: false,
            host_request: None,
            focus_viewport: false,
            embedded_layer: None,
            embedded_focus: None,
        }
    }

    fn ensure_session_open(&mut self) {
        if self.session.source().is_none() && self.session.pending_transition().is_none() {
            match self.session.request_new() {
                Ok(_) => {}
                Err(error) => self.set_error(error),
            }
        }
    }

    pub(crate) fn initialize_recovery_preference(&mut self, enabled: bool) {
        self.remember_source = enabled;
    }

    pub(crate) fn initialize_display_preferences(
        &mut self,
        preferences: crate::settings::CalculationPadDisplayPreferences,
    ) {
        self.display_preferences = preferences;
    }

    pub(crate) fn display_preferences(&self) -> crate::settings::CalculationPadDisplayPreferences {
        self.display_preferences
    }

    #[cfg(test)]
    pub(crate) fn test_display_preferences(
        &self,
    ) -> crate::settings::CalculationPadDisplayPreferences {
        self.display_preferences
    }

    pub(crate) fn display_preferences_saved(
        &mut self,
        previous: crate::settings::CalculationPadDisplayPreferences,
        committed: crate::settings::CalculationPadDisplayPreferences,
        newer_pending: bool,
    ) {
        if !newer_pending && self.display_preferences == previous {
            self.display_preferences = committed;
        }
    }

    pub(crate) fn apply_committed_recovery_preference(
        &mut self,
        enabled: bool,
    ) -> anyhow::Result<()> {
        if self.remember_source == enabled {
            return Ok(());
        }
        self.remember_source = enabled;
        if enabled {
            if self.open {
                self.save_recovery_now(None)
            } else {
                Ok(())
            }
        } else {
            self.recovery_attempted = false;
            let result = self.recovery_writer.clear_now();
            if result.is_ok() {
                self.last_recovery_identity = None;
            }
            result
        }
    }

    fn restore_recovery_on_explicit_open(&mut self) {
        if !self.remember_source || self.recovery_attempted {
            return;
        }
        self.recovery_attempted = true;
        match self.recovery_writer.load() {
            Ok(Some(snapshot)) => {
                if let Err(error) = self.session.restore_recovered_source(snapshot.source) {
                    self.set_recovery_error(error.to_string());
                }
            }
            Ok(None) => {}
            Err(error) => self.set_recovery_error(error.to_string()),
        }
    }

    fn recovery_snapshot(&self) -> Option<PadRecoverySnapshot> {
        Some(PadRecoverySnapshot {
            document_id: self.session.current_id()?,
            revision: self.session.revision()?,
            source: self.session.source()?.to_owned(),
        })
    }

    fn schedule_recovery_snapshot(&mut self, ctx: &egui::Context) {
        if !self.remember_source {
            return;
        }
        let Some(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let identity = (snapshot.document_id, snapshot.revision);
        if let Err(error) = self.recovery_writer.schedule(snapshot) {
            self.set_recovery_error(error.to_string());
        } else {
            self.last_recovery_identity = Some(identity);
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }
    }

    fn save_recovery_now(&mut self, snapshot: Option<PadRecoverySnapshot>) -> anyhow::Result<()> {
        if !self.remember_source {
            return Ok(());
        }
        let Some(snapshot) = snapshot.or_else(|| self.recovery_snapshot()) else {
            return Ok(());
        };
        let identity = (snapshot.document_id, snapshot.revision);
        self.recovery_writer.save_now(snapshot)?;
        self.last_recovery_identity = Some(identity);
        Ok(())
    }

    fn report_async_recovery_errors(&mut self) {
        let failures = self.recovery_writer.take_async_errors();
        self.publish_recovery_failures(failures);
    }

    fn recovery_error_identity(&self) -> Option<(DocumentId, u64)> {
        self.session
            .current_id()
            .zip(self.session.revision())
            .or(self.last_recovery_identity)
    }

    fn publish_recovery_failures(&mut self, failures: Vec<PadRecoveryWriteFailure>) {
        let current_identity = self
            .remember_source
            .then(|| self.recovery_error_identity())
            .flatten();
        let messages = failures
            .into_iter()
            .filter(|failure| current_identity == Some((failure.document_id, failure.revision)))
            .map(|failure| failure.message)
            .collect::<Vec<_>>();
        if !messages.is_empty() {
            self.set_recovery_error(messages.join("; "));
        }
    }

    fn set_recovery_error(&mut self, message: String) {
        self.error = Some(format!("Calculation Pad recovery failed: {message}"));
        self.notice = None;
    }

    pub(crate) fn shutdown_recovery(&mut self) -> anyhow::Result<()> {
        let mut failures = self.recovery_writer.take_async_errors();
        let mut direct_errors = Vec::new();
        if self.remember_source && self.open {
            match self.recovery_snapshot() {
                Some(snapshot) => {
                    if let Err(error) = self.save_recovery_now(Some(snapshot)) {
                        direct_errors.push(error.to_string());
                    }
                }
                None => direct_errors.push("open Calculation Pad has no recoverable source".into()),
            }
        }
        let flush_result = self.recovery_writer.flush();
        let shutdown_result = self.recovery_writer.shutdown();
        match flush_result {
            Ok(mut drained) => failures.append(&mut drained),
            Err(error) => direct_errors.push(error.to_string()),
        }
        match shutdown_result {
            Ok(mut drained) => failures.append(&mut drained),
            Err(error) => direct_errors.push(error.to_string()),
        }
        let current_identity = self
            .remember_source
            .then(|| self.recovery_error_identity())
            .flatten();
        direct_errors.extend(
            failures
                .into_iter()
                .filter(|failure| current_identity == Some((failure.document_id, failure.revision)))
                .map(|failure| failure.message),
        );
        if direct_errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(direct_errors.join("; ")))
        }
    }

    pub(crate) fn request_close(&mut self) {
        if !self.open {
            return;
        }
        if self.session.pending_transition().is_some() {
            self.open = true;
            return;
        }
        self.host_close_requested = true;
    }

    fn apply_close_request(&mut self) {
        if self.session.pending_transition().is_some() {
            self.open = true;
            return;
        }
        match self.session.request_close() {
            Ok(TransitionOutcome::Applied) => {
                self.open = self.session.source().is_some();
                if !self.open {
                    self.reset_transient_ui();
                } else {
                    self.request_source_focus();
                }
            }
            Ok(TransitionOutcome::DecisionRequired) => self.open = true,
            Ok(TransitionOutcome::Cancelled) => self.open = true,
            Err(error) => {
                self.set_error(error);
                self.open = true;
            }
        }
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.error = None;
                self.notice = Some("Copied to the clipboard.".into());
            }
            Err(error) => {
                self.conflict_document = None;
                self.error = Some(format!("Clipboard copy failed: {error}"));
                self.notice = None;
            }
        }
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) {
        #[cfg(test)]
        {
            self.test_window_rect = None;
            self.test_discard_rect = None;
            self.test_keep_editing_rect = None;
            self.test_save_rect = None;
            self.test_editor_id = None;
            self.test_editor_rect = None;
            self.test_examples_button_rect = None;
            self.test_examples_button_ownership = None;
            self.test_examples_window_rect = None;
            self.test_examples_copy_rect = None;
            self.test_examples_copy_state = None;
            self.test_examples_controls = None;
            self.test_examples_group_rect = None;
            self.test_examples_rendered_sources.clear();
            self.test_host_button_rect = None;
            self.test_callback_viewport_id = None;
            self.test_child_discard_requested = false;
        }
        if !self.open {
            if first_ui_pass
                && (self.examples_open
                    || self.examples_owner.is_some()
                    || self.examples_host.presentation() == PadHost::Popout)
            {
                self.pending_root_effects = Some(RootEffects {
                    pass: ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                    help: None,
                    host: None,
                });
            }
            return;
        }

        self.prepare_open_presentation(ctx);
        let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut host_effects = None;
        match self.host_state.presentation() {
            PadHost::Docked => {
                let identity = self.host_identity();
                let mut effects = self.empty_host_effects(egui::ViewportId::ROOT);
                let mut window_open = self.open;
                let mut body_authorized = false;
                let content_rect = ctx.content_rect();
                let min_size = egui::vec2(420.0, 300.0).min(content_rect.size());
                let output = egui::Window::new("Calculation Pad")
                    .id(Self::root_host_layer_id().id)
                    .open(&mut window_open)
                    .resizable(true)
                    .default_size(egui::vec2(900.0, 650.0))
                    .min_size(min_size)
                    .max_size(content_rect.size())
                    .show_utility(
                        ctx,
                        "calculation-pad",
                        Self::root_host_layer_id().id,
                        None,
                        crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(
                            egui::vec2(900.0, 650.0),
                        )
                        .with_minimum(min_size)
                        .with_maximum(content_rect.size()),
                        |ui| {
                            let pass_authorized = first_ui_pass
                                && !ctx.will_discard()
                                && ui.is_visible()
                                && !ui.is_sizing_pass();
                            let modal_open =
                                ctx.memory(|memory| memory.top_modal_layer().is_some());
                            let surface_allowed = root_input_allowed && !modal_open;
                            body_authorized = pass_authorized && surface_allowed;
                            self.draw_host_contents(
                                ui,
                                ctx,
                                identity,
                                egui::ViewportId::ROOT,
                                body_authorized,
                                body_authorized,
                                &mut effects,
                            );
                        },
                    );

                #[cfg(test)]
                {
                    self.test_window_rect = output.as_ref().map(|output| output.response.rect);
                }
                let lifecycle_authorized = first_ui_pass
                    && !ctx.will_discard()
                    && output
                        .as_ref()
                        .is_some_and(|output| output.response.rect.intersects(ctx.content_rect()));
                effects.close_requested = lifecycle_authorized
                    && self.session.pending_transition().is_none()
                    && (self.host_close_requested || !window_open);
                effects.pass_accepted = lifecycle_authorized;
                effects.pass = lifecycle_authorized.then_some(root_pass);
                if body_authorized || effects.close_requested {
                    host_effects = Some(effects);
                }
            }
            PadHost::Popout => {
                let identity = self.host_identity();
                let mut callback_effects = None;
                ctx.show_viewport_immediate(
                    pad_viewport_id(),
                    pad_viewport_builder(),
                    |child, class| {
                        let child_ctx = child.ctx().clone();
                        let viewport_id = child_ctx.viewport_id();
                        let independent = class == egui::ViewportClass::Immediate
                            && viewport_id == pad_viewport_id();
                        let current_pass = child.current_pass_index() == 0;
                        let modal_open = child.memory(|memory| memory.top_modal_layer().is_some());
                        let (focused, native_close_requested) = child.input(|input| {
                            (
                                input.focused,
                                independent && input.viewport().close_requested(),
                            )
                        });
                        let pass_authorized = first_ui_pass
                            && current_pass
                            && !ctx.will_discard()
                            && !child.will_discard();
                        let surface_allowed = if independent {
                            application_input_allowed
                        } else {
                            root_input_allowed && viewport_id == egui::ViewportId::ROOT
                        };
                        let source_focus_allowed =
                            pass_authorized && surface_allowed && !modal_open;
                        let actions_allowed = source_focus_allowed
                            && (!independent || focused)
                            && !native_close_requested;
                        let popup_open = egui::Popup::is_any_open(&child_ctx);
                        let local_tools_focus = child.memory(|memory| [
                            egui::Id::new("calculation-pad-source"),
                            egui::Id::new("calculation-pad-find"),
                            egui::Id::new("calculation-pad-replace"),
                            egui::Id::new("calculation-pad-goto"),
                        ].into_iter().any(|id| memory.has_focus(id)))
                            || child.memory(|memory| memory.focused())
                                .and_then(|id| child_ctx.read_response(id))
                                .is_some_and(|response| response.layer_id == child.layer_id());
                        let completion_escape = actions_allowed && !popup_open && self.completion.visible
                            && child.memory(|memory| memory.has_focus(egui::Id::new("calculation-pad-source")))
                            && child.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        let local_editor_escape = actions_allowed && !popup_open && self.editor_tools.open
                            && (independent || local_tools_focus)
                            && child.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        let keyboard_close_requested = if actions_allowed && !popup_open {
                            child.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                            })
                        } else {
                            false
                        };

                        let mut effects = self.empty_host_effects(viewport_id);
                        if completion_escape {
                            let mut completion = self.completion.clone();
                            completion.visible = false;
                            let caret = egui::widgets::text_edit::TextEditState::load(&child_ctx, egui::Id::new("calculation-pad-source"))
                                .and_then(|state| state.cursor.char_range())
                                .and_then(|range| self.session.source().and_then(|source| {
                                    let projection = self.fold_projection(source, &self.folds).1;
                                    projection.byte_at_character(range.primary.index.0)
                                        .and_then(|byte| projection.source_byte(byte))
                                }));
                            completion.dismissed = self.session.revision().zip(caret);
                            effects.completion = Some(completion);
                        }
                        if local_editor_escape {
                            let mut tools = self.editor_tools.clone();
                            tools.open = false;
                            effects.editor_tools = Some(tools);
                        }
                        effects.viewport_class = class;
                        if independent {
                            effects.viewport_info =
                                Some(child.input(|input| input.viewport().clone()));
                        }
                        effects.native_close_requested = native_close_requested;
                        effects.close_requested = pass_authorized
                            && self.session.pending_transition().is_none()
                            && (native_close_requested
                                || keyboard_close_requested
                                || self.host_close_requested);
                        effects.pass_accepted =
                            pass_authorized && !ctx.will_discard() && !child.will_discard();
                        effects.pass = effects
                            .pass_accepted
                            .then_some(ViewportPassProvenance::current(&child_ctx, viewport_id));
                        effects.focus_viewport = independent
                            && self.host_state.focus_pending()
                            && pass_authorized
                            && application_input_allowed
                            && !modal_open;

                        let mut body_rendered = false;
                        if independent {
                            let _output = egui::CentralPanel::default().show(child, |ui| {
                                body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                                self.draw_host_contents(
                                    ui,
                                    &child_ctx,
                                    identity,
                                    viewport_id,
                                    actions_allowed && body_rendered,
                                    source_focus_allowed && body_rendered,
                                    &mut effects,
                                );
                            });
                            #[cfg(test)]
                            {
                                self.test_window_rect = Some(_output.response.rect);
                            }
                        } else {
                            body_rendered = child.is_visible() && !child.is_sizing_pass();
                            self.draw_host_contents(
                                child,
                                &child_ctx,
                                identity,
                                viewport_id,
                                actions_allowed && body_rendered,
                                source_focus_allowed && body_rendered,
                                &mut effects,
                            );
                            if viewport_id == egui::ViewportId::ROOT {
                                let layer = child.layer_id();
                                effects.embedded_root_layer = Some(layer);
                                let pointer_position =
                                    child.input(|input| input.pointer.interact_pos());
                                let pointer_active = child.input(|input| {
                                    input.pointer.any_pressed() || input.pointer.any_released()
                                });
                                let pointer_over_host = pointer_position.is_some_and(|position| {
                                    child_ctx.layer_id_at(position) == Some(layer)
                                });
                                let focused_id = child.memory(|memory| memory.focused());
                                effects.embedded_root_focused_id =
                                    if pointer_active && pointer_over_host {
                                        focused_id.or(effects.source_editor_id)
                                    } else if pointer_active {
                                        None
                                    } else {
                                        self.host_state
                                            .embedded_root_focused_id()
                                            .filter(|id| focused_id == Some(*id))
                                    };
                            }
                        }

                        if body_rendered
                            && pass_authorized
                            && surface_allowed
                            && !modal_open
                            && !effects.close_requested
                            && let Some((target_viewport, editor_id)) = self.deferred_source_focus
                            && target_viewport == viewport_id
                        {
                            self.deferred_source_focus = None;
                            child.memory_mut(|memory| memory.request_focus(editor_id));
                            if effects.embedded_root_layer.is_some() {
                                effects.embedded_root_focused_id = Some(editor_id);
                            }
                        }

                        #[cfg(test)]
                        if std::mem::take(&mut self.test_request_child_discard) {
                            child.request_discard("Calculation Pad test child pass discard");
                            self.test_child_discard_requested = true;
                        }
                        effects.pass_accepted =
                            pass_authorized && !ctx.will_discard() && !child.will_discard();
                        if !effects.pass_accepted {
                            effects.pass = None;
                            effects.embedded_root_layer = None;
                            effects.embedded_root_focused_id = None;
                        }
                        #[cfg(test)]
                        {
                            self.test_callback_viewport_id = Some(viewport_id);
                        }
                        callback_effects = Some(effects);
                    },
                );
                host_effects = callback_effects;
            }
        }

        let help_effects = self.show_examples(
            ctx,
            first_ui_pass,
            application_input_allowed,
            root_input_allowed,
            root_pass,
        );
        if first_ui_pass && (host_effects.is_some() || help_effects.is_some()) {
            self.pending_root_effects = Some(RootEffects {
                pass: root_pass,
                help: help_effects,
                host: host_effects,
            });
        }
    }

    fn show_examples(
        &mut self,
        ctx: &egui::Context,
        first: bool,
        application_input_allowed: bool,
        root_input_allowed: bool,
        root_pass: ViewportPassProvenance,
    ) -> Option<HelpEffects> {
        if !self.examples_open {
            return None;
        }
        let reference_namespace = ("calculation-pad-reference-host", self.examples_epoch);
        match self.examples_host.presentation() {
            PadHost::Docked => {
                let mut effects = self.empty_examples_effects(egui::ViewportId::ROOT);
                let mut open = true;
                let content = ctx.content_rect().size();
                let min_size = MIN_REFERENCE_SIZE.min(content);
                let output = egui::Window::new("Calculation Pad Examples & Reference")
                    .id(Self::examples_window_id())
                    .order(egui::Order::Middle)
                    .open(&mut open)
                    .resizable(true)
                    .default_size(REFERENCE_SIZE)
                    .min_size(min_size)
                    .max_size(content)
                    .show_utility(
                        ctx,
                        "calculation-pad-reference",
                        Self::examples_window_id(),
                        None,
                        crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(
                            REFERENCE_SIZE,
                        )
                        .with_minimum(min_size)
                        .with_maximum(content),
                        |ui| {
                            // Keep the retained widget tree interactive on a replay;
                            // staged effects are independently guarded by provenance.
                            let allowed = ui.is_visible()
                                && !ui.is_sizing_pass()
                                && root_input_allowed
                                && !ctx.memory(|m| m.top_modal_layer().is_some());
                            ui.add_enabled_ui(allowed, |ui| {
                                ui.push_id(reference_namespace, |ui| {
                                    self.examples_ui(ui, &mut effects)
                                });
                            });
                        },
                    );
                #[cfg(test)]
                {
                    self.test_examples_window_rect =
                        output.as_ref().map(|output| output.response.rect);
                }
                effects.accepted = first
                    && !ctx.will_discard()
                    && output
                        .as_ref()
                        .is_some_and(|output| output.response.rect.intersects(ctx.content_rect()));
                effects.pass = effects.accepted.then_some(root_pass);
                effects.close = effects.accepted
                    && root_input_allowed
                    && !ctx.memory(|memory| memory.top_modal_layer().is_some())
                    && !open;
                effects.accepted.then_some(effects)
            }
            PadHost::Popout => {
                let mut result = None;
                ctx.show_viewport_immediate(
                    reference_viewport_id(),
                    egui::ViewportBuilder::default()
                        .with_title("Calculation Pad Examples & Reference")
                        .with_inner_size(REFERENCE_SIZE)
                        .with_min_inner_size(MIN_REFERENCE_SIZE)
                        .with_resizable(true)
                        .with_visible(true),
                    |child, class| {
                        let child_ctx = child.ctx().clone();
                        let viewport = child_ctx.viewport_id();
                        let independent = class == egui::ViewportClass::Immediate
                            && viewport == reference_viewport_id();
                        let mut effects = self.empty_examples_effects(viewport);
                        effects.viewport_class = class;
                        let (focused, close) = child.input(|input| {
                            (
                                input.focused,
                                independent && input.viewport().close_requested(),
                            )
                        });
                        if independent {
                            effects.viewport_info =
                                Some(child.input(|input| input.viewport().clone()));
                        }
                        let pass_ok = first
                            && child.current_pass_index() == 0
                            && !ctx.will_discard()
                            && !child.will_discard();
                        let surface = if independent {
                            application_input_allowed
                        } else {
                            root_input_allowed && viewport == egui::ViewportId::ROOT
                        };
                        let modal = child.memory(|memory| memory.top_modal_layer().is_some());
                        let allowed = surface && !modal && (!independent || focused) && !close;
                        effects.native_close = close;
                        effects.close = pass_ok && close;
                        effects.focus_viewport = independent
                            && self.examples_host.focus_pending()
                            && pass_ok
                            && application_input_allowed
                            && !modal;
                        if independent {
                            let _output = egui::CentralPanel::default().show(child, |ui| {
                                ui.add_enabled_ui(
                                    allowed && ui.is_visible() && !ui.is_sizing_pass(),
                                    |ui| {
                                        ui.push_id(reference_namespace, |ui| {
                                            self.examples_ui(ui, &mut effects)
                                        });
                                    },
                                );
                            });
                            #[cfg(test)]
                            {
                                self.test_examples_window_rect = Some(_output.response.rect);
                            }
                        } else {
                            let visible = child.is_visible() && !child.is_sizing_pass();
                            child.add_enabled_ui(allowed && visible, |ui| {
                                ui.push_id(reference_namespace, |ui| {
                                    self.examples_ui(ui, &mut effects)
                                });
                            });
                            if viewport == egui::ViewportId::ROOT {
                                effects.embedded_layer = Some(child.layer_id());
                                effects.embedded_focus = child.memory(|memory| memory.focused());
                            }
                        }
                        effects.accepted = pass_ok && !ctx.will_discard() && !child.will_discard();
                        effects.pass = effects
                            .accepted
                            .then_some(ViewportPassProvenance::current(&child_ctx, viewport));
                        result = Some(effects);
                    },
                );
                result
            }
        }
    }

    fn editor_tools_ui(
        &self,
        ui: &mut egui::Ui,
        editor_id: egui::Id,
        source: &str,
        tools: &mut EditorTools,
        effects: &mut HostEffects,
    ) {
        let find_id = egui::Id::new("calculation-pad-find");
        let replace_id = egui::Id::new("calculation-pad-replace");
        let goto_id = egui::Id::new("calculation-pad-goto");
        let focused = ui.memory(|memory| memory.focused());
        let pad_focused = ui.memory(|memory| {
            [editor_id, find_id, replace_id, goto_id]
                .into_iter().any(|id| memory.has_focus(id))
        }) || focused.and_then(|id| ui.ctx().read_response(id))
            .is_some_and(|response| response.layer_id == ui.layer_id())
            || (self.editor_tool_focus_pending && focused.is_none());
        if ui.is_enabled() && pad_focused && self.session.pending_transition().is_none() {
            let shortcut = ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::COMMAND, egui::Key::F) { Some('f') }
                else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::H) { Some('h') }
                else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::G) { Some('g') }
                else { None }
            });
            if let Some(shortcut) = shortcut {
                tools.open = true;
                tools.goto = shortcut == 'g';
                tools.replace = shortcut == 'h';
                tools.message = None;
                effects.editor_tool_focus = Some(if tools.goto { goto_id } else { find_id });
            }
        }
        if !tools.open { return; }
        if self.editor_tool_focus_pending {
            let target = if tools.goto { goto_id } else { find_id };
            if ui.memory(|memory| memory.has_focus(target)) {
                effects.editor_tool_focus_acknowledged = true;
            } else if pad_focused {
                effects.editor_tool_focus = Some(target);
            }
        }
        if ui.is_enabled() && pad_focused
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            tools.open = false;
            effects.editor_tool_focus = None;
            effects.editor_tool_focus_acknowledged = true;
            return;
        }
        let revision = self.session.revision().unwrap_or_default();
        tools.refresh(source, revision);
        ui.group(|ui| {
            if tools.goto {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Go to line");
                    let response = ui.add(egui::TextEdit::singleline(&mut tools.line_input).id(goto_id).desired_width(70.0));
                    let submit = ui.button("Go").clicked()
                        || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                    if submit {
                        let range = tools.line_input.trim().parse::<usize>().ok()
                            .and_then(|line| SourcePositions::new(source).line_range(line));
                        if let Some(range) = range {
                            effects.editor_navigation = Some(range.start..range.start);
                            tools.message = None;
                        } else {
                            tools.message = Some("Enter an existing physical line number.".into());
                        }
                    }
                    if ui.small_button("Close").clicked() { tools.open = false; }
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Find");
                    let changed = ui.add(egui::TextEdit::singleline(&mut tools.query).id(find_id).char_limit(crate::calculation_pad::MAX_DOCUMENT_BYTES).desired_width(150.0)).changed();
                    let case_changed = ui.checkbox(&mut tools.case_sensitive, "Match case").changed();
                    if changed || case_changed { tools.refresh(source, revision); }
                    if ui.small_button("Previous").clicked() { effects.editor_navigation = tools.navigate(false); }
                    if ui.small_button("Next").clicked() { effects.editor_navigation = tools.navigate(true); }
                    let selected = tools.current.map_or(0, |index| index + 1);
                    ui.label(format!("{selected}/{}", tools.matches.len()));
                    if ui.small_button("Close").clicked() { tools.open = false; }
                });
                if tools.replace {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Replace");
                        ui.add(egui::TextEdit::singleline(&mut tools.replacement).id(replace_id).char_limit(crate::calculation_pad::MAX_DOCUMENT_BYTES).desired_width(150.0));
                        if ui.button("Replace").clicked() {
                            if tools.current.is_none() { let _ = tools.navigate(true); }
                            effects.editor_replacement = tools.replacement(source, false);
                            if effects.editor_replacement.is_none() { tools.message = Some("No replacement was made or the result is too large.".into()); }
                        }
                        if ui.button("Replace all").clicked() {
                            effects.editor_replacement = tools.replacement(source, true);
                            if effects.editor_replacement.is_none() { tools.message = Some("No replacement was made or the result is too large.".into()); }
                        }
                    });
                }
            }
            if let Some(message) = &tools.message { ui.label(message); }
        });
        if !tools.open {
            effects.editor_tool_focus = None;
            effects.editor_tool_focus_acknowledged = true;
        }
    }

    fn persist_view_state_in_session(&mut self) {
        let view = crate::calculation_pad::PadViewMetadata {
            inputs: self.pad_view.inputs.iter().map(|input| crate::calculation_pad::PadInputReference {
                id: input.id, name: input.name.clone(), unavailable: input.unavailable,
            }).collect(),
            watches: self.pad_view.watches.clone(),
        };
        if let Err(message) = self.session.set_view_metadata(view) { self.error = Some(message.into()); }
    }

    fn restore_view_from_session(&mut self) {
        let metadata = self.session.view_metadata().cloned().unwrap_or_default();
        self.pad_view = PadView {
            inputs: metadata.inputs.into_iter().map(|input| InputSelection {
                id: input.id, name: input.name, unavailable: input.unavailable, draft: None,
            }).collect(),
            watches: metadata.watches,
            ..PadView::default()
        };
    }

    fn reconcile_input_selections(&mut self, old_source: Option<&str>, old_ids: Option<&[LineId]>, exact_input_edit: bool) {
        if self.pad_view.inputs.is_empty() { return; }
        let source = self.session.source().unwrap_or_default();
        let inputs = crate::calculation_pad::literal_inputs(source);
        let ids = self.session.line_ids().unwrap_or_default();
        for selected in &mut self.pad_view.inputs {
            let ambiguous = !exact_input_edit && old_source.zip(old_ids).is_some_and(|(old, old_ids)| {
                let Some(index) = old_ids.iter().position(|id| *id == selected.id) else { return false; };
                let Some(line) = old.lines().nth(index) else { return false; };
                let previous: Vec<_> = old.lines().enumerate().filter_map(|(i, candidate)| (candidate == line).then_some(i)).collect();
                let current: Vec<_> = source.lines().enumerate().filter_map(|(i, candidate)| (candidate == line).then_some(i)).collect();
                previous.len() > 1 && previous != current
            });
            if ambiguous || !inputs.iter().any(|input| ids.get(input.line) == Some(&selected.id) && input.name == selected.name) {
                selected.unavailable = true;
            }
        }
    }

    fn panels_ui(
        &self, ui: &mut egui::Ui, source: &str,
        evaluation: Option<&crate::calculation_pad::SessionEvaluation>,
        view: &mut PadView, effects: &mut HostEffects,
    ) {
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut view.show_inputs, "Editable inputs");
            ui.checkbox(&mut view.show_watches, "Output watches");
        });
        ui.menu_button("Number format", |ui| {
            use crate::settings::CalculationPadNumberMode as Mode;
            let mut preferences = effects.display_preferences.unwrap_or(self.display_preferences);
            let before = preferences;
            let values = || evaluation.into_iter().flat_map(|result| result.lines.iter()).filter_map(|line| match &line.outcome {
                LineOutcome::Value(value) => Some(value), _ => None,
            });
            let has_precision = values().any(result_format::supports_precision);
            let has_percent = values().any(result_format::supports_percent);
            ui.selectable_value(&mut preferences.number_mode, Mode::Auto, "Auto");
            ui.add_enabled_ui(has_precision, |ui| {
                ui.selectable_value(&mut preferences.number_mode, Mode::Fixed, "Fixed decimal");
                ui.selectable_value(&mut preferences.number_mode, Mode::Scientific, "Scientific");
                ui.add_enabled_ui(preferences.number_mode != Mode::Auto, |ui| {
                    ui.add(egui::Slider::new(&mut preferences.decimal_places, 0..=15).text("Decimal places"));
                });
            });
            ui.checkbox(&mut preferences.group_digits, "Group digits");
            ui.add_enabled(has_percent, egui::Checkbox::new(&mut preferences.percent, "Percent for scalars"));
            if preferences != before { effects.display_preferences = Some(preferences); }
        });
        let ids = self.session.line_ids().unwrap_or_default();
        let revision = self.session.revision().unwrap_or_default();
        if view.show_inputs {
            let available = if let Some(basis) = effects.document_basis {
                ui.ctx().data_mut(|data| {
                    let id = egui::Id::new("calculation-pad-literal-input-cache");
                    let cache = data.get_temp::<InputCache>(id).filter(|cache| cache.basis == basis)
                        .unwrap_or_else(|| InputCache { basis, inputs: crate::calculation_pad::literal_inputs(source) });
                    let inputs = cache.inputs.clone();
                    data.insert_temp(id, cache);
                    inputs
                })
            } else { Vec::new() };
            for selected in &mut view.inputs {
                if !available.iter().any(|input| ids.get(input.line) == Some(&selected.id) && input.name == selected.name) {
                    selected.unavailable = true;
                }
            }
            ui.group(|ui| {
                ui.label("Editable inputs");
                ui.menu_button("Choose inputs", |ui| {
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for input in &available {
                        let Some(&id) = ids.get(input.line) else { continue; };
                        let selected = view.inputs.iter().position(|entry| entry.id == id);
                        let mut checked = selected.is_some_and(|index| !view.inputs[index].unavailable);
                        if ui.checkbox(&mut checked, format!("{} · line {}", input.name, input.line + 1)).changed() {
                            if let Some(index) = selected { view.inputs.remove(index); }
                            if checked && view.inputs.len() < 64 {
                                view.inputs.push(InputSelection { id, name: input.name.clone(), unavailable: false, draft: None });
                            }
                        }
                    }
                    });
                });
                egui::ScrollArea::vertical().id_salt("pad-inputs").max_height(100.0).show(ui, |ui| {
                    let mut remove = None;
                    for (index, selected) in view.inputs.iter_mut().enumerate() {
                        let descriptor = available.iter().find(|input| ids.get(input.line) == Some(&selected.id) && input.name == selected.name);
                        ui.push_id(selected.id.get(), |ui| { ui.horizontal_wrapped(|ui| {
                            ui.label(format!("{} · line {}", selected.name, descriptor.map_or(0, |input| input.line + 1)));
                            if selected.unavailable || descriptor.is_none() {
                                ui.weak("Unavailable; reselect this input");
                            } else if let Some(input) = descriptor {
                                let draft = selected.draft.get_or_insert_with(|| InputDraft {
                                    value: input.value.clone(), original: input.source_line.clone(), revision, error: None, dirty: false,
                                });
                                if !draft.dirty && draft.revision != revision {
                                    *draft = InputDraft { value: input.value.clone(), original: input.source_line.clone(), revision, error: None, dirty: false };
                                }
                                if draft.revision != revision { draft.error = Some("Source changed; reset this draft.".into()); }
                                if ui.add(egui::TextEdit::singleline(&mut draft.value).desired_width(90.0).char_limit(crate::calculation_pad::MAX_EXPRESSION_BYTES)).changed() { draft.dirty = true; draft.error = None; }
                                if ui.button("Apply").clicked() {
                                    effects.input_commit = Some(InputCommit { id: selected.id, original: draft.original.clone(), revision: draft.revision, value: draft.value.clone() });
                                }
                                if ui.button("Reset").clicked() {
                                    *draft = InputDraft { value: input.value.clone(), original: input.source_line.clone(), revision, error: None, dirty: false };
                                }
                                if let Some(error) = &draft.error { ui.colored_label(ui.visuals().error_fg_color, error); }
                            }
                            if ui.small_button("×").clicked() { remove = Some(index); }
                        }); });
                    }
                    if let Some(index) = remove { view.inputs.remove(index); }
                });
            });
        }
        if view.show_watches {
            ui.group(|ui| {
                ui.label("Output watches");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut view.watch_entry).desired_width(100.0).hint_text("Name"));
                    if ui.button("Pin").clicked() {
                        if let Some(name) = crate::calculation_pad::watch_name(&view.watch_entry)
                            && view.watches.len() < 64 && !view.watches.contains(&name)
                        { view.watches.push(name); view.watch_entry.clear(); }
                    }
                });
                egui::ScrollArea::vertical().id_salt("pad-watches").max_height(100.0).show(ui, |ui| {
                    let mut remove = None;
                    let mut swap = None;
                    for (index, name) in view.watches.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(name);
                            match evaluation.and_then(|evaluation| evaluation.final_bindings.get(name)) {
                                Some(binding) => match &binding.outcome {
                                    LineOutcome::Value(value) => {
                                        ui.label(result_format::display(value, self.display_preferences).unwrap_or_else(|| "Unavailable".into()));
                                        ui.weak(match value {
                                            crate::calculation_pad::CalculationValue::Scalar(_) => "scalar",
                                            crate::calculation_pad::CalculationValue::Boolean(_) => "boolean",
                                            crate::calculation_pad::CalculationValue::Sequence(_) => "sequence",
                                            crate::calculation_pad::CalculationValue::Vector(_) => "vector",
                                            crate::calculation_pad::CalculationValue::Integer(_) | crate::calculation_pad::CalculationValue::ExactBase { .. } => "integer",
                                            _ => "quantity",
                                        });
                                        if ui.small_button("Copy").clicked() { effects.watch_copy = Some((name.clone(), false)); }
                                        if ui.small_button("Copy formatted").clicked() { effects.watch_copy = Some((name.clone(), true)); }
                                    }
                                    LineOutcome::Error(error) => { ui.colored_label(ui.visuals().error_fg_color, error.to_string()); }
                                    _ => { ui.weak("Not executed"); }
                                },
                                None if evaluation.is_some_and(|result| result.diagnostic.is_some()) => { ui.weak("Evaluation failed"); }
                                None if evaluation.is_some() => { ui.weak("Missing"); }
                                None => { ui.weak(match self.session.evaluation_status() {
                                    crate::calculation_pad::EvaluationStatus::Failed(_) => "Evaluation failed",
                                    crate::calculation_pad::EvaluationStatus::Inactive => "Unavailable",
                                    _ => "Pending",
                                }); }
                            }
                            if index > 0 && ui.small_button("↑").clicked() { swap = Some((index - 1, index)); }
                            if index + 1 < view.watches.len() && ui.small_button("↓").clicked() { swap = Some((index, index + 1)); }
                            if ui.small_button("×").clicked() { remove = Some(index); }
                        });
                    }
                    if let Some((a, b)) = swap { view.watches.swap(a, b); }
                    if let Some(index) = remove { view.watches.remove(index); }
                });
            });
        }
    }

    fn draw_host_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
        actions_allowed: bool,
        source_focus_allowed: bool,
        effects: &mut HostEffects,
    ) {
        let repaint = ctx.clone();
        self.session.enable_background_evaluation(move || {
            repaint.request_repaint_of(egui::ViewportId::ROOT);
            repaint.request_repaint_of(pad_viewport_id());
        });
        if self.dependency_navigation.is_some_and(|pending| {
            let nav = pending.navigation;
            pending.host != self.host_identity()
                || self.session.current_id() != Some(nav.document)
                || self.session.document_incarnation() != Some(nav.incarnation)
                || self.session.revision() != Some(pending.revision)
                || !self.open
                || !self
                    .session
                    .line_ids()
                    .is_some_and(|ids| ids.contains(&nav.line))
        }) {
            self.dependency_navigation = None;
        }
        let content_rect = ui.clip_rect();
        let editor_id = egui::Id::new("calculation-pad-source");
        effects.source_editor_id = Some(editor_id);
        if source_focus_allowed && self.focus_source {
            effects.source_focus_requested = true;
        }
        if actions_allowed && self.session.pending_transition().is_none()
            && let Some(pending) = self.deferred_projected_input.as_ref()
            && pending.host == identity
            && Some(pending.basis) == effects.document_basis
            && ui.memory(|memory| memory.has_focus(editor_id))
        {
            ui.input_mut(|input| { input.events.splice(0..0, pending.events.clone()); });
            effects.projected_input_replayed = true;
        }
        self.clear_text_edit_undoer(ctx, editor_id);
        ui.add_enabled_ui(actions_allowed, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("calculation-pad-window-content")
                .max_height(content_rect.height())
                .show(ui, |ui| {
                    if actions_allowed && self.session.pending_transition().is_none() {
                        effects.editor_history_action =
                            self.take_editor_history_shortcut(ui, editor_id);
                    }
                    let source = self.session.source().unwrap_or_default().to_owned();
                    let evaluation = self.session.shared_evaluation();
                    let mut view = self.pad_view.clone();
                    ui.add_enabled_ui(self.session.pending_transition().is_none(), |ui| {
                        self.panels_ui(ui, &source, evaluation.as_deref(), &mut view, effects);
                    });
                    if view != self.pad_view || effects.input_commit.is_some() { effects.pad_view = Some(view); }
                    let mut tools = effects.editor_tools.clone().unwrap_or_else(|| self.editor_tools.clone());
                    self.editor_tools_ui(ui, editor_id, &source, &mut tools, effects);
                    if tools != self.editor_tools { effects.editor_tools = Some(tools); }
                    let mut completion = effects.completion.clone().unwrap_or_else(|| self.completion.clone());
                    let mut folds = self.folds.clone();
                    let mut staged_save_as_name = self.save_as_name.clone();
                    self.contents_ui(
                        ui,
                        editor_id,
                        source,
                        evaluation.as_deref(),
                        source_focus_allowed,
                        &mut staged_save_as_name,
                        &mut effects.source_focus_requested,
                        &mut effects.edited_source,
                        &mut effects.intent,
                        &mut effects.host_request,
                        &mut effects.result_copy,
                        &mut effects.inspector_selection,
                        &mut effects.inspector_caret,
                        &mut effects.editor_primary_caret,
                        &mut effects.open_inspector,
                        &mut effects.open_dependencies,
                        &mut effects.dependency_applied,
                        &mut effects.result_navigation,
                        &mut effects.selected_result,
                        &mut effects.result_scroll,
                        &mut effects.display_preferences,
                        &mut effects.display_selection,
                        &mut completion,
                        &mut effects.completion_cursor,
                        &mut folds,
                        &mut effects.projection_state,
                        &mut effects.source_clipboard,
                        &mut effects.deferred_projected_input,
                        &mut effects.split_action,
                        identity,
                        viewport_id,
                    );
                    if completion != self.completion { effects.completion = Some(completion); }
                    if folds != self.folds { effects.folds = Some(folds); }
                    if staged_save_as_name != self.save_as_name {
                        effects.save_as_name = Some(staged_save_as_name);
                    }
                });
        });
    }

    /// Apply host state only after ROOT and the originating host pass are retained.
    /// Source edits precede document intents and result-copy resolution.
    pub(super) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<CalculationPadDialogEffect> {
        let Some(effects) = self.pending_root_effects.take() else {
            return Vec::new();
        };
        let root_retained = effects.pass.was_retained(ctx);
        if !root_retained {
            if effects
                .host
                .as_ref()
                .is_some_and(|host| host.native_close_requested)
            {
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            if effects.help.as_ref().is_some_and(|help| {
                help.native_close
                    && self.examples_open
                    && help.epoch == self.examples_epoch
                    && help.identity == self.examples_host.identity()
                    && help.owner == self.examples_owner()
            }) {
                ctx.send_viewport_cmd_to(
                    reference_viewport_id(),
                    egui::ViewportCommand::CancelClose,
                );
            }
            return Vec::new();
        }

        let help = effects.help;
        let mut reference_open = None;
        let mut delivered = effects.host.map_or_else(Vec::new, |host| {
            self.complete_pad_host(ctx, effects.pass, host, &mut reference_open)
        });
        if (self.examples_open
            || self.examples_owner.is_some()
            || self.examples_host.presentation() == PadHost::Popout)
            && (!self.open || !self.examples_open || self.examples_owner != self.examples_owner())
        {
            if !self.open && self.examples_open {
                super::utility_host::remember_presentation(ctx, reference_tool_key(), self.examples_host.presentation());
            }
            self.retire_examples(ctx);
        }
        if let Some(help) = help {
            self.complete_examples(ctx, help, &mut delivered);
        }
        if let Some((receipt, was_open)) = reference_open
            && self.accepts_inspector_origin(&receipt)
            && self.examples_open
        {
            let key = reference_tool_key();
            let remembered = super::utility_presentation::UtilityPresentationManager::for_context(ctx)
                .and_then(|manager| manager.preference(key))
                .map(Into::into);
            let desired = super::utility_host::resolve_child_presentation(
                was_open.then(|| self.examples_host.presentation()), remembered,
                self.examples_host.presentation(), &super::utility_host::ChildOpenOrigin::Parent(receipt),
            );
            if self.examples_host.transition(desired) {
                self.examples_host.request_focus();
            }
            if desired == PadHost::Popout {
                super::utility_host::remember_presentation(ctx, key, desired);
            }
            if desired == PadHost::Docked {
                delivered.push(CalculationPadDialogEffect::ShowRoot);
            }
        }
        delivered
    }

    pub(super) fn accepts_inspector_origin(&self, origin: &PadOpenReceipt) -> bool {
        self.open
            && !self.host_close_requested
            && !self.open_chooser
            && !self.save_as_open
            && self.session.pending_transition().is_none()
            && self.session.document_incarnation().zip(self.session.current_id())
                .is_some_and(|session| origin.is_current(self.host_identity(), &session))
    }

    fn retire_examples(&mut self, ctx: &egui::Context) {
        let identity = self.examples_host.identity();
        if identity.host == PadHost::Popout {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                ctx,
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: reference_tool_key(),
                    viewport_id: reference_viewport_id(),
                    owner_generation: self.examples_epoch,
                    host_generation: identity.generation,
                },
            );
            ctx.send_viewport_cmd_to(reference_viewport_id(), egui::ViewportCommand::Close);
        }
        self.examples_host.retire_to_docked();
        self.examples_presentation_prepared = false;
        self.examples_open = false;
        self.examples_owner = None;
        self.examples_epoch = self.examples_epoch.wrapping_add(1);
    }

    fn complete_examples(
        &mut self,
        ctx: &egui::Context,
        mut help: HelpEffects,
        delivered: &mut Vec<CalculationPadDialogEffect>,
    ) {
        let retained = help.accepted
            && self.open
            && self.examples_open
            && help.identity == self.examples_host.identity()
            && help.epoch == self.examples_epoch
            && help.owner.is_some()
            && help.owner == self.examples_owner
            && help.owner == self.examples_owner()
            && help
                .pass
                .is_some_and(|pass| pass.viewport_id == help.viewport_id && pass.was_retained(ctx));
        if !retained {
            if help.native_close
                && self.examples_open
                && help.epoch == self.examples_epoch
                && help.identity == self.examples_host.identity()
                && help.owner == self.examples_owner()
            {
                ctx.send_viewport_cmd_to(
                    reference_viewport_id(),
                    egui::ViewportCommand::CancelClose,
                );
            }
            return;
        }
        let needs_repaint = help.search.is_some()
            || help.copy.is_some()
            || help.close
            || help.host_request.is_some()
            || help.focus_viewport
            || self.examples_host.bootstrap_pending();
        if let Some(search) = help.search.take() {
            self.examples_search = search;
        }
        if let Some(scroll) = help.scroll
            && scroll.is_finite()
        {
            self.examples_scroll = scroll.max(0.0);
        }
        if help.close {
            super::utility_host::remember_presentation(ctx, reference_tool_key(), help.identity.host);
            self.retire_examples(ctx);
            ctx.request_repaint();
            return;
        }
        let old = help.identity;
        if let Some(requested) = help.host_request
            && self.examples_host.transition(requested)
        {
            super::utility_host::remember_presentation(ctx, reference_tool_key(), requested);
            if requested == PadHost::Docked {
                crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                    ctx,
                    crate::gui::utility_window_geometry::NativeViewportIdentity {
                        key: reference_tool_key(),
                        viewport_id: help.viewport_id,
                        owner_generation: help.epoch,
                        host_generation: old.generation,
                    },
                );
                ctx.send_viewport_cmd_to(reference_viewport_id(), egui::ViewportCommand::Close);
                delivered.push(CalculationPadDialogEffect::ShowRoot);
            }
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if self.examples_host.identity() == old && old.host == PadHost::Popout {
            if let Some(info) = help.viewport_info.as_ref() {
                crate::gui::utility_dialog_size::apply_native_utility_geometry_restore(
                    ctx,
                    help.viewport_class,
                    true,
                    "Calculation Pad Examples & Reference",
                    crate::gui::utility_window_geometry::NativeViewportIdentity {
                        key: reference_tool_key(),
                        viewport_id: help.viewport_id,
                        owner_generation: help.epoch,
                        host_generation: old.generation,
                    },
                    info,
                    REFERENCE_SIZE,
                    Some(MIN_REFERENCE_SIZE),
                    None,
                );
            }
            if help.focus_viewport {
                ctx.send_viewport_cmd_to(reference_viewport_id(), egui::ViewportCommand::Focus);
                self.examples_host.complete_focus(old);
            }
        }
        if self.examples_host.identity() == old {
            self.examples_host
                .complete_bootstrap(old, help.embedded_layer, help.embedded_focus);
            if let Some(text) = help.copy {
                delivered.push(CalculationPadDialogEffect::CopyText(text));
            }
        }
        if needs_repaint {
            ctx.request_repaint_of(help.viewport_id);
        }
    }

    fn complete_pad_host(
        &mut self,
        ctx: &egui::Context,
        root: ViewportPassProvenance,
        mut host: HostEffects,
        reference_open: &mut Option<(PadOpenReceipt, bool)>,
    ) -> Vec<CalculationPadDialogEffect> {
        let mut delivered = Vec::new();
        let host_pass_retained = host.pass_accepted
            && host.identity == self.host_identity()
            && self.open
            && host
                .pass
                .is_some_and(|pass| pass.viewport_id == host.viewport_id && pass.was_retained(ctx));
        if !host_pass_retained {
            if host.native_close_requested {
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return delivered;
        }

        if let Some(preferences) = host.display_preferences.take()
            && preferences != self.display_preferences
        {
            self.display_preferences = preferences;
            delivered.push(CalculationPadDialogEffect::SaveDisplayPreferences(preferences));
            ctx.request_repaint_of(host.viewport_id);
        }
        match host.split_action.take() {
            Some(SplitAction::Preview(preview)) if preview.host == host.identity => {
                self.split_preview = Some(preview);
                ctx.request_repaint_of(host.viewport_id);
            }
            Some(SplitAction::Preview(_)) => {}
            Some(SplitAction::Finish(preview)) => {
                let accepted = preview.host == host.identity
                    && self.split_preview.is_none_or(|old| old.host == host.identity && old.orientation == preview.orientation);
                self.split_preview = None;
                if accepted {
                    let mut preferences = self.display_preferences;
                    match preview.orientation {
                        SplitOrientation::Wide => preferences.wide_source_ratio = preview.ratio,
                        SplitOrientation::Stacked => preferences.stacked_source_ratio = preview.ratio,
                    }
                    if preferences != self.display_preferences {
                        self.display_preferences = preferences;
                        delivered.push(CalculationPadDialogEffect::SaveDisplayPreferences(preferences));
                    }
                }
                ctx.request_repaint_of(host.viewport_id);
            }
            Some(SplitAction::Cancel) => self.split_preview = None,
            None => {}
        }

        let current_basis = self
            .session
            .document_incarnation()
            .zip(self.session.current_id())
            .zip(self.session.revision())
            .map(|((incarnation, document), revision)| (incarnation, document, revision));
        // A pending batch gets exactly one retained opportunity on its owning host.
        if self.deferred_projected_input.is_some() {
            self.deferred_projected_input = None;
        }
        if !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && self.session.pending_transition().is_none()
            && host.document_basis == current_basis
            && let (Some(events), Some(basis)) = (host.deferred_projected_input.take(), current_basis)
        {
            self.deferred_projected_input = Some(DeferredProjectedInput { host: host.identity, basis, events });
            ctx.request_repaint_of(host.viewport_id);
        }
        if host.document_basis != current_basis {
            host.edited_source = None;
            host.pad_view = None;
            host.input_commit = None;
            host.watch_copy = None;
            host.editor_history_action = None;
            host.inspector_selection = None;
            host.inspector_caret = None;
            host.open_inspector = false;
            host.open_dependencies = false;
            host.dependency_applied = None;
            host.result_navigation = None;
            host.result_copy = None;
            host.editor_tools = None;
            host.completion = None;
            host.completion_cursor = None;
            host.folds = None;
            host.projection_state = None;
            host.source_clipboard = None;
            host.editor_replacement = None;
            host.editor_navigation = None;
            host.editor_tool_focus = None;
            host.editor_tool_focus_acknowledged = false;
            host.selected_result = None;
            host.editor_primary_caret = None;
            host.result_scroll = None;
        }

        if host.edited_source.is_some() || host.editor_history_action.is_some() {
            host.pad_view = None;
            host.input_commit = None;
            host.watch_copy = None;
        }
        let mut accepted_input_commit = false;
        if host.edited_source.is_none() && host.editor_history_action.is_none()
            && host.intent.is_none() && host.host_request.is_none() && !host.close_requested
            && let Some(commit) = host.input_commit.take()
        {
            match self.session.input_replacement(commit.revision, commit.id, &commit.original, &commit.value) {
                Ok(source) => {
                    accepted_input_commit = true;
                    host.edited_source = Some(source);
                    if let Some(view) = host.pad_view.as_mut()
                        && let Some(input) = view.inputs.iter_mut().find(|input| input.id == commit.id)
                    { input.draft = None; }
                }
                Err(error) => {
                    if let Some(view) = host.pad_view.as_mut()
                        && let Some(draft) = view.inputs.iter_mut().find(|input| input.id == commit.id).and_then(|input| input.draft.as_mut())
                    { draft.error = Some(error); }
                }
            }
        }
        if host.edited_source.is_none()
            && host.editor_history_action.is_none()
            && self.session.pending_transition().is_none()
        {
            if let Some(replacement) = host.editor_replacement.take() {
                host.edited_source = Some(replacement);
                host.inspector_caret = None;
                host.editor_primary_caret = None;
            }
        }
        let needs_repaint = host.edited_source.is_some()
            || host.save_as_name.is_some()
            || host.source_focus_requested
            || host.editor_history_action.is_some()
            || host.intent.is_some()
            || host.host_request.is_some()
            || host.result_copy.is_some()
            || host.editor_tools.is_some()
            || host.editor_navigation.is_some()
            || host.editor_tool_focus.is_some()
            || host.selected_result.is_some()
            || host.editor_primary_caret.is_some()
            || host.result_scroll.is_some()
            || host.close_requested
            || host.focus_viewport
            || self.host_state.bootstrap_pending();
        let host_request_requested = host.host_request.is_some();
        let accepted_source_or_history =
            host.edited_source.is_some() || host.editor_history_action.is_some();
        let before_document = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        let source_was_edited = host.edited_source.is_some();
        let old_source = (!self.pad_view.inputs.is_empty() && accepted_source_or_history)
            .then(|| self.session.source().unwrap_or_default().to_owned());
        let old_ids = old_source.as_ref().map(|_| self.session.line_ids().unwrap_or_default().to_vec());

        if let Some(source) = host.edited_source.take() {
            if let Err(error) = self.session.replace_source(source) {
                self.set_error(error);
                if host.close_requested {
                    self.host_close_requested = false;
                }
                if host.native_close_requested {
                    ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
                }
                ctx.request_repaint();
                return delivered;
            }
            self.notice = None;
        }
        if let Some(name) = host.save_as_name.take() {
            self.save_as_name = name;
        }
        let history_caret = if host.editor_history_action.is_some() && !self.folds.collapsed.is_empty() {
            host.source_editor_id.and_then(|id| {
                let state = egui::widgets::text_edit::TextEditState::load(ctx, id)?;
                let selection = state.cursor.char_range()?;
                let source = self.session.source()?;
                let projection = self.fold_projection(source, &self.folds).1;
                Some((id, state, projection.source_character(source, selection.primary.index.0)?, projection.source_character(source, selection.secondary.index.0)?))
            })
        } else { None };
        match host.editor_history_action {
            Some(true) => {
                self.session.undo();
                ctx.request_repaint();
            }
            Some(false) => {
                self.session.redo();
                ctx.request_repaint();
            }
            None => {}
        }
        if let Some((id, mut state, primary, secondary)) = history_caret
            && let Some(source) = self.session.source()
        {
            self.folds.collapsed.clear();
            let end = source.chars().count();
            let mut selection = state.cursor.char_range().unwrap_or_default();
            selection.primary.index = primary.min(end).into();
            selection.secondary.index = secondary.min(end).into();
            state.cursor.set_char_range(Some(selection));
            state.store(ctx, id);
        }
        if self.session.pending_transition().is_none()
            && let Some(view) = host.pad_view.take()
        { self.pad_view = view; }
        if accepted_source_or_history {
            self.reconcile_input_selections(old_source.as_deref(), old_ids.as_deref(), accepted_input_commit);
        }
        if current_basis.is_some() && host.document_basis == current_basis && self.session.pending_transition().is_none() {
            self.persist_view_state_in_session();
        }
        if accepted_source_or_history {
            self.source_highlight = None;
            self.dependency_navigation = None;
            self.selected_result = None;
            self.active_source_line = None;
            self.pending_result_scroll = None;
            self.schedule_recovery_snapshot(ctx);
        }
        if let Some(completion) = host.completion.take() { self.completion = completion; }
        if let Some(folds) = host.folds.take()
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && let (Some(editor_id), Some(source)) = (host.source_editor_id, self.session.source())
        {
            let (_, old_projection) = self.fold_projection(source, &self.folds);
            let (_, next_projection) = self.fold_projection(source, &folds);
            if !accepted_source_or_history && host.projection_state.is_none()
                && let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, editor_id)
                && let Some(range) = state.cursor.char_range()
                && let (Some(primary), Some(secondary)) = (
                    old_projection.source_character(source, range.primary.index.0),
                    old_projection.source_character(source, range.secondary.index.0),
                )
                && let (Some(primary), Some(secondary)) = (
                    next_projection.visible_character_clamped(source, primary),
                    next_projection.visible_character_clamped(source, secondary),
                )
            {
                let mut mapped = range;
                mapped.primary.index = primary.into();
                mapped.secondary.index = secondary.into();
                state.cursor.set_char_range(Some(mapped));
                state.store(ctx, editor_id);
            }
            self.folds = folds;
            ctx.request_repaint_of(host.viewport_id);
        }
        if let Some(state) = host.projection_state.take()
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && let Some(editor_id) = host.source_editor_id
        {
            state.store(ctx, editor_id);
        }
        if let Some(tools) = host.editor_tools.take() {
            self.editor_tools = tools;
            if !self.editor_tools.open { self.editor_tool_focus_pending = false; }
        }
        if host.editor_tool_focus_acknowledged { self.editor_tool_focus_pending = false; }
        if let Some(id) = host.editor_tool_focus
            && host.host_request.is_none()
            && !host.close_requested
            && !host.native_close_requested
        {
            self.editor_tool_focus_pending = true;
            if host.viewport_id == egui::ViewportId::ROOT {
                ctx.memory_mut(|memory| memory.request_focus(id));
            } else {
                self.deferred_source_focus = Some((host.viewport_id, id));
            }
        }

        let recovery_transition_pending = host.intent.as_ref().is_some_and(|intent| {
            matches!(
                intent,
                DialogIntent::New
                    | DialogIntent::Open(_)
                    | DialogIntent::Decision(_)
                    | DialogIntent::SaveAs(_)
            )
        });
        let before_intent_recovery = (self.remember_source && recovery_transition_pending)
            .then(|| self.recovery_snapshot())
            .flatten();
        let pending_before_intent = self.session.pending_transition();
        let mut pending_decision = None;
        let mut transition_intent = false;
        if let Some(intent) = host.intent.take() {
            transition_intent = matches!(
                &intent,
                DialogIntent::New
                    | DialogIntent::Open(_)
                    | DialogIntent::Decision(_)
                    | DialogIntent::SaveAs(_)
            );
            if let DialogIntent::Decision(decision) = &intent {
                pending_decision = Some(*decision);
            } else if matches!(&intent, DialogIntent::SaveAs(_)) {
                pending_decision = Some(TransitionDecision::Save);
            }
            let show_root = matches!(&intent, DialogIntent::OpenExamples);
            let reference_was_open = self.examples_open;
            let requested_recovery_preference = match &intent {
                DialogIntent::SetRememberSource(enabled) => Some(*enabled),
                _ => None,
            };
            self.apply_intent(intent);
            self.prepare_open_presentation(ctx);
            if show_root && self.examples_open
                && let (Some(incarnation), Some(document), Some(owner)) = (
                    self.session.document_incarnation(), self.session.current_id(), host.pass,
                )
                && let Some(receipt) = super::utility_host::AcceptedParentOpen::from_retained(
                    ctx, host.identity, (incarnation, document), root, owner,
                )
            {
                *reference_open = Some((receipt, reference_was_open));
            }
            if let Some(enabled) = requested_recovery_preference
                && enabled != self.remember_source
            {
                delivered.push(CalculationPadDialogEffect::SaveRecoveryPreference(enabled));
            }
        }
        if transition_intent && self.remember_source {
            let after_intent_recovery = self.recovery_snapshot();
            if let Some(snapshot) = after_intent_recovery.clone()
                && before_intent_recovery.as_ref() != Some(&snapshot)
            {
                if let Err(error) = self.save_recovery_now(Some(snapshot)) {
                    self.set_recovery_error(error.to_string());
                }
            } else if after_intent_recovery.is_none()
                && !self.open
                && pending_before_intent == Some(PendingTransition::Close)
                && pending_decision == Some(TransitionDecision::Save)
                && let Some(snapshot) = before_intent_recovery
                && let Err(error) = self.save_recovery_now(Some(snapshot))
            {
                self.set_recovery_error(error.to_string());
            }
        }
        let after_document = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        if before_document != after_document {
            if before_document.map(|(incarnation, _)| incarnation) != after_document.map(|(incarnation, _)| incarnation) {
                self.restore_view_from_session();
            }
            self.completion = CompletionState::default();
            self.editor_tools = EditorTools::default();
            self.folds.collapsed.clear();
            self.selected_result = None;
            self.active_source_line = None;
            self.pending_result_scroll = None;
        }
        if before_document == after_document
            && let Some((incarnation, document)) = after_document
        {
            if let Some(line) = host.inspector_selection.take()
                && self
                    .session
                    .line_ids()
                    .is_some_and(|ids| ids.contains(&line))
            {
                let selected = InspectorSelection {
                    incarnation,
                    document,
                    line,
                };
                if self.inspector_selection.as_ref() != Some(&selected) {
                    self.inspector_selection = Some(selected);
                    self.inspector_selection_generation =
                        self.inspector_selection_generation.wrapping_add(1);
                }
            }
            if host.editor_history_action.is_none()
                && let Some(range) = host.inspector_caret.take()
                && let Some(revision) = self.session.revision()
                && self
                    .session
                    .source()
                    .and_then(|source| source.get(range.clone()))
                    .is_some()
            {
                let next = InspectorCaret {
                    incarnation,
                    document,
                    revision,
                    range,
                    generation: self.inspector_caret_generation,
                };
                if self.inspector_caret.as_ref().is_none_or(|old| {
                    old.incarnation != next.incarnation
                        || old.document != next.document
                        || old.revision != next.revision
                        || old.range != next.range
                }) {
                    self.selected_result = None;
                    self.inspector_caret_generation =
                        self.inspector_caret_generation.wrapping_add(1);
                    self.inspector_caret = Some(InspectorCaret {
                        generation: self.inspector_caret_generation,
                        ..next
                    });
                }
            } else if source_was_edited || host.editor_history_action.is_some() {
                self.inspector_caret = None;
            }
        }
        if before_document == after_document
            && host.editor_history_action.is_none()
            && let Some(byte) = host.editor_primary_caret.take()
            && let (Some(incarnation), Some(document), Some(revision), Some(source)) = (
                self.session.document_incarnation(), self.session.current_id(),
                self.session.revision(), self.session.source(),
            )
            && let Some(index) = SourcePositions::new(source).line_at(byte)
            && let Some(line) = self.session.line_id(index)
        {
            let current = (incarnation, document, revision, line);
            if self.active_source_line != Some(current) {
                self.active_source_line = Some(current);
                self.pending_result_scroll = Some(current);
                self.selected_result = None;
            }
        }
        self.sync_inspector_context();
        if !accepted_source_or_history
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && let Some((incarnation, document, revision)) = host.document_basis
            && let Some(line) = host.selected_result.take()
            && self.session.line_ids().is_some_and(|ids| ids.contains(&line))
        {
            let navigation = super::calculation_dependency_dialog::DependencyNavigation { incarnation, document, line };
            if self.can_navigate_source(navigation, host.identity) {
                self.selected_result = Some((incarnation, document, revision, line));
                self.active_source_line = None;
                self.pending_result_scroll = None;
                self.dependency_navigation = Some(SourceNavigation {
                    host: host.identity, navigation, revision, result_navigation: true,
                    selection: None, focus_source: false,
                });
                ctx.request_repaint_of(host.viewport_id);
            }
        }
        if !accepted_source_or_history
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && let Some(range) = host.editor_navigation.take()
            && let Some((incarnation, document, revision)) = host.document_basis
            && let Some(source) = self.session.source()
            && source.get(range.clone()).is_some()
            && let Some(index) = SourcePositions::new(source).line_at(range.start)
            && let Some(line) = self.session.line_id(index)
        {
            let navigation = super::calculation_dependency_dialog::DependencyNavigation { incarnation, document, line };
            if self.can_navigate_live_source(navigation, host.identity) {
                self.dependency_navigation = Some(SourceNavigation {
                    host: host.identity, navigation, revision, result_navigation: false,
                    selection: Some((range.start, range.end)), focus_source: false,
                });
                ctx.request_repaint_of(host.viewport_id);
            }
        }
        if host.open_inspector || host.open_dependencies {
            match self.session.evaluation_status() {
                crate::calculation_pad::EvaluationStatus::Pending => {
                    self.notice = Some("Calculation is pending. Try again when the results are ready.".into());
                    host.open_inspector = false;
                    host.open_dependencies = false;
                }
                crate::calculation_pad::EvaluationStatus::Failed(message) => {
                    self.error = Some(message);
                    host.open_inspector = false;
                    host.open_dependencies = false;
                }
                _ => {}
            }
        }
        if host.open_inspector && !host.close_requested && host.host_request.is_none()
            && host.document_basis.map(|(incarnation, document, _)| (incarnation, document))
                == self.session.document_incarnation().zip(self.session.current_id())
            && let (Some(incarnation), Some(document), Some(owner)) = (
                self.session.document_incarnation(), self.session.current_id(), host.pass,
            )
            && let Some(receipt) = super::utility_host::AcceptedParentOpen::from_retained(
                ctx, host.identity, (incarnation, document), root, owner,
            )
        {
            delivered.push(CalculationPadDialogEffect::OpenInspector(receipt));
        }
        if host.open_dependencies
            && !host.close_requested
            && host.host_request.is_none()
            && host
                .document_basis
                .map(|(incarnation, document, _)| (incarnation, document))
                == self
                    .session
                    .document_incarnation()
                    .zip(self.session.current_id())
            && let (Some(incarnation), Some(document), Some(owner)) = (
                self.session.document_incarnation(),
                self.session.current_id(),
                host.pass,
            )
            && let Some(receipt) = super::utility_host::AcceptedParentOpen::from_retained(
                ctx,
                host.identity,
                (incarnation, document),
                root,
                owner,
            )
        {
            delivered.push(CalculationPadDialogEffect::OpenDependencies(receipt));
        }
        if let Some((nav, revision)) = host.result_navigation.take()
            && !accepted_source_or_history
            && !host.close_requested
            && !host.native_close_requested
            && host.host_request.is_none()
            && self.session.revision() == Some(revision)
            && self.can_navigate_source(nav, host.identity)
        {
            self.dependency_navigation = Some(SourceNavigation {
                host: host.identity,
                navigation: nav,
                revision,
                result_navigation: true,
                selection: None,
                focus_source: true,
            });
            ctx.request_repaint_of(host.viewport_id);
        }
        if let Some(update) = host.result_scroll.take()
            && self.pending_result_scroll == Some(update.target)
            && host.document_basis == Some((update.target.0, update.target.1, update.target.2))
            && !accepted_source_or_history
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
        {
            update.state.store(ctx, update.scroll_id);
            self.pending_result_scroll = None;
            ctx.request_repaint_of(host.viewport_id);
        }
        if let Some(update) = host.dependency_applied
            && self.dependency_navigation == Some(SourceNavigation {
                host: host.identity,
                navigation: update.navigation,
                revision: update.revision,
                result_navigation: update.result_navigation,
                selection: update.selection,
                focus_source: update.focus_source,
            })
            && self.session.revision() == Some(update.revision)
            && self.host_identity() == host.identity
            && self.open
            && !host.close_requested && !host.native_close_requested && host.host_request.is_none()
            && (update.selection.is_none() || self.can_navigate_live_source(update.navigation, host.identity))
            && (!update.result_navigation || (
                !host.close_requested && !host.native_close_requested
                    && host.host_request.is_none()
                    && self.can_navigate_source(update.navigation, host.identity)
            ))
        {
            update.editor_state.store(ctx, update.editor_id);
            update.scroll_state.store(ctx, update.scroll_id);
            if update.focus_source { self.host_state.request_focus(); }
            ctx.request_repaint_of(host.viewport_id);
            host.source_focus_requested |= update.focus_source;
            self.dependency_navigation = None;
            if let Some((start, end)) = update.selection {
                self.selected_result = None;
                let current = (update.navigation.incarnation, update.navigation.document, update.revision, update.navigation.line);
                if self.active_source_line != Some(current) {
                    self.active_source_line = Some(current);
                    self.pending_result_scroll = Some(current);
                }
                self.inspector_caret_generation = self.inspector_caret_generation.wrapping_add(1);
                self.inspector_caret = Some(InspectorCaret {
                    incarnation: update.navigation.incarnation,
                    document: update.navigation.document,
                    revision: update.revision,
                    range: start..end,
                    generation: self.inspector_caret_generation,
                });
                self.sync_inspector_context();
            }
            if update.result_navigation {
                let now = ctx.input_for(host.viewport_id, |input| input.time);
                self.source_highlight = Some(SourceHighlight {
                    host: host.identity,
                    navigation: update.navigation,
                    revision: update.revision,
                    expires_at: now + 1.5,
                });
                self.inspector_caret_generation = self.inspector_caret_generation.wrapping_add(1);
                self.inspector_caret = Some(InspectorCaret {
                    incarnation: update.navigation.incarnation,
                    document: update.navigation.document,
                    revision: update.revision,
                    range: update.source_byte_offset..update.source_byte_offset,
                    generation: self.inspector_caret_generation,
                });
                self.sync_inspector_context();
            }
        }
        if self.source_highlight.is_some_and(|highlight| {
            highlight.host != self.host_identity()
                || self.session.revision() != Some(highlight.revision)
                || self.session.resolve_dependency_line(highlight.navigation.document, highlight.navigation.incarnation, highlight.navigation.line).is_none()
                || ctx.input_for(host.viewport_id, |input| input.time) >= highlight.expires_at
        }) {
            self.source_highlight = None;
        }
        if host.close_requested {
            self.host_close_requested = false;
            if self.open {
                let recovery_before_close = self.recovery_snapshot();
                let remembered_scratch = self.remember_source
                    && !self.session.is_saved()
                    && !self.session.is_view_dirty()
                    && self.session.pending_transition().is_none()
                    && recovery_before_close.is_some();
                if remembered_scratch {
                    match self.save_recovery_now(recovery_before_close.clone()) {
                        Ok(()) => match self.session.close_recovered_scratch() {
                            Ok(()) => {
                                self.open = false;
                                self.reset_transient_ui();
                            }
                            Err(error) => self.set_error(error),
                        },
                        Err(error) => self.set_recovery_error(error.to_string()),
                    }
                } else {
                    self.apply_close_request();
                }
                if !self.open && self.session.source().is_none() {
                    if !remembered_scratch
                        && let Err(error) = self.save_recovery_now(recovery_before_close)
                    {
                        self.set_recovery_error(error.to_string());
                    }
                    self.recovery_attempted = false;
                }
            }
        }

        if let Some(requested_host) = host.host_request.take()
            && self.open
            && self.apply_host_request(ctx, requested_host)
            && requested_host == PadHost::Docked
        {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                ctx,
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
            );
            delivered.push(CalculationPadDialogEffect::ShowRoot);
        }

        if self.host_identity() == host.identity
            && host.identity.host == PadHost::Popout
            && !host.close_requested
            && !host.native_close_requested
            && let Some(viewport_info) = host.viewport_info.as_ref()
        {
            crate::gui::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                host.viewport_class,
                host.pass_accepted,
                "Calculation Pad",
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
                viewport_info,
                egui::vec2(PAD_VIEWPORT_SIZE[0], PAD_VIEWPORT_SIZE[1]),
                Some(egui::vec2(
                    MIN_PAD_VIEWPORT_SIZE[0],
                    MIN_PAD_VIEWPORT_SIZE[1],
                )),
                None,
            );
        }

        if let Some(selection) = host.completion_cursor
            && source_was_edited && host.editor_history_action.is_none()
            && before_document == after_document
            && self.host_identity() == host.identity
            && !host_request_requested && !host.close_requested && !host.native_close_requested
            && !transition_intent
            && host.document_basis == current_basis
            && self.session.revision() == current_basis.map(|(_, _, revision)| revision.saturating_add(1))
            && let Some(editor_id) = host.source_editor_id
            && let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, editor_id)
        {
            state.cursor.set_char_range(Some(selection));
            state.store(ctx, editor_id);
            host.source_focus_requested = true;
        }
        if let Some(selection) = host.display_selection
            && !accepted_source_or_history
            && host.document_basis == current_basis
            && !host.close_requested
            && !host.native_close_requested
            && !host_request_requested
            && self.dependency_navigation.is_none()
            && let Some(editor_id) = host.source_editor_id
            && let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, editor_id)
        {
            state.cursor.set_char_range(Some(selection));
            state.store(ctx, editor_id);
            host.source_focus_requested = true;
        }
        if host.source_focus_requested {
            if self.host_state.identity() == host.identity {
                if host.viewport_id == egui::ViewportId::ROOT {
                    if let Some(editor_id) = host.source_editor_id {
                        ctx.memory_mut(|memory| memory.request_focus(editor_id));
                    }
                } else if let Some(editor_id) = host.source_editor_id {
                    self.deferred_source_focus = Some((host.viewport_id, editor_id));
                }
                self.focus_source = false;
            } else if self.open {
                self.request_source_focus();
            }
        }

        if host.focus_viewport && self.host_state.identity() == host.identity {
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Focus);
            self.host_state.complete_focus(host.identity);
        }

        if host.identity == self.host_identity() {
            self.host_state.complete_bootstrap(
                host.identity,
                host.embedded_root_layer,
                host.embedded_root_focused_id,
            );
        }

        if let Some((name, formatted)) = host.watch_copy.take()
            && !accepted_source_or_history && !transition_intent && !host_request_requested
            && !host.close_requested && !host.native_close_requested && self.session.pending_transition().is_none()
            && self.host_identity() == host.identity && host.document_basis == current_basis
            && self.session.revision() == current_basis.map(|(_, _, revision)| revision)
            && let Some(evaluation) = self.session.shared_evaluation()
            && let Some(binding) = evaluation.final_bindings.get(&name)
            && let LineOutcome::Value(value) = &binding.outcome
            && let Some(text) = if formatted { result_format::display(value, self.display_preferences) } else { value.canonical_result() }
        { delivered.push(CalculationPadDialogEffect::CopyText(text)); }

        if let Some(copy) = host.result_copy.take()
            && !host.close_requested
            && self.open
            && copy.host == self.host_identity()
            && copy.viewport_id == host.viewport_id
            && (!matches!(copy.target, ResultCopyTarget::FormattedLine(_))
                || (!accepted_source_or_history && !transition_intent && !host_request_requested
                    && !host.native_close_requested && self.session.pending_transition().is_none()
                    && host.document_basis == current_basis
                    && self.session.revision() == current_basis.map(|(_, _, revision)| revision)))
        {
            let copied = self.resolve_result_copy(copy);
            if let Some(effect) = copied {
                delivered.push(effect);
            } else {
                match self.session.evaluation_status() {
                    crate::calculation_pad::EvaluationStatus::Pending =>
                        self.notice = Some("Calculation is pending. Try Copy when the results are ready.".into()),
                    crate::calculation_pad::EvaluationStatus::Failed(message) => self.error = Some(message),
                    _ => {}
                }
            }
        }
        if let Some(text) = host.source_clipboard.take()
            && host.document_basis == current_basis
            && !host.close_requested && !host.native_close_requested && !host_request_requested
            && !transition_intent
        {
            delivered.push(CalculationPadDialogEffect::CopyText(text));
        }

        if !self.open {
            super::utility_host::remember_presentation(ctx, calculation_pad_tool_key(), host.identity.host);
        }
        let close_popout = host.identity.host == PadHost::Popout && !self.open;
        if close_popout {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                ctx,
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
            );
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Close);
            self.reset_host_to_docked();
        } else if host.native_close_requested {
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
        }

        if needs_repaint {
            ctx.request_repaint_of(host.viewport_id);
        }
        self.report_async_recovery_errors();
        if !self.open {
            self.recovery_attempted = false;
        }
        if self.host_state.bootstrap_pending() || host_request_requested {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        self.sync_inspector_context();
        delivered
    }

    fn sync_inspector_context(&mut self) {
        let identity = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        if self.inspector_selection.as_ref().is_some_and(|selected| {
            identity != Some((selected.incarnation, selected.document))
                || !self
                    .session
                    .line_ids()
                    .is_some_and(|ids| ids.contains(&selected.line))
        }) {
            self.inspector_selection = None;
            self.inspector_selection_generation =
                self.inspector_selection_generation.wrapping_add(1);
        }
        if self.inspector_caret.as_ref().is_some_and(|caret| {
            identity != Some((caret.incarnation, caret.document))
                || self.session.revision() != Some(caret.revision)
        }) {
            self.inspector_caret = None;
            self.inspector_caret_generation = self.inspector_caret_generation.wrapping_add(1);
        }
        self.session.evaluation();
        let evaluation_ready = self.session.evaluation_status() == crate::calculation_pad::EvaluationStatus::Ready;
        let key =
            identity
                .zip(self.session.revision())
                .map(|((incarnation, document), revision)| {
                    (
                        incarnation,
                        document,
                        revision,
                        self.inspector_selection
                            .as_ref()
                            .map(|selection| selection.line),
                        self.inspector_selection_generation,
                        evaluation_ready,
                    )
                });
        let mut context = None;
        if key != self.inspector_context_key {
            let selected = self
                .inspector_selection
                .as_ref()
                .map(|selection| selection.line);
            let candidate = selected.and_then(|line| self.session.inspector_source(line));
            let comparison = self
                .inspector_context
                .comparison
                .as_ref()
                .filter(|context| {
                    Some((context.incarnation, context.document_id)) == identity
                        && Some(context.revision) == self.session.revision()
                })
                .cloned()
                .or_else(|| self.session.comparison_context().map(std::sync::Arc::new));
            context = Some(InspectorPadContext {
                candidate,
                comparison,
                selected_line: selected,
                caret: self.inspector_caret.clone(),
                selection_generation: self.inspector_selection_generation,
                editable: self.session.pending_transition().is_none(),
            });
            self.inspector_context_key = key;
        }
        if context.is_none()
            && (self.inspector_context.caret != self.inspector_caret
                || self.inspector_context.selection_generation
                    != self.inspector_selection_generation
                || self.inspector_context.editable != self.session.pending_transition().is_none())
        {
            let mut updated = (*self.inspector_context).clone();
            updated.caret = self.inspector_caret.clone();
            updated.selection_generation = self.inspector_selection_generation;
            updated.editable = self.session.pending_transition().is_none();
            context = Some(updated);
        }
        if let Some(context) = context {
            self.inspector_context = std::sync::Arc::new(context);
        }
    }

    pub(super) fn dependency_context(
        &mut self,
    ) -> Option<std::sync::Arc<crate::calculation_pad::DependencySnapshot>> {
        if !self.open
            || self.host_close_requested
            || self.open_chooser
            || self.save_as_open
            || self.session.pending_transition().is_some()
        {
            return None;
        }
        self.session.dependency_snapshot()
    }

    pub(super) fn dependency_evaluation_state(
        &self,
    ) -> (Option<(DocumentId, DocumentIncarnation)>, crate::calculation_pad::EvaluationStatus) {
        if !self.open || self.host_close_requested || self.open_chooser || self.save_as_open
            || self.session.pending_transition().is_some()
        {
            return (None, crate::calculation_pad::EvaluationStatus::Inactive);
        }
        (self.session.current_id().zip(self.session.document_incarnation()), self.session.evaluation_status())
    }

    pub(super) fn dependency_host(&self) -> Option<super::utility_host::HostIdentity> {
        (self.open
            && !self.host_close_requested
            && !self.open_chooser
            && !self.save_as_open
            && self.session.pending_transition().is_none())
        .then(|| self.host_identity())
    }

    pub(super) fn dependency_seed(&self) -> Option<LineId> {
        let caret = self.inspector_caret.as_ref()?;
        if self.session.document_incarnation() != Some(caret.incarnation)
            || self.session.current_id() != Some(caret.document)
            || self.session.revision() != Some(caret.revision)
        {
            return None;
        }
        let source = self.session.source()?;
        let index = source
            .get(..caret.range.start)?
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        self.session.line_id(index)
    }

    pub(super) fn navigate_dependency(
        &mut self,
        nav: super::calculation_dependency_dialog::DependencyNavigation,
        revision: u64,
        source_host: super::utility_host::HostIdentity,
        ctx: &egui::Context,
    ) -> bool {
        if self.session.revision() != Some(revision) || !self.can_navigate_source(nav, source_host) {
            return false;
        }
        self.dependency_navigation = Some(SourceNavigation {
            host: self.host_identity(),
            navigation: nav,
            revision,
            result_navigation: false,
            selection: None,
            focus_source: true,
        });
        let viewport = if source_host.host == PadHost::Popout {
            pad_viewport_id()
        } else {
            egui::ViewportId::ROOT
        };
        ctx.request_repaint_of(viewport);
        true
    }

    fn can_navigate_source(
        &self,
        nav: super::calculation_dependency_dialog::DependencyNavigation,
        source_host: PadHostIdentity,
    ) -> bool {
        !(self.host_identity() != source_host
            || !self.open
            || self.host_close_requested
            || self.open_chooser
            || self.save_as_open
            || self.session.pending_transition().is_some()
            || self
                .session
                .resolve_dependency_line(nav.document, nav.incarnation, nav.line)
                .is_none())
    }

    fn resolve_live_source(
        &self,
        navigation: super::calculation_dependency_dialog::DependencyNavigation,
    ) -> Option<crate::calculation_pad::DependencyLocation> {
        if self.session.current_id() != Some(navigation.document)
            || self.session.document_incarnation() != Some(navigation.incarnation)
        {
            return None;
        }
        let index = self.session.line_ids()?.iter().position(|line| *line == navigation.line)?;
        let source_range = SourcePositions::new(self.session.source()?).line_range(index + 1)?;
        Some(crate::calculation_pad::DependencyLocation { line_index: index, source_range })
    }

    fn can_navigate_live_source(
        &self,
        navigation: super::calculation_dependency_dialog::DependencyNavigation,
        source_host: PadHostIdentity,
    ) -> bool {
        self.host_identity() == source_host
            && self.open
            && !self.host_close_requested
            && !self.open_chooser
            && !self.save_as_open
            && self.session.pending_transition().is_none()
            && self.resolve_live_source(navigation).is_some()
    }

    pub(super) fn inspector_context(&self) -> std::sync::Arc<InspectorPadContext> {
        std::sync::Arc::clone(&self.inspector_context)
    }

    pub(super) fn apply_inspector_insert(
        &mut self,
        ctx: &egui::Context,
        context: &InspectorPadContext,
        request: crate::calculation_pad::InspectorInsertRequest,
    ) -> bool {
        self.sync_inspector_context();
        if self.inspector_context.as_ref() != context
            || context.candidate.as_ref() != Some(request.source())
            || context
                .caret
                .as_ref()
                .is_none_or(|caret| &caret.range != request.target())
        {
            return false;
        }
        if let Some(source) = self.session.source() {
            let regions = editor::fold_regions(source);
            let ids = self.session.line_ids().unwrap_or_default();
            let target = request.target();
            self.folds.collapsed.retain(|id| !regions.iter().any(|region| {
                ids.get(region.header) == Some(id)
                    && region.hidden.start <= target.end && target.start < region.hidden.end
            }));
        }
        let old_source = (!self.pad_view.inputs.is_empty()).then(|| self.session.source().unwrap_or_default().to_owned());
        let old_ids = old_source.as_ref().map(|_| self.session.line_ids().unwrap_or_default().to_vec());
        match self.session.apply_inspector_insert(request) {
            Ok(changed) => {
                if changed {
                    self.reconcile_input_selections(old_source.as_deref(), old_ids.as_deref(), false);
                    self.persist_view_state_in_session();
                    self.inspector_caret = None;
                    self.schedule_recovery_snapshot(ctx);
                    self.request_source_focus();
                }
                self.sync_inspector_context();
                changed
            }
            Err(error) => {
                self.error = Some(format!("Inspector insert refused: {error:?}"));
                false
            }
        }
    }

    fn apply_host_request(&mut self, ctx: &egui::Context, requested_host: PadHost) -> bool {
        if self.host_state.presentation() == requested_host {
            return false;
        }
        self.host_state.transition(requested_host);
        self.split_preview = None;
        self.dependency_navigation = None;
        self.source_highlight = None;
        super::utility_host::remember_presentation(ctx, calculation_pad_tool_key(), requested_host);
        self.deferred_source_focus = None;
        match requested_host {
            PadHost::Docked => {
                self.focus_source = true;
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Close);
            }
            PadHost::Popout => {
                self.focus_source = true;
            }
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        true
    }

    fn resolve_result_copy(
        &mut self,
        intent: ResultCopyIntent,
    ) -> Option<CalculationPadDialogEffect> {
        if self.session.current_id() != Some(intent.document_id) {
            return None;
        }
        let text = match intent.target {
            ResultCopyTarget::All => {
                let (source, evaluation) = self.session.evaluation_with_source()?;
                copy_all_results(source, evaluation)
            }
            ResultCopyTarget::Line(line_id) | ResultCopyTarget::FormattedLine(line_id) => self
                .session
                .evaluation()?
                .lines
                .iter()
                .find(|line| line.id == line_id)
                .and_then(|line| match &line.outcome {
                    LineOutcome::Value(value) => if matches!(intent.target, ResultCopyTarget::FormattedLine(_)) {
                        result_format::display(value, self.display_preferences)
                    } else { value.canonical_result() },
                    _ => None,
                })?,
        };
        Some(CalculationPadDialogEffect::CopyText(text))
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        editor_id: egui::Id,
        mut source: String,
        evaluation: Option<&crate::calculation_pad::SessionEvaluation>,
        authorized_pass: bool,
        staged_save_as_name: &mut String,
        source_focus_requested: &mut bool,
        edited_source: &mut Option<String>,
        intent: &mut Option<DialogIntent>,
        host_request: &mut Option<PadHost>,
        result_copy: &mut Option<ResultCopyIntent>,
        inspector_selection: &mut Option<LineId>,
        inspector_caret: &mut Option<std::ops::Range<usize>>,
        editor_primary_caret: &mut Option<usize>,
        open_inspector: &mut bool,
        open_dependencies: &mut bool,
        dependency_applied: &mut Option<DependencyEditorUpdate>,
        result_navigation: &mut Option<(super::calculation_dependency_dialog::DependencyNavigation, u64)>,
        selected_result: &mut Option<LineId>,
        result_scroll: &mut Option<ResultScrollUpdate>,
        display_preferences: &mut Option<crate::settings::CalculationPadDisplayPreferences>,
        display_selection: &mut Option<egui::text::CCursorRange>,
        completion: &mut CompletionState,
        completion_cursor: &mut Option<egui::text::CCursorRange>,
        folds: &mut FoldState,
        projection_state: &mut Option<egui::widgets::text_edit::TextEditState>,
        source_clipboard: &mut Option<String>,
        deferred_projected_input: &mut Option<Vec<egui::Event>>,
        split_action: &mut Option<SplitAction>,
        host_identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                *intent = Some(DialogIntent::New);
            }
            if ui.button("Open").clicked() {
                *intent = Some(DialogIntent::OpenChooser);
            }
            let save = ui.add_enabled(
                self.session.pending_transition().is_none(),
                egui::Button::new("Save"),
            );
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_save_rect = Some(save.rect);
            }
            if save.clicked() {
                *intent = Some(DialogIntent::Save);
            }
            if ui.button("Save As…").clicked() {
                *intent = Some(DialogIntent::OpenSaveAs);
            }
            let examples = ui.button("Examples & reference");
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_examples_button_rect = Some(examples.rect);
                self.test_examples_button_ownership = Some((ui.clip_rect(), examples.enabled()));
            }
            if examples.clicked() {
                *intent = Some(DialogIntent::OpenExamples);
            }
            let dependencies = ui.button("Dependencies…");
            #[cfg(test)] { self.test_dependency_open = Some((dependencies.rect, ui.clip_rect(), dependencies.enabled())); }
            if dependencies.clicked() { *open_dependencies = true; }
            let inspector = ui.button("Programmer Inspector…");
            #[cfg(test)]
            {
                self.test_inspector_reopen =
                    Some((inspector.rect, ui.clip_rect(), inspector.enabled()));
            }
            if inspector.clicked() {
                *open_inspector = true;
            }
            let (host_button, requested_host) =
                super::utility_host::presentation_button(ui, host_identity.host, true);
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_host_button_rect = Some(host_button.rect);
            }
            if host_button.clicked() {
                *host_request = Some(requested_host);
            }
            ui.separator();
            let mut remember_source = self.remember_source;
            if ui
                .checkbox(&mut remember_source, "Remember last source")
                .changed()
            {
                *intent = Some(DialogIntent::SetRememberSource(remember_source));
            }
            let title = self.session.current_name().unwrap_or("Untitled");
            ui.label(title);
            if self.session.is_dirty() {
                ui.colored_label(ui.visuals().warn_fg_color, "Unsaved changes");
            }
        });

        if let Some(error) = self.error.as_deref() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if let Some(id) = self.conflict_document
                    && self.session.pending_transition().is_none()
                    && ui.button("Reload saved version…").clicked()
                {
                    *intent = Some(DialogIntent::Open(id));
                }
                if self.conflict_document.is_some() {
                    ui.label("Save As keeps this draft; Keep Editing cancels the close request.");
                }
            });
        } else if let Some(notice) = self.notice.as_deref() {
            ui.label(notice);
        }

        if self.open_chooser {
            self.document_chooser_ui(ui, intent);
        }
        if self.save_as_open {
            Self::save_as_ui(ui, staged_save_as_name, intent);
        }

        if let Some(pending) = self.session.pending_transition() {
            self.pending_transition_ui(ui, pending, intent);
        }

        ui.separator();
        let available_height = ui.available_height().max(120.0);
        let available_width = ui.available_width();
        let orientation = if available_width >= 680.0 {
            SplitOrientation::Wide
        } else {
            SplitOrientation::Stacked
        };
        let preview = self.split_preview.filter(|preview| {
            preview.host == host_identity && preview.orientation == orientation
        });
        if self.split_preview.is_some() && preview.is_none() {
            *split_action = Some(SplitAction::Cancel);
        }
        let saved_ratio = match orientation {
            SplitOrientation::Wide => self.display_preferences.wide_source_ratio,
            SplitOrientation::Stacked => self.display_preferences.stacked_source_ratio,
        };
        let ratio = preview.map_or(saved_ratio, |preview| preview.ratio);
        if orientation == SplitOrientation::Wide {
            let (source_width, results_width, ratio) =
                split_dimensions(available_width, ratio, 220.0, 0.5);
            let child_item_spacing = ui.spacing().item_spacing.x;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let source_ui = ui.allocate_ui_with_layout(
                    egui::vec2(source_width, available_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = child_item_spacing;
                        ui.set_min_width(source_width);
                        self.editor_ui(
                            ui,
                            editor_id,
                            &mut source,
                            available_height,
                            false,
                            authorized_pass && intent.is_none() && host_request.is_none(),
                            source_focus_requested,
                            edited_source,
                            inspector_caret,
                            editor_primary_caret,
                            dependency_applied,
                            display_preferences,
                            display_selection,
                            completion,
                            completion_cursor,
                            folds,
                            projection_state,
                            source_clipboard,
                            deferred_projected_input,
                        );
                    },
                );
                let response = splitter(ui, orientation, source_ui.response.rect.height().max(80.0));
                #[cfg(test)]
                { self.test_splitter = Some((orientation, response.rect)); }
                self.stage_split_action(
                    ui, &response, orientation, ratio, available_width, 220.0, split_action,
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(results_width, available_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = child_item_spacing;
                        ui.set_min_width(results_width);
                        self.results_ui(
                            ui,
                            evaluation,
                            available_height,
                            result_copy,
                            host_identity,
                            viewport_id,
                            inspector_selection,
                            open_inspector,
                            result_navigation,
                            selected_result,
                            result_scroll,
                        );
                    },
                );
            });
        } else {
            let total_height = available_height.max(248.0);
            let (source_height, results_height, ratio) =
                split_dimensions(total_height, ratio, 120.0, 0.48);
            self.editor_ui(
                ui,
                editor_id,
                &mut source,
                (source_height - 36.0).max(70.0),
                true,
                authorized_pass && intent.is_none() && host_request.is_none(),
                source_focus_requested,
                edited_source,
                inspector_caret,
                editor_primary_caret,
                dependency_applied,
                display_preferences,
                display_selection,
                completion,
                completion_cursor,
                folds,
                projection_state,
                source_clipboard,
                deferred_projected_input,
            );
            let response = splitter(ui, orientation, available_width.max(0.0));
            #[cfg(test)]
            { self.test_splitter = Some((orientation, response.rect)); }
            self.stage_split_action(
                ui, &response, orientation, ratio, total_height, 120.0, split_action,
            );
            self.results_ui(
                ui,
                evaluation,
                (results_height - 36.0).max(70.0),
                result_copy,
                host_identity,
                viewport_id,
                inspector_selection,
                open_inspector,
                result_navigation,
                selected_result,
                result_scroll,
            );
        }
    }

    fn stage_split_action(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        orientation: SplitOrientation,
        ratio: f32,
        total: f32,
        minimum: f32,
        action: &mut Option<SplitAction>,
    ) {
        if action.is_some() {
            return;
        }
        let dragging = response.dragged_by(egui::PointerButton::Primary);
        let finished = response.drag_stopped_by(egui::PointerButton::Primary);
        if dragging || finished {
            let axis = |position: egui::Pos2| match orientation {
                SplitOrientation::Wide => position.x,
                SplitOrientation::Stacked => position.y,
            };
            let origin = ui.input(|input| {
                input.pointer.press_origin().or_else(|| input.events.iter().find_map(|event| {
                    match event {
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } => Some(*pos),
                        _ => None,
                    }
                }))
            });
            let pointer = ui.input(|input| input.pointer.interact_pos());
            let preview = self.split_preview.filter(|preview| {
                preview.host == self.host_identity() && preview.orientation == orientation
            }).or_else(|| origin.map(|position| SplitPreview {
                host: self.host_identity(), orientation, ratio, start_ratio: ratio,
                press_position: axis(position),
            }));
            if let (Some(mut preview), Some(pointer)) = (preview, pointer) {
                let scaling = ui.ctx().layer_transform_from_global(response.layer_id).map_or(1.0, |transform| transform.scaling);
                let content = (total - SPLITTER_SIZE).max(minimum * 2.0);
                let delta = (axis(pointer) - preview.press_position) * scaling;
                let (_, _, next) = split_dimensions(total, preview.start_ratio + delta / content, minimum, ratio);
                preview.ratio = next;
                *action = Some(if finished { SplitAction::Finish(preview) } else { SplitAction::Preview(preview) });
            }
        } else if self.split_preview.is_some() && !ui.input(|input| input.pointer.primary_down()) {
            *action = Some(SplitAction::Cancel);
        }
    }

    fn examples_ui(&mut self, ui: &mut egui::Ui, effects: &mut HelpEffects) {
        let mut query = self.examples_search.clone();
        #[cfg(test)]
        let mut controls = (
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            let width = (ui.available_width() - 170.0).max(80.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .desired_width(width)
                    .hint_text("Function, keyword, or example"),
            );
            let clear = ui.button("Clear");
            if clear.clicked() {
                query.clear();
            }
            let (button, requested) = super::utility_host::presentation_button(
                ui,
                self.examples_host.presentation(),
                true,
            );
            #[cfg(test)]
            {
                controls = (search.rect, clear.rect, button.rect);
            }
            #[cfg(not(test))]
            let _ = search;
            if button.clicked() {
                effects.host_request = Some(requested);
            }
        });
        if query != self.examples_search {
            effects.search = Some(query.clone());
        }
        ui.separator();
        let active = !query.trim().is_empty();
        let scroll = egui::ScrollArea::vertical().id_salt("calculation-pad-reference-content")
            .vertical_scroll_offset(self.examples_scroll).max_height(ui.available_height())
            .show(ui, |ui| {
                if !active {
                    ui.label("One expression or name = expression per physical line; # or // starts a full-line comment.");
                    ui.label("Results include scalars, vectors, exact integers, quantities, and exact base conversions.");
                    ui.collapsing("Operator and function reference", |ui| {
                        ui.label("Functions are case-insensitive in call position. Unbound pi and pie mean PI. Infix mod matches signed %, while mod(a, b) is Euclidean.");
                        ui.label("Functions: abs, signum, sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh, floor, round, ceil, trunc, fract, exp, sqrt, cbrt, ln, log, log2, log10, atan2, min, max.");
                        ui.label("Vectors contain 1–128 finite scalar components; +, -, *, / are element-wise, dot/· returns a scalar, and cross/× requires three components.");
                    });
                    ui.collapsing("Exact integers, units, and limits", |ui| {
                        ui.label("Integer constructors and programmer operators use checked exact values and widths from 8 to 128 bits; Inspect bits opens the separate Inspector.");
                        ui.label("Use +, -, *, ^, idiv, rem and % for checked integer arithmetic; &, |, ~, and/or/xor/not and shifts operate on bits. Shift counts must fit the width; rotl, rotr, popcount, mask and extract use the selected mode.");
                        ui.label("Checked cast validates a value; Reinterpret bits preserves the low bits. Inspector changes remain private until Insert, and Undo restores the Pad source.");
                        ui.label("Quantities preserve dimensions and support compatible arithmetic and conversions. Names and rN references use earlier physical source lines.");
                        ui.label("Compatible units can be added, scaled, multiplied, divided and converted using 'to'. Temperature and fuel-economy units remain conversion-only; month/year conversions may be approximate.");
                        ui.label("Braced names may contain spaces and Unicode. rN counts physical source lines, including comments and blanks; forward or failed references are diagnosed.");
                        ui.label("Scalar integer literals are bounded to 2^53. Expressions are limited to 2,048 bytes, nesting to 32, documents to 64 KiB and 256 lines, and identifiers to 64 per line and 512 per document.");
                        ui.label("Quantity expressions are limited to 256 tokens and unit powers from -8 through 8. Finite operands and intermediate values are required.");
                    });
                    ui.separator();
                }
                let mut matches = 0;
                for (group_index, group) in EXAMPLE_GROUPS.iter().enumerate() {
                    let examples = group.examples.iter().filter(|example| example.matches(group.title, &query)).collect::<Vec<_>>();
                    if examples.is_empty() { continue; }
                    matches += examples.len();
                    let mut body = |ui: &mut egui::Ui| {
                        for example in &examples {
                            #[cfg(test)] self.test_examples_rendered_sources.push(example.source);
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.strong(example.title);
                                    let copy = ui.small_button("Copy example");
                                    #[cfg(test)] if self.test_examples_copy_rect.is_none() {
                                        self.test_examples_copy_rect = Some(copy.rect);
                                        self.test_examples_copy_state = Some((copy.enabled(), ui.clip_rect()));
                                    }
                                    if copy.clicked() { effects.copy = Some(example.source.to_owned()); }
                                });
                                ui.code(example.source);
                                ui.label(format!("→ {}", example.result_text()));
                                ui.weak(example.explanation);
                            });
                        }
                    };
                    if active { ui.strong(group.title); body(ui); }
                    else {
                        let header = egui::CollapsingHeader::new(group.title).id_salt(("calculation-pad-example-group", group_index)).default_open(true).show(ui, &mut body);
                        #[cfg(test)] if self.test_examples_group_rect.is_none() { self.test_examples_group_rect = Some(header.header_response.rect); }
                        #[cfg(not(test))] let _ = header;
                    }
                }
                if matches == 0 {
                    ui.label("No matching examples.");
                    if ui.button("Clear search").clicked() { effects.search = Some(String::new()); }
                }
            });
        effects.scroll = Some(scroll.state.offset.y);
        #[cfg(test)]
        {
            self.test_examples_controls =
                Some((controls.0, controls.1, controls.2, scroll.inner_rect));
        }
    }

    fn fold_projection(&self, source: &str, folds: &FoldState) -> (Vec<FoldRegion>, Projection) {
        let regions = editor::fold_regions(source);
        let ids = self.session.line_ids().unwrap_or_default();
        let hidden = regions.iter().filter_map(|region| {
            ids.get(region.header)
                .filter(|id| folds.collapsed.contains(id))
                .map(|_| region.hidden.clone())
        }).collect::<Vec<_>>();
        let projection = Projection::new(source, &hidden);
        (regions, projection)
    }

    fn editor_ui(
        &mut self,
        ui: &mut egui::Ui,
        editor_id: egui::Id,
        source: &mut String,
        max_height: f32,
        fill_height: bool,
        authorized_pass: bool,
        source_focus_requested: &mut bool,
        edited_source: &mut Option<String>,
        inspector_caret: &mut Option<std::ops::Range<usize>>,
        editor_primary_caret: &mut Option<usize>,
        dependency_applied: &mut Option<DependencyEditorUpdate>,
        display_preferences: &mut Option<crate::settings::CalculationPadDisplayPreferences>,
        display_selection: &mut Option<egui::text::CCursorRange>,
        completion: &mut CompletionState,
        completion_cursor: &mut Option<egui::text::CCursorRange>,
        folds: &mut FoldState,
        projection_state: &mut Option<egui::widgets::text_edit::TextEditState>,
        source_clipboard: &mut Option<String>,
        deferred_projected_input: &mut Option<Vec<egui::Event>>,
    ) {
        let (regions, mut projection) = self.fold_projection(source, &self.folds);
        // Only an actual selection gesture changes the caret; passive hover does not.
        let editor_rect = ui.ctx().read_response(editor_id).map(|response| response.rect);
        let dragging_editor = ui.ctx().is_being_dragged(editor_id);
        let pointer_gesture = ui.input(|input| input.events.iter().any(|event| match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, .. } =>
                dragging_editor || editor_rect.is_some_and(|rect| rect.contains(*pos)),
            egui::Event::PointerMoved(_) => dragging_editor,
            _ => false,
        }));
        // Retain pointer placement on the projection the user saw, then replay
        // only editor input against the resulting canonical caret.
        if authorized_pass && !self.folds.collapsed.is_empty() && pointer_gesture {
            let deferred = ui.input_mut(|input| {
                let has_edit = input.events.iter().any(|event| matches!(event,
                    egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_)
                    | egui::Event::Key { key: egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter | egui::Key::Tab, pressed: true, .. }));
                if !has_edit { return Vec::new(); }
                let mut deferred = Vec::new();
                input.events.retain(|event| {
                    let editor_event = matches!(event,
                        egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_)
                        | egui::Event::Key { key: egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter | egui::Key::Tab
                            | egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::ArrowUp | egui::Key::ArrowDown
                            | egui::Key::Home | egui::Key::End | egui::Key::A | egui::Key::Z, .. });
                    if editor_event { deferred.push(event.clone()); }
                    !editor_event
                });
                deferred
            });
            if !deferred.is_empty() { *deferred_projected_input = Some(deferred); }
        }
        let clipboard_requested = ui.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Copy | egui::Event::Cut)));
        let original_state = egui::widgets::text_edit::TextEditState::load(ui.ctx(), editor_id);
        let persistent_selection = original_state.as_ref().and_then(|state| state.cursor.char_range());
        let mut editor_selection = ui.memory(|memory| memory.has_focus(editor_id)).then(|| {
            original_state.clone()
                .and_then(|state| state.cursor.char_range())
        }).flatten();
        let mut expanded = false;
        if authorized_pass && !self.folds.collapsed.is_empty()
            && let Some(selection) = editor_selection
            && let (Some(primary), Some(secondary)) = (
                projection.byte_at_character(selection.primary.index.0),
                projection.byte_at_character(selection.secondary.index.0),
            )
        {
            let low = primary.min(secondary);
            let high = primary.max(secondary);
            let events = ui.input(|input| input.events.clone());
            let select_all = events.iter().any(|event| matches!(event,
                egui::Event::Key { key: egui::Key::A, pressed: true, modifiers, .. } if modifiers.command));
            let selection_has_newline = projection.visible.get(low..high).is_some_and(|text| text.contains('\n'));
            let changes_lines = events.iter().any(|event| match event {
                egui::Event::Paste(text) | egui::Event::Text(text) => text.contains(['\n', '\r']),
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => true,
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } =>
                    selection_has_newline || projection.visible.get(..low).is_some_and(|text| text.ends_with('\n')),
                egui::Event::Key { key: egui::Key::Delete, pressed: true, .. } =>
                    selection_has_newline || projection.visible.get(high..).is_some_and(|text| text.starts_with('\n')),
                egui::Event::Cut => selection_has_newline,
                _ => false,
            });
            let modifying = events.iter().any(|event| matches!(event,
                egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_)
                | egui::Event::Key { key: egui::Key::Backspace | egui::Key::Delete | egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::Enter | egui::Key::Tab, pressed: true, .. }));
            let pointer_and_edit = modifying && pointer_gesture;
            let edit_or_copy = events.iter().any(|event| matches!(event,
                egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_)
                | egui::Event::Key { key: egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter, pressed: true, .. }));
            let mixed_navigation = edit_or_copy && events.iter().any(|event| matches!(event,
                egui::Event::Key { key: egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::ArrowUp | egui::Key::ArrowDown | egui::Key::Home | egui::Key::End, pressed: true, .. }));
            let gaps = projection.gaps.iter().filter(|(at, _)| {
                select_all || changes_lines || pointer_and_edit || mixed_navigation || (modifying && low <= *at && *at <= high)
            }).map(|(_, range)| range.clone()).collect::<Vec<_>>();
            if !gaps.is_empty() {
                let ids = self.session.line_ids().unwrap_or_default();
                folds.collapsed.retain(|id| !regions.iter().any(|region| {
                    ids.get(region.header) == Some(id)
                        && gaps.iter().any(|gap| region.hidden.start < gap.end && gap.start < region.hidden.end)
                }));
                let next = self.fold_projection(source, folds).1;
                if let Some(mut state) = original_state.clone()
                    && let (Some(a), Some(b)) = (
                        projection.source_character(source, selection.primary.index.0),
                        projection.source_character(source, selection.secondary.index.0),
                    )
                    && let (Some(a), Some(b)) = (
                        next.visible_character_clamped(source, a),
                        next.visible_character_clamped(source, b),
                    )
                {
                    let mut mapped = selection;
                    mapped.primary.index = a.into();
                    mapped.secondary.index = b.into();
                    state.cursor.set_char_range(Some(mapped));
                    state.store(ui.ctx(), editor_id);
                    editor_selection = Some(mapped);
                    projection = next;
                    expanded = true;
                }
            }
        }
        if authorized_pass && editor_selection.is_none() && !self.folds.collapsed.is_empty() {
            let edit = ui.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_)
                    | egui::Event::Key { key: egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter, pressed: true, .. })));
            if pointer_gesture && edit {
                folds.collapsed.clear();
                let next = self.fold_projection(source, folds).1;
                if let (Some(selection), Some(mut state)) = (persistent_selection, original_state.clone())
                    && let (Some(a), Some(b)) = (
                        projection.source_character(source, selection.primary.index.0),
                        projection.source_character(source, selection.secondary.index.0),
                    )
                    && let (Some(a), Some(b)) = (
                        next.visible_character_clamped(source, a),
                        next.visible_character_clamped(source, b),
                    )
                {
                    let mut mapped = selection;
                    mapped.primary.index = a.into();
                    mapped.secondary.index = b.into();
                    state.cursor.set_char_range(Some(mapped));
                    state.store(ui.ctx(), editor_id);
                    editor_selection = Some(mapped);
                }
                projection = next;
                expanded = true;
            }
        }
        if authorized_pass
            && let Some(pending) = self.dependency_navigation
            && pending.host == self.host_identity()
            && self.session.revision() == Some(pending.revision)
            && let Some(location) = self.resolve_live_source(pending.navigation)
        {
            let target = pending.selection.map_or(location.source_range.start, |selection| selection.0);
            let ids = self.session.line_ids().unwrap_or_default();
            let before = folds.collapsed.len();
            folds.collapsed.retain(|id| !regions.iter().any(|region| {
                ids.get(region.header) == Some(id)
                    && region.hidden.start <= target && target < region.hidden.end
            }));
            if folds.collapsed.len() != before {
                let next = self.fold_projection(source, folds).1;
                if let (Some(selection), Some(mut state)) = (editor_selection, original_state.clone())
                    && let (Some(a), Some(b)) = (
                        projection.source_character(source, selection.primary.index.0),
                        projection.source_character(source, selection.secondary.index.0),
                    )
                    && let (Some(a), Some(b)) = (
                        next.visible_character_clamped(source, a),
                        next.visible_character_clamped(source, b),
                    )
                {
                    let mut mapped = selection;
                    mapped.primary.index = a.into();
                    mapped.secondary.index = b.into();
                    state.cursor.set_char_range(Some(mapped));
                    state.store(ui.ctx(), editor_id);
                    editor_selection = Some(mapped);
                }
                projection = next;
                expanded = true;
            }
        }
        let mut visible_source = projection.visible.clone();
        ui.horizontal_wrapped(|ui| {
            ui.heading("Source");
            let mut preferences = display_preferences.unwrap_or(self.display_preferences);
            let numbers = ui.checkbox(&mut preferences.line_numbers, "Line numbers");
            let wrap = ui.checkbox(&mut preferences.word_wrap, "Word wrap");
            #[cfg(test)]
            { self.test_display_controls = Some((numbers.rect, wrap.rect)); }
            let mut assistance_changed = false;
            ui.menu_button("Editor options", |ui| {
                assistance_changed |= ui.checkbox(&mut preferences.completion, "Completions").changed();
                assistance_changed |= ui.checkbox(&mut preferences.parameter_hints, "Parameter hints").changed();
                assistance_changed |= ui.checkbox(&mut preferences.auto_close_delimiters, "Auto-close delimiters").changed();
                if !regions.is_empty() {
                    ui.separator();
                    for region in &regions {
                        if let Some(id) = self.session.line_id(region.header) {
                            let mut closed = folds.collapsed.contains(&id);
                            let text = source.lines().nth(region.header).unwrap_or("").trim();
                            if ui.checkbox(&mut closed, format!("Fold line {}: {text}", region.header + 1)).changed() {
                                folds.collapsed.retain(|existing| *existing != id);
                                let selection_overlaps = editor_selection.or(persistent_selection).and_then(|selection| {
                                    let a = projection.byte_at_character(selection.primary.index.0).and_then(|byte| projection.source_byte(byte))?;
                                    let b = projection.byte_at_character(selection.secondary.index.0).and_then(|byte| projection.source_byte(byte))?;
                                    Some(a.min(b) < region.hidden.end && a.max(b) >= region.hidden.start)
                                }).unwrap_or(false);
                                if closed && !selection_overlaps { folds.collapsed.push(id); }
                            }
                        }
                    }
                }
            });
            if numbers.changed() || wrap.changed() || assistance_changed {
                *display_preferences = Some(preferences);
            }
        });
        let preferences = display_preferences.unwrap_or(self.display_preferences);
        let focused = authorized_pass && ui.memory(|memory| memory.has_focus(editor_id));
        let caret = editor_selection.and_then(|range| projection.source_character(source, range.primary.index.0))
            .and_then(|index| source.char_indices().map(|(offset, _)| offset)
            .chain(std::iter::once(source.len())).nth(index));
        let revision = self.session.revision().unwrap_or_default();
        let candidates = if focused && preferences.completion && !self.editor_tools.open {
            caret.map_or_else(Vec::new, |caret| editor::suggestions(source, caret))
        } else { Vec::new() };
        completion.visible = !candidates.is_empty()
            && caret.is_some_and(|caret| completion.dismissed != Some((revision, caret)));
        if completion.selected >= candidates.len() { completion.selected = 0; }
        let mut accept = false;
        let conflicting_edit = ui.input(|input| input.events.iter().any(|event| match event {
            egui::Event::Text(text) => !text.is_empty(),
            egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Ime(_) => true,
            egui::Event::Key { key, pressed: true, modifiers, .. } =>
                (*key == egui::Key::Z && modifiers.command) || matches!(key, egui::Key::Backspace | egui::Key::Delete),
            _ => false,
        }));
        if completion.visible && !conflicting_edit {
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                completion.visible = false;
                completion.dismissed = caret.map(|caret| (revision, caret));
            } else if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                completion.selected = (completion.selected + 1) % candidates.len();
            } else if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                completion.selected = (completion.selected + candidates.len() - 1) % candidates.len();
            } else {
                accept = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
        }
        let mut smart_cursor = None;
        let mut smart_source_cursor = None;
        if authorized_pass && focused && preferences.auto_close_delimiters && !completion.visible
            && let Some(selection) = editor_selection
            && let (Some(primary), Some(secondary)) = (
                projection.byte_at_character(selection.primary.index.0).and_then(|byte| projection.source_byte(byte)),
                projection.byte_at_character(selection.secondary.index.0).and_then(|byte| projection.source_byte(byte)),
            )
        {
            let range = primary.min(secondary)..primary.max(secondary);
            let typed = ui.input(|input| {
                if pointer_gesture || input.events.iter().any(|event| match event {
                    egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_) => true,
                    egui::Event::Text(text) => text.chars().count() != 1,
                    egui::Event::Key { key, pressed: true, modifiers, .. } => modifiers.command
                        || matches!(key, egui::Key::Backspace | egui::Key::Delete | egui::Key::ArrowLeft
                            | egui::Key::ArrowRight | egui::Key::ArrowUp | egui::Key::ArrowDown
                            | egui::Key::Home | egui::Key::End | egui::Key::Enter | egui::Key::Tab),
                    _ => false,
                }) {
                    return None;
                }
                let mut text = input.events.iter().filter_map(|event| match event {
                    egui::Event::Text(text) if text.chars().count() == 1 => text.chars().next(),
                    _ => None,
                });
                let character = text.next()?;
                text.next().is_none().then_some(character)
            });
            if let Some(character) = typed {
                let action = if matches!(character, '(' | '[') {
                    editor::smart_edit(source, range.clone(), character).map(|(updated, cursor)| (Some(updated), cursor))
                } else if range.is_empty() && matches!(character, ')' | ']')
                    && source[range.start..].starts_with(character)
                {
                    Some((None, range.start + character.len_utf8()))
                } else { None };
                if let Some((updated, cursor)) = action {
                    ui.input_mut(|input| input.events.retain(|event| !matches!(event, egui::Event::Text(text) if text.chars().eq(std::iter::once(character)))));
                    if let Some(updated) = updated {
                        *source = updated;
                        *edited_source = Some(source.clone());
                    }
                    projection = self.fold_projection(source, folds).1;
                    visible_source = projection.visible.clone();
                    smart_cursor = SourcePositions::new(source).character(cursor)
                        .and_then(|index| projection.visible_character_clamped(source, index));
                    smart_source_cursor = Some(cursor);
                }
            }
        }
        if authorized_pass && focused && !completion.visible && !accept && smart_cursor.is_none()
            && let Some(selection) = editor_selection
            && let (Some(primary), Some(secondary)) = (
                projection.byte_at_character(selection.primary.index.0).and_then(|byte| projection.source_byte(byte)),
                projection.byte_at_character(selection.secondary.index.0).and_then(|byte| projection.source_byte(byte)),
            )
        {
            let range = primary.min(secondary)..primary.max(secondary);
            let line_start = source[..range.start].rfind('\n').map_or(0, |offset| offset + 1);
            let line = &source[line_start..range.start];
            let indent = line.len() - line.trim_start_matches(' ').len();
            let add_level = crate::calculation_pad::editor_loop_header(line.trim());
            if (indent > 0 || add_level)
                && !pointer_gesture
                && !ui.input(|input| input.events.iter().any(|event| match event {
                    egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Copy | egui::Event::Ime(_) | egui::Event::Text(_) => true,
                    egui::Event::Key { key, pressed: true, modifiers, .. } =>
                        *key != egui::Key::Enter || *modifiers != egui::Modifiers::NONE,
                    _ => false,
                }))
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
            {
                let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
                let insertion = format!("{newline}{}", " ".repeat(indent + usize::from(add_level) * 4));
                let mut updated = source.clone();
                updated.replace_range(range.clone(), &insertion);
                let cursor = range.start + insertion.len();
                if updated.len() > crate::calculation_pad::MAX_DOCUMENT_BYTES
                    || updated.bytes().filter(|byte| *byte == b'\n').count() + 1 > crate::calculation_pad::MAX_SOURCE_LINES
                {
                    *edited_source = Some(updated);
                } else {
                    *source = updated;
                    *edited_source = Some(source.clone());
                    projection = self.fold_projection(source, folds).1;
                    visible_source = projection.visible.clone();
                    smart_cursor = SourcePositions::new(source).character(cursor)
                        .and_then(|index| projection.visible_character_clamped(source, index));
                    smart_source_cursor = Some(cursor);
                }
            }
        }
        let navigation = self.dependency_navigation.and_then(|pending| {
            let nav = pending.navigation;
            (authorized_pass
                && pending.host == self.host_identity()
                && self.session.revision() == Some(pending.revision)
                && self.open
                && !self.open_chooser
                && !self.save_as_open
                && self.session.pending_transition().is_none())
            .then(|| {
                (if pending.selection.is_some() {
                    self.resolve_live_source(nav)
                } else {
                    self.session.resolve_dependency_line(nav.document, nav.incarnation, nav.line)
                })
                    .map(|location| (nav, location, pending.result_navigation, pending.selection, pending.focus_source))
            })
            .flatten()
        });
        let reserved = if completion.visible { 72.0 } else { 0.0 }
            + if preferences.parameter_hints && focused && caret.and_then(|at| editor::call_hint(source, at)).is_some() { 22.0 } else { 0.0 };
        let max_height = (max_height - reserved).max(35.0);
        let scroll = ui.add_enabled_ui(self.session.pending_transition().is_none(), |ui| {
            ui.horizontal_top(|ui| {
                let font = egui::TextStyle::Monospace.resolve(ui.style());
                let gutter = if self.display_preferences.line_numbers {
                    let digits = source.bytes().filter(|byte| *byte == b'\n').count() + 1;
                    let width = ui.fonts_mut(|fonts| fonts.layout_no_wrap(
                        digits.to_string(), font.clone(), ui.visuals().weak_text_color(),
                    ).size().x) + 8.0;
                    Some(ui.allocate_exact_size(egui::vec2(width, 0.0), egui::Sense::hover()).0)
                } else {
                    None
                };
                let viewport_width = ui.available_width().max(24.0);
                let word_wrap = self.display_preferences.word_wrap;
                let mut area = egui::ScrollArea::both()
                    .id_salt("calculation-pad-editor-scroll")
                    .max_width(viewport_width)
                    .max_height(max_height)
                    .auto_shrink([false, !fill_height]);
                if word_wrap {
                    area = area.horizontal_scroll_offset(0.0);
                }
                let scroll = area.show(ui, |ui| {
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                        let text = text.as_str();
                        let spans = ui.ctx().data_mut(|data| {
                            let id = editor_id.with("syntax-cache");
                            let mut cache = data.get_temp::<SyntaxCache>(id).unwrap_or_default();
                            if cache.source != text {
                                cache.source = text.to_owned();
                                cache.spans = editor::syntax_spans(text);
                                data.insert_temp(id, cache.clone());
                            }
                            cache.spans
                        });
                        let mut job = egui::text::LayoutJob::default();
                        job.wrap.max_width = if word_wrap { width } else { f32::INFINITY };
                        job.keep_trailing_whitespace = true;
                        let row_height = ui.fonts_mut(|fonts| fonts.row_height(&font))
                            + ui.spacing().extra_text_line_spacing;
                        for span in spans {
                            let color = match span.kind {
                                SyntaxKind::Comment => ui.visuals().weak_text_color(),
                                SyntaxKind::Number => ui.visuals().hyperlink_color,
                                SyntaxKind::Keyword | SyntaxKind::Constant => ui.visuals().warn_fg_color,
                                SyntaxKind::Operator => ui.visuals().weak_text_color(),
                                SyntaxKind::Call => ui.visuals().strong_text_color(),
                                _ => ui.visuals().text_color(),
                            };
                            job.append(&text[span.range], 0.0, egui::TextFormat {
                                font_id: font.clone(), color, line_height: Some(row_height), ..Default::default()
                            });
                        }
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };
                    let output_start = ui.ctx().output(|output| output.commands.len());
                    let edit = egui::TextEdit::multiline(&mut visible_source)
                        .id(editor_id)
                        .desired_rows(12)
                        .desired_width(viewport_width)
                        .font(egui::TextStyle::Monospace)
                        .layouter(&mut layouter)
                        .hint_text("total = 2 + 3\nr1 * 4")
                        .show(ui);
                    ui.ctx().output_mut(|output| {
                        let mut index = output_start.min(output.commands.len());
                        while index < output.commands.len() {
                            if matches!(output.commands[index], egui::OutputCommand::CopyText(_)) {
                                if let egui::OutputCommand::CopyText(text) = output.commands.remove(index) {
                                    *source_clipboard = Some(text);
                                }
                            } else { index += 1; }
                        }
                    });
                    edit
                });
                if let Some(gutter) = gutter {
                    let clip = egui::Rect::from_min_max(
                        egui::pos2(gutter.left(), scroll.inner_rect.top()),
                        egui::pos2(gutter.right(), scroll.inner_rect.bottom()),
                    ).intersect(ui.clip_rect());
                    let painter = ui.painter().with_clip_rect(clip);
                    for (visible_line, y) in physical_line_positions(&scroll.inner.galley, scroll.inner.galley_pos.y) {
                        let line_number = projection.visible_lines.get(visible_line - 1).copied().unwrap_or(visible_line - 1) + 1;
                        painter.text(
                            egui::pos2(gutter.right() - 4.0, y),
                            egui::Align2::RIGHT_TOP, line_number.to_string(), font.clone(),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                scroll
            }).inner
        }).inner;
        if scroll.inner.response.changed() {
            if visible_source.len() > crate::calculation_pad::MAX_DOCUMENT_BYTES {
                *edited_source = Some(visible_source.clone());
            } else if let Some(updated) = projection.apply_edit(source, &visible_source) {
                if updated.len() > crate::calculation_pad::MAX_DOCUMENT_BYTES
                    || updated.bytes().filter(|byte| *byte == b'\n').count() + 1 > crate::calculation_pad::MAX_SOURCE_LINES
                {
                    *edited_source = Some(updated);
                } else {
                    *source = updated;
                    *edited_source = Some(source.clone());
                    projection = self.fold_projection(source, folds).1;
                }
            }
        }
        #[cfg(test)]
        self.test_highlight_rows.clear();
        if let Some(highlight) = self.source_highlight
            && highlight.host == self.host_identity()
            && self.session.revision() == Some(highlight.revision)
            && !scroll.inner.response.changed()
            && ui.input(|input| input.time) < highlight.expires_at
            && let Some(location) = self.session.resolve_dependency_line(
                highlight.navigation.document, highlight.navigation.incarnation, highlight.navigation.line,
            )
        {
            let remaining = highlight.expires_at - ui.input(|input| input.time);
            ui.ctx().request_repaint_after_for(
                std::time::Duration::from_secs_f64(remaining), ui.ctx().viewport_id(),
            );
            let clip = scroll.inner_rect.intersect(scroll.inner.text_clip_rect);
            let painter = ui.painter().with_clip_rect(clip);
            let mut physical_line = 0;
            for row in &scroll.inner.galley.rows {
                if projection.visible_lines.get(physical_line) == Some(&location.line_index) {
                    let top = scroll.inner.galley_pos.y + row.pos.y;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(clip.left(), top),
                        egui::pos2(clip.right(), top + row.size.y),
                    );
                    #[cfg(test)]
                    if rect.intersects(clip) { self.test_highlight_rows.push(rect.intersect(clip)); }
                    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(255, 210, 70, 55));
                }
                if row.ends_with_newline { physical_line += 1; }
            }
        }
        if let Some((incarnation, document, revision, line)) = self.selected_result
            && self.session.revision() == Some(revision)
            && let Some(location) = self.session.resolve_dependency_line(document, incarnation, line)
            && !scroll.inner.response.changed()
        {
            let clip = scroll.inner_rect.intersect(scroll.inner.text_clip_rect);
            let painter = ui.painter().with_clip_rect(clip);
            let mut physical_line = 0;
            for row in &scroll.inner.galley.rows {
                if projection.visible_lines.get(physical_line) == Some(&location.line_index) {
                    let top = scroll.inner.galley_pos.y + row.pos.y;
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(clip.left(), top),
                        egui::pos2(clip.right(), top + row.size.y),
                    );
                    painter.rect_filled(rect, 0.0, ui.visuals().selection.bg_fill.gamma_multiply(0.20));
                }
                if row.ends_with_newline { physical_line += 1; }
            }
        }
        if self.editor_tools.open && !self.editor_tools.goto
            && !scroll.inner.response.changed()
            && let Some(range) = self.session.revision().and_then(|revision| self.editor_tools.current_match(revision))
            && source.get(range.clone()).is_some()
            && let (Some(start), Some(end)) = (
                SourcePositions::new(source).character(range.start).and_then(|char| projection.visible_character(source, char)),
                SourcePositions::new(source).character(range.end).and_then(|char| projection.visible_character(source, char)),
            )
        {
            let galley = &scroll.inner.galley;
            let start = galley.pos_from_cursor(egui::text::CCursor::new(start));
            let end = galley.pos_from_cursor(egui::text::CCursor::new(end));
            let clip = scroll.inner_rect.intersect(scroll.inner.text_clip_rect);
            let painter = ui.painter().with_clip_rect(clip);
            for row in &galley.rows {
                let y = row.pos.y;
                if y + row.size.y < start.min.y || y > end.max.y { continue; }
                let left = if (y - start.min.y).abs() < 1.0 { start.min.x } else { row.pos.x };
                let right = if (y - end.min.y).abs() < 1.0 { end.min.x } else { row.pos.x + row.size.x };
                if right > left {
                    let rect = egui::Rect::from_min_max(
                        scroll.inner.galley_pos + egui::vec2(left, y),
                        scroll.inner.galley_pos + egui::vec2(right, y + row.size.y),
                    );
                    painter.rect_filled(rect, 1.0, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
                }
            }
        }
        if focused && !scroll.inner.response.changed()
            && let Some(caret) = caret
            && let Some((left, right)) = editor::matching_delimiters(source, caret)
        {
            let painter = ui.painter().with_clip_rect(scroll.inner_rect.intersect(scroll.inner.text_clip_rect));
            for byte in [left, right] {
                if let Some(index) = SourcePositions::new(source).character(byte)
                    .and_then(|index| projection.visible_character(source, index))
                {
                    let start = scroll.inner.galley.pos_from_cursor(egui::text::CCursor::new(index));
                    let end = scroll.inner.galley.pos_from_cursor(egui::text::CCursor::new(index + 1));
                    let rect = egui::Rect::from_min_max(
                        scroll.inner.galley_pos + start.min.to_vec2(),
                        scroll.inner.galley_pos + egui::vec2(end.min.x.max(start.min.x + 3.0), start.max.y),
                    );
                    painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                }
            }
        }
        if let Some((nav, location, result_navigation, selection, focus_source)) = navigation
            && !scroll.inner.response.changed()
            && let Some(revision) = self.session.revision()
        {
            let selected = selection.unwrap_or((location.source_range.start, location.source_range.end));
            let positions = SourcePositions::new(source);
            let (Some(start), Some(end)) = (
                positions.character(selected.0).and_then(|char| projection.visible_character(source, char)),
                positions.character(selected.1).and_then(|char| projection.visible_character(source, char)),
            ) else { return; };
            let mut state = scroll.inner.state.clone();
            let range = if result_navigation && selection.is_none() {
                egui::text::CCursorRange::one(egui::text::CCursor::new(start))
            } else {
                egui::text::CCursorRange::two(egui::text::CCursor::new(start), egui::text::CCursor::new(end))
            };
            state.cursor.set_char_range(Some(range));
            let target = scroll
                .inner
                .galley
                .pos_from_cursor(egui::text::CCursor::new(start))
                .translate(scroll.inner.galley_pos.to_vec2());
            let mut scroll_state = scroll.state;
            scroll_state.offset.y =
                (scroll_state.offset.y + target.center().y - scroll.inner_rect.center().y).max(0.0);
            scroll_state.offset.x =
                (scroll_state.offset.x + target.left() - scroll.inner_rect.left()).max(0.0);
            *dependency_applied = Some(DependencyEditorUpdate {
                navigation: nav,
                revision,
                editor_id,
                editor_state: state,
                scroll_id: scroll.id,
                scroll_state,
                result_navigation,
                source_byte_offset: location.source_range.start,
                selection,
                focus_source,
            });
        }
        #[cfg(test)]
        {
            self.test_editor_scroll_id = Some(scroll.id);
            let rows = physical_line_positions(&scroll.inner.galley, scroll.inner.galley_pos.y).collect();
            self.test_editor_geometry = Some((scroll.inner.galley.size().x, scroll.inner_rect.width(), scroll.content_size.x, rows));
        }
        let response = &scroll.inner.response;
        let cursor_interaction = response.clicked()
            || response.dragged()
            || response.changed()
            || ui.input(|input| {
                input.events.iter().any(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        matches!(
                            key,
                            egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                                | egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::Home
                                | egui::Key::End
                        ) || (*key == egui::Key::A && modifiers.command)
                    }
                    _ => false,
                })
            });
        if authorized_pass && response.has_focus() && cursor_interaction {
            if let Some(range) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), editor_id)
                .and_then(|state| state.cursor.char_range())
            {
                let to_byte = |index: usize| projection.byte_at_character(index)
                    .and_then(|byte| projection.source_byte(byte));
                if let (Some(a), Some(b)) = (
                    to_byte(range.primary.index.into()),
                    to_byte(range.secondary.index.into()),
                ) {
                    *inspector_caret = Some(a.min(b)..a.max(b));
                    *editor_primary_caret = Some(a);
                }
            }
        }
        #[cfg(test)]
        {
            self.test_editor_id = Some(editor_id);
            self.test_editor_rect = Some(response.rect);
            self.test_editor_clip = Some(scroll.inner_rect.intersect(ui.clip_rect()));
        }

        if preferences.parameter_hints && focused
            && let Some((signature, argument)) = caret.and_then(|caret| editor::call_hint(source, caret))
        {
            let overload = if argument > 0 { signature.rsplit("  ·  ").next().unwrap_or(&signature) } else { signature.as_str() };
            let parameter = overload.split_once('(').and_then(|(_, rest)| rest.split_once(')'))
                .and_then(|(parameters, _)| parameters.split(',').nth(argument))
                .map(|parameter| parameter.trim().trim_matches(['[', ']']).trim());
            ui.small(match parameter {
                Some(parameter) => format!("{signature}  ·  {parameter}"),
                None => format!("{signature}  ·  argument {}", argument + 1),
            });
        }
        if completion.visible {
            egui::ScrollArea::vertical().id_salt("calculation-pad-completions")
                .max_height(64.0).show(ui, |ui| {
                    for (index, candidate) in candidates.iter().enumerate() {
                        if ui.selectable_label(index == completion.selected, &candidate.label).clicked() {
                            completion.selected = index;
                            accept = true;
                        }
                    }
                });
        }
        if accept && let Some(selection) = editor_selection
            && let (Some(a), Some(b)) = (
                projection.byte_at_character(selection.primary.index.0),
                projection.byte_at_character(selection.secondary.index.0),
            )
        {
            let gaps = projection.gaps.iter()
                .filter(|(at, _)| a != b && a.min(b) <= *at && *at <= a.max(b))
                .map(|(_, range)| range.clone()).collect::<Vec<_>>();
            if !gaps.is_empty() {
                let ids = self.session.line_ids().unwrap_or_default();
                folds.collapsed.retain(|id| !regions.iter().any(|region| {
                    ids.get(region.header) == Some(id)
                        && gaps.iter().any(|gap| region.hidden.start < gap.end && gap.start < region.hidden.end)
                }));
                completion.visible = false;
                accept = false;
            }
        }
        if accept && !response.changed() && let Some(candidate) = candidates.get(completion.selected) {
            let replacement = if let Some(range) = editor_selection.filter(|range| range.primary.index != range.secondary.index) {
                let to_byte = |index| projection.byte_at_character(index)
                    .and_then(|byte| projection.source_byte(byte));
                match (to_byte(range.primary.index.0), to_byte(range.secondary.index.0)) {
                    (Some(a), Some(b)) => a.min(b)..a.max(b),
                    _ => candidate.replacement.clone(),
                }
            } else { candidate.replacement.clone() };
            let cursor = replacement.start + candidate.cursor - candidate.replacement.start;
            source.replace_range(replacement, &candidate.insertion);
            let character = self.fold_projection(source, folds).1
                .visible_character_clamped(source, source[..cursor].chars().count()).unwrap_or(0);
            *completion_cursor = Some(egui::text::CCursorRange::one(egui::text::CCursor::new(character)));
            *inspector_caret = None;
            *editor_primary_caret = None;
            *edited_source = Some(source.clone());
            completion.visible = false;
            completion.dismissed = Some((revision.saturating_add(1), cursor));
        }
        if expanded || smart_cursor.is_some() || clipboard_requested || deferred_projected_input.is_some() {
            let mut state = scroll.inner.state.clone();
            if let Some(cursor) = smart_cursor {
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor))));
            }
            *projection_state = Some(state);
            if let Some(state) = original_state { state.store(ui.ctx(), editor_id); }
        }
        if let Some(cursor) = smart_source_cursor {
            *inspector_caret = Some(cursor..cursor);
            *editor_primary_caret = Some(cursor);
        }
        if display_preferences.is_some()
            && !response.changed()
            && ui.memory(|memory| memory.focused().is_none_or(|id| id == editor_id))
        {
            *display_selection = editor_selection;
        }
        if authorized_pass && self.focus_source && self.session.pending_transition().is_none() {
            *source_focus_requested = true;
        }
    }

    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        evaluation: Option<&crate::calculation_pad::SessionEvaluation>,
        max_height: f32,
        result_copy: &mut Option<ResultCopyIntent>,
        host_identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
        inspector_selection: &mut Option<LineId>,
        open_inspector: &mut bool,
        result_navigation: &mut Option<(super::calculation_dependency_dialog::DependencyNavigation, u64)>,
        selected_result: &mut Option<LineId>,
        result_scroll: &mut Option<ResultScrollUpdate>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Results");
            let response = ui.button("Copy all");
            #[cfg(test)]
            {
                self.test_copy_all_rect = Some(response.rect);
            }
            if response.clicked()
                && let Some(document_id) = self.session.current_id()
            {
                *result_copy = Some(ResultCopyIntent {
                    host: host_identity,
                    viewport_id,
                    document_id,
                    target: ResultCopyTarget::All,
                });
            }
        });

        #[cfg(test)]
        self.test_line_copy_rects.clear();
        #[cfg(test)]
        self.test_inspect_rects.clear();
        #[cfg(test)]
        {
            self.test_result_rows.clear();
            self.test_rendered_result_rows.clear();
            self.test_select_comparison = None;
            self.test_go_to_source = None;
            self.test_result_bodies.clear();
        }

        let mut scroll_target = None;
        let scroll = egui::ScrollArea::vertical()
            .id_salt("calculation-pad-results")
            .max_height(max_height)
            .show(ui, |ui| {
                let Some(evaluation) = evaluation else {
                    match self.session.evaluation_status() {
                        crate::calculation_pad::EvaluationStatus::Pending => { ui.label("Calculating…"); }
                        crate::calculation_pad::EvaluationStatus::Failed(message) => { ui.colored_label(ui.visuals().error_fg_color, message); }
                        _ => { ui.label("Open the pad to start a calculation."); }
                    }
                    return;
                };
                if let Some(diagnostic) = evaluation.diagnostic.as_ref() {
                    ui.colored_label(ui.visuals().error_fg_color, diagnostic.to_string());
                }
                let selected = self.selected_result
                    .filter(|(incarnation, document, revision, _)| {
                        self.session.document_incarnation() == Some(*incarnation)
                            && self.session.current_id() == Some(*document)
                            && self.session.revision() == Some(*revision)
                    })
                    .map(|(_, _, _, line)| line)
                    .or_else(|| self.active_source_line.filter(|(incarnation, document, revision, _)| {
                        self.session.document_incarnation() == Some(*incarnation)
                            && self.session.current_id() == Some(*document)
                            && self.session.revision() == Some(*revision)
                    }).map(|(_, _, _, line)| line))
                    .or_else(|| self.dependency_seed());
                for line in &evaluation.lines {
                    let formatted = match &line.outcome {
                        LineOutcome::Blank | LineOutcome::Comment => continue,
                        LineOutcome::Value(value) => {
                            let formatted = result_format::display(value, self.display_preferences);
                            if formatted.as_ref().is_some_and(|text| text.trim().is_empty()) {
                                continue;
                            }
                            formatted
                        }
                        LineOutcome::Error(_) => None,
                    };
                    #[cfg(test)]
                    let mut text_rect = egui::Rect::NOTHING;
                    let highlight = ui.painter().add(egui::Shape::Noop);
                    let scope = ui.scope_builder(
                        egui::UiBuilder::new()
                            .id_salt((
                                "calculation-result-row",
                                self.session.current_id(),
                                self.session.document_incarnation(),
                                host_identity.generation,
                                line.id,
                            ))
                            .sense(egui::Sense::click()),
                        |ui| ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        #[cfg(test)]
                        self.test_rendered_result_rows.push((line.line_index, line.id));
                        ui.horizontal_wrapped(|ui| {
                            let row = ui
                                .monospace(format!("r{}", line.line_index + 1))
                                .interact(egui::Sense::click());
                            if matches!(line.outcome, LineOutcome::Value(_)) {
                                #[cfg(test)]
                                self.test_result_rows.push((line.line_index, row.rect));
                            }
                            let mut row_response = row;
                            if let Some(name) = line.assignment_name.as_deref() {
                                row_response = row_response.union(ui.strong(name));
                            }
                            if !line.dependency_indices.is_empty() {
                                let refs = line
                                    .dependency_indices
                                    .iter()
                                    .map(|index| format!("r{}", index + 1))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                row_response = row_response.union(ui.weak(format!("uses {refs}")));
                            }
                            match &line.outcome {
                                LineOutcome::Blank | LineOutcome::Comment => {}
                                LineOutcome::Value(value) => {
                                    let value_label = ui.label(formatted.as_deref().unwrap_or("unavailable"));
                                    #[cfg(test)] { text_rect = value_label.rect; }
                                    row_response = row_response.union(value_label);
                                    if matches!(value, crate::calculation_pad::CalculationValue::Integer(_) | crate::calculation_pad::CalculationValue::ExactBase { .. }) {
                                        let inspect = ui.small_button("Inspect bits…");
                                        #[cfg(test)]
                                        self.test_inspect_rects.push((line.line_index, inspect.rect, ui.clip_rect(), inspect.enabled()));
                                        if inspect.clicked() {
                                            *inspector_selection = Some(line.id);
                                            *open_inspector = true;
                                        }
                                        row_response = row_response.union(inspect);
                                    }
                                    if value.is_approximate() {
                                        row_response = row_response.union(ui.weak("approx."));
                                    }
                                    let response = ui.small_button("Copy");
                                    #[cfg(test)]
                                    self.test_line_copy_rects
                                        .push((line.line_index, response.rect));
                                    if response.clicked()
                                        && let Some(document_id) = self.session.current_id()
                                    {
                                        *result_copy = Some(ResultCopyIntent {
                                            host: host_identity,
                                            viewport_id,
                                            document_id,
                                            target: ResultCopyTarget::Line(line.id),
                                        });
                                    }
                                    row_response = row_response.union(response);
                                    let response = ui.small_button("Copy formatted");
                                    if response.clicked() && let Some(document_id) = self.session.current_id() {
                                        *result_copy = Some(ResultCopyIntent {
                                            host: host_identity, viewport_id, document_id,
                                            target: ResultCopyTarget::FormattedLine(line.id),
                                        });
                                    }
                                    row_response = row_response.union(response);
                                }
                                LineOutcome::Error(error) => {
                                    let error_label = ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        error.to_string(),
                                    );
                                    #[cfg(test)] { text_rect = error_label.rect; }
                                    row_response = row_response.union(error_label);
                                }
                            }
                            row_response
                        }).inner
                    }),
                    );
                    #[cfg(test)]
                    self.test_result_bodies.push((line.line_index, scope.response.rect, text_rect));
                    let row_response = scope.response.union(scope.inner.inner);
                    if selected == Some(line.id) {
                        ui.painter().set(highlight, egui::Shape::rect_filled(
                            row_response.rect, 2.0, ui.visuals().selection.bg_fill.gamma_multiply(0.35),
                        ));
                    }
                    if self.pending_result_scroll.is_some_and(|(_, _, revision, target)| {
                        revision == evaluation.revision && target == line.id
                    }) {
                        scroll_target = Some(row_response.rect);
                    }
                    if row_response.clicked() {
                        *selected_result = Some(line.id);
                    }
                    row_response.context_menu(|ui| {
                        let go_to_source = ui.button("Go to source");
                        #[cfg(test)] { self.test_go_to_source = Some(go_to_source.rect); }
                        if go_to_source.clicked()
                            && let (Some(document), Some(incarnation), Some(revision)) = (
                                self.session.current_id(), self.session.document_incarnation(), self.session.revision(),
                            )
                        {
                            *result_navigation = Some((super::calculation_dependency_dialog::DependencyNavigation {
                                document, incarnation, line: line.id,
                            }, revision));
                            ui.close();
                        }
                        if matches!(line.outcome, LineOutcome::Value(_)) {
                            let select = ui.button("Select for comparison");
                            #[cfg(test)] { self.test_select_comparison = Some(select.rect); }
                            if select.clicked() {
                                *inspector_selection = Some(line.id);
                                ui.close();
                            }
                        }
                    });
                }
            });
        if let (Some(target), Some(identity)) = (scroll_target, self.pending_result_scroll) {
            let mut state = scroll.state;
            if !scroll.inner_rect.contains_rect(target) {
                state.offset.y = (state.offset.y + target.center().y - scroll.inner_rect.center().y).max(0.0);
            }
            *result_scroll = Some(ResultScrollUpdate { target: identity, scroll_id: scroll.id, state });
        }
    }

    fn document_chooser_ui(&mut self, ui: &mut egui::Ui, intent: &mut Option<DialogIntent>) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong("Saved Calculation Pads");
                if ui.small_button("Refresh").clicked() {
                    *intent = Some(DialogIntent::RefreshDocuments);
                }
                if ui.small_button("Hide").clicked() {
                    *intent = Some(DialogIntent::HideChooser);
                }
            });
            if self.saved_documents.is_empty() {
                ui.label("No saved pads yet.");
            }
            egui::ScrollArea::vertical()
                .id_salt("calculation-pad-document-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for document in &self.saved_documents {
                        let text = format!(
                            "{}  ·  {} lines  ·  {} bytes",
                            document.name, document.line_count, document.source_bytes
                        );
                        if ui.selectable_label(false, text).clicked() {
                            *intent = Some(DialogIntent::Open(document.id));
                        }
                    }
                });
        });
    }

    fn save_as_ui(ui: &mut egui::Ui, save_as_name: &mut String, intent: &mut Option<DialogIntent>) {
        ui.group(|ui| {
            ui.strong("Save Calculation Pad As");
            ui.horizontal_wrapped(|ui| {
                ui.label("Document name");
                ui.text_edit_singleline(save_as_name);
                if ui.button("Save Copy").clicked() {
                    *intent = Some(DialogIntent::SaveAs(save_as_name.clone()));
                }
                if ui.button("Cancel").clicked() {
                    *intent = Some(DialogIntent::CancelSaveAs);
                }
            });
        });
    }

    fn pending_transition_ui(
        &mut self,
        ui: &mut egui::Ui,
        transition: PendingTransition,
        intent: &mut Option<DialogIntent>,
    ) {
        let description = match transition {
            PendingTransition::New => "Save changes before creating a new pad?",
            PendingTransition::Open(_) => "Save changes before opening the selected pad?",
            PendingTransition::Close => "Save changes before closing the Calculation Pad?",
        };
        ui.group(|ui| {
            ui.strong("Unsaved changes");
            ui.label(description);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Save));
                }
                let discard = ui.button("Discard");
                #[cfg(test)]
                {
                    self.test_discard_rect = Some(discard.rect);
                }
                if discard.clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Discard));
                }
                let keep_editing = ui.button("Keep Editing");
                #[cfg(test)]
                if ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_keep_editing_rect = Some(keep_editing.rect);
                }
                if keep_editing.clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Cancel));
                }
            });
        });
    }

    fn apply_intent(&mut self, intent: DialogIntent) {
        match intent {
            DialogIntent::New => match self.session.request_new() {
                Ok(TransitionOutcome::Applied) => self.after_transition_applied(),
                Ok(TransitionOutcome::DecisionRequired) => self.clear_feedback(),
                Ok(TransitionOutcome::Cancelled) => self.clear_feedback(),
                Err(error) => self.set_error(error),
            },
            DialogIntent::OpenChooser => {
                self.open_chooser = true;
                self.load_saved_documents();
            }
            DialogIntent::RefreshDocuments => self.load_saved_documents(),
            DialogIntent::Open(id) => match self.session.request_open(id) {
                Ok(TransitionOutcome::Applied) => {
                    self.open_chooser = false;
                    self.after_transition_applied();
                }
                Ok(TransitionOutcome::DecisionRequired) => {
                    self.open_chooser = false;
                    self.clear_feedback();
                }
                Ok(TransitionOutcome::Cancelled) => self.clear_feedback(),
                Err(error) => self.set_error(error),
            },
            DialogIntent::OpenSaveAs => {
                self.save_as_open = true;
                self.save_as_name = self.session.current_name().unwrap_or_default().to_owned();
            }
            DialogIntent::HideChooser => self.open_chooser = false,
            DialogIntent::CancelSaveAs => self.save_as_open = false,
            DialogIntent::Save => self.save_current(),
            DialogIntent::SaveAs(name) => self.save_as(&name),
            DialogIntent::OpenExamples => self.open_examples(),
            DialogIntent::Decision(decision) => self.decide_pending(decision),
            DialogIntent::SetRememberSource(_) => {}
        }
    }

    fn load_saved_documents(&mut self) {
        match self.session.list_saved_documents() {
            Ok(documents) => {
                self.saved_documents = documents;
                self.error = None;
                self.conflict_document = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn save_current(&mut self) {
        match self.session.save() {
            Ok(()) => {
                self.clear_feedback();
                self.notice = Some("Calculation Pad saved.".into());
            }
            Err(SessionError::SaveAsRequired) => {
                self.save_as_open = true;
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn save_as(&mut self, name: &str) {
        match self.session.save_as(name) {
            Ok(()) => {
                self.save_as_open = false;
                self.save_as_name.clear();
                self.clear_feedback();
                self.notice = Some("Calculation Pad saved as a new document.".into());
                if self.session.pending_transition().is_some() {
                    self.decide_pending(TransitionDecision::Save);
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn decide_pending(&mut self, decision: TransitionDecision) {
        let pending_before = self.session.pending_transition();
        if decision == TransitionDecision::Discard
            && pending_before.is_some()
            && self.remember_source
            && let Err(error) = self.recovery_writer.clear_now()
        {
            self.set_recovery_error(error.to_string());
            return;
        }
        if decision == TransitionDecision::Discard && pending_before.is_some() {
            self.last_recovery_identity = None;
        }
        match self.session.resolve_pending(decision) {
            Ok(TransitionOutcome::Applied) => self.after_transition_applied(),
            Ok(TransitionOutcome::DecisionRequired) => self.clear_feedback(),
            Ok(TransitionOutcome::Cancelled) => {
                self.open = true;
                self.request_source_focus();
                self.clear_feedback();
            }
            Err(SessionError::SaveAsRequired) if decision == TransitionDecision::Save => {
                self.save_as_open = true;
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn after_transition_applied(&mut self) {
        self.open = self.session.source().is_some();
        if self.open {
            self.request_source_focus();
        }
        self.open_chooser = false;
        self.save_as_open = false;
        self.clear_feedback();
    }

    fn clear_feedback(&mut self) {
        self.error = None;
        self.conflict_document = None;
        self.notice = None;
    }

    fn reset_transient_ui(&mut self) {
        self.pad_view = PadView::default();
        self.dependency_navigation = None;
        self.deferred_projected_input = None;
        self.split_preview = None;
        self.source_highlight = None;
        self.editor_tools = EditorTools::default();
        self.folds.collapsed.clear();
        self.editor_tool_focus_pending = false;
        self.selected_result = None;
        self.active_source_line = None;
        self.pending_result_scroll = None;
        self.open_chooser = false;
        self.examples_open = false;
        self.saved_documents.clear();
        self.save_as_open = false;
        self.save_as_name.clear();
        self.clear_feedback();
    }

    fn set_error(&mut self, error: SessionError) {
        self.conflict_document = match &error {
            SessionError::Store(PadStoreError::DiskConflict(id)) => Some(*id),
            _ => None,
        };
        self.error = Some(error.to_string());
        self.notice = None;
    }

    fn clear_text_edit_undoer(&self, ctx: &egui::Context, id: egui::Id) {
        if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, id) {
            state.clear_undoer();
            state.store(ctx, id);
        }
    }

    fn take_editor_history_shortcut(&self, ui: &mut egui::Ui, id: egui::Id) -> Option<bool> {
        if !ui.memory(|memory| memory.has_focus(id)) {
            return None;
        }
        ui.input_mut(|input| {
            if input.consume_key(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::Z,
            ) {
                Some(false)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y) {
                Some(false)
            } else {
                input
                    .consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
                    .then_some(true)
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        self.session.replace_source(source).unwrap();
    }

    #[cfg(test)]
    pub(crate) fn test_resolve_pending(&mut self, decision: TransitionDecision) {
        self.decide_pending(decision);
    }

    #[cfg(test)]
    pub(crate) fn test_source(&self) -> Option<&str> {
        self.session.source()
    }

    #[cfg(test)]
    pub(crate) fn test_current_document_id(&self) -> Option<DocumentId> {
        self.session.current_id()
    }

    #[cfg(test)]
    pub(crate) fn test_is_dirty(&self) -> bool {
        self.session.is_dirty()
    }

    #[cfg(test)]
    pub(crate) fn test_save_button_rect(&self) -> Option<egui::Rect> {
        self.test_save_rect
    }

    #[cfg(test)]
    pub(crate) fn test_copy_all_button_rect(&self) -> Option<egui::Rect> {
        self.test_copy_all_rect
    }

    #[cfg(test)]
    pub(crate) fn test_pending_transition(&self) -> Option<PendingTransition> {
        self.session.pending_transition()
    }

    #[cfg(test)]
    pub(crate) fn test_was_rendered(&self) -> bool {
        self.test_window_rect.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_editor_rect(&self) -> Option<egui::Rect> {
        self.test_editor_rect
    }

    #[cfg(test)]
    pub(crate) fn test_inspect_button(
        &self,
        index: usize,
    ) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_inspect_rects
            .iter()
            .find(|(line, _, _, _)| *line == index)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }

    #[cfg(test)]
    pub(crate) fn test_dependency_navigation_pending(&self) -> bool { self.dependency_navigation.is_some() }

    #[cfg(test)]
    pub(crate) fn test_dependency_open(&self) -> Option<(egui::Rect, egui::Rect, bool)> { self.test_dependency_open }

    #[cfg(test)]
    pub(crate) fn test_inspector_reopen(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_inspector_reopen
    }

    #[cfg(test)]
    pub(crate) fn test_result_row(&self, index: usize) -> Option<egui::Rect> {
        self.test_result_rows
            .iter()
            .find(|(row, _)| *row == index)
            .map(|(_, rect)| *rect)
    }

    #[cfg(test)]
    pub(crate) fn test_select_comparison(&self) -> Option<egui::Rect> {
        self.test_select_comparison
    }

    #[cfg(test)]
    pub(crate) fn test_editor_has_focus(&self, ctx: &egui::Context) -> bool {
        self.test_editor_id
            .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
    }

    #[cfg(test)]
    pub(crate) fn test_focus_source_pending(&self) -> bool {
        self.focus_source
    }

    #[cfg(test)]
    pub(crate) fn test_can_undo(&self) -> bool {
        self.session.can_undo()
    }

    #[cfg(test)]
    pub(crate) fn test_open_examples(&mut self) {
        self.open_examples();
    }

    #[cfg(test)]
    pub(crate) fn test_popout_pad(&mut self, ctx: &egui::Context) {
        self.apply_host_request(ctx, PadHost::Popout);
    }

    #[cfg(test)]
    pub(crate) fn test_examples_button_ownership(&self) -> Option<(egui::Rect, bool)> {
        self.test_examples_button_ownership
    }

    #[cfg(test)]
    pub(crate) fn test_reference_is_popout(&self) -> bool {
        self.examples_open && self.examples_host.presentation() == PadHost::Popout
    }

    #[cfg(test)]
    pub(crate) fn test_examples_button_rect(&self) -> Option<egui::Rect> {
        self.test_examples_button_rect
    }

    #[cfg(test)]
    pub(crate) fn test_examples_window_rect(&self) -> Option<egui::Rect> {
        self.test_examples_window_rect
    }

    #[cfg(test)]
    pub(crate) fn test_examples_copy_rect(&self) -> Option<egui::Rect> {
        self.test_examples_copy_rect
    }

    #[cfg(test)]
    pub(crate) fn test_apply_save_as(&mut self, name: &str) {
        self.save_as(name);
    }

    #[cfg(test)]
    pub(crate) fn test_open_document(&mut self, id: DocumentId) {
        self.apply_intent(DialogIntent::Open(id));
    }
}

fn copy_all_results(
    source: &str,
    evaluation: &crate::calculation_pad::SessionEvaluation,
) -> String {
    let mut labeled = String::new();
    let mut unlabeled = String::new();
    let mut results = Vec::new();
    if evaluation.lines.is_empty() {
        // Preserve source even if a document-level diagnostic prevented evaluation.
        labeled.push_str(source);
        unlabeled.push_str(source);
    }
    for (index, line) in evaluation.lines.iter().enumerate() {
        let Some(expression) = source.get(line.source_range.clone()) else {
            continue;
        };
        let outcome = match &line.outcome {
            LineOutcome::Blank | LineOutcome::Comment => None,
            LineOutcome::Value(value) => Some(
                value
                    .canonical_result()
                    .unwrap_or_else(|| "unavailable".into()),
            ),
            LineOutcome::Error(error) => Some(format!("error: {error}")),
        };
        if let Some(outcome) = outcome {
            let number = line.line_index + 1;
            labeled.push_str(&format!("r{number} = {expression} = {outcome}"));
            unlabeled.push_str(&format!("{expression} = {outcome}"));
            results.push(format!("r{number} = {outcome}"));
        } else {
            labeled.push_str(expression);
            unlabeled.push_str(expression);
        }
        let end = evaluation
            .lines
            .get(index + 1)
            .map_or(source.len(), |next| next.source_range.start);
        if let Some(separator) = source.get(line.source_range.end..end) {
            labeled.push_str(separator);
            unlabeled.push_str(separator);
        }
    }
    if let Some(diagnostic) = &evaluation.diagnostic {
        results.push(format!("Document error: {diagnostic}"));
    }
    format!(
        "Source and results\n{labeled}\n\nResults only\n{}\n\nSource and results without row labels\n{unlabeled}",
        results.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::{CalculationPadDialog, CalculationPadDialogEffect, DialogIntent, copy_all_results};
    use crate::calculation_pad::{
        CalculationPadSession, CalculationValue, LineOutcome, MAX_DOCUMENT_BYTES, PadDocumentStore,
        PadRecoverySnapshot, PadRecoveryStore, PadRecoveryWriteFailure, PadRecoveryWriter,
        PendingTransition, TransitionDecision,
    };
    use crate::platform::app_data::AppDataRoot;
    use eframe::egui;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn inspector_pad_receipt_rejects_stale_source_selection_and_caret() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("u8(1)");
        dialog.session.evaluation();
        let line = dialog.session.line_id(0).unwrap();
        let mut old = dialog.empty_host_effects(egui::ViewportId::ROOT);
        old.pass_accepted = true;
        old.edited_source = Some("u8(2)".into());
        old.inspector_selection = Some(line);
        old.inspector_caret = Some(0..5);
        old.open_inspector = true;
        dialog.test_replace_source("u8(3)");
        assert!(complete_host_effects(&mut dialog, &ctx, old).is_empty());
        assert_eq!(dialog.test_source(), Some("u8(3)"));
        assert!(dialog.inspector_context().candidate.is_none());
        assert!(dialog.inspector_context().caret.is_none());
    }

    fn fixture() -> (TempDir, AppDataRoot, CalculationPadDialog) {
        let directory = tempfile::tempdir().expect("create isolated pad data root");
        let root = AppDataRoot::from_path(directory.path().to_path_buf());
        let dialog = CalculationPadDialog::new(&root);
        (directory, root, dialog)
    }

    fn copy_all_session(session: &mut CalculationPadSession) -> String {
        let (source, evaluation) = session.evaluation_with_source().unwrap();
        copy_all_results(source, evaluation)
    }

    fn input(screen_size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<CalculationPadDialogEffect> {
        frame_effects(dialog, ctx, screen_size, events)
            .into_iter()
            .next()
    }

    fn frame_effects(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<CalculationPadDialogEffect> {
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
        });
        let effect = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effect
    }

    fn complete_host_effects(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        mut host: super::HostEffects,
    ) -> Vec<CalculationPadDialogEffect> {
        let root_pass = super::ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        host.pass = if host.pass_accepted {
            Some(super::ViewportPassProvenance::current(
                ctx,
                host.viewport_id,
            ))
        } else {
            None
        };

        let output = ctx.run_ui(input(egui::vec2(1200.0, 900.0), Vec::new()), |_| {});
        dialog.pending_root_effects = Some(super::RootEffects {
            pass: root_pass,
            help: None,
            host: Some(host),
        });
        let effects = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effects
    }

    #[test]
    fn closing_saved_view_then_opening_blank_does_not_copy_watches() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 2");
        let mut save = dialog.empty_host_effects(egui::ViewportId::ROOT);
        save.pass_accepted = true;
        save.pad_view = Some(super::PadView { watches: vec!["x".into()], ..Default::default() });
        save.intent = Some(super::DialogIntent::SaveAs("saved view".into()));
        complete_host_effects(&mut dialog, &ctx, save);
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        assert!(!dialog.open);
        dialog.open();
        assert!(dialog.pad_view.watches.is_empty());
        let host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        let mut host = host; host.pass_accepted = true;
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(dialog.session.view_metadata().unwrap().watches.is_empty());
    }

    #[test]
    fn remembered_scratch_with_dirty_view_requires_close_decision() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        let mut edit = dialog.empty_host_effects(egui::ViewportId::ROOT);
        edit.pass_accepted = true;
        edit.pad_view = Some(super::PadView { watches: vec!["x".into()], ..Default::default() });
        complete_host_effects(&mut dialog, &ctx, edit);
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        assert!(dialog.open);
        assert_eq!(dialog.session.pending_transition(), Some(crate::calculation_pad::PendingTransition::Close));
    }

    #[test]
    fn retained_view_changes_are_saved_with_same_frame_save_as() {
        let (_directory, root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 2");
        let id = dialog.session.line_id(0).unwrap();
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.pad_view = Some(super::PadView {
            inputs: vec![super::InputSelection { id, name: "x".into(), unavailable: false, draft: None }],
            watches: vec!["x".into()], ..Default::default()
        });
        host.intent = Some(super::DialogIntent::SaveAs("view retained".into()));
        complete_host_effects(&mut dialog, &ctx, host);
        let document = dialog.session.current_id().unwrap();
        let mut reopened = crate::calculation_pad::CalculationPadSession::new(&root);
        reopened.request_open(document).unwrap();
        let metadata = reopened.view_metadata().unwrap();
        assert_eq!(metadata.inputs[0].id, id);
        assert_eq!(metadata.watches, vec!["x"]);
    }

    #[test]
    fn deleting_an_indistinguishable_duplicate_invalidates_selected_input() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 2\nx = 2");
        let id = dialog.session.line_id(0).unwrap();
        dialog.pad_view.inputs.push(super::InputSelection { id, name: "x".into(), unavailable: false, draft: None });
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("x = 2".into());
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(dialog.pad_view.inputs[0].unavailable);
    }

    #[test]
    fn reordering_duplicate_assignments_invalidates_selection() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 2\nx = 2\ny = 1");
        let id = dialog.session.line_id(0).unwrap();
        dialog.pad_view.inputs.push(super::InputSelection { id, name: "x".into(), unavailable: false, draft: None });
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("x = 2\ny = 1\nx = 2".into());
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(dialog.pad_view.inputs[0].unavailable);
    }

    #[test]
    fn input_commit_is_retained_and_stale_commits_are_rejected() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 1\nx = 2");
        let id = dialog.session.line_id(1).unwrap();
        let revision = dialog.session.revision().unwrap();
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.input_commit = Some(super::InputCommit { id, original: "x = 2".into(), revision, value: "8".into() });
        complete_host_effects(&mut dialog, &ctx, host);
        assert_eq!(dialog.test_source(), Some("x = 1\nx = 8"));
        assert!(dialog.session.undo());
        assert_eq!(dialog.test_source(), Some("x = 1\nx = 2"));
        let mut stale = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale.pass_accepted = true;
        stale.input_commit = Some(super::InputCommit { id, original: "x = 2".into(), revision: dialog.session.revision().unwrap(), value: "9".into() });
        dialog.test_replace_source("x = 1\nx = 4");
        complete_host_effects(&mut dialog, &ctx, stale);
        assert_eq!(dialog.test_source(), Some("x = 1\nx = 4"));
    }

    #[test]
    fn editor_replacement_is_one_retained_transaction_and_rejects_stale_edits() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("雪 = 1\r\n雪 + 1\r\n");
        let original = dialog.test_source().unwrap().to_owned();
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.editor_replacement = Some("月 = 1\r\n月 + 1\r\n".into());
        complete_host_effects(&mut dialog, &ctx, host);
        assert_eq!(dialog.test_source(), Some("月 = 1\r\n月 + 1\r\n"));
        assert!(dialog.session.undo());
        assert_eq!(dialog.test_source(), Some(original.as_str()));

        let mut stale = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale.pass_accepted = true;
        stale.editor_replacement = Some("stale".into());
        dialog.test_replace_source("newer = 4");
        complete_host_effects(&mut dialog, &ctx, stale);
        assert_eq!(dialog.test_source(), Some("newer = 4"));

        let mut simultaneous = dialog.empty_host_effects(egui::ViewportId::ROOT);
        simultaneous.pass_accepted = true;
        simultaneous.edited_source = Some("typed = 9".into());
        simultaneous.editor_replacement = Some("replacement".into());
        complete_host_effects(&mut dialog, &ctx, simultaneous);
        assert_eq!(dialog.test_source(), Some("typed = 9"));
    }

    #[test]
    fn editor_navigation_and_result_selection_require_current_retained_host() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("雪 = 1\r\n雪 + 2");
        let second = dialog.session.line_id(1).unwrap();
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.editor_navigation = Some(9..12);
        complete_host_effects(&mut dialog, &ctx, host);
        let pending = dialog.dependency_navigation.unwrap();
        assert_eq!(pending.navigation.line, second);
        assert_eq!(pending.selection, Some((9, 12)));
        assert!(!pending.focus_source);

        dialog.dependency_navigation = None;
        let mut stale = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale.pass_accepted = true;
        stale.selected_result = Some(second);
        dialog.test_replace_source("new source");
        complete_host_effects(&mut dialog, &ctx, stale);
        assert!(dialog.selected_result.is_none());
    }

    #[test]
    fn editor_shortcuts_open_replace_and_escape_dismisses_local_tools() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("amount = 7");
        let size = egui::vec2(1000.0, 700.0);
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, size, vec![key(egui::Key::H, egui::Modifiers::COMMAND)]);
        assert!(dialog.editor_tools.open && dialog.editor_tools.replace);
        assert_eq!(dialog.test_source(), Some("amount = 7"));
        frame(&mut dialog, &ctx, size, vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        assert!(!dialog.editor_tools.open);
        assert!(dialog.open);
    }

    #[test]
    fn folded_editor_edits_visible_text_and_select_all_reveals_full_source() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("# region values\n雪 = 2\n# endregion\ntotal = 3");
        dialog.folds.collapsed.push(dialog.session.line_id(0).unwrap());
        let size = egui::vec2(1000.0, 700.0);
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
        let visible_len = "# region values\ntotal = 3".chars().count();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(visible_len))));
        state.store(&ctx, editor_id);
        frame(&mut dialog, &ctx, size, vec![egui::Event::Text("X".into())]);
        assert!(dialog.test_source().unwrap().contains("雪 = 2\n# endregion"));
        assert!(dialog.test_source().unwrap().ends_with("total = 3X"));
        assert_eq!(dialog.folds.collapsed.len(), 1);
        let hover = dialog.test_editor_rect.unwrap().center();
        frame(&mut dialog, &ctx, size, vec![egui::Event::Text("Y".into()), egui::Event::PointerMoved(hover)]);
        frame(&mut dialog, &ctx, size, vec![egui::Event::PointerMoved(hover), egui::Event::Text("Z".into())]);
        assert!(dialog.test_source().unwrap().ends_with("total = 3XYZ"));
        assert_eq!(dialog.folds.collapsed.len(), 1);
        frame(&mut dialog, &ctx, size, vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
        assert!(dialog.folds.collapsed.is_empty());
        let selection = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap()
            .cursor.char_range().unwrap().as_sorted_char_range();
        assert_eq!(selection.start.0, 0);
        assert_eq!(selection.end.0, dialog.test_source().unwrap().chars().count());
        let expected = dialog.test_source().unwrap().to_owned();
        let effects = frame_effects(&mut dialog, &ctx, size, vec![egui::Event::Copy]);
        assert!(effects.contains(&CalculationPadDialogEffect::CopyText(expected)));
    }

    #[test]
    fn smart_edit_wraps_and_skips_closing_delimiter() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("雪");
        dialog.display_preferences.auto_close_delimiters = true;
        let size = egui::vec2(1000.0, 700.0);
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(1))));
        state.store(&ctx, editor_id);
        frame(&mut dialog, &ctx, size, vec![egui::Event::Text("(".into())]);
        assert_eq!(dialog.test_source(), Some("(雪)"));
        frame(&mut dialog, &ctx, size, vec![egui::Event::Text(")".into())]);
        assert_eq!(dialog.test_source(), Some("(雪)"));

        dialog.test_replace_source("ab");
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(2))));
        state.store(&ctx, editor_id);
        frame(&mut dialog, &ctx, size, vec![
            egui::Event::Text("(".into()), key(egui::Key::Backspace, egui::Modifiers::NONE),
        ]);
        assert_eq!(dialog.test_source(), Some("ab"));
    }

    #[test]
    fn discarded_source_copy_and_cut_do_not_publish_or_edit() {
        for event in [egui::Event::Copy, egui::Event::Cut] {
            let (_directory, _root, mut dialog) = fixture();
            let ctx = egui::Context::default();
            dialog.open();
            dialog.test_replace_source("secret = 4");
            let size = egui::vec2(1000.0, 700.0);
            for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
            let editor_id = dialog.test_editor_id.unwrap();
            ctx.memory_mut(|memory| memory.request_focus(editor_id));
            let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0), egui::text::CCursor::new(10),
            )));
            let selection = state.cursor.char_range();
            state.store(&ctx, editor_id);
            let output = ctx.run_ui(input(size, vec![event]), |ui| {
                let first = ui.ctx().current_pass_index() == 0;
                dialog.show(ui.ctx(), first, true, true);
                if first { ui.ctx().request_discard("source copy/cut retained-pass check"); }
            });
            assert!(!output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(_))));
            assert!(!dialog.on_ui_frame_complete(&ctx).iter().any(|effect| matches!(effect, CalculationPadDialogEffect::CopyText(_))));
            assert_eq!(dialog.test_source(), Some("secret = 4"));
            assert_eq!(egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap().cursor.char_range(), selection);
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn compact_completion_accepts_and_disabled_enter_remains_editor_input() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(620.0, 480.0);
        dialog.open();
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, size, vec![egui::Event::Text("sqr".into())]);
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(dialog.completion.visible);
        frame(&mut dialog, &ctx, size, vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
        assert_eq!(dialog.test_source(), Some("sqrt()"));
        dialog.display_preferences.completion = false;
        dialog.test_replace_source("sqr");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, size, Vec::new());
        frame(&mut dialog, &ctx, size, vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        assert!(dialog.test_source().unwrap().contains('\n'));
    }

    #[test]
    fn editor_navigation_is_rejected_when_host_is_retiring() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("abc");
        let nav = super::super::calculation_dependency_dialog::DependencyNavigation {
            document: dialog.session.current_id().unwrap(),
            incarnation: dialog.session.document_incarnation().unwrap(),
            line: dialog.session.line_id(0).unwrap(),
        };
        let revision = dialog.session.revision().unwrap();
        let identity = dialog.host_identity();
        dialog.dependency_navigation = Some(super::SourceNavigation {
            host: identity, navigation: nav, revision, result_navigation: false,
            selection: Some((0, 2)), focus_source: false,
        });
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.host_request = Some(super::PadHost::Popout);
        host.dependency_applied = Some(super::DependencyEditorUpdate {
            navigation: nav, revision,
            editor_id: egui::Id::new("retired-editor"),
            editor_state: egui::widgets::text_edit::TextEditState::default(),
            scroll_id: egui::Id::new("retired-scroll"),
            scroll_state: egui::scroll_area::State::default(),
            result_navigation: false, source_byte_offset: 0,
            selection: Some((0, 2)), focus_source: false,
        });
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(egui::widgets::text_edit::TextEditState::load(&ctx, egui::Id::new("retired-editor")).is_none());
        assert!(dialog.inspector_context().caret.is_none());
    }

    #[test]
    fn editor_navigation_resolves_live_source_before_results_are_ready() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("total = 0\nfor x in range(1000):\n    total = total + x");
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.editor_navigation = Some(10..10);
        complete_host_effects(&mut dialog, &ctx, host);
        let pending = dialog.dependency_navigation.unwrap();
        assert_eq!(pending.navigation.line, dialog.session.line_id(1).unwrap());
        assert_eq!(pending.selection, Some((10, 10)));
    }

    fn complete_help_effects(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        mut help: super::HelpEffects,
        mut host: Option<super::HostEffects>,
    ) -> Vec<CalculationPadDialogEffect> {
        let pass = super::ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        help.pass = help.accepted.then_some(pass);
        if let Some(host) = host.as_mut() {
            host.pass = host.pass_accepted.then_some(pass);
        }
        let output = ctx.run_ui(input(egui::vec2(1200.0, 900.0), Vec::new()), |_| {});
        dialog.pending_root_effects = Some(super::RootEffects {
            pass,
            help: Some(help),
            host,
        });
        let effects = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effects
    }

    #[test]
    fn dependency_navigation_applies_current_utf8_caret_and_scroll_only_after_retention() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        let mut lines = (0..50).map(|n| format!("v{n} = {n}")).collect::<Vec<_>>();
        lines.push("{顧客 名} = 42".into());
        dialog.test_replace_source(&lines.join("\n"));
        let size = egui::vec2(900.0, 700.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let snapshot = dialog.dependency_context().unwrap();
        let target = snapshot.rows.last().unwrap();
        let nav = super::super::calculation_dependency_dialog::DependencyNavigation {
            document: snapshot.document_id,
            incarnation: snapshot.incarnation,
            line: target.id,
        };
        let host = dialog.host_identity();
        let editor_id = dialog.test_editor_id.unwrap();
        let scroll_id = dialog.test_editor_scroll_id.unwrap();
        let before_cursor = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id)
            .and_then(|state| state.cursor.char_range());
        let before_scroll = egui::scroll_area::State::load(&ctx, scroll_id)
            .unwrap()
            .offset;
        let before = (
            dialog.session.source().unwrap().to_owned(),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
            dialog.session.evaluation_count(),
        );
        assert!(dialog.navigate_dependency(nav, snapshot.revision, host, &ctx));
        assert!(
            !dialog.focus_source,
            "queued navigation does not force focus before retention"
        );
        frame_requesting_late_discard(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(
            egui::widgets::text_edit::TextEditState::load(&ctx, editor_id)
                .and_then(|state| state.cursor.char_range()),
            before_cursor
        );
        assert_eq!(
            egui::scroll_area::State::load(&ctx, scroll_id)
                .unwrap()
                .offset,
            before_scroll
        );
        frame(&mut dialog, &ctx, size, Vec::new());
        let cursor = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        let expected_start = dialog.session.source().unwrap()[..target.source_range.start]
            .chars()
            .count();
        let expected_end = dialog.session.source().unwrap()[..target.source_range.end]
            .chars()
            .count();
        assert_eq!(
            (
                usize::from(cursor.primary.index),
                usize::from(cursor.secondary.index)
            ),
            (expected_end, expected_start)
        );
        assert!(
            egui::scroll_area::State::load(&ctx, scroll_id)
                .unwrap()
                .offset
                .y
                > before_scroll.y
        );
        assert_eq!(
            (
                dialog.session.source().unwrap().to_owned(),
                dialog.session.revision(),
                dialog.session.is_dirty(),
                dialog.session.can_undo(),
                dialog.session.can_redo(),
                dialog.session.evaluation_count()
            ),
            before
        );
        assert!(dialog.navigate_dependency(nav, snapshot.revision, host, &ctx));
        assert!(!dialog.focus_source);
        dialog.host_state.transition(super::PadHost::Popout);
        assert!(
            !dialog.focus_source,
            "a queued request cannot focus a retired host"
        );
        assert!(!dialog.navigate_dependency(nav, snapshot.revision, host, &ctx));
        dialog.host_state.transition(super::PadHost::Docked);
        assert!(!dialog.navigate_dependency(nav, snapshot.revision, host, &ctx));
    }

    #[test]
    fn pad_and_reference_restore_accepted_presentations_and_ignore_cancelled_close() {
        use super::super::utility_presentation::UtilityPresentationManager;
        use crate::settings::{RememberedPresentation, Settings, UtilityPresentations};
        use crate::settings_save::SettingsSaveWorkerRegistry;

        let (_directory, root, mut dialog) = fixture();
        let settings_dir = tempfile::tempdir().unwrap();
        let path = settings_dir
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        Settings::default().save(&path).unwrap();
        let workers = SettingsSaveWorkerRegistry::default();
        let ctx = egui::Context::default();
        let manager = UtilityPresentationManager::new(
            path.clone(),
            workers.clone(),
            UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        dialog.open();
        dialog.prepare_open_presentation(&ctx);
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);
        dialog.test_replace_source("dirty = 7");
        let mut discarded = dialog.empty_host_effects(egui::ViewportId::ROOT);
        discarded.host_request = Some(super::PadHost::Popout);
        complete_host_effects(&mut dialog, &ctx, discarded);
        assert_eq!(manager.preference(super::calculation_pad_tool_key()), None);
        let mut pop = dialog.empty_host_effects(egui::ViewportId::ROOT);
        pop.pass_accepted = true;
        pop.host_request = Some(super::PadHost::Popout);
        complete_host_effects(&mut dialog, &ctx, pop);
        assert_eq!(
            manager.preference(super::calculation_pad_tool_key()),
            Some(RememberedPresentation::Popout)
        );
        dialog.open_examples();
        dialog.prepare_open_presentation(&ctx);
        let mut reference = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        reference.accepted = true;
        reference.host_request = Some(super::PadHost::Popout);
        complete_help_effects(&mut dialog, &ctx, reference, None);
        let mut reference_close = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        reference_close.accepted = true;
        reference_close.close = true;
        complete_help_effects(&mut dialog, &ctx, reference_close, None);
        dialog.open_examples();
        dialog.prepare_open_presentation(&ctx);
        assert_eq!(dialog.examples_host.presentation(), super::PadHost::Popout);
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        assert!(dialog.open);
        let mut cancel = dialog.empty_host_effects(egui::ViewportId::ROOT);
        cancel.pass_accepted = true;
        cancel.intent = Some(super::DialogIntent::Decision(TransitionDecision::Cancel));
        complete_host_effects(&mut dialog, &ctx, cancel);
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("dirty = 7"));
        assert_eq!(
            manager.preference(super::calculation_pad_tool_key()),
            Some(RememberedPresentation::Popout)
        );
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        let mut discard = dialog.empty_host_effects(egui::ViewportId::ROOT);
        discard.pass_accepted = true;
        discard.intent = Some(super::DialogIntent::Decision(TransitionDecision::Discard));
        complete_host_effects(&mut dialog, &ctx, discard);
        assert!(!dialog.open);
        dialog.open();
        dialog.prepare_open_presentation(&ctx);
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);
        workers.join_all();
        let saved = Settings::load(&path).unwrap();
        let workers2 = SettingsSaveWorkerRegistry::default();
        let ctx2 = egui::Context::default();
        let manager2 =
            UtilityPresentationManager::new(path, workers2.clone(), saved.utility_presentations);
        manager2.install_in_context(&ctx2);
        let mut restarted = CalculationPadDialog::new(&root);
        assert!(!restarted.open);
        restarted.open();
        restarted.prepare_open_presentation(&ctx2);
        assert_eq!(restarted.host_state.presentation(), super::PadHost::Popout);
        restarted.open_examples();
        restarted.prepare_open_presentation(&ctx2);
        assert_eq!(
            restarted.examples_host.presentation(),
            super::PadHost::Popout
        );
        let mut dock = restarted.empty_host_effects(egui::ViewportId::ROOT);
        dock.pass_accepted = true;
        dock.host_request = Some(super::PadHost::Docked);
        complete_host_effects(&mut restarted, &ctx2, dock);
        assert_eq!(
            manager2.preference(super::calculation_pad_tool_key()),
            Some(RememberedPresentation::Docked)
        );
        workers2.join_all();
    }

    #[test]
    fn reference_host_preserves_pad_and_rejects_retired_effects() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 1\nx + 2");
        dialog.test_replace_source("x = 2\nx + 2");
        let before = (
            dialog.session.source().unwrap().to_owned(),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
        );
        dialog.open_examples();
        let mut pop = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        pop.accepted = true;
        pop.search = Some("sqrt".into());
        pop.scroll = Some(42.0);
        pop.host_request = Some(super::PadHost::Popout);
        assert!(complete_help_effects(&mut dialog, &ctx, pop, None).is_empty());
        assert_eq!(dialog.examples_host.presentation(), super::PadHost::Popout);
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);
        assert_eq!(dialog.examples_search, "sqrt");
        assert_eq!(dialog.examples_scroll, 42.0);

        let mut discarded = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        discarded.copy = Some("bad".into());
        discarded.close = true;
        discarded.native_close = true;
        assert!(complete_help_effects(&mut dialog, &ctx, discarded, None).is_empty());
        assert!(dialog.examples_open);
        let mut dock = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        dock.accepted = true;
        dock.host_request = Some(super::PadHost::Docked);
        assert_eq!(
            complete_help_effects(&mut dialog, &ctx, dock, None),
            vec![CalculationPadDialogEffect::ShowRoot]
        );
        assert_eq!(dialog.examples_search, "sqrt");
        assert_eq!(dialog.examples_scroll, 42.0);

        let mut old = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        old.accepted = true;
        old.copy = Some("stale".into());
        let mut close = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        close.accepted = true;
        close.close = true;
        assert!(complete_help_effects(&mut dialog, &ctx, close, None).is_empty());
        assert!(!dialog.examples_open);
        dialog.open_examples();
        assert!(complete_help_effects(&mut dialog, &ctx, old, None).is_empty());
        let after = (
            dialog.session.source().unwrap().to_owned(),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
        );
        assert_eq!(before, after);
    }

    #[test]
    fn reference_owner_retirement_precedes_same_frame_copy() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 4");
        dialog.open_examples();
        let mut new = dialog.empty_host_effects(egui::ViewportId::ROOT);
        new.pass_accepted = true;
        new.intent = Some(super::DialogIntent::New);
        complete_host_effects(&mut dialog, &ctx, new);
        assert!(
            dialog.examples_open,
            "a pending dirty decision keeps help open"
        );
        let mut help = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        help.accepted = true;
        help.copy = Some("old owner".into());
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.intent = Some(super::DialogIntent::Decision(
            crate::calculation_pad::TransitionDecision::Discard,
        ));
        assert!(complete_help_effects(&mut dialog, &ctx, help, Some(host)).is_empty());
        assert!(!dialog.examples_open);
        assert_eq!(dialog.session.source(), Some(""));
    }

    #[test]
    fn reference_stays_open_across_save_as_of_same_session() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 4");
        dialog.open_examples();
        dialog.examples_search = "floor".into();
        dialog.examples_scroll = 25.0;
        let incarnation = dialog.session.document_incarnation();
        let old_id = dialog.session.current_id();
        let mut help = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        help.accepted = true;
        help.copy = Some("Floor(2.9)".into());
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.intent = Some(super::DialogIntent::SaveAs("same session".into()));
        assert_eq!(
            complete_help_effects(&mut dialog, &ctx, help, Some(host)),
            vec![CalculationPadDialogEffect::CopyText("Floor(2.9)".into())]
        );
        assert_ne!(dialog.session.current_id(), old_id);
        assert_eq!(dialog.session.document_incarnation(), incarnation);
        assert!(dialog.examples_open);
        assert_eq!(dialog.examples_search, "floor");
        assert_eq!(dialog.examples_scroll, 25.0);
    }

    #[test]
    fn reference_small_window_keeps_search_actions_and_scroll_visible() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.open_examples();
        let size = egui::vec2(420.0, 360.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let window = dialog.test_examples_window_rect.unwrap();
        let (search, clear, host, body) = dialog.test_examples_controls.unwrap();
        for control in [search, clear, host] {
            assert!(
                window.contains_rect(control),
                "control {control:?} outside {window:?}"
            );
        }
        assert!(body.is_positive());
        assert!(
            search.bottom() <= body.top()
                && clear.bottom() <= body.top()
                && host.bottom() <= body.top()
        );
        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                egui::Event::PointerMoved(body.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -180.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let offset = dialog.examples_scroll;
        assert!(
            offset > 0.0,
            "wheel should scroll the reference body, got {offset}"
        );
        let mut pop = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        pop.accepted = true;
        pop.host_request = Some(super::PadHost::Popout);
        complete_help_effects(&mut dialog, &ctx, pop, None);
        let mut dock = dialog.empty_examples_effects(egui::ViewportId::ROOT);
        dock.accepted = true;
        dock.host_request = Some(super::PadHost::Docked);
        complete_help_effects(&mut dialog, &ctx, dock, None);
        assert_eq!(dialog.examples_scroll, offset);
        dialog.examples_scroll = 0.0;
        dialog.examples_search = "floor".into();
        for _ in 0..2 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        assert!(
            dialog.test_examples_copy_rect.is_some(),
            "active search shows matching examples without opening groups"
        );
        dialog.examples_search = "no-such-result".into();
        frame(&mut dialog, &ctx, size, Vec::new());
        let clear = dialog.test_examples_controls.unwrap().1;
        click(&mut dialog, &ctx, size, clear.center());
        assert_eq!(dialog.examples_search, "");
    }

    #[test]
    fn reference_search_reveals_matches_from_a_closed_group() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        dialog.open();
        dialog.open_examples();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let header = dialog.test_examples_group_rect.expect("first group header");
        click(&mut dialog, &ctx, size, header.center());
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(
            !dialog
                .test_examples_rendered_sources
                .contains(&"radius = 3\nPI * radius ^ 2")
        );
        dialog.examples_search = "radius".into();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(
            dialog
                .test_examples_rendered_sources
                .contains(&"radius = 3\nPI * radius ^ 2")
        );
    }

    #[test]
    fn reference_docked_copy_recovers_after_discarded_root_pass() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        dialog.open();
        dialog.open_examples();
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let copy = dialog
            .test_examples_copy_rect
            .expect("reference copy button");
        frame(&mut dialog, &ctx, size, click_events(copy.center(), true));
        assert!(
            frame_requesting_late_discard(
                &mut dialog,
                &ctx,
                size,
                click_events(copy.center(), false)
            )
            .is_none()
        );
        assert_eq!(
            click(&mut dialog, &ctx, size, copy.center()),
            Some(CalculationPadDialogEffect::CopyText(
                "radius = 3\nPI * radius ^ 2".into()
            ))
        );
    }

    #[test]
    fn reference_native_callback_runs_with_hidden_root_and_os_close_keeps_pad() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 5");
        dialog.open_examples();
        dialog.examples_host.transition(super::PadHost::Popout);
        let before = (
            dialog.session.source().unwrap().to_owned(),
            dialog.session.revision(),
            dialog.session.is_dirty(),
        );
        let run = |dialog: &mut CalculationPadDialog| {
            super::super::render::with_immediate_viewport_renderer(&ctx, || {
                let output = ctx.run_ui(input(egui::vec2(900.0, 700.0), Vec::new()), |ui| {
                    dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, false);
                });
                output.drop_without_applying_deltas();
                dialog.on_ui_frame_complete(&ctx)
            })
        };
        for _ in 0..3 {
            run(&mut dialog);
        }
        assert!(dialog.test_examples_window_rect.is_some());
        assert!(!dialog.examples_host.bootstrap_pending());
        let copy = dialog
            .test_examples_copy_rect
            .expect("native reference copy button");
        let copy_state = dialog.test_examples_copy_state.unwrap();
        assert!(
            copy_state.0 && copy_state.1.contains(copy.center()),
            "native copy button is disabled or clipped: {copy:?}, {copy_state:?}"
        );
        let click_native = |dialog: &mut CalculationPadDialog, pressed| {
            super::super::render::with_immediate_viewport_test_events_for(
                super::reference_viewport_id(),
                click_events(copy.center(), pressed),
                || run(dialog),
            )
        };
        click_native(&mut dialog, true);
        let first_copy = click_native(&mut dialog, false);
        assert_eq!(
            first_copy,
            vec![CalculationPadDialogEffect::CopyText(
                "radius = 3\nPI * radius ^ 2".into()
            )]
        );
        click_native(&mut dialog, true);
        super::super::render::IMMEDIATE_VIEWPORT_TEST_DISCARD
            .with(|discard| discard.set(Some(super::reference_viewport_id())));
        assert!(click_native(&mut dialog, false).is_empty());
        click_native(&mut dialog, true);
        let copied = click_native(&mut dialog, false);
        assert_eq!(
            copied,
            vec![CalculationPadDialogEffect::CopyText(
                "radius = 3\nPI * radius ^ 2".into()
            )]
        );
        super::super::render::with_immediate_viewport_test_close_request(
            super::reference_viewport_id(),
            || run(&mut dialog),
        );
        assert!(!dialog.examples_open);
        assert!(dialog.open);
        assert_eq!(
            before,
            (
                dialog.session.source().unwrap().to_owned(),
                dialog.session.revision(),
                dialog.session.is_dirty()
            )
        );
    }

    fn immediate_viewport_frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        root_events: Vec<egui::Event>,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let output = ctx.run_ui(input(egui::vec2(1200.0, 900.0), root_events), |ui| {
                dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
            });
            let commands = output
                .viewport_output
                .get(&super::pad_viewport_id())
                .map(|output| output.commands.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
            commands
        })
    }

    fn immediate_child_frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_test_events_for(
            super::pad_viewport_id(),
            events,
            || immediate_viewport_frame(dialog, ctx, Vec::new()),
        )
    }

    fn click_immediate_child(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        point: egui::Pos2,
    ) -> Vec<egui::ViewportCommand> {
        let mut commands = Vec::new();
        for pressed in [true, false] {
            commands = immediate_child_frame(dialog, ctx, click_events(point, pressed));
        }
        commands
    }

    fn frame_requesting_discard(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (Option<CalculationPadDialogEffect>, bool) {
        let mut first_ui_pass_seen = false;
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            if first_ui_pass {
                first_ui_pass_seen = true;
                ui.ctx()
                    .request_discard("Calculation Pad discarded-pass regression");
            }
            dialog.show(ui.ctx(), first_ui_pass, true, true);
        });
        let effect = dialog.on_ui_frame_complete(ctx).into_iter().next();
        output.drop_without_applying_deltas();
        (effect, first_ui_pass_seen)
    }

    fn frame_requesting_late_discard(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<CalculationPadDialogEffect> {
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            dialog.show(ui.ctx(), first_ui_pass, true, true);
            if first_ui_pass {
                ui.ctx()
                    .request_discard("Calculation Pad late ROOT discard regression");
            }
        });
        let effect = dialog.on_ui_frame_complete(ctx).into_iter().next();
        output.drop_without_applying_deltas();
        effect
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn click_events(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn click(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        size: egui::Vec2,
        point: egui::Pos2,
    ) -> Option<CalculationPadDialogEffect> {
        frame(dialog, ctx, size, click_events(point, true));
        frame(dialog, ctx, size, click_events(point, false))
    }

    fn right_click(dialog: &mut CalculationPadDialog, ctx: &egui::Context, size: egui::Vec2, point: egui::Pos2) {
        for pressed in [true, false] {
            frame(dialog, ctx, size, vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton { pos: point, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE },
            ]);
        }
        frame(dialog, ctx, size, Vec::new());
    }

    #[test]
    fn result_context_menu_navigates_value_error_and_row_background() {
        for target in ["value", "error", "background"] {
            let (_directory, _root, mut dialog) = fixture();
            let ctx = egui::Context::default();
            let size = egui::vec2(1000.0, 760.0);
            let source = format!("{}{{雪}} = 2\r\nbad =\r\n", "# hidden\r\n".repeat(80));
            dialog.open();
            dialog.test_replace_source(&source);
            for _ in 0..4 { frame(&mut dialog, &ctx, size, Vec::new()); }
            let index = if target == "error" { 81 } else { 80 };
            let initial_rows = &dialog.test_editor_geometry.as_ref().unwrap().3;
            assert!(initial_rows[index].1 > dialog.test_editor_clip.unwrap().bottom());
            let (_, body, text) = dialog.test_result_bodies.iter().find(|(line, _, _)| *line == index).copied().unwrap();
            let point = if target == "background" {
                egui::pos2(body.right() - 4.0, body.center().y)
            } else {
                text.center()
            };
            right_click(&mut dialog, &ctx, size, point);
            let menu = dialog.test_go_to_source.expect("row context menu offers Go to source");
            if target != "error" { assert!(dialog.test_select_comparison.is_some()); }
            click(&mut dialog, &ctx, size, menu.center());
            for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
            let highlight = dialog.source_highlight.expect("navigation starts highlight");
            assert_eq!(highlight.navigation.line, dialog.session.line_id(index).unwrap());
            let location = dialog.session.resolve_dependency_line(highlight.navigation.document, highlight.navigation.incarnation, highlight.navigation.line).unwrap();
            let expected = source[..location.source_range.start].chars().count();
            assert_eq!(dialog.inspector_context().caret.as_ref().unwrap().range, location.source_range.start..location.source_range.start);
            let editor_id = dialog.test_editor_id.unwrap();
            let range = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap().cursor.char_range().unwrap();
            assert_eq!(usize::from(range.primary.index), expected);
            assert_eq!(range.primary, range.secondary);
            assert!(!dialog.test_highlight_rows.is_empty(), "target visual row is painted after scroll");
            let scroll_id = dialog.test_editor_scroll_id.unwrap();
            assert!(egui::scroll_area::State::load(&ctx, scroll_id).unwrap().offset.y > 0.0);

            let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
            let selection = egui::text::CCursorRange::two(egui::text::CCursor::new(1), egui::text::CCursor::new(3));
            state.cursor.set_char_range(Some(selection));
            state.store(&ctx, editor_id);
            ctx.memory_mut(|memory| memory.request_focus(editor_id));
            let mut raw = input(size, Vec::new());
            raw.time = Some(highlight.expires_at + 0.1);
            let output = ctx.run_ui(raw, |ui| dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true));
            dialog.on_ui_frame_complete(&ctx);
            output.drop_without_applying_deltas();
            assert!(dialog.source_highlight.is_none());
            assert_eq!(egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap().cursor.char_range(), Some(selection));
            let other = egui::Id::new("other-focus");
            ctx.memory_mut(|memory| memory.request_focus(other));
            frame(&mut dialog, &ctx, size, Vec::new());
            assert!(!ctx.memory(|memory| memory.has_focus(editor_id)), "later frames must not steal focus");
            assert_eq!(dialog.test_source(), Some(source.as_str()));
        }
    }

    #[test]
    fn result_navigation_rejects_discarded_stale_and_same_frame_edits() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("# note\r\nx = 2\r\n");
        dialog.session.evaluation();
        let nav = super::super::calculation_dependency_dialog::DependencyNavigation {
            document: dialog.session.current_id().unwrap(),
            incarnation: dialog.session.document_incarnation().unwrap(),
            line: dialog.session.line_id(1).unwrap(),
        };
        let revision = dialog.session.revision().unwrap();
        let mut discarded = dialog.empty_host_effects(egui::ViewportId::ROOT);
        discarded.result_navigation = Some((nav, revision));
        assert!(complete_host_effects(&mut dialog, &ctx, discarded).is_empty());
        assert!(dialog.dependency_navigation.is_none());
        let mut edited = dialog.empty_host_effects(egui::ViewportId::ROOT);
        edited.pass_accepted = true;
        edited.edited_source = Some("# note\r\nx = 3\r\n".into());
        edited.result_navigation = Some((nav, revision));
        assert!(complete_host_effects(&mut dialog, &ctx, edited).is_empty());
        assert!(dialog.dependency_navigation.is_none());
        let mut stale = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale.pass_accepted = true;
        stale.result_navigation = Some((nav, revision));
        assert!(complete_host_effects(&mut dialog, &ctx, stale).is_empty());
        assert!(dialog.dependency_navigation.is_none());
        assert!(dialog.source_highlight.is_none());
    }

    #[test]
    fn result_navigation_is_not_applied_during_host_transition() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("x = 2");
        dialog.session.evaluation();
        let nav = super::super::calculation_dependency_dialog::DependencyNavigation {
            document: dialog.session.current_id().unwrap(),
            incarnation: dialog.session.document_incarnation().unwrap(),
            line: dialog.session.line_id(0).unwrap(),
        };
        let revision = dialog.session.revision().unwrap();
        let identity = dialog.host_identity();
        dialog.dependency_navigation = Some(super::SourceNavigation { host: identity, navigation: nav, revision, result_navigation: true, selection: None, focus_source: true });
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.host_request = Some(super::PadHost::Popout);
        host.dependency_applied = Some(super::DependencyEditorUpdate {
            navigation: nav,
            revision,
            editor_id: egui::Id::new("calculation-pad-source"),
            editor_state: egui::widgets::text_edit::TextEditState::default(),
            scroll_id: egui::Id::new("test-scroll"),
            scroll_state: egui::scroll_area::State::default(),
            result_navigation: true,
            source_byte_offset: 0,
            selection: None,
            focus_source: true,
        });
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(dialog.source_highlight.is_none());
        assert!(dialog.dependency_navigation.is_none());
        assert!(dialog.inspector_context().caret.is_none());
    }

    fn value(session: &mut CalculationPadSession, line: usize) -> f64 {
        match &session.evaluation().unwrap().lines[line].outcome {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar result, got {other:?}"),
        }
    }

    #[test]
    fn blank_save_opens_save_as_without_reporting_a_saved_document() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();

        dialog.apply_intent(DialogIntent::Save);

        assert!(dialog.save_as_open);
        assert_eq!(dialog.notice, None);
        assert!(dialog.session.list_saved_documents().unwrap().is_empty());
    }

    #[test]
    fn discarded_first_ui_pass_does_not_apply_editor_text_or_consume_focus() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("initial = 1");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        dialog.open();

        let (effect, saw_first_ui_pass) = frame_requesting_discard(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("discarded edit".into()),
            ],
        );
        assert!(saw_first_ui_pass, "the regression requested a real discard");
        assert_eq!(effect, None);
        assert_eq!(dialog.test_source(), Some("initial = 1"));
        assert!(
            dialog.focus_source,
            "discarded pass must retain pending focus"
        );

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        assert!(ctx.memory(|memory| memory.has_focus(editor_id)));
        assert_eq!(dialog.test_source(), Some("initial = 1"));
    }

    #[test]
    fn splitter_retains_discarded_motion_and_move_coalesced_with_release() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(1100.0, 850.0);
        dialog.open();
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let start = dialog.test_splitter.unwrap().1.center();
        frame(&mut dialog, &ctx, size, click_events(start, true));
        assert!(dialog.split_preview.is_some());
        let discarded = start + egui::vec2(45.0, 0.0);
        frame_requesting_late_discard(&mut dialog, &ctx, size, vec![egui::Event::PointerMoved(discarded)]);
        assert_eq!(dialog.display_preferences.wide_source_ratio, 0.5);
        let release = start + egui::vec2(95.0, 0.0);
        let effect = frame(&mut dialog, &ctx, size, click_events(release, false));
        assert!(matches!(effect, Some(CalculationPadDialogEffect::SaveDisplayPreferences(_))));
        assert!(dialog.display_preferences.wide_source_ratio > 0.55);
        assert!(dialog.split_preview.is_none());
        assert_eq!(frame(&mut dialog, &ctx, size, Vec::new()), None);
    }

    #[test]
    fn splitters_drag_in_both_layouts_and_save_once_on_release() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let wide = egui::vec2(1100.0, 850.0);
        dialog.open();
        dialog.test_replace_source("x = 2\nx * 4");
        for _ in 0..3 { frame(&mut dialog, &ctx, wide, Vec::new()); }
        let before = (dialog.session.revision(), dialog.session.is_dirty(), dialog.session.can_undo(), dialog.session.can_redo());
        let (orientation, rect) = dialog.test_splitter.unwrap();
        assert_eq!(orientation, super::SplitOrientation::Wide);
        let start = rect.center();
        assert_eq!(frame(&mut dialog, &ctx, wide, click_events(start, true)), None);
        assert_eq!(frame(&mut dialog, &ctx, wide, vec![egui::Event::PointerMoved(start + egui::vec2(90.0, 0.0))]), None);
        assert!(dialog.split_preview.is_some());
        assert_eq!(dialog.display_preferences.wide_source_ratio, 0.5);
        frame(&mut dialog, &ctx, wide, Vec::new());
        assert!(dialog.test_splitter.unwrap().1.center().x > start.x + 20.0);
        let release = start + egui::vec2(90.0, 0.0);
        assert!(matches!(frame(&mut dialog, &ctx, wide, click_events(release, false)), Some(CalculationPadDialogEffect::SaveDisplayPreferences(_))));
        assert!(dialog.split_preview.is_none());
        let wide_ratio = dialog.display_preferences.wide_source_ratio;
        assert!(wide_ratio > 0.5);
        assert_eq!(dialog.display_preferences.stacked_source_ratio, 0.48);
        assert_eq!(frame(&mut dialog, &ctx, wide, Vec::new()), None);

        let narrow = egui::vec2(590.0, 850.0);
        for _ in 0..4 { frame(&mut dialog, &ctx, narrow, Vec::new()); }
        let (orientation, rect) = dialog.test_splitter.unwrap();
        assert_eq!(orientation, super::SplitOrientation::Stacked);
        let start = rect.center();
        assert_eq!(frame(&mut dialog, &ctx, narrow, click_events(start, true)), None);
        assert_eq!(frame(&mut dialog, &ctx, narrow, vec![egui::Event::PointerMoved(start + egui::vec2(0.0, 60.0))]), None);
        frame(&mut dialog, &ctx, narrow, Vec::new());
        assert!(dialog.test_splitter.unwrap().1.center().y > start.y + 10.0);
        let release = start + egui::vec2(0.0, 60.0);
        assert!(matches!(frame(&mut dialog, &ctx, narrow, click_events(release, false)), Some(CalculationPadDialogEffect::SaveDisplayPreferences(_))));
        assert!(dialog.display_preferences.stacked_source_ratio > 0.48);
        assert_eq!(dialog.display_preferences.wide_source_ratio, wide_ratio);
        assert_eq!(dialog.test_source(), Some("x = 2\nx * 4"));
        assert_eq!((dialog.session.revision(), dialog.session.is_dirty(), dialog.session.can_undo(), dialog.session.can_redo()), before);
    }

    #[test]
    fn split_preview_rejects_retired_passes_and_clamps_extremes() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        let mut rejected = dialog.empty_host_effects(egui::ViewportId::ROOT);
        let preview = super::SplitPreview { host: dialog.host_identity(), orientation: super::SplitOrientation::Wide, ratio: 0.7, start_ratio: 0.5, press_position: 0.0 };
        rejected.split_action = Some(super::SplitAction::Preview(preview));
        assert!(complete_host_effects(&mut dialog, &ctx, rejected).is_empty());
        assert!(dialog.split_preview.is_none());
        let mut accepted = dialog.empty_host_effects(egui::ViewportId::ROOT);
        accepted.pass_accepted = true;
        accepted.split_action = Some(super::SplitAction::Preview(preview));
        assert!(complete_host_effects(&mut dialog, &ctx, accepted).is_empty());
        assert!(dialog.split_preview.is_some());
        let old_host = dialog.host_identity();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);
        assert!(dialog.split_preview.is_none());
        let mut retired = dialog.empty_host_effects(egui::ViewportId::ROOT);
        retired.identity = old_host;
        retired.pass_accepted = true;
        retired.split_action = Some(super::SplitAction::Finish(preview));
        assert!(complete_host_effects(&mut dialog, &ctx, retired).is_empty());
        assert_eq!(dialog.display_preferences.wide_source_ratio, 0.5);
        for ratio in [f32::NAN, f32::NEG_INFINITY, -999.0, 999.0] {
            let (first, second, valid) = super::split_dimensions(1_000_000.0, ratio, 220.0, 0.5);
            assert!(first.is_finite() && second.is_finite());
            assert!((0.05..=0.95).contains(&valid));
        }
    }

    #[test]
    fn display_toggle_preserves_selection_only_when_editor_was_focused() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        dialog.open();
        dialog.test_replace_source("amount = 12345");
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let editor_id = dialog.test_editor_id.unwrap();
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, size, Vec::new());
        let selection = egui::text::CCursorRange::two(egui::text::CCursor::new(2), egui::text::CCursor::new(8));
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap();
        state.cursor.set_char_range(Some(selection));
        state.store(&ctx, editor_id);
        let toggle = dialog.test_display_controls.unwrap().0;
        click(&mut dialog, &ctx, size, toggle.center());
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(editor_id)));
        assert_eq!(egui::widgets::text_edit::TextEditState::load(&ctx, editor_id).unwrap().cursor.char_range(), Some(selection));
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("other-widget")));
        let toggle = dialog.test_display_controls.unwrap().1;
        click(&mut dialog, &ctx, size, toggle.center());
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(!ctx.memory(|memory| memory.has_focus(editor_id)));
        assert_eq!(dialog.test_source(), Some("amount = 12345"));
    }

    #[test]
    fn display_controls_preserve_document_and_follow_galley_rows() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        let source = format!("{{雪}} = {}\r\n# comment\r\n\r\n", "1 + ".repeat(80) + "1");
        dialog.open();
        dialog.test_replace_source(&source);
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let before = (dialog.session.revision(), dialog.session.is_dirty(), dialog.session.can_undo(), dialog.session.can_redo());
        let (width, viewport, content, rows) = dialog.test_editor_geometry.clone().unwrap();
        assert!(width > viewport, "unwrapped galley must overflow its scroll viewport");
        assert!(content > viewport, "scroll area must expose horizontal overflow");
        assert_eq!(rows.iter().map(|(line, _)| *line).collect::<Vec<_>>(), vec![1, 2, 3, 4]);

        let scroll_id = dialog.test_editor_scroll_id.unwrap();
        let mut scroll_state = egui::scroll_area::State::load(&ctx, scroll_id).unwrap();
        scroll_state.offset.x = 100.0;
        scroll_state.store(&ctx, scroll_id);
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(egui::scroll_area::State::load(&ctx, scroll_id).unwrap().offset.x > 0.0);

        let wrap = dialog.test_display_controls.unwrap().1;
        assert!(matches!(click(&mut dialog, &ctx, size, wrap.center()), Some(CalculationPadDialogEffect::SaveDisplayPreferences(_))));
        for _ in 0..2 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let (wrapped_width, wrapped_viewport, _, wrapped_rows) = dialog.test_editor_geometry.clone().unwrap();
        assert!(wrapped_width <= wrapped_viewport + 1.0);
        assert_eq!(egui::scroll_area::State::load(&ctx, scroll_id).unwrap().offset.x, 0.0);
        assert_eq!(wrapped_rows.iter().map(|(line, _)| *line).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        assert!(wrapped_rows[1].1 - wrapped_rows[0].1 > rows[1].1 - rows[0].1);
        let numbers = dialog.test_display_controls.unwrap().0;
        assert!(matches!(click(&mut dialog, &ctx, size, numbers.center()), Some(CalculationPadDialogEffect::SaveDisplayPreferences(_))));
        assert!(!dialog.display_preferences.line_numbers);
        assert_eq!(dialog.test_source(), Some(source.as_str()));
        assert_eq!((dialog.session.revision(), dialog.session.is_dirty(), dialog.session.can_undo(), dialog.session.can_redo()), before);
    }

    #[test]
    fn display_preferences_publish_only_after_retained_host_pass() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        let changed = crate::settings::CalculationPadDisplayPreferences { line_numbers: false, word_wrap: true, ..Default::default() };
        let mut rejected = dialog.empty_host_effects(egui::ViewportId::ROOT);
        rejected.display_preferences = Some(changed);
        assert!(complete_host_effects(&mut dialog, &ctx, rejected).is_empty());
        assert_ne!(dialog.display_preferences, changed);
        let mut accepted = dialog.empty_host_effects(egui::ViewportId::ROOT);
        accepted.pass_accepted = true;
        accepted.display_preferences = Some(changed);
        assert_eq!(complete_host_effects(&mut dialog, &ctx, accepted), vec![CalculationPadDialogEffect::SaveDisplayPreferences(changed)]);
        assert_eq!(dialog.display_preferences, changed);
    }

    #[test]
    fn editor_updates_dependencies_errors_and_session_owned_keyboard_undo_redo() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        dialog.open();
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![egui::Event::Text("amount = 2\namount * 3".into())],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount * 3".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        frame(
            &mut dialog,
            &ctx,
            size,
            vec![key(egui::Key::Y, egui::Modifiers::COMMAND)],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount / 0".into()),
            ],
        );
        assert!(matches!(
            dialog.session.evaluation().unwrap().lines[1].outcome,
            LineOutcome::Error(_)
        ));
    }

    #[test]
    fn keyboard_undo_survives_modifier_release_later_in_the_same_frame() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        dialog.test_replace_source("amount = 5\namount * 3");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::Z, egui::Modifiers::COMMAND),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);
    }

    #[test]
    fn results_hide_comments_and_blanks_without_changing_physical_identity() {
        let (_directory, _root, mut dialog) = fixture();
        let source = "# inputs\r\nx = 2\r\n\r\n// ignored = missing\r\nr2 * 4\r\n0\r\nbad =\r\n";
        dialog.open();
        dialog.test_replace_source(source);
        let ids = dialog.session.line_ids().unwrap().to_vec();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        assert_eq!(
            dialog.test_rendered_result_rows,
            vec![(1, ids[1]), (4, ids[4]), (5, ids[5]), (6, ids[6])]
        );
        assert_eq!(dialog.test_splitter.unwrap().0, super::SplitOrientation::Wide);
        let row_label = dialog.test_result_row(5).unwrap();
        let value_label = dialog.test_result_bodies.iter()
            .find(|(index, _, _)| *index == 5)
            .map(|(_, _, text)| *text)
            .unwrap();
        assert!(value_label.left() > row_label.right());
        let (line_numbers, word_wrap) = dialog.test_display_controls.unwrap();
        assert!(word_wrap.left() > line_numbers.right());
        assert_eq!(dialog.session.line_ids().unwrap(), ids);
        let evaluation = dialog.session.evaluation().unwrap();
        assert_eq!(evaluation.lines.len(), 8);
        assert_eq!(evaluation.lines[4].dependency_indices, vec![1]);
        for (index, expected) in [(1, "2"), (4, "8"), (5, "0")] {
            let LineOutcome::Value(value) = &evaluation.lines[index].outcome else {
                panic!("expected a value on physical line {}", index + 1);
            };
            assert_eq!(value.formatted_result().as_deref(), Some(expected));
        }
        assert!(matches!(evaluation.lines[6].outcome, LineOutcome::Error(_)));
        let summary = copy_all_session(&mut dialog.session);
        assert!(summary.contains("# inputs\r\nr2 = x = 2 = 2\r\n\r\n// ignored = missing\r\nr5 = r2 * 4 = 8\r\nr6 = 0 = 0\r\nr7 = bad = = error:"));
        assert!(summary.contains("Results only\nr2 = 2\nr5 = 8\nr6 = 0\nr7 = error:"));
        assert!(summary.contains("# inputs\r\nx = 2 = 2\r\n\r\n// ignored = missing\r\nr2 * 4 = 8"));
        assert_eq!(dialog.test_source(), Some(source));
    }

    #[test]
    fn explicit_copy_buttons_return_single_result_and_source_order_summary() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("total = 2 + 3\nr1 * 4\n1 / 0");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        frame(&mut dialog, &ctx, size, Vec::new());
        for _ in 0..2 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let line_copy = dialog
            .test_line_copy_rects
            .iter()
            .find(|(line, _)| *line == 1)
            .map(|(_, rect)| *rect)
            .expect("a result row exposes an explicit copy button");
        assert_eq!(
            click(&mut dialog, &ctx, size, line_copy.center()),
            Some(CalculationPadDialogEffect::CopyText("20".into()))
        );

        let all_copy = dialog
            .test_copy_all_rect
            .expect("results toolbar exposes explicit Copy all");
        let Some(CalculationPadDialogEffect::CopyText(summary)) =
            click(&mut dialog, &ctx, size, all_copy.center())
        else {
            panic!("Copy all returns a clipboard effect only after an explicit click");
        };
        assert!(summary.contains("r1 = total = 2 + 3 = 5\nr2 = r1 * 4 = 20\nr3 = 1 / 0 = error:"));
        assert!(summary.contains("Results only\nr1 = 5\nr2 = 20\nr3 = error:"));
        assert_eq!(dialog.test_source(), Some("total = 2 + 3\nr1 * 4\n1 / 0"));
    }

    #[test]
    fn vector_copy_buttons_return_pasteable_line_and_summary() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("v = [1, 2, 3]\nv * 2");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let line_copy = dialog
            .test_line_copy_rects
            .iter()
            .find(|(line, _)| *line == 1)
            .map(|(_, rect)| *rect)
            .expect("vector row exposes copy");
        assert_eq!(
            click(&mut dialog, &ctx, size, line_copy.center()),
            Some(CalculationPadDialogEffect::CopyText("[2, 4, 6]".into()))
        );
        let all_copy = dialog.test_copy_all_rect.expect("copy all is available");
        let Some(CalculationPadDialogEffect::CopyText(summary)) =
            click(&mut dialog, &ctx, size, all_copy.center())
        else {
            panic!("copy all effect")
        };
        assert!(summary.contains("r1 = v = [1, 2, 3] = [1, 2, 3]"));
        assert!(summary.contains("r2 = [2, 4, 6]"));
        assert!(summary.contains("v * 2 = [2, 4, 6]"));
    }

    #[test]
    fn number_format_changes_presentation_without_reevaluation_or_canonical_copy_changes() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("x = 1000 / 3");
        let document_id = dialog.test_current_document_id().unwrap();
        let line = dialog.session.line_id(0).unwrap();
        let _ = value(&mut dialog.session, 0);
        let count = dialog.session.evaluation_count();
        let revision = dialog.session.revision();
        dialog.display_preferences.number_mode = crate::settings::CalculationPadNumberMode::Fixed;
        dialog.display_preferences.decimal_places = 2;
        dialog.display_preferences.group_digits = true;
        let intent = |target| super::ResultCopyIntent {
            host: dialog.host_identity(), viewport_id: egui::ViewportId::ROOT, document_id, target,
        };
        let canonical = intent(super::ResultCopyTarget::Line(line));
        let formatted = intent(super::ResultCopyTarget::FormattedLine(line));
        assert_eq!(dialog.resolve_result_copy(canonical), Some(CalculationPadDialogEffect::CopyText("333.333".into())));
        assert_eq!(dialog.resolve_result_copy(formatted), Some(CalculationPadDialogEffect::CopyText("333.33".into())));
        assert_eq!(dialog.session.evaluation_count(), count);
        assert_eq!(dialog.session.revision(), revision);
    }

    #[test]
    fn formatted_copy_is_retained_only_for_the_painted_revision() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("x = 1 / 3");
        let document_id = dialog.test_current_document_id().unwrap();
        let line = dialog.session.line_id(0).unwrap();
        let _ = value(&mut dialog.session, 0);
        dialog.display_preferences.number_mode = crate::settings::CalculationPadNumberMode::Fixed;
        dialog.display_preferences.decimal_places = 2;
        let ctx = egui::Context::default();
        let mut accepted = dialog.empty_host_effects(egui::ViewportId::ROOT);
        accepted.pass_accepted = true;
        accepted.result_copy = Some(super::ResultCopyIntent { host: accepted.identity,
            viewport_id: egui::ViewportId::ROOT, document_id, target: super::ResultCopyTarget::FormattedLine(line) });
        assert_eq!(complete_host_effects(&mut dialog, &ctx, accepted), vec![CalculationPadDialogEffect::CopyText("0.33".into())]);
        let mut stale = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale.pass_accepted = true;
        stale.edited_source = Some("x = 2 / 3".into());
        stale.result_copy = Some(super::ResultCopyIntent { host: stale.identity,
            viewport_id: egui::ViewportId::ROOT, document_id, target: super::ResultCopyTarget::FormattedLine(line) });
        assert!(!complete_host_effects(&mut dialog, &ctx, stale).iter().any(|effect| matches!(effect, CalculationPadDialogEffect::CopyText(_))));
    }

    #[test]
    fn accepted_copy_resolves_after_same_frame_source_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        let document_id = dialog.test_current_document_id().unwrap();
        let retained_line = dialog.session.line_id(1).unwrap();
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 1);

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("amount = 5\namount * 3".into());
        host.result_copy = Some(super::ResultCopyIntent {
            host: host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(retained_line),
        });

        assert_eq!(
            complete_host_effects(&mut dialog, &egui::Context::default(), host),
            vec![CalculationPadDialogEffect::CopyText("15".into())]
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(dialog.session.evaluation_count(), 2);
        assert!(dialog.test_is_dirty());
        assert!(dialog.session.undo());
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 3);
        assert!(dialog.session.redo());
        assert_eq!(value(&mut dialog.session, 1), 15.0);
        assert_eq!(dialog.session.evaluation_count(), 4);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("amount = 7\namount * 3".into());
        host.result_copy = Some(super::ResultCopyIntent {
            host: host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::All,
        });
        assert_eq!(
            complete_host_effects(&mut dialog, &egui::Context::default(), host),
            vec![CalculationPadDialogEffect::CopyText(
                "Source and results\nr1 = amount = 7 = 7\nr2 = amount * 3 = 21\n\nResults only\nr1 = 7\nr2 = 21\n\nSource and results without row labels\namount = 7 = 7\namount * 3 = 21".into()
            )]
        );
        assert_eq!(dialog.test_source(), Some("amount = 7\namount * 3"));
        assert_eq!(dialog.session.evaluation_count(), 5);
        assert!(dialog.session.undo());
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
    }

    #[test]
    fn retired_line_copy_is_refused_after_same_frame_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("1 + 2\n3 + 4");
        let document_id = dialog.test_current_document_id().unwrap();
        let surviving_line = dialog.session.line_id(0).unwrap();
        let retired_line = dialog.session.line_id(1).unwrap();

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("1 + 2".into());
        host.result_copy = Some(super::ResultCopyIntent {
            host: host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(retired_line),
        });

        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), host).is_empty());
        assert_eq!(dialog.test_source(), Some("1 + 2"));
        assert_eq!(dialog.session.line_id(0), Some(surviving_line));
        assert_ne!(dialog.session.line_id(0), Some(retired_line));
    }

    #[test]
    fn remembered_scratch_close_saves_same_frame_edit_before_reopen() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("initial");
        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("final λ\n42".into());
        host.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, host);
        assert!(!dialog.open);
        assert_eq!(dialog.test_pending_transition(), None);
        assert_eq!(dialog.recovery_writer.load().unwrap().unwrap().source, "final λ\n42");
        dialog.open();
        assert_eq!(dialog.test_source(), Some("final λ\n42"));
    }

    #[test]
    fn remembered_scratch_failed_recovery_cancels_native_close() {
        let (_directory, root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(1000.0, 760.0);
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("keep this draft");
        for _ in 0..3 { frame(&mut dialog, &ctx, size, Vec::new()); }
        let popout_button = dialog.test_host_button_rect.unwrap();
        click(&mut dialog, &ctx, size, popout_button.center());
        for _ in 0..3 { immediate_viewport_frame(&mut dialog, &ctx, Vec::new()); }
        let store = PadRecoveryStore::new(&root);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        assert!(dialog.recovery_writer.flush().unwrap().is_empty());
        match std::fs::remove_file(store.path()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove fixture recovery file: {error}"),
        }
        std::fs::create_dir(store.path()).unwrap();
        super::super::render::with_immediate_viewport_test_close_request(
            super::pad_viewport_id(), || immediate_viewport_frame(&mut dialog, &ctx, Vec::new()),
        );
        let commands = immediate_viewport_frame(&mut dialog, &ctx, Vec::new());
        assert!(commands.iter().any(|command| matches!(command, egui::ViewportCommand::CancelClose)));
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("keep this draft"));
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(dialog.error.as_deref().is_some_and(|error| error.contains("recovery failed")));
    }

    #[test]
    fn save_and_close_follow_an_accepted_same_frame_source_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Accepted edit");
        let document_id = dialog.test_current_document_id().unwrap();

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("amount = 9\namount * 2".into());
        host.intent = Some(DialogIntent::Save);
        host.close_requested = true;

        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), host).is_empty());
        assert!(!dialog.open);
        assert_eq!(dialog.session.source(), None);
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some("amount = 9\namount * 2".into()),
            "same-frame edit is captured before Save and close retire the session"
        );

        dialog.test_open_document(document_id);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
        assert!(!dialog.test_is_dirty());
    }

    #[test]
    fn pad_recovery_preference_waits_for_an_accepted_pass_and_settings_commit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();

        let mut rejected = dialog.empty_host_effects(egui::ViewportId::ROOT);
        rejected.intent = Some(DialogIntent::SetRememberSource(true));
        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), rejected).is_empty());
        assert!(!dialog.remember_source);

        let mut accepted = dialog.empty_host_effects(egui::ViewportId::ROOT);
        accepted.pass_accepted = true;
        accepted.intent = Some(DialogIntent::SetRememberSource(true));
        assert_eq!(
            complete_host_effects(&mut dialog, &egui::Context::default(), accepted),
            vec![CalculationPadDialogEffect::SaveRecoveryPreference(true)]
        );
        assert!(
            !dialog.remember_source,
            "the runtime preference changes only after the settings-save acknowledgement"
        );
    }

    #[test]
    fn accepted_transition_intents_preserve_or_clear_recovery_at_lifecycle_boundaries() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("persisted draft");
        dialog.save_recovery_now(None).unwrap();

        let accept_intent = |dialog: &mut CalculationPadDialog, intent| {
            let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
            host.pass_accepted = true;
            host.intent = Some(intent);
            complete_host_effects(dialog, &ctx, host)
        };

        accept_intent(&mut dialog, DialogIntent::New);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::New)
        );
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Cancel),
        );
        assert_eq!(dialog.test_source(), Some("persisted draft"));
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some("persisted draft".into()),
            "Cancel retains the prior recoverable draft"
        );

        accept_intent(&mut dialog, DialogIntent::New);
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Discard),
        );
        assert_eq!(dialog.test_source(), Some(""));
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some(String::new()),
            "Discard before New replaces the prior source with a blank recovery record"
        );

        dialog.test_replace_source("queued close discard draft");
        dialog.schedule_recovery_snapshot(&ctx);
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(!dialog.open);
        assert_eq!(dialog.recovery_writer.load().unwrap().unwrap().source, "queued close discard draft");

        dialog.open();
        assert_eq!(dialog.test_source(), Some("queued close discard draft"));
        dialog.test_replace_source("queued disable draft");
        dialog.schedule_recovery_snapshot(&ctx);
        dialog.apply_committed_recovery_preference(false).unwrap();
        assert!(!dialog.remember_source);
        assert_eq!(dialog.recovery_writer.load().unwrap(), None);

        let mut close_disabled = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close_disabled.pass_accepted = true;
        close_disabled.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close_disabled);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Discard),
        );
        dialog.apply_committed_recovery_preference(true).unwrap();
        dialog.open();
        assert_eq!(dialog.test_source(), Some(""));
        assert_eq!(dialog.recovery_writer.load().unwrap(), None);
    }

    #[test]
    fn delayed_old_document_recovery_failure_is_not_published_on_the_new_document() {
        let (_directory, root, mut dialog) = fixture();
        let store = PadRecoveryStore::new(&root);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let release_receiver = std::sync::Arc::new(std::sync::Mutex::new(release_receiver));
        dialog
            .recovery_writer
            .set_before_write_hook(std::sync::Arc::new(move || {
                started_sender.send(()).unwrap();
                release_receiver.lock().unwrap().recv().unwrap();
            }));
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("old document draft");
        let old_document = dialog.test_current_document_id().unwrap();
        let old_revision = dialog.session.revision().unwrap();
        std::fs::create_dir(store.path()).unwrap();
        dialog.schedule_recovery_snapshot(&egui::Context::default());

        started_receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("the background recovery write reached its delayed section");
        dialog.test_apply_save_as("new document identity");
        let new_document = dialog.test_current_document_id().unwrap();
        assert_ne!(new_document, old_document);
        release_sender.send(()).unwrap();

        let failures = dialog.recovery_writer.flush().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].document_id, old_document);
        assert_eq!(failures[0].revision, old_revision);
        assert!(!failures[0].message.is_empty());
        dialog.publish_recovery_failures(failures);
        assert!(
            dialog
                .error
                .as_deref()
                .is_none_or(|message| !message.contains("recovery failed")),
            "a delayed failure for the old identity must not be shown on the new document"
        );
    }

    #[test]
    fn recovery_load_is_lazy_until_explicit_open_and_preserves_a_live_draft_on_error() {
        let (_directory, root, mut dialog) = fixture();
        let mut saved_session = CalculationPadSession::new(&root);
        saved_session.replace_source("seed").unwrap();
        saved_session.save_as("identity").unwrap();
        let saved_id = saved_session.current_id().unwrap();
        let mut writer = PadRecoveryWriter::new(PadRecoveryStore::new(&root));
        writer
            .save_now(PadRecoverySnapshot {
                document_id: saved_id,
                revision: 7,
                source: "π = 2\r\nπ * 3\r\n".into(),
            })
            .unwrap();
        writer.shutdown().unwrap();

        dialog.initialize_recovery_preference(true);
        dialog.ensure_open();
        assert_eq!(dialog.test_source(), Some(""));
        dialog.open();
        assert_eq!(dialog.test_source(), Some("π = 2\r\nπ * 3\r\n"));
        assert!(dialog.test_is_dirty());
        assert!(!dialog.test_can_undo());

        let malformed_store = PadRecoveryStore::new(&root);
        std::fs::write(malformed_store.path(), b"not-json").unwrap();
        dialog.recovery_attempted = false;
        dialog.open();
        assert_eq!(dialog.test_source(), Some("π = 2\r\nπ * 3\r\n"));
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|message| message.contains("recovery failed"))
        );
        dialog.apply_committed_recovery_preference(false).unwrap();
        assert!(!malformed_store.path().exists());
        assert_eq!(saved_session.source(), Some("seed"));
    }

    #[test]
    fn explicit_examples_and_dock_requests_emit_show_root_only_after_host_acceptance() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();

        let mut examples = dialog.empty_host_effects(egui::ViewportId::ROOT);
        examples.pass_accepted = true;
        examples.intent = Some(DialogIntent::OpenExamples);
        assert_eq!(
            complete_host_effects(&mut dialog, &ctx, examples),
            vec![CalculationPadDialogEffect::ShowRoot]
        );
        assert!(dialog.examples_open);
        assert!(dialog.needs_root_ui_when_hidden());

        let retired_docked_identity = dialog.host_identity();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);
        let mut stale_dock = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_dock.pass_accepted = true;
        stale_dock.identity = retired_docked_identity;
        stale_dock.host_request = Some(super::PadHost::Docked);
        assert!(complete_host_effects(&mut dialog, &ctx, stale_dock).is_empty());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);

        let mut dock = dialog.empty_host_effects(egui::ViewportId::ROOT);
        dock.pass_accepted = true;
        dock.host_request = Some(super::PadHost::Docked);
        assert_eq!(
            complete_host_effects(&mut dialog, &ctx, dock),
            vec![CalculationPadDialogEffect::ShowRoot]
        );
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);

        dialog.examples_open = false;
        let mut stale_examples = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_examples.pass_accepted = true;
        stale_examples.identity.generation = stale_examples.identity.generation.wrapping_add(1);
        stale_examples.intent = Some(DialogIntent::OpenExamples);
        assert!(complete_host_effects(&mut dialog, &ctx, stale_examples).is_empty());
        assert!(!dialog.examples_open);
    }

    #[test]
    fn retained_native_pad_opens_reference_and_invalidates_old_inspector_origin() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);

        let mut inspector = dialog.empty_host_effects(egui::ViewportId::ROOT);
        inspector.pass_accepted = true;
        inspector.open_inspector = true;
        let effects = complete_host_effects(&mut dialog, &ctx, inspector);
        let receipt = match effects.into_iter().next().unwrap() {
            CalculationPadDialogEffect::OpenInspector(receipt) => receipt,
            other => panic!("unexpected effect: {other:?}"),
        };
        assert!(dialog.accepts_inspector_origin(&receipt));

        let mut examples = dialog.empty_host_effects(egui::ViewportId::ROOT);
        examples.pass_accepted = true;
        examples.intent = Some(DialogIntent::OpenExamples);
        assert!(complete_host_effects(&mut dialog, &ctx, examples).is_empty());
        assert_eq!(dialog.examples_host.presentation(), super::PadHost::Popout);
        assert!(dialog.examples_layer_if_root().is_none());

        dialog.apply_host_request(&ctx, super::PadHost::Docked);
        assert!(!dialog.accepts_inspector_origin(&receipt));
    }

    #[test]
    fn inspector_origin_rejects_closed_replaced_and_modal_parent() {
        fn accepted(dialog: &mut CalculationPadDialog, ctx: &egui::Context) -> super::PadOpenReceipt {
            let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
            host.pass_accepted = true;
            host.open_inspector = true;
            match complete_host_effects(dialog, ctx, host).into_iter().next().unwrap() {
                CalculationPadDialogEffect::OpenInspector(receipt) => receipt,
                other => panic!("unexpected effect: {other:?}"),
            }
        }

        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        let old_document = accepted(&mut dialog, &ctx);
        dialog.apply_intent(DialogIntent::New);
        assert!(!dialog.accepts_inspector_origin(&old_document));

        let before_close = accepted(&mut dialog, &ctx);
        dialog.apply_close_request();
        assert!(!dialog.open);
        assert!(!dialog.accepts_inspector_origin(&before_close));

        dialog.open();
        let before_chooser = accepted(&mut dialog, &ctx);
        dialog.apply_intent(DialogIntent::OpenChooser);
        assert!(!dialog.accepts_inspector_origin(&before_chooser));
        dialog.apply_intent(DialogIntent::HideChooser);
        assert!(dialog.accepts_inspector_origin(&before_chooser));
        dialog.apply_intent(DialogIntent::OpenSaveAs);
        assert!(!dialog.accepts_inspector_origin(&before_chooser));
        dialog.apply_intent(DialogIntent::CancelSaveAs);
        dialog.test_replace_source("unsaved");
        let before_decision = accepted(&mut dialog, &ctx);
        dialog.apply_intent(DialogIntent::New);
        assert!(dialog.session.pending_transition().is_some());
        assert!(!dialog.accepts_inspector_origin(&before_decision));
    }

    #[test]
    fn rejected_child_or_retired_host_effect_does_not_apply_source_or_copy() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount * 2");
        let document_id = dialog.test_current_document_id().unwrap();
        let line_id = dialog.session.line_id(1).unwrap();
        let original_source = dialog.test_source().unwrap().to_owned();
        let ctx = egui::Context::default();
        let retired_host_identity = dialog.host_identity();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);

        let mut child_discard = dialog.empty_host_effects(super::pad_viewport_id());
        child_discard.pass_accepted = false;
        child_discard.edited_source = Some("amount = 5\namount * 2".into());
        child_discard.native_close_requested = true;
        child_discard.result_copy = Some(super::ResultCopyIntent {
            host: child_discard.identity,
            viewport_id: super::pad_viewport_id(),
            document_id,
            target: super::ResultCopyTarget::Line(line_id),
        });
        assert!(complete_host_effects(&mut dialog, &ctx, child_discard).is_empty());
        assert_eq!(dialog.test_source(), Some(original_source.as_str()));
        assert!(dialog.open);

        let mut stale_host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_host.pass_accepted = true;
        stale_host.identity = retired_host_identity;
        stale_host.edited_source = Some("amount = 8\namount * 2".into());
        stale_host.result_copy = Some(super::ResultCopyIntent {
            host: stale_host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(line_id),
        });
        assert!(complete_host_effects(&mut dialog, &ctx, stale_host).is_empty());
        assert_eq!(dialog.test_source(), Some(original_source.as_str()));
        assert!(dialog.open);
    }

    #[test]
    fn studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);
        let initial_size = egui::vec2(1000.0, 760.0);
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 1);
        assert_eq!(
            dialog.session.request_new().unwrap(),
            crate::calculation_pad::TransitionOutcome::DecisionRequired
        );

        for _ in 0..3 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        let editor_id = dialog
            .test_editor_id
            .expect("the source editor is rendered");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, initial_size, Vec::new());
        dialog.test_open_examples();
        assert!(dialog.needs_root_ui_when_hidden());
        frame(&mut dialog, &ctx, initial_size, Vec::new());
        let editor_center = dialog
            .test_editor_rect
            .expect("source editor rect")
            .center();
        frame(
            &mut dialog,
            &ctx,
            initial_size,
            vec![
                egui::Event::PointerMoved(editor_center),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -60.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut dialog, &ctx, egui::vec2(1280.0, 900.0), Vec::new());
        dialog.examples_open = false;
        frame(&mut dialog, &ctx, initial_size, Vec::new());

        let state_before = (
            dialog.session.current_id(),
            dialog.session.current_name().map(str::to_owned),
            dialog.session.source().map(str::to_owned),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.line_ids().unwrap().to_vec(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
            dialog.session.pending_transition(),
            dialog.session.evaluation_count(),
            copy_all_session(&mut dialog.session),
        );
        let popout_button = dialog
            .test_host_button_rect
            .expect("the docked host offers Pop out");
        click(&mut dialog, &ctx, initial_size, popout_button.center());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);
        assert!(dialog.needs_root_ui_when_hidden());

        for _ in 0..3 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(egui::ViewportId::ROOT)
        );
        assert!(dialog.host_state.embedded_root_layer().is_some());
        assert!(!dialog.host_state.bootstrap_pending());
        assert!(dialog.needs_root_ui_when_hidden());
        let dock_button = dialog
            .test_host_button_rect
            .expect("the embedded popout offers Dock back");
        assert_eq!(
            click(&mut dialog, &ctx, initial_size, dock_button.center()),
            Some(CalculationPadDialogEffect::ShowRoot)
        );
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);
        assert!(!dialog.needs_root_ui_when_hidden());

        for _ in 0..2 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        let state_after = (
            dialog.session.current_id(),
            dialog.session.current_name().map(str::to_owned),
            dialog.session.source().map(str::to_owned),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.line_ids().unwrap().to_vec(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
            dialog.session.pending_transition(),
            dialog.session.evaluation_count(),
            copy_all_session(&mut dialog.session),
        );
        assert_eq!(state_after, state_before);
    }

    #[test]
    fn native_popout_child_discard_and_os_close_keep_the_edited_session() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(1000.0, 760.0);
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let popout_button = dialog
            .test_host_button_rect
            .expect("the docked pad offers a popout action");
        click(&mut dialog, &ctx, size, popout_button.center());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);
        for _ in 0..3 {
            immediate_viewport_frame(&mut dialog, &ctx, Vec::new());
        }
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(super::pad_viewport_id())
        );
        assert!(!dialog.host_state.bootstrap_pending());
        assert!(dialog.needs_root_ui_when_hidden());
        let editor_id = dialog
            .test_editor_id
            .expect("native child renders the editor");
        let editor_center = dialog
            .test_editor_rect
            .expect("native child publishes the editor rect")
            .center();
        assert!(dialog.test_editor_clip.unwrap().contains(editor_center), "native editor center {editor_center:?} is outside clip {:?}", dialog.test_editor_clip);
        assert!(!dialog.test_result_bodies.iter().any(|(_, rect, _)| rect.contains(editor_center)), "result row overlaps native editor center {editor_center:?}: {:?}", dialog.test_result_bodies);
        click_immediate_child(&mut dialog, &ctx, editor_center);
        assert_eq!(
            super::super::render::immediate_viewport_test_focused_id(),
            Some((super::pad_viewport_id(), Some(editor_id)))
        );

        immediate_child_frame(
            &mut dialog,
            &ctx,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount * 3".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);
        let accepted_revision = dialog.session.revision();
        let accepted_evaluations = dialog.session.evaluation_count();

        dialog.test_request_child_discard = true;
        immediate_child_frame(
            &mut dialog,
            &ctx,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 9\namount * 3".into()),
            ],
        );
        assert!(dialog.test_child_discard_requested);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(dialog.session.revision(), accepted_revision);
        assert_eq!(dialog.session.evaluation_count(), accepted_evaluations);

        let close_pass_commands = super::super::render::with_immediate_viewport_test_close_request(
            super::pad_viewport_id(),
            || immediate_viewport_frame(&mut dialog, &ctx, Vec::new()),
        );
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(super::pad_viewport_id())
        );
        assert!(dialog.open);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert!(
            !close_pass_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );

        let cancel_close_commands = immediate_viewport_frame(&mut dialog, &ctx, Vec::new());
        assert!(
            cancel_close_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        let keep_editing = dialog
            .test_keep_editing_rect
            .expect("native close decision offers Keep Editing");
        click_immediate_child(&mut dialog, &ctx, keep_editing.center());
        assert!(dialog.open);
        assert_eq!(dialog.test_pending_transition(), None);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert!(dialog.test_is_dirty());
    }

    #[test]
    fn studio_usability_calculation_help_open_copy_preserves_draft_dirty_and_history() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        let draft = "draft = 7\ndraft * 3";
        dialog.test_replace_source(draft);
        let ctx = egui::Context::default();
        let size = egui::vec2(1200.0, 900.0);

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let examples_button = dialog
            .test_examples_button_rect()
            .expect("the main pad has an Examples & reference action");
        assert_eq!(
            click(&mut dialog, &ctx, size, examples_button.center()),
            Some(CalculationPadDialogEffect::ShowRoot)
        );
        assert!(dialog.examples_open);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        assert!(
            dialog
                .test_examples_window_rect()
                .is_some_and(|rect| rect.is_positive())
        );

        let copy_button = dialog
            .test_examples_copy_rect()
            .expect("the shared catalog exposes a copy button");
        let source_before = dialog.test_source().unwrap().to_owned();
        let revision_before = dialog.session.revision();
        let can_undo_before = dialog.test_can_undo();
        let dirty_before = dialog.test_is_dirty();
        assert!(can_undo_before);
        assert!(dirty_before);

        assert_eq!(
            click(&mut dialog, &ctx, size, copy_button.center()),
            Some(CalculationPadDialogEffect::CopyText(
                "radius = 3\nPI * radius ^ 2".into()
            ))
        );
        assert_eq!(dialog.test_source(), Some(source_before.as_str()));
        assert_eq!(dialog.session.revision(), revision_before);
        assert_eq!(dialog.test_can_undo(), can_undo_before);
        assert_eq!(dialog.test_is_dirty(), dirty_before);
    }

    #[test]
    fn saved_document_reopens_and_dirty_close_write_conflict_keeps_the_draft() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 4\namount * 2");
        dialog.test_apply_save_as("Estimate");
        assert!(!dialog.session.is_dirty());
        let saved_id = dialog.session.current_id().unwrap();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(!dialog.open);

        dialog.open();
        dialog.test_open_document(saved_id);
        assert_eq!(dialog.test_source(), Some("amount = 4\namount * 2"));
        assert_eq!(dialog.session.current_name(), Some("Estimate"));
        dialog.apply_committed_recovery_preference(true).unwrap();

        dialog.test_replace_source("amount = 9\namount * 2");
        let directory = PadDocumentStore::new(&root).directory().to_path_buf();
        let path = directory.join(format!("{:032x}.json", saved_id.as_u128()));
        let mut external = std::fs::read(&path).unwrap();
        external.push(b' ');
        std::fs::write(path, external).unwrap();

        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        dialog.test_resolve_pending(TransitionDecision::Save);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|text| text.contains("changed"))
        );

        dialog.test_resolve_pending(TransitionDecision::Cancel);
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
    }

    #[test]
    fn pad_late_root_discard_keeps_edited_source_and_saved_file_before_save() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Retained");
        let id = dialog.test_current_document_id().unwrap();
        let store = PadDocumentStore::new(&root);
        let path = store
            .directory()
            .join(format!("{:032x}.json", id.as_u128()));
        let saved_bytes = std::fs::read(&path).expect("Save As creates a native document");

        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog
            .test_editor_id
            .expect("the source TextEdit is rendered");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        assert!(dialog.test_editor_has_focus(&ctx));
        let original_source = "amount = 1\namount";
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());
        let discarded_edit = frame_requesting_late_discard(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 2\namount".into()),
            ],
        );
        assert_eq!(discarded_edit, None);
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());
        assert!(dialog.test_can_undo());
        assert!(dialog.session.undo());
        assert_eq!(dialog.test_source(), Some(""));
        assert!(dialog.session.redo());
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());

        frame(&mut dialog, &ctx, size, Vec::new());
        let editor_rect = dialog
            .test_editor_rect
            .expect("a retained frame republishes the enabled source editor");
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(editor_rect.center(), true),
        );
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(editor_rect.center(), false),
        );
        assert!(dialog.test_editor_has_focus(&ctx));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 2\namount".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount"));
        assert!(dialog.test_is_dirty());

        let save = dialog.test_save_button_rect().expect("Save is visible");
        frame(&mut dialog, &ctx, size, click_events(save.center(), true));
        assert_eq!(
            frame_requesting_late_discard(
                &mut dialog,
                &ctx,
                size,
                click_events(save.center(), false),
            ),
            None,
            "the discarded Save action must not escape the ROOT completion boundary"
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount"));
        assert!(dialog.test_is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    }

    #[test]
    fn pad_rejected_source_edit_aborts_same_frame_save_intent() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Retained");
        let id = dialog.test_current_document_id().unwrap();
        let path = PadDocumentStore::new(&root)
            .directory()
            .join(format!("{:032x}.json", id.as_u128()));
        let saved_bytes = std::fs::read(&path).unwrap();

        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        let pass = super::ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        let mut host_effects = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host_effects.pass = Some(pass);
        host_effects.pass_accepted = true;
        host_effects.edited_source = Some("x".repeat(MAX_DOCUMENT_BYTES + 1));
        host_effects.intent = Some(DialogIntent::Save);
        host_effects.close_requested = true;
        dialog.pending_root_effects = Some(super::RootEffects {
            pass,
            help: None,
            host: Some(host_effects),
        });
        let output = ctx.run_ui(input(size, Vec::new()), |_| {});
        assert!(dialog.on_ui_frame_complete(&ctx).is_empty());
        output.drop_without_applying_deltas();

        assert_eq!(dialog.test_source(), Some("amount = 1\namount"));
        assert!(!dialog.test_is_dirty());
        assert!(dialog.open, "rejected source edits cannot apply Close");
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(dialog.error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    }

    #[test]
    fn clicking_discard_in_pending_close_closes_the_rendered_dialog() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("draft = 7");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        dialog.request_close();
        assert!(dialog.open);
        frame(&mut dialog, &ctx, size, Vec::new());
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        for _ in 0..2 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let discard = dialog
            .test_discard_rect
            .expect("pending close renders an actual Discard button");

        assert_eq!(click(&mut dialog, &ctx, size, discard.center()), None);
        assert!(!dialog.open, "Discard applies the pending Close transition");
        assert_eq!(dialog.test_source(), None);
        assert_eq!(dialog.test_pending_transition(), None);

        assert_eq!(frame(&mut dialog, &ctx, size, Vec::new()), None);
        assert!(
            !dialog.test_was_rendered(),
            "the next UI frame does not resurrect the closed Calculation Pad"
        );
    }

    #[test]
    fn collapsed_pad_host_close_is_applied_without_contents_ui() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
            &ctx,
            egui::Id::new("calculation-pad-window").with("collapsing"),
            true,
        );
        collapsing.set_open(false);
        collapsing.store(&ctx);
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(
            dialog.test_editor_id.is_none(),
            "collapsed Pad skips its editor"
        );

        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(!dialog.open);
        assert!(
            dialog.test_was_rendered(),
            "the collapsed title bar was rendered"
        );
    }

    #[test]
    fn constrained_window_keeps_editor_and_result_controls_inside_the_viewport() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("answer = 42\nanswer");
        let ctx = egui::Context::default();
        let size = egui::vec2(500.0, 340.0);
        for _ in 0..4 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let window = dialog
            .test_window_rect
            .expect("dialog has real window geometry");
        let editor = dialog.test_editor_rect.expect("source TextEdit is visible");
        let copy = dialog.test_copy_all_rect.expect("copy remains reachable");
        assert!(screen.contains_rect(window));
        assert!(editor.is_positive());
        assert!(copy.is_positive());
        assert!(screen.contains(editor.center()));
        assert!(screen.contains(copy.center()));
        assert!(window.intersects(editor));
        assert!(window.intersects(copy));
    }

    #[test]
    fn all_results_summary_preserves_physical_line_numbers_and_comments() {
        let mut evaluation = crate::calculation_pad::CalculationPadSession::with_store(
            PadDocumentStore::new(&AppDataRoot::from_path(Path::new("unused-root"))),
        );
        let source = "{雪} = 3\r\n# note\r\n\r\nr1 * 4\r\n1 / 0\r\n";
        evaluation.replace_source(source).unwrap();
        let summary = copy_all_session(&mut evaluation);
        let error = "division or remainder by zero, or non-finite arithmetic result";
        assert_eq!(
            summary,
            format!(
                "Source and results\nr1 = {{雪}} = 3 = 3\r\n# note\r\n\r\nr4 = r1 * 4 = 12\r\nr5 = 1 / 0 = error: {error}\r\n\n\nResults only\nr1 = 3\nr4 = 12\nr5 = error: {error}\n\nSource and results without row labels\n{{雪}} = 3 = 3\r\n# note\r\n\r\nr1 * 4 = 12\r\n1 / 0 = error: {error}\r\n"
            )
        );
        evaluation.replace_source("").unwrap();
        assert_eq!(
            copy_all_session(&mut evaluation),
            "Source and results\n\n\nResults only\n\n\nSource and results without row labels\n"
        );
    }
}
