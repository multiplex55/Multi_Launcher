# Volume Widget

G23 owns `dashboard-widget-volume`. The `VolumeWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-VOLUME-01 | Preserve master level, current sessions, refresh, Mute Active and per-session level and mute. Actions apply immediately to real audio state. Check current PID and preserve the widget's existing slider and mute behavior. |

Use a synthetic volume snapshot and fake dispatch. Select `migrated_volume_power_and_todo_cases_use_owned_commands`, `search_mute_active` and `search_pid_level`.

Native and GPUI remain pending.
