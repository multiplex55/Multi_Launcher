# G08 — Diff workspaces, text/binary/folder views, and child dialogs

**Status:** Pending. **Owner:** G08 implementer, coordinating file mutations with their existing domain owners. No execution evidence exists yet. Continue on the single migration branch.

## Objective and dependencies

Recreate the whole Diff interface in GPUI Kit: its workspace/view organization, supported input sources, text/binary/folder comparisons, settings/rules, previews, export, and any operations exposed by the current UI. Preserve source identity and decisions when comparison workers or filesystem changes occur. A side-by-side text viewer is only one part of this goal.

Depend on G00–G04, G05 routing, and the G07 File Search handoff where applicable. Shared comparison engines remain authoritative. Refer to [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md). G00 must inventory Diff separately: it is spread across many child modules and is not adequately represented by the main `Panel` enum.

## Verified source entry points

Existing sources at the pinned baseline `f7bc610a86bcad1c14b69dce65b1d804df1b7934`:

| Responsibility | Existing source |
|---|---|
| Workspace and input decisions | [src/gui/diff_dialog/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/diff_dialog/mod.rs), `host.rs`, `effects.rs`, `child_hosts.rs` in that directory |
| Views and children | `text_view.rs`, `binary_view.rs`, `folder_view.rs`, `folder_table.rs`, `rules_dialog.rs`, `folder_rules_dialog.rs`, `operation_preview.rs`, `export.rs` |
| Shared behavior | [src/diff/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/diff/model.rs), `text_compare.rs`, `binary_compare.rs`, `folder_compare.rs`, `folder_runtime.rs`, `file_ops.rs`, `watch.rs`, `persistence.rs` |
| Routes | `src/plugins/diff.rs`, `src/commands/handlers/file_search_diff.rs` |

The inspected workspace code records view IDs, effects, text/folder actions, recent path pairs, and text input drafts tied to source IDs/revisions. The proposed GPUI state must preserve these meanings without importing egui host objects into shared contracts.

## Per-GUI scope

Enumerate every workspace/view selector, input-source editor, text comparison control, binary mode, folder tree/table, filters/rules, statuses, and operation/export option. Include text input and replacement decisions, rules and folder-rules children, operation previews, recent-source menus, save/export pickers and confirmations, error states, and close decisions.

Preserve the supported relationship between parent workspaces and independently hosted children. Source watchers, comparison progress, selection, scrolling, and reload prompts must remain correctly associated with their view. Port custom painting through GPUI elements where needed; do not simplify a binary or folder view into a plain text dump. Exact controls and optional modes come from inventory rather than assumptions about common diff tools.

## Bounded checkpoint packets

### A — Separate workspace intent from framework hosting

**Owner:** G08 with the shared engine owner. Trace creation/open/replace/close actions, source snapshots, comparison requests, result application, watcher events, and child-effect publication. Extract only behavior currently trapped in egui handlers. Define neutral parent/view/source identities and compatible commands.

**Invariants:** An input decision applies to the reviewed source revision; late work cannot replace a newer comparison. Closing a view retires its effects and child ownership. Text input validation retains the user's original draft on failure.

**Evidence and acceptance:** A view/source/effect ownership map and focused existing session tests prove preserved transitions. **Commit boundary:** Neutral session/host contracts plus egui regression adaptations, not a replacement diff engine.

### B — Implement the three primary comparison surfaces

**Owner:** G08 frontend implementer. Recreate text, binary, and folder views with the same information hierarchy, controls, result selection, scrolling, and keyboard navigation. Preserve legends/status distinctions and source metadata. Use bounded rendering and caches where the baseline relies on them.

**Invariants:** Display truncation or virtualization does not alter source data; folding/filtering/sorting preserves target identity and navigation. Workers remain off the UI thread. No successful rendering path invents a completed comparison while work is pending.

**Evidence and acceptance:** Matched fixtures for identical/changed/error inputs, long text and binary content, and nested folders verify displayed semantics and actions. **Commit boundary:** Complete principal views, with documented pending child interfaces.

### C — Port rules, input, operation, and export children

**Owner:** G08, using existing filesystem/export services. Implement every child host and picker; preserve draft validation, cancellation, overwrite/reload/close decisions, and any preview-before-operation workflow. Recreate native/docked forms and parent-child focus behavior.

**Invariants:** A confirmation is bound to the actual reviewed plan/source, not whatever row is selected later. Child close cannot accidentally close the wrong workspace. Native picker completion is rejected after owner retirement. A user action publishes one operation/export.

**Evidence and acceptance:** Disposable-tree workflows show accepted, canceled, failed, and stale decisions; output comparison verifies bytes or intended normalized content. **Commit boundary:** Child GUI/effect integration with targeted fixes and acceptance records.

### D — Verify session, watch, and window integration

**Owner:** G08 acceptance owner. Exercise simultaneous supported views, external file changes, close/reopen while workers are active, tool rehosting, and hidden-root operation. Validate File Search and launcher opening routes with their real payloads once available.

**Invariants:** The same shared service state drives both frontends; no watchers or operations are duplicated by view rendering. Retained state belongs to the correct workspace. **Evidence and acceptance:** Native Windows interaction records, watcher/session test results, and round-trip persistence/export comparisons. **Commit boundary:** Final parity corrections and evidence. Keep unfinished cross-goal routes open in G25.

## Focused validation

Verified integration sources include `tests/domain_cases/diff_text_engine.rs`, `diff_folder_engine.rs`, `diff_sessions.rs`, `diff_watch.rs`, and `diff_file_ops.rs`, aggregated under `--test domain`; plugin cases include `diff_plugin.rs` under `plugin_queries`. Discover/list the narrow subset before running, for example `cargo nextest run -p multi_launcher --test domain -E 'test(diff_sessions) | test(diff_watch)'` when host/session changes justify it. Match actual names and require nonzero selection. Tests of comparison algorithms do not replace GPUI scroll, modal, focus, or child-window evidence. Run only affected file-operation cases on disposable trees.

## Exit and continuation

Each comparison mode and child surface has its own accepted row; no text-only milestone closes the full goal. Required operations, source decisions, exports, watchers, and window modes must have evidence. Record outstanding external dependencies honestly and continue independent file-tool work. Carry shared host changes back to previously accepted GUI rows and continue G09–G11 and G25 integration.
