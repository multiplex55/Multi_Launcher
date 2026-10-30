# G02 KICKSTART — G09 Table Utility: Full-File Publication Safety Foundation

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Close the safety design/owner gap behind G09 M2-A: establish the smallest existing-owner, source-bound transaction/publication contract necessary before Table Utility can safely perform full-file transformation/export beyond bounded file-backed browsing.

**Hard gate:** None; G03 depends on this goal reaching ACCEPTED_COMPLETE. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/table_utility_session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/table_utility_session.rs) — `TableFileBackedSource`, work/session generations, export receipts, resource leases
- [`src/gui/table_utility_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/table_utility_dialog.rs) — file-backed paging and compact export/overwrite workflows
- [`src/common/atomic_file.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/common/atomic_file.rs) — existing atomic-save helpers with limited guarantees
- [`crates/multi_launcher_table_core/src/lib.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/crates/multi_launcher_table_core/src/lib.rs) — existing extracted table-core package
- [Previous partial G09 integration](https://github.com/multiplex55/Multi_Launcher/pull/1455) — M1-A/M1-B accepted; M2-A/M2-B not accepted

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G02/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Reconfirm precise gap and finalize safety contract:** Inspect actual file-backed page/source/export code and prior integration report. Compare read admission, last-good source, source file-identity/fingerprint, selected-row semantics, output occupancy, cancellation, failed writes and recovery. Save `SAFETY_CONTRACT.md`, source/call-site matrix, rejected alternatives, and M2-A exact acceptance before source changes.
   - **M1-A — Build smallest safe output reservation/publishing seam:** Use an owner-private output reservation/manifest capturing destination existence/identity and source revision. Fail closed on stale sources, occupied or swapped outputs, unsupported same-path in-place overwrite and external modification. Define staging in destination filesystem, bounded streaming, flush/close, publish, cleanup and exact failure retention. Preserve no-clobber/overwrite confirmation semantics; do not assume `rename`/`MoveFileExW` alone provides cross-file transaction/recovery.
   - **M1-B — Handle cancellation, partial writes and recovery:** Keep last valid document/page available during rejected work; account bytes during streaming, cancellation, session close and stale worker completion. Clean up task-owned temp objects on recoverable failure; retain/reconcile any unresolved publication receipt before new mutation. If robust recovery would require broad architectural changes, stop before publication and output a fully scoped design package, not an unsafe writer.
   - **M2-A — Focused deterministic proof:** Use synthetic bounded files to test source replaced during scan, revision changes, invalid destination, destination created between preview/write, write short/error, cancellation before/after staging, worker shutdown, quota release and last-good-state preservation. Single chosen test subset plus focused first-party lint as needed; native acceptance only for the exact Windows identity/publish boundary after it is actually supported.
   - **M3-A — Safety gate outcome:** Accept only after exact owner and source-of-truth invariants demonstrably pass. Otherwise classify `DEFERRED_WITH_DESIGN` and preserve an implementation-ready Goal/Kickstart identifying the one unresolved native/transaction requirement. Do not authorize G03 until G02 accepted.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- No full-file destructive write may become reachable before source/output freshness and failure/recovery contracts are proven.
- Keep G09 M1-A/M1-B browsing and compact CSV/TSV/JSON workflows unchanged, including last-good data and row/column selection.
- Stale session, source/view/selection/export operation IDs never publish an old result. Resource leases/quotas are released only on actual ownership completion.
- Do not reintroduce a prior excluded unsafe M2-A candidate or add a second table engine/command path.

## Exact test intent

- Pure/owned temporary-file tests for read identity and stale source, invalid same-path output, destination conflict/no-clobber, partial write, cancellation and recovery.
- Controlled stream/writer-failure injection; source and output hashes prove no unintended overwrite.
- Existing file-backed browse/page and compact export tests selected for directly affected paths.
- If native Windows safety proof cannot be performed by available supported tooling, record `NATIVE_NOT_PROVEN` and **do not** claim foundation acceptance or make publication available.

## Done report to orchestrator

Include `G02` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
