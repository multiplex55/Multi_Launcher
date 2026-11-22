# Recent Clipboard Items

G23 owns `dashboard-widget-clipboard_recent`. The `ClipboardRecentWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-CLIP-RECENT-01 | Preserve configured count, cached items, scroll, row and tooltip. Copy explicitly uses the checked Clipboard operation. The current index route and byte-based preview truncation need reorder and Unicode verification before acceptance. |

Select `clipboard_history_action_preserves_index_and_clipboard_identity` for the related wire and `protected_clipboard_history_skips_only_the_marked_item` for the source. Use synthetic cache and fake clipboard.

Native and GPUI remain pending.
