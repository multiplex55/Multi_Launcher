# Dashboard Reset Widget Confirmation

G23 owns `dashboard-reset-widget-confirmation`. Properties → Reset to defaults uses the selected widget descriptor. The destructive-confirmation setting controls whether this child is presented.

| Criterion | Source-backed behavior |
| --- | --- |
| DASHBOARD-RESET-01 | Bind reviewed default data to the current parent, distinct confirmation kind and exact selected row token. Fresh Confirm replaces only the widget settings in the draft. Cancel and native close keep them. Save owns the runtime change. |

Select `jump_mode_dashboard_editor_selection_and_reset_confirmation_need_fresh_activation` and `jump_mode_dashboard_native_confirmation_requires_exact_viewport_and_child_layer`. Native and GPUI remain pending.
