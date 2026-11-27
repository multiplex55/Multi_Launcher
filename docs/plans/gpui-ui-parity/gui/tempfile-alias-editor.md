# Temporary File Alias

G06 owns `tempfile-alias-editor`. A current `tmp list` result's Set Alias action opens the root Temporary File Alias form. The direct `tmp alias` command is separate.

| Criterion | Source-backed behavior |
| --- | --- |
| TEMPFILE-ALIAS-01 | Preserve the exact source, initial stem, unchanged extension, empty/invalid name and collision checks, then atomic rename within the configured store. Cancel leaves path and content intact. |

Select `long_tempfile_target_keeps_actions_reachable_without_render_mutation` and `each_target_gets_primary_and_current_specialized_capabilities`. Use an explicit scratch `ML_TMP_DIR`.

`G06-EGUI-FOLDER-TEMPFILE-ALIAS-DRAFTS` verifies the native egui current path, initial alias, editing, and Cancel with filename and contents preserved. GPUI implements the exact list-to-form entry, stock prefill, checked Save and Cancel for ordinary local text files. Paired Windows validation and rename replay remain open. [Retained Alias checkpoint](../CHECKPOINTS.md#p016-retained-temporary-file-alias)
