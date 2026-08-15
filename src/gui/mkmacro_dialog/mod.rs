pub mod action_catalog;
pub mod action_editor;
pub mod image_search_editor;
mod macro_list;
mod macro_properties;
pub mod recorder_controller;
mod step_table;
mod toolbar;
pub mod uia_editor;

use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::mkmacro::{
    DiagnosticSeverity, MkMacro, MkMacroDocument, MkMacroStore, NormalizationConfig, RecordedStep,
    repair_ids, validate_document,
};
use std::sync::Arc;
pub use step_table::{Selection, duplicate_steps, duplicate_steps_with_ids, move_steps};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyDecision {
    KeepEditing,
    Discard,
}

pub struct MkMacroDialog {
    pub open: bool,
    pub store: Arc<MkMacroStore>,
    pub draft: MkMacroDocument,
    baseline: Arc<MkMacroDocument>,
    pub dirty: bool,
    pub conflict: bool,
    pub selected_macro_id: Option<u64>,
    pub selection: Selection,
    pub search: String,
    pub delete_confirmation: ConfirmationModal,
    pub hotkey_capture: bool,
    pub action_catalog_visible: bool,
    pub action_search: String,
    pub structural_insertion: Option<action_catalog::StructuralInsertion>,
    pub uia_editor: uia_editor::UiaEditorState,
    pub action_editor: action_editor::ActionEditorState,
    pub quick_insert: action_editor::QuickInsertState,
    pub command_error: Option<String>,
    /// Editable options are copied into the runtime at Start and never mutate an active session.
    pub recorder_options: NormalizationConfig,
    /// Kept when the target was deleted so the user can restore it without losing captured data.
    pub pending_recording: Option<(u64, Vec<RecordedStep>)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkHotkey, MkKey, MkStep};
    fn dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        (dir, MkMacroDialog::new(Arc::new(store)))
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
            id: 0,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Delay { milliseconds: 1 },
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
    fn names_and_details_cover_catalog() {
        for x in action_catalog::descriptors() {
            let a = (x.make_default)();
            assert!(!action_catalog::action_name(&a).is_empty());
            let _ = action_catalog::action_details(&a);
        }
    }
}
impl MkMacroDialog {
    pub fn new(store: Arc<MkMacroStore>) -> Self {
        let baseline = store.snapshot();
        Self {
            open: false,
            draft: (*baseline).clone(),
            baseline,
            store,
            dirty: false,
            conflict: false,
            selected_macro_id: None,
            selection: Default::default(),
            search: String::new(),
            delete_confirmation: Default::default(),
            hotkey_capture: false,
            action_catalog_visible: false,
            action_search: String::new(),
            structural_insertion: None,
            uia_editor: Default::default(),
            action_editor: Default::default(),
            quick_insert: action_editor::QuickInsertState {
                repeat: 1,
                ..Default::default()
            },
            command_error: None,
            recorder_options: Default::default(),
            pending_recording: None,
        }
    }
    pub fn open(&mut self) {
        self.sync_external();
        self.open = true;
    }
    pub fn sync_external(&mut self) {
        let current = self.store.snapshot();
        if !Arc::ptr_eq(&current, &self.baseline) {
            if self.dirty {
                self.conflict = true;
            } else {
                self.draft = (*current).clone();
                self.baseline = current;
            }
        }
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        repair_ids(&mut self.draft);
        self.baseline = self.store.save(self.draft.clone())?;
        self.dirty = false;
        self.conflict = false;
        Ok(())
    }
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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
            self.baseline = current;
            self.dirty = false;
            self.conflict = false;
        }
        self.open = false;
        true
    }
    pub fn reload_with_decision(&mut self, decision: DirtyDecision) -> bool {
        if self.dirty && decision == DirtyDecision::KeepEditing {
            return false;
        }
        let current = self.store.snapshot();
        self.draft = (*current).clone();
        self.baseline = current;
        self.dirty = false;
        self.conflict = false;
        true
    }
    /// Applies normalized recorder output atomically. A missing target leaves the draft untouched.
    pub fn apply_recording(
        &mut self,
        macro_id: u64,
        recorded: &[RecordedStep],
    ) -> Result<Vec<u64>, String> {
        let next = self
            .draft
            .macros
            .iter()
            .flat_map(|m| &m.steps)
            .map(|s| s.id)
            .max()
            .unwrap_or(0);
        let inserted = crate::mkmacro::to_macro_steps(recorded, next);
        let ids = inserted.iter().map(|s| s.id).collect::<Vec<_>>();
        let m = self
            .draft
            .macros
            .iter_mut()
            .find(|m| m.id == macro_id)
            .ok_or("recording target no longer exists")?;
        m.steps.extend(inserted);
        repair_ids(&mut self.draft);
        self.selection.ids = ids.iter().copied().collect();
        self.mark_dirty();
        Ok(ids)
    }
    pub fn selected_macro(&self) -> Option<&MkMacro> {
        let id = self.selected_macro_id?;
        self.draft.macros.iter().find(|m| m.id == id)
    }
    pub fn selected_macro_mut(&mut self) -> Option<&mut MkMacro> {
        let id = self.selected_macro_id?;
        self.draft.macros.iter_mut().find(|m| m.id == id)
    }
    pub fn create_macro(&mut self) {
        self.draft.macros.push(MkMacro {
            id: 0,
            name: "New Macro".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps: vec![],
        });
        repair_ids(&mut self.draft);
        self.selected_macro_id = self.draft.macros.last().map(|m| m.id);
        self.selection.clear();
        self.mark_dirty();
    }
    pub fn duplicate_selected_macro(&mut self) {
        let Some(mut copy) = self.selected_macro().cloned() else {
            return;
        };
        copy.id = 0;
        copy.name.push_str(" Copy");
        copy.hotkey = None;
        for step in &mut copy.steps {
            step.id = 0;
        }
        self.draft.macros.push(copy);
        repair_ids(&mut self.draft);
        self.selected_macro_id = self.draft.macros.last().map(|m| m.id);
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
        self.draft.macros.remove(index);
        self.selected_macro_id = self
            .draft
            .macros
            .get(index)
            .or_else(|| index.checked_sub(1).and_then(|i| self.draft.macros.get(i)))
            .map(|m| m.id);
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
        if crate::mkmacro::runtime::snapshot().is_some_and(|s| {
            matches!(
                s.state,
                crate::mkmacro::RuntimeState::Running
                    | crate::mkmacro::RuntimeState::Paused
                    | crate::mkmacro::RuntimeState::Stopping
            )
        }) {
            return Some("Another playback operation is active".into());
        }
        let d = validate_document(&self.draft, None);
        d.iter()
            .find(|x| x.severity == DiagnosticSeverity::Fatal)
            .map(|x| x.message.clone())
    }

    fn prepare_execution(&mut self) -> anyhow::Result<u64> {
        if self.dirty {
            self.save()?;
        }
        self.selected_macro_id
            .ok_or_else(|| anyhow::anyhow!("Select a macro"))
    }
    pub fn run_selected_macro(&mut self) -> anyhow::Result<()> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let id = self.prepare_execution()?;
        crate::mkmacro::runtime::run(id)
    }
    pub fn run_from_step(&mut self, original_step_id: u64) -> anyhow::Result<()> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let index = self
            .selected_macro()
            .and_then(|m| m.steps.iter().position(|s| s.id == original_step_id))
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists"))?;
        let id = self.prepare_execution()?;
        let step = self
            .selected_macro()
            .and_then(|m| m.steps.get(index))
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists after save"))?
            .id;
        crate::mkmacro::runtime::run_from(id, step)
    }
    pub fn run_selected_steps(&mut self) -> anyhow::Result<()> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let positions: Vec<usize> = self
            .selected_macro()
            .map(|m| {
                m.steps
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| self.selection.ids.contains(&s.id).then_some(i))
                    .collect()
            })
            .unwrap_or_default();
        if positions.is_empty() {
            anyhow::bail!("Select one or more steps");
        }
        let id = self.prepare_execution()?;
        let ids = positions
            .into_iter()
            .filter_map(|i| {
                self.selected_macro()
                    .and_then(|m| m.steps.get(i))
                    .map(|s| s.id)
            })
            .collect();
        crate::mkmacro::runtime::run_selection(id, ids)
    }
    pub fn ui(&mut self, ctx: &eframe::egui::Context) {
        self.sync_external();
        if !self.open {
            return;
        }
        let mut open = self.open;
        eframe::egui::Window::new("Mouse/Keyboard Macros")
            .open(&mut open)
            .default_size([920.0, 620.0])
            .min_size([680.0, 420.0])
            .resizable(true)
            .show(ctx, |ui| {
                self.show_contents(ui);
            });
        self.open = open;
    }
    pub fn show_contents(&mut self, ui: &mut eframe::egui::Ui) {
        toolbar::show(ui, self);
        if self.draft.macros.is_empty() {
            macro_list::show_empty(ui, self);
        } else {
            egui_extras::StripBuilder::new(ui)
                .size(egui_extras::Size::exact(macro_list::SIDEBAR_WIDTH))
                .size(egui_extras::Size::remainder())
                .horizontal(|mut strip| {
                    strip.cell(|ui| macro_list::show(ui, self));
                    strip.cell(|ui| {
                        macro_properties::show(ui, self);
                        ui.separator();
                        step_table::show(ui, self);
                    });
                });
        }
        action_catalog::show_modal(ui.ctx(), self);
        action_editor::show(ui.ctx(), self);
        uia_editor::show(ui, &mut self.uia_editor);
        if self.delete_confirmation.ui(ui.ctx()) == ConfirmationResult::Confirmed {
            self.delete_selected_macro();
        }
    }
}
