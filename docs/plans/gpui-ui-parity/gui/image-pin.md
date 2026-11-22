# Screenshot Image Pin

G22 owns `image-pin`, the deferred per-pin window after accepted static region capture or live window selection. The capture coordinator reserves the shared selector before parking the root and existing pins. Static and live sources use their own capture paths.

| Criterion | Source-backed behavior |
| --- | --- |
| IMAGE-PIN-01 | Require current operation, generation and native hide acknowledgements before selection, then restore original visibility before publication or feedback. Preserve the exact live HWND and latest complete frame. Respect capture and memory budgets. |
| IMAGE-PIN-02 | Bind native controls to pin ID, lifetime and validated window identity. Preserve Fit, 1:1, Zoom, live Pause/Resume, opacity, topmost, click-through and recovery. Close retires only the exact owner. |
| IMAGE-PIN-03 | Copy and Save As use canonical static or latest complete live pixels. Preserve exact native chooser owner, cancel, PNG destination and overwrite behavior. Accepted write jobs have their own completion lifecycle. Use generated pixels and injected authorizations for isolated checks. |

Select `fake_capture_waits_for_both_native_acknowledgements_and_restores_before_commit`, `pinboard_live_selection_resolves_source_then_restores_hidden_root_without_snapshot`, `pin_controls_use_exact_child_commands_and_reject_invalid_owners` and `pin_opacity_ui_toolbar_and_pin_list_share_validated_state`. Native and GPUI remain pending.
