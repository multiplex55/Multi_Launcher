# Plugin Home

G23 owns `dashboard-widget-plugin_home`. The `PluginHomeWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-PLUGIN-HOME-01 | Choose the current provider, Commands or Search, optional query and row limit. Preserve checked asynchronous generations, cached results, failure and Retry. Send the full current provider Action plus label query override. |

Select `cold_render_boundary_returns_before_blocked_provider`, `new_plugin_epoch_replaces_blocked_executor_and_suppresses_old_result` and `explicit_retry_allows_same_source_to_run_again`. Use an inert mock provider.

Native and GPUI remain pending.
