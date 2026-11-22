# Unsaved Radial Cell Properties

G20 owns `radial-unsaved-cell-properties`, the Cell Properties display of the Designer's shared Scope Change Guard. A queued navigation or editor operation keeps the exact local Properties draft available while the guard owns the decision.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-CELL-DIRTY-01 | Apply and continue requires a current cell and valid draft. Stage query or exact command text through its explicit control first. Validate generation, content and graph before revalidating the queued operation. Replay-unsafe intents require a fresh request after Apply edits. |
| RADIAL-CELL-DIRTY-02 | Discard and continue retires local edits and performs a still-current intent. For a stale intent, discard staged edits and cancel the change. Cancel navigation, owned title close and Escape retain the draft and selection. |

Select `tree_search_navigation_guard_supports_cancel_discard_and_apply`, `properties_query_must_be_saved_before_navigation_apply_and_continue`, `properties_command_must_be_set_before_navigation_apply_and_continue` and `properties_action_text_is_discarded_only_by_explicit_discard`. Native and GPUI remain pending.
