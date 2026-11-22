# Mouse/Keyboard Macros

G19 owns `mkmacro`. Exact `mkmacro` emits `mkmacro:dialog` and opens the retained authoring manager. Named run results are separate typed commands. `MkMacroDialog` owns the editable `MkMacroDocument`, selections, revision and conflict state. `MkMacroStore`, compiler and runtime retain canonical persistence, validation and execution authority.

| Criterion | Source-backed behavior |
| --- | --- |
| MKMACRO-01 | Preserve list/folder grouping, filter/collapse, new, duplicate, delete, move and rename. New and duplicate are draft-only and receive stable fresh identities. Duplicate clears copied hotkey. Preserve macro and step selection through current typed edits. |
| MKMACRO-02 | Preserve properties, step table and scoped editing, multiselection, structural folds, diagnostics, + Action → [Action Catalog](mkmacro-action-catalog.md) and direct [Action Editor](mkmacro-action-editor.md). Execution and recording remain explicit separate operations. |
| MKMACRO-03 | Save validates and publishes current draft through canonical storage. Clean external refresh updates baseline, dirty conflict remains visible. Close prompts Save/Discard/Cancel. Cancel retains, Discard reloads current store/retire. |
| MKMACRO-04 | Preserve accepted owner, retained draft, child scope and stable macro/step generations across Dock/Pop Out. A retired frame or authority cannot deliver stale mutations or execute. |

Select `create_duplicate_and_delete_are_draft_only`, `new_folder_marks_draft_dirty_without_changing_saved_document`, `moving_macro_changes_only_folder_and_retains_macro_and_step_selection` and `minimum_size_main_layout_keeps_toolbar_and_steps_reachable_without_mutation`. `G19-EGUI-MKMACRO-DELAY-DRAFT` verifies native manager, unsaved draft, Catalog/Action Editor route and Discard. Save/conflict, recording/playback, host and GPUI remain pending.
