# MkMacro Step Search

G19 owns `mkmacro-step-search`. [MkMacro](mkmacro.md) Edit steps or current table shortcuts open Find, Replace or Jump to Step. The independently hosted search owns current inputs and navigation. Existing authoring fields and mutation own preview and replacement semantics.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-STEP-SEARCH-01 | Find case-insensitively matches current step type, fields and details. Jump can show all rows. Previous/Next wrap where applicable and reveal the exact stable step through contained folds and scroll. Return focus to the current editor. |
| MK-STEP-SEARCH-02 | Replace uses exact case-sensitive Find/Replace and a required affected-field old/new preview. Replace Current or All validates current owner, macro and revision and applies one transaction. Staleness reports and preserves current draft. |
| MK-STEP-SEARCH-03 | Preserve scoped focus, existing active editor/modal precedence, retained host/epoch and current presentation. Close retires the child and keeps draft. Search never executes or saves the macro. |

Select `hosted_replace_rejects_stale_basis_and_applies_once`, `next_previous_wrap_and_empty_matches_have_stable_source_order`, `replace_apply_is_one_revision_and_rejects_stale_preview_or_macro_switch`, `search_keyboard_ownership_respects_text_fields_and_table_modal_routing`, `native_scaled_metrics_keep_search_and_table_controls_visible_without_mutation` and `mkmacro_native_parent_opens_ordinary_children_with_retained_origin`. Native and GPUI evidence are pending.
