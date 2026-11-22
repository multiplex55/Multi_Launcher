# Shell Plugin Settings

G24 owns `plugin-settings-shell`, the built-in callback for Open shell commands in WezTerm. The G06 Shell Commands editor has separate ownership.

| Criterion | Source-backed behavior |
| --- | --- |
| SHELL-SETTINGS-01 | Preserve the default-off preference, live builder selection and common profile save. Opening the callback spawns no process. Verify native command construction with a fake or argv-only fixture. |

Select `cmd_is_used_when_wezterm_disabled`, `wezterm_is_used_when_enabled` and `settings_search_catalog_covers_builtin_and_dedicated_control_inventory`. Native and GPUI remain pending.
