# Radial Cell Properties

G20 owns `radial-cell-properties`. A current authored cell context or eligible spacer opens the scoped [Designer](radial-designer.md) Properties draft. It can search launcher actions, stage stable reference, save typed live query or use an exact parsed command. Inspector shares the action binding UI with a separate scope.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-CELL-01 | Keep current cell/content, eligibility and available action/query modes. Search result and explicit Test carry exact request/editor/surface/target generations and eligible binding. Test and assignment are separate. |
| RADIAL-CELL-02 | Pin/Save/Set in Properties updates only the local draft. Apply validates action/content/submenu graph and current owner before atomic draft edit. Cancel and stale effects preserve current document. |

Select `properties_pin_control_stages_exact_binding_then_apply_continues_navigation`, `properties_draft_is_stable_across_frames_and_rejects_stale_apply`, `repeated_searches_have_distinct_request_identities`, `discard_reopen_rejects_already_queued_search_and_test_completions` and `d08_live_window_pin_and_test_are_disabled_and_save_query_is_typed_on_both_surfaces`. Native and GPUI remain pending.
