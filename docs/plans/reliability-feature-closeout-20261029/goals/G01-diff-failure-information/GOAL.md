# G01 GOAL — Folder Diff: Preserve Destination Read Errors and Operation Feedback

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P0 / first production fix  
> **Lane:** focused Rust implementation  
> **Prerequisite:** None; primary source-supported bugfix  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Fix the two independently source-supported information losses in Folder Diff: distinguish destination `NotFound` from all other read errors during mutation admission, and preserve/display delivered current-view failure/cancellation/partial results across normal rescans.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- `fresh` currently uses `let now = FileIdentity::read(&dst).ok();`; this folds `PermissionDenied`, `InvalidInput` and other errors into the same `None` as actual absence. This is a verified source mechanism, not a reproduced destructive outcome.
- `poll_folder_runtime` consumes `OperationEvent::Completed(report)` if `report.generation` matches, then clears the operation and passes only `report.affected_subtree` to `refresh_after_mutation`; `report.items` and `report.cancelled` are not surfaced in this path.
- The separate AUD-F002 research selected **H0 best-effort path/metadata mutation authority**. This goal does not reopen H0 or claim native object identity/atomic transaction guarantees.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/diff/file_ops.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/diff/file_ops.rs) — `fresh`, `FileIdentity::read`, `execute_copy`, `ExecutionReport`, `ItemOutcome`
- [`src/gui/diff_dialog/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/diff_dialog/mod.rs) — `poll_folder_runtime`, `refresh_after_mutation`
- [`src/diff/folder_runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/diff/folder_runtime.rs) — `FolderRuntime` / active operation
- [`tests/domain_cases/diff_file_ops.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/tests/domain_cases/diff_file_ops.rs) — existing bounded copy/recycle coverage
- [`docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/GOAL.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/GOAL.md) — already complete source-grounded original proposal
- [`docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/KICKSTART.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/KICKSTART.md) — original execution instructions

## 3. Exact ownership, permitted changes and exclusions

- Domain: `src/diff/file_ops.rs` in the existing mutation/freshness owner; use a narrow private injected-read seam only if necessary for deterministic tests.
- Presentation: `src/gui/diff_dialog/mod.rs` and a minimal existing controller/runtime state owner, including actual completion call sites; model `src/diff/folder_runtime.rs` only if established ownership requires it.
- Tests: existing `tests/domain_cases/diff_file_ops.rs`, directly owned inline Diff tests, and a small model fixture for delivered reports.

**Non-goals / changes to refuse:**

- No new native object-bound mutation contract, atomic batch, undo journal, different Recycle behavior or universal filesystem framework.
- No full-system input automation or real-file destructive test.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G01/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- Preserve approved preview direction, captured exact paths, containment, nested selection, dirty destination protection, recycle-only GUI, no Permanent fallback, and between-item cancellation.
- Report lifecycle must be bound to current view plus ordered source/destination roots, not just a generation or view ID.
- A delivered partial operation is not an atomic rollback; copied or recycled successful items stay reported as completed.
- Do not change the existing native file identity, trash backend, journals, stored schema, confirmation or command bus.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Revalidate exact owner and historical handoff | Scoped work + tangible artifact; `docs(closeout): [G01/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Preserve destination observation errors | Scoped work + tangible artifact; `docs(closeout): [G01/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Preserve current-operation feedback | Scoped work + tangible artifact; `docs(closeout): [G01/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Focus tests and review | Scoped work + tangible artifact; `docs(closeout): [G01/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Publish accepted correction | Scoped work + tangible artifact; `docs(closeout): [G01/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Revalidate exact owner and historical handoff

**Objective and detailed handoff.** Read the original merged Goal and Kickstart, check the two cited mechanisms still exist on the actual task base, inspect plan/executor and GUI caller tests. If one is already corrected, mark a source-backed no-op for that boundary; do not double-fix it.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Preserve destination observation errors

**Objective and detailed handoff.** In `fresh`, map successful identity to `Some`, **only** `io::ErrorKind::NotFound` to `None`, and all other errors to explicit per-item validation failure with error context before `fs::copy`. Maintain expected `None`/`Some` freshness, dirty path, root/canonical containment, preview direction and overwrite semantics.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Preserve current-operation feedback

**Objective and detailed handoff.** At actual completed-report consumption, validate generation AND owning view ID AND ordered left/right root identity captured at dispatch **before** result adoption and rescan. Store bounded, transient current-view feedback; retain it across normal same-view rescans and preview open/cancel, but clear or replace on new dispatch, view discard, side/root swap, or changed retained parent. Present exact failed paths/reasons, cancellation and preceding successes without double counting or rollback claims.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Focus tests and review

**Objective and detailed handoff.** Add injected `NotFound`, `PermissionDenied` and another non-NotFound error; normal absent/existing overwrite controls; delivered `Completed` success+Failed, cancelled, stale generation, same-ID root swap/parent change, rescan retention and new-run replacement. Run a nonzero targeted Nextest selection only, examine all callers and one independent review. Fix one bounded substantive finding if needed.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Publish accepted correction

**Objective and detailed handoff.** Inspect cumulative diff and documentation, commit/push accepted source and exact test receipts; if an unresolved correctness failure remains, mark `INCOMPLETE` and do not import implementation. No native race experiments.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Injected filesystem observation: `NotFound` permits expected absence, `PermissionDenied`/`InvalidInput` are failures and do not invoke the effect.
- Existing-source expected destination + read failure or changed identity refuses; valid ordinary copy/overwrite still admits.
- Inject `ExecutionReport` with successes followed by failure and with cancellation; verify bounded GUI details and no invented results.
- Simulate swapped ordered roots or reused view ID before reconciliation; stale results must not publish, while a same-root rescan preserves accepted feedback.
- One targeted Nextest batch (nonzero inventory) plus narrow lint/format only if the code change warrants; no broad suite.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If new evidence shows receipt delivery itself (not GUI consumption) is the root issue, stop expansion and create a distinct future Goal/Kickstart for that owner.
- If current code provides no deterministic report injection seam, add a small private seam; do not require real trash/filesystem ACL games.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g01-diff-failure-information-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G01/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Both information losses are fixed at their existing owners, selective tests pass, independent review accepts the exact scope, and one normal push is attempted after each coherent source checkpoint.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `NOT_STARTED`.
- **Run ID and branch:** assigned by the orchestrator after safe source verification.
- **Actual master/source base:** `UNVERIFIED_FOR_EXECUTION`.
- **Current milestone:** `M0-A` pending.
- **Active role and requested/configured/resolved model:** `NONE / unverified`.
- **Last known commit / push:** none for this package.
- **Next action:** read [KICKSTART](KICKSTART.md), confirm prerequisite and revalidate source; then begin on a fresh task branch.
- **Blocker and scope:** none reported.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | PENDING | — | Revalidate exact owner and historical handoff |
| `M1-A` | PENDING | — | Preserve destination observation errors |
| `M1-B` | PENDING | — | Preserve current-operation feedback |
| `M2-A` | PENDING | — | Focus tests and review |
| `M3-A` | PENDING | — | Publish accepted correction |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.
