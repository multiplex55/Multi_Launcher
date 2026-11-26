# Snippets

G13 owns `snippets`. Bare `cs` emits `snippet:dialog` and opens the saved-list editor. `cs edit <alias>` opens the exact eligible entry. The Snippets plugin owns the canonical `snippets.json`, exact aliases, normalization and snapshot writes. The retained editor owns browse, draft, filter and removal review.

| Criterion | Source-backed behavior |
| --- | --- |
| SNIPPETS-01 | Show saved list, case-insensitive alias/body/tag filter and all selected tags, Clear Filters, select/Edit/Remove/Add Snippet and Close. Mask hidden previews while retaining current search matching and stable row identity. |
| SNIPPETS-02 | Edit the exact alias, multiline Text, Hide Contents/Reveal, Prompt for Fields, bounded normalized tags and ordered field label/default/required/type. Preserve exact alias case and surrounding spaces. Reject new duplicates, retain editable legacy unchanged duplicate aliases and reconcile fields only for valid prompted templates. |
| SNIPPETS-03 | Save against the current full snapshot, keep the last good data and show conflicts. Inline removal binds the exact current row and snapshot to Confirm/Cancel. The editor's Test / Preview uses the unsaved candidate in the owned [Snippet Prompt](snippet-prompt.md), then returns to the same draft. |
| SNIPPETS-04 | Preserve current Dock/Pop Out, geometry and epoch. Root and exact host receipts gate actions and child preview. True close retires unsaved and revealed state. Hidden content masks previews. Stored and copied values remain plaintext. |

Select `selected_tag_filters_match_case_insensitively_with_and_semantics`, `alias_validation_is_case_sensitive_and_preserves_untrimmed_aliases`, `external_append_body_and_privacy_changes_reject_stale_snapshot`, `malformed_reload_and_commit_keep_dialog_last_good_and_read_only`, `stale_removal_confirmation_keeps_external_update_unchanged`, `preview_uses_unsaved_draft_without_mutating_editor_or_persisted_state` and `snippets_detached_preview_returns_to_the_same_unsaved_draft`. `G13-EGUI-SNIPPET-TEMPFILE-DRAFTS` proves native list, edit, preview and return with identical store bytes.

## GPUI retained Snippets editor

The exact stock `cs` action opens a docked saved list backed by the current app-data profile. The pane opens at its filter above an independently scrolling list. Text and selected tags use the shared stock predicate. Hidden previews stay masked, including when hidden body text matches the filter. Reload keeps the last good rows and disables edits on unreadable or malformed data.

Edit binds the exact occurrence in the loaded snapshot. The form supports alias, multiline body, tags and preview privacy. It preserves unchanged legacy aliases, literal commas in tags and authored prompt metadata through the shared preparation path. Reveal creates the body editor and Hide retires it while keeping the exact draft. Cancel discards the draft without writing. Save uses the shared atomic transaction with live native and snapshot checks. A conflict keeps the draft and requires reload before editing again.

Add opens the same editor with a blank draft. Its Save appends to the exact captured list and refuses duplicates or changes made on disk since the draft opened. Remove stages a separate inline review for one exact occurrence. Cancel does not write, and recovery preserves the review only while its authority can still be used. [Add and Remove checkpoint](../CHECKPOINTS.md#g13-snippets-add-and-exact-remove-review)

Named `cs edit` uses a watched snapshot of the canonical file. Stock action and tag filters select the candidates. Exact duplicate aliases remain editable by occurrence through Browse. Before opening a named draft, a worker checks the whole current file and the exact unique alias, then returns to the current Search, root and foreground owner. [Named Edit checkpoint](../CHECKPOINTS.md#g13-canonical-snippets-search-and-named-edit)

Unique alias-only Run reaches checked Copy or standalone Fill through the same canonical Search snapshot. [Standalone Fill checkpoint](../CHECKPOINTS.md#g13-standalone-snippet-run-and-fill)

The editor supports ordered field labels, defaults, required state and input type. Test / Preview uses the unsaved candidate and restores the same draft. Remembered presentation, scoped Jump and matched Windows replay remain open. [Editor Preview checkpoint](../CHECKPOINTS.md#g13-snippets-field-authoring-and-editor-preview)
