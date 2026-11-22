# Screen Draw Canvas

G22 owns `screen-draw-canvas`. An admitted frozen signed desktop snapshot creates a native session with the shared canvas document. Drawing captures the relevant input. Ghost and Finish use passive native presentation.

| Criterion | Source-backed behavior |
| --- | --- |
| SCREEN-DRAW-CANVAS-01 | Preserve the ten tools, preview and exact commit, text editing, history and export. Clip to signed snapshot bounds. Exclude toolbar input and partition strokes across its bounds without reconnecting hidden samples. Keep fading ink transient and sample Eyedropper only from the frozen snapshot. |
| SCREEN-DRAW-CANVAS-02 | Release transient input and suppression before passive display. Preserve document and recovery on monitor change while retiring the active input session. Keep Ghost and Finish click-through and nonactivating. |

Select `shape_preview_is_ephemeral_and_commits_exactly_once_on_release`, `native_text_state_supports_edit_multiline_commit_and_cancel`, `full_pen_crossing_emits_two_live_fragments_and_one_break_aware_undo_item`, `eyedropper_samples_only_signed_frozen_snapshot_coordinates` and `display_change_releases_input_hides_both_surfaces_and_preserves_worker`. Use generated pixels and the fake native bridge. Native and GPUI remain pending.
