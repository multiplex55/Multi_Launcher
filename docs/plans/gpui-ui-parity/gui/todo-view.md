# View Todos

G23 owns `todo-view`. `todo view` opens the retained list. Exact launcher edit and Note-origin routes resolve to a stable todo ID before targeting the editor.

| Criterion | Source-backed behavior |
| --- | --- |
| TODO-VIEW-01 | Preserve sort, text and tag filters, exact editor target, text, priority, tags and Note picker. Rehosting retains the current session and dirty editor. Target navigation does not replace dirty state. |
| TODO-VIEW-02 | Save re-resolves the stable item and compares the exact original record under current ownership, preserving unrelated edits and order. Opening Note or anchor revalidates row, token and destination. Cancel leaves the original record intact. |

Select `accepted_todo_edit_targets_identity_after_store_reorder`, `replaced_todo_source_does_not_accept_stale_edit`, `todo_note_receipt_rejects_host_epoch_and_replaced_source`, `native_note_todo_and_todo_note_links_use_owned_hosts` and `live_target_navigation_preserves_dirty_editor_and_session`. `G23-EGUI-CALENDAR-TODO` verifies the native task row, priority and negative and tag filters. Editing, Note handoffs and GPUI remain pending.

## GPUI View Todos

The retained `todo view` opens the canonical watched Todo list with stock text and tag filters, Priority and Name ordering, and exact checked completion. Ordinary current checked Todo results also toggle from Omni under its own category gate. Paired Windows replay, Edit, links and other handoffs remain open. [Omni Todo checkpoint](../CHECKPOINTS.md#p006-checked-omni-todo-and-view-todos)
