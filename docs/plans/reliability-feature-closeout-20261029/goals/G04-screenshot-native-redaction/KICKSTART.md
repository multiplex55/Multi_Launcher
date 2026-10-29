# G04 KICKSTART — G12 Screenshot Redaction: Native Crop, Output and Interaction Acceptance

**Binding task contract:** [GOAL.md](GOAL.md), overarching [MEGA_GOAL.md](../../MEGA_GOAL.md), [ORCHESTRATOR_KICKSTART](../../ORCHESTRATOR_KICKSTART.md), and [VERIFICATION_POLICY](../../VERIFICATION_POLICY.md). If run under the orchestrator, **do not create a new independent program or ask for this already supplied task scope**.

## Dispatch packet

**Desired result:** Close the outstanding screenshot-redaction M2-A native workflow gap without weakening existing crop, gesture ownership, save/copy semantics or sensitive-content protection. Prioritize actual supported interaction proof over adding unreviewed features.

**Hard gate:** No Table dependency; native proof required for end-to-end completion. Before doing any source work, read the current Goal mutable progress and the authoritative `PROGRAM_STATUS.md` and check this gate. If unmet, write `DEFERRED_DEPENDENCY` with exact missing proof; do not do the blocked implementation, and return to the parent so the next independent goal proceeds.

**Current-state references:**

