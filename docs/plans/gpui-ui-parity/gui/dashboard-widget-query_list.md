# Query List

G23 owns `dashboard-widget-query_list`. The `QueryListWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-QUERY-LIST-01 | Preserve input, row cap, descriptions, refresh and provider filters. Empty query makes no provider request. Current manual results wait for exact tickets and accept at most one follow-up. Activate the full Action and label query override. |

Select `manual_query_waits_for_every_exact_ticket_and_consumes_once`, `fresh_manual_query_with_no_ticket_does_not_schedule_follow_up` and `provider_actions_match_on_synchronous_and_snapshot_paths`. Use empty or synthetic-provider query.

Native and GPUI remain pending.
