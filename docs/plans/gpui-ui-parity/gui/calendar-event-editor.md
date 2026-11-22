# Calendar Event Editor

G23 owns `calendar-event-editor`. The retained Calendar, Event Details and Date & Time Studio handoffs open an owned event draft. An already live editor retains its draft.

| Criterion | Source-backed behavior |
| --- | --- |
| CALENDAR-EDITOR-01 | Keep title, notes, validated local dates, times and duration, all-day, reminders, tags and recurrence. Date & Time Studio prefill uses compatible local minute alignment. Cancel and native close leave events intact. |
| CALENDAR-EDITOR-02 | Save validates accepted child and exact current occurrence or series. Atomically add, update or split while preserving supported metadata. Corruption and missing/stale sources retain the unsaved draft with an actionable failure. |

Select `editor_commit_uses_latest_valid_event_and_rejects_corruption`, `retained_editor_update_and_split_cannot_restore_a_deleted_event`, `calendar_editor_jump_focus_requires_fresh_save_activation` and `date_time_studio_calendar_prefill_requires_minute_alignment_and_preserves_exact_minute`. `G23-EGUI-CALENDAR-TODO` verifies the native Details-origin draft, synthetic title edit and Cancel. Save and GPUI remain pending.
