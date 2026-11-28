# Bookmark Alias

G06 owns `bookmark-alias-editor`. Set Alias for a current Bookmark result opens the root Bookmark Alias form for its exact stored URL.

| Criterion | Source-backed behavior |
| --- | --- |
| BOOKMARK-ALIAS-01 | Preserve readonly URL, current alias, blank clear, Cancel and the existing Enter-to-save route. Revalidate the exact current record before mutation. Verify stale prefill when its record is no longer present. |

Select `long_bookmark_target_keeps_actions_reachable_without_render_mutation` and the bookmark provider's `malformed_file_rejects_add_remove_and_alias_without_changing_bytes`.

`G06-EGUI-BOOKMARK-DRAFTS` verifies the native egui Set Alias action, URL and current alias prefill, editing and Cancel with the existing bookmark bytes preserved. GPUI resolves the current live record from the Action Sheet and guards Save against the immutable original URL and alias. Retry retains the attempted value separately. Blank clears the alias. Direct Search and Omni Bookmark rows have focused evidence. Stock store tests distinguish legacy HTTP and HTTPS siblings. Matched Windows Save, stale refusal, retry and Cancel replay remain open. [Bookmark checkpoint](../CHECKPOINTS.md#p004-canonical-bookmark-authoring) [Omni Alias checkpoint](../CHECKPOINTS.md#p003-p004-p006-set-alias-from-omni-search)
