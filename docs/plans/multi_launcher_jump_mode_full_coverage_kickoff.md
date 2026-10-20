# Codex Orchestrator Kickoff — Jump Mode Expanded Coverage (Priorities 1–8)

Implement the **entire approved goal** defined in:

**`multi_launcher_jump_mode_full_coverage_plan.md`**

Read that exact file **in full**, plus the **current checkout's `AGENTS.md`** and the role definitions in **`.codex/agents/{planner,implementer,reviewer}.toml`**, before delegating any implementation. Treat the current repository checkout as authoritative; the plan's October 20 `master` SHA is a source-inspection reference, **not** authorization to reset or work against stale source. Historical source archives and earlier Jump Mode plans are **not prerequisites**.

## Mission

Extend the existing **Shift+Space Jump Mode** to **all eight specified plugin areas in one long autonomous effort**, reusing `JumpModeUi`, `JumpModeScope`, typed target identities, exact foreground/dialog ownership, native focus adapters and frame-scoped hint painting. This is **coverage expansion**, not a replacement Jump Mode implementation, hotkey redesign, generic UI automation engine, or general launcher rewrite.

The current Jump Mode implementation already works and has 109-test focused acceptance evidence. **Preserve it.** The work is to reach additional visible controls in currently unsupported or partially supported panels without accidentally activating them or taking keyboard focus away from the topmost legitimate owner.

## Mandatory eight areas — implement every group

1. **File Search (`fs`):** query/root/refinement fields, Filename/Content controls, Search/Cancel, currently visible results, Open/Copy.
2. **Snippets (`cs`):** saved rows, filter/alias/tags/body where reveal is allowed, Save/Cancel/Preview and scoped confirmation.
3. **Clipboard History:** opaque history rows, edit/copy/remove and editor confirmation, with no plaintext hint leaks.
4. **Dashboard gaps:** Pinned Commands, Command History, and missing simple Todo/Scratchpad/Calendar controls.
5. **JSON, Regex and XML:** source/pattern/search fields, tabs/views, test/result/tree rows, Previous/Next and enabled Copy/Save/Close actions.
6. **Mouse Settings and Mouse Gestures:** checkboxes, sliders, selectors, management rows and binding editors, preserving native capture/recording exclusivity.
7. **Batch Rename and Search & Replace:** edit fields/rules, filters, visible file/match rows, previews, selections and safely owned Apply/Undo/Restore confirmation controls.
8. **Radial Designer and Dashboard Editor:** existing menus/lists/rings/cell/slot selection, property forms, nested editors, Save/Discard/Cancel. Do not synthesize pointer drags on canvases.

**Do not finish after the first 3–4 groups.** Complete M0 through M11, including all eight groups, within the plan's bounded safety/testing policy. For a purely drag-only, non-actionable or unsupported popup control, implement the safe neighboring focusable controls and record the precise individual limitation. Do not claim universal coverage.

## Agent orchestration — use the actual configured runners

The parent/orchestrator owns branch/worktree/commits, status ledger, milestone progression and final handoff. Invoke the **named configured sub-agents** from the repo, not an invented generic agent role:

- **`planner`** — `.codex/agents/planner.toml`, GPT-6.1 Sol, High reasoning, read-only. Give each milestone grouping one bounded source/architecture check and an implementer-ready checkpoint packet. Use the already-detailed plan; do not commission a second giant plan or a new requirements questionnaire.
- **`implementer`** — `.codex/agents/implementer.toml`, GPT-6 Luna, Max reasoning, workspace-write. **Sole source writer.** Give it exactly **one checkpoint at a time** with current HEAD, objective, allowed files, steps, invariants, minimal tests and done criteria. Do not let it begin the next checkpoint until the parent handles review/commit and reassigns work.
- **`reviewer`** — `.codex/agents/reviewer.toml`, GPT-6.1 Sol, High reasoning, read-only. Review meaningful finished ownership/routing changes, dialog interactions and file-mutation confirmation boundaries. Return **concrete** issues by severity; the same implementer fixes only substantive findings. Do not run repetitive independent reviewer loops over unchanged source.

Do not run parallel writers or assign overlapping worktrees. Parent may use read-only reviewers/planners concurrently only for independent source questions. Reuse one reviewer pass for related checkpoints where practical: core M1; dialogs M2–M4; Dashboard/tools/mouse M5–M7; sensitive file/designer M8–M9; final integration M10–M11.

## Required workflow and commit cadence

