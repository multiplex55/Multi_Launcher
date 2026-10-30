# G03 GOAL — G09 Table Utility: Conditional Full-File Operations and UX Closure

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P2 / hard-gated  
> **Lane:** conditional implementation  
> **Prerequisite:** G02 accepted safety foundation, otherwise DEFERRED_DEPENDENCY without source changes  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Only if G02 has proved its publisher and identity invariants, complete the user-facing full-file operation(s) explicitly supported by the current Table utility contract, with bounded memory, truthful selection scope, progress and cancellation.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- The current file-backed view supports bounded projected pages; a page selection is not automatically a whole-file transformation selection.
- The prior source accepted only M1-A/M1-B and did not accept G09 M2-B full-file behavior, >500 MiB cases, or native lifecycle contract.
- This goal is inherently dependent on safe publication and cannot be “finished” by adding a button to call compact export on a paged source.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/gui/table_utility_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/table_utility_dialog.rs) — `show_file_backed_results`, `show_footer`, export/confirmation controls
- [`src/table_utility_session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/table_utility_session.rs) — `TableExportKind`, `TableExportReceipt`, session worker
- [`crates/multi_launcher_table_core/src/lib.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/crates/multi_launcher_table_core/src/lib.rs) — separate existing table core
- [G09 partial integration](https://github.com/multiplex55/Multi_Launcher/pull/1455) — paging currently supported, complete full-file workflow missing

## 3. Exact ownership, permitted changes and exclusions

- `src/gui/table_utility_dialog.rs` file-backed controls, page-selection/full-source labels, overwrite confirmation and activity progress.
- `src/table_utility_session.rs` bounded worker, export basis, operation IDs and receipts.
- Pure `crates/multi_launcher_table_core/src/` APIs only if an explicit supported full-file algorithm is chosen by M0 requirements.

**Non-goals / changes to refuse:**

- No new spreadsheet feature, data science engine, broad table UX rewrite, or automatic G10/17 execution.
- Do not change older compact file limits merely to make a large-file test appear successful.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G03/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- No value from one page is misrepresented as full-file export, sorted selection, filter, or safe replacement.
- Never export/write from a stale source or stale selection revision; preserve last-good compact state.
- Memory use remains bounded by configured accepted source/pool quotas and output chunk sizes; no whole-file materialization to dodge design complexity.
- No activation of G10 or G17 based on partial G09 acceptance.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Hard readiness gate and user-facing scope | Scoped work + tangible artifact; `docs(closeout): [G03/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Bounded full-file worker | Scoped work + tangible artifact; `docs(closeout): [G03/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Unambiguous UI and progress | Scoped work + tangible artifact; `docs(closeout): [G03/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Focused proof and decision | Scoped work + tangible artifact; `docs(closeout): [G03/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Close or defer | Scoped work + tangible artifact; `docs(closeout): [G03/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Hard readiness gate and user-facing scope

**Objective and detailed handoff.** Prove G02 `ACCEPTED_COMPLETE` and own source integration. Enumerate supported full-file operations and inputs from current table core; decide a minimal user-visible first deliverable (e.g. full-file CSV export when valid) and explicit unsupported operations. Record whether sort/filter is view-only or truly whole-file and why.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Bounded full-file worker

**Objective and detailed handoff.** Implement only the chosen streaming source-bound operation, with clear source revision/selection semantics, stable column mapping, quotas and cancellation checkpoints; use accepted G02 publisher unchanged. Reject missing or changed sources and unsupported row/projection combinations.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Unambiguous UI and progress

**Objective and detailed handoff.** Show page vs whole-file scope, projected sample vs actual output, bytes/rows progress and cancellation. Preserve compact workflows and optional previous-valid behavior. Keep output confirmation and error details consistent with existing Table dialog owner.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Focused proof and decision

**Objective and detailed handoff.** Run small synthetic normal/partial/cancellation/stale-source/overwrite fixtures; where feasible exercise one bounded >500 MiB synthetic source without reading it entirely into RAM, under an explicit resource/time cap and owned cleanup. If unavailable, mark large-source acceptance incomplete, not passed.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Close or defer

**Objective and detailed handoff.** If full-file and resource/native invariants are met, label completed subset exactly. If only a safe subset is implemented, mark `PARTIAL_ACCEPTED` with unsupported features disabled and publish a narrow follow-on pair for omitted operations.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Nonzero selected tests for full-source versus page row counts, selected columns, mixed widths and headers.
- Synthetic source changed during preparation, cancellation mid-stream, output already occupied, source deleted, late/stale worker result and last-good restoration.
- Bounded large-file/lifecycle acceptance when supported; explicit `NOT_RUN` if unavailable, not generic elapsed-time claims.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If G02 is deferred, skip this goal entirely except publishing a precise dependency report.
- No unsafe write path, no unbounded in-memory parse, no invented “fully transactional” guarantee.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g03-table-full-file-operations-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G03/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

The exact chosen full-file capability works with accepted publisher, scoped tests and honest UI; or the dependency/partial outcome is documented without mislabeling G09 as fully accepted.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `DEFERRED_DEPENDENCY`.
- **Run ID and branch:** `20261029-r01` / `codex/closeout-g03-table-full-file-operations-20261029-r01`.
- **Actual master:** `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`. Authoritative run: [PROGRAM_STATUS](../../../execution/multi-launcher-reliability-closeout-20261029-20261029-r01/PROGRAM_STATUS.md).
- **Source base:** bootstrap `02d0ac64d51655f43d6e958ea551f5bf658ee48d` was a documentation-only exception because accepted G02 ancestry did not exist. It is not a valid future implementation base.
- **Last known closing donor/import:** `e0f556cd8041c502d63479d89d0e68301cceb3b2` → `d326679d536008167a294880e5bd9b16b502e732`, normal pushes and independent documentation acceptance recorded.
- **Current milestone:** dependency-only closure; implementation and tests unexecuted.
- **Active role/model:** none for G03; historical role attribution preserved. G10 only reconciles mutable status.
- **Next action:** await ALL G02 accepted safety/native/lifecycle/cancel/cleanup/recovery, reviewed integration, valid new G03 base and fresh full-contract/current-source M0. OS5 resolution alone is insufficient.
- **Blocker/scope:** G02 implementation remains unaccepted. No source, Cargo, native or large-file fixtures. Original complete Goal/Kickstart remains the conditional future pair.

| Milestone | State | Evidence / next action |
|---|---|---|
| M0-A | DEFERRED_DEPENDENCY | Dependency receipt independently accepted; feature scope selection unexecuted. |
| M1-A | DEFERRED_DEPENDENCY | Worker unexecuted; await accepted G02 and fresh M0. |
| M1-B | DEFERRED_DEPENDENCY | UI/progress unexecuted; same gate. |
| M2-A | DEFERRED_DEPENDENCY | Tests/native/large-source proof NOT_RUN. |
| M3-A | DEFERRED_DEPENDENCY | Safe documentation closure accepted; no feature acceptance or further automatic dispatch. |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime2026-10-30: G02 lacks ACCEPTED_COMPLETE/native/lifecycle proof. Integration78dd134b accepted safe docs only; failed donor sourcef879a2a8 excluded. G03 fresh bootstrap branch documentation only; no accepted dependency base exists. No source work or large fixture authorized.

- 2026-10-30 01:15:07 UTC: planner dependency packet executed as documentation only; [stall](../../../execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G03/stalls/M0-A-20261030T011507Z.md) recorded. Both G02 batches were 15 passed / 4 failed / 19 selected before staging at CreateFileW OS error 5; ancestor/cause unknown, M1-B unstarted. Last accepted integration source fd300ff7. No fallback because hard gate requires skip; no G03 implementation/tests/resources. Original complete Goal/Kickstart retained as conditional future contract; NO_ACTION for distinct follow-ons.

- Closure2026-10-30: reviewer01:20:19UTC accepts documentation only at36fd12a9, no substantive defects. G03 remains DEFERRED_DEPENDENCY; all implementation/native/large proof unexecuted. Parent imports safe docs, proceeds independent G04. Original complete Goal/Kickstart retained as future contract.

- 2026-10-30 G10 documentation reconciliation: current G03 mutable fields aligned with existing receipts and preserved contracts/events; no new feature/source/test acceptance. Authoritative current action remains parent G10 review/import/publication.
