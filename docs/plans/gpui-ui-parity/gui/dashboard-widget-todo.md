# Todo Widget

G23 owns `dashboard-widget-todo`. The `TodoWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-TODO-01 | Retain count, progress, status, priority, tags, query suggestions and sort options, including legacy show_done. Keep exact row and accepted popup scope. Open uses the current Todo route. The explicit checkbox commits completion. Existing completion reloads by index, so reorder safety needs separate verification. |

Select `filters_combine_before_sorting`, `legacy_todo_widgets_are_migrated` and `jump_mode_dashboard_simple_controls_todo_filters_and_done_action`. Use an isolated stable-ID Todo.
