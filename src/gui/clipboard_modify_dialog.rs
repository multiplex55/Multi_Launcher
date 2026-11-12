use crate::clipboard_modify::catalog::ArgumentRequirements;
use crate::clipboard_modify::clipboard::{
    ClipboardBackend, ClipboardError, ClipboardService, ClipboardSummary,
};
use crate::clipboard_modify::coordinator::{PreviewCoordinator, PreviewState};
use crate::clipboard_modify::model::{
    ClipboardModifierCatalog, ClipboardTemplate, OperationId, SavedPipeline, StageArguments,
    StageSpec, TemplateProcessor,
};
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use crate::clipboard_modify::pipeline::find_template;
use crate::clipboard_modify::store::ClipboardModifierStore;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use eframe::egui;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const LARGE_SOURCE_CONFIRM_BYTES: usize = 5 * 1024 * 1024;
pub const CLIPBOARD_MODIFY_DEFAULT_WIDTH: f32 = 900.0;
pub const CLIPBOARD_MODIFY_DEFAULT_HEIGHT: f32 = 640.0;
pub const CLIPBOARD_MODIFY_MIN_WIDTH: f32 = 520.0;
pub const CLIPBOARD_MODIFY_MIN_HEIGHT: f32 = 360.0;
pub const CLIPBOARD_MODIFY_VIEWPORT_MARGIN: f32 = 16.0;
pub const CLIPBOARD_MODIFY_MULTILINE_ROWS: usize = 10;
const CLIPBOARD_MODIFY_WINDOW_ID: &str = "clipboard_modify_dialog_window";
const CLIPBOARD_MODIFY_CONSTRAINED_CONTENT_SCROLL_ID: &str =
    "clipboard_modify_constrained_content_scroll";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardModifyDialogSection {
    #[default]
    Modify,
    Templates,
    SavedPipelines,
    ManageTemplates,
    ManagePipelines,
    Help,
}

const CLIPBOARD_MODIFY_TAB_LABELS: [(ClipboardModifyDialogSection, &str); 6] = [
    (ClipboardModifyDialogSection::Modify, "Modify"),
    (ClipboardModifyDialogSection::Templates, "Templates"),
    (
        ClipboardModifyDialogSection::SavedPipelines,
        "Saved Pipelines",
    ),
    (
        ClipboardModifyDialogSection::ManageTemplates,
        "Manage Templates",
    ),
    (
        ClipboardModifyDialogSection::ManagePipelines,
        "Manage Pipelines",
    ),
    (ClipboardModifyDialogSection::Help, "Help"),
];

