//! Stable-ID radial menu authoring window.

mod asset_picker;
mod audio_controls;
mod canvas;
mod import_export;
mod preview;
mod skin_editor;
mod work_area;

use crate::gui::LauncherApp;
use crate::radial::acceptance_trace::{
    self, BodyBlock, Correlation, Event, FocusEdge, RequestKind, ViewportClass, WidgetCategory,
    WidgetResponse,
};
use crate::radial::authoring::menu::{self, ResizeResolution, SubmenuDuplication};
use crate::radial::authoring::{
    AuthoringClient, AuthoringError, AuthoringSnapshot, CloseIntent, CommitDisposition,
    ConflictResolution, DiskSha256, EditKey, EditPhase, PendingRequestKind, RadialAuthoringSession,
    StableSelection,
};
use crate::radial::context::InvocationContext;
use crate::radial::model::{
    AfterActionPolicy, CellContent, CellId, Control, DynamicSource, InteractionMode, LayoutKind,
    MenuId, RadialDocument, RingId, SubmenuPresentation,
};
use canvas::{
    CanvasPoint, CanvasTransform, DesignerMode, DragPayload, PlacementDraft, PreferenceDebounce,
    ProjectedCellProvenance, ProjectedSelection, VisitedMenuPath, WindowGeometry,
};
use eframe::egui;
use import_export::PendingImport;
use preview::{EmbeddedPreview, PreviewPreset};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) fn radial_designer_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("radial-designer")
}

fn trace_viewport_class(class: egui::ViewportClass) -> ViewportClass {
    match class {
        egui::ViewportClass::Root => ViewportClass::Root,
        egui::ViewportClass::Deferred => ViewportClass::Deferred,
        egui::ViewportClass::Immediate => ViewportClass::Immediate,
        egui::ViewportClass::Embedded => ViewportClass::Embedded,
    }
}

fn trace_request_kind(kind: PendingRequestKind) -> RequestKind {
    match kind {
        PendingRequestKind::Snapshot => RequestKind::Snapshot,
        PendingRequestKind::Commit(CommitDisposition::Apply) => RequestKind::CommitApply,
        PendingRequestKind::Commit(CommitDisposition::Save) => RequestKind::CommitSave,
        PendingRequestKind::Commit(CommitDisposition::RevertAppliedAndClose) => {
            RequestKind::CommitRevertAndClose
        }
        PendingRequestKind::LivePreview => RequestKind::LivePreview,
        PendingRequestKind::CancelPreview => RequestKind::CancelPreview,
        PendingRequestKind::ExportPackage => RequestKind::ExportPackage,
        PendingRequestKind::ExportSkin => RequestKind::ExportSkin,
        PendingRequestKind::AuditionManagedAsset => RequestKind::AuditionManagedAsset,
        PendingRequestKind::FontCatalog => RequestKind::FontCatalog,
        PendingRequestKind::PrepareEmbeddedPreview => RequestKind::PrepareEmbeddedPreview,
        PendingRequestKind::ReplacePackage => RequestKind::ReplacePackage,
        PendingRequestKind::StartNativePreview => RequestKind::StartNativePreview,
        PendingRequestKind::UpdateNativePreview => RequestKind::UpdateNativePreview,
        PendingRequestKind::StopNativePreview => RequestKind::StopNativePreview,
    }
}

fn trace_correlation(session: Option<&RadialAuthoringSession>) -> Correlation {
    let Some(session) = session else {
        return Correlation::default();
    };
    let pending = session.pending_request.or(session.pending_native_preview);
    pending.map_or(
        Correlation {
            session_id: session.editor_session.0,
            generation: session.generation.0,
            terminal: false,
            ..Correlation::default()
        },
        |pending| Correlation {
            request_id: pending.id.0,
            request_kind: trace_request_kind(pending.kind),
            session_id: pending.editor_session.0,
            generation: pending.generation.0,
            terminal: false,
        },
    )
}

fn trace_pointer_release_response(
    ui: &egui::Ui,
    response: &egui::Response,
    category: WidgetCategory,
    accepted: bool,
    correlation: Correlation,
) {
    if acceptance_trace::enabled()
        && ui.input(|input| input.pointer.any_released())
        && response.hovered()
    {
        acceptance_trace::emit(Event::DesignerWidget {
            category,
            response: if accepted {
                WidgetResponse::Accepted
            } else {
                WidgetResponse::Rejected
            },
            correlation,
        });
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DesignerUiIntent {
    TestAction {
        binding: crate::radial::model::ActionBinding,
        invocation: InvocationContext,
        history_query: String,
    },
}

#[derive(Clone, Debug)]
enum DesignerFileDialogRequest {
    StyleMedia {
        scope: skin_editor::StyleScope,
        section: String,
        field: String,
        media_kind: crate::radial::model::MediaKind,
        managed: bool,
    },
    CellExternalIcon {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    ImportManaged {
        kind: crate::radial::model::MediaKind,
        label: String,
        extensions: Vec<String>,
    },
    PreviewPackage,
    ExportMenu {
        menu_id: MenuId,
    },
    ExportPackage {
        roots: Vec<MenuId>,
    },
    ExportSkin {
        skin_id: crate::radial::model::SkinId,
    },
    PreviewLegacy {
        source: crate::radial::compatibility::CompatibilitySource,
    },
    ReplaceBackup,
}

#[derive(Clone, Debug)]
enum DesignerFileDialogResult {
    Cancelled,
    File(std::path::PathBuf),
    Files(Vec<std::path::PathBuf>),
}

#[derive(Default)]
pub(crate) struct DesignerIntentBridge {
    intents: Mutex<VecDeque<DesignerUiIntent>>,
    /// Native dialogs are requested by the deferred viewport but must be
    /// opened by the root update after the editor lock is released.  Keeping
    /// this queue on the bridge makes the hand-off explicit and lets the
    /// enqueue side wake the root exactly once per accepted request.
    file_dialogs: Mutex<VecDeque<DesignerFileDialogRequest>>,
    file_dialog_pending: AtomicBool,
    /// A child viewport sets this bit once its debounced presentation state
    /// is ready.  The bit is consumed by the root persistence boundary, so a
    /// quiescent/hidden root still receives one explicit wake rather than
    /// relying on a raw-frame poll.
    preferences_ready: AtomicBool,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    viewport_wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl DesignerIntentBridge {
    fn push(&self, intent: DesignerUiIntent) {
        let accepted = if let Ok(mut intents) = self.intents.lock() {
            if intents.len() < 64 {
                intents.push_back(intent);
                true
            } else {
                false
            }
        } else {
            false
        };
        if !accepted {
            return;
        }
        let wake = self
            .wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn set_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = wake;
        }
    }

    fn set_viewport_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut slot) = self.viewport_wake.lock() {
            *slot = wake;
        }
    }

    fn wake_root(&self) {
        let wake = self
            .wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn wake_viewport(&self) {
        let wake = self
            .viewport_wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    fn finish_file_dialog(&self) {
        // The completed request changed Designer state, while any queued
        // request still belongs to the root because native dialogs must not
        // run from the deferred viewport callback.
        self.wake_viewport();
        if self.has_pending_file_dialog() {
            self.wake_root();
        }
    }

    fn enqueue_file_dialog(&self, request: DesignerFileDialogRequest) {
        let accepted = if let Ok(mut requests) = self.file_dialogs.lock() {
            // A stalled native dialog must not allow an unbounded queue to
            // grow behind the root update.
            if requests.len() < 8 {
                requests.push_back(request);
                self.file_dialog_pending.store(true, Ordering::Release);
                true
            } else {
                false
            }
        } else {
            false
        };
        if accepted {
            // Deliberately wake only after the queue lock has been released;
            // the root callback may synchronously inspect the bridge.
            self.wake_root();
        }
    }

    fn take_file_dialog(&self) -> Option<DesignerFileDialogRequest> {
        self.file_dialogs.lock().ok().and_then(|mut requests| {
            let request = requests.pop_front();
            if requests.is_empty() {
                // Update the flag while holding the same queue lock as the
                // pop.  An enqueue that races this operation then acquires
                // the lock afterwards and restores the pending bit instead
                // of having its wake silently lost.
                self.file_dialog_pending.store(false, Ordering::Release);
            }
            request
        })
    }

    pub(crate) fn has_pending_file_dialog(&self) -> bool {
        self.file_dialog_pending.load(Ordering::Acquire)
    }

    fn enqueue_preferences_ready(&self) {
        if !self.preferences_ready.swap(true, Ordering::AcqRel) {
            self.wake_root();
        }
    }

    fn take_preferences_ready(&self) -> bool {
        self.preferences_ready.swap(false, Ordering::AcqRel)
    }

    fn clear_preferences_ready(&self) {
        self.preferences_ready.store(false, Ordering::Release);
    }

    #[cfg(test)]
    fn has_pending_preferences(&self) -> bool {
        self.preferences_ready.load(Ordering::Acquire)
    }

    pub(crate) fn drain(&self) -> Vec<DesignerUiIntent> {
        self.intents
            .lock()
            .map(|mut intents| intents.drain(..).collect())
            .unwrap_or_default()
    }

    fn clear(&self) {
        if let Ok(mut intents) = self.intents.lock() {
            intents.clear();
        }
        if let Ok(mut requests) = self.file_dialogs.lock() {
            requests.clear();
            self.file_dialog_pending.store(false, Ordering::Release);
        }
        // Preference readiness belongs to the root persistence hand-off.  A
        // close may clear the request queues and unregister callbacks after
        // setting this bit; leave it intact until ROOT consumes the saved
        // presentation snapshot.
    }
}

#[derive(Clone)]
struct DesignerFrameContext {
    feature_defaults: crate::radial::model::RadialFeatureSettings,
    expected_diagnostics: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    action_catalog: crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot,
    require_confirm_destructive: bool,
    intent_bridge: Arc<DesignerIntentBridge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingCellDrop {
    source_menu: MenuId,
    source_ring: RingId,
    source_cell: CellId,
    destination_menu: MenuId,
    destination_ring: RingId,
    destination_cell: CellId,
    destination_index: usize,
    generation: crate::radial::authoring::DraftGeneration,
}

/// The popup is a multi-frame editor, not a one-frame command.  Keep all
/// editable values together with the generation and target from which they
/// were initialized so a delayed Apply can never merge stale fields into a
/// newer authoring draft.
#[derive(Clone, Debug, PartialEq)]
struct PropertiesDraft {
    target: StableSelection,
    generation: crate::radial::authoring::DraftGeneration,
    label: String,
    content_kind: u8,
    action_binding: Option<crate::radial::model::ActionBinding>,
    dynamic_source: u8,
    submenu_target: Option<MenuId>,
    icon_kind: u8,
    original_content: CellContent,
    original_icon: crate::radial::model::Override<crate::radial::model::MediaReference>,
}

impl PropertiesDraft {
    fn from_cell(
        target: StableSelection,
        generation: crate::radial::authoring::DraftGeneration,
        cell: &crate::radial::model::CellDefinition,
    ) -> Self {
        Self {
            target,
            generation,
            label: cell.label.clone(),
            content_kind: cell_content_kind_key(&cell.content),
            action_binding: match &cell.content {
                CellContent::Action { binding } => Some(binding.clone()),
                _ => None,
            },
            dynamic_source: match &cell.content {
                CellContent::Dynamic { source } => dynamic_source_key(source),
                _ => 0,
            },
            submenu_target: match &cell.content {
                CellContent::Submenu { menu_id } => Some(menu_id.clone()),
                _ => None,
            },
            icon_kind: match &cell.icon {
                crate::radial::model::Override::Inherit => 0,
                crate::radial::model::Override::Clear => 1,
                crate::radial::model::Override::Value(_) => 2,
            },
            original_content: cell.content.clone(),
            original_icon: cell.icon.clone(),
        }
    }

    #[cfg(test)]
    fn is_current(
        &self,
        target: &StableSelection,
        generation: crate::radial::authoring::DraftGeneration,
    ) -> bool {
        &self.target == target && self.generation == generation
    }
}

#[derive(Clone, Debug)]
enum EditorCommand {
    MoveCell {
        source_menu: MenuId,
        source_ring: RingId,
        cell: CellId,
        destination_menu: MenuId,
        destination_ring: RingId,
        index: usize,
        expected_generation: crate::radial::authoring::DraftGeneration,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResourceNoticeSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceNotice {
    severity: ResourceNoticeSeverity,
    message: String,
}

impl ResourceNotice {
    fn info(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Info,
            message: message.into(),
        }
    }
    fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Warning,
            message: message.into(),
        }
    }
    fn error(message: impl Into<String>) -> Self {
        Self {
            severity: ResourceNoticeSeverity::Error,
            message: message.into(),
        }
    }
}

fn show_resource_notice(ui: &mut egui::Ui, notice: &ResourceNotice) {
    let (color, prefix) = match notice.severity {
        ResourceNoticeSeverity::Info => (ui.visuals().text_color(), "Info"),
        ResourceNoticeSeverity::Warning => (ui.visuals().warn_fg_color, "Warning"),
        ResourceNoticeSeverity::Error => (ui.visuals().error_fg_color, "Error"),
    };
    ui.colored_label(color, format!("{prefix}: {}", notice.message));
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RadialDiagnosticDisplayModel {
    actionable: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    expected_layout: Vec<crate::radial::diagnostics::RadialDiagnostic>,
    omitted: Option<crate::radial::diagnostics::RadialDiagnosticOmission>,
}

fn radial_diagnostic_display_model(
    diagnostics: impl IntoIterator<Item = crate::radial::diagnostics::RadialDiagnostic>,
) -> RadialDiagnosticDisplayModel {
    let diagnostics = crate::radial::diagnostics::bound_diagnostics(
        diagnostics.into_iter().collect(),
        crate::radial::diagnostics::MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
        crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS,
    );
    let omitted = diagnostics
        .iter()
        .find_map(|diagnostic| diagnostic.omission());
    let mut model = RadialDiagnosticDisplayModel {
        omitted,
        ..Default::default()
    };
    for diagnostic in diagnostics {
        if diagnostic.omission().is_some() {
            continue;
        }
        if diagnostic.is_expected_layout() {
            model.expected_layout.push(diagnostic);
        } else {
            model.actionable.push(diagnostic);
        }
    }
    model
}

pub(super) fn show_radial_diagnostics<'a>(
    ui: &mut egui::Ui,
    diagnostics: impl IntoIterator<Item = &'a crate::radial::diagnostics::RadialDiagnostic>,
    show_expected_layout: bool,
    id_salt: impl std::hash::Hash,
) {
    let model = radial_diagnostic_display_model(diagnostics.into_iter().cloned());
    if !model.actionable.is_empty() || model.omitted.is_some_and(|omitted| omitted.actionable > 0) {
        egui::CollapsingHeader::new(format!(
            "Actionable radial diagnostics ({} shown)",
            model.actionable.len()
        ))
        .id_source(("radial-actionable-diagnostics", &id_salt))
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for diagnostic in &model.actionable {
                        let color = match diagnostic.severity {
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Info => {
                                ui.visuals().text_color()
                            }
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Warning => {
                                ui.visuals().warn_fg_color
                            }
                            crate::radial::diagnostics::RadialDiagnosticSeverity::Error => {
                                ui.visuals().error_fg_color
                            }
                        };
                        ui.colored_label(color, &diagnostic.message);
                    }
                });
            if let Some(omitted) = model.omitted.filter(|omitted| omitted.actionable > 0) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} additional diagnostics omitted", omitted.total),
                );
            }
        });
    }

    let expected_omitted = model.omitted.map_or(0, |omitted| omitted.expected_layout);
    if show_expected_layout && (!model.expected_layout.is_empty() || expected_omitted > 0) {
        egui::CollapsingHeader::new(format!(
            "Expected layout diagnostics ({} shown; {} omitted)",
            model.expected_layout.len(),
            expected_omitted,
        ))
        .id_source(("radial-expected-layout-diagnostics", &id_salt))
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for diagnostic in &model.expected_layout {
                        ui.small(format!("{}", diagnostic.message));
                    }
                });
        });
    }
}

pub(crate) struct RadialEditorState {
    pub(crate) open: bool,
    viewport_close_pending: bool,
    viewport_restore_pending: bool,
    viewport_focus_pending: bool,
    session: Option<RadialAuthoringSession>,
    client: Option<AuthoringClient>,
    preview: EmbeddedPreview,
    close_intent: CloseIntent,
    close_stop_attempted: bool,
    close_prompt: bool,
    delete_message: Option<String>,
    delete_ring_prompt: Option<(MenuId, RingId)>,
    resize_prompt: Option<menu::ResizePlan>,
    action_filter: String,
    submenu_name: String,
    submenu_link_target: Option<MenuId>,
    drag_source: Option<(
        MenuId,
        RingId,
        CellId,
        crate::radial::authoring::DraftGeneration,
    )>,
    post_render: Vec<EditorCommand>,
    preview_zoom: f32,
    preview_preset: PreviewPreset,
    sample_native_context: bool,
    pending_import: Option<PendingImport>,
    export_destination: Option<std::path::PathBuf>,
    replace_backup_path: Option<std::path::PathBuf>,
    replace_confirmed: bool,
    resource_notice: Option<ResourceNotice>,
    show_resources: bool,
    style_text_inputs: std::collections::BTreeMap<String, String>,
    ring_resize_drafts: std::collections::BTreeMap<(MenuId, RingId), usize>,
    focus_restore: Option<StableSelection>,
    preferences: crate::settings::RadialDesignerPreferences,
    preferences_dirty: bool,
    preference_debounce: PreferenceDebounce,
    preferences_flush_requested: bool,
    designer_mode: DesignerMode,
    tree_visible: bool,
    tree_widget_epoch: u64,
    inspector_visible: bool,
    tree_width: f32,
    inspector_width: f32,
    canvas_pan: CanvasPoint,
    pan_drag_start: Option<CanvasPoint>,
    visited_path: VisitedMenuPath,
    projected_selection: Option<ProjectedSelection>,
    drag_payload: Option<DragPayload>,
    placement_draft: Option<PlacementDraft>,
    pending_drop: Option<PendingCellDrop>,
    properties_popup: Option<StableSelection>,
    properties_draft: Option<PropertiesDraft>,
    intent_bridge: Arc<DesignerIntentBridge>,
}

