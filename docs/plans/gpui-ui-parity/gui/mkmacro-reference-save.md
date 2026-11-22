# Save Captured Reference Image

G19 owns `mkmacro-reference-save`. A completed current Reference Capture reaches the [Action Editor](mkmacro-action-editor.md)-owned Naming decision. The capture workflow owns pending pixels and exact token. The current stable editor destination owns retargeting.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-REF-SAVE-01 | Hold pixels until explicit valid filename and Save As or Replace Existing. Cancel releases pending pixels without writing. Collision stays in the editable naming decision. |
| MK-REF-SAVE-02 | Accept exact current workflow, host and typed target, then publish one managed asset. Retarget only after matching current destination and token. Preserve stale result/draft rejection. |

Select `reference_capture_keeps_pixels_pending_until_filename_confirmation`, `collision_choices_replace_save_as_and_cancel_are_explicit` and related exact-destination cases in the Action Editor. Existing fake capture workflow holds generated pixels. Native and GPUI remain pending.
