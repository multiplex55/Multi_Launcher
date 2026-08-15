pub mod action_catalog;
pub mod action_editor;
pub mod image_search_editor;
mod key_capture;
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
    use crate::mkmacro::{
        MkAction, MkCoordinateTarget, MkHotkey, MkImagePayload, MkKey, MkMouseButton,
        MkMousePayload, MkStep, MkWaitOptions,
    };
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

    #[test]
    fn visible_catalog_metadata_is_routable_and_supported() {
        for descriptor in action_catalog::visible_descriptors() {
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert_eq!(
                descriptor.runtime,
                action_catalog::RuntimeAvailability::Supported
            );
            assert!(!descriptor.name.trim().is_empty());
            let action = (descriptor.make_default)();
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
            ));
        }
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
            action_catalog::format_coordinate_target(&MkCoordinateTarget::Variable {
                name: "cursor".into()
            }),
            "Variable <cursor>"
        );
        assert_eq!(
            action_catalog::format_coordinate_target(&MkCoordinateTarget::Image {
                asset_id: 9,
                offset: MkPoint { x: -3, y: 5 }
            }),
            "Image asset 9 offset (-3, 5)"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::MouseMove(screen.clone())),
            "Screen (824, 446)"
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
            asset_id: 42,
            confidence: 0.85,
            wait: MkWaitOptions {
                timeout_ms: 2500,
                poll_interval_ms: 100,
            },
        };
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageFind(image.clone())),
            "Asset 42 · 85% confidence · screen · 2500 ms timeout"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageClick(image)),
            "Asset 42 · 85% confidence · screen · 2500 ms timeout"
        );
    }

    #[test]
    fn coordinate_details_are_payload_only_and_non_mutating() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        for i in 0..500 {
            action_catalog::insert_action(
                &mut dialog,
                MkAction::MouseMove(MkCoordinateTarget::Screen {
                    point: crate::mkmacro::variables::MkPoint {
                        x: i - 250,
                        y: 250 - i,
                    },
                }),
            );
        }
        let before = serde_json::to_vec(&dialog.draft).unwrap();
        for step in &dialog.selected_macro().unwrap().steps {
            let MkAction::MouseMove(MkCoordinateTarget::Screen { point }) = &step.action else {
                panic!("unexpected action")
            };
            assert_eq!(
                action_catalog::action_details(&step.action),
                format!("Screen ({}, {})", point.x, point.y)
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
        d.action_catalog_visible = true;
        d.action_search = "key".into();

        assert!(action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Key Press")
        ));
        let original = d.action_editor.draft.as_ref().unwrap().action.clone();
        assert!(d.action_catalog_visible);
        assert!(!action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Text")
        ));
        assert_eq!(d.action_editor.draft.as_ref().unwrap().action, original);

        let mut editor = std::mem::take(&mut d.action_editor);
        assert!(editor.apply(&mut d).is_some());
        d.action_editor = editor;
        assert_eq!(d.selected_macro().unwrap().steps.len(), 1);
        assert!(d.action_catalog_visible);
        assert_eq!(d.action_search, "key");
    }

    #[test]
    fn cancelling_catalog_editor_preserves_macro_and_catalog() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.action_catalog_visible = true;
        d.action_search = "mouse".into();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("Mouse Click"));
        d.action_editor.cancel();
        assert!(d.selected_macro().unwrap().steps.is_empty());
        assert!(d.action_catalog_visible);
        assert_eq!(d.action_search, "mouse");
    }

    #[test]
    fn direct_catalog_actions_insert_and_keep_catalog_open() {
        for name in [
            "Else",
            "End If",
            "End Repeat",
            "End While",
            "Break",
            "Continue",
        ] {
            let (_dir, mut d) = dialog();
            d.create_macro();
            d.action_catalog_visible = true;
            d.action_search = "structure".into();
            assert!(action_catalog::select_descriptor(
                &mut d,
                &catalog_descriptor(name)
            ));
            assert_eq!(d.selected_macro().unwrap().steps.len(), 1, "{name}");
            assert_eq!(d.selection.ids.len(), 1, "{name}");
            assert!(d.dirty, "{name}");
            assert!(d.action_catalog_visible, "{name}");
            assert_eq!(d.action_search, "structure", "{name}");
        }
    }

    #[test]
    fn catalog_close_paths_and_parent_close_reset_children() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.action_catalog_visible = true;
        action_catalog::close(&mut d); // Explicit Close button transition.
        assert!(!d.action_catalog_visible);
        d.action_catalog_visible = true;
        action_catalog::close(&mut d); // Simulated window-X transition.
        assert!(!d.action_catalog_visible);

        d.action_catalog_visible = true;
        d.action_editor
            .begin_new(MkAction::Delay { milliseconds: 1 });
        d.open = false;
        d.close_children();
        assert!(!d.action_catalog_visible);
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
                id: 0,
                enabled: true,
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
            self.close_children();
            return;
        }
        let mut open = self.open;
        eframe::egui::Window::new("Mouse/Keyboard Macros")
            .id(eframe::egui::Id::new("mkmacro_main_window"))
            .open(&mut open)
            .default_size([920.0, 620.0])
            .min_size([680.0, 420.0])
            .resizable(true)
            .show(ctx, |ui| {
                self.show_contents(ui);
            });
        self.open = open;
        if !self.open {
            self.close_children();
        }
    }
    fn close_children(&mut self) {
        self.action_catalog_visible = false;
        self.action_editor.cancel();
        self.structural_insertion = None;
    }
    pub fn show_contents(&mut self, ui: &mut eframe::egui::Ui) {
        toolbar::show(ui, self);
        if self.draft.macros.is_empty() {
            let body_size = ui.available_size();
            ui.allocate_ui_with_layout(
                body_size,
                eframe::egui::Layout::top_down(eframe::egui::Align::Center),
                |ui| macro_list::show_empty(ui, self),
            );
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
        if self.delete_confirmation.ui(ui.ctx()) == ConfirmationResult::Confirmed {
            self.delete_selected_macro();
        }
    }
}
