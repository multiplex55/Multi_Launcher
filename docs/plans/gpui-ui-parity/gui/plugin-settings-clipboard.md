# Clipboard Plugin Settings

G13 owns `plugin-settings-clipboard` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| CLIP-SETTINGS-01 | The common Clipboard callback edits the 1–200 history limit with default 20. It applies to provider memory while rendering. Changing it does not trim history until the next accepted ordinary text insertion. Settings Save persists and synchronizes the legacy mirror. |

Select `protected_clipboard_history_skips_only_the_marked_item` and shared Settings catalog tests. Use an isolated store, `ML_SKIP_CLIPBOARD_SYNC=1` and no approval-sensitive operations.

Native and GPUI remain pending.
