# Screen Color Picker

G18 owns `screen-color-picker`. Exact `color pick` emits `color:pick` and reaches the Windows native overlay. `ColorPickLifecycle` owns current authority, parking and old query/selection. The frozen native picker owns local pixels and exact RGB sampling. The typed color route owns formatted color results.

| Criterion | Source-backed behavior |
| --- | --- |
| SCREEN-COLOR-01 | Admit one current pick only with eligible root and no competing native owner. Park the launcher and verify actual off-desktop state before acquisition. Reject newer visibility/focus/geometry or expired worker/session outcomes. |
| SCREEN-COLOR-02 | Show the owned native topmost preview, magnifier and crosshair using the same immutable signed-coordinate snapshot. Accept one exact current RGB selection. Escape, native close, display/DPI failure or timeout cancels. Retire HWND before publishing. |
| SCREEN-COLOR-03 | Restore actual geometry/focus using current visibility ownership and retain the transaction on recoverable restore failure. Only current success changes the query to typed color and opens equivalent Hex/RGB/HSL results. This action does not persist the default color or publish frozen screen pixels. |

Select `signed_mixed_layout_bounds_and_exclusive_edges_select_exact_rgb`, `park_then_later_verify_precedes_single_frozen_capture_and_cleanup`, `cancellation_waits_for_native_cleanup_and_allows_reopening`, `color_pick_picked_uses_existing_results_after_cleanup_and_exact_restore`, `color_pick_new_visibility_cancels_preserves_new_query_and_native_placement`, `color_pick_restore_failure_retains_transaction_and_outcome_until_retry` and `screen_pixel_and_typed_hex_use_identical_results`. Use injected generated pixels for a fixture; the production overlay captures the desktop. Native and GPUI evidence are pending.
