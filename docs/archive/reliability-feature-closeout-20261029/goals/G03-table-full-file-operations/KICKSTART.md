# G03 KICKSTART — G09 Table Utility: Conditional Full-File Operations and UX Closure

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Only if G02 has proved its publisher and identity invariants, complete the user-facing full-file operation(s) explicitly supported by the current Table utility contract, with bounded memory, truthful selection scope, progress and cancellation.

**Hard gate:** G02 accepted safety foundation, otherwise DEFERRED_DEPENDENCY without source changes. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/gui/table_utility_dialog.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/table_utility_dialog.rs) — `show_file_backed_results`, `show_footer`, export/confirmation controls
- [`src/table_utility_session.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/table_utility_session.rs) — `TableExportKind`, `TableExportReceipt`, session worker
- [`crates/multi_launcher_table_core/src/lib.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/crates/multi_launcher_table_core/src/lib.rs) — separate existing table core
- [G09 partial integration](https://github.com/multiplex55/Multi_Launcher/pull/1455) — paging currently supported, complete full-file workflow missing

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G03/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Hard readiness gate and user-facing scope:** Prove G02 `ACCEPTED_COMPLETE` and own source integration. Enumerate supported full-file operations and inputs from current table core; decide a minimal user-visible first deliverable (e.g. full-file CSV export when valid) and explicit unsupported operations. Record whether sort/filter is view-only or truly whole-file and why.
   - **M1-A — Bounded full-file worker:** Implement only the chosen streaming source-bound operation, with clear source revision/selection semantics, stable column mapping, quotas and cancellation checkpoints; use accepted G02 publisher unchanged. Reject missing or changed sources and unsupported row/projection combinations.
   - **M1-B — Unambiguous UI and progress:** Show page vs whole-file scope, projected sample vs actual output, bytes/rows progress and cancellation. Preserve compact workflows and optional previous-valid behavior. Keep output confirmation and error details consistent with existing Table dialog owner.
   - **M2-A — Focused proof and decision:** Run small synthetic normal/partial/cancellation/stale-source/overwrite fixtures; where feasible exercise one bounded >500 MiB synthetic source without reading it entirely into RAM, under an explicit resource/time cap and owned cleanup. If unavailable, mark large-source acceptance incomplete, not passed.
   - **M3-A — Close or defer:** If full-file and resource/native invariants are met, label completed subset exactly. If only a safe subset is implemented, mark `PARTIAL_ACCEPTED` with unsupported features disabled and publish a narrow follow-on pair for omitted operations.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- No value from one page is misrepresented as full-file export, sorted selection, filter, or safe replacement.
- Never export/write from a stale source or stale selection revision; preserve last-good compact state.
- Memory use remains bounded by configured accepted source/pool quotas and output chunk sizes; no whole-file materialization to dodge design complexity.
- No activation of G10 or G17 based on partial G09 acceptance.

## Exact test intent

- Nonzero selected tests for full-source versus page row counts, selected columns, mixed widths and headers.
- Synthetic source changed during preparation, cancellation mid-stream, output already occupied, source deleted, late/stale worker result and last-good restoration.
- Bounded large-file/lifecycle acceptance when supported; explicit `NOT_RUN` if unavailable, not generic elapsed-time claims.

## Done report to orchestrator

Include `G03` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
