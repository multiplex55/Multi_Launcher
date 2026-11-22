# Image Viewer

G18 owns `image-viewer`. The [Note Editor](note-editor.md) preview image/linked-asset action opens the exact verified file through the accepted note owner. `ImagePanel` owns the per-file session, texture and zoom. `ImageWindowHost` owns independent Dock/Pop Out and current viewport.

| Criterion | Source-backed behavior |
| --- | --- |
| IMAGE-VIEW-01 | Resolve accepted attachment path and open recognizable images. Deduplicate the same current path while allowing different files. Lazy decode retains actionable failed-load feedback. |
| IMAGE-VIEW-02 | Preserve filename/path, content zoom and two-axis scrolling. Open in Default Viewer performs the external action only for the exact accepted owner. Close/Escape retires the correct session and texture. |
| IMAGE-VIEW-03 | Inherit eligible parent presentation through an accepted live note receipt. Preserve independent host and per-file zoom. Retired source and stale or wrong owner cannot retarget another file. |

Select `retained_note_image_open_inherits_and_rejects_retired_source`, `image_viewer_jump_uses_foreground_session_without_activation`, `note_auxiliary_hosts_reuse_instances_and_parent_presentation`, `image_panel_closes_with_escape` and `parse_note_image_target_handles_assets_and_width_suffixes`. `G18-EGUI-NOTE-IMAGE-VIEWER` verifies the native preview link opens the exact synthetic PNG. Active zoom, external viewer, native host transitions and GPUI remain pending.
