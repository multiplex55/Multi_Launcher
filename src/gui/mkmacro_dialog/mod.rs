pub mod action_catalog;
pub mod action_editor;
pub mod condition_editor;
pub mod image_asset_picker;
pub mod image_authoring_destination;
pub mod image_authoring_job;
pub mod image_preview;
pub mod image_search_controls;
pub mod image_search_editor;
mod key_capture;
pub mod launcher_action_picker;
mod macro_list;
mod macro_properties;
pub mod recorder_controller;
pub(crate) mod runtime_inspector;
mod step_table;
mod toolbar;
pub mod uia_editor;
pub mod variable_catalog;
pub mod visual_capture_workflow;
pub mod visual_overlay;
mod window_matcher_editor;
pub mod window_picker;

use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::mkmacro::{
    DiagnosticSeverity, MkHotkeyScope, MkMacro, MkMacroDocument, MkMacroFolder, MkMacroStore,
    MkWindowMatcher, NormalizationConfig, RecordedStep, repair_ids, validate_document,
};
use std::collections::HashSet;
use std::sync::Arc;
pub use step_table::{Selection, duplicate_steps, duplicate_steps_with_ids, move_steps};
use visual_capture_workflow::SharedVisualOverlayController;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyDecision {
    KeepEditing,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderNameError {
    Empty,
    Duplicate(String),
    MissingFolder(u64),
}

impl std::fmt::Display for FolderNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "A macro folder name cannot be empty."),
            Self::Duplicate(name) => write!(f, "A macro folder named \"{name}\" already exists."),
            Self::MissingFolder(_) => write!(f, "This macro folder no longer exists."),
        }
    }
}

impl std::error::Error for FolderNameError {}

/// Immutable launcher data made available to macro authoring.  This is a
/// snapshot: authoring a macro can never edit the launcher's action list.
#[derive(Clone, Default)]
pub struct MkMacroAuthoringContext {
    pub launcher_actions: Arc<Vec<crate::actions::Action>>,
}

