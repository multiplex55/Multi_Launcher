# Quick Notes

G12 owns `quick-notes`. The `notes` plugin accepts the bare `note` or `notes` query and emits `note:dialog` for the Quick Notes panel. `src/gui/notes_dialog.rs` owns the retained list, inline draft and row actions. `src/plugins/note.rs` owns source files, slugs and the shared notes cache.

| Criterion | Source-backed behavior |
| --- | --- |
| QUICK-NOTES-01 | Search note content, aliases, slugs and tags case-insensitively. Show stable, virtualized rows and bounded previews. Keep the last good derived result and defer refresh while the inline draft is active. |
| QUICK-NOTES-02 | Add Note and row Edit open the inline multiline draft. Save requires nonempty input, derives a new title from the first line, uses the existing slug on edit and retains the draft on failure. Successful saves update the shared cache and command results. The draft context can insert a timestamp at the caret. |
| QUICK-NOTES-03 | Row Open or owned row Jump Mode opens the exact [Note Editor](note-editor.md). Preserve row action ownership for Edit, Open externally, Copy link/slug, aliases, templates, link wrapping, Remove and Link to todo. Support the Unused Assets and optional Templates entry routes. |
| QUICK-NOTES-04 | Preserve remembered Dock/Pop Out and geometry, retained draft and scroll, exact row and child ownership, and stale host action rejection. Refresh remains usable across live host changes. |

Select `note_root_query_returns_actions_in_order`, `quick_notes_browsing_virtualizes_rows_and_reuses_geometry`, `quick_notes_defers_drafts_and_keeps_last_good_candidate_until_recovery`, `failed_quick_notes_edit_retains_dialog_and_committed_note`, `quick_notes_visible_host_buttons_move_the_same_draft_and_discard_stale_host_intent`, `note_open_receipt_rejects_closed_reopened_and_docked_list` and `jump_mode_notes_rows_use_visible_slug_identity_and_fresh_enter_opens_exact_note` when covering the respective boundary. `G12-EGUI-QUICK-NOTES-EDITOR` covers the actual launcher, body-text filter, multiline Add Note draft and Cancel retaining the filter and source. Row context Open, Save, host changes and GPUI remain pending.