1. **Read, inspect, then implement.** Check `git status -sb`, current branch/HEAD and `AGENTS.md`, validate owner types in `src/gui/jump_mode/ui.rs`, existing supported dialog examples and direct tests. Do not spend the first half of the task on exploratory redesign.
2. **Safe branch discipline.** If on clean `master`, create a fresh task branch `feat/jump-mode-expanded-coverage` (or unique suffix) from that current state. If a task branch is already selected, continue there. Never overwrite user work, switch away from a dirty worktree, reset, rebase, force-push or import old ZIP source.
3. **Use the plan's checkpoints in order**, M0-A to M11-C. Keep a small durable execution ledger `docs/plans/jump_mode_expanded_execution.md`. Make one meaningful **small checkpoint commit** per independent unit of source work (`[M1-A]`, `[M2-B]`, etc.) after focused verification and diff review. Parent tries one normal best-effort `git push` per commit; unsuccessful pushes must never halt implementation.
4. **Reuse the current Jump implementation.** Centralize new window/owner kinds in `NavigationOwner`/`NavigationWindows`; use the existing scope/registry/focus APIs. Every new dialog must be included in actual topmost visible egui layer ownership before its child targets are considered eligible. Do not create a second keyboard hook, second hint engine, persistent per-dialog shortcuts or special hotkey mode.
5. **Focus before activation.** Hint completion never immediately activates. A later distinct Enter/Space/arrow/typing action uses the existing native focused widget. Preserve same-frame key discard and Escape priority; background, inactive, hidden, offscreen, stale and ambiguous targets must fail closed.
6. **Preserve safety/privacy.** Don't place clipboard/plaintext hidden snippet contents, file bodies or edited user data into hint labels or logs. File Apply/Restore/Delete and other dangerous actions remain governed by existing explicit confirmations; never allow an old hint/Enter to spill into a newly opened confirmation. All fixtures use disposable isolated data only.
7. **Keep the scope broad but not runaway.** Complete all eight requested groups. Do not add entirely unrelated tools, an animation engine, performance roadmap, dashboard redesign, UI scripting language or support for screen-picker/recording/mouse-drag surfaces that lack ordinary selectable responses.

## Precise verification budget

- During implementation, run **one focused Nextest slice per checkpoint**, and at most one second adjacent slice where shared owner code actually changed. Never run full Nextest/Clippy during M0–M9. Fix concrete compiler/assertion errors locally; do not dilute tests or cycle through test-harness rewrites.
- Use **existing egui UI tests and `dev/jump_mode/run.ps1`**. Build new regression fixtures only when an identified behavior isn't already proved; no separate native automation framework, generalized macro driver or new screenshot backend. Reuse owned process/safe profile isolation.
- At most **12 native GUI sessions / 24 small redacted images for the whole project** (normally one per priority area; Batch Rename/Search Replace and both designers may each need one). Use only safely owned fixtures and nonmutating UI states. If desktop access is unavailable, **one diagnostic + one retry** then record `not_run_environment` and continue with source/egui evidence—never ask the user to operate the app mid-task.
- For a repeated identical failure, allow **at most two substantive fixes and focused reruns**, then report an accurately scoped blocker and proceed with independent work. Input safety and file-write confirmation defects are blockers, not optional verification details.
- In M10 perform at most two cross-dialog focused regression selections and reuse existing native evidence. Finish ordinary code, docs, scoped formatting and fixed verification obligations before final gates.
- In M11 perform **one full `cargo nextest run --workspace --no-fail-fast`**, with **at most one full repaired rerun** if truly needed. Keep test profiles' Paste Sequence Next/Cancel hotkeys disabled per `AGENTS.md` without changing production defaults.
- **Clippy is the last development verification phase (M11-C)**; use maintained workspace targets and document existing baseline Pinboard `never_loop` issues or unrelated baseline formatting rather than modifying unrelated modules or vendored source. Finish all normal tests/format/docs/commits before the last successful lint invocation. Do not claim a failing command passed.

## Key acceptance outcomes the reviewer must check

- **Only current foreground/visible window** gets hints; switching File Search ↔ Snippets ↔ Settings ↔ designers never leaves old hints active.
- **Stable IDs:** filtering/reordering/duplicate labels/history insertions/slot rearrangement cannot redirect a completion to another row or another file.
- **Key safety:** Shift+Space → hint focuses; only later new Enter/Space performs the action once. Escape/cancel and off-state remain normal.
- **Privacy:** Clipboard/hidden snippet/file payloads absent from badges, IDs, logs and QA screenshots.
- **File safety:** selecting a result/checkbox never performs an Apply/Undo/Restore; child Confirm/Cancel dialogs own their own foreground focus and require separate user activation.
- **Designers:** existing navigable lists/forms/slots are covered; purely drag-only canvases remain unautomated without inventing a new pointer mechanism.
- **No regression:** earlier root, list/grid, dashboard, Notes/Todo/Timer/Paste/MkMacro Jump behavior and ordinary keyboard/mouse input still work.

## Final response and stop

When M11 finishes, report the actual work branch/HEAD and commit hashes, implemented controls **for every one of the eight groups**, meaningful excluded subcontrols, focused tests, one full Nextest result, final Clippy result with any unchanged baseline findings, native screenshots/receipts where actually obtained and exact environmental limitations. Clearly identify any truly blocked checkpoint; do not label it complete merely because it compiles.

Do not request interim user tests, screenshots, answers or approval. The user will review the completed work once. **Do not invent Phase 3 or another implementation plan. Stop after the final handoff.**

**Begin now:** Read `multi_launcher_jump_mode_full_coverage_plan.md` and `AGENTS.md`, verify the current checkout, invoke `planner` for M1's bounded owner-routing packet, assign M1-A to the configured `implementer`, and proceed sequentially through every priority.
