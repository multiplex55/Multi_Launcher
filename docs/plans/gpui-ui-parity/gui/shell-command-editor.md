# Shell Commands Editor

G06 owns `shell-command-editor`. Bare `sh` opens the saved-command authoring form. Shell execution and the WezTerm plugin setting are separate routes.

| Criterion | Source-backed behavior |
| --- | --- |
| SHELL-EDITOR-01 | Preserve Add/Edit/Remove, required name and multiline command, Enter and Shift+Enter newlines, Keep command prompt open, stored autocomplete and Cancel. Accepted Save persists through the checked store. Invalid data retains last-good state. The form does not launch a process. |

Select `enter_inserts_newline`, `shift_enter_inserts_newline`, `opening_error_and_pinned_close_preserve_draft_until_reopen` and `invalid_reload_and_commit_keep_last_good_and_read_only`. `G06-EGUI-FAVORITES-SHELL-DRAFTS` verifies multiline draft and Cancel.

Save and GPUI remain pending.
