# Jump Mode Overlay

G05 owns `jump-mode-overlay`. Jump Mode is an optional overlay for the exact current UI owner. It defaults off and uses a configurable activation shortcut.

| Criterion | Source-backed behavior |
| --- | --- |
| JUMP-OVERLAY-01 | Paint only current visible, enabled and owned targets. Enforce exact viewport, pass, popup and modal fences and handle repeat/cancel. Root result hint selection keeps the exact Action and returns focus to Search. Fresh normal activation performs the effect. |

Select `jump_mode_root_result_hints_select_without_activation_in_list_and_grid_then_fresh_enter_activates_full_action`, `jump_mode_root_result_registry_is_empty_while_context_menu_owns_input`, `jump_mode_root_input_deduplicates_text_repeats_and_cancels_without_leaks` and `jump_mode_owned_viewport_binding_is_exact_and_mismatched_context_cannot_steal_input`.

Native and GPUI remain pending.
