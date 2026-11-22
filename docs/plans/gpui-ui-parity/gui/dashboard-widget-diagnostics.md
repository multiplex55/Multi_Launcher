# Dashboard Diagnostics

G23 owns `dashboard-widget-diagnostics`. `DiagnosticsWidget` renders this registered slot from the current dashboard state.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-DIAGNOSTICS-01 | Show current frame, cache and refresh measurements or the unavailable state. Include a current snapshot when this slot exists. There is no settings callback or dispatched action. |

Select `frame_metrics_throttle_until_interval` and `widget_refresh_flags_throttle_over_threshold` for the measurement model.

Native and GPUI remain pending.