impl Default for RadialEditorState {
    fn default() -> Self {
        Self {
            open: false,
            viewport_close_pending: false,
            viewport_restore_pending: true,
            viewport_focus_pending: false,
            session: None,
            client: None,
            preview: EmbeddedPreview::default(),
            close_intent: CloseIntent::None,
            close_stop_attempted: false,
            close_prompt: false,
            delete_message: None,
            delete_ring_prompt: None,
            resize_prompt: None,
            action_filter: String::new(),
            submenu_name: "New submenu".into(),
            submenu_link_target: None,
            drag_source: None,
            post_render: Vec::new(),
            preview_zoom: 1.0,
            preview_preset: PreviewPreset::Current,
            sample_native_context: false,
            pending_import: None,
            export_destination: None,
            replace_backup_path: None,
            replace_confirmed: false,
            resource_notice: None,
            show_resources: false,
            style_text_inputs: Default::default(),
            ring_resize_drafts: Default::default(),
            focus_restore: None,
            preferences: Default::default(),
            preferences_dirty: false,
            preference_debounce: PreferenceDebounce::default(),
            preferences_flush_requested: false,
            designer_mode: DesignerMode::Design,
            tree_visible: false,
            tree_widget_epoch: 0,
            inspector_visible: false,
            tree_width: 180.0,
            inspector_width: 300.0,
            canvas_pan: CanvasPoint::default(),
            pan_drag_start: None,
            visited_path: VisitedMenuPath::default(),
            projected_selection: None,
            drag_payload: None,
            placement_draft: None,
            pending_drop: None,
            properties_popup: None,
            properties_draft: None,
            intent_bridge: Arc::new(DesignerIntentBridge::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DesignerPaneLayout {
    tree_width: f32,
    canvas_width: f32,
    inspector_width: f32,
    height: f32,
}

/// Allocate every Designer pane from the actual remaining viewport.  Side
/// panes yield width before the canvas does, and the final widths always fit
/// within the caller's bounded rect (including compact 720px windows).
fn designer_pane_layout(
    available: egui::Vec2,
    tree_visible: bool,
    inspector_visible: bool,
    tree_width: f32,
    inspector_width: f32,
) -> DesignerPaneLayout {
    const GAP: f32 = 6.0;
    const MIN_CANVAS: f32 = 140.0;
    const MIN_TREE: f32 = 120.0;
    const MIN_INSPECTOR: f32 = 220.0;

    let width = if available.x.is_finite() {
        available.x.clamp(1.0, 10_000.0)
    } else {
        1.0
    };
    let height = if available.y.is_finite() {
        available.y.clamp(1.0, 10_000.0)
    } else {
        1.0
    };
    let gap_count = [tree_visible, inspector_visible]
        .into_iter()
        .filter(|visible| *visible)
        .count();
    let gaps = GAP * gap_count as f32;
    let mut tree = tree_visible
        .then(|| tree_width.clamp(MIN_TREE, 420.0))
        .unwrap_or(0.0);
    let mut inspector = inspector_visible
        .then(|| inspector_width.clamp(MIN_INSPECTOR, 560.0))
        .unwrap_or(0.0);
    let side_total = tree + inspector;
    let side_budget = (width - gaps - MIN_CANVAS).max(0.0);
    if side_total > side_budget && side_total > 0.0 {
        let ratio = side_budget / side_total;
        tree *= ratio;
        inspector *= ratio;
    }
    let canvas = (width - gaps - tree - inspector).max(1.0);
    DesignerPaneLayout {
        tree_width: tree,
        canvas_width: canvas,
        inspector_width: inspector,
        height,
    }
}

impl RadialEditorState {
    pub(crate) fn set_preferences(
        &mut self,
        preferences: crate::settings::RadialDesignerPreferences,
    ) {
        let previous = preferences.clone().normalized();
        let preferences = preferences.migrate_layout();
        self.designer_mode = match preferences.active_mode {
            crate::settings::RadialDesignerMode::Design => DesignerMode::Design,
            crate::settings::RadialDesignerMode::PreviewTest => DesignerMode::PreviewTest,
        };
        self.tree_visible = preferences.tree_visible;
        self.inspector_visible = preferences.inspector_visible;
        self.show_resources = preferences.show_skins;
        self.tree_width = preferences.tree_width;
        self.inspector_width = preferences.inspector_width;
        self.preview_zoom = preferences.zoom;
        self.canvas_pan = CanvasPoint::new(preferences.pan.0, preferences.pan.1);
        self.preferences = preferences.clone();
        // Persist a one-time legacy presentation upgrade, but do not mark a
        // normal startup dirty.  Only the presentation marker and pane
        // visibility can change here; the authoring draft is untouched.
        self.preferences_dirty = preferences != previous;
        self.preference_debounce.clear();
        if self.preferences_dirty {
            self.preference_debounce.mark_changed(Instant::now());
        }
        self.preferences_flush_requested = false;
        self.intent_bridge.clear_preferences_ready();
    }

    /// Restore only the Designer presentation.  The authoring session,
    /// selection, undo history, assets, and persisted radial document are
    /// intentionally outside this boundary.
    fn reset_designer_layout(&mut self) {
        let preferences = crate::settings::RadialDesignerPreferences::default();
        self.preferences = preferences.clone();
        self.designer_mode = match preferences.active_mode {
            crate::settings::RadialDesignerMode::Design => DesignerMode::Design,
            crate::settings::RadialDesignerMode::PreviewTest => DesignerMode::PreviewTest,
        };
        self.tree_visible = preferences.tree_visible;
        // Reset retained egui collapse state along with the presentation
        // preferences.  Ordinary preference loads must preserve explicit
        // expansion choices, so the widget namespace only changes here.
        self.tree_widget_epoch = self.tree_widget_epoch.wrapping_add(1);
        self.inspector_visible = preferences.inspector_visible;
        self.show_resources = preferences.show_skins;
        self.tree_width = preferences.tree_width;
        self.inspector_width = preferences.inspector_width;
        self.preview_zoom = preferences.zoom;
        self.canvas_pan = CanvasPoint::new(preferences.pan.0, preferences.pan.1);
        self.pan_drag_start = None;
        self.viewport_restore_pending = true;
        self.preview.cancel_tooltip();
        self.mark_preferences_changed();
    }

    fn mark_preferences_changed(&mut self) {
        self.preferences_dirty = true;
        self.preference_debounce.mark_changed(Instant::now());
        self.preferences.layout_version =
            crate::settings::RadialDesignerPreferences::CURRENT_LAYOUT_VERSION;
        self.preferences.tree_visible = self.tree_visible;
        self.preferences.inspector_visible = self.inspector_visible;
        self.preferences.show_skins = self.show_resources;
        self.preferences.tree_width = self.tree_width;
        self.preferences.inspector_width = self.inspector_width;
        self.preferences.zoom = self.preview_zoom;
        self.preferences.pan = (self.canvas_pan.x, self.canvas_pan.y);
        self.preferences.active_mode = match self.designer_mode {
            DesignerMode::Design => crate::settings::RadialDesignerMode::Design,
            DesignerMode::PreviewTest => crate::settings::RadialDesignerMode::PreviewTest,
        };
    }

    fn enqueue_preferences_ready_if_due(&self) {
        if self.preferences_dirty && self.preference_debounce.ready(Instant::now()) {
            self.intent_bridge.enqueue_preferences_ready();
        }
    }

    pub(crate) fn take_preferences_for_persist(
        &mut self,
    ) -> Option<crate::settings::RadialDesignerPreferences> {
        let signaled = self.intent_bridge.take_preferences_ready();
        let ready = signaled || self.preference_debounce.ready(Instant::now());
        (self.preferences_dirty && (ready || self.preferences_flush_requested)).then(|| {
            self.preferences_dirty = false;
            self.preference_debounce.clear();
            self.preferences_flush_requested = false;
            self.preferences.clone().normalized()
        })
    }

    pub(crate) fn intent_bridge(&self) -> Arc<DesignerIntentBridge> {
        Arc::clone(&self.intent_bridge)
    }

    /// Drain at most one native file dialog request.  The request is removed
    /// under the editor lock, the blocking native dialog is opened without
    /// that lock, and only then is the result applied to the session.  This is
    /// the ownership boundary that keeps import/export dialogs independent of
    /// the deferred viewport callback.
    pub(crate) fn process_pending_file_dialog(shared: &Arc<Mutex<Self>>) {
        let bridge = shared
            .lock()
            .ok()
            .map(|editor| Arc::clone(&editor.intent_bridge));
        let Some(bridge) = bridge else {
            return;
        };
        let request = bridge.take_file_dialog();
        let Some(request) = request else {
            return;
        };
        let result = open_designer_file_dialog(&request);
        if let Ok(mut editor) = shared.lock() {
            editor.apply_file_dialog_result(request, result);
        }
        // Dialog completion is an event for the independent viewport.  Wake
        // it only after the editor lock is released.
        bridge.finish_file_dialog();
    }

    fn apply_file_dialog_result(
        &mut self,
        request: DesignerFileDialogRequest,
        result: DesignerFileDialogResult,
    ) {
        let (file, files) = match result {
            DesignerFileDialogResult::Cancelled => (None, None),
            DesignerFileDialogResult::File(path) => (Some(path), None),
            DesignerFileDialogResult::Files(paths) => (None, Some(paths)),
        };
        match request {
            DesignerFileDialogRequest::StyleMedia {
                scope,
                section,
                field,
                media_kind,
                managed,
            } => {
                let Some(path) = file else { return };
                let selected =
                    asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, media_kind, managed,
                            )
                        });
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                match selected {
                    Ok(choice) => {
                        self.resource_notice = Some(if managed {
                            ResourceNotice::info(choice.portability_diagnostic())
                        } else {
                            ResourceNotice::warning(choice.portability_diagnostic())
                        });
                        if let Err(error) = skin_editor::set_media_override(
                            session, &scope, &section, &field, &choice,
                        ) {
                            self.resource_notice = Some(ResourceNotice::error(error));
                        }
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                    }
                }
            }
            DesignerFileDialogRequest::CellExternalIcon {
                menu_id,
                ring_id,
                cell_id,
            } => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                let mut document = (*session.draft).clone();
                let Some(cell) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter_mut().find(|cell| cell.id == cell_id))
                else {
                    return;
                };
                cell.icon = crate::radial::model::Override::Value(
                    crate::radial::model::MediaReference::ExternalFile {
                        path: path.display().to_string(),
                    },
                );
                if let Err(error) = session.replace_document_atomic(document) {
                    self.resource_notice = Some(ResourceNotice::error(format!("{error:?}")));
                }
            }
            DesignerFileDialogRequest::ImportManaged {
                kind,
                label,
                extensions: _,
            } => {
                let Some(path) = file else { return };
                let selected =
                    asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, kind, true,
                            )
                        })
                        .and_then(|choice| {
                            self.session
                                .as_mut()
                                .ok_or_else(|| "Radial authoring session unavailable".to_owned())
                                .and_then(|session| {
                                    asset_picker::add_resource(session, &choice).map(|_| choice)
                                })
                        });
                match selected {
                    Ok(choice) => {
                        self.resource_notice = Some(ResourceNotice::info(format!(
                            "{}: {}",
                            label,
                            choice.portability_diagnostic()
                        )));
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error));
                    }
                }
            }
            DesignerFileDialogRequest::PreviewPackage => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                self.pending_import = match asset_picker::read_bounded(
                    &path,
                    crate::radial::package::MAX_PACKAGE_COMPRESSED_BYTES,
                ) {
                    Ok(bytes) => match PendingImport::package(&bytes, session) {
                        Ok(preview) => Some(preview),
                        Err(error) => {
                            self.resource_notice = Some(ResourceNotice::error(error.to_string()));
                            None
                        }
                    },
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error.to_string()));
                        None
                    }
                };
            }
            DesignerFileDialogRequest::ExportMenu { menu_id } => {
                let Some(path) = file else { return };
                self.begin_export_package(vec![menu_id], path);
            }
            DesignerFileDialogRequest::ExportPackage { roots } => {
                let Some(path) = file else { return };
                self.begin_export_package(roots, path);
            }
            DesignerFileDialogRequest::ExportSkin { skin_id } => {
                let Some(path) = file else { return };
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                if session.is_dirty() {
                    self.resource_notice = Some(ResourceNotice::warning(
                        "Save or apply the draft before exporting persisted package bytes",
                    ));
                    return;
                }
                match session.request_export_skin(skin_id) {
                    Ok(request) => {
                        let pending = session.pending_request;
                        let result = self.client.as_ref().map_or_else(
                            || Err(AuthoringError::ServiceClosed),
                            |client| client.send(request),
                        );
                        match result {
                            Ok(()) => self.export_destination = Some(path),
                            Err(error) => {
                                if let Some(pending) = pending {
                                    session.reconcile_request_delivery_failure(
                                        pending,
                                        format!("{error:?}"),
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => session.last_error = Some(format!("{error:?}")),
                }
            }
            DesignerFileDialogRequest::PreviewLegacy { source } => {
                let Some(paths) = files else {
                    return;
                };
                let Some(session) = self.session.as_ref() else {
                    return;
                };
                match preview_legacy_files(&paths, source, session) {
                    Ok(preview) => self.pending_import = Some(preview),
                    Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                }
            }
            DesignerFileDialogRequest::ReplaceBackup => {
                let Some(path) = file else { return };
                self.replace_backup_path = Some(path);
            }
        }
    }

    fn begin_export_package(&mut self, roots: Vec<MenuId>, path: std::path::PathBuf) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.is_dirty() {
            self.resource_notice = Some(ResourceNotice::warning(
                "Save or apply the draft before exporting persisted package bytes",
            ));
            return;
        }
        match session.request_export_package(roots) {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                match result {
                    Ok(()) => self.export_destination = Some(path),
                    Err(error) => {
                        if let Some(pending) = pending {
                            session
                                .reconcile_request_delivery_failure(pending, format!("{error:?}"));
                        }
                    }
                }
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
        }
    }

    /// Register one stable deferred viewport.  The callback owns only the
    /// editor state and immutable service/catalog snapshots; it never borrows
    /// `LauncherApp` or the root egui frame.
    pub(crate) fn show_deferred(shared: &Arc<Mutex<Self>>, ctx: &egui::Context, app: &LauncherApp) {
        let (
            open,
            viewport_close_pending,
            viewport_restore_pending,
            preferences,
            action_catalog,
            feature_defaults,
            diagnostics,
            require_confirm,
        ) = match shared.lock() {
            Ok(editor) => (
                editor.open,
                editor.viewport_close_pending,
                editor.viewport_restore_pending,
                editor.preferences.clone().normalized(),
                app.universal_action_catalog_snapshot(),
                app.radial_feature_settings.clone(),
                app.radial_expected_diagnostics.iter().cloned().collect(),
                app.require_confirm_destructive,
            ),
            Err(_) => return,
        };
        if !open && !viewport_close_pending {
            return;
        }
        let viewport_id = radial_designer_viewport_id();
        let reply_ctx = ctx.clone();
        let reply_wake: Arc<dyn Fn() + Send + Sync> =
            Arc::new(move || reply_ctx.request_repaint_of(viewport_id));
        let intent_bridge = shared
            .lock()
            .map(|editor| Arc::clone(&editor.intent_bridge))
            .unwrap_or_else(|_| Arc::new(DesignerIntentBridge::default()));
        let root_ctx = ctx.clone();
        intent_bridge.set_wake(Some(Arc::new(move || {
            root_ctx.request_repaint_of(egui::ViewportId::ROOT);
        })));
        intent_bridge.set_viewport_wake(Some(Arc::clone(&reply_wake)));
        if let Ok(editor) = shared.lock()
            && let Some(client) = editor.client.as_ref()
        {
            client.set_reply_wake(Some(reply_wake));
        }
        let frame = DesignerFrameContext {
            feature_defaults,
            expected_diagnostics: diagnostics,
            action_catalog,
            require_confirm_destructive: require_confirm,
            intent_bridge,
        };
        let saved_geometry = WindowGeometry {
            position: preferences
                .window_position
                .map(|(x, y)| CanvasPoint::new(x, y)),
            size: CanvasPoint::new(preferences.window_size.0, preferences.window_size.1),
        };
        let saved_position_scale_factor = preferences.window_scale_factor;
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Radial Designer")
            .with_min_inner_size([520.0, 380.0])
            .with_resizable(true)
            .with_visible(open);
        if viewport_restore_pending {
            // The initial builder only supplies a bounded size.  Desktop
            // position and mixed-DPI normalization belong to the child
            // callback, where the Designer's own zoom factor is known.
            builder = builder.with_inner_size([saved_geometry.size.x, saved_geometry.size.y]);
        }
        let shared = Arc::clone(shared);
        ctx.show_viewport_deferred(viewport_id, builder, move |child, class| {
            let trace_viewport = trace_viewport_class(class);
            let (pointer_down, pointer_up) =
                child.input(|input| (input.pointer.any_pressed(), input.pointer.any_released()));
            let Ok(mut editor) = shared.lock() else {
                return;
            };
            let correlation = trace_correlation(editor.session.as_ref());
            let close_requested = class != egui::ViewportClass::Embedded
                && child.input(|input| input.viewport().close_requested());
            // Retained deferred callbacks can execute every frame.  Trace
            // only interaction/focus/close edges so the bounded diagnostic
            // budget remains useful for the complete manual sequence.
            let _callback_trace =
                (pointer_down || pointer_up || close_requested || editor.viewport_focus_pending)
                    .then(|| acceptance_trace::DesignerCallbackGuard::enter(trace_viewport));
            if close_requested {
                editor.request_close();
                // A dirty designer must keep its deferred viewport alive long
                // enough to show Save/Discard/Keep editing.  Clean close is
                // handled below by sending the actual Close command.
                if editor.open {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
            }
            if !editor.open {
                if class != egui::ViewportClass::Embedded {
                    child.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                editor.viewport_close_pending = false;
                return;
            }
            if class == egui::ViewportClass::Embedded {
                // Embedded callbacks are not an authoring surface.  Keep the
                // state alive for the caller to close/reopen, but never render
                // the authoritative editor into the root viewport.
                egui::Window::new("Radial Designer unavailable")
                    .collapsible(false)
                    .resizable(false)
                    .default_size(egui::vec2(380.0, 150.0))
                    .show(child, |ui| {
                        ui.label("Radial Designer requires an independent viewport.");
                        ui.small("This host only provides an embedded viewport.");
                    });
                return;
            }
            let focus_requested = std::mem::take(&mut editor.viewport_focus_pending);
            if focus_requested {
                acceptance_trace::emit(Event::DesignerFocus {
                    edge: FocusEdge::Requested,
                    viewport: trace_viewport,
                    correlation,
                });
            }
            if editor.viewport_restore_pending {
                let restored = work_area::restore_geometry(
                    child,
                    saved_geometry,
                    saved_position_scale_factor,
                    child.zoom_factor(),
                );
                child.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    restored.size.x,
                    restored.size.y,
                )));
                if let Some(position) = restored.position {
                    child.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        position.x, position.y,
                    )));
                }
                editor.viewport_restore_pending = false;
            }
            if focus_requested {
                child.send_viewport_cmd(egui::ViewportCommand::Focus);
                acceptance_trace::emit(Event::DesignerFocus {
                    edge: FocusEdge::Consumed,
                    viewport: trace_viewport,
                    correlation,
                });
            }
            editor.viewport_ui(child, &frame, class);
        });
    }

    pub(crate) fn open(&mut self) {
        if self.open {
            self.viewport_focus_pending = true;
            return;
        }
        self.open = true;
        self.viewport_close_pending = false;
        self.viewport_restore_pending = true;
        self.viewport_focus_pending = true;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.close_prompt = false;
        self.drag_source = None;
        self.post_render.clear();
        self.focus_restore = None;
        self.visited_path = VisitedMenuPath::new(RadialDocument::starter().default_menu_id);
        self.projected_selection = None;
        self.drag_payload = None;
        self.placement_draft = None;
        self.pending_drop = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.pan_drag_start = None;
        let document = RadialDocument::starter();
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot {
            revision: document.revision,
            document: std::sync::Arc::new(document),
            disk_sha256: DiskSha256(String::new()),
        });
        self.client = super::radial_authoring_client();
        if let Some(client) = &self.client {
            client.acquire_resources(session.editor_session());
            if let Ok(request) = session.request_snapshot() {
                let pending = session.pending_request;
                if let Err(error) = client.send(request)
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
        }
        self.session = Some(session);
    }

    /// Keep a pinned Designer open without treating each maintenance pass as
    /// an explicit user focus request.
    pub(crate) fn ensure_open(&mut self) {
        if !self.open {
            self.open();
        }
    }

    pub(crate) fn open_skins(&mut self) {
        self.open();
        self.show_resources = true;
        self.mark_preferences_changed();
    }

    pub(crate) fn open_menus(&mut self) {
        self.open();
        self.show_resources = false;
        self.mark_preferences_changed();
    }

    #[cfg(test)]
    pub(crate) fn is_showing_resources(&self) -> bool {
        self.show_resources
    }

    #[cfg(test)]
    pub(crate) fn is_dirty(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(RadialAuthoringSession::is_dirty)
    }

    #[cfg(test)]
    pub(crate) fn has_close_prompt(&self) -> bool {
        self.close_prompt
    }

    #[cfg(test)]
    pub(crate) fn make_dirty_for_test(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let id = session.draft.menus[0].id.clone();
        session
            .mutate(
                crate::radial::authoring::DocumentMutation::RenameMenu {
                    id,
                    name: "Unsaved test menu".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn open_test_snapshot(&mut self) {
        let document = RadialDocument::starter();
        self.open = true;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.client = None;
        self.properties_popup = None;
        self.properties_draft = None;
        self.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        )));
    }

    pub(crate) fn request_close(&mut self) {
        if self.close_intent == CloseIntent::Requested {
            return;
        }
        self.close_intent = CloseIntent::Requested;
        self.close_stop_attempted = false;
        if let Some(session) = self.session.as_mut() {
            session.request_close_intent();
        }
        self.preview.cancel_tooltip();
        // Window move/resize state must be persisted when close interaction
        // ends, even if the debounce interval has not elapsed.
        self.preferences_flush_requested = true;
        self.preference_debounce.flush();
        self.intent_bridge.enqueue_preferences_ready();
        self.intent_bridge.clear();
        self.placement_draft = None;
        self.pending_drop = None;
        self.properties_popup = None;
        self.properties_draft = None;
        // A preview preparation has no durable side effect and can be
        // cancelled locally.  This lets an OS close finish promptly without
        // leaving a late preparation reply attached to a hidden viewport.
        // Read-only/preview preparation can be invalidated locally.  Durable
        // requests remain pending until their correlated terminal reply.
        if let Some(pending) = self
            .session
            .as_ref()
            .and_then(|session| session.pending_request)
            .filter(|pending| pending.kind.is_disposable())
            && let Some(session) = self.session.as_mut()
        {
            session.cancel_pending_request_exact(pending);
        }
        if let Some(pending) = self
            .session
            .as_ref()
            .and_then(|session| session.pending_native_preview)
            && matches!(
                pending.kind,
                PendingRequestKind::StartNativePreview | PendingRequestKind::UpdateNativePreview
            )
        {
            // The request has already crossed the client boundary.  Queue a
            // correlated stop behind it instead of dropping the local slot:
            // the service will process Start/Update then Stop in order, and
            // the close path will wait for the terminal stop reply.
            self.stop_native_preview();
        }
        self.maybe_finish_close();
    }

    fn finish_close(&mut self) {
        self.preview.dispose();
        self.release_authoring_resources();
        self.open = false;
        self.viewport_close_pending = true;
        self.session = None;
        self.close_prompt = false;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
    }

    fn maybe_finish_close(&mut self) {
        if self.close_intent != CloseIntent::Requested {
            if self.close_intent != CloseIntent::DiscardRequested {
                return;
            }
        }
        let Some(session) = self.session.as_ref() else {
            self.open = false;
            self.viewport_close_pending = true;
            self.close_intent = CloseIntent::None;
            self.close_stop_attempted = false;
            return;
        };
        if session.is_closed() {
            self.finish_close();
            return;
        }
        // Commit/ReplacePackage and an in-flight native stop are terminal
        // work.  Do not dispose the callbacks or session before their exact
        // reply arrives.
        if session.pending_request.is_some() || session.pending_native_preview.is_some() {
            return;
        }
        let native_preview_active =
            session.native_preview_may_be_open || session.native_preview_lease.is_some();
        if native_preview_active && !self.close_stop_attempted {
            self.stop_native_preview();
            return;
        }
        if self.close_intent == CloseIntent::DiscardRequested {
            self.finish_close();
        } else if session.is_dirty() {
            self.close_prompt = true;
        } else {
            self.finish_close();
        }
    }

    pub(crate) fn force_close(&mut self) {
        self.preview.cancel_tooltip();
        self.preferences_flush_requested = true;
        self.preference_debounce.flush();
        self.intent_bridge.enqueue_preferences_ready();
        self.stop_native_preview();
        self.preview.dispose();
        self.release_authoring_resources();
        self.open = false;
        self.viewport_close_pending = true;
        self.viewport_restore_pending = true;
        self.viewport_focus_pending = false;
        self.session = None;
        self.close_intent = CloseIntent::None;
        self.close_stop_attempted = false;
        self.close_prompt = false;
        self.focus_restore = None;
        self.placement_draft = None;
        self.pending_drop = None;
        self.properties_popup = None;
        self.properties_draft = None;
    }

    fn send_commit(&mut self, disposition: CommitDisposition) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let mut delivery_failed = false;
        match session.request_commit(disposition) {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                    delivery_failed = true;
                }
            }
            Err(error) => {
                session.last_error = Some(format!("{error:?}"));
                delivery_failed = true;
            }
        }
        if delivery_failed && self.close_intent != CloseIntent::None {
            // Keep the close decision actionable after a failed Save or
            // Discard transaction instead of leaving the editor latched in a
            // non-prompting close state.
            self.close_prompt = true;
        }
    }

    fn poll_replies(&mut self) {
        let session_closed = {
            let Some(session) = self.session.as_mut() else {
                return;
            };
            if let Some(client) = &self.client {
                while let Some(reply) = client.try_recv() {
                    session.accept_reply(reply);
                }
            }
            if self.close_intent == CloseIntent::None
                && !session.font_catalog_loaded
                && session.pending_request.is_none()
            {
                match session.request_font_catalog() {
                    Ok(request) => {
                        let pending = session.pending_request;
                        let result = self.client.as_ref().map_or_else(
                            || Err(AuthoringError::ServiceClosed),
                            |client| client.send(request),
                        );
                        if let Err(error) = result
                            && let Some(pending) = pending
                        {
                            session
                                .reconcile_request_delivery_failure(pending, format!("{error:?}"));
                        }
                    }
                    Err(error) => session.last_error = Some(format!("{error:?}")),
                }
            }
            if let (Some(bytes), Some(path)) = (
                session.exported_package.take(),
                self.export_destination.take(),
            ) {
                match crate::common::atomic_file::save_atomic(&path, &bytes) {
                    Ok(()) => {
                        self.resource_notice = Some(ResourceNotice::info(format!(
                            "Exported package to {}",
                            path.display()
                        )))
                    }
                    Err(error) => {
                        self.resource_notice = Some(ResourceNotice::error(error.to_string()))
                    }
                }
            }
            session.is_closed()
        };
        if session_closed {
            self.finish_close();
        } else {
            self.maybe_finish_close();
        }
    }

    fn release_authoring_resources(&mut self) {
        if self.preferences_dirty {
            // Preserve the final close flush before unregistering the wake
            // callbacks.  ROOT can consume the bit after this viewport has
            // disposed its authoring resources.
            self.intent_bridge.enqueue_preferences_ready();
        }
        self.intent_bridge.set_wake(None);
        self.intent_bridge.set_viewport_wake(None);
        self.intent_bridge.clear();
        if let (Some(client), Some(session)) = (&self.client, &self.session) {
            client.set_reply_wake(None);
            client.release_resources(session.editor_session());
        }
        self.client = None;
    }

    fn send_native_preview(&mut self, update: bool) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let menu_id =
            selected_menu_id(session).unwrap_or_else(|| session.draft.default_menu_id.clone());
        let selected_skin = match session.selection.as_ref() {
            Some(StableSelection::Skin(id)) => Some(id.clone()),
            _ => None,
        };
        let request = if update {
            session.request_update_native_preview(
                menu_id,
                self.sample_native_context,
                selected_skin,
            )
        } else {
            session.request_start_native_preview(menu_id, self.sample_native_context, selected_skin)
        };
        match request {
            Ok(request) => {
                let pending = session.pending_native_preview;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
        }
    }

    fn stop_native_preview(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let request = session.request_stop_native_preview();
        match request {
            Ok(request) => {
                let pending = session.pending_native_preview;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if self.close_intent != CloseIntent::None {
                    self.close_stop_attempted = true;
                }
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                }
            }
            Err(error) => {
                if self.close_intent != CloseIntent::None {
                    self.close_stop_attempted = true;
                }
                session.last_error = Some(format!("{error:?}"));
            }
        }
    }

    fn sync_native_preview_generation(&mut self) {
        let needs_stop = self.session.as_ref().is_some_and(|session| {
            session.pending_native_preview.is_none()
                && session.native_preview_may_be_open
                && session.native_preview_lease.is_none()
        });
        if needs_stop {
            self.stop_native_preview();
            return;
        }
        let needs_update = self.session.as_ref().is_some_and(|session| {
            session.pending_native_preview.is_none()
                && session
                    .native_preview_lease
                    .as_ref()
                    .is_some_and(|lease| lease.generation != session.generation)
        });
        if needs_update {
            self.send_native_preview(true);
        }
    }

    fn viewport_ui(
        &mut self,
        ctx: &egui::Context,
        frame: &DesignerFrameContext,
        viewport_class: egui::ViewportClass,
    ) {
        if viewport_class == egui::ViewportClass::Embedded {
            return;
        }
        let (pointer_down, pointer_up) =
            ctx.input(|input| (input.pointer.any_pressed(), input.pointer.any_released()));
        if pointer_down || pointer_up {
            acceptance_trace::emit(Event::DesignerPointer {
                down: pointer_down,
                up: pointer_up,
                window_under_cursor: crate::window_manager::window_under_cursor(),
                correlation: trace_correlation(self.session.as_ref()),
            });
        }
        self.poll_replies();
        if self.close_intent == CloseIntent::None {
            self.sync_native_preview_generation();
        }
        if !self.open {
            return;
        }
        if let Some(document) = self.session.as_ref().map(|session| session.draft.clone()) {
            self.visited_path
                .ensure_root(document.default_menu_id.clone(), |id| {
                    document.menus.iter().any(|menu| &menu.id == id)
                });
        }
        let selection_before = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        let tooltip_preferences =
            crate::radial::tooltip::TooltipPreferences::from(&frame.feature_defaults);
        let (
            conflict_reason,
            preview_selection,
            prepared_preview,
            draft,
            generation,
            editor_session,
            initial_snapshot_pending,
            show_expected_layout_diagnostics,
        ) = match self.session.as_mut() {
            Some(session) => {
                let conflict_reason = session
                    .conflict
                    .as_ref()
                    .map(|conflict| conflict.reason.clone());
                if self.designer_mode == DesignerMode::Design {
                    let selected_menu =
                        session
                            .selection
                            .as_ref()
                            .and_then(|selection| match selection {
                                StableSelection::Menu(menu_id) => Some(menu_id.clone()),
                                StableSelection::Ring { menu_id, .. }
                                | StableSelection::Cell { menu_id, .. } => Some(menu_id.clone()),
                                _ => None,
                            });
                    if let Some(selected_menu) = selected_menu.filter(|menu_id| {
                        session.draft.menus.iter().any(|menu| &menu.id == menu_id)
                    }) {
                        self.visited_path
                            .select_menu(session.draft.default_menu_id.clone(), selected_menu);
                    }
                }
                let preview_selection = session.selection.clone();
                let preparation_preset = if self.designer_mode == DesignerMode::Design {
                    PreviewPreset::Current
                } else {
                    self.preview_preset
                };
                self.preview.sync_preparation(
                    session,
                    self.client.as_ref(),
                    preparation_preset,
                    preview_selection.as_ref(),
                    tooltip_preferences,
                );
                let prepared_preview = self.preview.prepared_frame(session);
                let draft = session.draft.clone();
                let generation = session.generation.0;
                let editor_session = session.editor_session;
                let initial_snapshot_pending = session.is_initial_snapshot_pending();
                let show_expected_layout_diagnostics =
                    frame.feature_defaults.show_expected_layout_diagnostics;
                (
                    conflict_reason,
                    preview_selection,
                    prepared_preview,
                    draft,
                    generation,
                    editor_session,
                    initial_snapshot_pending,
                    show_expected_layout_diagnostics,
                )
            }
            None => {
                egui::CentralPanel::default().show(ctx, |ui| ui.spinner());
                return;
            }
        };

        let body_state = if initial_snapshot_pending {
            BodyBlock::InitialSnapshot
        } else if conflict_reason.is_some() {
            BodyBlock::Conflict
        } else {
            BodyBlock::Enabled
        };
        if pointer_down || pointer_up {
            acceptance_trace::emit(Event::DesignerBody {
                state: body_state,
                correlation: trace_correlation(self.session.as_ref()),
            });
        }

        let expanded_sections_before = self.preferences.expanded_sections.clone();
        let designer_body = |ui: &mut egui::Ui| {
            self.toolbar(ui);
            ui.separator();
            if let Some(reason) = conflict_reason.as_deref() {
                self.conflict_ui(ui, reason);
                ui.separator();
            }
            self.preview_controls(ui, show_expected_layout_diagnostics);
            ui.separator();
            if initial_snapshot_pending {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading the authoritative radial configuration…");
                });
            }
            show_radial_diagnostics(
                ui,
                frame.expected_diagnostics.iter(),
                show_expected_layout_diagnostics,
                "runtime",
            );
            ui.add_enabled_ui(
                !initial_snapshot_pending && conflict_reason.is_none(),
                |ui| {
                    self.designer_controls(ui);
                    self.pending_drop_ui(ui);
                    let pane = designer_pane_layout(
                        ui.available_size(),
                        self.tree_visible,
                        self.inspector_visible,
                        self.tree_width,
                        self.inspector_width,
                    );
                    if self.show_resources {
                        // Skins/assets are the main bounded content in Skins
                        // mode.  They replace the full-height canvas rather
                        // than being appended below it, so every control is
                        // reachable through this internal scroll region at
                        // compact viewport sizes.
                        egui::ScrollArea::vertical()
                            .id_source("radial-designer-resources")
                            .show(ui, |ui| self.resources_ui(ui));
                    } else {
                        ui.horizontal(|ui| {
                            if pane.tree_width > 0.0 {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(pane.tree_width, pane.height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        self.tree(ui, &frame.feature_defaults);
                                    },
                                );
                                ui.add_space(6.0);
                            }
                            ui.allocate_ui_with_layout(
                                egui::vec2(pane.canvas_width, pane.height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    self.preview.ui(
                                        ui,
                                        &draft,
                                        generation,
                                        self.preview_zoom,
                                        self.preview_preset,
                                        preview_selection.as_ref(),
                                        prepared_preview.as_deref(),
                                        editor_session,
                                        show_expected_layout_diagnostics,
                                        self.designer_mode,
                                        self.session.as_mut(),
                                        &mut self.projected_selection,
                                        &mut self.drag_payload,
                                        &mut self.placement_draft,
                                        &mut self.pending_drop,
                                        &mut self.properties_popup,
                                        &mut self.visited_path,
                                        &mut self.canvas_pan,
                                        &mut self.pan_drag_start,
                                    );
                                },
                            );
                            if pane.inspector_width > 0.0 {
                                ui.add_space(6.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(pane.inspector_width, pane.height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_source("radial-designer-inspector")
                                            .show(ui, |ui| self.inspector(ui, frame));
                                    },
                                );
                            }
                        });
                    }
                },
            );
        };
        // `show_deferred` handles the embedded class with a bounded status
        // message before this body can be reached.  The authoritative editor
        // is therefore rendered only by the independent native viewport.
        if viewport_class != egui::ViewportClass::Embedded {
            egui::CentralPanel::default().show(ctx, designer_body);
        }
        if self.preferences.expanded_sections != expanded_sections_before {
            self.mark_preferences_changed();
        }
        self.properties_popup_ui(ctx, frame);
        if self.preferences.zoom != self.preview_zoom
            || self.preferences.pan != (self.canvas_pan.x, self.canvas_pan.y)
        {
            self.mark_preferences_changed();
        }
        if !self
            .session
            .as_ref()
            .is_some_and(RadialAuthoringSession::is_initial_snapshot_pending)
        {
            self.apply_post_render();
        }
        if pointer_down || pointer_up {
            acceptance_trace::emit(Event::DesignerPresented {
                correlation: trace_correlation(self.session.as_ref()),
            });
        }
        self.prompts(ctx);
        self.keyboard_shortcuts(ctx);
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            let size = (rect.width(), rect.height());
            if size.0.is_finite() && size.1.is_finite() && size.0 > 0.0 && size.1 > 0.0 {
                let size = (size.0, size.1);
                if self.preferences.window_size != size {
                    self.preferences.window_size = size;
                    self.mark_preferences_changed();
                }
            }
        }
        if viewport_class != egui::ViewportClass::Embedded {
            if let Some(rect) = ctx.input(|input| input.viewport().outer_rect) {
                let position = (rect.min.x, rect.min.y);
                let scale_factor = ctx.pixels_per_point();
                if position.0.is_finite()
                    && position.1.is_finite()
                    && scale_factor.is_finite()
                    && (crate::settings::RadialDesignerPreferences::MIN_WINDOW_SCALE_FACTOR
                        ..=crate::settings::RadialDesignerPreferences::MAX_WINDOW_SCALE_FACTOR)
                        .contains(&scale_factor)
                {
                    let position_changed = self.preferences.window_position != Some(position);
                    let scale_changed = self.preferences.window_scale_factor != Some(scale_factor);
                    if position_changed {
                        self.preferences.window_position = Some(position);
                    }
                    if scale_changed {
                        self.preferences.window_scale_factor = Some(scale_factor);
                    }
                    if position_changed || scale_changed {
                        self.mark_preferences_changed();
                    }
                }
            }
        }
        let selection_after = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        if selection_after != selection_before {
            self.focus_restore = selection_after;
        }
        self.enqueue_preferences_ready_if_due();
        if self.preferences_dirty && !self.preferences_flush_requested {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }

    fn conflict_ui(&mut self, ui: &mut egui::Ui, reason: &str) {
        ui.group(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Radial configuration changed externally",
            );
            ui.label(reason);
            ui.small("Choose how to reconcile the open draft before continuing to edit.");
            ui.horizontal(|ui| {
                if ui.button("Reload external").clicked() {
                    self.resolve_conflict(ConflictResolution::Reload);
                }
                if ui.button("Discard draft").clicked() {
                    self.resolve_conflict(ConflictResolution::DiscardDraft);
                }
                if ui.button("Rebase local draft").clicked() {
                    self.resolve_conflict(ConflictResolution::Rebase);
                }
            });
        });
    }

    fn properties_popup_ui(&mut self, ctx: &egui::Context, frame: &DesignerFrameContext) {
        let Some(StableSelection::Cell {
            menu_id,
            ring_id,
            cell_id,
        }) = self.properties_popup.clone()
        else {
            return;
        };
        let cell = self
            .session
            .as_ref()
            .and_then(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                    .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
            })
            .cloned();
        let Some(cell) = cell else {
            self.properties_popup = None;
            self.properties_draft = None;
            return;
        };
        let target = StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        };
        let generation = self
            .session
            .as_ref()
            .map(|session| session.generation)
            .unwrap_or_default();
        // A draft is initialized exactly once for a target.  If the service
        // advances the generation while the popup is open, retain the edits
        // and let Apply reject the stale generation instead of silently
        // replacing them with a newer snapshot.
        if self
            .properties_draft
            .as_ref()
            .is_none_or(|draft| draft.target != target)
        {
            self.properties_draft = Some(PropertiesDraft::from_cell(
                target.clone(),
                generation,
                &cell,
            ));
        }
        let Some(draft) = self.properties_draft.as_mut() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut popup_open = true;
        let invocation_context = self
            .session
            .as_ref()
            .and_then(|session| session.sampled_preview_context.clone())
            .unwrap_or_else(|| InvocationContext::empty(0));
        let action_rows =
            crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::build(
                &frame.action_catalog,
                &invocation_context,
                "",
            )
            .rows()
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>();
        let submenu_choices = self
            .session
            .as_ref()
            .map(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .filter(|menu| menu.id != menu_id)
                    .map(|menu| (menu.id.clone(), menu.name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        egui::Window::new("Cell properties")
            .id(egui::Id::new(("radial-designer-cell-popup", &cell_id)))
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .open(&mut popup_open)
            .show(ctx, |ui| {
                ui.label("Cell properties");
                ui.add(egui::TextEdit::singleline(&mut draft.label).hint_text("Label"));
                enum_combo(
                    ui,
                    "Type",
                    &mut draft.content_kind,
                    &[
                        ("Spacer", 0),
                        ("Action", 1),
                        ("Submenu", 2),
                        ("Dynamic source", 3),
                        ("Control", 4),
                    ],
                );
                if draft.content_kind == 3 {
                    enum_combo(
                        ui,
                        "Dynamic source",
                        &mut draft.dynamic_source,
                        &[
                            ("Favorites", 0),
                            ("Recent", 1),
                            ("Clipboard", 2),
                            ("Snippets", 3),
                            ("Notes", 4),
                            ("Windows", 5),
                            ("Macros", 6),
                            ("Applications", 7),
                            ("Dashboard", 8),
                            ("Launcher results", 9),
                            ("Launcher query", 10),
                        ],
                    );
                }
                if draft.content_kind == 1 {
                    ui.label(if draft.action_binding.is_some() {
                        "Action assigned"
                    } else {
                        "Choose an action"
                    });
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for row in &action_rows {
                                let label = row.presentation.label.clone();
                                if ui
                                    .selectable_label(
                                        draft.action_binding.as_ref() == row.binding.as_ref(),
                                        label,
                                    )
                                    .on_hover_text(&row.target_command)
                                    .clicked()
                                {
                                    draft.action_binding = row.assignment().ok();
                                }
                            }
                        });
                }
                if draft.content_kind == 2 {
                    egui::ComboBox::from_label("Submenu")
                        .selected_text(
                            draft
                                .submenu_target
                                .as_ref()
                                .and_then(|target| {
                                    submenu_choices
                                        .iter()
                                        .find(|(id, _)| id == target)
                                        .map(|(_, name)| name.clone())
                                })
                                .unwrap_or_else(|| "Choose menu".into()),
                        )
                        .show_ui(ui, |ui| {
                            for (id, name) in &submenu_choices {
                                ui.selectable_value(
                                    &mut draft.submenu_target,
                                    Some(id.clone()),
                                    name,
                                );
                            }
                        });
                }
                ui.horizontal(|ui| {
                    ui.label("Icon");
                    ui.selectable_value(&mut draft.icon_kind, 0, "Inherit");
                    ui.selectable_value(&mut draft.icon_kind, 1, "Clear");
                    ui.selectable_value(&mut draft.icon_kind, 2, "Keep value");
                });
                ui.small(
                    "Action bindings and advanced appearance options are available in Inspector.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("Open in Inspector").clicked() {
                        cancel = true;
                    }
                });
            });
        if !popup_open {
            cancel = true;
        }
        if cancel {
            self.properties_popup = None;
            self.properties_draft = None;
            return;
        }
        if apply {
            let draft = self
                .properties_draft
                .clone()
                .expect("draft initialized above");
            let content = match draft.content_kind {
                0 => CellContent::Spacer,
                1 => match &draft.original_content {
                    CellContent::Action { .. } => draft
                        .action_binding
                        .map(|binding| CellContent::Action { binding })
                        .unwrap_or_else(|| draft.original_content.clone()),
                    _ => {
                        let Some(binding) = draft.action_binding else {
                            self.resource_notice = Some(ResourceNotice::warning(
                                "Choose an action before applying this type",
                            ));
                            return;
                        };
                        CellContent::Action { binding }
                    }
                },
                2 => {
                    let Some(menu_id) = draft.submenu_target.clone() else {
                        self.resource_notice = Some(ResourceNotice::warning(
                            "Choose a submenu before applying this type",
                        ));
                        return;
                    };
                    CellContent::Submenu { menu_id }
                }
                3 => CellContent::Dynamic {
                    source: dynamic_source_for_key(draft.dynamic_source),
                },
                _ => CellContent::Control {
                    control: match &draft.original_content {
                        CellContent::Control { control } => *control,
                        _ => Control::Back,
                    },
                },
            };
            let Some(session) = self.session.as_mut() else {
                return;
            };
            if session.generation != draft.generation {
                self.resource_notice = Some(ResourceNotice::warning(
                    "Cell properties draft is stale; reopen it before applying",
                ));
                return;
            }
            let mut document = (*session.draft).clone();
            if let Some(slot) = document
                .menus
                .iter_mut()
                .find(|menu| menu.id == menu_id)
                .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                .and_then(|ring| ring.cells.iter_mut().find(|cell| cell.id == cell_id))
            {
                slot.label = draft.label;
                slot.content = content;
                slot.icon = match draft.icon_kind {
                    0 => crate::radial::model::Override::Inherit,
                    1 => crate::radial::model::Override::Clear,
                    _ => draft.original_icon,
                };
                if let Err(error) = menu::validate_submenu_graph(&document) {
                    self.resource_notice = Some(ResourceNotice::error(format!(
                        "Cell properties rejected: {error:?}"
                    )));
                } else if let Err(error) = session.replace_document_atomic(document) {
                    self.resource_notice = Some(ResourceNotice::error(format!(
                        "Cell properties failed: {error:?}"
                    )));
                } else {
                    self.properties_popup = None;
                    self.properties_draft = None;
                    self.placement_draft = None;
                }
            }
        }
    }

    fn resolve_conflict(&mut self, resolution: ConflictResolution) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Err(error) = session.resolve_conflict(resolution) {
            session.last_error = Some(format!("{error:?}"));
        } else {
            self.preview.cancel_tooltip();
            self.visited_path
                .replace_invalid_tail(|id| session.draft.menus.iter().any(|menu| &menu.id == id));
        }
    }

    fn designer_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Mode:");
            if ui
                .selectable_label(self.designer_mode == DesignerMode::Design, "Design")
                .clicked()
            {
                self.designer_mode = DesignerMode::Design;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            if ui
                .selectable_label(
                    self.designer_mode == DesignerMode::PreviewTest,
                    "Preview / Test",
                )
                .clicked()
            {
                self.designer_mode = DesignerMode::PreviewTest;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            ui.separator();
            let fit = ui.button("Fit");
            trace_pointer_release_response(
                ui,
                &fit,
                WidgetCategory::Zoom,
                fit.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if fit.clicked() {
                self.preview_zoom = 1.0;
                self.canvas_pan = CanvasPoint::default();
                self.pan_drag_start = None;
                self.preview.cancel_tooltip();
                self.mark_preferences_changed();
            }
            let menus = ui.selectable_label(!self.show_resources, "Menus");
            trace_pointer_release_response(
                ui,
                &menus,
                WidgetCategory::Menus,
                menus.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if menus.clicked() {
                self.show_resources = false;
                self.mark_preferences_changed();
            }
            let skins = ui.selectable_label(self.show_resources, "Skins");
            trace_pointer_release_response(
                ui,
                &skins,
                WidgetCategory::Skins,
                skins.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if skins.clicked() {
                self.show_resources = true;
                self.mark_preferences_changed();
            }
            let tree = ui
                .selectable_label(self.tree_visible, "Tree")
                .on_hover_text("Show or hide the menu tree");
            trace_pointer_release_response(
                ui,
                &tree,
                WidgetCategory::Tree,
                tree.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if tree.clicked() {
                self.tree_visible = !self.tree_visible;
                self.mark_preferences_changed();
            }
            let inspector = ui
                .selectable_label(self.inspector_visible, "Inspector")
                .on_hover_text("Show or hide the inspector");
            trace_pointer_release_response(
                ui,
                &inspector,
                WidgetCategory::Inspector,
                inspector.clicked(),
                trace_correlation(self.session.as_ref()),
            );
            if inspector.clicked() {
                self.inspector_visible = !self.inspector_visible;
                self.mark_preferences_changed();
            }
            ui.menu_button("Layout", |ui| {
                ui.label("Presentation only");
                if self.tree_visible {
                    let response = ui.add(
                        egui::DragValue::new(&mut self.tree_width)
                            .prefix("Tree ")
                            .suffix(" px")
                            .clamp_range(120.0..=420.0),
                    );
                    if response.changed() {
                        self.mark_preferences_changed();
                    }
                }
                if self.inspector_visible {
                    let response = ui.add(
                        egui::DragValue::new(&mut self.inspector_width)
                            .prefix("Inspector ")
                            .suffix(" px")
                            .clamp_range(220.0..=560.0),
                    );
                    if response.changed() {
                        self.mark_preferences_changed();
                    }
                }
                if ui
                    .button("Reset Designer layout")
                    .on_hover_text("Reset panes, sections, canvas view, and window geometry only")
                    .clicked()
                {
                    self.reset_designer_layout();
                    ui.close_menu();
                }
            });
            ui.small("Direct Design gestures edit the draft only");
        });
    }

    fn pending_drop_ui(&mut self, ui: &mut egui::Ui) {
        let Some(pending) = self.pending_drop.clone() else {
            return;
        };
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Destination occupied");
                ui.small("Choose Swap to exchange both authored slots, or Cancel.");
                if ui.button("Swap").clicked() {
                    let result = self
                        .session
                        .as_mut()
                        .ok_or(crate::radial::authoring::menu::MenuEditError::MissingEntity)
                        .and_then(|session| {
                            crate::radial::authoring::menu::move_cell_to_slot(
                                session,
                                (
                                    &pending.source_menu,
                                    &pending.source_ring,
                                    &pending.source_cell,
                                ),
                                (
                                    &pending.destination_menu,
                                    &pending.destination_ring,
                                    pending.destination_index,
                                ),
                                crate::radial::authoring::menu::CellDropResolution::Swap,
                                pending.generation,
                            )
                        });
                    match result {
                        Ok(()) => self.projected_selection = None,
                        Err(error) => {
                            self.projected_selection = Some(ProjectedSelection {
                                label: format!("Swap cancelled: {error:?}"),
                                provenance: ProjectedCellProvenance::Authored {
                                    menu_id: pending.destination_menu.clone(),
                                    ring_id: pending.destination_ring.clone(),
                                    cell_id: pending.destination_cell.clone(),
                                },
                            });
                        }
                    }
                    self.pending_drop = None;
                }
                if ui.button("Cancel").clicked() {
                    self.pending_drop = None;
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let pending = self
                .session
                .as_ref()
                .is_some_and(|session| session.pending_request.is_some());
            if ui
                .add_enabled(!pending, egui::Button::new("Undo"))
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked()
            {
                let _ = self
                    .session
                    .as_mut()
                    .is_some_and(RadialAuthoringSession::undo);
            }
            if ui
                .add_enabled(!pending, egui::Button::new("Redo"))
                .on_hover_text("Redo (Ctrl+Shift+Z)")
                .clicked()
            {
                let _ = self
                    .session
                    .as_mut()
                    .is_some_and(RadialAuthoringSession::redo);
            }
            ui.separator();
            let validation = self
                .session
                .as_ref()
                .and_then(|session| crate::radial::validation::validate(&session.draft).err());
            let valid = validation.is_none();
            if ui
                .add_enabled(!pending && valid, egui::Button::new("Apply"))
                .on_disabled_hover_text("Resolve the inline validation diagnostics before Apply")
                .clicked()
            {
                self.send_commit(CommitDisposition::Apply);
            }
            if ui
                .add_enabled(!pending && valid, egui::Button::new("Save"))
                .on_disabled_hover_text("Resolve the inline validation diagnostics before Save")
                .on_hover_text("Save and close (Ctrl+S)")
                .clicked()
            {
                self.send_commit(CommitDisposition::Save);
            }
            if ui.button("Cancel").clicked() {
                self.cancel();
            }
            if let Some(session) = &self.session {
                ui.label(if session.is_dirty() {
                    "Modified"
                } else {
                    "Saved"
                });
                if let Some(error) = &session.last_error {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
                }
            }
        });
        if let Some(errors) = self
            .session
            .as_ref()
            .and_then(|session| crate::radial::validation::validate(&session.draft).err())
        {
            ui.collapsing(
                format!("{} validation diagnostic(s)", errors.0.len()),
                |ui| {
                    for issue in errors.0 {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("Error at {}: {}", issue.path, issue.message),
                        );
                    }
                },
            );
        }
    }

    /// Consume editor shortcuts after widgets have had a chance to consume
    /// text-editing keys. This preserves native text undo while still making
    /// the authoring operations keyboard-only accessible.
    fn keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let pending = self
            .session
            .as_ref()
            .is_some_and(|session| session.pending_request.is_some());
        let modal_open = self.close_prompt
            || self.delete_message.is_some()
            || self.delete_ring_prompt.is_some()
            || self.resize_prompt.is_some();
        if pending || modal_open {
            return;
        }
        let (redo, undo, save) = ctx.input_mut(|input| {
            let redo =
                input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z);
            let undo = !redo && input.consume_key(egui::Modifiers::CTRL, egui::Key::Z);
            let save = input.consume_key(egui::Modifiers::CTRL, egui::Key::S);
            (redo, undo, save)
        });
        if undo {
            let _ = self
                .session
                .as_mut()
                .is_some_and(RadialAuthoringSession::undo);
        } else if redo {
            let _ = self
                .session
                .as_mut()
                .is_some_and(RadialAuthoringSession::redo);
        }
        if save
            && self
                .session
                .as_ref()
                .is_some_and(|session| crate::radial::validation::validate(&session.draft).is_ok())
        {
            self.send_commit(CommitDisposition::Save);
        }
    }

    fn preview_controls(&mut self, ui: &mut egui::Ui, show_expected_layout_diagnostics: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Preview");
            egui::ComboBox::from_id_source("radial-preview-preset")
                .selected_text(format!("{:?}", self.preview_preset))
                .show_ui(ui, |ui| {
                    for preset in [
                        PreviewPreset::Current,
                        PreviewPreset::OneRing,
                        PreviewPreset::MultiRing,
                        PreviewPreset::Submenu,
                        PreviewPreset::LongLabels,
                        PreviewPreset::HighDpi,
                    ] {
                        ui.selectable_value(
                            &mut self.preview_preset,
                            preset,
                            format!("{preset:?}"),
                        );
                    }
                });
            let zoom_response = ui.add(
                egui::Slider::new(&mut self.preview_zoom, 0.5..=2.0)
                    .text("Zoom")
                    .logarithmic(true),
            );
            let zoom_changed = zoom_response.changed();
            trace_pointer_release_response(
                ui,
                &zoom_response,
                WidgetCategory::Zoom,
                zoom_changed,
                trace_correlation(self.session.as_ref()),
            );
            if zoom_changed {
                self.mark_preferences_changed();
            }
            if ui
                .selectable_label(self.show_resources, "Skins, assets & packages")
                .clicked()
            {
                self.show_resources = !self.show_resources;
                self.mark_preferences_changed();
            }
            ui.small("Preview controls are UI-only");
            ui.separator();
            ui.checkbox(&mut self.sample_native_context, "Sample external context");
            let native_active = self
                .session
                .as_ref()
                .and_then(|session| session.native_preview_lease.as_ref())
                .is_some();
            if !native_active && ui.button("Open desktop preview").clicked() {
                self.send_native_preview(false);
            }
            if native_active && ui.button("Update desktop preview").clicked() {
                self.send_native_preview(true);
            }
            if native_active && ui.button("Stop desktop preview").clicked() {
                self.stop_native_preview();
            }
            if let Some(session) = &self.session {
                ui.small(if session.pending_native_preview.is_some() {
                    "Desktop preview request pending"
                } else if native_active {
                    "Desktop preview active (actions disabled)"
                } else {
                    "Desktop preview stopped"
                });
                if let Some(context) = session.sampled_preview_context.as_ref() {
                    let context_label = context
                        .preferred_external()
                        .map(|window| format!("Context: {}", window.title))
                        .unwrap_or_else(|| "Context: synthetic".into());
                    ui.small(context_label);
                }
                show_radial_diagnostics(
                    ui,
                    session.native_preview_diagnostics.iter(),
                    show_expected_layout_diagnostics,
                    "native-preview",
                );
            }
        });
    }

    fn resources_ui(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        ui.heading("Skins, assets, import and export");
        ui.small("Application defaults (read-only) → user defaults → skin → menu → ring → cell");
        ui.horizontal(|ui| {
            ui.label("Skin:");
            if ui.button("User defaults").clicked() {
                session.select(None);
            }
            for skin in session.draft.skins.clone() {
                if ui.button(&skin.name).clicked() {
                    session.select(Some(StableSelection::Skin(skin.id)));
                }
            }
            if ui.button("New skin").clicked() {
                let _ = skin_editor::create_skin(session, "New skin");
            }
        });
        if let Some(StableSelection::Skin(skin_id)) = session.selection.clone() {
            if let Some(skin) = session.draft.skins.iter().find(|skin| skin.id == skin_id) {
                let mut name = skin.name.clone();
                let response = ui.text_edit_singleline(&mut name);
                if response.changed() || response.lost_focus() {
                    let _ = skin_editor::rename_skin(
                        session,
                        &skin_id,
                        name,
                        widget_edit_phase(&response),
                    );
                }
                if ui.button("Duplicate skin").clicked() {
                    let _ = skin_editor::duplicate_skin(session, &skin_id);
                }
                let impact = crate::radial::store::references_to_skin(&session.draft, &skin_id);
                if impact.paths.is_empty() {
                    if ui.button("Delete skin").clicked() {
                        let _ = skin_editor::delete_skin(session, &skin_id);
                    }
                } else {
                    ui.label(format!("In use: {}", impact.paths.join(", ")));
                }
            }
        }
        let scope = match session.selection.clone() {
            Some(StableSelection::Skin(id)) => Some(skin_editor::StyleScope::Skin(id)),
            Some(StableSelection::Menu(id)) => Some(skin_editor::StyleScope::Menu(id)),
            Some(StableSelection::Ring { menu_id, ring_id }) => {
                Some(skin_editor::StyleScope::Ring { menu_id, ring_id })
            }
            Some(StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            }) => Some(skin_editor::StyleScope::Cell {
                menu_id,
                ring_id,
                cell_id,
            }),
            _ => Some(skin_editor::StyleScope::UserDefaults),
        };
        if let Some(scope) = scope {
            ui.label(format!("Style scope: {scope:?}"));
            if ui.button("Reset this scope to inherited values").clicked() {
                let _ = skin_editor::reset_scope(session, &scope);
            }
            match skin_editor::rows(&session.draft, &scope) {
                Ok(rows) => {
                    egui::ScrollArea::vertical()
                        .id_source("radial-style-fields")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for row in rows {
                                let input_key = format!("{scope:?}.{}.{}", row.section, row.field);
                                let kind = skin_editor::control_kind(&row.section, &row.field)
                                    .expect("style schema must have an editor control");
                                let text_input = self
                                    .style_text_inputs
                                    .entry(input_key.clone())
                                    .or_default();
                                ui.push_id(
                                    menu::widget_key(
                                        "style",
                                        &format!("{scope:?}"),
                                        &format!("{}.{}", row.section, row.field),
                                    ),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(format!("{}.{}", row.section, row.field));
                                            if ui.button("Inherit").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    skin_editor::inherit_value(),
                                                );
                                            }
                                            if ui.button("Set").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    row.inherited.clone(),
                                                );
                                            }
                                            if ui.button("Clear").clicked() {
                                                let _ = skin_editor::set_override(
                                                    session,
                                                    &scope,
                                                    &row.section,
                                                    &row.field,
                                                    skin_editor::clear_value(),
                                                );
                                            }
                                            if let Some((value, phase)) = style_value_widget(
                                                ui,
                                                kind,
                                                &row.current,
                                                &row.inherited,
                                                text_input,
                                                &session.font_families,
                                                &format!("{}.{}", row.section, row.field),
                                            ) && let Err(error) = skin_editor::set_override_edit(
                                                session,
                                                &scope,
                                                &row.section,
                                                &row.field,
                                                value,
                                                phase,
                                            ) {
                                                self.resource_notice = Some(ResourceNotice::error(error));
                                            }
                                            if skin_editor::is_media_field(&row.section, &row.field) {
                                                let media_kind = if row.section == "sounds" {
                                                    crate::radial::model::MediaKind::Sound
                                                } else {
                                                    crate::radial::model::MediaKind::Image
                                                };
                                                for (label, managed) in [
                                                    ("Managed file", true),
                                                    ("External file", false),
                                                ] {
                                                    if ui.button(label).clicked() {
                                                        self.intent_bridge.enqueue_file_dialog(
                                                            DesignerFileDialogRequest::StyleMedia {
                                                                scope: scope.clone(),
                                                                section: row.section.clone(),
                                                                field: row.field.clone(),
                                                                media_kind,
                                                                managed,
                                                            },
                                                        );
                                                    }
                                                }
                                                if ui.button("Search-path").clicked()
                                                    && !text_input.trim().is_empty()
                                                {
                                                    let value = crate::radial::model::MediaReference::SearchPath {
                                                        file_name: text_input.trim().to_owned(),
                                                    };
                                                    let _ = skin_editor::set_override(
                                                        session,
                                                        &scope,
                                                        &row.section,
                                                        &row.field,
                                                        skin_editor::with_payload(
                                                            serde_json::to_value(value).unwrap_or_default(),
                                                        ),
                                                    );
                                                }
                                                if kind == skin_editor::StyleControlKind::Resource
                                                    && row.section == "images"
                                                    && ui.button("Icon resource").clicked()
                                                    && !text_input.trim().is_empty()
                                                {
                                                    let value = crate::radial::model::MediaReference::IconResource {
                                                        path: text_input.trim().to_owned(),
                                                        index: 1,
                                                    };
                                                    let _ = skin_editor::set_override(
                                                        session,
                                                        &scope,
                                                        &row.section,
                                                        &row.field,
                                                        skin_editor::with_payload(
                                                            serde_json::to_value(value).unwrap_or_default(),
                                                        ),
                                                    );
                                                }
                                            }
                                            ui.small(format!("inherited from {:?}", row.source));
                                        });
                                    },
                                );
                            }
                        });
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
                }
            }
        }
        document_rule_controls(ui, session, &mut self.preferences.expanded_sections);
        ui.separator();
        ui.label("Managed assets");
        ui.horizontal(|ui| {
            for (label, kind, extensions) in [
                (
                    "Import managed image",
                    crate::radial::model::MediaKind::Image,
                    &["png", "gif", "jpg", "jpeg"][..],
                ),
                (
                    "Import managed WAV",
                    crate::radial::model::MediaKind::Sound,
                    &["wav"][..],
                ),
            ] {
                if ui.button(label).clicked() {
                    self.intent_bridge.enqueue_file_dialog(
                        DesignerFileDialogRequest::ImportManaged {
                            kind,
                            label: label.to_owned(),
                            extensions: extensions
                                .iter()
                                .map(|extension| (*extension).to_owned())
                                .collect(),
                        },
                    );
                }
            }
        });
        for asset in session.draft.assets.clone() {
            ui.horizontal(|ui| {
                ui.label(format!("{} ({:?})", asset.id, asset.kind));
                if asset.kind == crate::radial::model::MediaKind::Sound
                    && ui.button("Audition").clicked()
                {
                    if let Some(addition) = session
                        .pending_assets
                        .additions
                        .iter()
                        .find(|addition| addition.record.id == asset.id)
                    {
                        let _ = audio_controls::audition(std::sync::Arc::clone(&addition.bytes));
                    } else {
                        match session.request_audition_managed_asset(asset.id.clone()) {
                            Ok(request) => {
                                let pending = session.pending_request;
                                let result = self.client.as_ref().map_or_else(
                                    || Err(AuthoringError::ServiceClosed),
                                    |client| client.send(request),
                                );
                                if let Err(error) = result
                                    && let Some(pending) = pending
                                {
                                    session.reconcile_request_delivery_failure(
                                        pending,
                                        format!("{error:?}"),
                                    );
                                }
                            }
                            Err(error) => session.last_error = Some(format!("{error:?}")),
                        }
                    }
                }
                let impact = asset_picker::delete_impact(session, &asset.id);
                if impact.paths.is_empty() {
                    if ui.button("Delete").clicked() {
                        let _ = asset_picker::delete_managed_asset(session, &asset.id);
                    }
                } else {
                    ui.label(format!("In use: {}", impact.paths.join(", ")));
                }
            });
        }
        if let Some(path) = session.last_backup_path.as_ref() {
            ui.label(format!("Verified replacement backup: {}", path.display()));
        }
        self.package_controls(ui);
        if let Some(notice) = &self.resource_notice {
            show_resource_notice(ui, notice);
        }
    }

    fn package_controls(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Preview .mlradial import").clicked() {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::PreviewPackage);
            }
            if ui.button("Export selected menu").clicked() {
                if let Some(menu_id) = selected_menu_id(session) {
                    self.intent_bridge
                        .enqueue_file_dialog(DesignerFileDialogRequest::ExportMenu { menu_id });
                }
            }
            if ui.button("Export full package").clicked() {
                let roots = session
                    .draft
                    .menus
                    .iter()
                    .map(|menu| menu.id.clone())
                    .collect::<Vec<_>>();
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::ExportPackage { roots });
            }
            if ui.button("Export selected skin").clicked()
                && let Some(StableSelection::Skin(skin_id)) = session.selection.clone()
            {
                self.intent_bridge
                    .enqueue_file_dialog(DesignerFileDialogRequest::ExportSkin { skin_id });
            }
            for (label, source) in [
                (
                    "Preview Radify files",
                    crate::radial::compatibility::CompatibilitySource::Radify,
                ),
                (
                    "Preview RM4 files",
                    crate::radial::compatibility::CompatibilitySource::RadialMenuV4,
                ),
            ] {
                if ui.button(label).clicked() {
                    self.intent_bridge
                        .enqueue_file_dialog(DesignerFileDialogRequest::PreviewLegacy { source });
                }
            }
        });
        if let Some(preview) = self.pending_import.clone() {
            ui.group(|ui| {
                ui.label("Import preview (no files or draft changed)");
                ui.label(format!("Destination: {}", preview.destination()));
                for mapping in preview.mappings() {
                    ui.label(mapping);
                }
                for warning in preview.warnings() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Warning: {warning}"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Create new (default)").clicked() {
                        self.pending_import = None;
                        self.replace_confirmed = false;
                        self.replace_backup_path = None;
                        if let Err(error) = preview.clone().accept_create_new(session) {
                            self.resource_notice = Some(ResourceNotice::error(format!("{error:?}")));
                        }
                    }
                    if ui.button("Cancel preview").clicked() {
                        self.pending_import = None;
                        self.replace_confirmed = false;
                        self.replace_backup_path = None;
                    }
                });
                let replace_plan = preview.plan();
                ui.separator();
                ui.add_enabled_ui(replace_plan.is_some(), |ui| {
                ui.label("Replace the entire radial document");
                ui.checkbox(
                    &mut self.replace_confirmed,
                    "I understand this replaces all current radial menus, skins, and assets",
                );
                ui.horizontal(|ui| {
                    if ui.button("Choose verified backup destination…").clicked() {
                        self.intent_bridge.enqueue_file_dialog(
                            DesignerFileDialogRequest::ReplaceBackup,
                        );
                    }
                    if let Some(path) = &self.replace_backup_path {
                        ui.label(path.display().to_string());
                    }
                });
                let ready = self.replace_confirmed && self.replace_backup_path.is_some();
                if ui
                    .add_enabled(ready, egui::Button::new("Replace using verified backup"))
                    .clicked()
                {
                    let backup_path = self.replace_backup_path.clone().unwrap_or_default();
                    match session.request_replace_package(
                        replace_plan.clone().expect("replace UI requires a menu package"),
                        preview.revision,
                        &preview.disk_sha256,
                        preview.generation,
                        backup_path,
                        self.replace_confirmed,
                    ) {
                        Ok(request) => {
                            let pending = session.pending_request;
                            let result = self.client.as_ref().map_or_else(
                                || Err(AuthoringError::ServiceClosed),
                                |client| client.send(request),
                            );
                            match result {
                                Ok(()) => {
                                    self.pending_import = None;
                                    self.replace_confirmed = false;
                                    self.replace_backup_path = None;
                                }
                                Err(error) => {
                                    if let Some(pending) = pending {
                                        session.reconcile_request_delivery_failure(
                                            pending,
                                            format!("{error:?}"),
                                        );
                                    }
                                }
                            }
                        }
                        Err(error) => session.last_error = Some(format!("{error:?}")),
                    }
                }
                });
                if replace_plan.is_none() {
                    ui.small("Skin bundles merge as one undoable draft edit and cannot replace the full document.");
                }
            });
        }
    }

    fn tree(&mut self, ui: &mut egui::Ui, defaults: &crate::radial::model::RadialFeatureSettings) {
        ui.heading("Menus and rings");
        let drag_source = &mut self.drag_source;
        let post_render = &mut self.post_render;
        let focus_restore = &mut self.focus_restore;
        let expanded_sections = &mut self.preferences.expanded_sections;
        let preferences_dirty = &mut self.preferences_dirty;
        let visited_path = &mut self.visited_path;
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let menus = session.draft.menus.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for menu in &menus {
                let menu_selected =
                    session.selection == Some(StableSelection::Menu(menu.id.clone()));
                let expansion_key = format!("menu:{}", menu.id);
                let default_open = expanded_sections
                    .get(&expansion_key)
                    .copied()
                    // A collapsed tree is the compact default.  Once a
                    // section is explicitly opened or closed, its persisted
                    // value is authoritative and selection/diagnostics never
                    // force it open.
                    .unwrap_or(false);
                let header = egui::CollapsingHeader::new(&menu.name)
                    .id_source((
                        self.tree_widget_epoch,
                        menu::widget_key("menu", menu.id.as_str(), "tree"),
                    ))
                    .default_open(default_open)
                    .show(ui, |ui| {
                        for (ring_index, ring) in menu.rings.iter().enumerate() {
                            let ring_selection = StableSelection::Ring {
                                menu_id: menu.id.clone(),
                                ring_id: ring.id.clone(),
                            };
                            let ring_response = ui
                                .push_id(
                                    menu::widget_key(
                                        "ring",
                                        &format!("{}/{}", menu.id, ring.id),
                                        "tree",
                                    ),
                                    |ui| {
                                        ui.selectable_label(
                                            session.selection.as_ref() == Some(&ring_selection),
                                            format!("Ring {}", ring_index + 1),
                                        )
                                    },
                                )
                                .inner;
                            ring_response.clone().on_hover_text(format!(
                                "Ring {} · {}",
                                ring_index + 1,
                                ring.id
                            ));
                            if focus_restore.as_ref() == Some(&ring_selection) {
                                ring_response.request_focus();
                                *focus_restore = None;
                            }
                            if ring_response.clicked() {
                                visited_path.select_direct(menu.id.clone());
                                session.select(Some(ring_selection));
                            }
                            ui.indent(
                                menu::widget_key(
                                    "ring",
                                    &format!("{}/{}", menu.id, ring.id),
                                    "cells",
                                ),
                                |ui| {
                                    for (index, cell) in ring.cells.iter().enumerate() {
                                        let selection = StableSelection::Cell {
                                            menu_id: menu.id.clone(),
                                            ring_id: ring.id.clone(),
                                            cell_id: cell.id.clone(),
                                        };
                                        let response = ui
                                            .push_id(
                                                menu::widget_key(
                                                    "cell",
                                                    &format!("{}/{}/{}", menu.id, ring.id, cell.id),
                                                    "tree",
                                                ),
                                                |ui| {
                                                    let selected = session.selection.as_ref()
                                                        == Some(&selection);
                                                    let accessible_name =
                                                        cell_tree_accessible_name(cell, index);
                                                    let response = ui
                                                        .selectable_label(selected, &cell.label)
                                                        .on_hover_text(if cell.label.is_empty() {
                                                            accessible_name.clone()
                                                        } else {
                                                            cell.label.clone()
                                                        });
                                                    response.widget_info(|| {
                                                        egui::WidgetInfo::selected(
                                                            egui::WidgetType::SelectableLabel,
                                                            selected,
                                                            accessible_name.clone(),
                                                        )
                                                    });
                                                    response
                                                },
                                            )
                                            .inner;
                                        if focus_restore.as_ref() == Some(&selection) {
                                            response.request_focus();
                                            *focus_restore = None;
                                        }
                                        if response.clicked() {
                                            visited_path.select_direct(menu.id.clone());
                                            session.select(Some(selection));
                                        }
                                        if response.drag_started() {
                                            *drag_source = Some((
                                                menu.id.clone(),
                                                ring.id.clone(),
                                                cell.id.clone(),
                                                session.generation,
                                            ));
                                        }
                                        if response.hovered()
                                            && ui.input(|input| input.pointer.any_released())
                                        {
                                            if let Some((
                                                source_menu,
                                                source_ring,
                                                cell,
                                                expected_generation,
                                            )) = drag_source.take()
                                            {
                                                post_render.push(EditorCommand::MoveCell {
                                                    source_menu,
                                                    source_ring,
                                                    cell,
                                                    destination_menu: menu.id.clone(),
                                                    destination_ring: ring.id.clone(),
                                                    index,
                                                    expected_generation,
                                                });
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    });
                let is_open = header.body_returned.is_some();
                header
                    .header_response
                    .clone()
                    .on_hover_text(menu.name.clone());
                let expansion_was_interacted_with = expanded_sections.contains_key(&expansion_key)
                    || header.header_response.clicked();
                if expansion_was_interacted_with
                    && expanded_sections.get(&expansion_key).copied() != Some(is_open)
                {
                    expanded_sections.insert(expansion_key, is_open);
                    *preferences_dirty = true;
                }
                if focus_restore.as_ref() == Some(&StableSelection::Menu(menu.id.clone())) {
                    header.header_response.request_focus();
                    *focus_restore = None;
                }
                if header.header_response.clicked() || menu_selected && session.selection.is_none()
                {
                    visited_path.select_direct(menu.id.clone());
                    session.select(Some(StableSelection::Menu(menu.id.clone())));
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("New menu").clicked() {
                let _ = menu::create_menu_with_defaults(
                    session,
                    "menu",
                    "New menu",
                    defaults.default_interaction,
                    defaults.default_submenu_presentation,
                );
            }
            if ui.button("Add ring").clicked() {
                if let Some(menu_id) = selected_menu_id(session) {
                    let _ = menu::add_ring(session, &menu_id);
                }
            }
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui, frame: &DesignerFrameContext) {
        ui.heading("Inspector");
        if let Some(selection) = self.projected_selection.clone() {
            self.projected_selection_ui(ui, &selection);
            ui.separator();
        }
        let Some(selection) = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone())
        else {
            ui.label("Select a menu, ring, or cell.");
            return;
        };
        match selection {
            StableSelection::Menu(menu_id) => {
                self.menu_inspector(ui, menu_id, frame.feature_defaults.default_menu_id.as_ref())
            }
            StableSelection::Ring { menu_id, ring_id } => self.ring_inspector(ui, menu_id, ring_id),
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => self.cell_inspector(ui, frame, menu_id, ring_id, cell_id),
            _ => {
                ui.label("This entity is edited by a later milestone.");
            }
        };
    }

    fn menu_inspector(
        &mut self,
        ui: &mut egui::Ui,
        menu_id: MenuId,
        configured_default: Option<&MenuId>,
    ) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(found) = session.draft.menus.iter().find(|menu| menu.id == menu_id) else {
            return;
        };
        let mut name = found.name.clone();
        ui.label(format!("ID: {}", menu_id));
        let response = ui
            .push_id(menu::widget_key("menu", menu_id.as_str(), "name"), |ui| {
                ui.text_edit_singleline(&mut name)
            })
            .inner;
        if response.changed() || response.lost_focus() || response.drag_stopped() {
            let phase = widget_edit_phase(&response);
            let _ = menu::rename_menu(session, menu_id.clone(), name, phase);
        }
        let menu_index = session
            .draft
            .menus
            .iter()
            .position(|menu| menu.id == menu_id)
            .unwrap_or(0);
        let current_skin = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .map(|menu| menu.skin_id.clone());
        if let Some(mut selected_skin) = current_skin {
            egui::ComboBox::from_id_source(menu::widget_key(
                "menu",
                menu_id.as_str(),
                "default-skin",
            ))
            .selected_text(selected_skin.to_string())
            .show_ui(ui, |ui| {
                for skin in &session.draft.skins {
                    ui.selectable_value(&mut selected_skin, skin.id.clone(), &skin.name);
                }
            });
            let differs = session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .is_some_and(|menu| menu.skin_id != selected_skin);
            if differs {
                let mut document = (*session.draft).clone();
                if let Some(menu) = document.menus.iter_mut().find(|menu| menu.id == menu_id) {
                    menu.skin_id = selected_skin;
                    let _ = session.replace_document_atomic(document);
                }
            }
        }
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .cloned()
        {
            let original = edited.clone();
            let continuous =
                menu_behavior_controls(ui, &mut edited, &mut self.preferences.expanded_sections);
            if edited != original || continuous.is_some() {
                let mut document = (*session.draft).clone();
                if let Some(menu) = document.menus.iter_mut().find(|menu| menu.id == menu_id) {
                    *menu = edited;
                    if let Some(edit) = continuous {
                        let _ = dispatch_widget_document_edit(
                            session,
                            document,
                            format!("menu:{menu_id}"),
                            &edit.field,
                            edit.signals,
                        );
                    } else {
                        let _ = session.replace_document_atomic(document);
                    }
                }
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(menu_index > 0, egui::Button::new("Move up"))
                .clicked()
            {
                if menu::move_menu(session, &menu_id, menu_index.saturating_sub(1)).is_ok() {
                    self.focus_restore = session.selection.clone();
                }
            }
            if ui.button("Move down").clicked() {
                if menu::move_menu(session, &menu_id, menu_index + 1).is_ok() {
                    self.focus_restore = session.selection.clone();
                }
            }
        });
        if ui.button("Duplicate (link submenus)").clicked() {
            let _ = menu::duplicate_menu(session, &menu_id, SubmenuDuplication::LinkExisting);
        }
        if ui.button("Duplicate subtree").clicked() {
            let _ =
                menu::duplicate_menu(session, &menu_id, SubmenuDuplication::CloneSubmenuClosure);
        }
        let configured_default = configured_default == Some(&menu_id);
        if ui
            .add_enabled(!configured_default, egui::Button::new("Delete menu"))
            .on_disabled_hover_text("This menu is selected as the default in Settings")
            .clicked()
        {
            if let Err(error) = menu::delete_menu(session, &menu_id) {
                self.delete_message = Some(format!("Delete blocked: {error:?}"));
            }
        }
    }

    fn projected_selection_ui(&mut self, ui: &mut egui::Ui, selection: &ProjectedSelection) {
        let ProjectedCellProvenance::Dynamic {
            menu_id,
            ring_id,
            source_cell_id,
            source,
            result_index,
            ..
        } = &selection.provenance
        else {
            return;
        };
        ui.group(|ui| {
            ui.label("Generated preview result (read-only)");
            ui.label(&selection.label);
            ui.label(format!("Source: {source:?} · result {result_index}"));
            let source_selection = self.session.as_ref().and_then(|session| {
                session
                    .draft
                    .menus
                    .iter()
                    .find(|menu| &menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                    .and_then(|ring| {
                        ring.cells.iter().find_map(|cell| {
                            (cell.id == *source_cell_id
                                && matches!(&cell.content, CellContent::Dynamic { .. }))
                            .then(|| StableSelection::Cell {
                                menu_id: menu_id.clone(),
                                ring_id: ring_id.clone(),
                                cell_id: cell.id.clone(),
                            })
                        })
                    })
            });
            if let Some(source_selection) = source_selection {
                if ui.button("Edit dynamic source definition").clicked() {
                    if let Some(session) = self.session.as_mut() {
                        session.select(Some(source_selection));
                    }
                    self.projected_selection = None;
                }
            } else {
                ui.small("The authored source definition is not present in this menu.");
            }
        });
    }

    fn ring_inspector(&mut self, ui: &mut egui::Ui, menu_id: MenuId, ring_id: RingId) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let count = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .map_or(0, |ring| ring.cells.len());
        ui.label(format!("ID: {}", ring_id));
        ui.label(format!("{count} cells"));
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .cloned()
        {
            let radius_response = ui.add(
                egui::DragValue::new(&mut edited.radius)
                    .prefix("Radius ")
                    .speed(1.0),
            );
            let cell_radius_response = ui.add(
                egui::DragValue::new(&mut edited.cell_radius)
                    .prefix("Cell radius ")
                    .speed(1.0),
            );
            let rotation_response = ui.add(
                egui::DragValue::new(&mut edited.rotation_degrees)
                    .prefix("Rotation ° ")
                    .speed(1.0),
            );
            let gap_response = ui.add(
                egui::DragValue::new(&mut edited.gap)
                    .prefix("Gap ")
                    .speed(0.5),
            );
            let edit = [
                ("radius", radius_response),
                ("cell_radius", cell_radius_response),
                ("rotation_degrees", rotation_response),
                ("gap", gap_response),
            ]
            .into_iter()
            .find(|(_, response)| {
                response.changed() || response.lost_focus() || response.drag_stopped()
            });
            if let Some((field, response)) = edit {
                let mut document = (*session.draft).clone();
                if let Some(ring) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                {
                    *ring = edited;
                    let _ = dispatch_widget_document_edit(
                        session,
                        document,
                        format!("ring:{menu_id}/{ring_id}"),
                        field,
                        WidgetEditSignals::from_response(&response),
                    );
                }
            }
        }
        let ring_index = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().position(|ring| ring.id == ring_id))
            .unwrap_or(0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ring_index > 0, egui::Button::new("Move inward"))
                .clicked()
            {
                if menu::move_ring(session, &menu_id, &ring_id, ring_index.saturating_sub(1))
                    .is_ok()
                {
                    self.focus_restore = session.selection.clone();
                }
            }
            if ui.button("Move outward").clicked() {
                if menu::move_ring(session, &menu_id, &ring_id, ring_index + 1).is_ok() {
                    self.focus_restore = session.selection.clone();
                }
            }
        });
        if ui.button("Add spacer").clicked() {
            let _ = menu::add_spacer(session, &menu_id, &ring_id);
        }
        let resize_key = (menu_id.clone(), ring_id.clone());
        let requested = self
            .ring_resize_drafts
            .entry(resize_key.clone())
            .or_insert(count);
        let resize_response = ui.add(egui::DragValue::new(requested).prefix("Cells "));
        let resize_ended = resize_response.lost_focus()
            || resize_response.drag_stopped()
            || (resize_response.changed()
                && !resize_response.has_focus()
                && !resize_response.dragged());
        if resize_ended {
            let requested = self.ring_resize_drafts.remove(&resize_key).unwrap_or(count);
            if let Ok(plan) = menu::resize_plan(&session.draft, &menu_id, &ring_id, requested) {
                if plan.requires_resolution() {
                    self.resize_prompt = Some(plan);
                } else {
                    let _ = menu::apply_resize(session, plan, None);
                }
            }
        } else if !resize_response.has_focus() && !resize_response.dragged() {
            self.ring_resize_drafts.remove(&resize_key);
        }
        if ui.button("Delete ring").clicked() {
            if count == 0 {
                let _ = menu::delete_ring(session, &menu_id, &ring_id);
            } else {
                self.delete_ring_prompt = Some((menu_id, ring_id));
            }
        }
    }

    fn cell_inspector(
        &mut self,
        ui: &mut egui::Ui,
        frame: &DesignerFrameContext,
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    ) {
        let invocation_context = self
            .session
            .as_ref()
            .and_then(|session| session.sampled_preview_context.clone())
            .unwrap_or_else(|| InvocationContext::empty(0));
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Some(cell) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|cell| cell.id == cell_id))
            .cloned()
        else {
            return;
        };
        ui.label(format!("ID: {}", cell_id));
        if self.placement_draft.as_ref().is_some_and(|draft| {
            draft.menu_id == menu_id && draft.ring_id == ring_id && draft.cell_id == cell_id
        }) {
            ui.group(|ui| {
                ui.label("Placement draft");
                ui.small("Choose valid content below to commit this empty authored slot.");
                if ui.button("Cancel placement").clicked() {
                    self.placement_draft = None;
                }
            });
        }
        let mut label = cell.label.clone();
        let label_response = ui
            .push_id(
                menu::widget_key("cell", &format!("{menu_id}/{ring_id}/{cell_id}"), "label"),
                |ui| ui.text_edit_singleline(&mut label),
            )
            .inner;
        if label_response.changed() || label_response.lost_focus() || label_response.drag_stopped()
        {
            let _ = menu::set_cell_label(
                session,
                &menu_id,
                &ring_id,
                &cell_id,
                label,
                widget_edit_phase(&label_response),
            );
        }
        if let Some(mut edited) = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
            .and_then(|ring| ring.cells.iter().find(|candidate| candidate.id == cell_id))
            .cloned()
        {
            let original = edited.clone();
            after_action_combo(ui, "Primary after action", &mut edited.after_action);
            after_action_combo(
                ui,
                "Secondary after action",
                &mut edited.secondary_after_action,
            );
            let content_edit = cell_content_controls(
                ui,
                &session.draft,
                &mut edited,
                &mut self.preferences.expanded_sections,
            );
            let media_edit = cell_media_controls(
                ui,
                &session.draft,
                &mut edited,
                &self.intent_bridge,
                &mut self.preferences.expanded_sections,
                &menu_id,
                &ring_id,
                &cell_id,
            );
            let trigger_edit = cell_trigger_controls(
                ui,
                session,
                &mut edited,
                &mut self.preferences.expanded_sections,
            );
            let continuous = content_edit.or(media_edit).or(trigger_edit);
            if edited != original || continuous.is_some() {
                let mut document = (*session.draft).clone();
                if let Some(slot) = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                    .and_then(|ring| {
                        ring.cells
                            .iter_mut()
                            .find(|candidate| candidate.id == cell_id)
                    })
                {
                    let completes_placement = matches!(&original.content, CellContent::Spacer)
                        && !matches!(&edited.content, CellContent::Spacer);
                    *slot = edited;
                    if let Err(error) = menu::validate_submenu_graph(&document) {
                        self.delete_message = Some(format!("Cell edit blocked: {error:?}"));
                    } else {
                        if let Some(edit) = continuous {
                            let _ = dispatch_widget_document_edit(
                                session,
                                document,
                                format!("cell:{menu_id}/{ring_id}/{cell_id}"),
                                &edit.field,
                                edit.signals,
                            );
                        } else {
                            let _ = session.replace_document_atomic(document);
                        }
                        if completes_placement {
                            self.placement_draft = None;
                        }
                    }
                }
            }
        }
        if matches!(&cell.content, CellContent::Spacer) {
            ui.separator();
            ui.label("Empty authored slot");
            ui.text_edit_singleline(&mut self.submenu_name);
            if ui.button("Create and link new submenu").clicked() {
                if let Err(error) = menu::create_submenu_and_link(
                    session,
                    &menu_id,
                    &ring_id,
                    &cell_id,
                    &self.submenu_name,
                ) {
                    self.delete_message = Some(format!("Create submenu failed: {error:?}"));
                }
            }
            let mut linked_menu = self.submenu_link_target.clone();
            if linked_menu
                .as_ref()
                .is_some_and(|id| !session.draft.menus.iter().any(|menu| &menu.id == id))
            {
                linked_menu = None;
            }
            egui::ComboBox::from_label("Link existing submenu")
                .selected_text(
                    linked_menu
                        .as_ref()
                        .and_then(|id| session.draft.menus.iter().find(|menu| &menu.id == id))
                        .map_or_else(|| "Choose menu".into(), |menu| menu.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for menu in &session.draft.menus {
                        if menu.id != menu_id {
                            ui.selectable_value(
                                &mut linked_menu,
                                Some(menu.id.clone()),
                                &menu.name,
                            );
                        }
                    }
                });
            self.submenu_link_target = linked_menu.clone();
            if ui
                .add_enabled(linked_menu.is_some(), egui::Button::new("Link existing"))
                .clicked()
            {
                if let Some(linked_menu) = linked_menu {
                    if let Err(error) = menu::link_existing_submenu(
                        session,
                        &menu_id,
                        &ring_id,
                        &cell_id,
                        &linked_menu,
                    ) {
                        self.delete_message = Some(format!("Link submenu failed: {error:?}"));
                    }
                }
            }
        }
        if ui.button("Copy cell").clicked() {
            if let Err(error) = menu::copy_cell(session, &menu_id, &ring_id, &cell_id) {
                self.delete_message = Some(format!("Copy failed: {error:?}"));
            }
        }
        if let CellContent::Submenu { menu_id: linked } = &cell.content
            && ui.button("Clone reusable submenu graph").clicked()
            && let Ok(cloned) =
                menu::duplicate_menu(session, linked, SubmenuDuplication::CloneSubmenuClosure)
        {
            let _ = menu::set_cell_content(
                session,
                &menu_id,
                &ring_id,
                &cell_id,
                CellContent::Submenu { menu_id: cloned },
            );
            session.select(Some(StableSelection::Cell {
                menu_id: menu_id.clone(),
                ring_id: ring_id.clone(),
                cell_id: cell_id.clone(),
            }));
        }
        if ui.button("Make spacer").clicked() {
            let _ =
                menu::set_cell_content(session, &menu_id, &ring_id, &cell_id, CellContent::Spacer);
        }
        if ui.button("Delete cell").clicked() {
            let _ = menu::delete_cell(session, &menu_id, &ring_id, &cell_id);
            return;
        }
        ui.separator();
        ui.label("Universal Action");
        ui.text_edit_singleline(&mut self.action_filter);
        let catalog = crate::gui::universal_action_catalog::UniversalActionAuthoringCatalog::build(
            &frame.action_catalog,
            &invocation_context,
            &self.action_filter,
        );
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for row in catalog
                    .rows()
                    .iter()
                    .filter(|row| {
                        self.action_filter.is_empty()
                            || row
                                .presentation
                                .label
                                .to_lowercase()
                                .contains(&self.action_filter.to_lowercase())
                            || row
                                .target_command
                                .to_lowercase()
                                .contains(&self.action_filter.to_lowercase())
                    })
                    .take(100)
                {
                    let label = row.presentation.label.clone();
                    let assignable = row.binding.is_some();
                    let testable = assignable && row.availability.is_available();
                    ui.push_id(
                        menu::widget_key(
                            "action",
                            &format!("{}:{}", row.target_command, row.action_id),
                            "picker-row",
                        ),
                        |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(assignable, egui::Button::new(label))
                                    .on_disabled_hover_text(
                                        row.unavailable_reason
                                            .as_deref()
                                            .unwrap_or("Not persistable"),
                                    )
                                    .clicked()
                                    && let Ok(binding) = row.assignment()
                                {
                                    if menu::set_cell_content(
                                        session,
                                        &menu_id,
                                        &ring_id,
                                        &cell_id,
                                        CellContent::Action { binding },
                                    )
                                    .is_ok()
                                    {
                                        self.placement_draft = None;
                                    }
                                }
                                for (label, gesture) in [
                                    (
                                        "Cell secondary",
                                        crate::radial::model::ClickGesture::Secondary,
                                    ),
                                    ("Cell Ctrl", crate::radial::model::ClickGesture::CtrlPrimary),
                                    (
                                        "Cell Shift",
                                        crate::radial::model::ClickGesture::ShiftPrimary,
                                    ),
                                    ("Cell Alt", crate::radial::model::ClickGesture::AltPrimary),
                                ] {
                                    if ui
                                        .add_enabled(assignable, egui::Button::new(label))
                                        .clicked()
                                        && let Ok(binding) = row.assignment()
                                    {
                                        assign_alternate_click(
                                            session, &menu_id, &ring_id, &cell_id, gesture, binding,
                                        );
                                    }
                                }
                                for (label, target) in [
                                    ("Center L", MenuActionSlot::CenterPrimary),
                                    ("Center R", MenuActionSlot::CenterSecondary),
                                    ("Background L", MenuActionSlot::BackgroundPrimary),
                                    ("Background R", MenuActionSlot::BackgroundSecondary),
                                ] {
                                    if ui
                                        .add_enabled(assignable, egui::Button::new(label))
                                        .clicked()
                                        && let Ok(binding) = row.assignment()
                                    {
                                        assign_menu_action(session, &menu_id, target, binding);
                                    }
                                }
                                if ui
                                    .add_enabled(testable, egui::Button::new("Test"))
                                    .on_disabled_hover_text(
                                        row.availability
                                            .disabled_reason()
                                            .or(row.unavailable_reason.as_deref())
                                            .unwrap_or("Not testable"),
                                    )
                                    .clicked()
                                    && let Ok(binding) = row.assignment()
                                {
                                    frame.intent_bridge.push(DesignerUiIntent::TestAction {
                                        binding,
                                        invocation: invocation_context.clone(),
                                        history_query: self.action_filter.clone(),
                                    });
                                }
                                if row.destructive {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        if frame.require_confirm_destructive {
                                            "Destructive action · confirmation required"
                                        } else {
                                            "Destructive action"
                                        },
                                    );
                                }
                                ui.vertical(|ui| {
                                    ui.small(format!("Command: {}", row.target_command));
                                    ui.small(format!("Action ID: {}", row.action_id));
                                    ui.small(format!("Persistence: {:?}", row.persistence));
                                    ui.small(format!("Availability: {:?}", row.availability));
                                    ui.small(format!("Interaction: {:?}", row.interaction));
                                    ui.small(format!(
                                        "Close policy: current={} tree={} keep-open={}",
                                        row.after_action.close_current_menu,
                                        row.after_action.close_tree,
                                        row.after_action.keep_open,
                                    ));
                                });
                                if let Some(reason) = &row.after_action.keep_open_reason {
                                    ui.small(reason);
                                }
                            });
                        },
                    );
                }
            });
    }

    fn apply_post_render(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        for command in self.post_render.drain(..) {
            match command {
                EditorCommand::MoveCell {
                    source_menu,
                    source_ring,
                    cell,
                    destination_menu,
                    destination_ring,
                    index,
                    expected_generation,
                } => {
                    let destination_cell = session
                        .draft
                        .menus
                        .iter()
                        .find(|menu| menu.id == destination_menu)
                        .and_then(|menu| menu.rings.iter().find(|ring| ring.id == destination_ring))
                        .and_then(|ring| ring.cells.get(index))
                        .map(|cell| cell.id.clone());
                    match menu::move_cell_to_slot(
                        session,
                        (&source_menu, &source_ring, &cell),
                        (&destination_menu, &destination_ring, index),
                        menu::CellDropResolution::MoveIntoSpacer,
                        expected_generation,
                    ) {
                        Ok(()) => {
                            self.focus_restore = session.selection.clone();
                            acceptance_trace::emit(Event::DesignerWidget {
                                category: WidgetCategory::Canvas,
                                response: WidgetResponse::Accepted,
                                correlation: trace_correlation(Some(session)),
                            });
                        }
                        Err(menu::MenuEditError::DestinationOccupied) => {
                            acceptance_trace::emit(Event::DesignerWidget {
                                category: WidgetCategory::Canvas,
                                response: WidgetResponse::Rejected,
                                correlation: trace_correlation(Some(session)),
                            });
                            if let Some(destination_cell) = destination_cell {
                                self.pending_drop = Some(PendingCellDrop {
                                    source_menu,
                                    source_ring,
                                    source_cell: cell,
                                    destination_menu,
                                    destination_ring,
                                    destination_cell,
                                    destination_index: index,
                                    generation: expected_generation,
                                });
                            }
                        }
                        Err(_) => acceptance_trace::emit(Event::DesignerWidget {
                            category: WidgetCategory::Canvas,
                            response: WidgetResponse::Rejected,
                            correlation: trace_correlation(Some(session)),
                        }),
                    }
                }
            }
        }
    }

    fn cancel(&mut self) {
        self.preview.cancel_tooltip();
        self.close_intent = CloseIntent::DiscardRequested;
        if let Some(session) = self.session.as_mut() {
            session.request_discard_close_intent();
        }
        self.stop_native_preview();
        let Some(session) = self.session.as_mut() else {
            self.force_close();
            return;
        };
        let result = session.request_commit(CommitDisposition::RevertAppliedAndClose);
        match result {
            Ok(request) => {
                let pending = session.pending_request;
                let result = self.client.as_ref().map_or_else(
                    || Err(AuthoringError::ServiceClosed),
                    |client| client.send(request),
                );
                if let Err(error) = result
                    && let Some(pending) = pending
                {
                    session.reconcile_request_delivery_failure(pending, format!("{error:?}"));
                    self.close_intent = CloseIntent::Requested;
                    session.request_close_intent();
                    self.close_prompt = true;
                }
            }
            Err(AuthoringError::NothingToRevert) => {
                // If StopNativePreview was queued, retain the session until
                // its terminal reply; otherwise this discard is already a
                // safe clean teardown.
                self.close_prompt = false;
                self.maybe_finish_close();
            }
            Err(error) => {
                session.last_error = Some(format!("{error:?}"));
                self.close_intent = CloseIntent::Requested;
                session.request_close_intent();
                self.close_prompt = true;
            }
        }
    }

    fn prompts(&mut self, ctx: &egui::Context) {
        if self.close_prompt {
            egui::Window::new("Unsaved radial changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Save, discard, or keep editing?");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.close_prompt = false;
                            self.send_commit(CommitDisposition::Save);
                        }
                        if ui.button("Discard").clicked() {
                            self.close_intent = CloseIntent::DiscardRequested;
                            if let Some(session) = self.session.as_mut() {
                                session.request_discard_close_intent();
                            }
                            self.close_prompt = false;
                            self.cancel();
                        }
                        if ui.button("Keep editing").clicked() {
                            self.close_prompt = false;
                            self.close_intent = CloseIntent::None;
                            self.close_stop_attempted = false;
                            if let Some(session) = self.session.as_mut() {
                                session.clear_close_intent();
                            }
                        }
                    });
                });
        }
        if let Some(message) = self.delete_message.clone() {
            egui::Window::new("Delete impact").show(ctx, |ui| {
                ui.label(message);
                if ui.button("OK").clicked() {
                    self.delete_message = None;
                }
            });
        }
        if let Some((menu_id, ring_id)) = self.delete_ring_prompt.clone() {
            let destinations: Vec<_> = self
                .session
                .as_ref()
                .into_iter()
                .flat_map(|session| session.draft.menus.iter())
                .filter(|menu| menu.id == menu_id)
                .flat_map(|menu| menu.rings.iter())
                .filter(|ring| ring.id != ring_id)
                .map(|ring| ring.id.clone())
                .collect();
            let populated = self
                .session
                .as_ref()
                .and_then(|session| session.draft.menus.iter().find(|menu| menu.id == menu_id))
                .and_then(|menu| menu.rings.iter().find(|ring| ring.id == ring_id))
                .map_or(0, |ring| {
                    ring.cells
                        .iter()
                        .filter(|cell| !matches!(cell.content, CellContent::Spacer))
                        .count()
                });
            egui::Window::new("Delete populated ring").show(ctx, |ui| {
                ui.label(format!(
                    "This removes {populated} populated cells. Relocate them or explicitly discard."
                ));
                for destination in destinations {
                    if ui
                        .button(format!("Move cells to {destination} and delete"))
                        .clicked()
                    {
                        if let Some(session) = self.session.as_mut() {
                            match relocate_and_delete_ring(
                                session,
                                &menu_id,
                                &ring_id,
                                &destination,
                            ) {
                                Ok(()) => self.delete_ring_prompt = None,
                                Err(error) => self.delete_message = Some(error),
                            }
                        }
                    }
                }
                if ui.button("Discard populated cells and delete").clicked() {
                    if let Some(session) = self.session.as_mut() {
                        match menu::delete_ring(session, &menu_id, &ring_id) {
                            Ok(()) => self.delete_ring_prompt = None,
                            Err(error) => {
                                self.delete_message = Some(format!("Delete failed: {error:?}"))
                            }
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.delete_ring_prompt = None;
                }
            });
        }
        if let Some(plan) = self.resize_prompt.clone() {
            egui::Window::new("Resolve populated cells").show(ctx, |ui| {
                ui.label(format!(
                    "{} populated cells would be removed.",
                    plan.populated_removed.len()
                ));
                if ui.button("Move to overflow ring").clicked() {
                    if let Some(session) = self.session.as_mut() {
                        match menu::apply_resize(
                            session,
                            plan.clone(),
                            Some(ResizeResolution::OverflowRing),
                        ) {
                            Ok(()) => self.resize_prompt = None,
                            Err(error) => {
                                self.delete_message = Some(format!("Resize failed: {error:?}"))
                            }
                        }
                    }
                }
                let destinations: Vec<_> = self
                    .session
                    .as_ref()
                    .into_iter()
                    .flat_map(|session| session.draft.menus.iter())
                    .filter(|menu| menu.id == plan.menu_id)
                    .flat_map(|menu| menu.rings.iter())
                    .filter(|ring| ring.id != plan.ring_id)
                    .map(|ring| ring.id.clone())
                    .collect();
                for destination in destinations {
                    if ui.button(format!("Relocate to {destination}")).clicked() {
                        if let Some(session) = self.session.as_mut() {
                            match menu::apply_resize(
                                session,
                                plan.clone(),
                                Some(ResizeResolution::Relocate {
                                    menu_id: plan.menu_id.clone(),
                                    ring_id: destination,
                                }),
                            ) {
                                Ok(()) => self.resize_prompt = None,
                                Err(error) => {
                                    self.delete_message = Some(format!("Resize failed: {error:?}"))
                                }
                            }
                        }
                    }
                }
                if ui.button("Discard cells").clicked() {
                    if let Some(session) = self.session.as_mut() {
                        match menu::apply_resize(
                            session,
                            plan.clone(),
                            Some(ResizeResolution::ConfirmDiscard),
                        ) {
                            Ok(()) => self.resize_prompt = None,
                            Err(error) => {
                                self.delete_message = Some(format!("Resize failed: {error:?}"))
                            }
                        }
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.resize_prompt = None;
                }
            });
        }
    }
}

fn cell_tree_accessible_name(cell: &crate::radial::model::CellDefinition, index: usize) -> String {
    if !cell.label.trim().is_empty() {
        return cell.label.trim().to_owned();
    }
    if let crate::radial::model::Override::Value(tooltip) = &cell.tooltip
        && !tooltip.trim().is_empty()
    {
        return tooltip.trim().to_owned();
    }
    if matches!(cell.icon, crate::radial::model::Override::Value(_)) {
        let role = match &cell.content {
            CellContent::Action { .. } => "Action",
            CellContent::Dynamic { .. } => "Dynamic item",
            CellContent::Submenu { .. } => "Submenu",
            CellContent::Control { control } => match control {
                Control::Back => "Back",
                Control::Close => "Close",
                Control::NextPage => "Next page",
                Control::PreviousPage => "Previous page",
                Control::Drag => "Drag",
            },
            CellContent::Spacer => "Spacer",
        };
        return format!("{role} icon");
    }
    format!("Cell {} ({})", index + 1, cell.id)
}

fn cell_content_kind(content: &CellContent) -> &'static str {
    match content {
        CellContent::Action { .. } => "Action",
        CellContent::Dynamic { .. } => "Dynamic item",
        CellContent::Submenu { .. } => "Submenu",
        CellContent::Control { .. } => "Control",
        CellContent::Spacer => "Spacer",
    }
}

