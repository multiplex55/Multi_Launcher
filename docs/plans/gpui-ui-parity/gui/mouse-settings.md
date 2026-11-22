# Mouse Settings

G22 owns `mouse-settings`, the retained Mouse form opened by `mouse settings`, `mouse click settings` or the Settings cross-route. The form owns the appearance draft. The app coordinate controller owns session effects and merges accepted preferences.

| Criterion | Source-backed behavior |
| --- | --- |
| MOUSE-SETTINGS-01 | Preserve remembered dock and popout, accepted retained frame, exact draft and baseline through host changes, and appearance sections for click, crosshair, halo, magnifier and coordinates. Session toggles apply separately and immediately. |
| MOUSE-SETTINGS-02 | Apply merges edited fields into the latest preferences, retains errors and draft on failure and leaves the clean form open on success. Cancel and native retirement discard appearance changes. Reopen reads current preferences. |

Select `mouse_settings_command_opens_and_focuses_idle_panel`, `jump_mode_mouse_settings_focuses_checkbox_and_slider_then_cancel_discards_draft`, `mouse_settings_popout_stays_live_when_root_hides_and_docks_back`, `failed_apply_keeps_the_draft_and_error_visible` and `draft_commit_merges_only_edited_fields_into_the_latest_settings_transaction`. Native and GPUI remain pending.