#[derive(Debug, Clone)]
pub struct DialogStage {
    pub id: u64,
    pub spec: StageSpec,
    pub error: Option<String>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct NativeScrollObservation {
    id: egui::Id,
    inner_rect: egui::Rect,
    clip_rect: egui::Rect,
    content_size: egui::Vec2,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct SectionLayoutObservation {
    navigation_overflows: bool,
    body_rect: egui::Rect,
    body_max_height: f32,
    constrained_scroll: Option<NativeScrollObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingDialogAction {
    Apply,
    ApplyAndClose,
    CopyResult,
}

#[derive(Clone)]
struct ClipboardModifyPopupAnchor {
    response: egui::Id,
    popup: egui::Id,
    layer: egui::LayerId,
}
#[derive(Clone, Default)]
struct ClipboardModifyPopupAnchors {
    session_epoch: u64,
    anchors: Vec<ClipboardModifyPopupAnchor>,
}
fn clipboard_modify_popup_key(ctx: &egui::Context) -> egui::Id {
    egui::Id::new(("clipboard-modify-popup-anchors", ctx.viewport_id()))
}
fn record_clipboard_modify_combo(ui: &egui::Ui, response: &egui::Response) {
    if !response.enabled() || !response.rect.intersect(ui.clip_rect()).is_positive() {
        return;
    }
    let key = clipboard_modify_popup_key(ui.ctx());
    ui.ctx().data_mut(|data| {
        if let Some(mut current) = data.get_temp::<ClipboardModifyPopupAnchors>(key) {
            current.anchors.push(ClipboardModifyPopupAnchor {
                response: response.id,
                popup: response.id.with("popup"),
                layer: response.layer_id,
            });
            data.insert_temp(key, current);
        }
    });
}

#[derive(Clone)]
enum ClipboardModifyHostedEffect {
    PreviewDirty,
    RequestPreview(ClipboardModifyIntent),
    CancelPreview,
    Reload,
    SaveTemplates,
    SavePipelines,
    Commit {
        id: crate::clipboard_modify::coordinator::OperationId,
        source: String,
        baseline: String,
        close: bool,
        mode: ClipboardModifyCommitMode,
    },
    Close,
    CopyText(String),
}
#[derive(Default)]
pub(super) struct ClipboardModifyHostActions {
    effects: Vec<ClipboardModifyHostedEffect>,
    pub(super) host_request: Option<super::utility_host::UtilityPresentation>,
    pub(super) close: bool,
    session_epoch: u64,
    pinned: bool,
    #[cfg(test)]
    pub(super) host_button: Option<(egui::Rect, egui::Rect, bool)>,
}

#[derive(Clone)]
enum ClipboardModifyCommitMode {
    Apply,
    CopyResult,
    ReviewedOverwrite(ClipboardSummary),
}

impl ClipboardModifyHostActions {
    pub(super) fn needs_completion(&self) -> bool {
        self.close || !self.effects.is_empty()
    }
}

pub struct ClipboardModifyDialogState {
    pub open: bool,
    host_session_epoch: u64,
    hosted_effects: Option<Vec<ClipboardModifyHostedEffect>>,
    pub section: ClipboardModifyDialogSection,
    pub source: String,
    pub baseline: String,
    pub reset_source: String,
    pub source_fingerprint: u64,
    pub source_error: Option<String>,
    pub stages: Vec<DialogStage>,
    next_stage_id: u64,
    selected_stage_operation: OperationId,
    pub preview: PreviewCoordinator,
    pub acknowledged_large_sources: HashSet<(u64, usize)>,
    pub large_confirmation_open: bool,
    pub external_change: Option<ClipboardSummary>,
    pub pending_action: Option<PendingDialogAction>,
    overwrite_preview: Option<crate::clipboard_modify::coordinator::OperationId>,
    pub unsaved_template_draft: bool,
    pub unsaved_pipeline_draft: bool,
    pub wrap_preview: bool,
    pub last_action: Option<String>,
    pub template_filter: String,
    pub help_filter: String,
    pub selected_template: Option<String>,
    pub template_draft: Vec<ClipboardTemplate>,
    pub template_editor_error: Option<String>,
    pub template_delete_confirmation: Option<String>,
    pub selected_pipeline: Option<String>,
    pub pipeline_draft: Vec<SavedPipeline>,
    pub pipeline_editor_error: Option<String>,
    pub pipeline_delete_confirmation: Option<String>,
    pub duplicate_save_confirmation: Option<String>,
    #[cfg(test)]
    test_host_add_stage: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_host_apply: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    section_layout_observation: Option<SectionLayoutObservation>,
    #[cfg(test)]
    apply_footer_clip_rect: Option<egui::Rect>,
}

/// Rollback for editable state; the preview worker remains in its live owner.
#[derive(Clone)]
pub(crate) struct ClipboardModifyInteractionCheckpoint {
    session_epoch: u64,
    section: ClipboardModifyDialogSection,
    source: String,
    source_error: Option<String>,
    stages: Vec<DialogStage>,
    next_stage_id: u64,
    selected_stage_operation: OperationId,
    acknowledged_large_sources: HashSet<(u64, usize)>,
    large_confirmation_open: bool,
    external_change: Option<ClipboardSummary>,
    pending_action: Option<PendingDialogAction>,
    unsaved_template_draft: bool,
    unsaved_pipeline_draft: bool,
    wrap_preview: bool,
    last_action: Option<String>,
    template_filter: String,
    help_filter: String,
    selected_template: Option<String>,
    template_draft: Vec<ClipboardTemplate>,
    template_editor_error: Option<String>,
    template_delete_confirmation: Option<String>,
    selected_pipeline: Option<String>,
    pipeline_draft: Vec<SavedPipeline>,
    pipeline_editor_error: Option<String>,
    pipeline_delete_confirmation: Option<String>,
    duplicate_save_confirmation: Option<String>,
}
impl ClipboardModifyInteractionCheckpoint {
    pub(crate) fn capture(state: &ClipboardModifyDialogState) -> Self {
        Self {
            session_epoch: state.host_session_epoch,
            section: state.section,
            source: state.source.clone(),
            source_error: state.source_error.clone(),
            stages: state.stages.clone(),
            next_stage_id: state.next_stage_id,
            selected_stage_operation: state.selected_stage_operation,
            acknowledged_large_sources: state.acknowledged_large_sources.clone(),
            large_confirmation_open: state.large_confirmation_open,
            external_change: state.external_change.clone(),
            pending_action: state.pending_action,
            unsaved_template_draft: state.unsaved_template_draft,
            unsaved_pipeline_draft: state.unsaved_pipeline_draft,
            wrap_preview: state.wrap_preview,
            last_action: state.last_action.clone(),
            template_filter: state.template_filter.clone(),
            help_filter: state.help_filter.clone(),
            selected_template: state.selected_template.clone(),
            template_draft: state.template_draft.clone(),
            template_editor_error: state.template_editor_error.clone(),
            template_delete_confirmation: state.template_delete_confirmation.clone(),
            selected_pipeline: state.selected_pipeline.clone(),
            pipeline_draft: state.pipeline_draft.clone(),
            pipeline_editor_error: state.pipeline_editor_error.clone(),
            pipeline_delete_confirmation: state.pipeline_delete_confirmation.clone(),
            duplicate_save_confirmation: state.duplicate_save_confirmation.clone(),
        }
    }
    pub(crate) fn restore(self, state: &mut ClipboardModifyDialogState) {
        if state.host_session_epoch != self.session_epoch {
            return;
        }
        state.section = self.section;
        state.source = self.source;
        state.source_error = self.source_error;
        state.stages = self.stages;
        state.next_stage_id = self.next_stage_id;
        state.selected_stage_operation = self.selected_stage_operation;
        state.acknowledged_large_sources = self.acknowledged_large_sources;
        state.large_confirmation_open = self.large_confirmation_open;
        state.external_change = self.external_change;
        state.pending_action = self.pending_action;
        state.unsaved_template_draft = self.unsaved_template_draft;
        state.unsaved_pipeline_draft = self.unsaved_pipeline_draft;
        state.wrap_preview = self.wrap_preview;
        state.last_action = self.last_action;
        state.template_filter = self.template_filter;
        state.help_filter = self.help_filter;
        state.selected_template = self.selected_template;
        state.template_draft = self.template_draft;
        state.template_editor_error = self.template_editor_error;
        state.template_delete_confirmation = self.template_delete_confirmation;
        state.selected_pipeline = self.selected_pipeline;
        state.pipeline_draft = self.pipeline_draft;
        state.pipeline_editor_error = self.pipeline_editor_error;
        state.pipeline_delete_confirmation = self.pipeline_delete_confirmation;
        state.duplicate_save_confirmation = self.duplicate_save_confirmation;
    }
}

impl Default for ClipboardModifyDialogState {
    fn default() -> Self {
        Self {
            open: false,
            host_session_epoch: 0,
            hosted_effects: None,
            section: Default::default(),
            source: String::new(),
            baseline: String::new(),
            reset_source: String::new(),
            source_fingerprint: 0,
            source_error: None,
            stages: Vec::new(),
            next_stage_id: 1,
            selected_stage_operation: OperationId::Uppercase,
            preview: PreviewCoordinator::default(),
            acknowledged_large_sources: HashSet::new(),
            large_confirmation_open: false,
            external_change: None,
            pending_action: None,
            overwrite_preview: None,
            unsaved_template_draft: false,
            unsaved_pipeline_draft: false,
            wrap_preview: true,
            last_action: None,
            template_filter: String::new(),
            help_filter: String::new(),
            selected_template: None,
            template_draft: Vec::new(),
            template_editor_error: None,
            template_delete_confirmation: None,
            selected_pipeline: None,
            pipeline_draft: Vec::new(),
            pipeline_editor_error: None,
            pipeline_delete_confirmation: None,
            duplicate_save_confirmation: None,
            #[cfg(test)]
            test_host_add_stage: None,
            #[cfg(test)]
            test_host_apply: None,
            #[cfg(test)]
            section_layout_observation: None,
            #[cfg(test)]
            apply_footer_clip_rect: None,
        }
    }
}

impl ClipboardModifyDialogState {
    #[cfg(test)]
    pub(super) fn test_host_buttons(
        &self,
    ) -> (
        Option<(egui::Rect, egui::Rect, bool)>,
        Option<(egui::Rect, egui::Rect, bool)>,
    ) {
        (self.test_host_add_stage, self.test_host_apply)
    }

    pub fn open_section<B: ClipboardBackend>(
        &mut self,
        section: ClipboardModifyDialogSection,
        service: &Arc<ClipboardService<B>>,
    ) {
        let was_open = self.open;
        if !was_open {
            self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
        self.section = section;
        if !was_open {
            self.load_clipboard(service);
        }
    }
    pub fn open_section_without_loading_for_test(&mut self, section: ClipboardModifyDialogSection) {
        if !self.open {
            self.host_session_epoch = self.host_session_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
        self.section = section;
    }
    pub fn load_clipboard<B: ClipboardBackend>(&mut self, service: &Arc<ClipboardService<B>>) {
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::Reload);
            return;
        }
        match service.read_text_for_modify() {
            Ok(text) => self.replace_loaded_source(text),
            Err(ClipboardError::NonText) => {
                self.source_error = Some("Clipboard does not contain text".into());
                self.replace_loaded_source(String::new());
            }
            Err(e) => self.source_error = Some(e.to_string()),
        }
    }
    pub fn replace_loaded_source(&mut self, text: String) {
        self.clear_overwrite_review();
        self.baseline = text.clone();
        self.reset_source = text.clone();
        self.source = text;
        self.source_fingerprint = fingerprint(&self.source);
        self.source_error = None;
        self.preview.cancel_and_invalidate();
        self.large_confirmation_open = self.requires_large_ack();
    }
    pub fn cleanup_after_close(&mut self) {
        if let Some(effects) = &mut self.hosted_effects {
            self.open = true;
            effects.push(ClipboardModifyHostedEffect::Close);
            return;
        }
        let wrap = self.wrap_preview;
        self.preview.cancel_and_invalidate();
        self.source.clear();
        self.baseline.clear();
        self.reset_source.clear();
        self.source_fingerprint = 0;
        self.source_error = None;
        self.stages.clear();
        self.preview = PreviewCoordinator::default();
        self.acknowledged_large_sources.clear();
        self.clear_overwrite_review();
        self.unsaved_template_draft = false;
        self.unsaved_pipeline_draft = false;
        self.large_confirmation_open = false;
        self.wrap_preview = wrap;
    }
    pub fn can_apply(&self) -> bool {
        matches!(self.preview.state(), PreviewState::Completed { .. })
            && self.preview.apply_text().is_some()
    }
    pub fn mark_dirty(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::PreviewDirty);
            return;
        }
        self.clear_overwrite_review();
        self.source_fingerprint = fingerprint(&self.source);
        if self.requires_large_ack() {
            self.preview.cancel_and_invalidate();
            self.large_confirmation_open = true;
            return;
        }
        let stages = self.stages.iter().map(|s| s.spec.clone()).collect();
        self.preview.request(
            self.source.clone(),
            ClipboardModifyIntent::Stages(stages),
            catalog,
        );
    }
    pub fn requires_large_ack(&self) -> bool {
        self.source.len() > LARGE_SOURCE_CONFIRM_BYTES
            && !self
                .acknowledged_large_sources
                .contains(&(fingerprint(&self.source), self.source.len()))
    }
    pub fn acknowledge_large_source(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        self.acknowledged_large_sources
            .insert((fingerprint(&self.source), self.source.len()));
        self.large_confirmation_open = false;
        self.mark_dirty(catalog);
    }
    pub fn reset(
        &mut self,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        self.source = self.reset_source.clone();
        self.stages.clear();
        self.mark_dirty(catalog);
    }
    pub fn add_stage(
        &mut self,
        op: OperationId,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        let id = self.next_stage_id;
        self.next_stage_id += 1;
        self.stages.push(DialogStage {
            id,
            spec: StageSpec {
                operation: op,
                arguments: StageArguments::default(),
            },
            error: None,
        });
        self.mark_dirty(catalog);
    }
    pub fn ui<B: ClipboardBackend>(
        &mut self,
        ctx: &egui::Context,
        legacy_window_size: egui::Vec2,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) {
        if !self.open {
            return;
        }
        let viewport_max_size = clipboard_modify_usable_viewport_size(ctx.content_rect().size());
        let min_size = effective_clipboard_modify_min_size(viewport_max_size);
        let mut open = self.open;
        let key = crate::settings::UtilityToolKey::parse("clipboard-modify")
            .expect("clipboard-modify is a stable utility size key");
        let access = crate::gui::utility_dialog_size::UtilityDialogSizeManager::for_context(ctx);
        let default_size =
            clipboard_modify_default_window_size(legacy_window_size, access.preference(key));
        let window = egui::Window::new("Clipboard Modify")
            .id(egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID))
            .resizable(true)
            .min_size(min_size)
            .max_size(viewport_max_size);
        let _response = access.show_window(
            ctx,
            key,
            egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID),
            &mut open,
            default_size,
            min_size,
            window,
            |ui| {
                self.shortcuts(ui, service, catalog.clone());
                let _footer_response =
                    self.render_section_contents(ui, service, catalog.clone(), store);
            },
        );
        if self.open && !open {
            self.open = false;
            self.cleanup_after_close();
        }
    }
    pub(crate) fn host_session_epoch(&self) -> u64 {
        self.host_session_epoch
    }

    pub(super) fn render_host<B: ClipboardBackend>(
        &mut self,
        ctx: &egui::Context,
        legacy_window_size: egui::Vec2,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
        owner_current: bool,
        first: bool,
        pinned: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: super::utility_host::UtilityPresentation,
    ) -> ClipboardModifyHostActions {
        let mut actions = ClipboardModifyHostActions {
            session_epoch: self.host_session_epoch,
            pinned,
            ..Default::default()
        };
        if !self.open {
            return actions;
        }
        self.hosted_effects = Some(Vec::new());
        #[cfg(test)]
        {
            self.test_host_add_stage = None;
            self.test_host_apply = None;
        }
        if owner_current && first {
            let popup_key = clipboard_modify_popup_key(ctx);
            ctx.data_mut(|data| {
                data.insert_temp(
                    popup_key,
                    ClipboardModifyPopupAnchors {
                        session_epoch: self.host_session_epoch,
                        anchors: Vec::new(),
                    },
                )
            });
        }
        let mut open = self.open;
        let mut render_authorized = false;
        let output_start = ctx.output(|output| output.commands.len());
        let mut draw_body = |ui: &mut egui::Ui| {
            let allowed = owner_current && first && ui.is_visible() && !ui.is_sizing_pass();
            render_authorized = allowed;
            ui.add_enabled_ui(allowed, |ui| {
                let (response, requested) =
                    super::utility_host::presentation_button(ui, presentation, allowed);
                #[cfg(test)]
                {
                    actions.host_button = Some((response.rect, ui.clip_rect(), response.enabled()));
                }
                if response.clicked() && allowed && !ctx.will_discard() {
                    actions.host_request = Some(requested);
                }
                if allowed {
                    self.shortcuts(ui, service, catalog.clone());
                }
                self.render_section_contents(ui, service, catalog.clone(), store);
            });
        };
        if let Some(ui) = child_ui {
            draw_body(ui);
        } else {
            let viewport_max_size =
                clipboard_modify_usable_viewport_size(ctx.content_rect().size());
            let min_size = effective_clipboard_modify_min_size(viewport_max_size);
            let key = crate::settings::UtilityToolKey::parse("clipboard-modify")
                .expect("stable utility key");
            let access =
                crate::gui::utility_dialog_size::UtilityDialogSizeManager::for_context(ctx);
            let default_size =
                clipboard_modify_default_window_size(legacy_window_size, access.preference(key));
            let window = egui::Window::new("Clipboard Modify")
                .id(egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID))
                .resizable(true)
                .min_size(min_size)
                .max_size(viewport_max_size);
            access.show_window(
                ctx,
                key,
                egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID),
                &mut open,
                default_size,
                min_size,
                window,
                draw_body,
            );
        }
        let copies = ctx.output_mut(|output| {
            let mut copies = Vec::new();
            for command in output.commands.split_off(output_start) {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
            copies
        });
        let effects = self.hosted_effects.take().unwrap_or_default();
        if render_authorized {
            actions.effects = effects;
            actions.effects.extend(
                copies
                    .into_iter()
                    .map(ClipboardModifyHostedEffect::CopyText),
            );
        }
        if owner_current && first && !open && !pinned {
            actions.effects.push(ClipboardModifyHostedEffect::Close);
        }
        actions.close = !pinned
            && actions
                .effects
                .iter()
                .any(|effect| matches!(effect, ClipboardModifyHostedEffect::Close));
        if pinned {
            actions
                .effects
                .retain(|effect| !matches!(effect, ClipboardModifyHostedEffect::Close));
        }
        actions
    }

    pub(crate) fn owned_popup_layer(
        &self,
        ctx: &egui::Context,
        parent: egui::LayerId,
    ) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        let popup_key = clipboard_modify_popup_key(ctx);
        let recorded = ctx.data(|data| data.get_temp::<ClipboardModifyPopupAnchors>(popup_key))?;
        if recorded.session_epoch != self.host_session_epoch {
            return None;
        }
        recorded.anchors.iter().find_map(|anchor| {
            if anchor.layer != parent {
                return None;
            }
            let response = ctx.read_response(anchor.response)?;
            // An input-disabled replay can publish the same response ID.
            // The recorded anchor came from an enabled owner pass.
            if response.layer_id != parent || !egui::Popup::is_id_open(ctx, anchor.popup) {
                return None;
            }
            let rect = ctx.memory(|memory| memory.area_rect(anchor.popup))?;
            let layer = egui::LayerId::new(egui::Order::Foreground, anchor.popup);
            let sizing = egui::containers::AreaState::load(ctx, anchor.popup)
                .is_some_and(|state| !state.interactable);
            (rect.is_positive() && (ctx.layer_id_at(rect.center()) == Some(layer) || sizing))
                .then_some(layer)
        })
    }

    pub(super) fn complete_host_actions<B: ClipboardBackend>(
        &mut self,
        actions: ClipboardModifyHostActions,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) -> bool {
        if !self.open || self.host_session_epoch != actions.session_epoch {
            return false;
        }
        // Shortcuts run before the body is drawn. An edit anywhere in the
        // retained frame invalidates its old preview, even if queued later.
        let invalidates_preview = actions.effects.iter().any(|effect| {
            matches!(
                effect,
                ClipboardModifyHostedEffect::PreviewDirty
                    | ClipboardModifyHostedEffect::RequestPreview(_)
                    | ClipboardModifyHostedEffect::CancelPreview
                    | ClipboardModifyHostedEffect::Reload
            )
        });
        let mut attempted_commit = false;
        for effect in actions.effects {
            match effect {
                ClipboardModifyHostedEffect::PreviewDirty => self.mark_dirty(catalog.clone()),
                ClipboardModifyHostedEffect::RequestPreview(intent) => {
                    self.request_preview(intent, catalog.clone())
                }
                ClipboardModifyHostedEffect::CancelPreview => self.cancel_pipeline_preview(),
                ClipboardModifyHostedEffect::Reload => self.load_clipboard(service),
                ClipboardModifyHostedEffect::SaveTemplates => {
                    if let Some(store) = store {
                        let _ = self.save_template_draft(&catalog, store);
                    }
                }
                ClipboardModifyHostedEffect::SavePipelines => {
                    if let Some(store) = store {
                        let _ = self.save_pipeline_draft(&catalog, store);
                    }
                }
                ClipboardModifyHostedEffect::Commit {
                    id,
                    source,
                    baseline,
                    close,
                    mode,
                } => {
                    if invalidates_preview
                        || attempted_commit
                        || !matches!(self.preview.state(), PreviewState::Completed { id: current, .. } if *current == id)
                        || self.source != source
                        || self.baseline != baseline
                    {
                        continue;
                    }
                    let Some(output) = self.preview.apply_text() else {
                        continue;
                    };
                    attempted_commit = true;
                    let label = "Clipboard Modify dialog";
                    let result = match mode {
                        ClipboardModifyCommitMode::Apply
                        | ClipboardModifyCommitMode::CopyResult => service.commit_dialog(
                            &self.baseline,
                            &self.source,
                            output,
                            matches!(mode, ClipboardModifyCommitMode::CopyResult),
                            label,
                        ),
                        ClipboardModifyCommitMode::ReviewedOverwrite(review) => {
                            if self.overwrite_preview != Some(id)
                                || self.external_change.as_ref() != Some(&review)
                                || self.pending_action != Some(Self::pending_commit(close))
                            {
                                continue;
                            }
                            service.commit_reviewed_dialog(&self.baseline, output, &review, label)
                        }
                    };
                    match result {
                        Ok(_) => {
                            self.clear_overwrite_review();
                            if close && !actions.pinned {
                                self.open = false;
                                self.cleanup_after_close();
                            }
                        }
                        Err(ClipboardError::ConfirmationRequired(conflict)) => {
                            self.external_change = Some(conflict.current);
                            self.pending_action = Some(Self::pending_commit(close));
                            self.overwrite_preview = Some(id);
                        }
                        Err(error) => self.source_error = Some(error.to_string()),
                    }
                }
                ClipboardModifyHostedEffect::Close => {
                    if !actions.pinned {
                        self.open = false;
                        self.cleanup_after_close();
                    }
                }
                ClipboardModifyHostedEffect::CopyText(text) => {
                    #[cfg(test)]
                    let result = service
                        .backend()
                        .write_text(&text)
                        .map_err(|error| error.to_string());
                    #[cfg(not(test))]
                    let result = crate::actions::clipboard::set_text(&text)
                        .map_err(|error| error.to_string());
                    if let Err(error) = result {
                        self.source_error = Some(error);
                    }
                }
            }
            if !self.open {
                break;
            }
        }
        !self.open
    }

    fn render_tab_navigation(&mut self, ui: &mut egui::Ui) -> bool {
        let tab_row_height = tab_navigation_reserved_height(ui);
        let response = ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), tab_row_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_max_height(tab_row_height);
                let scroll = egui::ScrollArea::horizontal()
                    .id_salt("clipboard_modify_tab_navigation")
                    .auto_shrink([false, true])
                    .max_height(tab_row_height)
                    .show(ui, |ui| {
                        ui.set_max_height(tab_row_height);
                        ui.horizontal(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            for (section, label) in CLIPBOARD_MODIFY_TAB_LABELS {
                                if ui
                                    .selectable_label(self.section == section, label)
                                    .clicked()
                                {
                                    self.section = section;
                                }
                            }
                        });
                    });
                scroll.content_size.x > scroll.inner_rect.width()
            },
        );
        response.inner
    }

    fn render_section_body<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        available_height: f32,
        available_width: f32,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) -> egui::Rect {
        let output = egui::ScrollArea::both()
            .id_salt(section_scroll_id(self.section))
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_width(available_width.max(0.0))
            .max_height(available_height.max(0.0))
            .show(ui, |ui| {
                ui.set_max_width(available_width.max(0.0));
                match self.section {
                    ClipboardModifyDialogSection::Modify => {
                        self.modify_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::Templates => {
                        self.templates_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::SavedPipelines => {
                        self.saved_pipelines_ui(ui, service, catalog.clone())
                    }
                    ClipboardModifyDialogSection::ManageTemplates => {
                        if let Some(store) = store {
                            self.manage_templates_ui(ui, catalog.clone(), store)
                        } else {
                            ui.label("Template management is unavailable.");
                        }
                    }
                    ClipboardModifyDialogSection::ManagePipelines => {
                        if let Some(store) = store {
                            self.manage_pipelines_ui(ui, catalog.clone(), store)
                        } else {
                            ui.label("Pipeline management is unavailable.");
                        }
                    }
                    ClipboardModifyDialogSection::Help => self.help_ui(ui, catalog.clone()),
                }
            });
        output.inner_rect
    }

    fn render_section_footer<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) -> Option<egui::Response> {
        match self.section {
            ClipboardModifyDialogSection::Modify => {
                ui.separator();
                Some(self.modify_footer_ui(ui, service))
            }
            ClipboardModifyDialogSection::Templates => {
                if let Some(id) = self
                    .selected_template
                    .clone()
                    .filter(|id| find_template(catalog.as_ref(), id).is_some())
                {
                    ui.separator();
                    let button = ui.button("Apply Template");
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                    if response.clicked() {
                        self.apply_template_through_commit_path(&id, catalog.clone());
                        self.commit(service, false, false);
                    }
                    Some(response)
                } else {
                    None
                }
            }
            ClipboardModifyDialogSection::ManageTemplates => {
                if let Some(store) = store {
                    ui.separator();
                    let button = ui.add_enabled(
                        self.can_save_template_draft(catalog.as_ref()),
                        egui::Button::new("Save"),
                    );
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                    if response.clicked() {
                        let _ = self.save_template_draft(catalog.as_ref(), store);
                    }
                    Some(response)
                } else {
                    None
                }
            }
            ClipboardModifyDialogSection::ManagePipelines => {
                if let Some(store) = store {
                    ui.separator();
                    let button = ui.button("Save");
                    let response = with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                    if response.clicked() {
                        let _ = self.save_pipeline_draft(catalog.as_ref(), store);
                        self.duplicate_save_confirmation = None;
                    }
                    Some(response)
                } else {
                    None
                }
            }
            ClipboardModifyDialogSection::SavedPipelines | ClipboardModifyDialogSection::Help => {
                None
            }
        }
    }

    fn render_section_contents<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
        store: Option<&ClipboardModifierStore>,
    ) -> Option<egui::Response> {
        let contents_id = ui.id();
        let available_height = ui.available_height();
        let available_width = ui.available_width();
        let available_rect = ui.available_rect_before_wrap();
        let navigation_height = tab_navigation_reserved_height(ui);
        // A tab click changes `self.section` before the body and footer render
        // later in this same frame. The Modify action row is the widest footer,
        // so reserving it conservatively covers every section transition.
        let footer_reserved_height =
            section_footer_reserved_height(ui, ClipboardModifyDialogSection::Modify, true);
        let spacing = ui.spacing().clone();
        let separator_height = ui
            .style()
            .separator_style(
                &egui::widget_style::Classes::default(),
                egui::widget_style::WidgetState::Noninteractive,
            )
            .spacing;
        let navigation_separator_height = spacing.item_spacing.y + separator_height;
        let constrained_overflow = available_height.is_finite()
            && navigation_height + navigation_separator_height + footer_reserved_height
                > available_height;

        // Both paths use the Window content's original identity for section
        // widgets. The constrained outer ScrollArea can therefore wrap them
        // without changing their existing scroll and editor state IDs.
        let mut render_contents = |ui: &mut egui::Ui, constrained: bool| {
            let navigation_overflows = self.render_tab_navigation(ui);
            ui.separator();
            let has_footer = match self.section {
                ClipboardModifyDialogSection::Modify | ClipboardModifyDialogSection::Templates => {
                    true
                }
                ClipboardModifyDialogSection::ManageTemplates
                | ClipboardModifyDialogSection::ManagePipelines => store.is_some(),
                ClipboardModifyDialogSection::SavedPipelines
                | ClipboardModifyDialogSection::Help => false,
            };
            let footer_reserved_height =
                section_footer_reserved_height(ui, self.section, has_footer);
            let body_max_height = if constrained {
                available_height.max(1.0)
            } else {
                clipboard_modify_body_max_height(ui.available_height(), 0.0, footer_reserved_height)
            };
            let body_rect = self.render_section_body(
                ui,
                body_max_height,
                available_width,
                service,
                catalog.clone(),
                store,
            );
            let footer_response = self.render_section_footer(ui, service, catalog.clone(), store);
            (
                navigation_overflows,
                body_rect,
                body_max_height,
                footer_response,
            )
        };

        #[cfg(test)]
        let mut constrained_scroll = None;
        let (_navigation_overflows, _body_rect, _body_max_height, footer_response) =
            if constrained_overflow {
                #[cfg(test)]
                let mut scroll_clip_rect = egui::Rect::NOTHING;
                let output = egui::ScrollArea::both()
                    .id_salt(CLIPBOARD_MODIFY_CONSTRAINED_CONTENT_SCROLL_ID)
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_width(available_width.max(0.0))
                    .max_height(available_height.max(0.0))
                    .show(ui, |ui| {
                        let constrained_contents_rect = egui::Rect::from_min_size(
                            ui.available_rect_before_wrap().min,
                            egui::vec2(available_width.max(0.0), available_height.max(0.0)),
                        );
                        let contents = ui.scope_builder(
                            egui::UiBuilder::new()
                                .id(contents_id)
                                .max_rect(constrained_contents_rect),
                            |ui| render_contents(ui, true),
                        );
                        #[cfg(test)]
                        {
                            scroll_clip_rect = ui.clip_rect();
                        }
                        contents.inner
                    });
                #[cfg(test)]
                {
                    constrained_scroll = Some(NativeScrollObservation {
                        id: output.id,
                        inner_rect: output.inner_rect,
                        clip_rect: scroll_clip_rect,
                        content_size: output.content_size,
                    });
                }
                output.inner
            } else {
                let contents = ui.scope_builder(
                    egui::UiBuilder::new()
                        .id(contents_id)
                        .max_rect(available_rect),
                    |ui| render_contents(ui, false),
                );
                contents.inner
            };

        #[cfg(test)]
        {
            self.section_layout_observation = Some(SectionLayoutObservation {
                navigation_overflows: _navigation_overflows,
                body_rect: _body_rect,
                body_max_height: _body_max_height,
                constrained_scroll,
            });
        }
        footer_response
    }

    pub fn filtered_templates<'a>(
        &self,
        catalog: &'a ClipboardModifierCatalog,
    ) -> Vec<&'a ClipboardTemplate> {
        let q = crate::clipboard_modify::catalog::normalize_name(&self.template_filter);
        catalog
            .templates
            .iter()
            .filter(|t| {
                q.is_empty()
                    || crate::clipboard_modify::catalog::normalize_name(&t.id).contains(&q)
                    || crate::clipboard_modify::catalog::normalize_name(&t.label).contains(&q)
                    || t.aliases
                        .iter()
                        .any(|a| crate::clipboard_modify::catalog::normalize_name(a).contains(&q))
            })
            .collect()
    }

    pub fn pipeline_stage_summary(p: &SavedPipeline) -> String {
        p.stages
            .iter()
            .map(|s| crate::clipboard_modify::catalog::canonical_command(s.operation))
            .collect::<Vec<_>>()
            .join(" → ")
    }
    pub fn pipeline_identity_line(p: &SavedPipeline) -> String {
        format!(
            "{} ({}) aliases: {}",
            p.label,
            p.id,
            if p.aliases.is_empty() {
                "—".into()
            } else {
                p.aliases.join(", ")
            }
        )
    }
    fn request_preview(
        &mut self,
        intent: ClipboardModifyIntent,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::RequestPreview(intent));
        } else {
            self.clear_overwrite_review();
            self.preview.request(self.source.clone(), intent, catalog);
        }
    }

    pub fn preview_pipeline(&mut self, id: String, catalog: Arc<ClipboardModifierCatalog>) {
        self.request_preview(
            ClipboardModifyIntent::ApplySavedPipeline { name: id },
            catalog,
        );
    }
    pub fn apply_pipeline_through_commit_path(
        &mut self,
        id: &str,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        self.preview_pipeline(id.to_string(), catalog);
    }
    pub fn duplicate_pipeline_id(base: &ClipboardModifierCatalog, id: &str) -> String {
        let root = format!(
            "{}-copy",
            crate::clipboard_modify::catalog::normalize_name(id)
        );
        let taken = |n: &str| {
            base.pipelines
                .iter()
                .any(|p| p.id == n || p.aliases.iter().any(|a| a == n))
                || base
                    .templates
                    .iter()
                    .any(|t| t.id == n || t.aliases.iter().any(|a| a == n))
        };
        if !taken(&root) {
            return root;
        }
        for i in 2.. {
            let c = format!("{root}-{i}");
            if !taken(&c) {
                return c;
            }
        }
        unreachable!()
    }
    pub fn begin_pipeline_edit(&mut self, catalog: &ClipboardModifierCatalog) {
        self.pipeline_draft = catalog.pipelines.clone();
        self.unsaved_pipeline_draft = false;
        self.pipeline_editor_error = None;
    }
    pub fn validate_pipeline_draft(
        &self,
        base: &ClipboardModifierCatalog,
    ) -> Result<ClipboardModifierCatalog, String> {
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: base.templates.clone(),
            pipelines: self.pipeline_draft.clone(),
        };
        crate::clipboard_modify::config::validate_model(&model).map_err(|e| e.to_string())
    }
    pub fn save_pipeline_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
    ) -> Result<ClipboardModifierCatalog, String> {
        let validated = self.validate_pipeline_draft(base)?;
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::SavePipelines);
            return Ok(validated);
        }
        let catalog = match store.save_pipelines(base, self.pipeline_draft.clone()) {
            Ok(catalog) => catalog,
            Err(e) => {
                self.pipeline_editor_error = Some(format!(
                    "Could not save clipboard pipelines. Check file permissions and retry: {e}"
                ));
                return Err(self.pipeline_editor_error.clone().unwrap());
            }
        };
        self.unsaved_pipeline_draft = false;
        self.pipeline_editor_error = None;
        Ok(catalog)
    }
    pub fn delete_pipeline_from_draft_and_save(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
        id: &str,
    ) -> Result<(), String> {
        self.pipeline_draft.retain(|p| p.id != id);
        self.unsaved_pipeline_draft = true;
        self.save_pipeline_draft(base, store).map(|_| ())
    }
    pub fn reorder_pipeline_draft_and_save(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        if from < self.pipeline_draft.len() && to < self.pipeline_draft.len() {
            self.pipeline_draft.swap(from, to);
        }
        self.unsaved_pipeline_draft = true;
        self.save_pipeline_draft(base, store).map(|_| ())
    }
    pub fn allowed_stage_operations() -> Vec<OperationId> {
        crate::clipboard_modify::catalog::operations()
            .iter()
            .filter(|o| o.pipeline_available)
            .map(|o| o.id)
            .collect()
    }
    pub fn cancel_pipeline_preview(&mut self) {
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::CancelPreview);
            return;
        }
        self.preview.cancel_active();
    }

    fn help_ui(&mut self, ui: &mut egui::Ui, catalog: Arc<ClipboardModifierCatalog>) {
        compact_section_heading(ui, "Clipboard Modify help", None);
        ui.horizontal_wrapped(|ui| {
            ui.label("Search help");
            ui.text_edit_singleline(&mut self.help_filter);
        });
        ui.separator();
        ui.set_max_width(ui.available_width());
        for entry in crate::clipboard_modify::help::build_help_entries(catalog.as_ref())
            .into_iter()
            .filter(|entry| entry.matches_filter(&self.help_filter))
        {
            ui.group(|ui| {
                ui.set_max_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(&entry.canonical_syntax);
                    ui.label(format!("— {}", entry.description))
                        .on_hover_text(&entry.description);
                });
                ui.label(format!(
                    "Category: {} | Pipeline: {}",
                    entry.category,
                    if entry.pipeline_allowed {
                        "allowed"
                    } else {
                        "not allowed"
                    }
                ));
                ui.label(format!("Arguments: {}", entry.arguments));
                if !entry.aliases.is_empty() {
                    ui.label(format!("Aliases: {}", entry.aliases.join(", ")));
                }
                if !entry.examples.is_empty() {
                    ui.label(format!("Examples: {}", entry.examples.join("; ")));
                }
            });
        }
    }
    fn templates_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        _service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        compact_section_heading(
            ui,
            "Templates",
            Some("Select a template to preview it against the current source."),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.template_filter);
            if ui.button("Manage Templates").clicked() {
                self.begin_template_edit(catalog.as_ref());
                self.section = ClipboardModifyDialogSection::ManageTemplates;
            }
        });
        let selected_template = self
            .selected_template
            .clone()
            .filter(|id| find_template(catalog.as_ref(), id).is_some());
        for t in self.filtered_templates(catalog.as_ref()) {
            let selected = self.selected_template.as_deref() == Some(&t.id);
            if ui
                .selectable_label(selected, format!("{} ({})", t.label, t.id))
                .clicked()
            {
                self.selected_template = Some(t.id.clone());
                self.preview_template(t.id.clone(), catalog.clone());
            }
        }
        if selected_template.is_some() {
            ui.separator();
            ui.label("Preview");
            self.preview_state_ui(ui, egui::Id::new("cm_template_preview"));
        }
    }
    pub fn preview_template(&mut self, id: String, catalog: Arc<ClipboardModifierCatalog>) {
        self.request_preview(ClipboardModifyIntent::ApplyTemplate { name: id }, catalog);
    }
    pub fn apply_template_through_commit_path(
        &mut self,
        id: &str,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        self.preview_template(id.to_string(), catalog);
    }
    pub fn begin_template_edit(&mut self, catalog: &ClipboardModifierCatalog) {
        self.template_draft = catalog.templates.clone();
        self.unsaved_template_draft = false;
        self.template_editor_error = None;
    }
    pub fn validate_template_draft(
        &self,
        base: &ClipboardModifierCatalog,
    ) -> Result<ClipboardModifierCatalog, String> {
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: self.template_draft.clone(),
            pipelines: base.pipelines.clone(),
        };
        crate::clipboard_modify::config::validate_model(&model).map_err(|e| e.to_string())
    }
    pub fn can_save_template_draft(&self, base: &ClipboardModifierCatalog) -> bool {
        self.validate_template_draft(base).is_ok()
    }
    pub fn save_template_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        store: &ClipboardModifierStore,
    ) -> Result<ClipboardModifierCatalog, String> {
        let validated = self.validate_template_draft(base)?;
        if let Some(effects) = &mut self.hosted_effects {
            effects.push(ClipboardModifyHostedEffect::SaveTemplates);
            return Ok(validated);
        }
        let catalog = match store.save_templates(base, self.template_draft.clone()) {
            Ok(catalog) => catalog,
            Err(e) => {
                self.template_editor_error = Some(format!(
                    "Could not save clipboard templates. Check file permissions and retry: {e}"
                ));
                return Err(self.template_editor_error.clone().unwrap());
            }
        };
        self.unsaved_template_draft = false;
        self.template_editor_error = None;
        Ok(catalog)
    }
    pub fn referencing_pipelines(
        catalog: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Vec<String> {
        catalog
            .pipelines
            .iter()
            .filter(|p| {
                p.stages.iter().any(|s| {
                    s.operation == OperationId::Template
                        && s.arguments.name.as_deref() == Some(template_id)
                })
            })
            .map(|p| format!("{} ({})", p.label, p.id))
            .collect()
    }
    pub fn delete_template_from_draft(
        &mut self,
        base: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Result<(), String> {
        let refs = Self::referencing_pipelines(base, template_id);
        if !refs.is_empty() {
            return Err(format!(
                "Cannot delete template because saved pipelines reference it: {}",
                refs.join(", ")
            ));
        }
        self.template_draft.retain(|t| t.id != template_id);
        self.unsaved_template_draft = true;
        Ok(())
    }
    pub fn template_delete_confirmation_text(
        catalog: &ClipboardModifierCatalog,
        template_id: &str,
    ) -> Option<String> {
        find_template(catalog, template_id).map(|t| {
            format!(
                "Delete template \"{}\" (ID: {})? Saved pipelines must be updated before referenced templates can be deleted.",
                t.label, t.id
            )
        })
    }
    fn manage_templates_ui(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Arc<ClipboardModifierCatalog>,
        store: &ClipboardModifierStore,
    ) {
        compact_section_heading(
            ui,
            "Manage templates",
            Some("Aliases are comma-separated. Saved changes update the template catalog."),
        );
        if self.template_draft.is_empty() && !catalog.templates.is_empty() {
            self.begin_template_edit(catalog.as_ref());
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add template").clicked() {
                self.template_draft.push(ClipboardTemplate {
                    id: "new-template".into(),
                    label: "New Template".into(),
                    aliases: vec![],
                    template: "{{clipboard}}".into(),
                    processor: Some(TemplateProcessor::Literal),
                });
                self.unsaved_template_draft = true;
            }
        });
        let mut remove = None;
        let mut moved = false;
        let mut edited = false;
        for i in 0..self.template_draft.len() {
            let record_id =
                egui::Id::new(("cm_template_record", self.template_draft[i].id.as_str()));
            ui.push_id(record_id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("ID");
                    edited |= ui
                        .push_id("template_id", |ui| {
                            ui.text_edit_singleline(&mut self.template_draft[i].id)
                        })
                        .inner
                        .changed();
                    ui.label("Label");
                    edited |= ui
                        .push_id("template_label", |ui| {
                            ui.text_edit_singleline(&mut self.template_draft[i].label)
                        })
                        .inner
                        .changed();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("↑").clicked() && i > 0 {
                            self.template_draft.swap(i, i - 1);
                            moved = true;
                        }
                        if ui.button("↓").clicked() && i + 1 < self.template_draft.len() {
                            self.template_draft.swap(i, i + 1);
                            moved = true;
                        }
                        if ui.button("Delete").clicked() {
                            self.template_delete_confirmation =
                                Some(self.template_draft[i].id.clone());
                            remove = Some(self.template_draft[i].id.clone());
                        }
                    });
                });
                let mut aliases = self.template_draft[i].aliases.join(", ");
                ui.label("Aliases");
                if ui
                    .push_id("template_aliases", |ui| {
                        ui.text_edit_singleline(&mut aliases)
                    })
                    .inner
                    .changed()
                {
                    self.template_draft[i].aliases = aliases
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                    edited = true;
                }
                ui.label("Template body");
                edited |= editable_multiline_ten_rows(
                    ui,
                    egui::Id::new(format!(
                        "cm_template_{}_template_body",
                        self.template_draft[i].id
                    )),
                    &mut self.template_draft[i].template,
                )
                .changed();
                let combo = egui::ComboBox::from_label("Processor")
                    .selected_text(format!("{:?}", self.template_draft[i].processor))
                    .show_ui(ui, |ui| {
                        edited |= ui
                            .selectable_value(
                                &mut self.template_draft[i].processor,
                                Some(TemplateProcessor::Literal),
                                "Literal",
                            )
                            .changed();
                        edited |= ui
                            .selectable_value(
                                &mut self.template_draft[i].processor,
                                Some(TemplateProcessor::RustRawString),
                                "Rust raw string",
                            )
                            .changed();
                    });
                record_clipboard_modify_combo(ui, &combo.response);
            });
        }
        if let Some(id) = remove
            && let Err(e) = self.delete_template_from_draft(catalog.as_ref(), &id)
        {
            self.template_editor_error = Some(e);
        }
        if let Some(id) = &self.template_delete_confirmation
            && let Some(message) = Self::template_delete_confirmation_text(catalog.as_ref(), id)
        {
            semantic_status(ui, StatusTone::Warning, "Template deletion", None);
            readonly_multiline_preview(
                ui,
                egui::Id::new("cm_template_delete_confirmation"),
                &message,
                true,
            );
        }
        if moved {
            let _ = self.save_template_draft(catalog.as_ref(), store);
        }
        if edited {
            self.unsaved_template_draft = true;
        }
        if let Err(e) = self.validate_template_draft(catalog.as_ref()) {
            semantic_status(ui, StatusTone::Error, "Template validation failed", None);
            readonly_multiline_preview(ui, egui::Id::new("cm_template_validation_error"), &e, true);
        }
        if let Some(e) = &self.template_editor_error {
            semantic_status(ui, StatusTone::Error, "Template save failed", None);
            readonly_multiline_preview(ui, egui::Id::new("cm_template_editor_error"), e, true);
        }
    }

    fn saved_pipelines_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<ClipboardModifierCatalog>,
    ) {
        compact_section_heading(
            ui,
            "Saved pipelines",
            Some("Preview a pipeline against the current source before running it."),
        );
        if ui.button("Manage Pipelines").clicked() {
            self.begin_pipeline_edit(catalog.as_ref());
            self.section = ClipboardModifyDialogSection::ManagePipelines;
        }
        let source_preview = capped_current_source_preview(&self.source);
        for p in &catalog.pipelines {
            ui.separator();
            let selected = self.selected_pipeline.as_deref() == Some(&p.id);
            if ui
                .selectable_label(selected, Self::pipeline_identity_line(p))
                .clicked()
            {
                self.selected_pipeline = Some(p.id.clone());
                self.preview_pipeline(p.id.clone(), catalog.clone());
            }
            ui.label(format!("Stages: {}", Self::pipeline_stage_summary(p)));
            ui.label(format!("Current-source preview: {source_preview}"));
            if selected {
                self.preview_state_ui(ui, egui::Id::new("cm_pipeline_preview"));
            }
            if ui.button(format!("Execute {}", p.label)).clicked() {
                self.apply_pipeline_through_commit_path(&p.id, catalog.clone());
                self.commit(service, false, false);
            }
        }
    }
    fn manage_pipelines_ui(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Arc<ClipboardModifierCatalog>,
        store: &ClipboardModifierStore,
    ) {
        compact_section_heading(
            ui,
            "Manage pipelines",
            Some("Aliases are comma-separated. Changes are written to the saved pipeline catalog."),
        );
        if self.pipeline_draft.is_empty() && !catalog.pipelines.is_empty() {
            self.begin_pipeline_edit(catalog.as_ref());
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add pipeline").clicked() {
                self.pipeline_draft.push(SavedPipeline {
                    id: Self::duplicate_pipeline_id(catalog.as_ref(), "new-pipeline"),
                    label: "New Pipeline".into(),
                    aliases: vec![],
                    stages: vec![],
                });
                self.unsaved_pipeline_draft = true;
            }
            if ui.button("Cancel preview").clicked() {
                self.cancel_pipeline_preview();
            }
        });
        let mut remove = None;
        let mut swap = None;
        let mut edited = false;
        for i in 0..self.pipeline_draft.len() {
            let pipeline_field_id = format!("cm_pipeline_{}_record", self.pipeline_draft[i].id);
            ui.push_id(&pipeline_field_id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("ID");
                    edited |= ui
                        .push_id("pipeline_id", |ui| {
                            ui.text_edit_singleline(&mut self.pipeline_draft[i].id)
                        })
                        .inner
                        .changed();
                    ui.label("Label");
                    edited |= ui
                        .push_id("pipeline_label", |ui| {
                            ui.text_edit_singleline(&mut self.pipeline_draft[i].label)
                        })
                        .inner
                        .changed();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Duplicate").clicked() {
                            let mut p = self.pipeline_draft[i].clone();
                            p.id = Self::duplicate_pipeline_id(catalog.as_ref(), &p.id);
                            p.label = format!("{} Copy", p.label);
                            self.duplicate_save_confirmation = Some(p.id.clone());
                            self.pipeline_draft.push(p);
                            self.unsaved_pipeline_draft = true;
                        }
                        if ui.button("↑").clicked() && i > 0 {
                            swap = Some((i, i - 1));
                        }
                        if ui.button("↓").clicked() && i + 1 < self.pipeline_draft.len() {
                            swap = Some((i, i + 1));
                        }
                        if ui.button("Delete").clicked() {
                            self.pipeline_delete_confirmation =
                                Some(self.pipeline_draft[i].id.clone());
                            remove = Some(self.pipeline_draft[i].id.clone());
                        }
                    });
                });
                let mut aliases = self.pipeline_draft[i].aliases.join(", ");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut aliases)
                            .hint_text("Aliases (comma-separated)"),
                    )
                    .changed()
                {
                    self.pipeline_draft[i].aliases = aliases
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                    self.unsaved_pipeline_draft = true;
                }
                let mut remove_stage = None;
                for j in 0..self.pipeline_draft[i].stages.len() {
                    let stage_id = egui::Id::new((pipeline_field_id.as_str(), "stage", j));
                    ui.push_id(stage_id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            compact_section_heading(ui, &format!("Stage {}", j + 1), None);
                            if ui.button("Remove stage").clicked() {
                                remove_stage = Some(j);
                            }
                        });
                        edited |= stage_spec_editor(
                            ui,
                            stage_id.with("editor"),
                            &mut self.pipeline_draft[i].stages[j],
                        );
                    });
                }
                if let Some(j) = remove_stage {
                    self.pipeline_draft[i].stages.remove(j);
                    edited = true;
                }
                ui.horizontal(|ui| {
                    stage_operation_selector(
                        ui,
                        egui::Id::new((pipeline_field_id.as_str(), "add_stage_operation")),
                        &mut self.selected_stage_operation,
                    );
                    if ui.button("Add stage").clicked() {
                        self.pipeline_draft[i].stages.push(StageSpec {
                            operation: self.selected_stage_operation,
                            arguments: Default::default(),
                        });
                        self.unsaved_pipeline_draft = true;
                    }
                });
                if ui.button("Preview complete output").clicked() {
                    self.request_preview(
                        ClipboardModifyIntent::Stages(self.pipeline_draft[i].stages.clone()),
                        catalog.clone(),
                    );
                }
            });
        }
        if let Some((a, b)) = swap {
            let _ = self.reorder_pipeline_draft_and_save(catalog.as_ref(), store, a, b);
        }
        if let Some(id) = remove {
            let _ = self.delete_pipeline_from_draft_and_save(catalog.as_ref(), store, &id);
        }
        if edited {
            self.unsaved_pipeline_draft = true;
        }
        if let PreviewState::Completed { display, .. } = self.preview.state() {
            ui.checkbox(&mut self.wrap_preview, "Wrap preview");
            readonly_multiline_preview(
                ui,
                egui::Id::new("cm_pipeline_preview"),
                &display.text,
                self.wrap_preview,
            );
        }
        if let Some(id) = &self.duplicate_save_confirmation {
            semantic_status(
                ui,
                StatusTone::Warning,
                "Duplicate pipeline needs confirmation",
                Some(id),
            );
            readonly_multiline_preview(
                ui,
                egui::Id::new("cm_duplicate_save_confirmation"),
                &format!("Confirm saving duplicated pipeline {id} before pressing Save."),
                true,
            );
        }
        if let Some(e) = &self.pipeline_editor_error {
            semantic_status(ui, StatusTone::Error, "Pipeline save failed", None);
            readonly_multiline_preview(ui, egui::Id::new("cm_pipeline_editor_error"), e, true);
        }
    }
    fn shortcuts<B: ClipboardBackend>(
        &mut self,
        ui: &egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        ui.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::Enter) {
                self.last_action = Some(
                    if i.modifiers.shift {
                        "apply_close"
                    } else {
                        "apply"
                    }
                    .into(),
                );
            }
            if i.modifiers.command && i.key_pressed(egui::Key::R) {
                self.last_action = Some("reload".into());
            }
        });
        if let Some(a) = self.last_action.take() {
            match a.as_str() {
                "apply" => self.commit(service, false, false),
                "apply_close" => self.commit(service, true, false),
                "reload" if self.source == self.reset_source => {
                    self.load_clipboard(service);
                    self.mark_dirty(catalog);
                }
                _ => {}
            }
        }
    }
    fn modify_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if let Some(e) = &self.source_error {
            semantic_status(ui, StatusTone::Error, "Clipboard source error", Some(e));
        }
        compact_section_heading(ui, "Modify clipboard", None);
        ui.horizontal(|ui| {
            stage_operation_selector(
                ui,
                egui::Id::new("cm_modify_add_stage_operation"),
                &mut self.selected_stage_operation,
            );
            let add_stage = ui.button("Add stage");
            #[cfg(test)]
            {
                self.test_host_add_stage =
                    Some((add_stage.rect, ui.clip_rect(), add_stage.enabled()));
            }
            if add_stage.clicked() {
                self.add_stage(self.selected_stage_operation, catalog.clone())
            }
            if ui.button("Reset").clicked() {
                self.reset(catalog.clone())
            }
            if ui.button("Reload Clipboard").clicked() {
                self.load_clipboard(service);
                self.mark_dirty(catalog.clone())
            }
        });

        ui.separator();
        self.modify_body_ui(ui, service, catalog.clone());
    }

    fn modify_body_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if self.large_confirmation_open {
            semantic_status(
                ui,
                StatusTone::Warning,
                "Preview paused for large source",
                Some(&format!("{} bytes", self.source.len())),
            );
            if ui.button("Preview this source").clicked() {
                self.acknowledge_large_source(catalog.clone());
            }
            return;
        }

        compact_section_heading(ui, "Source", None);
        if editable_multiline_ten_rows(ui, egui::Id::new("cm_source"), &mut self.source).changed() {
            self.mark_dirty(catalog.clone());
        }

        ui.separator();
        compact_section_heading(ui, "Stages", None);
        let mut remove = None;
        let mut changed = false;
        for i in 0..self.stages.len() {
            let stage_id = self.stages[i].id;
            ui.push_id(self.stages[i].id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Stage {}", i + 1,));
                    if ui.button("↑").clicked() && i > 0 {
                        self.stages.swap(i, i - 1);
                        changed = true;
                    }
                    if ui.button("↓").clicked() && i + 1 < self.stages.len() {
                        self.stages.swap(i, i + 1);
                        changed = true;
                    }
                    if ui.button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
                changed |= stage_spec_editor(
                    ui,
                    egui::Id::new(("cm_modify_stage_editor", stage_id)),
                    &mut self.stages[i].spec,
                );
            });
        }
        if let Some(i) = remove {
            self.stages.remove(i);
            changed = true;
        }
        if changed {
            self.mark_dirty(catalog.clone());
        }

        ui.separator();
        compact_section_heading(ui, "Result", None);
        self.modify_preview_ui(ui);
        self.external_change_warning_ui(ui, service);
    }

    fn modify_preview_ui(&mut self, ui: &mut egui::Ui) {
        self.preview_state_ui(ui, egui::Id::new("cm_modify_result"));
    }

    fn preview_state_ui(&mut self, ui: &mut egui::Ui, preview_id: egui::Id) {
        match self.preview.state() {
            PreviewState::Completed { display, .. } => {
                let summary = result_preview_summary(
                    display.metadata.bytes,
                    display.metadata.chars,
                    display.metadata.lines,
                    display.truncated,
                );
                semantic_status(
                    ui,
                    if display.truncated {
                        StatusTone::Warning
                    } else {
                        StatusTone::Success
                    },
                    &summary,
                    None,
                );
                ui.checkbox(&mut self.wrap_preview, "Wrap preview");
                readonly_multiline_preview(ui, preview_id, &display.text, self.wrap_preview);
            }
            PreviewState::Failed { error, .. } => {
                semantic_status(ui, StatusTone::Error, "Preview failed", Some(error));
            }
            PreviewState::PendingDebounce { .. } => {
                semantic_status(ui, StatusTone::Info, "Preview pending", None);
            }
            PreviewState::Running { .. } => {
                semantic_status(ui, StatusTone::Info, "Preview running", None);
            }
            PreviewState::Cancelled { .. } => {
                semantic_status(ui, StatusTone::Info, "Preview cancelled", None);
            }
            PreviewState::IdleMissing => {
                semantic_status(ui, StatusTone::Info, "Preview not requested", None);
            }
        }
    }

    fn external_change_warning_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
    ) {
        if let Some(cur) = &self.external_change {
            ui.separator();
            let message = external_clipboard_change_warning(cur);
            semantic_status(
                ui,
                StatusTone::Warning,
                "Clipboard changed",
                Some("Confirm before overwriting the current clipboard."),
            );
            readonly_multiline_preview(
                ui,
                egui::Id::new("cm_external_change_warning"),
                &message,
                true,
            );
            let button = ui.button("Overwrite clipboard");
            let response = with_button_emphasis(ui, button, ButtonEmphasis::Destructive);
            if response.clicked() {
                let close = matches!(
                    self.pending_action,
                    Some(PendingDialogAction::ApplyAndClose)
                );
                if self.hosted_effects.is_some() {
                    self.queue_host_commit(
                        close,
                        ClipboardModifyCommitMode::ReviewedOverwrite(cur.clone()),
                    );
                } else {
                    self.commit(service, close, true);
                    self.clear_overwrite_review();
                }
            }
        }
    }

    fn modify_footer_ui<B: ClipboardBackend>(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ClipboardService<B>>,
    ) -> egui::Response {
        let labels = section_footer_action_labels(ClipboardModifyDialogSection::Modify);
        let (button_height, content_width) = footer_button_row_metrics(ui, labels);
        let needs_scroll = content_width > ui.available_width();
        let scroll_height = button_height
            + if needs_scroll {
                ui.spacing().scroll.bar_width
            } else {
                0.0
            };
        let visibility = if needs_scroll {
            egui::scroll_area::ScrollBarVisibility::AlwaysVisible
        } else {
            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
        };
        #[cfg(test)]
        let mut footer_clip_rect = egui::Rect::NOTHING;
        let output = egui::ScrollArea::horizontal()
            .id_salt("clipboard_modify_footer_actions")
            .auto_shrink([false, true])
            .scroll_bar_visibility(visibility)
            .max_height(scroll_height)
            .show(ui, |ui| {
                #[cfg(test)]
                {
                    footer_clip_rect = ui.clip_rect();
                }
                ui.set_max_height(button_height);
                ui.horizontal(|ui| {
                    let enabled = self.can_apply();
                    let apply_response = ui
                        .add_enabled_ui(enabled, |ui| {
                            let button = ui.button("Apply");
                            let response =
                                with_button_emphasis(ui, button, ButtonEmphasis::Primary);
                            #[cfg(test)]
                            {
                                self.test_host_apply =
                                    Some((response.rect, ui.clip_rect(), response.enabled()));
                            }
                            if response.clicked() {
                                self.commit(service, false, false);
                            }
                            response
                        })
                        .inner;
                    let apply_close = ui.add_enabled(enabled, egui::Button::new("Apply and Close"));
                    if apply_close.clicked() {
                        self.commit(service, true, false);
                    }
                    let copy_result = ui.add_enabled(enabled, egui::Button::new("Copy Result"));
                    if copy_result.clicked() {
                        self.commit(service, false, true);
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                        self.cleanup_after_close();
                    }
                    apply_response
                })
                .inner
            });
        #[cfg(test)]
        {
            self.apply_footer_clip_rect = Some(footer_clip_rect);
        }
        output.inner
    }
    fn queue_host_commit(&mut self, close: bool, mode: ClipboardModifyCommitMode) {
        if let Some(effects) = &mut self.hosted_effects
            && let PreviewState::Completed { id, .. } = self.preview.state()
        {
            effects.push(ClipboardModifyHostedEffect::Commit {
                id: *id,
                source: self.source.clone(),
                baseline: self.baseline.clone(),
                close,
                mode,
            });
        }
    }

    fn pending_commit(close: bool) -> PendingDialogAction {
        if close {
            PendingDialogAction::ApplyAndClose
        } else {
            PendingDialogAction::Apply
        }
    }

    fn clear_overwrite_review(&mut self) {
        self.external_change = None;
        self.pending_action = None;
        self.overwrite_preview = None;
    }

    pub fn commit<B: ClipboardBackend>(
        &mut self,
        service: &Arc<ClipboardService<B>>,
        close: bool,
        confirmed: bool,
    ) {
        if self.hosted_effects.is_some() {
            self.queue_host_commit(
                close,
                if confirmed {
                    ClipboardModifyCommitMode::CopyResult
                } else {
                    ClipboardModifyCommitMode::Apply
                },
            );
            return;
        }
        if let Some(out) = self.preview.apply_text() {
            match service.commit_dialog(
                &self.baseline,
                &self.source,
                out,
                confirmed,
                "Clipboard Modify dialog",
            ) {
                Ok(_) => {
                    if close {
                        self.open = false;
                        self.cleanup_after_close();
                    }
                }
                Err(ClipboardError::ConfirmationRequired(c)) => {
                    self.external_change = Some(c.current);
                    self.pending_action = Some(if close {
                        PendingDialogAction::ApplyAndClose
                    } else {
                        PendingDialogAction::Apply
                    });
                }
                Err(e) => self.source_error = Some(e.to_string()),
            }
        }
    }
}