pub struct MkMacroDialog {
    pub open: bool,
    pub store: Arc<MkMacroStore>,
    pub authoring_context: MkMacroAuthoringContext,
    /// Authoritative client keeping the dialog-wide native overlay service alive.
    pub(crate) visual_overlay: SharedVisualOverlayController,
    pub draft: MkMacroDocument,
    baseline: Arc<MkMacroDocument>,
    pub dirty: bool,
    pub conflict: bool,
    pub selected_macro_id: Option<u64>,
    pub selection: Selection,
    pub search: String,
    /// Process-local presentation state; never participates in draft dirty tracking.
    pub collapsed_folders: HashSet<u64>,
    pub pending_folder_rename: Option<u64>,
    pub folder_rename_text: String,
    folder_rename_needs_focus: bool,
    pub pending_delete_folder: Option<u64>,
    pub folder_delete_confirmation: ConfirmationModal,
    pub folder_error: Option<String>,
    pub delete_confirmation: ConfirmationModal,
    pub unwrap_confirmation: ConfirmationModal,
    pub pending_unwrap_block: Option<u64>,
    pub pending_unwrap_selection: Option<Selection>,
    pub hotkey_capture: bool,
    pub record_hotkey_capture: bool,
    pub action_catalog_visible: bool,
    pub action_search: String,
    pub structural_insertion: Option<action_catalog::StructuralInsertion>,
    pub uia_editor: uia_editor::UiaEditorState,
    pub action_editor: action_editor::ActionEditorState,
    pub window_picker: window_picker::WindowPickerState,
    pub launcher_action_picker: launcher_action_picker::LauncherActionPickerState,
    pub command_error: Option<String>,
    /// Editable options are copied into the runtime at Start and never mutate an active session.
    pub recorder_options: NormalizationConfig,
    /// Kept when the target was deleted so the user can restore it without losing captured data.
    pub pending_recording: Option<(u64, Vec<RecordedStep>)>,
    /// Process-local read-only runtime presentation state. None of these fields
    /// participate in draft dirty tracking, persistence, or save conflicts.
    pub runtime_inspector_open: bool,
    pub runtime_inspector_show_internal: bool,
    pub runtime_inspector_builtins_open: bool,
    pub runtime_inspector_snapshot: Option<Arc<crate::mkmacro::RuntimeSnapshot>>,
    pub runtime_inspector_is_current_debug_run: bool,
    runtime_inspector_observed_run: Option<(crate::mkmacro::RuntimeRunMode, u64)>,
    runtime_inspector_active_breakpoint: Option<(u64, u64)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkVirtualDesktopAction;
    use crate::mkmacro::{
        AlphaPolicy, ExecutionOptions, LoadDisposition, MKMACROS_FILE, MacroRuntime, MkAction,
        MkCondition, MkCoordinateTarget, MkDelayPayload, MkHotkey, MkImageNotFoundPolicy,
        MkImageOutputs, MkImagePayload, MkKey, MkMouseButton, MkMouseMovePayload, MkMousePayload,
        MkMouseScrollAxis, MkStep, MkValue, MkWaitOptions, ReturnPoint, RuntimeCommand,
        RuntimeState, SCHEMA_VERSION, SearchRegion,
    };
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    fn five_macros() -> MkMacroDocument {
        MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            folders: vec![],
            macros: (1..=5)
                .map(|id| MkMacro {
                    id,
                    name: format!("Macro {id}"),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: Default::default(),
                    steps: vec![],
                    image_assets: vec![],
                })
                .collect(),
        }
    }

    fn wait_for_empty_and_stable(dialog: &mut MkMacroDialog, path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stable_since = None;
        loop {
            let snapshot_count = dialog.store.snapshot().macros.len();
            let contents =
                fs::read_to_string(path).unwrap_or_else(|e| format!("<read error: {e}>"));
            let disk_count = serde_json::from_str::<MkMacroDocument>(&contents)
                .map(|d| d.macros.len())
                .unwrap_or(usize::MAX);
            if snapshot_count == 0 && disk_count == 0 {
                let since = stable_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(250) {
                    return;
                }
            } else {
                stable_since = None;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not stabilize on empty document: snapshot_count={snapshot_count}, disk_count={disk_count}, file={contents:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        (dir, MkMacroDialog::new(Arc::new(store)))
    }

    #[test]
    fn debug_execution_methods_match_normal_admission_rejections() {
        let (_dir, mut d) = dialog();
        assert_eq!(
            d.run_selected_macro().unwrap_err().to_string(),
            d.debug_selected_macro().unwrap_err().to_string()
        );

        d.create_macro();
        d.selected_macro_mut().unwrap().enabled = false;
        assert_eq!(
            d.run_selected_macro().unwrap_err().to_string(),
            d.debug_selected_macro().unwrap_err().to_string()
        );
        assert_eq!(
            d.run_from_step(1).unwrap_err().to_string(),
            d.debug_from_step(1).unwrap_err().to_string()
        );
        assert_eq!(
            d.run_selected_steps().unwrap_err().to_string(),
            d.debug_selected_steps().unwrap_err().to_string()
        );

        d.selected_macro_mut().unwrap().enabled = true;
        d.selected_macro_mut().unwrap().steps.push(MkStep {
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Delay(MkDelayPayload {
                fixed_ms: u64::MAX,
                ..Default::default()
            }),
        });
        d.selection.ids = std::collections::BTreeSet::from([1]);
        for (normal, debug) in [
            (
                d.run_selected_macro().unwrap_err().to_string(),
                d.debug_selected_macro().unwrap_err().to_string(),
            ),
            (
                d.run_from_step(1).unwrap_err().to_string(),
                d.debug_from_step(1).unwrap_err().to_string(),
            ),
            (
                d.run_selected_steps().unwrap_err().to_string(),
                d.debug_selected_steps().unwrap_err().to_string(),
            ),
        ] {
            assert_eq!(normal, debug);
        }
    }

    #[test]
    fn shared_execution_preparation_saves_and_remaps_targets_by_position() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.selected_macro_mut().unwrap().steps = vec![
            MkStep {
                id: 11,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
            MkStep {
                id: 22,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
            MkStep {
                id: 33,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Delay(MkDelayPayload::default()),
            },
        ];
        d.selection.ids = std::collections::BTreeSet::from([11, 33]);
        let positions = d.selected_step_positions().unwrap();
        assert_eq!(positions, vec![0, 2]);

        let (macro_id, from_id) = d.prepare_from_step(22).unwrap();
        assert_eq!((macro_id, from_id), (1, 22));
        assert!(!d.dirty);

        d.selected_macro_mut().unwrap().steps[0].id = 101;
        d.selected_macro_mut().unwrap().steps[2].id = 303;
        assert_eq!(d.step_ids_at_positions(&positions), vec![101, 303]);

        d.selection.ids = std::collections::BTreeSet::from([101, 303]);
        d.mark_dirty();
        let (macro_id, selected_ids) = d.prepare_selected_steps().unwrap();
        assert_eq!((macro_id, selected_ids), (1, vec![101, 303]));
        assert!(!d.dirty);
        assert_eq!(d.store.snapshot().macros[0].steps[0].id, 101);
        assert_eq!(d.store.snapshot().macros[0].steps[2].id, 303);
    }

    fn folder_dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let (dir, mut d) = dialog();
        d.draft.folders = vec![crate::mkmacro::MkMacroFolder {
            id: 42,
            name: "Utilities".into(),
        }];
        d.save().unwrap();
        (dir, d)
    }

    #[test]
    fn new_folder_id_is_nonzero_unique_and_stable_across_creation_and_save() {
        let (_dir, mut d) = folder_dialog();
        let first = d.create_folder();
        let second = d.create_folder();
        assert_ne!(first, 0);
        assert_ne!(second, 0);
        let ids: Vec<_> = d.draft.folders.iter().map(|folder| folder.id).collect();
        assert_eq!(ids, vec![42, first, second]);
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 3);
        assert!(!repair_ids(&mut d.draft));
        d.save().unwrap();
        assert!(d.reload_with_decision(DirtyDecision::Discard));
        assert_eq!(
            d.draft
                .folders
                .iter()
                .map(|folder| folder.id)
                .collect::<Vec<_>>(),
            ids
        );
    }

    #[test]
    fn new_folder_marks_draft_dirty_without_changing_saved_document() {
        let (_dir, mut d) = folder_dialog();
        let saved = d.store.snapshot();
        assert!(!d.dirty);
        let id = d.create_folder();
        assert!(d.dirty);
        assert_eq!(
            d.draft.folders.last().unwrap(),
            &MkMacroFolder {
                id,
                name: "New Folder".into(),
            }
        );
        assert_eq!(*d.store.snapshot(), *saved);
        assert_eq!(*d.baseline, *saved);
    }

    #[test]
    fn new_folder_names_use_first_available_case_insensitive_suffix() {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.extend([
            MkMacroFolder {
                id: 43,
                name: "nEw fOlDeR".into(),
            },
            MkMacroFolder {
                id: 44,
                name: "NEW FOLDER 2".into(),
            },
            MkMacroFolder {
                id: 45,
                name: "New Folder 4".into(),
            },
        ]);
        d.create_folder();
        assert_eq!(d.draft.folders.last().unwrap().name, "New Folder 3");
        d.create_folder();
        assert_eq!(d.draft.folders.last().unwrap().name, "New Folder 5");
        let names: HashSet<_> = d
            .draft
            .folders
            .iter()
            .map(|f| f.name.to_lowercase())
            .collect();
        assert_eq!(names.len(), d.draft.folders.len());
    }

    fn assert_rejected_folder_rename(proposed: &str, expected: FolderNameError) {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(MkMacroFolder {
            id: 43,
            name: "Work".into(),
        });
        d.begin_folder_rename(42);
        d.folder_rename_text = proposed.into();
        let before = d.draft.clone();
        assert_eq!(d.rename_folder(42, proposed), Err(expected));
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.folder_rename_text, proposed);
    }

    #[test]
    fn rename_folder_rejects_empty_name() {
        assert_rejected_folder_rename("", FolderNameError::Empty);
        assert_eq!(
            FolderNameError::Empty.to_string(),
            "A macro folder name cannot be empty."
        );
    }

    #[test]
    fn rename_folder_rejects_whitespace_only_name() {
        assert_rejected_folder_rename(" \t\n\u{2003}", FolderNameError::Empty);
    }

    #[test]
    fn rename_folder_rejects_duplicate_name() {
        assert_rejected_folder_rename("Work", FolderNameError::Duplicate("Work".into()));
    }

    #[test]
    fn rename_folder_rejects_case_insensitive_duplicate_with_canonical_diagnostic() {
        assert_rejected_folder_rename("  wOrK  ", FolderNameError::Duplicate("Work".into()));
        assert_eq!(
            FolderNameError::Duplicate("Work".into()).to_string(),
            "A macro folder named \"Work\" already exists."
        );
    }

    #[test]
    fn rename_folder_matches_trimmed_unicode_names() {
        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(MkMacroFolder {
            id: 43,
            name: "  ÉCOLE  ".into(),
        });
        let before = d.draft.clone();
        assert_eq!(
            d.rename_folder(42, "école"),
            Err(FolderNameError::Duplicate("  ÉCOLE  ".into()))
        );
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
    }

    #[test]
    fn rename_folder_trims_and_changes_only_name_preserving_macros_and_presentation() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.draft.macros[2].folder_id = Some(42);
        d.draft.macros[2].description = "Preserve macro contents".into();
        d.draft.macros[2].steps.push(MkStep {
            id: 11,
            enabled: true,
            breakpoint: false,
            repeat: 2,
            delay_after_ms: 17,
            on_error: Default::default(),
            action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 50,
                ..Default::default()
            }),
        });
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        d.selection.click(&[11], 0, false, false);
        d.collapsed_folders.insert(42);
        d.begin_folder_rename(42);
        d.folder_error = Some("Previous validation error".into());
        let macros = d.draft.macros.clone();
        let baseline = d.baseline.clone();
        let mut expected = d.draft.clone();
        expected.folders[0].name = "Work".into();
        assert_eq!(d.rename_folder(42, " \tWork\u{2003}"), Ok(true));
        assert_eq!(d.draft.macros, macros);
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
        assert!(Arc::ptr_eq(&d.baseline, &baseline));
        assert_eq!(*d.store.snapshot(), *baseline);
        assert_eq!(d.selected_macro_id, Some(3));
        assert_eq!(d.selection.ids, std::collections::BTreeSet::from([11]));
        assert_eq!(d.collapsed_folders, HashSet::from([42]));
        assert_no_pending_folder_operations(&d);
    }

    #[test]
    fn rename_folder_allows_case_only_and_whitespace_normalization_of_self() {
        for (stored, proposed, expected) in [
            ("Utilities", "UTILITIES", "UTILITIES"),
            ("  Utilities  ", " Utilities ", "Utilities"),
        ] {
            let (_dir, mut d) = folder_dialog();
            d.draft.folders[0].name = stored.into();
            assert_eq!(d.rename_folder(42, proposed), Ok(true));
            assert_eq!(d.draft.folders[0].name, expected);
            assert!(d.dirty);
        }
    }

    #[test]
    fn rename_folder_no_op_preserves_dirty_state_and_clears_edit() {
        for dirty in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.dirty = dirty;
            d.begin_folder_rename(42);
            d.folder_error = Some("Previous validation error".into());
            let before = d.draft.clone();
            assert_eq!(d.rename_folder(42, "  Utilities  "), Ok(false));
            assert_eq!(d.draft, before);
            assert_eq!(d.dirty, dirty);
            assert_no_pending_folder_operations(&d);
        }
    }

    #[test]
    fn rename_folder_missing_id_does_not_change_draft_or_pending_edit() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        let before = d.draft.clone();
        for id in [0, 999] {
            assert_eq!(
                d.rename_folder(id, "Work"),
                Err(FolderNameError::MissingFolder(id))
            );
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Utilities");
        }
    }

    #[test]
    fn moving_macro_changes_only_folder_and_retains_macro_and_step_selection() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        let m = &mut d.draft.macros[2];
        m.description = "Keep this description".into();
        m.hotkey = Some(MkHotkey {
            key: MkKey::Delete,
            modifiers: vec![],
        });
        m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            process: Some("editor.exe".into()),
            ..Default::default()
        });
        m.steps = (11..=13)
            .map(|id| MkStep {
                id,
                enabled: true,
                breakpoint: false,
                repeat: 2,
                delay_after_ms: 17,
                on_error: Default::default(),
                action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                    fixed_ms: id * 10,
                    ..Default::default()
                }),
            })
            .collect();
        m.image_assets.push(crate::mkmacro::MkImageAsset {
            id: 7,
            name: "Preserve this asset".into(),
            relative_path: "mkmacro_assets/3/7.png".into(),
        });
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        let rows = [11, 12, 13];
        d.selection.click(&rows, 1, false, false);
        d.selection.click(&rows, 2, true, false);

        for destination in [Some(42), None] {
            let mut expected = d.draft.clone();
            expected.macros[2].folder_id = destination;
            let selected_steps = d.selection.ids.clone();
            let mut expected_selection = d.selection.clone();
            assert!(!d.dirty);
            assert!(d.move_macro_to_folder(3, destination));
            assert!(d.dirty);
            assert_eq!(d.draft, expected);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, selected_steps);
            // A subsequent shift click also verifies that the anchor survives.
            expected_selection.click(&rows, 0, false, true);
            d.selection.click(&rows, 0, false, true);
            assert_eq!(d.selection.ids, expected_selection.ids);
            d.save().unwrap();
            assert!(!d.move_macro_to_folder(3, destination));
            assert!(!d.dirty);
            assert_eq!(d.draft, expected);
        }
    }

    #[test]
    fn moving_folders_preserves_macro_id_hotkey_compilation_and_manual_run_eligibility() {
        use crate::mkmacro::{compile, hotkeys::compile_hotkey};

        let (_dir, mut d) = folder_dialog();
        d.draft.folders.push(crate::mkmacro::MkMacroFolder {
            id: 43,
            name: "Work".into(),
        });
        d.draft.macros = five_macros().macros;
        d.selected_macro_id = Some(1);
        d.draft.macros[0].hotkey = Some(MkHotkey {
            key: MkKey::Delete,
            modifiers: vec![],
        });
        let expected_hotkey = compile_hotkey(d.draft.macros[0].hotkey.as_ref().unwrap());
        assert!(expected_hotkey.is_some());
        for enabled in [true, false] {
            d.draft.macros[0].enabled = enabled;
            for destination in [Some(42), Some(43), None] {
                assert!(d.move_macro_to_folder(1, destination));
                let m = &d.draft.macros[0];
                assert_eq!(m.folder_id, destination);
                assert_eq!(m.id, 1);
                assert_eq!(compile(m).unwrap().macro_id, 1);
                assert_eq!(compile_hotkey(m.hotkey.as_ref().unwrap()), expected_hotkey);
                assert_eq!(d.playback_block_reason().is_none(), enabled);
            }
        }
    }

    #[test]
    fn choosing_unfiled_normalizes_a_dangling_folder_reference() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        d.draft.macros[2].folder_id = Some(999);
        let mut expected = d.draft.clone();
        expected.macros[2].folder_id = None;
        assert!(d.move_macro_to_folder(3, None));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_real_folder_assigns_id_and_marks_dirty() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        let mut expected = d.draft.clone();
        expected.macros[0].folder_id = Some(42);
        assert!(d.move_macro_to_folder(1, Some(42)));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_unfiled_clears_folder_and_marks_dirty() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.draft.macros[0].folder_id = Some(42);
        d.save().unwrap();
        let mut expected = d.draft.clone();
        expected.macros[0].folder_id = None;
        assert!(d.move_macro_to_folder(1, None));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
    }

    #[test]
    fn moving_macro_to_current_destination_has_no_additional_mutation() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        for destination in [Some(42), None] {
            assert!(d.move_macro_to_folder(1, destination));
            let draft = d.draft.clone();
            assert!(!d.move_macro_to_folder(1, destination));
            assert_eq!(d.draft, draft);
            assert!(d.dirty);
            d.save().unwrap();
            let baseline = d.baseline.clone();
            assert!(!d.move_macro_to_folder(1, destination));
            assert_eq!(d.draft, draft);
            assert!(!d.dirty);
            assert!(Arc::ptr_eq(&d.baseline, &baseline));
        }
    }

    #[test]
    fn moving_macro_to_missing_folder_rejects_dangling_reference() {
        let (_dir, mut d) = folder_dialog();
        d.draft.macros = five_macros().macros;
        d.save().unwrap();
        for current in [None, Some(42)] {
            d.draft.macros[0].folder_id = current;
            d.save().unwrap();
            let draft = d.draft.clone();
            for destination in [Some(999), Some(0)] {
                assert!(!d.move_macro_to_folder(1, destination));
                assert_eq!(d.draft, draft);
                assert!(!d.dirty);
            }
        }
    }

    #[test]
    fn moving_missing_macro_leaves_draft_unchanged() {
        let (_dir, mut d) = folder_dialog();
        let draft = d.draft.clone();
        for destination in [Some(42), None] {
            assert!(!d.move_macro_to_folder(999, destination));
            assert_eq!(d.draft, draft);
            assert!(!d.dirty);
        }
    }

    fn folder_deletion_dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let (dir, mut d) = folder_dialog();
        d.draft.folders.extend([
            MkMacroFolder {
                id: 43,
                name: "Other".into(),
            },
            MkMacroFolder {
                id: 44,
                name: "Empty".into(),
            },
        ]);
        d.draft.macros = five_macros().macros;
        for (index, m) in d.draft.macros.iter_mut().enumerate() {
            m.folder_id = [Some(42), Some(43), Some(42), None, Some(42)][index];
            m.description = format!("Description {}", m.id);
            m.enabled = index % 2 == 0;
            m.hotkey = Some(MkHotkey {
                key: MkKey::Function(m.id as u8),
                modifiers: vec![MkKey::Control],
            });
            m.hotkey_scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
                process: Some(format!("app{}.exe", m.id)),
                ..Default::default()
            });
            m.playback.speed_percent = 150;
            m.playback.random_delay_ms = 23;
            m.playback.random_offset_px = 7;
            m.steps = (1..=3)
                .map(|offset| MkStep {
                    id: m.id * 10 + offset,
                    enabled: offset != 2,
                    breakpoint: false,
                    repeat: 2,
                    delay_after_ms: 17,
                    on_error: crate::mkmacro::MkErrorPolicy::Continue,
                    action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                        fixed_ms: offset * 10,
                        ..Default::default()
                    }),
                })
                .collect();
            let relative_path = d
                .store
                .write_png_asset(
                    m.id,
                    1,
                    &image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255])),
                )
                .unwrap();
            m.image_assets.push(crate::mkmacro::MkImageAsset {
                id: 1,
                name: format!("Reference {}", m.id),
                relative_path: relative_path.to_string_lossy().into_owned(),
            });
        }
        d.save().unwrap();
        d.selected_macro_id = Some(3);
        d.selection.click(&[31, 32, 33], 1, false, false);
        d.selection.click(&[31, 32, 33], 2, true, false);
        d.collapsed_folders = HashSet::from([42, 43]);
        (dir, d)
    }

    #[test]
    fn folder_deletion_confirmation_shows_name_exact_count_and_preservation_warning() {
        use eframe::egui::{Context, epaint::Shape};
        fn collect_text(shape: &Shape, text: &mut String) {
            match shape {
                Shape::Text(shape) => {
                    text.push_str(&shape.galley.job.text);
                    text.push('\n');
                }
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, text);
                    }
                }
                _ => {}
            }
        }
        let (_dir, mut d) = folder_deletion_dialog();
        let before = d.draft.clone();
        for (id, name, count) in [(42, "Utilities", 3), (43, "Other", 1), (44, "Empty", 0)] {
            d.request_delete_folder(id);
            assert_eq!(d.pending_delete_folder, Some(id));
            assert!(d.folder_delete_confirmation.is_open());
            let ctx = Context::default();
            // A second frame includes the window after egui's initial sizing pass.
            let _ = ctx.run(Default::default(), |ctx| {
                d.folder_delete_confirmation.ui(ctx);
            });
            let output = ctx.run(Default::default(), |ctx| {
                d.folder_delete_confirmation.ui(ctx);
            });
            let mut text = String::new();
            for shape in &output.shapes {
                collect_text(&shape.shape, &mut text);
            }
            assert!(
                text.contains(&format!("Delete folder \"{name}\"? Members: {count}.")),
                "{text}"
            );
            assert!(text.contains("Members will move to Unfiled."), "{text}");
            assert!(text.contains("No macros will be deleted."), "{text}");
            assert_eq!(d.draft, before);
            assert!(!d.dirty);
        }
    }

    #[test]
    fn folder_deletion_with_zero_members_changes_only_the_folder_list() {
        let (_dir, mut d) = folder_deletion_dialog();
        let mut expected = d.draft.clone();
        expected.folders.remove(2);
        assert!(d.delete_folder(44));
        assert_eq!(d.draft, expected);
        assert!(d.dirty);
        assert_eq!(d.selected_macro_id, Some(3));
        assert_eq!(d.collapsed_folders, HashSet::from([42, 43]));
        d.save().unwrap();
        assert!(!d.delete_folder(44));
        assert!(!d.dirty);
        assert_eq!(d.draft, expected);
    }

    #[test]
    fn folder_deletion_preserves_all_macro_fields_assets_order_and_selection() {
        for rename_target in [42, 43] {
            let (_dir, mut d) = folder_deletion_dialog();
            d.begin_folder_rename(rename_target);
            d.folder_rename_text = "Uncommitted rename".into();
            d.folder_error = Some("Rename error".into());
            let before = d.draft.clone();
            let baseline = d.baseline.clone();
            let disk_before = fs::read(_dir.path().join(MKMACROS_FILE)).unwrap();
            let assets: Vec<_> = before
                .macros
                .iter()
                .map(|m| {
                    let path = d.store.asset_path(m.id, 1).unwrap();
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                })
                .collect();
            let mut expected_selection = d.selection.clone();
            d.request_delete_folder(42);
            d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
            let mut expected = before.clone();
            expected.folders.remove(0);
            for m in &mut expected.macros {
                if m.folder_id == Some(42) {
                    m.folder_id = None;
                }
            }
            // Full structural equality covers every macro field, unrelated membership,
            // global order, settings, and all surviving folders.
            assert_eq!(d.draft, expected);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, expected_selection.ids);
            d.selection.click(&[31, 32, 33], 0, false, true);
            expected_selection.click(&[31, 32, 33], 0, false, true);
            assert_eq!(
                d.selection.ids, expected_selection.ids,
                "selection anchor survives"
            );
            assert_eq!(d.collapsed_folders, HashSet::from([43]));
            if rename_target == 42 {
                assert_eq!(d.pending_folder_rename, None);
                assert!(d.folder_rename_text.is_empty());
                assert!(!d.folder_rename_needs_focus);
                assert_eq!(d.folder_error, None);
            } else {
                assert_eq!(d.pending_folder_rename, Some(43));
                assert_eq!(d.folder_rename_text, "Uncommitted rename");
                assert!(d.folder_rename_needs_focus);
                assert_eq!(d.folder_error.as_deref(), Some("Rename error"));
            }
            assert_eq!(d.pending_delete_folder, None);
            assert!(!d.folder_delete_confirmation.is_open());
            assert!(!d.delete_confirmation.is_open());
            assert!(d.dirty);
            assert!(Arc::ptr_eq(&baseline, &d.baseline));
            assert_eq!(*d.store.snapshot(), before);
            assert_eq!(
                fs::read(_dir.path().join(MKMACROS_FILE)).unwrap(),
                disk_before
            );
            for (path, bytes) in assets {
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn folder_deletion_cancellation_changes_only_pending_deletion_state() {
        for dirty in [false, true] {
            let (_dir, mut d) = folder_deletion_dialog();
            d.dirty = dirty;
            d.begin_folder_rename(42);
            d.folder_rename_text = "Uncommitted rename".into();
            d.folder_error = Some("Rename error".into());
            d.search = "query".into();
            d.request_delete_selected_macro();
            let before = d.draft.clone();
            let baseline = d.baseline.clone();
            let mut selection = d.selection.clone();
            d.request_delete_folder(42);
            d.handle_folder_delete_confirmation(ConfirmationResult::None);
            assert_eq!(d.pending_delete_folder, Some(42));
            d.handle_folder_delete_confirmation(ConfirmationResult::Cancelled);
            assert_eq!(d.pending_delete_folder, None);
            assert!(!d.folder_delete_confirmation.is_open());
            assert_eq!(d.draft, before);
            assert_eq!(*d.store.snapshot(), before);
            assert!(Arc::ptr_eq(&baseline, &d.baseline));
            assert_eq!(d.dirty, dirty);
            assert!(!d.conflict);
            assert_eq!(d.selected_macro_id, Some(3));
            assert_eq!(d.selection.ids, selection.ids);
            d.selection.click(&[31, 32, 33], 0, false, true);
            selection.click(&[31, 32, 33], 0, false, true);
            assert_eq!(d.selection.ids, selection.ids);
            assert_eq!(d.collapsed_folders, HashSet::from([42, 43]));
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Uncommitted rename");
            assert!(d.folder_rename_needs_focus);
            assert_eq!(d.folder_error.as_deref(), Some("Rename error"));
            assert_eq!(d.search, "query");
            assert!(d.delete_confirmation.is_open());
        }
    }

    #[test]
    fn folder_deletion_stale_confirmation_after_external_sync_is_harmless() {
        let (_dir, mut d) = folder_deletion_dialog();
        d.request_delete_folder(42);
        let mut external = d.draft.clone();
        external.folders.remove(0);
        for m in &mut external.macros {
            if m.folder_id == Some(42) {
                m.folder_id = None;
            }
        }
        d.store.save(external.clone()).unwrap();
        d.sync_external();
        assert_eq!(d.pending_delete_folder, None);
        d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
        assert_eq!(d.draft, external);
        assert!(!d.dirty);
        assert_eq!(d.selected_macro_id, Some(3));
        assert!(!d.folder_delete_confirmation.is_open());
    }

    #[test]
    fn folder_deletion_revalidates_target_before_mutating_any_members() {
        let (_dir, mut d) = folder_deletion_dialog();
        d.request_delete_folder(42);
        d.draft.folders.remove(0);
        // Even dangling membership must be untouched when the target is absent.
        let before = d.draft.clone();
        d.handle_folder_delete_confirmation(ConfirmationResult::Confirmed);
        assert_eq!(d.draft, before);
        assert!(!d.dirty);
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
        d.request_delete_folder(42);
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
    }

    fn assert_no_pending_folder_operations(d: &MkMacroDialog) {
        assert_eq!(d.pending_folder_rename, None);
        assert!(d.folder_rename_text.is_empty());
        assert_eq!(d.pending_delete_folder, None);
        assert!(!d.folder_delete_confirmation.is_open());
        assert_eq!(d.folder_error, None);
    }

    #[test]
    fn folder_collapse_changes_only_presentation_state() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.folder_rename_text = "Uncommitted rename".into();
        d.request_delete_folder(42);
        d.folder_error = Some("Existing validation error".into());
        d.search = "Utilities".into();
        let draft = d.draft.clone();
        let baseline = d.baseline.clone();
        let store = d.store.snapshot();
        for collapsed in [true, false] {
            d.toggle_folder_collapsed(42);
            assert_eq!(d.is_folder_collapsed(42), collapsed);
            assert_eq!(d.collapsed_folders.len(), usize::from(collapsed));
            assert_eq!(d.draft, draft);
            assert!(Arc::ptr_eq(&d.baseline, &baseline));
            assert!(Arc::ptr_eq(&d.store.snapshot(), &store));
            assert!(!d.dirty);
            assert!(!d.conflict);
            assert_eq!(d.selected_macro_id, None);
            assert_eq!(d.search, "Utilities");
            assert_eq!(d.pending_folder_rename, Some(42));
            assert_eq!(d.folder_rename_text, "Uncommitted rename");
            assert_eq!(d.pending_delete_folder, Some(42));
            assert!(d.folder_delete_confirmation.is_open());
            assert_eq!(d.folder_error.as_deref(), Some("Existing validation error"));
        }
        d.mark_dirty();
        d.toggle_folder_collapsed(42);
        assert!(
            d.dirty,
            "collapse must also preserve an already dirty draft"
        );
    }

    #[test]
    fn folder_reload_and_discard_clear_pending_operations_and_stale_collapse() {
        for close in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.draft.folders.push(crate::mkmacro::MkMacroFolder {
                id: 99,
                name: "Draft only".into(),
            });
            d.mark_dirty();
            d.toggle_folder_collapsed(42);
            d.toggle_folder_collapsed(99);
            d.begin_folder_rename(99);
            d.request_delete_folder(99);
            d.folder_error = Some("Invalid name".into());
            assert!(if close {
                d.close_with_decision(DirtyDecision::Discard)
            } else {
                d.reload_with_decision(DirtyDecision::Discard)
            });
            assert_no_pending_folder_operations(&d);
            assert_eq!(d.collapsed_folders, HashSet::from([42]));
            assert_eq!(d.draft, *d.baseline);
            assert!(!d.dirty);
        }
    }

    #[test]
    fn folder_reload_cancels_even_still_valid_pending_operations() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        assert!(d.reload_with_decision(DirtyDecision::Discard));
        assert_no_pending_folder_operations(&d);
    }

    #[test]
    fn folder_close_paths_cancel_operations_but_preserve_collapse_on_reopen() {
        for close_children in [false, true] {
            let (_dir, mut d) = folder_dialog();
            d.open();
            d.toggle_folder_collapsed(42);
            d.begin_folder_rename(42);
            d.request_delete_folder(42);
            if close_children {
                d.open = false;
                d.close_children();
            } else {
                assert!(d.close_with_decision(DirtyDecision::KeepEditing));
            }
            assert_no_pending_folder_operations(&d);
            d.open();
            assert!(d.is_folder_collapsed(42));
            assert!(!d.dirty);
        }
    }

    #[test]
    fn folder_keep_editing_preserves_pending_operations() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        d.mark_dirty();
        assert!(!d.reload_with_decision(DirtyDecision::KeepEditing));
        assert!(!d.close_with_decision(DirtyDecision::KeepEditing));
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.pending_delete_folder, Some(42));
        assert!(d.folder_delete_confirmation.is_open());
    }

    #[test]
    fn folder_sync_prunes_against_draft_even_without_external_changes() {
        let (_dir, mut d) = folder_dialog();
        d.toggle_folder_collapsed(42);
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        d.draft.folders.clear();
        d.mark_dirty();
        d.sync_external();
        assert!(d.collapsed_folders.is_empty());
        assert_no_pending_folder_operations(&d);
        assert!(d.dirty);
    }

    #[test]
    fn folder_external_reload_prunes_removed_ids_and_keeps_remaining_collapse() {
        let (_dir, mut d) = folder_dialog();
        d.toggle_folder_collapsed(42);
        d.collapsed_folders.insert(99);
        d.begin_folder_rename(42);
        d.request_delete_folder(42);
        let mut external = d.draft.clone();
        external.folders[0].name = "Updated".into();
        d.store.save(external).unwrap();
        d.sync_external();
        assert_eq!(d.collapsed_folders, HashSet::from([42]));
        assert_no_pending_folder_operations(&d);
        let mut external = d.draft.clone();
        external.folders.clear();
        d.store.save(external).unwrap();
        d.sync_external();
        assert!(d.collapsed_folders.is_empty());
        assert!(!d.dirty);
    }

    #[test]
    fn folder_requests_validate_ids_and_rename_can_be_cancelled() {
        let (_dir, mut d) = folder_dialog();
        d.begin_folder_rename(42);
        assert_eq!(d.pending_folder_rename, Some(42));
        assert_eq!(d.folder_rename_text, "Utilities");
        d.cancel_folder_rename();
        assert_no_pending_folder_operations(&d);
        d.begin_folder_rename(999);
        d.request_delete_folder(999);
        d.toggle_folder_collapsed(999);
        assert_no_pending_folder_operations(&d);
        assert!(d.collapsed_folders.is_empty());
        assert!(!d.dirty);
    }

    #[test]
    fn folder_ui_state_is_absent_from_serialized_document() {
        let (_dir, mut d) = folder_dialog();
        let before = serde_json::to_value(&d.draft).unwrap();
        d.toggle_folder_collapsed(42);
        d.begin_folder_rename(42);
        d.folder_rename_text = "Transient rename".into();
        d.request_delete_folder(42);
        d.folder_error = Some("Transient error".into());
        let after = serde_json::to_value(&d.draft).unwrap();
        assert_eq!(before, after);
        assert_eq!(
            after["folders"],
            serde_json::json!([{"id": 42, "name": "Utilities"}])
        );
        for field in [
            "collapsed_folders",
            "pending_folder_rename",
            "folder_rename_text",
            "pending_delete_folder",
            "folder_delete_confirmation",
            "folder_error",
        ] {
            assert!(
                after.get(field).is_none(),
                "unexpected serialized field: {field}"
            );
            assert!(after["folders"][0].get(field).is_none());
        }
        let round_trip: MkMacroDocument = serde_json::from_value(after).unwrap();
        assert_eq!(round_trip, d.draft);
    }

    fn picker_matcher(title: &str) -> MkWindowMatcher {
        MkWindowMatcher {
            title: Some(title.into()),
            ..Default::default()
        }
    }

    fn picker_macro(id: u64, hotkey_scope: MkHotkeyScope) -> MkMacro {
        MkMacro {
            id,
            name: format!("Macro {id}"),
            description: format!("Description {id}"),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Function(1),
                modifiers: vec![MkKey::Control],
            }),
            hotkey_scope,
            folder_id: Some(9),
            playback: Default::default(),
            steps: vec![],
            image_assets: vec![],
        }
    }

    fn picker_request(
        macro_id: u64,
        original: MkWindowMatcher,
    ) -> window_picker::MatcherEditRequest {
        window_picker::MatcherEditRequest {
            destination: window_picker::MatcherDestination::MacroHotkey { macro_id },
            original,
        }
    }

    #[test]
    fn picker_confirmation_targets_stable_macro_id_after_selection_changes() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let original_22 = picker_matcher("Original 22");
        let before_17 = picker_macro(17, MkHotkeyScope::ActiveWindow(original_17.clone()));
        let before_22 = picker_macro(22, MkHotkeyScope::ActiveWindow(original_22.clone()));
        d.draft.macros = vec![before_17.clone(), before_22.clone()];
        d.selected_macro_id = Some(22);

        let replacement = picker_matcher("Picked 17");
        let request = picker_request(17, original_17);
        assert!(d.apply_window_picker_confirmation(&request, replacement.clone()));

        let mut expected_17 = before_17;
        expected_17.hotkey_scope = MkHotkeyScope::ActiveWindow(replacement);
        assert_eq!(d.draft.macros[0], expected_17);
        assert_eq!(d.draft.macros[1], before_22);
        assert!(d.dirty);
    }

    #[test]
    fn picker_confirmation_discards_deleted_macro() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let before_22 = picker_macro(
            22,
            MkHotkeyScope::ActiveWindow(picker_matcher("Original 22")),
        );
        d.draft.macros = vec![
            picker_macro(17, MkHotkeyScope::ActiveWindow(original_17.clone())),
            before_22.clone(),
        ];
        d.draft.macros.retain(|macro_| macro_.id != 17);
        d.selected_macro_id = Some(22);

        assert!(!d.apply_window_picker_confirmation(
            &picker_request(17, original_17),
            picker_matcher("Picked 17"),
        ));
        assert_eq!(d.draft.macros, vec![before_22]);
        assert!(!d.dirty);
    }

    #[test]
    fn picker_confirmation_discards_macro_that_switched_to_any_window() {
        let (_dir, mut d) = dialog();
        let original_17 = picker_matcher("Original 17");
        let before_17 = picker_macro(17, MkHotkeyScope::AnyWindow);
        d.draft.macros = vec![before_17.clone()];
        d.selected_macro_id = Some(22);

        assert!(!d.apply_window_picker_confirmation(
            &picker_request(17, original_17),
            picker_matcher("Picked 17"),
        ));
        assert_eq!(d.draft.macros, vec![before_17]);
        assert!(!d.dirty);
    }

    #[test]
    fn picker_confirmation_replaces_matching_active_window_matcher_and_marks_dirty() {
        let (_dir, mut d) = dialog();
        let original = picker_matcher("Original");
        d.draft.macros = vec![picker_macro(
            17,
            MkHotkeyScope::ActiveWindow(original.clone()),
        )];
        d.selected_macro_id = None;

        let replacement = picker_matcher("Picked");
        assert!(
            d.apply_window_picker_confirmation(&picker_request(17, original), replacement.clone(),)
        );
        assert_eq!(
            d.draft.macros[0].hotkey_scope,
            MkHotkeyScope::ActiveWindow(replacement),
        );
        assert!(d.dirty);
    }

    #[test]
    fn picker_confirmation_of_identical_matcher_does_not_mark_dirty() {
        let (_dir, mut d) = dialog();
        let original = picker_matcher("Original");
        d.draft.macros = vec![picker_macro(
            17,
            MkHotkeyScope::ActiveWindow(original.clone()),
        )];
        d.selected_macro_id = Some(22);

        assert!(
            d.apply_window_picker_confirmation(&picker_request(17, original.clone()), original,)
        );
        assert_eq!(
            d.draft.macros[0].hotkey_scope,
            MkHotkeyScope::ActiveWindow(picker_matcher("Original")),
        );
        assert!(!d.dirty);
    }

    #[test]
    fn image_shortcuts_insert_around_stable_nested_anchor_and_report_change() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let matcher = crate::mkmacro::MkWindowMatcher {
            title: Some("Exact".into()),
            title_regex: Some("E.*".into()),
            process: Some("app.exe".into()),
            class: Some("Widget".into()),
        };
        let payload = MkImagePayload {
            asset_id: 10,
            region: SearchRegion::ClientArea {
                matcher: matcher.clone(),
            },
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            wait: MkWaitOptions {
                timeout_ms: 1000,
                poll_interval_ms: 50,
            },
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        };
        let steps = &mut d.selected_macro_mut().unwrap().steps;
        steps.extend([
            MkStep {
                id: 11,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::RepeatStart { count: 2 },
            },
            MkStep {
                id: 12,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::ImageFind(payload.clone()),
            },
            MkStep {
                id: 13,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::RepeatEnd,
            },
        ]);
        assert!(action_editor::insert_smooth_move_after(
            &mut d, 12, &payload
        ));
        assert!(action_editor::insert_activate_window_before(
            &mut d, 12, &payload
        ));
        let steps = &d.selected_macro().unwrap().steps;
        let anchor = steps.iter().position(|s| s.id == 12).unwrap();
        assert!(
            matches!(&steps[anchor - 1].action, MkAction::WindowActivate(p) if p.matcher == matcher)
        );
        assert!(matches!(
            &steps[anchor + 1].action,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Image {
                    asset_id: 10,
                    offset: crate::mkmacro::variables::MkPoint { x: 0, y: 0 }
                },
                duration_ms: 500
            })
        ));
        let ids: std::collections::HashSet<_> = steps.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), steps.len());
        assert!(steps.iter().all(|s| s.id != 0));
        assert!(d.dirty);
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
    }

    fn assert_created_macro(d: &MkMacroDialog, folder_id: Option<u64>) {
        let created = d.draft.macros.last().unwrap();
        assert_ne!(created.id, 0);
        assert_eq!(d.selected_macro_id, Some(created.id));
        assert!(d.selection.ids.is_empty());
        assert!(d.dirty);
        assert_eq!(
            created,
            &MkMacro {
                id: created.id,
                name: "New Macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id,
                playback: Default::default(),
                steps: vec![],
                image_assets: vec![],
            }
        );
    }

    #[test]
    fn global_creation_is_unfiled_even_when_a_folder_macro_is_selected() {
        let (_dir, mut d) = folder_dialog();
        assert!(d.create_macro_in_folder(Some(42)));
        let source = d.selected_macro().unwrap().clone();
        d.selection.click(&[17, 18], 0, false, false);
        d.dirty = false;

        d.create_macro();

        assert_created_macro(&d, None);
        assert_eq!(d.draft.macros.len(), 2);
        assert_eq!(d.draft.macros[0], source);
        assert_ne!(d.selected_macro_id, Some(source.id));
    }

    #[test]
    fn folder_context_creation_preserves_defaults_and_selects_repaired_id() {
        let (_dir, mut d) = folder_dialog();
        d.selection.click(&[17, 18], 0, false, false);
        let before = d.draft.clone();

        assert!(d.create_macro_in_folder(Some(42)));

        assert_created_macro(&d, Some(42));
        assert_eq!(d.draft.macros.len(), before.macros.len() + 1);
        assert_eq!(d.draft.folders, before.folders);
        assert!(!repair_ids(&mut d.draft));
    }

    #[test]
    fn invalid_destination_creation_is_atomic_even_for_an_existing_zero_folder() {
        let (_dir, mut d) = folder_dialog();
        d.create_macro();
        d.draft.folders.push(MkMacroFolder {
            id: 0,
            name: "Invalid folder".into(),
        });
        d.selection.click(&[17, 18], 0, false, false);
        let before = d.draft.clone();
        let selected = d.selected_macro_id;
        let selection = d.selection.clone();

        for dirty in [false, true] {
            d.dirty = dirty;
            for folder_id in [0, 999] {
                assert!(!d.create_macro_in_folder(Some(folder_id)));
                assert_eq!(d.draft, before);
                assert_eq!(d.selected_macro_id, selected);
                assert_eq!(d.selection.ids, selection.ids);
                assert_eq!(d.dirty, dirty);
            }
        }
        // Rejection must also retain the selection anchor.
        d.selection.click(&[17, 18], 1, false, true);
        assert_eq!(d.selection.ids, [17, 18].into_iter().collect());
    }

    #[test]
    fn duplication_preserves_membership_source_and_folders_but_clears_only_copy_hotkey() {
        for folder_id in [Some(42), None] {
            let (_dir, mut d) = folder_dialog();
            d.draft.macros = five_macros().macros;
            d.draft.folders.push(MkMacroFolder {
                id: 43,
                name: "Other folder".into(),
            });
            d.set_selected_macro(Some(2));
            let source = d.selected_macro_mut().unwrap();
            source.folder_id = folder_id;
            source.description = "Keep the source intact".into();
            source.enabled = false;
            source.hotkey = Some(MkHotkey {
                key: MkKey::Function(8),
                modifiers: vec![MkKey::Control],
            });
            source.hotkey_scope = MkHotkeyScope::ActiveWindow(picker_matcher("Editor"));
            source.steps = [17, 18]
                .into_iter()
                .map(|id| MkStep {
                    id,
                    enabled: true,
                    breakpoint: false,
                    repeat: 2,
                    delay_after_ms: 25,
                    on_error: Default::default(),
                    action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                        fixed_ms: id,
                        ..Default::default()
                    }),
                })
                .collect();
            let before = d.draft.clone();
            d.selection.click(&[17, 18], 0, false, false);
            d.dirty = false;

            d.duplicate_selected_macro();

            let source = &before.macros[1];
            let copy = d.draft.macros.last().unwrap();
            assert_eq!(copy.folder_id, source.folder_id);
            assert!(copy.hotkey.is_none());
            assert!(d.draft.macros[1].hotkey.is_some());
            assert_eq!(d.draft.macros[1].hotkey, source.hotkey);
            assert_ne!(copy.id, 0);
            assert!(before.macros.iter().all(|m| m.id != copy.id));
            let copied_ids: HashSet<_> = copy.steps.iter().map(|s| s.id).collect();
            assert_eq!(copied_ids.len(), source.steps.len());
            assert!(!copied_ids.contains(&0));
            assert!(source.steps.iter().all(|s| !copied_ids.contains(&s.id)));
            let mut expected_copy = source.clone();
            expected_copy.id = copy.id;
            expected_copy.name.push_str(" Copy");
            expected_copy.hotkey = None;
            for (expected, actual) in expected_copy.steps.iter_mut().zip(&copy.steps) {
                expected.id = actual.id;
            }
            assert_eq!(copy, &expected_copy);
            assert_eq!(d.selected_macro_id, Some(copy.id));
            assert!(d.selection.ids.is_empty());
            assert!(d.dirty);
            // Appending must leave every original macro and folder in document order.
            let mut originals = d.draft.clone();
            originals.macros.pop();
            assert_eq!(originals, before);
            assert!(!repair_ids(&mut d.draft));
        }
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
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }),
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
    fn delete_every_macro_save_empty_and_reopen_loaded_document() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = Arc::new(store);
        store.save(five_macros()).unwrap();
        let mut dialog = MkMacroDialog::new(Arc::clone(&store));

        while !dialog.draft.macros.is_empty() {
            let id = dialog.draft.macros[0].id;
            dialog.selected_macro_id = Some(id);
            dialog.delete_selected_macro();
            if dialog.draft.macros.is_empty() {
                assert!(dialog.selected_macro_id.is_none());
            } else {
                let selected = dialog.selected_macro_id.expect("selection advances");
                assert_ne!(selected, id);
                assert!(dialog.draft.macros.iter().any(|m| m.id == selected));
            }
        }
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.selected_macro_id.is_none());
        assert!(dialog.dirty);
        assert_eq!(
            store.snapshot().macros.len(),
            5,
            "deletes remain draft-only"
        );

        dialog.save().unwrap();
        assert!(dialog.store.snapshot().macros.is_empty());
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.dirty);
        assert!(!dialog.conflict);

        let path = dir.path().join(MKMACROS_FILE);
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        let disk: MkMacroDocument = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(disk.schema_version, SCHEMA_VERSION);
        assert!(disk.macros.is_empty());

        drop(dialog);
        drop(store);
        let (reopened, disposition) = MkMacroStore::open(dir.path()).unwrap();
        assert!(matches!(disposition, LoadDisposition::Loaded));
        assert!(reopened.snapshot().macros.is_empty());
    }

    #[test]
    fn watcher_and_external_sync_cannot_restore_macros_after_empty_save() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = Arc::new(store);
        store.save(five_macros()).unwrap();
        let mut dialog = MkMacroDialog::new(store);
        dialog.draft.macros.clear();
        dialog.dirty = true;
        dialog.save().unwrap();
        dialog.sync_external();
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.conflict);

        wait_for_empty_and_stable(&mut dialog, &dir.path().join(MKMACROS_FILE));
        dialog.sync_external();
        assert!(dialog.store.snapshot().macros.is_empty());
        assert!(dialog.draft.macros.is_empty());
        assert!(dialog.baseline.macros.is_empty());
        assert!(!dialog.conflict);
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
    fn structural_editor_is_atomic_configurable_and_cancelable() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.action_catalog_visible = true;
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("Repeat"));
        assert!(d.selected_macro().unwrap().steps.is_empty());
        if let MkAction::RepeatStart { count } = &mut d.action_editor.draft.as_mut().unwrap().action
        {
            *count = 9;
        }
        let mut editor = d.take_action_editor();
        editor.apply(&mut d).unwrap();
        d.action_editor = editor;
        assert!(matches!(
            d.selected_macro().unwrap().steps[0].action,
            MkAction::RepeatStart { count: 9 }
        ));
        assert!(matches!(
            d.selected_macro().unwrap().steps[1].action,
            MkAction::RepeatEnd
        ));
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
        let before = d.selected_macro().unwrap().steps.len();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("While"));
        d.action_editor.cancel();
        assert_eq!(d.selected_macro().unwrap().steps.len(), before);
    }

    #[test]
    fn wrapping_rejects_noncontiguous_selection_atomically() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        for milliseconds in 1..=3 {
            action_catalog::insert_action(
                &mut d,
                MkAction::Delay(crate::mkmacro::MkDelayPayload {
                    fixed_ms: milliseconds,
                    ..Default::default()
                }),
            );
        }
        let ids: Vec<_> = d
            .selected_macro()
            .unwrap()
            .steps
            .iter()
            .map(|s| s.id)
            .collect();
        d.selection.ids = [ids[0], ids[2]].into_iter().collect();
        let before = serde_json::to_vec(d.selected_macro().unwrap()).unwrap();
        action_catalog::select_descriptor(&mut d, &catalog_descriptor("If"));
        let mut editor = d.take_action_editor();
        assert!(editor.apply(&mut d).is_none());
        d.action_editor = editor;
        assert_eq!(
            serde_json::to_vec(d.selected_macro().unwrap()).unwrap(),
            before
        );
        assert!(d.command_error.as_deref().unwrap().contains("contiguous"));
    }

    #[test]
    fn direct_loop_controls_require_context_and_keep_palette() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.action_catalog_visible = true;
        let before = serde_json::to_vec(d.selected_macro().unwrap()).unwrap();
        action_catalog::insert_action(&mut d, MkAction::Break);
        assert_eq!(
            serde_json::to_vec(d.selected_macro().unwrap()).unwrap(),
            before
        );
        assert!(d.command_error.is_some());
        assert!(d.action_catalog_visible);
        action_catalog::insert_action(&mut d, MkAction::RepeatStart { count: 5 });
        let opener = d.selected_macro().unwrap().steps[0].id;
        d.selection.ids.clear();
        d.selection.ids.insert(opener);
        action_catalog::insert_action(&mut d, MkAction::Continue);
        assert!(matches!(
            d.selected_macro().unwrap().steps[1].action,
            MkAction::Continue
        ));
        assert!(d.action_catalog_visible);
        assert!(crate::mkmacro::compile(d.selected_macro().unwrap()).is_ok());
    }
    // Independent expectations for every model variant, including hidden actions.
    // Do not add a catch-all: new variants need an explicit capability decision.
    fn expected_action_contract(action: &MkAction) -> (action_catalog::EditorKind, bool) {
        use action_catalog::EditorKind;
        match action {
            MkAction::KeyDown(_)
            | MkAction::KeyUp(_)
            | MkAction::KeyPress(_)
            | MkAction::Hotkey(_) => (EditorKind::Keyboard, true),
            MkAction::Text(_) => (EditorKind::Text, true),
            MkAction::Notify(_) => (EditorKind::Notify, cfg!(windows)),
            MkAction::PlaySound(_) => (EditorKind::PlaySound, true),
            MkAction::MouseMove(_) => (EditorKind::MouseMove, true),
            MkAction::MouseDrag(_) => (EditorKind::MouseDrag, true),
            MkAction::MouseClick(_) | MkAction::ClickWithinRegion(_) => {
                (EditorKind::MouseClick, true)
            }
            MkAction::MouseDown(_) | MkAction::MouseUp(_) => (EditorKind::MouseButton, true),
            MkAction::MouseScroll { .. } => (EditorKind::MouseScroll, true),
            MkAction::Delay(_) => (EditorKind::Timing, true),
            MkAction::Process(_) => (EditorKind::Process, true),
            MkAction::LauncherCommand(_) => (EditorKind::Launcher, true),
            MkAction::WindowActivate(_)
            | MkAction::WindowClose(_)
            | MkAction::WindowWait(_)
            | MkAction::WindowMoveResize(_)
            | MkAction::WindowState { .. } => (EditorKind::Window, true),
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { .. }) => {
                (EditorKind::VirtualDesktop, cfg!(windows))
            }
            MkAction::VirtualDesktop(
                MkVirtualDesktopAction::Create
                | MkVirtualDesktopAction::SwitchLeft
                | MkVirtualDesktopAction::SwitchRight
                | MkVirtualDesktopAction::CloseCurrent,
            ) => (EditorKind::DirectInsert, cfg!(windows)),
            MkAction::If(_) | MkAction::WhileStart { .. } | MkAction::WaitUntil { .. } => {
                (EditorKind::Condition, true)
            }
            MkAction::RepeatStart { .. } => (EditorKind::Repeat, true),
            MkAction::SetVariable { .. } | MkAction::UnsetVariable { .. } => {
                (EditorKind::Variable, true)
            }
            MkAction::PromptInput(_) => (EditorKind::PromptInput, true),
            MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatEnd
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue => (EditorKind::DirectInsert, true),
            MkAction::ImageFind(_) | MkAction::ImageClick(_) => (EditorKind::Image, true),
            MkAction::CaptureScreenshot(_) | MkAction::WaitForVisualChange(_) => {
                (EditorKind::Screenshot, true)
            }
            MkAction::PixelCheck { .. } | MkAction::FindPixel(_) => (EditorKind::Pixel, true),
            MkAction::UiInvoke(_)
            | MkAction::UiSetValue { .. }
            | MkAction::UiReadValue { .. }
            | MkAction::UiToggle(_)
            | MkAction::UiSelect(_)
            | MkAction::UiFocus(_)
            | MkAction::UiWait(_) => (EditorKind::General, false),
        }
    }

    fn detail_examples() -> [(MkAction, &'static str); 4] {
        use crate::mkmacro::{MkClickWithinRegionPayload, MkDelayMode, MkDelayPayload, ScreenRect};
        [
            (
                MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
                    rect: ScreenRect::new(500, 300, 400, 200),
                    button: MkMouseButton::Left,
                    clicks: 1,
                    edge_padding_px: 10,
                }),
                "400×200 @ (500,300) · Left · random · padding 10",
            ),
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::RandomRange,
                    // Inactive fixed duration must not affect this action.
                    fixed_ms: u64::MAX,
                    minimum_ms: 250,
                    maximum_ms: 1250,
                }),
                "Random 250–1250 ms",
            ),
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::Fixed,
                    fixed_ms: 1000,
                    // Inactive range must not affect this action.
                    minimum_ms: u64::MAX,
                    maximum_ms: 0,
                }),
                "Fixed 1000 ms",
            ),
            (
                MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 3 }),
                "Desktop 3",
            ),
        ]
    }

    fn document_with_action(action: MkAction) -> MkMacroDocument {
        let mut document = five_macros();
        document.macros.truncate(1);
        document.macros[0].steps.push(MkStep {
            id: 1,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        });
        document
    }

    #[test]
    fn names_and_details_cover_catalog() {
        let descriptors = action_catalog::descriptors();
        let representatives = descriptors
            .iter()
            .map(|descriptor| (descriptor.make_default)())
            .chain(detail_examples().into_iter().map(|(action, _)| action));
        for action in representatives {
            let (editor, supported) = expected_action_contract(&action);
            let name = action_catalog::action_name(&action);
            assert!(!name.trim().is_empty(), "{action:?}: empty name");
            let details = action_catalog::action_details(&action);
            assert_eq!(
                details,
                action_catalog::action_details(&action.clone()),
                "{name}"
            );
            assert_eq!(
                details,
                action_catalog::action_details_with_assets(&action, &[]),
                "{name}"
            );
            assert_eq!(
                details,
                action_catalog::action_details_with_asset_name(&action, None),
                "{name}"
            );
            assert_eq!(action_catalog::editor_for_action(&action), editor, "{name}");
            assert_eq!(
                action_catalog::editor_route_recognizes(&action, editor),
                editor != action_catalog::EditorKind::General,
                "{name}: hidden UI Automation has no authoring route",
            );
            assert_eq!(
                crate::mkmacro::executor::has_runtime_support(&action),
                supported,
                "{name}: runtime classification",
            );
            let descriptor = descriptors.iter().find(|descriptor| {
                action_catalog::descriptor_name_matches_action(descriptor, &action)
            });
            // Legacy UI Automation intentionally shares an unavailable display name.
            if editor != action_catalog::EditorKind::General {
                let descriptor =
                    descriptor.unwrap_or_else(|| panic!("{name}: missing catalog entry"));
                assert_eq!(descriptor.editor, editor, "{name}: catalog editor");
            }

            // Standalone structural markers and missing assets may be invalid,
            // but validation and compilation must agree and neither may panic.
            let document = document_with_action(action);
            let diagnostics = validate_document(&document, None);
            match crate::mkmacro::compile(&document.macros[0]) {
                Ok(plan) => {
                    assert!(
                        crate::mkmacro::can_run(&diagnostics),
                        "{name}: {diagnostics:?}"
                    );
                    assert_eq!(
                        plan.instructions[0].step.action,
                        document.macros[0].steps[0].action
                    );
                }
                Err(errors) => {
                    assert!(!crate::mkmacro::can_run(&diagnostics), "{name}: {errors:?}");
                    assert_eq!(errors, diagnostics, "{name}");
                }
            }
        }
        for descriptor in descriptors {
            let (editor, supported) = expected_action_contract(&(descriptor.make_default)());
            assert_eq!(descriptor.editor, editor, "{}", descriptor.name);
            assert_eq!(
                descriptor.runtime,
                if supported {
                    action_catalog::RuntimeAvailability::Supported
                } else {
                    action_catalog::RuntimeAvailability::Unavailable
                },
                "{}",
                descriptor.name,
            );
        }
    }

    #[test]
    fn region_delay_and_desktop_details_are_exact() {
        for (action, expected) in detail_examples() {
            assert_eq!(action_catalog::action_details(&action), expected);
            let document = document_with_action(action);
            assert!(crate::mkmacro::can_run(&validate_document(&document, None)));
            assert!(crate::mkmacro::compile(&document.macros[0]).is_ok());
        }
        assert_eq!(
            action_catalog::action_name(&MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo {
                desktop: 3,
            })),
            "Go To Virtual Desktop",
        );
    }

    #[test]
    fn region_and_desktop_unsupported_backend_diagnostics_are_explicit() {
        use crate::mkmacro::{Backends, DiagnosticKind, Executor, RunControl};

        let [(region, _), _, _, (desktop, _)] = detail_examples();
        for (action, operation) in [(region, "SendInput"), (desktop, "virtual desktop")] {
            let document = document_with_action(action);
            let plan = crate::mkmacro::compile(&document.macros[0]).unwrap();
            // Exercise unsupported backends even on Windows, without sending
            // input or changing the user's desktop.
            let error = Executor::new(Backends::unsupported(), Arc::new(RunControl::default()))
                .execute(&plan, ExecutionOptions::normal(), &|_| {})
                .unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
            assert!(!error.message.trim().is_empty());
            assert_eq!(
                error.context.get("backend_operation").map(String::as_str),
                Some(operation)
            );
            assert_eq!(error.context.get("step_id").map(String::as_str), Some("1"));
            if operation == "virtual desktop" {
                assert_eq!(
                    error.message,
                    "Virtual desktop automation is available only on Windows"
                );
                assert_eq!(error.context.get("desktop").map(String::as_str), Some("3"));
                assert_eq!(
                    error.context.get("action").map(String::as_str),
                    Some("GoTo { desktop: 3 }")
                );
            }
        }
    }

    #[test]
    fn malformed_region_delay_and_desktop_payloads_are_rejected() {
        use crate::mkmacro::{
            DiagnosticSeverity, MkClickWithinRegionPayload, MkDelayMode, MkDelayPayload, ScreenRect,
        };
        let mut cases = vec![
            (
                MkAction::Delay(MkDelayPayload {
                    mode: MkDelayMode::RandomRange,
                    minimum_ms: 1250,
                    maximum_ms: 250,
                    ..Default::default()
                }),
                "invalid_delay_range",
            ),
            (
                MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 0 }),
                "invalid_virtual_desktop_number",
            ),
        ];
        // Exactly empty, over-padded, and overflow-prone persisted padding.
        for padding in [100, 201, u32::MAX] {
            cases.push((
                MkAction::ClickWithinRegion(MkClickWithinRegionPayload {
                    rect: ScreenRect::new(500, 300, 400, 200),
                    button: MkMouseButton::Left,
                    clicks: 1,
                    edge_padding_px: padding,
                }),
                "invalid_click_region_padding",
            ));
        }
        for (action, code) in cases {
            let document = document_with_action(action);
            let diagnostics = validate_document(&document, None);
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == code
                        && diagnostic.severity == DiagnosticSeverity::Fatal
                        && diagnostic.macro_id == 1
                        && diagnostic.step_id == Some(1)
                        && !diagnostic.message.trim().is_empty()
                }),
                "{code}: {diagnostics:?}"
            );
            assert!(!crate::mkmacro::can_run(&diagnostics), "{code}");
            assert_eq!(
                crate::mkmacro::compile(&document.macros[0]).unwrap_err(),
                diagnostics
            );
        }
    }

    #[test]
    fn visible_catalog_metadata_is_routable_and_supported() {
        for descriptor in action_catalog::visible_descriptors() {
            let action = (descriptor.make_default)();
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert!(
                descriptor.category.is_enabled(),
                "disabled category leaked through visible descriptor {:?}",
                descriptor.name
            );
            assert_eq!(
                descriptor.runtime,
                action_catalog::RuntimeAvailability::Supported
            );
            assert!(!descriptor.name.trim().is_empty());
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
                    | MkAction::VirtualDesktop(
                        MkVirtualDesktopAction::Create
                            | MkVirtualDesktopAction::SwitchLeft
                            | MkVirtualDesktopAction::SwitchRight
                            | MkVirtualDesktopAction::CloseCurrent
                    )
            ));
        }
    }

    #[test]
    fn image_result_details_use_friendly_catalog_description() {
        let action = MkAction::MouseMove(MkMouseMovePayload {
            target: MkCoordinateTarget::Image {
                asset_id: 10,
                offset: crate::mkmacro::variables::MkPoint { x: 2, y: -3 },
            },
            duration_ms: 500,
        });
        let assets = [crate::mkmacro::MkImageAsset {
            id: 10,
            name: "Save Button".into(),
            relative_path: "refs/save_button.png".into(),
        }];
        let details = action_catalog::action_details_with_assets(&action, &assets);
        assert_eq!(
            details,
            "Image Result: Save Button + (2,-3) · Smooth 500 ms"
        );
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
            action_catalog::format_coordinate_target(&MkCoordinateTarget::WindowClient {
                matcher: crate::mkmacro::MkWindowMatcher {
                    process: Some("editor.exe".into()),
                    title: Some("Document".into()),
                    ..Default::default()
                },
                point: MkPoint { x: 8, y: -2 },
            }),
            "Matched Window editor.exe (8, -2)"
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
            "Image Result: Missing image #9 + (-3,5)"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::MouseMove(MkMouseMovePayload {
                target: screen.clone(),
                duration_ms: 0
            })),
            "Screen (824, 446) · Instant"
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
            wait: MkWaitOptions {
                timeout_ms: 2500,
                poll_interval_ms: 100,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        };
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageFind(image.clone())),
            "Missing image #42 · Entire Desktop · fail if missing"
        );
        assert_eq!(
            action_catalog::action_details(&MkAction::ImageClick(image)),
            "Missing image #42 · Entire Desktop · center · 2500 ms"
        );
    }

    #[test]
    fn coordinate_details_are_payload_only_and_non_mutating() {
        let (_dir, mut dialog) = dialog();
        dialog.create_macro();
        for i in 0..500 {
            action_catalog::insert_action(
                &mut dialog,
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::Screen {
                        point: crate::mkmacro::variables::MkPoint {
                            x: i - 250,
                            y: 250 - i,
                        },
                    },
                    duration_ms: 0,
                }),
            );
        }
        let before = serde_json::to_vec(&dialog.draft).unwrap();
        for step in &dialog.selected_macro().unwrap().steps {
            let MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Screen { point },
                ..
            }) = &step.action
            else {
                panic!("unexpected action")
            };
            assert_eq!(
                action_catalog::action_details(&step.action),
                format!("Screen ({}, {}) · Instant", point.x, point.y)
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

        let mut editor = d.take_action_editor();
        assert!(editor.apply(&mut d).is_some());
        d.action_editor = editor;
        assert_eq!(d.selected_macro().unwrap().steps.len(), 1);
        assert!(d.action_catalog_visible);
        assert_eq!(d.action_search, "key");
    }

    #[test]
    fn keyboard_actions_are_visible_ready_and_insertable_through_catalog_editor() {
        let (_dir, mut d) = dialog();
        d.create_macro();

        for name in ["Key Press", "Key Down", "Key Up", "Hotkey"] {
            let descriptor = catalog_descriptor(name);
            assert_eq!(descriptor.name, name);
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert!(action_catalog::is_available_in_palette(&descriptor));

            let expected = (descriptor.make_default)();
            assert!(
                matches!(
                    (name, &expected),
                    ("Key Press", MkAction::KeyPress(_))
                        | ("Key Down", MkAction::KeyDown(_))
                        | ("Key Up", MkAction::KeyUp(_))
                        | ("Hotkey", MkAction::Hotkey(_))
                ),
                "{name} produced the wrong default keyboard action: {expected:?}"
            );

            assert!(action_catalog::select_descriptor(&mut d, &descriptor));
            let mut editor = d.take_action_editor();
            assert!(editor.apply(&mut d).is_some());
            d.action_editor = editor;
            assert_eq!(
                d.selected_macro().unwrap().steps.last().unwrap().action,
                expected
            );
        }

        assert_eq!(d.selected_macro().unwrap().steps.len(), 4);
    }

    #[test]
    fn palette_supports_five_consecutive_configurable_insertions() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        d.action_catalog_visible = true;
        d.action_search = "delay".into();
        for _ in 0..5 {
            assert!(action_catalog::select_descriptor(
                &mut d,
                &catalog_descriptor("Delay")
            ));
            let mut editor = d.take_action_editor();
            assert!(editor.apply(&mut d).is_some());
            d.action_editor = editor;
            assert!(d.action_catalog_visible);
            assert_eq!(d.action_search, "delay");
        }
        assert_eq!(d.selected_macro().unwrap().steps.len(), 5);
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
    fn advanced_control_markers_are_directly_insertable() {
        for name in [
            "Else",
            "End If",
            "End Repeat",
            "End While",
            "Break",
            "Continue",
        ] {
            let descriptor = action_catalog::descriptors()
                .into_iter()
                .find(|d| d.name == name)
                .unwrap();
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready
            );
            assert_eq!(descriptor.editor, action_catalog::EditorKind::DirectInsert);
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
            .begin_new(MkAction::Delay(crate::mkmacro::MkDelayPayload {
                fixed_ms: 1,
                ..Default::default()
            }));
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
                breakpoint: false,
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
    #[test]
    fn canonical_visible_catalog_contract() {
        use std::collections::HashSet;
        let mut names = HashSet::new();
        let mut variants = HashSet::new();
        for descriptor in action_catalog::visible_descriptors() {
            let context = |capability: &str, action: &MkAction| {
                format!(
                    "descriptor={:?}, variant={}, editor={:?}, capability={capability}",
                    descriptor.name,
                    action_catalog::action_name(action),
                    descriptor.editor
                )
            };
            let action = (descriptor.make_default)();
            assert!(
                descriptor.category.is_enabled(),
                "{}",
                context("enabled category", &action)
            );
            assert_eq!(
                descriptor.availability,
                action_catalog::ActionAvailability::Ready,
                "{}",
                context("visible availability", &action)
            );
            assert_eq!(
                descriptor.runtime,
                action_catalog::RuntimeAvailability::Supported,
                "{}",
                context("ready runtime", &action)
            );
            assert!(
                names.insert(descriptor.name),
                "duplicate name {}",
                descriptor.name
            );
            assert!(
                !descriptor.name.trim().is_empty(),
                "{}",
                context("name", &action)
            );
            assert!(
                !descriptor.description.trim().is_empty()
                    && descriptor.description.trim() != descriptor.name.trim(),
                "{}",
                context("meaningful description", &action)
            );
            assert!(
                serde_json::to_string(&action).is_ok(),
                "{}",
                context("serialization", &action)
            );
            // Some typed action families intentionally share one serialized action
            // variant while remaining separately discoverable catalog rows. Include
            // the typed operation in their catalog identity so this invariant still
            // catches genuinely duplicated descriptors.
            let variant = (
                std::mem::discriminant(&action),
                match &action {
                    MkAction::WindowState { state, .. } => Some(format!("window:{state:?}")),
                    MkAction::VirtualDesktop(operation) => {
                        Some(format!("virtual_desktop:{operation:?}"))
                    }
                    MkAction::WaitUntil {
                        condition: MkCondition::ImageSearch { found, .. },
                        ..
                    } => Some(format!("wait_image:{found}")),
                    _ => None,
                },
            );
            assert!(
                variants.insert(variant),
                "duplicate action variant for {}",
                descriptor.name
            );
            let no_configuration = action_catalog::requires_no_configuration(&action);
            assert_eq!(
                no_configuration,
                matches!(descriptor.editor, action_catalog::EditorKind::DirectInsert),
                "{}",
                context("no-configuration classification", &action)
            );
            assert!(
                no_configuration
                    || action_catalog::editor_route_recognizes(&action, descriptor.editor),
                "{}",
                context("implemented editor route", &action)
            );
            assert_eq!(
                descriptor.runtime == action_catalog::RuntimeAvailability::Supported,
                crate::mkmacro::executor::has_runtime_support(&action),
                "{}",
                context("runtime metadata agreement", &action)
            );
            let configurable = matches!(
                action_catalog::editor_contract(descriptor.editor),
                Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
            );
            assert!(!configurable || descriptor.editor != action_catalog::EditorKind::DirectInsert);
            let text = format!(
                "{} {} {} {}",
                descriptor.name,
                descriptor.description,
                action_catalog::action_name(&action),
                action_catalog::action_details(&action)
            );
            assert!(
                !action_catalog::contains_placeholder_wording(&text),
                "placeholder text for {}: {text}",
                descriptor.name
            );
            assert!(
                !text.contains("This action uses its existing specialized editor"),
                "historical fallback copy returned for {}",
                descriptor.name
            );
            assert!(descriptor.hidden_reason.is_none());

            // Exercise the real insertion/editor transaction, then the same
            // document validator used by save and run.
            let (_dir, mut dialog) = dialog();
            dialog.create_macro();
            assert!(action_catalog::select_descriptor(&mut dialog, &descriptor));
            let draft_contract = action_catalog::draft_validation_contract(&action);
            if configurable
                && draft_contract == action_catalog::DraftValidationContract::CommitReady
            {
                assert!(dialog.action_editor.draft.is_some());
                let mut editor = dialog.take_action_editor();
                assert!(editor.apply(&mut dialog).is_some());
                dialog.action_editor = editor;
            }
            let diagnostics = validate_document(&dialog.draft, None);
            match draft_contract {
                action_catalog::DraftValidationContract::CommitReady => assert!(
                    crate::mkmacro::can_run(&diagnostics),
                    "invalid default {}: {diagnostics:?}",
                    descriptor.name
                ),
                action_catalog::DraftValidationContract::AwaitingRequiredAsset => {
                    assert!(dialog.action_editor.draft.is_some());
                    assert!(dialog.selected_macro().unwrap().steps.is_empty());
                    let draft = &dialog.action_editor.draft.as_ref().unwrap().action;
                    assert!(
                        matches!(draft, MkAction::ImageFind(p) | MkAction::ImageClick(p) if p.asset_id == 0)
                            || matches!(draft, MkAction::WaitUntil {
                                condition: MkCondition::ImageSearch { search, .. }, ..
                            } if search.asset_id == 0)
                    );
                }
            }
        }
    }

    #[test]
    fn complete_catalog_metadata_and_hidden_policy() {
        use std::collections::HashSet;
        let descriptors = action_catalog::descriptors();
        let visible: HashSet<_> = action_catalog::visible_descriptors()
            .map(|d| d.name)
            .collect();
        let mut names = HashSet::new();
        for descriptor in descriptors {
            let action = (descriptor.make_default)();
            let context = format!(
                "descriptor={:?}, variant={}, editor={:?}",
                descriptor.name,
                action_catalog::action_name(&action),
                descriptor.editor
            );
            assert!(
                names.insert(descriptor.name),
                "{context}: duplicate descriptor name"
            );
            if descriptor.category != action_catalog::ActionCategory::UiAutomation {
                assert!(
                    action_catalog::descriptor_name_matches_action(&descriptor, &action),
                    "{context}: action-name mapping"
                );
            } else {
                assert!(
                    !action_catalog::action_name(&action).trim().is_empty(),
                    "{context}: hidden legacy action-name mapping"
                );
            }
            assert_eq!(
                descriptor.runtime == action_catalog::RuntimeAvailability::Supported,
                crate::mkmacro::executor::has_runtime_support(&action),
                "{context}: runtime contract"
            );
            if descriptor.availability == action_catalog::ActionAvailability::Hidden {
                assert_eq!(
                    descriptor.category,
                    action_catalog::ActionCategory::UiAutomation,
                    "{context}: only explicitly deferred UI Automation actions may be hidden"
                );
                assert!(
                    descriptor
                        .hidden_reason
                        .is_some_and(|r| !r.trim().is_empty()),
                    "{context}: hidden reason"
                );
                assert!(
                    !visible.contains(descriptor.name),
                    "{context}: hidden search leak"
                );
                // Filtering raw registry rows is intentionally insufficient:
                // the render input must originate from visible_descriptors().
                assert!(
                    !action_catalog::visible_descriptors()
                        .filter(|row| action_catalog::matches(row, descriptor.name))
                        .any(|row| row.name == descriptor.name),
                    "{context}: hidden descriptor rendered for an exact-name search"
                );

                let (_dir, mut dialog) = dialog();
                dialog.create_macro();
                assert!(
                    !action_catalog::select_descriptor(&mut dialog, &descriptor),
                    "{context}: hidden descriptor was selectable"
                );
                assert!(dialog.selected_macro().unwrap().steps.is_empty());
                assert!(dialog.action_editor.draft.is_none());

                // Hidden is an authoring policy only: existing documents must
                // retain the complete payload during load/save round trips.
                let encoded = serde_json::to_vec(&action).unwrap();
                let decoded: MkAction = serde_json::from_slice(&encoded).unwrap();
                assert_eq!(decoded, action, "{context}: saved payload changed");
            }
        }
    }

    #[test]
    fn authoritative_capability_audit_covers_every_descriptor() {
        for descriptor in action_catalog::descriptors() {
            let action = (descriptor.make_default)();
            let context = format!(
                "descriptor={:?}, action={}",
                descriptor.name,
                action_catalog::action_name(&action)
            );
            match descriptor.availability {
                action_catalog::ActionAvailability::Ready => {
                    assert_eq!(
                        descriptor.runtime,
                        action_catalog::RuntimeAvailability::Supported,
                        "{context}: runtime metadata"
                    );
                    assert!(
                        crate::mkmacro::executor::has_runtime_support(&action),
                        "{context}: executor support"
                    );
                    let contract = descriptor
                        .editor
                        .contract()
                        .unwrap_or_else(|| panic!("{context}: missing editor contract"));
                    match contract {
                        action_catalog::EditorContract::DirectInsert { .. } => assert_eq!(
                            descriptor.editor,
                            action_catalog::EditorKind::DirectInsert,
                            "{context}"
                        ),
                        action_catalog::EditorContract::Configurable { field_count } => {
                            assert!(field_count > 0, "{context}: editor has no fields");
                            assert!(
                                action_catalog::editor_route_recognizes(&action, descriptor.editor),
                                "{context}: editor route"
                            );
                            let completeness =
                                action_catalog::editor_completeness(descriptor.editor)
                                    .expect("complete editor metadata");
                            assert!(
                                completeness.has_primary_control,
                                "{context}: no primary control"
                            );
                            assert!(
                                !completeness.intentionally_disabled,
                                "{context}: permanently disabled editor"
                            );
                            assert!(
                                completeness.placeholder_copy.is_none(),
                                "{context}: placeholder editor copy"
                            );
                        }
                    }
                    assert!(
                        serde_json::to_vec(&action).is_ok(),
                        "{context}: serialization"
                    );
                    assert!(
                        action_catalog::descriptor_name_matches_action(&descriptor, &action),
                        "{context}: action name"
                    );
                    let details = action_catalog::action_details(&action);
                    assert!(
                        !details.trim().is_empty() && details.trim() != descriptor.name,
                        "{context}: meaningful details"
                    );
                    for text in [
                        descriptor.name,
                        descriptor.description,
                        action_catalog::action_name(&action),
                        details.as_str(),
                    ] {
                        assert!(
                            !action_catalog::contains_placeholder_wording(text),
                            "{context}: deferred wording: {text:?}"
                        );
                    }
                }
                action_catalog::ActionAvailability::Hidden => {
                    assert_eq!(
                        descriptor.category,
                        action_catalog::ActionCategory::UiAutomation,
                        "{context}: hidden non-UIA action"
                    );
                    assert_eq!(
                        descriptor.runtime,
                        action_catalog::RuntimeAvailability::Unavailable,
                        "{context}: hidden UIA must remain unavailable until independently complete"
                    );
                    assert!(
                        !action_catalog::is_available_in_palette(&descriptor),
                        "{context}: hidden palette leak"
                    );
                }
            }
        }
    }

    #[test]
    fn virtual_desktops_have_explicit_windows_action_routes() {
        let descriptors = action_catalog::descriptors();
        let direct_rows: Vec<_> = descriptors
            .iter()
            .filter(|descriptor| {
                descriptor.editor == action_catalog::EditorKind::DirectInsert
                    && matches!((descriptor.make_default)(), MkAction::VirtualDesktop(_))
            })
            .collect();
        assert_eq!(direct_rows.len(), 4);
        assert!(direct_rows.iter().all(|descriptor| {
            descriptor.category == action_catalog::ActionCategory::Windows
                && descriptor.editor == action_catalog::EditorKind::DirectInsert
                && action_catalog::requires_no_configuration(&(descriptor.make_default)())
                && action_catalog::editor_for_action(&(descriptor.make_default)())
                    == action_catalog::EditorKind::DirectInsert
        }));
        assert_eq!(
            direct_rows
                .iter()
                .map(|descriptor| descriptor.name)
                .collect::<Vec<_>>(),
            [
                "Create Virtual Desktop",
                "Switch Virtual Desktop Left",
                "Switch Virtual Desktop Right",
                "Close Current Virtual Desktop",
            ]
        );
        let go_to = descriptors
            .iter()
            .find(|descriptor| descriptor.name == "Go To Virtual Desktop")
            .expect("Go To Virtual Desktop");
        assert_eq!(go_to.category, action_catalog::ActionCategory::Windows);
        assert_eq!(go_to.editor, action_catalog::EditorKind::VirtualDesktop);
        assert!(matches!(
            go_to.editor.contract(),
            Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
        ));
        for desktop in [0, 1, 3, u32::MAX] {
            let action = MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop });
            assert_eq!(
                action_catalog::editor_for_action(&action),
                action_catalog::EditorKind::VirtualDesktop
            );
            assert!(!action_catalog::requires_no_configuration(&action));
        }
        assert!(matches!(
            (go_to.make_default)(),
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 1 })
        ));
    }

    #[test]
    #[cfg(windows)]
    fn virtual_desktop_catalog_selection_uses_the_intended_transactions() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        for name in [
            "Create Virtual Desktop",
            "Switch Virtual Desktop Left",
            "Switch Virtual Desktop Right",
            "Close Current Virtual Desktop",
        ] {
            let descriptor = catalog_descriptor(name);
            let count = d.selected_macro().unwrap().steps.len();
            assert!(action_catalog::select_descriptor(&mut d, &descriptor));
            assert!(d.action_editor.draft.is_none());
            let steps = &d.selected_macro().unwrap().steps;
            assert_eq!(steps.len(), count + 1);
            assert_eq!(steps.last().unwrap().action, (descriptor.make_default)());
        }

        let before = d.draft.clone();
        assert!(action_catalog::select_descriptor(
            &mut d,
            &catalog_descriptor("Go To Virtual Desktop")
        ));
        assert_eq!(d.draft, before);
        assert_eq!(
            d.action_editor.editor,
            Some(action_catalog::EditorKind::VirtualDesktop)
        );
        assert_eq!(
            d.action_editor.draft.as_ref().unwrap().action,
            MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop: 1 })
        );
        d.action_editor.cancel();
        assert_eq!(d.draft, before);
    }

    #[test]
    fn virtual_desktop_editor_defaults_edits_cancels_and_rejects_zero() {
        let (_dir, mut d) = dialog();
        d.create_macro();
        let descriptor = action_catalog::descriptors()
            .into_iter()
            .find(|row| row.name == "Go To Virtual Desktop")
            .unwrap();
        let go_to = |desktop| MkAction::VirtualDesktop(MkVirtualDesktopAction::GoTo { desktop });
        let mut editor = d.take_action_editor();
        editor.begin_new((descriptor.make_default)());
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(1));

        // Invalid new actions must not insert a step or consume the draft.
        editor.draft.as_mut().unwrap().action = go_to(0);
        let before = d.draft.clone();
        assert!(editor.apply(&mut d).is_none());
        assert_eq!(d.draft, before);
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(0));
        editor.draft.as_mut().unwrap().action = go_to(1);
        let id = editor.apply(&mut d).unwrap();
        let original = d.selected_macro().unwrap().steps[0].clone();
        assert_eq!(original.action, go_to(1));

        editor.begin_edit(&original);
        editor.draft.as_mut().unwrap().action = go_to(3);
        assert_eq!(d.selected_macro().unwrap().steps[0], original);
        assert_eq!(editor.apply(&mut d), Some(id));
        assert_eq!(d.selected_macro().unwrap().steps.len(), 1);
        let committed = d.selected_macro().unwrap().steps[0].clone();
        assert_eq!(committed.action, go_to(3));
        assert_eq!(
            action_catalog::action_details(&committed.action),
            "Desktop 3"
        );

        editor.begin_edit(&committed);
        assert_eq!(editor.draft.as_ref().unwrap(), &committed);
        editor.draft.as_mut().unwrap().action = go_to(8);
        editor.cancel();
        assert_eq!(d.selected_macro().unwrap().steps[0], committed);

        editor.begin_edit(&committed);
        editor.draft.as_mut().unwrap().action = go_to(0);
        let before = d.draft.clone();
        assert!(editor.apply(&mut d).is_none());
        assert_eq!(d.draft, before);
        assert_eq!(editor.editing_id, Some(id));
        assert_eq!(
            editor.editor,
            Some(action_catalog::EditorKind::VirtualDesktop)
        );
        assert_eq!(editor.draft.as_ref().unwrap().action, go_to(0));
        editor.draft.as_mut().unwrap().action = go_to(3);
        assert_eq!(editor.apply(&mut d), Some(id));
        assert_eq!(d.selected_macro().unwrap().steps[0], committed);
    }

    #[test]
    fn completed_actions_keep_real_editor_routes() {
        for (name, expected) in [
            ("Close Window", action_catalog::EditorKind::Window),
            ("Set Variable", action_catalog::EditorKind::Variable),
            ("Unset Variable", action_catalog::EditorKind::Variable),
            ("Wait Until", action_catalog::EditorKind::Condition),
            ("If", action_catalog::EditorKind::Condition),
            ("While", action_catalog::EditorKind::Condition),
            ("Repeat", action_catalog::EditorKind::Repeat),
        ] {
            let descriptor = catalog_descriptor(name);
            let action = (descriptor.make_default)();
            assert_eq!(
                descriptor.editor,
                expected,
                "descriptor={name:?}, variant={}, editor={:?}: expected specialized configurable editor",
                action_catalog::action_name(&action),
                descriptor.editor
            );
            assert!(
                matches!(
                    action_catalog::editor_contract(descriptor.editor),
                    Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
                ),
                "descriptor={name:?}, variant={}, editor={:?}: configurable editor contract",
                action_catalog::action_name(&action),
                descriptor.editor
            );
        }
    }

    #[test]
    fn configurable_editor_routes_build_meaningful_fields() {
        for descriptor in action_catalog::visible_descriptors().filter(|d| {
            matches!(
                action_catalog::editor_contract(d.editor),
                Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
            )
        }) {
            let (_dir, mut dialog) = dialog();
            dialog.create_macro();
            assert!(action_catalog::select_descriptor(&mut dialog, &descriptor));
            let draft = dialog
                .action_editor
                .draft
                .as_ref()
                .expect("configurable action must open editor");
            assert!(action_catalog::editor_route_recognizes(
                &draft.action,
                descriptor.editor
            ));
            assert!(
                matches!(
                    action_catalog::editor_contract(descriptor.editor),
                    Some(action_catalog::EditorContract::Configurable { field_count: 1.. })
                ),
                "{} produced an inert editor",
                descriptor.name
            );
        }
    }

    #[test]
    fn mouse_catalog_contracts() {
        let action = |name| (catalog_descriptor(name).make_default)();
        assert!(matches!(
            action("Mouse Move"),
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Screen { .. },
                duration_ms: 0
            })
        ));
        assert!(matches!(
            action("Mouse Click"),
            MkAction::MouseClick(MkMousePayload {
                button: MkMouseButton::Left,
                clicks: 1,
                ..
            })
        ));
        assert!(matches!(
            action("Mouse Drag"),
            MkAction::MouseDrag(crate::mkmacro::MkMouseDragPayload {
                from: MkCoordinateTarget::Screen { .. },
                to: MkCoordinateTarget::Screen { .. },
                button: MkMouseButton::Left,
                duration_ms: 400
            })
        ));
        assert_eq!(
            catalog_descriptor("Mouse Down").editor,
            action_catalog::EditorKind::MouseButton
        );
        assert_eq!(
            catalog_descriptor("Mouse Up").editor,
            action_catalog::EditorKind::MouseButton
        );
        assert_eq!(
            catalog_descriptor("Mouse Scroll").editor,
            action_catalog::EditorKind::MouseScroll
        );
        assert!(matches!(
            action("Mouse Scroll"),
            MkAction::MouseScroll {
                axis: MkMouseScrollAxis::Vertical,
                i32_delta: -120
            }
        ));
        for name in [
            "Mouse Move",
            "Mouse Click",
            "Mouse Drag",
            "Mouse Down",
            "Mouse Up",
            "Mouse Scroll",
        ] {
            assert!(
                crate::mkmacro::executor::has_runtime_support(&action(name)),
                "{name}"
            );
        }
    }

    fn synthetic_debug_snapshot(
        run_id: u64,
        revision: u64,
        state: crate::mkmacro::RuntimeState,
        reason: crate::mkmacro::DebugSnapshotReason,
        pause_reason: Option<crate::mkmacro::RuntimePauseReason>,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let variables = Arc::new(crate::mkmacro::RuntimeVariables::from([(
            "answer".into(),
            crate::mkmacro::MkValue::Number(42.0),
        )]));
        Arc::new(crate::mkmacro::RuntimeSnapshot {
            state,
            run_mode: crate::mkmacro::RuntimeRunMode::Debug,
            run_id,
            macro_id: Some(7),
            pause_reason,
            debug_snapshot: Some(Arc::new(crate::mkmacro::DebugSnapshot {
                step_id: None,
                variables: variables.clone(),
                reason,
            })),
            debug_variables: variables,
            debug_snapshot_reason: Some(reason),
            revision,
            ..crate::mkmacro::RuntimeSnapshot::default()
        })
    }

    fn synthetic_normal_snapshot(
        run_id: u64,
        revision: u64,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let mut snapshot = (*synthetic_debug_snapshot(
            run_id,
            revision,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
            None,
        ))
        .clone();
        snapshot.run_mode = crate::mkmacro::RuntimeRunMode::Normal;
        snapshot.debug_snapshot = None;
        snapshot.debug_variables = Arc::new(Default::default());
        snapshot.debug_snapshot_reason = None;
        Arc::new(snapshot)
    }

    fn wait_for_debug_boundary_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
        reason: crate::mkmacro::DebugSnapshotReason,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id
                && snapshot
                    .debug_snapshot
                    .as_ref()
                    .is_some_and(|debug| debug.reason == reason)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish {reason:?} for run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_terminal_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id
                && matches!(
                    snapshot.state,
                    RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
                )
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_mode_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
        mode: crate::mkmacro::RuntimeRunMode,
    ) -> Arc<crate::mkmacro::RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id && snapshot.run_mode == mode {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish {mode:?} run after {previous_run_id}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn submit_after_previous_run(runtime: &MacroRuntime, command: RuntimeCommand) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match runtime.command(command.clone()) {
                crate::mkmacro::CommandResult::Accepted => return,
                crate::mkmacro::CommandResult::AlreadyRunning { .. } => {
                    assert!(
                        Instant::now() < deadline,
                        "runtime admission was not released"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                crate::mkmacro::CommandResult::Rejected(diagnostic) => {
                    panic!("runtime rejected {command:?}: {diagnostic:?}")
                }
            }
        }
    }

    fn real_runtime_dialog() -> (
        tempfile::TempDir,
        MkMacroDialog,
        MacroRuntime,
        Arc<crate::mkmacro::executor::fake::FakeBackend>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let set_variable = |id: u64, name: &str, value: MkValue| MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::SetVariable {
                name: name.into(),
                value,
            },
        };
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                macros: vec![
                    MkMacro {
                        id: 1,
                        name: "First debug run".into(),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: Default::default(),
                        steps: vec![
                            MkStep {
                                delay_after_ms: 25,
                                ..set_variable(11, "count", MkValue::Number(1.0))
                            },
                            set_variable(12, "result", MkValue::String("done".into())),
                        ],
                        image_assets: vec![],
                    },
                    MkMacro {
                        id: 2,
                        name: "Second debug run".into(),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: Default::default(),
                        steps: vec![MkStep {
                            delay_after_ms: 25,
                            ..set_variable(21, "fresh", MkValue::String("new".into()))
                        }],
                        image_assets: vec![],
                    },
                ],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let dialog = MkMacroDialog::new(store.clone());
        let fake = Arc::new(crate::mkmacro::executor::fake::FakeBackend::default());
        let runtime = MacroRuntime::new(store, fake.clone().backends());
        (directory, dialog, runtime, fake)
    }

    #[test]
    fn runtime_inspector_lifecycle_replaces_debug_runs_and_clears_on_normal_run() {
        let (_dir, mut dialog, runtime, _fake) = real_runtime_dialog();
        let first_previous_run_id = runtime.snapshot().run_id;
        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            crate::mkmacro::CommandResult::Accepted
        );
        let first_started = wait_for_debug_boundary_after(
            &runtime,
            first_previous_run_id,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
        );
        dialog.observe_runtime_snapshot(Some(first_started.clone()));
        assert_eq!(
            dialog
                .runtime_inspector_snapshot
                .as_ref()
                .map(|snapshot| snapshot.run_id),
            Some(first_started.run_id)
        );
        assert!(dialog.runtime_inspector_is_current_debug_run);

        let first = wait_for_terminal_after(&runtime, first_previous_run_id);
        assert_eq!(first.state, RuntimeState::Completed);
        assert_eq!(first.run_mode, crate::mkmacro::RuntimeRunMode::Debug);
        assert_eq!(first.last_completed_step_id, Some(12));
        assert_eq!(first.debug_variables_step_id, Some(12));
        let first_debug = first
            .debug_snapshot
            .as_ref()
            .expect("completed debug run retains its final snapshot");
        assert_eq!(
            first_debug.reason,
            crate::mkmacro::DebugSnapshotReason::RunFinished
        );
        assert_eq!(first_debug.step_id, Some(12));
        assert_eq!(
            first_debug.variables.get("count"),
            Some(&MkValue::Number(1.0))
        );
        assert_eq!(
            first_debug.variables.get("result"),
            Some(&MkValue::String("done".into()))
        );
        dialog.observe_runtime_snapshot(Some(first.clone()));
        assert!(!dialog.runtime_inspector_is_current_debug_run);
        let retained_first = dialog
            .runtime_inspector_snapshot
            .as_ref()
            .expect("completed debug snapshot is retained by the dialog");
        assert_eq!(retained_first.state, RuntimeState::Completed);
        assert_eq!(retained_first.run_id, first.run_id);
        assert_eq!(
            retained_first.debug_snapshot.as_ref().unwrap().reason,
            crate::mkmacro::DebugSnapshotReason::RunFinished
        );
        assert_eq!(
            retained_first.debug_snapshot.as_ref().unwrap().step_id,
            Some(12)
        );
        assert_eq!(
            retained_first
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("count"),
            Some(&MkValue::Number(1.0))
        );
        assert_eq!(
            retained_first
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("result"),
            Some(&MkValue::String("done".into()))
        );
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(
                retained_first,
                &dialog.draft,
            )
            .variable_label,
            "Variables at successful completion"
        );
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(
                retained_first,
                &dialog.draft,
            )
            .title,
            "Runtime Inspector — Last Debug Run"
        );

        let second_previous_run_id = first.run_id;
        submit_after_previous_run(&runtime, RuntimeCommand::DebugRun(2));
        let second_started = wait_for_debug_boundary_after(
            &runtime,
            second_previous_run_id,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
        );
        assert!(second_started.run_id > first.run_id);
        assert_eq!(second_started.state, RuntimeState::Running);
        assert!(
            !second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("count")
        );
        assert!(
            !second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("result")
        );
        assert_eq!(
            second_started
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("macro.id"),
            Some(&MkValue::Number(2.0))
        );
        dialog.observe_runtime_snapshot(Some(second_started.clone()));
        assert_eq!(
            dialog.runtime_inspector_snapshot.as_ref().unwrap().run_id,
            second_started.run_id
        );
        assert!(dialog.runtime_inspector_is_current_debug_run);

        let second = wait_for_terminal_after(&runtime, second_previous_run_id);
        assert_eq!(second.state, RuntimeState::Completed);
        assert_eq!(second.run_mode, crate::mkmacro::RuntimeRunMode::Debug);
        assert_eq!(
            second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .get("fresh"),
            Some(&MkValue::String("new".into()))
        );
        assert!(
            !second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("count")
        );
        assert!(
            !second
                .debug_snapshot
                .as_ref()
                .unwrap()
                .variables
                .contains_key("result")
        );
        dialog.observe_runtime_snapshot(Some(second.clone()));
        let retained_second = dialog
            .runtime_inspector_snapshot
            .as_ref()
            .expect("second completed debug snapshot replaces the first");
        assert_eq!(retained_second.run_id, second.run_id);
        let retained_second_variables = &retained_second.debug_snapshot.as_ref().unwrap().variables;
        assert_eq!(
            retained_second_variables.get("fresh"),
            Some(&MkValue::String("new".into()))
        );
        assert!(!retained_second_variables.contains_key("count"));
        assert!(!retained_second_variables.contains_key("result"));

        submit_after_previous_run(&runtime, RuntimeCommand::Run(2));
        let normal_started = wait_for_mode_after(
            &runtime,
            second.run_id,
            crate::mkmacro::RuntimeRunMode::Normal,
        );
        assert!(normal_started.debug_snapshot.is_none());
        dialog.observe_runtime_snapshot(Some(normal_started));
        assert!(dialog.runtime_inspector_snapshot.is_none());
        assert!(!dialog.runtime_inspector_is_current_debug_run);

        let normal = wait_for_terminal_after(&runtime, second.run_id);
        assert_eq!(normal.run_mode, crate::mkmacro::RuntimeRunMode::Normal);
        assert!(normal.debug_snapshot.is_none());
        assert!(normal.debug_variables.is_empty());
        assert_eq!(
            runtime_inspector::RuntimeInspectorViewModel::from_snapshot(&normal, &dialog.draft)
                .title,
            "Runtime Inspector — Normal Run"
        );
    }

    #[test]
    fn runtime_inspector_opens_once_per_breakpoint_occurrence() {
        let (_dir, mut dialog) = dialog();
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            1,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::RunStarted,
            None,
        )));
        let breakpoint = synthetic_debug_snapshot(
            11,
            2,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id: 99 }),
        );
        dialog.observe_runtime_snapshot(Some(breakpoint.clone()));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(dialog.runtime_inspector_active_breakpoint, Some((11, 99)));

        dialog.runtime_inspector_open = false;
        let mut newer_breakpoint = (*breakpoint).clone();
        newer_breakpoint.revision = 3;
        newer_breakpoint.debug_variables = Arc::new(crate::mkmacro::RuntimeVariables::from([(
            "answer".into(),
            crate::mkmacro::MkValue::Number(84.0),
        )]));
        let newer_variables = newer_breakpoint.debug_variables.clone();
        newer_breakpoint.debug_snapshot = Some(Arc::new(crate::mkmacro::DebugSnapshot {
            step_id: Some(99),
            variables: newer_variables,
            reason: crate::mkmacro::DebugSnapshotReason::Breakpoint,
        }));
        dialog.observe_runtime_snapshot(Some(Arc::new(newer_breakpoint)));
        assert!(!dialog.runtime_inspector_open);
        assert_eq!(
            dialog.runtime_inspector_snapshot.as_ref().unwrap().revision,
            3
        );
        assert_eq!(
            dialog
                .runtime_inspector_snapshot
                .as_ref()
                .unwrap()
                .debug_variables
                .get("answer"),
            Some(&crate::mkmacro::MkValue::Number(84.0))
        );
        assert_eq!(dialog.runtime_inspector_active_breakpoint, Some((11, 99)));

        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            4,
            crate::mkmacro::RuntimeState::Running,
            crate::mkmacro::DebugSnapshotReason::StepBoundary,
            None,
        )));
        assert!(dialog.runtime_inspector_snapshot.is_some());
        assert_eq!(dialog.runtime_inspector_active_breakpoint, None);

        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            5,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id: 99 }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(dialog.runtime_inspector_active_breakpoint, Some((11, 99)));

        dialog.runtime_inspector_open = false;
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            11,
            6,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id: 100 }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(dialog.runtime_inspector_active_breakpoint, Some((11, 100)));

        dialog.runtime_inspector_open = false;
        dialog.observe_runtime_snapshot(Some(synthetic_debug_snapshot(
            12,
            1,
            crate::mkmacro::RuntimeState::Paused,
            crate::mkmacro::DebugSnapshotReason::Breakpoint,
            Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id: 100 }),
        )));
        assert!(dialog.runtime_inspector_open);
        assert_eq!(dialog.runtime_inspector_active_breakpoint, Some((12, 100)));
    }
}
impl MkMacroDialog {
    /// Returns an operation client for constructing dialog-scoped visual tools.
    pub fn visual_overlay_controller(&self) -> SharedVisualOverlayController {
        self.visual_overlay.clone()
    }

