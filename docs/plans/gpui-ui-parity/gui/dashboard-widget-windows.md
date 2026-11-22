# Windows Widget

G23 owns `dashboard-widget-windows`. The `WindowsWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-WINDOWS-01 | Retain refresh, case-folded title grouping, limits and current native handle actions. Equal-title grouping currently merges candidates. Switch restores/focuses and Close sends the requested native close. Verify ownership with generated handles. |

Select `legacy_window_action_labels_and_protocols_are_preserved`, `discovery_snapshot_is_coherent_idle_and_refresh_coalesces_followup` and `manual_request_consumes_own_later_publication_exactly_once`. Use a fake catalog.

Native and GPUI remain pending.
