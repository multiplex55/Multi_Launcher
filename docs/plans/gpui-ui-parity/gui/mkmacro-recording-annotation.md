# MkMacro Recording Annotation

G19 owns `mkmacro-recording-annotation`. The active recorder toolbar Annotate stages an exact accepted [MkMacro](mkmacro.md) owner/recording request. The annotation host owns its draft. The recorder owns the timed note and pause identity.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-RECORD-NOTE-01 | Open for current Recording or Paused and focus one line of note text. Add note requires trimmed nonempty input and submits one timed note to the exact current recorder. Preserve draft through retained host moves and rejected passes. |
| MK-RECORD-NOTE-02 | Cancel, close or retired parent clears this child. Resume only a pause created by this annotation and still matching the same recording and pause ID. A prior manual pause remains owned by its caller. |

Select `native_annotation_retains_draft_and_adds_once_without_resuming_manual_pause`, `annotation_pause_ownership_preserves_manual_pause`, `rejected_main_checkpoint_does_not_rewind_annotation_draft_or_pause_owner` and `retired_recording_identity_cannot_annotate_or_resume_a_restarted_macro`. Existing fake recorder exercises the owner without real recording. Native and GPUI remain pending.
