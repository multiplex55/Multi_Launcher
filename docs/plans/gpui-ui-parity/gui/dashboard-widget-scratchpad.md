# Scratchpad

G23 owns `dashboard-widget-scratchpad`. The `ScratchpadWidget::settings_ui` callback owns this slot's configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-SCRATCHPAD-01 | Preserve text, refresh, timestamp insertion and explicit Copy. Scope debounced JSON writes and loads to current path, generation and content. Protect dirty text against stale loads and malformed source against replacement. |

Use an explicit fixture path. Select `jump_mode_dashboard_simple_controls_scratchpad_focus_and_discarded_input`, `stale_load_from_previous_path_cannot_overwrite_current_edit` and `missing_and_empty_initialize_but_malformed_storage_is_preserved`.

Native and GPUI remain pending.