fn cell_content_kind_key(content: &CellContent) -> u8 {
    match content {
        CellContent::Spacer => 0,
        CellContent::Action { .. } => 1,
        CellContent::Submenu { .. } => 2,
        CellContent::Dynamic { .. } => 3,
        CellContent::Control { .. } => 4,
    }
}

fn selected_menu_id(session: &RadialAuthoringSession) -> Option<MenuId> {
    match session.selection.as_ref()? {
        StableSelection::Menu(id)
        | StableSelection::Ring { menu_id: id, .. }
        | StableSelection::Cell { menu_id: id, .. } => Some(id.clone()),
        _ => None,
    }
}

fn widget_edit_phase(response: &egui::Response) -> EditPhase {
    if response.lost_focus() || response.drag_stopped() {
        EditPhase::End
    } else if response.drag_started() {
        EditPhase::Begin
    } else {
        EditPhase::Update
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WidgetEditSignals {
    drag_started: bool,
    ended: bool,
}

#[derive(Clone, Debug)]
struct ContinuousWidgetEdit {
    field: String,
    signals: WidgetEditSignals,
}

fn continuous_widget_edit(
    response: &egui::Response,
    field: impl Into<String>,
) -> Option<ContinuousWidgetEdit> {
    (response.changed() || response.lost_focus() || response.drag_stopped()).then(|| {
        ContinuousWidgetEdit {
            field: field.into(),
            signals: WidgetEditSignals::from_response(response),
        }
    })
}

fn set_first_edit<T>(slot: &mut Option<T>, candidate: Option<T>) {
    if slot.is_none() {
        *slot = candidate;
    }
}

impl WidgetEditSignals {
    fn from_response(response: &egui::Response) -> Self {
        Self {
            drag_started: response.drag_started(),
            ended: response.lost_focus() || response.drag_stopped(),
        }
    }

    fn phase(self) -> EditPhase {
        if self.ended {
            EditPhase::End
        } else if self.drag_started {
            EditPhase::Begin
        } else {
            EditPhase::Update
        }
    }
}

fn dispatch_widget_document_edit(
    session: &mut RadialAuthoringSession,
    document: RadialDocument,
    entity: String,
    field: &str,
    signals: WidgetEditSignals,
) -> Result<(), AuthoringError> {
    session.replace_document_edit(
        document,
        EditKey {
            entity,
            field: field.into(),
        },
        signals.phase(),
    )
}

fn style_value_widget(
    ui: &mut egui::Ui,
    kind: skin_editor::StyleControlKind,
    current: &serde_json::Value,
    inherited: &serde_json::Value,
    text_input: &mut String,
    font_families: &[String],
    accessible_name: &str,
) -> Option<(serde_json::Value, EditPhase)> {
    use skin_editor::StyleControlKind;
    let payload = skin_editor::override_payload(current)
        .or_else(|| skin_editor::override_payload(inherited))
        .unwrap_or(serde_json::Value::Null);
    let changed = match kind {
        StyleControlKind::Toggle => {
            let mut value = payload.as_bool().unwrap_or(false);
            let response = ui.checkbox(&mut value, "");
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Checkbox, value, accessible_name)
            });
            (response.changed() || response.lost_focus())
                .then(|| (serde_json::json!(value), widget_edit_phase(&response)))
        }
        StyleControlKind::Scalar | StyleControlKind::Opacity => {
            let mut value = payload.as_f64().unwrap_or(1.0) as f32;
            let response = if kind == StyleControlKind::Opacity {
                ui.add(egui::Slider::new(&mut value, 0.0..=1.0))
            } else {
                ui.add(egui::DragValue::new(&mut value).speed(0.05))
            };
            response.widget_info(|| {
                if kind == StyleControlKind::Opacity {
                    egui::WidgetInfo::slider(f64::from(value), accessible_name)
                } else {
                    egui::WidgetInfo::labeled(egui::WidgetType::DragValue, accessible_name)
                }
            });
            (response.changed() || response.drag_stopped() || response.lost_focus())
                .then(|| (serde_json::json!(value), widget_edit_phase(&response)))
        }
        StyleControlKind::Color => {
            let component = |name: &str, fallback| {
                payload
                    .get(name)
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(fallback) as u8
            };
            let original = [
                component("red", 255),
                component("green", 255),
                component("blue", 255),
                component("alpha", 255),
            ];
            let mut color = egui::Color32::from_rgba_unmultiplied(
                original[0],
                original[1],
                original[2],
                original[3],
            );
            let response = ui.color_edit_button_srgba(&mut color);
            let [red, green, blue, alpha] = if response.changed() {
                color.to_srgba_unmultiplied()
            } else {
                original
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::ColorButton,
                    format!(
                        "{accessible_name}: rgba({}, {}, {}, {})",
                        red, green, blue, alpha
                    ),
                )
            });
            (response.changed() || response.drag_stopped() || response.lost_focus()).then(|| {
                (
                    serde_json::json!({
                        "red": red, "green": green, "blue": blue, "alpha": alpha
                    }),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::Offset => {
            let mut x = payload
                .get("x")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let mut y = payload
                .get("y")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            let x_response = ui.add(egui::DragValue::new(&mut x).prefix("x "));
            let y_response = ui.add(egui::DragValue::new(&mut y).prefix("y "));
            x_response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::DragValue,
                    format!("{accessible_name} x"),
                )
            });
            y_response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::DragValue,
                    format!("{accessible_name} y"),
                )
            });
            let response = if x_response.changed() || x_response.drag_stopped() {
                x_response
            } else {
                y_response
            };
            (response.changed() || response.drag_stopped() || response.lost_focus()).then(|| {
                (
                    serde_json::json!({ "x": x, "y": y }),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::TooltipMode | StyleControlKind::Quality => {
            let choices: &[(&str, &str)] = if kind == StyleControlKind::TooltipMode {
                &[
                    ("Disabled", "disabled"),
                    ("Explicit", "explicit"),
                    ("Automatic", "automatic"),
                ]
            } else {
                &[
                    ("Fast", "fast"),
                    ("Balanced", "balanced"),
                    ("High quality", "high_quality"),
                ]
            };
            let mut value = payload.as_str().unwrap_or(choices[0].1).to_owned();
            let before = value.clone();
            let combo = egui::ComboBox::from_id_source(ui.next_auto_id())
                .selected_text(
                    choices
                        .iter()
                        .find(|(_, wire)| *wire == value)
                        .map_or(value.as_str(), |(label, _)| label),
                )
                .show_ui(ui, |ui| {
                    for (label, wire) in choices {
                        ui.selectable_value(&mut value, (*wire).to_owned(), *label);
                    }
                });
            combo.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, accessible_name)
            });
            (value != before).then(|| (serde_json::json!(value), EditPhase::Atomic))
        }
        StyleControlKind::Font => {
            if text_input.is_empty() {
                *text_input = payload.as_str().unwrap_or_default().to_owned();
            }
            let before = text_input.clone();
            let combo = egui::ComboBox::from_id_source(ui.next_auto_id())
                .selected_text(if text_input.is_empty() {
                    "System fallback"
                } else {
                    text_input.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(text_input, String::new(), "System fallback");
                    for family in font_families {
                        ui.selectable_value(text_input, family.clone(), family);
                    }
                });
            combo.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, accessible_name)
            });
            let combo_changed = *text_input != before;
            let response = ui.text_edit_singleline(text_input);
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::text_edit("", text_input.as_str());
                info.label = Some(accessible_name.into());
                info
            });
            if response.changed() || response.lost_focus() {
                Some((
                    serde_json::json!(text_input.clone()),
                    widget_edit_phase(&response),
                ))
            } else if combo_changed {
                Some((serde_json::json!(text_input.clone()), EditPhase::Atomic))
            } else {
                None
            }
        }
        StyleControlKind::Text => {
            if text_input.is_empty() {
                *text_input = payload.as_str().unwrap_or_default().to_owned();
            }
            let response = ui.text_edit_singleline(text_input);
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::text_edit("", text_input.as_str());
                info.label = Some(accessible_name.into());
                info
            });
            (response.changed() || response.lost_focus()).then(|| {
                (
                    serde_json::json!(text_input.clone()),
                    widget_edit_phase(&response),
                )
            })
        }
        StyleControlKind::Resource => {
            if text_input.is_empty() {
                *text_input = payload
                    .get("path")
                    .or_else(|| payload.get("file_name"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
            }
            let path_response = ui
                .text_edit_singleline(text_input)
                .on_hover_text("Search-path file name or icon resource path");
            path_response.widget_info(|| {
                let mut info = egui::WidgetInfo::text_edit("", text_input.as_str());
                info.label = Some(accessible_name.into());
                info
            });
            let mut resource = payload.clone();
            if let Some(object) = resource.as_object_mut() {
                if object.contains_key("path") {
                    object.insert("path".into(), serde_json::json!(text_input.clone()));
                } else if object.contains_key("file_name") {
                    object.insert("file_name".into(), serde_json::json!(text_input.clone()));
                }
            }
            let mut response = continuous_widget_edit(&path_response, "resource.path")
                .map(|edit| (resource.clone(), edit.signals.phase()));
            if let Some(index) = resource.get("index").and_then(serde_json::Value::as_i64) {
                let mut index = i32::try_from(index).unwrap_or(1).max(1);
                let index_response =
                    ui.add(egui::DragValue::new(&mut index).prefix("Resource index "));
                index_response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::DragValue,
                        format!("{accessible_name} resource index"),
                    )
                });
                if let Some(object) = resource.as_object_mut() {
                    object.insert("index".into(), serde_json::json!(index));
                }
                response = response.or_else(|| {
                    continuous_widget_edit(&index_response, "resource.index")
                        .map(|edit| (resource, edit.signals.phase()))
                });
            }
            response
        }
    };
    changed.map(|(value, phase)| (skin_editor::with_payload(value), phase))
}

