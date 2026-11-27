# Timer Completion

G23 owns `timer-completion`, the separate root Timer Finished dialog. The scheduler admits only the current occurrence, reports the typed local fact and applies configured sound. Completion dialog capability controls presentation.

| Criterion | Source-backed behavior |
| --- | --- |
| TIMER-FINISHED-01 | Show the accepted completion message and acknowledge it with OK or native close. Keep timer identity and generation checks at admission and ensure one sanitized local fact per occurrence. Preserve the current latest-message behavior when multiple completions drain. |

Select `notification_inbox_timer_completion_claims_only_current_occurrence_once`, `notification_inbox_timer_completion_emits_sanitized_typed_fact` and `timer_pause_resume_and_cancel_preserve_heap_generation_without_worker`. `G23-EGUI-TIMER-LOCAL-INBOX` verifies the actual named silent timer completion, OK and exact local record. Other modes and GPUI remain pending.

## GPUI completion

The shared scheduler publishes the latest successful completion to the selected GPUI session. The launcher presents it beneath Search with an exact OK action and retains it while hidden. Paired Windows replay and separate dialog parity remain open. [Timer and Alarm checkpoint](../CHECKPOINTS.md#p017-retained-timers-and-checked-alarms)
