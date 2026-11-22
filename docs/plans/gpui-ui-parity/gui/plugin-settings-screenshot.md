# Screenshot Plugin Settings

G18 owns `plugin-settings-screenshot` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| SCREENSHOT-SETTINGS-01 | Preserve the typed path and folder chooser, save-on-copy, editor and auto-save choices. The common Settings draft owns edits until Save. The setting route itself performs no acquisition, file save or clipboard copy. |

Select `screenshot_plugin_settings_round_trip_syncs_editor_state` and the shared Settings catalog test. Use a generated scratch destination.

Native and GPUI remain pending.
