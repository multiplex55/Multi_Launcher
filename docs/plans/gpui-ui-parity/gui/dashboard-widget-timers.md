# Active Timers

G23 owns `dashboard-widget-timers`. The `ActiveTimersWidget::settings_ui` callback edits this slot's draft configuration through the Dashboard Editor. Runtime actions require the accepted current slot and dashboard pass.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-TIMERS-01 | Show live countdown order and optional latest completions. Retain count and completion settings, existing New Timer draft route and exact current timer Pause/Cancel actions. |

Select `new_timer_action_uses_existing_dialog_route_without_overrides` and `timer_pause_resume_and_cancel_preserve_heap_generation_without_worker`. Use an empty scratch scheduler or synthetic owner.
