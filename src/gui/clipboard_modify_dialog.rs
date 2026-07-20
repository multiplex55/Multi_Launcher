use crate::clipboard_modify::clipboard::{
    ClipboardError, ClipboardSummary, ProductionClipboardService,
};
use crate::clipboard_modify::coordinator::{PreviewCoordinator, PreviewState};
use crate::clipboard_modify::model::{OperationId, StageArguments, StageSpec};
use crate::clipboard_modify::parser::ClipboardModifyIntent;
use eframe::egui;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const LARGE_SOURCE_CONFIRM_BYTES: usize = 5 * 1024 * 1024;

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

#[derive(Debug, Clone)]
pub struct DialogStage {
    pub id: u64,
    pub spec: StageSpec,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingDialogAction {
    Apply,
    ApplyAndClose,
    CopyResult,
}

pub struct ClipboardModifyDialogState {
    pub open: bool,
    pub section: ClipboardModifyDialogSection,
    pub source: String,
    pub baseline: String,
    pub reset_source: String,
    pub source_fingerprint: u64,
    pub source_error: Option<String>,
    pub stages: Vec<DialogStage>,
    next_stage_id: u64,
    pub preview: PreviewCoordinator,
    pub acknowledged_large_sources: HashSet<(u64, usize)>,
    pub large_confirmation_open: bool,
    pub external_change: Option<ClipboardSummary>,
    pub pending_action: Option<PendingDialogAction>,
    pub unsaved_template_draft: bool,
    pub unsaved_pipeline_draft: bool,
    pub wrap_preview: bool,
    pub last_action: Option<String>,
}

impl Default for ClipboardModifyDialogState {
    fn default() -> Self {
        Self {
            open: false,
            section: Default::default(),
            source: String::new(),
            baseline: String::new(),
            reset_source: String::new(),
            source_fingerprint: 0,
            source_error: None,
            stages: Vec::new(),
            next_stage_id: 1,
            preview: PreviewCoordinator::default(),
            acknowledged_large_sources: HashSet::new(),
            large_confirmation_open: false,
            external_change: None,
            pending_action: None,
            unsaved_template_draft: false,
            unsaved_pipeline_draft: false,
            wrap_preview: true,
            last_action: None,
        }
    }
}

impl ClipboardModifyDialogState {
    pub fn open_section(
        &mut self,
        section: ClipboardModifyDialogSection,
        service: &Arc<ProductionClipboardService>,
    ) {
        let was_open = self.open;
        self.open = true;
        self.section = section;
        if !was_open {
            self.load_clipboard(service);
        }
    }
    pub fn open_section_without_loading_for_test(&mut self, section: ClipboardModifyDialogSection) {
        self.open = true;
        self.section = section;
    }
    pub fn load_clipboard(&mut self, service: &Arc<ProductionClipboardService>) {
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
        self.baseline = text.clone();
        self.reset_source = text.clone();
        self.source = text;
        self.source_fingerprint = fingerprint(&self.source);
        self.source_error = None;
        self.preview = PreviewCoordinator::default();
        self.large_confirmation_open = self.requires_large_ack();
    }
    pub fn cleanup_after_close(&mut self) {
        let wrap = self.wrap_preview;
        self.preview.cancel_active();
        self.source.clear();
        self.baseline.clear();
        self.reset_source.clear();
        self.source_fingerprint = 0;
        self.source_error = None;
        self.stages.clear();
        self.preview = PreviewCoordinator::default();
        self.external_change = None;
        self.pending_action = None;
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
        self.source_fingerprint = fingerprint(&self.source);
        if self.requires_large_ack() {
            self.preview = PreviewCoordinator::default();
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
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if !self.open {
            return;
        }
        self.preview.tick();
        let mut open = self.open;
        egui::Window::new("Clipboard Modify")
            .open(&mut open)
            .show(ctx, |ui| {
                self.shortcuts(ui, service, catalog.clone());
                ui.horizontal(|ui| {
                    for (s, l) in [
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
                    ] {
                        if ui.selectable_label(self.section == s, l).clicked() {
                            self.section = s;
                        }
                    }
                });
                ui.separator();
                match self.section {
                    ClipboardModifyDialogSection::Modify => {
                        self.modify_ui(ui, service, catalog.clone())
                    }
                    _ => {
                        ui.label(format!("{:?} section", self.section));
                    }
                }
            });
        if self.open && !open {
            self.open = false;
            self.cleanup_after_close();
        }
    }
    fn shortcuts(
        &mut self,
        ui: &egui::Ui,
        service: &Arc<ProductionClipboardService>,
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
                "reload" => {
                    if self.source == self.reset_source {
                        self.load_clipboard(service);
                        self.mark_dirty(catalog);
                    }
                }
                _ => {}
            }
        }
    }
    fn modify_ui(
        &mut self,
        ui: &mut egui::Ui,
        service: &Arc<ProductionClipboardService>,
        catalog: Arc<crate::clipboard_modify::model::ClipboardModifierCatalog>,
    ) {
        if let Some(e) = &self.source_error {
            ui.colored_label(egui::Color32::RED, e);
        }
        if self.large_confirmation_open {
            ui.label(format!(
                "Large clipboard source: {} bytes. Preview is paused.",
                self.source.len()
            ));
            if ui.button("Preview this source").clicked() {
                self.acknowledge_large_source(catalog.clone());
            }
            return;
        }
        if ui
            .add(
                egui::TextEdit::multiline(&mut self.source)
                    .id_source("cm_source")
                    .desired_rows(8),
            )
            .changed()
        {
            self.mark_dirty(catalog.clone());
        }
        ui.horizontal(|ui| {
            if ui.button("Add uppercase stage").clicked() {
                self.add_stage(OperationId::Uppercase, catalog.clone())
            }
            if ui.button("Reset").clicked() {
                self.reset(catalog.clone())
            }
            if ui.button("Reload Clipboard").clicked() {
                self.load_clipboard(service);
                self.mark_dirty(catalog.clone())
            }
        });
        let mut remove = None;
        let mut changed = false;
        for i in 0..self.stages.len() {
            ui.push_id(self.stages[i].id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Stage {}: {:?}",
                        i + 1,
                        self.stages[i].spec.operation
                    ));
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
        match self.preview.state() {
            PreviewState::Completed { display, .. } => {
                ui.label(format!(
                    "Result: {} bytes, {} chars, {} lines{}",
                    display.metadata.bytes,
                    display.metadata.chars,
                    display.metadata.lines,
                    if display.truncated {
                        " (visible preview truncated)"
                    } else {
                        ""
                    }
                ));
                ui.add(egui::TextEdit::multiline(&mut display.text.clone()).desired_rows(8));
            }
            PreviewState::Failed { error, .. } => {
                ui.colored_label(egui::Color32::RED, error);
            }
            s => {
                ui.label(format!("Preview: {:?}", s));
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.can_apply(), |ui| {
                if ui.button("Apply").clicked() {
                    self.commit(service, false, false)
                }
                if ui.button("Apply and Close").clicked() {
                    self.commit(service, true, false)
                }
                if ui.button("Copy Result").clicked() {
                    self.commit(service, false, true)
                }
            });
            if ui.button("Cancel").clicked() {
                self.open = false;
                self.cleanup_after_close();
            }
        });
        if let Some(cur) = &self.external_change {
            ui.separator();
            ui.label(format!("Clipboard changed since preview source was captured. Current clipboard: {} bytes, {} chars, fingerprint {:016x}. Applying overwrites it; undo restores the replaced value.", cur.bytes, cur.chars, cur.fingerprint));
            if ui.button("Overwrite clipboard").clicked() {
                let close = matches!(
                    self.pending_action,
                    Some(PendingDialogAction::ApplyAndClose)
                );
                self.commit(service, close, true);
                self.external_change = None;
            }
        }
    }
    pub fn commit(
        &mut self,
        service: &Arc<ProductionClipboardService>,
        close: bool,
        confirmed: bool,
    ) {
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

pub fn fingerprint(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn ctrl_z_not_global_clipboard_modify_undo() {
        assert_ne!("Ctrl+Z", "cm undo");
    }
}
