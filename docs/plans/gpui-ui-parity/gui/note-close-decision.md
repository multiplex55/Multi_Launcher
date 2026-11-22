# Note Close Decision

G12 owns `note-close-decision`, runtime ID `note-close-prompt`. The owner is the initiating [Note Editor](note-editor.md) in Dock or Pop Out. Window close and supported close keys pass through the existing save-on-close and dirty-confirmation policies.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-CLOSE-01 | Show Save Changes, Discard Changes and Keep Editing for an eligible dirty draft. Clean and direct-save paths retain their configured behavior. A failed save-on-close keeps the note and retry path live. An existing conflict may remain the current decision. |
| NOTE-CLOSE-02 | Save closes after accepted success, Discard closes without saving, and Keep Editing, Escape or title close returns to the live draft and focus. Only the exact top editor prompt may accept the input. |
| NOTE-CLOSE-03 | Gate effects by panel instance, host generation, prompt epoch, source revision and accepted root/owner frame. Discarded frames restore draft and cancel proposed native close. |

Select `note_panel_auto_saves_on_close`, `note_panel_remains_closable_after_auto_save_failure_then_retry`, `studio_usability_note_close_failed_autosave_keeps_draft_and_table_builder_for_retry`, `studio_usability_note_close_prompt_can_save_and_conflict_prompts_are_panel_owned` and `native_note_close_prompt_admits_editor_focus_and_owns_escape`. Verify Keep Editing then Discard against an isolated note with unchanged backing bytes. Native and GPUI evidence are pending.
