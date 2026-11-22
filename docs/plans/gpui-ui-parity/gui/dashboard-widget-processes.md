# Processes Widget

G23 owns `dashboard-widget-processes`. The `ProcessesWidget::settings_ui` callback owns the selected widget configuration draft through the Dashboard Editor.

| Criterion | Source-backed behavior |
| --- | --- |
| DASH-PROCESSES-01 | Retain refresh, PID-grouped rows and row limit. Switch and Kill are explicit live process actions keyed by current PID. Keep their effects separate from refresh and validate the current target in parity checks. |

Select `cached_process_results_preserve_modes_and_filtering`, `requests_arriving_during_work_form_a_followup_batch` and `static_and_manual_widgets_are_event_driven`. Use generated process identities without invoking Kill.

Native and GPUI remain pending.
