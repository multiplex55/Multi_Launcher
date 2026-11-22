# Launcher Action Confirmation

G05 owns `launcher-action-confirmation`. The launcher retains the pending typed command or universal-action invocation before opening its existing confirmation.

| Criterion | Source-backed behavior |
| --- | --- |
| LAUNCH-CONFIRM-01 | Confirm consumes and dispatches the exact reviewed request once. Revalidate current feature owners as applicable. Cancel and title close discard it. Keep this confirmation separate from Data Recovery's owned decision. |

Select `pending_confirmation_retains_typed_invocation_metadata`, `single_macro_destructive_result_uses_confirmation_before_exactly_one_execution` and `long_confirmation_keeps_actions_visible_and_window_close_cancels_once`.

Native and GPUI remain pending.