    /// Temporarily moves the editor out while preserving its required shared
    /// visual-overlay client in the replacement state.
    pub fn take_action_editor(&mut self) -> action_editor::ActionEditorState {
        std::mem::replace(
            &mut self.action_editor,
            action_editor::ActionEditorState::new(self.visual_overlay.clone()),
        )
    }

    pub fn new(store: Arc<MkMacroStore>) -> Self {
        Self::new_with_authoring_context(store, MkMacroAuthoringContext::default())
    }
    pub fn new_with_authoring_context(
        store: Arc<MkMacroStore>,
        authoring_context: MkMacroAuthoringContext,
    ) -> Self {
        let baseline = store.snapshot();
        // The dialog is the sole production ownership boundary for the native
        // visual-overlay worker; every authoring surface receives a clone.
        let visual_overlay = SharedVisualOverlayController::new_dialog_owner();
        Self {
            open: false,
            draft: (*baseline).clone(),
            baseline,
            store,
            authoring_context,
            action_editor: action_editor::ActionEditorState::new(visual_overlay.clone()),
            visual_overlay,
            dirty: false,
            conflict: false,
            selected_macro_id: None,
            selection: Default::default(),
            search: String::new(),
            collapsed_folders: HashSet::new(),
            pending_folder_rename: None,
            folder_rename_text: String::new(),
            folder_rename_needs_focus: false,
            pending_delete_folder: None,
            folder_delete_confirmation: Default::default(),
            folder_error: None,
            delete_confirmation: Default::default(),
            unwrap_confirmation: Default::default(),
            pending_unwrap_block: None,
            pending_unwrap_selection: None,
            hotkey_capture: false,
            record_hotkey_capture: false,
            action_catalog_visible: false,
            action_search: String::new(),
            structural_insertion: None,
            uia_editor: Default::default(),
            window_picker: Default::default(),
            launcher_action_picker: Default::default(),
            command_error: None,
            recorder_options: Default::default(),
            pending_recording: None,
            runtime_inspector_open: false,
            runtime_inspector_show_internal: false,
            runtime_inspector_builtins_open: false,
            runtime_inspector_snapshot: None,
            runtime_inspector_is_current_debug_run: false,
            runtime_inspector_observed_run: None,
            runtime_inspector_active_breakpoint: None,
        }
    }
    pub fn open(&mut self) {
        self.sync_external();
        self.open = true;
        crate::mkmacro::runtime::set_recording_target(self.selected_macro_id);
        crate::mkmacro::runtime::set_recording_options(self.recorder_options.clone());
    }
    pub fn sync_external(&mut self) {
        let current = self.store.snapshot();
        if *current != *self.baseline {
            if self.dirty {
                self.conflict = true;
            } else {
                self.draft = (*current).clone();
                self.baseline = current;
                self.cancel_folder_operations();
                if self.selected_macro().is_none() {
                    self.set_selected_macro(None);
                }
            }
        }
        self.prune_folder_ui_state();
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

    pub fn create_folder(&mut self) -> u64 {
        let names: HashSet<_> = self
            .draft
            .folders
            .iter()
            .map(|folder| folder.name.to_lowercase())
            .collect();
        let mut name = "New Folder".to_owned();
        let mut suffix = 2;
        while names.contains(&name.to_lowercase()) {
            name = format!("New Folder {suffix}");
            suffix += 1;
        }
        self.draft.folders.push(MkMacroFolder { id: 0, name });
        repair_ids(&mut self.draft);
        let id = self.draft.folders.last().unwrap().id;
        self.mark_dirty();
        id
    }

    /// Returns true only when an existing macro changes to a valid destination.
    pub fn move_macro_to_folder(&mut self, macro_id: u64, folder_id: Option<u64>) -> bool {
        if folder_id
            .is_some_and(|id| id == 0 || !self.draft.folders.iter().any(|folder| folder.id == id))
        {
            return false;
        }
        let Some(m) = self.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
            return false;
        };
        if m.folder_id == folder_id {
            return false;
        }
        m.folder_id = folder_id;
        self.mark_dirty();
        true
    }