fn persisted_section(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    key: &str,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let open = expanded_sections.get(key).copied().unwrap_or(false);
    let response = egui::CollapsingHeader::new(label)
        .id_source(("radial-designer-section", key))
        .default_open(open)
        .open(Some(open))
        .show(ui, body);
    if response.header_response.changed() {
        expanded_sections.insert(key.to_owned(), !open);
        true
    } else {
        false
    }
}

fn menu_behavior_controls(
    ui: &mut egui::Ui,
    menu: &mut crate::radial::model::MenuDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
) -> Option<ContinuousWidgetEdit> {
    let mut continuous = None;
    enum_combo(
        ui,
        "Layout",
        &mut menu.layout,
        &[
            ("Circular cells", LayoutKind::CircularCells),
            ("Wedges", LayoutKind::Wedges),
        ],
    );
    enum_combo(
        ui,
        "Interaction",
        &mut menu.interaction,
        &[
            ("Sticky click", InteractionMode::StickyClick),
            ("Release to select", InteractionMode::ReleaseToSelect),
            ("Hold and click", InteractionMode::HoldAndClick),
        ],
    );
    let mut dwell_enabled = menu.hover_dwell_ms.is_some();
    if ui.checkbox(&mut dwell_enabled, "Hover dwell").changed() {
        menu.hover_dwell_ms = dwell_enabled.then_some(350);
    }
    if let Some(dwell) = menu.hover_dwell_ms.as_mut() {
        let response = ui.add(
            egui::DragValue::new(dwell)
                .prefix("Dwell ms ")
                .clamp_range(1..=10_000),
        );
        continuous = continuous_widget_edit(&response, "hover_dwell_ms");
    }
    enum_combo(
        ui,
        "Open child menus",
        &mut menu.submenu_presentation,
        &[
            ("Cascade", SubmenuPresentation::Cascade),
            ("Same center", SubmenuPresentation::SameCenter),
        ],
    );
    after_action_combo(ui, "Menu after action", &mut menu.after_action);
    let center_response = ui.add(
        egui::DragValue::new(&mut menu.center_radius)
            .prefix("Center radius ")
            .speed(1.0),
    );
    set_first_edit(
        &mut continuous,
        continuous_widget_edit(&center_response, "center_radius"),
    );
    ui.checkbox(
        &mut menu.mirror_primary_to_secondary,
        "Mirror primary to secondary",
    );
    persisted_section(
        ui,
        "Center mappings",
        &format!("menu:{}:center-mappings", menu.id),
        expanded_sections,
        |ui| {
            let before = menu.center_control;
            optional_control_combo(ui, "Primary control", &mut menu.center_control);
            if menu.center_control.is_some() && menu.center_control != before {
                menu.center_action = None;
            }
            if menu.center_action.is_some() && ui.button("Clear primary action").clicked() {
                menu.center_action = None;
            }
            after_action_combo(
                ui,
                "Primary after action",
                &mut menu.center_primary_after_action,
            );
            let before = menu.center_secondary_control;
            optional_control_combo(ui, "Secondary control", &mut menu.center_secondary_control);
            if menu.center_secondary_control.is_some() && menu.center_secondary_control != before {
                menu.center_secondary_action = None;
            }
            if menu.center_secondary_action.is_some()
                && ui.button("Clear secondary action").clicked()
            {
                menu.center_secondary_action = None;
            }
            after_action_combo(
                ui,
                "Secondary after action",
                &mut menu.center_secondary_after_action,
            );
            ui.small(if menu.center_action.is_some() {
                "Primary action assigned"
            } else {
                "No primary action"
            });
            ui.small(if menu.center_secondary_action.is_some() {
                "Secondary action assigned"
            } else {
                "No secondary action"
            });
        },
    );
    persisted_section(
        ui,
        "Background mappings",
        &format!("menu:{}:background-mappings", menu.id),
        expanded_sections,
        |ui| {
            let before = menu.background_control;
            optional_control_combo(ui, "Primary control", &mut menu.background_control);
            if menu.background_control.is_some() && menu.background_control != before {
                menu.background_action = None;
            }
            if menu.background_action.is_some() && ui.button("Clear primary action").clicked() {
                menu.background_action = None;
            }
            after_action_combo(
                ui,
                "Primary after action",
                &mut menu.background_primary_after_action,
            );
            let before = menu.background_secondary_control;
            optional_control_combo(
                ui,
                "Secondary control",
                &mut menu.background_secondary_control,
            );
            if menu.background_secondary_control.is_some()
                && menu.background_secondary_control != before
            {
                menu.background_secondary_action = None;
            }
            if menu.background_secondary_action.is_some()
                && ui.button("Clear secondary action").clicked()
            {
                menu.background_secondary_action = None;
            }
            after_action_combo(
                ui,
                "Secondary after action",
                &mut menu.background_secondary_after_action,
            );
            ui.small(if menu.background_action.is_some() {
                "Primary action assigned"
            } else {
                "No primary action"
            });
            ui.small(if menu.background_secondary_action.is_some() {
                "Secondary action assigned"
            } else {
                "No secondary action"
            });
        },
    );
    continuous
}

