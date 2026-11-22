# Insert Link

G12 owns `note-insert-link`. A current [Note Editor](note-editor.md) in Edit or Split opens it from Markdown → Insert Link. The source editor's Ctrl+. context menu also has the Markdown route. The independent retained `NoteFormLink` carries the exact editor and focus receipt.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-LINK-01 | Prefill the Text field from the captured Unicode selection and start URL empty. Editable draft fields clear prior inline error. Cancel or close retires only the form. Preserve draft and allowed Dock/Pop Out through the current owner. |
| NOTE-LINK-02 | Before applying, revalidate exact panel, editor, slug/path, host, revision/source and UTF-8 selection. A current Insert replaces that range with Markdown link syntax and uses the captured selection when Text is empty. Restore parent caret/focus and one undo step. Submit performs no navigation. |

Select `child_form_uses_captured_unicode_selection_and_rejects_stale_source`, `insert_link_replaces_selection`, `native_note_form_preserves_draft_and_applies_captured_selection` and `note_editor_context_jump_follows_the_keyboard_popup_and_unwinds_each_level`. A safe native smoke can select synthetic Unicode, inspect the prefill and Cancel. Native and GPUI evidence are pending.
