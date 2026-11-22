# Screenshot Pins

G22 owns `pinboard`, the root-layer list opened by `pinboard list` or Apps → Pin List. The typed route shows and focuses the launcher if needed. The presenter shares live pin state with independent native pin windows.

| Criterion | Source-backed behavior |
| --- | --- |
| PIN-LIST-01 | Preserve empty and populated states, stable row identity, live status, Pause/Resume, topmost, click-through, opacity and reset. Focus Recover All when recovery is available, otherwise Close All when pins exist. The empty list has no action focus. |
| PIN-LIST-02 | Recover, Close, Recover All and Close All act only on current live IDs and owners. Preserve existing eight-pin and byte budgets and the separation between listing, static capture and live source selection. |

Select `pinboard_typed_actions_reach_the_gui_host_without_generic_visibility_or_focus_policy`, `pin_list_is_keyboard_focusable_and_stale_rows_cannot_reopen_closed_pins` and `pin_opacity_ui_toolbar_and_pin_list_share_validated_state`. `G22-EGUI-PIN-LIST-IDLE-TOOLBAR` verifies the empty list, disabled Recover All and dismissal. Live rows and GPUI remain pending.
