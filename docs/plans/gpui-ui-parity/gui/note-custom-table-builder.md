# Note Custom Table Builder

G12 owns `note-custom-table-builder`. The [Note Editor](note-editor.md) Markdown → Table → Insert custom table action opens `NoteForm<Table>` in Edit/Split. It carries the exact panel, host, slug/path/revision, editor, source and UTF-8 range.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-TABLE-01 | Default to two columns and one data row with editable headers. Support 1–256 rows and columns, resize the header fields with column count, and retain reachable Insert/Cancel and bounded scroll. |
| NOTE-TABLE-02 | Insert at the selection start while retaining selected text. Escape cell markup, preserve dominant newline and enforce UTF-8, cell count and source budgets. Apply one undoable edit and restore editor focus/caret under the existing save policy. |
| NOTE-TABLE-03 | Preserve the retained form through current owner/host changes. Validate exact source and selection at open and submit. Retire on parent close. Invalid or stale input stays retryable, and Cancel leaves note content unchanged. |

Select `studio_usability_custom_table_keeps_note_and_caret_owner_across_query_and_second_note`, `custom_table_builder_refuses_a_changed_origin_source`, `discarded_custom_table_insert_leaves_builder_retryable`, `custom_table_builder_keeps_insert_and_cancel_visible_in_a_small_window`, `custom_table_insertion_escapes_names_and_preserves_crlf` and `custom_table_insertion_rejects_invalid_dimensions_and_excess_bytes`. Native smoke can adjust dimensions on a synthetic note and Cancel, then compare source bytes. Native and GPUI evidence are pending.
