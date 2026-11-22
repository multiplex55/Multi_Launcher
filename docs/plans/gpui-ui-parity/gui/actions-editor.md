# App Editor

G06 owns `actions-editor`. File → Apps → Edit Apps opens the root App Editor. The current custom action store and indexed actions share its view.

| Criterion | Source-backed behavior |
| --- | --- |
| APP-EDITOR-01 | Preserve label, description and command filters and New/Edit/Remove/Repair targeting. Only the customizable portion may be mutated. Persist before publishing and retain the indexed tail. Remove is immediate in the current form. |

Select `constrained_editor_and_add_form_keep_full_commands_and_footer_reachable_without_publication`, `failed_custom_action_save_retains_published_state_and_version` and `target_repair_transaction_publishes_committed_custom_rows_and_preserves_indexed_tail`.

`G06-EGUI-PLUGIN-APP-EDITORS` verifies the native File menu route, child opening and continued parent after Cancel. `G06-EGUI-CUSTOM-ACTION-SAVE` verifies rejection of an empty path without a store change, persistence of one custom row, and the reopened `app` search result. Editing, Remove, Repair and GPUI remain pending.
