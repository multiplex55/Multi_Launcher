# CODEX KICKSTART — Implement Multi Launcher Jump Mode Autonomously

**Instruction to Codex orchestrator:** Begin the complete approved Jump Mode implementation program now. This is a **single, continuous, long-running execution task**, not a request for a new open-ended plan or another round of questions.

## Primary authority

1. Read `AGENTS.md` from **your actual current repository checkout** and follow its orchestration, Git, checkpoint commit, testing, reviewer and best-effort push policies.
2. Find and read the complete **`CODEX_GOAL_JUMP_MODE.md`** implementation plan in the repository, preferably at `docs/plans/CODEX_GOAL_JUMP_MODE.md` (or the actual locally present path if stored elsewhere). This document is the authoritative feature specification, milestone scope, guardrails, GUI-harness acceptance matrix, and done criteria. **Read the whole file before broad changes.**
3. Work from your **current checked-out source**, tests, and configuration. Use that checkout as the source of truth. Do **not** reference or require any uploaded ZIP files, historical snapshot, old branch, prior Codex artifacts, or screenshots as code authority. The user screenshots in the request are UI examples only.
4. Treat all previously approved product decisions in the Goal as settled. Resolve routine Rust/egui/Windows implementation uncertainties yourself by inspecting source and testing. Do not ask the user to make more choices mid-implementation.

If the Goal document is genuinely missing, search the current repository once for its exact basename. Do not silently substitute a different historical plan or invent a new scope. Record a clear missing-input blocker rather than modifying production code without the approved plan.

## Critical first action — create and own a new branch

**Before editing source:**

- Inspect `git status --short --branch`, the checked-out branch, `HEAD` SHA, and configured remotes.
- Create a **new unique task branch** named in the style `feat/jump-mode-<date>-<unique-suffix>` **from the checked-out HEAD**. Do not work directly on `master` or any pre-existing feature branch without creating a new branch.
- If the current tree contains user changes, preserve them. The two user-supplied Jump Mode Markdown documents are **task inputs**, not unrelated changes. If only those docs are untracked, a new branch in the current worktree is safe; otherwise prefer a new clean Git worktree off HEAD if supported. If using a separate worktree, explicitly copy those two task input docs into it so the approved Goal remains accessible; **do not copy or commit unrelated dirty work**. Never reset, clean, stash, overwrite, or accidentally commit the user's pre-existing edits. Do not fetch/rebase/merge simply to change the base.
- Record the starting SHA, new branch, relevant environment facts, and source ownership map in `docs/plans/JUMP_MODE_EXECUTION.md`.
- Own this branch throughout. Do not merge it or create a PR automatically. Do not force push.

**Best-effort push:** After each coherent stage/checkpoint commit, attempt **one normal push** to the intended remote/upstream. If remote access, auth, or push fails, retain the local commit, record failure, and **continue immediately**. Do not request credentials or user help. Do not loop on push failures, rewrite history or abandon the work because pushing was blocked.

## Execution model — no user intervention

Execute the entire Goal in order, **M00 through M15**, moving automatically from each completed milestone to the next:

- **P0 — M00–M01:** Branch/base ownership inspection, source map, safe isolated Windows GUI fixture/launch/capture/cleanup harness skeleton.
- **P1 — M02–M05:** Disabled-by-default persisted setting, pure deterministic prefix-free A–Z hint engine, stable target/surface registry, focus/selection intent, keyboard lifecycle and nonintrusive visual overlay.
- **P2 — M06–M09:** Main action grid/`note list` focus-then-Enter behavior, dashboard discrete controls/search field, actual keyboard-driven screenshot harness, Phase-1 functional review and stabilization.
- **P3 — M10–M13:** Modal ownership, Settings, representative Notes/Todo/Timer/Paste Sequence dialogs, common complex controls and representative MkMacro coverage, extended automated UI evidence.
- **P4 — M14–M15:** Focused reliability/performance review, regression checks, consolidated evidence and final autonomous handoff.

This is a **mandatory multi-phase feature implementation**, not “Phase 1 only.” However, optional speculative enhancements from the requirements discussion (external UIA, arbitrary mouse targeting, offscreen auto-scroll, mnemonic/fuzzy matching, direct execution, broad theme redesign) are not part of this run. Respect each milestone's explicit dependency, non-goals and acceptance criteria.

