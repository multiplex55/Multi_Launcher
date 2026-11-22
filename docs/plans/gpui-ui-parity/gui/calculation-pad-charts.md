# Calculation Pad Charts

G15 owns `calculation-pad-charts`, runtime navigation `calculation-chart`. [Calculation Pad](calculation-pad.md) Config Menu → Tools → Charts opens the independent companion using the current Pad receipt. Shared `ChartDataset` and `ChartScene` own prepared chart, hit testing and image output for the current definition and data.

| Criterion | Source-backed behavior |
| --- | --- |
| CALC-CHART-01 | Line, Bar and Scatter accept current named/physical vectors or sensitivity results, index or explicit X and up to eight Y series. Histogram and Cumulative use a distinct current numeric vector, with staged bins, count/percentage and virtualized summary. |
| CALC-CHART-02 | Preserve source-keyed labels, styles and visibility, bar labels, title, grid, legend, finite axis/tick/linear/log options and explicit/current named reference lines. Options apply/cancel/reset has an isolated draft. Preserve plot pan/zoom and raw-value tooltips for the live identity. |
| CALC-CHART-03 | Bind results and reference lines to exact Pad document/incarnation, current evaluation/sensitivity, definition and host. Accept explicit Copy Chart as Image only for the current scene and settled options. Show image/clipboard failures. |
| CALC-CHART-04 | Persist committed chart metadata through the existing document/recovery owner. Keep transient scene/options and preview state through allowed host movement. Pad close retires the chart. |

Select `chart_open_requires_retained_current_pad_owner`, `chart_jump_targets_follow_docked_and_native_owner`, `chart_options_are_staged_and_reset_axes_without_resetting_view`, `chart_distribution_controls_preserve_source_and_stage_bins`, `chart_named_reference_uses_current_evaluation_and_sensitivity_stamp`, `chart_copy_uses_scene_and_surfaces_clipboard_failure` and `chart_definition_roundtrip_and_rename_history_keeps_independent_selection`. Native and GPUI evidence are pending.
