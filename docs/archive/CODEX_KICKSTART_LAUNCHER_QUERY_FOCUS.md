# CODEX KICKSTART — Launcher Query Initial Focus and Notes/Jump Mode Interoperability

**Execute the entire companion Goal:** `CODEX_GOAL_LAUNCHER_QUERY_FOCUS.md`.

Read the Goal **completely** and the current repository's `AGENTS.md`, `.codex/config.toml`, and actual `.codex/agents/{planner,implementer,reviewer}.toml`. Follow their current rules. The checked-out repository is authoritative; the Goal's source file references are a planning map, not permission to assume stale APIs. **No source ZIP or user attachment is required by Codex.**

## 0 — Mandatory FIRST action: create a new branch and commit the baseline

**Before editing application code, assigning any implementation writer, running implementation tests, or starting M1:**

1. Perform read-only `git status -sb`, branch/HEAD/upstream/default-branch and `git remote -v` inspection; preserve existing user changes and any other ongoing task branch.
2. Select an appropriate up-to-date default integration base (`master`/`main`, safely fetched/fast-forwarded only) unless this task's starting branch is explicitly specified. Never reset, stash, rebase, auto-merge, discard changes, or switch away from dirty work destructively; use a separate clean worktree when necessary and safe.
3. Create/switch to a **new, unique** `fix/launcher-query-focus-jump-notes` branch (append a descriptive uniqueness suffix if already taken). Never reuse an existing feature branch simply because it is checked out.
4. Add a minimal branch execution ledger at `docs/plans/launcher_query_focus_execution.md` with original source branch/SHA, task branch and pending M1–M3; commit only Codex-owned bootstrap content as **`chore(plan): [B0] establish launcher query focus baseline`**. An explicit `--allow-empty` baseline commit is acceptable if it avoids an artificial ledger change. Record the post-commit **`BRANCH_BASELINE_SHA`**.
5. Verify the `origin` remote, then make **one normal non-force** upstream push attempt (`git push -u origin HEAD`). If denied/unavailable, record the reason, **keep the successful local commit**, and continue without repeated pushes or approval bypasses.
6. **All** subsequent implementation, focused tests, review, checkpoint commits and push attempts must happen on this new branch, with the B0 SHA as the task baseline. Do not change branches again during the Goal.

**No application feature work before B0 is complete.** Read-only inspection for Git safety is permitted.

## Mission

Fix **two related user-visible focus problems** using the **existing** egui/Jump Mode and `VisibilityRevision` ownership system:

**A. Hidden → visible:** When an interactive launcher hotkey/explicit show action reveals the launcher, the **Main Launcher Query Input**, `egui::Id::new("query_input")` bound to `LauncherApp.query`, normally receives native keyboard focus/caret **once**, regardless of Dashboard, query content, fixed/follow-mouse location, and Jump enabled/disabled. Do not repeatedly refocus it while the launcher stays visible. Preserve modal/independent-viewport/`RootFocusIntent::PreserveForeground` ownership and intentional active note editor focus.

**B. Notes + Jump enabled:** A user with the ordinary nonmodal Quick Notes window (and, where applicable, regular Note Panels) open can deliberately **click or Jump to the visible unobstructed launcher query**, type, navigate/execute a result using a separate new Enter, and later return to editing the same Notes draft. Do not close Notes or discard drafts. Do not allow hint completion to execute or permit general background navigation through Notes/unknown/true modal windows.

**Required technical insight from the inspected source:** Existing `show_root_query_editor` makes the query `.interactive(root_navigation_allowed...)`, which is false whenever Jump Mode is on and Notes owns `NavigationOwner::Notes` or `any_panel_open()`. `query_response.request_focus()` is guarded by that same gate even on `just_became_visible`. The normal launcher Enter path additionally excludes `notes_dialog.open` and nonempty `note_panels`. **Fix the precise split between query interactivity, one-shot focus, Jump target ownership, and deliberate launcher command activation. A cosmetic request_focus alone is insufficient.**

## Configured orchestration

Use real configured role files, not invented sub-agents:

- **`planner`** (GPT-6.1 Sol, High, read-only): make one small implementer-ready packet for the show/focus boundary and one for Notes/Jump owner handoff, verifying current source changes only. Do not produce another large plan.
- **`implementer`** (GPT-6 Luna, Max, sole writer): implement one bounded checkpoint at a time, reporting changed files, focused tests and exact outcomes. No parallel source writers.
- **`reviewer`** (GPT-6.1 Sol, High, read-only): one focused review after the shared focus/owner change and one at final integration if needed. Pay special attention to false root eligibility, stale hints, Enter leakage and Notes draft preservation.
- **Parent orchestrator:** owns B0/Git/ledger, sequential handoffs, checkpoint verification and commits, best-effort push attempts, scope and final report.

