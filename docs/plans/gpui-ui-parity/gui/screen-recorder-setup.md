# Screen Recorder Setup

G19 owns `screen-recorder-setup`, utility `screen-recording`. Exact `record` exposes separate Open Screen Recorder and Stop Screen Recording typed actions. The dialog owns revisioned setup, target selection, scoped preview and native chooser receipt. The recorder backend owns acquisition and output after explicit admission.

| Criterion | Source-backed behavior |
| --- | --- |
| RECORD-SETUP-01 | Render Display/region or Window, validated exact target identity, passive region outline, Refresh, GIF/MP4, FPS, scale and cursor controls. Preserve duration input/step controls, 0 Unlimited or whole 1–1000 seconds and current dimension/status. |
| RECORD-SETUP-02 | Choose output and record freezes the exact valid request for the current host. Validate request and HWND both sides of chooser, and recheck live target and capture/recording conflicts. Cancel or stale result starts no job. |
| RECORD-SETUP-03 | Preserve accepted dock/popout draft, epoch and passive preview ownership. Close retires setup resources while [Recording](screen-recorder-controller.md) owns current active session. |

Select `record_query_is_exact_case_insensitive_and_lists_only_recording_controls`, `open_and_stop_share_the_typed_recording_host_without_starting_on_open`, `recording_setup_cancelled_path_does_not_admit_work`, `recording_duration_retained_edit_and_choose_path_freeze_request`, `recording_ui_reciprocal_capture_guards_reject_before_mutation` and `recorder_setup_hosts_preserve_real_controls_popup_and_stale_region`. `G19-EGUI-LEGACY-DRAFT-RECORDER-SETUP` covers native invalid/Unlimited duration and chooser Cancel without capture. Active admission, target/host variants and GPUI remain pending.
