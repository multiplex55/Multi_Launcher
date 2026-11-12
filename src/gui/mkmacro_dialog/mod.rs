pub mod action_catalog;
pub mod action_editor;
mod analysis_cache;
mod call_editor;
pub mod condition_editor;
mod editor_operations;
mod folding;
pub mod image_asset_picker;
pub mod image_authoring;
pub mod image_authoring_destination;
pub mod image_authoring_job;
pub mod image_crop_editor;
pub mod image_preview;
pub mod image_search_controls;
pub mod image_search_editor;
mod image_search_test_job;
mod jump_mode;
mod key_capture;
pub mod launcher_action_picker;
mod macro_list;
mod macro_properties;
mod navigation;
pub mod ocr_controls;
mod ocr_test_job;
mod outline;
mod package_ui;
pub(crate) mod parameter_prompt;
pub mod recorder_controller;
pub mod recording_review;
mod recording_annotation;
pub(crate) mod runtime_inspector;
mod search;
mod signature_editor;
mod step_table;
mod toolbar;
pub(crate) mod typed_value;
pub mod uia_editor;
pub mod variable_catalog;
pub mod visual_capture_workflow;
pub mod visual_overlay;
mod window_matcher_editor;
pub mod window_picker;

use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::{
    HostIdentity, UtilityHostState, UtilityPresentation, ViewportPassProvenance,
};
use crate::gui::utility_window_geometry::NativeViewportIdentity;
use crate::mkmacro::{
    DiagnosticSeverity, MkHotkeyScope, MkMacro, MkMacroDocument, MkMacroFolder, MkMacroStore,
    MkWindowMatcher, NormalizationConfig, repair_ids, validate_document,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
pub use step_table::Selection;
use visual_capture_workflow::SharedVisualOverlayController;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyDecision {
    KeepEditing,
    Discard,
}

const MKMACRO_VIEWPORT_KEY: &str = "mkmacro-editor";
const MKMACRO_BODY_ID: &str = "mkmacro-editor-body";
const MKMACRO_NATIVE_TITLE: &str = "Mouse/Keyboard Macros";

fn mkmacro_viewport_id(owner_instance: u64) -> eframe::egui::ViewportId {
    eframe::egui::ViewportId::from_hash_of((MKMACRO_VIEWPORT_KEY, owner_instance))
}

fn mkmacro_native_title(owner_instance: u64) -> String {
    format!("{MKMACRO_NATIVE_TITLE} — {owner_instance}")
}

fn mkmacro_viewport_builder(owner_instance: u64) -> eframe::egui::ViewportBuilder {
    eframe::egui::ViewportBuilder::default()
        .with_title(mkmacro_native_title(owner_instance))
        .with_inner_size(eframe::egui::vec2(920.0, 620.0))
        .with_min_inner_size(eframe::egui::vec2(680.0, 420.0))
        .with_resizable(true)
}

fn mkmacro_close_prompt_id(owner_instance: u64) -> eframe::egui::Id {
    eframe::egui::Id::new(("mkmacro_close_prompt", owner_instance))
}

fn mkmacro_close_prompt_layer(owner_instance: u64) -> eframe::egui::LayerId {
    eframe::egui::LayerId::new(
        eframe::egui::Order::Middle,
        mkmacro_close_prompt_id(owner_instance),
    )
}

fn has_draft_input_event(ctx: &eframe::egui::Context) -> bool {
    use eframe::egui::Event;
    ctx.input(|input| {
        input.raw.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { pressed: true, .. }
                    | Event::Text(_)
                    | Event::Paste(_)
                    | Event::PointerButton { .. }
            )
        })
    })
}

fn surrender_mkmacro_focus(ctx: &eframe::egui::Context, layer: eframe::egui::LayerId) {
    if ctx.viewport_id() != eframe::egui::ViewportId::ROOT {
        return;
    }
    if let Some(focused) = ctx.memory(|memory| memory.focused())
        && ctx
            .read_response(focused)
            .is_some_and(|response| response.layer_id == layer)
    {
        ctx.memory_mut(|memory| memory.surrender_focus(focused));
    }
}

fn mkmacro_owned_layers(owner_instance: u64) -> [eframe::egui::LayerId; 14] {
    use eframe::egui::{Id, LayerId, Order};
    [
        Id::new("mkmacro_main_window"),
        Id::new("mkmacro_package_modal"),
        Id::new(Some("Action Editor")),
        Id::new(Some("Window Picker")),
        Id::new("mkmacro_step_search"),
        Id::new("mkmacro_crop_reference_image"),
        Id::new("mkmacro_crop_overwrite_confirmation"),
        Id::new(Some("Add Action")),
        Id::new(Some("Choose Program or Launcher Query")),
        Id::new(Some("Choose Launcher Command Suggestion")),
        Id::new(Some("Confirm destructive action")),
        Id::new(Some("Save captured reference image")),
        Id::new(Some("Reference image already exists")),
        mkmacro_close_prompt_id(owner_instance),
    ]
    .map(|id| LayerId::new(Order::Middle, id))
}

fn mkmacro_surface_owns_input(
    ctx: &eframe::egui::Context,
    viewport: eframe::egui::ViewportId,
    base_layer: eframe::egui::LayerId,
    owner_instance: u64,
) -> bool {
    if ctx.viewport_id() != viewport || !ctx.input(|input| input.focused) {
        return false;
    }
    let visible_layer = crate::gui::jump_mode::ui::visible_top_middle_layer(ctx);
    let focused_layer = crate::gui::jump_mode::ui::focused_response_layer(ctx);
    let modal_layer = ctx.memory(|memory| memory.top_modal_layer());
    let owned_layers = mkmacro_owned_layers(owner_instance);
    let layer_is_owned =
        |layer: eframe::egui::LayerId| layer == base_layer || owned_layers.contains(&layer);
    let visible_is_owned = if base_layer == eframe::egui::LayerId::background() {
        visible_layer.is_none_or(layer_is_owned)
    } else {
        visible_layer.is_some_and(layer_is_owned)
    };
    let focused_is_owned = focused_layer.is_none_or(layer_is_owned);
    let modal_is_owned = modal_layer.is_none_or(layer_is_owned);
    visible_is_owned && focused_is_owned && modal_is_owned
}

fn mkmacro_native_identity(
    host: HostIdentity,
    owner_instance: u64,
) -> Option<NativeViewportIdentity> {
    (host.host == UtilityPresentation::Popout).then_some(NativeViewportIdentity {
        key: crate::settings::UtilityToolKey::parse("mkmacro").ok()?,
        viewport_id: mkmacro_viewport_id(owner_instance),
        owner_generation: owner_instance,
        host_generation: host.generation,
    })
}

#[cfg(test)]
mod recording_preview_document_tests {
    use super::*;
    use crate::mkmacro::{
        MkMacroOutput, MkMacroParameter, MkMacroSignature, MkParameterInputKind, MkSignatureId,
        MkValueType, RecordingPlan, RecordingTarget,
    };

    #[test]
    fn preview_clone_clears_only_the_ephemeral_root_contract() {
        let signature = MkMacroSignature {
            parameters: vec![MkMacroParameter {
                id: MkSignatureId(1),
                name: "required".into(),
                value_type: MkValueType::String,
                input_kind: MkParameterInputKind::Text,
                description: String::new(),
                default_value: None,
            }],
            outputs: vec![MkMacroOutput {
                id: MkSignatureId(2),
                name: "result".into(),
                value_type: MkValueType::String,
                description: String::new(),
            }],
        };
        let root = MkMacro {
            signature: signature.clone(),
            id: 7,
            name: "Root".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: crate::mkmacro::MkPlayback {
                speed_percent: 125,
                ..Default::default()
            },
            steps: vec![],
        };
        let callee = MkMacro {
            id: 8,
            name: "Callee".into(),
            ..root.clone()
        };
        let draft = MkMacroDocument {
            macros: vec![root, callee.clone()],
            ..Default::default()
        };
        let preview = build_recording_preview_document(
            &draft,
            RecordingTarget {
                macro_id: 7,
                insertion_anchor_step_id: None,
                insertion_anchor_generation: None,
            },
            &RecordingPlan { steps: vec![] },
        )
        .unwrap();

        assert_eq!(draft.macros[0].signature, signature);
        assert_eq!(preview.macros[0].signature, MkMacroSignature::default());
        assert_eq!(preview.macros[0].playback.speed_percent, 125);
        assert_eq!(preview.macros[1], callee);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingReviewOpenDisposition {
    Opened,
    Queued,
}

fn annotation_pause_ownership(state: crate::mkmacro::RecorderRuntimeState) -> anyhow::Result<bool> {
    match state {
        crate::mkmacro::RecorderRuntimeState::Recording => Ok(true),
        crate::mkmacro::RecorderRuntimeState::Paused => Ok(false),
        crate::mkmacro::RecorderRuntimeState::Stopping
        | crate::mkmacro::RecorderRuntimeState::Idle => anyhow::bail!("Recorder is not active"),
    }
}

fn should_resume_annotation_owned(
    pause_owned: bool,
    state: Option<crate::mkmacro::RecorderRuntimeState>,
) -> bool {
    pause_owned && state == Some(crate::mkmacro::RecorderRuntimeState::Paused)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderNameError {
    Empty,
    Duplicate(String),
    MissingFolder(u64),
}

impl std::fmt::Display for FolderNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "A macro folder name cannot be empty."),
            Self::Duplicate(name) => write!(f, "A macro folder named \"{name}\" already exists."),
            Self::MissingFolder(_) => write!(f, "This macro folder no longer exists."),
        }
    }
}

impl std::error::Error for FolderNameError {}

/// Immutable launcher data made available to macro authoring.  This is a
/// snapshot: authoring a macro can never edit the launcher's action list.
#[derive(Clone, Default)]
pub struct MkMacroAuthoringContext {
    pub launcher_actions: Arc<Vec<crate::actions::Action>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RecordHotkeyCaptureOwner {
    #[default]
    None,
    Properties,
    Toolbar,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct CellTextGeometry {
    cell: eframe::egui::Rect,
    clip: eframe::egui::Rect,
    label: eframe::egui::Rect,
}

#[cfg(test)]
impl CellTextGeometry {
    fn is_bounded(&self) -> bool {
        self.cell.contains_rect(self.label) && self.clip.contains_rect(self.label)
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct FirstStepRowGeometry {
    assigned_cell: eframe::egui::Rect,
    cell_clip: eframe::egui::Rect,
    current_cell_contents: Vec<eframe::egui::Rect>,
    breakpoint_button: eframe::egui::Rect,
    header_row_height: f32,
    nominal_row_height: f32,
    action_text: Option<CellTextGeometry>,
    diagnostic_text: Option<CellTextGeometry>,
    details_text: Option<CellTextGeometry>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct StepChromeGeometry {
    viewport: eframe::egui::Rect,
    clip: eframe::egui::Rect,
    environment_notice: Option<eframe::egui::Rect>,
    document_diagnostics_header: Option<eframe::egui::Rect>,
    environment_pending: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct StripCellGeometry {
    assigned: eframe::egui::Rect,
    used: eframe::egui::Rect,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct MainBodyGeometry {
    assigned: eframe::egui::Rect,
    after_toolbar: eframe::egui::Rect,
    after_strip: eframe::egui::Rect,
    strip_cells: [Option<StripCellGeometry>; 3],
}

#[cfg(test)]
impl FirstStepRowGeometry {
    fn row_rect(&self) -> eframe::egui::Rect {
        let bottom = self
            .current_cell_contents
            .iter()
            .map(|rect| rect.bottom())
            .fold(self.assigned_cell.bottom(), f32::max);
        eframe::egui::Rect::from_min_max(
            self.assigned_cell.min,
            eframe::egui::pos2(self.assigned_cell.right(), bottom),
        )
    }

    fn visible_in(&self, table: eframe::egui::Rect) -> bool {
        let row = self.row_rect();
        row.top() >= self.cell_clip.top() - 0.1
            && row.bottom() <= self.cell_clip.bottom() + 0.1
            && row.top() >= table.top() - 0.1
            && row.bottom() <= table.bottom() + 0.1
            && self.cell_clip.contains_rect(self.breakpoint_button)
    }
}

pub struct MkMacroDialog {
    pub open: bool,
    host_state: UtilityHostState,
    presentation_prepared: bool,
    owner_instance: u64,
    show_root_after_host_change: bool,
    pending_host_frame: Option<MkMacroHostFrame>,
    retained_host_checkpoint: Option<(u64, u64, MkMacroDraftCheckpoint)>,
    package_hosts: package_ui::PackageHosts,
    pending_package_open: Option<(package_ui::PackageOpen, u64, Option<u64>, super::utility_host::AcceptedParentOpen<u64>)>,
    search_host: super::utility_window_host::UtilityWindowHost<search::SearchState>,
    pending_search_open: Option<(search::SearchMode, u64, Option<u64>, super::utility_host::AcceptedParentOpen<u64>)>,
    active_host: Option<HostIdentity>,
    native_jump_mode: crate::gui::jump_mode::ui::JumpModeUi,
    native_jump_frame: Option<u64>,
    defer_host_effects: bool,
    pending_host_effects: Vec<MkMacroHostAction>,
    pending_recording_target_publish: bool,
    pending_recording_review_drain: bool,
    picker_owner: Option<(eframe::egui::Context, Option<NativeViewportIdentity>)>,
    host_input_allowed: bool,
    host_controls_allowed: bool,
    pending_host_presentation: Option<UtilityPresentation>,
    pending_launcher_close: bool,
    close_prompt_open: bool,
    pub store: Arc<MkMacroStore>,
    pub authoring_context: MkMacroAuthoringContext,
    /// Authoritative client keeping the dialog-wide native overlay service alive.
    pub(crate) visual_overlay: SharedVisualOverlayController,
    /// Direct external edits must call mark_dirty before reading cached analysis.
    pub draft: MkMacroDocument,
    draft_revision: u64,
    revision_document: MkMacroDocument,
    analysis_cache: std::cell::RefCell<analysis_cache::AnalysisCache>,
    baseline: Arc<MkMacroDocument>,
    pub dirty: bool,
    pub conflict: bool,
    pub selected_macro_id: Option<u64>,
    pub selection: Selection,
    editor_state: editor_operations::EditorState,
    navigation: navigation::NavigationState,
    pub search: String,
    /// Process-local presentation state; never participates in draft dirty tracking.
    pub collapsed_folders: HashSet<u64>,
    pub pending_folder_rename: Option<u64>,
    pub folder_rename_text: String,
    folder_rename_needs_focus: bool,
    pub pending_delete_folder: Option<u64>,
    pub folder_delete_confirmation: ConfirmationModal,
    pub folder_error: Option<String>,
    pub delete_confirmation: ConfirmationModal,
    pub unwrap_confirmation: ConfirmationModal,
    pub pending_unwrap_block: Option<u64>,
    pub pending_unwrap_selection: Option<Selection>,
    pub hotkey_capture: bool,
    pub record_hotkey_capture: bool,
    record_hotkey_capture_owner: RecordHotkeyCaptureOwner,
    pub pause_record_hotkey_capture: bool,
    pub marker_record_hotkey_capture: bool,
    catalog: action_catalog::CatalogState,
    catalog_host: super::utility_window_host::UtilityWindowHost<action_catalog::CatalogState>,
    pending_catalog_open: Option<(u64, Option<u64>, super::utility_host::AcceptedParentOpen<u64>)>,
    pending_editor_origin: Option<(EditorOpenOrigin, bool)>,
    editor_host: super::utility_window_host::UtilityWindowHost<action_editor::ActionEditorState>,
    window_picker_host: super::utility_window_host::UtilityWindowHost<window_picker::WindowPickerState>,
    pending_window_picker_open: Option<(window_picker::MatcherEditRequest, PickerOpenOrigin)>,
    launcher_picker_host: super::utility_window_host::UtilityWindowHost<launcher_action_picker::LauncherActionPickerState>,
    pending_launcher_picker_open: Option<(launcher_action_picker::LauncherActionRequest, super::utility_host::AcceptedParentOpen<u64>)>,
    crop_host: super::utility_window_host::UtilityWindowHost<image_crop_editor::ImageCropEditorState>,
    pending_crop_open: Option<(image_authoring_destination::ImageCropDestination, super::utility_host::AcceptedParentOpen<u64>)>,
    pub structural_insertion: Option<action_catalog::StructuralInsertion>,
    pub uia_editor: uia_editor::UiaEditorState,
    pub action_editor: action_editor::ActionEditorState,
    pub(crate) image_search_backend: Arc<dyn crate::mkmacro::VisualSearch>,
    pub image_crop_editor: image_crop_editor::ImageCropEditorState,
    pub window_picker: window_picker::WindowPickerState,
    pub launcher_action_picker: launcher_action_picker::LauncherActionPickerState,
    pub command_error: Option<String>,
    ui_notices: VecDeque<String>,
    /// Transient annotation editor. `annotation_pause_owned` is true only when
    /// opening it paused an actively recording session.
    annotation: recording_annotation::AnnotationState,
    annotation_host: super::utility_window_host::UtilityWindowHost<recording_annotation::AnnotationState>,
    pending_annotation_open: Option<(super::utility_host::AcceptedParentOpen<u64>, bool)>,
    /// Process-local capture review. It never participates in draft dirty tracking or persistence.
    pub recording_review: Option<recording_review::RecordingReviewSession>,
    review_host: super::utility_window_host::UtilityWindowHost<recording_review::RecordingReviewSession>,
    pending_review_open: Option<(Option<super::utility_host::AcceptedParentOpen<u64>>, bool)>,
    review_open_blocked: bool,
    next_review_epoch: u64,
    #[cfg(test)]
    recording_preview_test_facade: Option<RecordingPreviewTestFacade>,
    #[cfg(test)]
    runtime_test: Option<Arc<crate::mkmacro::MacroRuntime>>,
    #[cfg(test)]
    next_recording_preview_test_ticket: u64,
    #[cfg(test)]
    last_main_window_rect: Option<eframe::egui::Rect>,
    #[cfg(test)]
    test_host_button: Option<(eframe::egui::Rect, eframe::egui::Rect, bool)>,
    #[cfg(test)]
    test_annotation_button: Option<(eframe::egui::Rect, eframe::egui::Rect, bool)>,
    #[cfg(test)]
    test_review_button: Option<(eframe::egui::Rect, eframe::egui::Rect, bool)>,
    #[cfg(test)]
    test_run_button: Option<(eframe::egui::Rect, eframe::egui::Rect, bool)>,
    #[cfg(test)]
    test_add_action: Option<(eframe::egui::Rect, eframe::egui::Rect, bool)>,
    #[cfg(test)]
    last_first_step_row_geometry: Option<FirstStepRowGeometry>,
    #[cfg(test)]
    last_first_step_cell_rects: Option<Vec<eframe::egui::Rect>>,
    #[cfg(test)]
    last_step_chrome_geometry: Option<StepChromeGeometry>,
    #[cfg(test)]
    last_main_body_geometry: Option<MainBodyGeometry>,
    #[cfg(test)]
    last_macro_list_geometry: Option<macro_list::LayoutGeometry>,
    #[cfg(test)]
    last_outline_closed_button_geometry: Option<(eframe::egui::Rect, eframe::egui::Rect)>,
    queued_recording_reviews: VecDeque<recording_review::RecordingReviewSession>,
    recording_insert_undo: Vec<RecordingInsertTransaction>,
    recording_insert_redo: Vec<RecordingInsertTransaction>,
    recording_target_cache: std::cell::RefCell<Option<RecordingTargetCache>>,
    step_instance_generations: HashMap<(u64, u64), u64>,
    next_step_instance_generation: u64,
    /// Process-local read-only runtime presentation state. None of these fields
    /// participate in draft dirty tracking, persistence, or save conflicts.
    pub runtime_inspector_open: bool,
    pub runtime_inspector_show_internal: bool,
    pub runtime_inspector_builtins_open: bool,
    pub runtime_inspector_snapshot: Option<Arc<crate::mkmacro::RuntimeSnapshot>>,
    pub runtime_inspector_is_current_debug_run: bool,
    runtime_inspector_observed_run: Option<(crate::mkmacro::RuntimeRunMode, u64)>,
    runtime_inspector_active_breakpoint: Option<crate::mkmacro::BreakpointOccurrence>,
    /// Settings-owned presentation state, separate from the macro draft.
    pub(crate) mkmacro_properties_preferences: crate::settings::MkMacroPropertiesPreferences,
    pending_mkmacro_properties_preferences: Option<crate::settings::MkMacroPropertiesPreferences>,
    package_ui: package_ui::PackageUiState,
}

#[derive(Default)]
struct MkMacroHostEffects {
    host_request: Option<UtilityPresentation>,
    close_requested: bool,
    focus_host: bool,
    actions: Vec<MkMacroHostAction>,
}

struct MkMacroHostFrame {
    root_pass: ViewportPassProvenance,
    host: HostIdentity,
    owner_instance: u64,
    owner_context: eframe::egui::Context,
    owner_viewport: eframe::egui::ViewportId,
    owner_pass: Option<ViewportPassProvenance>,
    owner_pass_accepted: bool,
    callback_observed: bool,
    callback_class: eframe::egui::ViewportClass,
    native_viewport: Option<eframe::egui::ViewportInfo>,
    embedded_root_layer: Option<eframe::egui::LayerId>,
    embedded_root_focused_id: Option<eframe::egui::Id>,
    owner_modal_layer: Option<eframe::egui::LayerId>,
    os_close_requested: bool,
    body_close_requested: bool,
    draft_checkpoint: Option<MkMacroDraftCheckpoint>,
    effects: MkMacroHostEffects,
    native_jump_mode: crate::gui::jump_mode::ui::JumpModeUi,
    native_jump_frame: Option<u64>,
}

#[derive(Clone)]
struct MkMacroDraftCheckpoint {
    draft: MkMacroDocument,
    revision_document: MkMacroDocument,
    baseline: Arc<MkMacroDocument>,
    draft_revision: u64,
    dirty: bool,
    conflict: bool,
    selected_macro_id: Option<u64>,
    selection: Selection,
    step_instance_generations: HashMap<(u64, u64), u64>,
    next_step_instance_generation: u64,
    recording_insert_undo: Vec<RecordingInsertTransaction>,
    recording_insert_redo: Vec<RecordingInsertTransaction>,
    preferences: crate::settings::MkMacroPropertiesPreferences,
    pending_preferences: Option<crate::settings::MkMacroPropertiesPreferences>,
    pending_recording_target_publish: bool,
    pending_recording_review_drain: bool,
    close_prompt_open: bool,
    package_ui: package_ui::PackageUiState,
    editor_state: editor_operations::EditorState,
}

impl MkMacroDraftCheckpoint {
    fn capture(dialog: &MkMacroDialog) -> Self {
        Self {
            draft: dialog.draft.clone(),
            revision_document: dialog.revision_document.clone(),
            baseline: dialog.baseline.clone(),
            draft_revision: dialog.draft_revision,
            dirty: dialog.dirty,
            conflict: dialog.conflict,
            selected_macro_id: dialog.selected_macro_id,
            selection: dialog.selection.clone(),
            step_instance_generations: dialog.step_instance_generations.clone(),
            next_step_instance_generation: dialog.next_step_instance_generation,
            recording_insert_undo: dialog.recording_insert_undo.clone(),
            recording_insert_redo: dialog.recording_insert_redo.clone(),
            preferences: dialog.mkmacro_properties_preferences.clone(),
            pending_preferences: dialog.pending_mkmacro_properties_preferences.clone(),
            pending_recording_target_publish: dialog.pending_recording_target_publish,
            pending_recording_review_drain: dialog.pending_recording_review_drain,
            close_prompt_open: dialog.close_prompt_open,
            package_ui: dialog.package_ui.clone(),
            editor_state: dialog.editor_state.clone(),
        }
    }

    fn restore(self, dialog: &mut MkMacroDialog) {
        dialog.draft = self.draft;
        dialog.revision_document = self.revision_document;
        dialog.baseline = self.baseline;
        dialog.draft_revision = self.draft_revision;
        dialog.dirty = self.dirty;
        dialog.conflict = self.conflict;
        dialog.selected_macro_id = self.selected_macro_id;
        dialog.selection = self.selection;
        dialog.step_instance_generations = self.step_instance_generations;
        dialog.next_step_instance_generation = self.next_step_instance_generation;
        dialog.recording_insert_undo = self.recording_insert_undo;
        dialog.recording_insert_redo = self.recording_insert_redo;
        dialog.mkmacro_properties_preferences = self.preferences;
        dialog.pending_mkmacro_properties_preferences = self.pending_preferences;
        dialog.pending_recording_target_publish = self.pending_recording_target_publish;
        dialog.pending_recording_review_drain = self.pending_recording_review_drain;
        dialog.close_prompt_open = self.close_prompt_open;
        dialog.package_ui = self.package_ui;
        dialog.editor_state = self.editor_state;
        *dialog.recording_target_cache.borrow_mut() = None;
    }
}

#[derive(Clone)]
struct MkMacroHostAction {
    host: HostIdentity,
    draft_revision: u64,
    macro_id: Option<u64>,
    step_ids: Vec<u64>,
    action: MkMacroHostActionKind,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Host actions keep operation-specific data inline for dialog dispatch."
)]
#[derive(Clone)]
enum MkMacroHostActionKind {
    Save,
    Run {
        debug: bool,
        selection: RunSelection,
    },
    Playback {
        command: PlaybackCommand,
        run_id: u64,
        expected_state: crate::mkmacro::RuntimeState,
    },
    Recorder {
        command: RecorderCommand,
        target: crate::mkmacro::RecordingTarget,
        expected_state: crate::mkmacro::RecorderRuntimeState,
    },
    AnnotationBegin {
        macro_id: Option<u64>,
        recording_id: Option<u64>,
        expected_state: crate::mkmacro::RecorderRuntimeState,
    },
    Package(package_ui::PackageHostAction),
    OpenSearch(search::SearchMode),
    OpenCatalog,
    OpenReview { epoch: u64 },
    EditStep { step: crate::mkmacro::MkStep, generation: Option<u64> },
    OpenWindowPicker(window_picker::MatcherEditRequest),
    CloseDecision(MkMacroCloseChoice),
}

enum EditorOpenOrigin {
    Main(super::utility_host::AcceptedParentOpen<u64>),
    Catalog(super::utility_host::AcceptedParentOpen<u64>),
    Review(super::utility_host::AcceptedParentOpen<u64>),
}

enum PickerOpenOrigin {
    Main(super::utility_host::AcceptedParentOpen<u64>),
    Editor(super::utility_host::AcceptedParentOpen<u64>),
}

#[derive(Clone, Copy)]
pub(super) enum RunSelection {
    Whole,
    From(u64),
    Selected,
}

#[derive(Clone, Copy)]
pub(super) enum PlaybackCommand {
    Pause,
    Resume,
    Stop,
}

#[derive(Clone, Copy)]
pub(super) enum RecorderCommand {
    Record,
    Pause,
    Resume,
    Marker,
    StopForReview,
}

#[derive(Clone, Copy)]
pub(super) enum MkMacroCloseChoice {
    Save,
    Discard,
    Cancel,
}

pub(crate) struct MkMacroHostCompletion {
    pub(crate) closed: bool,
    pub(crate) show_root: bool,
    pub(crate) persisted_preferences: Option<crate::settings::MkMacroPropertiesPreferences>,
    pub(crate) notices: Vec<String>,
}

#[cfg(feature = "qa-framebuffer-capture")]
pub(crate) struct MkMacroDialogQaObservation {
    pub(crate) open: bool,
    pub(crate) draft_revision: u64,
    pub(crate) selected_macro_id: Option<u64>,
    pub(crate) selected_step_ids: Vec<u64>,
    pub(crate) table_focus_id: Option<eframe::egui::Id>,
    pub(crate) table_focused: bool,
    pub(crate) document_macro_count: usize,
    pub(crate) runtime_active: bool,
    pub(crate) recorder_active: bool,
}

#[cfg(test)]
struct RecordingPreviewTestFacade {
    runtime: Arc<crate::mkmacro::MacroRuntime>,
    recorder_state: crate::mkmacro::RecorderRuntimeState,
}

#[derive(Debug, Clone)]
struct RecordingInsertTransaction {
    macro_id: u64,
    before: Vec<crate::mkmacro::MkStep>,
    after: Vec<crate::mkmacro::MkStep>,
}

#[derive(Debug, Clone, Copy)]
struct RecordingTargetCache {
    macro_id: u64,
    anchor_id: Option<u64>,
    draft_revision: u64,
    target: crate::mkmacro::RecordingTarget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkVirtualDesktopAction;
    use crate::mkmacro::{
        AlphaPolicy, ExecutionOptions, LoadDisposition, MKMACROS_FILE, MacroRuntime, MkAction,
        MkCondition, MkCoordinateTarget, MkDelayPayload, MkHotkey, MkImageNotFoundPolicy,
        MkImageOutputs, MkImagePayload, MkKey, MkMouseButton, MkMouseMovePayload, MkMousePayload,
        MkMouseScrollAxis, MkStep, MkValue, MkValueSource, MkWaitOptions,
        MkWriteClipboardTextPayload, ReturnPoint, RuntimeCommand, RuntimeState, SCHEMA_VERSION,
        SearchRegion,
    };
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    #[test]
    fn annotation_pause_ownership_preserves_manual_pause() {
        use crate::mkmacro::RecorderRuntimeState;

        assert!(annotation_pause_ownership(RecorderRuntimeState::Recording).unwrap());
        assert!(!annotation_pause_ownership(RecorderRuntimeState::Paused).unwrap());
        assert!(annotation_pause_ownership(RecorderRuntimeState::Idle).is_err());
        assert!(annotation_pause_ownership(RecorderRuntimeState::Stopping).is_err());
        assert!(should_resume_annotation_owned(
            true,
            Some(RecorderRuntimeState::Paused)
        ));
        assert!(!should_resume_annotation_owned(
            false,
            Some(RecorderRuntimeState::Paused)
        ));
        assert!(!should_resume_annotation_owned(
            true,
            Some(RecorderRuntimeState::Idle)
        ));
        assert!(!should_resume_annotation_owned(true, None));
    }

    #[test]
    fn rejected_main_checkpoint_does_not_rewind_annotation_draft_or_pause_owner() {
        let (_directory, mut dialog) = dialog();
        dialog.annotation.open = true;
        dialog.annotation.epoch = 7;
        dialog.annotation.recording_id = Some(11);
        dialog.annotation.pause_id = Some(13);
        dialog.annotation.pause_owned = true;
        dialog.annotation.text = "before".into();
        let checkpoint = MkMacroDraftCheckpoint::capture(&dialog);
        dialog.annotation.text = "retained child edit".into();
        checkpoint.restore(&mut dialog);
        assert_eq!(dialog.annotation.text, "retained child edit");
        assert_eq!(dialog.annotation.epoch, 7);
        assert_eq!(dialog.annotation.recording_id, Some(11));
        assert_eq!(dialog.annotation.pause_id, Some(13));
        assert!(dialog.annotation.pause_owned);
    }

    fn five_macros() -> MkMacroDocument {
        MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            folders: vec![],
            macros: (1..=5)
                .map(|id| MkMacro {
                    signature: Default::default(),
                    id,
                    name: format!("Macro {id}"),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: Default::default(),
                    steps: vec![],
                })
                .collect(),
        }
    }

    fn wait_for_empty_and_stable(dialog: &mut MkMacroDialog, path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stable_since = None;
        loop {
            let snapshot_count = dialog.store.snapshot().macros.len();
            let contents =
                fs::read_to_string(path).unwrap_or_else(|e| format!("<read error: {e}>"));
            let disk_count = serde_json::from_str::<MkMacroDocument>(&contents)
                .map(|d| d.macros.len())
                .unwrap_or(usize::MAX);
            if snapshot_count == 0 && disk_count == 0 {
                let since = stable_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(250) {
                    return;
                }
            } else {
                stable_since = None;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not stabilize on empty document: snapshot_count={snapshot_count}, disk_count={disk_count}, file={contents:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        (dir, MkMacroDialog::new(Arc::new(store)))
    }

    fn layout_input(screen_rect: eframe::egui::Rect, native_scale: f32) -> eframe::egui::RawInput {
        let mut input = eframe::egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&eframe::egui::ViewportId::ROOT)
            .expect("default raw input includes the root viewport")
            .native_pixels_per_point = Some(native_scale);
        input
    }

    fn render_main_window_frame(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        screen_rect: eframe::egui::Rect,
        native_scale: f32,
    ) -> Option<eframe::egui::Rect> {
        render_main_window_frame_with_input(ctx, dialog, layout_input(screen_rect, native_scale))
    }

    fn render_main_window_frame_with_input(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        input: eframe::egui::RawInput,
    ) -> Option<eframe::egui::Rect> {
        let mut viewport = None;
        let mut window_open = dialog.open;
        let output = ctx.run_ui(input, |ctx| {
            dialog.sync_external();
            viewport = dialog.show_main_window(ctx, &mut window_open);
        });
        dialog.open = window_open;
        output.drop_without_applying_deltas();
        viewport
    }

    #[test]
    fn minimum_size_main_layout_keeps_toolbar_and_steps_reachable_without_mutation() {
        use eframe::egui::{self, accesskit::Role, pos2, vec2};

        let (_directory, mut dialog) = dialog();
        dialog.open = true;
        dialog.create_macro();
        dialog.selected_macro_mut().unwrap().steps.push(MkStep {
            id: 90_001,
            action: MkAction::Delay(MkDelayPayload::default()),
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
        });
        dialog.mark_dirty();
        dialog.dirty = false;

        let before_draft = dialog.draft.clone();
        let before_selected_macro = dialog.selected_macro_id;
        let before_selection = (
            dialog.selection.ids.clone(),
            dialog.selection.anchor,
            dialog.selection.primary,
        );
        let before_revision = dialog.draft_revision();
        let before_run =
            crate::mkmacro::runtime::snapshot().map(|snapshot| (snapshot.run_id, snapshot.state));
        let before_recorder = crate::mkmacro::runtime::recorder_snapshot().map(|snapshot| {
            (
                snapshot.state,
                snapshot.raw_event_count,
                snapshot.estimated_action_count,
                snapshot.dropped_event_count,
            )
        });
        let before_stop_pending = crate::mkmacro::runtime::record_stop_pending();

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(680.0, 420.0));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();

        dialog.mkmacro_properties_preferences.master_open = true;
        let mut expanded_previous: Option<egui::Rect> = None;
        let mut expanded_viewport = None;
        for _ in 0..4 {
            let mut viewport = None;
            let mut window_open = dialog.open;
            let output = ctx.run_ui(layout_input(screen_rect, 1.0), |ctx| {
                assert_eq!(ctx.viewport_rect(), screen_rect);
                assert!((ctx.pixels_per_point() - 1.0).abs() < 0.001);
                dialog.sync_external();
                viewport = dialog.show_main_window(ctx, &mut window_open);
            });
            dialog.open = window_open;
            if let (Some(previous), Some(current)) = (expanded_previous, viewport) {
                let stable = (previous.min.x - current.min.x).abs() < 0.5
                    && (previous.min.y - current.min.y).abs() < 0.5
                    && (previous.max.x - current.max.x).abs() < 0.5
                    && (previous.max.y - current.max.y).abs() < 0.5;
                if stable {
                    expanded_viewport = Some(current);
                }
            }
            expanded_previous = viewport;
            output.drop_without_applying_deltas();
        }
        let expanded_height = expanded_viewport
            .expect("expanded property layout settles")
            .height();
        dialog.mkmacro_properties_preferences.master_open = false;

        let mut previous_viewport: Option<egui::Rect> = None;
        let mut previous_actions_visible = None;
        let mut settled_viewport = None;
        let mut settled_actions_visible = [false; 4];
        let mut stable_frame_observed = false;
        for frame in 0..4 {
            let mut viewport = None;
            let mut window_open = dialog.open;
            let output = ctx.run_ui(layout_input(screen_rect, 1.0), |ctx| {
                assert_eq!(ctx.viewport_rect(), screen_rect);
                assert!((ctx.pixels_per_point() - 1.0).abs() < 0.001);
                dialog.sync_external();
                viewport = dialog.show_main_window(ctx, &mut window_open);
            });
            dialog.open = window_open;

            let actions_visible = output
                .platform_output
                .accesskit_update
                .as_ref()
                .map(|update| {
                    ["Save", "Run", "Record", "Stop"].map(|label| {
                        update.nodes.iter().any(|(_, node)| {
                            let Some(bounds) = node.bounds() else {
                                return false;
                            };
                            let bounds = egui::Rect::from_min_max(
                                pos2(bounds.x0 as f32, bounds.y0 as f32),
                                pos2(bounds.x1 as f32, bounds.y1 as f32),
                            );
                            node.role() == Role::Button
                                && node.label() == Some(label)
                                && bounds.is_positive()
                                && screen_rect.contains_rect(bounds)
                        })
                    })
                });
            if frame > 0
                && let (Some(previous), Some(current)) = (previous_viewport, viewport)
            {
                let stable = (previous.min.x - current.min.x).abs() < 0.5
                    && (previous.min.y - current.min.y).abs() < 0.5
                    && (previous.max.x - current.max.x).abs() < 0.5
                    && (previous.max.y - current.max.y).abs() < 0.5;
                if stable {
                    stable_frame_observed = true;
                    settled_viewport = Some(current);
                    if let Some(actions_visible) = actions_visible.or(previous_actions_visible) {
                        settled_actions_visible = actions_visible;
                    }
                }
            }
            previous_viewport = viewport;
            previous_actions_visible = actions_visible;
            output.drop_without_applying_deltas();
        }

        assert!(stable_frame_observed, "the visible main window settles");
        let step_viewport = settled_viewport.expect("the selected macro has a visible step area");
        let low_scale_row_geometry = dialog.last_first_step_row_geometry.clone();
        assert!(
            step_viewport.is_positive(),
            "the step table keeps a positive minimum viewport: {step_viewport:?}"
        );
        assert!(
            step_viewport.height() > expanded_height + 20.0,
            "collapsing properties gives the step table more room ({expanded_height:.1} -> {:.1})",
            step_viewport.height()
        );
        assert!(
            settled_actions_visible.into_iter().all(|visible| visible),
            "Save, Run, Record, and Stop remain inside the minimum-size window"
        );
        assert!(dialog.open);
        assert!(!dialog.dirty);
        assert_eq!(dialog.draft, before_draft);
        assert_eq!(dialog.selected_macro_id, before_selected_macro);
        assert_eq!(
            (
                dialog.selection.ids.clone(),
                dialog.selection.anchor,
                dialog.selection.primary,
            ),
            before_selection
        );
        assert_eq!(dialog.draft_revision(), before_revision);
        assert_eq!(
            crate::mkmacro::runtime::snapshot().map(|snapshot| (snapshot.run_id, snapshot.state)),
            before_run,
            "layout rendering does not start or advance playback"
        );
        assert_eq!(
            crate::mkmacro::runtime::recorder_snapshot().map(|snapshot| {
                (
                    snapshot.state,
                    snapshot.raw_event_count,
                    snapshot.estimated_action_count,
                    snapshot.dropped_event_count,
                )
            }),
            before_recorder,
            "layout rendering does not start or advance recording"
        );
        assert_eq!(
            crate::mkmacro::runtime::record_stop_pending(),
            before_stop_pending
        );
        assert_eq!(dialog.take_macro_properties_preferences_for_persist(), None);
        let mut geometry_violations = Vec::new();
        if let Some(geometry) = low_scale_row_geometry {
            let visible = geometry.visible_in(step_viewport);
            if !visible {
                geometry_violations.push(format!(
                    "100% row_visible={visible}, geometry={geometry:?}, table={step_viewport:?}"
                ));
            }
        } else {
            geometry_violations.push("100% first body row was not painted".to_owned());
        }
        let (_scaled_directory, mut scaled_dialog) = self::dialog();
        scaled_dialog.open = true;
        scaled_dialog.create_macro();
        let sample_macro_id = scaled_dialog
            .selected_macro()
            .expect("the initial short macro is selected")
            .id;
        scaled_dialog
            .selected_macro_mut()
            .unwrap()
            .steps
            .push(MkStep {
                id: 90_002,
                action: MkAction::UnsetVariable {
                    name: "ui polish unused_1".into(),
                },
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                metadata: Default::default(),
            });
        scaled_dialog.mark_dirty();
        scaled_dialog.dirty = false;
        let mut scaled_before_draft = scaled_dialog.draft.clone();
        let mut scaled_before_revision = scaled_dialog.draft_revision();
        let scaled_context = egui::Context::default();
        let mut scaled_theme = crate::settings::ThemeSettings::default();
        scaled_theme.preferences.typography_scale = 2.0;
        crate::gui::theme::publish_committed(&scaled_context, &scaled_theme);
        scaled_context.enable_accesskit();
        scaled_context.style_mut_of(scaled_context.theme(), |style| style.animation_time = 0.0);
        let scaled_screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));

        for (master_open, focus_steps, inspector_open) in [
            (false, false, false),
            (true, false, false),
            (true, true, false),
            (false, false, true),
            (true, false, false),
        ] {
            if master_open {
                scaled_dialog.mkmacro_properties_preferences.expand_all();
            }
            scaled_dialog.mkmacro_properties_preferences.master_open = master_open;
            scaled_dialog.mkmacro_properties_preferences.focus_steps = focus_steps;
            scaled_dialog.runtime_inspector_open = inspector_open;

            let mut previous_viewport: Option<egui::Rect> = None;
            let mut previous_window: Option<egui::Rect> = None;
            let mut stable_viewport = None;
            let mut stable_window = None;
            let mut observed_geometry = Vec::new();
            for _ in 0..32 {
                let viewport = render_main_window_frame(
                    &scaled_context,
                    &mut scaled_dialog,
                    scaled_screen_rect,
                    1.0,
                );
                let window = scaled_dialog.last_main_window_rect;
                observed_geometry.push((
                    viewport,
                    window,
                    scaled_dialog.last_first_step_row_geometry.clone(),
                ));
                if observed_geometry.len() > 5 {
                    observed_geometry.remove(0);
                }
                if let (Some(previous), Some(current), Some(previous_window), Some(current_window)) =
                    (previous_viewport, viewport, previous_window, window)
                    && (previous.min.x - current.min.x).abs() < 0.5
                    && (previous.min.y - current.min.y).abs() < 0.5
                    && (previous.max.x - current.max.x).abs() < 0.5
                    && (previous.max.y - current.max.y).abs() < 0.5
                    && (previous_window.min.x - current_window.min.x).abs() < 0.5
                    && (previous_window.min.y - current_window.min.y).abs() < 0.5
                    && (previous_window.max.x - current_window.max.x).abs() < 0.5
                    && (previous_window.max.y - current_window.max.y).abs() < 0.5
                {
                    stable_viewport = viewport;
                    stable_window = window;
                    break;
                }
                previous_viewport = viewport;
                previous_window = window;
            }

            let viewport = stable_viewport.unwrap_or_else(|| {
                panic!("the 200% step viewport and window settle; frames={observed_geometry:?}")
            });
            let window = stable_window.expect("the 200% main window settles");
            let geometry = scaled_dialog
                .last_first_step_row_geometry
                .clone()
                .expect("the 200% first step row is painted");
            let row = geometry.row_rect();
            let cells = scaled_dialog.last_first_step_cell_rects.clone();
            let chrome = scaled_dialog
                .last_step_chrome_geometry
                .clone()
                .expect("the pending environment notice uses the bounded table chrome");
            let macro_list = scaled_dialog
                .last_macro_list_geometry
                .clone()
                .expect("the macro list records its control and entry viewports");
            let body = scaled_dialog
                .last_main_body_geometry
                .clone()
                .expect("the current-pass main body and Strip cell bounds are recorded");
            let rail_geometry = scaled_dialog.last_outline_closed_button_geometry;
            assert_eq!(
                scaled_context.viewport_rect().size(),
                scaled_screen_rect.size()
            );
            assert!((scaled_context.pixels_per_point() - 1.0).abs() < 0.001);
            let resolved = crate::gui::theme::resolved_for_current_context(&scaled_context)
                .expect("the 200% typography theme is installed");
            assert!((resolved.metrics.body_text_size - 28.0).abs() < 0.01);
            assert!(viewport.is_positive());
            assert!(
                row.height() >= 40.0,
                "the 200% body row uses scaled text: {row:?}"
            );
            let row_visible = geometry.visible_in(viewport);
            let action_text_bounded = geometry
                .action_text
                .as_ref()
                .is_some_and(CellTextGeometry::is_bounded);
            let details_text_bounded = geometry
                .details_text
                .as_ref()
                .is_some_and(CellTextGeometry::is_bounded);
            let diagnostic_text_bounded = geometry
                .diagnostic_text
                .as_ref()
                .is_some_and(|text| text.is_bounded() && text.label.is_positive());
            let macro_controls_bounded = macro_list
                .controls_assigned
                .contains_rect(macro_list.controls_viewport)
                && macro_list
                    .controls_clip
                    .contains_rect(macro_list.controls_viewport);
            let macro_entry_visible = macro_list.first_macro_row.is_some_and(|entry| {
                macro_list.list_viewport.contains_rect(entry)
                    && macro_list.list_clip.contains_rect(entry)
            });
            let chrome_bounded = chrome.environment_pending
                && chrome.viewport.is_positive()
                && chrome.clip.intersects(chrome.viewport)
                && chrome
                    .environment_notice
                    .is_some_and(|notice| notice.intersects(chrome.viewport));
            let chrome_precedes_table = chrome.viewport.bottom() <= viewport.top() + 0.5;
            let toolbar_overflows = body.after_toolbar.bottom() > body.assigned.bottom() + 0.5;
            let strip_overflows = body.strip_cells.iter().flatten().any(|cell| {
                cell.used.bottom() > cell.assigned.bottom() + 0.5
                    || cell.used.right() > cell.assigned.right() + 0.5
            });
            let body_overflows = body.after_strip.bottom() > body.assigned.bottom() + 0.5;
            let window_contained = scaled_screen_rect.contains_rect(window);
            let rail_contained = rail_geometry.is_some_and(|(rail, button)| {
                button.width() <= rail.width() + 0.1
                    && button.left() >= rail.left() - 0.1
                    && button.right() <= rail.right() + 0.1
            });
            if !row_visible
                || !action_text_bounded
                || !details_text_bounded
                || !diagnostic_text_bounded
                || !window_contained
                || !rail_contained
                || !chrome_bounded
                || !chrome_precedes_table
                || !macro_controls_bounded
                || !macro_entry_visible
            {
                geometry_violations.push(format!(
                    "200% master_open={master_open}, focus_steps={focus_steps}, inspector_open={inspector_open}: row_visible={row_visible}, action_text_bounded={action_text_bounded}, details_text_bounded={details_text_bounded}, diagnostic_text_bounded={diagnostic_text_bounded}, window_contained={window_contained}, rail_contained={rail_contained}, chrome_bounded={chrome_bounded}, chrome_precedes_table={chrome_precedes_table}, macro_controls_bounded={macro_controls_bounded}, macro_entry_visible={macro_entry_visible}, toolbar_overflows={toolbar_overflows}, strip_overflows={strip_overflows}, body_overflows={body_overflows}, window={window:?}, body={body:?}, macro_list={macro_list:?}, chrome={chrome:?}, viewport={viewport:?}, row={row:?}, row_geometry={geometry:?}, header={:.1}, nominal_body_row={:.1}, cells={cells:?}, closed_rail/button={rail_geometry:?}",
                    geometry.header_row_height,
                    geometry.nominal_row_height,
                ));
            }
            assert_eq!(scaled_dialog.draft, scaled_before_draft);
            assert_eq!(scaled_dialog.draft_revision(), scaled_before_revision);
            assert!(!scaled_dialog.dirty);
        }
        let diagnostic_label = scaled_dialog
            .last_first_step_row_geometry
            .as_ref()
            .and_then(|geometry| geometry.diagnostic_text.as_ref())
            .expect("the invalid variable row exposes a bounded diagnostic target")
            .label;
        let diagnostic_position = diagnostic_label.center();
        let mut diagnostic_press = layout_input(scaled_screen_rect, 1.0);
        diagnostic_press.events = vec![
            egui::Event::PointerMoved(diagnostic_position),
            egui::Event::PointerButton {
                pos: diagnostic_position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, diagnostic_press);
        let mut diagnostic_release = layout_input(scaled_screen_rect, 1.0);
        diagnostic_release.events = vec![
            egui::Event::PointerMoved(diagnostic_position),
            egui::Event::PointerButton {
                pos: diagnostic_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(
            &scaled_context,
            &mut scaled_dialog,
            diagnostic_release,
        );
        assert_eq!(scaled_dialog.selection.primary, Some(90_002));
        assert_eq!(scaled_dialog.draft, scaled_before_draft);
        assert_eq!(scaled_dialog.draft_revision(), scaled_before_revision);
        assert!(!scaled_dialog.dirty);
        scaled_dialog.selection.clear();
        assert_eq!(
            scaled_dialog.take_macro_properties_preferences_for_persist(),
            None
        );
        let sample_folder_id = scaled_dialog.create_folder();
        scaled_dialog
            .draft
            .folders
            .iter_mut()
            .find(|folder| folder.id == sample_folder_id)
            .expect("the sample folder was created")
            .name = "UI polish samples".into();
        for index in 0..6 {
            assert!(scaled_dialog.create_macro_in_folder(Some(sample_folder_id)));
            scaled_dialog
                .selected_macro_mut()
                .expect("the filler macro is selected")
                .name = format!("Sample workflow {index}");
        }
        let target_macro = scaled_dialog
            .draft
            .macros
            .iter_mut()
            .find(|macro_| macro_.id == sample_macro_id)
            .expect("the initial macro remains in the document");
        target_macro.name = "UI polish sample A".into();
        target_macro.folder_id = Some(sample_folder_id);
        let target_index = scaled_dialog
            .draft
            .macros
            .iter()
            .position(|macro_| macro_.id == sample_macro_id)
            .expect("the named sample macro remains in the document");
        let target_macro = scaled_dialog.draft.macros.remove(target_index);
        scaled_dialog.draft.macros.push(target_macro);
        scaled_dialog.set_selected_macro(Some(sample_macro_id));
        scaled_dialog.dirty = false;
        scaled_before_draft = scaled_dialog.draft.clone();
        scaled_before_revision = scaled_dialog.draft_revision();
        render_main_window_frame(&scaled_context, &mut scaled_dialog, scaled_screen_rect, 1.0);
        let macro_list = scaled_dialog
            .last_macro_list_geometry
            .clone()
            .expect("the final layout records a scrollable macro-list control area");
        let maximum_controls_scroll =
            (macro_list.controls_content_size.y - macro_list.controls_viewport.height()).max(0.0);
        assert!(
            maximum_controls_scroll > 1.0,
            "the 200% control viewport reserves a real macro-entry row; controls={macro_list:?}"
        );
        let mut scrolled_macro_list = macro_list.clone();
        for _ in 0..8 {
            if scrolled_macro_list
                .controls_viewport
                .contains_rect(scrolled_macro_list.search_rect)
            {
                break;
            }
            let wheel_position = scrolled_macro_list.controls_viewport.center();
            let mut wheel_input = layout_input(scaled_screen_rect, 1.0);
            wheel_input.events = vec![
                eframe::egui::Event::PointerMoved(wheel_position),
                eframe::egui::Event::MouseWheel {
                    unit: eframe::egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -maximum_controls_scroll - 300.0),
                    phase: eframe::egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, wheel_input);
            scrolled_macro_list = scaled_dialog
                .last_macro_list_geometry
                .clone()
                .expect("the scrolled control and list viewports remain observable");
        }
        assert!(
            scrolled_macro_list.controls_offset.y > 0.0,
            "wheel input scrolls the bounded macro controls: {scrolled_macro_list:?}"
        );
        assert!(
            scrolled_macro_list
                .controls_viewport
                .contains_rect(scrolled_macro_list.search_rect),
            "Search becomes fully reachable within the controls viewport: {scrolled_macro_list:?}"
        );
        let search_position = scrolled_macro_list.search_rect.center();
        let mut search_input = layout_input(scaled_screen_rect, 1.0);
        search_input.events = vec![
            egui::Event::PointerMoved(search_position),
            egui::Event::PointerButton {
                pos: search_position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, search_input);
        let mut focused_macro_list = scaled_dialog
            .last_macro_list_geometry
            .clone()
            .expect("the focused search layout remains observable");
        assert!(
            focused_macro_list.search_has_focus,
            "the scrolled Search field accepts focus"
        );
        let mut search_release = layout_input(scaled_screen_rect, 1.0);
        search_release.events = vec![
            egui::Event::PointerMoved(search_position),
            egui::Event::PointerButton {
                pos: search_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, search_release);
        focused_macro_list = scaled_dialog
            .last_macro_list_geometry
            .clone()
            .expect("the released Search layout remains observable");
        assert!(focused_macro_list.search_has_focus);
        scaled_context.memory_mut(|memory| {
            memory.surrender_focus(focused_macro_list.search_id);
        });
        let folder_row = focused_macro_list
            .folder_rows
            .get(&sample_folder_id)
            .copied()
            .expect("the named sample folder row is rendered");
        assert!(
            focused_macro_list.list_viewport.contains_rect(folder_row)
                && focused_macro_list.list_clip.contains_rect(folder_row),
            "the actual sample folder is fully visible in the list: {focused_macro_list:?}"
        );
        let maximum_list_scroll = (focused_macro_list.list_content_size.y
            - focused_macro_list.list_viewport.height())
        .max(0.0);
        assert!(
            maximum_list_scroll > 1.0,
            "the sample folder contains enough entries to exercise list scrolling: {focused_macro_list:?}"
        );
        scaled_dialog.set_selected_macro(None);
        let mut revealed_macro_list = focused_macro_list.clone();
        for _ in 0..8 {
            let target_row = revealed_macro_list
                .macro_rows
                .get(&sample_macro_id)
                .copied()
                .expect("the named sample macro row is rendered");
            let target_visible = revealed_macro_list.list_viewport.contains_rect(target_row)
                && revealed_macro_list.list_clip.contains_rect(target_row);
            if revealed_macro_list.list_offset.y > 0.0 && target_visible {
                break;
            }
            let desired_offset = (revealed_macro_list.list_offset.y + target_row.center().y
                - revealed_macro_list.list_viewport.center().y)
                .clamp(0.0, maximum_list_scroll);
            let scroll_delta = desired_offset - revealed_macro_list.list_offset.y;
            assert!(
                scroll_delta.abs() > 0.5,
                "the target row can be aligned with the visible list viewport"
            );
            let wheel_position = revealed_macro_list.list_viewport.center();
            let mut wheel_input = layout_input(scaled_screen_rect, 1.0);
            wheel_input.events = vec![
                egui::Event::PointerMoved(wheel_position),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -scroll_delta),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, wheel_input);
            render_main_window_frame(&scaled_context, &mut scaled_dialog, scaled_screen_rect, 1.0);
            revealed_macro_list = scaled_dialog
                .last_macro_list_geometry
                .clone()
                .expect("the scrolled macro rows remain observable");
        }
        let sample_macro_row = revealed_macro_list
            .macro_rows
            .get(&sample_macro_id)
            .copied()
            .expect("the named sample macro row is rendered after scrolling");
        assert!(
            revealed_macro_list.list_offset.y > 0.0
                && revealed_macro_list
                    .list_viewport
                    .contains_rect(sample_macro_row)
                && revealed_macro_list
                    .list_clip
                    .contains_rect(sample_macro_row),
            "real list wheel input fully reveals the named sample macro: {revealed_macro_list:?}"
        );
        let sample_macro_position = sample_macro_row.center();
        let mut select_press = layout_input(scaled_screen_rect, 1.0);
        select_press.events = vec![
            egui::Event::PointerMoved(sample_macro_position),
            egui::Event::PointerButton {
                pos: sample_macro_position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, select_press);
        let mut select_release = layout_input(scaled_screen_rect, 1.0);
        select_release.events = vec![
            egui::Event::PointerMoved(sample_macro_position),
            egui::Event::PointerButton {
                pos: sample_macro_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        render_main_window_frame_with_input(&scaled_context, &mut scaled_dialog, select_release);
        assert_eq!(scaled_dialog.selected_macro_id, Some(sample_macro_id));
        assert_eq!(scaled_dialog.draft, scaled_before_draft);
        assert_eq!(scaled_dialog.draft_revision(), scaled_before_revision);
        assert!(!scaled_dialog.dirty);
        assert!(scaled_dialog.search.is_empty());
        assert!(
            geometry_violations.is_empty(),
            "the first full body row and its window/rail remain within their 100%/200% viewports:\n{}",
            geometry_violations.join("\n")
        );
    }

    #[test]
    fn empty_macro_actions_remain_inside_a_200_percent_viewport() {
        use eframe::egui::{self, accesskit::Role, pos2, vec2};

        fn button_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
            output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    if node.role() != Role::Button || node.label() != Some(label) {
                        return None;
                    }
                    let bounds = node.bounds()?;
                    Some(egui::Rect::from_min_max(
                        pos2(bounds.x0 as f32, bounds.y0 as f32),
                        pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ))
                })
        }

        let (_directory, mut dialog) = dialog();
        dialog.open = true;
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut theme = crate::settings::ThemeSettings::default();
        theme.preferences.typography_scale = 2.0;
        crate::gui::theme::publish_committed(&ctx, &theme);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        let mut previous_window = None;
        let mut stable_window = None;
        let mut new_macro_bounds = None;
        let mut new_folder_bounds = None;

        for _ in 0..32 {
            let mut open = dialog.open;
            let output = ctx.run_ui(layout_input(screen_rect, 1.0), |ctx| {
                assert_eq!(ctx.viewport_rect(), screen_rect);
                assert!((ctx.pixels_per_point() - 1.0).abs() < 0.001);
                dialog.sync_external();
                dialog.show_main_window(ctx, &mut open);
            });
            dialog.open = open;
            let window = dialog
                .last_main_window_rect
                .expect("the empty macro window is rendered");
            new_macro_bounds = button_bounds(&output, "New Macro");
            new_folder_bounds = button_bounds(&output, "New Folder");
            output.drop_without_applying_deltas();

            if previous_window.is_some_and(|previous: egui::Rect| {
                (previous.min - window.min).length() < 0.5
                    && (previous.max - window.max).length() < 0.5
            }) {
                stable_window = Some(window);
                break;
            }
            previous_window = Some(window);
        }

        let window = stable_window.expect("the scaled empty macro window settles");
        assert!(screen_rect.contains_rect(window), "window={window:?}");
        for (label, bounds) in [
            ("New Macro", new_macro_bounds),
            ("New Folder", new_folder_bounds),
        ] {
            let bounds = bounds.unwrap_or_else(|| panic!("{label} remains accessible"));
            assert!(
                window.contains_rect(bounds),
                "{label} remains inside the scaled empty-window viewport: window={window:?}, bounds={bounds:?}"
            );
        }
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.draft.folders.is_empty());
        assert!(!dialog.dirty);
    }

    #[test]
    fn macro_property_preferences_survive_selection_sync_and_close() {
        let (_directory, mut dialog) = dialog();
        dialog.create_macro();
        let mut preferences = crate::settings::MkMacroPropertiesPreferences::default();
        preferences.expand_all();
        preferences.focus_steps = true;
        preferences.recording_open = false;
        dialog.set_macro_properties_preferences(preferences.clone());
        let revision = dialog.draft_revision();

        dialog.hotkey_capture = true;
        dialog.start_property_record_hotkey_capture();
        dialog.pause_record_hotkey_capture = true;
        dialog.marker_record_hotkey_capture = true;
        dialog.set_selected_macro(None);
        assert!(!dialog.hotkey_capture);
        assert!(!dialog.record_hotkey_capture);
        assert!(dialog.pause_record_hotkey_capture);
        assert!(dialog.marker_record_hotkey_capture);
        dialog.open();
        dialog.sync_external();
        dialog.close_children();
        dialog.open();

        assert_eq!(dialog.mkmacro_properties_preferences, preferences);
        assert_eq!(dialog.draft_revision(), revision);
        assert_eq!(dialog.take_macro_properties_preferences_for_persist(), None);
    }

    #[test]
    fn analysis_revision_tracks_mutations_save_repair_reload_and_presentation() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let initial = d.draft_revision();
        let diagnostics = d.cached_diagnostics();
        d.mark_dirty();
        d.selection.clear();
        d.toggle_folder_collapsed(123);
        assert_eq!(d.draft_revision(), initial);
        assert!(Arc::ptr_eq(&diagnostics, &d.cached_diagnostics()));
        d.selected_macro_mut().unwrap().steps.push(MkStep {
            id: 0,
            action: MkAction::Delay(Default::default()),
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
        });
        d.mark_dirty();
        assert_eq!(d.draft_revision(), initial + 1);
        assert!(!Arc::ptr_eq(&diagnostics, &d.cached_diagnostics()));
        d.save().unwrap();
        assert_eq!(d.draft_revision(), initial + 2);
        assert!(!d.dirty);
        assert_ne!(d.selected_macro().unwrap().steps[0].id, 0);
        let saved = d.draft_revision();
        d.selected_macro_mut().unwrap().steps[0].metadata.comment = "changed".into();
        d.mark_dirty();
        d.mark_dirty();
        assert_eq!(d.draft_revision(), saved + 1);
        assert!(d.dirty);
        d.reload_with_decision(DirtyDecision::Discard);
        assert_eq!(d.draft_revision(), saved + 2);
        assert!(!d.dirty);
        assert!(
            d.selected_macro().unwrap().steps[0]
                .metadata
                .comment
                .is_empty()
        );
        let mut external = d.draft.clone();
        external.macros[0].name = "External".into();
        d.store.save(external).unwrap();
        d.sync_external();
        assert_eq!(d.draft_revision(), saved + 3);
        assert!(!d.dirty);
        assert_eq!(d.selected_macro().unwrap().name, "External");
    }

    #[test]
    fn debug_execution_methods_match_normal_admission_rejections() {
        let (_dir, mut d) = dialog();
        assert_eq!(
            d.run_selected_macro().unwrap_err().to_string(),
            d.debug_selected_macro().unwrap_err().to_string()
        );

        d.create_macro();
        d.selected_macro_mut().unwrap().enabled = false;
        assert_eq!(
            d.run_selected_macro().unwrap_err().to_string(),
            d.debug_selected_macro().unwrap_err().to_string()
        );
        assert_eq!(
            d.run_from_step(1).unwrap_err().to_string(),
            d.debug_from_step(1).unwrap_err().to_string()
        );
        assert_eq!(
            d.run_selected_steps().unwrap_err().to_string(),
            d.debug_selected_steps().unwrap_err().to_string()
        );

        d.selected_macro_mut().unwrap().enabled = true;
        d.selected_macro_mut().unwrap().steps.push(MkStep {
            metadata: Default::default(),
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Delay(MkDelayPayload {
                fixed_ms: u64::MAX,
                ..Default::default()
            }),
        });
        d.selection.ids = std::collections::BTreeSet::from([1]);
        for (normal, debug) in [
            (
                d.run_selected_macro().unwrap_err().to_string(),
                d.debug_selected_macro().unwrap_err().to_string(),
            ),
            (
                d.run_from_step(1).unwrap_err().to_string(),
                d.debug_from_step(1).unwrap_err().to_string(),
            ),
            (
                d.run_selected_steps().unwrap_err().to_string(),
                d.debug_selected_steps().unwrap_err().to_string(),
            ),
        ] {
            assert_eq!(normal, debug);
        }
    }

    #[test]
    fn shared_execution_preparation_saves_and_remaps_targets_by_position() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.selected_macro_mut().unwrap().steps = vec![
            MkStep {
                metadata: Default::default(),
                id: 11,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
            MkStep {
                metadata: Default::default(),
                id: 22,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
            MkStep {
                metadata: Default::default(),
                id: 33,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
        ];
        d.selection.ids = std::collections::BTreeSet::from([11, 33]);
        let positions = d.selected_step_positions().unwrap();
        assert_eq!(positions, vec![0, 2]);

        let (macro_id, from_id) = d.prepare_from_step(22).unwrap();
        assert_eq!((macro_id, from_id), (1, 22));
        assert!(!d.dirty);

        d.selected_macro_mut().unwrap().steps[0].id = 101;
        d.selected_macro_mut().unwrap().steps[2].id = 303;
        assert_eq!(d.step_ids_at_positions(&positions), vec![101, 303]);

        d.selection.ids = std::collections::BTreeSet::from([101, 303]);
        d.mark_dirty();
        let (macro_id, selected_ids) = d.prepare_selected_steps().unwrap();
        assert_eq!((macro_id, selected_ids), (1, vec![101, 303]));
        assert!(!d.dirty);
        assert_eq!(d.store.snapshot().macros[0].steps[0].id, 101);
        assert_eq!(d.store.snapshot().macros[0].steps[2].id, 303);
    }

    fn folder_dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let (dir, mut d) = dialog();
        d.draft.folders = vec![crate::mkmacro::MkMacroFolder {
            id: 42,
            name: "Utilities".into(),
        }];
        d.save().unwrap();
        (dir, d)
    }

    #[test]
    fn new_folder_id_is_nonzero_unique_and_stable_across_creation_and_save() {
        let (_dir, mut d) = folder_dialog();
        let first = d.create_folder();
        let second = d.create_folder();
        assert_ne!(first, 0);
        assert_ne!(second, 0);
        let ids: Vec<_> = d.draft.folders.iter().map(|folder| folder.id).collect();
        assert_eq!(ids, vec![42, first, second]);
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 3);
        assert!(!repair_ids(&mut d.draft));
        d.save().unwrap();
        assert!(d.reload_with_decision(DirtyDecision::Discard));
        assert_eq!(
            d.draft
                .folders
                .iter()
                .map(|folder| folder.id)
                .collect::<Vec<_>>(),
            ids
        );
    }

    #[test]
    fn new_folder_marks_draft_dirty_without_changing_saved_document() {
        let (_dir, mut d) = folder_dialog();
        let saved = d.store.snapshot();
        assert!(!d.dirty);
        let id = d.create_folder();
        assert!(d.dirty);
        assert_eq!(
            d.draft.folders.last().unwrap(),
            &MkMacroFolder {
                id,
                name: "New Folder".into(),
            }
        );
        assert_eq!(*d.store.snapshot(), *saved);
        assert_eq!(*d.baseline, *saved);
    }

    #[test]
    fn new_folder_names_use_first_available_case_insensitive_suffix() {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.extend([
            MkMacroFolder {
                id: 43,
                name: "nEw fOlDeR".into(),
            },
            MkMacroFolder {
                id: 44,
                name: "NEW FOLDER 2".into(),
            },
            MkMacroFolder {
                id: 45,
                name: "New Folder 4".into(),
            },
        ]);
        d.create_folder();
        assert_eq!(d.draft.folders.last().unwrap().name, "New Folder 3");
        d.create_folder();
        assert_eq!(d.draft.folders.last().unwrap().name, "New Folder 5");
        let names: HashSet<_> = d
            .draft
            .folders
            .iter()
            .map(|f| f.name.to_lowercase())
            .collect();
        assert_eq!(names.len(), d.draft.folders.len());
    }

    fn assert_rejected_folder_rename(proposed: &str, expected: FolderNameError) {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(MkMacroFolder {
            id: 43,
            name: "Work".into(),
        });
        d.begin_folder_rename(42);
        d.folder_rename_text = proposed.into();
        let before = d.draft.clone();
        assert_eq!(d.rename_folder(42, proposed), Err(expected));
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.folder_rename_text, proposed);
    }

    #[test]
    fn rename_folder_rejects_empty_name() {
        assert_rejected_folder_rename("", FolderNameError::Empty);
        assert_eq!(
            FolderNameError::Empty.to_string(),
            "A macro folder name cannot be empty."
        );
    }

    #[test]
    fn rename_folder_rejects_whitespace_only_name() {
        assert_rejected_folder_rename(" \t\n\u{2003}", FolderNameError::Empty);
    }

    #[test]
    fn rename_folder_rejects_duplicate_name() {
        assert_rejected_folder_rename("Work", FolderNameError::Duplicate("Work".into()));
    }

    #[test]
    fn rename_folder_rejects_case_insensitive_duplicate_with_canonical_diagnostic() {
        assert_rejected_folder_rename("  wOrK  ", FolderNameError::Duplicate("Work".into()));
        assert_eq!(
            FolderNameError::Duplicate("Work".into()).to_string(),
            "A macro folder named \"Work\" already exists."
        );
    }

    #[test]
    fn rename_folder_matches_trimmed_unicode_names() {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(MkMacroFolder {
            id: 43,
            name: "  ÉCOLE  ".into(),
        });
        let before = d.draft.clone();
        assert_eq!(
            d.rename_folder(42, "école"),
            Err(FolderNameError::Duplicate("  ÉCOLE  ".into()))
        );
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
    }

    #[test]
    fn rename_folder_trims_and_changes_only_name_preserving_macros_and_presentation() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.draft.macros[2].folder_id = Some(42);
        d.draft.macros[2].description = "Preserve macro contents".into();
        d.draft.macros[2].steps.push(MkStep {
            metadata: Default::default(),
            id: 11,
            enabled: true,
            breakpoint: false,
            repeat: 2,
            delay_after_ms: 17,
            on_error: Default::default(),
            action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 50,
                ..Default::default()
            }),
        });
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        d.selection.click(&[11], 0, false, false);
        d.collapsed_folders.insert(42);
        d.begin_folder_rename(42);
        d.folder_error = Some("Previous validation error".into());
        let macros = d.draft.macros.clone();
        let baseline = d.baseline.clone();
        let mut expected = d.draft.clone();
        expected.folders[0].name = "Work".into();
        assert_eq!(d.rename_folder(42, " \tWork\u{2003}"), Ok(true));
        assert_eq!(d.draft.macros, macros);
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
        assert!(Arc::ptr_eq(&d.baseline, &baseline));
        assert_eq!(*d.store.snapshot(), *baseline);
        assert_eq!(d.selected_macro_id, Some(3));
        assert_eq!(d.selection.ids, std::collections::BTreeSet::from([11]));
        assert_eq!(d.collapsed_folders, HashSet::from([42]));
        assert_no_pending_folder_operations(&d);
    }

    #[test]
    fn rename_folder_allows_case_only_and_whitespace_normalization_of_self() {
        for (stored, proposed, expected) in [
            ("Utilities", "UTILITIES", "UTILITIES"),
            ("  Utilities  ", " Utilities ", "Utilities"),
        ] {
            let (_dir, mut d) = folder_dialog();
            d.draft.folders[0].name = stored.into();
            d.mark_dirty();
            assert_eq!(d.rename_folder(42, proposed), Ok(true));
            assert_eq!(d.draft.folders[0].name, expected);
            assert!(d.dirty);
        }
    }

    #[test]
    fn rename_folder_no_op_preserves_dirty_state_and_clears_edit() {
        for dirty in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.dirty = dirty;
            d.begin_folder_rename(42);
            d.folder_error = Some("Previous validation error".into());
            let before = d.draft.clone();
            assert_eq!(d.rename_folder(42, "  Utilities  "), Ok(false));
            assert_eq!(d.draft, before);
            assert_eq!(d.dirty, dirty);
            assert_no_pending_folder_operations(&d);
        }
    }

    #[test]
    fn rename_folder_missing_id_does_not_change_draft_or_pending_edit() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        let before = d.draft.clone();
        for id in [0, 999] {
            assert_eq!(
                d.rename_folder(id, "Work"),
                Err(FolderNameError::MissingFolder(id))
            );
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Utilities");
        }
    }

    #[test]
    fn moving_macro_changes_only_folder_and_retains_macro_and_step_selection() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        let m = &mut d.draft.macros[2];
        m.description = "Keep this description".into();
        m.hotkey = Some(MkHotkey {
            key: MkKey::Delete,
            modifiers: vec![],
        });
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            process: Some("editor.exe".into()),
            ..Default::default()
        });
        m.steps = (11..=13)
            .map(|id| MkStep {
                metadata: Default::default(),
                id,
                enabled: true,
                breakpoint: false,
                repeat: 2,
                delay_after_ms: 17,
                on_error: Default::default(),
                action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                    fixed_ms: id * 10,
                    ..Default::default()
                }),
            })
            .collect();
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        let rows = [11, 12, 13];
        d.selection.click(&rows, 1, false, false);
        d.selection.click(&rows, 2, true, false);

        for destination in [Some(42), None] {
            let mut expected = d.draft.clone();
            expected.macros[2].folder_id = destination;
            let selected_steps = d.selection.ids.clone();
            let mut expected_selection = d.selection.clone();
            assert!(!d.dirty);
            assert!(d.move_macro_to_folder(3, destination));
            assert!(d.dirty);
            assert_eq!(d.draft, expected);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, selected_steps);
            // A subsequent shift click also verifies that the anchor survives.
            expected_selection.click(&rows, 0, false, true);
            d.selection.click(&rows, 0, false, true);
            assert_eq!(d.selection.ids, expected_selection.ids);
            d.save().unwrap();
            assert!(!d.move_macro_to_folder(3, destination));
            assert!(!d.dirty);
            assert_eq!(d.draft, expected);
        }
    }

    #[test]
    fn moving_folders_preserves_macro_id_hotkey_compilation_and_manual_run_eligibility() {
        use crate::mkmacro::{compile, hotkeys::compile_hotkey};

        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(crate::mkmacro::MkMacroFolder {
            id: 43,
            name: "Work".into(),
        });
        d.draft.macros = five_macros().macros;
        d.selected_macro_id = Some(1);
        d.draft.macros[0].hotkey = Some(MkHotkey {
            key: MkKey::Delete,
            modifiers: vec![],
        });
        let expected_hotkey = compile_hotkey(d.draft.macros[0].hotkey.as_ref().unwrap());
        assert!(expected_hotkey.is_some());
        for enabled in [true, false] {
            d.draft.macros[0].enabled = enabled;
            for destination in [Some(42), Some(43), None] {
                assert!(d.move_macro_to_folder(1, destination));
                let m = &d.draft.macros[0];
                assert_eq!(m.folder_id, destination);
                assert_eq!(m.id, 1);
                assert_eq!(compile(m).unwrap().macro_id, 1);
                assert_eq!(compile_hotkey(m.hotkey.as_ref().unwrap()), expected_hotkey);
                assert_eq!(d.playback_block_reason().is_none(), enabled);
            }
        }
    }

    #[test]
    fn choosing_unfiled_normalizes_a_dangling_folder_reference() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        d.draft.macros[2].folder_id = Some(999);
        d.mark_dirty();
        let mut expected = d.draft.clone();
        expected.macros[2].folder_id = None;
        assert!(d.move_macro_to_folder(3, None));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_real_folder_assigns_id_and_marks_dirty() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        let mut expected = d.draft.clone();
        expected.macros[0].folder_id = Some(42);
        assert!(d.move_macro_to_folder(1, Some(42)));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_unfiled_clears_folder_and_marks_dirty() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.draft.macros[0].folder_id = Some(42);
        d.save().unwrap();
        let mut expected = d.draft.clone();
        expected.macros[0].folder_id = None;
        assert!(d.move_macro_to_folder(1, None));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_current_destination_has_no_additional_mutation() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        for destination in [Some(42), None] {
            assert!(d.move_macro_to_folder(1, destination));
            let draft = d.draft.clone();
            assert!(!d.move_macro_to_folder(1, destination));
            assert_eq!(d.draft, draft);
            assert!(d.dirty);
            d.save().unwrap();
            let baseline = d.baseline.clone();
            assert!(!d.move_macro_to_folder(1, destination));
            assert_eq!(d.draft, draft);
            assert!(!d.dirty);
            assert!(Arc::ptr_eq(&d.baseline, &baseline));
        }
    }

    #[test]
    fn moving_macro_to_missing_folder_rejects_dangling_reference() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        for current in [None, Some(42)] {
            d.draft.macros[0].folder_id = current;
            d.save().unwrap();
            let draft = d.draft.clone();
            for destination in [Some(999), Some(0)] {
                assert!(!d.move_macro_to_folder(1, destination));
                assert_eq!(d.draft, draft);
                assert!(!d.dirty);
            }
        }
    }

    #[test]
    fn moving_missing_macro_leaves_draft_unchanged() {
        let (_dir, mut d) = folder_dialog();
        let draft = d.draft.clone();
        for destination in [Some(42), None] {
            assert!(!d.move_macro_to_folder(999, destination));
            assert_eq!(d.draft, draft);
            assert!(!d.dirty);
        }
    }

    fn folder_deletion_dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let (dir, mut d) = folder_dialog();
        d.draft.folders.extend([
            MkMacroFolder {
                id: 43,
                name: "Other".into(),
            },
            MkMacroFolder {
                id: 44,
                name: "Empty".into(),
            },
        ]);
        d.draft.macros = five_macros().macros;
        for (index, m) in d.draft.macros.iter_mut().enumerate() {
            m.folder_id = [Some(42), Some(43), Some(42), None, Some(42)][index];
            m.description = format!("Description {}", m.id);
            m.enabled = index % 2 == 0;
            m.hotkey = Some(MkHotkey {
                key: MkKey::Function(m.id as u8),
                modifiers: vec![MkKey::Control],
            });
            m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
                process: Some(format!("app{}.exe", m.id)),
                ..Default::default()
            });
            m.playback.speed_percent = 150;
            m.playback.random_delay_ms = 23;
            m.playback.random_offset_px = 7;
            m.steps = (1..=3)
                .map(|offset| MkStep {
                    metadata: Default::default(),
                    id: m.id * 10 + offset,
                    enabled: offset != 2,
                    breakpoint: false,
                    repeat: 2,
                    delay_after_ms: 17,
                    on_error: crate::mkmacro::MkErrorPolicy::Continue,
                    action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                        fixed_ms: offset * 10,
                        ..Default::default()
                    }),
                })
                .collect();
            d.store
                .write_captured_png(
                    &image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])),
                    crate::mkmacro::MkImageRef::from_filename("shared.png"),
                    crate::mkmacro::ImageImportChoice::ReplaceExisting,
                )
                .unwrap();
        }
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        d.selection.click(&[31, 32, 33], 1, false, false);
        d.selection.click(&[31, 32, 33], 2, true, false);
        d.collapsed_folders = HashSet::from([42, 43]);
        (dir, d)
    }

    #[test]
    fn folder_deletion_confirmation_shows_name_exact_count_and_preservation_warning() {
        use eframe::egui::{Context, epaint::Shape};
        fn collect_text(shape: &Shape, text: &mut String) {
            match shape {
                Shape::Text(shape) => {
                    text.push_str(&shape.galley.job.text);
                    text.push('\n');
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, text);
                    }
                }
                _ => {}
            }
        }
        let (_dir, mut d) = folder_deletion_dialog();
        let before = d.draft.clone();
        for (id, name, count) in [(42, "Utilities", 3), (43, "Other", 1), (44, "Empty", 0)] {
            d.request_delete_folder(id);
            assert_eq!(d.pending_delete_folder, Some(id));
            assert!(d.folder_delete_confirmation.is_open());
            let ctx = Context::default();
            // A second frame includes the window after egui's initial sizing pass.
            ctx.run_ui(Default::default(), |ctx| {
                d.folder_delete_confirmation.ui(ctx);
            })
            .drop_without_applying_deltas();
            let output = ctx.run_ui(Default::default(), |ctx| {
                d.folder_delete_confirmation.ui(ctx);
            });
            let mut text = String::new();
            for shape in &output.shapes {
                collect_text(&shape.shape, &mut text);
            }
            assert!(
                text.contains(&format!("Delete folder \"{name}\"? Members: {count}.")),
                "{text}"
            );
            assert!(text.contains("Members will move to Unfiled."), "{text}");
            assert!(text.contains("No macros will be deleted."), "{text}");
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn folder_deletion_with_zero_members_changes_only_the_folder_list() {
        let (_dir, mut d) = folder_deletion_dialog();
        let mut expected = d.draft.clone();
        expected.folders.remove(2);
        assert!(d.delete_folder(44));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
        assert_eq!(d.selected_macro_id, Some(3));
        assert_eq!(d.collapsed_folders, HashSet::from([42, 43]));
        d.save().unwrap();
        assert!(!d.delete_folder(44));
        assert!(!d.dirty);
        assert_eq!(d.draft, expected);
    }

    #[test]
    fn folder_deletion_preserves_all_macro_fields_assets_order_and_selection() {
        for rename_target in [42, 43] {
            let (_dir, mut d) = folder_deletion_dialog();
            d.begin_folder_rename(rename_target);
            d.folder_rename_text = "Uncommitted rename".into();
            d.folder_error = Some("Rename error".into());
            let before = d.draft.clone();
            let baseline = d.baseline.clone();
            let disk_before = fs::read(_dir.path().join(MKMACROS_FILE)).unwrap();
            let assets: Vec<_> = before
                .macros
                .iter()
                .map(|_| {
                    let path = d
                        .store
                        .image_path(&crate::mkmacro::MkImageRef::from_filename("shared.png"))
                        .unwrap();
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                })
                .collect();
            let mut expected_selection = d.selection.clone();
            d.request_delete_folder(42);
            d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
            let mut expected = before.clone();
            expected.folders.remove(0);
            for m in &mut expected.macros {
                if m.folder_id == Some(42) {
                    m.folder_id = None;
                }
            }
            // Full structural equality covers every macro field, unrelated membership,
            // global order, settings, and all surviving folders.
            assert_eq!(d.draft, expected);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, expected_selection.ids);
            d.selection.click(&[31, 32, 33], 0, false, true);
            expected_selection.click(&[31, 32, 33], 0, false, true);
            assert_eq!(
                d.selection.ids, expected_selection.ids,
                "selection anchor survives"
            );
            assert_eq!(d.collapsed_folders, HashSet::from([43]));
            if rename_target == 42 {
                assert_eq!(d.pending_folder_rename, None);
                assert!(d.folder_rename_text.is_empty());
                assert!(!d.folder_rename_needs_focus);
                assert_eq!(d.folder_error, None);
            } else {
                assert_eq!(d.pending_folder_rename, Some(43));
                assert_eq!(d.folder_rename_text, "Uncommitted rename");
                assert!(d.folder_rename_needs_focus);
                assert_eq!(d.folder_error.as_deref(), Some("Rename error"));
            }
            assert_eq!(d.pending_delete_folder, None);
            assert!(!d.folder_delete_confirmation.is_open());
            assert!(!d.delete_confirmation.is_open());
            assert!(d.dirty);
            assert!(Arc::ptr_eq(&baseline, &d.baseline));
            assert_eq!(*d.store.snapshot(), before);
            assert_eq!(
                fs::read(_dir.path().join(MKMACROS_FILE)).unwrap(),
                disk_before
            );
            for (path, bytes) in assets {
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn folder_deletion_cancellation_changes_only_pending_deletion_state() {
        for dirty in [false, true] {
            let (_dir, mut d) = folder_deletion_dialog();
            d.dirty = dirty;
            d.begin_folder_rename(42);
            d.folder_rename_text = "Uncommitted rename".into();
            d.folder_error = Some("Rename error".into());
            d.search = "query".into();
            d.request_delete_selected_macro();
            let before = d.draft.clone();
            let baseline = d.baseline.clone();
            let mut selection = d.selection.clone();
            d.request_delete_folder(42);
            d.handle_folder_delete_confirmation(ConfirmationResult::None);
            assert_eq!(d.pending_delete_folder, Some(42));
            d.handle_folder_delete_confirmation(ConfirmationResult::Cancelled);
            assert_eq!(d.pending_delete_folder, None);
            assert!(!d.folder_delete_confirmation.is_open());
            assert_eq!(d.draft, before);
            assert_eq!(*d.store.snapshot(), before);
            assert!(Arc::ptr_eq(&baseline, &d.baseline));
            assert_eq!(d.dirty, dirty);
            assert!(!d.conflict);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, selection.ids);
            d.selection.click(&[31, 32, 33], 0, false, true);
            selection.click(&[31, 32, 33], 0, false, true);
            assert_eq!(d.selection.ids, selection.ids);
            assert_eq!(d.collapsed_folders, HashSet::from([42, 43]));
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Uncommitted rename");
            assert!(d.folder_rename_needs_focus);
            assert_eq!(d.folder_error.as_deref(), Some("Rename error"));
            assert_eq!(d.search, "query");
            assert!(d.delete_confirmation.is_open());
        }
    }

    #[test]
    fn folder_deletion_stale_confirmation_after_external_sync_is_harmless() {
        let (_dir, mut d) = folder_deletion_dialog();
        d.request_delete_folder(42);
        let mut external = d.draft.clone();
        external.folders.remove(0);
        for m in &mut external.macros {
            if m.folder_id == Some(42) {
                m.folder_id = None;
            }
        }
        d.store.save(external.clone()).unwrap();
        d.sync_external();
        assert_eq!(d.pending_delete_folder, None);
        d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
        assert_eq!(d.draft, external);
        assert!(!d.dirty);
        assert_eq!(d.selected_macro_id, Some(3));
        assert!(!d.folder_delete_confirmation.is_open());
    }

    #[test]
    fn folder_deletion_revalidates_target_before_mutating_any_members() {
        let (_dir, mut d) = folder_deletion_dialog();
        d.request_delete_folder(42);
        d.draft.folders.remove(0);
        // Even dangling membership must be untouched when the target is absent.
        let before = d.draft.clone();
        d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
        d.request_delete_folder(42);
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
    }

    fn assert_no_pending_folder_operations(d: &MkMacroDialog) {
        assert_eq!(d.pending_folder_rename, None);
        assert!(d.folder_rename_text.is_empty());
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
        assert_eq!(d.folder_error, None);
    }

    #[test]
    fn folder_collapse_changes_only_presentation_state() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.folder_rename_text = "Uncommitted rename".into();
        d.request_delete_folder(42);
        d.folder_error = Some("Existing validation error".into());
        d.search = "Utilities".into();
        let draft = d.draft.clone();
        let baseline = d.baseline.clone();
        let store = d.store.snapshot();
        for collapsed in [true, false] {
            d.toggle_folder_collapsed(42);
            assert_eq!(d.is_folder_collapsed(42), collapsed);
            assert_eq!(d.collapsed_folders.len(), usize::from(collapsed));
            assert_eq!(d.draft, draft);
            assert!(Arc::ptr_eq(&d.baseline, &baseline));
            assert!(Arc::ptr_eq(&d.store.snapshot(), &store));
            assert!(!d.dirty);
            assert!(!d.conflict);
            assert_eq!(d.selected_macro_id, None);
            assert_eq!(d.search, "Utilities");
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Uncommitted rename");
            assert_eq!(d.pending_delete_folder, Some(42));
            assert!(d.folder_delete_confirmation.is_open());
            assert_eq!(d.folder_error.as_deref(), Some("Existing validation error"));
        }
        d.draft.folders[0].name = "Dirty Utilities".into();
        d.mark_dirty();
        d.toggle_folder_collapsed(42);
        assert!(
            d.dirty,
            "collapse must also preserve an already dirty draft"
        );
    }

    #[test]
    fn folder_reload_and_discard_clear_pending_operations_and_stale_collapse() {
        for close in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.draft.folders.push(crate::mkmacro::MkMacroFolder {
                id: 99,
                name: "Draft only".into(),
            });
            d.mark_dirty();
            d.toggle_folder_collapsed(42);
            d.toggle_folder_collapsed(99);
            d.begin_folder_rename(99);
            d.request_delete_folder(99);
            d.folder_error = Some("Invalid name".into());
            assert!(if close {
                d.close_with_decision(DirtyDecision::Discard)
            } else {
                d.reload_with_decision(DirtyDecision::Discard)
            });
            assert_no_pending_folder_operations(&d);
            assert_eq!(d.collapsed_folders, HashSet::from([42]));
            assert_eq!(d.draft, *d.baseline);
            assert!(!d.dirty);
        }
    }

    #[test]
    fn folder_reload_cancels_even_still_valid_pending_operations() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        assert!(d.reload_with_decision(DirtyDecision::Discard));
        assert_no_pending_folder_operations(&d);
    }

    #[test]
    fn folder_close_paths_cancel_operations_but_preserve_collapse_on_reopen() {
        for close_children in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.open();
            d.toggle_folder_collapsed(42);
            d.begin_folder_rename(42);
            d.request_delete_folder(42);
            if close_children {
                d.open = false;
                d.close_children();
            } else {
                assert!(d.close_with_decision(DirtyDecision::KeepEditing));
            }
            assert_no_pending_folder_operations(&d);
            d.open();
            assert!(d.is_folder_collapsed(42));
            assert!(!d.dirty);
        }
    }

    #[test]
    fn folder_keep_editing_preserves_pending_operations() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        d.draft.folders[0].name = "Dirty Utilities".into();
        d.mark_dirty();
        assert!(!d.reload_with_decision(DirtyDecision::KeepEditing));
        assert!(!d.close_with_decision(DirtyDecision::KeepEditing));
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.pending_delete_folder, Some(42));
        assert!(d.folder_delete_confirmation.is_open());
    }

    #[test]
    fn folder_sync_prunes_against_draft_even_without_external_changes() {
        let (_dir, mut d) = folder_dialog();
        d.toggle_folder_collapsed(42);
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        d.draft.folders.clear();
        d.mark_dirty();
        d.sync_external();
        assert!(d.collapsed_folders.is_empty());
        assert_no_pending_folder_operations(&d);
        assert!(d.dirty);
    }

    #[test]
    fn folder_external_reload_prunes_removed_ids_and_keeps_remaining_collapse() {
        let (_dir, mut d) = folder_dialog();
        d.toggle_folder_collapsed(42);
        d.collapsed_folders.insert(99);
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        let mut external = d.draft.clone();
        external.folders[0].name = "Updated".into();
        d.store.save(external).unwrap();
        d.sync_external();
        assert_eq!(d.collapsed_folders, HashSet::from([42]));
        assert_no_pending_folder_operations(&d);
        let mut external = d.draft.clone();
        external.folders.clear();
        d.store.save(external).unwrap();
        d.sync_external();
        assert!(d.collapsed_folders.is_empty());
        assert!(!d.dirty);
    }

    #[test]
    fn folder_requests_validate_ids_and_rename_can_be_cancelled() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.folder_rename_text, "Utilities");
        d.cancel_folder_rename();
        assert_no_pending_folder_operations(&d);
        d.begin_folder_rename(999);
        d.request_delete_folder(999);
        d.toggle_folder_collapsed(999);
        assert_no_pending_folder_operations(&d);
        assert!(d.collapsed_folders.is_empty());
        assert!(!d.dirty);
    }

    #[test]
    fn folder_ui_state_is_absent_from_serialized_document() {
        let (_dir, mut d) = folder_dialog();
        let before = serde_json::to_value(&d.draft).unwrap();
        d.toggle_folder_collapsed(42);
        d.begin_folder_rename(42);
        d.folder_rename_text = "Transient rename".into();
        d.request_delete_folder(42);
        d.folder_error = Some("Transient error".into());
        let after = serde_json::to_value(&d.draft).unwrap();
        assert_eq!(before, after);
        assert_eq!(
            after["folders"],
            serde_json::json!([{"id": 42, "name": "Utilities"}])
        );
        for field in [
            "collapsed_folders",
            "pending_folder_rename",
            "folder_rename_text",
            "pending_delete_folder",
            "folder_delete_confirmation",
            "folder_error",
        ] {
            assert!(
                after.get(field).is_none(),
                "unexpected serialized field: {field}"
            );
            assert!(after["folders"][0].get(field).is_none());
        }
        let round_trip: MkMacroDocument = serde_json::from_value(after).unwrap();
        assert_eq!(round_trip, d.draft);
    }

    fn picker_matcher(title: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            title: Some(title.into()),
            ..Default::default()
        }
    }

    fn picker_macro(id: u64, hotkey_scope: MkHotkeyScope) -> MkMacro {
        MkMacro {
            signature: Default::default(),
            id,
            name: format!("Macro {id}"),
            description: format!("Description {id}"),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Function(1),
                modifiers: vec![MkKey::Control],
            }),
            hotkey_scope,
            folder_id: Some(9),
            playback: Default::default(),
            steps: vec![],
        }
    }

    fn picker_request(
        macro_id: u64,
        original: MkWindowMatcher,
    ) -> window_picker::MatcherEditRequest {
        window_picker::MatcherEditRequest {
            destination: window_picker::MatcherDestination::MacroHotkey { macro_id },
            original,
        }
    }

    #[test]
    fn picker_confirmation_targets_stable_macro_id_after_selection_changes() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let original_22 = picker_matcher("Original 22");
        let before_17 = picker_macro(17, MkHotkeyScope::ActiveWindow(original_17.clone()));
        let before_22 = picker_macro(22, MkHotkeyScope::ActiveWindow(original_22.clone()));
        d.draft.macros = vec![before_17.clone(), before_22.clone()];
        d.selected_macro_id = Some(22);

        let replacement = picker_matcher("Picked 17");
        let request = picker_request(17, original_17);
        assert!(d.apply_window_picker_confirmation(&request, replacement.clone()));

        let mut expected_17 = before_17;
        expected_17.hotkey_scope = MkHotkeyScope::ActiveWindow(replacement);
        assert_eq!(d.draft.macros[0], expected_17);
        assert_eq!(d.draft.macros[1], before_22);
        assert!(d.dirty);
    }

    #[test]
    fn picker_confirmation_discards_deleted_macro() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let before_22 = picker_macro(
            22,
            MkHotkeyScope::ActiveWindow(picker_matcher("Original 22")),
        );
        d.draft.macros = vec![
            picker_macro(17, MkHotkeyScope::ActiveWindow(original_17.clone())),
            before_22.clone(),
        ];
        d.draft.macros.retain(|macro_| macro_.id != 17);
        d.selected_macro_id = Some(22);

        assert!(!d.apply_window_picker_confirmation(
            &picker_request(17, original_17),
            picker_matcher("Picked 17"),
        ));
        assert_eq!(d.draft.macros, vec![before_22]);
        assert!(!d.dirty);
    }

    #[test]
    fn picker_confirmation_discards_macro_that_switched_to_any_window() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let before_17 = picker_macro(17, MkHotkeyScope::AnyWindow);
        d.draft.macros = vec![before_17.clone()];
        d.selected_macro_id = Some(22);

        assert!(!d.apply_window_picker_confirmation(
            &picker_request(17, original_17),
            picker_matcher("Picked 17"),
        ));
        assert_eq!(d.draft.macros, vec![before_17]);
        assert!(!d.dirty);
    }

    #[test]
    fn picker_confirmation_rejects_changed_original_matcher() {
        let (_dir, mut d) = dialog();
        let original = picker_matcher("Original");
        let changed = picker_matcher("Changed");
        d.draft.macros = vec![picker_macro(17, MkHotkeyScope::ActiveWindow(changed.clone()))];
        assert!(!d.apply_window_picker_confirmation(&picker_request(17, original), picker_matcher("Picked")));
        assert_eq!(d.draft.macros[0].hotkey_scope, MkHotkeyScope::ActiveWindow(changed));
        assert!(!d.dirty);
    }

    #[test]
    fn macro_hotkey_picker_inherits_retained_parent_and_rejects_retired_origin() {
        use super::super::utility_host::{AcceptedParentOpen, UtilityPresentation, ViewportPassProvenance};
        let (directory, mut d) = dialog();
        let original = picker_matcher("Original");
        d.draft.macros = vec![picker_macro(17, MkHotkeyScope::ActiveWindow(original.clone()))];
        d.selected_macro_id = Some(17);
        d.open = true;
        d.host_state.transition(UtilityPresentation::Popout);
        d.window_picker.test_candidates = Some(Vec::new());
        let ctx = eframe::egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(directory.path().join("settings.json").to_string_lossy().into_owned(), workers.clone(), crate::settings::UtilityPresentations::default());
        manager.install_in_context(&ctx);
        manager.remember(&ctx, crate::settings::UtilityToolKey::parse("mkmacro-window-picker").unwrap(), crate::settings::RememberedPresentation::Docked);
        let pass = ViewportPassProvenance::current(&ctx, eframe::egui::ViewportId::ROOT);
        let output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
        let origin = AcceptedParentOpen::from_retained(&ctx, d.host_state.identity(), d.owner_instance, pass, pass).unwrap();
        output.drop_without_applying_deltas();
        let request = picker_request(17, original);
        d.pending_window_picker_open = Some((request.clone(), PickerOpenOrigin::Main(origin.clone())));
        assert!(!d.complete_window_picker_child(&ctx));
        assert!(d.window_picker.open && d.window_picker_host.is_popped_out());
        let epoch = d.window_picker.epoch;
        d.window_picker.search = "unsaved filter".into();
        d.pending_window_picker_open = Some((request.clone(), PickerOpenOrigin::Main(origin.clone())));
        assert!(!d.complete_window_picker_child(&ctx));
        assert_eq!(d.window_picker.epoch, epoch);
        assert_eq!(d.window_picker.search, "unsaved filter");
        d.window_picker_host.close_from_app(&ctx);
        d.window_picker.cancel("closed");
        d.host_state.transition(UtilityPresentation::Docked);
        d.pending_window_picker_open = Some((request, PickerOpenOrigin::Main(origin)));
        assert!(!d.complete_window_picker_child(&ctx));
        assert!(!d.window_picker.open);
        workers.join_all();
    }

    #[test]
    fn live_step_editor_reopens_from_native_parent_without_replacing_draft() {
        use super::super::utility_host::{AcceptedParentOpen, ChildOpenOrigin, UtilityPresentation, ViewportPassProvenance};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let step = dialog.selected_macro().unwrap().steps[0].clone();
        dialog.action_editor.begin_edit_in_macro(Some(1), &step);
        dialog.action_editor.draft.as_mut().unwrap().delay_after_ms = 987;
        let epoch = dialog.action_editor.host_epoch;
        let ctx = eframe::egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory.path().join("settings.json").to_string_lossy().into_owned(),
            workers.clone(), crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        dialog.editor_host.open_from_origin(&ctx, &dialog.action_editor, false, &ChildOpenOrigin::<()>::Root);
        assert!(dialog.editor_host.root_layer().is_some());
        dialog.host_state.transition(UtilityPresentation::Popout);
        let pass = ViewportPassProvenance::current(&ctx, eframe::egui::ViewportId::ROOT);
        let output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
        let origin = AcceptedParentOpen::from_retained(&ctx, dialog.host_state.identity(), dialog.owner_instance, pass, pass).unwrap();
        output.drop_without_applying_deltas();
        dialog.defer_host_effects = true;
        dialog.open_action_editor_from_main(step.clone());
        let staged = dialog.pending_host_effects.pop().expect("same-step open is staged");
        assert!(dialog.host_action_is_current(&staged, dialog.host_state.identity(), dialog.owner_instance));
        dialog.pending_editor_origin = Some((EditorOpenOrigin::Main(origin), true));
        assert!(!dialog.complete_editor_child(&ctx));
        assert!(dialog.editor_host.is_popped_out());
        assert_eq!(dialog.action_editor.host_epoch, epoch);
        assert_eq!(dialog.action_editor.draft.as_ref().unwrap().delay_after_ms, 987);
        assert!(dialog.action_editor.edits_source(Some(1), &step));
        workers.join_all();
    }

    #[test]
    fn picker_confirmation_replaces_matching_active_window_matcher_and_marks_dirty() {
        let (_dir, mut d) = dialog();
        let original = picker_matcher("Original");
        d.draft.macros = vec![picker_macro(
            17,
            MkHotkeyScope::ActiveWindow(original.clone()),
        )];
        d.selected_macro_id = None;

        let replacement = picker_matcher("Picked");
        assert!(
            d.apply_window_picker_confirmation(&picker_request(17, original), replacement.clone(),)
        );
        assert_eq!(
            d.draft.macros[0].hotkey_scope,
            MkHotkeyScope::ActiveWindow(replacement),
        );
        assert!(d.dirty);
    }

    #[test]
    fn picker_confirmation_of_identical_matcher_does_not_mark_dirty() {
        let (_dir, mut d) = dialog();
        let original = picker_matcher("Original");
        d.draft.macros = vec![picker_macro(
            17,
            MkHotkeyScope::ActiveWindow(original.clone()),
        )];
        d.selected_macro_id = Some(22);

        assert!(
            d.apply_window_picker_confirmation(&picker_request(17, original.clone()), original,)
        );
        assert_eq!(
            d.draft.macros[0].hotkey_scope,
            MkHotkeyScope::ActiveWindow(picker_matcher("Original")),
        );
        assert!(!d.dirty);
    }

    #[test]
    fn image_shortcuts_insert_around_stable_nested_anchor_and_report_change() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let matcher = crate::mkmacro::MkWindowMatcher {
            title: Some("Exact".into()),
            title_regex: Some("E.*".into()),
            process: Some("app.exe".into()),
            class: Some("Widget".into()),
        };
        let payload = MkImagePayload {
            image: crate::mkmacro::MkImageRef::from_filename("10.png"),
            region: SearchRegion::ClientArea {
                matcher: matcher.clone(),
            },
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            wait: MkWaitOptions {
                timeout_ms: 1000,
                poll_interval_ms: 50,
            },
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        };
        let steps = &mut d.selected_macro_mut().unwrap().steps;
        steps.extend([
            MkStep {
                metadata: Default::default(),
                id: 11,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::RepeatStart { count: 2 },
            },
            MkStep {
                metadata: Default::default(),
                id: 12,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::ImageFind(payload.clone()),
            },
            MkStep {
                metadata: Default::default(),
                id: 13,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::RepeatEnd,
            },
        ]);
        assert!(action_editor::insert_smooth_move_after(
            &mut d, 12, &payload
        ));
        assert!(action_editor::insert_activate_window_before(
            &mut d, 12, &payload
        ));
        let steps = &d.selected_macro().unwrap().steps;
        let anchor = steps.iter().position(|s| s.id == 12).unwrap();
        assert!(
            matches!(&steps[anchor - 1].action, MkAction::WindowActivate(p) if p.matcher == matcher)
        );
        assert!(matches!(
            &steps[anchor + 1].action,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Image {
                    image: _,
                    offset: crate::mkmacro::variables::MkPoint { x: 0, y: 0 }
                },
                duration_ms: 500
            })
        ));
        let ids: std::collections::HashSet<_> = steps.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), steps.len());
        assert!(steps.iter().all(|s| s.id != 0));
        assert!(d.dirty);
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
    }

    fn assert_created_macro(d: &MkMacroDialog, folder_id: Option<u64>) {
        let created = d.draft.macros.last().unwrap();
        assert_ne!(created.id, 0);
        assert_eq!(d.selected_macro_id, Some(created.id));
        assert!(d.selection.ids.is_empty());
        assert!(d.dirty);
        assert_eq!(
            created,
            &MkMacro {
                signature: Default::default(),
                id: created.id,
                name: "New Macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id,
                playback: Default::default(),
                steps: vec![],
            }
        );
    }

    #[test]
    fn global_creation_is_unfiled_even_when_a_folder_macro_is_selected() {
        let (_dir, mut d) = folder_dialog();
        assert!(d.create_macro_in_folder(Some(42)));
        let source = d.selected_macro().unwrap().clone();
        d.selection.click(&[17, 18], 0, false, false);
        d.dirty = false;

        d.create_macro();

        assert_created_macro(&d, None);
        assert_eq!(d.draft.macros.len(), 2);
        assert_eq!(d.draft.macros[0], source);
        assert_ne!(d.selected_macro_id, Some(source.id));
    }

    #[test]
    fn folder_context_creation_preserves_defaults_and_selects_repaired_id() {
        let (_dir, mut d) = folder_dialog();
        d.selection.click(&[17, 18], 0, false, false);
        let before = d.draft.clone();

        assert!(d.create_macro_in_folder(Some(42)));

        assert_created_macro(&d, Some(42));
        assert_eq!(d.draft.macros.len(), before.macros.len() + 1);
        assert_eq!(d.draft.folders, before.folders);
        assert!(!repair_ids(&mut d.draft));
    }

    #[test]
    fn invalid_destination_creation_is_atomic_even_for_an_existing_zero_folder() {
        let (_dir, mut d) = folder_dialog();
        d.create_macro();
        d.draft.folders.push(MkMacroFolder {
            id: 0,
            name: "Invalid folder".into(),
        });
        d.selection.click(&[17, 18], 0, false, false);
        let before = d.draft.clone();
        let selected = d.selected_macro_id;
        let selection = d.selection.clone();

        for dirty in [false, true] {
            d.dirty = dirty;
            for folder_id in [0, 999] {
                assert!(!d.create_macro_in_folder(Some(folder_id)));
                assert_eq!(d.draft, before);
                assert_eq!(d.selected_macro_id, selected);
                assert_eq!(d.selection.ids, selection.ids);
                assert_eq!(d.dirty, dirty);
            }
        }
        // Rejection must also retain the selection anchor.
        d.selection.click(&[17, 18], 1, false, true);
        assert_eq!(d.selection.ids, [17, 18].into_iter().collect());
    }

    #[test]
    fn duplication_preserves_membership_source_and_folders_but_clears_only_copy_hotkey() {
        for folder_id in [Some(42), None] {
            let (_dir, mut d) = folder_dialog();
            d.draft.macros = five_macros().macros;
            d.draft.folders.push(MkMacroFolder {
                id: 43,
                name: "Other folder".into(),
            });
            d.set_selected_macro(Some(2));
            let source = d.selected_macro_mut().unwrap();
            source.folder_id = folder_id;
            source.description = "Keep the source intact".into();
            source.enabled = false;
            source.hotkey = Some(MkHotkey {
                key: MkKey::Function(8),
                modifiers: vec![MkKey::Control],
            });
            source.hotkey_scope = MkHotkeyScope::ActiveWindow(picker_matcher("Editor"));
            source.steps = [17, 18]
                .into_iter()
                .map(|id| MkStep {
                    metadata: Default::default(),
                    id,
                    enabled: true,
                    breakpoint: false,
                    repeat: 2,
                    delay_after_ms: 25,
                    on_error: Default::default(),
                    action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                        fixed_ms: id,
                        ..Default::default()
                    }),
                })
                .collect();
            let before = d.draft.clone();
            d.selection.click(&[17, 18], 0, false, false);
            d.dirty = false;

            d.duplicate_selected_macro();

            let source = &before.macros[1];
            let copy = d.draft.macros.last().unwrap();
            assert_eq!(copy.folder_id, source.folder_id);
            assert!(copy.hotkey.is_none());
            assert!(d.draft.macros[1].hotkey.is_some());
            assert_eq!(d.draft.macros[1].hotkey, source.hotkey);
            assert_ne!(copy.id, 0);
            assert!(before.macros.iter().all(|m| m.id != copy.id));
            let copied_ids: HashSet<_> = copy.steps.iter().map(|s| s.id).collect();
            assert_eq!(copied_ids.len(), source.steps.len());
            assert!(!copied_ids.contains(&0));
            assert!(source.steps.iter().all(|s| !copied_ids.contains(&s.id)));
            let mut expected_copy = source.clone();
            expected_copy.id = copy.id;
            expected_copy.name.push_str(" Copy");
            expected_copy.hotkey = None;
            for (expected, actual) in expected_copy.steps.iter_mut().zip(&copy.steps) {
                expected.id = actual.id;
            }
            assert_eq!(copy, &expected_copy);
            assert_eq!(d.selected_macro_id, Some(copy.id));
            assert!(d.selection.ids.is_empty());
            assert!(d.dirty);
            // Appending must leave every original macro and folder in document order.
            let mut originals = d.draft.clone();
            originals.macros.pop();
            assert_eq!(originals, before);
            assert!(!repair_ids(&mut d.draft));
        }
    }

    #[test]
    fn create_duplicate_and_delete_are_draft_only() {
        let (_dir, mut d) = dialog();
        let baseline = d.store.snapshot();
        d.create_macro();
        let first = d.selected_macro_id.unwrap();
        assert_ne!(first, 0);
        d.selected_macro_mut().unwrap().hotkey = Some(MkHotkey {
            key: MkKey::Function(8),
            modifiers: vec![MkKey::Control],
        });
        d.selected_macro_mut().unwrap().steps.push(MkStep {
            metadata: Default::default(),
            id: 0,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }),
        });
        repair_ids(&mut d.draft);
        d.duplicate_selected_macro();
        let copy = d.selected_macro().unwrap();
        assert_ne!(copy.id, first);
        assert!(copy.hotkey.is_none());
        assert_eq!(copy.name, "New Macro Copy");
        assert!(copy.steps.iter().all(|s| s.id != 0));
        d.delete_selected_macro();
        assert_eq!(d.selected_macro_id, Some(first));
        assert!(d.dirty);
        assert!(Arc::ptr_eq(&baseline, &d.store.snapshot()));
    }

    #[test]
    fn delete_every_macro_save_empty_and_reopen_loaded_document() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = Arc::new(store);
        store.save(five_macros()).unwrap();
        let mut dialog = MkMacroDialog::new(Arc::clone(&store));

        while !dialog.draft.macros.is_empty() {
            let id = dialog.draft.macros[0].id;
            dialog.selected_macro_id = Some(id);
            dialog.delete_selected_macro();
            if dialog.draft.macros.is_empty() {
                assert!(dialog.selected_macro_id.is_none());
            } else {
                let selected = dialog.selected_macro_id.expect("selection advances");
                assert_ne!(selected, id);
                assert!(dialog.draft.macros.iter().any(|m| m.id == selected));
            }
        }
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.selected_macro_id.is_none());
        assert!(dialog.dirty);
        assert_eq!(
            store.snapshot().macros.len(),
            5,
            "deletes remain draft-only"
        );

        dialog.save().unwrap();
        assert!(dialog.store.snapshot().macros.is_empty());
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.dirty);
        assert!(!dialog.conflict);

        let path = dir.path().join(MKMACROS_FILE);
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        let disk: MkMacroDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(disk.schema_version, SCHEMA_VERSION);
        assert!(disk.macros.is_empty());

        drop(dialog);
        drop(store);
        let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        assert!(reopened.snapshot().macros.is_empty());
    }

    #[test]
    fn watcher_and_external_sync_cannot_restore_macros_after_empty_save() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = Arc::new(store);
        store.save(five_macros()).unwrap();
        let mut dialog = MkMacroDialog::new(store);
        dialog.draft.macros.clear();
        dialog.dirty = true;
        dialog.save().unwrap();
        dialog.sync_external();
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.conflict);

        wait_for_empty_and_stable(&mut dialog, &dir.path().join(MKMACROS_FILE));
        dialog.sync_external();
        assert!(dialog.store.snapshot().macros.is_empty());
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.conflict);
    }
    #[test]
    fn catalog_search_and_structures() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        assert!(
            action_catalog::descriptors()
                .iter()
                .any(|x| action_catalog::matches(x, "launch"))
        );
        action_catalog::insert_action(
            &mut d,
            MkAction::If(crate::mkmacro::MkCondition::All { conditions: vec![] }),
        );
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
        action_catalog::insert_action(&mut d, MkAction::RepeatStart { count: 2 });
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
    }

    #[test]
    fn structural_editor_is_atomic_configurable_and_cancelable() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("Repeat"));
        assert!(d.selected_macro().unwrap().steps.is_empty());
        if let MkAction::RepeatStart { count } = &mut d.action_editor.draft.as_mut().unwrap().action
        {
            *count = 9;
        }
        let mut editor = d.take_action_editor();
        editor.apply(&mut d).unwrap();
        d.action_editor = editor;
        assert!(matches!(
            d.selected_macro().unwrap().steps[0].action,
            MkAction::RepeatStart { count: 9 }
        ));
        assert!(matches!(
            d.selected_macro().unwrap().steps[1].action,
            MkAction::RepeatEnd
        ));
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
        let before = d.selected_macro().unwrap().steps.len();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("While"));
        d.action_editor.cancel();
        assert_eq!(d.selected_macro().unwrap().steps.len(), before);
    }

    #[test]
    fn wrapping_rejects_noncontiguous_selection_atomically() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        for milliseconds in 1..=3 {
            action_catalog::insert_action(
                &mut d,
                MkAction::Delay(crate::mkmacro::MkDelayPayload {
                    fixed_ms: milliseconds,
                    ..Default::default()
                }),
            );
        }
        let ids: Vec<_> = d
            .selected_macro()
            .unwrap()
            .steps
            .iter()
            .map(|s| s.id)
            .collect();
        d.selection.ids = [ids[0], ids[2]].into_iter().collect();
        let before = serde_json::to_vec(d.selected_macro().unwrap()).unwrap();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("If"));
        let mut editor = d.take_action_editor();
        assert!(editor.apply(&mut d).is_none());
        d.action_editor = editor;
        assert_eq!(
            serde_json::to_vec(d.selected_macro().unwrap()).unwrap(),
            before
        );
        assert!(d.command_error.as_deref().unwrap().contains("contiguous"));
    }

    #[test]
    fn direct_loop_controls_require_context_and_keep_palette() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        let before = serde_json::to_vec(d.selected_macro().unwrap()).unwrap();
        action_catalog::insert_action(&mut d, MkAction::Break);
        assert_eq!(
            serde_json::to_vec(d.selected_macro().unwrap()).unwrap(),
            before
        );
        assert!(d.command_error.is_some());
        assert!(d.catalog.open);
        action_catalog::insert_action(&mut d, MkAction::RepeatStart { count: 5 });
        let opener = d.selected_macro().unwrap().steps[0].id;
        d.selection.ids.clear();
        d.selection.ids.insert(opener);
        action_catalog::insert_action(&mut d, MkAction::Continue);
        assert!(matches!(
            d.selected_macro().unwrap().steps[1].action,
            MkAction::Continue
        ));
        assert!(d.catalog.open);
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
    }
    // Independent expectations for every model variant, including hidden actions.
    // Do not add a catch-all: new variants need an explicit capability decision.
    fn expected_action_contract(action: &MkAction) -> (action_catalog::EditorKind, bool) {
        use action_catalog::EditorKind;
        match action {
            MkAction::CallMacro(_) => (EditorKind::CallMacro, true),
            MkAction::Return(_) => (EditorKind::Return, true),
            MkAction::KeyDown(_)
            | MkAction::KeyUp(_)
            | MkAction::KeyPress(_)
            | MkAction::Hotkey(_) => (EditorKind::Keyboard, true),
            MkAction::Text(_) => (EditorKind::Text, true),
            MkAction::Notify(_) => (EditorKind::Notify, cfg!(windows)),
            MkAction::PlaySound(_) => (EditorKind::PlaySound, true),
            MkAction::MouseMove(_) => (EditorKind::MouseMove, true),
            MkAction::MouseDrag(_) => (EditorKind::MouseDrag, true),
            MkAction::MouseClick(_) | MkAction::ClickWithinRegion(_) => {
                (EditorKind::MouseClick, true)
            }
            MkAction::MouseDown(_) | MkAction::MouseUp(_) => (EditorKind::MouseButton, true),
            MkAction::MouseScroll { .. } => (EditorKind::MouseScroll, true),
            MkAction::Delay(_) => (EditorKind::Timing, true),
            MkAction::Process(_) => (EditorKind::Process, true),
            MkAction::LauncherCommand(_) => (EditorKind::Launcher, true),
            MkAction::OpenFolder(_) => (EditorKind::OpenFolder, cfg!(windows)),
            MkAction::OpenUrl(_) => (EditorKind::OpenUrl, cfg!(windows)),
            MkAction::WindowActivate(_)
            | MkAction::WindowClose(_)
            | MkAction::WindowWait(_)
            | MkAction::WindowMoveResize(_)
            | MkAction::WindowState { .. } => (EditorKind::Window, true),
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { .. }) => {
                (EditorKind::VirtualDesktop, cfg!(windows))
            }
            MkAction::VirtualDesktop(
                MkVirtualDesktopAction::Create
                | MkVirtualDesktopAction::SwitchLeft
                | MkVirtualDesktopAction::SwitchRight
                | MkVirtualDesktopAction::CloseCurrent,
            ) => (EditorKind::DirectInsert, cfg!(windows)),
            MkAction::If(_) | MkAction::WhileStart { .. } | MkAction::WaitUntil { .. } => {
                (EditorKind::Condition, true)
            }
            MkAction::RepeatStart { .. } => (EditorKind::Repeat, true),
            MkAction::SetVariable { .. } | MkAction::UnsetVariable { .. } => {
                (EditorKind::Variable, true)
            }
            MkAction::ReadClipboardText { .. } => (EditorKind::ReadClipboardText, true),
            MkAction::WriteClipboardText(_) => (EditorKind::WriteClipboardText, true),
            MkAction::PromptInput(_) => (EditorKind::PromptInput, true),
            MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatEnd
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue => (EditorKind::DirectInsert, true),
            MkAction::ImageFind(_) | MkAction::ImageClick(_) => (EditorKind::Image, true),
            MkAction::OcrFindText(_) | MkAction::OcrClickText(_) => {
                (EditorKind::OcrSearch, cfg!(windows))
            }
            MkAction::OcrReadText(_) => (EditorKind::OcrRead, cfg!(windows)),
            MkAction::CaptureScreenshot(_) | MkAction::WaitForVisualChange(_) => {
                (EditorKind::Screenshot, true)
            }
            MkAction::PixelCheck { .. } | MkAction::FindPixel(_) => (EditorKind::Pixel, true),
            MkAction::UiInvoke(_)
            | MkAction::UiSetValue { .. }
            | MkAction::UiReadValue { .. }
            | MkAction::UiToggle(_)
            | MkAction::UiSelect(_)
            | MkAction::UiFocus(_)
            | MkAction::UiWait(_) => (EditorKind::General, false),
        }
    }

    fn detail_examples() -> [(MkAction, &'static str); 6] {
        use crate::mkmacro::{MkClickWithinRegionPayload, MkDelayMode, MkDelayPayload, ScreenRect};
        [
            (
                MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
                    rect: ScreenRect::new(500, 300, 400, 200),
                    button: MkMouseButton::Left,
                    clicks: 1,
                    edge_padding_px: 10,
                }),
                "400×200 @ (500,300) · Left · random · padding 10",
            ),
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::RandomRange,
                    // Inactive fixed duration must not affect this action.
                    fixed_ms: u64::MAX,
                    minimum_ms: 250,
                    maximum_ms: 1250,
                }),
                "Random 250–1250 ms",
            ),
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::Fixed,
                    fixed_ms: 1000,
                    // Inactive range must not affect this action.
                    minimum_ms: u64::MAX,
                    maximum_ms: 0,
                }),
                "Fixed 1000 ms",
            ),
            (
                MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 3 }),
                "Desktop 3",
            ),
            (
                MkAction::ReadClipboardText {
                    variable: "clipboard_text".into(),
                },
                "Read text → clipboard_text",
            ),
            (
                MkAction::WriteClipboardText(MkWriteClipboardTextPayload {
                    source: MkValueSource::Variable {
                        name: "clipboard_text".into(),
                    },
                }),
                "Variable clipboard_text",
            ),
        ]
    }

    fn document_with_action(action: MkAction) -> MkMacroDocument {
        let mut document = five_macros();
        document.macros.truncate(1);
        document.macros[0].steps.push(MkStep {
            metadata: Default::default(),
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        });
        document
    }

    #[test]
    fn names_and_details_cover_catalog() {
        let descriptors = action_catalog::descriptors();
        let representatives = descriptors
            .iter()
            .map(|descriptor| (descriptor.make_default)())
            .chain(detail_examples().into_iter().map(|(action, _)| action));
        for action in representatives {
            let (editor, supported) = expected_action_contract(&action);
            let name = action_catalog::action_name(&action);
            assert!(!name.trim().is_empty(), "{action:?}: empty name");
            let details = action_catalog::action_details(&action);
            assert_eq!(
                details,
                action_catalog::action_details(&action.clone()),
                "{name}"
            );
            assert_eq!(
                details,
                action_catalog::action_details_with_assets(&action, &[]),
                "{name}"
            );
            assert_eq!(
                details,
                action_catalog::action_details_with_asset_name(&action, None),
                "{name}"
            );
            assert_eq!(action_catalog::editor_for_action(&action), editor, "{name}");
            assert_eq!(
                action_catalog::editor_route_recognizes(&action, editor),
                editor != action_catalog::EditorKind::General,
                "{name}: hidden UI Automation has no authoring route",
            );
            assert_eq!(
                crate::mkmacro::executor::has_runtime_support(&action),
                supported,
                "{name}: runtime classification",
            );
            let descriptor = descriptors.iter().find(|descriptor| {
                action_catalog::descriptor_name_matches_action(descriptor, &action)
            });
            // Legacy UI Automation intentionally shares an unavailable display name.
            if editor != action_catalog::EditorKind::General {
                let descriptor =
                    descriptor.unwrap_or_else(|| panic!("{name}: missing catalog entry"));
                assert_eq!(descriptor.editor, editor, "{name}: catalog editor");
            }

            // Standalone structural markers and missing assets may be invalid,
            // but validation and compilation must agree and neither may panic.
            let document = document_with_action(action);
            let diagnostics = validate_document(&document, None);
            match crate::mkmacro::compile(&document.macros[0]) {
                Ok(plan) => {
                    assert!(
                        crate::mkmacro::can_run(&diagnostics),
                        "{name}: {diagnostics:?}"
                    );
                    assert_eq!(
                        plan.instructions[0].step.action,
                        document.macros[0].steps[0].action
                    );
                }
                Err(errors) => {
                    assert!(!crate::mkmacro::can_run(&diagnostics), "{name}: {errors:?}");
                    assert_eq!(errors, diagnostics, "{name}");
                }
            }
        }
        for descriptor in descriptors {
            let (editor, supported) = expected_action_contract(&(descriptor.make_default)());
            assert_eq!(descriptor.editor, editor, "{}", descriptor.name);
            assert_eq!(
                descriptor.runtime,
                if supported {
                    action_catalog::RuntimeAvailability::Supported
                } else {
                    action_catalog::RuntimeAvailability::Unavailable
                },
                "{}",
                descriptor.name,
            );
        }
    }

    #[test]
    fn clipboard_action_editor_loaded_rows_are_named_redacted_and_supported() {
        let sentinel = "private_clipboard_sentinel";
        let action = MkAction::WriteClipboardText(MkWriteClipboardTextPayload {
            source: MkValueSource::Literal(MkValue::String(sentinel.into())),
        });
        assert_eq!(action_catalog::action_name(&action), "Write Clipboard Text");
        assert_eq!(
            action_catalog::editor_for_action(&action),
            action_catalog::EditorKind::WriteClipboardText
        );
        assert!(action_catalog::editor_route_recognizes(
            &action,
            action_catalog::EditorKind::WriteClipboardText
        ));
        assert!(!action_catalog::action_details(&action).contains(sentinel));
        assert!(!format!("{action:?}").contains(sentinel));
        assert!(action_catalog::descriptors().iter().any(|descriptor| {
            action_catalog::descriptor_name_matches_action(descriptor, &action)
        }));
        assert!(crate::mkmacro::executor::has_runtime_support(&action));
        assert!(matches!(
            action_catalog::draft_validation_contract(&action),
            action_catalog::DraftValidationContract::CommitReady
        ));

        let read = MkAction::ReadClipboardText {
            variable: "clipboard_text".into(),
        };
        assert_eq!(
            action_catalog::editor_for_action(&read),
            action_catalog::EditorKind::ReadClipboardText
        );
        assert!(action_catalog::editor_route_recognizes(
            &read,
            action_catalog::EditorKind::ReadClipboardText
        ));
        assert!(action_catalog::descriptors().iter().any(|descriptor| {
            action_catalog::descriptor_name_matches_action(descriptor, &read)
        }));
        assert!(crate::mkmacro::executor::has_runtime_support(&read));
        assert!(matches!(
            action_catalog::draft_validation_contract(&read),
            action_catalog::DraftValidationContract::CommitReady
        ));
    }

    #[test]
    fn direct_open_actions_are_typed_discoverable_and_configurable() {
        use crate::mkmacro::{MkOpenFolderPayload, MkOpenUrlPayload};

        let actions = [
            (
                MkAction::OpenFolder(MkOpenFolderPayload {
                    path: "C:\\work with spaces\\notes".into(),
                }),
                action_catalog::EditorKind::OpenFolder,
                "Open Folder",
                "C:\\work with spaces\\notes",
            ),
            (
                MkAction::OpenUrl(MkOpenUrlPayload {
                    url: "https://example.test/path?token=visible-in-authoring#part".into(),
                }),
                action_catalog::EditorKind::OpenUrl,
                "Open URL",
                "https://example.test/path?token=visible-in-authoring#part",
            ),
        ];

        for (action, editor, name, detail) in actions {
            assert_eq!(action_catalog::action_name(&action), name);
            assert_eq!(action_catalog::action_details(&action), detail);
            assert_eq!(action_catalog::editor_for_action(&action), editor);
            assert!(action_catalog::editor_route_recognizes(&action, editor));
            let descriptor = action_catalog::descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap_or_else(|| panic!("{name} missing from the action palette"));
            assert_eq!(
                descriptor.category,
                action_catalog::ActionCategory::ProgramsLauncher
            );
            assert_eq!(descriptor.editor, editor);
            assert_eq!(
                descriptor.runtime == action_catalog::RuntimeAvailability::Supported,
                cfg!(windows)
            );
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert!(!descriptor.description.trim().is_empty());
            assert_eq!(
                action_catalog::editor_contract(editor),
                Some(action_catalog::EditorContract::Configurable { field_count: 1 })
            );
            let completeness = action_catalog::editor_completeness(editor).unwrap();
            assert!(completeness.has_primary_control);
            assert!(!completeness.intentionally_disabled);
            assert!(completeness.placeholder_copy.is_none());
            assert_eq!(
                action_catalog::is_available_in_palette(&descriptor),
                cfg!(windows)
            );
            assert_eq!(
                crate::mkmacro::executor::has_runtime_support(&action),
                cfg!(windows)
            );
            assert_eq!(
                action_catalog::group_visible_descriptors(name)
                    .iter()
                    .flat_map(|(_, rows)| rows)
                    .any(|row| row.name == name),
                cfg!(windows)
            );
        }
        let descriptor_order = action_catalog::descriptors()
            .iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();
        assert!(
            descriptor_order
                .iter()
                .position(|name| *name == "Run Program")
                < descriptor_order
                    .iter()
                    .position(|name| *name == "Launcher Command")
        );
        assert!(
            descriptor_order
                .iter()
                .position(|name| *name == "Launcher Command")
                < descriptor_order
                    .iter()
                    .position(|name| *name == "Open Folder")
        );
        assert!(
            descriptor_order
                .iter()
                .position(|name| *name == "Open Folder")
                < descriptor_order.iter().position(|name| *name == "Open URL")
        );
        for (name, keyword) in [("Open Folder", "explorer"), ("Open URL", "https")] {
            let descriptor = action_catalog::descriptors()
                .into_iter()
                .find(|descriptor| descriptor.name == name)
                .unwrap();
            assert!(action_catalog::matches(&descriptor, keyword));
        }
    }

    #[test]
    fn region_delay_and_desktop_details_are_exact() {
        for (action, expected) in detail_examples() {
            assert_eq!(action_catalog::action_details(&action), expected);
            let document = document_with_action(action);
            assert!(crate::mkmacro::can_run(&validate_document(&document, None)));
            assert!(crate::mkmacro::compile(&document.macros[0]).is_ok());
        }
        assert_eq!(
            action_catalog::action_name(&MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo {
                desktop: 3,
            })),
            "Go To Virtual Desktop",
        );
    }

    #[test]
    fn region_and_desktop_unsupported_backend_diagnostics_are_explicit() {
        use crate::mkmacro::{Backends, DiagnosticKind, Executor, RunControl};

        let [(region, _), _, _, (desktop, _), _, _] = detail_examples();
        for (action, operation) in [(region, "SendInput"), (desktop, "virtual desktop")] {
            let document = document_with_action(action);
            let plan = crate::mkmacro::compile(&document.macros[0]).unwrap();
            // Exercise unsupported backends even on Windows, without sending
            // input or changing the user's desktop.
            let error = Executor::new(Backends::unsupported(), Arc::new(RunControl::default()))
                .execute(&plan, ExecutionOptions::normal(), &|_| {})
                .unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
            assert!(!error.message.trim().is_empty());
            assert_eq!(
                error.context.get("backend_operation").map(String::as_str),
                Some(operation)
            );
            assert_eq!(error.context.get("step_id").map(String::as_str), Some("1"));
            if operation == "virtual desktop" {
                assert_eq!(
                    error.message,
                    "Virtual desktop automation is available only on Windows"
                );
                assert_eq!(error.context.get("desktop").map(String::as_str), Some("3"));
                assert_eq!(
                    error.context.get("action").map(String::as_str),
                    Some("GoTo { desktop: 3 }")
                );
            }
        }
    }

    #[test]
    fn malformed_region_delay_and_desktop_payloads_are_rejected() {
        use crate::mkmacro::{
            DiagnosticSeverity, MkClickWithinRegionPayload, MkDelayMode, MkDelayPayload, ScreenRect,
        };
        let mut cases = vec![
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::RandomRange,
                    minimum_ms: 1250,
                    maximum_ms: 250,
                    ..Default::default()
                }),
                "invalid_delay_range",
            ),
            (
                MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 0 }),
                "invalid_virtual_desktop_number",
            ),
        ];
        // Exactly empty, over-padded, and overflow-prone persisted padding.
        for padding in [100, 201, u32::MAX] {
            cases.push((
                MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
                    rect: ScreenRect::new(500, 300, 400, 200),
                    button: MkMouseButton::Left,
                    clicks: 1,
                    edge_padding_px: padding,
                }),
                "invalid_click_region_padding",
            ));
        }
        for (action, code) in cases {
            let document = document_with_action(action);
            let diagnostics = validate_document(&document, None);
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == code
                        && diagnostic.severity == DiagnosticSeverity::Fatal
                        && diagnostic.macro_id == 1
                        && diagnostic.step_id == Some(1)
                        && !diagnostic.message.trim().is_empty()
                }),
                "{code}: {diagnostics:?}"
            );
            assert!(!crate::mkmacro::can_run(&diagnostics), "{code}");
            assert_eq!(
                crate::mkmacro::compile(&document.macros[0]).unwrap_err(),
                diagnostics
            );
        }
    }

    #[test]
    fn visible_catalog_metadata_is_routable_and_supported() {
        for descriptor in action_catalog::visible_descriptors() {
            let action = (descriptor.make_default)();
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert!(
                descriptor.category.is_enabled(),
                "disabled category leaked through visible descriptor {:?}",
                descriptor.name
            );
            assert_eq!(
                descriptor.runtime,
                action_catalog::RuntimeAvailability::Supported
            );
            assert!(!descriptor.name.trim().is_empty());
            let details = action_catalog::action_details(&action);
            assert!(
                !details.trim().is_empty(),
                "{} has empty details",
                descriptor.name
            );
            assert!(!details.contains("existing specialized editor"));
            assert!(crate::mkmacro::executor::has_runtime_support(&action));
        }
        for descriptor in action_catalog::descriptors()
            .into_iter()
            .filter(|d| matches!(d.editor, action_catalog::EditorKind::DirectInsert))
        {
            assert!(matches!(
                (descriptor.make_default)(),
                MkAction::Else
                    | MkAction::EndIf
                    | MkAction::RepeatEnd
                    | MkAction::WhileEnd
                    | MkAction::Break
                    | MkAction::Continue
                    | MkAction::VirtualDesktop(
                        MkVirtualDesktopAction::Create
                            | MkVirtualDesktopAction::SwitchLeft
                            | MkVirtualDesktopAction::SwitchRight
                            | MkVirtualDesktopAction::CloseCurrent
                    )
            ));
        }
    }

    #[test]
    fn image_result_details_use_friendly_catalog_description() {
        let action = MkAction::MouseMove(MkMouseMovePayload {
            target: MkCoordinateTarget::Image {
                image: crate::mkmacro::MkImageRef::from_filename("10.png"),
                offset: crate::mkmacro::variables::MkPoint { x: 2, y: -3 },
            },
            duration_ms: 500,
        });
        let details = action_catalog::action_details(&action);
        assert_eq!(details, "Image Result: 10.png + (2,-3) · Smooth 500 ms");
    }

    #[test]
    fn coordinate_targets_and_visual_details_are_exact() {
        use crate::mkmacro::variables::MkPoint;
        let screen = MkCoordinateTarget::Screen {
            point: MkPoint { x: 824, y: 446 },
        };
        let window = MkCoordinateTarget::ActiveWindow {
            point: MkPoint { x: -8, y: -12 },
        };
        assert_eq!(
            action_catalog::format_coordinate_target(&screen),
            "Screen (824, 446)"
        );
        assert_eq!(
            action_catalog::format_coordinate_target(&window),
            "Active Window (-8, -12)"
        );
        assert_eq!(
            action_catalog::format_coordinate_target(&MkCoordinateTarget::WindowClient {
                matcher: crate::mkmacro::MkWindowMatcher {
                    process: Some("editor.exe".into()),
                    title: Some("Document".into()),
                    ..Default::default()
                },
                point: MkPoint { x: 8, y: -2 },
            }),
            "Matched Window editor.exe (8, -2)"
        );
        assert_eq!(
            action_catalog::format_coordinate_target(&MkCoordinateTarget::Variable {
                name: "cursor".into()
            }),
            "Variable <cursor>"
        );
        assert_eq!(
            action_catalog::format_coordinate_target(&MkCoordinateTarget::Image {
                image: crate::mkmacro::MkImageRef::from_filename("9.png"),
                offset: MkPoint { x: -3, y: 5 }
            }),
            "Image Result: 9.png + (-3,5)"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::MouseMove(MkMouseMovePayload {
                target: screen.clone(),
                duration_ms: 0
            })),
            "Screen (824, 446) · Instant"
        );
        for (clicks, expected) in [
            (1, "Left ×1 @ Screen (824, 446)"),
            (2, "Left ×2 @ Screen (824, 446)"),
        ] {
            assert_eq!(
                action_catalog::action_details(&MkAction::MouseClick(MkMousePayload {
                    target: screen.clone(),
                    button: MkMouseButton::Left,
                    clicks
                })),
                expected
            );
        }
        assert_eq!(
            action_catalog::action_details(&MkAction::PixelCheck {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 410, y: 220 }
                },
                color: "#00FF00".into(),
                tolerance: 8
            }),
            "#00FF00 ±8 @ Screen (410, 220)"
        );
        let image = MkImagePayload {
            image: crate::mkmacro::MkImageRef::from_filename("42.png"),
            wait: MkWaitOptions {
                timeout_ms: 2500,
                poll_interval_ms: 100,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        };
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageFind(image.clone())),
            "42.png · Entire Desktop · fail if missing"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageClick(image)),
            "42.png · Entire Desktop · center · 2500 ms"
        );
    }

    #[test]
    fn coordinate_details_are_payload_only_and_non_mutating() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        for i in 0..500 {
            action_catalog::insert_action(
                &mut dialog,
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::Screen {
                        point: crate::mkmacro::variables::MkPoint {
                            x: i - 250,
                            y: 250 - i,
                        },
                    },
                    duration_ms: 0,
                }),
            );
        }
        let before = serde_json::to_vec(&dialog.draft).unwrap();
        for step in &dialog.selected_macro().unwrap().steps {
            let MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Screen { point },
                ..
            }) = &step.action
            else {
                panic!("unexpected action")
            };
            assert_eq!(
                action_catalog::action_details(&step.action),
                format!("Screen ({}, {}) · Instant", point.x, point.y)
            );
        }
        assert_eq!(serde_json::to_vec(&dialog.draft).unwrap(), before);
    }

    fn catalog_descriptor(name: &str) -> action_catalog::ActionDescriptor {
        action_catalog::visible_descriptors()
            .find(|descriptor| descriptor.name == name)
            .unwrap_or_else(|| panic!("missing catalog descriptor {name}"))
    }

    #[test]
    fn configurable_catalog_selection_is_transactional_and_keeps_catalog_state() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        d.catalog.query = "key".into();

        assert!(action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Key Press")
        ));
        let original = d.action_editor.draft.as_ref().unwrap().action.clone();
        assert!(d.catalog.open);
        assert!(!action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Text")
        ));
        assert_eq!(d.action_editor.draft.as_ref().unwrap().action, original);

        let mut editor = d.take_action_editor();
        assert!(editor.apply(&mut d).is_some());
        d.action_editor = editor;
        assert_eq!(d.selected_macro().unwrap().steps.len(), 1);
        assert!(d.catalog.open);
        assert_eq!(d.catalog.query, "key");
    }

    #[test]
    fn keyboard_actions_are_visible_ready_and_insertable_through_catalog_editor() {
        let (_dir, mut d) = dialog();
        d.create_macro();

        for name in ["Key Press", "Key Down", "Key Up", "Hotkey"] {
            let descriptor = catalog_descriptor(name);
            assert_eq!(descriptor.name, name);
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert!(action_catalog::is_available_in_palette(&descriptor));

            let expected = (descriptor.make_default)();
            assert!(
                matches!(
                    (name, &expected),
                    ("Key Press", MkAction::KeyPress(_))
                        | ("Key Down", MkAction::KeyDown(_))
                        | ("Key Up", MkAction::KeyUp(_))
                        | ("Hotkey", MkAction::Hotkey(_))
                ),
                "{name} produced the wrong default keyboard action: {expected:?}"
            );

            assert!(action_catalog::select_descriptor(&mut d, &descriptor));
            let mut editor = d.take_action_editor();
            assert!(editor.apply(&mut d).is_some());
            d.action_editor = editor;
            assert_eq!(
                d.selected_macro().unwrap().steps.last().unwrap().action,
                expected
            );
        }

        assert_eq!(d.selected_macro().unwrap().steps.len(), 4);
    }

    #[test]
    fn palette_supports_five_consecutive_configurable_insertions() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        d.catalog.query = "delay".into();
        for _ in 0..5 {
            assert!(action_catalog::select_descriptor(
                &mut d,
                &catalog_descriptor("Delay")
            ));
            let mut editor = d.take_action_editor();
            assert!(editor.apply(&mut d).is_some());
            d.action_editor = editor;
            assert!(d.catalog.open);
            assert_eq!(d.catalog.query, "delay");
        }
        assert_eq!(d.selected_macro().unwrap().steps.len(), 5);
    }

    #[test]
    fn cancelling_catalog_editor_preserves_macro_and_catalog() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        d.catalog.query = "mouse".into();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("Mouse Click"));
        d.action_editor.cancel();
        assert!(d.selected_macro().unwrap().steps.is_empty());
        assert!(d.catalog.open);
        assert_eq!(d.catalog.query, "mouse");
    }

    #[test]
    fn advanced_control_markers_are_directly_insertable() {
        for name in [
            "Else",
            "End If",
            "End Repeat",
            "End While",
            "Break",
            "Continue",
        ] {
            let descriptor = action_catalog::descriptors()
                .into_iter()
                .find(|d| d.name == name)
                .unwrap();
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert_eq!(descriptor.editor, action_catalog::EditorKind::DirectInsert);
        }
    }

    #[test]
    fn catalog_close_paths_and_parent_close_reset_children() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.catalog.open = true;
        action_catalog::close(&mut d); // Explicit Close button transition.
        assert!(!d.catalog.open);
        d.catalog.open = true;
        action_catalog::close(&mut d); // Simulated window-X transition.
        assert!(!d.catalog.open);

        d.catalog.open = true;
        d.action_editor
            .begin_new(MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }));
        d.open = false;
        d.close_children();
        assert!(!d.catalog.open);
        assert!(d.action_editor.draft.is_none());
    }

    fn uia_actions() -> Vec<MkAction> {
        action_catalog::descriptors()
            .into_iter()
            .filter(|descriptor| {
                descriptor.category == action_catalog::ActionCategory::UiAutomation
            })
            .map(|descriptor| (descriptor.make_default)())
            .collect()
    }

    #[test]
    fn uia_catalog_entries_are_complete_but_not_visible_or_searchable() {
        let complete: Vec<_> = action_catalog::descriptors()
            .into_iter()
            .filter(|descriptor| {
                descriptor.category == action_catalog::ActionCategory::UiAutomation
            })
            .collect();
        assert_eq!(complete.len(), 7);
        assert!(complete.iter().all(|descriptor| {
            descriptor.availability == action_catalog::ActionAvailability::Hidden
        }));

        let visible: Vec<_> = action_catalog::visible_descriptors().collect();
        assert!(visible.iter().all(|descriptor| {
            descriptor.category != action_catalog::ActionCategory::UiAutomation
        }));
        for query in [
            "UI Automation",
            "UIA",
            "Invoke UI Element",
            "Set UI Value",
            "Read UI Value",
            "Toggle UI Element",
            "Select UI Element",
            "Focus UI Element",
            "Wait for UI Element",
        ] {
            assert!(
                !visible
                    .iter()
                    .any(|descriptor| action_catalog::matches(descriptor, query))
            );
        }
    }

    #[test]
    fn every_uia_variant_has_an_unavailable_display_label() {
        let actions = uia_actions();
        assert_eq!(actions.len(), 7);
        for action in actions {
            assert_eq!(
                action_catalog::action_name(&action),
                "UI Automation — currently unavailable"
            );
            assert!(action_catalog::action_details(&action).contains("Unavailable"));
        }
    }

    #[test]
    fn uia_payloads_survive_display_roundtrip_repair_and_save() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let original_actions = uia_actions();
        d.selected_macro_mut().unwrap().steps = original_actions
            .iter()
            .cloned()
            .map(|action| MkStep {
                metadata: Default::default(),
                id: 0,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action,
            })
            .collect();

        let encoded = serde_json::to_string(&d.draft).unwrap();
        let mut decoded: MkMacroDocument = serde_json::from_str(&encoded).unwrap();
        for step in &decoded.macros[0].steps {
            let _ = action_catalog::action_name(&step.action);
            let _ = action_catalog::action_details(&step.action);
        }
        let before_repair: Vec<_> = decoded.macros[0]
            .steps
            .iter()
            .map(|step| step.action.clone())
            .collect();
        repair_ids(&mut decoded);
        assert_eq!(
            decoded.macros[0]
                .steps
                .iter()
                .map(|step| &step.action)
                .collect::<Vec<_>>(),
            before_repair.iter().collect::<Vec<_>>()
        );
        d.draft = decoded;
        d.save().unwrap();
        let saved = d.store.snapshot();
        assert_eq!(
            saved.macros[0]
                .steps
                .iter()
                .map(|step| &step.action)
                .collect::<Vec<_>>(),
            original_actions.iter().collect::<Vec<_>>()
        );
        let reencoded = serde_json::to_string(&*saved).unwrap();
        let final_document: MkMacroDocument = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(
            final_document.macros[0]
                .steps
                .iter()
                .map(|step| &step.action)
                .collect::<Vec<_>>(),
            original_actions.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn hidden_uia_editor_and_runtime_types_remain_compiled() {
        let _: uia_editor::UiaEditorState = Default::default();
        let _show: fn(&mut eframe::egui::Ui, &mut uia_editor::UiaEditorState) = uia_editor::show;
        assert!(std::mem::size_of::<crate::mkmacro::MkUiPayload>() > 0);
        assert!(std::mem::size_of::<crate::mkmacro::uia::UiCommand>() > 0);
    }
    #[test]
    fn canonical_visible_catalog_contract() {
        use std::collections::HashSet;
        let mut names = HashSet::new();
        let mut variants = HashSet::new();
        for descriptor in action_catalog::visible_descriptors() {
            let context = |capability: &str, action: &MkAction| {
                format!(
                    "descriptor={:?}, variant={}, editor={:?}, capability={capability}",
                    descriptor.name,
                    action_catalog::action_name(action),
                    descriptor.editor
                )
            };
            let action = (descriptor.make_default)();
            assert!(
                descriptor.category.is_enabled(),
                "{}",
                context("enabled category", &action)
            );
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready,
                "{}",
                context("visible availability", &action)
            );
            assert_eq!(
                descriptor.runtime,
                action_catalog::RuntimeAvailability::Supported,
                "{}",
                context("ready runtime", &action)
            );
            assert!(
                names.insert(descriptor.name),
                "duplicate name {}",
                descriptor.name
            );
            assert!(
                !descriptor.name.trim().is_empty(),
                "{}",
                context("name", &action)
            );
            assert!(
                !descriptor.description.trim().is_empty()
                    && descriptor.description.trim() != descriptor.name.trim(),
                "{}",
                context("meaningful description", &action)
            );
            assert!(
                serde_json::to_string(&action).is_ok(),
                "{}",
                context("serialization", &action)
            );
            // Some typed action families intentionally share one serialized action
            // variant while remaining separately discoverable catalog rows. Include
            // the typed operation in their catalog identity so this invariant still
            // catches genuinely duplicated descriptors.
            let variant = (
                std::mem::discriminant(&action),
                match &action {
                    MkAction::WindowState { state, .. } => Some(format!("window:{state:?}")),
                    MkAction::VirtualDesktop(operation) => {
                        Some(format!("virtual_desktop:{operation:?}"))
                    }
                    MkAction::WaitUntil {
                        condition: MkCondition::ImageSearch { found, .. },
                        ..
                    } => Some(format!("wait_image:{found}")),
                    MkAction::WaitUntil {
                        condition: MkCondition::OcrTextSearch { found, .. },
                        ..
                    } => Some(format!("wait_ocr_text:{found}")),
                    _ => None,
                },
            );
            assert!(
                variants.insert(variant),
                "duplicate action variant for {}",
                descriptor.name
            );
            let no_configuration = action_catalog::requires_no_configuration(&action);
            assert_eq!(
                no_configuration,
                matches!(descriptor.editor, action_catalog::EditorKind::DirectInsert),
                "{}",
                context("no-configuration classification", &action)
            );
            assert!(
                no_configuration
                    || action_catalog::editor_route_recognizes(&action, descriptor.editor),
                "{}",
                context("implemented editor route", &action)
            );
            assert_eq!(
                descriptor.runtime == action_catalog::RuntimeAvailability::Supported,
                crate::mkmacro::executor::has_runtime_support(&action),
                "{}",
                context("runtime metadata agreement", &action)
            );
            let configurable = matches!(
                action_catalog::editor_contract(descriptor.editor),
                Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
            );
            assert!(!configurable || descriptor.editor != action_catalog::EditorKind::DirectInsert);
            let text = format!(
                "{} {} {} {}",
                descriptor.name,
                descriptor.description,
                action_catalog::action_name(&action),
                action_catalog::action_details(&action)
            );
            assert!(
                !action_catalog::contains_placeholder_wording(&text),
                "placeholder text for {}: {text}",
                descriptor.name
            );
            assert!(
                !text.contains("This action uses its existing specialized editor"),
                "historical fallback copy returned for {}",
                descriptor.name
            );
            assert!(descriptor.hidden_reason.is_none());

            // Exercise the real insertion/editor transaction, then the same
            // document validator used by save and run.
            let (_dir, mut dialog) = dialog();
            dialog.create_macro();
            let caller_id = dialog.selected_macro_id.unwrap();
            let call_target = if matches!(action, MkAction::CallMacro(_)) {
                dialog.create_macro();
                let target = dialog.selected_macro_id.unwrap();
                dialog.rename_selected("Call Target");
                dialog.set_selected_macro(Some(caller_id));
                Some(target)
            } else {
                None
            };
            assert!(action_catalog::select_descriptor(&mut dialog, &descriptor));
            if let Some(target) = call_target {
                let MkAction::CallMacro(call) = &mut dialog
                    .action_editor
                    .draft
                    .as_mut()
                    .expect("Call opens a transaction")
                    .action
                else {
                    unreachable!()
                };
                call.macro_id = target;
            }
            let draft_contract = action_catalog::draft_validation_contract(&action);
            if configurable
                && draft_contract == action_catalog::DraftValidationContract::CommitReady
            {
                assert!(dialog.action_editor.draft.is_some());
                let mut editor = dialog.take_action_editor();
                assert!(editor.apply(&mut dialog).is_some());
                dialog.action_editor = editor;
            }
            let diagnostics = validate_document(&dialog.draft, None);
            match draft_contract {
                action_catalog::DraftValidationContract::CommitReady => assert!(
                    crate::mkmacro::can_run(&diagnostics),
                    "invalid default {}: {diagnostics:?}",
                    descriptor.name
                ),
                action_catalog::DraftValidationContract::AwaitingRequiredAsset => {
                    assert!(dialog.action_editor.draft.is_some());
                    assert!(dialog.selected_macro().unwrap().steps.is_empty());
                    let draft = &dialog.action_editor.draft.as_ref().unwrap().action;
                    assert!(
                        matches!(draft, MkAction::ImageFind(p) | MkAction::ImageClick(p) if p.image.is_empty())
                            || matches!(draft, MkAction::WaitUntil {
                                condition: MkCondition::ImageSearch { search, .. }, ..
                            } if search.image.is_empty())
                    );
                }
            }
        }
    }

    #[test]
    fn complete_catalog_metadata_and_hidden_policy() {
        use std::collections::HashSet;
        let descriptors = action_catalog::descriptors();
        let visible: HashSet<_> = action_catalog::visible_descriptors()
            .map(|d| d.name)
            .collect();
        let mut names = HashSet::new();
        for descriptor in descriptors {
            let action = (descriptor.make_default)();
            let context = format!(
                "descriptor={:?}, variant={}, editor={:?}",
                descriptor.name,
                action_catalog::action_name(&action),
                descriptor.editor
            );
            assert!(
                names.insert(descriptor.name),
                "{context}: duplicate descriptor name"
            );
            if descriptor.category != action_catalog::ActionCategory::UiAutomation {
                assert!(
                    action_catalog::descriptor_name_matches_action(&descriptor, &action),
                    "{context}: action-name mapping"
                );
            } else {
                assert!(
                    !action_catalog::action_name(&action).trim().is_empty(),
                    "{context}: hidden legacy action-name mapping"
                );
            }
            assert_eq!(
                descriptor.runtime == action_catalog::RuntimeAvailability::Supported,
                crate::mkmacro::executor::has_runtime_support(&action),
                "{context}: runtime contract"
            );
            if descriptor.availability == action_catalog::ActionAvailability::Hidden {
                assert_eq!(
                    descriptor.category,
                    action_catalog::ActionCategory::UiAutomation,
                    "{context}: only UI Automation may be hidden"
                );
                assert!(
                    descriptor
                        .hidden_reason
                        .is_some_and(|r| !r.trim().is_empty()),
                    "{context}: hidden reason"
                );
                assert!(
                    !visible.contains(descriptor.name),
                    "{context}: hidden search leak"
                );
                // Filtering raw registry rows is intentionally insufficient:
                // the render input must originate from visible_descriptors().
                assert!(
                    !action_catalog::visible_descriptors()
                        .filter(|row| action_catalog::matches(row, descriptor.name))
                        .any(|row| row.name == descriptor.name),
                    "{context}: hidden descriptor rendered for an exact-name search"
                );

                let (_dir, mut dialog) = dialog();
                dialog.create_macro();
                assert!(
                    !action_catalog::select_descriptor(&mut dialog, &descriptor),
                    "{context}: hidden descriptor was selectable"
                );
                assert!(dialog.selected_macro().unwrap().steps.is_empty());
                assert!(dialog.action_editor.draft.is_none());

                // Hidden is an authoring policy only: existing documents must
                // retain the complete payload during load/save round trips.
                let encoded = serde_json::to_vec(&action).unwrap();
                let decoded: MkAction = serde_json::from_slice(&encoded).unwrap();
                assert_eq!(decoded, action, "{context}: saved payload changed");
            }
        }
    }

    #[test]
    fn authoritative_capability_audit_covers_every_descriptor() {
        for descriptor in action_catalog::descriptors() {
            let action = (descriptor.make_default)();
            let context = format!(
                "descriptor={:?}, action={}",
                descriptor.name,
                action_catalog::action_name(&action)
            );
            match descriptor.availability {
                action_catalog::ActionAvailability::Ready => {
                    assert_eq!(
                        descriptor.runtime,
                        action_catalog::RuntimeAvailability::Supported,
                        "{context}: runtime metadata"
                    );
                    assert!(
                        crate::mkmacro::executor::has_runtime_support(&action),
                        "{context}: executor support"
                    );
                    let contract = descriptor
                        .editor
                        .contract()
                        .unwrap_or_else(|| panic!("{context}: missing editor contract"));
                    match contract {
                        action_catalog::EditorContract::DirectInsert { .. } => assert_eq!(
                            descriptor.editor,
                            action_catalog::EditorKind::DirectInsert,
                            "{context}"
                        ),
                        action_catalog::EditorContract::Configurable { field_count } => {
                            assert!(field_count > 0, "{context}: editor has no fields");
                            assert!(
                                action_catalog::editor_route_recognizes(&action, descriptor.editor),
                                "{context}: editor route"
                            );
                            let completeness =
                                action_catalog::editor_completeness(descriptor.editor)
                                    .expect("complete editor metadata");
                            assert!(
                                completeness.has_primary_control,
                                "{context}: no primary control"
                            );
                            assert!(
                                !completeness.intentionally_disabled,
                                "{context}: permanently disabled editor"
                            );
                            assert!(
                                completeness.placeholder_copy.is_none(),
                                "{context}: placeholder editor copy"
                            );
                        }
                    }
                    assert!(
                        serde_json::to_vec(&action).is_ok(),
                        "{context}: serialization"
                    );
                    assert!(
                        action_catalog::descriptor_name_matches_action(&descriptor, &action),
                        "{context}: action name"
                    );
                    let details = action_catalog::action_details(&action);
                    assert!(
                        !details.trim().is_empty() && details.trim() != descriptor.name,
                        "{context}: meaningful details"
                    );
                    for text in [
                        descriptor.name,
                        descriptor.description,
                        action_catalog::action_name(&action),
                        details.as_str(),
                    ] {
                        assert!(
                            !action_catalog::contains_placeholder_wording(text),
                            "{context}: deferred wording: {text:?}"
                        );
                    }
                }
                action_catalog::ActionAvailability::Hidden => {
                    assert_eq!(
                        descriptor.category,
                        action_catalog::ActionCategory::UiAutomation,
                        "{context}: hidden action category"
                    );
                    assert_eq!(
                        descriptor.runtime,
                        action_catalog::RuntimeAvailability::Unavailable,
                        "{context}: hidden UIA remains unavailable"
                    );
                    assert!(
                        !action_catalog::is_available_in_palette(&descriptor),
                        "{context}: hidden palette leak"
                    );
                }
            }
        }
    }

    #[test]
    fn virtual_desktops_have_explicit_windows_action_routes() {
        let descriptors = action_catalog::descriptors();
        let direct_rows: Vec<_> = descriptors
            .iter()
            .filter(|descriptor| {
                descriptor.editor == action_catalog::EditorKind::DirectInsert
                    && matches!((descriptor.make_default)(), MkAction::VirtualDesktop(_))
            })
            .collect();
        assert_eq!(direct_rows.len(), 4);
        assert!(direct_rows.iter().all(|descriptor| {
            descriptor.category == action_catalog::ActionCategory::Windows
                && descriptor.editor == action_catalog::EditorKind::DirectInsert
                && action_catalog::requires_no_configuration(&(descriptor.make_default)())
                && action_catalog::editor_for_action(&(descriptor.make_default)())
                    == action_catalog::EditorKind::DirectInsert
        }));
        assert_eq!(
            direct_rows
                .iter()
                .map(|descriptor| descriptor.name)
                .collect::<Vec<_>>(),
            [
                "Create Virtual Desktop",
                "Switch Virtual Desktop Left",
                "Switch Virtual Desktop Right",
                "Close Current Virtual Desktop",
            ]
        );
        let go_to = descriptors
            .iter()
            .find(|descriptor| descriptor.name == "Go To Virtual Desktop")
            .expect("Go To Virtual Desktop");
        assert_eq!(go_to.category, action_catalog::ActionCategory::Windows);
        assert_eq!(go_to.editor, action_catalog::EditorKind::VirtualDesktop);
        assert!(matches!(
            go_to.editor.contract(),
            Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
        ));
        for desktop in [0, 1, 3, u32::MAX] {
            let action = MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop });
            assert_eq!(
                action_catalog::editor_for_action(&action),
                action_catalog::EditorKind::VirtualDesktop
            );
            assert!(!action_catalog::requires_no_configuration(&action));
        }
        assert!(matches!(
            (go_to.make_default)(),
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 1 })
        ));
    }

    #[test]
    #[cfg(windows)]
    fn virtual_desktop_catalog_selection_uses_the_intended_transactions() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        for name in [
            "Create Virtual Desktop",
            "Switch Virtual Desktop Left",
            "Switch Virtual Desktop Right",
            "Close Current Virtual Desktop",
        ] {
            let descriptor = catalog_descriptor(name);
            let count = d.selected_macro().unwrap().steps.len();
            assert!(action_catalog::select_descriptor(&mut d, &descriptor));
            assert!(d.action_editor.draft.is_none());
            let steps = &d.selected_macro().unwrap().steps;
            assert_eq!(steps.len(), count + 1);
            assert_eq!(steps.last().unwrap().action, (descriptor.make_default)());
        }

        let before = d.draft.clone();
        assert!(action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Go To Virtual Desktop")
        ));
        assert_eq!(d.draft, before);
        assert_eq!(
            d.action_editor.editor,
            Some(action_catalog::EditorKind::VirtualDesktop)
        );
        assert_eq!(
            d.action_editor.draft.as_ref().unwrap().action,
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 1 })
        );
        d.action_editor.cancel();
        assert_eq!(d.draft, before);
    }

    #[test]
    fn virtual_desktop_editor_defaults_edits_cancels_and_rejects_zero() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let descriptor = action_catalog::descriptors()
            .into_iter()
            .find(|row| row.name == "Go To Virtual Desktop")
            .unwrap();
        let go_to = |desktop| MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop });
        let mut editor = d.take_action_editor();
        editor.begin_new((descriptor.make_default)());
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(1));

        // Invalid new actions must not insert a step or consume the draft.
        editor.draft.as_mut().unwrap().action = go_to(0);
        let before = d.draft.clone();
        assert!(editor.apply(&mut d).is_none());
        assert_eq!(d.draft, before);
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(0));
        editor.draft.as_mut().unwrap().action = go_to(1);
        let id = editor.apply(&mut d).unwrap();
        let original = d.selected_macro().unwrap().steps[0].clone();
        assert_eq!(original.action, go_to(1));

        editor.begin_edit(&original);
        editor.draft.as_mut().unwrap().action = go_to(3);
        assert_eq!(d.selected_macro().unwrap().steps[0], original);
        assert_eq!(editor.apply(&mut d), Some(id));
        assert_eq!(d.selected_macro().unwrap().steps.len(), 1);
        let committed = d.selected_macro().unwrap().steps[0].clone();
        assert_eq!(committed.action, go_to(3));
        assert_eq!(
            action_catalog::action_details(&committed.action),
            "Desktop 3"
        );

        editor.begin_edit(&committed);
        assert_eq!(editor.draft.as_ref().unwrap(), &committed);
        editor.draft.as_mut().unwrap().action = go_to(8);
        editor.cancel();
        assert_eq!(d.selected_macro().unwrap().steps[0], committed);

        editor.begin_edit(&committed);
        editor.draft.as_mut().unwrap().action = go_to(0);
        let before = d.draft.clone();
        assert!(editor.apply(&mut d).is_none());
        assert_eq!(d.draft, before);
        assert_eq!(editor.editing_id, Some(id));
        assert_eq!(
            editor.editor,
            Some(action_catalog::EditorKind::VirtualDesktop)
        );
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(0));
        editor.draft.as_mut().unwrap().action = go_to(3);
        assert_eq!(editor.apply(&mut d), Some(id));
        assert_eq!(d.selected_macro().unwrap().steps[0], committed);
    }

    #[test]
    fn completed_actions_keep_real_editor_routes() {
        for (name, expected) in [
            ("Close Window", action_catalog::EditorKind::Window),
            ("Set Variable", action_catalog::EditorKind::Variable),
            ("Unset Variable", action_catalog::EditorKind::Variable),
            ("Wait Until", action_catalog::EditorKind::Condition),
            ("If", action_catalog::EditorKind::Condition),
            ("While", action_catalog::EditorKind::Condition),
            ("Repeat", action_catalog::EditorKind::Repeat),
        ] {
            let descriptor = catalog_descriptor(name);
            let action = (descriptor.make_default)();
            assert_eq!(
                descriptor.editor,
                expected,
                "descriptor={name:?}, variant={}, editor={:?}: expected specialized configurable editor",
                action_catalog::action_name(&action),
                descriptor.editor
            );
            assert!(
                matches!(
                    action_catalog::editor_contract(descriptor.editor),
                    Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
                ),
                "descriptor={name:?}, variant={}, editor={:?}: configurable editor contract",
                action_catalog::action_name(&action),
                descriptor.editor
            );
        }
    }

    #[test]
    fn configurable_editor_routes_build_meaningful_fields() {
        for descriptor in action_catalog::visible_descriptors().filter(|d| {
            matches!(
                action_catalog::editor_contract(d.editor),
                Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
            )
        }) {
            let (_dir, mut dialog) = dialog();
            dialog.create_macro();
            assert!(action_catalog::select_descriptor(&mut dialog, &descriptor));
            let draft = dialog
                .action_editor
                .draft
                .as_ref()
                .expect("configurable action must open editor");
            assert!(action_catalog::editor_route_recognizes(
                &draft.action,
                descriptor.editor
            ));
            assert!(
                matches!(
                    action_catalog::editor_contract(descriptor.editor),
                    Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
                ),
                "{} produced an inert editor",
                descriptor.name
            );
        }
    }

    #[test]
    fn mouse_catalog_contracts() {
        let action = |name| (catalog_descriptor(name).make_default)();
        assert!(matches!(
            action("Mouse Move"),
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Screen { .. },
                duration_ms: 0
            })
        ));
        assert!(matches!(
            action("Mouse Click"),
            MkAction::MouseClick(MkMousePayload {
                button: MkMouseButton::Left,
                clicks: 1,
                ..
            })
        ));
        assert!(matches!(
            action("Mouse Drag"),
            MkAction::MouseDrag(crate::mkmacro::MkMouseDragPayload {
                from: MkCoordinateTarget::Screen { .. },
                to: MkCoordinateTarget::Screen { .. },
                button: MkMouseButton::Left,
                duration_ms: 400
            })
        ));
        assert_eq!(
            catalog_descriptor("Mouse Down").editor,
            action_catalog::EditorKind::MouseButton
        );
        assert_eq!(
            catalog_descriptor("Mouse Up").editor,
            action_catalog::EditorKind::MouseButton
        );
        assert_eq!(
            catalog_descriptor("Mouse Scroll").editor,
            action_catalog::EditorKind::MouseScroll
        );
        assert!(matches!(
            action("Mouse Scroll"),
            MkAction::MouseScroll {
                axis: MkMouseScrollAxis::Vertical,
                i32_delta: -120
            }
        ));
        for name in [
            "Mouse Move",
            "Mouse Click",
            "Mouse Drag",
            "Mouse Down",
            "Mouse Up",
            "Mouse Scroll",
        ] {
            assert!(
                crate::mkmacro::executor::has_runtime_support(&action(name)),
                "{name}"
            );
        }
    }

    fn synthetic_debug_snapshot(
        run_id: u64,
        revision: u64,
        state: crate::mkmacro::RuntimeState,
        reason: crate::mkmacro::DebugSnapshotReason,
        pause_reason: Option<crate::mkmacro::RuntimePauseReason>,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let variables = Arc::new(crate::mkmacro::RuntimeVariables::from([(
            "answer".into(),
            crate::mkmacro::MkValue::Number(42.0),
        )]));
        Arc::new(crate::mkmacro::RuntimeSnapshot {
            state,
            run_mode: crate::mkmacro::RuntimeRunMode::Debug,
            run_id,
            macro_id: Some(7),
            call_stack: Arc::new(vec![crate::mkmacro::ExecutionFrameSnapshot {
                context: crate::mkmacro::ExecutionFrameContext::root(7),
                macro_name: Arc::from("Macro 7"),
                active_step_id: match pause_reason {
                    Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id, .. }) => {
                        Some(step_id)
                    }
                    _ => None,
                },
            }]),
            pause_reason,
            breakpoint_sequence: u64::from(matches!(
                pause_reason,
                Some(crate::mkmacro::RuntimePauseReason::Breakpoint { .. })
            )),
            debug_snapshot: Some(Arc::new(crate::mkmacro::DebugSnapshot {
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
                macro_name: Arc::from("Macro 7"),
                step_id: None,
                variables: variables.clone(),
                reason,
            })),
            debug_variables: variables,
            debug_snapshot_reason: Some(reason),
            revision,
            ..crate::mkmacro::RuntimeSnapshot::default()
        })
    }

    fn synthetic_normal_snapshot(
        run_id: u64,
        revision: u64,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let mut snapshot = (*synthetic_debug_snapshot(
            run_id,
            revision,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
            None,
        ))
        .clone();
        snapshot.run_mode = crate::mkmacro::RuntimeRunMode::Normal;
        snapshot.debug_snapshot = None;
        snapshot.debug_variables = Arc::new(Default::default());
        snapshot.debug_snapshot_reason = None;
        Arc::new(snapshot)
    }

    fn wait_for_debug_boundary_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
        reason: crate::mkmacro::DebugSnapshotReason,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id
                && snapshot
                    .debug_snapshot
                    .as_ref()
                    .is_some_and(|debug| debug.reason == reason)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish {reason:?} for run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_terminal_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id
                && matches!(
                    snapshot.state,
                    RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
                )
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_mode_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
        mode: crate::mkmacro::RuntimeRunMode,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id && snapshot.run_mode == mode {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish {mode:?} run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn submit_after_previous_run(runtime: &MacroRuntime, command: RuntimeCommand) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match runtime.command(command.clone()) {
                crate::mkmacro::CommandResult::Accepted => return,
                crate::mkmacro::CommandResult::AlreadyRunning { .. } => {
                    assert!(
                        Instant::now() < deadline,
                        "runtime admission was not released"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                crate::mkmacro::CommandResult::Rejected(diagnostic) => {
                    panic!("runtime rejected {command:?}: {diagnostic:?}")
                }
            }
        }
    }

    fn real_runtime_dialog() -> (
        tempfile::TempDir,
        MkMacroDialog,
        Arc<MacroRuntime>,
        Arc<crate::mkmacro::executor::fake::FakeBackend>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let set_variable = |id: u64, name: &str, value: MkValue| MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::SetVariable {
                name: name.into(),
                value,
            },
        };
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                macros: vec![
                    MkMacro {
                        signature: Default::default(),
                        id: 1,
                        name: "First debug run".into(),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: Default::default(),
                        steps: vec![
                            MkStep {
                                delay_after_ms: 25,
                                ..set_variable(11, "count", MkValue::Number(1.0))
                            },
                            set_variable(12, "result", MkValue::String("done".into())),
                        ],
                    },
                    MkMacro {
                        signature: Default::default(),
                        id: 2,
                        name: "Second debug run".into(),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: Default::default(),
                        steps: vec![MkStep {
                            delay_after_ms: 25,
                            ..set_variable(21, "fresh", MkValue::String("new".into()))
                        }],
                    },
                ],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let mut dialog = MkMacroDialog::new(store.clone());
        let fake = Arc::new(crate::mkmacro::executor::fake::FakeBackend::default());
        let runtime = Arc::new(MacroRuntime::new(store, fake.clone().backends()));
        dialog.set_runtime_for_test(runtime.clone());
        (directory, dialog, runtime, fake)
    }

    fn render_host_frame(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        screen: eframe::egui::Rect,
        root_visible: bool,
        events: Vec<eframe::egui::Event>,
        discard: bool,
    ) {
        render_host_frame_for(ctx, dialog, screen, root_visible, events, discard, None);
    }

    fn render_host_frame_for(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        screen: eframe::egui::Rect,
        root_visible: bool,
        events: Vec<eframe::egui::Event>,
        discard: bool,
        target: Option<eframe::egui::ViewportId>,
    ) {
        let mut input = layout_input(screen, 1.0);
        let child = target.or_else(|| dialog.is_popped_out().then(|| dialog.test_viewport_id()));
        let child_events = if child.is_some() {
            events
        } else {
            input.events = events;
            Vec::new()
        };
        dialog.test_host_button = None;
        dialog.test_run_button = None;
        let draw = || {
            ctx.run_ui(input, |ui| {
                let first_pass = ui.current_pass_index() == 0;
                dialog.show_with_host(
                    ui.ctx(),
                    None,
                    first_pass,
                    true,
                    root_visible,
                    root_visible,
                    true,
                    77,
                    false,
                );
                dialog.show_search_child(
                    ui.ctx(),
                    first_pass,
                    root_visible,
                    root_visible,
                    true,
                    false,
                );
                dialog.show_package_children(
                    ui.ctx(),
                    first_pass,
                    root_visible,
                    root_visible,
                    true,
                    false,
                );
                dialog.show_catalog_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_annotation_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_review_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_editor_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_window_picker_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_launcher_picker_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                dialog.show_crop_child(ui.ctx(), first_pass, root_visible, root_visible, true, false);
                if discard && first_pass {
                    ui.ctx().request_discard("discard the MkMacro click");
                }
            })
        };
        let output = super::super::render::with_immediate_viewport_renderer(ctx, || {
            if let Some(viewport) = child {
                super::super::render::with_immediate_viewport_test_events_for(
                    viewport,
                    child_events,
                    draw,
                )
            } else {
                draw()
            }
        });
        dialog.on_host_frame_complete(ctx);
        dialog.complete_search_child(ctx);
        dialog.complete_package_children(ctx);
        dialog.complete_catalog_child(ctx);
        dialog.complete_annotation_child(ctx);
        dialog.complete_review_child(ctx);
        dialog.complete_editor_child(ctx);
        dialog.complete_window_picker_child(ctx);
        dialog.complete_launcher_picker_child(ctx);
        dialog.complete_crop_child(ctx);
        output.drop_without_applying_deltas();
    }

    fn click_host_button(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        screen: eframe::egui::Rect,
        root_visible: bool,
        run: bool,
        discard_release: bool,
    ) {
        for _ in 0..3 {
            render_host_frame(ctx, dialog, screen, root_visible, Vec::new(), false);
        }
        let (rect, clip, enabled) = if run {
            dialog.test_run_button.expect("Run response")
        } else {
            dialog.test_host_button.expect("Dock or Pop Out response")
        };
        assert!(
            enabled && clip.contains_rect(rect),
            "the real button is visible and enabled"
        );
        click_host_position(
            ctx,
            dialog,
            screen,
            root_visible,
            rect.center(),
            discard_release,
        );
    }

    fn click_host_position(
        ctx: &eframe::egui::Context,
        dialog: &mut MkMacroDialog,
        screen: eframe::egui::Rect,
        root_visible: bool,
        pos: eframe::egui::Pos2,
        discard_release: bool,
    ) {
        use eframe::egui::{Event, Modifiers, PointerButton};

        for pressed in [true, false] {
            let events = vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            render_host_frame(
                ctx,
                dialog,
                screen,
                root_visible,
                events,
                discard_release && !pressed,
            );
        }
    }

    #[test]
    fn mkmacro_visible_pop_out_and_dock_run_same_macro_once() {
        use eframe::egui::{self, vec2};

        let (directory, mut dialog, runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        dialog.selected_macro_mut().unwrap().description = "unsaved edit".into();
        dialog.mark_dirty();
        dialog.selection.ids.insert(11);
        let previous_run_id = runtime.snapshot().run_id;
        let ctx = egui::Context::default();
        let settings_path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            settings_path,
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        let key = crate::settings::UtilityToolKey::parse("mkmacro").unwrap();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));

        click_host_button(&ctx, &mut dialog, screen, true, false, true);
        assert!(!dialog.is_popped_out());
        assert_eq!(manager.preference(key), None);
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        assert!(dialog.is_popped_out());
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Popout)
        );
        dialog.close_prompt_open = true;
        assert!(!dialog.complete_close_choice(&ctx, MkMacroCloseChoice::Cancel));
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Popout)
        );
        assert!(dialog.dirty);
        assert!(dialog.selection.ids.contains(&11));
        assert!(dialog.store.snapshot().macros[0].description.is_empty());

        click_host_button(&ctx, &mut dialog, screen, false, true, true);
        assert_eq!(runtime.snapshot().run_id, previous_run_id);
        assert!(dialog.dirty);
        assert!(dialog.store.snapshot().macros[0].description.is_empty());

        click_host_button(&ctx, &mut dialog, screen, false, true, false);
        let started = wait_for_terminal_after(&runtime, previous_run_id);
        assert_eq!(started.macro_id, Some(1));
        assert_eq!(started.state, RuntimeState::Completed);
        assert_eq!(started.run_id, previous_run_id + 1);
        assert_eq!(
            dialog.store.snapshot().macros[0].description,
            "unsaved edit"
        );

        click_host_button(&ctx, &mut dialog, screen, false, false, false);
        assert!(!dialog.is_popped_out());
        assert_eq!(
            manager.preference(key),
            Some(crate::settings::RememberedPresentation::Docked)
        );
        workers.join_all();
        assert_eq!(dialog.selected_macro_id, Some(1));
        assert!(dialog.selection.ids.contains(&11));
        assert_eq!(runtime.snapshot().run_id, started.run_id);
    }

    #[test]
    fn mkmacro_native_parent_opens_ordinary_children_with_retained_origin() {
        use eframe::egui::{self, vec2};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let draft = dialog.draft.clone();
        let revision = dialog.draft_revision;
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        for key in [
            "mkmacro-step-search",
            "mkmacro-help",
            "mkmacro-templates",
            "mkmacro-template-save",
            "mkmacro-package-export",
        ] {
            manager.remember(
                &ctx,
                crate::settings::UtilityToolKey::parse(key).unwrap(),
                crate::settings::RememberedPresentation::Docked,
            );
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        assert!(dialog.is_popped_out());
        let ctrl_f = egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        render_host_frame(&ctx, &mut dialog, screen, false, vec![ctrl_f.clone()], true);
        assert!(dialog.navigation.search.mode.is_none());
        render_host_frame(&ctx, &mut dialog, screen, false, vec![ctrl_f], false);
        assert!(dialog.navigation.search.mode.is_some());
        assert!(dialog.search_host.is_popped_out());
        let original_text_styles = ctx.style_of(ctx.theme()).text_styles.clone();
        ctx.style_mut_of(ctx.theme(), |style| {
            for font in style.text_styles.values_mut() {
                font.size *= 1.2;
            }
        });
        let search_viewport = super::super::utility_window_host::UtilityWindowHost::<
            search::SearchState,
        >::test_viewport(dialog.navigation.search.test_epoch());
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            Vec::new(),
            false,
            Some(search_viewport),
        );
        // The new native window's first observation is its non-interactive sizing pass.
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            Vec::new(),
            false,
            Some(search_viewport),
        );
        let (query, clip, enabled, focused) = dialog
            .navigation
            .search
            .test_query_response
            .expect("native search query");
        assert!(
            enabled && focused && clip.contains_rect(query),
            "query={query:?}, clip={clip:?}, enabled={enabled}, focused={focused}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(query.center()),
                egui::Event::PointerButton {
                    pos: query.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(search_viewport),
            );
        }
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("discarded".into())],
            true,
            Some(search_viewport),
        );
        assert_eq!(dialog.navigation.search.test_query(), "");
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("result".into())],
            false,
            Some(search_viewport),
        );
        assert_eq!(dialog.navigation.search.test_query(), "result");
        let (next, next_clip, next_enabled) = dialog
            .navigation
            .search
            .test_next_response
            .expect("Next response");
        let (close, close_clip, close_enabled) = dialog
            .navigation
            .search
            .test_close_response
            .expect("Close response");
        assert!(
            next_enabled && next_clip.contains_rect(next),
            "next={next:?}, clip={next_clip:?}, enabled={next_enabled}"
        );
        assert!(
            close_enabled && close_clip.contains_rect(close),
            "close={close:?}, clip={close_clip:?}, enabled={close_enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(next.center()),
                egui::Event::PointerButton {
                    pos: next.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(search_viewport),
            );
        }
        assert_eq!(dialog.selection.primary, Some(12));
        assert_eq!(dialog.draft_revision, revision);
        assert!(!dialog.dirty);
        let epoch = dialog.navigation.search.test_epoch();
        let ctrl_h = egui::Event::Key {
            key: egui::Key::H,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        render_host_frame(&ctx, &mut dialog, screen, false, vec![ctrl_h], false);
        assert_eq!(
            dialog.navigation.search.mode,
            Some(search::SearchMode::Replace)
        );
        assert_eq!(dialog.navigation.search.test_epoch(), epoch);
        assert_eq!(dialog.navigation.search.test_query(), "result");
        ctx.style_mut_of(ctx.theme(), |style| {
            style.text_styles = original_text_styles
        });
        for item in ["help", "templates", "save", "export"] {
            for _ in 0..3 {
                render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
            }
            let (reuse, clip, enabled) =
                dialog.package_ui.test_reuse().expect("Reuse menu response");
            assert!(
                enabled && clip.contains_rect(reuse),
                "Reuse rect={reuse:?} clip={clip:?} enabled={enabled}"
            );
            click_host_position(&ctx, &mut dialog, screen, false, reuse.center(), false);
            for _ in 0..2 {
                render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
            }
            let (item_rect, clip, enabled) = dialog
                .package_ui
                .test_item(item)
                .expect("package menu response");
            assert!(
                enabled && clip.contains_rect(item_rect),
                "{item} rect={item_rect:?} clip={clip:?} enabled={enabled}"
            );
            click_host_position(&ctx, &mut dialog, screen, false, item_rect.center(), false);
            assert!(
                dialog.package_hosts.test_popped(item),
                "{item} inherited native parent"
            );
            if item == "save" {
                dialog.package_hosts.test_set_save_name("unsaved template");
                render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), true);
                assert_eq!(
                    dialog.package_hosts.test_save_name(),
                    Some("unsaved template")
                );
                let viewport = dialog.package_hosts.test_save_viewport();
                render_host_frame_for(
                    &ctx,
                    &mut dialog,
                    screen,
                    false,
                    Vec::new(),
                    false,
                    Some(viewport),
                );
                render_host_frame_for(
                    &ctx,
                    &mut dialog,
                    screen,
                    false,
                    Vec::new(),
                    false,
                    Some(viewport),
                );
                let (button, clip, enabled) = dialog
                    .package_hosts
                    .test_save_response()
                    .expect("save template button");
                assert!(
                    enabled && clip.contains_rect(button),
                    "save={button:?}, clip={clip:?}, enabled={enabled}"
                );
                let before = dialog
                    .store
                    .load_template_catalog()
                    .unwrap()
                    .templates
                    .len();
                for discard_release in [true, false] {
                    for pressed in [true, false] {
                        let events = vec![
                            egui::Event::PointerMoved(button.center()),
                            egui::Event::PointerButton {
                                pos: button.center(),
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ];
                        render_host_frame_for(
                            &ctx,
                            &mut dialog,
                            screen,
                            false,
                            events,
                            discard_release && !pressed,
                            Some(viewport),
                        );
                    }
                    let count = dialog
                        .store
                        .load_template_catalog()
                        .unwrap()
                        .templates
                        .len();
                    assert_eq!(count, before + usize::from(!discard_release));
                }
            }
            dialog.package_hosts.close_all(&ctx);
        }
        assert_eq!(dialog.draft, draft);
        assert_eq!(dialog.draft_revision, revision);
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn native_catalog_opens_editor_and_commits_one_action() {
        use eframe::egui::{self, vec2};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let revision = dialog.draft_revision;
        let before = dialog.selected_macro().unwrap().steps.len();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        for key in [
            "mkmacro-action-catalog",
            "mkmacro-action-editor",
            "mkmacro-launcher-picker",
        ] {
            manager.remember(
                &ctx,
                crate::settings::UtilityToolKey::parse(key).unwrap(),
                crate::settings::RememberedPresentation::Docked,
            );
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        let (add, clip, enabled) = dialog.test_add_action.expect("Add Action control");
        assert!(enabled && clip.contains_rect(add));
        click_host_position(&ctx, &mut dialog, screen, false, add.center(), true);
        assert!(!dialog.catalog.open);
        render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        let (add, clip, enabled) = dialog.test_add_action.expect("fresh Add Action control");
        assert!(
            enabled && clip.contains_rect(add),
            "add={add:?} clip={clip:?} enabled={enabled}"
        );
        click_host_position(&ctx, &mut dialog, screen, false, add.center(), false);
        assert!(
            dialog.catalog.open && dialog.catalog_host.is_popped_out(),
            "open={} popped={} pending={} parent={:?} revision={} selected={:?}",
            dialog.catalog.open,
            dialog.catalog_host.is_popped_out(),
            dialog.pending_catalog_open.is_some(),
            dialog.host_state.identity(),
            dialog.draft_revision,
            dialog.selected_macro_id
        );
        let catalog_viewport = super::super::utility_window_host::UtilityWindowHost::<
            action_catalog::CatalogState,
        >::test_viewport(dialog.catalog.epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(catalog_viewport),
            );
        }
        let (query, clip, enabled) = dialog.catalog.test_query.expect("Catalog query");
        assert!(
            enabled && clip.contains_rect(query),
            "query={query:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(query.center()),
                egui::Event::PointerButton {
                    pos: query.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(catalog_viewport),
            );
        }
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("Delay".into())],
            false,
            Some(catalog_viewport),
        );
        let (entry, clip, enabled) = dialog.catalog.test_delay.expect("Delay entry");
        assert!(
            enabled && clip.contains_rect(entry),
            "entry={entry:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(entry.center()),
                egui::Event::PointerButton {
                    pos: entry.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(catalog_viewport),
            );
        }
        assert!(dialog.action_editor.draft.is_some() && dialog.editor_host.is_popped_out());
        assert_eq!(dialog.draft_revision, revision);
        let editor_viewport = super::super::utility_window_host::UtilityWindowHost::<
            action_editor::ActionEditorState,
        >::test_viewport(dialog.action_editor.host_epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(editor_viewport),
            );
        }
        let (combo, clip, enabled) = dialog
            .action_editor
            .test_error_combo
            .expect("On error combo");
        assert!(
            enabled && clip.contains_rect(combo),
            "combo={combo:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(combo.center()),
                egui::Event::PointerButton {
                    pos: combo.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(editor_viewport),
            );
        }
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            Vec::new(),
            false,
            Some(editor_viewport),
        );
        let (choice, clip, enabled) = dialog
            .action_editor
            .test_error_continue
            .expect("Continue option");
        assert!(
            enabled && clip.contains_rect(choice),
            "choice={choice:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(choice.center()),
                egui::Event::PointerButton {
                    pos: choice.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(editor_viewport),
            );
        }
        assert!(matches!(
            dialog.action_editor.draft.as_ref().unwrap().on_error,
            crate::mkmacro::MkErrorPolicy::Continue
        ));
        render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), true);
        assert!(matches!(
            dialog.action_editor.draft.as_ref().unwrap().on_error,
            crate::mkmacro::MkErrorPolicy::Continue
        ));
        assert_eq!(dialog.draft_revision, revision);
        let (apply, clip) = action_editor::finish_button_geometry(&dialog.action_editor, true)
            .expect("Editor Apply");
        assert!(clip.contains_rect(apply), "apply={apply:?}, clip={clip:?}");
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(apply.center()),
                egui::Event::PointerButton {
                    pos: apply.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(editor_viewport),
            );
        }
        assert_eq!(dialog.draft_revision, revision + 1);
        assert_eq!(dialog.selected_macro().unwrap().steps.len(), before + 1);
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            Vec::new(),
            false,
            Some(catalog_viewport),
        );
        let (query, clip, enabled) = dialog
            .catalog
            .test_query
            .expect("Catalog query after editor close");
        assert!(enabled && clip.contains_rect(query));
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(query.center()),
                egui::Event::PointerButton {
                    pos: query.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(catalog_viewport),
            );
        }
        let select_all = egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        };
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![select_all, egui::Event::Text("Run Program".into())],
            false,
            Some(catalog_viewport),
        );
        assert_eq!(
            dialog.catalog.query, "Run Program",
            "Catalog query must be replaced before finding the new entry"
        );
        let (entry, clip, enabled) = dialog.catalog.test_run_program.expect("Run Program entry");
        assert!(
            enabled && clip.contains_rect(entry),
            "entry={entry:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(entry.center()),
                egui::Event::PointerButton {
                    pos: entry.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(catalog_viewport),
            );
        }
        assert!(dialog.action_editor.draft.is_some() && dialog.editor_host.is_popped_out());
        let viewport = super::super::utility_window_host::UtilityWindowHost::<
            action_editor::ActionEditorState,
        >::test_viewport(dialog.action_editor.host_epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(viewport),
            );
        }
        let (picker, clip, enabled): (egui::Rect, egui::Rect, bool) = ctx
            .data(|data| data.get_temp(egui::Id::new(("mkmacro-test-launcher-button", viewport))))
            .expect("Choose From Launcher control");
        assert!(
            enabled && clip.contains_rect(picker),
            "picker={picker:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(picker.center()),
                egui::Event::PointerButton {
                    pos: picker.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(viewport),
            );
        }
        assert!(dialog.launcher_action_picker.open && dialog.launcher_picker_host.is_popped_out());
        assert_eq!(dialog.draft_revision, revision + 1);
        let picker_viewport = super::super::utility_window_host::UtilityWindowHost::<
            launcher_action_picker::LauncherActionPickerState,
        >::test_viewport(dialog.launcher_action_picker.epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(picker_viewport),
            );
        }
        let (search, clip, enabled, focused) = dialog
            .launcher_action_picker
            .test_search_response
            .expect("Launcher picker search");
        assert!(
            enabled && focused && clip.contains_rect(search),
            "search={search:?}, clip={clip:?}, enabled={enabled}, focused={focused}"
        );
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("discarded".into())],
            true,
            Some(picker_viewport),
        );
        assert!(dialog.launcher_action_picker.search.is_empty());
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("query".into())],
            false,
            Some(picker_viewport),
        );
        assert_eq!(dialog.launcher_action_picker.search, "query");
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn native_review_opens_editor_and_returns_same_review_step() {
        use eframe::egui::{self, vec2};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let target = crate::mkmacro::RecordingTarget {
            macro_id: 1,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };
        let before = dialog.draft.clone();
        let revision = dialog.draft_revision;
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        manager.remember(
            &ctx,
            crate::settings::UtilityToolKey::parse("mkmacro-action-editor").unwrap(),
            crate::settings::RememberedPresentation::Docked,
        );
        manager.remember(
            &ctx,
            crate::settings::UtilityToolKey::parse("mkmacro-recording-review").unwrap(),
            crate::settings::RememberedPresentation::Docked,
        );
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        dialog
            .open_recording_review(review_result(
                target,
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 1,
                    ..Default::default()
                })],
            ))
            .unwrap();
        let review_id = dialog.recording_review.as_ref().unwrap().proposed_steps()[0].review_id;
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .select(review_id, false, false);
        for _ in 0..2 {
            render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        }
        assert!(dialog.review_host.is_popped_out());
        let review_viewport =
            super::super::utility_window_host::UtilityWindowHost::<
                recording_review::RecordingReviewSession,
            >::test_viewport(dialog.recording_review.as_ref().unwrap().epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(review_viewport),
            );
        }
        let review_epoch = dialog.recording_review.as_ref().unwrap().epoch;
        let (dock, clip, enabled) = dialog
            .recording_review
            .as_ref()
            .unwrap()
            .test_host_button
            .unwrap();
        assert!(
            enabled && clip.contains_rect(dock),
            "dock={dock:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(dock.center()),
                    egui::Event::PointerButton {
                        pos: dock.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(review_viewport),
            );
        }
        assert!(!dialog.review_host.is_popped_out());
        for _ in 0..2 {
            render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        }
        let (reopen, clip, enabled) = dialog.test_review_button.unwrap();
        assert!(
            enabled && clip.contains_rect(reopen),
            "reopen={reopen:?}, clip={clip:?}, enabled={enabled}"
        );
        click_host_position(&ctx, &mut dialog, screen, false, reopen.center(), false);
        assert!(dialog.review_host.is_popped_out());
        assert_eq!(
            dialog.recording_review.as_ref().unwrap().epoch,
            review_epoch
        );
        assert!(
            dialog
                .recording_review
                .as_ref()
                .unwrap()
                .selection
                .contains(&review_id)
        );
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(review_viewport),
            );
        }
        let (edit, clip, enabled) = dialog
            .recording_review
            .as_ref()
            .unwrap()
            .test_edit
            .expect("Review Edit control");
        assert!(
            enabled && clip.contains_rect(edit),
            "edit={edit:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(edit.center()),
                    egui::Event::PointerButton {
                        pos: edit.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(review_viewport),
            );
        }
        assert!(dialog.editor_host.is_popped_out());
        assert_eq!(dialog.action_editor.review_editing_id(), Some(review_id));
        dialog.action_editor.draft.as_mut().unwrap().action = MkAction::Delay(MkDelayPayload {
            fixed_ms: 99,
            ..Default::default()
        });
        let viewport = super::super::utility_window_host::UtilityWindowHost::<
            action_editor::ActionEditorState,
        >::test_viewport(dialog.action_editor.host_epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(viewport),
            );
        }
        let (apply, clip) =
            action_editor::finish_button_geometry(&dialog.action_editor, true).unwrap();
        assert!(clip.contains_rect(apply));
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(apply.center()),
                egui::Event::PointerButton {
                    pos: apply.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(viewport),
            );
        }
        assert_eq!(dialog.draft, before);
        assert_eq!(dialog.draft_revision, revision);
        assert!(!dialog.dirty);
        assert!(
            matches!(&dialog.recording_review.as_ref().unwrap().proposed_steps()[0].action, MkAction::Delay(payload) if payload.fixed_ms == 99)
        );
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(review_viewport),
            );
        }
        let (cancel, clip, enabled) = dialog
            .recording_review
            .as_ref()
            .unwrap()
            .test_cancel
            .unwrap();
        assert!(
            enabled && clip.contains_rect(cancel),
            "cancel={cancel:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(cancel.center()),
                    egui::Event::PointerButton {
                        pos: cancel.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(review_viewport),
            );
        }
        assert!(dialog.recording_review.is_none());
        dialog
            .open_recording_review(review_result(target, Vec::new()))
            .unwrap();
        for _ in 0..2 {
            render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        }
        assert!(dialog.review_host.is_popped_out());
        assert_ne!(
            dialog.recording_review.as_ref().unwrap().epoch,
            review_epoch
        );
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn stale_review_open_keeps_recording_without_root_fallback() {
        use super::super::utility_host::{
            AcceptedParentOpen, UtilityPresentation, ViewportPassProvenance,
        };
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        let target = crate::mkmacro::RecordingTarget {
            macro_id: 1,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };
        dialog
            .open_recording_review(review_result(target, Vec::new()))
            .unwrap();
        let epoch = dialog.recording_review.as_ref().unwrap().epoch;
        let ctx = eframe::egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        let pass = ViewportPassProvenance::current(&ctx, eframe::egui::ViewportId::ROOT);
        let output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
        let origin = AcceptedParentOpen::from_retained(
            &ctx,
            dialog.host_state.identity(),
            dialog.owner_instance,
            pass,
            pass,
        )
        .unwrap();
        output.drop_without_applying_deltas();
        dialog.pending_review_open = Some((Some(origin), false));
        dialog.host_state.transition(UtilityPresentation::Popout);
        assert!(!dialog.complete_review_child(&ctx));
        assert!(dialog.review_open_blocked);
        assert_eq!(dialog.recording_review.as_ref().unwrap().epoch, epoch);
        assert!(dialog.ordinary_child_root_layer().is_none());
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn native_annotation_retains_draft_and_adds_once_without_resuming_manual_pause() {
        use eframe::egui::{self, vec2};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        let recorder = Arc::new(crate::mkmacro::recorder_runtime::isolated_test_recorder(
            directory.path(),
        ));
        let previous = crate::mkmacro::runtime::replace_recorder_for_test(Some(recorder.clone()));
        recorder
            .start(1, crate::mkmacro::NormalizationConfig::default())
            .unwrap();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| {
            for font in style.text_styles.values_mut() {
                font.size *= 1.25;
            }
        });
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        manager.remember(
            &ctx,
            crate::settings::UtilityToolKey::parse("mkmacro-recording-annotation").unwrap(),
            crate::settings::RememberedPresentation::Docked,
        );
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        for _ in 0..2 {
            render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        }
        let (button, clip, enabled) = dialog.test_annotation_button.expect("Annotate control");
        assert!(
            enabled && clip.contains_rect(button),
            "button={button:?}, clip={clip:?}, enabled={enabled}"
        );
        click_host_position(&ctx, &mut dialog, screen, false, button.center(), false);
        assert!(dialog.annotation_host.is_popped_out());
        assert_eq!(
            recorder.snapshot().state,
            crate::mkmacro::RecorderRuntimeState::Paused
        );
        let epoch = dialog.annotation.epoch;
        let viewport = super::super::utility_window_host::UtilityWindowHost::<
            recording_annotation::AnnotationState,
        >::test_viewport(epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(viewport),
            );
        }
        let (text, clip, enabled, focused) = dialog.annotation.test_text.unwrap();
        assert!(
            enabled && focused && clip.contains_rect(text),
            "text={text:?}, clip={clip:?}, enabled={enabled}, focused={focused}"
        );
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("discarded".into())],
            true,
            Some(viewport),
        );
        assert!(dialog.annotation.text.is_empty());
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            Vec::new(),
            false,
            Some(viewport),
        );
        let (text, clip, enabled, focused) = dialog.annotation.test_text.unwrap();
        assert!(
            enabled && focused && clip.contains_rect(text),
            "after rejected pass: text={text:?}, clip={clip:?}, enabled={enabled}, focused={focused}"
        );
        render_host_frame_for(
            &ctx,
            &mut dialog,
            screen,
            false,
            vec![egui::Event::Text("note".into())],
            false,
            Some(viewport),
        );
        assert_eq!(dialog.annotation.text, "note");
        let (dock, clip, enabled) = dialog.annotation.test_host_button.unwrap();
        assert!(enabled && clip.contains_rect(dock));
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(dock.center()),
                    egui::Event::PointerButton {
                        pos: dock.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(viewport),
            );
        }
        assert!(!dialog.annotation_host.is_popped_out());
        assert_eq!(dialog.annotation.epoch, epoch);
        assert_eq!(dialog.annotation.text, "note");
        for _ in 0..2 {
            render_host_frame(&ctx, &mut dialog, screen, false, Vec::new(), false);
        }
        let (reopen, clip, enabled) = dialog.test_annotation_button.unwrap();
        assert!(enabled && clip.contains_rect(reopen));
        click_host_position(&ctx, &mut dialog, screen, false, reopen.center(), false);
        assert!(dialog.annotation_host.is_popped_out());
        assert_eq!(dialog.annotation.epoch, epoch);
        assert_eq!(dialog.annotation.text, "note");
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(viewport),
            );
        }
        let (add, clip, enabled) = dialog.annotation.test_add.unwrap();
        assert!(
            enabled && clip.contains_rect(add),
            "add={add:?}, clip={clip:?}, enabled={enabled}, owner={:?}, host={:?}, text={:?}",
            dialog.annotation.test_owner,
            dialog.annotation_host.identity(),
            dialog.annotation.text
        );
        let (cancel, cancel_clip, cancel_enabled) = dialog.annotation.test_cancel.unwrap();
        assert!(
            cancel_enabled && cancel_clip.contains_rect(cancel),
            "cancel={cancel:?}, clip={cancel_clip:?}, enabled={cancel_enabled}"
        );
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(add.center()),
                    egui::Event::PointerButton {
                        pos: add.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(viewport),
            );
        }
        assert!(!dialog.annotation.open);
        assert_eq!(
            recorder.snapshot().state,
            crate::mkmacro::RecorderRuntimeState::Recording
        );
        assert_eq!(recorder.stop().unwrap().notes.len(), 1);
        recorder
            .start(1, crate::mkmacro::NormalizationConfig::default())
            .unwrap();
        recorder.pause().unwrap();
        dialog.begin_recording_annotation().unwrap();
        assert!(!dialog.annotation.pause_owned);
        dialog.annotation_host.open_from_origin(
            &ctx,
            &dialog.annotation,
            false,
            &super::super::utility_host::ChildOpenOrigin::<()>::Root,
        );
        let manual_viewport = super::super::utility_window_host::UtilityWindowHost::<
            recording_annotation::AnnotationState,
        >::test_viewport(dialog.annotation.epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(manual_viewport),
            );
        }
        let (cancel, clip, enabled) = dialog.annotation.test_cancel.unwrap();
        assert!(enabled && clip.contains_rect(cancel));
        for pressed in [true, false] {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                vec![
                    egui::Event::PointerMoved(cancel.center()),
                    egui::Event::PointerButton {
                        pos: cancel.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                Some(manual_viewport),
            );
        }
        assert!(!dialog.annotation.open);
        assert_eq!(
            recorder.snapshot().state,
            crate::mkmacro::RecorderRuntimeState::Paused
        );
        recorder.stop().unwrap();
        recorder
            .start(1, crate::mkmacro::NormalizationConfig::default())
            .unwrap();
        dialog.begin_recording_annotation().unwrap();
        dialog.annotation_host.open_from_origin(
            &ctx,
            &dialog.annotation,
            false,
            &super::super::utility_host::ChildOpenOrigin::<()>::Root,
        );
        let close_viewport = super::super::utility_window_host::UtilityWindowHost::<
            recording_annotation::AnnotationState,
        >::test_viewport(dialog.annotation.epoch);
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(close_viewport),
            );
        }
        super::super::render::with_immediate_viewport_test_close_request(close_viewport, || {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(close_viewport),
            );
        });
        assert!(!dialog.annotation.open);
        assert_eq!(
            recorder.snapshot().state,
            crate::mkmacro::RecorderRuntimeState::Recording
        );
        dialog.begin_recording_annotation().unwrap();
        dialog.annotation_host.open_from_origin(
            &ctx,
            &dialog.annotation,
            false,
            &super::super::utility_host::ChildOpenOrigin::<()>::Root,
        );
        let retired_id = recorder.snapshot().recording_id.unwrap();
        recorder.stop().unwrap();
        recorder
            .start(1, crate::mkmacro::NormalizationConfig::default())
            .unwrap();
        let new_id = recorder.snapshot().recording_id.unwrap();
        assert_ne!(retired_id, new_id);
        crate::mkmacro::runtime::set_recording_annotation_active(new_id, true).unwrap();
        dialog.complete_annotation_child(&ctx);
        assert!(!dialog.annotation.open);
        assert_eq!(
            recorder.snapshot().state,
            crate::mkmacro::RecorderRuntimeState::Recording
        );
        assert_eq!(
            crate::mkmacro::runtime::recording_annotation_owner_for_test(),
            new_id
        );
        crate::mkmacro::runtime::set_recording_annotation_active(new_id, false).unwrap();
        recorder.stop().unwrap();
        dialog.force_close(&ctx);
        crate::mkmacro::runtime::replace_recorder_for_test(previous);
        workers.join_all();
    }

    #[test]
    fn stale_native_crop_save_never_writes_asset() {
        use eframe::egui::{self, vec2};
        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let action = crate::mkmacro::MkAction::ImageFind(crate::mkmacro::MkImagePayload {
            image: crate::mkmacro::MkImageRef::from_filename("source.png"),
            region: crate::mkmacro::SearchRegion::Desktop,
            wait: crate::mkmacro::MkWaitOptions::default(),
            tolerance: 0,
            alpha: crate::mkmacro::AlphaPolicy::Ignore,
            return_point: crate::mkmacro::ReturnPoint::TopLeft,
            not_found_policy: crate::mkmacro::MkImageNotFoundPolicy::Fail,
            outputs: Default::default(),
        });
        dialog.action_editor.begin_new(action);
        dialog.action_editor.bind_owner(Some(1));
        let destination = dialog.action_editor.image_crop_destination(1).unwrap();
        dialog.image_crop_editor.test_seed_loading(destination);
        let ctx = egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        manager.remember(
            &ctx,
            crate::settings::UtilityToolKey::parse("mkmacro-reference-crop").unwrap(),
            crate::settings::RememberedPresentation::Popout,
        );
        dialog.crop_host.open_from_origin(
            &ctx,
            &dialog.image_crop_editor,
            false,
            &super::super::utility_host::ChildOpenOrigin::<()>::Root,
        );
        let viewport = super::super::utility_window_host::UtilityWindowHost::<
            image_crop_editor::ImageCropEditorState,
        >::test_viewport(dialog.image_crop_editor.epoch);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        for _ in 0..2 {
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                Vec::new(),
                false,
                Some(viewport),
            );
        }
        let identity = dialog.crop_host.identity();
        let (status, rect, clip) = dialog.image_crop_editor.test_status.unwrap();
        assert_eq!(status, "loading");
        assert!(clip.contains_rect(rect), "loading={rect:?}, clip={clip:?}");
        dialog.image_crop_editor.test_set_error();
        render_host_frame_for(&ctx, &mut dialog, screen, false, Vec::new(), false, Some(viewport));
        let (status, rect, clip) = dialog.image_crop_editor.test_status.unwrap();
        assert_eq!(status, "error");
        assert!(clip.contains_rect(rect), "error={rect:?}, clip={clip:?}");
        assert_eq!(dialog.crop_host.identity(), identity);
        dialog.image_crop_editor.test_set_ready();
        for _ in 0..2 {
            render_host_frame_for(&ctx, &mut dialog, screen, false, Vec::new(), false, Some(viewport));
        }
        assert_eq!(dialog.crop_host.identity(), identity);
        dialog.action_editor.draft_generation =
            dialog.action_editor.draft_generation.wrapping_add(1);
        let (save, clip, enabled) = dialog
            .image_crop_editor
            .test_save_button()
            .expect("Crop Save As");
        assert!(
            enabled && clip.contains_rect(save),
            "save={save:?}, clip={clip:?}, enabled={enabled}"
        );
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(save.center()),
                egui::Event::PointerButton {
                    pos: save.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            render_host_frame_for(
                &ctx,
                &mut dialog,
                screen,
                false,
                events,
                false,
                Some(viewport),
            );
        }
        assert!(
            !dialog
                .store
                .asset_root()
                .join("should-not-write.png")
                .exists()
        );
        assert!(
            dialog
                .action_editor
                .capture_message
                .as_deref()
                .is_some_and(|message| message.contains("no image was saved"))
        );
        assert!(dialog.image_crop_editor.is_open());
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn mkmacro_detached_action_editor_finishes_only_on_retained_clicks() {
        use eframe::egui::{self, vec2};

        let (directory, mut dialog, _runtime, _fake) = real_runtime_dialog();
        dialog.open = true;
        dialog.set_selected_macro(Some(1));
        let ctx = egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = super::super::utility_presentation::UtilityPresentationManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            crate::settings::UtilityPresentations::default(),
        );
        manager.install_in_context(&ctx);
        manager.remember(
            &ctx,
            crate::settings::UtilityToolKey::parse("mkmacro-action-editor").unwrap(),
            crate::settings::RememberedPresentation::Popout,
        );
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0));
        click_host_button(&ctx, &mut dialog, screen, true, false, false);
        assert!(dialog.is_popped_out());

        let original = dialog.selected_macro().unwrap().steps[0].clone();
        dialog.action_editor.begin_edit_in_macro(Some(1), &original);
        dialog.editor_host.open_from_origin(
            &ctx,
            &dialog.action_editor,
            false,
            &super::super::utility_host::ChildOpenOrigin::<()>::Root,
        );
        dialog.action_editor.draft.as_mut().unwrap().delay_after_ms = 125;

        for (apply, discard) in [(true, true), (true, false), (false, true), (false, false)] {
            if !apply && dialog.action_editor.draft.is_none() {
                let step = dialog.selected_macro().unwrap().steps[0].clone();
                dialog.action_editor.begin_edit_in_macro(Some(1), &step);
                dialog.editor_host.open_from_origin(
                    &ctx,
                    &dialog.action_editor,
                    false,
                    &super::super::utility_host::ChildOpenOrigin::<()>::Root,
                );
                dialog.action_editor.draft.as_mut().unwrap().delay_after_ms = 250;
            }
            let viewport = super::super::utility_window_host::UtilityWindowHost::<
                action_editor::ActionEditorState,
            >::test_viewport(dialog.action_editor.host_epoch);
            for _ in 0..3 {
                render_host_frame_for(
                    &ctx,
                    &mut dialog,
                    screen,
                    false,
                    Vec::new(),
                    false,
                    Some(viewport),
                );
            }
            let (rect, clip) = action_editor::finish_button_geometry(&dialog.action_editor, apply)
                .expect("Action Editor footer button was rendered");
            assert!(clip.contains_rect(rect));
            for pressed in [true, false] {
                let events = vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                render_host_frame_for(
                    &ctx,
                    &mut dialog,
                    screen,
                    false,
                    events,
                    discard && !pressed,
                    Some(viewport),
                );
            }
            assert_eq!(
                dialog.action_editor.draft.is_some(),
                discard,
                "apply={apply}, discard={discard}"
            );
            let delay = dialog.selected_macro().unwrap().steps[0].delay_after_ms;
            assert_eq!(
                delay,
                if apply && discard {
                    original.delay_after_ms
                } else {
                    125
                }
            );
        }
        dialog.force_close(&ctx);
        workers.join_all();
    }

    #[test]
    fn runtime_inspector_lifecycle_replaces_debug_runs_and_clears_on_normal_run() {
        let (_dir, mut dialog, runtime, _fake) = real_runtime_dialog();
        let first_previous_run_id = runtime.snapshot().run_id;
        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            crate::mkmacro::CommandResult::Accepted
        );
        let first_started = wait_for_debug_boundary_after(
            &runtime,
            first_previous_run_id,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
        );
        dialog.observe_runtime_snapshot(Some(first_started.clone()));
        assert_eq!(
            dialog
                .runtime_inspector_snapshot
                .as_ref()
                .map(|snapshot| snapshot.run_id),
            Some(first_started.run_id)
        );
        assert!(dialog.runtime_inspector_is_current_debug_run);

        let first = wait_for_terminal_after(&runtime, first_previous_run_id);
        assert_eq!(first.state, RuntimeState::Completed);
        assert_eq!(first.run_mode, crate::mkmacro::RuntimeRunMode::Debug);
        assert_eq!(first.last_completed_step_id, Some(12));
        assert_eq!(first.debug_variables_step_id, Some(12));
        let first_debug = first
            .debug_snapshot
            .as_ref()
            .expect("completed debug run retains its final snapshot");
        assert_eq!(
            first_debug.reason,
            crate::mkmacro::DebugSnapshotReason::RunFinished
        );
        assert_eq!(first_debug.step_id, Some(12));
        assert_eq!(
            first_debug.variables.get("count"),
            Some(&MkValue::Number(1.0))
        );
        assert_eq!(
            first_debug.variables.get("result"),
            Some(&MkValue::String("done".into()))
        );
        dialog.observe_runtime_snapshot(Some(first.clone()));
        assert!(!dialog.runtime_inspector_is_current_debug_run);
        let retained_first = dialog
            .runtime_inspector_snapshot
            .as_ref()
            .expect("completed debug snapshot is retained by the dialog");
        assert_eq!(retained_first.state, RuntimeState::Completed);
        assert_eq!(retained_first.run_id, first.run_id);
        assert_eq!(
            retained_first.debug_snapshot.as_ref().unwrap().reason,
            crate::mkmacro::DebugSnapshotReason::RunFinished
        );
        assert_eq!(
            retained_first.debug_snapshot.as_ref().unwrap().step_id,
            Some(12)
        );
        assert_eq!(
            retained_first
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("count"),
            Some(&MkValue::Number(1.0))
        );
        assert_eq!(
            retained_first
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("result"),
            Some(&MkValue::String("done".into()))
        );
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(
                retained_first,
                &dialog.draft,
            )
            .variable_label,
            "Variables at successful completion"
        );
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(
                retained_first,
                &dialog.draft,
            )
            .title,
            "Runtime Inspector — Last Debug Run"
        );

        let second_previous_run_id = first.run_id;
        submit_after_previous_run(&runtime, RuntimeCommand::DebugRun(2));
        let second_started = wait_for_debug_boundary_after(
            &runtime,
            second_previous_run_id,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
        );
        assert!(second_started.run_id > first.run_id);
        assert_eq!(second_started.state, RuntimeState::Running);
        assert!(
            !second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("count")
        );
        assert!(
            !second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("result")
        );
        assert_eq!(
            second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("macro.id"),
            Some(&MkValue::Number(2.0))
        );
        dialog.observe_runtime_snapshot(Some(second_started.clone()));
        assert_eq!(
            dialog.runtime_inspector_snapshot.as_ref().unwrap().run_id,
            second_started.run_id
        );
        assert!(dialog.runtime_inspector_is_current_debug_run);

        let second = wait_for_terminal_after(&runtime, second_previous_run_id);
        assert_eq!(second.state, RuntimeState::Completed);
        assert_eq!(second.run_mode, crate::mkmacro::RuntimeRunMode::Debug);
        assert_eq!(
            second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("fresh"),
            Some(&MkValue::String("new".into()))
        );
        assert!(
            !second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("count")
        );
        assert!(
            !second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("result")
        );
        dialog.observe_runtime_snapshot(Some(second.clone()));
        let retained_second = dialog
            .runtime_inspector_snapshot
            .as_ref()
            .expect("second completed debug snapshot replaces the first");
        assert_eq!(retained_second.run_id, second.run_id);
        let retained_second_variables = &retained_second.debug_snapshot.as_ref().unwrap().variables;
        assert_eq!(
            retained_second_variables.get("fresh"),
            Some(&MkValue::String("new".into()))
        );
        assert!(!retained_second_variables.contains_key("count"));
        assert!(!retained_second_variables.contains_key("result"));

        submit_after_previous_run(&runtime, RuntimeCommand::Run(2));
        let normal_started = wait_for_mode_after(
            &runtime,
            second.run_id,
            crate::mkmacro::RuntimeRunMode::Normal,
        );
        assert!(normal_started.debug_snapshot.is_none());
        dialog.observe_runtime_snapshot(Some(normal_started));
        assert!(dialog.runtime_inspector_snapshot.is_none());
        assert!(!dialog.runtime_inspector_is_current_debug_run);

        let normal = wait_for_terminal_after(&runtime, second.run_id);
        assert_eq!(normal.run_mode, crate::mkmacro::RuntimeRunMode::Normal);
        assert!(normal.debug_snapshot.is_none());
        assert!(normal.debug_variables.is_empty());
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(&normal, &dialog.draft)
                .title,
            "Runtime Inspector — Normal Run"
        );
    }

    #[test]
    fn runtime_inspector_opens_once_per_breakpoint_occurrence() {
        let (_dir, mut dialog) = dialog();
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            1,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
            None,
        )));
        let breakpoint = synthetic_debug_snapshot(
            11,
            2,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint {
                step_id: 99,
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
            }),
        );
        dialog.observe_runtime_snapshot(Some(breakpoint.clone()));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_active_breakpoint,
            Some(crate::mkmacro::BreakpointOccurrence {
                occurrence: 1,
                run_id: 11,
                frame_id: 1,
                step: crate::mkmacro::MacroStepKey::new(7, 99)
            })
        );

        dialog.runtime_inspector_open = false;
        let mut newer_breakpoint = (*breakpoint).clone();
        newer_breakpoint.revision = 3;
        newer_breakpoint.debug_variables = Arc::new(crate::mkmacro::RuntimeVariables::from([(
            "answer".into(),
            crate::mkmacro::MkValue::Number(84.0),
        )]));
        let newer_variables = newer_breakpoint.debug_variables.clone();
        newer_breakpoint.debug_snapshot = Some(Arc::new(crate::mkmacro::DebugSnapshot {
            frame: crate::mkmacro::ExecutionFrameContext::root(7),
            macro_name: Arc::from("Macro 7"),
            step_id: Some(99),
            variables: newer_variables,
            reason: crate::mkmacro::DebugSnapshotReason::Breakpoint,
        }));
        dialog.observe_runtime_snapshot(Some(Arc::new(newer_breakpoint)));
        assert!(!dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_snapshot.as_ref().unwrap().revision,
            3
        );
        assert_eq!(
            dialog
                .runtime_inspector_snapshot
                .as_ref()
                .unwrap()
                .debug_variables
                .get("answer"),
            Some(&crate::mkmacro::MkValue::Number(84.0))
        );
        assert_eq!(
            dialog.runtime_inspector_active_breakpoint,
            Some(crate::mkmacro::BreakpointOccurrence {
                occurrence: 1,
                run_id: 11,
                frame_id: 1,
                step: crate::mkmacro::MacroStepKey::new(7, 99)
            })
        );

        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            4,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::StepBoundary,
            None,
        )));
        assert!(dialog.runtime_inspector_snapshot.is_some());
        assert_eq!(dialog.runtime_inspector_active_breakpoint, None);

        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            5,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint {
                step_id: 99,
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
            }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_active_breakpoint,
            Some(crate::mkmacro::BreakpointOccurrence {
                occurrence: 1,
                run_id: 11,
                frame_id: 1,
                step: crate::mkmacro::MacroStepKey::new(7, 99)
            })
        );

        dialog.runtime_inspector_open = false;
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            6,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint {
                step_id: 100,
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
            }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_active_breakpoint,
            Some(crate::mkmacro::BreakpointOccurrence {
                occurrence: 1,
                run_id: 11,
                frame_id: 1,
                step: crate::mkmacro::MacroStepKey::new(7, 100)
            })
        );

        dialog.runtime_inspector_open = false;
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            12,
            1,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint {
                step_id: 100,
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
            }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_active_breakpoint,
            Some(crate::mkmacro::BreakpointOccurrence {
                occurrence: 1,
                run_id: 12,
                frame_id: 1,
                step: crate::mkmacro::MacroStepKey::new(7, 100)
            })
        );
    }

    #[test]
    fn runtime_inspector_reopens_for_unobserved_same_frame_breakpoint_reentry() {
        let (_dir, mut dialog) = dialog();
        let paused = synthetic_debug_snapshot(
            18,
            1,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint {
                step_id: 99,
                frame: crate::mkmacro::ExecutionFrameContext::root(7),
            }),
        );
        dialog.observe_runtime_snapshot(Some(paused.clone()));
        assert!(dialog.runtime_inspector_open);
        dialog.runtime_inspector_open = false;
        let mut same_pause = (*paused).clone();
        same_pause.revision += 1;
        dialog.observe_runtime_snapshot(Some(Arc::new(same_pause.clone())));
        assert!(!dialog.runtime_inspector_open);
        let mut reentered = same_pause;
        reentered.revision += 2;
        reentered.breakpoint_sequence += 1;
        dialog.observe_runtime_snapshot(Some(Arc::new(reentered.clone())));
        assert!(dialog.runtime_inspector_open);
        dialog.runtime_inspector_open = false;
        reentered.revision += 1;
        dialog.observe_runtime_snapshot(Some(Arc::new(reentered.clone())));
        assert!(!dialog.runtime_inspector_open);
        // A new invocation of the same macro and step also has a distinct key.
        let mut frame = crate::mkmacro::ExecutionFrameContext::root(7);
        frame.frame_id = 4;
        reentered.pause_reason =
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id: 99, frame });
        dialog.observe_runtime_snapshot(Some(Arc::new(reentered)));
        assert!(dialog.runtime_inspector_open);
    }

    fn review_result(
        target: crate::mkmacro::RecordingTarget,
        actions: Vec<MkAction>,
    ) -> crate::mkmacro::RecordingResult {
        let steps = actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| crate::mkmacro::PlannedStep {
                review_id: crate::mkmacro::ReviewStepId(index as u64 + 1),
                source: crate::mkmacro::RecordingSourceSpan {
                    first: index,
                    last: index,
                },
                association_source: index,
                provenance: crate::mkmacro::RecordingProvenance::Literal,
                action,
                enabled: true,
                breakpoint: false,
                delay_after_ms: 0,
                repeat: 1,
                on_error: crate::mkmacro::MkErrorPolicy::Stop,
                metadata: Default::default(),
            })
            .collect();
        crate::mkmacro::RecordingResult {
            target,
            literal_steps: Vec::new(),
            plan: crate::mkmacro::RecordingPlan { steps },
            suggestions: Vec::new(),
            clipboard_observations: Vec::new(),
            click_inspections: Vec::new(),
            window_observations: Vec::new(),
            notes: Vec::new(),
            capture_duration: Duration::from_secs(1),
            raw_event_count: 2,
            dropped_event_count: 0,
        }
    }

    fn freeze_review_result(
        target: crate::mkmacro::RecordingTarget,
        secret: &str,
    ) -> crate::mkmacro::RecordingResult {
        let mut result = review_result(
            target,
            vec![MkAction::Hotkey(vec![
                MkKey::LeftControl,
                MkKey::Character("V".into()),
            ])],
        );
        result.clipboard_observations = vec![crate::mkmacro::ClipboardObservation {
            timestamp_us: 10,
            text: crate::mkmacro::SensitiveClipboardText::new(secret.into()),
        }];
        result.suggestions = crate::mkmacro::discover_suggestions(
            &result.plan,
            &crate::mkmacro::MkRecorderSettings::default(),
            &crate::mkmacro::ObservationBaseline::default(),
            &[],
            &result.clipboard_observations,
            &[(0, 10)],
        );
        result
    }

    fn delay_step(id: u64, fixed_ms: u64) -> MkStep {
        MkStep {
            id,
            action: MkAction::Delay(MkDelayPayload {
                fixed_ms,
                ..Default::default()
            }),
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn recording_review_applies_at_start_anchor_and_undo_redo_is_one_transaction() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps =
            vec![delay_step(1, 1), delay_step(2, 2), delay_step(3, 3)];
        dialog.mark_dirty();
        let anchor = dialog.selected_macro().unwrap().steps[1].clone();
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: Some(anchor.id),
            insertion_anchor_generation: dialog
                .step_instance_generations
                .get(&(macro_id, anchor.id))
                .copied(),
        };
        dialog.create_macro();
        let other_macro_id = dialog.selected_macro_id.unwrap();
        dialog.set_selected_macro(Some(macro_id));
        dialog
            .open_recording_review(review_result(
                target,
                vec![
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 10,
                        ..Default::default()
                    }),
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 20,
                        ..Default::default()
                    }),
                ],
            ))
            .unwrap();
        dialog.selection.replace([3]);
        dialog.set_selected_macro(Some(other_macro_id));
        let ids = dialog.apply_recording_review().unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(dialog.selected_macro_id, Some(other_macro_id));
        assert!(dialog.selected_macro().unwrap().steps.is_empty());
        let values: Vec<_> = dialog
            .draft
            .macros
            .iter()
            .find(|item| item.id == macro_id)
            .unwrap()
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                MkAction::Delay(payload) => Some(payload.fixed_ms),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec![1, 2, 10, 20, 3]);
        dialog.undo_last_recording_insert().unwrap();
        assert_eq!(
            dialog
                .draft
                .macros
                .iter()
                .find(|item| item.id == macro_id)
                .unwrap()
                .steps
                .len(),
            3
        );
        dialog.redo_last_recording_insert().unwrap();
        assert_eq!(
            dialog
                .draft
                .macros
                .iter()
                .find(|item| item.id == macro_id)
                .unwrap()
                .steps
                .len(),
            5
        );
    }

    #[test]
    fn completed_recording_and_cancel_are_a_total_draft_and_store_no_op() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps = vec![delay_step(1, 1)];
        dialog.mark_dirty();
        dialog.save().unwrap();
        let draft_before = dialog.draft.clone();
        let stored_before = dialog.store.snapshot();
        let revision_before = dialog.draft_revision();

        dialog
            .open_recording_review(review_result(
                crate::mkmacro::RecordingTarget {
                    macro_id,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 99,
                    ..Default::default()
                })],
            ))
            .unwrap();

        assert_eq!(dialog.draft, draft_before);
        assert_eq!(dialog.store.snapshot(), stored_before);
        assert!(!dialog.dirty);
        assert_eq!(dialog.draft_revision(), revision_before);
        dialog.cancel_recording_review();
        assert_eq!(dialog.draft, draft_before);
        assert_eq!(dialog.store.snapshot(), stored_before);
        assert!(!dialog.dirty);
        assert_eq!(dialog.draft_revision(), revision_before);
    }

    #[test]
    fn disabled_freeze_paste_never_materializes_or_persists_clipboard_text() {
        const SECRET: &str = "recording-only clipboard sentinel";
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.save().unwrap();
        let stored_before = dialog.store.snapshot();
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };

        dialog
            .open_recording_review(freeze_review_result(target, SECRET))
            .unwrap();
        assert!(
            dialog
                .recording_review
                .as_ref()
                .unwrap()
                .enabled_suggestions
                .is_empty()
        );
        dialog.cancel_recording_review();
        assert!(
            !serde_json::to_string(&dialog.draft)
                .unwrap()
                .contains(SECRET)
        );
        assert_eq!(dialog.store.snapshot(), stored_before);

        dialog
            .open_recording_review(freeze_review_result(target, SECRET))
            .unwrap();
        dialog.discard_all_recording_reviews();
        assert!(dialog.recording_review.is_none());
        assert!(
            !serde_json::to_string(&dialog.draft)
                .unwrap()
                .contains(SECRET)
        );
        assert_eq!(dialog.store.snapshot(), stored_before);

        dialog
            .open_recording_review(freeze_review_result(target, SECRET))
            .unwrap();
        let inserted = dialog.apply_recording_review().unwrap();
        assert_eq!(inserted.len(), 1);
        assert!(matches!(
            &dialog.selected_macro().unwrap().steps[0].action,
            MkAction::Hotkey(keys)
                if keys == &vec![MkKey::LeftControl, MkKey::Character("V".into())]
        ));
        assert!(
            !serde_json::to_string(&dialog.draft)
                .unwrap()
                .contains(SECRET)
        );
        assert_eq!(dialog.store.snapshot(), stored_before);
    }

    #[test]
    fn no_anchor_appends_with_fresh_ids() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps = vec![delay_step(1, 1), delay_step(2, 2)];
        dialog.mark_dirty();
        dialog
            .open_recording_review(review_result(
                crate::mkmacro::RecordingTarget {
                    macro_id,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                vec![
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 10,
                        ..Default::default()
                    }),
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 20,
                        ..Default::default()
                    }),
                ],
            ))
            .unwrap();

        let inserted = dialog.apply_recording_review().unwrap();
        assert_eq!(inserted.len(), 2);
        assert!(inserted.iter().all(|id| ![1, 2].contains(id)));
        let all_ids: HashSet<_> = dialog
            .selected_macro()
            .unwrap()
            .steps
            .iter()
            .map(|step| step.id)
            .collect();
        assert_eq!(all_ids.len(), 4);
        let values: Vec<_> = dialog
            .selected_macro()
            .unwrap()
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                MkAction::Delay(payload) => Some(payload.fixed_ms),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec![1, 2, 10, 20]);
    }

    #[test]
    fn deleted_anchor_blocks_apply_until_explicit_append_recovery() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps = vec![delay_step(1, 1), delay_step(2, 2)];
        dialog.mark_dirty();
        let generation = dialog
            .step_instance_generations
            .get(&(macro_id, 1))
            .copied();
        dialog
            .open_recording_review(review_result(
                crate::mkmacro::RecordingTarget {
                    macro_id,
                    insertion_anchor_step_id: Some(1),
                    insertion_anchor_generation: generation,
                },
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 9,
                    ..Default::default()
                })],
            ))
            .unwrap();

        dialog.selected_macro_mut().unwrap().steps.remove(0);
        dialog.mark_dirty();
        assert!(dialog.apply_recording_review().is_err());
        assert!(dialog.recording_review.is_some());
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .recover_append_to_original_macro();
        dialog.apply_recording_review().unwrap();
        let values: Vec<_> = dialog
            .selected_macro()
            .unwrap()
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                MkAction::Delay(payload) => Some(payload.fixed_ms),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec![2, 9]);
    }

    #[test]
    fn deleted_target_macro_preserves_review_until_explicit_retarget() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let target_id = dialog.selected_macro_id.unwrap();
        dialog.create_macro();
        let recovery_id = dialog.selected_macro_id.unwrap();
        dialog.set_selected_macro(Some(target_id));
        dialog
            .open_recording_review(review_result(
                crate::mkmacro::RecordingTarget {
                    macro_id: target_id,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 9,
                    ..Default::default()
                })],
            ))
            .unwrap();

        dialog.delete_selected_macro();
        assert!(dialog.apply_recording_review().is_err());
        assert!(dialog.recording_review.is_some());
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .retarget_append(recovery_id);
        dialog.apply_recording_review().unwrap();
        let recovery = dialog
            .draft
            .macros
            .iter()
            .find(|item| item.id == recovery_id)
            .unwrap();
        assert_eq!(recovery.steps.len(), 1);
    }

    #[test]
    fn recording_insert_history_blocks_stale_undo_and_redo() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps = vec![delay_step(1, 1)];
        dialog.mark_dirty();
        dialog
            .open_recording_review(review_result(
                crate::mkmacro::RecordingTarget {
                    macro_id,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 2,
                    ..Default::default()
                })],
            ))
            .unwrap();
        dialog.apply_recording_review().unwrap();
        let inserted_state = dialog.selected_macro().unwrap().steps.clone();

        dialog.selected_macro_mut().unwrap().steps[0]
            .metadata
            .comment = "changed".into();
        dialog.mark_dirty();
        assert!(!dialog.can_undo_recording_insert());
        assert!(dialog.undo_last_recording_insert().is_err());
        dialog.selected_macro_mut().unwrap().steps = inserted_state;
        dialog.mark_dirty();
        dialog.undo_last_recording_insert().unwrap();
        assert_eq!(dialog.selected_macro().unwrap().steps.len(), 1);

        dialog.selected_macro_mut().unwrap().steps[0]
            .metadata
            .comment = "changed again".into();
        dialog.mark_dirty();
        assert!(!dialog.can_redo_recording_insert());
        assert!(dialog.redo_last_recording_insert().is_err());
        assert_eq!(dialog.selected_macro().unwrap().steps.len(), 1);
    }

    #[test]
    fn preview_is_transient_and_review_terminal_actions_stop_the_owned_run() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let store = Arc::new(store);
        let mut dialog = MkMacroDialog::new(store.clone());
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.selected_macro_mut().unwrap().steps = vec![delay_step(1, 1)];
        dialog.mark_dirty();
        dialog.save().unwrap();
        let fake = Arc::new(crate::mkmacro::executor::fake::FakeBackend::default());
        let runtime = Arc::new(MacroRuntime::new(store.clone(), fake.clone().backends()));
        dialog.set_recording_preview_runtime_for_test(runtime.clone());
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };
        let long_preview = || {
            review_result(
                target,
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                })],
            )
        };
        let wait_for_state = |wanted, previous_run_id| {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let snapshot = runtime.snapshot();
                if snapshot.run_id > previous_run_id && snapshot.state == wanted {
                    break snapshot;
                }
                assert!(
                    Instant::now() < deadline,
                    "preview did not reach {wanted:?}: {snapshot:?}"
                );
                thread::sleep(Duration::from_millis(2));
            }
        };
        let draft_before = dialog.draft.clone();
        let stored_before = store.snapshot();

        dialog.open_recording_review(long_preview()).unwrap();
        let previous = runtime.snapshot().run_id;
        let barrier = runtime.install_test_worker_barrier();
        let cancel_ticket = dialog.preview_recording_review(false).unwrap();
        barrier.wait_until_blocked();
        assert_eq!(dialog.draft, draft_before);
        assert_eq!(store.snapshot(), stored_before);
        dialog.cancel_recording_review();
        barrier.release();
        let cancelled = wait_for_state(RuntimeState::Stopped, previous);
        assert_eq!(
            cancelled.origin,
            crate::mkmacro::RuntimeOrigin::RecordingPreview {
                ticket: cancel_ticket
            }
        );
        assert!(fake.events().is_empty());

        dialog.open_recording_review(long_preview()).unwrap();
        let previous = cancelled.run_id;
        let barrier = runtime.install_test_worker_barrier();
        let apply_ticket = dialog.preview_recording_review(false).unwrap();
        barrier.wait_until_blocked();
        dialog.apply_recording_review().unwrap();
        barrier.release();
        let applied = wait_for_state(RuntimeState::Stopped, previous);
        assert_eq!(
            applied.origin,
            crate::mkmacro::RuntimeOrigin::RecordingPreview {
                ticket: apply_ticket
            }
        );
        assert!(fake.events().is_empty());

        dialog.open_recording_review(long_preview()).unwrap();
        let previous = applied.run_id;
        let barrier = runtime.install_test_worker_barrier();
        let close_ticket = dialog.preview_recording_review(false).unwrap();
        barrier.wait_until_blocked();
        assert!(dialog.close_with_decision(DirtyDecision::Discard));
        barrier.release();
        let closed = wait_for_state(RuntimeState::Stopped, previous);
        assert_eq!(
            closed.origin,
            crate::mkmacro::RuntimeOrigin::RecordingPreview {
                ticket: close_ticket
            }
        );
        assert!(fake.events().is_empty());

        assert_eq!(
            runtime.command(RuntimeCommand::Run(macro_id)),
            crate::mkmacro::CommandResult::Accepted
        );
        let later = wait_for_state(RuntimeState::Completed, closed.run_id);
        assert_eq!(later.origin, crate::mkmacro::RuntimeOrigin::Stored);
    }

    #[test]
    fn reused_anchor_id_blocks_apply_and_explicit_append_preserves_review() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        let original = delay_step(8, 8);
        dialog.selected_macro_mut().unwrap().steps = vec![original.clone()];
        dialog.mark_dirty();
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: Some(8),
            insertion_anchor_generation: dialog
                .step_instance_generations
                .get(&(macro_id, 8))
                .copied(),
        };
        dialog
            .open_recording_review(review_result(
                target,
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 9,
                    ..Default::default()
                })],
            ))
            .unwrap();
        dialog.selected_macro_mut().unwrap().steps.clear();
        dialog.mark_dirty();
        dialog.selected_macro_mut().unwrap().steps.push(original);
        dialog.mark_dirty();
        assert!(
            dialog
                .recording_review_apply_blocker()
                .is_some_and(|message| message.contains("original insertion step"))
        );
        assert!(dialog.apply_recording_review().is_err());
        assert!(dialog.recording_review.is_some());
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .recover_append_to_original_macro();
        dialog.apply_recording_review().unwrap();
        assert_eq!(dialog.selected_macro().unwrap().steps.len(), 2);
    }

    #[test]
    fn completed_recordings_queue_without_overwriting_active_review() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let first_id = dialog.selected_macro_id.unwrap();
        dialog.create_macro();
        let second_id = dialog.selected_macro_id.unwrap();
        let target = |macro_id| crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };
        dialog
            .open_recording_review(review_result(target(first_id), Vec::new()))
            .unwrap();
        assert_eq!(
            dialog
                .open_recording_review(review_result(target(second_id), Vec::new()))
                .unwrap(),
            RecordingReviewOpenDisposition::Queued
        );
        assert_eq!(
            dialog.recording_review.as_ref().unwrap().target.macro_id,
            first_id
        );
        dialog.cancel_recording_review();
        assert_eq!(
            dialog.recording_review.as_ref().unwrap().target.macro_id,
            second_id
        );
        dialog
            .open_recording_review(review_result(target(first_id), Vec::new()))
            .unwrap();
        assert!(dialog.close_with_decision(DirtyDecision::Discard));
        assert!(dialog.recording_review.is_none());
        assert!(dialog.queued_recording_reviews.is_empty());
    }

    #[test]
    fn review_action_editor_returns_clone_without_dirtying_document() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        let macro_id = dialog.selected_macro_id.unwrap();
        dialog.save().unwrap();
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id: None,
            insertion_anchor_generation: None,
        };
        dialog
            .open_recording_review(review_result(
                target,
                vec![MkAction::Delay(MkDelayPayload {
                    fixed_ms: 1,
                    ..Default::default()
                })],
            ))
            .unwrap();
        let review_id = dialog.recording_review.as_ref().unwrap().proposed_steps()[0].review_id;
        let step = dialog
            .recording_review
            .as_ref()
            .unwrap()
            .step_for_editor(review_id)
            .unwrap();
        let document_before = dialog.draft.clone();
        dialog
            .action_editor
            .begin_review_edit(macro_id, review_id, &step);
        let draft = dialog.action_editor.draft.as_mut().unwrap();
        draft.action = MkAction::Delay(MkDelayPayload {
            fixed_ms: 99,
            ..Default::default()
        });
        draft.on_error = crate::mkmacro::MkErrorPolicy::Continue;
        let (edited_id, edited) = dialog.action_editor.take_review_edited_step().unwrap();
        dialog
            .recording_review
            .as_mut()
            .unwrap()
            .replace_step(edited_id, &edited)
            .unwrap();
        assert_eq!(dialog.draft, document_before);
        assert!(!dialog.dirty);
        let reviewed = &dialog.recording_review.as_ref().unwrap().proposed_steps()[0];
        assert!(matches!(
            reviewed.action,
            MkAction::Delay(MkDelayPayload { fixed_ms: 99, .. })
        ));
        assert_eq!(reviewed.on_error, crate::mkmacro::MkErrorPolicy::Continue);
    }
}
fn build_recording_preview_document(
    draft: &MkMacroDocument,
    target: crate::mkmacro::RecordingTarget,
    plan: &crate::mkmacro::RecordingPlan,
) -> anyhow::Result<MkMacroDocument> {
    let mut document = draft.clone();
    let preview = document
        .macros
        .iter_mut()
        .find(|macro_item| macro_item.id == target.macro_id)
        .ok_or_else(|| anyhow::anyhow!("The recording target macro no longer exists"))?;
    preview.name = format!("{} — Recording Preview", preview.name);
    preview.enabled = true;
    preview.hotkey = None;
    // A recording proposal is an argument/output-free fragment. Keep the
    // target ID, playback settings and document dependency closure, while
    // preventing unrelated required parameters/Return contracts from
    // blocking it. The authored draft remains untouched.
    preview.signature = Default::default();
    preview.steps = crate::mkmacro::materialize_plan(plan, 0);
    Ok(document)
}

impl MkMacroDialog {
    pub(super) fn start_property_record_hotkey_capture(&mut self) {
        self.record_hotkey_capture = true;
        self.record_hotkey_capture_owner = RecordHotkeyCaptureOwner::Properties;
        self.pause_record_hotkey_capture = false;
        self.marker_record_hotkey_capture = false;
    }

    pub(super) fn start_toolbar_record_hotkey_capture(&mut self) {
        self.record_hotkey_capture = true;
        self.record_hotkey_capture_owner = RecordHotkeyCaptureOwner::Toolbar;
        self.pause_record_hotkey_capture = false;
        self.marker_record_hotkey_capture = false;
    }

    pub(super) fn cancel_property_hotkey_captures(&mut self) {
        self.hotkey_capture = false;
        self.cancel_property_record_hotkey_capture();
    }

    pub(super) fn cancel_property_record_hotkey_capture(&mut self) {
        if self.record_hotkey_capture_owner != RecordHotkeyCaptureOwner::Toolbar {
            self.cancel_record_hotkey_capture();
        }
    }

    pub(super) fn cancel_record_hotkey_capture(&mut self) {
        self.record_hotkey_capture = false;
        self.record_hotkey_capture_owner = RecordHotkeyCaptureOwner::None;
    }

    pub(crate) fn set_macro_properties_preferences(
        &mut self,
        preferences: crate::settings::MkMacroPropertiesPreferences,
    ) {
        self.mkmacro_properties_preferences = preferences;
        self.pending_mkmacro_properties_preferences = None;
    }

    pub(super) fn queue_macro_properties_preferences(&mut self) {
        self.pending_mkmacro_properties_preferences =
            Some(self.mkmacro_properties_preferences.clone());
    }

    pub(crate) fn take_macro_properties_preferences_for_persist(
        &mut self,
    ) -> Option<crate::settings::MkMacroPropertiesPreferences> {
        self.pending_mkmacro_properties_preferences.take()
    }

    /// Returns an operation client for constructing dialog-scoped visual tools.
    pub fn visual_overlay_controller(&self) -> SharedVisualOverlayController {
        self.visual_overlay.clone()
    }

    pub(crate) fn set_coordinate_capture_active(&mut self, active: bool) {
        self.action_editor.set_coordinate_capture_active(active);
    }

    /// Temporarily moves the editor out while preserving its required shared
    /// visual-overlay client in the replacement state.
    pub fn take_action_editor(&mut self) -> action_editor::ActionEditorState {
        std::mem::replace(
            &mut self.action_editor,
            action_editor::ActionEditorState::new(self.visual_overlay.clone()),
        )
    }

    pub fn new(store: Arc<MkMacroStore>) -> Self {
        Self::new_with_authoring_context(store, MkMacroAuthoringContext::default())
    }
    pub fn new_with_authoring_context(
        store: Arc<MkMacroStore>,
        authoring_context: MkMacroAuthoringContext,
    ) -> Self {
        let baseline = store.snapshot();
        let image_search_backend: Arc<dyn crate::mkmacro::VisualSearch> = {
            #[cfg(windows)]
            {
                Arc::new(crate::mkmacro::image_search::ProductionVisualSearch::new(
                    store.clone(),
                    Arc::new(crate::mkmacro::WindowsScreenCaptureBackend::system()),
                ))
            }
            #[cfg(not(windows))]
            {
                Arc::new(crate::mkmacro::screen::UnsupportedVisualSearch)
            }
        };
        // The dialog is the sole production ownership boundary for the native
        // visual-overlay worker; every authoring surface receives a clone.
        let visual_overlay = SharedVisualOverlayController::new_dialog_owner();
        let mut next_step_instance_generation = 1;
        let mut step_instance_generations = HashMap::new();
        for macro_item in &baseline.macros {
            for step in &macro_item.steps {
                step_instance_generations
                    .insert((macro_item.id, step.id), next_step_instance_generation);
                next_step_instance_generation += 1;
            }
        }
        Self {
            open: false,
            host_state: UtilityHostState::docked(),
            presentation_prepared: false,
            owner_instance: 0,
            show_root_after_host_change: false,
            pending_host_frame: None,
            retained_host_checkpoint: None,
            package_hosts: Default::default(),
            pending_package_open: None,
            search_host: Default::default(),
            pending_search_open: None,
            active_host: None,
            native_jump_mode: Default::default(),
            native_jump_frame: None,
            defer_host_effects: false,
            pending_host_effects: Vec::new(),
            pending_recording_target_publish: false,
            pending_recording_review_drain: false,
            picker_owner: None,
            host_input_allowed: false,
            host_controls_allowed: false,
            pending_host_presentation: None,
            pending_launcher_close: false,
            close_prompt_open: false,
            draft: (*baseline).clone(),
            draft_revision: 0,
            revision_document: (*baseline).clone(),
            analysis_cache: Default::default(),
            baseline,
            store,
            authoring_context,
            action_editor: action_editor::ActionEditorState::new(visual_overlay.clone()),
            image_search_backend,
            image_crop_editor: Default::default(),
            visual_overlay,
            dirty: false,
            conflict: false,
            selected_macro_id: None,
            selection: Default::default(),
            editor_state: Default::default(),
            navigation: Default::default(),
            search: String::new(),
            collapsed_folders: HashSet::new(),
            pending_folder_rename: None,
            folder_rename_text: String::new(),
            folder_rename_needs_focus: false,
            pending_delete_folder: None,
            folder_delete_confirmation: Default::default(),
            folder_error: None,
            delete_confirmation: Default::default(),
            unwrap_confirmation: Default::default(),
            pending_unwrap_block: None,
            pending_unwrap_selection: None,
            hotkey_capture: false,
            record_hotkey_capture: false,
            record_hotkey_capture_owner: RecordHotkeyCaptureOwner::None,
            pause_record_hotkey_capture: false,
            marker_record_hotkey_capture: false,
            catalog: Default::default(),
            catalog_host: Default::default(),
            pending_catalog_open: None,
            pending_editor_origin: None,
            editor_host: Default::default(),
            window_picker_host: Default::default(),
            pending_window_picker_open: None,
            launcher_picker_host: Default::default(),
            pending_launcher_picker_open: None,
            crop_host: Default::default(),
            pending_crop_open: None,
            structural_insertion: None,
            uia_editor: Default::default(),
            window_picker: Default::default(),
            launcher_action_picker: Default::default(),
            command_error: None,
            ui_notices: VecDeque::new(),
            annotation: Default::default(),
            annotation_host: Default::default(),
            pending_annotation_open: None,
            recording_review: None,
            review_host: Default::default(),
            pending_review_open: None,
            review_open_blocked: false,
            next_review_epoch: 0,
            #[cfg(test)]
            recording_preview_test_facade: None,
            #[cfg(test)]
            runtime_test: None,
            #[cfg(test)]
            next_recording_preview_test_ticket: 1,
            #[cfg(test)]
            last_main_window_rect: None,
            #[cfg(test)]
            test_host_button: None,
            #[cfg(test)]
            test_annotation_button: None,
            #[cfg(test)]
            test_review_button: None,
            #[cfg(test)]
            test_run_button: None,
            #[cfg(test)]
            test_add_action: None,
            #[cfg(test)]
            last_first_step_row_geometry: None,
            #[cfg(test)]
            last_first_step_cell_rects: None,
            #[cfg(test)]
            last_step_chrome_geometry: None,
            #[cfg(test)]
            last_main_body_geometry: None,
            #[cfg(test)]
            last_macro_list_geometry: None,
            #[cfg(test)]
            last_outline_closed_button_geometry: None,
            queued_recording_reviews: VecDeque::new(),
            recording_insert_undo: Vec::new(),
            recording_insert_redo: Vec::new(),
            recording_target_cache: Default::default(),
            step_instance_generations,
            next_step_instance_generation,
            runtime_inspector_open: false,
            runtime_inspector_show_internal: false,
            runtime_inspector_builtins_open: false,
            runtime_inspector_snapshot: None,
            runtime_inspector_is_current_debug_run: false,
            runtime_inspector_observed_run: None,
            runtime_inspector_active_breakpoint: None,
            mkmacro_properties_preferences: Default::default(),
            pending_mkmacro_properties_preferences: None,
            package_ui: Default::default(),
        }
    }
    pub fn open(&mut self) {
        if !self.open {
            self.presentation_prepared = false;
        }
        if self.open && self.is_popped_out() {
            self.request_host_focus();
            return;
        }
        self.sync_external();
        self.refresh_environment();
        self.open = true;
        self.publish_recording_target();
    }

    pub(super) fn prepare_open_presentation(&mut self, ctx: &eframe::egui::Context) {
        if self.open && !self.presentation_prepared {
            let key =
                crate::settings::UtilityToolKey::parse("mkmacro").expect("stable MkMacro key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host_state);
            self.presentation_prepared = true;
        }
    }

    pub(crate) fn is_popped_out(&self) -> bool {
        self.host_state.presentation() == UtilityPresentation::Popout
    }

    pub(crate) fn host_layer_if_root(&self) -> Option<eframe::egui::LayerId> {
        if !self.open {
            return None;
        }
        match self.host_state.presentation() {
            UtilityPresentation::Docked => {
                Some(crate::gui::jump_mode::ui::mkmacro_dialog_layer_id())
            }
            UtilityPresentation::Popout => self.host_state.embedded_root_layer(),
        }
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        (self.open && self.is_popped_out())
            || self.host_state.bootstrap_pending()
            || self.pending_host_frame.is_some()
            || self.search_host.needs_root_ui_when_hidden(self.navigation.search.mode.is_some())
            || self.package_hosts.needs_root_ui_when_hidden()
            || self.catalog_host.needs_root_ui_when_hidden(self.catalog.open)
            || self.editor_host.needs_root_ui_when_hidden(self.action_editor.draft.is_some())
            || self.window_picker_host.needs_root_ui_when_hidden(self.window_picker.open)
            || self.launcher_picker_host.needs_root_ui_when_hidden(self.launcher_action_picker.open)
            || self.crop_host.needs_root_ui_when_hidden(self.image_crop_editor.is_open())
            || self.annotation_host.needs_root_ui_when_hidden(self.annotation.open)
            || self.review_host.needs_root_ui_when_hidden(self.recording_review.is_some())
    }

    #[cfg(test)]
    pub(crate) fn catalog_is_open(&self) -> bool { self.catalog.open }
    pub(crate) fn ordinary_child_root_layer(&self) -> Option<eframe::egui::LayerId> {
        self.package_hosts.root_layer().or_else(|| self.search_root_layer())
            .or_else(|| self.catalog.open.then(|| self.catalog_host.root_layer()).flatten())
            .or_else(|| self.action_editor.draft.as_ref().and_then(|_| self.editor_host.root_layer()))
            .or_else(|| self.window_picker.open.then(|| self.window_picker_host.root_layer()).flatten())
            .or_else(|| self.launcher_action_picker.open.then(|| self.launcher_picker_host.root_layer()).flatten())
            .or_else(|| self.image_crop_editor.is_open().then(|| self.crop_host.root_layer()).flatten())
            .or_else(|| (self.annotation.open && self.pending_annotation_open.as_ref().is_none_or(|(_, was_open)| *was_open))
                .then(|| self.annotation_host.root_layer()).flatten())
            .or_else(|| (self.pending_review_open.as_ref().is_none_or(|(_, was_open)| *was_open) && !self.review_open_blocked)
                .then(|| self.recording_review.as_ref().and_then(|_| self.review_host.root_layer())).flatten())
    }

    pub(crate) fn show_annotation_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        if !self.annotation.open {
            return;
        }
        let parent_owner = root_input
            && self
                .annotation_host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        self.annotation_host.show(
            ctx,
            &mut self.annotation,
            &(),
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner: NavigationOwner::Blocked,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
    }

    pub(crate) fn show_review_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        if self.pending_review_open.is_some() || self.review_open_blocked {
            return;
        }
        let Some(ticket) = self
            .recording_review
            .as_ref()
            .map(|review| review.preview_ticket)
        else {
            return;
        };
        let (preview_active, preview_snapshot) = ticket.map_or((false, None), |ticket| {
            self.recording_preview_status(ticket)
        });
        if let Some(review) = self.recording_review.as_mut() {
            if let Some(snapshot) = preview_snapshot.as_deref() {
                review.observe_preview(snapshot);
            } else if ticket.is_some() && !preview_active {
                review.preview_state = Some(crate::mkmacro::RuntimeState::Stopped);
            }
        }
        let apply_blocker = self.recording_review_apply_blocker();
        let runtime = recording_review::ReviewRuntime {
            review_editor_open: self.action_editor.review_editing_id().is_some(),
            macros: if apply_blocker
                .as_deref()
                .is_some_and(|b| b.contains("target macro"))
            {
                self.draft
                    .macros
                    .iter()
                    .map(|m| (m.id, m.name.clone()))
                    .collect()
            } else {
                Vec::new()
            },
            apply_blocker,
            preview_active,
            playback_active: self
                .recording_preview_runtime_snapshot()
                .as_deref()
                .is_some_and(|snapshot| {
                    matches!(
                        snapshot.state,
                        crate::mkmacro::RuntimeState::Running
                            | crate::mkmacro::RuntimeState::Paused
                            | crate::mkmacro::RuntimeState::Stopping
                    )
                }),
        };
        let parent_owner = root_input
            && self
                .review_host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        if let Some(review) = self.recording_review.as_mut() {
            self.review_host.show(
                ctx,
                review,
                &runtime,
                HostInput {
                    first,
                    root_visible,
                    root_input,
                    native_input,
                    root_owner: NavigationOwner::Blocked,
                    parent_owner,
                    root_jump_active,
                    close_shortcut: !runtime.review_editor_open,
                    allow_os_close: !runtime.review_editor_open,
                    confirmation_owner: NavigationOwner::Blocked,
                    parent_instance: 0,
                    root_scopes: (None, None),
                },
            );
        }
    }

    pub(crate) fn complete_review_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        use recording_review::ReviewIntent;
        let completion = self
            .recording_review
            .as_mut()
            .and_then(|review| self.review_host.complete(ctx, review));
        let mut show_root = false;
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                match command {
                    ReviewIntent::Edit(id) => {
                        if self.action_editor.draft.is_none()
                            && let (Some(review), Some(origin)) =
                                (self.recording_review.as_ref(), completion.origin.clone())
                            && self.review_host.accepts_origin(review, &origin)
                            && let Some(step) = review.step_for_editor(id)
                        {
                            self.action_editor.begin_review_edit(
                                review.effective_target().macro_id,
                                id,
                                &step,
                            );
                            self.action_editor.bind_review_epoch(review.epoch);
                            self.pending_editor_origin =
                                Some((EditorOpenOrigin::Review(origin), false));
                        }
                    }
                    ReviewIntent::AppendRecovery => {
                        if let Some(review) = &mut self.recording_review {
                            review.recover_append_to_original_macro();
                        }
                    }
                    ReviewIntent::Retarget(id) => {
                        if self.draft.macros.iter().any(|item| item.id == id)
                            && let Some(review) = &mut self.recording_review
                        {
                            review.retarget_append(id);
                        }
                    }
                    ReviewIntent::StopPreview => self.stop_owned_recording_preview(),
                    ReviewIntent::Preview(selected) => {
                        if let Err(error) = self.preview_recording_review(selected)
                            && let Some(review) = &mut self.recording_review
                        {
                            review.message = Some(error.to_string());
                        }
                    }
                    ReviewIntent::Apply => {
                        if let Err(error) = self.apply_recording_review() {
                            if let Some(review) = &mut self.recording_review {
                                review.message = Some(error);
                            }
                        } else {
                            self.review_host.close_from_app(ctx);
                            if self.recording_review.is_some() {
                                self.pending_review_open = Some((None, false));
                            }
                        }
                    }
                    ReviewIntent::Cancel | ReviewIntent::Retired => {
                        if self.action_editor.review_editing_id().is_none() {
                            if !matches!(command, ReviewIntent::Retired) {
                                self.review_host.close_from_app(ctx);
                            }
                            self.cancel_recording_review();
                            if self.recording_review.is_some() {
                                self.pending_review_open = Some((None, false));
                            }
                        }
                    }
                }
            }
        }
        if let Some((origin, was_open)) = self.pending_review_open.take()
            && let Some(review) = self.recording_review.as_ref()
        {
            let presentation = match origin {
                Some(origin)
                    if self.open
                        && origin.is_current(self.host_state.identity(), &self.owner_instance) =>
                {
                    self.review_host.open_from_origin(
                        ctx,
                        review,
                        was_open,
                        &ChildOpenOrigin::Parent(origin),
                    )
                }
                Some(_) => {
                    if !was_open { self.review_open_blocked = true; }
                    None
                }
                None if self.open => self.review_host.open_from_origin(
                    ctx,
                    review,
                    was_open,
                    &ChildOpenOrigin::<()>::Root,
                ),
                None => {
                    self.review_open_blocked = true;
                    None
                }
            };
            show_root |= presentation == Some(UtilityPresentation::Docked);
        }
        show_root && self.recording_review.is_some() && !self.review_host.is_popped_out()
    }

    pub(crate) fn complete_annotation_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        use recording_annotation::AnnotationCommand;
        let completion = self.annotation_host.complete(ctx, &mut self.annotation);
        let mut show_root = false;
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                let commit = matches!(command, AnnotationCommand::Add);
                if !matches!(command, AnnotationCommand::Retired) {
                    self.annotation_host.close_from_app(ctx);
                }
                if let Err(error) = self.finish_recording_annotation(commit) {
                    self.command_error = Some(error.to_string());
                }
            }
        }
        if self.annotation.open
            && !crate::mkmacro::runtime::recorder_snapshot().is_some_and(|snapshot| {
                snapshot.recording_id == self.annotation.recording_id
                    && matches!(
                        snapshot.state,
                        crate::mkmacro::RecorderRuntimeState::Recording
                            | crate::mkmacro::RecorderRuntimeState::Paused
                    )
            })
        {
            self.annotation_host.close_from_app(ctx);
            let _ = self.finish_recording_annotation(false);
        }
        if let Some((origin, was_open)) = self.pending_annotation_open.take() {
            if self.open
                && origin.is_current(self.host_state.identity(), &self.owner_instance)
                && self.annotation.open
            {
                show_root |= self.annotation_host.open_from_origin(
                    ctx,
                    &self.annotation,
                    was_open,
                    &ChildOpenOrigin::Parent(origin),
                ) == Some(UtilityPresentation::Docked);
            } else if !was_open && self.annotation.open {
                let _ = self.finish_recording_annotation(false);
            }
        }
        show_root && self.annotation.open && !self.annotation_host.is_popped_out()
    }

    fn retire_review_and_annotation(&mut self, ctx: &eframe::egui::Context) {
        if self.annotation.open {
            if self
                .pending_annotation_open
                .as_ref()
                .is_none_or(|(_, was_open)| *was_open)
            {
                self.annotation_host.close_from_app(ctx);
            }
            let _ = self.finish_recording_annotation(false);
        }
        self.pending_annotation_open = None;
        if self.recording_review.is_some()
            && self.pending_review_open.as_ref().is_none_or(|(_, was_open)| *was_open)
            && !self.review_open_blocked
        {
            self.review_host.close_from_app(ctx);
        }
        self.pending_review_open = None;
        self.review_open_blocked = false;
    }

    pub(crate) fn show_window_picker_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        if !self.window_picker.open {
            return;
        }
        let parent_owner = root_input
            && self
                .window_picker_host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        self.window_picker_host.show(
            ctx,
            &mut self.window_picker,
            &(),
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner: NavigationOwner::Blocked,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
    }

    pub(crate) fn complete_window_picker_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let completion = self
            .window_picker_host
            .complete(ctx, &mut self.window_picker);
        let mut show_root = false;
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                match command {
                    window_picker::WindowPickerIntent::Refresh => self.window_picker.refresh(),
                    window_picker::WindowPickerIntent::Retired => {
                        self.window_picker.cancel("Window picking cancelled")
                    }
                    window_picker::WindowPickerIntent::Close => {
                        self.window_picker_host.close_from_app(ctx);
                        self.window_picker.cancel("Window picking cancelled");
                    }
                    window_picker::WindowPickerIntent::Confirm => {
                        if let Some((request, matcher)) = self.window_picker.take_confirmation() {
                            self.window_picker_host.close_from_app(ctx);
                            if !self.apply_window_picker_confirmation(&request, matcher) {
                                self.command_error = Some(
                                    "The matcher target no longer exists; no changes were made"
                                        .into(),
                                );
                            }
                        }
                    }
                }
            }
        }
        if self.window_picker.open {
            window_picker::prepare(&mut self.window_picker, ctx);
            if !self.window_picker.open { self.window_picker_host.close_from_app(ctx); }
        }
        if let Some((request, origin)) = self.pending_window_picker_open.take() {
            let current = self.open
                && !self.close_prompt_open
                && self.window_picker_request_is_current(&request)
                && match &origin {
                    PickerOpenOrigin::Main(origin) => {
                        origin.is_current(self.host_state.identity(), &self.owner_instance)
                    }
                    PickerOpenOrigin::Editor(origin) => {
                        self.editor_host.accepts_origin(&self.action_editor, origin)
                    }
                };
            if current {
                let was_open = self.window_picker.open;
                if !was_open {
                    self.window_picker.open(request.clone());
                }
                if self.window_picker.request.as_ref() == Some(&request) {
                    let origin = match origin {
                        PickerOpenOrigin::Main(origin) | PickerOpenOrigin::Editor(origin) => {
                            ChildOpenOrigin::Parent(origin)
                        }
                    };
                    show_root |= self.window_picker_host.open_from_origin(
                        ctx,
                        &self.window_picker,
                        was_open,
                        &origin,
                    ) == Some(UtilityPresentation::Docked);
                } else {
                    self.window_picker.message =
                        Some("Finish the current window selection first.".into());
                }
            }
        }
        show_root && self.window_picker.open && !self.window_picker_host.is_popped_out()
    }

    pub(crate) fn show_launcher_picker_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        if !self.launcher_action_picker.open {
            return;
        }
        let parent_owner = root_input
            && self
                .launcher_picker_host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        self.launcher_picker_host.show(
            ctx,
            &mut self.launcher_action_picker,
            &self.authoring_context.launcher_actions,
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner: NavigationOwner::Blocked,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
    }

    pub(crate) fn complete_launcher_picker_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let completion = self
            .launcher_picker_host
            .complete(ctx, &mut self.launcher_action_picker);
        let mut show_root = false;
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                match command {
                    launcher_action_picker::LauncherPickerIntent::Retired => {
                        self.launcher_action_picker.cancel()
                    }
                    launcher_action_picker::LauncherPickerIntent::Close => {
                        self.launcher_picker_host.close_from_app(ctx);
                        self.launcher_action_picker.cancel();
                    }
                    launcher_action_picker::LauncherPickerIntent::Choose { action, convert } => {
                        let applied =
                            self.launcher_action_picker
                                .request
                                .as_ref()
                                .is_some_and(|request| {
                                    self.authoring_context.launcher_actions.contains(&action)
                                        && self.action_editor.apply_launcher_picker_action(
                                            request,
                                            &action,
                                            self.selected_macro_id,
                                            convert,
                                        )
                                });
                        if applied {
                            self.launcher_picker_host.close_from_app(ctx);
                            self.launcher_action_picker.cancel();
                        } else {
                            self.launcher_action_picker.error =
                                Some("The action or editor changed; choose again.".into());
                        }
                    }
                }
            }
        }
        if let Some((request, origin)) = self.pending_launcher_picker_open.take()
            && self.open
            && !self.close_prompt_open
            && self
                .editor_host
                .accepts_origin(&self.action_editor, &origin)
            && self
                .action_editor
                .launcher_request_is_current(&request, self.selected_macro_id)
        {
            let was_open = self.launcher_action_picker.open;
            if !was_open {
                self.launcher_action_picker.open(request.clone());
            }
            if self.launcher_action_picker.request.as_ref() == Some(&request) {
                show_root |= self.launcher_picker_host.open_from_origin(
                    ctx,
                    &self.launcher_action_picker,
                    was_open,
                    &ChildOpenOrigin::Parent(origin),
                ) == Some(UtilityPresentation::Docked);
            } else {
                self.launcher_action_picker.error =
                    Some("Finish the current launcher selection first.".into());
            }
        }
        show_root && self.launcher_action_picker.open && !self.launcher_picker_host.is_popped_out()
    }

    pub(crate) fn show_crop_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::{HostInput, HostedUtilityDialog};
        if !self.image_crop_editor.is_open() {
            return;
        }
        self.image_crop_editor.poll_load(ctx);
        let parent_owner = root_input
            && self
                .crop_host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        let confirmation_owner = self
            .image_crop_editor
            .confirmation_binding(eframe::egui::ViewportId::ROOT, self.image_crop_editor.epoch)
            .filter(|binding| {
                root_input
                    && binding.base_is_current(ctx)
                    && binding.focus_is_owned(ctx, None)
                    && !eframe::egui::Popup::is_any_open(ctx)
            })
            .map_or(NavigationOwner::Blocked, |binding| binding.owner);
        self.crop_host.show(
            ctx,
            &mut self.image_crop_editor,
            &(),
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
    }

    pub(crate) fn complete_crop_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let completion = self.crop_host.complete(ctx, &mut self.image_crop_editor);
        let mut show_root = false;
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                let retired = matches!(command, image_crop_editor::CropUiAction::Retired);
                let cancel = retired || matches!(command, image_crop_editor::CropUiAction::Cancel);
                if !cancel
                    && (!self.open
                        || self.close_prompt_open
                        || !self.action_editor.crop_destination_is_current(
                            self.image_crop_editor.destination(),
                            self.selected_macro_id,
                        ))
                {
                    self.action_editor.capture_message =
                        Some("The crop target changed; no image was saved.".into());
                    continue;
                }
                let mut crop = std::mem::take(&mut self.image_crop_editor);
                image_crop_editor::apply_action(ctx, self, &mut crop, command);
                self.image_crop_editor = crop;
                if !self.image_crop_editor.is_open() && !retired {
                    self.crop_host.close_from_app(ctx);
                }
            }
        }
        if let Some((destination, origin)) = self.pending_crop_open.take()
            && self.open
            && !self.close_prompt_open
            && self
                .editor_host
                .accepts_origin(&self.action_editor, &origin)
            && self
                .action_editor
                .crop_destination_is_current(&destination, self.selected_macro_id)
        {
            let was_open = self.image_crop_editor.is_open();
            if !was_open
                && let Err(error) = self
                    .image_crop_editor
                    .open(self.store.clone(), destination.clone())
            {
                self.action_editor.capture_message = Some(error.into());
            }
            if self.image_crop_editor.is_open()
                && self.image_crop_editor.destination() == &destination
            {
                show_root |= self.crop_host.open_from_origin(
                    ctx,
                    &self.image_crop_editor,
                    was_open,
                    &ChildOpenOrigin::Parent(origin),
                ) == Some(UtilityPresentation::Docked);
            }
        }
        show_root && self.image_crop_editor.is_open() && !self.crop_host.is_popped_out()
    }

    pub(crate) fn show_catalog_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        let mut host = std::mem::take(&mut self.catalog_host);
        let mut catalog = std::mem::take(&mut self.catalog);
        let parent_owner = root_input
            && host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        host.show(
            ctx,
            &mut catalog,
            self,
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner: NavigationOwner::Blocked,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
        self.catalog = catalog;
        self.catalog_host = host;
    }

    pub(crate) fn complete_catalog_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let mut host = std::mem::take(&mut self.catalog_host);
        let mut catalog = std::mem::take(&mut self.catalog);
        let mut show_root = false;
        if let Some(completion) = host.complete(ctx, &mut catalog) {
            show_root = completion.show_root;
            for command in completion.commands {
                match command {
                    action_catalog::CatalogCommand::Retired => catalog.open = false,
                    action_catalog::CatalogCommand::Close => {
                        host.close_from_app(ctx);
                        catalog.open = false;
                    }
                    action_catalog::CatalogCommand::Select(name) => {
                        let current = self.open
                            && !self.close_prompt_open
                            && catalog.owner == self.owner_instance
                            && catalog.revision == self.draft_revision
                            && catalog.macro_id == self.selected_macro_id;
                        if !current {
                            catalog.error =
                                Some("The macro or draft changed. Reopen Add Action.".into());
                            continue;
                        }
                        let Some(origin) = completion.origin.clone() else {
                            continue;
                        };
                        let Some(descriptor) = action_catalog::descriptors()
                            .into_iter()
                            .find(|entry| entry.name == name)
                        else {
                            continue;
                        };
                        if action_catalog::select_descriptor(self, &descriptor) {
                            if self.action_editor.draft.is_some() {
                                self.pending_editor_origin =
                                    Some((EditorOpenOrigin::Catalog(origin), false));
                            } else {
                                catalog.revision = self.draft_revision;
                            }
                        }
                    }
                }
            }
        }
        self.catalog = catalog;
        if let Some((revision, macro_id, origin)) = self.pending_catalog_open.take()
            && self.open
            && !self.close_prompt_open
            && revision == self.draft_revision
            && macro_id == self.selected_macro_id
            && origin.is_current(self.host_state.identity(), &self.owner_instance)
        {
            let was_open = self.catalog.open;
            if !was_open {
                self.catalog.epoch = self.catalog.epoch.wrapping_add(1).max(1);
                self.catalog.open = true;
                self.catalog.owner = self.owner_instance;
                self.catalog.revision = self.draft_revision;
                self.catalog.macro_id = self.selected_macro_id;
                self.catalog.error = None;
            }
            show_root |= host.open_from_origin(
                ctx,
                &self.catalog,
                was_open,
                &ChildOpenOrigin::Parent(origin),
            ) == Some(UtilityPresentation::Docked);
        }
        show_root &= self.catalog.open && !host.is_popped_out();
        self.catalog_host = host;
        show_root
    }

    pub(crate) fn show_editor_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        if self.action_editor.draft.is_none() {
            return;
        }
        action_editor::prepare_editor(ctx, self);
        let mut host = std::mem::take(&mut self.editor_host);
        let mut editor = self.take_action_editor();
        let parent_owner = root_input
            && host
                .root_layer()
                .is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        let confirmation_owner = <action_editor::ActionEditorState as super::utility_window_host::HostedUtilityDialog>::confirmation_binding(&editor, eframe::egui::ViewportId::ROOT, editor.host_epoch)
            .filter(|binding| root_input && binding.base_is_current(ctx) && binding.focus_is_owned(ctx, None) && !eframe::egui::Popup::is_any_open(ctx))
            .map_or(NavigationOwner::Blocked, |binding| binding.owner);
        host.show(
            ctx,
            &mut editor,
            self,
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: NavigationOwner::Blocked,
                parent_owner,
                root_jump_active,
                close_shortcut: true,
                allow_os_close: true,
                confirmation_owner,
                parent_instance: 0,
                root_scopes: (None, None),
            },
        );
        self.action_editor = editor;
        self.editor_host = host;
    }

    pub(crate) fn complete_editor_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let mut host = std::mem::take(&mut self.editor_host);
        let mut editor = self.take_action_editor();
        let completion = host.complete(ctx, &mut editor);
        self.action_editor = editor;
        self.editor_host = host;
        if self.action_editor.review_epoch().is_some_and(|epoch| self.recording_review.as_ref().is_none_or(|review| review.epoch != epoch)) {
            self.editor_host.close_from_app(ctx);
            self.action_editor.cancel();
            return false;
        }
        let window_was_open = self.window_picker.open;
        let launcher_was_open = self.launcher_action_picker.open;
        let crop_was_open = self.image_crop_editor.is_open();
        let mut show_root = false;
        let mut retired = false;
        let had_completion = completion.is_some();
        if let Some(completion) = completion {
            show_root = completion.show_root;
            for command in completion.commands {
                match command {
                    action_editor::EditorCommand::Retired => {
                        retired = true;
                        action_editor::complete_finish(
                            self,
                            action_editor::ActionEditorFinish::Cancel,
                        );
                    }
                    action_editor::EditorCommand::Effects {
                        effects,
                        hwnd,
                        epoch,
                        generation,
                        owner,
                        owner_instance,
                    } => {
                        if self.open
                            && !self.close_prompt_open
                            && self.owner_instance == owner_instance
                            && self.action_editor.host_epoch == epoch
                            && self.action_editor.draft_generation == generation
                            && self.action_editor.owner_macro_id() == Some(owner)
                            && self.action_editor.review_epoch().is_none_or(|epoch| self.recording_review.as_ref().is_some_and(|review| review.epoch == epoch))
                            && let Some(origin) = completion.origin.clone()
                        {
                            let before_revision = self.draft_revision;
                            action_editor::complete_editor_actions(
                                ctx,
                                self,
                                effects,
                                hwnd,
                                Some(origin),
                            );
                            if self.catalog.open
                                && self.catalog.owner == self.owner_instance
                                && self.catalog.macro_id == self.selected_macro_id
                                && self.catalog.revision == before_revision
                            {
                                self.catalog.revision = self.draft_revision;
                            }
                        }
                    }
                }
            }
        }
        if window_was_open && !self.window_picker.open {
            self.window_picker_host.close_from_app(ctx);
        }
        if launcher_was_open && !self.launcher_action_picker.open {
            self.launcher_picker_host.close_from_app(ctx);
        }
        if crop_was_open && !self.image_crop_editor.is_open() {
            self.crop_host.close_from_app(ctx);
        }
        let mut host = std::mem::take(&mut self.editor_host);
        if had_completion && !retired && self.action_editor.draft.is_none() {
            host.close_from_app(ctx);
        }
        if let Some((origin, was_open)) = self.pending_editor_origin.take() {
            let current = self.open
                && !self.close_prompt_open
                && self.action_editor.draft.is_some()
                && match &origin {
                    EditorOpenOrigin::Main(origin) => {
                        origin.is_current(self.host_state.identity(), &self.owner_instance)
                    }
                    EditorOpenOrigin::Catalog(origin) => {
                        self.catalog_host.accepts_origin(&self.catalog, origin)
                    }
                    EditorOpenOrigin::Review(origin) => self.recording_review.as_ref().is_some_and(|review| self.review_host.accepts_origin(review, origin)),
                };
            if current {
                show_root |= match origin {
                    EditorOpenOrigin::Main(origin) | EditorOpenOrigin::Catalog(origin) => host.open_from_origin(ctx, &self.action_editor, was_open, &ChildOpenOrigin::Parent(origin)),
                    EditorOpenOrigin::Review(origin) => host.open_from_origin(ctx, &self.action_editor, was_open, &ChildOpenOrigin::Parent(origin)),
                } == Some(UtilityPresentation::Docked);
            } else if !was_open {
                self.action_editor.cancel();
            }
        }
        show_root &= self.action_editor.draft.is_some() && !host.is_popped_out();
        self.editor_host = host;
        show_root
    }

    pub(crate) fn show_package_children(&mut self, ctx: &eframe::egui::Context, first: bool, root_visible: bool, root_input: bool, native_input: bool, root_jump_active: bool) {
        let mut hosts = std::mem::take(&mut self.package_hosts);
        hosts.show(ctx, self, first, root_visible, root_input, native_input, root_jump_active);
        self.package_hosts = hosts;
    }

    pub(crate) fn complete_package_children(&mut self, ctx: &eframe::egui::Context) -> bool {
        let mut hosts = std::mem::take(&mut self.package_hosts);
        let mut show_root = hosts.complete(ctx, self);
        if let Some((request, revision, macro_id, origin)) = self.pending_package_open.take()
            && self.open && !self.close_prompt_open
            && revision == self.draft_revision && macro_id == self.selected_macro_id
            && origin.is_current(self.host_state.identity(), &self.owner_instance)
        {
            show_root |= hosts.open(ctx, self, request, origin);
        }
        show_root &= hosts.has_root_layer();
        self.package_hosts = hosts;
        show_root
    }

    pub(crate) fn search_root_layer(&self) -> Option<eframe::egui::LayerId> {
        self.navigation.search.mode.and_then(|_| self.search_host.root_layer())
    }

    #[cfg(test)]
    pub(crate) fn test_open_native_search(&mut self, ctx: &eframe::egui::Context) {
        search::open_accepted(self, search::SearchMode::Find);
        let manager = super::utility_presentation::UtilityPresentationManager::for_context(ctx).unwrap();
        manager.remember(ctx, crate::settings::UtilityToolKey::parse("mkmacro-step-search").unwrap(), crate::settings::RememberedPresentation::Popout);
        self.search_host.open_from_origin(ctx, &self.navigation.search, false, &super::utility_host::ChildOpenOrigin::<()>::Root);
    }

    #[cfg(test)]
    pub(crate) fn test_open_native_review(&mut self, ctx: &eframe::egui::Context) {
        let result = crate::mkmacro::RecordingResult {
            target: crate::mkmacro::RecordingTarget { macro_id: 1, insertion_anchor_step_id: None, insertion_anchor_generation: None },
            literal_steps: Vec::new(), plan: crate::mkmacro::RecordingPlan { steps: Vec::new() },
            suggestions: Vec::new(), clipboard_observations: Vec::new(), click_inspections: Vec::new(),
            window_observations: Vec::new(), notes: Vec::new(), capture_duration: std::time::Duration::ZERO,
            raw_event_count: 0, dropped_event_count: 0,
        };
        self.open_recording_review(result).unwrap();
        self.pending_review_open = None;
        let manager = super::utility_presentation::UtilityPresentationManager::for_context(ctx).unwrap();
        manager.remember(ctx, crate::settings::UtilityToolKey::parse("mkmacro-recording-review").unwrap(), crate::settings::RememberedPresentation::Popout);
        self.review_host.open_from_origin(ctx, self.recording_review.as_ref().unwrap(), false, &super::utility_host::ChildOpenOrigin::<()>::Root);
    }

    #[cfg(test)]
    pub(crate) fn test_open_native_editor(&mut self, ctx: &eframe::egui::Context) {
        self.action_editor.begin_new(crate::mkmacro::MkAction::Delay(Default::default()));
        let manager = super::utility_presentation::UtilityPresentationManager::for_context(ctx).unwrap();
        manager.remember(ctx, crate::settings::UtilityToolKey::parse("mkmacro-action-editor").unwrap(), crate::settings::RememberedPresentation::Popout);
        self.editor_host.open_from_origin(ctx, &self.action_editor, false, &super::utility_host::ChildOpenOrigin::<()>::Root);
    }

    pub(crate) fn show_search_child(
        &mut self,
        ctx: &eframe::egui::Context,
        first: bool,
        root_visible: bool,
        root_input: bool,
        native_input: bool,
        root_jump_active: bool,
    ) {
        use super::jump_mode::ui::{NavigationOwner, visible_top_middle_layer};
        use super::utility_window_host::HostInput;
        let mut host = std::mem::take(&mut self.search_host);
        let mut state = std::mem::take(&mut self.navigation.search);
        let parent_owner = root_input && host.root_layer().is_some_and(|layer| visible_top_middle_layer(ctx) == Some(layer))
            && !eframe::egui::Popup::is_any_open(ctx);
        host.show(ctx, &mut state, self, HostInput {
            first, root_visible, root_input, native_input,
            root_owner: NavigationOwner::Blocked, parent_owner, root_jump_active,
            close_shortcut: true, allow_os_close: true,
            confirmation_owner: NavigationOwner::Blocked, parent_instance: 0, root_scopes: (None, None),
        });
        self.navigation.search = state;
        self.search_host = host;
    }

    pub(crate) fn complete_search_child(&mut self, ctx: &eframe::egui::Context) -> bool {
        use super::utility_host::{ChildOpenOrigin, UtilityPresentation};
        let mut host = std::mem::take(&mut self.search_host);
        let mut state = std::mem::take(&mut self.navigation.search);
        let mut show_root = false;
        if let Some(completion) = host.complete(ctx, &mut state) {
            show_root = completion.show_root;
            for actions in completion.commands {
                let (closed, retired) = search::apply_host_actions(self, &mut state, actions);
                if closed && !retired { host.close_from_app(ctx); }
            }
        }
        self.navigation.search = state;
        if let Some((mode, revision, macro_id, origin)) = self.pending_search_open.take()
            && self.open && !self.close_prompt_open
            && self.draft_revision == revision && self.selected_macro_id == macro_id
            && origin.is_current(self.host_state.identity(), &self.owner_instance)
        {
            let was_open = self.navigation.search.mode.is_some();
            if self.navigation.search.mode != Some(mode) { search::open_accepted(self, mode); }
            let desired = host.open_from_origin(ctx, &self.navigation.search, was_open, &ChildOpenOrigin::Parent(origin));
            show_root |= desired == Some(UtilityPresentation::Docked);
        }
        show_root &= self.navigation.search.mode.is_some() && !host.is_popped_out();
        self.search_host = host;
        show_root
    }

    pub(crate) fn owns_root_input(&self, ctx: &eframe::egui::Context) -> bool {
        self.open
            && self.host_layer_if_root().is_some_and(|layer| {
                mkmacro_surface_owns_input(
                    ctx,
                    eframe::egui::ViewportId::ROOT,
                    layer,
                    self.owner_instance,
                )
            })
    }

    pub(crate) fn request_close(&mut self) {
        if self.open {
            self.pending_launcher_close = true;
        }
    }

    pub(crate) fn request_host_focus(&mut self) {
        self.host_state.request_focus();
    }

    #[cfg(test)]
    pub(crate) fn test_pop_out(&mut self) {
        self.open = true;
        self.host_state.transition(UtilityPresentation::Popout);
    }

    #[cfg(test)]
    pub(crate) fn test_viewport_id(&self) -> eframe::egui::ViewportId {
        mkmacro_viewport_id(self.owner_instance)
    }

    pub(super) fn host_child_input_allowed(&self) -> bool {
        !self.defer_host_effects || self.host_input_allowed
    }

    pub(crate) fn force_close(&mut self, ctx: &eframe::egui::Context) {
        self.retire_review_and_annotation(ctx);
        if self.open {
            let key = crate::settings::UtilityToolKey::parse("mkmacro").expect("stable MkMacro key");
            super::utility_host::remember_presentation(ctx, key, self.host_state.presentation());
        }
        if self.navigation.search.mode.is_some() { self.search_host.close_from_app(ctx); }
        self.package_hosts.close_all(ctx);
        if self.catalog.open {
            self.catalog_host.close_from_app(ctx);
            self.catalog.open = false;
        }
        if self.action_editor.draft.is_some()
            && self.pending_editor_origin.as_ref().is_none_or(|(_, was_open)| *was_open)
        {
            self.editor_host.close_from_app(ctx);
        }
        if self.window_picker.open {
            self.window_picker_host.close_from_app(ctx);
            self.window_picker.cancel("Window picking cancelled");
        }
        if self.launcher_action_picker.open {
            self.launcher_picker_host.close_from_app(ctx);
            self.launcher_action_picker.cancel();
        }
        if self.image_crop_editor.is_open() {
            self.crop_host.close_from_app(ctx);
            self.image_crop_editor.cancel();
        }
        self.pending_crop_open = None;
        self.pending_launcher_picker_open = None;
        self.pending_window_picker_open = None;
        self.pending_catalog_open = None;
        self.pending_editor_origin = None;
        self.pending_search_open = None;
        self.pending_package_open = None;
        self.presentation_prepared = false;
        let identity = mkmacro_native_identity(self.host_state.identity(), self.owner_instance);
        self.defer_host_effects = false;
        self.open = false;
        self.close_children();
        if let Some(identity) = identity {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, eframe::egui::ViewportCommand::Close);
        }
        self.host_state.retire_to_docked();
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.pending_host_frame = None;
        self.pending_host_effects.clear();
        self.pending_launcher_close = false;
        self.pending_recording_target_publish = false;
        self.pending_recording_review_drain = false;
        self.retained_host_checkpoint = None;
        ctx.request_repaint_of(eframe::egui::ViewportId::ROOT);
    }

    pub(crate) fn take_show_root_after_host_change(&mut self) -> bool {
        std::mem::take(&mut self.show_root_after_host_change)
    }

    pub(crate) fn show_with_host(
        &mut self,
        ctx: &eframe::egui::Context,
        root_jump_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
        first_ui_pass: bool,
        application_input_allowed: bool,
        root_visible: bool,
        root_input_allowed: bool,
        close_shortcut_allowed: bool,
        owner_instance: u64,
        jump_mode_enabled: bool,
    ) {
        self.prepare_open_presentation(ctx);
        use eframe::egui;

        self.owner_instance = owner_instance;
        self.defer_host_effects = true;
        self.host_input_allowed = false;
        self.pending_host_effects.clear();
        self.pending_host_frame = None;
        let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let retained_checkpoint = self
            .retained_host_checkpoint
            .as_ref()
            .filter(|(frame, _, _)| *frame == root_pass.frame_nr)
            .map(|(frame, first_pass, checkpoint)| (*frame, *first_pass, checkpoint.clone()));
        let mut draft_checkpoint = retained_checkpoint
            .as_ref()
            .map(|(_, _, checkpoint)| checkpoint.clone());
        if let Some((_, first_pass, checkpoint)) = retained_checkpoint
            && root_pass.pass_nr > first_pass
        {
            checkpoint.restore(self);
        }
        if self
            .retained_host_checkpoint
            .as_ref()
            .is_some_and(|(frame, _, _)| *frame != root_pass.frame_nr)
        {
            self.retained_host_checkpoint = None;
        }
        let host = self.host_state.identity();
        let mut native_jump_mode = std::mem::take(&mut self.native_jump_mode);
        let mut native_jump_frame = self.native_jump_frame.take();
        let mut callback_observed = false;
        let mut callback_class = egui::ViewportClass::Root;
        let mut owner_context = ctx.clone();
        let mut owner_viewport = egui::ViewportId::ROOT;
        let mut owner_pass = None;
        let mut owner_pass_accepted = false;
        let mut native_viewport = None;
        let mut embedded_root_layer = None;
        let mut embedded_root_focused_id = None;
        let mut owner_modal_layer = None;
        let mut close_requested = self.pending_launcher_close;
        let mut os_close_requested = false;
        let mut focus_host = false;

        match host.host {
            UtilityPresentation::Docked => {
                if self.open && (root_visible || self.host_state.bootstrap_pending()) {
                    let mut window_open = true;
                    let layer = crate::gui::jump_mode::ui::mkmacro_dialog_layer_id();
                    let output = self.show_main_window_host(
                        ctx,
                        &mut window_open,
                        root_jump_scope.as_ref(),
                        first_ui_pass,
                        root_input_allowed,
                        close_shortcut_allowed,
                        &mut callback_observed,
                        &mut callback_class,
                        &mut owner_pass,
                        &mut owner_pass_accepted,
                        &mut owner_modal_layer,
                        &mut draft_checkpoint,
                        &mut close_requested,
                    );
                    if output.is_some() && !window_open && owner_pass_accepted {
                        close_requested = true;
                    }
                    if callback_observed {
                        embedded_root_layer = Some(layer);
                        embedded_root_focused_id = ctx.memory(|memory| memory.focused());
                    }
                }
            }
            UtilityPresentation::Popout => {
                let viewport_id = mkmacro_viewport_id(owner_instance);
                let viewport_builder = mkmacro_viewport_builder(owner_instance);
                let focused_pending = self.host_state.focus_pending();
                let nested_owner_open = self.jump_mode_unsupported_nested_owner_open();
                let jump_generation = self.jump_mode_target_generation();
                native_jump_mode.set_enabled(jump_mode_enabled);
                ctx.show_viewport_immediate(viewport_id, viewport_builder, |child, class| {
                    callback_observed = true;
                    callback_class = class;
                    let child_ctx = child.ctx().clone();
                    owner_context = child_ctx.clone();
                    let independent = class == egui::ViewportClass::Immediate
                        && child_ctx.viewport_id() == viewport_id;
                    owner_viewport = if independent {
                        viewport_id
                    } else {
                        egui::ViewportId::ROOT
                    };
                    let pass = ViewportPassProvenance::current(&child_ctx, child_ctx.viewport_id());
                    owner_pass = Some(pass);
                    let pass_authorized = first_ui_pass
                        && child.current_pass_index() == 0
                        && !ctx.will_discard()
                        && !child_ctx.will_discard();
                    owner_pass_accepted = pass_authorized;
                    if independent {
                        native_viewport = Some(child.input(|input| input.viewport().clone()));
                    }
                    let visible_body =
                        independent || (child.is_visible() && !child.is_sizing_pass());
                    let modal_open = child_ctx.memory(|memory| memory.top_modal_layer().is_some());
                    let popup_open = egui::Popup::is_any_open(&child_ctx);
                    let expected_layer = if independent {
                        egui::LayerId::background()
                    } else {
                        child.layer_id()
                    };
                    let owner_current = if independent {
                        application_input_allowed
                            && mkmacro_surface_owns_input(
                                &child_ctx,
                                viewport_id,
                                expected_layer,
                                owner_instance,
                            )
                    } else {
                        root_input_allowed
                            && mkmacro_surface_owns_input(
                                &child_ctx,
                                egui::ViewportId::ROOT,
                                expected_layer,
                                owner_instance,
                            )
                    };
                    if !independent && !owner_current {
                        surrender_mkmacro_focus(&child_ctx, expected_layer);
                    }

                    let binding = independent.then_some(
                        crate::gui::jump_mode::ui::NavigationViewportBinding {
                            viewport: viewport_id,
                            owner: crate::gui::jump_mode::ui::NavigationOwner::MkMacro {
                                instance: owner_instance,
                            },
                            base_layer: egui::LayerId::background(),
                        },
                    );
                    if independent {
                        let frame_number = child_ctx.cumulative_frame_nr();
                        if native_jump_frame != Some(frame_number) {
                            native_jump_mode.begin_frame();
                            native_jump_frame = Some(frame_number);
                        }
                    }
                    let navigation_owner = binding.map_or(
                        crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                        |binding| {
                            native_jump_mode.update_navigation_owner_for_viewport(
                                &child_ctx,
                                binding,
                                application_input_allowed && jump_mode_enabled,
                            )
                        },
                    );
                    let jump_owner_current = binding.is_some_and(|binding| {
                        navigation_owner == binding.owner
                            && native_jump_mode.frame_navigation_owner() == binding.owner
                    });
                    if independent {
                        native_jump_mode.handle_viewport_input(
                            &child_ctx,
                            jump_owner_current,
                            pass_authorized,
                            viewport_id,
                        );
                    }

                    let keyboard_owner = close_shortcut_allowed
                        && owner_current
                        && (independent || root_input_allowed)
                        && !nested_owner_open
                        && native_jump_mode.active_session().is_none();
                    let keyboard_close = pass_authorized
                        && keyboard_owner
                        && child_ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    if keyboard_close {
                        close_requested = true;
                    }
                    os_close_requested = independent
                        && pass_authorized
                        && child.input(|input| input.viewport().close_requested());
                    focus_host = focused_pending && pass_authorized && visible_body && !modal_open;
                    owner_modal_layer = child_ctx.memory(|memory| memory.top_modal_layer());

                    let surface_input_allowed = pass_authorized && owner_current && visible_body;
                    let body_allowed = surface_input_allowed
                        && !modal_open
                        && !popup_open
                        && !close_requested
                        && !os_close_requested;
                    let host_controls_allowed = body_allowed
                        && !modal_open
                        && !popup_open
                        && self.action_editor.draft.is_none()
                        && !step_table::table_modal_open(self);
                    if surface_input_allowed && owner_current && has_draft_input_event(&child_ctx) {
                        draft_checkpoint = Some(MkMacroDraftCheckpoint::capture(self));
                    }

                    if independent {
                        native_jump_mode.begin_ui_pass();
                        let native_scope = if jump_owner_current {
                            binding.and_then(|binding| {
                                native_jump_mode.scope_for_viewport_binding(
                                    crate::gui::jump_mode::SurfaceId::dialog(
                                        "mkmacro",
                                        owner_instance,
                                    ),
                                    jump_generation,
                                    "mkmacro",
                                    binding,
                                )
                            })
                        } else {
                            None
                        };
                        egui::CentralPanel::default().show(child, |ui| {
                            self.render_host_body(
                                ui,
                                &child_ctx,
                                native_scope.as_ref(),
                                body_allowed,
                                host_controls_allowed,
                                surface_input_allowed,
                            );
                        });
                        native_jump_mode.finish_ui_pass();
                        let after_owner = binding.map_or(
                            crate::gui::jump_mode::ui::NavigationOwner::Blocked,
                            |binding| {
                                native_jump_mode.update_navigation_owner_for_viewport(
                                    &child_ctx,
                                    binding,
                                    application_input_allowed && jump_mode_enabled,
                                )
                            },
                        );
                        if let Some(binding) = binding {
                            if native_jump_mode.prepare_pass_for_viewport(
                                &child_ctx,
                                binding,
                                after_owner,
                                application_input_allowed,
                            ) {
                                crate::gui::jump_mode::paint::paint(&child_ctx, &native_jump_mode);
                            }
                            if !child_ctx.will_discard() {
                                native_jump_mode.complete_frame_for_viewport(
                                    &child_ctx,
                                    binding,
                                    after_owner,
                                    application_input_allowed,
                                );
                            }
                        }
                    } else {
                        let root_scope = root_jump_scope.as_ref();
                        self.render_host_body(
                            child,
                            &child_ctx,
                            root_scope,
                            body_allowed,
                            host_controls_allowed,
                            surface_input_allowed,
                        );
                        if child_ctx.viewport_id() == egui::ViewportId::ROOT {
                            embedded_root_layer = Some(child.layer_id());
                            embedded_root_focused_id = child_ctx.memory(|memory| memory.focused());
                        }
                    }
                });
            }
        }

        if callback_observed {
            if self.retained_host_checkpoint.is_none()
                && let Some(checkpoint) = draft_checkpoint.as_ref()
            {
                self.retained_host_checkpoint =
                    Some((root_pass.frame_nr, root_pass.pass_nr, checkpoint.clone()));
            }
            let effects = MkMacroHostEffects {
                host_request: self.pending_host_presentation.take(),
                close_requested,
                focus_host,
                actions: std::mem::take(&mut self.pending_host_effects),
            };
            self.pending_host_frame = Some(MkMacroHostFrame {
                root_pass,
                host,
                owner_instance,
                owner_context,
                owner_viewport,
                owner_pass,
                owner_pass_accepted,
                callback_observed,
                callback_class,
                native_viewport,
                embedded_root_layer,
                embedded_root_focused_id,
                owner_modal_layer,
                os_close_requested,
                body_close_requested: false,
                draft_checkpoint: draft_checkpoint.clone(),
                effects,
                native_jump_mode,
                native_jump_frame,
            });
        } else {
            self.native_jump_mode = native_jump_mode;
            self.native_jump_frame = native_jump_frame;
        }
        self.host_input_allowed = false;
        self.host_controls_allowed = false;
    }

    pub(crate) fn on_host_frame_complete(
        &mut self,
        ctx: &eframe::egui::Context,
    ) -> MkMacroHostCompletion {
        use eframe::egui;

        let Some(mut frame) = self.pending_host_frame.take() else {
            self.defer_host_effects = false;
            self.host_input_allowed = false;
            self.host_controls_allowed = false;
            return MkMacroHostCompletion {
                closed: false,
                show_root: self.take_show_root_after_host_change(),
                persisted_preferences: None,
                notices: Vec::new(),
            };
        };
        let owner_pass_retained = frame.callback_observed
            && frame.owner_pass_accepted
            && frame.owner_pass.is_some_and(|pass| {
                pass.viewport_id == frame.owner_viewport && pass.was_retained(ctx)
            });
        if !frame.root_pass.was_retained(ctx)
            || frame.host != self.host_state.identity()
            || frame.owner_instance != self.owner_instance
            || !owner_pass_retained
        {
            if let Some(checkpoint) = frame.draft_checkpoint.take() {
                checkpoint.restore(self);
            }
            if frame.os_close_requested {
                ctx.send_viewport_cmd_to(
                    mkmacro_viewport_id(frame.owner_instance),
                    egui::ViewportCommand::CancelClose,
                );
            }
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            self.defer_host_effects = false;
            self.host_input_allowed = false;
            self.host_controls_allowed = false;
            return MkMacroHostCompletion {
                closed: false,
                show_root: false,
                persisted_preferences: None,
                notices: Vec::new(),
            };
        }

        if self
            .retained_host_checkpoint
            .as_ref()
            .is_some_and(|(root_frame, _, _)| *root_frame == frame.root_pass.frame_nr)
        {
            self.retained_host_checkpoint = None;
        }
        self.pending_launcher_close = false;

        if frame.os_close_requested {
            ctx.send_viewport_cmd_to(
                mkmacro_viewport_id(frame.owner_instance),
                egui::ViewportCommand::CancelClose,
            );
        }
        self.host_state.complete_bootstrap(
            frame.host,
            frame.embedded_root_layer,
            frame.embedded_root_focused_id,
        );
        self.native_jump_mode = std::mem::take(&mut frame.native_jump_mode);
        self.native_jump_frame = frame.native_jump_frame;
        if frame.effects.focus_host {
            if frame.callback_class == egui::ViewportClass::Immediate {
                ctx.send_viewport_cmd_to(
                    mkmacro_viewport_id(frame.owner_instance),
                    egui::ViewportCommand::Focus,
                );
            } else if let Some(layer) = frame.embedded_root_layer {
                ctx.move_to_top(layer);
            }
            self.host_state.complete_focus(frame.host);
        }
        let native_identity = mkmacro_native_identity(frame.host, frame.owner_instance);
        let is_closing =
            frame.effects.close_requested || frame.os_close_requested || frame.body_close_requested;
        if let (Some(identity), Some(viewport_info)) =
            (native_identity, frame.native_viewport.as_ref())
            && frame.callback_class == egui::ViewportClass::Immediate
            && !is_closing
            && frame.effects.host_request != Some(UtilityPresentation::Docked)
        {
            crate::gui::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                frame.callback_class,
                true,
                &mkmacro_native_title(frame.owner_instance),
                identity,
                viewport_info,
                egui::vec2(920.0, 620.0),
                Some(egui::vec2(680.0, 420.0)),
                None,
            );
        }

        self.defer_host_effects = false;
        self.sync_external();
        if self.pending_recording_target_publish {
            self.pending_recording_target_publish = false;
            self.publish_recording_target_now();
        }
        if self.pending_recording_review_drain {
            self.pending_recording_review_drain = false;
            self.drain_pending_recording_reviews_now();
        }
        self.picker_owner = Some((
            frame.owner_context.clone(),
            (frame.callback_class == egui::ViewportClass::Immediate)
                .then_some(native_identity)
                .flatten(),
        ));

        if is_closing {
            if frame.os_close_requested {
                ctx.send_viewport_cmd_to(
                    mkmacro_viewport_id(frame.owner_instance),
                    egui::ViewportCommand::CancelClose,
                );
            }
            if self.dirty {
                self.close_prompt_open = true;
            } else {
                self.finish_host_close(ctx, frame.host, frame.owner_instance);
            }
        }

        let mut closed = !self.open;
        if self.open && !is_closing {
            let child_origin = frame.owner_pass.and_then(|owner| {
                super::utility_host::AcceptedParentOpen::from_retained(
                    ctx, frame.host, frame.owner_instance, frame.root_pass, owner,
                )
            });
            if self.pending_review_open.as_ref().is_some_and(|(origin, _)| origin.is_none())
                && let Some(origin) = child_origin.as_ref()
            {
                self.pending_review_open = Some((Some(origin.clone()), false));
            }
            for action in frame.effects.actions.drain(..) {
                if !self.host_action_is_current(&action, frame.host, frame.owner_instance) {
                    continue;
                }
                if matches!(action.action, MkMacroHostActionKind::AnnotationBegin { .. }) {
                    if let Some(origin) = child_origin.as_ref() {
                        let was_open = self.annotation.open;
                        let result = if was_open { Ok(()) } else { self.begin_recording_annotation() };
                        match result {
                            Ok(()) => self.pending_annotation_open = Some((origin.clone(), was_open)),
                            Err(error) => self.command_error = Some(error.to_string()),
                        }
                    }
                    continue;
                }
                if let MkMacroHostActionKind::Package(package_ui::PackageHostAction::Open(open)) = &action.action {
                    if let Some(origin) = child_origin.as_ref() {
                        self.pending_package_open = Some((open.clone(), action.draft_revision, action.macro_id, origin.clone()));
                    }
                    continue;
                }
                if let MkMacroHostActionKind::OpenSearch(mode) = &action.action {
                    if let Some(origin) = child_origin.as_ref() {
                        self.pending_search_open = Some((*mode, action.draft_revision, action.macro_id, origin.clone()));
                    }
                    continue;
                }
                if matches!(action.action, MkMacroHostActionKind::OpenCatalog) {
                    if let Some(origin) = child_origin.as_ref() {
                        self.pending_catalog_open = Some((action.draft_revision, action.macro_id, origin.clone()));
                    }
                    continue;
                }
                if matches!(action.action, MkMacroHostActionKind::OpenReview { .. }) {
                    if let Some(origin) = child_origin.as_ref() {
                        let was_open = !self.review_open_blocked && self.pending_review_open.is_none();
                        self.review_open_blocked = false;
                        self.pending_review_open = Some((Some(origin.clone()), was_open));
                    }
                    continue;
                }
                if let MkMacroHostActionKind::EditStep { step, .. } = &action.action {
                    if let Some(origin) = child_origin.as_ref() {
                        let was_open = self.action_editor.draft.is_some();
                        if !was_open {
                            self.action_editor.begin_edit_in_macro(action.macro_id, step);
                        }
                        self.pending_editor_origin = Some((EditorOpenOrigin::Main(origin.clone()), was_open));
                    }
                    continue;
                }
                if let MkMacroHostActionKind::OpenWindowPicker(request) = &action.action {
                    if let Some(origin) = child_origin.as_ref() {
                        self.pending_window_picker_open = Some((request.clone(), PickerOpenOrigin::Main(origin.clone())));
                    }
                    continue;
                }
                if self.complete_host_action(ctx, action) {
                    closed = true;
                    break;
                }
            }
            if self.open
                && !closed
                && let Some(requested) = frame.effects.host_request
                && requested != self.host_state.presentation()
            {
                if requested == UtilityPresentation::Docked {
                    if let Some(identity) = native_identity {
                        crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                            ctx, identity,
                        );
                    }
                    ctx.send_viewport_cmd_to(
                        mkmacro_viewport_id(frame.owner_instance),
                        egui::ViewportCommand::Close,
                    );
                }
                if self.host_state.transition(requested) {
                    let key = crate::settings::UtilityToolKey::parse("mkmacro").expect("stable MkMacro key");
                    super::utility_host::remember_presentation(ctx, key, requested);
                    self.native_jump_mode.cancel_all();
                    self.native_jump_frame = None;
                    if requested == UtilityPresentation::Docked {
                        self.show_root_after_host_change = true;
                        if let Some(layer) = self.host_layer_if_root() {
                            ctx.move_to_top(layer);
                        }
                    }
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                    if requested == UtilityPresentation::Popout {
                        ctx.request_repaint_of(mkmacro_viewport_id(frame.owner_instance));
                    }
                }
            }
        }

        self.host_input_allowed = false;
        self.host_controls_allowed = false;
        self.picker_owner = None;
        let persisted_preferences = self.take_macro_properties_preferences_for_persist();
        let mut notices = Vec::new();
        while let Some(notice) = self.take_ui_notice() {
            notices.push(notice);
        }
        let show_root = self.take_show_root_after_host_change();
        MkMacroHostCompletion {
            closed,
            show_root,
            persisted_preferences,
            notices,
        }
    }

    fn show_main_window_host(
        &mut self,
        ctx: &eframe::egui::Context,
        open: &mut bool,
        jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        first_ui_pass: bool,
        root_input_allowed: bool,
        close_shortcut_allowed: bool,
        callback_observed: &mut bool,
        callback_class: &mut eframe::egui::ViewportClass,
        owner_pass: &mut Option<ViewportPassProvenance>,
        owner_pass_accepted: &mut bool,
        owner_modal_layer: &mut Option<eframe::egui::LayerId>,
        draft_checkpoint: &mut Option<MkMacroDraftCheckpoint>,
        close_requested: &mut bool,
    ) -> Option<eframe::egui::InnerResponse<Option<()>>> {
        use eframe::egui;

        let viewport_size = ctx.viewport_rect().size();
        let max_size = [viewport_size.x.max(1.0), viewport_size.y.max(1.0)];
        let default_size = [920.0_f32.min(max_size[0]), 620.0_f32.min(max_size[1])];
        let min_size = [680.0_f32.min(max_size[0]), 420.0_f32.min(max_size[1])];
        let id = egui::Id::new("mkmacro_main_window");
        let window = egui::Window::new(MKMACRO_NATIVE_TITLE)
            .id(id)
            .enabled(root_input_allowed)
            .collapsible(false)
            .default_size(default_size)
            .min_size(min_size)
            .max_size(max_size)
            .resizable(true);
        let output = window.show_utility(
            ctx,
            "mkmacro",
            id,
            Some(open),
            crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::Vec2::from(
                    default_size,
                )),
                egui::Vec2::ZERO,
                [None, None],
            )
            .with_minimum(egui::Vec2::from(min_size))
            .with_maximum(egui::Vec2::from(max_size)),
            |ui| {
                *callback_observed = true;
                *callback_class = egui::ViewportClass::Root;
                let pass = ViewportPassProvenance::current(ui.ctx(), egui::ViewportId::ROOT);
                *owner_pass = Some(pass);
                let pass_authorized = first_ui_pass
                    && ui.current_pass_index() == 0
                    && !ctx.will_discard()
                    && !ui.ctx().will_discard();
                *owner_pass_accepted = pass_authorized;
                *owner_modal_layer = ui.ctx().memory(|memory| memory.top_modal_layer());
                let visible_body = ui.is_visible() && !ui.is_sizing_pass();
                let focused = ui.ctx().input(|input| input.focused);
                let modal_open = ui.ctx().memory(|memory| memory.top_modal_layer().is_some());
                let popup_open = egui::Popup::is_any_open(ui.ctx());
                let expected_layer = crate::gui::jump_mode::ui::mkmacro_dialog_layer_id();
                let root_owner_current = root_input_allowed
                    && mkmacro_surface_owns_input(
                        ui.ctx(),
                        egui::ViewportId::ROOT,
                        expected_layer,
                        self.owner_instance,
                    );
                let nested_owner_open =
                    self.jump_mode_unsupported_nested_owner_open() || self.close_prompt_open;
                if close_shortcut_allowed
                    && pass_authorized
                    && root_owner_current
                    && !nested_owner_open
                    && ui.ctx().input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    })
                {
                    *close_requested = true;
                }
                let body_allowed = pass_authorized
                    && root_input_allowed
                    && focused
                    && visible_body
                    && !modal_open
                    && !popup_open
                    && !*close_requested;
                let surface_input_allowed = pass_authorized
                    && root_input_allowed
                    && root_owner_current
                    && focused
                    && visible_body;
                let host_controls_allowed = body_allowed
                    && self.action_editor.draft.is_none()
                    && !step_table::table_modal_open(self);
                if surface_input_allowed
                    && root_owner_current
                    && has_draft_input_event(ui.ctx())
                    && draft_checkpoint.is_none()
                {
                    *draft_checkpoint = Some(MkMacroDraftCheckpoint::capture(self));
                }
                self.render_host_body(
                    ui,
                    &ui.ctx().clone(),
                    jump_mode_scope,
                    body_allowed,
                    host_controls_allowed,
                    surface_input_allowed,
                );
            },
        );
        #[cfg(test)]
        {
            self.last_main_window_rect = output.as_ref().map(|output| output.response.rect);
        }
        output
    }

    fn show_close_prompt(&mut self, ctx: &eframe::egui::Context, host_input_allowed: bool) {
        if !self.close_prompt_open {
            return;
        }
        let mut choice = None;
        let mut open = true;
        eframe::egui::Window::new("Unsaved MkMacro changes")
            .id(mkmacro_close_prompt_id(self.owner_instance))
            .enabled(host_input_allowed)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Save changes to this macro document before closing?");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(host_input_allowed, eframe::egui::Button::new("Save"))
                        .clicked()
                    {
                        choice = Some(MkMacroCloseChoice::Save);
                    }
                    if ui
                        .add_enabled(host_input_allowed, eframe::egui::Button::new("Discard"))
                        .clicked()
                    {
                        choice = Some(MkMacroCloseChoice::Discard);
                    }
                    if ui
                        .add_enabled(host_input_allowed, eframe::egui::Button::new("Cancel"))
                        .clicked()
                    {
                        choice = Some(MkMacroCloseChoice::Cancel);
                    }
                });
            });
        if host_input_allowed
            && ctx.input_mut(|input| {
                input.consume_key(eframe::egui::Modifiers::NONE, eframe::egui::Key::Escape)
            })
        {
            choice = Some(MkMacroCloseChoice::Cancel);
        }
        if host_input_allowed && let Some(choice) = choice {
            self.stage_host_action(MkMacroHostActionKind::CloseDecision(choice));
        } else if host_input_allowed && !open {
            self.stage_host_action(MkMacroHostActionKind::CloseDecision(
                MkMacroCloseChoice::Cancel,
            ));
        }
    }

    fn render_host_body(
        &mut self,
        ui: &mut eframe::egui::Ui,
        ctx: &eframe::egui::Context,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        body_allowed: bool,
        host_controls_allowed: bool,
        surface_input_allowed: bool,
    ) {
        self.host_input_allowed = surface_input_allowed;
        self.host_controls_allowed = host_controls_allowed;
        ui.scope_builder(
            eframe::egui::UiBuilder::new().id(eframe::egui::Id::new((
                MKMACRO_BODY_ID,
                self.owner_instance,
            ))),
            |ui| {
                ui.add_enabled_ui(body_allowed, |ui| {
                    let _ = self.show_contents_with_table_viewport(ui, jump_scope);
                });
            },
        );
        self.show_close_prompt(ctx, surface_input_allowed);
    }

    fn stage_host_action(&mut self, action: MkMacroHostActionKind) {
        let macro_id = self.selected_macro_id;
        let step_ids = self
            .selected_macro()
            .map(|macro_| {
                macro_
                    .steps
                    .iter()
                    .filter_map(|step| self.selection.ids.contains(&step.id).then_some(step.id))
                    .collect()
            })
            .unwrap_or_default();
        self.pending_host_effects.push(MkMacroHostAction {
            host: self.host_state.identity(),
            draft_revision: self.draft_revision,
            macro_id,
            step_ids,
            action,
        });
    }

    fn stage_host_presentation(&mut self, presentation: UtilityPresentation) {
        self.pending_host_presentation = Some(presentation);
    }

    fn stage_host_toolbar_action(&mut self, action: MkMacroHostActionKind) {
        self.stage_host_action(action);
    }

    pub(super) fn open_action_editor_from_main(&mut self, step: crate::mkmacro::MkStep) {
        if self.action_editor.draft.is_some()
            && !self.action_editor.edits_source(self.selected_macro_id, &step)
        {
            return;
        }
        if self.defer_host_effects {
            let generation = self.selected_macro_id.and_then(|id| self.step_instance_generations.get(&(id, step.id)).copied());
            self.stage_host_action(MkMacroHostActionKind::EditStep { step, generation });
        } else {
            self.action_editor.begin_edit_in_macro(self.selected_macro_id, &step);
        }
    }

    pub(super) fn open_window_picker_from_main(&mut self, request: window_picker::MatcherEditRequest) {
        if self.defer_host_effects {
            self.stage_host_action(MkMacroHostActionKind::OpenWindowPicker(request));
        } else {
            self.window_picker.open(request);
        }
    }

    pub(super) fn stage_host_playback_action(&mut self, command: PlaybackCommand) {
        let Some(snapshot) = self.host_runtime_snapshot() else {
            return;
        };
        self.pending_host_effects.push(MkMacroHostAction {
            host: self.host_state.identity(),
            draft_revision: self.draft_revision,
            macro_id: snapshot.macro_id,
            step_ids: Vec::new(),
            action: MkMacroHostActionKind::Playback {
                command,
                run_id: snapshot.run_id,
                expected_state: snapshot.state,
            },
        });
    }

    pub(super) fn stage_host_recorder_action(
        &mut self,
        command: RecorderCommand,
        target: crate::mkmacro::RecordingTarget,
        expected_state: crate::mkmacro::RecorderRuntimeState,
    ) {
        self.pending_host_effects.push(MkMacroHostAction {
            host: self.host_state.identity(),
            draft_revision: self.draft_revision,
            macro_id: Some(target.macro_id),
            step_ids: Vec::new(),
            action: MkMacroHostActionKind::Recorder {
                command,
                target,
                expected_state,
            },
        });
    }

    pub(super) fn stage_host_annotation_begin(
        &mut self,
        expected_state: crate::mkmacro::RecorderRuntimeState,
        macro_id: Option<u64>,
        recording_id: Option<u64>,
    ) {
        self.stage_host_action(MkMacroHostActionKind::AnnotationBegin {
            macro_id,
            recording_id,
            expected_state,
        });
    }

    fn host_action_is_current(
        &self,
        action: &MkMacroHostAction,
        host: HostIdentity,
        owner_instance: u64,
    ) -> bool {
        if action.host != host
            || host != self.host_state.identity()
            || owner_instance != self.owner_instance
        {
            return false;
        }
        if !matches!(
            &action.action,
            MkMacroHostActionKind::Playback { .. }
                | MkMacroHostActionKind::Recorder { .. }
                | MkMacroHostActionKind::CloseDecision(_)
        ) && action.draft_revision != self.draft_revision
        {
            return false;
        }
        match &action.action {
            MkMacroHostActionKind::CloseDecision(_) => true,
            MkMacroHostActionKind::Playback {
                run_id,
                expected_state,
                ..
            } => self.host_runtime_snapshot().is_some_and(|snapshot| {
                snapshot.run_id == *run_id
                    && snapshot.state == *expected_state
                    && action.macro_id == snapshot.macro_id
            }),
            MkMacroHostActionKind::Recorder {
                target,
                expected_state,
                ..
            } => {
                if *expected_state == crate::mkmacro::RecorderRuntimeState::Idle {
                    action.macro_id == self.selected_macro_id
                        && self.recording_target() == Some(*target)
                } else {
                    crate::mkmacro::runtime::recorder_snapshot().is_some_and(|snapshot| {
                        snapshot.state == *expected_state
                            && snapshot.macro_id == Some(target.macro_id)
                    })
                }
            }
            MkMacroHostActionKind::AnnotationBegin {
                macro_id,
                recording_id,
                expected_state,
            } => crate::mkmacro::runtime::recorder_snapshot().is_some_and(|snapshot| {
                snapshot.state == *expected_state
                    && snapshot.macro_id == *macro_id
                    && macro_id.is_some()
                    && snapshot.recording_id == *recording_id
                    && recording_id.is_some()
            }),
            MkMacroHostActionKind::OpenReview { epoch } => self.recording_review.as_ref().is_some_and(|review| review.epoch == *epoch),
            MkMacroHostActionKind::EditStep { step, generation } => {
                (self.action_editor.draft.is_none() || self.action_editor.edits_source(action.macro_id, step))
                    && action.macro_id == self.selected_macro_id
                    && generation.is_some()
                    && self.selected_macro_id.is_some_and(|id| {
                        self.step_instance_generations.get(&(id, step.id)).copied() == *generation
                            && self.selected_macro().is_some_and(|m| m.steps.iter().any(|current| current == step))
                    })
            }
            MkMacroHostActionKind::OpenWindowPicker(request) => self.window_picker_request_is_current(request),
            MkMacroHostActionKind::Package(package_action) => {
                package_ui::host_action_is_current(self, package_action)
            }
            _ => {
                if action.macro_id != self.selected_macro_id {
                    return false;
                }
                let current_step_ids = self
                    .selected_macro()
                    .map(|macro_| {
                        macro_
                            .steps
                            .iter()
                            .filter_map(|step| {
                                self.selection.ids.contains(&step.id).then_some(step.id)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                action.step_ids == current_step_ids
            }
        }
    }

    fn complete_host_action(
        &mut self,
        ctx: &eframe::egui::Context,
        action: MkMacroHostAction,
    ) -> bool {
        let result = match action.action {
            MkMacroHostActionKind::Save => self.save().map(|_| false),
            MkMacroHostActionKind::Run { debug, selection } => {
                let command = match selection {
                    RunSelection::Whole => self.prepare_execution_checked().map(|id| {
                        if debug {
                            crate::mkmacro::runtime::RuntimeCommand::DebugRun(id)
                        } else {
                            crate::mkmacro::runtime::RuntimeCommand::Run(id)
                        }
                    }),
                    RunSelection::From(step_id) => {
                        self.prepare_from_step(step_id).map(|(id, step)| {
                            if debug {
                                crate::mkmacro::runtime::RuntimeCommand::DebugRunFrom(id, step)
                            } else {
                                crate::mkmacro::runtime::RuntimeCommand::RunFrom(id, step)
                            }
                        })
                    }
                    RunSelection::Selected => self.prepare_selected_steps().map(|(id, steps)| {
                        if debug {
                            crate::mkmacro::runtime::RuntimeCommand::DebugRunSelection(id, steps)
                        } else {
                            crate::mkmacro::runtime::RuntimeCommand::RunSelection(id, steps)
                        }
                    }),
                };
                command.and_then(|command| self.submit_host_runtime_command(command).map(|_| false))
            }
            MkMacroHostActionKind::Playback {
                command,
                run_id,
                expected_state,
            } => {
                let still_current = self.host_runtime_snapshot().is_some_and(|snapshot| {
                    snapshot.run_id == run_id && snapshot.state == expected_state
                });
                if !still_current {
                    Ok(false)
                } else {
                    let command = match command {
                        PlaybackCommand::Pause => crate::mkmacro::runtime::RuntimeCommand::Pause,
                        PlaybackCommand::Resume => crate::mkmacro::runtime::RuntimeCommand::Resume,
                        PlaybackCommand::Stop => crate::mkmacro::runtime::RuntimeCommand::Stop,
                    };
                    self.submit_host_runtime_command(command).map(|_| false)
                }
            }
            MkMacroHostActionKind::Recorder {
                command,
                target,
                expected_state,
            } => self
                .complete_recorder_action(command, target, expected_state)
                .map(|_| false),
            MkMacroHostActionKind::AnnotationBegin { .. } => {
                self.begin_recording_annotation().map(|_| false)
            }
            MkMacroHostActionKind::Package(package_action) => {
                package_ui::complete_host_action(self, package_action).map(|_| false)
            }
            MkMacroHostActionKind::OpenSearch(_) => Ok(false),
            MkMacroHostActionKind::OpenCatalog => Ok(false),
            MkMacroHostActionKind::OpenReview { .. } => Ok(false),
            MkMacroHostActionKind::EditStep { .. } => Ok(false),
            MkMacroHostActionKind::OpenWindowPicker(_) => Ok(false),
            MkMacroHostActionKind::CloseDecision(choice) => {
                return self.complete_close_choice(ctx, choice);
            }
        };
        if let Err(error) = result {
            self.command_error = Some(error.to_string());
        }
        false
    }

    fn complete_close_choice(
        &mut self,
        ctx: &eframe::egui::Context,
        choice: MkMacroCloseChoice,
    ) -> bool {
        match choice {
            MkMacroCloseChoice::Cancel => {
                self.close_prompt_open = false;
                false
            }
            MkMacroCloseChoice::Save => match self.save() {
                Ok(()) => {
                    self.finish_host_close(ctx, self.host_state.identity(), self.owner_instance);
                    true
                }
                Err(error) => {
                    self.command_error = Some(error.to_string());
                    false
                }
            },
            MkMacroCloseChoice::Discard => {
                self.retire_review_and_annotation(ctx);
                if self.catalog.open {
                    self.catalog_host.close_from_app(ctx);
                    self.catalog.open = false;
                }
                if self.action_editor.draft.is_some()
                    && self.pending_editor_origin.as_ref().is_none_or(|(_, was_open)| *was_open)
                {
                    self.editor_host.close_from_app(ctx);
                }
                if self.window_picker.open {
                    self.window_picker_host.close_from_app(ctx);
                    self.window_picker.cancel("Window picking cancelled");
                }
                if self.launcher_action_picker.open {
                    self.launcher_picker_host.close_from_app(ctx);
                    self.launcher_action_picker.cancel();
                }
                if self.image_crop_editor.is_open() {
                    self.crop_host.close_from_app(ctx);
                    self.image_crop_editor.cancel();
                }
                self.pending_crop_open = None;
                self.pending_launcher_picker_open = None;
                self.pending_window_picker_open = None;
                self.close_with_decision(DirtyDecision::Discard);
                self.finish_host_close(ctx, self.host_state.identity(), self.owner_instance);
                true
            }
        }
    }

    fn finish_host_close(
        &mut self,
        ctx: &eframe::egui::Context,
        host: HostIdentity,
        owner_instance: u64,
    ) {
        self.retire_review_and_annotation(ctx);
        let key = crate::settings::UtilityToolKey::parse("mkmacro").expect("stable MkMacro key");
        super::utility_host::remember_presentation(ctx, key, host.host);
        self.presentation_prepared = false;
        let identity = mkmacro_native_identity(host, owner_instance);
        if self.navigation.search.mode.is_some() {
            self.search_host.close_from_app(ctx);
        }
        self.pending_search_open = None;
        self.pending_package_open = None;
        self.package_hosts.close_all(ctx);
        if self.catalog.open {
            self.catalog_host.close_from_app(ctx);
            self.catalog.open = false;
        }
        if self.action_editor.draft.is_some()
            && self.pending_editor_origin.as_ref().is_none_or(|(_, was_open)| *was_open)
        {
            self.editor_host.close_from_app(ctx);
        }
        if self.window_picker.open {
            self.window_picker_host.close_from_app(ctx);
            self.window_picker.cancel("Window picking cancelled");
        }
        if self.launcher_action_picker.open {
            self.launcher_picker_host.close_from_app(ctx);
            self.launcher_action_picker.cancel();
        }
        if self.image_crop_editor.is_open() {
            self.crop_host.close_from_app(ctx);
            self.image_crop_editor.cancel();
        }
        self.pending_crop_open = None;
        self.pending_launcher_picker_open = None;
        self.pending_window_picker_open = None;
        self.pending_catalog_open = None;
        self.pending_editor_origin = None;
        if self.open {
            if self.dirty {
                let _ = self.close_with_decision(DirtyDecision::Discard);
            } else {
                self.open = false;
                self.set_selected_macro(None);
            }
        }
        self.close_children();
        self.open = false;
        self.close_prompt_open = false;
        if let Some(identity) = identity {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, eframe::egui::ViewportCommand::Close);
        }
        self.host_state.retire_to_docked();
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.publish_recording_target_now();
        ctx.request_repaint_of(eframe::egui::ViewportId::ROOT);
    }

    fn submit_host_runtime_command(
        &self,
        command: crate::mkmacro::runtime::RuntimeCommand,
    ) -> anyhow::Result<()> {
        #[cfg(test)]
        if let Some(runtime) = &self.runtime_test {
            return match runtime.command(command) {
                crate::mkmacro::CommandResult::Accepted => Ok(()),
                result => anyhow::bail!("{result:?}"),
            };
        }
        match command {
            crate::mkmacro::runtime::RuntimeCommand::Run(id) => {
                crate::mkmacro::runtime::run_with_source(
                    id,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::RunFrom(id, step) => {
                crate::mkmacro::runtime::run_from_with_source(
                    id,
                    step,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::RunSelection(id, steps) => {
                crate::mkmacro::runtime::run_selection_with_source(
                    id,
                    steps,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::DebugRun(id) => {
                crate::mkmacro::runtime::debug_run_with_source(
                    id,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::DebugRunFrom(id, step) => {
                crate::mkmacro::runtime::debug_run_from_with_source(
                    id,
                    step,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::DebugRunSelection(id, steps) => {
                crate::mkmacro::runtime::debug_run_selection_with_source(
                    id,
                    steps,
                    crate::mkmacro::InvocationSource::MacroDialog,
                )
            }
            crate::mkmacro::runtime::RuntimeCommand::Pause => crate::mkmacro::runtime::pause(),
            crate::mkmacro::runtime::RuntimeCommand::Resume => crate::mkmacro::runtime::resume(),
            crate::mkmacro::runtime::RuntimeCommand::Stop => crate::mkmacro::runtime::stop(),
            other => anyhow::bail!("Unsupported host runtime command: {other:?}"),
        }
    }

    pub(super) fn host_runtime_snapshot(&self) -> Option<Arc<crate::mkmacro::RuntimeSnapshot>> {
        #[cfg(test)]
        if let Some(runtime) = &self.runtime_test {
            return Some(runtime.snapshot());
        }
        crate::mkmacro::runtime::snapshot()
    }

    fn complete_recorder_action(
        &mut self,
        command: RecorderCommand,
        target: crate::mkmacro::RecordingTarget,
        expected_state: crate::mkmacro::RecorderRuntimeState,
    ) -> anyhow::Result<()> {
        use crate::mkmacro::RecorderRuntimeState as State;
        let recorder = crate::mkmacro::runtime::recorder_snapshot();
        let current_state = if crate::mkmacro::runtime::record_stop_pending() {
            State::Stopping
        } else {
            recorder
                .as_ref()
                .map_or(State::Idle, |snapshot| snapshot.state)
        };
        if current_state != expected_state
            || (expected_state == State::Idle && self.recording_target() != Some(target))
            || (expected_state != State::Idle
                && recorder.as_ref().and_then(|snapshot| snapshot.macro_id)
                    != Some(target.macro_id))
        {
            anyhow::bail!(
                "The recorder state or selected macro changed before the command was accepted"
            );
        }
        match command {
            RecorderCommand::Record => crate::mkmacro::runtime::record_target(
                target,
                NormalizationConfig::from(&self.draft.settings.recorder),
            ),
            RecorderCommand::Pause => crate::mkmacro::runtime::record_pause(),
            RecorderCommand::Resume => crate::mkmacro::runtime::record_resume(),
            RecorderCommand::Marker => crate::mkmacro::runtime::record_marker(),
            RecorderCommand::StopForReview => {
                crate::mkmacro::runtime::request_record_stop_for_review().map(|_| ())
            }
        }
    }

    pub fn take_ui_notice(&mut self) -> Option<String> {
        self.ui_notices.pop_front()
    }

    pub(crate) fn queue_ui_notice(&mut self, notice: impl Into<String>) {
        self.ui_notices.push_back(notice.into());
    }

    #[cfg(test)]
    pub(crate) fn set_image_search_backend_for_test(
        &mut self,
        backend: Arc<dyn crate::mkmacro::VisualSearch>,
    ) {
        self.image_search_backend = backend;
    }
    pub fn sync_external(&mut self) {
        let current = self.store.snapshot();
        if *current != *self.baseline {
            if self.dirty {
                self.conflict = true;
            } else {
                self.draft = (*current).clone();
                self.recording_insert_undo.clear();
                self.recording_insert_redo.clear();
                self.record_draft_revision();
                self.baseline = current;
                self.cancel_folder_operations();
                if self.selected_macro().is_none() {
                    self.set_selected_macro(None);
                }
            }
        }
        self.prune_folder_ui_state();
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        repair_ids(&mut self.draft);
        self.record_draft_revision();
        self.refresh_environment();
        self.baseline = self.store.save(self.draft.clone())?;
        self.dirty = false;
        self.conflict = false;
        Ok(())
    }
    pub fn mark_dirty(&mut self) {
        if self.record_draft_revision() {
            self.dirty = true;
        }
    }

    fn record_draft_revision(&mut self) -> bool {
        if self.revision_document == self.draft {
            return false;
        }
        let current_steps: HashSet<_> = self
            .draft
            .macros
            .iter()
            .flat_map(|macro_item| {
                macro_item
                    .steps
                    .iter()
                    .map(move |step| (macro_item.id, step.id))
            })
            .collect();
        self.step_instance_generations
            .retain(|identity, _| current_steps.contains(identity));
        for identity in current_steps {
            self.step_instance_generations
                .entry(identity)
                .or_insert_with(|| {
                    let generation = self.next_step_instance_generation;
                    self.next_step_instance_generation =
                        self.next_step_instance_generation.wrapping_add(1).max(1);
                    generation
                });
        }
        self.draft_revision = self.draft_revision.wrapping_add(1);
        self.revision_document = self.draft.clone();
        self.editor_state.folds.retain_document(&self.draft);
        if let Some(m) = self.selected_macro() {
            let rows = m.steps.iter().map(|s| s.id).collect::<Vec<_>>();
            self.selection.reconcile(&rows);
        }
        true
    }

    pub fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_observation(&self, ctx: &eframe::egui::Context) -> MkMacroDialogQaObservation {
        let runtime_active = crate::mkmacro::runtime::snapshot().is_some_and(|snapshot| {
            matches!(
                snapshot.state,
                crate::mkmacro::runtime::RuntimeState::Running
                    | crate::mkmacro::runtime::RuntimeState::Paused
                    | crate::mkmacro::runtime::RuntimeState::Stopping
            )
        });
        let recorder_active = crate::mkmacro::runtime::recorder_snapshot()
            .is_some_and(|snapshot| snapshot.state != crate::mkmacro::RecorderRuntimeState::Idle);
        let table_focus_id = self.selected_macro_id.map(navigation::table_id);
        let table_focused =
            table_focus_id.is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)));
        MkMacroDialogQaObservation {
            open: self.open,
            draft_revision: self.draft_revision,
            selected_macro_id: self.selected_macro_id,
            selected_step_ids: self.selection.ids.iter().copied().collect(),
            table_focus_id,
            table_focused,
            document_macro_count: self.draft.macros.len(),
            runtime_active,
            recorder_active,
        }
    }

    pub(crate) fn jump_mode_target_generation(&self) -> crate::gui::jump_mode::TargetGeneration {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.draft_revision.hash(&mut hasher);
        self.search.hash(&mut hasher);
        self.selected_macro_id.hash(&mut hasher);

        let mut collapsed_folders = self.collapsed_folders.iter().copied().collect::<Vec<_>>();
        collapsed_folders.sort_unstable();
        collapsed_folders.hash(&mut hasher);

        if let Some(macro_id) = self.selected_macro_id
            && let Some(structure) = self.cached_structure(macro_id)
        {
            for row in self.editor_state.folds.visible_rows(macro_id, &structure) {
                structure.steps[row].step_id.hash(&mut hasher);
            }
        }

        crate::gui::jump_mode::TargetGeneration(hasher.finish())
    }

    pub(crate) fn jump_mode_unsupported_nested_owner_open(&self) -> bool {
        let recorder_active = crate::mkmacro::runtime::recorder_snapshot()
            .is_some_and(|snapshot| snapshot.state != crate::mkmacro::RecorderRuntimeState::Idle)
            || crate::mkmacro::runtime::record_stop_pending();
        self.action_editor.draft.is_some() || step_table::table_modal_open(self) || recorder_active
    }

    /// Refresh external image and monitor state without dirtying the document.
    pub fn refresh_environment(&mut self) {
        self.record_draft_revision();
        self.action_editor.image_refs_cache = None;
        self.analysis_cache
            .borrow_mut()
            .refresh_environment(&self.draft, &self.store.asset_root());
    }

    pub(super) fn cached_diagnostics(&self) -> Arc<Vec<crate::mkmacro::MkDiagnostic>> {
        self.analysis_cache
            .borrow_mut()
            .diagnostics(&self.draft, self.draft_revision)
    }

    pub(super) fn cached_structure(
        &self,
        macro_id: u64,
    ) -> Option<Arc<crate::mkmacro::StructureAnalysis>> {
        self.analysis_cache
            .borrow_mut()
            .structure(&self.draft, self.draft_revision, macro_id)
    }

    pub fn create_folder(&mut self) -> u64 {
        let names: HashSet<_> = self
            .draft
            .folders
            .iter()
            .map(|folder| folder.name.to_lowercase())
            .collect();
        let mut name = "New Folder".to_owned();
        let mut suffix = 2;
        while names.contains(&name.to_lowercase()) {
            name = format!("New Folder {suffix}");
            suffix += 1;
        }
        self.draft.folders.push(MkMacroFolder { id: 0, name });
        repair_ids(&mut self.draft);
        let id = self.draft.folders.last().unwrap().id;
        self.mark_dirty();
        id
    }

    /// Returns true only when an existing macro changes to a valid destination.
    pub fn move_macro_to_folder(&mut self, macro_id: u64, folder_id: Option<u64>) -> bool {
        if folder_id
            .is_some_and(|id| id == 0 || !self.draft.folders.iter().any(|folder| folder.id == id))
        {
            return false;
        }
        let Some(m) = self.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
            return false;
        };
        if m.folder_id == folder_id {
            return false;
        }
        m.folder_id = folder_id;
        self.mark_dirty();
        true
    }

    pub fn is_folder_collapsed(&self, folder_id: u64) -> bool {
        self.collapsed_folders.contains(&folder_id)
    }

    /// Collapse is presentation-only and must not mark the draft dirty.
    pub fn toggle_folder_collapsed(&mut self, folder_id: u64) {
        if !self
            .draft
            .folders
            .iter()
            .any(|folder| folder.id == folder_id)
        {
            return;
        }
        if !self.collapsed_folders.remove(&folder_id) {
            self.collapsed_folders.insert(folder_id);
        }
    }

    pub fn begin_folder_rename(&mut self, folder_id: u64) {
        self.cancel_folder_rename();
        if let Some(folder) = self
            .draft
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
        {
            self.pending_folder_rename = Some(folder_id);
            self.folder_rename_text = folder.name.clone();
            self.folder_rename_needs_focus = true;
        }
    }

    /// Names match using Rust's Unicode-aware `str::to_lowercase` after trimming.
    /// Returns false for an unchanged name, without changing the dirty flag.
    pub fn rename_folder(
        &mut self,
        folder_id: u64,
        proposed_name: &str,
    ) -> Result<bool, FolderNameError> {
        let index = self
            .draft
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
            .ok_or(FolderNameError::MissingFolder(folder_id))?;
        let name = proposed_name.trim();
        if name.is_empty() {
            return Err(FolderNameError::Empty);
        }
        let normalized = name.to_lowercase();
        if let Some(conflict) = self.draft.folders.iter().find(|folder| {
            folder.id != folder_id && folder.name.trim().to_lowercase() == normalized
        }) {
            return Err(FolderNameError::Duplicate(conflict.name.clone()));
        }
        let changed = self.draft.folders[index].name != name;
        if changed {
            self.draft.folders[index].name = name.to_owned();
            self.mark_dirty();
        }
        self.cancel_folder_rename();
        Ok(changed)
    }

    pub fn cancel_folder_rename(&mut self) {
        self.pending_folder_rename = None;
        self.folder_rename_text.clear();
        self.folder_rename_needs_focus = false;
        self.folder_error = None;
    }

    pub fn request_delete_folder(&mut self, folder_id: u64) {
        self.cancel_folder_deletion();
        if let Some(folder) = self
            .draft
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
        {
            self.pending_delete_folder = Some(folder_id);
            let member_count = self
                .draft
                .macros
                .iter()
                .filter(|m| m.folder_id == Some(folder_id))
                .count();
            self.folder_delete_confirmation.open_custom(
                format!(
                    "Delete folder \"{}\"? Members: {member_count}.",
                    folder.name
                ),
                "Members will move to Unfiled. No macros will be deleted.",
            );
        }
    }

    /// Remove only the folder organization, preserving macros and editor selection.
    /// Missing folders (including stale confirmations) are a no-op.
    pub fn delete_folder(&mut self, folder_id: u64) -> bool {
        let Some(index) = self
            .draft
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
        else {
            return false;
        };
        for m in &mut self.draft.macros {
            if m.folder_id == Some(folder_id) {
                m.folder_id = None;
            }
        }
        self.draft.folders.remove(index);
        self.collapsed_folders.remove(&folder_id);
        if self.pending_folder_rename == Some(folder_id) {
            self.cancel_folder_rename();
        }
        self.mark_dirty();
        true
    }

    fn handle_folder_delete_confirmation(&mut self, result: ConfirmationResult) {
        match result {
            ConfirmationResult::Confirmed => {
                if let Some(id) = self.pending_delete_folder {
                    self.delete_folder(id);
                }
                self.cancel_folder_deletion();
            }
            ConfirmationResult::Cancelled => self.cancel_folder_deletion(),
            ConfirmationResult::None => {}
        }
    }

    fn cancel_folder_deletion(&mut self) {
        self.pending_delete_folder = None;
        self.folder_delete_confirmation = ConfirmationModal::default();
    }

    fn cancel_folder_operations(&mut self) {
        self.cancel_folder_rename();
        self.cancel_folder_deletion();
    }

    /// Preserve collapse state only for folders in the current draft, including
    /// during an external conflict where the dirty draft remains authoritative.
    fn prune_folder_ui_state(&mut self) {
        let folder_ids: HashSet<_> = self.draft.folders.iter().map(|folder| folder.id).collect();
        self.collapsed_folders.retain(|id| folder_ids.contains(id));
        if self
            .pending_folder_rename
            .is_some_and(|id| !folder_ids.contains(&id))
        {
            self.cancel_folder_rename();
        }
        if self
            .pending_delete_folder
            .is_some_and(|id| !folder_ids.contains(&id))
        {
            self.cancel_folder_deletion();
        }
    }

    fn apply_hotkey_scope_matcher(&mut self, macro_id: u64, matcher: MkWindowMatcher) -> bool {
        let changed = {
            let Some(macro_) = self.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
                return false;
            };
            let MkHotkeyScope::ActiveWindow(current) = &mut macro_.hotkey_scope else {
                return false;
            };
            if *current == matcher {
                false
            } else {
                *current = matcher;
                true
            }
        };
        if changed {
            self.mark_dirty();
        }
        true
    }

    fn apply_window_picker_confirmation(
        &mut self,
        request: &window_picker::MatcherEditRequest,
        matcher: MkWindowMatcher,
    ) -> bool {
        if !self.window_picker_request_is_current(request) { return false; }
        match &request.destination {
            window_picker::MatcherDestination::Action { .. } => self
                .action_editor
                .apply_window_matcher(request, matcher, self.selected_macro_id),
            window_picker::MatcherDestination::MacroHotkey { macro_id } => {
                self.apply_hotkey_scope_matcher(*macro_id, matcher)
            }
        }
    }

    fn window_picker_request_is_current(&self, request: &window_picker::MatcherEditRequest) -> bool {
        match &request.destination {
            window_picker::MatcherDestination::Action { .. } => self.action_editor.window_matcher_request_is_current(request, self.selected_macro_id),
            window_picker::MatcherDestination::MacroHotkey { macro_id } => self.draft.macros.iter().find(|m| m.id == *macro_id).is_some_and(|m| matches!(&m.hotkey_scope, MkHotkeyScope::ActiveWindow(current) if *current == request.original)),
        }
    }
    /// Rename intent shared by non-widget controllers and the dialog.
    pub fn rename_selected(&mut self, name: impl Into<String>) -> bool {
        let name = name.into();
        let Some(m) = self.selected_macro_mut() else {
            return false;
        };
        if m.name == name {
            return false;
        }
        m.name = name;
        self.mark_dirty();
        true
    }
    /// Close/reload never silently destroys a dirty draft.
    pub fn close_with_decision(&mut self, decision: DirtyDecision) -> bool {
        if self.dirty && decision == DirtyDecision::KeepEditing {
            return false;
        }
        if self.dirty {
            let current = self.store.snapshot();
            self.draft = (*current).clone();
            self.recording_insert_undo.clear();
            self.recording_insert_redo.clear();
            self.record_draft_revision();
            self.baseline = current;
            self.dirty = false;
            self.conflict = false;
        }
        self.cancel_folder_operations();
        self.prune_folder_ui_state();
        self.action_editor.cancel();
        self.image_crop_editor.cancel();
        self.discard_all_recording_reviews();
        self.open = false;
        self.set_selected_macro(None);
        true
    }
    pub fn reload_with_decision(&mut self, decision: DirtyDecision) -> bool {
        if self.dirty && decision == DirtyDecision::KeepEditing {
            return false;
        }
        let current = self.store.snapshot();
        self.draft = (*current).clone();
        self.recording_insert_undo.clear();
        self.recording_insert_redo.clear();
        self.record_draft_revision();
        self.baseline = current;
        self.dirty = false;
        self.conflict = false;
        self.cancel_folder_operations();
        self.prune_folder_ui_state();
        true
    }
    pub fn open_recording_review(
        &mut self,
        result: crate::mkmacro::RecordingResult,
    ) -> Result<RecordingReviewOpenDisposition, String> {
        let mut review = recording_review::RecordingReviewSession::new(result);
        self.next_review_epoch = self.next_review_epoch.wrapping_add(1).max(1);
        review.epoch = self.next_review_epoch;
        if self.recording_review.is_some() {
            self.queued_recording_reviews.push_back(review);
            self.queue_ui_notice(
                "Another completed recording is queued behind the current Recording Review",
            );
            return Ok(RecordingReviewOpenDisposition::Queued);
        }
        self.recording_review = Some(review);
        self.review_open_blocked = false;
        self.pending_review_open = Some((None, false));
        self.publish_recording_target();
        Ok(RecordingReviewOpenDisposition::Opened)
    }

    pub fn drain_pending_recording_reviews(&mut self) {
        if self.defer_host_effects {
            self.pending_recording_review_drain = true;
            return;
        }
        self.drain_pending_recording_reviews_now();
    }

    fn drain_pending_recording_reviews_now(&mut self) {
        for result in crate::mkmacro::runtime::take_pending_recordings() {
            if let Err(error) = self.open_recording_review(result) {
                self.command_error = Some(error);
            }
        }
    }

    pub fn stop_recording_for_review(&mut self) -> anyhow::Result<()> {
        crate::mkmacro::runtime::request_record_stop_for_review()?;
        Ok(())
    }

    pub fn begin_recording_annotation(&mut self) -> anyhow::Result<()> {
        let snapshot = crate::mkmacro::runtime::recorder_snapshot()
            .ok_or_else(|| anyhow::anyhow!("No active recording is available to annotate"))?;
        let state = snapshot.state;
        let macro_id = snapshot
            .macro_id
            .ok_or_else(|| anyhow::anyhow!("The active recording has no macro target"))?;
        let recording_id = snapshot.recording_id
            .ok_or_else(|| anyhow::anyhow!("The active recording has no identity"))?;
        let pause_owned = annotation_pause_ownership(state)?;
        self.annotation.pause_id = None;
        crate::mkmacro::runtime::set_recording_annotation_active(recording_id, true)?;
        self.annotation.pause_owned = match pause_owned {
            true => {
                match crate::mkmacro::runtime::record_pause_for(recording_id) {
                    Ok(pause_id) => self.annotation.pause_id = Some(pause_id),
                    Err(error) => {
                        let _ = crate::mkmacro::runtime::set_recording_annotation_active(recording_id, false);
                        self.annotation.target = None;
                        return Err(error);
                    }
                }
                true
            }
            false => false,
        };
        self.annotation.text.clear();
        self.annotation.open = true;
        self.annotation.target = Some(macro_id);
        self.annotation.recording_id = Some(recording_id);
        self.annotation.epoch = self.annotation.epoch.wrapping_add(1).max(1);
        self.annotation.focus_requested = true;
        Ok(())
    }

    pub fn finish_recording_annotation(&mut self, commit: bool) -> anyhow::Result<()> {
        let text = self.annotation.text.trim().to_owned();
        let recording_id = self.annotation.recording_id;
        let snapshot = crate::mkmacro::runtime::recorder_snapshot();
        let current = snapshot.as_ref()
            .is_some_and(|snapshot| snapshot.recording_id == recording_id && recording_id.is_some());
        let annotation_result = if commit {
            if text.is_empty() {
                anyhow::bail!("Annotation cannot be empty");
            }
            match recording_id.filter(|_| current) {
                Some(id) => crate::mkmacro::runtime::record_annotation_for(id, text),
                None => Err(anyhow::anyhow!("The recording has changed")),
            }
        } else {
            Ok(())
        };
        self.annotation.open = false;
        self.annotation.text.clear();
        self.annotation.target = None;
        self.annotation.recording_id = None;
        if let Some(recording_id) = recording_id {
            let _ = crate::mkmacro::runtime::set_recording_annotation_active(recording_id, false);
        }
        let resume = std::mem::take(&mut self.annotation.pause_owned);
        let pause_id = self.annotation.pause_id.take();
        let resume_result = if should_resume_annotation_owned(
            resume && current && snapshot.as_ref().is_some_and(|snapshot| snapshot.pause_id == pause_id),
            snapshot.as_ref().map(|snapshot| snapshot.state),
        ) {
            match (recording_id, pause_id) {
                (Some(recording_id), Some(pause_id)) => crate::mkmacro::runtime::record_resume_for(recording_id, pause_id),
                _ => Ok(()),
            }
        } else {
            Ok(())
        };
        if let Err(error) = annotation_result {
            return Err(error);
        }
        resume_result
    }

    pub fn preview_recording_review(&mut self, selected_range: bool) -> anyhow::Result<u64> {
        if self.action_editor.draft.is_some() || self.annotation.open {
            anyhow::bail!("Close the active editor before previewing this recording");
        }
        if self
            .recording_preview_recorder_state()
            .is_some_and(|state| state != crate::mkmacro::RecorderRuntimeState::Idle)
        {
            anyhow::bail!("Stop recording before previewing");
        }
        if self
            .recording_preview_runtime_snapshot()
            .is_some_and(|snapshot| {
                matches!(
                    snapshot.state,
                    crate::mkmacro::RuntimeState::Running
                        | crate::mkmacro::RuntimeState::Paused
                        | crate::mkmacro::RuntimeState::Stopping
                )
            })
        {
            anyhow::bail!("Another playback operation is active");
        }
        let (target, plan) = {
            let review = self
                .recording_review
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No recording is awaiting review"))?;
            (
                review.effective_target(),
                review.preview_plan(selected_range)?,
            )
        };
        {
            let review = self.recording_review.as_mut().unwrap();
            review.preview_ticket = None;
            review.preview_state = None;
            review.preview_diagnostic = None;
            review.message = None;
        }
        let document = build_recording_preview_document(&self.draft, target, &plan)?;
        let ticket = self.submit_recording_preview(&document, target.macro_id)?;
        let review = self.recording_review.as_mut().unwrap();
        review.preview_ticket = Some(ticket);
        review.preview_state = Some(crate::mkmacro::RuntimeState::Running);
        review.preview_diagnostic = None;
        Ok(ticket)
    }

    pub(crate) fn stop_owned_recording_preview(&mut self) {
        if let Some(ticket) = self
            .recording_review
            .as_ref()
            .and_then(|review| review.preview_ticket)
        {
            self.stop_recording_preview(ticket);
        }
    }

    fn submit_recording_preview(
        &mut self,
        document: &MkMacroDocument,
        macro_id: u64,
    ) -> anyhow::Result<u64> {
        #[cfg(test)]
        if let Some(facade) = &self.recording_preview_test_facade {
            let ticket = self.next_recording_preview_test_ticket;
            self.next_recording_preview_test_ticket = ticket.wrapping_add(1).max(1);
            facade.runtime.preview(document, macro_id, ticket)?;
            return Ok(ticket);
        }
        crate::mkmacro::runtime::preview_document(document, macro_id)
    }

    fn stop_recording_preview(&self, ticket: u64) -> bool {
        #[cfg(test)]
        if let Some(facade) = &self.recording_preview_test_facade {
            return facade.runtime.stop_recording_preview(ticket);
        }
        crate::mkmacro::runtime::stop_recording_preview(ticket)
    }

    fn recording_preview_recorder_state(&self) -> Option<crate::mkmacro::RecorderRuntimeState> {
        #[cfg(test)]
        if let Some(facade) = &self.recording_preview_test_facade {
            return Some(facade.recorder_state);
        }
        crate::mkmacro::runtime::recorder_snapshot().map(|snapshot| snapshot.state)
    }

    fn recording_preview_runtime_snapshot(&self) -> Option<Arc<crate::mkmacro::RuntimeSnapshot>> {
        #[cfg(test)]
        if let Some(facade) = &self.recording_preview_test_facade {
            return Some(facade.runtime.snapshot());
        }
        crate::mkmacro::runtime::snapshot()
    }

    fn recording_preview_status(
        &self,
        ticket: u64,
    ) -> (bool, Option<Arc<crate::mkmacro::RuntimeSnapshot>>) {
        #[cfg(test)]
        if let Some(facade) = &self.recording_preview_test_facade {
            return facade.runtime.recording_preview_status(ticket);
        }
        crate::mkmacro::runtime::recording_preview_status(ticket)
    }

    #[cfg(test)]
    fn set_recording_preview_runtime_for_test(
        &mut self,
        runtime: Arc<crate::mkmacro::MacroRuntime>,
    ) {
        self.recording_preview_test_facade = Some(RecordingPreviewTestFacade {
            runtime,
            recorder_state: crate::mkmacro::RecorderRuntimeState::Idle,
        });
    }

    #[cfg(test)]
    fn set_runtime_for_test(&mut self, runtime: Arc<crate::mkmacro::MacroRuntime>) {
        self.runtime_test = Some(runtime);
    }

    /// Applies the complete reviewed proposal as one atomic document mutation.
    /// The captured target, not current macro/row selection, owns placement.
    pub fn recording_review_apply_blocker(&self) -> Option<String> {
        let review = self.recording_review.as_ref()?;
        let target = review.effective_target();
        let Some(macro_item) = self
            .draft
            .macros
            .iter()
            .find(|item| item.id == target.macro_id)
        else {
            return Some(
                "The recording target macro no longer exists. Choose a target macro or cancel."
                    .to_owned(),
            );
        };
        if let Some(anchor) = target.insertion_anchor_step_id
            && !macro_item.steps.iter().any(|step| {
                step.id == anchor
                    && target.insertion_anchor_generation.is_none_or(|generation| {
                        self.step_instance_generations
                            .get(&(target.macro_id, step.id))
                            .copied()
                            == Some(generation)
                    })
            })
        {
            return Some(
                "The original insertion step no longer exists. Choose Append to end or cancel."
                    .to_owned(),
            );
        }
        None
    }

    pub fn apply_recording_review(&mut self) -> Result<Vec<u64>, String> {
        if let Some(blocker) = self.recording_review_apply_blocker() {
            return Err(blocker);
        }
        self.stop_owned_recording_preview();
        let review = self
            .recording_review
            .as_ref()
            .ok_or("No recording is awaiting review")?;
        let target = review.effective_target();
        let plan = review.proposal_plan();
        let macro_index = self
            .draft
            .macros
            .iter()
            .position(|item| item.id == target.macro_id)
            .ok_or(
                "The recording target macro no longer exists. Choose a target macro or cancel.",
            )?;
        let fragment = crate::mkmacro::materialize_plan(&plan, 0);
        let before = self.draft.macros[macro_index].steps.clone();
        let anchor = target.insertion_anchor_step_id.map_or(
            crate::mkmacro::editor_mutation::InsertionAnchor::End,
            crate::mkmacro::editor_mutation::InsertionAnchor::After,
        );
        let inserted = crate::mkmacro::editor_mutation::insert_fragment(
            &mut self.draft.macros[macro_index].steps,
            &fragment,
            anchor,
        )
        .map_err(|error| error.to_string())?;
        let ids = inserted.inserted_ids;
        let after = self.draft.macros[macro_index].steps.clone();
        if self.selected_macro_id == Some(target.macro_id) {
            self.selection.replace(ids.iter().copied());
        }
        self.mark_dirty();
        self.recording_insert_undo.push(RecordingInsertTransaction {
            macro_id: target.macro_id,
            before,
            after,
        });
        self.recording_insert_redo.clear();
        if self.action_editor.review_editing_id().is_some() {
            self.action_editor.cancel();
        }
        if let Some(mut review) = self.recording_review.take() {
            review.clear_sensitive();
        }
        self.recording_review = self.queued_recording_reviews.pop_front();
        self.publish_recording_target();
        Ok(ids)
    }

    pub fn cancel_recording_review(&mut self) {
        self.stop_owned_recording_preview();
        if self.action_editor.review_editing_id().is_some() {
            self.action_editor.cancel();
        }
        if let Some(mut review) = self.recording_review.take() {
            review.clear_sensitive();
        }
        self.recording_review = self.queued_recording_reviews.pop_front();
        self.publish_recording_target();
    }

    fn discard_all_recording_reviews(&mut self) {
        self.stop_owned_recording_preview();
        if self.action_editor.review_editing_id().is_some() {
            self.action_editor.cancel();
        }
        if let Some(mut review) = self.recording_review.take() {
            review.clear_sensitive();
        }
        for mut review in self.queued_recording_reviews.drain(..) {
            review.clear_sensitive();
        }
        self.publish_recording_target();
    }

    pub fn can_undo_recording_insert(&self) -> bool {
        self.recording_insert_undo
            .last()
            .is_some_and(|transaction| {
                self.draft
                    .macros
                    .iter()
                    .any(|item| item.id == transaction.macro_id && item.steps == transaction.after)
            })
    }

    pub fn can_redo_recording_insert(&self) -> bool {
        self.recording_insert_redo
            .last()
            .is_some_and(|transaction| {
                self.draft
                    .macros
                    .iter()
                    .any(|item| item.id == transaction.macro_id && item.steps == transaction.before)
            })
    }

    pub fn undo_last_recording_insert(&mut self) -> Result<(), String> {
        let transaction = self
            .recording_insert_undo
            .pop()
            .ok_or("No recording insertion is available to undo")?;
        let Some(target) = self
            .draft
            .macros
            .iter()
            .position(|item| item.id == transaction.macro_id)
        else {
            self.recording_insert_undo.push(transaction);
            return Err("The recording target macro no longer exists".into());
        };
        if self.draft.macros[target].steps != transaction.after {
            self.recording_insert_undo.push(transaction);
            return Err(
                "The inserted recording changed and cannot be undone as one transaction".into(),
            );
        }
        self.draft.macros[target].steps = transaction.before.clone();
        self.selection.clear();
        self.mark_dirty();
        self.recording_insert_redo.push(transaction);
        Ok(())
    }

    pub fn redo_last_recording_insert(&mut self) -> Result<(), String> {
        let transaction = self
            .recording_insert_redo
            .pop()
            .ok_or("No recording insertion is available to redo")?;
        let Some(target) = self
            .draft
            .macros
            .iter()
            .position(|item| item.id == transaction.macro_id)
        else {
            self.recording_insert_redo.push(transaction);
            return Err("The recording target macro no longer exists".into());
        };
        if self.draft.macros[target].steps != transaction.before {
            self.recording_insert_redo.push(transaction);
            return Err(
                "The macro changed after undo; the recording transaction cannot be redone safely"
                    .into(),
            );
        }
        self.draft.macros[target].steps = transaction.after.clone();
        self.selection.clear();
        self.mark_dirty();
        self.recording_insert_undo.push(transaction);
        Ok(())
    }
    pub fn selected_macro(&self) -> Option<&MkMacro> {
        let id = self.selected_macro_id?;
        self.draft.macros.iter().find(|m| m.id == id)
    }
    pub fn set_selected_macro(&mut self, id: Option<u64>) {
        let next_id = id.filter(|id| self.draft.macros.iter().any(|m| m.id == *id));
        if self.selected_macro_id != next_id {
            self.selection.clear();
            self.editor_state.drag = None;
            self.editor_state.scroll_to = None;
            self.navigation.focus_table = false;
            self.cancel_property_hotkey_captures();
        }
        self.selected_macro_id = next_id;
        self.publish_recording_target();
    }
    pub(crate) fn recording_target(&self) -> Option<crate::mkmacro::RecordingTarget> {
        if self.recording_review.is_some() {
            return None;
        }
        let macro_id = self.selected_macro_id?;
        let insertion_anchor_step_id = self.selection.primary.filter(|step_id| {
            self.selected_macro()
                .is_some_and(|m| m.steps.iter().any(|step| step.id == *step_id))
        });
        if let Some(cached) = *self.recording_target_cache.borrow()
            && cached.macro_id == macro_id
            && cached.anchor_id == insertion_anchor_step_id
            && cached.draft_revision == self.draft_revision
        {
            return Some(cached.target);
        }
        let target = crate::mkmacro::RecordingTarget {
            macro_id,
            insertion_anchor_step_id,
            insertion_anchor_generation: insertion_anchor_step_id.and_then(|step_id| {
                self.step_instance_generations
                    .get(&(macro_id, step_id))
                    .copied()
            }),
        };
        *self.recording_target_cache.borrow_mut() = Some(RecordingTargetCache {
            macro_id,
            anchor_id: insertion_anchor_step_id,
            draft_revision: self.draft_revision,
            target,
        });
        Some(target)
    }
    fn publish_recording_target(&mut self) {
        if self.defer_host_effects {
            self.pending_recording_target_publish = true;
            return;
        }
        self.publish_recording_target_now();
    }

    fn publish_recording_target_now(&self) {
        crate::mkmacro::runtime::arm_recording(
            self.recording_target(),
            NormalizationConfig::from(&self.draft.settings.recorder),
        );
    }
    pub fn selected_macro_mut(&mut self) -> Option<&mut MkMacro> {
        let id = self.selected_macro_id?;
        self.draft.macros.iter_mut().find(|m| m.id == id)
    }
    pub fn create_macro(&mut self) {
        self.create_macro_in_folder(None);
    }
    /// Create and select a macro, rejecting invalid destinations before changing the draft.
    pub fn create_macro_in_folder(&mut self, folder_id: Option<u64>) -> bool {
        if folder_id
            .is_some_and(|id| id == 0 || !self.draft.folders.iter().any(|folder| folder.id == id))
        {
            return false;
        }
        self.draft.macros.push(MkMacro {
            signature: Default::default(),
            id: 0,
            name: "New Macro".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id,
            playback: Default::default(),
            steps: vec![],
        });
        repair_ids(&mut self.draft);
        self.set_selected_macro(self.draft.macros.last().map(|m| m.id));
        self.selection.clear();
        self.mark_dirty();
        true
    }
    /// Clone in the same folder and append in global document order, with fresh IDs
    /// and no hotkey on the copy. The source macro remains unchanged.
    pub fn duplicate_selected_macro(&mut self) {
        let Some(source) = self.selected_macro() else {
            return;
        };
        let mut copy = source.clone();
        copy.id = 0;
        copy.name.push_str(" Copy");
        copy.hotkey = None;
        match crate::mkmacro::editor_mutation::clone_fragment(&source.steps, &[]) {
            Ok(fragment) => copy.steps = fragment.steps,
            Err(error) => {
                self.command_error = Some(error.to_string());
                return;
            }
        }
        debug_assert_eq!(
            copy.folder_id, source.folder_id,
            "cloning preserves folder membership"
        );
        self.draft.macros.push(copy);
        repair_ids(&mut self.draft);
        self.set_selected_macro(self.draft.macros.last().map(|m| m.id));
        self.selection.clear();
        self.mark_dirty();
    }
    pub fn request_delete_selected_macro(&mut self) {
        if self.selected_macro().is_some() {
            self.delete_confirmation
                .open_for(DestructiveAction::DeleteMacro);
        }
    }
    pub fn delete_selected_macro(&mut self) {
        let Some(id) = self.selected_macro_id else {
            return;
        };
        let Some(index) = self.draft.macros.iter().position(|m| m.id == id) else {
            return;
        };
        // Child editor requests capture the selected macro ID. Discard them
        // before that target disappears so they cannot later apply to a stale
        // draft (especially when the final macro is removed).
        self.action_editor.cancel();
        self.launcher_action_picker.cancel();
        self.package_ui.close();
        self.draft.macros.remove(index);
        let selected = self
            .draft
            .macros
            .get(index)
            .or_else(|| index.checked_sub(1).and_then(|i| self.draft.macros.get(i)))
            .map(|m| m.id);
        self.set_selected_macro(selected);
        self.selection.clear();
        self.mark_dirty();
    }
    pub fn playback_block_reason(&self) -> Option<String> {
        let Some(m) = self.selected_macro() else {
            return Some("Select a macro".into());
        };
        if !m.enabled {
            return Some("The selected macro is disabled".into());
        }
        if self.recording_review.is_some() {
            return Some("Close Recording Review before running a saved macro".into());
        }
        if crate::mkmacro::runtime::recorder_snapshot()
            .is_some_and(|snapshot| snapshot.state != crate::mkmacro::RecorderRuntimeState::Idle)
            || crate::mkmacro::runtime::record_stop_pending()
        {
            return Some("Recording is active".into());
        }
        if self.host_runtime_snapshot().is_some_and(|s| {
            matches!(
                s.state,
                crate::mkmacro::RuntimeState::Running
                    | crate::mkmacro::RuntimeState::Paused
                    | crate::mkmacro::RuntimeState::Stopping
            )
        }) {
            return Some("Another playback operation is active".into());
        }
        self.analysis_cache
            .borrow_mut()
            .root_failure(&self.draft, self.draft_revision, m.id)
    }

    fn prepare_execution(&mut self) -> anyhow::Result<u64> {
        if self.dirty {
            self.save()?;
        }
        self.selected_macro_id
            .ok_or_else(|| anyhow::anyhow!("Select a macro"))
    }

    fn prepare_execution_checked(&mut self) -> anyhow::Result<u64> {
        self.refresh_environment();
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        self.prepare_execution()
    }

    fn step_position(&self, step_id: u64) -> anyhow::Result<usize> {
        self.selected_macro()
            .and_then(|m| m.steps.iter().position(|s| s.id == step_id))
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists"))
    }

    fn step_id_at_position(&self, position: usize) -> anyhow::Result<u64> {
        self.selected_macro()
            .and_then(|m| m.steps.get(position))
            .map(|step| step.id)
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists after save"))
    }

    fn step_ids_at_positions(&self, positions: &[usize]) -> Vec<u64> {
        positions
            .iter()
            .filter_map(|position| {
                self.selected_macro()
                    .and_then(|m| m.steps.get(*position))
                    .map(|step| step.id)
            })
            .collect()
    }

    fn prepare_from_step(&mut self, original_step_id: u64) -> anyhow::Result<(u64, u64)> {
        self.refresh_environment();
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let position = self.step_position(original_step_id)?;
        let macro_id = self.prepare_execution()?;
        let step_id = self.step_id_at_position(position)?;
        Ok((macro_id, step_id))
    }

    fn selected_step_positions(&self) -> anyhow::Result<Vec<usize>> {
        let positions = self
            .selected_macro()
            .map(|m| {
                m.steps
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| self.selection.ids.contains(&s.id).then_some(i))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if positions.is_empty() {
            anyhow::bail!("Select one or more steps");
        }
        Ok(positions)
    }

    fn prepare_selected_steps(&mut self) -> anyhow::Result<(u64, Vec<u64>)> {
        self.refresh_environment();
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let positions = self.selected_step_positions()?;
        let macro_id = self.prepare_execution()?;
        let step_ids = self.step_ids_at_positions(&positions);
        Ok((macro_id, step_ids))
    }

    pub fn run_selected_macro(&mut self) -> anyhow::Result<()> {
        let id = self.prepare_execution_checked()?;
        self.submit_host_runtime_command(crate::mkmacro::runtime::RuntimeCommand::Run(id))
    }
    pub fn run_from_step(&mut self, original_step_id: u64) -> anyhow::Result<()> {
        let (id, step) = self.prepare_from_step(original_step_id)?;
        self.submit_host_runtime_command(crate::mkmacro::runtime::RuntimeCommand::RunFrom(id, step))
    }
    pub fn run_selected_steps(&mut self) -> anyhow::Result<()> {
        let (id, ids) = self.prepare_selected_steps()?;
        self.submit_host_runtime_command(crate::mkmacro::runtime::RuntimeCommand::RunSelection(
            id, ids,
        ))
    }
    pub fn debug_selected_macro(&mut self) -> anyhow::Result<()> {
        let id = self.prepare_execution_checked()?;
        self.submit_host_runtime_command(crate::mkmacro::runtime::RuntimeCommand::DebugRun(id))
    }
    pub fn debug_from_step(&mut self, original_step_id: u64) -> anyhow::Result<()> {
        let (id, step) = self.prepare_from_step(original_step_id)?;
        self.submit_host_runtime_command(crate::mkmacro::runtime::RuntimeCommand::DebugRunFrom(
            id, step,
        ))
    }
    pub fn debug_selected_steps(&mut self) -> anyhow::Result<()> {
        let (id, ids) = self.prepare_selected_steps()?;
        self.submit_host_runtime_command(
            crate::mkmacro::runtime::RuntimeCommand::DebugRunSelection(id, ids),
        )
    }

    fn observe_runtime_snapshot(&mut self, snapshot: Option<Arc<crate::mkmacro::RuntimeSnapshot>>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let run_identity = (snapshot.run_mode, snapshot.run_id);
        let new_run = self.runtime_inspector_observed_run != Some(run_identity);
        self.runtime_inspector_observed_run = Some(run_identity);
        match snapshot.run_mode {
            crate::mkmacro::RuntimeRunMode::Normal => {
                if new_run && self.runtime_inspector_snapshot.is_some() {
                    self.runtime_inspector_snapshot = None;
                    self.runtime_inspector_is_current_debug_run = false;
                    self.runtime_inspector_open = false;
                    self.runtime_inspector_active_breakpoint = None;
                }
            }
            crate::mkmacro::RuntimeRunMode::Debug => {
                let same_debug_run = self.runtime_inspector_snapshot.as_ref().is_some_and(|old| {
                    old.run_mode == crate::mkmacro::RuntimeRunMode::Debug
                        && old.run_id == snapshot.run_id
                });
                if !same_debug_run {
                    self.runtime_inspector_active_breakpoint = None;
                }
                // Retain the whole immutable runtime snapshot. In particular,
                // this preserves terminal variables, outcomes, and diagnostics
                // after the worker has moved on to unrelated UI repainting.
                self.runtime_inspector_snapshot = Some(snapshot.clone());
                self.runtime_inspector_is_current_debug_run = matches!(
                    snapshot.state,
                    crate::mkmacro::RuntimeState::Running
                        | crate::mkmacro::RuntimeState::Paused
                        | crate::mkmacro::RuntimeState::Stopping
                );
                let breakpoint = snapshot.breakpoint_occurrence();
                if let Some(breakpoint) = breakpoint {
                    if self.runtime_inspector_active_breakpoint != Some(breakpoint) {
                        self.runtime_inspector_open = true;
                        self.runtime_inspector_active_breakpoint = Some(breakpoint);
                    }
                } else {
                    self.runtime_inspector_active_breakpoint = None;
                }
            }
        }
    }

    pub fn ui(&mut self, ctx: &eframe::egui::Context) {
        self.ui_with_jump_mode(ctx, None);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &eframe::egui::Context,
        jump_mode_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        self.sync_external();
        self.ui_with_jump_mode_synced(ctx, jump_mode_scope);
    }

    pub(crate) fn ui_with_jump_mode_synced(
        &mut self,
        ctx: &eframe::egui::Context,
        jump_mode_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if !self.open {
            self.close_children();
            self.set_selected_macro(None);
            return;
        }
        let mut open = self.open;
        let _ = self.show_main_window_with_jump_mode(ctx, &mut open, jump_mode_scope.as_ref());
        self.open = open;
        if !self.open {
            self.close_children();
            self.set_selected_macro(None);
        }
    }
    fn show_main_window(
        &mut self,
        ctx: &eframe::egui::Context,
        open: &mut bool,
    ) -> Option<eframe::egui::Rect> {
        self.show_main_window_with_jump_mode(ctx, open, None)
    }

    fn show_main_window_with_jump_mode(
        &mut self,
        ctx: &eframe::egui::Context,
        open: &mut bool,
        jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) -> Option<eframe::egui::Rect> {
        let mut table_viewport = None;
        let viewport_size = ctx.viewport_rect().size();
        let max_size = [viewport_size.x.max(1.0), viewport_size.y.max(1.0)];
        let default_size = [920.0_f32.min(max_size[0]), 620.0_f32.min(max_size[1])];
        let min_size = [680.0_f32.min(max_size[0]), 420.0_f32.min(max_size[1])];
        // Manual regression: resize this window, add 100 steps, expand/edit rows,
        // and delete them again. Its chosen outer height must remain unchanged.
        let _window_output = eframe::egui::Window::new("Mouse/Keyboard Macros")
            .id(eframe::egui::Id::new("mkmacro_main_window"))
            .default_size(default_size)
            .min_size(min_size)
            .max_size(max_size)
            .resizable(true)
            .show_utility(
                ctx,
                "mkmacro",
                eframe::egui::Id::new("mkmacro_main_window"),
                Some(open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                    crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                        eframe::egui::Vec2::from(default_size),
                    ),
                    eframe::egui::Vec2::ZERO,
                    [None, None],
                )
                .with_minimum(eframe::egui::Vec2::from(min_size))
                .with_maximum(eframe::egui::Vec2::from(max_size)),
                |ui| {
                    table_viewport = self.show_contents_with_table_viewport(ui, jump_mode_scope);
                },
            );
        #[cfg(test)]
        {
            self.last_main_window_rect = _window_output.as_ref().map(|output| output.response.rect);
        }
        table_viewport
    }
    fn close_children(&mut self) {
        self.hotkey_capture = false;
        self.cancel_record_hotkey_capture();
        if self.annotation.open {
            let _ = self.finish_recording_annotation(false);
        }
        search::close(self);
        self.cancel_folder_operations();
        self.catalog.open = false;
        self.discard_all_recording_reviews();
        self.action_editor.cancel();
        self.image_crop_editor.cancel();
        self.structural_insertion = None;
        self.window_picker
            .cancel("Window picker closed because the macro dialog closed");
        self.launcher_action_picker.cancel();
        self.package_ui.close();
        let stored = self.store.snapshot();
        crate::mkmacro::runtime::arm_recording(
            None,
            NormalizationConfig::from(&stored.settings.recorder),
        );
    }
    pub fn show_contents(&mut self, ui: &mut eframe::egui::Ui) {
        let _ = self.show_contents_with_table_viewport(ui, None);
    }
    fn show_contents_with_table_viewport(
        &mut self,
        ui: &mut eframe::egui::Ui,
        jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) -> Option<eframe::egui::Rect> {
        #[cfg(test)]
        let body_assigned = ui.max_rect();
        #[cfg(test)]
        {
            self.last_main_body_geometry = None;
        }
        search::shortcuts(ui.ctx(), self);
        self.publish_recording_target();
        self.observe_runtime_snapshot(self.host_runtime_snapshot());
        self.drain_pending_recording_reviews();
        let mut jump_visual_order = 0;
        toolbar::show_with_jump_mode(ui, self, jump_mode_scope, &mut jump_visual_order);
        #[cfg(test)]
        let body_after_toolbar = ui.min_rect();
        let mut table_viewport = None;
        if self.draft.macros.is_empty() && self.draft.folders.is_empty() {
            let properties_height = (ui.available_height() - 100.0).max(0.0);
            eframe::egui::ScrollArea::vertical()
                .id_salt("mkmacro_empty_macro_properties_scroll")
                .auto_shrink([false, true])
                .max_height(properties_height)
                .show(ui, |ui| {
                    macro_properties::show_with_jump_mode(
                        ui,
                        self,
                        jump_mode_scope,
                        &mut jump_visual_order,
                    )
                });
            ui.separator();
            let body_size = ui.available_size();
            ui.allocate_ui_with_layout(
                body_size,
                eframe::egui::Layout::top_down(eframe::egui::Align::Center),
                |ui| macro_list::show_empty(ui, self),
            );
        } else {
            #[cfg(test)]
            let mut strip_cells = [None; 3];
            let closed_outline_width = outline::closed_rail_width(ui);
            egui_extras::StripBuilder::new(ui)
                .size(egui_extras::Size::exact(macro_list::SIDEBAR_WIDTH))
                .size(egui_extras::Size::remainder())
                .size(egui_extras::Size::exact(if self.navigation.outline.open {
                    250.0
                } else {
                    closed_outline_width
                }))
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        #[cfg(test)]
                        let assigned = ui.max_rect();
                        macro_list::show_with_jump_mode(
                            ui,
                            self,
                            jump_mode_scope,
                            &mut jump_visual_order,
                        );
                        #[cfg(test)]
                        {
                            strip_cells[0] = Some(StripCellGeometry {
                                assigned,
                                used: ui.min_rect(),
                            });
                        }
                    });
                    strip.cell(|ui| {
                        #[cfg(test)]
                        let assigned = ui.max_rect();
                        let available_height = ui.available_height();
                        let step_layout = step_table::StepPaneLayout::new(self);
                        let table_height = step_table::minimum_viewport_height(ui, &step_layout);
                        let header_height = runtime_inspector::header_height(ui);
                        let spacing = ui.spacing().item_spacing.y;
                        let separator_height = ui
                            .style()
                            .separator_style(
                                &eframe::egui::widget_style::Classes::default(),
                                eframe::egui::widget_style::WidgetState::Inactive,
                            )
                            .spacing;
                        let reserved_gaps = 4.0 * spacing + 2.0 * separator_height;
                        let inspector_body_height = if self.runtime_inspector_open {
                            (available_height - table_height - header_height - reserved_gaps)
                                .clamp(0.0, runtime_inspector::inspector_body_height())
                        } else {
                            0.0
                        };
                        let inspector_height = runtime_inspector::reserved_height(
                            ui,
                            self.runtime_inspector_open,
                            inspector_body_height,
                        );
                        let properties_height =
                            (available_height - table_height - inspector_height - reserved_gaps)
                                .max(0.0);
                        eframe::egui::ScrollArea::vertical()
                            .id_salt("mkmacro_macro_properties_scroll")
                            .auto_shrink([false, true])
                            .min_scrolled_height(0.0)
                            .max_height(properties_height)
                            .show(ui, |ui| {
                                macro_properties::show_with_jump_mode(
                                    ui,
                                    self,
                                    jump_mode_scope,
                                    &mut jump_visual_order,
                                )
                            });
                        ui.separator();
                        runtime_inspector::show(ui, self, inspector_body_height);
                        ui.separator();
                        table_viewport = step_table::show_with_layout_and_jump_mode(
                            ui,
                            self,
                            &step_layout,
                            jump_mode_scope,
                            &mut jump_visual_order,
                        );
                        #[cfg(test)]
                        {
                            strip_cells[1] = Some(StripCellGeometry {
                                assigned,
                                used: ui.min_rect(),
                            });
                        }
                    });
                    strip.cell(|ui| {
                        #[cfg(test)]
                        let assigned = ui.max_rect();
                        outline::show(ui, self);
                        #[cfg(test)]
                        {
                            strip_cells[2] = Some(StripCellGeometry {
                                assigned,
                                used: ui.min_rect(),
                            });
                        }
                    });
                });
            #[cfg(test)]
            {
                self.last_main_body_geometry = Some(MainBodyGeometry {
                    assigned: body_assigned,
                    after_toolbar: body_after_toolbar,
                    after_strip: ui.min_rect(),
                    strip_cells,
                });
            }
        }
        package_ui::show(ui.ctx(), self);
        if self.delete_confirmation.ui(ui.ctx()) == ConfirmationResult::Confirmed {
            self.delete_selected_macro();
        }
        let folder_delete_result = self.folder_delete_confirmation.ui(ui.ctx());
        self.handle_folder_delete_confirmation(folder_delete_result);
        match self.unwrap_confirmation.ui(ui.ctx()) {
            ConfirmationResult::Confirmed => {
                if let Some(id) = self.pending_unwrap_block.take() {
                    self.pending_unwrap_selection = None;
                    step_table::apply_confirmed_unwrap(self, id);
                }
            }
            ConfirmationResult::Cancelled => {
                self.pending_unwrap_block = None;
                if let Some(selection) = self.pending_unwrap_selection.take() {
                    self.selection = selection;
                }
            }
            ConfirmationResult::None => {}
        }
        table_viewport
    }
}
