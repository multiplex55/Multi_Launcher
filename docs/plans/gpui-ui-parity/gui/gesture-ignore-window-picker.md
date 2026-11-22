# Mouse Gesture Plugin Ignore Picker

G21 owns `gesture-ignore-window-picker`. The built-in plugin still defines an inline chooser in `MouseGesturesPlugin::settings_ui`, and an isolated fixture covers it. Current production Settings special-cases Mouse Gestures and opens the separate [hosted picker](mouse-gesture-window-picker.md). A production entry to this inline branch is unconfirmed.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-PLUGIN-IGNORE-01 | Preserve Windows-only visible-title enumeration, empty/error states, normalized case-insensitive dedupe and immediate plugin update after explicit Add wherever this callback is exposed. Dismissal only hides the chooser. Track the separate fixture and current production route independently. |

Select `utility_size_inventory_plugin_picker_shares_parent_context_without_locking_contents` with synthetic titles. Native reachability and GPUI remain pending.