### Delegation and ownership

Use the repository's configured orchestration roles whenever available:

- **Planner:** Read-only source ownership clarification and a concise implementer handoff at meaningful phase boundaries or when real repository facts contradict the Goal. Do **not** commission another long general plan or ask it to broaden the scope.
- **Implementer:** One bounded milestone (or coherent checkpoint) at a time, with explicit tasks, invariants, tests, target paths and acceptance criteria from the Goal. Only one writer at a time; do not run parallel implementers against overlapping repository state.
- **Reviewer:** Read-only examination after important integration boundaries and once near completion; concrete correctness or regression findings only. Feed actionable findings back to one implementer for targeted repair; avoid endless style-refinement loops.
- **Parent/orchestrator (you):** Own branch/worktree, persistent ledger, milestone transitions, verification, commit/push cadence, final results and screenshot index. Stay active until complete or truly blocked.

Follow **Active Checkpoint Commit Cadence** as already specified in `AGENTS.md`: commit coherent `M#-A`, `M#-B` work when it is meaningful, not each tiny edit and not a giant one-shot commit after many milestones. Do not alter agent configuration or `AGENTS.md` to accomplish this task.

## Required runtime semantics — never compromise

- `Shift+Space` while Multi Launcher is focused starts Jump Mode; the same chord while active cancels it. Escape cancels; Backspace removes a typed hint character.
- Short, uniquely assigned **prefix-free A–Z hints**, displayed uppercase, typed case-insensitively, appear next to *visible eligible actionable* controls. The mapping is stable for a session and deterministic for identical target populations.
- **Hint completion focuses/selects but NEVER executes.** The overlay exits; a later distinct Enter/Space/arrow or normal character input interacts with the target through its **existing** action/focus behavior. A successful jump must visibly identify the target.
- Preserve existing search text and focused field contents, including the `Shift+Space` chord never adding a stray space. No action or note search should be executed just because a hint was typed.
- Dynamic/removed targets must never accidentally resolve to a different command through an outdated index. Background controls behind dialogs are not selectable.
- Feature flag defaults **off** and legacy settings load off. When off, existing global hotkey, radial/pie menu, query, grid and dialog keyboard behavior must remain unchanged.
- Build toward broad support through **small opt-in integration adapters**, not a second action dispatcher, rewritten plugin framework or generalized system-wide navigation layer.

## GUI test harness is mandatory, and must run itself

The user explicitly wants you to **launch the actual app, interact through the keyboard, take screenshots, and close the app on your own**. The resulting screenshots are for the user's later/asynchronous inspection; they are **not** permission to request live manual checks.

- Reuse or carefully adapt existing `src/gui/qa_framebuffer.rs`, radial/smoke acceptance patterns and `dev/run_owned_process.ps1`/other process-containment utilities as appropriate after reading them. Do not duplicate large infrastructure unnecessarily.
- Create safe, deterministic fixture/config/data sources under `dev/jump_mode/`; isolate them completely from real notes, commands, clipboard data, real macros and other production user state. No destructive or externally visible action testing.
- Build/run an instrumented **real GUI** session on an interactive Windows desktop if available. Use actual keyboard events to exercise the leader, partial hints, full selection, Enter, cancel, text preservation, dense hints, modal ownership, Settings and representative dialogs.
- Collect **both** named screenshots and structured events/trace assertions proving that a target became selected before anything executed and the correct existing handler ran exactly once after Enter. Images alone are not sufficient evidence.
- Every run must create `dev/jump_mode/evidence/<run-id>/` with screenshot PNGs, a manifest, `scenarios.json`, `events.jsonl`, `summary.md`, stdout/stderr and **process cleanup proof**. Update evidence incrementally so it is readable **while the plan runs**. Reference the latest run and useful PNGs from the execution ledger. At phase boundaries, also retain a small **fixture-only** curated PNG selection in `docs/plans/jump_mode_review/` (about 8 MiB total budget) and commit/push best-effort so remote branch review is possible. Do not commit the entire raw screenshot set or any private content.
- The harness controls only the app process tree **it launched**. Close gracefully, wait for exit, then perform bounded cleanup by verified PID identity if needed. Never kill a pre-existing Multi Launcher or any process by executable name.
- Use proper readiness/visual-state observation with bounded timeouts; do not use arbitrary long sleeps and hope focus was correct. If there is no interactive desktop, mark real GUI cases `NOT_RUN_ENVIRONMENT`, preserve honest evidence, continue headless tests and implementation where possible, and **never fabricate a passing screenshot**.
- The one-command harness and its README must be useful for a later optional user rerun, but do not ask the user to operate it during execution.

