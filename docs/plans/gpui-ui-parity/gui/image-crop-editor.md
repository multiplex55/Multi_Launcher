# Crop Image

G18 owns `image-crop-editor`. `crop image` or `crop file` emits `crop:image` and opens the file picker. The separate screenshot crop route uses the admitted region/capture operation before converging on the same canvas. `CropDialogState` owns source and save intent. `image_crop` owns exact RGBA source-space slicing.

| Criterion | Source-backed behavior |
| --- | --- |
| CROP-IMAGE-01 | File picker admits supported PNG/JPEG/BMP, validates actual extension and decodes successfully before replacing the editor. Cancel preserves prior state. Screenshot source uses the exact current capture operation and source rectangle. |
| CROP-IMAGE-02 | Show source, dimensions and selection. Start at full image; support move, eight normalized/clamped handles, create, zoom, pan, Reset and Select Full Image. Preserve exact source-space pixels when zoom or host changes. |
| CROP-IMAGE-03 | Save As offers the source-based name or screenshot name and supported conversion. Existing path and existing-file Overwrite Current stage Confirm crop overwrite for the exact path. Cancel/X preserve source and draft. Explicit confirmation writes the current accepted crop and closes only on success. The current writer uses direct encode/write after collision detection. |
| CROP-IMAGE-04 | Keep reachable canvas and scrolling metadata/footer. Retain current editor on write/encode failure. GPUI acceptance must verify actual source/picker/decision admission and reject duplicate or retired gestures. |

Select `crop_queries_return_stable_gui_action_ids`, `crop_routes_through_the_typed_host_and_keeps_history_skipped`, `cancelled_crop_picker_result_keeps_dialog_closed`, `crop_has_exact_dimensions_and_preserves_source_pixels`, `zoom_is_presentation_only_and_does_not_change_stored_selection`, `crop_chrome_bounds_long_metadata_and_keeps_canvas_and_actions_reachable`, `collision_is_reported_before_destination_changes` and `invalid_existing_context_does_not_leave_an_empty_dialog_open`. `G18-EGUI-CROP-IMAGE-PICKER` verifies actual picker delivery, dimensions and overwrite Cancel with exact file bytes. Pointer handles, accepted output, screenshot route and GPUI remain pending.
