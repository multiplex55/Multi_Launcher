# MkMacro Recording Review

G19 owns `mkmacro-recording-review`. Stop Recording finalizes one owned result through the recorder runtime and queues it for [MkMacro](mkmacro.md). Open Recording Review can recover the retained review. The child owns editable proposal and exact review epoch. The macro draft owner accepts the reviewed insertion.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-RECORD-REVIEW-01 | Preserve current immutable Literal/Cleaned and Generated/Editable proposals, diagnostics, confidence, annotations and provenance. Queue later completed recordings independently. Local range/edit/trim/delete/undo/redo and current Action Editor do not publish the draft. |
| MK-RECORD-REVIEW-02 | Apply binds exact captured macro and anchor generation. Missing/reused anchor requires explicit Append to end, and a missing target requires deliberate retarget. Apply inserts one typed undoable fragment. It does not save the store. |
| MK-RECORD-REVIEW-03 | Scope explicit playback preparation to current review selection, reject competing operations and preserve exact owner/epoch. Cancel retires current proposal, clears transient observations and advances the queue. |

Select `native_review_opens_editor_and_returns_same_review_step`, `stale_review_open_keeps_recording_without_root_fallback`, `recording_review_applies_at_start_anchor_and_undo_redo_is_one_transaction`, `reused_anchor_id_blocks_apply_and_explicit_append_preserves_review`, `completed_recordings_queue_without_overwriting_active_review` and `rejected_review_frame_restores_editable_state_without_cloning_capture`. Synthetic typed review fixtures avoid real recording/playback. Native and GPUI remain pending.
