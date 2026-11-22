# Date & Time Studio

G16 owns `date-time-studio`. Exact `date studio` emits `date:studio` and reaches the typed dialog route. The dialog owns retained drafts, frozen reference and current pickers. The existing date arithmetic engine owns typed Between, Offset, Time Zone and Schedule results.

| Criterion | Source-backed behavior |
| --- | --- |
| DATE-STUDIO-01 | Between supports two dates, optional local times, signed duration/day/week differences and clipped-calendar span. Add/Subtract uses typed anchor, amount, unit and sign. Use frozen now and Refresh now preserve explicit input relationships. |
| DATE-STUDIO-02 | Time Zones supports date/time and named source, searchable ordered targets and explicit fold choice/gap status. Preserve owned date/time/zone pickers and shared conversion results. |
| DATE-STUDIO-03 | Schedule uses typed cadence and bounded rows, occurrence selection, fold/gap/resolved status and current result copy. Prepare unsaved Calendar entry carries the exact current minute-aligned resolved instant to the eligible existing G23 editor. |
| DATE-STUDIO-04 | Retain current draft and reference across tabs, accepted Dock/Pop Out and warm reopen. Durable host size/mode persists separately. Retire old popup/focus owner and require exact current frame for clipboard or calendar effects. |

Select `date_time_studio_has_one_exact_typed_route`, `date_time_studio_date_picker_and_keyboard_time_edit_update_between_inputs`, `date_time_studio_fold_choice_is_explicit_and_clears_when_source_changes`, `date_time_studio_schedule_selection_survives_and_copy_matches_selected_row`, `date_time_studio_calendar_prepare_effect_uses_the_selected_resolved_instant` and `date_time_studio_host_moves_preserve_draft_reference_selection_and_popup_ownership`. `G16-EGUI-CSV-DATES` verifies real Between input, exact calendar span and warm reopen. Other modes, effects, host and GPUI remain pending.
