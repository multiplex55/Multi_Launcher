# Command Help

G05 owns `command-help`. Help → Command List and Linking Guide, or the typed `help:show` result, open the retained Command Help utility.

| Criterion | Source-backed behavior |
| --- | --- |
| COMMAND-HELP-01 | Preserve actual registered plugin names, capability, description, usage, commands and optional examples. Apply the common retained filter. Linking Guide seeds its specific query. Save Show Examples only through the existing successful persistence path. |

Select `command_help_filters_real_plugin_commands_without_static_tutorials` and `bounded_help_scroll_keeps_controls_and_full_content_reachable`.

`G05-EGUI-ACTION-SHEET-HELP` verifies the typed panel opens. `G05-EGUI-HELP-MENU-ROUTES` verifies the Linking Guide populates the exact filter and the scoped empty result, then closes on Escape. Matching command rows, preference persistence and GPUI remain pending. [Validation log](../VALIDATION_LOG.md)
