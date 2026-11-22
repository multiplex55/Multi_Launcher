# Dashboard Remove Slot Confirmation

G23 owns `dashboard-remove-slot-confirmation`. Remove opens a child for the exact current slot when the Remove controls are shown. The editor draft owns the candidate change.

| Criterion | Source-backed behavior |
| --- | --- |
| DASHBOARD-REMOVE-01 | Confirm requires the accepted visible child, matching parent and confirmation instances, current slot token and fresh activation. Remove only that draft slot and update presentation, selection and swap metadata. Cancel and native close preserve the draft. |

Select `jump_mode_dashboard_native_confirmation_requires_exact_viewport_and_child_layer` and `jump_mode_designer_ownership_quick_tools_rows_follow_move_and_retire_on_remove`. `G23-EGUI-DASHBOARD-QUICK-TOOLS` verifies the exact one-slot decision and Cancel. Confirm and GPUI remain pending.
