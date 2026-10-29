# G01 KICKSTART — Folder Diff: Preserve Destination Read Errors and Operation Feedback

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Fix the two independently source-supported information losses in Folder Diff: distinguish destination `NotFound` from all other read errors during mutation admission, and preserve/display delivered current-view failure/cancellation/partial results across normal rescans.

**Hard gate:** None; primary source-supported bugfix. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/diff/file_ops.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/diff/file_ops.rs) — `fresh`, `FileIdentity::read`, `execute_copy`, `ExecutionReport`, `ItemOutcome`
- [`src/gui/diff_dialog/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/diff_dialog/mod.rs) — `poll_folder_runtime`, `refresh_after_mutation`
- [`src/diff/folder_runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/diff/folder_runtime.rs) — `FolderRuntime` / active operation
- [`tests/domain_cases/diff_file_ops.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/tests/domain_cases/diff_file_ops.rs) — existing bounded copy/recycle coverage
- [`docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/GOAL.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/GOAL.md) — already complete source-grounded original proposal
- [`docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/KICKSTART.md`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/docs/research/diff-mutation-authority-r01/proposals/diff-failure-information/KICKSTART.md) — original execution instructions

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G01/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Revalidate exact owner and historical handoff:** Read the original merged Goal and Kickstart, check the two cited mechanisms still exist on the actual task base, inspect plan/executor and GUI caller tests. If one is already corrected, mark a source-backed no-op for that boundary; do not double-fix it.
   - **M1-A — Preserve destination observation errors:** In `fresh`, map successful identity to `Some`, **only** `io::ErrorKind::NotFound` to `None`, and all other errors to explicit per-item validation failure with error context before `fs::copy`. Maintain expected `None`/`Some` freshness, dirty path, root/canonical containment, preview direction and overwrite semantics.
   - **M1-B — Preserve current-operation feedback:** At actual completed-report consumption, validate generation AND owning view ID AND ordered left/right root identity captured at dispatch **before** result adoption and rescan. Store bounded, transient current-view feedback; retain it across normal same-view rescans and preview open/cancel, but clear or replace on new dispatch, view discard, side/root swap, or changed retained parent. Present exact failed paths/reasons, cancellation and preceding successes without double counting or rollback claims.
   - **M2-A — Focus tests and review:** Add injected `NotFound`, `PermissionDenied` and another non-NotFound error; normal absent/existing overwrite controls; delivered `Completed` success+Failed, cancelled, stale generation, same-ID root swap/parent change, rescan retention and new-run replacement. Run a nonzero targeted Nextest selection only, examine all callers and one independent review. Fix one bounded substantive finding if needed.
   - **M3-A — Publish accepted correction:** Inspect cumulative diff and documentation, commit/push accepted source and exact test receipts; if an unresolved correctness failure remains, mark `INCOMPLETE` and do not import implementation. No native race experiments.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- Preserve approved preview direction, captured exact paths, containment, nested selection, dirty destination protection, recycle-only GUI, no Permanent fallback, and between-item cancellation.
- Report lifecycle must be bound to current view plus ordered source/destination roots, not just a generation or view ID.
- A delivered partial operation is not an atomic rollback; copied or recycled successful items stay reported as completed.
- Do not change the existing native file identity, trash backend, journals, stored schema, confirmation or command bus.

## Exact test intent

- Injected filesystem observation: `NotFound` permits expected absence, `PermissionDenied`/`InvalidInput` are failures and do not invoke the effect.
- Existing-source expected destination + read failure or changed identity refuses; valid ordinary copy/overwrite still admits.
- Inject `ExecutionReport` with successes followed by failure and with cancellation; verify bounded GUI details and no invented results.
- Simulate swapped ordered roots or reused view ID before reconciliation; stale results must not publish, while a same-root rescan preserves accepted feedback.
- One targeted Nextest batch (nonzero inventory) plus narrow lint/format only if the code change warrants; no broad suite.

## Done report to orchestrator

Include `G01` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
