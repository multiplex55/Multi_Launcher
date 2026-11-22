# Choose Launcher Action

G19 owns `mkmacro-launcher-picker`. In the [Action Editor](mkmacro-action-editor.md), Run Program or Launcher Command can open the picker against the existing authoring action snapshot. The picker owns search and current selection. The initiating editor owns the exact typed field and local draft.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-LAUNCHER-PICKER-01 | Search labels, descriptions, actions and arguments with stable source-index ordering. Keep the scoped eligible program or query rows, keyboard/pointer selection and current focus. Use Selected Program/Use Query Suggestion transfers exact fields. |
| MK-LAUNCHER-PICKER-02 | Preserve parsed program arguments and existing working directory. Explicit conversion asks Use Launcher Query instead or Keep Run Program. Accepted conversion changes the typed local action. Validate current action source and same editor/macro/step/draft before applying. |
| MK-LAUNCHER-PICKER-03 | Cancel/Escape clears the child and preserves its parent draft. Invalid, stale or retired requests have no effect except actionable feedback. Apply/Save/execution belong to their respective owners. |

Select `mkmacro_picker_chrome_keeps_confirmation_and_actions_above_long_results_without_mutation`, `matching_covers_every_field_case_insensitively_and_is_stable`, `process_transfer_parses_quotes_and_respects_working_directory`, `unsafe_resolved_targets_are_not_raw_queries` and `stale_picker_request_does_not_mutate_draft`. Native and GPUI evidence are pending.
