# Dashboard

G23 owns `dashboard`, the launcher root composition for a blank query when both dashboard flags are enabled. The canonical layout, widget registry and runtime own slots, instance data, repaint and rendering. Dashboard Settings opens the separate editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASHBOARD-01 | Preserve empty-query routing, configured grid and scroll, the standard initial composition, and invalid-file warning with fallback. Keep launcher query/results and dashboard input owners coordinated. |
| DASHBOARD-02 | Resolve actions by current composition generation and stable slot/widget identity. Retain all 39 registered widgets, including 36 settings callbacks and the three render-only descriptors. Keep widget-specific effects in existing owners. |

Select `registry_defaults_are_deterministic_and_include_expected_widgets`, `metadata_reports_settings_ui` and `jump_mode_dashboard_composition_collects_and_activates_only_the_pinned_action`. `G23-EGUI-DASHBOARD-QUICK-TOOLS` verifies blank-query composition and the configured manual action. GPUI retains the watched stock layout beneath the anchored Search field when both Dashboard flags allow it and a supported slot has real content. Active Timers and manual Quick Tools have focused evidence. Blank Browse returns when the host is disabled or no supported slot is eligible. Paired Windows replay, full grid and geometry parity, Editor and remaining widgets stay open. [Dashboard checkpoint](../CHECKPOINTS.md#p101-retained-dashboard-with-active-timers-and-manual-quick-tools)
