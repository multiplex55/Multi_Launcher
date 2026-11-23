# Toast History

G23 owns `toast-history`. Help → View Toast Log opens the root viewer for the last 20 lines of the current working directory's toast log. Open Toast Log is the distinct external-file action.

| Criterion | Source-backed behavior |
| --- | --- |
| TOAST-HISTORY-01 | Preserve tail order, empty or unreadable state, explicit Refresh and Close. The viewer reads the log without creating or opening an external file. Use generated lines in an isolated working directory. |
| TOAST-HISTORY-02 | Preserve the resizable viewer's default 360×200 outer size and shared recorded size through the `toast-log` utility geometry key. |

The existing `report_error_respects_inline_and_toast_settings` covers the writer. `G05-EGUI-HELP-MENU-ROUTES` verifies the internal viewer opens from Help and dismisses on Escape. `G05-G23-EGUI-TOAST-TAIL-REFRESH-CLOSE` verifies the exact 20-line tail, retained contents until explicit Refresh and the Close button on Windows. `G05-G23-EGUI-TOAST-EXTERNAL` records the OS association opening a new Notepad window for the owned log, with document inspection and close unverified. [Validation log](../VALIDATION_LOG.md#g05-file-close-and-g23-toast-history-native-baseline)

`G05-G23-GPUI-TOAST-MENUS`, `G05-G23-GPUI-TOAST-ASSOCIATION` and `G05-G23-EGUI-TOAST-MATCHED` verify the retained GPUI tail, native Refresh, Close and Escape, compact scrolling, fresh reopen and canonical external handoff, with matching egui behavior on the same executable. The eight focused Rust cases also verified the missing and invalid UTF-8 read behavior and the current receipt guards. TOAST-HISTORY-01 has scoped acceptance. The GPUI `toast-log` utility geometry remains pending under TOAST-HISTORY-02. [Validation log](../VALIDATION_LOG.md#g05g23-gpui-file-close-and-toast-log)
