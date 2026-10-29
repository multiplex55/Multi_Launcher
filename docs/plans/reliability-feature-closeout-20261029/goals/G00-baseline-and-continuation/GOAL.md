# G00 GOAL — Source Baseline, Stale Continuation Cleanup and Execution Ledger

> **Program:** `multi-launcher-reliability-closeout-20261029`  
> **Disposition at plan creation:** `NOT_STARTED / NOT_EXECUTED`  
> **Priority:** P0 / mandatory  
> **Lane:** docs-only  
> **Prerequisite:** None; must complete before production work  
> **Companion:** [KICKSTART.md](KICKSTART.md)  
> **Historical planning-time master:** `56223fdc123396f4c41c48e0542506d912c6c634` (October 29, 2026; verify current upstream at runtime; **never reset to this SHA**).

## 1. Goal and user-visible benefit

Make the merged repository the honest starting point for a new sequential program, retire obsolete automatic resume instructions, and create a durable source- and branch-aware program ledger without altering historical research evidence.

**Scope of user authorization when this package is launched:** execute only the milestones and bounded continuation rules in this Goal; this isn't permission to import unaccepted old queue branches, broaden into refactors, run a full test campaign, or merge into `master` automatically. This is a new action program, not the prior Astra research-only program.

## 2. Source-grounded initial observations

- The root `docs/plans/continue.md` currently instructs resumption of `codex/queue-integration-pre-goal17-20261027` from an obsolete commit; treating that as a live command could send Codex into historical work.
- The merge-plan execution reports correctly describe an earlier external approval block, but they are not the post-merge operational status. Preserve them verbatim as dated evidence.
- The two Astra research goals are complete and their four initial follow-on dispositions are already documented; only a new, specific source-supported change should become new production work.

The above is a **planning-time source observation**, not proof of runtime behavior or an assertion that all functions remain identical after this program starts. The worker must re-read the named owners at its actual task base and classify any drift before editing. Important links are pinned to the merged master for provenance:

