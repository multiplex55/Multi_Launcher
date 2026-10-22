# CODEX KICKSTART — Capture Stability & Productivity Expansion

**Execute the approved, detailed goal document:**

**`CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md`**

Read this exact file **completely** and the current checked-out repository's **`AGENTS.md`** before source edits. Also read `.codex/config.toml` and the existing `.codex/agents/{planner,implementer,reviewer}.toml` role definitions. **Use the real configured runners.** The parent Codex process is the orchestrator. The Goal's source map is a vetted aid, not a substitute for checking the current repository.

## B0 — FIRST ACTION: Create, baseline-commit and push a new feature branch

**This is mandatory for this Goal, not optional. Perform it BEFORE M0, before any source/test changes, and before assigning work to `implementer`.** Read-only inspection of `AGENTS.md`, the Goal, Git branch/HEAD, remote and worktree is allowed first.

1. Inspect `git status -sb`, current branch/HEAD, staged/untracked changes and `git remote -v`. Preserve all unrelated user work. Source base is normally current HEAD; if on clean `master`/`main`, attempt `git fetch origin` and safe fast-forward-only synchronization before branching. Never reset/rebase/merge to force this. If dirty, leave user edits untouched and prefer an **isolated clean Git worktree from current HEAD** for the new branch; if unsafe, report the exact blocker instead of committing/stashing user changes.
2. **Create and checkout a brand-new, uniquely named branch**: prefer **`feat/capture-productivity-expansion`**. If already taken locally or remotely, append a descriptive date/counter (e.g. `feat/capture-productivity-expansion-20261020-2`). **Never reuse an existing feature branch**. Branch creation is the first task-specific Git change after any safe fast-forward-only default-branch synchronization.
3. Initialize `docs/plans/capture_productivity_expansion_execution.md` with the original source branch/SHA, new branch, B0 metadata and pending M0–M11 checkpoints. Stage **only** this new Codex-owned ledger, then make the **first branch commit**: `chore(plan): [B0] establish capture productivity branch baseline`. If no distinct safe tracked ledger change is possible, the user's explicit request authorizes **one** labeled `git commit --allow-empty` for B0 alone; ensure user-staged files cannot enter that commit.
4. Verify `origin` is the intended repository and **immediately attempt one** `git push -u origin HEAD` to publish the newly committed branch and establish its upstream. If an automatic security reviewer blocks pushing, **keep the local B0 commit, record the result and continue**; do not bypass approval or loop on push retries.
5. Obtain the **new B0 commit SHA** (`git rev-parse HEAD`) and treat it as **`BRANCH_BASELINE_SHA`**. M0 records it in the ledger. **All source edits, tests, agent implementation assignments and subsequent commits M0–M11 happen only on this new branch from that baseline.** Do not return to `master`, rebase, reset or replace the source baseline mid-plan.

**B0 done means:** A new task branch exists, its initial baseline commit is present, first push result is recorded, and original user work is untouched. **Do not start implementation until B0 is complete locally.** The mandatory B0 initialization commit is an intentional exception to the usual prohibition on artificial checkpoint commits.

## Mission and order — implement all ten requested changes

**Two screenshot problems have absolute first priority.** Do not start a new productivity plugin while either P0 issue is still under active implementation. Once the bounded fix/evidence for P0 is recorded, continue autonomously through every approved feature area.

