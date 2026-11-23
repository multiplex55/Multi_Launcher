# Jump Mode Overlay

G05 owns `jump-mode-overlay`. Jump Mode is an optional overlay for the exact current UI owner. It defaults off and uses a configurable activation shortcut.

| Criterion | Source-backed behavior |
| --- | --- |
| JUMP-OVERLAY-01 | Paint only current visible, enabled and owned targets. Enforce exact viewport, pass, popup and modal fences and handle repeat/cancel. Root result hint selection keeps the exact Action and returns focus to Search. Fresh normal activation performs the effect. |

Select `jump_mode_root_result_hints_select_without_activation_in_list_and_grid_then_fresh_enter_activates_full_action`, `jump_mode_root_result_registry_is_empty_while_context_menu_owns_input`, `jump_mode_root_input_deduplicates_text_repeats_and_cancels_without_leaks` and `jump_mode_owned_viewport_binding_is_exact_and_mismatched_context_cannot_steal_input`.

The egui baseline and exact-build fallback replay verify visible List and Grid hints, exact row highlight and reveal, unchanged Search, separate Enter activation, Escape and the configured shortcut and alphabet. [Validation log](../VALIDATION_LOG.md#g05-native-root-grid-jump-mode-baseline)

The Windows GPUI Search slice has exact-build native evidence for clipped List and Grid targets, current membership, the Search caret, independent fresh activation, Hash focus transfer and Alt+Space. The focused GPUI, shared configuration and egui cases passed. The final same-executable replay restored egui keyboard-only Jump Mode and retained the custom GPUI path. Dashboard, completion, popup and utility targets remain tracked separately. [Validation log](../VALIDATION_LOG.md#g05-gpui-search-jump-mode-native-checkpoint)
