# G02 GOAL — G09 Table Utility: Full-File Publication Safety Foundation

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P1 / high safety  
> **Lane:** implementation only after explicit proof gate  
> **Prerequisite:** None; G03 depends on this goal reaching ACCEPTED_COMPLETE  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Close the safety design/owner gap behind G09 M2-A: establish the smallest existing-owner, source-bound transaction/publication contract necessary before Table Utility can safely perform full-file transformation/export beyond bounded file-backed browsing.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- The current master includes bounded file-backed sources, projected page browsing, resource quotas and compact export receipts. This is **not** equivalent to full-file save/replace/transaction safety.
- The prior G09 M2-A design was selected but its implementer did not release a safe source candidate because of an agent/thread limit. Historical uncommitted or unsafe alternatives must not be cherry-picked.
- Table core already exists as `multi_launcher_table_core`; an additional crate-extraction or generic worker framework is not an evidence-backed prerequisite.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`src/table_utility_session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/table_utility_session.rs) — `TableFileBackedSource`, work/session generations, export receipts, resource leases
- [`src/gui/table_utility_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/table_utility_dialog.rs) — file-backed paging and compact export/overwrite workflows
- [`src/common/atomic_file.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/common/atomic_file.rs) — existing atomic-save helpers with limited guarantees
- [`crates/multi_launcher_table_core/src/lib.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/crates/multi_launcher_table_core/src/lib.rs) — existing extracted table-core package
- [Previous partial G09 integration](https://github.com/multiplex55/Multi_Launcher/pull/1455) — M1-A/M1-B accepted; M2-A/M2-B not accepted

## 3. Exact ownership, permitted changes and exclusions

- `src/table_utility_session.rs` for admitted source identity, snapshot generations, export lifecycle, resource accounting and publication receipts.
- `src/gui/table_utility_dialog.rs` for existing export controls/confirmation; do not widen visible behavior in foundation checkpoint.
- `crates/multi_launcher_table_core/src/` only where pure parser/export contracts require safe full-file boundaries; `src/common/atomic_file.rs` is reference, not permission to treat replacement as fully journaled transactions.
- Owner-specific focused unit/integration tests, synthetic temp files/controlled fail injection; no user documents.

**Non-goals / changes to refuse:**

- Do not implement complete full-file sort/filter/export UI in this foundation; that belongs to G03.
- Do not refactor the table crate, process-wide scheduler or other atomic-file users.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G02/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- No full-file destructive write may become reachable before source/output freshness and failure/recovery contracts are proven.
- Keep G09 M1-A/M1-B browsing and compact CSV/TSV/JSON workflows unchanged, including last-good data and row/column selection.
- Stale session, source/view/selection/export operation IDs never publish an old result. Resource leases/quotas are released only on actual ownership completion.
- Do not reintroduce a prior excluded unsafe M2-A candidate or add a second table engine/command path.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Reconfirm precise gap and finalize safety contract | Scoped work + tangible artifact; `docs(closeout): [G02/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Build smallest safe output reservation/publishing seam | Scoped work + tangible artifact; `docs(closeout): [G02/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Handle cancellation, partial writes and recovery | Scoped work + tangible artifact; `docs(closeout): [G02/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Focused deterministic proof | Scoped work + tangible artifact; `docs(closeout): [G02/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Safety gate outcome | Scoped work + tangible artifact; `docs(closeout): [G02/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Reconfirm precise gap and finalize safety contract

**Objective and detailed handoff.** Inspect actual file-backed page/source/export code and prior integration report. Compare read admission, last-good source, source file-identity/fingerprint, selected-row semantics, output occupancy, cancellation, failed writes and recovery. Save `SAFETY_CONTRACT.md`, source/call-site matrix, rejected alternatives, and M2-A exact acceptance before source changes.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Build smallest safe output reservation/publishing seam

**Objective and detailed handoff.** Use an owner-private output reservation/manifest capturing destination existence/identity and source revision. Fail closed on stale sources, occupied or swapped outputs, unsupported same-path in-place overwrite and external modification. Define staging in destination filesystem, bounded streaming, flush/close, publish, cleanup and exact failure retention. Preserve no-clobber/overwrite confirmation semantics; do not assume `rename`/`MoveFileExW` alone provides cross-file transaction/recovery.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Handle cancellation, partial writes and recovery

**Objective and detailed handoff.** Keep last valid document/page available during rejected work; account bytes during streaming, cancellation, session close and stale worker completion. Clean up task-owned temp objects on recoverable failure; retain/reconcile any unresolved publication receipt before new mutation. If robust recovery would require broad architectural changes, stop before publication and output a fully scoped design package, not an unsafe writer.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Focused deterministic proof

**Objective and detailed handoff.** Use synthetic bounded files to test source replaced during scan, revision changes, invalid destination, destination created between preview/write, write short/error, cancellation before/after staging, worker shutdown, quota release and last-good-state preservation. Single chosen test subset plus focused first-party lint as needed; native acceptance only for the exact Windows identity/publish boundary after it is actually supported.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Safety gate outcome

**Objective and detailed handoff.** Accept only after exact owner and source-of-truth invariants demonstrably pass. Otherwise classify `DEFERRED_WITH_DESIGN` and preserve an implementation-ready Goal/Kickstart identifying the one unresolved native/transaction requirement. Do not authorize G03 until G02 accepted.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Pure/owned temporary-file tests for read identity and stale source, invalid same-path output, destination conflict/no-clobber, partial write, cancellation and recovery.
- Controlled stream/writer-failure injection; source and output hashes prove no unintended overwrite.
- Existing file-backed browse/page and compact export tests selected for directly affected paths.
- If native Windows safety proof cannot be performed by available supported tooling, record `NATIVE_NOT_PROVEN` and **do not** claim foundation acceptance or make publication available.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- Safety design incomplete or required native proof unavailable → `DEFERRED_WITH_DESIGN`; leave existing feature fail closed and continue to G04 independent goals.
- Do not use a demonstration against files in user-controlled folders; only task-owned, sandboxed fixtures.
- G03 is hard dependency only on `ACCEPTED_COMPLETE` G02; a documentation-only M2-A result cannot satisfy it.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g02-table-publication-safety-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G02/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

A source-bound, recovery-safe publication foundation with focused test receipts is accepted; or a truthful deferred architecture package is committed with exact remaining proof and G03 is not entered.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `DEFERRED_WITH_DESIGN / NATIVE_NOT_PROVEN` / 20261029-r01.
- **Run ID and branch:** 20261029-r01; codex/closeout-g02-table-publication-safety-20261029-r01.
- **Actual master/source base:** master 4a1283e6edd2c245f562a07dbe3dcc6a61061eed; bootstrap 02d0ac64d51655f43d6e958ea551f5bf658ee48d.
- **Current milestone:** M3-A documentation decision complete; source UNACCEPTED.
- **Active role and requested/configured/resolved model:** NONE; planner/reviewer Astra max and implementer Astra xhigh requested; configured Sol high/Luna max; backend UNVERIFIED.
- **Last known commit / push:** donor f879a2a880d59e941486241e652299dabc002365, normal push succeeded; closure docs next.
- **Next action:** import only reviewed safe documentation; G03 dependency deferral then independent G04. Future native-parent-admission-proof pair requires new dispatch.
- **Blocker and scope:** native parent admission CreateFileW OS error5 before staging; denied ancestor/cause unknown; source donor-only; G03 blocked.

| Milestone | State | Real artifact, source commit, verification and owner | Next exact action |
|---|---|---|---|
| `M0-A` | ACCEPTED_COMPLETE | safety contract24f3c13f | Documentation only |
| `M1-A` | FAILED_VERIFICATION | two runs15/19; private source unaccepted | Exact native admission follow-on |
| `M1-B` | DEFERRED_DEPENDENCY | no caller added | Requires proved M1-A |
| `M2-A` | NATIVE_PROOF_DEFERRED | deterministic subset passes; native gate fails | Future exact proof package |
| `M3-A` | ACCEPTED_COMPLETE | independent documentation review2026-10-30 01:05:51UTC | Decision only; no source import |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime event, 2026-10-29 / 20261029-r01: G00 and G01 accepted before G02. Integration receipt b96d67f3 records accepted source through fd300ff7. G02 independent branch begins at bootstrap; source safety contract and exact native proof gate remain pending. G03 is not authorized by documentation-only progress.

- Dated runtime scope clarification, 2026-10-29: necessary additive resource-owner and existing file_operations native adapters are permitted solely to connect quota/cleanup ownership to guarded staged publication. This does not authorize a table-engine refactor, a new generic framework, duplicated native effects, or UI enablement. The safety contract must name exact paths and native proof before implementation. A proven no-clobber-only foundation may support a minimal G03 export; unsupported replacement remains refused.

- Runtime stop, 2026-10-29: both selected batches15 passed/4 failed; first responsible native parent admission CreateFileW returns OS error5 before stage creation. Denied ancestor and root cause unproven. Incremental preparation320.148s/600s, affected correction rerun consumed. Source remains UNACCEPTED on goal branch; complete implementer future pair recorded. M1-B not entered; G03 blocked. Parent checkpoints then requests documentation-only disposition review.

- Closure2026-10-30: reviewer accepted documentation/deferral only with no substantive defects; donor f879a2a8 source remains UNACCEPTED. Stop report and canonical complete pair preserved. G03 gate remains closed; proceed independent program goals.