fn enum_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    choices: &[(&str, T)],
) {
    let selected = choices
        .iter()
        .find(|(_, item)| item == value)
        .map_or("Unknown", |(label, _)| *label);
    egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (label, choice) in choices {
                ui.selectable_value(value, *choice, *label);
            }
        });
}

fn after_action_combo(ui: &mut egui::Ui, label: &str, value: &mut AfterActionPolicy) {
    enum_combo(
        ui,
        label,
        value,
        &[
            ("Inherit", AfterActionPolicy::Inherit),
            ("Keep open", AfterActionPolicy::KeepOpen),
            ("Close current", AfterActionPolicy::CloseCurrentMenu),
            ("Close tree", AfterActionPolicy::CloseTree),
        ],
    );
}

fn optional_control_combo(ui: &mut egui::Ui, label: &str, value: &mut Option<Control>) {
    enum_combo(
        ui,
        label,
        value,
        &[
            ("None", None),
            ("Back", Some(Control::Back)),
            ("Close", Some(Control::Close)),
            ("Next page", Some(Control::NextPage)),
            ("Previous page", Some(Control::PreviousPage)),
            ("Drag", Some(Control::Drag)),
        ],
    );
}

fn cell_content_controls(
    ui: &mut egui::Ui,
    document: &RadialDocument,
    cell: &mut crate::radial::model::CellDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
) -> Option<ContinuousWidgetEdit> {
    let mut continuous = None;
    if ui.button("Dynamic source").clicked() && !matches!(cell.content, CellContent::Dynamic { .. })
    {
        cell.content = CellContent::Dynamic {
            source: DynamicSource::Favorites,
        };
    }
    if ui.button("Control").clicked() && !matches!(cell.content, CellContent::Control { .. }) {
        cell.content = CellContent::Control {
            control: Control::Back,
        };
    }
    if ui.button("Submenu").clicked()
        && !matches!(cell.content, CellContent::Submenu { .. })
        && let Some(menu) = document.menus.first()
    {
        cell.content = CellContent::Submenu {
            menu_id: menu.id.clone(),
        };
    }
    match &mut cell.content {
        CellContent::Dynamic { source } => {
            let mut key = dynamic_source_key(source);
            enum_combo(
                ui,
                "Source",
                &mut key,
                &[
                    ("Favorites", 0),
                    ("Recent", 1),
                    ("Clipboard", 2),
                    ("Snippets", 3),
                    ("Notes", 4),
                    ("Windows", 5),
                    ("Macros", 6),
                    ("Applications", 7),
                    ("Dashboard", 8),
                    ("Launcher results", 9),
                    ("Launcher query", 10),
                ],
            );
            if key != dynamic_source_key(source) {
                *source = dynamic_source_for_key(key);
            }
            match source {
                DynamicSource::LauncherResults { max_items } => {
                    let response = ui.add(
                        egui::DragValue::new(max_items)
                            .prefix("Maximum ")
                            .clamp_range(1..=100),
                    );
                    continuous = continuous_widget_edit(&response, "dynamic.max_items");
                }
                DynamicSource::LauncherQuery { query, max_items } => {
                    let query_response = ui
                        .text_edit_singleline(query)
                        .on_hover_text("Read-only launcher query");
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&query_response, "dynamic.query"),
                    );
                    let maximum_response = ui.add(
                        egui::DragValue::new(max_items)
                            .prefix("Maximum ")
                            .clamp_range(1..=100),
                    );
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&maximum_response, "dynamic.max_items"),
                    );
                }
                _ => {}
            }
            ui.small("Runtime status is shown truthfully as loading, unavailable, empty, action, or manage rows.");
        }
        CellContent::Control { control } => enum_combo(
            ui,
            "Control",
            control,
            &[
                ("Back", Control::Back),
                ("Close", Control::Close),
                ("Next page", Control::NextPage),
                ("Previous page", Control::PreviousPage),
                ("Drag", Control::Drag),
            ],
        ),
        CellContent::Submenu { menu_id } => {
            egui::ComboBox::from_label("Link existing submenu")
                .selected_text(
                    document
                        .menus
                        .iter()
                        .find(|menu| menu.id == *menu_id)
                        .map_or_else(|| menu_id.to_string(), |menu| menu.name.clone()),
                )
                .show_ui(ui, |ui| {
                    for menu in &document.menus {
                        ui.selectable_value(menu_id, menu.id.clone(), &menu.name);
                    }
                });
            ui.small("Use ‘Duplicate subtree’ on the linked menu to create an independent reusable submenu graph.");
        }
        CellContent::Action { .. } => {
            ui.small("Primary action is assigned from the Universal Action picker below.");
        }
        CellContent::Spacer => {
            ui.small("Spacer (no action)");
        }
    };
    persisted_section(
        ui,
        "Alternate clicks and controls",
        &format!("cell:{}:alternate-controls", cell.id),
        expanded_sections,
        |ui| {
            let mut remove_click = None;
            for (index, binding) in cell.alternate_clicks.iter_mut().enumerate() {
                enum_combo(
                    ui,
                    "Gesture",
                    &mut binding.gesture,
                    &[
                        ("Primary", crate::radial::model::ClickGesture::Primary),
                        ("Secondary", crate::radial::model::ClickGesture::Secondary),
                        (
                            "Ctrl+primary",
                            crate::radial::model::ClickGesture::CtrlPrimary,
                        ),
                        (
                            "Shift+primary",
                            crate::radial::model::ClickGesture::ShiftPrimary,
                        ),
                        (
                            "Alt+primary",
                            crate::radial::model::ClickGesture::AltPrimary,
                        ),
                    ],
                );
                after_action_combo(ui, "After action", &mut binding.after_action);
                if ui.button("Remove alternate action").clicked() {
                    remove_click = Some(index);
                }
            }
            if let Some(index) = remove_click {
                cell.alternate_clicks.remove(index);
            }
            let mut remove_control = None;
            for (index, binding) in cell.alternate_controls.iter_mut().enumerate() {
                enum_combo(
                    ui,
                    "Control gesture",
                    &mut binding.gesture,
                    &[
                        ("Secondary", crate::radial::model::ClickGesture::Secondary),
                        (
                            "Ctrl+primary",
                            crate::radial::model::ClickGesture::CtrlPrimary,
                        ),
                    ],
                );
                enum_combo(
                    ui,
                    "Control",
                    &mut binding.control,
                    &[
                        ("Back", Control::Back),
                        ("Close", Control::Close),
                        ("Drag", Control::Drag),
                    ],
                );
                if ui.button("Remove alternate control").clicked() {
                    remove_control = Some(index);
                }
            }
            if let Some(index) = remove_control {
                cell.alternate_controls.remove(index);
            }
            if ui.button("Add alternate control").clicked() {
                let gesture = [
                    crate::radial::model::ClickGesture::Secondary,
                    crate::radial::model::ClickGesture::CtrlPrimary,
                    crate::radial::model::ClickGesture::ShiftPrimary,
                    crate::radial::model::ClickGesture::AltPrimary,
                ]
                .into_iter()
                .find(|gesture| {
                    !cell
                        .alternate_controls
                        .iter()
                        .any(|entry| entry.gesture == *gesture)
                        && !cell
                            .alternate_clicks
                            .iter()
                            .any(|entry| entry.gesture == *gesture)
                });
                if let Some(gesture) = gesture {
                    cell.alternate_controls
                        .push(crate::radial::model::ControlClickBinding {
                            gesture,
                            control: Control::Back,
                        });
                }
            }
            let control_gestures = cell
                .alternate_controls
                .iter()
                .map(|entry| entry.gesture)
                .collect::<std::collections::BTreeSet<_>>();
            cell.alternate_clicks
                .retain(|entry| !control_gestures.contains(&entry.gesture));
        },
    );
    continuous
}

