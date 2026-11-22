# Dependency Explorer

G15 owns `calculation-pad-dependencies` with navigation ID `calculation-dependencies`. [Calculation Pad](calculation-pad.md) Config Menu → Tools → Dependencies opens this independently hosted read-only companion with a checked current Pad receipt. The companion owns the selected `(DocumentId, DocumentIncarnation, LineId)`. The calculation dependency snapshot owns relation and result identity.

| Criterion | Source-backed behavior |
| --- | --- |
| CALC-DEP-01 | Seed with the accepted current caret, choose a source line and expose current source, result, error or projected loop iteration. Show direct/transitive upstream and downstream connections and distinct pending, stale, failed and inactive states. |
| CALC-DEP-02 | Go to source and relationship rows navigate the current exact line only after accepted basis, owner, revision and open/modal validation. Set Pad selection/caret/scroll and return editor focus without source mutation. |
| CALC-DEP-03 | Keep retained selection through valid host changes and same-document pending evaluation. Reject stale navigation after close, source replacement or document incarnation change. An open companion reports Pad unavailable when the parent is closed. |

Select `dependency_explorer_opens_browses_and_navigates_current_pad`, `native_pad_dependency_explorer_inherits_and_reuses_session`, `dependency_actions_require_revision_and_pending_preserves_selection`, `dependency_navigation_applies_current_utf8_caret_and_scroll_only_after_retention` and `dependency_companion_rejects_deleted_and_reopened_source_identity`. `G15-EGUI-PAD-DEPENDENCY` verifies native current row, direct upstream delivery, exact source selection and inactive state after Pad close. Other tabs/host states and GPUI remain pending.
