# Snippets

G13 owns `snippets`. Bare `cs` emits `snippet:dialog` and opens the saved-list editor. `cs edit <alias>` opens the exact eligible entry. The Snippets plugin owns the canonical `snippets.json`, exact aliases, normalization and snapshot writes. The retained editor owns browse, draft, filter and removal review.

| Criterion | Source-backed behavior |
| --- | --- |
| SNIPPETS-01 | Show saved list, case-insensitive alias/body/tag filter and all selected tags, Clear Filters, select/Edit/Remove/Add Snippet and Close. Mask hidden previews while retaining current search matching and stable row identity. |
| SNIPPETS-02 | Edit the exact alias, multiline Text, Hide Contents/Reveal, Prompt for Fields, bounded normalized tags and ordered field label/default/required/type. Preserve exact alias case and surrounding spaces. Reject new duplicates, retain editable legacy unchanged duplicate aliases and reconcile fields only for valid prompted templates. |
| SNIPPETS-03 | Save against the current full snapshot, keep the last good data and show conflicts. Inline removal binds the exact current row and snapshot to Confirm/Cancel. The editor's Test / Preview uses the unsaved candidate in the owned [Snippet Prompt](snippet-prompt.md), then returns to the same draft. |
| SNIPPETS-04 | Preserve current Dock/Pop Out, geometry and epoch. Root and exact host receipts gate actions and child preview. True close retires unsaved and revealed state. Hidden content masks previews. Stored and copied values remain plaintext. |

Select `selected_tag_filters_match_case_insensitively_with_and_semantics`, `alias_validation_is_case_sensitive_and_preserves_untrimmed_aliases`, `external_append_body_and_privacy_changes_reject_stale_snapshot`, `malformed_reload_and_commit_keep_dialog_last_good_and_read_only`, `stale_removal_confirmation_keeps_external_update_unchanged`, `preview_uses_unsaved_draft_without_mutating_editor_or_persisted_state` and `snippets_detached_preview_returns_to_the_same_unsaved_draft`. `G13-EGUI-SNIPPET-TEMPFILE-DRAFTS` proves native list, edit, preview and return with identical store bytes. Save, removal, host and GPUI remain pending.