    pub fn is_folder_collapsed(&self, folder_id: u64) -> bool {
        self.collapsed_folders.contains(&folder_id)
    }

    /// Collapse is presentation-only and must not mark the draft dirty.
    pub fn toggle_folder_collapsed(&mut self, folder_id: u64) {
        if !self
            .draft
            .folders
            .iter()
            .any(|folder| folder.id == folder_id)
        {
            return;
        }
        if !self.collapsed_folders.remove(&folder_id) {
            self.collapsed_folders.insert(folder_id);
        }
    }

    pub fn begin_folder_rename(&mut self, folder_id: u64) {
        self.cancel_folder_rename();
        if let Some(folder) = self
            .draft
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
        {
            self.pending_folder_rename = Some(folder_id);
            self.folder_rename_text = folder.name.clone();
            self.folder_rename_needs_focus = true;
        }
    }

    /// Names match using Rust's Unicode-aware `str::to_lowercase` after trimming.
    /// Returns false for an unchanged name, without changing the dirty flag.
    pub fn rename_folder(
        &mut self,
        folder_id: u64,
        proposed_name: &str,
    ) -> Result<bool, FolderNameError> {
        let index = self
            .draft
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
            .ok_or(FolderNameError::MissingFolder(folder_id))?;
        let name = proposed_name.trim();
        if name.is_empty() {
            return Err(FolderNameError::Empty);
        }
        let normalized = name.to_lowercase();
        if let Some(conflict) = self.draft.folders.iter().find(|folder| {
            folder.id != folder_id && folder.name.trim().to_lowercase() == normalized
        }) {
            return Err(FolderNameError::Duplicate(conflict.name.clone()));
        }
        let changed = self.draft.folders[index].name != name;
        if changed {
            self.draft.folders[index].name = name.to_owned();
            self.mark_dirty();
        }
        self.cancel_folder_rename();
        Ok(changed)
    }

