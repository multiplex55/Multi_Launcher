# Screen Draw Toolbar

G22 owns `screen-draw-toolbar`, the independent fixed topmost toolbar. The exact `sd toolbar` route shows the toolbar while idle. Start and New Capture separately request capture through the app controller.

| Criterion | Source-backed behavior |
| --- | --- |
| SCREEN-DRAW-BAR-01 | Opening the idle toolbar exposes defaults, orientation, position and Close without capturing. Start checks current surface and exclusive selection, parks the exact launcher and waits for admission. New Capture retires the native generation before replacement. |
| SCREEN-DRAW-BAR-02 | Preserve Drawing, Ghost, Finish and display-change controls, tool/background/history, checked transition, error/recovery, and explicit full or region export to Copy, PNG or Screenshot Editor. Reject stale capture and export responses. |

Select `launcher_host_routes_screen_draw_commands_only_through_the_controller`, `idle_preference_actions_update_defaults_without_starting_a_session`, `toolbar_viewport_policy_is_fixed_topmost_and_absent_from_taskbar`, `hidden_root_ui_registers_independent_screen_draw_toolbar` and `screen_draw_recovery_cancels_both_capture_startup_states_and_restores_exactly`. Native and GPUI remain pending.
