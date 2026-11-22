# Screenshot Editor

G18 owns `screenshot-editor`. An accepted capture opens the retained annotation utility when the configured editor policy permits it. The capture/delivery owner supplies RGBA and destination context. The editor owns markup, crop, history and explicit output.

| Criterion | Source-backed behavior |
| --- | --- |
| SHOT-EDIT-01 | Preserve captured source, requested starting Pen/Rectangle, supported Pen/Arrow/Rectangle/Highlight/Marker/Text/Redact tools, color/thickness, text size, zoom, crop and reachable controls. Existing undo/redo and shortcuts operate on the current markup. |
| SHOT-EDIT-02 | Compose markup in source pixels, then crop and output exact result. Preserve Solid/Blur/Pixelate bounds and settings, current preview/history identity and failure behavior. Display zoom cannot change export pixels. |
| SHOT-EDIT-03 | Explicit Save uses configured destination or selected PNG path with cancellation. Explicit Copy uses the approved edited RGBA and current extra-file preference. Close releases textures and has no implicit output. |

Select `screenshot_redaction_drag_captures_effects_and_history_shortcuts`, `screenshot_redaction_apply_edits_crops_after_effect_and_saved_png_matches`, `screenshot_marker_canvas_click_is_single_shot_zoomed_and_matches_preview_export`, `screenshot_redaction_render_error_never_displays_stale_pixels_or_retries_idle` and `screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image`. Existing generated RGBA `ScreenshotEditor::new` fixtures avoid desktop capture. Native and GPUI evidence are pending.
