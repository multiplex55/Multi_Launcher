# Pinned Commands

G23 owns `dashboard-widget-pinned_commands`. The `PinnedCommandsWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-PINNED-01 | Resolve the ordered selected keys against current Action, Favorite and legacy entries. Keep independent List/Grid, add/remove choices, duplicate row identity and cache invalidation. Activation sends the current complete Action and label query override. |

Select `refreshes_when_action_ids_change`, `settings_choices_use_supplied_favorites_snapshot` and `jump_mode_dashboard_commands_pinned_rows_are_opaque_stable_and_focus_only`. Use an inert query-only action.
