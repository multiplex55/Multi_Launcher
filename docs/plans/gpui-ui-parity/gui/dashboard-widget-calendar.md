# Calendar Widget

G23 owns `dashboard-widget-calendar`. The `CalendarWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-CALENDAR-01 | Retain view, date, range, count, tags and compact settings with the available shared calendar snapshot. Keep navigation and current occurrence rows. Open uses the existing Calendar route. Add immediately creates an all-day event on the selected day. |

Select `jump_mode_dashboard_simple_controls_calendar_focus_and_fresh_actions` and `missing_calendar_state_initializes_but_malformed_update_is_rejected`. Use a temporary Calendar without reminders and leave Add inactive in passive review.

Native and GPUI remain pending.