fn dynamic_source_key(source: &DynamicSource) -> u8 {
    match source {
        DynamicSource::Favorites => 0,
        DynamicSource::RecentItems => 1,
        DynamicSource::Clipboard => 2,
        DynamicSource::Snippets => 3,
        DynamicSource::Notes => 4,
        DynamicSource::Windows => 5,
        DynamicSource::Macros => 6,
        DynamicSource::Applications => 7,
        DynamicSource::Dashboard => 8,
        DynamicSource::LauncherResults { .. } => 9,
        DynamicSource::LauncherQuery { .. } => 10,
    }
}

fn dynamic_source_for_key(key: u8) -> DynamicSource {
    match key {
        0 => DynamicSource::Favorites,
        1 => DynamicSource::RecentItems,
        2 => DynamicSource::Clipboard,
        3 => DynamicSource::Snippets,
        4 => DynamicSource::Notes,
        5 => DynamicSource::Windows,
        6 => DynamicSource::Macros,
        7 => DynamicSource::Applications,
        8 => DynamicSource::Dashboard,
        9 => DynamicSource::LauncherResults { max_items: 12 },
        _ => DynamicSource::LauncherQuery {
            query: String::new(),
            max_items: 12,
        },
    }
}

fn cell_media_controls(
    ui: &mut egui::Ui,
    document: &RadialDocument,
    cell: &mut crate::radial::model::CellDefinition,
    intent_bridge: &DesignerIntentBridge,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
) -> Option<ContinuousWidgetEdit> {
    use crate::radial::model::{MediaKind, MediaReference, Override};
    let mut continuous = None;
    persisted_section(
        ui,
        "Icon and tooltip",
        &format!("cell:{}:icon-tooltip", cell.id),
        expanded_sections,
        |ui| {
            ui.horizontal(|ui| {
                ui.label("Tooltip");
                if ui.button("Inherit").clicked() {
                    cell.tooltip = Override::Inherit;
                }
                if ui.button("Clear").clicked() {
                    cell.tooltip = Override::Clear;
                }
                if ui.button("Set").clicked() && !matches!(cell.tooltip, Override::Value(_)) {
                    cell.tooltip = Override::Value(cell.label.clone());
                }
                if let Override::Value(value) = &mut cell.tooltip {
                    let response = ui.text_edit_singleline(value);
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "tooltip"),
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label("Icon");
                if ui.button("Inherit").clicked() {
                    cell.icon = Override::Inherit;
                }
                if ui.button("Clear").clicked() {
                    cell.icon = Override::Clear;
                }
                if ui.button("External file").clicked() {
                    intent_bridge.enqueue_file_dialog(
                        DesignerFileDialogRequest::CellExternalIcon {
                            menu_id: menu_id.clone(),
                            ring_id: ring_id.clone(),
                            cell_id: cell_id.clone(),
                        },
                    );
                }
                if ui.button("Search path").clicked() {
                    cell.icon = Override::Value(MediaReference::SearchPath {
                        file_name: "icon.png".into(),
                    });
                }
                if ui.button("Icon resource").clicked() {
                    cell.icon = Override::Value(MediaReference::IconResource {
                        path: "shell32.dll".into(),
                        index: 1,
                    });
                }
            });
            let managed: Vec<_> = document
                .assets
                .iter()
                .filter(|asset| asset.kind == MediaKind::Image)
                .map(|asset| asset.id.clone())
                .collect();
            if !managed.is_empty() {
                let mut selected = match &cell.icon {
                    Override::Value(MediaReference::Managed { asset_id }) => Some(asset_id.clone()),
                    _ => None,
                };
                egui::ComboBox::from_label("Managed icon")
                    .selected_text(selected.as_ref().map_or("Choose asset", |id| id.as_str()))
                    .show_ui(ui, |ui| {
                        for id in managed {
                            ui.selectable_value(&mut selected, Some(id.clone()), id.as_str());
                        }
                    });
                if let Some(asset_id) = selected {
                    cell.icon = Override::Value(MediaReference::Managed { asset_id });
                }
            }
            match &mut cell.icon {
                Override::Value(MediaReference::SearchPath { file_name }) => {
                    let response = ui.text_edit_singleline(file_name);
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "icon.search_path"),
                    );
                }
                Override::Value(MediaReference::ExternalFile { path }) => {
                    let response = ui.text_edit_singleline(path);
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&response, "icon.external_path"),
                    );
                }
                Override::Value(MediaReference::IconResource { path, index }) => {
                    let path_response = ui.text_edit_singleline(path);
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&path_response, "icon.resource_path"),
                    );
                    let index_response =
                        ui.add(egui::DragValue::new(index).prefix("Resource index "));
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&index_response, "icon.resource_index"),
                    );
                }
                _ => {}
            }
        },
    );
    continuous
}

