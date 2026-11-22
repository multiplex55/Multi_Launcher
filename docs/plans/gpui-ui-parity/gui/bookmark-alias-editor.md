# Bookmark Alias

G06 owns `bookmark-alias-editor`. Set Alias for a current bookmark opens the root Bookmark Alias form for the normalized URL.

| Criterion | Source-backed behavior |
| --- | --- |
| BOOKMARK-ALIAS-01 | Preserve readonly URL, current alias, blank clear, Cancel and the existing Enter-to-save route. Revalidate the exact current record before mutation. Verify stale prefill when its record is no longer present. |

Select `long_bookmark_target_keeps_actions_reachable_without_render_mutation` and the bookmark provider's `malformed_file_rejects_add_remove_and_alias_without_changing_bytes`.

`G06-EGUI-BOOKMARK-DRAFTS` verifies the native egui Set Alias action, URL and current alias prefill, editing and Cancel with the existing bookmark bytes preserved. Save, stale-record handling and GPUI remain pending.