    pub fn cancel_folder_rename(&mut self) {
        self.pending_folder_rename = None;
        self.folder_rename_text.clear();
        self.folder_rename_needs_focus = false;
        self.folder_error = None;
    }

    pub fn request_delete_folder(&mut self, folder_id: u64) {
        self.cancel_folder_deletion();
        if let Some(folder) = self
            .draft
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
        {
            self.pending_delete_folder = Some(folder_id);
            let member_count = self
                .draft
                .macros
                .iter()
                .filter(|m| m.folder_id == Some(folder_id))
                .count();
            self.folder_delete_confirmation.open_custom(
                format!(
                    "Delete folder \"{}\"? Members: {member_count}.",
                    folder.name
                ),
                "Members will move to Unfiled. No macros will be deleted.",
            );
        }
    }

    /// Remove only the folder organization, preserving macros and editor selection.
    /// Missing folders (including stale confirmations) are a no-op.
    pub fn delete_folder(&mut self, folder_id: u64) -> bool {
        let Some(index) = self
            .draft
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
        else {
            return false;
        };
        for m in &mut self.draft.macros {
            if m.folder_id == Some(folder_id) {
                m.folder_id = None;
            }
        }
        self.draft.folders.remove(index);
        self.collapsed_folders.remove(&folder_id);
        if self.pending_folder_rename == Some(folder_id) {
            self.cancel_folder_rename();
        }
        self.mark_dirty();
        true
    }