fn cell_trigger_controls(
    ui: &mut egui::Ui,
    session: &mut RadialAuthoringSession,
    cell: &mut crate::radial::model::CellDefinition,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
) -> Option<ContinuousWidgetEdit> {
    let mut continuous = None;
    persisted_section(
        ui,
        "Shortcuts and hotstrings",
        &format!("cell:{}:shortcuts-hotstrings", cell.id),
        expanded_sections,
        |ui| {
            let mut remove_shortcut = None;
            for (index, shortcut) in cell.shortcuts.iter_mut().enumerate() {
                let response = ui.text_edit_singleline(&mut shortcut.chord);
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("shortcut:{}.chord", shortcut.id)),
                );
                enum_combo(
                    ui,
                    "Scope",
                    &mut shortcut.scope,
                    &[
                        ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                        ("Global", crate::radial::model::TriggerScope::Global),
                    ],
                );
                if ui.button("Remove shortcut").clicked() {
                    remove_shortcut = Some(index);
                }
            }
            if let Some(index) = remove_shortcut {
                cell.shortcuts.remove(index);
            }
            if ui.button("Add shortcut").clicked() {
                cell.shortcuts.push(crate::radial::model::ItemShortcut {
                    id: session.allocate_shortcut_id("shortcut"),
                    chord: "Ctrl+Alt+1".into(),
                    gesture: crate::radial::model::ClickGesture::Primary,
                    scope: crate::radial::model::TriggerScope::MenuLocal,
                });
            }
            let mut remove_hotstring = None;
            for (index, hotstring) in cell.hotstrings.iter_mut().enumerate() {
                let response = ui.text_edit_singleline(&mut hotstring.text);
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("hotstring:{}.text", hotstring.id)),
                );
                ui.checkbox(&mut hotstring.case_sensitive, "Case sensitive");
                enum_combo(
                    ui,
                    "Scope",
                    &mut hotstring.scope,
                    &[
                        ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                        ("Global", crate::radial::model::TriggerScope::Global),
                    ],
                );
                if ui.button("Remove hotstring").clicked() {
                    remove_hotstring = Some(index);
                }
            }
            if let Some(index) = remove_hotstring {
                cell.hotstrings.remove(index);
            }
            if ui.button("Add hotstring").clicked() {
                cell.hotstrings.push(crate::radial::model::ItemHotstring {
                    id: session.allocate_hotstring_id("hotstring"),
                    text: ";radial".into(),
                    gesture: crate::radial::model::ClickGesture::Primary,
                    case_sensitive: false,
                    scope: crate::radial::model::TriggerScope::MenuLocal,
                });
            }
        },
    );
    continuous
}

fn document_rule_controls(
    ui: &mut egui::Ui,
    session: &mut RadialAuthoringSession,
    expanded_sections: &mut std::collections::BTreeMap<String, bool>,
) {
    let mut document = (*session.draft).clone();
    let original = document.clone();
    let mut continuous: Option<(String, ContinuousWidgetEdit)> = None;
    let menu_choices: Vec<_> = document
        .menus
        .iter()
        .map(|menu| (menu.id.clone(), menu.name.clone()))
        .collect();
    persisted_section(
        ui,
        "Context rules",
        "document:context-rules",
        expanded_sections,
        |ui| {
            let mut remove = None;
            for (index, rule) in document.context_rules.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.checkbox(&mut rule.enabled, "Enabled");
                    let priority_response =
                        ui.add(egui::DragValue::new(&mut rule.priority).prefix("Priority "));
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&priority_response, "priority")
                            .map(|edit| (format!("context-rule:{}", rule.id), edit)),
                    );
                    ui.label(format!("ID: {}", rule.id));
                    let entity = format!("context-rule:{}", rule.id);
                    let process_edit =
                        optional_text(ui, "Process", &mut rule.process_name).map(|signals| {
                            ContinuousWidgetEdit {
                                field: "process_name".into(),
                                signals,
                            }
                        });
                    set_first_edit(
                        &mut continuous,
                        process_edit.map(|edit| (entity.clone(), edit)),
                    );
                    let title_edit =
                        optional_text(ui, "Window title contains", &mut rule.window_title_contains)
                            .map(|signals| ContinuousWidgetEdit {
                                field: "window_title_contains".into(),
                                signals,
                            });
                    set_first_edit(
                        &mut continuous,
                        title_edit.map(|edit| (entity.clone(), edit)),
                    );
                    let monitor_edit =
                        optional_text(ui, "Monitor ID", &mut rule.monitor_id).map(|signals| {
                            ContinuousWidgetEdit {
                                field: "monitor_id".into(),
                                signals,
                            }
                        });
                    set_first_edit(&mut continuous, monitor_edit.map(|edit| (entity, edit)));
                    egui::ComboBox::from_label("Menu")
                        .selected_text(rule.menu_id.to_string())
                        .show_ui(ui, |ui| {
                            for (id, name) in &menu_choices {
                                ui.selectable_value(&mut rule.menu_id, id.clone(), name);
                            }
                        });
                    if ui.button("Remove rule").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                document.context_rules.remove(index);
            }
            if ui.button("Add context rule").clicked()
                && let Some(menu) = document.menus.first()
            {
                let id = session.allocate_context_rule_id("context");
                let menu_id = menu.id.clone();
                document
                    .context_rules
                    .push(crate::radial::model::ContextRule {
                        id,
                        enabled: true,
                        priority: 0,
                        process_name: None,
                        window_title_contains: None,
                        monitor_id: None,
                        menu_id,
                    });
            }
        },
    );
    persisted_section(
        ui,
        "Custom triggers",
        "document:custom-triggers",
        expanded_sections,
        |ui| {
            let mut remove = None;
            for (index, trigger) in document.custom_triggers.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.label(format!("ID: {}", trigger.id));
                    let chord_response = ui.text_edit_singleline(&mut trigger.chord);
                    set_first_edit(
                        &mut continuous,
                        continuous_widget_edit(&chord_response, "chord")
                            .map(|edit| (format!("custom-trigger:{}", trigger.id), edit)),
                    );
                    enum_combo(
                        ui,
                        "Scope",
                        &mut trigger.scope,
                        &[
                            ("Menu local", crate::radial::model::TriggerScope::MenuLocal),
                            ("Global", crate::radial::model::TriggerScope::Global),
                        ],
                    );
                    egui::ComboBox::from_label("Menu")
                        .selected_text(trigger.menu_id.to_string())
                        .show_ui(ui, |ui| {
                            for (id, name) in &menu_choices {
                                ui.selectable_value(&mut trigger.menu_id, id.clone(), name);
                            }
                        });
                    if ui.button("Remove trigger").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                document.custom_triggers.remove(index);
            }
            if ui.button("Add custom trigger").clicked()
                && let Some(menu) = document.menus.first()
            {
                let id = session.allocate_trigger_id("trigger");
                let chord = "Ctrl+Shift+1".into();
                let menu_id = menu.id.clone();
                document
                    .custom_triggers
                    .push(crate::radial::model::TriggerDefinition {
                        id,
                        chord,
                        menu_id,
                        scope: crate::radial::model::TriggerScope::MenuLocal,
                    });
            }
        },
    );
    persisted_section(
        ui,
        "Media search paths",
        "document:media-search-paths",
        expanded_sections,
        |ui| {
            for (index, root) in document
                .media_search_roots
                .image_directories
                .iter_mut()
                .enumerate()
            {
                let response = ui.text_edit_singleline(root);
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("image_directories[{index}]"))
                        .map(|edit| ("media-search-roots".into(), edit)),
                );
            }
            if ui.button("Add image search path").clicked() {
                document
                    .media_search_roots
                    .image_directories
                    .push(String::new());
            }
            for (index, root) in document
                .media_search_roots
                .sound_directories
                .iter_mut()
                .enumerate()
            {
                let response = ui.text_edit_singleline(root);
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&response, format!("sound_directories[{index}]"))
                        .map(|edit| ("media-search-roots".into(), edit)),
                );
            }
            if ui.button("Add sound search path").clicked() {
                document
                    .media_search_roots
                    .sound_directories
                    .push(String::new());
            }
            ui.checkbox(
                &mut document.media_search_roots.search_windows_media_for_sounds,
                "Search Windows media for sounds",
            );
        },
    );
    if document != original || continuous.is_some() {
        if let Some((entity, edit)) = continuous {
            let _ =
                dispatch_widget_document_edit(session, document, entity, &edit.field, edit.signals);
        } else {
            let _ = session.replace_document_atomic(document);
        }
    }
}

fn optional_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
) -> Option<WidgetEditSignals> {
    let mut enabled = value.is_some();
    let mut signals = None;
    ui.horizontal(|ui| {
        if ui.checkbox(&mut enabled, label).changed() {
            *value = enabled.then(String::new);
        }
        if let Some(value) = value.as_mut() {
            let response = ui.text_edit_singleline(value);
            if response.changed() || response.lost_focus() || response.drag_stopped() {
                signals = Some(WidgetEditSignals::from_response(&response));
            }
        }
    });
    signals
}

fn relocate_and_delete_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    source_ring: &RingId,
    destination_ring: &RingId,
) -> Result<(), String> {
    if source_ring == destination_ring {
        return Err("source and destination ring are identical".into());
    }
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or("menu missing")?;
    if menu.rings.len() <= 1 {
        return Err("cannot delete the last ring".into());
    }
    let source_index = menu
        .rings
        .iter()
        .position(|ring| &ring.id == source_ring)
        .ok_or("source ring missing")?;
    let cells = menu.rings[source_index].cells.clone();
    let destination = menu
        .rings
        .iter_mut()
        .find(|ring| &ring.id == destination_ring)
        .ok_or("destination ring missing")?;
    destination.cells.extend(cells);
    menu.rings.remove(source_index);
    session
        .replace_document_atomic(document)
        .map_err(|error| format!("{error:?}"))
}

#[derive(Clone, Copy)]
enum MenuActionSlot {
    CenterPrimary,
    CenterSecondary,
    BackgroundPrimary,
    BackgroundSecondary,
}

fn assign_menu_action(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    slot: MenuActionSlot,
    binding: crate::radial::model::ActionBinding,
) {
    let mut document = (*session.draft).clone();
    if let Some(menu) = document.menus.iter_mut().find(|menu| &menu.id == menu_id) {
        match slot {
            MenuActionSlot::CenterPrimary => {
                menu.center_action = Some(binding);
                menu.center_control = None;
            }
            MenuActionSlot::CenterSecondary => {
                menu.center_secondary_action = Some(binding);
                menu.center_secondary_control = None;
            }
            MenuActionSlot::BackgroundPrimary => {
                menu.background_action = Some(binding);
                menu.background_control = None;
            }
            MenuActionSlot::BackgroundSecondary => {
                menu.background_secondary_action = Some(binding);
                menu.background_secondary_control = None;
            }
        }
        let _ = session.replace_document_atomic(document);
    }
}

fn assign_alternate_click(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    gesture: crate::radial::model::ClickGesture,
    action: crate::radial::model::ActionBinding,
) {
    let mut document = (*session.draft).clone();
    if let Some(cell) = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .and_then(|menu| menu.rings.iter_mut().find(|ring| &ring.id == ring_id))
        .and_then(|ring| ring.cells.iter_mut().find(|cell| &cell.id == cell_id))
    {
        cell.alternate_controls
            .retain(|entry| entry.gesture != gesture);
        if let Some(existing) = cell
            .alternate_clicks
            .iter_mut()
            .find(|entry| entry.gesture == gesture)
        {
            existing.action = action;
        } else {
            cell.alternate_clicks
                .push(crate::radial::model::ClickBinding {
                    gesture,
                    action,
                    after_action: AfterActionPolicy::Inherit,
                });
        }
        let _ = session.replace_document_atomic(document);
    }
}

fn preview_legacy_files(
    paths: &[std::path::PathBuf],
    source: crate::radial::compatibility::CompatibilitySource,
    session: &RadialAuthoringSession,
) -> Result<PendingImport, String> {
    let mut owned = Vec::new();
    let mut total = 0usize;
    let common_parent = paths.first().and_then(|path| path.parent());
    for path in paths {
        let bytes =
            asset_picker::read_bounded(path, crate::radial::model::limits::MAX_IMPORT_BYTES)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("legacy input byte count overflow")?;
        if total > crate::radial::model::limits::MAX_IMPORT_BYTES as usize {
            return Err("legacy inputs exceed the import byte budget".into());
        }
        let relative = common_parent
            .and_then(|parent| path.strip_prefix(parent).ok())
            .unwrap_or(path.as_path());
        let name = relative
            .to_str()
            .ok_or("legacy input has a non-Unicode file name")?
            .replace('\\', "/");
        owned.push((name, bytes));
    }
    let inputs = owned
        .iter()
        .map(|(name, bytes)| crate::radial::import::ImportInput {
            relative_path: name,
            bytes,
            evidence: crate::radial::import::ImportEvidence::UserSelected,
        })
        .collect::<Vec<_>>();
    let menu_ids = session
        .draft
        .menus
        .iter()
        .map(|menu| menu.id.as_str().to_owned())
        .collect();
    let skin_ids = session
        .draft
        .skins
        .iter()
        .map(|skin| skin.id.as_str().to_owned())
        .collect();
    let preview = match source {
        crate::radial::compatibility::CompatibilitySource::Radify => {
            crate::radial::import::preview_radify(&inputs, "Imported Radify", &menu_ids, &skin_ids)
        }
        crate::radial::compatibility::CompatibilitySource::RadialMenuV4 => {
            crate::radial::import::preview_rm4(&inputs, "Imported RM4", &menu_ids, &skin_ids)
        }
    };
    Ok(PendingImport::legacy(preview, session))
}

