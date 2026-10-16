//! Explicit preset authoring; the domain store owns validation and atomic writes.

use crate::regex_tester::{
    PresetContent, PresetId, PresetInput, PresetStore, RegexDraft, RegexTestCaseExpectation,
    RegexTestCases,
};
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
    fn configured_data_root_writes_private_profile_stores_and_reconstructs_dialog() {
        use crate::gui::regex_tester_dialog::RegexTesterDialogState;
        use crate::platform::app_data::AppDataRoot;
        use crate::regex_tester::session::EVALUATION_DEBOUNCE;
        use std::time::Instant;

        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("profiles").join("work");
        std::fs::create_dir_all(&profile).unwrap();
        let root = AppDataRoot::from_settings_path(profile.join("settings.json")).unwrap();
        assert_eq!(root.path(), profile);
        let history_path = profile.join("regex_history.json");
        let preset_path = profile.join("regex_presets.json");
        let mut dialog = RegexTesterDialogState::with_data_root(&root);
        assert!(!history_path.exists() && !preset_path.exists());
        dialog.session.draft = RegexDraft {
            pattern: "(?P<letter>é)".into(),
            test_text: "private sample É".into(),
            replacement: "private replacement ${letter}".into(),
            ..Default::default()
        };
        dialog.session.draft.flags.case_insensitive = true;
        dialog
            .session
            .draft
            .test_cases
            .add(
                Some("étiquette".into()),
                "private case input".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let original = dialog.session.draft.clone();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        assert!(dialog.session.tick(now + EVALUATION_DEBOUNCE));
        dialog.record_history();
        let presets = dialog.presets.as_mut().unwrap();
        presets.name = "Profile expression".into();
        assert!(!presets.include_sample && !presets.include_replacement);
        assert!(!presets.handle(PresetAction::Save, &mut dialog.session.draft));
        assert!(presets.feedback.as_ref().unwrap().is_ok());
        let id = presets.selected.unwrap();
        assert_eq!(dialog.session.draft, original);
        for path in [&history_path, &preset_path] {
            let saved = std::fs::read_to_string(path).unwrap();
            assert!(!saved.contains("private sample"));
            assert!(!saved.contains("private replacement"));
        }
        assert!(!directory.path().join("regex_history.json").exists());
        assert!(!directory.path().join("regex_presets.json").exists());
        drop(dialog);

        let restarted = RegexTesterDialogState::with_data_root(&root);
        let history = &restarted.history.as_ref().unwrap().store;
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].pattern, original.pattern);
        assert_eq!(history.entries()[0].flags, original.flags);
        let presets = &restarted.presets.as_ref().unwrap().store;
        assert_eq!(presets.entries().len(), 1);
        let saved = presets.get(id).unwrap();
        assert_eq!(saved.name, "Profile expression");
        assert_eq!(saved.pattern, original.pattern);
        assert_eq!(saved.flags, original.flags);
        assert_eq!(saved.sample_text, None);
        assert_eq!(saved.replacement, None);
        assert_eq!(saved.test_cases, original.test_cases);
    }

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
        draft
            .test_cases
            .add(
                Some("source".into()),
                "case source".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
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
        let bytes_before_case_edit = std::fs::read(&path).unwrap();
        let case_id = draft.test_cases.as_slice()[0].id;
        draft.test_cases.get_mut(case_id).unwrap().input = "case edited locally".into();
        assert_eq!(std::fs::read(&path).unwrap(), bytes_before_case_edit);
        let original = draft.clone();
        assert!(!state.handle(PresetAction::Update(id), &mut draft));
        assert_eq!(draft, original);
        let saved = state.store.get(id).unwrap();
        assert_eq!(saved.name, "External renamed preset");
        assert_eq!(saved.pattern, "é");
        assert_eq!(saved.sample_text.as_deref(), Some("sample é"));
        assert_eq!(saved.test_cases, draft.test_cases);
        assert_eq!(saved.id, id);
        assert_eq!(PresetStore::open(&path).get(id), Some(saved));
    }

    #[test]
    fn loading_same_pattern_and_flags_reschedules_changed_case_inputs() {
        use crate::gui::regex_tester_dialog::RegexTesterDialogState;
        use crate::regex_tester::RegexTestCaseStatus;
        use crate::regex_tester::session::EVALUATION_DEBOUNCE;
        use std::time::Instant;

        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("history.json");
        let preset_path = dir.path().join("presets.json");
        let mut first_cases = RegexTestCases::default();
        first_cases
            .add(None, "x".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        let mut second_cases = RegexTestCases::default();
        second_cases
            .add(None, "y".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        let mut store = PresetStore::open(&preset_path);
        let first = store
            .create(PresetInput {
                name: "First".into(),
                pattern: "x".into(),
                flags: Default::default(),
                sample_text: None,
                replacement: None,
                test_cases: first_cases.clone(),
            })
            .unwrap();
        let second = store
            .create(PresetInput {
                name: "Second".into(),
                pattern: "x".into(),
                flags: Default::default(),
                sample_text: None,
                replacement: None,
                test_cases: second_cases.clone(),
            })
            .unwrap();

        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        dialog.session.draft.pattern = "x".into();
        dialog.session.draft.test_cases = first_cases.clone();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        assert!(dialog.session.tick_test_cases(now + EVALUATION_DEBOUNCE));
        assert_eq!(
            dialog.session.test_case_evaluation().unwrap().results[0].status,
            RegexTestCaseStatus::Pass
        );

        dialog.preset_action(PresetAction::Load(second));
        assert_eq!(dialog.session.draft.pattern, "x");
        assert_eq!(dialog.session.draft.test_cases, second_cases);
        assert_eq!(
            dialog.session.test_case_evaluation().unwrap().results[0].status,
            RegexTestCaseStatus::Pending
        );
        let loaded_at = Instant::now();
        assert!(
            dialog
                .session
                .tick_test_cases(loaded_at + EVALUATION_DEBOUNCE)
        );
        assert_eq!(
            dialog.session.test_case_evaluation().unwrap().results[0].status,
            RegexTestCaseStatus::Fail
        );
        assert_eq!(store.get(first).unwrap().test_cases, first_cases);
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
        draft
            .test_cases
            .add(
                Some("expected".into()),
                "initial case".into(),
                RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        let original = draft.clone();
        assert!(!state.handle(PresetAction::Save, &mut draft));
        assert_eq!(draft, original);
        let id = state.selected.unwrap();
        let saved = state.store.get(id).unwrap();
        assert_eq!(saved.name, "Work in progress");
        assert_eq!(saved.sample_text, None);
        assert_eq!(saved.replacement, None);
        assert_eq!(saved.test_cases, original.test_cases);
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
        draft.test_cases = RegexTestCases::default();
        assert!(restarted.handle(PresetAction::Load(id), &mut draft));
        assert_eq!(draft, updated);
        assert_eq!(PresetStore::open(&path).get(id), restarted.store.get(id));

        let legacy_path = dir.path().join("legacy-presets.json");
        let legacy = br#"{"version":1,"next_id":2,"presets":[{"id":1,"name":"Legacy","pattern":"legacy","flags":{},"sample_text":null,"replacement":null}]}"#;
        std::fs::write(&legacy_path, legacy).unwrap();
        let mut legacy_state = PresetState::open(&legacy_path);
        draft.pattern = "current".into();
        draft.test_text = "keep sample".into();
        draft.replacement = "keep replacement".into();
        draft.test_cases = updated.test_cases.clone();
        let legacy_id = legacy_state.store.entries()[0].id;
        assert!(legacy_state.handle(PresetAction::Load(legacy_id), &mut draft));
        assert_eq!(draft.pattern, "legacy");
        assert_eq!(draft.test_text, "keep sample");
        assert_eq!(draft.replacement, "keep replacement");
        assert!(draft.test_cases.is_empty());
        assert_eq!(std::fs::read(&legacy_path).unwrap(), legacy);
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
            test_cases: draft.test_cases.clone(),
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
                    // Cases belong to a preset independently of the optional
                    // main example and replacement buffers. Loading a legacy
                    // preset therefore intentionally replaces cases with an
                    // empty collection.
                    draft.test_cases.clone_from(&preset.test_cases);
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
                    test_cases: draft.test_cases.clone(),
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
            .id_salt("regex_presets_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
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
                ui.add(egui::TextEdit::singleline(&mut self.name).id_salt("regex_preset_name").desired_width(ui.available_width()));
                ui.checkbox(&mut self.include_sample, "Include sample text");
                ui.checkbox(&mut self.include_replacement, "Include replacement");
                ui.weak("Save and Update store the current pattern and flags. Only checked buffers are stored; unchecked buffers are omitted.");
                if ui.add_enabled(self.store.is_writable(), egui::Button::new("Save new preset")).clicked() {
                    action = Some(PresetAction::Save);
                }
                if let Some(id) = self.selected
                    && let Some(preset) = self.store.get(id)
                {
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
            });
        action
    }
}
