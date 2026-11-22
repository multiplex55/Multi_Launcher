# Radial Runtime

G20 owns `radial-runtime`. Bare `radial` opens the effective default menu. A `radial show` result carries the exact menu ID. Typed show and close requests go through the main Radial controller, which also coordinates physical invocation. The native visual and input windows already have an independent Windows host.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-RUNTIME-01 | Keep typed default, exact ID, unique name and Close routing, feature and ownership gates, and one active or pending invocation. Reject stale preparation and session/frame/layout generations before opening or changing the host. |
| RADIAL-RUNTIME-02 | Preserve ring and wedge modes, style and audio, navigation, dynamic rows, tooltips and selection policies. Visual and input ordering must agree, including signed desktop coordinates, DPI, transparent holes, protected gaps and exterior dismiss. |
| RADIAL-RUNTIME-03 | Dispatch only the accepted current typed intent under the matching after-action and close acknowledgement rules. Preserve trigger-release and exclusive-action gates. Lifecycle cancellation retires the native input owner and clears queued handoffs. |

Select `show_and_close_issue_one_typed_request_and_preserve_root_state`, `exact_id_wins_before_unique_case_insensitive_name`, `native_region_plan_preserves_protected_gaps_and_optional_center_hole`, `native_event_routing_and_layout_hit_test_choose_the_same_topmost_overlap`, `asynchronous_preparation_rejects_stale_reply_before_host_open`, `item_shortcut_uses_the_normal_close_ack_handoff_exactly_once` and `lifecycle_cancel_after_selection_clears_handoff_before_close_ack_and_release`. Native and GPUI remain pending.