fn open_designer_file_dialog(request: &DesignerFileDialogRequest) -> DesignerFileDialogResult {
    match request {
        DesignerFileDialogRequest::StyleMedia { .. }
        | DesignerFileDialogRequest::CellExternalIcon { .. } => {
            rfd::FileDialog::new().pick_file().map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            )
        }
        DesignerFileDialogRequest::ImportManaged {
            label, extensions, ..
        } => {
            let extensions = extensions.iter().map(String::as_str).collect::<Vec<_>>();
            rfd::FileDialog::new()
                .add_filter(label, &extensions)
                .pick_file()
                .map_or(
                    DesignerFileDialogResult::Cancelled,
                    DesignerFileDialogResult::File,
                )
        }
        DesignerFileDialogRequest::PreviewPackage => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .pick_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportMenu { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("menu.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportPackage { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("all-radial-menus.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::ExportSkin { .. } => rfd::FileDialog::new()
            .add_filter("Multi Launcher radial", &["mlradial"])
            .set_file_name("skin.mlradial")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
        DesignerFileDialogRequest::PreviewLegacy { .. } => {
            rfd::FileDialog::new().pick_files().map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::Files,
            )
        }
        DesignerFileDialogRequest::ReplaceBackup => rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("radial-before-import.json")
            .save_file()
            .map_or(
                DesignerFileDialogResult::Cancelled,
                DesignerFileDialogResult::File,
            ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_designer_layout_is_canvas_first_and_panes_fit_compact_windows() {
        let preferences = crate::settings::RadialDesignerPreferences::default();
        assert!(!preferences.tree_visible);
        assert!(!preferences.inspector_visible);
        let canvas_only = designer_pane_layout(
            egui::vec2(900.0, 650.0),
            preferences.tree_visible,
            preferences.inspector_visible,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(canvas_only.canvas_width >= 899.0);
        let compact = designer_pane_layout(
            egui::vec2(700.0, 470.0),
            true,
            true,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(compact.canvas_width > 0.0);
        assert!(compact.height <= 470.0);
        assert!(
            compact.tree_width + compact.canvas_width + compact.inspector_width + 12.0 <= 700.01
        );
        let one_pane = designer_pane_layout(
            egui::vec2(700.0, 470.0),
            true,
            false,
            preferences.tree_width,
            preferences.inspector_width,
        );
        assert!(one_pane.tree_width + one_pane.canvas_width + 6.0 <= 700.01);
    }

    #[test]
    fn resetting_designer_layout_does_not_touch_dirty_document_or_selection() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.make_dirty_for_test();
        let selection = StableSelection::Menu(
            editor
                .session
                .as_ref()
                .expect("session")
                .draft
                .default_menu_id
                .clone(),
        );
        editor
            .session
            .as_mut()
            .expect("session")
            .select(Some(selection.clone()));
        let before_draft = editor.session.as_ref().unwrap().draft.clone();
        let before_generation = editor.session.as_ref().unwrap().generation;
        editor.tree_visible = true;
        editor.inspector_visible = true;
        editor.tree_width = 410.0;
        editor.inspector_width = 540.0;
        editor.preview_zoom = 1.8;
        editor.canvas_pan = CanvasPoint::new(22.0, -11.0);
        editor
            .preferences
            .expanded_sections
            .insert("menu:starter".into(), true);
        let tree_epoch = editor.tree_widget_epoch;
        editor.reset_designer_layout();
        let session = editor.session.as_ref().unwrap();
        assert_eq!(session.draft, before_draft);
        assert_eq!(session.generation, before_generation);
        assert_eq!(session.selection, Some(selection));
        assert!(!editor.tree_visible);
        assert!(!editor.inspector_visible);
        assert_eq!(editor.preview_zoom, 1.0);
        assert_eq!(editor.canvas_pan, CanvasPoint::default());
        assert!(editor.preferences.expanded_sections.is_empty());
        assert_ne!(editor.tree_widget_epoch, tree_epoch);
        assert!(editor.preferences_dirty);
    }

    #[test]
    fn designer_viewport_identity_and_duplicate_open_reuse_one_draft() {
        assert_eq!(radial_designer_viewport_id(), radial_designer_viewport_id());
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.make_dirty_for_test();
        let generation = editor.session.as_ref().unwrap().generation;
        editor.open();
        assert_eq!(editor.session.as_ref().unwrap().generation, generation);
        assert!(editor.is_dirty());
        editor.open_skins();
        assert!(editor.is_showing_resources());
    }

    #[test]
    fn cancelling_center_placement_keeps_the_authoring_document_unchanged() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let (menu_id, ring_id, cell_id, generation, before) = {
            let session = editor.session.as_ref().unwrap();
            (
                session.draft.menus[0].id.clone(),
                session.draft.menus[0].rings[0].id.clone(),
                session.draft.menus[0].rings[0].cells[0].id.clone(),
                session.generation,
                session.draft.clone(),
            )
        };
        editor.placement_draft = Some(PlacementDraft::new(menu_id, ring_id, cell_id, generation));
        editor.placement_draft = None;
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
    }

    #[test]
    fn closing_during_preview_preparation_disposes_the_pending_viewport_state() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_mut().unwrap();
        session.pending_request = Some(crate::radial::authoring::PendingAuthoringRequest {
            id: crate::radial::authoring::AuthoringRequestId(42),
            generation: session.generation,
            editor_session: session.editor_session,
            kind: crate::radial::authoring::PendingRequestKind::PrepareEmbeddedPreview,
        });

        editor.request_close();

        assert!(!editor.open);
        assert!(editor.session.is_none());
        assert!(editor.viewport_close_pending);
    }

    #[test]
    fn closing_pending_native_preview_supersedes_with_terminal_stop() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);
        let start = {
            let session = editor.session.as_mut().unwrap();
            session
                .request_start_native_preview(session.draft.default_menu_id.clone(), false, None)
                .unwrap()
        };
        editor.client.as_ref().unwrap().send(start).unwrap();

        editor.request_close();

        let queued_start = endpoint.request_rx.try_recv().expect("start request");
        let queued_stop = endpoint.request_rx.try_recv().expect("stop request");
        assert!(matches!(
            queued_start,
            crate::radial::authoring::AuthoringRequest::StartNativePreview { .. }
        ));
        assert!(matches!(
            queued_stop,
            crate::radial::authoring::AuthoringRequest::StopNativePreview { .. }
        ));
        assert!(editor.open);
        assert_eq!(
            editor
                .session
                .as_ref()
                .and_then(|session| session.pending_native_preview)
                .map(|pending| pending.kind),
            Some(crate::radial::authoring::PendingRequestKind::StopNativePreview)
        );
    }

    #[test]
    fn invalidated_native_update_syncs_terminal_stop_before_recovery_update() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);

        editor.send_native_preview(false);
        let start = endpoint.request_rx.try_recv().expect("start request");
        let start_lease = crate::radial::authoring::NativePreviewLease {
            editor_session: start.editor_session(),
            generation: start.generation(),
            request_id: start.id(),
        };
        assert!(matches!(
            &start,
            crate::radial::authoring::AuthoringRequest::StartNativePreview { .. }
        ));
        assert!(editor.session.as_mut().unwrap().accept_reply(
            crate::radial::authoring::AuthoringReply::NativePreviewStarted {
                id: start.id(),
                generation: start.generation(),
                editor_session: start.editor_session(),
                lease: start_lease,
                sampled_context: crate::radial::context::InvocationContext::empty(1),
                diagnostics: Vec::new(),
            }
        ));

        editor.send_native_preview(true);
        let update = endpoint.request_rx.try_recv().expect("update request");
        assert!(matches!(
            &update,
            crate::radial::authoring::AuthoringRequest::UpdateNativePreview { .. }
        ));
        let menu_id = editor
            .session
            .as_ref()
            .expect("session")
            .draft
            .default_menu_id
            .clone();
        editor
            .session
            .as_mut()
            .expect("session")
            .mutate(
                crate::radial::authoring::DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Edited while update is in flight".into(),
                },
                None,
                crate::radial::authoring::EditPhase::Atomic,
            )
            .unwrap();
        let session = editor.session.as_ref().unwrap();
        assert!(session.pending_native_preview.is_none());
        assert!(session.native_preview_lease.is_none());
        assert!(session.native_preview_may_be_open);

        editor.sync_native_preview_generation();
        let queued_stop = endpoint.request_rx.try_recv().expect("stop request");
        assert!(matches!(
            queued_stop,
            crate::radial::authoring::AuthoringRequest::StopNativePreview { .. }
        ));
        assert!(matches!(
            endpoint.request_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert_eq!(
            editor
                .session
                .as_ref()
                .and_then(|session| session.pending_native_preview)
                .map(|pending| pending.kind),
            Some(crate::radial::authoring::PendingRequestKind::StopNativePreview)
        );
    }

    #[test]
    fn discard_close_terminalizes_disconnected_preview_stop_without_retrying() {
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        editor.client = Some(client);
        editor.close_intent = CloseIntent::DiscardRequested;
        editor
            .session
            .as_mut()
            .expect("session")
            .native_preview_may_be_open = true;
        drop(endpoint);

        editor.stop_native_preview();
        assert!(editor.close_stop_attempted);
        assert!(
            editor
                .session
                .as_ref()
                .is_some_and(|session| !session.native_preview_may_be_open)
        );
        editor.maybe_finish_close();
        assert!(!editor.open);
        assert!(editor.session.is_none());
        let close_requests = editor.close_stop_attempted;
        editor.maybe_finish_close();
        assert_eq!(editor.close_stop_attempted, close_requests);
    }

    #[test]
    fn queued_test_intents_wake_the_root_owner_without_draining_in_the_viewport() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let wake_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback_count = std::sync::Arc::clone(&wake_count);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            callback_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        bridge.push(DesignerUiIntent::TestAction {
            binding: crate::radial::model::ActionBinding::Contextual {
                selector: crate::radial::model::TargetSelector::CapturedForeground,
                action_id: crate::universal_actions::ActionId::new("test"),
            },
            invocation: InvocationContext::empty(0),
            history_query: String::new(),
        });
        assert_eq!(wake_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(bridge.drain().len(), 1);
    }

    #[test]
    fn file_dialog_enqueue_and_return_wake_the_correct_owners() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let root_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let viewport_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let root_count = std::sync::Arc::clone(&root_wakes);
        let root_bridge = std::sync::Arc::clone(&bridge);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            // The callback can inspect the queue: enqueue has released the
            // queue lock before waking ROOT.
            assert!(root_bridge.has_pending_file_dialog());
            root_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        let viewport_count = std::sync::Arc::clone(&viewport_wakes);
        bridge.set_viewport_wake(Some(std::sync::Arc::new(move || {
            viewport_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        bridge.enqueue_file_dialog(DesignerFileDialogRequest::PreviewPackage);
        bridge.enqueue_file_dialog(DesignerFileDialogRequest::ReplaceBackup);
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(bridge.take_file_dialog().is_some());
        bridge.finish_file_dialog();
        assert_eq!(viewport_wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            root_wakes.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "a queued dialog must schedule another root-owned drain"
        );
        assert!(bridge.take_file_dialog().is_some());
        bridge.finish_file_dialog();
        assert_eq!(viewport_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 3);
        bridge.clear();
        assert!(!bridge.has_pending_file_dialog());
    }

    #[test]
    fn debounced_preferences_wake_root_and_close_flush_survives_disposal() {
        let bridge = std::sync::Arc::new(DesignerIntentBridge::default());
        let root_wakes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let root_count = std::sync::Arc::clone(&root_wakes);
        let root_bridge = std::sync::Arc::clone(&bridge);
        bridge.set_wake(Some(std::sync::Arc::new(move || {
            assert!(root_bridge.has_pending_preferences());
            root_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));

        let mut editor = RadialEditorState::default();
        editor.intent_bridge = std::sync::Arc::clone(&bridge);
        editor.preferences_dirty = true;
        let now = Instant::now();
        editor
            .preference_debounce
            .mark_changed(now - Duration::from_millis(300));
        editor.enqueue_preferences_ready_if_due();
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(editor.take_preferences_for_persist().is_some());
        assert!(!bridge.has_pending_preferences());

        // The close path queues the final flush before the editor disposes
        // its callbacks.  ROOT can still consume the snapshot afterward.
        editor.open_test_snapshot();
        editor.preferences_dirty = true;
        editor
            .preference_debounce
            .mark_changed(now - Duration::from_millis(300));
        editor.request_close();
        assert_eq!(root_wakes.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(editor.session.is_none());
        assert!(bridge.has_pending_preferences());
        assert!(editor.take_preferences_for_persist().is_some());
        assert!(!bridge.has_pending_preferences());
    }

    #[test]
    fn properties_draft_is_stable_across_frames_and_rejects_stale_apply() {
        let document = RadialDocument::starter();
        let cell = &document.menus[0].rings[0].cells[0];
        let original_label = cell.label.clone();
        let target = StableSelection::Cell {
            menu_id: document.menus[0].id.clone(),
            ring_id: document.menus[0].rings[0].id.clone(),
            cell_id: cell.id.clone(),
        };
        let generation = crate::radial::authoring::DraftGeneration(7);
        let mut draft = PropertiesDraft::from_cell(target.clone(), generation, cell);
        draft.label = "edited over several frames".into();
        assert!(draft.is_current(&target, generation));
        assert!(!draft.is_current(&target, crate::radial::authoring::DraftGeneration(8)));
        assert_eq!(draft.label, "edited over several frames");
        // Cancel is represented by dropping the draft; no document is ever
        // touched by local popup edits.
        drop(draft);
        assert_eq!(
            PropertiesDraft::from_cell(target, generation, cell).label,
            original_label
        );
    }

    #[test]
    fn diagnostic_display_model_bounds_actionables_and_discloses_overflow() {
        let actionables = (0..=crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS).map(|index| {
            crate::radial::diagnostics::RadialDiagnostic::new(
                crate::radial::diagnostics::RadialDiagnosticSeverity::Error,
                crate::radial::diagnostics::RadialDiagnosticKind::AssetUnavailable(
                    crate::radial::assets::AssetDiagnostic::NotFound,
                ),
                crate::radial::diagnostics::RadialDiagnosticSource::Asset {
                    menu_id: crate::radial::model::MenuId::new("menu"),
                    identity: format!("missing-{index}"),
                },
                index,
                "asset missing",
            )
        });
        let expected = crate::radial::diagnostics::RadialDiagnostic::from_font(
            &crate::radial::model::MenuId::new("menu"),
            &crate::radial::model::CellId::new("cell"),
            "long label",
            12_000_u32,
            &crate::radial::font_cache::FontDiagnostic::LabelTruncated,
        );
        let model = radial_diagnostic_display_model(actionables.chain([expected]));
        assert_eq!(
            model.actionable.len(),
            crate::radial::diagnostics::MAX_RADIAL_DIAGNOSTICS - 1
        );
        assert!(model.expected_layout.is_empty());
        assert_eq!(
            model.omitted,
            Some(crate::radial::diagnostics::RadialDiagnosticOmission {
                total: 3,
                actionable: 2,
                expected_layout: 1,
            })
        );
    }

    #[test]
    fn post_render_move_uses_stable_ids_and_selection_survives_reorder() {
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].content = CellContent::Spacer;
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.open = true;
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        let session = editor.session.as_mut().unwrap();
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let cell_id = session.draft.menus[0].rings[0].cells[1].id.clone();
        session.select(Some(StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: ring_id.clone(),
            cell_id: cell_id.clone(),
        }));
        let expected_generation = session.generation;
        editor.post_render.push(EditorCommand::MoveCell {
            source_menu: menu_id.clone(),
            source_ring: ring_id.clone(),
            cell: cell_id.clone(),
            destination_menu: menu_id.clone(),
            destination_ring: ring_id.clone(),
            index: 0,
            expected_generation,
        });
        editor.apply_post_render();
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].rings[0].cells[0].id,
            cell_id
        );
        assert!(
            matches!(editor.session.as_ref().unwrap().selection.as_ref(), Some(StableSelection::Cell { cell_id: selected, .. }) if selected == &cell_id)
        );
        assert!(
            matches!(editor.focus_restore.as_ref(), Some(StableSelection::Cell { cell_id: focused, .. }) if focused == &cell_id)
        );
    }

    #[test]
    fn preview_zoom_and_presets_never_dirty_the_authoring_session() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        let before = editor.session.as_ref().unwrap().draft.clone();
        editor.preview_zoom = 1.75;
        editor.preview_preset = PreviewPreset::HighDpi;
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
        assert!(!editor.session.as_ref().unwrap().is_dirty());
    }

    #[test]
    fn widget_dispatcher_coalesces_one_drag_without_erasing_older_history() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        let menu_id = session.draft.menus[0].id.clone();
        menu::rename_menu(
            &mut session,
            menu_id.clone(),
            "Older edit".into(),
            EditPhase::Atomic,
        )
        .unwrap();
        for (radius, signals) in [
            (
                100.0,
                WidgetEditSignals {
                    drag_started: true,
                    ended: false,
                },
            ),
            (120.0, WidgetEditSignals::default()),
            (
                140.0,
                WidgetEditSignals {
                    drag_started: false,
                    ended: true,
                },
            ),
        ] {
            let mut edited = (*session.draft).clone();
            edited.menus[0].rings[0].radius = radius;
            dispatch_widget_document_edit(
                &mut session,
                edited,
                "ring:starter/ring-0".into(),
                "radius",
                signals,
            )
            .unwrap();
        }
        assert_eq!(session.draft.menus[0].rings[0].radius, 140.0);
        assert!(session.undo());
        assert_ne!(session.draft.menus[0].rings[0].radius, 140.0);
        assert_eq!(session.draft.menus[0].name, "Older edit");
        assert!(session.undo());
        assert_ne!(session.draft.menus[0].name, "Older edit");
    }

    #[test]
    fn continuous_editor_categories_use_stable_field_dispatch_and_staged_resize() {
        let source = include_str!("mod.rs");
        let production = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        for field in [
            "hover_dwell_ms",
            "center_radius",
            "dynamic.max_items",
            "dynamic.query",
            "tooltip",
            "icon.resource_index",
            "priority",
            "window_title_contains",
            "chord",
            "image_directories[",
            "sound_directories[",
            "resource.index",
        ] {
            assert!(
                production.contains(field),
                "missing continuous field {field}"
            );
        }
        assert!(production.contains("continuous_widget_edit"));
        assert!(production.contains("dispatch_widget_document_edit"));
        assert!(production.contains("ring_resize_drafts"));
        assert!(production.contains("resize_ended"));
    }

    #[test]
    fn gui_uses_authoring_requests_without_store_ownership_or_local_packaging() {
        let source = include_str!("mod.rs");
        let production = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        assert!(!production.contains("RadialStore"));
        assert!(!production.contains("NativeHost"));
        assert!(!production.contains("RADIAL_ASSETS_DIRECTORY"));
        assert!(!production.contains("import_export::export_bytes"));
        assert!(production.contains("request_export_package"));
        assert!(production.contains("request_replace_package"));
        assert!(production.contains("request_start_native_preview"));
        assert!(production.contains("request_update_native_preview"));
        assert!(production.contains("request_stop_native_preview"));
    }

    #[test]
    fn populated_ring_delete_relocates_cells_atomically_with_stable_ids() {
        let mut document = RadialDocument::starter();
        let menu_id = document.menus[0].id.clone();
        let source = document.menus[0].rings[0].id.clone();
        let moved: Vec<_> = document.menus[0].rings[0]
            .cells
            .iter()
            .map(|cell| cell.id.clone())
            .collect();
        let mut second = document.menus[0].rings[0].clone();
        second.id = RingId::new("destination");
        second.cells.clear();
        document.menus[0].rings.push(second);
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        relocate_and_delete_ring(&mut session, &menu_id, &source, &RingId::new("destination"))
            .unwrap();
        assert_eq!(session.draft.menus[0].rings.len(), 1);
        assert_eq!(
            session.draft.menus[0].rings[0]
                .cells
                .iter()
                .map(|cell| cell.id.clone())
                .collect::<Vec<_>>(),
            moved
        );
    }

    #[test]
    fn assignment_helpers_cover_cell_secondary_and_all_menu_pointer_slots() {
        let document = RadialDocument::starter();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut session = RadialAuthoringSession::new(snapshot);
        let menu_id = session.draft.menus[0].id.clone();
        let ring_id = session.draft.menus[0].rings[0].id.clone();
        let cell_id = session.draft.menus[0].rings[0].cells[0].id.clone();
        let binding = crate::radial::model::ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: None,
                action_id: crate::universal_actions::ActionId::new("test"),
            },
        };
        assign_alternate_click(
            &mut session,
            &menu_id,
            &ring_id,
            &cell_id,
            crate::radial::model::ClickGesture::Secondary,
            binding.clone(),
        );
        for slot in [
            MenuActionSlot::CenterPrimary,
            MenuActionSlot::CenterSecondary,
            MenuActionSlot::BackgroundPrimary,
            MenuActionSlot::BackgroundSecondary,
        ] {
            assign_menu_action(&mut session, &menu_id, slot, binding.clone());
        }
        let menu = &session.draft.menus[0];
        assert!(menu.center_action.is_some() && menu.center_secondary_action.is_some());
        assert!(menu.background_action.is_some() && menu.background_secondary_action.is_some());
        assert_eq!(menu.rings[0].cells[0].alternate_clicks.len(), 1);
    }

    #[test]
    fn accessibility_contract_names_blank_controls_and_restores_stable_focus() {
        let source = include_str!("mod.rs");
        let production = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        assert!(production.contains("WidgetInfo::selected(egui::WidgetType::Checkbox"));
        assert!(production.contains("egui::WidgetType::ColorButton"));
        assert!(production.contains("egui::WidgetType::ComboBox"));
        assert!(production.contains("info.label = Some(accessible_name.into())"));
        assert!(production.contains("focus_restore.as_ref() == Some(&ring_selection)"));
        assert!(production.contains("focus_restore.as_ref() == Some(&selection)"));
        assert!(production.contains("response.request_focus()"));
        assert!(production.contains("self.focus_restore = session.selection.clone()"));
        assert!(production.contains("ui.visuals().error_fg_color"));
        assert!(production.contains("ui.visuals().warn_fg_color"));
        assert!(!production.contains("Color32::RED"));
        assert!(!production.contains("Color32::YELLOW"));

        // These visible words are intentional redundant state channels: unavailable,
        // error, selected, and destructive operations are not communicated by color
        // or icon alone.
        for semantic_text in [
            "Availability:",
            "Delete blocked:",
            "Loading the authoritative radial configuration",
            "Remove alternate action",
            "Move inward",
            "Move outward",
        ] {
            assert!(
                production.contains(semantic_text),
                "missing {semantic_text}"
            );
        }
    }

    #[test]
    fn headless_accesskit_tree_exposes_style_control_roles_names_and_state() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut scratch = String::new();
                let inherited_color = skin_editor::with_payload(serde_json::json!({
                    "red": 12,
                    "green": 34,
                    "blue": 56,
                    "alpha": 78
                }));
                let inherited_toggle = skin_editor::with_payload(serde_json::json!(false));
                let _ = style_value_widget(
                    ui,
                    skin_editor::StyleControlKind::Color,
                    &serde_json::Value::Null,
                    &inherited_color,
                    &mut scratch,
                    &[],
                    "text.color",
                );
                let _ = style_value_widget(
                    ui,
                    skin_editor::StyleControlKind::Toggle,
                    &serde_json::Value::Null,
                    &inherited_toggle,
                    &mut scratch,
                    &[],
                    "text.visible",
                );
            });
        });
        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit tree update");
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::ColorWell
                && node
                    .name()
                    .is_some_and(|name| name.contains("text.color: rgba(12, 34, 56, 78)"))
        }));
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::CheckBox
                && node.name() == Some("text.visible")
                && node.checked() == Some(egui::accesskit::Checked::False)
        }));
    }

    #[test]
    fn headless_accesskit_tree_names_a_completely_blank_cell_by_stable_id() {
        let mut document = RadialDocument::starter();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.label.clear();
        cell.tooltip = crate::radial::model::Override::Inherit;
        cell.icon = crate::radial::model::Override::Inherit;
        let expected = format!("Cell 1 ({})", cell.id);
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        )));
        let menu_id = editor.session.as_ref().unwrap().draft.menus[0].id.clone();
        editor
            .preferences
            .expanded_sections
            .insert(format!("menu:{menu_id}"), true);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                editor.tree(ui, &crate::radial::model::RadialFeatureSettings::default());
            });
        });
        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit tree update");
        assert!(update.nodes.iter().any(|(_, node)| {
            node.role() == egui::accesskit::Role::ToggleButton
                && node.name() == Some(expected.as_str())
        }));
    }

    #[test]
    fn resource_notice_severity_keeps_success_and_portability_non_error_in_accesskit() {
        let notices = [
            ResourceNotice::info("Exported package"),
            ResourceNotice::info("Managed and portable"),
            ResourceNotice::warning("External path; excluded from portable packages"),
            ResourceNotice::error("Decode failed"),
        ];
        assert_eq!(notices[0].severity, ResourceNoticeSeverity::Info);
        assert_eq!(notices[1].severity, ResourceNoticeSeverity::Info);
        assert_eq!(notices[2].severity, ResourceNoticeSeverity::Warning);
        assert_eq!(notices[3].severity, ResourceNoticeSeverity::Error);

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.begin_frame(egui::RawInput::default());
        egui::CentralPanel::default().show(&ctx, |ui| {
            for notice in &notices {
                show_resource_notice(ui, notice);
            }
        });
        let output = ctx.end_frame();
        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit tree update");
        let names = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.name())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| *name == "Info: Exported package"));
        assert!(
            names
                .iter()
                .any(|name| *name == "Info: Managed and portable")
        );
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("Warning: External path"))
        );
        assert!(names.iter().any(|name| *name == "Error: Decode failed"));
    }

    #[test]
    fn keyboard_only_undo_redo_routes_unconsumed_editor_shortcuts() {
        let document = RadialDocument::starter();
        let original = document.menus[0].name.clone();
        let menu_id = document.menus[0].id.clone();
        let snapshot = AuthoringSnapshot::new(std::sync::Arc::new(document), "test");
        let mut editor = RadialEditorState::default();
        editor.session = Some(RadialAuthoringSession::new(snapshot));
        menu::rename_menu(
            editor.session.as_mut().unwrap(),
            menu_id,
            "Keyboard edit".into(),
            EditPhase::Atomic,
        )
        .unwrap();

        let key = |modifiers| egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events: vec![key(egui::Modifiers::CTRL)],
            ..Default::default()
        });
        editor.keyboard_shortcuts(&ctx);
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].name,
            original
        );
        assert!(!ctx.input(|input| input.key_pressed(egui::Key::Z)));
        let _ = ctx.end_frame();

        ctx.begin_frame(egui::RawInput {
            events: vec![key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT)],
            ..Default::default()
        });
        editor.keyboard_shortcuts(&ctx);
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].name,
            "Keyboard edit"
        );
        let _ = ctx.end_frame();
    }
}