    fn handle_folder_delete_confirmation(&mut self, result: ConfirmationResult) {
        match result {
            ConfirmationResult::Confirmed => {
                if let Some(id) = self.pending_delete_folder {
                    self.delete_folder(id);
                }
                self.cancel_folder_deletion();
            }
            ConfirmationResult::Cancelled => self.cancel_folder_deletion(),
            ConfirmationResult::None => {}
        }
    }

    fn cancel_folder_deletion(&mut self) {
        self.pending_delete_folder = None;
        self.folder_delete_confirmation = ConfirmationModal::default();
    }

    fn cancel_folder_operations(&mut self) {
        self.cancel_folder_rename();
        self.cancel_folder_deletion();
    }

    /// Preserve collapse state only for folders in the current draft, including
    /// during an external conflict where the dirty draft remains authoritative.
    fn prune_folder_ui_state(&mut self) {
        let folder_ids: HashSet<_> = self.draft.folders.iter().map(|folder| folder.id).collect();
        self.collapsed_folders.retain(|id| folder_ids.contains(id));
        if self
            .pending_folder_rename
            .is_some_and(|id| !folder_ids.contains(&id))
        {
            self.cancel_folder_rename();
        }
        if self
            .pending_delete_folder
            .is_some_and(|id| !folder_ids.contains(&id))
        {
            self.cancel_folder_deletion();
        }
    }

