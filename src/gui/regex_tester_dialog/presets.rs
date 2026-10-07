//! Explicit preset authoring; the domain store owns validation and atomic writes.

use crate::regex_tester::{PresetContent, PresetId, PresetInput, PresetStore, RegexDraft};
use eframe::egui;

pub(super) struct PresetState {
    store: PresetStore,
    selected: Option<PresetId>,
    name: String,
    include_sample: bool,
    include_replacement: bool,
    delete_confirmation: Option<PresetId>,
    feedback: Option<Result<&'static str, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_update_preserves_external_rename_in_current_disk_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut state = PresetState::open(&path);
        state.name = "Original name".into();
        let mut draft = RegexDraft {
            pattern: "a".into(),
            ..Default::default()
        };
        state.handle(PresetAction::Save, &mut draft);
        let id = state.selected.unwrap();
        PresetStore::open(&path)
            .rename(id, "External renamed preset")
            .unwrap();
        assert_eq!(state.store.get(id).unwrap().name, "Original name");
        state.name = "Unsaved rename form".into();
        state.include_sample = true;
        draft.pattern = "é".into();
        draft.test_text = "sample é".into();
        let original = draft.clone();
        assert!(!state.handle(PresetAction::Update(id), &mut draft));
        assert_eq!(draft, original);
        let saved = state.store.get(id).unwrap();
        assert_eq!(saved.name, "External renamed preset");
        assert_eq!(saved.pattern, "é");
        assert_eq!(saved.sample_text.as_deref(), Some("sample é"));
        assert_eq!(saved.id, id);
        assert_eq!(PresetStore::open(&path).get(id), Some(saved));
    }

    #[test]
    fn explicit_optional_buffers_save_load_and_update_preserve_name_and_stable_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut state = PresetState::open(&path);
        assert!(!state.include_sample && !state.include_replacement);
        state.name = "  Work in progress  ".into();
        let mut draft = RegexDraft {
            pattern: "[".into(),
            test_text: "private sample".into(),
            replacement: "private replacement".into(),
            ..Default::default()
        };
        let original = draft.clone();
        assert!(!state.handle(PresetAction::Save, &mut draft));
        assert_eq!(draft, original);
        let id = state.selected.unwrap();
        let saved = state.store.get(id).unwrap();
        assert_eq!(saved.name, "Work in progress");
        assert_eq!(saved.sample_text, None);
        assert_eq!(saved.replacement, None);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("private"));
        let mut restarted = PresetState::open(&path);
        draft.pattern = "other".into();
        assert!(restarted.handle(PresetAction::Load(id), &mut draft));
        assert_eq!(draft, original);
        restarted.select(id);
        restarted.name = "Do not silently rename".into();
        restarted.include_sample = true;
        restarted.include_replacement = true;
        draft.pattern = "(?P<letter>é)".into();
        draft.flags.case_insensitive = true;
        draft.test_text.clear();
        draft.replacement.clear();
        let updated = draft.clone();
        assert!(!restarted.handle(PresetAction::Update(id), &mut draft));
        assert_eq!(draft, updated);
        let saved = restarted.store.get(id).unwrap();
        assert_eq!(saved.id, id);
        assert_eq!(saved.name, "Work in progress");
        assert_eq!(saved.sample_text.as_deref(), Some(""));
        assert_eq!(saved.replacement.as_deref(), Some(""));
        draft.test_text = "should clear".into();
        draft.replacement = "should clear".into();
        assert!(restarted.handle(PresetAction::Load(id), &mut draft));
        assert_eq!(draft, updated);
        assert_eq!(PresetStore::open(&path).get(id), restarted.store.get(id));
    }

    #[test]
    fn named_management_feedback_and_delete_confirmation_are_tied_to_selection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut state = PresetState::open(&path);
        let mut draft = RegexDraft {
            pattern: "a".into(),
            ..Default::default()
        };
        let original = draft.clone();
        state.name = "First".into();
        state.handle(PresetAction::Save, &mut draft);
        let first = state.selected.unwrap();
        state.name = "first".into();
        state.handle(PresetAction::Save, &mut draft);
        assert!(state.feedback.as_ref().unwrap().is_err());
        assert_eq!(state.store.entries().len(), 1);
        state.name = "Renamed".into();
        state.handle(PresetAction::Rename(first), &mut draft);
        assert_eq!(state.store.get(first).unwrap().name, "Renamed");
        assert_eq!(state.store.get(first).unwrap().pattern, "a");
        state.name = "Second".into();
        state.handle(PresetAction::Save, &mut draft);
        let second = state.selected.unwrap();
        state.select(first);
        state.handle(PresetAction::RequestDelete(first), &mut draft);
        state.handle(PresetAction::CancelDelete, &mut draft);
        assert!(state.delete_confirmation.is_none());
        state.handle(PresetAction::RequestDelete(first), &mut draft);
        state.select(second);
        assert!(state.delete_confirmation.is_none());
        state.handle(PresetAction::ConfirmDelete(first), &mut draft);
        assert!(state.store.get(first).is_some());
        assert!(state.feedback.as_ref().unwrap().is_err());
        state.handle(PresetAction::RequestDelete(second), &mut draft);
        state.handle(PresetAction::ConfirmDelete(second), &mut draft);
        assert!(state.store.get(second).is_none());
        state.select(first);
        state.handle(PresetAction::RequestDelete(first), &mut draft);
        PresetStore::open(&path).delete(first).unwrap();
        state.handle(PresetAction::Reload, &mut draft);
        assert!(state.selected.is_none());
        assert!(state.delete_confirmation.is_none());
        assert_eq!(draft, original);
    }

    #[test]
    fn corrupt_presets_keep_last_snapshot_and_bytes_until_explicit_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("presets.json");
        let mut state = PresetState::open(&path);
        let mut draft = RegexDraft::default();
        state.name = "Existing".into();
        state.handle(PresetAction::Save, &mut draft);
        let id = state.selected.unwrap();
        let valid = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"corrupt presets").unwrap();
        state.name = "New".into();
        state.handle(PresetAction::Save, &mut draft);
        assert!(state.feedback.as_ref().unwrap().is_err());
        assert!(!state.store.is_writable());
        assert!(state.store.get(id).is_some());
        assert_eq!(std::fs::read(&path).unwrap(), b"corrupt presets");
        std::fs::write(&path, &valid).unwrap();
        state.handle(PresetAction::Save, &mut draft);
        assert_eq!(std::fs::read(&path).unwrap(), valid);
        state.handle(PresetAction::Reload, &mut draft);
        state.handle(PresetAction::Save, &mut draft);
        assert_eq!(state.store.entries().len(), 2);
        assert!(state.feedback.as_ref().unwrap().is_ok());
    }
}

