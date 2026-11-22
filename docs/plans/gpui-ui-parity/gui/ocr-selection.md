# OCR Region Selection

G22 owns `ocr-selection`. Exact `ocr` and assigned typed actions use the current app OCR selection controller and shared rectangle selector.

| Criterion | Source-backed behavior |
| --- | --- |
| OCR-SELECTION-01 | Reject conflicts before mutation and retain the current owner on duplicate requests. Check launcher off-desktop parking and signed rectangle input under the exact selection generation and operation. |
| OCR-SELECTION-02 | Escape and cancellation await terminal acknowledgement. A confirmed selection starts the matching worker. Restore the launcher after image capture while recognition continues. Retire old results after visibility, query, selection or generation supersession. |

Select `ocr_begin_stages_without_desktop_or_capture_and_verifies_on_later_poll`, `ocr_capture_restores_before_recognition_and_close_detaches_old_generation` and `ocr_new_query_or_visibility_during_capture_and_recognition_invalidates_completion`. Use fake parking and generated pixels. Native and GPUI remain pending.
