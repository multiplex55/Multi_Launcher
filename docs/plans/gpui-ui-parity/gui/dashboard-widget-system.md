# System Actions

G23 owns `dashboard-widget-system`. The `SystemWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-SYSTEM-01 | Preserve current System provider results, manual and timed refresh, stable result identity and precise typed action/query. Rows can lock, log off, reboot or shut down. Opening and refreshing only display choices. |

Use a mock provider and inspect controls without dispatch. Select `exact_manager_storage_timer_system_and_layout_variants_are_owned` for the route.

Native and GPUI remain pending.
