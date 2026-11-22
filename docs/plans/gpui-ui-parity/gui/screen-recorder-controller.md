# Recording Controller

G19 owns `screen-recorder-controller`, the detached Recording viewport of a current recorder generation. It appears from admitted [Screen Recorder Setup](screen-recorder-setup.md), including when the root is hidden. The recorder session, output writer and window backend own capture, finalization and publication.

| Criterion | Source-backed behavior |
| --- | --- |
| RECORD-CONTROLLER-01 | Show Preparing, countdown, elapsed/timed/Unlimited, Finalizing and terminal outcomes. Preserve exact native host while the launcher hides. Stop, Discard and focused countdown Escape use the current accepted generation. Native title close is canceled while the session is live. |
| RECORD-CONTROLLER-02 | Stop freezes acquisition and finalizes accepted frames. Discard before claim removes only owned staging. Failure, zero frames or stale target never reports a saved file. Preserve bounded capture, target loss, cancellation and no-clobber publication. |
| RECORD-CONTROLLER-03 | Retire controller and passive outline with exact finished generation. Setup close does not cancel an already admitted recorder. Reject retired or unaccepted completion. |

Select `recording_ui_hidden_controller_commits_only_retained_current_stop`, `recorder_actual_setup_hidden_stop_retires_and_releases_capture_admission`, `recording_controller_prepares_writer_before_countdown_and_publishes_only_after_capture_retirement`, `recording_session_stop_freezes_time_and_ignores_inflight_frame`, `recording_output_discards_staged_file_when_cancel_wins_before_claim` and `recording_output_collision_preserves_destination_and_fails_claimed_session`. Existing fake frame/backend fixtures avoid live capture. Native and GPUI remain pending.
