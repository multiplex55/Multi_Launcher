# Screenshot Countdown

G18 owns `screenshot-countdown`. `ss delay 3`, `ss delay 5` or `ss delay 10` exposes typed delayed window/region/markup/desktop and clipboard variants. `ss cancel` cancels the current delayed capture. `ScreenshotCaptureOwner` owns request/phase/deadline, and `ScreenshotCountdownPresentation` owns the passive topmost Win32 countdown and its Cancel HWND.

| Criterion | Source-backed behavior |
| --- | --- |
| SHOT-COUNT-01 | Admit one exact supported delay without conflicting capture/recording owner. Show countdown seconds and a reachable nonactivating native Cancel, size to current DPI and verify actual HWND/thread/binding. Deadline alone does not grant capture. |
| SHOT-COUNT-02 | Require visible acknowledgement and then verified launcher park, exact hidden countdown acknowledgement, fresh request, current visibility and no conflicting owner before one capture grant. Exclude the owned elements. Cancellation before grant makes no capture. Cancellation after grant drains/ignores late pixels. |
| SHOT-COUNT-03 | Distinguish waiting, presentation, capture and draining, timeouts and failures. Restore only the current ownership and delivery policy after countdown retirement. Successful exact delivery alone records execution. |

Select `screenshot_delay_choices_are_typed_and_malformed_reserved_values_do_not_fallback`, `screenshot_delay_waits_for_expiry_and_an_explicit_current_grant`, `screenshot_countdown_hidden_runtime_waits_for_hide_and_parking_then_restores_by_delivery_policy`, `screenshot_countdown_native_cancel_before_grant_never_acquires_or_records_success`, `screenshot_countdown_cancel_after_child_close_drains_worker_and_restores_parking` and `screenshot_countdown_cancel_button_fits_client_area_at_scaled_dpi`. Existing fake HWND/capture probes avoid real screens. Native and GPUI evidence are pending.
