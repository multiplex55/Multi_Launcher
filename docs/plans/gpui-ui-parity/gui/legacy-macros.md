# Legacy Macros

G19 owns `legacy-macros`. Exact bare `macro` offers `macro: edit macros` and emits typed `macro:dialog`. Named run results use the separate saved macro execution path. `MacroDialog` owns the root panel and draft. The legacy Macros plugin owns the working-directory `macros.json` and snapshot publication.

| Criterion | Source-backed behavior |
| --- | --- |
| LEGACY-MACRO-01 | Show current Label/Description rows and Add/Edit/Remove/Close. Edit retains labels, description, automatic or per-step delay, arguments, step order and category/action query. Capturing authored actions changes only the local draft. |
| LEGACY-MACRO-02 | Save validates label and args, publishes through the existing transaction and atomic writer, refreshes search and accepts stored state. Explicit Remove uses the existing list mutation. Malformed storage keeps last-good state read-only. Cancel preserves bytes. |
| LEGACY-MACRO-03 | Preserve root geometry/pinning and existing reload semantics. Pinned open keeps the current editable state. Bare typed opener reloads the snapshot and resets edit/filter. |

Select `constrained_legacy_macro_preserves_pinned_draft_and_keeps_long_editor_reachable`, `invalid_reload_and_commit_keep_last_good_and_read_only`, `legacy_macro_contract_remains_unchanged`, `typed_states_defaults_and_legacy_filename_remain_compatible` and `malformed_store_rejects_update_and_full_replacement_unchanged`. `G19-EGUI-LEGACY-DRAFT-RECORDER-SETUP` verifies native exact stored row, edit and Cancel with identical bytes. Save/remove, execution and GPUI remain pending.
