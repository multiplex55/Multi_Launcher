# Diagram Viewer

G17 owns `diagram-viewer`. Current [Diagram Studio](diagram-studio.md) Open Viewer or the Visual View menu creates or focuses the distinct viewer. The child owns host generation, sizing, scope, camera and native lifecycle. Preview data comes from the authoritative Studio document.

| Criterion | Source-backed behavior |
| --- | --- |
| DIAGRAM-VIEW-01 | Observe the exact current Mermaid or Visual document and revision, pending state, errors and last good preview. Share the bounded render cache. Visual mode is read-only. Keep separate zoom, pan, minimap, Fit and 100% controls. |
| DIAGRAM-VIEW-02 | Preserve independent mode/geometry, eligible parent promotion, and scoped owner, viewport, keyboard and pointer roles. Native viewer can stay live while a docked root is hidden. Fit follows resize until manual zoom. |
| DIAGRAM-VIEW-03 | Accept current open/drag/focus/close only through exact retained parent and child receipt. Child Close retains Studio. Canceled parent dirty close retains the viewer. Accepted parent close retires it. Reject stale document, transition or simultaneous mutation input. |

Select `studio_usability_diagram_viewer_uses_a_distinct_callback_and_independent_camera`, `studio_usability_diagram_viewer_discard_close_and_reopen_are_generation_scoped`, `studio_usability_diagram_viewer_same_revision_document_switch_updates_identity`, `studio_usability_diagram_viewer_generic_resize_and_camera_share_one_render_result`, `diagram_viewer_jump_has_its_own_docked_and_native_owner` and `diagram_viewer_preview_drag_rejects_stale_discarded_and_transition_effects`. `G17-EGUI-DIAGRAM-VIEWER-DRAFT` covers native viewer entry, independent zoom, retained camera after parent Cancel and scoped child Close. Canvas pixels, detached host, pointer input and GPUI remain pending.