1. **M0 — Inspect from the new B0 baseline:** Confirm the branch/first commit SHA and recorded push result, then inspect relevant owners and AGENTS contracts; continue the baseline ledger. No discovery/benchmark marathon.
2. **M1 — P0 Screenshot Pin shield failure:** Fix the real recurring error `screen selection failed: Rectangle input shield monitor 1: topmost style missing` after repeated **normal/static and live** pinboard captures. Keep `WS_EX_TOPMOST`/layered/input-hit-test verification, restore valid shield coverage and cleanly release all old native shield/selection resources so another capture works without app restart. **Never suppress safety checks to hide the error.**
3. **M2 — P0 Screenshot Editor layout:** Put **Zoom and Text Size sliders side by side**, on one compact row **directly above** Pen/Arrow/Rect/Highlight/Text tools, responsive at narrow dialog widths. No editor behavior change.
4. **M3 — Search all Settings:** Add search and direct navigation/focus to existing main and plugin-owned settings controls, with true Settings Save/Cancel, not a second settings store.
5. **M4 — Consolidated Hotkey Directory:** Searchable owner/chord/action table of configured and **acknowledged effective** bindings, conflicts, status, and a direct Edit navigation action. No second registration manager.
6. **M5 — Searchable Paste Sequence source pickers:** Search snippet aliases/tags/visible text and current in-memory clipboard history text/metadata; show selected previews and Insert. **Never expose hidden snippet contents**, persist clipboard search text, or change existing sequence playback.
7. **M6 — Per-pin opacity:** A user-settable **10–100%** visual opacity on each independent **static or live** pin, default 100%, through pin toolbar/Pin List. It must actually show the underlying application, not just fade image pixels onto an opaque window. Keep Always on Top, Click-Through, Copy/Save pixels and lifetime safe.
8. **M7 — Richer MkMacro parameter forms:** File picker, folder picker and authored Choice list on appropriate existing String parameters through the signature editor and typed pre-run prompt. Keep old macro JSON, CallMacro bindings, type checks and Cancel intact.
9. **M8 — Explicit MkMacro clipboard actions:** **Read Clipboard Text → Variable** and **Write Text/Variable → Clipboard** as separate authorable/executable actions. Text-only, explicit, controlled errors, no implicit paste or restoration.
10. **M9 — CSV/TSV utility:** `table` plugin for pasted/opened UTF-8 CSV/TSV, quoted fields, bounded responsive rows, filter/sort, selected column/row copy, JSON conversion and explicit Copy/Save As. **Read-only original source.**
11. **M10 — Named individual monitor brightness:** Extend existing `bright` dialog using checked Windows physical monitor APIs; enumerate, display capability/current reading and set each supported display independently. Keep legacy command. Unsupported monitor = clear unavailable status, not made-up current value.
12. **M11 — Integration/closeout:** Focused shared-boundary checks, **one full Nextest near the end**, formatting/docs/ledger/commits, then **M11-D Clippy last**.

All listed feature areas must be addressed. A real safety/hardware blocker may prevent an individual subcomponent; document actual evidence and continue only independent work. Do not silently drop features or mark a source-only stub done.

## Mandatory orchestration and checkpoint protocol

- **`planner` — GPT-6.1 Sol / High, read-only:** Inspect only needed current owners. Produce one implementer-ready packet per significant milestone: goal, paths, expected behavior, invariants, tests, commit boundary and blockers. Do not create a second massive implementation plan or repeatedly re-plan accepted behavior.
- **`implementer` — GPT-6 Luna / Max, sole source writer:** Receive explicit **one checkpoint** at a time. Implement real code and focused tests, preserve existing integration, report exactly changed files, errors and verified outcomes. Do not start next checkpoint by yourself. Use one writer; never have parallel agents editing shared files.
- **`reviewer` — GPT-6.1 Sol / High, read-only:** Independent focused review of the actual diff at meaningful boundaries (especially native shield ownership M1, pin opacity M6, macro runtime M7–M8, device brightness M10). Return concrete P0/P1/P2 defects. The same implementer repairs only substantive findings, then move on. No repeated unbounded reviewer/implementer cycle.
- **Parent orchestrator:** Owns and **executes B0 before any implementer handoff** (new branch, first baseline commit, first push attempt and baseline SHA); then owns sequential checkpoint packets, ledger, verified commits and best-effort pushes **exclusively on that branch**. Honor **Active Checkpoint Commit Cadence** with labels `[M1-A]`, `[M1-B]`, etc. Commit locally first, inspect diff, attempt one normal non-force push when authorized; if denied, continue locally and report. Never reset, rebase, force-push, return to the source branch or rewrite user history.

### Code first, no follow-up questionnaire

