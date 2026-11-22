# Screenshot Region Selection

G18 owns `screenshot-region-selection` as an external Windows Snipping Tool handoff. Immediate or delayed Region, Region Markup and Region Clip reach `actions::screenshot` through the typed screenshot command. Windows owns the selection UI. The application owns acquisition timing and result delivery.

| Criterion | Source-backed behavior |
| --- | --- |
| SHOT-REGION-01 | Use the current region destination policy and accepted capture owner. Snapshot existing clipboard image identity, invoke Windows region selection and poll within the existing bound. A changed valid RGBA image is the existing completion condition. |
| SHOT-REGION-02 | Preserve typed cancellation/timeout/error and avoid success/history on missing result. Delayed caller retires stale results and restores its own parking authority. The current clipboard comparison does not establish provenance. |
| SHOT-REGION-03 | Deliver current capture to the existing [Screenshot Editor](screenshot-editor.md), clipboard or file path according to the current request and configured policy. Existing Snipping Tool window lifecycle belongs to Windows. |

Use integration target `screenshot_cancel` with `wait_for_new_clipboard_image_detects_changed_snapshot`, `wait_for_new_clipboard_image_times_out_quickly_with_cancel_error` and `cancel_classifier_detects_expected_errors`. These closure-fed fixtures avoid system clipboard and screen interaction. Native and GPUI evidence are pending.
