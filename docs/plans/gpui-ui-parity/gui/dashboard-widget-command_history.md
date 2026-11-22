# Command History

G23 owns `dashboard-widget-command_history`. The `CommandHistoryWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-CMD-HISTORY-01 | Preserve count, pinned-only and filter, current history/pin snapshot, exact Action/query/arguments and duplicate identity. Unavailable pins can be unpinned and cannot run. Explicit star changes the pins store. |

Select `jump_mode_dashboard_commands_history_targets_keep_provenance_and_filter_focus`, `history_resolution_preserves_duplicate_order_and_exact_args` and `failed_pin_reload_retains_last_good_then_recovers`. Use inert generated history.

Native and GPUI remain pending.
