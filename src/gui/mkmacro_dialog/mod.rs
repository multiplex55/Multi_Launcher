pub mod action_catalog;
pub mod action_editor;
pub mod condition_editor;
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
mod step_table;
mod toolbar;
pub mod uia_editor;
pub mod visual_capture_workflow;
pub mod visual_overlay;
pub mod window_picker;

use crate::gui::confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use crate::mkmacro::{
    DiagnosticSeverity, MkMacro, MkMacroDocument, MkMacroStore, NormalizationConfig, RecordedStep,
    repair_ids, validate_document,
};
use std::sync::Arc;
pub use step_table::{Selection, duplicate_steps, duplicate_steps_with_ids, move_steps};
use visual_capture_workflow::SharedVisualOverlayController;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyDecision {
    KeepEditing,
    Discard,
}

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        AlphaPolicy, LoadDisposition, MKMACROS_FILE, MkAction, MkCondition, MkCoordinateTarget,
        MkHotkey, MkImageNotFoundPolicy, MkImageOutputs, MkImagePayload, MkKey, MkMouseButton,
        MkMouseMovePayload, MkMousePayload, MkMouseScrollAxis, MkStep, MkWaitOptions, ReturnPoint,
        SCHEMA_VERSION, SearchRegion,
    };
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    fn five_macros() -> MkMacroDocument {
        MkMacroDocument {
            settings: Default::default(),
            schema_version: SCHEMA_VERSION,
            macros: (1..=5)
                .map(|id| MkMacro {
                    id,
                    name: format!("Macro {id}"),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
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
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::RepeatStart { count: 2 },
            },
            MkStep {
                id: 12,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::ImageFind(payload.clone()),
            },
            MkStep {
                id: 13,
                enabled: true,
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
            action_catalog::insert_action(&mut d, MkAction::Delay { milliseconds });
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
                    | MkAction::VirtualDesktop(_)
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
    fn virtual_desktops_are_direct_windows_actions() {
        let descriptors = action_catalog::descriptors();
        let rows: Vec<_> = descriptors
            .iter()
            .filter(|descriptor| matches!((descriptor.make_default)(), MkAction::VirtualDesktop(_)))
            .collect();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|descriptor| {
            descriptor.category == action_catalog::ActionCategory::Windows
                && descriptor.editor == action_catalog::EditorKind::DirectInsert
        }));
        assert_eq!(
            rows.iter()
                .map(|descriptor| descriptor.name)
                .collect::<Vec<_>>(),
            [
                "Create Virtual Desktop",
                "Switch Virtual Desktop Left",
                "Switch Virtual Desktop Right",
                "Close Current Virtual Desktop",
            ]
        );
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
                if self.selected_macro().is_none() {
                    self.set_selected_macro(None);
                }
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
        self.draft.macros.push(MkMacro {
            id: 0,
            name: "New Macro".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps: vec![],
            image_assets: vec![],
        });
        repair_ids(&mut self.draft);
        self.set_selected_macro(self.draft.macros.last().map(|m| m.id));
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
        self.action_catalog_visible = false;
        self.action_editor.cancel();
        self.structural_insertion = None;
        self.window_picker
            .cancel("Window picker closed because the macro dialog closed");
        self.launcher_action_picker.cancel();
    }
    pub fn show_contents(&mut self, ui: &mut eframe::egui::Ui) {
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
        launcher_action_picker::show(ui.ctx(), self);
        window_picker::show(ui.ctx(), &mut self.window_picker);
        if self.window_picker.confirm_ready {
            self.window_picker.confirm_ready = false;
            if let Some((request, matcher)) = self.window_picker.take_confirmation() {
                if !self.action_editor.apply_window_matcher(
                    &request,
                    matcher,
                    self.selected_macro_id,
                ) {
                    self.command_error =
                        Some("The matcher target no longer exists; no changes were made".into());
                }
            }
        }
        if self.delete_confirmation.ui(ui.ctx()) == ConfirmationResult::Confirmed {
            self.delete_selected_macro();
        }
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
