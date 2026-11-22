# G09 — Batch Rename and Search Replace

**Status:** Pending. **Owner:** G09 implementer with the existing rename/replacement transaction owners. Drafting has executed no tests or file operations. Use the single program branch.

## Objective and dependencies

Recreate both complete file-editing workflows in GPUI Kit, preserving the separation between editable drafts, reviewed previews, accepted plans, execution, and recovery. The current application contains deliberate identity and freshness checks; visual restyling must retain those protections and all supported controls.

Depend on G00–G04 and G05 command routing. Coordinate the operation-history/recovery handoff with G11 and G06, without creating a second transaction engine. Read [REQUIREMENTS.md](../REQUIREMENTS.md), [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md), and the [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md).

## Verified source entry points

Pinned existing sources at `f7bc610a86bcad1c14b69dce65b1d804df1b7934`:

| Tool | Existing source |
|---|---|
| Batch Rename UI | [src/gui/batch_rename_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/batch_rename_dialog.rs) |
| Rename engine/transaction | `src/batch_rename.rs`, `src/batch_rename_transaction.rs`, `src/file_operations.rs`, `src/commands/handlers/batch_rename.rs` |
| Search Replace UI | [src/gui/search_replace_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/search_replace_dialog.rs) |
| Replacement behavior | `src/search_replace.rs`, `src/replace_executor.rs`, `src/commands/handlers/search_replace.rs`, related plugins |

Full UI source inspection confirms Batch Rename uses scan generations, immutable verified snapshots/previews/plans, rule revisions, selection by snapshot reference, and separate apply/undo confirmation identities. Search Replace tracks search generation, preview revision, selected matches, multiple preview panes, apply/recovery modals, and session-scoped host actions.

## Per-GUI scope

**Batch Rename:** Root selection, recursive scan, folder tree, every rule type/editor, rule ordering and extension policy, include/select state, filtering/sorting, source/proposed names, row statuses, preview, apply confirmation, execution feedback, undo/recovery, and all supported host modes. Visual sorting must not redefine scan or transaction identity.

**Search Replace:** Search/replacement editors, root/files/settings/advanced sections, match selection and approvals, file/result filtering and sorting, original/proposed/unified inspection panes, folding and independent scrolling, apply confirmation, progress/cancel, failure and recovery decisions. Keep multiline and long-line content exact. Include all feature-specific plugin settings registered through G06.

## Bounded checkpoint packets

### A — Establish fresh-plan and exact-selection contracts

**Owner:** G09 with the transaction owner. Trace all user intents into shared scan/preview/apply/undo operations. Document which edits invalidate a preview or confirmation, and the identity carried from a displayed row to a verified file/match. Extract behavioral validation from egui handlers without weakening existing proofs.

**Invariants:** Draft edits revoke stale apply authority. A confirmation refers to one frozen plan. Filtering, folding, or sorting cannot broaden the approved set. Preview never mutates source files.

**Evidence and acceptance:** Existing guarded-plan and fresh-match tests run against the retained egui path; contract tables describe invalidation boundaries. **Commit boundary:** Shared contracts and minimal egui rewiring with focused regression evidence.

### B — Port Batch Rename end to end

**Owner:** G09 frontend implementer. Recreate all scan/rule/preview controls and result columns with stable identities and bounded rendering. Implement apply/undo confirmations and status/error presentation using the original transaction services. Include keyboard/Jump Mode behavior and reachable controls in narrow windows.

**Invariants:** Scan completion cannot populate a closed or repurposed session. Changing root or rules invalidates the relevant authority. One acceptance executes one plan; stale confirmations cannot retarget a newer preview.

**Evidence and acceptance:** Disposable directories verify exact proposed names, selected inclusion, successful apply, cancellation, conflicting paths, and guarded undo/recovery. **Commit boundary:** Complete Batch Rename GUI and its focused evidence, leaving Search Replace separately pending.

### C — Port Search Replace end to end

**Owner:** G09 frontend implementer. Recreate editors, settings, file/match approval controls, all preview panes and folds, apply/recovery decisions, and job status. Use shared search/replacement services; preserve cancellation and atomic/recovery behavior as actually implemented.

**Invariants:** The inspected candidate and approved matches remain exact across sorting/virtualization. Independent pane scroll positions do not corrupt selection. Source changes between preview and apply are handled by the existing freshness policy. UI rendering never submits duplicate writes.

**Evidence and acceptance:** Disposable fixtures cover selected multiline matches, unselected matches, invalid expressions, external edits, canceled confirmation, failed execution, and recovery. Compare resulting bytes with the baseline expectation. **Commit boundary:** Complete Search Replace GUI and focused evidence, not a new replacement algorithm.

### D — Verify host transitions and cross-tool recovery

**Owner:** G09 acceptance owner with G06/G11 handoff owners. Test popout/dock while drafts and confirmations exist, root-hidden completion, close/reopen during scan/work, keyboard-only approval, and recovery routes. Revalidate common host behavior if this work extends its contracts.

**Invariants:** Rehosting preserves the reviewed plan and draft when the reference allows it, while retirement prevents stale effects. Operation history identifies the actual run. **Evidence and acceptance:** Native recordings, command/operation traces, before/after fixture trees, and row-level results. **Commit boundary:** Remaining interaction fixes and acceptance records; cross-tool rows stay pending until their real route passes.

## Focused validation

Verified inline cases include `m8d_confirmed_plan_is_frozen_and_draft_edits_revoke_apply_authority`, `m8e_launcher_route_previews_applies_and_undoes_with_guarded_recovery`, `stale_host_actions_do_not_dispatch`, `m9d_selected_match_freezes_bounded_candidate_without_writing_source`, and `m9e_launcher_replace_applies_only_fresh_selected_matches_and_routes_recovery_restore`. Other discovered cases cover reachable confirmation footers, exact approvals under folding/virtual scrolling, and retained checkpoints. Select the affected cases under `-p multi_launcher --lib` after listing nonzero matches; use their exact names or narrow substrings. Add only meaningful GPUI-specific interaction tests. Every filesystem mutation test uses isolated temporary fixtures; existing domain checks alone do not prove the GPUI confirmation path.

## Exit and continuation

Both tools, every review/confirmation/recovery child, and all assigned window modes require separate evidence. A matching preview is insufficient without guarded execution and cancellation behavior. Keep all source/data invariants intact and preserve egui fallback. Continue G10–G11, revisit linked recovery integration when available, and retain global parity status as pending until the full program finishes.
