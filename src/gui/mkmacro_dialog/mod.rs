pub mod action_catalog;
mod action_editor;
pub mod image_search_editor;
mod macro_list;
mod macro_properties;
pub mod recorder_controller;
mod step_table;
mod toolbar;
pub mod uia_editor;

use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::mkmacro::{
    DiagnosticSeverity, MkMacro, MkMacroDocument, MkMacroStore, repair_ids, validate_document,
};
use std::sync::Arc;
pub use step_table::{Selection, duplicate_steps, duplicate_steps_with_ids, move_steps};

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
        let d = validate_document(&self.draft, None);
        d.iter()
            .find(|x| x.severity == DiagnosticSeverity::Fatal)
            .map(|x| x.message.clone())
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
