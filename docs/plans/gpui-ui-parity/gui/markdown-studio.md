# Markdown Studio

G12 owns `markdown-studio`. The `markdown_studio` plugin accepts the exact trimmed, case-insensitive `markdown studio` query, emits `markdown:studio` and dispatches `DialogCommand::MarkdownStudio`. `src/gui/markdown_studio.rs` owns host, caret and active view. `MarkdownDocument` owns source, path, disk baseline, bounded history and dirty transitions.

| Criterion | Source-backed behavior |
| --- | --- |
| MD-STUDIO-01 | Show Edit, Preview and Split, with wide columns and narrow stacking. Preserve caret byte range, editable source and source revision. Apply same-frame source edits before formatting or publication. Use source-keyed preview and bounded local raster assets. |
| MD-STUDIO-02 | Preserve formatting and insertion actions for emphasis, strike, code, links, images, headings, lists, tasks, quotes, fences and tables. Table edits address the intended cells and dimensions. Outline navigates and stages revision-bound moves with confirmation. |
| MD-STUDIO-03 | New, Open and Close respect dirty Save/Discard/Cancel. Native Open and Save As use the accepted owner. Save conflicts or canceled pickers preserve the draft. Undo/redo is scoped to the current document. |
| MD-STUDIO-04 | Export Markdown or Static HTML from the current source without consuming the draft or history. Refuse active or occupied destinations. Export / Print opens the [shared child](markdown-print.md). Diagram editing and return use the exact fence identity and current source; a valid return applies one history step. |
| MD-STUDIO-05 | Keep remembered Dock/Pop Out, host and document epochs, root/owner frame receipts and hidden-root bootstrap. Retire actions from a replaced host or document. |

Select `markdown_studio_has_one_exact_typed_route`, `markdown_studio_query_enter_opens_an_empty_clean_source_editor`, `markdown_studio_visible_host_buttons_pop_out_and_dock_the_same_document`, `markdown_studio_format_toolbar_preserves_selected_unicode_source_range`, `markdown_document_dirty_transitions_cancel_discard_and_reread_open_after_save`, `outline_move_review_fences_revision_and_requires_confirmation`, `markdown_studio_export_publication_preserves_dirty_document_and_history` and `markdown_studio_diagram_source_handoff_rejects_changed_origin_source`. `G12-EGUI-MARKDOWN-PREVIEW` verifies the real launcher route, unsaved source, Preview and Split, the publication child and explicit Discard. Editor actions, native files and host lifecycle, diagram round-trip and GPUI remain pending.
