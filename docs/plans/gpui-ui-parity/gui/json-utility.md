# JSON Utility

G14 owns `json-utility`. The `json_utility` plugin sends `json_utility:open` for bare `json`, `json_utility:format` for `json format` or `json pretty`, and `json_utility:minify` for `json minify`. Typed Open initializes from valid protected clipboard JSON. Pinned reopen reuses the existing utility without initialization. The shared `JsonEditorState` owns the editor, projection worker and tree and search revisions. The egui `JsonUtilityDialogState` supplies the retained host, controls and repaint notification.

| Criterion | Source-backed behavior |
| --- | --- |
| JSON-UTIL-01 | Text edits the exact source. Format and Minify preserve object ordering, update the text only and show precise invalid input diagnostics without rewriting it. Clear resets, explicit Paste supports repair and Copy Result validates the exact current text. |
| JSON-UTIL-02 | Tree shows virtualized structured rows, typed key/index paths, branch expand/collapse, stable selection and bounded details. Copy Value, Copy Subtree and Copy JSON Pointer preserve their distinct typed data and escaping. |
| JSON-UTIL-03 | Search decoded keys, values or both literally and case-sensitively, clear search and wrap navigation. Reveal the exact current match. Preserve surviving typed expansion/selection and reject stale, pending or invalid snapshots for copy/details. |
| JSON-UTIL-04 | Preserve retained host, geometry, exact source/search revisions and coalesced worker. Typed fresh open resets the draft using its protected-source policy. Rehost/pinned reopen preserves current semantics and true close retires stale effects. |

Select `json_query_family_maps_to_general_format_alias_and_minify_intents`, `json_utility_dispatch_opens_the_requested_mode_without_other_side_effects`, `each_open_loads_only_valid_strict_clipboard_json_and_keeps_intent`, `parse_failure_preserves_exact_input_and_location_and_never_copies`, `worker_discards_stale_results_and_coalesces_pending_source`, `tree_copy_actions_are_explicit_and_keep_string_value_vs_subtree_semantics`, `tree_search_caches_matches_wraps_and_reveals_collapsed_results` and `pending_invalid_and_stale_snapshots_cannot_enable_tree_copy`. `G14-EGUI-STRUCTURED-TEXT` covers pinned native Text, Format, Tree search, exact pointer and Minify. Matched Windows evidence remains pending for clipboard initialization and effects, host transitions, and the GPUI workflows.

## GPUI Text workspace

The docked Kit editor opens from the stock typed Search provider and remains below anchored Search. Format, Minify, Paste, Clear and Copy Result use the shared editor and protected clipboard backend. Explicit typed reopen resets source and intent. The view serializes clipboard work, coalesces rapid reopens to the newest request and checks editor generation and native visibility before effects and publication. Clear focuses the editor. True Close drops the source and projection worker, and restart review includes any active text draft.

Verify typing and Unicode, exact Format and Minify, valid versus invalid or protected initial clipboard, explicit Paste and Copy, rapid reopen, hide/minimize, Clear focus, Escape, and close/restart in the paired Windows candidate at 1516 × 937. Add the Tree, saved mode and pin criteria in the current G14 batch.

## GPUI Tree and retained host

The docked Tree uses bounded virtual rows with source-generation and node identity, typed branch and selection actions, literal Keys, Values or Both search, match navigation and reveal, and bounded Details. Copy Value, Copy Subtree and Copy JSON Pointer bind the exact current selection, with a separate Tree revision so Text clipboard actions keep their source authority. The wide paired fixture exercises a long result range and an escaped pointer.

The JSON editor session and clipboard lane live in one retained owner. Window-bound mounts own their local editors, focus and subscriptions. GPUI has native Pop Out, a checked Dock foreground receipt, scoped presentation persistence and event-driven geometry capture and restore. Saved pins mount passively without clipboard initialization. Pinned workspace close is refused. A checked top toolbar unpin preserves unrelated Settings and closes the exact current workspace after acknowledgement. Compare both frontends at 1516 × 937 with the paired pin fixtures.