- [`src/gui/screenshot_editor.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_editor.rs) — `ScreenshotEditor::ui`, `apply_edits`, `save_image`, `copy_to_clipboard`, redaction/crop gesture owners
- [`src/gui/screenshot_capture.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/gui/screenshot_capture.rs) — capture admission, cancellation, worker retirement
- [`src/annotation/redaction.rs`](https://github.com/multiplex55/Multi_Launcher/blob/56223fdc123396f4c41c48e0542506d912c6c634/src/annotation/redaction.rs) — bounded solid/blur/pixelate compositor
- [PR #1455](https://github.com/multiplex55/Multi_Launcher/pull/1455) — G12 M1-A/M1-B accepted, unproved M2-A excluded

## Execute sequentially

1. **Safe start:** discover current repository/worktree/AGENTS instructions; fetch and verify `origin/master`; check user changes, other worktrees and active Git operations. Resume an existing matching task branch only when its program identity and dependency ancestry are valid. Otherwise let parent create fresh named branch from program-start base or accepted dependency. Historical SHA `56223fdc123396f4c41c48e0542506d912c6c634` is planning evidence, never a reset instruction.
2. **Role assignment:** parent orchestrator owns Git, progress and conflicts. Have planner (configured Astra/max) inspect real owners and produce one approved bounded milestone handoff. Have exactly one implementer/author (configured Astra/xhigh, optionally Sol/xhigh if explicitly supported and attributed) execute that handoff. Reviewer (configured Astra/max) examines the stable diff and focused proof. One active child at a time; do not require a second worker to start another goal.
3. **Create durable task state:** write `docs/plans/execution/multi-launcher-reliability-closeout-20261029/goals/G04/` with `SOURCE_REVALIDATION.md`, `PROGRESS.md`, `VERIFICATION.md`, `RISKS.md`, role folders and `follow-on/`. Copy or link this exact Goal/Kickstart. Initialize the mutable progress in GOAL.md; docs baseline commit/push before source work.
4. **Execute checkpoints in order.** Do not enter a subsequent checkpoint until the previous relevant source evidence is accepted or a truthful defer/no-change boundary has been documented:

   - **M0-A — Native feasibility and accepted-source inventory:** Document exact existing controls and distinguish primary redaction drag from secondary crop drag. Inspect input harness/API and supported screenshot pixel-observation mechanisms, identify which native actions can truly be driven, what user-state isolation is needed, and which M2-A aspects are absent from master.
   - **M1-A — Build a truthful synthetic-or-native proof protocol:** Describe one owned scratch screenshot with colored known pixels and secret-like synthetic text, one primary solid-redaction rectangle, one secondary crop, undo/redo, Save to owned temp PNG and Copy to controlled clipboard backend. Define expected pixel/text dimensions and exact order. Prefer automation support; do not simulate a secondary drag by calling internal crop setters while claiming native proof.
   - **M1-B — Correct only demonstrated defects:** If supported automation exposes native path failures, change the smallest actual controller/editor owner and tests. Preserve existing accepted redaction/raster paths, markup history and crop semantics. An unprovable native interaction is recorded as `NATIVE_BLOCKED` with a complete future acceptance Kickstart, not an excuse to cherry-pick the excluded patch.
   - **M2-A — Run focused tests and native path if support exists:** Selected unit/editor tests for raster/coords/hit testing, then one bounded native end-to-end capture-to-save/copy workflow *only* when physical button event capability is confirmed. Inspect the actual saved/copied pixel output. Ensure temporary files and clipboard state are isolated/restored when supported; do not require the user to perform manual checks mid-goal.
   - **M3-A — Acceptance classification:** Declare `ACCEPTED_COMPLETE` only with real tested native gesture/output paths; otherwise mark `UNIT_VERIFIED_NATIVE_PENDING` and publish exact missing driver/tool constraints, safe controls and one final user manual-review checklist for eventual proof.

5. **Verification:** choose actual nonzero targeted tests matched to changed modules; respect per-checkpoint compilation/execution caps in VERIFICATION_POLICY. No default full nextest/Clippy/native campaign. Source-only claims require inspection, not invented passing tests. Native-proof-specific user flows need supported automation, real effects in owned fixtures and actual results; otherwise mark that piece `NATIVE_PROOF_DEFERRED`.
6. **Review and publication:** review the exact cumulative source diff and changed callers. Apply at most one substantive correction cycle, rerun only the affected checks, and preserve failures. Commit coherent accepted checkpoints; attempt one normal push after each. Report real task/parent/head/upstream and push outcome. Parent may import **accepted** changes only; do not independently merge to master or start another Goal.
7. **Follow-on package duty:** for each new *source-supported* actionable issue, write a fully scoped Goal/Kickstart pair in the discovering role's folder and parent canonical library. Include confidence, counterevidence, chosen existing owner, exact scope/callers, migrations/defaults, stage boundaries, focused tests/native proof, dependencies, model assignments, small commits, rollbacks, and defer criteria. Do not execute generated follow-ons or invent an unbounded queue.
8. **Stop handling:** if hard proof/tool/permission/native scope fails, save the source/condition, actual command/error, exact resume step and a Markdown report. Return a `DEFERRED_*` state and let the parent proceed to the next **independent** goal. Never claim future background work or hand off an unfinished source mutation as accepted.

## Must-preserve checklist

- The original screenshot editor, capture cancellation, crop, marker/text/draw and undo/redo tools retain previous behavior.
- Save and Copy must use the same finalized redaction/crop composition; neither may leak previously visible source pixels in a claimed solid-redacted region.
- Blur/pixelate are visual effects, not guaranteed secret erasure; maintain truthful labels.
- Native interaction authority/focus and previous-foreground restoration remain owned by existing capture/editor paths, not a new global mouse hook.

## Exact test intent

- Pure raster tests: Solid obscures exact known pixel region, Blur/Pixelate bounded, crop bounds/transform and layer order, undo/redo final output.
- UI interaction tests: primary redaction and secondary crop are distinct; no underlying click-through/capture leakage if existing interfaces permit proof.
- Native output: owned synthetic image; explicitly invoke real primary/secondary drag; save/copy; compare PNG dimensions/hash/masked pixel coordinates.
- If driver lacks secondary drag, do **not** report end-to-end PASS; log exact missing API and produce a future native proof plan without repeatedly failing the same test.

## Done report to orchestrator

Include `G04` outcome (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `NO_CHANGE_REVALIDATED` or precise defer), source/master/branch/closing SHA/push, diff and owners, tests actually run + nonzero counts, native evidence vs `NOT_RUN`, reviewer and model receipts, unresolved issues, created follow-on Goal/Kickstart paths and integration eligibility. **Never claim the presence of a Kickstart itself proves implementation acceptance.**
