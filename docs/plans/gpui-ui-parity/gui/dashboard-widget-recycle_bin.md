# Recycle Bin Widget

G23 owns `dashboard-widget-recycle_bin`. The `RecycleBinWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-RECYCLE-01 | Preserve read-only snapshot totals, empty/unavailable state, refresh and separate Clean confirmation. Clean calls the actual Windows cross-drive operation. Verify the completion/failure contract with a fake backend before acceptance. |

Use a synthetic totals snapshot. Select `typed_destructive_metadata_maps_all_eight_existing_protocols` and `destructive_confirmation_supports_queue_confirm_and_cancel_paths` without emptying the real bin.

Native and GPUI remain pending.
