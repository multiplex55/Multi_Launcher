# Mouse Gesture Hint Overlay

G21 owns `mouse-gesture-hint-overlay`. The running gesture service shows a passive Windows tooltip during an eligible gesture, with the configured foreground restrictions and hint setting. There is no independent preview command or non-Windows native tooltip host.

| Criterion | Source-backed behavior |
| --- | --- |
| GESTURE-HINT-01 | Preserve candidate and alternative bindings, delayed idle cheat sheet, matching progress and no-match feedback. Position from signed cursor coordinates and configured offset. Clear on release, cancel, disable or empty content and suppress repeated equivalent updates. |
| GESTURE-HINT-02 | Use the existing passive nonactivating click-through host. Preserve right-click/no-match behavior. Use a fake runtime for synthetic input until a native fixture can isolate click replay and dispatch. |

Select `passive_tooltip_native_policy_is_layered_transparent_and_nonactivating`, `passive_tooltip_native_callback_declines_mouse_hits_and_activation`, `no_match_and_cancel_behaviors_are_unchanged` and `deadzone_and_wheel_gate_semantics_are_unchanged`. Native and GPUI remain pending.
