# Export / Print

G12 owns `markdown-print`, titled Export / Print. [Markdown Studio](markdown-studio.md) and [Note Editor](note-editor.md) open the child using an accepted current document/host receipt. `src/gui/markdown_print/` owns presentation and worker admission. `notes_markdown::print_layout` owns prepared layout.

| Criterion | Source-backed behavior |
| --- | --- |
| MD-PRINT-01 | Show paper, orientation, margins, font sizes and optional title/header/footer. Preview reports pages, warnings and errors. Next/Previous, Fit and explicit zoom reuse the current scene. Superseded workers and scene revisions cannot overwrite the current preview. |
| MD-PRINT-02 | Save PDF uses the native owner and a reviewed target. A file replacement needs explicit Replace PDF/Cancel. Reject the active source path or alias and preserve exact source/target identity through output admission. |
| MD-PRINT-03 | The owned Windows printer chooser supports the allowed page ranges and a single copy. Current printer/layout review reports scaling and possible clipping. Submit print job is a separate, exact approval. Preserve cancellation and worker outcomes. |
| MD-PRINT-04 | Preserve Dock/Pop Out and the exact live source owner through child refresh. Changes to source, settings or host retire stale scene/output approvals. A retired document closes its child. |

Select `markdown_print_studio_opens_from_same_retained_frame_as_draft_edit`, `markdown_print_native_child_keeps_root_ui_alive_when_launcher_hidden`, `markdown_print_host_retires_draft_and_reuses_current_scene_for_page_and_zoom`, `markdown_print_pdf_chooser_requires_exact_current_scene_and_cancel_keeps_output_idle`, `markdown_print_pdf_writes_new_and_waits_for_exact_replacement`, `printer_chooser_result_gate_and_owner_validation` and `markdown_print_printer_reserves_output_until_exact_approval_or_retirement` for the applicable boundaries. `G12-EGUI-MARKDOWN-PREVIEW` verifies the live Studio handoff, one-page preview and close retaining the parent draft. The Note Editor route, native output and choosers, host lifecycle and GPUI remain pending.