pub fn result_preview_summary(bytes: usize, chars: usize, lines: usize, truncated: bool) -> String {
    format!(
        "Result: {bytes} bytes, {chars} chars, {lines} lines{}",
        if truncated {
            " (visible preview truncated)"
        } else {
            ""
        }
    )
}

pub fn capped_current_source_preview(source: &str) -> String {
    const CAP: usize = 80;
    let mut preview: String = source.chars().take(CAP).collect();
    if source.chars().nth(CAP).is_some() {
        preview.push('…');
    }
    preview
}

pub fn external_clipboard_change_warning(cur: &ClipboardSummary) -> String {
    format!(
        "Clipboard changed since preview source was captured. Current clipboard: {} bytes, {} chars, fingerprint {:016x}. Applying overwrites it; undo restores the replaced value.",
        cur.bytes, cur.chars, cur.fingerprint
    )
}

pub fn multiline_control_height_for(row_height: f32, frame_padding_y: f32) -> f32 {
    row_height * CLIPBOARD_MODIFY_MULTILINE_ROWS as f32 + frame_padding_y * 2.0
}

pub fn multiline_control_height(ui: &egui::Ui) -> f32 {
    multiline_control_height_for(
        ui.text_style_height(&egui::TextStyle::Body),
        ui.spacing().button_padding.y,
    )
}

