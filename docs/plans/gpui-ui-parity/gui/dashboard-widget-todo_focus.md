# Todo Focus

G23 owns `dashboard-widget-todo_focus`. The `TodoFocusWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-TODO-FOCUS-01 | Preserve status, priorities, tags, sort and query override. Open targets the current row editor. Completion commits immediately. Existing index routing needs reorder verification. |

Use one synthetic stable-ID Todo with a fake effect sink. Select `focus_filters_by_status_priority_and_tags` and `focus_config_persists_filter_state`.

Native and GPUI remain pending.