    fn apply_hotkey_scope_matcher(&mut self, macro_id: u64, matcher: MkWindowMatcher) -> bool {
        let changed = {
            let Some(macro_) = self.draft.macros.iter_mut().find(|m| m.id == macro_id) else {
                return false;
            };
            let MkHotkeyScope::ActiveWindow(current) = &mut macro_.hotkey_scope else {
                return false;
            };
            if *current == matcher {
                false
            } else {
                *current = matcher;
                true
            }
        };
        if changed {
            self.mark_dirty();
        }
        true
    }

    fn apply_window_picker_confirmation(
        &mut self,
        request: &window_picker::MatcherEditRequest,
        matcher: MkWindowMatcher,
    ) -> bool {
        match &request.destination {
            window_picker::MatcherDestination::Action { .. } => self
                .action_editor
                .apply_window_matcher(request, matcher, self.selected_macro_id),
            window_picker::MatcherDestination::MacroHotkey { macro_id } => {
                self.apply_hotkey_scope_matcher(*macro_id, matcher)
            }
        }
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
        self.cancel_folder_operations();
        self.prune_folder_ui_state();
        self.action_editor.cancel();
        self.open = false;
        self.set_selected_macro(None);
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
        self.cancel_folder_operations();
        self.prune_folder_ui_state();
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
        let inserted = crate::mkmacro::to_macro_steps(recorded, next, true);
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
    pub fn set_selected_macro(&mut self, id: Option<u64>) {
        self.selected_macro_id = id.filter(|id| self.draft.macros.iter().any(|m| m.id == *id));
        crate::mkmacro::runtime::set_recording_target(self.selected_macro_id);
    }
    pub fn selected_macro_mut(&mut self) -> Option<&mut MkMacro> {
        let id = self.selected_macro_id?;
        self.draft.macros.iter_mut().find(|m| m.id == id)
    }
    pub fn create_macro(&mut self) {
        self.create_macro_in_folder(None);
    }
    /// Create and select a macro, rejecting invalid destinations before changing the draft.
    pub fn create_macro_in_folder(&mut self, folder_id: Option<u64>) -> bool {
        if folder_id
            .is_some_and(|id| id == 0 || !self.draft.folders.iter().any(|folder| folder.id == id))
        {
            return false;
        }
        self.draft.macros.push(MkMacro {
            id: 0,
            name: "New Macro".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id,
            playback: Default::default(),
            steps: vec![],
            image_assets: vec![],
        });
        repair_ids(&mut self.draft);
        self.set_selected_macro(self.draft.macros.last().map(|m| m.id));
        self.selection.clear();
        self.mark_dirty();
        true
    }
    /// Clone in the same folder and append in global document order, with fresh IDs
    /// and no hotkey on the copy. The source macro remains unchanged.
    pub fn duplicate_selected_macro(&mut self) {
        let Some(source) = self.selected_macro() else {
            return;
        };
        let mut copy = source.clone();
        copy.id = 0;
        copy.name.push_str(" Copy");
        copy.hotkey = None;
        for step in &mut copy.steps {
            step.id = 0;
        }
        debug_assert_eq!(
            copy.folder_id, source.folder_id,
            "cloning preserves folder membership"
        );
        self.draft.macros.push(copy);
        repair_ids(&mut self.draft);
        self.set_selected_macro(self.draft.macros.last().map(|m| m.id));
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
        // Child editor requests capture the selected macro ID. Discard them
        // before that target disappears so they cannot later apply to a stale
        // draft (especially when the final macro is removed).
        self.action_editor.cancel();
        self.launcher_action_picker.cancel();
        self.draft.macros.remove(index);
        let selected = self
            .draft
            .macros
            .get(index)
            .or_else(|| index.checked_sub(1).and_then(|i| self.draft.macros.get(i)))
            .map(|m| m.id);
        self.set_selected_macro(selected);
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

    fn prepare_execution_checked(&mut self) -> anyhow::Result<u64> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        self.prepare_execution()
    }

    fn step_position(&self, step_id: u64) -> anyhow::Result<usize> {
        self.selected_macro()
            .and_then(|m| m.steps.iter().position(|s| s.id == step_id))
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists"))
    }

    fn step_id_at_position(&self, position: usize) -> anyhow::Result<u64> {
        self.selected_macro()
            .and_then(|m| m.steps.get(position))
            .map(|step| step.id)
            .ok_or_else(|| anyhow::anyhow!("Selected step no longer exists after save"))
    }

    fn step_ids_at_positions(&self, positions: &[usize]) -> Vec<u64> {
        positions
            .iter()
            .filter_map(|position| {
                self.selected_macro()
                    .and_then(|m| m.steps.get(*position))
                    .map(|step| step.id)
            })
            .collect()
    }

    fn prepare_from_step(&mut self, original_step_id: u64) -> anyhow::Result<(u64, u64)> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let position = self.step_position(original_step_id)?;
        let macro_id = self.prepare_execution()?;
        let step_id = self.step_id_at_position(position)?;
        Ok((macro_id, step_id))
    }