fn editable_multiline_ten_rows(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
) -> egui::Response {
    let size = egui::vec2(ui.available_width(), multiline_control_height(ui));
    ui.add_sized(
        size,
        egui::TextEdit::multiline(text)
            .id_salt(id)
            .desired_rows(CLIPBOARD_MODIFY_MULTILINE_ROWS)
            .desired_width(f32::INFINITY),
    )
}

fn stage_argument_editor(ui: &mut egui::Ui, id: egui::Id, text: &mut String) -> egui::Response {
    if text.contains('\n') {
        editable_multiline_ten_rows(ui, id, text)
    } else {
        ui.add(egui::TextEdit::singleline(text).id_salt(id))
    }
}

fn stage_operation_selector(ui: &mut egui::Ui, id: egui::Id, operation: &mut OperationId) {
    let selected_label = crate::clipboard_modify::catalog::operation_by_id(*operation)
        .map(|info| info.label)
        .unwrap_or_else(|| crate::clipboard_modify::catalog::canonical_command(*operation));
    let combo = egui::ComboBox::from_id_salt(id)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for info in crate::clipboard_modify::catalog::operations()
                .iter()
                .filter(|info| info.pipeline_available)
            {
                ui.selectable_value(operation, info.id, info.label);
            }
        });
    record_clipboard_modify_combo(ui, &combo.response);
}

