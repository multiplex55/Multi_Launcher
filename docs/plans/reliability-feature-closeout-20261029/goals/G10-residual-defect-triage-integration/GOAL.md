# G10 GOAL — Residual Defects, Deferred Goals and Final Integration Closeout

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P0 at end / coordination  
> **Lane:** docs + accepted-checkpoint integration  
> **Prerequisite:** Follows individual goal dispositions, never requires all goals to pass  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Reconcile every executed Goal and source-backed finding, preserve skipped/deferred work, import only accepted verified changes, perform a narrow integration review, prepare a truthful PR and final master handoff without a new unbounded implementation campaign.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- The original 17-goal integration is **partial**, not completion: G04/G06/G09/G11/G12/G14/G15/G16 had partial checkpoints, Goals 05/07/08/13 were deferred and Goals 10/17 skipped for prerequisites.
- The Astra audit accepted only AUD-F001 (already fixed/merged) and the subsequent Diff study H0; it explicitly rejected broad replacement frameworks. Search admission research H0 says do not build metadata/ranker/UI now.
- Current master research artifacts can contain historical dates and proposal statuses; a new current ledger must distinguish actual new changes from those frozen reports.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`docs/research/multi-launcher-research-20261028-r01/01-static-audit/FINDINGS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/01-static-audit/FINDINGS.md) — proven versus unaccepted findings
- [`docs/research/multi-launcher-research-20261028-r01/01-static-audit/RECOMMENDATIONS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/01-static-audit/RECOMMENDATIONS.md) — audit dispositions
- [`docs/research/multi-launcher-research-20261028-r01/02-search-discovery/RECOMMENDATIONS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/02-search-discovery/RECOMMENDATIONS.md) — no shipping intelligent search recommendation
- [`docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md) — three complete/one dependency deferred historically
- [Original 17-goal matrix](https://github.com/multiplex55/Multi_Launcher/blob/da898f90b6d83fede9f11f94e32dbeaaef6465c6/docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/INTEGRATION_STATUS.md)

## 3. Exact ownership, permitted changes and exclusions

- Program-owned `docs/plans/execution/<PROGRAM_ID>/` ledger, per-goal `follow-on/` packages, final `INTEGRATION_MANIFEST.md`, `PR_SUMMARY.md`, `FINAL_HANDOFF.md`, source/commits map and response.
- Integration branch carrying only separately accepted goal commits, never unaccepted native candidates or failed source code.
- Read-only recheck source/CI status and narrow overlapping contracts; no speculative general refactor.

**Non-goals / changes to refuse:**

- No full-suite/native campaign or new source feature inside closeout.
- No merge to master, branch deletion, force-push, security-setting change or rewriting historical evidence without explicit user authorization.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G10/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- No unverified source change or unaccepted native/unsafe candidate reaches the combined integration branch.
- A completed Goal with partial user-visible behavior is labeled `PARTIAL_ACCEPTED`, not full feature completion.
- No automatic execution of historical Goals 04/05/07/08/13/10/17 from an old queue branch.
- Do not implement intelligent discovery, new plugin ABI, universal input/worker system, or standalone new workspace feature without a fresh user decision and evidence.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Reconcile end-to-end progress and dependencies | Scoped work + tangible artifact; `docs(closeout): [G10/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Evidence-based narrow defect disposition | Scoped work + tangible artifact; `docs(closeout): [G10/M1-A] ...` or `fix/feat` when source changed |
| `M1-B` | Integrate only accepted checkpoints | Scoped work + tangible artifact; `docs(closeout): [G10/M1-B] ...` or `fix/feat` when source changed |
| `M2-A` | Bounded combined review and readiness | Scoped work + tangible artifact; `docs(closeout): [G10/M2-A] ...` or `fix/feat` when source changed |
| `M2-B` | PR and final handoff | Scoped work + tangible artifact; `docs(closeout): [G10/M2-B] ...` or `fix/feat` when source changed |
| `M3-A` | Final source-of-truth reconciliation | Scoped work + tangible artifact; `docs(closeout): [G10/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Reconcile end-to-end progress and dependencies

**Objective and detailed handoff.** Collect exact goal branch heads and SHA, acceptance classifications, test/native evidence, diff owners and new candidate findings. For each incomplete original Goal 04/05/07/08/13/10/17, record reason, current relevance and whether explicitly user-deferred or upstream-blocked.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Evidence-based narrow defect disposition

**Objective and detailed handoff.** Review new demonstrated defects only: code path, adverse scenario, counterevidence, severity, owner, current source and proof scope. For a distinct actionable issue, author full `GOAL.md` and `KICKSTART.md` under per-model role folders and promote a canonical pair; do not implement it within this closing goal. Speculative/non-reproduced issues become bounded research spikes or NO_ACTION.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-B — Integrate only accepted checkpoints

**Objective and detailed handoff.** Inspect exact per-goal source patch and test evidence. Cherry-pick -x accepted coherent commits to program integration branch or reapply exact approved diff with source provenance, excluding unaccepted/dirty candidates; resolve minor conflicts at real owners and review. Defer hard dependency features when prerequisite not accepted.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Bounded combined review and readiness

**Objective and detailed handoff.** Check `git diff --check`, no unrelated source or configuration files, source/manifest provenance, target selected test compatibility only where goals share changed owners. Do not rerun each previously passed suite just for documentation import. One independent final review for combined behavior/ownership, not a synthetic “all apps tested” claim.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-B — PR and final handoff

**Objective and detailed handoff.** Prepare a detailed PR to current master with exact included feature slices, incomplete/default-disabled work, tests/lint/native statuses and blockers. Open PR only when connector/credentials and permission support it; **do not merge automatically**. If blocked, publish a resumable report and finish the queue without repeated loops.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Final source-of-truth reconciliation

**Objective and detailed handoff.** Record all accepted and deferred goal outcomes, PR link, head SHA, conflicts, remaining follow-on pairs, source and branch topology, and one final user manual-review checklist. Preserve branches until merged and verified; do not delete source/history.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- Exact changed-file manifest and Git blob/hash comparison for each accepted code donor and provenance commit; docs link/JSON validation.
- Nonzero focused cross-owner regression only where source owners overlap or merge conflict changes code; no full Nextest/Clippy acceptance marathon by default.
- Independent final review of accepted source scopes, native-unproven labels, privacy and safety requirements; genuine unsupported areas get follow-on plans.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If merge conflicts would require substantial unplanned semantic refactoring, stop that donor import, label `INTEGRATION_DEFERRED`, and continue independent accepted donors; no force/rebase/reset.
- If GitHub PR/approval/merge is unavailable, preserve branch and exact manual follow-up; never claim master changed.
- If the source repository drifts materially during the run, record old pinned source and current master; reconcile with one focused inspection, not silent mixed baselines.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g10-residual-defect-triage-integration-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G10/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

Final integration branch contains only accepted work; a truthful PR and dependency-indexed follow-on library exist; all deferred goals and stop reports have exact resume instructions and no invented success.

**Required closure record:** real branch/base, all milestone dispositions, exact changed files, accepted and rejected interfaces, focused test counts and command receipts, actual native proof or `NOT_RUN`, model/role provenance, reviewer disposition, commits/pushes, output/follow-on paths, source safety constraints, and exact next action. No PR merge/CI success may be claimed without host-confirmed evidence.

## 10. Execution progress — mutable (Codex updates here)

- **Current state:** `IN_PROGRESS`.
- **Run ID and branch:** `20261029-r01` / `codex/closeout-g10-residual-defect-triage-integration-20261029-r01`.
- **Actual master:** `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`. Authoritative run: [PROGRAM_STATUS](../../../execution/multi-launcher-reliability-closeout-20261029-20261029-r01/PROGRAM_STATUS.md).
- **Dependency base:** accepted integration `ea3a7007e23090c1c2b13ea9018b29754579e4ef`, not bootstrap; original G10 baseline `535993a4fa32a5646e0ade6076c6941a71d47230`.
- **Current supplied packet/head:** `874065dbe21f227c4c3189dc859ba5521b210eb5`, parent reports committed/pushed. Final closing/import/PR SHA and host checks/review remain PENDING.
- **Current milestone:** M2-A accepted cumulative review; parent integration and publication next.
- **Active role/model:** Implementer requested Astra/xhigh, configured Luna/max unchanged, serving backend UNVERIFIED; no independent acceptance claim.
- **Next action:** parent reviews/applies exact proposals, obtains cumulative review, imports accepted docs, creates/updates one PR and records actual host facts. No source/Cargo/native/CI work in this checkpoint.
- **Blocker/scope:** no distinct runtime finding (NO_ACTION). Existing incomplete/native gates remain; documentation preparation is not final program acceptance.

| Milestone | State | Evidence / next action |
|---|---|---|
| M0-A | ACCEPTED_COMPLETE | Eleven dispositions, branch/source provenance and dependencies reconciled. |
| M1-A | NO_ACTION | Existing residuals preserved, no new distinct demonstrated defect. |
| M1-B | PENDING | Prior accepted source already in dependency base; G10 accepted docs import awaits review. |
| M2-A | ACCEPTED_COMPLETE | Independent cumulative review accepted7318e862, no substantive findings or extra tests. |
| M2-B | PENDING | PR summary/handoff/checklist prepared; actual publication/head/check facts pending. |
| M3-A | PENDING | Final reconciliation after parent operations; no merge/master-change claim. |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.

- Runtime2026-10-30 M0-A IN_PROGRESS: dependentbranch codex/closeout-g10-residual-defect-triage-integration-20261029-r01 fromaccepted integrationea3a7007; unchangedfetchedmaster4a. Allpriorgoaldispositionspresent. Boundedreconciliationplanner next; no newproductionfeature/testcampaign.

- 2026-10-30 G10 documentation reconciliation: current G10 mutable fields aligned with existing receipts and preserved contracts/events; no new feature/source/test acceptance. Authoritative current action remains parent G10 review/import/publication.
