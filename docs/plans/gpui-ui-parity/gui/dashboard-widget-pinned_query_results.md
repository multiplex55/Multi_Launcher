# Pinned Query Results

G23 owns `dashboard-widget-pinned_query_results`. The `PinnedQueryResultsWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-PINNED-RESULTS-01 | Preserve provider selector, enabled or unavailable state, query, row limit and exact refresh modes. Scope manual updates to the selected provider's ticket. Run sends the original Action, while Fill Search sets its label as query. |

Select `jump_mode_pinned_result_focus_then_enter_preserves_widget_action_payload`, `unrelated_publication_does_not_arm_manual_provider_refresh` and `manual_request_consumes_own_later_publication_exactly_once`. Use a fake provider.

Native and GPUI remain pending.
