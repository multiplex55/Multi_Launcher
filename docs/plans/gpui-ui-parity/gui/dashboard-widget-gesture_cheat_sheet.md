# Gesture Cheat Sheet

G23 owns `dashboard-widget-gesture_cheat_sheet`. The `GestureCheatSheetWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-GESTURE-SHEET-01 | Preserve count, optional disabled rows, bound gestures sorted by usage and label, and current primary binding. Title opens the checked authoring route. The explicit checkbox applies an exact durable gesture toggle. |

Select `gesture_refresh_retains_last_good_recovers_and_deduplicates_self_write`, `focus_uses_typed_payload_and_missing_payload_falls_back_to_dialog` and `toggle_uses_typed_payload_while_missing_payload_is_claimed_no_op`. Keep gesture service disabled for fixture browsing.

Native and GPUI remain pending.