- [`docs/plans/continue.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/continue.md) — historical command still names an earlier queue branch and commit
- [`docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md) — historical mutable state predates the actual merges
- [`docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md) — dated four-package disposition
- [PR #1456](https://github.com/multiplex55/Multi_Launcher/pull/1456) and [PR #1457](https://github.com/multiplex55/Multi_Launcher/pull/1457) were merged into `56223fdc123396f4c41c48e0542506d912c6c634`

## 3. Exact ownership, permitted changes and exclusions

- `docs/plans/continue.md` only after verifying it is an intentional active entrypoint; update it to a stable pointer to this orchestrator or a safe non-executing index.
- New files only under `docs/plans/execution/<PROGRAM_ID>/`; the supplied plan files under `docs/plans/reliability-feature-closeout-20261029/` may have a marked mutable progress section.
- Read-only inspect `AGENTS.md`, `.codex/config.toml`, `.codex/agents/{planner,implementer,reviewer}.toml`, `master`, remote, existing worktrees and in-progress Git operations.

**Non-goals / changes to refuse:**

- Do not restart historical Goals 04/05/07/08/13; do not try to reconstruct unmounted historical Codex worktrees.
- No running test suite just to validate a documentation-only change.

New files may be created under the goal-specific execution directory `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G00/` and in tightly scoped production owners above only. Do not edit `.codex` role/configuration files, historical research reports, unrelated worktrees, vendored crates, snapshots or independent user content. A previously uncommitted or blocked candidate is **evidence only**, never an implementation source to cherry-pick wholesale.

## 4. Required behavior and preserved invariants

- Do not modify historical research, experiment data, original queue receipt, previously merged PR descriptions or public SHA evidence.
- No production source, test, Cargo/lockfile, agent model settings or native UI changes.
- A future Codex reader must not be told to restart a previously merged queue on a historical source branch.
- The program source pin is a historical planning observation, not a checkout reset mandate.

If a required invariant has a concrete counterexample that the stated milestone cannot fix without substantial adjacent work, **stop that checkpoint**, preserve its evidence and author an explicit follow-on Goal/Kickstart. Continue the next independent program goal. An accepted subset may be integrated only if default behavior is safe, the subset is independent and the acceptance scope is truthful.

## 5. Milestones and bounded handoffs

| Checkpoint | Owner and outcome | Acceptance / intended commit |
|---|---|---|
| `M0-A` | Source verification and safe bootstrap | Scoped work + tangible artifact; `docs(closeout): [G00/M0-A] ...` or `fix/feat` when source changed |
| `M1-A` | Retire outdated auto-resume wording | Scoped work + tangible artifact; `docs(closeout): [G00/M1-A] ...` or `fix/feat` when source changed |
| `M2-A` | Create truthful cross-goal ledger | Scoped work + tangible artifact; `docs(closeout): [G00/M2-A] ...` or `fix/feat` when source changed |
| `M3-A` | Docs-only review and checkpoint | Scoped work + tangible artifact; `docs(closeout): [G00/M3-A] ...` or `fix/feat` when source changed |

### M0-A — Source verification and safe bootstrap

**Objective and detailed handoff.** Fetch current `origin/master` once; confirm repository/worktree, status, staged paths, upstream, and relevant user edits. Record exact new MASTER_START and relevant source hashes. Create program integration branch and bootstrap the entire plan package as its first commit. Never reset to the planning-time SHA.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M1-A — Retire outdated auto-resume wording

**Objective and detailed handoff.** Confirm `continue.md` is a live pointer. Replace its obsolete imperative with one stable reference to the new orchestrator and an explicit historical-only label for the old queue. Do not alter `AGENTS.md`, frozen research result files, prior stop receipts or any user-specific configuration.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M2-A — Create truthful cross-goal ledger

**Objective and detailed handoff.** Initialize `PROGRAM_STATUS.md`, `BRANCH_MAP.md`, `SOURCE_BASELINE.md`, `RISKS_AND_DECISIONS.md`, `FOLLOW_ON_CATALOG.md` and per-role directories. State current and historical source states separately; record PR merges #1456/#1457 and current master; provide exact next action.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### M3-A — Docs-only review and checkpoint

**Objective and detailed handoff.** Check Markdown links locally when possible, verify no source/config/manifests touched, obtain one short independent review, commit/push. A changed or missing historical file becomes a cited limitation, not a reason to fabricate it.

**Verification/Done boundary.** The exact artifact or behavior described here must be source-corroborated. Author and reviewer must record the selected tests, proof limits and unresolved competing owner assumptions, if any. Commit a coherent checkpoint and attempt one normal push. Never call a design-only note a production acceptance.


### Reviewer handoff rule

For each completed production milestone, the parent must provide a reviewer: actual diff from the correct base, original objective, named callers and existing defenses, new tests and selection inventory, first meaningful failure if any, exact run/receipt, stop/accept criteria and non-goals. The reviewer should return **severity-ranked substantive findings only** and identify stale callers, duplicate state, false positive test passes, unsafe activation, migrations or ownership violations. One substantive correction pass is allowed before a recorded stop/spike; do not loop indefinitely.

## 6. Focused acceptance / negative regression tests

- `git status --short`, `git diff --check`, scoped `git diff --name-only` and Markdown link/path existence checks; **no Cargo**.
- Check `continue.md` points to an actually committed document and cannot be mistaken for an automatic old-queue invocation.
- Record one reviewer disposition and commit/push results.

Test execution is **not automatic simply because it is listed here**; first identify the actual Cargo package, present test names and test-target support. Select a nonzero meaningful subset. Prefer `cargo nextest list` and `cargo nextest run` scoped to named modules or exact test cases; if a test command selects zero, that is not PASS. Default allowance for one changed source checkpoint is one selected preparation, one selected run and at most two correction reruns **only when required**. The program-wide budgets in [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md) control timings, native proof and documentation-only checks. Never weaken tests, delete failures or claim native acceptance from pure raster/source tests.

## 7. Decision gates: block only unsafe scope, not the whole queue

- If `continue.md` is intentionally owned by another active external process or contains conflicting uncommitted edits, preserve it and create a new advisory index; record `DEFERRED_USER_OWNED` for that edit.
- If a new integration branch already exists with the same program identity, resume it instead of recreating it.

Classify each checkpoint as `PENDING`, `IN_PROGRESS`, `ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, or `NO_CHANGE_REVALIDATED`. The label must match the documented facts; a `PARTIAL_ACCEPTED` status is not feature-wide completion. On a true stop, create a durable per-goal `stalls/<checkpoint>-UTC.md` with actual source/ref, blocker, attempted one-shot fallback, unrun commands, ownership/cleanup, accepted files and exact next step; then return control to the parent to schedule the next independent goal.

## 8. Branch, commit, push, integration and model rules

1. Parent verifies actual `origin/master` and the program's accepted integration ancestry. Each **new** Goal receives its own branch `codex/closeout-g00-baseline-and-continuation-<RUN_ID>` from the pinned program-start branch (contains only this plan) unless an explicitly accepted predecessor must be present. Dependent goals start from the prior **accepted integration commit** and record this exception. Never base on a rejected/dirty historical implementation tip.
2. Keep one active writer per goal; parent owns Git and the authoritative program ledger. Create the goal's execution directory and a `M0-A` docs baseline commit before editing production source. Accepted milestones each get meaningful Conventional Commit-style subjects including `G00/<milestone>` and one normal non-force push after commit. If push fails, preserve local commits and proceed; remote success must be recorded truthfully.
3. Parent integrates only reviewed, acceptance-gated production/documentation checkpoints into `codex/closeout-integration-<RUN_ID>` in dependency order (normally `git cherry-pick -x`). Do not import failed/unaccepted source. A rejected goal may contribute its **documentation-only** stop report/complete future pair, not an unsafe patch.
4. Role defaults verified from current `.codex`: orchestrator `gpt-6-astra/xhigh`; planner/reviewer `gpt-6-astra/max`; implementer `gpt-6-astra/xhigh`. These are configured role preferences, not proof of actual backend. If supported and selected for a narrow bounded implementation, `gpt-6.1-sol/xhigh` may be used deliberately, with honest attribution. No silent `.codex` configuration edits.
5. Roles contribute `roles/planner/`, `roles/implementer/`, `roles/reviewer/` and `roles/orchestrator/` Markdown under this goal's execution directory. New actionable recommendations get complete role-authored `roles/<role>/follow-on/<id>/GOAL.md` and `KICKSTART.md`; parent deduplicates into a canonical `follow-on/<id>/` library. Do not fabricate findings to fill quotas.
6. Do not ask the user to manually test between milestones; use supported app/native automation if available. If unavailable, log the gap and reserve a **single final manual-review checklist** for the program handoff.

## 9. Completed outcome definition

The new run has a verified source/role baseline, a safe current continuation pointer or an explicit protected-edit disposition, a published status ledger, and zero unauthorized source changes.

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
| `M0-A` | PENDING | — | Source verification and safe bootstrap |
| `M1-A` | PENDING | — | Retire outdated auto-resume wording |
| `M2-A` | PENDING | — | Create truthful cross-goal ledger |
| `M3-A` | PENDING | — | Docs-only review and checkpoint |

### Append-only execution events

- Planning package created 2026-10-29; no goal code, test, branch, PR or merge was executed by generating these documents.
