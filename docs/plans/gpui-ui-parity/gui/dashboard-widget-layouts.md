# Layouts Widget

G23 owns `dashboard-widget-layouts`. The `LayoutsWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-LAYOUTS-01 | Preserve refresh modes, timing, health and autosave, current worker snapshot and selected draft. Check exact current import/export, restore, rename and delete effects. Health and dry-run can enumerate windows, and Restore moves them. |

Select `active_layout_is_selected_from_worker_snapshot`, `mutation_merges_external_edit_and_stale_refresh_cannot_replace_it` and `cancelled_export_is_not_queued`. Disable health and use a synthetic layout for passive review.

Native and GPUI remain pending.
