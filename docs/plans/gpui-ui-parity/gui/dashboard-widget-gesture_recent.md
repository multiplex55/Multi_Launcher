# Recent Gestures

G23 owns `dashboard-widget-gesture_recent`. The `GestureRecentWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-GESTURE-RECENT-01 | Retain count and current usage ordering, exact gesture identity and binding display with Unknown fallback. The typed handoff carries optional binding index. The present host focuses the gesture row. |

Select `focus_uses_typed_payload_and_missing_payload_falls_back_to_dialog` and `missing_initializes_but_malformed_record_is_skipped_unchanged`. Use synthetic usage without replay.

Native and GPUI remain pending.
