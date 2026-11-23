# Quick Help

G05 owns `quick-help`. The configured help shortcut or Help → Quick Help Overlay opens the root utility while the launcher is visible. F1 is the usual default.

| Criterion | Source-backed behavior |
| --- | --- |
| QUICK-HELP-01 | Preserve the visible-only toggle and retained filter, scrollable hotkeys and guides, plus actual installed plugin metadata and descriptions. Keep the same filter available to Command Help. |

Select `bounded_help_scroll_keeps_controls_and_full_content_reachable`.

`G05-EGUI-HELP-MENU-ROUTES` verifies the actual menu entry, representative Hotkeys and Jump Mode content, and Escape dismissal. `G05-EGUI-HELP-SHARED-FILTER-EXAMPLES` verifies that the other Help window's exact filter is retained with the matched plugin metadata. The shortcut, Quick Help body scrolling and GPUI remain pending. [Validation log](../VALIDATION_LOG.md#g05-native-help-filter-and-examples-baseline)
