# Toast History

G23 owns `toast-history`. Help → View Toast Log opens the root viewer for the last 20 lines of the current working directory's toast log. Open Toast Log is the distinct external-file action.

| Criterion | Source-backed behavior |
| --- | --- |
| TOAST-HISTORY-01 | Preserve tail order, empty or unreadable state, explicit Refresh and Close. The viewer reads the log without creating or opening an external file. Use generated lines in an isolated working directory. |

The existing `report_error_respects_inline_and_toast_settings` covers the writer. `G05-EGUI-HELP-MENU-ROUTES` verifies the internal viewer opens from Help and dismisses on Escape. Tail content, explicit Refresh and GPUI remain pending. [Validation log](../VALIDATION_LOG.md)
