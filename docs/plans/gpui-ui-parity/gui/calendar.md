# Calendar

G23 owns `calendar`. Exact `cal` opens the retained calendar on the current local day. The typed Calendar action may update view state. Event drafts and Details use their own retained child owners.

| Criterion | Source-backed behavior |
| --- | --- |
| CALENDAR-01 | Preserve date selection, month navigation, Today, keyboard movement and available Day/Week agenda modes with local instances. Maintain independent view state and current presentation. |
| CALENDAR-02 | Route Add/Enter to the editor with the exact day or occurrence. Agenda action opens [Event Details](calendar-event-details.md). Retain live dirty child and current parent identity across rehost. |

Select `calendar_jump_focus_then_enter_changes_month_once`, `detached_calendar_retains_occurrence_and_saves_and_deletes_only_on_accepted_passes` and `calendar_parent_open_promotes_live_editor_without_replacing_draft`. `G23-EGUI-CALENDAR-TODO` verifies native Day view and the exact agenda handoff. Other native controls and GPUI remain pending.

## GPUI Day agenda and Calendar search

The retained `cal` agenda shows canonical events for the selected day. The checked Omni routes use the stock Calendar Open, Search, Upcoming and generated date Jump actions. Open and Jump update view state through the shared checked writer. Paired Windows replay, additional calendar modes and authoring remain open. [Omni Calendar checkpoint](../CHECKPOINTS.md#p006-checked-omni-calendar-and-day-agenda)