fn edit_optional_stage_argument(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    slot: &mut Option<String>,
) -> bool {
    ui.label(label);
    let mut value = slot.clone().unwrap_or_default();
    let changed = stage_argument_editor(ui, id, &mut value).changed();
    if changed {
        *slot = Some(value);
    }
    changed
}

fn stage_spec_editor(ui: &mut egui::Ui, id: egui::Id, spec: &mut StageSpec) -> bool {
    let mut changed = false;
    let previous = spec.operation;
    stage_operation_selector(ui, id.with("operation"), &mut spec.operation);
    if spec.operation != previous {
        // Changing the operation is explicit and clears arguments that belong
        // to the previous operation. Merely rendering an editor never does.
        spec.arguments = StageArguments::default();
        changed = true;
    }

    let requirements = crate::clipboard_modify::catalog::operation_by_id(spec.operation)
        .map(|info| info.argument_requirements);
    match requirements {
        Some(ArgumentRequirements::None) | None => {}
        Some(ArgumentRequirements::CustomWrap) => {
            ui.horizontal_wrapped(|ui| {
                changed |= edit_optional_stage_argument(
                    ui,
                    id.with("prefix"),
                    "Prefix",
                    &mut spec.arguments.prefix,
                );
                changed |= edit_optional_stage_argument(
                    ui,
                    id.with("suffix"),
                    "Suffix",
                    &mut spec.arguments.suffix,
                );
            });
        }
        Some(ArgumentRequirements::NamedWrap | ArgumentRequirements::Template) => {
            ui.horizontal_wrapped(|ui| {
                changed |= edit_optional_stage_argument(
                    ui,
                    id.with("name"),
                    "Template or wrapper",
                    &mut spec.arguments.name,
                );
            });
        }
        Some(ArgumentRequirements::CodeBlock) => {
            ui.horizontal_wrapped(|ui| {
                changed |= edit_optional_stage_argument(
                    ui,
                    id.with("language"),
                    "Optional language",
                    &mut spec.arguments.language,
                );
            });
        }
        Some(ArgumentRequirements::OptionalSeparator) => {
            ui.horizontal_wrapped(|ui| {
                let mut explicit = spec.arguments.separator.is_some();
                if ui
                    .checkbox(&mut explicit, "Set literal separator (default: one space)")
                    .changed()
                {
                    spec.arguments.separator = explicit.then(String::new);
                    changed = true;
                }
                if let Some(separator) = &mut spec.arguments.separator {
                    changed |= stage_argument_editor(ui, id.with("separator"), separator).changed();
                }
            });
        }
        Some(ArgumentRequirements::RequiredSeparator) => {
            ui.horizontal_wrapped(|ui| {
                changed |= edit_optional_stage_argument(
                    ui,
                    id.with("separator"),
                    "Nonempty literal separator",
                    &mut spec.arguments.separator,
                );
            });
        }
    }
    changed
}

fn readonly_multiline_preview(ui: &mut egui::Ui, id: egui::Id, text: &str, wrap: bool) {
    let size = egui::vec2(ui.available_width(), multiline_control_height(ui));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(size);
        ui.set_max_size(size);
        if wrap {
            egui::ScrollArea::vertical()
                .id_salt(id.with("vertical_scroll"))
                .auto_shrink([false, false])
                .max_height(size.y)
                .show(ui, |ui| {
                    ui.set_width(size.x);
                    ui.add(
                        egui::Label::new(text)
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                });
        } else {
            egui::ScrollArea::both()
                .id_salt(id.with("both_scroll"))
                .auto_shrink([false, false])
                .max_height(size.y)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.add(
                        egui::Label::new(text)
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        }
    });
}

pub fn tab_scroll_id(section: ClipboardModifyDialogSection) -> &'static str {
    match section {
        ClipboardModifyDialogSection::Modify => "clipboard_modify_modify_scroll",
        ClipboardModifyDialogSection::Templates => "clipboard_modify_templates_scroll",
        ClipboardModifyDialogSection::SavedPipelines => "clipboard_modify_saved_pipelines_scroll",
        ClipboardModifyDialogSection::ManageTemplates => "clipboard_modify_manage_templates_scroll",
        ClipboardModifyDialogSection::ManagePipelines => "clipboard_modify_manage_pipelines_scroll",
        ClipboardModifyDialogSection::Help => "clipboard_modify_help_scroll",
    }
}

pub fn clipboard_modify_tab_navigation_height(button_height: f32) -> f32 {
    button_height.max(1.0) + 8.0
}

fn tab_navigation_reserved_height(ui: &mut egui::Ui) -> f32 {
    let labels = CLIPBOARD_MODIFY_TAB_LABELS.map(|(_, label)| label);
    let (button_height, content_width) = footer_button_row_metrics(ui, &labels);
    let scrollbar_height = if content_width > ui.available_width() {
        ui.spacing().scroll.bar_width
    } else {
        0.0
    };
    clipboard_modify_tab_navigation_height(button_height) + scrollbar_height
}

pub fn clipboard_modify_body_max_height(
    available_height: f32,
    navigation_reserved_height: f32,
    footer_reserved_height: f32,
) -> f32 {
    (available_height - navigation_reserved_height - footer_reserved_height).max(0.0)
}

fn section_footer_reserved_height(
    ui: &mut egui::Ui,
    section: ClipboardModifyDialogSection,
    has_footer: bool,
) -> f32 {
    if !has_footer {
        return 0.0;
    }
    let labels = section_footer_action_labels(section);
    let (button_height, content_width) = footer_button_row_metrics(ui, labels);
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let scrollbar_height = if content_width > ui.available_width() {
        spacing.scroll.bar_width
    } else {
        0.0
    };
    button_height + scrollbar_height + 2.0 * spacing.item_spacing.y + separator_height
}

fn section_footer_action_labels(section: ClipboardModifyDialogSection) -> &'static [&'static str] {
    match section {
        ClipboardModifyDialogSection::Modify => {
            &["Apply", "Apply and Close", "Copy Result", "Cancel"]
        }
        ClipboardModifyDialogSection::Templates => &["Apply Template"],
        ClipboardModifyDialogSection::ManageTemplates
        | ClipboardModifyDialogSection::ManagePipelines => &["Save"],
        ClipboardModifyDialogSection::SavedPipelines | ClipboardModifyDialogSection::Help => &[],
    }
}

fn footer_button_row_metrics(ui: &mut egui::Ui, labels: &[&str]) -> (f32, f32) {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let text_height =
        ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + spacing.extra_text_line_spacing;
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height =
        (text_height + button_frame.total_margin().sum().y).max(spacing.interact_size.y);
    let text_color = ui.visuals().text_color();
    let mut content_width = 0.0;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let button_width =
            (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        if content_width > 0.0 {
            content_width += spacing.item_spacing.x;
        }
        content_width += button_width;
    }
    (button_height, content_width)
}

pub fn section_scroll_id(section: ClipboardModifyDialogSection) -> &'static str {
    tab_scroll_id(section)
}

pub fn validate_clipboard_modify_size(width: f32, height: f32) -> Option<egui::Vec2> {
    if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
        Some(egui::vec2(width, height))
    } else {
        None
    }
}

pub fn clipboard_modify_default_size() -> egui::Vec2 {
    egui::vec2(
        CLIPBOARD_MODIFY_DEFAULT_WIDTH,
        CLIPBOARD_MODIFY_DEFAULT_HEIGHT,
    )
}

