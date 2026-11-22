# MkMacro Action Editor

G19 owns `mkmacro-action-editor`. A current step edit or configurable [Action Catalog](mkmacro-action-catalog.md) selection opens the retained action editor. `ActionEditorState` owns a typed local action draft and existing child pickers/test effects. The existing macro editor mutation applies accepted changes to the bound draft.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-EDIT-01 | Preserve action-specific controls and common metadata, initial fields and validation. Existing-step edit retains exact step identity and generation. New action retains exact insertion anchor or structural selection. |
| MK-EDIT-02 | Apply checks exact accepted parent, owner, epoch, current draft and valid step/anchor. Atomic structural insertion uses the existing mutation owner. Apply mutates only the current macro draft. Cancel/close retires local work. |
| MK-EDIT-03 | Preserve independent Dock/Pop Out, draft and current owner/recording review receipts. Retire superseded child picker/test outcomes. |

Select `configurable_catalog_selection_is_transactional_and_keeps_catalog_state`, `cancelling_catalog_editor_preserves_macro_and_catalog`, `live_step_editor_reopens_from_native_parent_without_replacing_draft`, `mkmacro_detached_action_editor_finishes_only_on_retained_clicks`, `apply_uses_captured_macro_identity_not_current_selection` and `stale_replacement_and_invalid_reusable_draft_remain_transactional`. `G19-EGUI-MKMACRO-DELAY-DRAFT` verifies actual Delay editor, Fixed/Random range controls and Cancel. Apply/other variants, native rehost and GPUI remain pending.
