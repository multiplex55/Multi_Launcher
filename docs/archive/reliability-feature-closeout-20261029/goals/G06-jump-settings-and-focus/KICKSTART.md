# G06 KICKSTART — G11 Jump Mode: Finish Customization UI and Focus Regression Coverage

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Complete the existing Jump Mode preference editor and reliable focus behavior while preserving the validated configurable chord/hint alphabet and current ownership/Enter activation semantics.

**Hard gate:** None. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/gui/jump_mode/config.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/jump_mode/config.rs) — `JumpModeChord`, `HintAlphabet`, validation/default fallback
- [`src/gui/jump_mode/ui.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/jump_mode/ui.rs) — semantic target and viewport/navigation ownership
- [`src/gui/render.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/render.rs) — query root and Jump integration
- [`src/settings/model.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/settings/model.rs) — persisted Jump preferences
- [`src/settings_editor.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/settings_editor.rs) — existing Settings editor entry point
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G11 M1-A/M1-B prefs/controller accepted; editor/native closure deferred

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G06/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Source and UX contract:** Trace initial activation chord, hint alphabet allocation, Settings draft/live committed fields, loaded invalid values, hotkey precedence and already-open dialog focus. Record exactly which UI controls are absent versus already available.
   - **M1-A — Editor and validation:** Add/choreograph Settings inputs for activation chord and allowed hint alphabet; expose clear inline validation, defaults/restore and stable preview of effective hints before saving. Reject conflicts/reserved keys and duplicate/unusable hint alphabets rather than silently committing invalid data.
   - **M1-B — Commit and live application:** Use existing Settings transaction and acknowledged live application; preserve original pending draft on validation failure; update Jump controller only on successful commit, invalidate stale hint session/viewport focus once and retain captured key-release fences.
   - **M2-A — Focused navigation tests:** Test initial launcher query focus after hotkey; Enter executes only after full hint selection; Esc cancellation; Shift+Space when typing; edited chord and hint alphabet; already-open Note/Settings/dialog; foreign foreground ownership; fallback from malformed saved setting; disabled Jump path. Use existing egui test seams/owned native proof only if changed.
   - **M3-A — Close with precise UI evidence:** Capture screenshot(s) of actual Settings/editor and Jump hints using supported UI tooling when available, and record changed-file/test proof. If native focus cannot be exercised, label the remaining acceptance item rather than recasting it as unit-PASS.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- Default Shift+Space behavior stays the same for users who have never configured a custom chord.
- Existing launcher hotkey always independently focuses the query when appropriate, even with Jump enabled; avoid suppressing search input when notes/dialogs close.
- Jump hint full sequence focuses/selects; Enter performs action; Esc exits; competing input owners and native focus fencing remain authoritative.
- No settings schema change without backward-compatible fallback/migration proof.

## Exact test intent

- Chord parse/display/roundtrip, conflict and reserved-key errors, invalid saved preference fallback.
- Hint alphabet uniqueness/length, stable deterministic target allocations and Jump session invalidation.
- Focused root query/Note/Settings modal input tests with nonzero selected test cases; native foreground context only if tool can prove it.
- Screenshot UI verification using first-party harness if available; no manual interruption until the program final review.

## Done report to orchestrator

Include `G06` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
