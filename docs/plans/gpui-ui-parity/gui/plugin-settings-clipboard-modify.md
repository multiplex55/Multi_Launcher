# Clipboard Modify Plugin Settings

G13 owns `plugin-settings-clipboard-modify` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| CLIP-MODIFY-SETTINGS-01 | Preserve the common draft's hide-after-apply, bounds, navigation, split, filter and sort. Its special controls open the utility, open or reload the separate config and confirm a factory reset. These operations retain their existing effects and owner checks independently of the preference draft. |

Select `clipboard_modify_settings_round_trip_editor_conversion` and `migration_and_factory_reset_create_backups`. Use synthetic preferences and keep clipboard capture and destructive operations uninvoked in passive inspection.

The current source reads the hide preference and legacy dialog dimensions. The navigation width, split, filters and management sort serialize in Settings, but their runtime consumers are unconfirmed.

The stock **Hide Launcher after applying template or pipeline** control is available in retained GPUI Settings. Settings Search targets the control. A verified Save updates future named template and pipeline runs. Focused tests cover the canonical setting, acknowledgement and execution policy. Settings also opens the exact existing configuration file and reloads a stable current-schema catalog into Search and the retained utility. Paired Windows replay and the remaining settings stay open. [Configuration checkpoint](../CHECKPOINTS.md#p007-p014-checked-clipboard-modify-configuration) [Clipboard visibility checkpoint](../CHECKPOINTS.md#p007-p014-live-clipboard-modify-named-apply-visibility)
