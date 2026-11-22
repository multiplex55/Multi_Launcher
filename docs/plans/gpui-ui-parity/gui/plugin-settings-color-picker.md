# Color Picker Plugin Settings

G18 owns `plugin-settings-color-picker` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| COLOR-PICKER-SETTINGS-01 | Preserve the sRGBA swatch, exact channels and configured default. Editing applies current color live and updates the Settings draft. Acquisition and Copy are separate actions. Cancel resets the draft without a distinct provider rollback. |

Select `settings_keep_existing_rgba_shape_and_color32_channel_behavior` and `default_and_typed_color_queries_preserve_existing_three_results`. Use a synthetic color.

Native and GPUI remain pending.
