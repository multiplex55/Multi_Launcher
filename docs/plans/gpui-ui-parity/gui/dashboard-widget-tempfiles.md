# Temporary Files Widget

G23 owns `dashboard-widget-tempfiles`. The `TempfilesWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-TEMPFILES-01 | Preserve current ordered files, alias and path, refresh, error and empty state. Add opens the existing draft. Explicit Open and Clear use the exact row path. Clear All uses the destructive confirmation policy. |

Use an explicit scratch `ML_TMP_DIR` and a fabricated file. Select `list_returns_existing_files`, `background_loader_is_nonblocking_single_flight_and_owned` and `typed_destructive_metadata_maps_all_eight_existing_protocols`.

Native and GPUI remain pending.
