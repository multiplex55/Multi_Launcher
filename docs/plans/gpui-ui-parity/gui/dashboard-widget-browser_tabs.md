# Browser Tabs Widget

G23 owns `dashboard-widget-browser_tabs`. The `BrowserTabsWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-BROWSER-01 | Retain row count, timer/manual refresh, current provider ticket and cached/error/empty rows with URL tooltip. Switch sends the exact current native runtime ID. Refresh reuses the ordinary provider search. |

Select `populated_snapshot_filters_without_discovery`, `recalc_each_query_coalesces_many_consumer_filters_per_cache_epoch` and `manual_request_consumes_own_later_publication_exactly_once`. Use fake tab IDs and reserved URLs without switching.

Native and GPUI remain pending.
