# Gesture Health

G23 owns `dashboard-widget-gesture_health`. `GestureHealthWidget` renders this registered slot from the current dashboard state.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-GESTURE-HEALTH-01 | Report current unbound gestures, duplicates and disabled counts. Four editor-oriented actions open gesture, add, bind and settings surfaces. There is no settings callback or binding dispatch. |

Select `gesture_stats_detects_unbound_and_duplicates` and `mouse_gestures_skip_history_and_generic_clear_hide_policy`. Use generated gestures and keep global capture disabled.

Native and GPUI remain pending.