The Goal supplies safe default product decisions and actual source entry points. Make ordinary engineering decisions without stopping to ask the user. The **two screenshot priorities first**, then Settings/Hotkeys/Paste/Pinboard/MkMacro/Table/Brightness, then integration. Reuse existing egui theme, Jump Mode focus ownership, Windows platform wrappers, plugin commands, background workers and test fixtures. Don't create new generic frameworks or unrelated utility features.

### Regression and privacy boundaries

- **Capture:** `WS_EX_TOPMOST` missing is a real defect. Re-establish and verify genuine hit-testable topmost shields before accepting any mouse-selection Down/drag/Up. Cleanup must permit repeated static/live capture, including an error followed by a new attempt. Keep screenshot capture/editor/MkMacro selection safe.
- **Settings/Hotkeys:** Navigation alone never applies a setting; a configured chord is not claimed Active without runtime ack. Don't register new hooks, reinterpret existing shortcuts or alter main/radial/gesture hotkeys.
- **Clipboard/Snippets:** No hidden snippet text in previews/search/logs; current clipboard text search is ephemeral to the open picker. MkMacro clipboard actions change data only at explicit execution and never log that text.
- **File writes:** Table source never changes; JSON Save As requires explicit path and overwrite confirmation. Native tests use disposable fixture directories only.
- **Pins:** Opacity is per-pin native composition with independent click-through/topmost, and never alters saved pixel buffers or the parent launcher.
- **Monitors:** Never alter real user-display brightness during automated tests; use fake monitor adapter and capability/handle-lifetime tests. Actual unsupported device remains `unsupported` without endless DDC retries.
- **Existing features:** Existing Paste Sequence, Screen Draw, Screenshot Editor editing, MkMacro recorder/calls, launcher dashboard, `fs`, `cs`, JSON/XML/Regex, and plugin search remain working. No application-workspaces features.

## Focused verification, final gates, and anti-spiral limits

- During M0–M10: **one or at most two focused Nextest slices per checkpoint**, and only directly necessary Cargo check/build/format/diff inspection. **Do not run full Nextest or Clippy during implementation.** Do not start a performance optimization track.
- Existing egui/reducer/native adapters should be the test bed. For graphical changes, allow **at most one owned native smoke session per relevant milestone** with 1–3 harmless screenshots; the M1 session may legitimately repeat several static/live captures. Never use personal files/clipboard or modify actual monitor brightness.
- If desktop automation can't access owned windows, use one diagnostic and one retry, then mark `not_run_environment` and proceed with deterministic tests. Do not ask the user to intervene or endlessly attempt computer-control approvals.
- **Identical failure budget:** at most two substantive fixes plus a focused rerun after each; preserve the actual error if still blocked. Never remove essential tests or disable input safety to report green.
- **M11:** one complete `cargo nextest run --workspace` near the end (at most one justified repaired full rerun). Finish all docs, screenshots, formatting, checkpoint commits and execution ledger. Only **then** run maintained-code Clippy with `-D warnings`, excluding vendored crates. Fix bounded first-party warnings, commit fixes, then the final clean Clippy invocation must be **the last automated verification**. Report truthful blockers; no extra coding after the last successful gate.

## Required final output

Give the **original source branch/SHA, newly created task branch, B0 baseline commit SHA and first push result**, then a precise status for **P0 shield**, **P0 slider row**, and each of the **eight features**, with committed checkpoint SHA(s), actual behavior, named focused test results, full Nextest and final Clippy outcomes, real native screenshot evidence if obtained, hardware limitations and final branch/push status. Identify any blocked subfeature separately instead of describing it as complete.

**Stop immediately after the final handoff.** Do not ask the user to run or review anything during execution. The user will perform a single manual review after Codex finishes.

**START:** Read `CODEX_GOAL_CAPTURE_AND_PRODUCTIVITY_EXPANSION.md` and `AGENTS.md`, then **complete B0 new-branch creation → baseline ledger/commit → initial push attempt → pin `BRANCH_BASELINE_SHA`** as the first Git changes. **Only afterward** invoke the configured `planner` for M0/M1, hand M1-A to the `implementer`, and continue all milestones/commits on that new branch through M11.
