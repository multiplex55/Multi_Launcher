# MkMacro Reference Crop

G19 owns `mkmacro-reference-crop`. A current [Action Editor](mkmacro-action-editor.md) Reference Image Crop opens for a managed image action or nested image condition. The crop host owns pending image load, canvas and draft filename. The asset store owns managed writes, while the exact bound action field owns retargeting.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-REF-CROP-01 | Admit exact current macro/step/draft/typed condition path and original managed ref. Async source read discards retired results. Keep source-pixel rectangle, bounds, zoom/pan, alpha and retained Dock/Pop Out. |
| MK-REF-CROP-02 | Save As validates safe managed PNG name and exact crop; collision retains editor. Retarget only current valid destination after successful asset publication. A stale retarget may leave the published asset. |
| MK-REF-CROP-03 | Confirm Overwrite Current warns existing references share the asset, keeps current path/draft identity and invalidates preview cache after write. Cancel/X preserves asset/canvas. Require current owner, crop epoch and destination before dispatch. |

Select `stale_native_crop_save_never_writes_asset`, `image_find_save_as_crop_updates_reference_and_marks_draft_changed`, `overwrite_crop_does_not_mutate_reference_or_mark_draft_changed`, `condition_crop_destination_routes_all_image_condition_action_hosts`, `stale_nested_condition_save_as_leaves_tree_unchanged_and_asset_persisted` and `save_as_crop_persists_exact_pixels_and_rejects_collision_without_mutation`. Native and GPUI remain pending.