**Minimum key screenshots:** Normal dashboard; dashboard hints; partial matching; dashboard selected; after Enter; normal `note list` grid; grid hints; note selected; after Enter; search field focus; Escape cancellation; second leader cancellation; >26-target dense hints; modal background safety; Settings hints; representative dialog hints/selection; feature disabled. Exact names, structured acceptance and safety rules are in the Goal.

## Bounded tests and anti-spiral policy

- Favor targeted `cargo nextest` filters, relevant Rust unit/integration checks, an appropriate compile/build, `cargo fmt --check` and no new Clippy warnings. Avoid a full suite by default unless the direct change scope makes it necessary.
- Verify each natural checkpoint sufficiently to avoid accumulating broken code, without rebuilding release binaries or rerunning every screenshot suite at every tiny edit.
- If a test fails, diagnose its actual owner and make focused repairs. Allow up to **two deliberate fix/recheck cycles per isolated problem** before documenting a blocker and continuing safe independent milestones. This is not permission to hide failures.
- Capture native GUI test results at integration boundaries and after targeted fixes; avoid unnecessary repetitive rendering/screenshot cycles.
- Do not “solve” focus or input issues with additional global keyboard hooks, uncontrolled synthetic mouse events, arbitrary waiting, hard-coded positions, weakened tests or test-only hacks in default production behavior.
- Do not widen scope into unrelated optimization, settings redesign, other launcher commands, macro playback, Multi Manager or workspace changes.

## Persistent outputs — not optional

Keep these up to date on the new branch:

1. `docs/plans/JUMP_MODE_EXECUTION.md` — branch/base SHA, phase/milestone states, checkpoints/commits, verification, current blocker, next step and push status.
2. `docs/plans/JUMP_MODE_RESULTS.md` — architecture decisions, exact changed-surface inventory, acceptance matrix, tests/commands and exits, screenshot index, performance, regressions, incomplete items, final status.
3. `dev/jump_mode/README.md` and its source-controlled runner/fixtures — one-command reproducible safe GUI test harness.
4. `dev/jump_mode/evidence/<run-id>/...` — locally retained actual GUI screenshots, structured assertions and process cleanup logs, normally ignored by Git to avoid repository bloat.
5. `docs/plans/jump_mode_review/` — small curated screenshots/index on the feature branch for convenient remote review (fixture-only, size-bounded).

At the end, perform a bounded final feature-focused smoke, check the actual diff and Git state, commit coherent final documentation and attempt a final normal best-effort push.

## Required final response from you (Codex)

When the execution is finished or genuinely blocked, report without asking the user to perform anything mid-process:

- **Branch name**, exact starting SHA and final HEAD SHA; whether any commits remain local because pushing failed.
- **What is actually implemented:** dashboard/grid, Settings, dialogs, complex controls, and where the Jump Mode toggle lives.
- **Milestone-by-milestone status:** complete vs incomplete vs blocked, with short root causes where relevant.
- **Evidence:** exact paths and links (as supported by the environment) to `JUMP_MODE_EXECUTION.md`, `JUMP_MODE_RESULTS.md`, latest harness `summary.md`, and screenshot directory / most representative PNGs.
- **Verification:** exact targeted tests, build/Clippy commands and actual pass/fail/skip outcomes; don't call an unrun GUI scenario passed.
- **Known limitations / risks** and one short final manual-review checklist for the user to perform **after** you are done.

**Start now:** Read the Goal and `AGENTS.md`, inspect Git state, create your unique feature branch, initialize the ledger, and execute **M00 → M15 autonomously** without requesting user intervention or waiting for milestone approvals.