fn clipboard_modify_default_window_size(
    legacy_size: egui::Vec2,
    preference: crate::settings::UtilitySizePreference,
) -> egui::Vec2 {
    match preference {
        crate::settings::UtilitySizePreference::Missing => legacy_size,
        crate::settings::UtilitySizePreference::Reset
        | crate::settings::UtilitySizePreference::Preferred(_)
        | crate::settings::UtilitySizePreference::Unsupported => clipboard_modify_default_size(),
    }
}

pub(crate) fn clipboard_modify_legacy_window_size(
    settings: &crate::settings::Settings,
) -> egui::Vec2 {
    let configured = settings
        .plugin_settings
        .get("clipboard_modify")
        .cloned()
        .and_then(|value| {
            serde_json::from_value::<
                    crate::plugins::clipboard_modify::ClipboardModifyPluginSettings,
                >(value)
                .ok()
        });
    configured
        .and_then(|preferences| {
            validate_clipboard_modify_size(preferences.dialog_width, preferences.dialog_height)
        })
        .unwrap_or_else(clipboard_modify_default_size)
}

pub fn clipboard_modify_min_size() -> egui::Vec2 {
    egui::vec2(CLIPBOARD_MODIFY_MIN_WIDTH, CLIPBOARD_MODIFY_MIN_HEIGHT)
}

pub fn clipboard_modify_usable_viewport_size(viewport_size: egui::Vec2) -> egui::Vec2 {
    let margin = CLIPBOARD_MODIFY_VIEWPORT_MARGIN * 2.0;
    egui::vec2(
        (viewport_size.x - margin).max(1.0),
        (viewport_size.y - margin).max(1.0),
    )
}

pub fn effective_clipboard_modify_min_size(max_size: egui::Vec2) -> egui::Vec2 {
    let nominal_min = clipboard_modify_min_size();
    egui::vec2(nominal_min.x.min(max_size.x), nominal_min.y.min(max_size.y))
}

pub fn clamp_clipboard_modify_size(
    requested_size: egui::Vec2,
    min_size: egui::Vec2,
    max_size: egui::Vec2,
) -> egui::Vec2 {
    let fallback = clipboard_modify_default_size();
    let requested_size =
        validate_clipboard_modify_size(requested_size.x, requested_size.y).unwrap_or(fallback);
    let effective_min = egui::vec2(min_size.x.min(max_size.x), min_size.y.min(max_size.y));
    egui::vec2(
        requested_size.x.clamp(effective_min.x, max_size.x),
        requested_size.y.clamp(effective_min.y, max_size.y),
    )
}

