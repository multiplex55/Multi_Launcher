# G08 KICKSTART — G15 MkMacro: Bounded Run History and Conditional Safe Recipes

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Make the already captured metadata-only MkMacro terminal summaries useful through a bounded, privacy-preserving history UI, and decide whether a small safe recipe workflow can be supported without bypassing existing macro execution authorization.

**Hard gate:** Recipes require accepted history and explicit policy; do not auto-run after mere display success. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/mkmacro/runtime/summary.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/mkmacro/runtime/summary.rs) — `MacroRunSummary`, `SummaryHistory`, safe failure categories and bounded capacity
- [`src/mkmacro/runtime.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/mkmacro/runtime.rs) — runtime entry points and terminal summary publishing
- [`src/gui/mkmacro_dialog/mod.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/mkmacro_dialog/mod.rs) — existing macro UI if this path still owns the dialog
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G15 M1-A summary metadata only; history UI/recipes missing

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G08/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — History owner and privacy contract:** Inspect current summary history API/retention, actual UI owner, lifecycle and safe metadata fields. Specify an on-demand, bounded read view with a stable run key and no unbounded Clone/long-held lock; clarify whether history is in-memory only.
   - **M1-A — User-visible history:** Add a History view with most recent completed run, outcome, timing, source, safe failure label and optional per-step aggregate details. Include bounded filter/sort/clear as already supported, no live raw log or secret payload. Lazy refresh only when dialog visible, preserve UI responsiveness and no idle polling.
   - **M1-B — History ownership/lifecycle tests:** Test completed-run retention/eviction, byte quotas, zero/error/busy/paused outcomes, malformed safe labels, history view reopen/clear, missing macro name and concurrent publication snapshot validity without blocking long-running worker. Preserve metadata-only contract.
   - **M2-A — Recipe scope decision then optional implementation:** Define what a recipe means from actual existing macro facilities: perhaps named saved launch invocation/profile, never replaying the last run from side effects. If an existing validated, permission-aware authoring/launch seam supports it with small migration, expose **one** explicit user-authored recipe with confirmation and no auto-trigger. Otherwise produce `RECIPES_DECISION.md` and a complete future Goal/Kickstart; do not manufacture execution shortcuts.
   - **M3-A — Focused review and closure:** Review privacy, per-run source accuracy, bounded RAM/UI lifetime and user-activation authority; commit/push history independent of uncertain recipe work. Report exactly whether recipe subset shipped, deferred, or rejected.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- No raw keystrokes, clipboard contents, OCR results, pixel buffers, file contents, secret variables or arbitrary error strings in run-history UI/diagnostics.
- History reading must not escape existing max count/byte/step-row charges, retain evicted records or hold the lock through egui rendering.
- Run/Stop/Pause, nested calls and original invoker attribution remain exactly as before.
- A displayed record never acts as an authorization to auto-rerun unsafe actions; recipes reuse existing validation and require explicit intent.

## Exact test intent

- In-memory history bounded eviction, byte charge and ongoing run/terminal publication under concurrent view.
- UI no-secret snapshot, safe labels, empty state, filtering, selection and close/reopen; no continuous repaint when hidden.
- If recipe implemented, explicit permission/revalidate/disabled target and unsafe action refusal; no new macro runner.

## Done report to orchestrator

Include `G08` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
