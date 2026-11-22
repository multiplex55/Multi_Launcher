# System Status

G23 owns `dashboard-widget-system_status`. The `SystemStatusWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-SYS-STATUS-01 | Preserve separate CPU, memory, disk, network, volume and brightness visibility and refresh options. The shared worker samples current status and reports unavailable data. No row dispatches a state change. |

Use a fake system snapshot. Select `production_prepare_materializes_favorites_and_complete_dashboard_status_families` and `successful_submission_advances_schedule_and_inflight_request_queues_followup`.

Native and GPUI remain pending.
