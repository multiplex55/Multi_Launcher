# G00 KICKSTART — Source Baseline, Stale Continuation Cleanup and Execution Ledger

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Make the merged repository the honest starting point for a new sequential program, retire obsolete automatic resume instructions, and create a durable source- and branch-aware program ledger without altering historical research evidence.

**Hard gate:** None; must complete before production work. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`docs/plans/continue.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/continue.md) — historical command still names an earlier queue branch and commit
- [`docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/research-merge-20261029/GOAL_RESEARCH_INTEGRATION_AND_MERGE.md) — historical mutable state predates the actual merges
- [`docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md) — dated four-package disposition
- [PR #1456](https://github.com/multiplex55/Multi_Launcher/pull/1456) and [PR #1457](https://github.com/multiplex55/Multi_Launcher/pull/1457) were merged into `56223fdc123396f4c41c48e0542506d912c6c634`

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G00/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Source verification and safe bootstrap:** Fetch current `origin/master` once; confirm repository/worktree, status, staged paths, upstream, and relevant user edits. Record exact new MASTER_START and relevant source hashes. Create program integration branch and bootstrap the entire plan package as its first commit. Never reset to the planning-time SHA.
   - **M1-A — Retire outdated auto-resume wording:** Confirm `continue.md` is a live pointer. Replace its obsolete imperative with one stable reference to the new orchestrator and an explicit historical-only label for the old queue. Do not alter `AGENTS.md`, frozen research result files, prior stop receipts or any user-specific configuration.
   - **M2-A — Create truthful cross-goal ledger:** Initialize `PROGRAM_STATUS.md`, `BRANCH_MAP.md`, `SOURCE_BASELINE.md`, `RISKS_AND_DECISIONS.md`, `FOLLOW_ON_CATALOG.md` and per-role directories. State current and historical source states separately; record PR merges #1456/#1457 and current master; provide exact next action.
   - **M3-A — Docs-only review and checkpoint:** Check Markdown links locally when possible, verify no source/config/manifests touched, obtain one short independent review, commit/push. A changed or missing historical file becomes a cited limitation, not a reason to fabricate it.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- Do not modify historical research, experiment data, original queue receipt, previously merged PR descriptions or public SHA evidence.
- No production source, test, Cargo/lockfile, agent model settings or native UI changes.
- A future Codex reader must not be told to restart a previously merged queue on a historical source branch.
- The program source pin is a historical planning observation, not a checkout reset mandate.

## Exact test intent

- `git status --short`, `git diff --check`, scoped `git diff --name-only` and Markdown link/path existence checks; **no Cargo**.
- Check `continue.md` points to an actually committed document and cannot be mistaken for an automatic old-queue invocation.
- Record one reviewer disposition and commit/push results.

## Done report to orchestrator

Include `G00` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
