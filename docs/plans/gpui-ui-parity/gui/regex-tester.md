# Regex Tester

G14 owns `regex-tester`. The plugin accepts exact bare `regex`, emits `regex:open` and opens `DialogCommand::RegexTester`. `RegexSession` and the existing engine/policy/persistence own evaluation. The retained dialog owns draft, selection, information section and presentation. Preset removal is a scoped inline child.

| Criterion | Source-backed behavior |
| --- | --- |
| REGEX-01 | Preserve exact pattern/flags/sample, Rust regex diagnostics and debounced current evaluation. Matches and selected capture distinguish empty and unmatched values, use exact UTF-8 spans and allow navigation without editing source. Keep bounded previews and full buffer identity. |
| REGEX-02 | Explanation, syntax/constructors, reference and examples share supported engine semantics. A constructor uses the current selection and creates one undo edit. Example and history loading preserve authored cases and applicable buffers. |
| REGEX-03 | History records accepted valid evaluation. Only explicit preset Load changes source. Save/Update retain cases and supported optional fields. Preset deletion needs exact object and current confirmation generation. Preserve invalid storage bytes and report unavailable writes. |
| REGEX-04 | Substitution and Test Cases retain their own invalidation and current identities. Clipboard import/export, match/capture and selected text effects require explicit accepted actions and exact source/selection authority. Opening, rendering or inspection makes no clipboard request. |
| REGEX-05 | Preserve remembered Dock/Pop Out, retained draft on reopen, worker result fencing, hidden-root polling and exact retained owner. Reject deferred actions after close, discarded pass or host retirement. |

Select `regex_tester_open_is_exact_and_arguments_remain_metadata`, `regex_tester_panel_lifecycle_preserves_draft_and_deduplicates_stack`, `regex_tester_open_render_and_inspection_never_access_clipboard_implicitly`, `regex_tester_explicit_match_capture_copy_handles_unicode_empty_unmatched_and_failure`, `regex_tester_host_moves_retain_revision_and_copy_only_after_accepted_child_input`, `jump_mode_regex_native_viewport_edits_cases_navigates_and_cancels_preset_removal`, `regex_visualization_constructor_is_one_undoable_edit_and_respects_discarded_passes` and `substitution_replacement_edits_coalesce_without_rerunning_accepted_matching`. `G14-EGUI-STRUCTURED-TEXT` verifies Unicode evaluation, exact match navigation, persisted synthetic history and unsupported look-around feedback.

## GPUI retained matcher

Stock exact `regex` Search opens the retained tester. Pattern, the five flags and multiline Test Text feed the shared `RegexSession`. One checked due-time callback runs the existing bounded evaluation and reflects pending, invalid, suspended and truncated states. Match rows are virtualized, and details show exact UTF-8 spans and distinguish empty and unmatched captures. Previous, Next and scoped F3 preserve authored input. Close retires pending view authority and retains the draft. Restart review accounts for the exact authored draft, including a closed retained workspace.

Regex defaults to the stock Pop Out and honors saved Dock and native geometry. A checked host move retains the same session, draft, pending evaluation and selected match. Window-local editor and interaction authority retires with the old mount. Dock acknowledges the exact launcher focus delivery and foreground before retiring its source. Escape and Ctrl+W close only the owned standalone workspace outside nested overlays. Grouped tests passed for typed Search, session state and host receipts. Native tests at the matched 1516 × 937 launcher baseline must still exercise actual editor remount, Windows foreground, geometry and close.

Automatic history records only the exact current accepted pattern and flags. A single owned lane uses the shared store, preserves accepted order, survives host moves and settles before Quit or restart. Its warning preserves the store diagnostic and corrupt bytes. Scoped policy, privacy, persistence and restart tests passed. Native recording and reload evidence remain pending.

## GPUI Substitution and Test Cases

Substitution evaluates edited Replacement independently of the current match selection. The UI shows distinct disabled, pending, invalid, paused and successful output states. The UTF-8 preview is bounded at 16 KiB and the session retains the complete output. Oversized Replacement remains authored and Clear remains reachable.

Test Cases edits the same retained collection through stable case IDs. Add, Duplicate, Remove, Move Up and Down, expectation, optional labels, multiline inputs and Run or Rerun use the shared case policy and separate revisions. Stable editor and action identities preserve keyboard focus through changes. Case evaluation leaves primary results and accepted history intact. Rehost rebuilds the local editors from the retained draft. The grouped launcher tests passed. Matched native replay remains pending.

## GPUI explicit clipboard actions

Use Clipboard Text reads through the shared protected backend and replaces only Test Text. The entire draft survives an oversized import. Explicit Copy uses the full selected and accepted payload. Capture choice is scoped to the exact current match, and Test Cases Copy summary includes counts only. One checked lane validates current host and action authority after Windows clipboard acquisition and before a write. It coordinates Close, rehost and restart.

The matched Windows replay still needs actual protected refusal, clipboard contents, focus and host behavior. Ordinary editor keyboard and context-menu clipboard behavior remains a separate scope. Continue History controls and Presets, explanation, syntax, reference, examples and scoped Jump.
