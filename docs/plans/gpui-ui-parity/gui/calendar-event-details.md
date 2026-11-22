# Calendar Event Details

G23 owns `calendar-event-details`. The agenda opens the independent retained Details for the exact instance. Its accepted receipt scopes edit, duplicate, delete and snooze.

| Criterion | Source-backed behavior |
| --- | --- |
| CALENDAR-DETAILS-01 | Preserve displayed event data and the exact occurrence handoff to Edit. Duplicate creates a one-off. Delete preserves current single and recurring scope choices. Revalidate exact occurrence and latest transaction before effects. |
| CALENDAR-DETAILS-02 | Preserve the bounded snooze control and current source checks. Close retires the exact child. Use a synthetic local event without reminders for passive baseline checks. |

Select `stale_details_occurrence_cannot_delete_or_duplicate_the_changed_series`, `duplicate_and_delete_use_transaction_and_reject_corruption` and `detached_calendar_retains_occurrence_and_saves_and_deletes_only_on_accepted_passes`. `G23-EGUI-CALENDAR-TODO` verifies the native exact agenda occurrence, notes and Edit handoff. Mutations and GPUI remain pending.
