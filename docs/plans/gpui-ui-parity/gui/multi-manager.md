# MultiManager

G24 owns `multi-manager`. Bare `mm` opens the root MultiManager utility. Workspace and binding stores own saved data. The app worker owns native capture, reconnect and window operations.

| Criterion | Source-backed behavior |
| --- | --- |
| MULTIMANAGER-01 | Preserve stable workspace cards, reorder, expand, enabled state, workspace hotkeys and save/reload. Keep exact inline Reload, Delete Workspace and Delete Window decisions. Check the current row before applying window deletion. |
| MULTIMANAGER-02 | Preserve current pending capture overlay/status, one-shot and bounded multiple capture, native Enter/Escape/skip, and launcher exclusion. Bind results to the accepted action and workspace. Cancel clears exact pending work. |
| MULTIMANAGER-03 | Preserve local home/target geometry, labels and desktops, explicit destination and movement, snapshot/reconnect processing and single-flight checked completion. Discard stale patches on a later owner edit. |

Select `activating_mm_open_opens_multi_manager_dialog`, `cancel_hotkey_edit_leaves_stored_workspace_hotkey_unchanged`, `confirm_event_in_one_shot_mode_clears_pending_capture`, `cancel_event_clears_queue_and_pending_session`, `recapture_during_reconnect_discards_stale_patch` and `only_one_reconnect_job_can_run`. Use isolated empty workspaces with capture and reconnection disabled for authoring observation. `G24-EGUI-MULTI-MANAGER-ROUTES` verifies the actual inactive workspace utility opens independently. GPUI docked workspace authoring, saved binding summaries, checked `mm restore bindings`, explicit Reconnect, current-desktop Home/Target and one-shot Capture Active Window have focused evidence. [Restore Bindings checkpoint](../CHECKPOINTS.md#p010-checked-multimanager-restore-bindings) Matched Windows replay, runtime and remaining controls are open. [Capture checkpoint](../CHECKPOINTS.md#p010-one-shot-multimanager-capture-active-window) · [Placement checkpoint](../CHECKPOINTS.md#p010-checked-multimanager-reconnect-and-placement)
