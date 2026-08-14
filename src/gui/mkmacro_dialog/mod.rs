mod action_editor;
pub mod image_search_editor;
mod macro_list;
pub mod recorder_controller;
mod step_table;
mod toolbar;
pub mod uia_editor;

use crate::mkmacro::{
    DiagnosticSeverity, MkMacroDocument, MkMacroStore, repair_ids, validate_document,
};
use std::sync::Arc;
pub use step_table::{Selection, duplicate_steps, move_steps};

pub struct MkMacroDialog {
    pub open: bool,
    pub store: Arc<MkMacroStore>,
    pub draft: MkMacroDocument,
    baseline: Arc<MkMacroDocument>,
    pub dirty: bool,
    pub conflict: bool,
    pub selected_macro: Option<u64>,
    pub selection: Selection,
    pub search: String,
    pub uia_editor: uia_editor::UiaEditorState,
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
            selected_macro: None,
            selection: Default::default(),
            search: String::new(),
            uia_editor: Default::default(),
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
                toolbar::show(ui, self);
                ui.columns(2, |cols| {
                    macro_list::show(&mut cols[0], self);
                    step_table::show(&mut cols[1], self);
                });
                if !self.uia_editor.editor_hidden() {
                    action_editor::show(ui, self);
                }
                uia_editor::show(ui, &mut self.uia_editor);
            });
        self.open = open;
    }
}
