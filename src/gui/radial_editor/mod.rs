//! Stable-ID radial menu authoring window.

mod asset_picker;
mod audio_controls;
mod import_export;
mod preview;
mod skin_editor;

use crate::gui::LauncherApp;
use crate::radial::authoring::menu::{self, ResizeResolution, SubmenuDuplication};
use crate::radial::authoring::{
    AuthoringClient, AuthoringError, AuthoringSnapshot, CloseDecision, CommitDisposition,
    ConflictResolution, DiskSha256, EditKey, EditPhase, RadialAuthoringSession, StableSelection,
};
use crate::radial::context::InvocationContext;
use crate::radial::model::{
    AfterActionPolicy, CellContent, CellId, Control, DynamicSource, InteractionMode, LayoutKind,
    MenuId, RadialDocument, RingId, SubmenuPresentation,
};
use eframe::egui;
use import_export::PendingImport;
use preview::{EmbeddedPreview, PreviewPreset};

#[derive(Clone, Debug)]
enum EditorCommand {
    MoveCell {
        source_menu: MenuId,
        source_ring: RingId,
        cell: CellId,
        destination_menu: MenuId,
        destination_ring: RingId,
        index: usize,
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

pub(crate) struct RadialEditorState {
    pub(crate) open: bool,
    session: Option<RadialAuthoringSession>,
    client: Option<AuthoringClient>,
    preview: EmbeddedPreview,
    close_prompt: bool,
    delete_message: Option<String>,
    delete_ring_prompt: Option<(MenuId, RingId)>,
    resize_prompt: Option<menu::ResizePlan>,
    action_filter: String,
    drag_source: Option<(MenuId, RingId, CellId)>,
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
}

impl Default for RadialEditorState {
    fn default() -> Self {
        Self {
            open: false,
            session: None,
            client: None,
            preview: EmbeddedPreview::default(),
            close_prompt: false,
            delete_message: None,
            delete_ring_prompt: None,
            resize_prompt: None,
            action_filter: String::new(),
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
        }
    }
}

impl RadialEditorState {
    pub(crate) fn open(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.close_prompt = false;
        self.drag_source = None;
        self.post_render.clear();
        self.focus_restore = None;
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
                let _ = client.send(request);
            }
        }
        self.session = Some(session);
    }

    pub(crate) fn open_skins(&mut self) {
        self.open();
        self.show_resources = true;
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
        self.client = None;
        self.session = Some(RadialAuthoringSession::new(AuthoringSnapshot::new(
            std::sync::Arc::new(document),
            "test",
        )));
    }

    pub(crate) fn request_close(&mut self) {
        let Some(session) = self.session.as_ref() else {
            self.open = false;
            return;
        };
        match session.close_decision() {
            CloseDecision::CloseClean => {
                self.stop_native_preview();
                self.release_authoring_resources();
                self.open = false;
            }
            CloseDecision::PromptDirty => self.close_prompt = true,
            CloseDecision::AwaitingRequest => {}
        }
    }

    pub(crate) fn force_close(&mut self) {
        self.stop_native_preview();
        self.release_authoring_resources();
        self.open = false;
        self.session = None;
        self.close_prompt = false;
        self.focus_restore = None;
    }

