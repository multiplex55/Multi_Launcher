# Mouse Gesture Settings

G21 owns `mouse-gesture-settings`. `mg settings` and the Settings shell shortcut open the dedicated retained settings host independently of the gesture editor.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-SETTINGS-01 | Preserve behavior enable/log, trail and hint appearance, thresholds, right-click and no-match policy, and ignore titles. Keep exact current ownership across dock, popout and hidden launcher. Review and opening never dispatch a binding. |
| GESTURE-SETTINGS-02 | Valid edits in this host apply live. Save persists the plugin settings and synchronizes the main Settings editor. Reload reads persisted settings. Close retains already applied live edits. Preserve exact [window-picker](mouse-gesture-window-picker.md) ownership and pending state. |

Select `jump_mode_mouse_gesture_settings_focus_save_close_and_reopen_preserve_live_semantics` and `gesture_settings_popout_stays_live_when_root_hides_and_docks_back`. Native and GPUI remain pending.
