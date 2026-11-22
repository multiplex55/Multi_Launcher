# Timer and Alarm Authoring

G23 owns `timer-manager`. Bare `timer` and `alarm` open the same retained authoring host in their respective modes. List commands produce launcher results. The scheduler owns admitted timers and alarms.

| Criterion | Source-backed behavior |
| --- | --- |
| TIMER-FORM-01 | Preserve duration and local date/time controls, label, sound selection and explicit preview. Fresh mode opens a fresh draft with no sound. Dock and popout retain the same draft and focused field. |
| TIMER-FORM-02 | Start acts once through the accepted owner and current local time. Validation and start failure keep the editable draft. Cancel and dismissal leave the scheduler unchanged. |

Select `timer_and_alarm_authoring_keep_their_drafts_across_actual_host_moves`, `actual_duration_picker_edits_submit_exact_timer_once`, `alarm_preview_and_start_use_the_current_local_clock` and `start_failure_stays_open_and_cancel_does_not_submit`. Native and GPUI remain pending.