    fn selected_step_positions(&self) -> anyhow::Result<Vec<usize>> {
        let positions = self
            .selected_macro()
            .map(|m| {
                m.steps
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| self.selection.ids.contains(&s.id).then_some(i))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if positions.is_empty() {
            anyhow::bail!("Select one or more steps");
        }
        Ok(positions)
    }

    fn prepare_selected_steps(&mut self) -> anyhow::Result<(u64, Vec<u64>)> {
        if let Some(reason) = self.playback_block_reason() {
            anyhow::bail!(reason);
        }
        let positions = self.selected_step_positions()?;
        let macro_id = self.prepare_execution()?;
        let step_ids = self.step_ids_at_positions(&positions);
        Ok((macro_id, step_ids))
    }

    pub fn run_selected_macro(&mut self) -> anyhow::Result<()> {
        let id = self.prepare_execution_checked()?;
        crate::mkmacro::runtime::run(id)
    }
    pub fn run_from_step(&mut self, original_step_id: u64) -> anyhow::Result<()> {
        let (id, step) = self.prepare_from_step(original_step_id)?;
        crate::mkmacro::runtime::run_from(id, step)
    }
    pub fn run_selected_steps(&mut self) -> anyhow::Result<()> {
        let (id, ids) = self.prepare_selected_steps()?;
        crate::mkmacro::runtime::run_selection(id, ids)
    }
    pub fn debug_selected_macro(&mut self) -> anyhow::Result<()> {
        let id = self.prepare_execution_checked()?;
        crate::mkmacro::runtime::debug_run(id)
    }
    pub fn debug_from_step(&mut self, original_step_id: u64) -> anyhow::Result<()> {
        let (id, step) = self.prepare_from_step(original_step_id)?;
        crate::mkmacro::runtime::debug_run_from(id, step)
    }
    pub fn debug_selected_steps(&mut self) -> anyhow::Result<()> {
        let (id, ids) = self.prepare_selected_steps()?;
        crate::mkmacro::runtime::debug_run_selection(id, ids)
    }

    fn observe_runtime_snapshot(&mut self, snapshot: Option<Arc<crate::mkmacro::RuntimeSnapshot>>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let run_identity = (snapshot.run_mode, snapshot.run_id);
        let new_run = self.runtime_inspector_observed_run != Some(run_identity);
        self.runtime_inspector_observed_run = Some(run_identity);
        match snapshot.run_mode {
            crate::mkmacro::RuntimeRunMode::Normal => {
                if new_run && self.runtime_inspector_snapshot.is_some() {
                    self.runtime_inspector_snapshot = None;
                    self.runtime_inspector_is_current_debug_run = false;
                    self.runtime_inspector_open = false;
                    self.runtime_inspector_active_breakpoint = None;
                }
            }
            crate::mkmacro::RuntimeRunMode::Debug => {
                let same_debug_run = self.runtime_inspector_snapshot.as_ref().is_some_and(|old| {
                    old.run_mode == crate::mkmacro::RuntimeRunMode::Debug
                        && old.run_id == snapshot.run_id
                });
                if !same_debug_run {
                    self.runtime_inspector_active_breakpoint = None;
                }
                // Retain the whole immutable runtime snapshot. In particular,
                // this preserves terminal variables, outcomes, and diagnostics
                // after the worker has moved on to unrelated UI repainting.
                self.runtime_inspector_snapshot = Some(snapshot.clone());
                self.runtime_inspector_is_current_debug_run = matches!(
                    snapshot.state,
                    crate::mkmacro::RuntimeState::Running
                        | crate::mkmacro::RuntimeState::Paused
                        | crate::mkmacro::RuntimeState::Stopping
                );
                let breakpoint = if snapshot.state == crate::mkmacro::RuntimeState::Paused {
                    match snapshot.pause_reason {
                        Some(crate::mkmacro::RuntimePauseReason::Breakpoint { step_id }) => {
                            Some((snapshot.run_id, step_id))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(breakpoint) = breakpoint {
                    if self.runtime_inspector_active_breakpoint != Some(breakpoint) {
                        self.runtime_inspector_open = true;
                        self.runtime_inspector_active_breakpoint = Some(breakpoint);
                    }
                } else {
                    self.runtime_inspector_active_breakpoint = None;
                }
            }
        }
    }

    pub fn ui(&mut self, ctx: &eframe::egui::Context) {
        self.sync_external();
        if !self.open {
            self.close_children();
            self.set_selected_macro(None);
            return;
        }
        let mut open = self.open;
        // Manual regression: resize this window, add 100 steps, expand/edit rows,
        // and delete them again. Its chosen outer height must remain unchanged.
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
            self.set_selected_macro(None);
        }
    }
    fn close_children(&mut self) {
        self.cancel_folder_operations();
        self.action_catalog_visible = false;
        self.action_editor.cancel();
        self.structural_insertion = None;
        self.window_picker
            .cancel("Window picker closed because the macro dialog closed");
        self.launcher_action_picker.cancel();
    }
    pub fn show_contents(&mut self, ui: &mut eframe::egui::Ui) {
        self.observe_runtime_snapshot(crate::mkmacro::runtime::snapshot());
        for result in crate::mkmacro::runtime::take_pending_recordings() {
            if self
                .apply_recording(result.macro_id, &result.generated_steps)
                .is_err()
            {
                self.pending_recording = Some((result.macro_id, result.generated_steps));
                self.command_error = Some(
                    "Recording target was deleted; captured actions were preserved for recovery"
                        .into(),
                );
            }
        }
        toolbar::show(ui, self);
        if self.draft.macros.is_empty() && self.draft.folders.is_empty() {
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
                        runtime_inspector::show(ui, self);
                        ui.separator();
                        step_table::show(ui, self);
                    });
                });
        }
        action_catalog::show_modal(ui.ctx(), self);
        action_editor::show(ui.ctx(), self);
        launcher_action_picker::show(ui.ctx(), self);
        window_picker::show(ui.ctx(), &mut self.window_picker);
        if self.window_picker.confirm_ready {
            self.window_picker.confirm_ready = false;
            if let Some((request, matcher)) = self.window_picker.take_confirmation() {
                let applied = self.apply_window_picker_confirmation(&request, matcher);
                if !applied {
                    self.command_error =
                        Some("The matcher target no longer exists; no changes were made".into());
                }
            }
        }
        if self.delete_confirmation.ui(ui.ctx()) == ConfirmationResult::Confirmed {
            self.delete_selected_macro();
        }
        let folder_delete_result = self.folder_delete_confirmation.ui(ui.ctx());
        self.handle_folder_delete_confirmation(folder_delete_result);
        match self.unwrap_confirmation.ui(ui.ctx()) {
            ConfirmationResult::Confirmed => {
                if let Some(id) = self.pending_unwrap_block.take() {
                    self.pending_unwrap_selection = None;
                    step_table::apply_confirmed_unwrap(self, id);
                }
            }
            ConfirmationResult::Cancelled => {
                self.pending_unwrap_block = None;
                if let Some(selection) = self.pending_unwrap_selection.take() {
                    self.selection = selection;
                }
            }
            ConfirmationResult::None => {}
        }
    }
}
