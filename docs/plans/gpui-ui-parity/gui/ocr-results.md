# OCR Results

G22 owns `ocr-results`, the Screen Region OCR utility with the transient recognition generation. It exposes progress, empty, error and editable multiline result states.

| Criterion | Source-backed behavior |
| --- | --- |
| OCR-RESULT-01 | Restore the accepted editor and focus state after recognition. Copy All uses exactly the current nonblank edited text through explicit protected clipboard action. The surface has no clipboard operation on render or reopen. |
| OCR-RESULT-02 | Re-capture follows normal guarded selection. Close and native dismissal retire the exact result generation and editor feedback. Reject stale result and editor effects. |

Select `ocr_surface_render_never_reads_or_writes_and_explicit_copy_uses_edited_text`, `ocr_surface_recapture_uses_normal_lifecycle_and_stale_view_intent_is_ignored` and `ocr_view_empty_progress_and_failures_hide_copy_and_keep_recovery_actions`. Use a fake recognizer and clipboard. Native and GPUI remain pending.
