# Brightness

G24 owns `brightness`. Exact `bright` opens the Brightness dialog. Settings search can focus the accepted current level. The shared service owns monitor inventory, helper lifetime and native effects.

| Criterion | Source-backed behavior |
| --- | --- |
| BRIGHTNESS-01 | Preserve independent per-monitor drafts and explicit Set, refresh tickets, current/unavailable rows and error feedback. A stale monitor, request or incomplete inventory cannot accept Set. |
| BRIGHTNESS-02 | Preserve Cancel during refresh, exact cleanup and reopening after retirement. The helper checks monitor target, range and identity before native writes. |

Select `brightness_dialog_rows_keep_independent_drafts_until_explicit_set`, `brightness_dialog_refresh_does_not_retarget_a_removed_selection`, `brightness_dialog_cancel_remains_available_while_refresh_is_loading`, `brightness_dialog_reports_final_fake_cleanup_failure` and `brightness_service_rejects_stale_targets_and_applies_current_target_once`. Native and GPUI remain pending.
