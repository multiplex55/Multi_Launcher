# Diagram Studio

G17 owns `diagram-studio`. Exact `diagram studio` emits `diagram:studio` and reaches `DialogCommand::DiagramStudio`. `DiagramDocumentController` owns the active Mermaid or Visual document, file baseline and transitions. The existing diagram session and canvas own source, graph/scene, transactions and history.

| Criterion | Source-backed behavior |
| --- | --- |
| DIAGRAM-01 | Preserve fresh Mermaid opening, current-document refocus, File/Edit/View/Templates/Help, Source/Split/Visual preview and distinct native Visual authoring. Keep suggestions, colors, inspectors, scroll and keyboard focus. |
| DIAGRAM-02 | Mermaid editing preserves exact invalid or unsupported source, labels the last good preview as stale and gates dependent gestures/exports. Visual editing preserves scene selection, tools, hit tests, snapping, zoom, marquee, drag, resize, labels, connectors and exact undo/redo. Escape cancels the current gesture before requesting close. |
| DIAGRAM-03 | Preserve exact document/owner Save/Discard/Cancel for New/Open/Import/Copy/Template/Close. Save As picker cancellation retains the current decision. Preserve canonical persistence, exporter and recent files. Discard delivers only the pending original request. |
| DIAGRAM-04 | Retain appropriate Dock/Pop Out and pin semantics across accepted host changes. [Diagram Viewer](diagram-viewer.md) uses the same current authoritative document and its own camera/host. |
| DIAGRAM-05 | A Note or Markdown fence hands off its bounded exact source and accepted origin. Use source in Markdown returns the current Mermaid text only to that still-current fence and applies one undoable source edit. Source replacement, retired origin or blocked owner rejects stale return. |

Select `diagram_studio_has_one_exact_typed_route`, `diagram_document_source_copy_is_in_memory_bounded_and_uses_dirty_decisions`, `diagram_studio_invalid_source_retains_last_good_preview_and_blocks_visual_edits`, `diagram_studio_visual_canvas_discarded_release_keeps_document_unchanged`, `diagram_studio_visual_canvas_escape_cancels_owned_gesture_before_close`, `diagram_studio_os_viewport_close_requests_cancel_until_dirty_decision`, `markdown_studio_diagram_source_handoff_replaces_exact_fence_as_one_undo` and `markdown_studio_diagram_source_handoff_rejects_changed_origin_source`. `G17-EGUI-DIAGRAM-VIEWER-DRAFT` covers actual Mermaid draft, preview readiness, child route and dirty Cancel/Discard. Native visual canvas, I/O, Markdown return and GPUI remain pending.
