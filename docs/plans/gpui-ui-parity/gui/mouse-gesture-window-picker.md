# Mouse Gesture Settings Window Picker

G21 owns `mouse-gesture-window-picker`, the dedicated Settings child for Ignore windows → Select window. Windows enumerates eligible visible titles and shows a normalized, sorted and deduplicated choice list. The picker inherits the exact settings parent.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-WINDOW-01 | Scope each Add to the accepted child epoch, parent session, current ignore-list basis and exact offered title. Preserve stable row identity, dedupe, dirty state and immediate live match updates. The settings Save owns persistence. Dismissal leaves the list unchanged. |
| GESTURE-WINDOW-02 | Preserve unavailable, failed and empty enumeration states, and block competing parent Jump Mode while this owner is active. Selecting a title performs no window activation or capture. Use synthetic titles in isolated tests. |

Select `hosted_picker_checks_session_epoch_and_current_ignore_basis`, `hosted_picker_adds_current_title_once_without_replacing_other_settings`, `native_gesture_window_picker_inherits_and_adds_accepted_title` and `add_ignore_window_title_avoids_case_insensitive_duplicates`. Native and GPUI remain pending.
