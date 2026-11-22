# Link New Note

G12 owns `note-link-new`. In a current [Note Editor](note-editor.md) in Edit or Split, use the source context menu (Ctrl+.) → Insert link → Link new. `NoteFormNewNote` owns the retained Name draft. `NotePanel` validates exact origin and the Notes plugin resolves and publishes new targets.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-LINK-NEW-01 | Start with a blank name and require a trimmed nonempty value. Preserve current child owner, placement and draft. Cancel or close has no content effect. |
| NOTE-LINK-NEW-02 | Validate exact note, editor, host, revision/source and Unicode selection. Resolve the first current existing target or compute the new slug and build the exact wiki link. Self-link is a no-op. Only a missing target calls the canonical note publisher. |
| NOTE-LINK-NEW-03 | Apply the prepared edit with focus/caret and undo semantics. A failed target publish retains the form and original source. Source undo does not undo a successfully created target file. |

Select `child_new_note_never_appends_for_stale_basis`, `link_new_creates_only_when_missing_and_inserts_exact_wiki_syntax`, `child_new_note_self_link_is_a_noop`, `insert_or_create_note_link_replaces_selection_and_sets_cursor` and `insert_or_create_note_link_empty_name_does_not_mutate_content`. Native and GPUI evidence are pending.