Follow **Active Checkpoint Commit Cadence** on the freshly created branch. Commit coherent proven changes as `[M1-A]`, `[M1-B]`, `[M2-A]`, `[M2-B]`, `[M2-C]`, `[M3-A]` where warranted; combine adjacent trivial tests with the corresponding change rather than making meaningless commits. Attempt one non-force push per commit when supported; failed pushes never block safe locally committed work.

## Execute checkpoints in order

1. **M1-A/B — Hidden→visible and dialog guards:** Trace `RootFocusIntent` / `VisibilityRevision` / `reconcile_root_visibility` / first response focus. Restore one-shot show autofocus only for legitimately interactive Root activation. If native ROOT focus arrives later than first visible egui render, use a visibility-revision-bound **single pending handoff**, consumed on actual eligible focus and cancelled on hide/owner change—not continuous refocusing. Protect true modals, active notes and deliberate native `PreserveForeground`. Keep initial query text and user focus changes intact.
2. **M2-A — Real query mouse interactivity:** Allow an unobstructed visible root `query_input` to be deliberately focused while **nonmodal Notes** is open with Jump enabled; do not unlock Dashboard widgets, foreign modal controls, native recorder input or root actions wholesale.
3. **M2-B — Safe Jump query handoff:** Reuse `JumpModeScope`/real TextEdit Response; make one exact query-focus target available from a valid Notes navigation context. Validate owner instance, topmost layer, occlusion and stale target generation. Hint completion focuses only; a fresh key is needed for an action; returning to Notes restores normal Notes owner/hints. Keep unsupported popups blocked.
4. **M2-C — Query-owned Enter/results:** Adjust blanket Notes-open Enter/result guards **only** after proving actual query ownership, so the user can type, select a current result and deliberately run it once without closing Notes. Notes editor Enter and modal/popup keys must never trigger launcher actions.
5. **M3-A — Focused integration:** Use existing actual-egui `run_root_frame_for_test` and Jump owner fixtures to prove show→query, note→query→note, Jump on/off, persisted draft, query history, fresh Enter exactly once, hidden/visible repeated cycles, and modal/independent focus protection. Reuse directly affected tests and update an old expectation only when the newly requested behavior intentionally changes it. Optional one isolated owned native UI smoke (1–2 redacted screenshots), with one diagnostic/retry then `not_run_environment` if GUI control is unavailable.
6. **M3-B — Closeout/Clippy last:** Finish all code, narrowly scoped tests, diffs, touched-source formatting, docs and commits **before** the final maintained-target Clippy. One bounded Clippy pass with `-D warnings` against actual first-party packages, no vendored lint work or blanket allows. If necessary, make at most two focused lint fixes, format/test/commit before the final clean Clippy invocation. Stop after final quality result and report.

## Non-negotiable guards

- **No permanent query focus.** Only one-shot interactive show and explicit user focus requests. Never steal focus back on steady-state UI frames, dialog close or background data refresh.
- **Nonmodal ≠ modal.** Quick Notes ordinary `egui::Window` does not inherently prohibit explicit query focus; true modal confirmation, popup, recorder, native picker or foreign owner must continue to block background input.
- **No global root unlock.** The Notes exception permits only the legitimate visible query and its focused search/activation workflow. Other root controls remain guarded by the existing foreground/owner/clip policy.
- **No hint execution.** A Jump hint focuses/selects only; no implicit click, toggle, Save, Delete, or Enter/Space event crossing the handoff.
- **No Notes changes.** Preserve open window, editor cursor/draft, selected note, shortcuts and existing one-shot note activation handoff.
- **No new hooks, native SendInput, fixed sleeps, settings switches, source persistence changes, unrelated dialog changes, workspaces features or general navigation refactor.**
- Test profiles must keep Paste Sequence Next/Cancel shortcuts disabled (`next_hotkey = None`, `cancel_hotkey = None`) per current `AGENTS.md`.

## Bounded validation

This is a **focused bug fix**. During M1/M2 run usually one direct Nextest slice per checkpoint and, when shared Jump owner code changes, at most one adjacent owner slice. In M3 run at most **two** final composed focused egui/Nextest slices, with a narrow compile check only when needed. **No scheduled full Nextest, release build, screenshot gallery, performance benchmark or broad plugin test suite.** Escalate only for an actual observed cross-boundary regression and report why.

For an unchanged failure, at most two substantive repairs and exact focused reruns before reporting a genuine blocker; do not weaken safety checks or pretend success. Use actual widget input/Responses, not only static predicate tests. No user-operated manual testing or intermediate approval. Native desktop evidence is optional where available; never invent it.

## Final report and stop

Report original source branch/SHA, **new task branch**, B0 baseline SHA, each checkpoint commit and push result, exactly which focus transitions were corrected, actual focused test filters/results, optional native evidence limitations, final maintained-code Clippy command/outcome and any real safety blockers. User performs one manual review after completion.

**START:** Read the Goal and `AGENTS.md` now; **perform B0 first**; then use `planner` and `implementer` for M1, `reviewer` for the focus/owner seam, continue M2/M3 without seeking intermediate confirmation, and stop after the factual final report.