pub fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hosted_pipeline_edit_revokes_apply_and_pinned_apply_keeps_the_session() {
        use crate::clipboard_modify::clipboard::fake::FakeClipboardBackend;

        let clipboard = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            "alpha",
        )));
        let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
        let mut state = ClipboardModifyDialogState::default();
        state.open_section(ClipboardModifyDialogSection::Modify, &clipboard);
        state.add_stage(OperationId::Uppercase, catalog.clone());
        state.preview.force_start_pending();
        assert_eq!(state.preview.apply_text(), Some("ALPHA"));

        state.hosted_effects = Some(Vec::new());
        state.commit(&clipboard, true, false);
        state.add_stage(OperationId::Lowercase, catalog.clone());
        let actions = ClipboardModifyHostActions {
            effects: state.hosted_effects.take().unwrap(),
            session_epoch: state.host_session_epoch(),
            ..Default::default()
        };
        assert!(!state.complete_host_actions(actions, &clipboard, catalog.clone(), None));
        assert!(clipboard.backend().writes().is_empty());
        assert!(!state.can_apply());

        state.preview.force_start_pending();
        assert_eq!(state.preview.apply_text(), Some("alpha"));
        state.hosted_effects = Some(Vec::new());
        state.commit(&clipboard, true, false);
        let actions = ClipboardModifyHostActions {
            effects: state.hosted_effects.take().unwrap(),
            session_epoch: state.host_session_epoch(),
            pinned: true,
            ..Default::default()
        };
        assert!(!state.complete_host_actions(actions, &clipboard, catalog, None));
        assert_eq!(clipboard.backend().writes(), ["alpha"]);
        assert!(state.open);
        assert_eq!(state.stages.len(), 2);
    }

    #[test]
    fn hosted_overwrite_requires_the_same_preview_and_reviewed_clipboard() {
        use crate::clipboard_modify::clipboard::fake::{FakeClipboardBackend, Op};

        let clipboard = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            "alpha",
        )));
        let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
        let mut state = ClipboardModifyDialogState::default();
        state.open_section(ClipboardModifyDialogSection::Modify, &clipboard);
        state.add_stage(OperationId::Uppercase, catalog.clone());
        state.preview.force_start_pending();
        assert_eq!(state.preview.apply_text(), Some("ALPHA"));

        let stage_commit = |state: &mut ClipboardModifyDialogState, mode| {
            state.hosted_effects = Some(Vec::new());
            state.queue_host_commit(false, mode);
            ClipboardModifyHostActions {
                effects: state.hosted_effects.take().unwrap(),
                session_epoch: state.host_session_epoch(),
                ..Default::default()
            }
        };
        clipboard.backend().push(Op::External("review A".into()));
        let apply = stage_commit(&mut state, ClipboardModifyCommitMode::Apply);
        state.complete_host_actions(apply, &clipboard, catalog.clone(), None);
        let review_a = state.external_change.clone().unwrap();
        let stale = stage_commit(
            &mut state,
            ClipboardModifyCommitMode::ReviewedOverwrite(review_a),
        );

        state.source = "beta".into();
        state.mark_dirty(catalog.clone());
        state.preview.force_start_pending();
        assert_eq!(state.preview.apply_text(), Some("BETA"));
        state.complete_host_actions(stale, &clipboard, catalog.clone(), None);
        assert!(clipboard.backend().writes().is_empty());

        let apply = stage_commit(&mut state, ClipboardModifyCommitMode::Apply);
        state.complete_host_actions(apply, &clipboard, catalog.clone(), None);
        let review_b = state.external_change.clone().unwrap();
        let overwrite = stage_commit(
            &mut state,
            ClipboardModifyCommitMode::ReviewedOverwrite(review_b),
        );
        clipboard.backend().push(Op::External("late change".into()));
        state.complete_host_actions(overwrite, &clipboard, catalog.clone(), None);
        assert!(clipboard.backend().writes().is_empty());
        let latest = state.external_change.clone().unwrap();
        assert_eq!(latest, ClipboardSummary::new("late change"));
        let overwrite = stage_commit(
            &mut state,
            ClipboardModifyCommitMode::ReviewedOverwrite(latest),
        );
        state.complete_host_actions(overwrite, &clipboard, catalog, None);
        assert_eq!(clipboard.backend().writes(), ["BETA"]);
        assert_eq!(
            clipboard.undo_record().unwrap().original_text,
            "late change"
        );
    }

    #[test]
    fn hosted_preview_is_deferred_and_checkpoint_preserves_worker() {
        let mut state = super::ClipboardModifyDialogState::default();
        state.open_section_without_loading_for_test(super::ClipboardModifyDialogSection::Modify);
        state.source = "before".into();
        let checkpoint = super::ClipboardModifyInteractionCheckpoint::capture(&state);
        state.hosted_effects = Some(Vec::new());
        state.source = "after".into();
        state.mark_dirty(std::sync::Arc::new(
            crate::clipboard_modify::defaults::default_catalog(),
        ));
        assert!(matches!(
            state.preview.state(),
            crate::clipboard_modify::coordinator::PreviewState::IdleMissing
        ));
        assert_eq!(state.hosted_effects.as_ref().unwrap().len(), 1);
        state.hosted_effects = None;
        checkpoint.restore(&mut state);
        assert_eq!(state.source, "before");
        assert!(matches!(
            state.preview.state(),
            crate::clipboard_modify::coordinator::PreviewState::IdleMissing
        ));
    }

    use super::*;
    use crate::clipboard_modify::model::{ClipboardModifierCatalog, SavedPipeline};
    use crate::clipboard_modify::store::ClipboardModifierStore;
    use std::sync::RwLock;

    fn tmpl(id: &str, label: &str, aliases: &[&str], body: &str) -> ClipboardTemplate {
        ClipboardTemplate {
            id: id.into(),
            label: label.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            template: body.into(),
            processor: Some(TemplateProcessor::Literal),
        }
    }

    fn assert_vec2_eq(actual: egui::Vec2, expected: egui::Vec2) {
        assert_eq!(actual.x, expected.x);
        assert_eq!(actual.y, expected.y);
    }

    #[test]
    fn multiline_rows_constant_is_ten() {
        assert_eq!(CLIPBOARD_MODIFY_MULTILINE_ROWS, 10);
    }

    #[test]
    fn multiline_control_height_includes_ten_rows_and_frame_padding() {
        assert_eq!(multiline_control_height_for(12.5, 2.0), 129.0);
    }

    #[test]
    fn tab_scroll_ids_are_distinct() {
        let sections = [
            ClipboardModifyDialogSection::Modify,
            ClipboardModifyDialogSection::Templates,
            ClipboardModifyDialogSection::SavedPipelines,
            ClipboardModifyDialogSection::ManageTemplates,
            ClipboardModifyDialogSection::ManagePipelines,
            ClipboardModifyDialogSection::Help,
        ];
        let ids: HashSet<&'static str> = sections.into_iter().map(tab_scroll_id).collect();
        assert_eq!(ids.len(), sections.len());
    }

    #[test]
    fn section_scroll_ids_are_distinct_and_stable() {
        let sections = [
            ClipboardModifyDialogSection::Modify,
            ClipboardModifyDialogSection::Templates,
            ClipboardModifyDialogSection::SavedPipelines,
            ClipboardModifyDialogSection::ManageTemplates,
            ClipboardModifyDialogSection::ManagePipelines,
            ClipboardModifyDialogSection::Help,
        ];
        let ids: HashSet<&'static str> = sections.into_iter().map(section_scroll_id).collect();
        assert_eq!(ids.len(), sections.len());
        assert_eq!(
            section_scroll_id(ClipboardModifyDialogSection::SavedPipelines),
            "clipboard_modify_saved_pipelines_scroll"
        );
    }

    #[test]
    fn body_height_subtracts_navigation_and_footer_reservations() {
        assert_eq!(clipboard_modify_body_max_height(300.0, 40.0, 44.0), 216.0);
        assert_eq!(clipboard_modify_body_max_height(50.0, 40.0, 44.0), 0.0);
    }

    #[test]
    fn footer_reservation_matches_fixed_control_sections() {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let mut reservations = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ui| {
                let modify =
                    section_footer_reserved_height(ui, ClipboardModifyDialogSection::Modify, true);
                let apply_template = section_footer_reserved_height(
                    ui,
                    ClipboardModifyDialogSection::Templates,
                    true,
                );
                let manage = section_footer_reserved_height(
                    ui,
                    ClipboardModifyDialogSection::ManageTemplates,
                    true,
                );
                let no_footer = section_footer_reserved_height(
                    ui,
                    ClipboardModifyDialogSection::SavedPipelines,
                    false,
                );
                let button_height = footer_button_row_metrics(
                    ui,
                    section_footer_action_labels(ClipboardModifyDialogSection::Modify),
                )
                .0;
                reservations = Some((modify, apply_template, manage, no_footer, button_height));
            },
        );
        let (modify, apply_template, manage, no_footer, button_height) =
            reservations.expect("the UI pass records its footer measurements");
        output.drop_without_applying_deltas();
        assert!(modify >= button_height);
        assert!(apply_template >= button_height);
        assert!(manage >= button_height);
        assert_eq!(no_footer, 0.0);
    }

    #[test]
    fn modify_constrained_navigation_body_footer_and_disabled_apply_remain_usable() {
        use crate::clipboard_modify::clipboard::{
            ArboardClipboardBackend, ProductionClipboardService,
        };
        use crate::gui::theme::initialize_runtime;
        use crate::settings::ThemeSettings;
        use std::sync::Arc;

        let ctx = egui::Context::default();
        let mut theme = ThemeSettings::default();
        theme.preferences.typography_scale = 1.75;
        initialize_runtime(&ctx, &theme);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 160.0));
        let window_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen_rect.size());
        let mut state = ClipboardModifyDialogState::default();
        state.open = true;
        state.section = ClipboardModifyDialogSection::Modify;
        state.source = "clipboard source".into();
        let service = Arc::new(ProductionClipboardService::new(ArboardClipboardBackend));
        let catalog = Arc::new(cat());
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let render_frame = |state: &mut ClipboardModifyDialogState| {
            let mut apply_enabled = true;
            let mut apply_rect = egui::Rect::NOTHING;
            let mut window_content_rect = egui::Rect::NOTHING;
            let mut rendered_window_rect = egui::Rect::NOTHING;
            let mut sizing_pass = true;
            let mut visible = false;
            let output = ctx.run_ui(input(), |ctx| {
                let window_output = egui::Window::new("Clipboard Modify constrained layout")
                    .fixed_rect(window_rect)
                    .resizable(false)
                    .min_size(effective_clipboard_modify_min_size(
                        clipboard_modify_usable_viewport_size(screen_rect.size()),
                    ))
                    .show(ctx, |ui| {
                        window_content_rect = ui.max_rect();
                        sizing_pass = ui.is_sizing_pass();
                        visible = ui.is_visible();
                        let apply = state
                            .render_section_contents(ui, &service, catalog.clone(), None)
                            .expect("Modify always has a footer");
                        apply_enabled = apply.enabled();
                        apply_rect = apply.rect;
                    });
                if let Some(window_output) = window_output {
                    rendered_window_rect = window_output.response.rect;
                }
            });
            (
                output,
                sizing_pass,
                visible,
                apply_enabled,
                apply_rect,
                window_content_rect,
                rendered_window_rect,
            )
        };
        let (first_frame_output, first_frame_sizing_pass, first_frame_visible, _, _, _, _) =
            render_frame(&mut state);
        first_frame_output.drop_without_applying_deltas();

        let (
            output,
            sizing_pass,
            visible,
            apply_enabled,
            initial_apply_rect,
            initial_window_content_rect,
            rendered_window_rect,
        ) = render_frame(&mut state);
        let initial_layout = state
            .section_layout_observation
            .expect("the rendered section records its constrained layout");
        let outer_scroll = initial_layout
            .constrained_scroll
            .expect("navigation and footer use native outer scrolling when they cannot fit");
        output.drop_without_applying_deltas();

        assert!(
            first_frame_sizing_pass && !first_frame_visible,
            "first Window frame should be the invisible sizing pass"
        );
        assert!(
            !sizing_pass && visible,
            "Modify geometry must come from a visible non-sizing frame"
        );
        assert!(
            initial_layout.navigation_overflows,
            "the existing tabs remain horizontally scrollable"
        );
        assert!(
            initial_layout.body_max_height > 0.0
                && initial_layout.body_rect.height() <= initial_layout.body_max_height + 0.5,
            "the source and forms retain a bounded positive body viewport"
        );
        assert!(
            outer_scroll.content_size.y > outer_scroll.inner_rect.height(),
            "constrained chrome remains reachable through native vertical scrolling"
        );

        let mut scroll_state = egui::scroll_area::State::load(&ctx, outer_scroll.id)
            .expect("constrained content stores native scroll state");
        scroll_state.offset.y = (scroll_state.offset.y + initial_apply_rect.center().y
            - outer_scroll.inner_rect.center().y)
            .max(0.0);
        scroll_state.store(&ctx, outer_scroll.id);
        let (
            scrolled_output,
            scrolled_sizing_pass,
            scrolled_visible,
            _,
            scrolled_apply_rect,
            scrolled_window_content_rect,
            scrolled_window_rect,
        ) = render_frame(&mut state);
        let scrolled_layout = state
            .section_layout_observation
            .expect("the scrolled section records its native viewport");
        let scrolled_outer_scroll = scrolled_layout
            .constrained_scroll
            .expect("the constrained content keeps the native overflow owner");
        let apply_footer_clip = state
            .apply_footer_clip_rect
            .expect("Apply records the native horizontal footer clip");
        let apply_visible_clip = scrolled_window_content_rect
            .intersect(scrolled_outer_scroll.inner_rect)
            .intersect(scrolled_outer_scroll.clip_rect)
            .intersect(apply_footer_clip);
        let apply_is_fully_visible = apply_visible_clip.contains_rect(scrolled_apply_rect);
        let window_fits_screen = screen_rect.contains_rect(initial_window_content_rect)
            && screen_rect.contains_rect(scrolled_window_content_rect)
            && screen_rect.contains_rect(rendered_window_rect)
            && screen_rect.contains_rect(scrolled_window_rect)
            && rendered_window_rect.width() <= screen_rect.width()
            && rendered_window_rect.height() <= screen_rect.height()
            && scrolled_window_rect.width() <= screen_rect.width()
            && scrolled_window_rect.height() <= screen_rect.height();
        scrolled_output.drop_without_applying_deltas();
        assert!(!scrolled_sizing_pass && scrolled_visible);
        assert!(
            window_fits_screen,
            "the native overflow owner keeps the window within its viewport"
        );
        assert!(
            apply_is_fully_visible,
            "Apply is fully visible within the native scrolled window and footer clips"
        );
        assert!(
            !apply_enabled,
            "Apply stays disabled without a completed preview"
        );
        assert!(!state.can_apply());
    }

    #[test]
    fn clipboard_modify_line_cleanup_editor_is_passive_and_exposes_new_operations() {
        let mut spec = StageSpec {
            operation: OperationId::JoinLines,
            arguments: StageArguments::default(),
        };
        let original = spec.clone();
        let ctx = egui::Context::default();
        let mut changed = false;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 500.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    changed =
                        stage_spec_editor(ui, egui::Id::new("passive_join_editor"), &mut spec);
                });
            },
        );
        output.drop_without_applying_deltas();

        assert!(!changed, "passive rendering does not edit the stage draft");
        assert_eq!(
            spec, original,
            "the unset join separator remains the default"
        );
        for operation in [
            OperationId::RemoveBlankLines,
            OperationId::NumberLines,
            OperationId::ReverseLines,
            OperationId::JoinLines,
            OperationId::SplitLines,
        ] {
            assert!(ClipboardModifyDialogState::allowed_stage_operations().contains(&operation));
        }
    }

    #[test]
    fn clipboard_modify_line_cleanup_preview_applies_and_undoes_through_real_response() {
        use crate::clipboard_modify::clipboard::fake::FakeClipboardBackend;

        let original = " a  \r\n \t\r\nβ\n".to_string();
        let service = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            original.clone(),
        )));
        let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
        let mut state = ClipboardModifyDialogState::default();
        state.source = original.clone();
        state.baseline = original.clone();
        state.reset_source = original.clone();
        state.stages.push(DialogStage {
            id: 41,
            spec: StageSpec {
                operation: OperationId::RemoveBlankLines,
                arguments: StageArguments::default(),
            },
            error: None,
        });
        state.mark_dirty(catalog);
        state.preview.force_start_pending();

        let expected = " a  \nβ";
        assert_eq!(state.preview.apply_text(), Some(expected));
        assert!(
            service.backend().writes().is_empty(),
            "preview is read-only"
        );

        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
        let frame = |events: Vec<egui::Event>, state: &mut ClipboardModifyDialogState| {
            let mut response = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        response = Some(state.modify_footer_ui(ui, &service));
                    });
                },
            );
            (output, response.expect("Apply response is rendered"))
        };

        let (output, apply) = frame(Vec::new(), &mut state);
        let position = apply.rect.center();
        let initially_enabled = apply.enabled();
        output.drop_without_applying_deltas();
        assert!(initially_enabled, "the completed preview enables Apply");
        assert!(service.backend().writes().is_empty());

        let (output, _) = frame(vec![egui::Event::PointerMoved(position)], &mut state);
        output.drop_without_applying_deltas();
        let (output, _) = frame(
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut state,
        );
        output.drop_without_applying_deltas();
        let (output, released) = frame(
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut state,
        );
        let clicked = released.clicked();
        output.drop_without_applying_deltas();

        assert!(clicked, "the enabled Apply Response received the click");
        assert_eq!(service.backend().writes(), vec![expected.to_string()]);
        service.undo().unwrap();
        assert_eq!(
            service.backend().writes(),
            vec![expected.to_string(), original]
        );
    }

    #[test]
    fn clipboard_modify_line_cleanup_rejects_empty_split_without_clipboard_write() {
        use crate::clipboard_modify::clipboard::fake::FakeClipboardBackend;

        let service = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            "a|b".to_string(),
        )));
        let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
        let mut state = ClipboardModifyDialogState::default();
        state.source = "a|b".into();
        state.baseline = state.source.clone();
        state.reset_source = state.source.clone();
        state.stages.push(DialogStage {
            id: 7,
            spec: StageSpec {
                operation: OperationId::SplitLines,
                arguments: StageArguments {
                    separator: Some(String::new()),
                    ..Default::default()
                },
            },
            error: None,
        });
        state.mark_dirty(catalog);
        state.preview.force_start_pending();

        assert!(matches!(state.preview.state(), PreviewState::Failed { .. }));
        assert!(!state.can_apply());

        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 400.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!state.modify_footer_ui(ui, &service).enabled());
                });
            },
        );
        output.drop_without_applying_deltas();
        assert!(
            service.backend().writes().is_empty(),
            "an invalid separator cannot write even when the dialog footer renders"
        );
    }

    #[test]
    fn viewport_clamping_handles_small_and_large_viewports() {
        assert_vec2_eq(
            clipboard_modify_usable_viewport_size(egui::vec2(20.0, 24.0)),
            egui::vec2(1.0, 1.0),
        );
        assert_vec2_eq(
            clipboard_modify_usable_viewport_size(egui::vec2(2000.0, 1200.0)),
            egui::vec2(1968.0, 1168.0),
        );
    }

    #[test]
    fn legacy_geometry_is_read_only_fallback_and_invalid_values_use_default() {
        let mut settings = crate::settings::Settings::default();
        settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::json!({ "dialog_width": 777.0, "dialog_height": 555.0 }),
        );
        assert_vec2_eq(
            clipboard_modify_legacy_window_size(&settings),
            egui::vec2(777.0, 555.0),
        );

        settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::json!({ "dialog_width": -1.0, "dialog_height": 555.0 }),
        );
        assert_vec2_eq(
            clipboard_modify_legacy_window_size(&settings),
            clipboard_modify_default_size(),
        );
        settings.plugin_settings.remove("clipboard_modify");
        assert_vec2_eq(
            clipboard_modify_legacy_window_size(&settings),
            clipboard_modify_default_size(),
        );
    }

    #[test]
    fn utility_size_integration_clipboard_precedence_renders_production_window() {
        use crate::clipboard_modify::clipboard::fake::FakeClipboardBackend;

        let key = crate::settings::UtilityToolKey::parse("clipboard-modify").unwrap();
        let legacy = egui::vec2(777.0, 555.0);
        let preferred = crate::settings::PreferredWindowSize::new(720.0, 480.0).unwrap();
        let expected = [
            ("missing", None, legacy),
            (
                "preferred",
                Some(preferred),
                egui::vec2(preferred.width, preferred.height),
            ),
            ("reset", None, clipboard_modify_default_size()),
        ];

        for (mode, configured_preference, expected_size) in expected {
            let ctx = egui::Context::default();
            let directory = tempfile::tempdir().unwrap();
            let mut sizes = crate::settings::UtilityDialogSizes::default();
            match mode {
                "preferred" => {
                    sizes.set(key, configured_preference.unwrap()).unwrap();
                }
                "reset" => {
                    sizes.reset(key).unwrap();
                }
                _ => {}
            }
            let manager = crate::gui::utility_dialog_size::UtilityDialogSizeManager::new(
                directory
                    .path()
                    .join("settings.json")
                    .to_string_lossy()
                    .into_owned(),
                crate::settings_save::SettingsSaveWorkerRegistry::default(),
                sizes,
            );
            manager.install_in_context(&ctx);

            let mut settings = crate::settings::Settings::default();
            settings.plugin_settings.insert(
                "clipboard_modify".into(),
                serde_json::json!({ "dialog_width": legacy.x, "dialog_height": legacy.y }),
            );
            let original_plugin_settings = settings.plugin_settings.clone();
            let legacy_size = clipboard_modify_legacy_window_size(&settings);
            let service = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
                "unchanged clipboard".to_string(),
            )));
            let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
            let mut state = ClipboardModifyDialogState::default();
            state.open = true;
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));

            for _ in 0..4 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |root| {
                        manager.begin_frame(root.ctx()).unwrap();
                        state.ui(root.ctx(), legacy_size, &service, catalog.clone(), None);
                    },
                );
                output.drop_without_applying_deltas();
            }

            let window_rect = ctx
                .memory(|memory| memory.area_rect(egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID)))
                .expect("Clipboard Modify renders through its production Window");
            assert!(
                (window_rect.width() - expected_size.x).abs() < 2.0
                    && (window_rect.height() - expected_size.y).abs() < 2.0,
                "{mode} preference selected the wrong outer size: {window_rect:?}"
            );
            assert_eq!(
                settings.plugin_settings, original_plugin_settings,
                "legacy plugin geometry is read-only for {mode} preference"
            );
            assert!(
                service.backend().writes().is_empty(),
                "measuring the production dialog does not use the clipboard"
            );
        }
    }

    #[test]
    fn clamps_to_smaller_viewport() {
        let max_size = clipboard_modify_usable_viewport_size(egui::vec2(700.0, 500.0));
        let clamped = clamp_clipboard_modify_size(
            egui::vec2(900.0, 640.0),
            clipboard_modify_min_size(),
            max_size,
        );
        assert_vec2_eq(clamped, max_size);
    }

    #[test]
    fn clamps_when_viewport_is_smaller_than_nominal_minimum() {
        let max_size = clipboard_modify_usable_viewport_size(egui::vec2(300.0, 240.0));
        let effective_min = effective_clipboard_modify_min_size(max_size);
        assert_vec2_eq(effective_min, max_size);

        let clamped = clamp_clipboard_modify_size(
            egui::vec2(900.0, 640.0),
            clipboard_modify_min_size(),
            max_size,
        );
        assert_vec2_eq(clamped, max_size);
    }

    #[test]
    fn cleanup_after_close_preserves_window_size_and_wrap_preview() {
        use crate::clipboard_modify::clipboard::fake::FakeClipboardBackend;

        let key = crate::settings::UtilityToolKey::parse("clipboard-modify").unwrap();
        let size = crate::settings::PreferredWindowSize::new(777.0, 555.0).unwrap();
        let mut shared_sizes = crate::settings::UtilityDialogSizes::default();
        shared_sizes.set(key, size).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let workers = crate::settings_save::SettingsSaveWorkerRegistry::default();
        let manager = crate::gui::utility_dialog_size::UtilityDialogSizeManager::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            workers.clone(),
            shared_sizes,
        );
        manager.install_in_context(&ctx);
        let service = Arc::new(ClipboardService::new(FakeClipboardBackend::with_text(
            "unchanged clipboard".to_string(),
        )));
        let catalog = Arc::new(crate::clipboard_modify::defaults::default_catalog());
        let mut state = ClipboardModifyDialogState::default();
        state.wrap_preview = false;
        state.source = "source".into();
        state.open = true;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0));

        let render = |state: &mut ClipboardModifyDialogState| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |root| {
                    manager.begin_frame(root.ctx()).unwrap();
                    state.ui(
                        root.ctx(),
                        clipboard_modify_default_size(),
                        &service,
                        catalog.clone(),
                        None,
                    );
                },
            );
            output.drop_without_applying_deltas();
        };
        for _ in 0..3 {
            render(&mut state);
        }
        let window_id = egui::Id::new(CLIPBOARD_MODIFY_WINDOW_ID);
        let original_rect = ctx
            .memory(|memory| memory.area_rect(window_id))
            .expect("production Clipboard Modify window is rendered");
        assert!((original_rect.width() - size.width).abs() < 2.0);
        assert!((original_rect.height() - size.height).abs() < 2.0);

        state.open = false;
        state.cleanup_after_close();
        assert_eq!(
            crate::gui::utility_dialog_size::UtilityDialogSizeManager::for_context(&ctx)
                .preference(key),
            crate::settings::UtilitySizePreference::Preferred(size),
            "cleanup must retain the preference in the installed app capability"
        );
        assert!(state.source.is_empty());
        assert!(!state.wrap_preview);

        // The surface is omitted while closed, then reopens from the same
        // context-owned size preference.
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |root| manager.begin_frame(root.ctx()).unwrap(),
        );
        output.drop_without_applying_deltas();
        state.open = true;
        for _ in 0..3 {
            render(&mut state);
        }
        let reopened_rect = ctx
            .memory(|memory| memory.area_rect(window_id))
            .expect("production Clipboard Modify window reopens");
        assert!((reopened_rect.width() - size.width).abs() < 2.0);
        assert!((reopened_rect.height() - size.height).abs() < 2.0);
        workers.join_all();

        assert!(service.backend().writes().is_empty());
    }

    #[test]
    fn cleanup_after_close_clears_source_buffers() {
        let mut state = ClipboardModifyDialogState::default();
        state.source = "edited clipboard source".into();
        state.baseline = "loaded clipboard source".into();
        state.reset_source = "reset clipboard source".into();
        state.source_fingerprint = fingerprint(&state.source);

        state.cleanup_after_close();

        assert!(state.source.is_empty());
        assert!(state.baseline.is_empty());
        assert!(state.reset_source.is_empty());
        assert_eq!(state.source_fingerprint, 0);
    }

    #[test]
    fn cleanup_after_close_clears_preview_state() {
        let mut state = ClipboardModifyDialogState::default();
        state.source = "preview source".into();
        state.preview.request(
            state.source.clone(),
            ClipboardModifyIntent::Stages(vec![StageSpec {
                operation: OperationId::Uppercase,
                arguments: StageArguments::default(),
            }]),
            Arc::new(cat()),
        );

        state.cleanup_after_close();

        assert!(matches!(state.preview.state(), PreviewState::IdleMissing));
        assert!(!state.can_apply());
    }

    #[test]
    fn result_preview_summary_reports_truncation_without_full_text() {
        assert_eq!(
            result_preview_summary(12, 10, 2, true),
            "Result: 12 bytes, 10 chars, 2 lines (visible preview truncated)"
        );
        assert_eq!(
            result_preview_summary(12, 10, 2, false),
            "Result: 12 bytes, 10 chars, 2 lines"
        );
    }

    #[test]
    fn current_source_preview_keeps_per_pipeline_card_cap() {
        let eighty = "a".repeat(80);
        assert_eq!(capped_current_source_preview(&eighty), eighty);

        let long = format!("{}b", "a".repeat(80));
        assert_eq!(
            capped_current_source_preview(&long),
            format!("{}…", "a".repeat(80))
        );
    }

    #[test]
    fn external_clipboard_warning_keeps_overwrite_context() {
        let warning = external_clipboard_change_warning(&ClipboardSummary {
            bytes: 7,
            chars: 5,
            fingerprint: 0xabc,
        });
        assert!(warning.contains("Clipboard changed"));
        assert!(warning.contains("7 bytes"));
        assert!(warning.contains("5 chars"));
        assert!(warning.contains("0000000000000abc"));
        assert!(warning.contains("undo restores"));
    }

    #[test]
    fn cleanup_after_close_clears_clipboard_bearing_runtime_state() {
        let mut state = ClipboardModifyDialogState::default();
        state.source = "source".into();
        state.baseline = "baseline".into();
        state.reset_source = "reset".into();
        state.source_fingerprint = 42;
        state.source_error = Some("error".into());
        state.stages.push(DialogStage {
            id: 1,
            spec: StageSpec {
                operation: OperationId::Uppercase,
                arguments: StageArguments::default(),
            },
            error: Some("stage error".into()),
        });
        state.preview.request(
            "source".into(),
            ClipboardModifyIntent::Stages(vec![]),
            Arc::new(cat()),
        );
        state.acknowledged_large_sources.insert((123, 456));
        state.external_change = Some(ClipboardSummary {
            bytes: 1,
            chars: 1,
            fingerprint: 99,
        });
        state.pending_action = Some(PendingDialogAction::CopyResult);
        state.large_confirmation_open = true;

        state.cleanup_after_close();

        assert!(state.source.is_empty());
        assert!(state.baseline.is_empty());
        assert!(state.reset_source.is_empty());
        assert_eq!(state.source_fingerprint, 0);
        assert!(state.source_error.is_none());
        assert!(state.stages.is_empty());
        assert!(matches!(state.preview.state(), PreviewState::IdleMissing));
        assert!(state.acknowledged_large_sources.is_empty());
        assert!(state.external_change.is_none());
        assert!(state.pending_action.is_none());
        assert!(!state.large_confirmation_open);
    }

    fn cat() -> ClipboardModifierCatalog {
        ClipboardModifierCatalog::new(
            vec![
                tmpl(
                    "prompt-context",
                    "Prompt Context",
                    &["pc"],
                    "Context {{clipboard}}",
                ),
                tmpl("quote", "Quote", &["blockquote"], "> {{clipboard}}"),
            ],
            vec![],
        )
        .unwrap()
    }
    #[test]
    fn targeted_opens_select_section() {
        let mut d = ClipboardModifyDialogState::default();
        d.open_section_without_loading_for_test(ClipboardModifyDialogSection::Templates);
        d.open_section_without_loading_for_test(ClipboardModifyDialogSection::SavedPipelines);
        assert_eq!(d.section, ClipboardModifyDialogSection::SavedPipelines);
    }
    #[test]
    fn close_cleanup_clears_transient_data() {
        let mut d = ClipboardModifyDialogState::default();
        d.open = true;
        d.source = "x".into();
        d.stages.push(DialogStage {
            id: 1,
            spec: StageSpec {
                operation: OperationId::Uppercase,
                arguments: Default::default(),
            },
            error: None,
        });
        d.wrap_preview = false;
        d.cleanup_after_close();
        assert!(d.source.is_empty() && d.stages.is_empty());
        assert!(!d.wrap_preview);
    }
    #[test]
    fn source_loading_accepts_empty_text() {
        let mut d = ClipboardModifyDialogState::default();
        d.replace_loaded_source(String::new());
        assert_eq!(d.baseline, "");
    }
    #[test]
    fn non_text_source_reports_error() {
        let e = ClipboardError::NonText;
        assert_eq!(e, ClipboardError::NonText);
    }
    #[test]
    fn large_input_blocks_preview_until_acknowledged() {
        let mut d = ClipboardModifyDialogState::default();
        d.replace_loaded_source("a".repeat(LARGE_SOURCE_CONFIRM_BYTES + 1));
        assert!(d.requires_large_ack());
    }
    #[test]
    fn stale_running_error_previews_disable_apply_buttons() {
        let d = ClipboardModifyDialogState::default();
        assert!(!d.can_apply());
    }
    #[test]
    fn cleanup_after_close_clears_content_bearing_fields() {
        let mut dlg = ClipboardModifyDialogState::default();
        dlg.open = true;
        dlg.source = "secret source".into();
        dlg.baseline = "secret baseline".into();
        dlg.reset_source = "secret reset".into();
        dlg.source_fingerprint = 99;
        dlg.source_error = Some("secret error".into());
        dlg.external_change = Some(ClipboardSummary {
            fingerprint: 1,
            bytes: 2,
            chars: 3,
        });
        dlg.pending_action = Some(PendingDialogAction::Apply);
        dlg.wrap_preview = false;
        dlg.cleanup_after_close();
        assert!(dlg.source.is_empty());
        assert!(dlg.baseline.is_empty());
        assert!(dlg.reset_source.is_empty());
        assert_eq!(dlg.source_fingerprint, 0);
        assert!(dlg.source_error.is_none());
        assert!(dlg.external_change.is_none());
        assert!(dlg.pending_action.is_none());
        assert!(!dlg.wrap_preview);
        assert!(matches!(dlg.preview.state(), PreviewState::IdleMissing));
    }

    #[test]
    fn ctrl_z_not_global_clipboard_modify_undo() {
        assert_ne!("Ctrl+Z", "cm undo");
    }

    #[test]
    fn template_filtering_matches_id_label_alias() {
        let catalog = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_filter = "prompt".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "prompt-context");
        d.template_filter = "quote".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "quote");
        d.template_filter = "block".into();
        assert_eq!(d.filtered_templates(&catalog)[0].id, "quote");
    }

    #[test]
    fn selecting_template_does_not_dirty_configuration() {
        let catalog = Arc::new(cat());
        let mut d = ClipboardModifyDialogState::default();
        d.source = "x".into();
        d.selected_template = Some("quote".into());
        d.preview_template("quote".into(), catalog);
        assert!(!d.unsaved_template_draft);
    }

    #[test]
    fn applying_template_uses_dialog_preview_commit_path() {
        let catalog = Arc::new(cat());
        let mut d = ClipboardModifyDialogState::default();
        d.source = "x".into();
        d.apply_template_through_commit_path("quote", catalog);
        assert!(matches!(
            d.preview.state(),
            PreviewState::PendingDebounce { .. } | PreviewState::Running { .. }
        ));
    }

    #[test]
    fn invalid_drafts_disable_save_and_messages_are_specific() {
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("x", "X", &[], "missing")];
        assert!(!d.can_save_template_draft(&base));
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("{{clipboard}}")
        );
        d.template_draft = vec![
            tmpl("x", "X", &[], "{{clipboard}}"),
            tmpl("x", "Y", &[], "{{clipboard}}"),
        ];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("duplicate")
        );
        d.template_draft = vec![tmpl("template", "X", &[], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("reserved")
        );
        d.template_draft = vec![tmpl("x", "", &[], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("label")
        );
        d.template_draft = vec![tmpl("x", "X", &["!!!"], "{{clipboard}}")];
        assert!(
            d.validate_template_draft(&base)
                .unwrap_err()
                .contains("valid alias")
        );
        d.template_draft = vec![tmpl(
            "x",
            "X",
            &[],
            &format!(
                "{{{{clipboard}}}}{}",
                "a".repeat(crate::clipboard_modify::config::MAX_CONFIG_BYTES as usize)
            ),
        )];
        let model = crate::clipboard_modify::config::VersionedClipboardModifiersFile {
            schema_version: crate::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
            templates: d.template_draft.clone(),
            pipelines: vec![],
        };
        assert!(
            crate::clipboard_modify::config::serialize_model(&model)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
    }

    #[test]
    fn successful_save_persists_and_replaces_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(cat()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("saved", "Saved", &[], "{{clipboard}}")];
        let saved = d.save_template_draft(&base, &store).unwrap();
        assert_eq!(saved.templates[0].id, "saved");
        assert!(
            std::fs::read_to_string(&store.path)
                .unwrap()
                .contains("saved")
        );
        assert_eq!(store.catalog.read().unwrap().templates[0].id, "saved");
    }

    #[test]
    fn failed_save_preserves_active_catalog_and_draft() {
        let dir = tempfile::tempdir().unwrap();
        let store = ClipboardModifierStore {
            path: dir.path().to_path_buf(),
            catalog: Arc::new(RwLock::new(Arc::new(cat()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let base = cat();
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = vec![tmpl("draft", "Draft", &[], "{{clipboard}}")];
        assert!(d.save_template_draft(&base, &store).is_err());
        assert_eq!(
            store.catalog.read().unwrap().templates[0].id,
            "prompt-context"
        );
        assert_eq!(d.template_draft[0].id, "draft");
    }

    #[test]
    fn deletion_blocked_when_pipeline_references_template() {
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "pipe".into(),
            label: "Pipe".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::Template,
                arguments: StageArguments {
                    name: Some("quote".into()),
                    ..Default::default()
                },
            }],
        }];
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = base.templates.clone();
        let err = d.delete_template_from_draft(&base, "quote").unwrap_err();
        assert!(err.contains("Pipe (pipe)"));
    }

    #[test]
    fn reorder_saves_immediately_and_refreshes_suggestions_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let base = cat();
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut d = ClipboardModifyDialogState::default();
        d.template_draft = base.templates.clone();
        d.template_draft.swap(0, 1);
        d.save_template_draft(&base, &store).unwrap();
        assert_eq!(store.catalog.read().unwrap().templates[0].id, "quote");
    }
    #[test]
    fn saved_pipeline_list_displays_identity_and_stage_summary() {
        let p = SavedPipeline {
            id: "clean".into(),
            label: "Clean Text".into(),
            aliases: vec!["ct".into()],
            stages: vec![
                StageSpec {
                    operation: OperationId::Trim,
                    arguments: Default::default(),
                },
                StageSpec {
                    operation: OperationId::Uppercase,
                    arguments: Default::default(),
                },
            ],
        };
        assert!(
            ClipboardModifyDialogState::pipeline_identity_line(&p)
                .contains("Clean Text (clean) aliases: ct")
        );
        assert_eq!(
            ClipboardModifyDialogState::pipeline_stage_summary(&p),
            "trim → uppercase"
        );
    }

    #[test]
    fn executing_saved_pipeline_uses_current_source_and_commit_preview_path() {
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "up".into(),
            label: "Up".into(),
            aliases: vec![],
            stages: vec![StageSpec {
                operation: OperationId::Uppercase,
                arguments: Default::default(),
            }],
        }];
        let catalog = Arc::new(base);
        let mut d = ClipboardModifyDialogState::default();
        d.source = "current dialog source".into();
        d.apply_pipeline_through_commit_path("up", catalog);
        assert!(matches!(
            d.preview.state(),
            PreviewState::PendingDebounce { .. } | PreviewState::Running { .. }
        ));
    }

    #[test]
    fn duplicate_pipeline_id_is_nonconflicting_and_requires_confirmation() {
        let mut base = cat();
        base.pipelines = vec![
            SavedPipeline {
                id: "pipe".into(),
                label: "Pipe".into(),
                aliases: vec![],
                stages: vec![],
            },
            SavedPipeline {
                id: "pipe-copy".into(),
                label: "Pipe Copy".into(),
                aliases: vec![],
                stages: vec![],
            },
        ];
        assert_eq!(
            ClipboardModifyDialogState::duplicate_pipeline_id(&base, "pipe"),
            "pipe-copy-2"
        );
        let mut d = ClipboardModifyDialogState::default();
        d.duplicate_save_confirmation = Some("pipe-copy-2".into());
        assert!(d.duplicate_save_confirmation.is_some());
    }

    #[test]
    fn editor_stage_choices_are_deterministic_and_nested_pipelines_rejected() {
        let ops = ClipboardModifyDialogState::allowed_stage_operations();
        assert!(ops.contains(&OperationId::Template));
        assert!(ops.contains(&OperationId::CustomWrap));
        let mut base = cat();
        base.pipelines = vec![SavedPipeline {
            id: "pipe".into(),
            label: "Pipe".into(),
            aliases: vec![],
            stages: vec![],
        }];
        let nested = vec![StageSpec {
            operation: OperationId::Template,
            arguments: StageArguments {
                name: Some("pipe".into()),
                ..Default::default()
            },
        }];
        assert!(
            crate::clipboard_modify::pipeline::validate_executable_stages(&nested, &base).is_err()
        );
    }

    #[test]
    fn preview_can_be_cancelled() {
        let mut d = ClipboardModifyDialogState::default();
        d.cancel_pipeline_preview();
        assert!(!d.can_apply());
    }

    #[test]
    fn pipeline_deletion_and_reorder_save_immediately_and_preserve_drafts_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut base = cat();
        base.pipelines = vec![
            SavedPipeline {
                id: "one".into(),
                label: "One".into(),
                aliases: vec![],
                stages: vec![],
            },
            SavedPipeline {
                id: "two".into(),
                label: "Two".into(),
                aliases: vec![],
                stages: vec![],
            },
        ];
        let store = ClipboardModifierStore {
            path: dir.path().join("cm.json"),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut d = ClipboardModifyDialogState::default();
        d.pipeline_draft = base.pipelines.clone();
        d.delete_pipeline_from_draft_and_save(&base, &store, "one")
            .unwrap();
        assert_eq!(store.catalog.read().unwrap().pipelines[0].id, "two");
        let active = (*store.catalog.read().unwrap()).as_ref().clone();
        d.reorder_pipeline_draft_and_save(&active, &store, 0, 0)
            .unwrap();
        assert_eq!(store.catalog.read().unwrap().pipelines[0].id, "two");

        let bad_store = ClipboardModifierStore {
            path: dir.path().to_path_buf(),
            catalog: Arc::new(RwLock::new(Arc::new(base.clone()))),
            diagnostic: Arc::new(RwLock::new(None)),
        };
        let mut failed = ClipboardModifyDialogState::default();
        failed.pipeline_draft = base.pipelines.clone();
        assert!(
            failed
                .reorder_pipeline_draft_and_save(&base, &bad_store, 0, 1)
                .is_err()
        );
        assert_eq!(bad_store.catalog.read().unwrap().pipelines[0].id, "one");
        assert_eq!(failed.pipeline_draft[0].id, "two");
    }
}
