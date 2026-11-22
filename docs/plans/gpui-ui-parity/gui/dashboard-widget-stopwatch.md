# Stopwatch Widget

G23 owns `dashboard-widget-stopwatch`. The `StopwatchWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-STOPWATCH-01 | Preserve stable current rows, live monotonic elapsed text, configured precision and refresh, pause/resume/stop. Checked actions reject missing or stale stopwatch states. |

Use an empty scratch session or synthetic in-memory stopwatch. Select `stopwatch_paused_tracks_live_pause_state_and_missing_entries`, `stale_timer_and_stopwatch_commands_report_instead_of_succeeding` and `refresh_after_sw_list_command`.

Native and GPUI remain pending.
