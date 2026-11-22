# Calculation Pad Examples & Reference

G15 owns `calculation-pad-reference`, runtime navigation `calculation-reference`. [Calculation Pad](calculation-pad.md) Config Menu → Tools → Examples & reference opens the separately hosted browser. The shared reference catalog, examples and insertion preparation own the source and result semantics.

| Criterion | Source-backed behavior |
| --- | --- |
| CALC-REF-01 | Examples filters and groups the shared examples, showing source, expected output and explanation. API Reference offers type/category search, keyboard selection, history/back, detailed signatures, params, limits, units and related links. Copy uses the exact shown example. |
| CALC-REF-02 | Insert Example preserves current document/host/evaluation and exact UTF-8 caret, range and fold policy. Freeze prepared insertion and ask explicit review for collisions or shifted row references. Stale source/caret/host retires approval. Accepted insertion is one source undo step and obeys manual calculation. |
| CALC-REF-03 | Preserve independent preferences, scrolling and active lookup. The default ordinary root context is docked. Keep current reference epoch and Pad incarnation receipts. Retire on actual source document replacement/close, with same-incarnation Save As preserving the browser. |

Select `reference_api_keyboard_search_and_related_navigation_preserve_the_pad`, `reference_api_insert_respects_retention_and_is_one_manual_edit`, `reference_api_review_does_not_apply_after_source_or_caret_changed`, `reference_host_preserves_pad_and_rejects_retired_effects`, `calculation_reference_is_ordinary_root_context_when_pad_is_native` and `reference_native_callback_runs_with_hidden_root_and_os_close_keeps_pad`. `G15-EGUI-PAD-DEPENDENCY` proves real native Tools routing only. The controller truncated child controls. Browse, insertion, host and GPUI remain pending.
