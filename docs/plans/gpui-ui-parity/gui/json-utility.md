# JSON Utility

G14 owns `json-utility`. The `json_utility` plugin sends `json_utility:open` for bare `json`, `json_utility:format` for `json format` or `json pretty`, and `json_utility:minify` for `json minify`. Typed Open initializes from valid protected clipboard JSON. Pinned reopen reuses the existing utility without initialization. `JsonUtilityDialogState` owns the retained editor and projection workers.

| Criterion | Source-backed behavior |
| --- | --- |
| JSON-UTIL-01 | Text edits the exact source. Format and Minify preserve object ordering, update the text only and show precise invalid input diagnostics without rewriting it. Clear resets, explicit Paste supports repair and Copy Result validates the exact current text. |
| JSON-UTIL-02 | Tree shows virtualized structured rows, typed key/index paths, branch expand/collapse, stable selection and bounded details. Copy Value, Copy Subtree and Copy JSON Pointer preserve their distinct typed data and escaping. |
| JSON-UTIL-03 | Search decoded keys, values or both literally and case-sensitively, clear search and wrap navigation. Reveal the exact current match. Preserve surviving typed expansion/selection and reject stale, pending or invalid snapshots for copy/details. |
| JSON-UTIL-04 | Preserve retained host, geometry, exact source/search revisions and coalesced worker. Typed fresh open resets the draft using its protected-source policy. Rehost/pinned reopen preserves current semantics and true close retires stale effects. |

Select `json_query_family_maps_to_general_format_alias_and_minify_intents`, `json_utility_dispatch_opens_the_requested_mode_without_other_side_effects`, `each_open_loads_only_valid_strict_clipboard_json_and_keeps_intent`, `parse_failure_preserves_exact_input_and_location_and_never_copies`, `worker_discards_stale_results_and_coalesces_pending_source`, `tree_copy_actions_are_explicit_and_keep_string_value_vs_subtree_semantics`, `tree_search_caches_matches_wraps_and_reveals_collapsed_results` and `pending_invalid_and_stale_snapshots_cannot_enable_tree_copy`. `G14-EGUI-STRUCTURED-TEXT` covers pinned native Text, Format, Tree search, exact pointer and Minify. Normal clipboard initialization/copy, host and GPUI remain pending.
