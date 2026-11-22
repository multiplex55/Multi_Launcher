# Windows Overview

G23 owns `dashboard-widget-windows_overview`. The `WindowsOverviewWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-WINDOWS-OVER-01 | Retain settings for row count, refresh and visible close controls. Keep current catalog identity and existing equal-title grouping. Switch and Close affect live windows only under the explicit action path. |

Select `legacy_window_action_labels_and_protocols_are_preserved`, `repaint_during_ticket_handoff_joins_the_reserved_post_request_generation` and `manual_request_consumes_own_later_publication_exactly_once`. Verify against simulated identities.

Native and GPUI remain pending.