    fn send_commit(&mut self, disposition: CommitDisposition) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let request = session.request_commit(disposition);
        match (request, &self.client) {
            (Ok(request), Some(client)) => {
                if let Err(error) = client.send(request) {
                    session.last_error = Some(format!("{error:?}"));
                }
            }
            (Err(error), _) => session.last_error = Some(format!("{error:?}")),
            (_, None) => session.last_error = Some("Radial authoring service unavailable".into()),
        }
    }

    fn poll_replies(&mut self) {
        let Some(client) = &self.client else { return };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        while let Some(reply) = client.try_recv() {
            session.accept_reply(reply);
        }
        if !session.font_catalog_loaded && session.pending_request.is_none() {
            match session.request_font_catalog() {
                Ok(request) => {
                    if let Err(error) = client.send(request) {
                        session.last_error = Some(format!("{error:?}"));
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
                Err(error) => self.resource_notice = Some(ResourceNotice::error(error.to_string())),
            }
        }
        if session.is_closed() {
            self.release_authoring_resources();
            self.open = false;
        }
    }

    fn release_authoring_resources(&mut self) {
        if let (Some(client), Some(session)) = (&self.client, &self.session) {
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
        match (request, &self.client) {
            (Ok(request), Some(client)) => {
                if let Err(error) = client.send(request) {
                    session.last_error = Some(format!("{error:?}"));
                }
            }
            (Err(error), _) => session.last_error = Some(format!("{error:?}")),
            (_, None) => session.last_error = Some("Radial authoring service unavailable".into()),
        }
    }

    fn stop_native_preview(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let Ok(request) = session.request_stop_native_preview() else {
            return;
        };
        if let Some(client) = &self.client {
            let _ = client.send(request);
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

    pub(crate) fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.poll_replies();
        self.sync_native_preview_generation();
        if !self.open {
            return;
        }
        let selection_before = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        let mut window_open = true;
        egui::Window::new("Radial Menu Editor")
            .id(egui::Id::new("radial-menu-editor"))
            .open(&mut window_open)
            .default_size(egui::vec2(1100.0, 720.0))
            .show(ctx, |ui| {
                self.toolbar(ui);
                ui.separator();
                let Some(session) = self.session.as_mut() else {
                    ui.spinner();
                    return;
                };
                if let Some(conflict_reason) = session
                    .conflict
                    .as_ref()
                    .map(|conflict| conflict.reason.clone())
                {
                    ui.group(|ui| {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("Conflict: {conflict_reason}"),
                        );
                        ui.horizontal(|ui| {
                            for (label, resolution) in [
                                ("Reload", ConflictResolution::Reload),
                                ("Discard local", ConflictResolution::DiscardDraft),
                                ("Rebase", ConflictResolution::Rebase),
                            ] {
                                if ui.button(label).clicked() {
                                    let _ = session.resolve_conflict(resolution);
                                }
                            }
                        });
                    });
                }
                let preview_selection = session.selection.clone();
                self.preview.sync_preparation(
                    session,
                    self.client.as_ref(),
                    self.preview_preset,
                    preview_selection.as_ref(),
                );
                let prepared_preview = self.preview.prepared_frame(session);
                let draft = session.draft.clone();
                let generation = session.generation.0;
                let initial_snapshot_pending = session.is_initial_snapshot_pending();
                let feature_defaults = app.radial_feature_settings.clone();
                if initial_snapshot_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading the authoritative radial configuration…");
                    });
                }
                ui.add_enabled_ui(!initial_snapshot_pending, |ui| {
                    self.preview_controls(ui);
                    ui.columns(3, |columns: &mut [egui::Ui]| {
                        self.tree(&mut columns[0], &feature_defaults);
                        self.preview.ui(
                            &mut columns[1],
                            &draft,
                            generation,
                            self.preview_zoom,
                            self.preview_preset,
                            preview_selection.as_ref(),
                            prepared_preview.as_deref(),
                        );
                        self.inspector(&mut columns[2], app);
                    });
                    if self.show_resources {
                        ui.separator();
                        self.resources_ui(ui);
                    }
                });
            });
        if !self
            .session
            .as_ref()
            .is_some_and(RadialAuthoringSession::is_initial_snapshot_pending)
        {
            self.apply_post_render();
        }
        if !window_open {
            self.request_close();
        }
        self.prompts(ctx);
        self.keyboard_shortcuts(ctx);
        let selection_after = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone());
        if selection_after != selection_before {
            self.focus_restore = selection_after;
        }
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

    fn preview_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
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
            ui.add(
                egui::Slider::new(&mut self.preview_zoom, 0.5..=2.0)
                    .text("Zoom")
                    .logarithmic(true),
            );
            if ui
                .selectable_label(self.show_resources, "Skins, assets & packages")
                .clicked()
            {
                self.show_resources = !self.show_resources;
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
                for diagnostic in &session.native_preview_diagnostics {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Desktop preview warning: {diagnostic}"),
                    );
                }
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
                                                    if ui.button(label).clicked()
                                                        && let Some(path) = rfd::FileDialog::new().pick_file()
                                                    {
                                                        let selected = asset_picker::read_bounded(
                                                            &path,
                                                            crate::radial::assets::MAX_SOURCE_BYTES,
                                                        )
                                                            .and_then(|bytes| {
                                                                asset_picker::ResourceChoice::from_selected_file(
                                                                    &path, bytes, media_kind, managed,
                                                                )
                                                            });
                                                        match selected {
                                                            Ok(choice) => {
                                                                self.resource_notice = Some(if managed {
                                                                    ResourceNotice::info(
                                                                        choice.portability_diagnostic(),
                                                                    )
                                                                } else {
                                                                    ResourceNotice::warning(
                                                                        choice.portability_diagnostic(),
                                                                    )
                                                                });
                                                                if let Err(error) = skin_editor::set_media_override(
                                                                    session,
                                                                    &scope,
                                                                    &row.section,
                                                                    &row.field,
                                                                    &choice,
                                                                ) {
                                                                    self.resource_notice = Some(ResourceNotice::error(error));
                                                                }
                                                            }
                                                            Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                                                        }
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
        document_rule_controls(ui, session);
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
                if ui.button(label).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter(label, extensions)
                        .pick_file()
                {
                    match asset_picker::read_bounded(&path, crate::radial::assets::MAX_SOURCE_BYTES)
                        .and_then(|bytes| {
                            asset_picker::ResourceChoice::from_selected_file(
                                &path, bytes, kind, true,
                            )
                        })
                        .and_then(|choice| {
                            asset_picker::add_resource(session, &choice).map(|_| choice)
                        }) {
                        Ok(choice) => {
                            self.resource_notice =
                                Some(ResourceNotice::info(choice.portability_diagnostic()))
                        }
                        Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                    }
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
                                if let Some(client) = &self.client {
                                    if let Err(error) = client.send(request) {
                                        session.last_error = Some(format!("{error:?}"));
                                    }
                                } else {
                                    session.last_error =
                                        Some("Radial authoring service unavailable".into());
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
            if ui.button("Preview .mlradial import").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Multi Launcher radial", &["mlradial"])
                    .pick_file()
            {
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
            if ui.button("Export selected menu").clicked() {
                if let Some(menu_id) = selected_menu_id(session) {
                    if session.is_dirty() {
                        self.resource_notice = Some(ResourceNotice::warning(
                            "Save or apply the draft before exporting persisted package bytes",
                        ));
                    } else if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Multi Launcher radial", &["mlradial"])
                        .set_file_name("menu.mlradial")
                        .save_file()
                    {
                        match session.request_export_package(vec![menu_id]) {
                            Ok(request) => match &self.client {
                                Some(client) => match client.send(request) {
                                    Ok(()) => self.export_destination = Some(path),
                                    Err(error) => session.last_error = Some(format!("{error:?}")),
                                },
                                None => {
                                    session.last_error =
                                        Some("Radial authoring service unavailable".into())
                                }
                            },
                            Err(error) => session.last_error = Some(format!("{error:?}")),
                        }
                    }
                }
            }
            if ui.button("Export full package").clicked() {
                let roots = session
                    .draft
                    .menus
                    .iter()
                    .map(|menu| menu.id.clone())
                    .collect::<Vec<_>>();
                match begin_package_export(
                    self.client.as_ref(),
                    session,
                    roots,
                    "all-radial-menus.mlradial",
                ) {
                    Ok(path) => self.export_destination = path,
                    Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                }
            }
            if ui.button("Export selected skin").clicked()
                && let Some(StableSelection::Skin(skin_id)) = session.selection.clone()
            {
                match begin_skin_export(self.client.as_ref(), session, skin_id, "skin.mlradial") {
                    Ok(path) => self.export_destination = path,
                    Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                }
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
                if ui.button(label).clicked()
                    && let Some(paths) = rfd::FileDialog::new().pick_files()
                {
                    match preview_legacy_files(&paths, source, session) {
                        Ok(preview) => self.pending_import = Some(preview),
                        Err(error) => self.resource_notice = Some(ResourceNotice::error(error)),
                    }
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
                        self.replace_backup_path = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .set_file_name("radial-before-import.json")
                            .save_file();
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
                        Ok(request) => match &self.client {
                            Some(client) => {
                                if let Err(error) = client.send(request) {
                                    session.last_error = Some(format!("{error:?}"));
                                } else {
                                    self.pending_import = None;
                                    self.replace_confirmed = false;
                                    self.replace_backup_path = None;
                                }
                            }
                            None => {
                                session.last_error =
                                    Some("Radial authoring service unavailable".into())
                            }
                        },
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
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let menus = session.draft.menus.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for menu in &menus {
                let menu_selected =
                    session.selection == Some(StableSelection::Menu(menu.id.clone()));
                let header = egui::CollapsingHeader::new(&menu.name)
                    .id_source(menu::widget_key("menu", menu.id.as_str(), "tree"))
                    .default_open(true)
                    .show(ui, |ui| {
                        for ring in &menu.rings {
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
                                            format!("Ring {}", ring.id),
                                        )
                                    },
                                )
                                .inner;
                            if focus_restore.as_ref() == Some(&ring_selection) {
                                ring_response.request_focus();
                                *focus_restore = None;
                            }
                            if ring_response.clicked() {
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
                                                    let response =
                                                        ui.selectable_label(selected, &cell.label);
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
                                            session.select(Some(selection));
                                        }
                                        if response.drag_started() {
                                            *drag_source = Some((
                                                menu.id.clone(),
                                                ring.id.clone(),
                                                cell.id.clone(),
                                            ));
                                        }
                                        if response.hovered()
                                            && ui.input(|input| input.pointer.any_released())
                                        {
                                            if let Some((source_menu, source_ring, cell)) =
                                                drag_source.take()
                                            {
                                                post_render.push(EditorCommand::MoveCell {
                                                    source_menu,
                                                    source_ring,
                                                    cell,
                                                    destination_menu: menu.id.clone(),
                                                    destination_ring: ring.id.clone(),
                                                    index,
                                                });
                                            }
                                        }
                                    }
                                },
                            );
                        }
                    });
                if focus_restore.as_ref() == Some(&StableSelection::Menu(menu.id.clone())) {
                    header.header_response.request_focus();
                    *focus_restore = None;
                }
                if header.header_response.clicked() || menu_selected && session.selection.is_none()
                {
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

    fn inspector(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        ui.heading("Inspector");
        let Some(selection) = self
            .session
            .as_ref()
            .and_then(|session| session.selection.clone())
        else {
            ui.label("Select a menu, ring, or cell.");
            return;
        };
        match selection {
            StableSelection::Menu(menu_id) => self.menu_inspector(
                ui,
                menu_id,
                app.radial_feature_settings.default_menu_id.as_ref(),
            ),
            StableSelection::Ring { menu_id, ring_id } => self.ring_inspector(ui, menu_id, ring_id),
            StableSelection::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => self.cell_inspector(ui, app, menu_id, ring_id, cell_id),
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
            let continuous = menu_behavior_controls(ui, &mut edited);
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
        app: &mut LauncherApp,
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
            let content_edit = cell_content_controls(ui, &session.draft, &mut edited);
            let media_edit = cell_media_controls(ui, &session.draft, &mut edited);
            let trigger_edit = cell_trigger_controls(ui, session, &mut edited);
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
                    *slot = edited;
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
        let catalog =
            app.universal_action_authoring_catalog(&invocation_context, &self.action_filter);
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
                                    let _ = menu::set_cell_content(
                                        session,
                                        &menu_id,
                                        &ring_id,
                                        &cell_id,
                                        CellContent::Action { binding },
                                    );
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
                                    let _ = app.test_radial_authoring_action(
                                        &binding,
                                        &invocation_context,
                                        &self.action_filter,
                                    );
                                }
                                if row.destructive {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        "Destructive action",
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
                } => {
                    if menu::move_cell(
                        session,
                        (&source_menu, &source_ring, &cell),
                        (&destination_menu, &destination_ring, index),
                    )
                    .is_ok()
                    {
                        self.focus_restore = session.selection.clone();
                    }
                }
            }
        }
    }

    fn cancel(&mut self) {
        self.stop_native_preview();
        let Some(session) = self.session.as_mut() else {
            self.force_close();
            return;
        };
        let result = session.request_commit(CommitDisposition::RevertAppliedAndClose);
        match result {
            Ok(request) => {
                if let Some(client) = &self.client {
                    let _ = client.send(request);
                }
            }
            Err(AuthoringError::NothingToRevert) => {
                self.open = false;
                self.session = None;
                self.close_prompt = false;
            }
            Err(error) => session.last_error = Some(format!("{error:?}")),
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
                            self.send_commit(CommitDisposition::Save);
                            self.close_prompt = false;
                        }
                        if ui.button("Discard").clicked() {
                            self.cancel();
                            self.close_prompt = false;
                        }
                        if ui.button("Keep editing").clicked() {
                            self.close_prompt = false;
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

fn menu_behavior_controls(
    ui: &mut egui::Ui,
    menu: &mut crate::radial::model::MenuDefinition,
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
        "Submenu presentation",
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
    ui.collapsing("Center mappings", |ui| {
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
        if menu.center_secondary_action.is_some() && ui.button("Clear secondary action").clicked() {
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
    });
    ui.collapsing("Background mappings", |ui| {
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
    });
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
    ui.collapsing("Alternate clicks and controls", |ui| {
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
    });
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
) -> Option<ContinuousWidgetEdit> {
    use crate::radial::model::{MediaKind, MediaReference, Override};
    let mut continuous = None;
    ui.collapsing("Icon and tooltip", |ui| {
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
            if ui.button("External file").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                cell.icon = Override::Value(MediaReference::ExternalFile {
                    path: path.display().to_string(),
                });
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
                let index_response = ui.add(egui::DragValue::new(index).prefix("Resource index "));
                set_first_edit(
                    &mut continuous,
                    continuous_widget_edit(&index_response, "icon.resource_index"),
                );
            }
            _ => {}
        }
    });
    continuous
}

fn cell_trigger_controls(
    ui: &mut egui::Ui,
    session: &mut RadialAuthoringSession,
    cell: &mut crate::radial::model::CellDefinition,
) -> Option<ContinuousWidgetEdit> {
    let mut continuous = None;
    ui.collapsing("Shortcuts and hotstrings", |ui| {
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
    });
    continuous
}

fn document_rule_controls(ui: &mut egui::Ui, session: &mut RadialAuthoringSession) {
    let mut document = (*session.draft).clone();
    let original = document.clone();
    let mut continuous: Option<(String, ContinuousWidgetEdit)> = None;
    let menu_choices: Vec<_> = document
        .menus
        .iter()
        .map(|menu| (menu.id.clone(), menu.name.clone()))
        .collect();
    ui.collapsing("Context rules", |ui| {
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
    });
    ui.collapsing("Custom triggers", |ui| {
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
    });
    ui.collapsing("Media search paths", |ui| {
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
    });
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

fn begin_package_export(
    client: Option<&AuthoringClient>,
    session: &mut RadialAuthoringSession,
    roots: Vec<MenuId>,
    file_name: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    if session.is_dirty() {
        return Err("Save or apply the draft before exporting persisted package bytes".into());
    }
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Multi Launcher radial", &["mlradial"])
        .set_file_name(file_name)
        .save_file()
    else {
        return Ok(None);
    };
    let request = session
        .request_export_package(roots)
        .map_err(|error| format!("{error:?}"))?;
    client
        .ok_or("Radial authoring service unavailable")?
        .send(request)
        .map_err(|error| format!("{error:?}"))?;
    Ok(Some(path))
}

fn begin_skin_export(
    client: Option<&AuthoringClient>,
    session: &mut RadialAuthoringSession,
    skin_id: crate::radial::model::SkinId,
    file_name: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    if session.is_dirty() {
        return Err("Save or apply the draft before exporting persisted package bytes".into());
    }
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Multi Launcher radial", &["mlradial"])
        .set_file_name(file_name)
        .save_file()
    else {
        return Ok(None);
    };
    let request = session
        .request_export_skin(skin_id)
        .map_err(|error| format!("{error:?}"))?;
    client
        .ok_or("Radial authoring service unavailable")?
        .send(request)
        .map_err(|error| format!("{error:?}"))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_render_move_uses_stable_ids_and_selection_survives_reorder() {
        let document = RadialDocument::starter();
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
        editor.post_render.push(EditorCommand::MoveCell {
            source_menu: menu_id.clone(),
            source_ring: ring_id.clone(),
            cell: cell_id.clone(),
            destination_menu: menu_id.clone(),
            destination_ring: ring_id.clone(),
            index: 0,
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
