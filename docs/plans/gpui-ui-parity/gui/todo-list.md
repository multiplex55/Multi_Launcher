# Todo List

G23 owns `todo-list`. Bare `todo` offers the typed editor route. The list uses the canonical todo file and reads it on a real open. Legacy entries without IDs can migrate on load.

| Criterion | Source-backed behavior |
| --- | --- |
| TODO-LIST-01 | Preserve text, tags, priority, Persist Tags, Add/Enter, existing filter grammar, stable choice identity and the owned local Note picker. Warm reopen retains an existing draft. |
| TODO-LIST-02 | Completion and Remove commit through the existing store. Clear Completed requires the exact inline confirmation. Invalid disk data leaves last-good state visible and read-only. |

Select `todo_row_widget_ids_stay_bound_to_entry_when_filter_moves_another_row_to_zero`, `enter_adds_todo_with_filter_focus`, `clear_completed_requires_confirmation`, `invalid_reload_and_commit_keep_last_good_and_read_only` and `pinned_open_preserves_drafts_and_reload_waits_for_a_real_close`. Native and GPUI remain pending.
