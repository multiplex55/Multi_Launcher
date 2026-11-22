# Note Save Conflict

G12 owns `note-save-conflict`, runtime ID `note-conflict-prompt`. The [Note Editor](note-editor.md) shows Note exists when the canonical writer reports an occupied other name/path under the `note_always_overwrite` policy. It belongs to the initiating panel and current host.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-CONFLICT-01 | Show Overwrite, Save as New and Cancel for the current candidate. Overwrite submits the candidate with overwrite enabled. Save as New requests a fresh slug and path. Cancel, Escape or title close leaves the draft and clears pending close. |
| NOTE-CONFLICT-02 | Install resulting identity, clear dirty and finish pending close after accepted success. Keep original draft and decision on failure and surface the error. A cache publication error can follow a disk commit. |
| NOTE-CONFLICT-03 | Only the exact panel instance, current host, prompt epoch, source revision and top decision may apply effects. Retain the existing name/path collision semantics. |

Select `studio_usability_note_close_save_as_new_publishes_identity_only_after_success`, `studio_usability_note_close_successful_save_as_new_closes_with_new_identity`, `note_conflict_prompt_jump_focus_then_escape_keeps_draft` and `studio_usability_note_close_prompt_can_save_and_conflict_prompts_are_panel_owned`. On a case-insensitive scratch filesystem, a case-variant `.MD` sentinel can stage the collision, then Cancel and compare bytes. Native and GPUI evidence are pending.
