# MkMacro Runtime Prompt

G19 owns `mkmacro-runtime-prompt`. Executing `MkAction::PromptInput` interpolates title, label and default, then submits a typed ID through `PromptBroker`. The launcher takes one current request and creates the scoped independent native prompt. The executor owns typed downstream variable and optional clipboard effects.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-RUNTIME-PROMPT-01 | Show current title/prompt and focused single-line default. OK or accepted Enter sends literal text exactly once. Cancel, Escape, window close or app exit returns Cancelled and cleans the request/UI. |
| MK-RUNTIME-PROMPT-02 | Preserve one current slot, repaint and hidden-root bootstrap without moving or focusing the launcher. Handle stopped/disconnected runtime safely. Optional clipboard applies only through the executor's configured accepted response. |

Select `hidden_macro_prompt_bootstraps_child_without_moving_or_focusing_root`, `hidden_macro_prompt_acquisition_schedules_root_before_child_ui`, `interpolation_uses_current_variables_and_prompt_answers` and `clipboard_action_integration_prompt_copy_is_explicit_and_cancel_safe`. Existing synthetic broker/executor fixtures avoid live macro operations. Native and GPUI remain pending.