#[derive(Clone, Copy)]
pub(super) enum PresetAction {
    Save,
    Load(PresetId),
    Rename(PresetId),
    Update(PresetId),
    RequestDelete(PresetId),
    ConfirmDelete(PresetId),
    CancelDelete,
    Reload,
}

impl PresetState {
    pub fn open(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: PresetStore::open(path),
            selected: None,
            name: String::new(),
            include_sample: false,
            include_replacement: false,
            delete_confirmation: None,
            feedback: None,
        }
    }

    fn select(&mut self, id: PresetId) {
        if self.selected != Some(id) {
            self.delete_confirmation = None;
            if let Some(preset) = self.store.get(id) {
                self.selected = Some(id);
                self.name.clone_from(&preset.name);
            }
        }
    }

    fn input(&self, name: String, draft: &RegexDraft) -> PresetInput {
        PresetInput {
            name,
            pattern: draft.pattern.clone(),
            flags: draft.flags,
            sample_text: self.include_sample.then(|| draft.test_text.clone()),
            replacement: self.include_replacement.then(|| draft.replacement.clone()),
        }
    }

    /// Returns true only for an explicit successful Load. Management never
    /// edits the document or schedules evaluation.
    pub fn handle(&mut self, action: PresetAction, draft: &mut RegexDraft) -> bool {
        let result = match action {
            PresetAction::Save => {
                let input = self.input(self.name.clone(), draft);
                self.store.create(input).map(|id| {
                    self.select(id);
                    "Saved preset"
                })
            }
            PresetAction::Load(id) => {
                if let Some(preset) = self.store.get(id) {
                    draft.pattern.clone_from(&preset.pattern);
                    draft.flags = preset.flags;
                    if let Some(text) = &preset.sample_text {
                        draft.test_text.clone_from(text);
                    }
                    if let Some(replacement) = &preset.replacement {
                        draft.replacement.clone_from(replacement);
                    }
                    self.select(id);
                    self.feedback = Some(Ok("Loaded preset"));
                    return true;
                }
                self.feedback = Some(Err(
                    "Selected preset is no longer available; reload presets.".into(),
                ));
                return false;
            }
            PresetAction::Rename(id) => {
                self.store.rename(id, &self.name).map(|()| "Renamed preset")
            }
            PresetAction::Update(id) => {
                let content = PresetContent {
                    pattern: draft.pattern.clone(),
                    flags: draft.flags,
                    sample_text: self.include_sample.then(|| draft.test_text.clone()),
                    replacement: self.include_replacement.then(|| draft.replacement.clone()),
                };
                self.store
                    .update_content(id, content)
                    .map(|()| "Updated preset")
            }
            PresetAction::RequestDelete(id) => {
                if self.selected == Some(id) && self.store.get(id).is_some() {
                    self.delete_confirmation = Some(id);
                }
                return false;
            }
            PresetAction::ConfirmDelete(id) => {
                if self.delete_confirmation != Some(id) || self.selected != Some(id) {
                    self.feedback = Some(Err(
                        "Delete target changed; select the preset and confirm again.".into(),
                    ));
                    return false;
                }
                self.delete_confirmation = None;
                self.store.delete(id).map(|()| {
                    self.selected = None;
                    "Deleted preset"
                })
            }
            PresetAction::CancelDelete => {
                self.delete_confirmation = None;
                return false;
            }
            PresetAction::Reload => {
                self.delete_confirmation = None;
                self.store.reload().map(|()| {
                    if self.selected.is_some_and(|id| self.store.get(id).is_none()) {
                        self.selected = None;
                    }
                    "Reloaded presets"
                })
            }
        };
        self.feedback = Some(result.map_err(|error| error.to_string()));
        false
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<PresetAction> {
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_source("regex_presets_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap = Some(true);
                if let Some(diagnostic) = self.store.diagnostic() {
                    ui.colored_label(ui.visuals().error_fg_color, &diagnostic.message);
                }
                if !self.store.is_writable() {
                    ui.weak("Preset writes are blocked. Existing data is preserved; repair the file and reload.");
                }
                if let Some(feedback) = &self.feedback {
                    match feedback {
                        Ok(message) => { ui.weak(*message); }
                        Err(message) => { ui.colored_label(ui.visuals().error_fg_color, message); }
                    }
                }
                if ui.button("Reload presets").clicked() { action = Some(PresetAction::Reload); }
                let mut selected = None;
                for preset in self.store.entries() {
                    ui.push_id(("regex_preset", preset.id.value()), |ui| {
                        if ui.selectable_label(self.selected == Some(preset.id), &preset.name).clicked() {
                            selected = Some(preset.id);
                        }
                    });
                }
                if let Some(id) = selected { self.select(id); }
                if self.store.entries().is_empty() { ui.weak("No saved presets."); }
                ui.separator();
                ui.label("Name for Save / Rename:");
                ui.add(egui::TextEdit::singleline(&mut self.name).id_source("regex_preset_name").desired_width(ui.available_width()));
                ui.checkbox(&mut self.include_sample, "Include sample text");
                ui.checkbox(&mut self.include_replacement, "Include replacement");
                ui.weak("Save and Update store the current pattern and flags. Only checked buffers are stored; unchecked buffers are omitted.");
                if ui.add_enabled(self.store.is_writable(), egui::Button::new("Save new preset")).clicked() {
                    action = Some(PresetAction::Save);
                }
                if let Some(id) = self.selected {
                    if let Some(preset) = self.store.get(id) {
                        ui.separator();
                        ui.strong(format!("Selected: {}", preset.name));
                        ui.label(egui::RichText::new(super::utf8_prefix(&preset.pattern, 160)).monospace());
                        ui.weak(format!("Flags {} · ID {}", preset.flags.suffix(), id.value()));
                        ui.weak("Load restores pattern and flags. Stored buffers replace current values, including empty values; omitted buffers preserve current values.");
                        ui.weak(format!("Sample text: {} · Replacement: {}", if preset.sample_text.is_some() { "stored" } else { "omitted" }, if preset.replacement.is_some() { "stored" } else { "omitted" }));
                        if ui.button("Load selected preset").clicked() { action = Some(PresetAction::Load(id)); }
                        ui.add_enabled_ui(self.store.is_writable(), |ui| {
                            if ui.button("Rename selected preset").clicked() { action = Some(PresetAction::Rename(id)); }
                            ui.weak(format!("Update replaces {:?} with the current pattern/flags and checked buffers, keeping its saved name.", preset.name));
                            if ui.button("Update selected preset").clicked() { action = Some(PresetAction::Update(id)); }
                            if ui.button("Delete selected preset…").clicked() { action = Some(PresetAction::RequestDelete(id)); }
                        });
                        if self.delete_confirmation == Some(id) {
                            ui.colored_label(ui.visuals().error_fg_color, format!("Delete {:?} (ID {})?", preset.name, id.value()));
                            ui.horizontal_wrapped(|ui| {
                                if ui.add_enabled(self.store.is_writable(), egui::Button::new("Confirm delete")).clicked() { action = Some(PresetAction::ConfirmDelete(id)); }
                                if ui.button("Cancel").clicked() { action = Some(PresetAction::CancelDelete); }
                            });
                        }
                    }
                }
            });
        action
    }
}
