# Mouse Gestures

G21 owns `mouse-gestures`. `mg edit` opens the retained editor. Search and list results open their editor route, and the dashboard handoff carries the stable gesture identity. The editor owns recording and authoring state. The independent app service owns global right-button capture and action dispatch.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-EDITOR-01 | Preserve grouped results, stable selection, labels, enabled state, direction mode and manual tokens. The primary-button editor canvas updates only the bounded recording draft, retains custom labels and stops capture on focus or owner loss. |
| GESTURE-EDITOR-02 | Preserve exact gesture delete confirmation and activation release. Add Gesture immediately persists an empty default entry and offers rename. Cancel rename keeps that entry. Save publishes the accepted transactional database and refreshes runtime, launcher, directory and dashboard. Save failure restores the last committed state. Corrupt storage retains read-only safeguards. |
| GESTURE-EDITOR-03 | Keep authoring distinct from runtime dispatch, practice mode and global right-click policy. Cancel and passive editor opening produce no dispatch. |

Select `gesture_delete_is_queued_until_confirmed`, `cancelling_gesture_delete_keeps_db_and_selection`, `save_failure_restores_committed_dialog_snapshot_and_retains_disk`, `jump_mode_mouse_gestures_review_canvas_capture_retires_when_root_hides` and `practice_mode_suppresses_execute_action`. `G21-EGUI-GESTURE-AUTHORING` verifies the editor route, immediate default add, rename cancellation and return from the binding child. Capture, dispatch and GPUI remain pending.

## Retained GPUI catalog

Stock `mg edit` opens the retained catalog. Checked Add, Rename, Four-direction tokens, Set Query, Enabled and Practice use the canonical gesture and Settings owners. The physical Windows gesture, foreground, click pass-through and retirement replay remains open. [Mouse Gestures checkpoint](../CHECKPOINTS.md#p018-checked-ordinary-mouse-gestures)
