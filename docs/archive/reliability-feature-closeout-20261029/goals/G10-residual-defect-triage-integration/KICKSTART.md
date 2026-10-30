# G10 KICKSTART — Residual Defects, Deferred Goals and Final Integration Closeout

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Reconcile every executed Goal and source-backed finding, preserve skipped/deferred work, import only accepted verified changes, perform a narrow integration review, prepare a truthful PR and final master handoff without a new unbounded implementation campaign.

**Hard gate:** Follows individual goal dispositions, never requires all goals to pass. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`docs/research/multi-launcher-research-20261028-r01/01-static-audit/FINDINGS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/01-static-audit/FINDINGS.md) — proven versus unaccepted findings
- [`docs/research/multi-launcher-research-20261028-r01/01-static-audit/RECOMMENDATIONS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/01-static-audit/RECOMMENDATIONS.md) — audit dispositions
- [`docs/research/multi-launcher-research-20261028-r01/02-search-discovery/RECOMMENDATIONS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/multi-launcher-research-20261028-r01/02-search-discovery/RECOMMENDATIONS.md) — no shipping intelligent search recommendation
- [`docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/plans/execution/research-follow-ons-20261029/INTEGRATION_STATUS.md) — three complete/one dependency deferred historically
- [Original 17-goal matrix](https://github.com/multiplex55/Multi_Launcher/blob/da898f90b6d83fede9f11f94e32dbeaaef6465c6/docs/plans/execution/multi-launcher-queue-20261025-eedc19d2/INTEGRATION_STATUS.md)

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G10/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Reconcile end-to-end progress and dependencies:** Collect exact goal branch heads and SHA, acceptance classifications, test/native evidence, diff owners and new candidate findings. For each incomplete original Goal 04/05/07/08/13/10/17, record reason, current relevance and whether explicitly user-deferred or upstream-blocked.
   - **M1-A — Evidence-based narrow defect disposition:** Review new demonstrated defects only: code path, adverse scenario, counterevidence, severity, owner, current source and proof scope. For a distinct actionable issue, author full `GOAL.md` and `KICKSTART.md` under per-model role folders and promote a canonical pair; do not implement it within this closing goal. Speculative/non-reproduced issues become bounded research spikes or NO_ACTION.
   - **M1-B — Integrate only accepted checkpoints:** Inspect exact per-goal source patch and test evidence. Cherry-pick -x accepted coherent commits to program integration branch or reapply exact approved diff with source provenance, excluding unaccepted/dirty candidates; resolve minor conflicts at real owners and review. Defer hard dependency features when prerequisite not accepted.
   - **M2-A — Bounded combined review and readiness:** Check `git diff --check`, no unrelated source or configuration files, source/manifest provenance, target selected test compatibility only where goals share changed owners. Do not rerun each previously passed suite just for documentation import. One independent final review for combined behavior/ownership, not a synthetic “all apps tested” claim.
   - **M2-B — PR and final handoff:** Prepare a detailed PR to current master with exact included feature slices, incomplete/default-disabled work, tests/lint/native statuses and blockers. Open PR only when connector/credentials and permission support it; **do not merge automatically**. If blocked, publish a resumable report and finish the queue without repeated loops.
   - **M3-A — Final source-of-truth reconciliation:** Record all accepted and deferred goal outcomes, PR link, head SHA, conflicts, remaining follow-on pairs, source and branch topology, and one final user manual-review checklist. Preserve branches until merged and verified; do not delete source/history.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- No unverified source change or unaccepted native/unsafe candidate reaches the combined integration branch.
- A completed Goal with partial user-visible behavior is labeled `PARTIAL_ACCEPTED`, not full feature completion.
- No automatic execution of historical Goals 04/05/07/08/13/10/17 from an old queue branch.
- Do not implement intelligent discovery, new plugin ABI, universal input/worker system, or standalone new workspace feature without a fresh user decision and evidence.

## Exact test intent

- Exact changed-file manifest and Git blob/hash comparison for each accepted code donor and provenance commit; docs link/JSON validation.
- Nonzero focused cross-owner regression only where source owners overlap or merge conflict changes code; no full Nextest/Clippy acceptance marathon by default.
- Independent final review of accepted source scopes, native-unproven labels, privacy and safety requirements; genuine unsupported areas get follow-on plans.

## Done report to orchestrator

Include `G10` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
