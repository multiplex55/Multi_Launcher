# Codex Kickstart — Calculation Pad Compact UI and Compound Assignments

Execute `CODEX_GOAL_CALC_PAD_CONFIG_MENU_COMPOUND_ASSIGNMENTS.md` autonomously. This Kickstart and Goal define **one task and one new branch**. Read the entire Goal before touching source. The Goal is the feature contract; `AGENTS.md` and local `.codex` configuration govern the agent workflow.

## 1. FIRST: current-feature-branch HEAD → fresh child branch → M0-A → initial push

**This is an explicit, one-off user override of normal branch-base selection.** The user wants the new feature branch to start **from the commit checked out right now in their existing feature branch**. **Do not go to `master`/`main`, search for an older Goal's base, pull/rebase, or replace the parent with an earlier handoff commit.** This exception is specific to this Goal.

Before code modifications or implementation builds/tests:

1. Read `AGENTS.md`, relevant `.codex` agent instructions, the entire Goal and this Kickstart.
2. In the **single existing checkout**, inspect `git status -sb`, `git branch --show-current`, `git rev-parse HEAD`, upstream/remotes and user modifications. Record `parent_branch` = branch currently checked out; `parent_tip_sha` = exact current `HEAD`. Verify it is the intended current feature branch and an otherwise safe starting state. If dirty, mid-merge or ambiguous, protect user changes and report a precise blocker rather than switching to another parent, stashing/resetting, or silently including them.
3. Run `git switch -c feat/calc-pad-config-menu-compound-assignments <parent_tip_sha>` (choose a unique lowercase suffix if the name already exists). Verify new branch `HEAD == parent_tip_sha` before baseline.
4. Create the independent `M0-A` baseline commit, e.g. `git commit --allow-empty -m "chore(calc-pad): [M0-A] establish compact UI branch"`, or a small clean execution ledger. Verify baseline's parent is `parent_tip_sha`. Record `baseline_sha` separately.
5. Attempt **one** ordinary `git push -u origin HEAD` when available. Log a failure and continue local work without bypasses or retry loops. All later commits, tests and pushes belong to this new child branch. Record final `final_tip_sha` for the next Goal.

**NO GIT WORKTREES:** No `git worktree add/move/prune`, alternate clones/checkouts, agent branches, parallel competing writes or unapproved rebase/merge/reset/cherry-pick. Do not push unrelated branches. Never stage unrelated user files.

## 2. Delegate and implement according to the Goal

Use the configured planner, implementer, reviewer and orchestrator responsibilities where available. In this repository, check the installed `.codex/agents/` configuration rather than inventing agent names or model settings. Give the **implementer agent** explicit, one-checkpoint-at-a-time scopes. Only one writer/build-test lane in the existing checkout. An independent reviewer inspects important feature changes before closeout.

Follow the Goal's ordered checkpoints, on this same branch:

- **M0-B:** Read the current source, inspect relevant tests, establish a small verification baseline.
- **M1-A:** Build one top-level `Config Menu` covering existing Calc Pad document actions, View, Number format, Source/Editor options, Remember last source, Tools, Dock/Pop Out and Results **Copy all**. Keep all context-local confirmation and input controls functional.
- **M1-B:** Remove inline per-row Copy/Copy formatted buttons; right-click order exactly **Go to source, Select for comparison, Copy, Copy formatted**. Preserve value/error gating, accepted copy intents, compare/source navigation and typed Inspect bits access.
- **M2-A:** Implement `+=`, `-=`, `*=`, `/=`, `%=`, `^=` through the existing assignment parser, type-specific arithmetic and prior-binding semantics. `%= ` is ordinary remainder, `^=` exponentiation. No implicit initializer, no unrequested bitwise compounds.
- **M2-B:** Integrate compound assignments with bounded `for` loops, error rollback, editor assistance, literal-input exclusion, final bindings, dependency/compare/Inspector/watch state, Undo/Redo, persistence and README/examples.
- **M3-A:** Run bounded directly owned verification, independent review, focused Windows smoke and final ledger/handoff. Fix feature-related defects only.

Commit each verified checkpoint with the Goal's `[M#-X]` labels and attempt one non-force push. Do not stop to ask approval between routine milestones, and **do not launch a new Goal** afterward.

## 3. Preserve existing features

