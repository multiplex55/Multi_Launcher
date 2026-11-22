# Clipboard Modify Plugin Settings

G13 owns `plugin-settings-clipboard-modify` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| CLIP-MODIFY-SETTINGS-01 | Preserve the common draft's hide-after-apply, bounds, navigation, split, filter and sort. Its special controls open the utility, open or reload the separate config and confirm a factory reset. These operations retain their existing effects and owner checks independently of the preference draft. |

Select `clipboard_modify_settings_round_trip_editor_conversion` and `migration_and_factory_reset_create_backups`. Use synthetic preferences and keep clipboard capture and destructive operations uninvoked in passive inspection.

The current source reads the hide preference and legacy dialog dimensions. The navigation width, split, filters and management sort serialize in Settings, but their runtime consumers are unconfirmed.

Native and GPUI remain pending.
