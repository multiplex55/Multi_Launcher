# Snippet Prompt

G13 owns `snippet-prompt`. A parameterized `snippet:run:<alias>` can open standalone Fill Snippet. [Snippets](snippets.md) Test / Preview uses the companion Preview Snippet. [Paste Sequences](paste-sequences.md) preflight can host Prepare Sequence for the current step. `SnippetPromptDialog` owns the scoped form state and existing execution/persistence owners retain side effects.

| Criterion | Source-backed behavior |
| --- | --- |
| SNIPPET-PROMPT-01 | Reconcile authored fields in order. Populate defaults, focus the first required or first available, retain single-line/multiline behavior, required/error feedback and live read-only result. Keep Tab order, scoped Ctrl+Enter and Escape/Cancel with the current prompt. |
| SNIPPET-PROMPT-02 | Preview returns to the exact unsaved editor draft with no copy or history effect. In standalone execution, Copy requires a current unique persisted entry and valid field inputs, writes once, then records the captured alias-only history and outcome. Stale entry or write failures retain the prompt. |
| SNIPPET-PROMPT-03 | Sequence Continue/Prepare advances the exact frozen preflight, reuses approved values and revalidates current source documents. The playback owner performs actual paste. Capture protected clipboard only for `sys.clipboard`, and foreground metadata only for `sys.window_title`. |
| SNIPPET-PROMPT-04 | Keep root/companion host, initiating editor epoch, current prompt generation and actual retained passes. Restore staged focus/draft on a rejected owner pass. Cancel scrubs prompt values. |

Select `prompted_run_queues_safe_snapshot_without_copy_or_success_history`, `required_and_missing_values_fail_without_copy_and_keep_the_session`, `changed_deleted_or_ambiguous_current_entries_are_stale`, `preview_cannot_resolve_or_copy_and_cancel_returns_exact_draft`, `sequence_prompt_reuses_fields_across_steps_and_completes_without_copying`, `snippet_prompt_focus_tab_multiline_and_escape_are_owned_by_the_dialog`, `snippet_prompt_ctrl_enter_submits_once_and_preview_consumes_without_copy` and `prompt_completion_uses_captured_history_and_root_policy_once`. `G13-EGUI-SNIPPET-TEMPFILE-DRAFTS` proves real field-only Fill and Preview without OS clipboard access.

## GPUI standalone Fill and Run

Canonical Snippets Search admits exact unique actions for literal, automatic and prompted entries. Fill uses the shared field order, defaults and read-only result. Its exact focus scope handles Tab, Shift+Tab, Ctrl+Enter and Escape. Copy uses the existing acquired clipboard worker and alias-only completion. A refused write returns the same retained values and captured context. [Standalone Fill checkpoint](../CHECKPOINTS.md#g13-standalone-snippet-run-and-fill)

Editor Test / Preview uses the same field controls and live result with one Return to Editor action. It holds the exact unsaved parent draft and permits no copy, History or persistence effect. Protected context capture checks authority at the native reader. [Editor Preview checkpoint](../CHECKPOINTS.md#g13-snippets-field-authoring-and-editor-preview)

Actual desktop Copy and system value replay, paired editor Preview and the Paste Sequence companion remain open.