- Use existing `DialogIntent`, utility host and retained `HostEffects` for Config Menu actions. No new persistent configuration schema or duplicate menu owners. The document title, dirty indicator, Source/Results panes, input Apply/Reset and Save/Discard/Keep Editing remain usable.
- Use existing result `ResultCopyIntent` and accepted-revision guarantees. `Copy` is canonical, `Copy formatted` uses display preferences, `Copy all` remains source-order faithful. Right-clicks on value text/label/background work. Error rows retain Go to source and disable value-only actions.
- Compound statements operate on the latest **preceding** binding; earlier row values never change. Keep line numbers, source bytes, CRLF, braced Unicode names, reserved names, `==`/`!=`/`<=`/`>=` parsing, `%`/`mod` distinction, `^` as power, exact integer width, quantity/vector safety, loop rollback/work limits and comparison's read-only expression surface.
- Keep global hotkeys, launcher query focus, detached windows, clipboard effects, source Undo/Redo, named document/recovery and background worker behavior intact. Do not broaden to other utilities.

## 4. Focused verification with explicit evidence

Run selected tests, not the full suite. For each batch record exact command/selector, exit code, test counts, relevant logs and cleanup receipt; use the project's bounded execution/watchdog convention (100-second Nextest execution batches where supported). If a test is flaky in a group, rerun the specific failing test individually without weakening its assertions.

**Minimal user-visible acceptance fixtures:**

1. **Config Menu:** Open docked Pad, show the sole `Config Menu`; activate New/Open/Save/Save As, Examples, Dependencies, Inspector, View, Number format, Source/Editor display controls, Remember last source, Copy all and Pop Out/Dock as applicable. Verify preservation of source, dirty prompts, settings, focus and copy outcomes; repeat relevant actions in popped-out mode. At a compact window width, every menu action remains reachable.
2. **Result actions:** Evaluate `a = 5\na * 4\n1 / 0`. Right-click `r2` row and confirm exact menu order, Copy returns `20`, formatted Copy respects the chosen display mode, Select for comparison targets `r2`, Go to source highlights physical line 2. Right-click `r3` error: Go to source works; value-only actions are disabled. Confirm no row Copy/Copy formatted buttons remain. Test a vector and an exact integer, including Inspect bits.
3. **Arithmetic compounds:** Evaluate `x = 2\nx += 3\nx *= 4\nx /= 2\nx %= 6\nx ^= 2\nx`; assert physical results `2, 5, 20, 10, 4, 16, 16`. Test `x = -7; x %= 3` gives `-1` (as physical lines), `missing += 1` diagnoses unknown name, and division by zero/unsupported typed operations error without corruption.
4. **Loop and source stability:** Evaluate `total = 0\nfor n in range(1, 4):\n    total += n\ntotal`; final result `6`. Undo/Redo and save/reopen; verify literal input selection never treats `total += n` as an editable constant. Include a braced Unicode name, an exact integer and a CRLF source fixture.
5. **Stale host and state:** In focused tests verify copy/navigation are rejected for retired host or outdated line; document comparison remains read-only and resolves final binding; repeated loop work still honors cancellation/bounds.

For Windows GUI attempts, record **before/after screenshots**, input sequence, expected/observed state, executable/source SHA and evidence paths. If native capture is unavailable or cannot reliably reach a control, report **UNVERIFIED**, not PASS, without creating a new testing framework or repeating unrelated prior native campaigns.

Only perform the compilation, formatting/lint checks necessary for the touched files; do not run full Nextest, release builds, or unrelated Clippy cleanup. Preserve historical limitations as non-gates unless a touched feature demonstrably regresses.

## 5. Final report and stop boundary

Before finishing, have the reviewer inspect the cumulative diff for scope, type semantics, exact menu order, UI focus/stale effects and regression coverage. Resolve actual feature findings with small commits. Update the **one** active execution ledger and give a concise final report containing:

- Finished checkpoint names and commit SHAs, visible changes, files modified and significant implementation choices.
- Selected tests: command, PASS/FAIL counts, exit codes and evidence locations.
- Native UI matrix: PASS/FAIL/UNVERIFIED and screenshot/receipt paths; issues distinguished from automation limits.
- Git: original checked-out `parent_branch`, its **captured current-HEAD** `parent_tip_sha`, new branch, `baseline_sha`, all checkpoint commits, final `final_tip_sha`, upstream push state and clean/dirty status.
- Final explicit handoff: **any next Goal branches from this Goal's `final_tip_sha`**, not `master`. No worktrees or intermediate merges were used.

**Stop once this Goal and bounded verification are complete. Do not start another category or optional polish.**
