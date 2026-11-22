# Quick Tools

G23 owns `dashboard-widget-quick_tools`. The `QuickToolsWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-QUICK-01 | Keep configured columns, visible rows, item order and per-item Auto submit. Preserve legacy normalization, stable duplicate row identity, scroll/clipping and responsive sizing. Manual mode fills the query. Auto mode uses the first-result command under the same action ownership. |

Select `actions_obey_manual_auto_submit_and_whitespace_contracts`, `responsive_grid_bounds_buttons_and_keeps_duplicate_ids_when_resized_and_scaled` and `quicktools_jump_focus_then_enter_activates_manual_and_auto_actions_once`. Use an inert manual query for native testing.

`G23-EGUI-DASHBOARD-QUICK-TOOLS` verifies the actual configured query-only action and selected Quick Tools settings callback. Auto submit and GPUI remain pending.
