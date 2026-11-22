# Mouse Gestures Plugin Settings

G21 owns `plugin-settings-mouse-gestures` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| MOUSE-GESTURE-PLUGIN-01 | Current common Settings intercepts this provider and shows a shortcut to the dedicated Mouse Gesture Settings, with its owner and live/save semantics. The provider's separate inline trait callback remains source-only. Preserve the visible entry and keep the hosted and legacy ignore pickers distinct. |

Select `settings_search_cross_owner_gesture_interval_focuses_with_jump_on_and_off` and `jump_mode_mouse_gesture_settings_focus_save_close_and_reopen_preserve_live_semantics`. Use disabled hooks and synthetic titles.

Native and GPUI remain pending.
