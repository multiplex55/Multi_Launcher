# Radial Designer

G20 owns `radial-designer`. `radial edit` and the File → Apps authoring route open Menus. `radial skins` opens Resources in the same Designer. The deferred Windows viewport owns rendering and exact widget state. `DesignerIntentBridge`, `RadialAuthoringSession` and `RadialStore` own accepted intents, authoritative draft and canonical publication.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-DESIGNER-01 | Wait for authoritative source before authoring. Preserve stable current tree/search/reveal/navigation, canvas ring/cell selection and inspector actions, responsive layout and independent display preferences. Reopening focuses the same live draft. |
| RADIAL-DESIGNER-02 | Scope typed edit and undo/redo to current draft and ID. Preserve exact [Cell Properties](radial-cell-properties.md), [Add to radial](radial-add-to-menu.md), [Skin Editor](radial-skin-editor.md) and [Asset Picker](radial-asset-picker.md) handoffs. |
| RADIAL-DESIGNER-03 | Apply publishes the exact current draft and updates the baseline. Save publishes and closes. Cancel/OS close respects dirty decision and checked revert of Apply where available. Reject stale/disk-conflicting work and retain required publication receipts through close. |

Select `designer_viewport_identity_and_duplicate_open_reuse_one_draft`, `closed_show_deferred_frames_skip_catalog_and_real_retained_callback_drains_close`, `apply_resets_checkpoint_save_closes_and_dirty_close_prompts`, `dirty_external_publish_conflicts_clean_publish_refreshes`, `cancel_after_apply_requests_checked_revert_of_last_apply` and `resetting_designer_layout_does_not_touch_dirty_document_or_selection`. `G20-EGUI-RADIAL-AUTHORING-ROUTES` covers both native route entries and clean window retirement. Child contents, editing, save, runtime and GPUI remain pending.
