# System Controls

G23 owns `dashboard-widget-system_controls`. The `SystemControlsWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-SYS-CONTROL-01 | Preserve refresh and current brightness, volume, mute and power plan. The sliders and selectors issue immediate typed system actions with the exact current value or plan GUID. Keep optimistic display changes scoped to the current snapshot. |

Use settings-only or a fake system snapshot/action sink. Select `brightness_legacy_protocol_preserves_raw_integer_and_dialog` and `migrated_volume_power_and_todo_cases_use_owned_commands`.

Native and GPUI remain pending.
