# CODEX GOAL — Multi Launcher Universal Jump Mode

**Status:** Approved feature specification; ready for autonomous implementation  
**Plan type:** Single continuous, multi-phase implementation program  
**Repository:** `multiplex55/Multi_Launcher`  
**Branch ownership:** The Codex orchestrator creates and owns a **new feature branch at kickoff**  
**User involvement:** One optional final manual review only; never request user intervention between milestones  
**Target platform:** Windows desktop; Rust/egui/eframe application  
**Feature name:** *Jump Mode* (working name; do not rename other subsystems)

> **Mission:** Implement a feature-gated, application-native, Hop.nvim/Flash.nvim-inspired keyboard targeting system that lets the user press **Shift+Space** while Multi Launcher is focused, see short hints next to visible actionable elements, type a unique hint, and move keyboard focus or logical selection directly to that element. **Typing a hint never executes an action.** After a successful jump, the overlay closes and normal input resumes; Enter or the native control key then activates the existing action. Extend this architecture from the dashboard and action grids to settings and representative major dialogs. Build a reproducible, autonomous, real-window visual test harness that starts Multi Launcher, enters keyboard sequences, captures screenshots and evidence, then closes its own process. Implement, validate, checkpoint-commit, and best-effort push each stage without asking the user to do anything.

---

# 0. Source authority and plan interpretation

1. **The actual checked-out source at Codex kickoff is authoritative.** Read repository `AGENTS.md`, the current Git branch/HEAD, settings types, app lifecycle, input paths, grid/dashboard rendering, action dispatch, and relevant tests before choosing code boundaries. Do not use or reference uploaded ZIP files; they are not part of Codex's working inputs.
2. The following repository paths were observed on GitHub `master` during plan preparation, **not guaranteed to be identical at execution time**:
   - `src/gui/mod.rs` — main launcher GUI and multiple rendering/navigation flows.
   - `src/gui/search.rs`, `src/gui/actions.rs`, `src/gui/numpad_navigation.rs` — relevant search, activation, and keyboard navigation seams.
   - `src/dashboard/dashboard.rs` and `src/dashboard/widgets/` — dashboard runtime and widget interface.
   - `src/settings/model.rs`, `src/settings/defaults.rs`, `src/settings_editor/render.rs` — persisted settings and settings UI.
   - `src/main.rs` — application startup and native window ownership.
   - `src/gui/qa_framebuffer.rs` — existing feature-gated QA framebuffer capture implementation (`qa-framebuffer-capture`).
   - `src/bin/radial_acceptance/`, `src/bin/passive_overlay_smoke.rs`, `dev/run_owned_process.ps1`, `dev/test_owned_process.ps1` — existing acceptance/smoke and Windows process-containment patterns to **inspect before extending or reusing**.
   - `tests/gui_visibility.rs`, `tests/focus_visibility.rs`, `tests/hotkey_events.rs`, `tests/settings_editor.rs`, `tests/notes_plugin.rs` — plausible relevant focused regression tests, **not an instruction to run every listed test indiscriminately**.
   - `.codex/agents/planner.toml`, `implementer.toml`, `reviewer.toml`, and `.codex/config.toml` — existing orchestration roles/configuration; use if available, do not rewrite them.
3. The user interface screenshots supplied during requirements solicitation are conceptual references for dark-green theme, dashboard columns, `note list` action cards, and hint-overlay placement. They are **not pixel-perfect golden images**. No exact colors, positions, or fictional commands in mockups are normative.
4. This document takes precedence over historical plans for **this feature only**, while repository `AGENTS.md` remains the governing engineering and Git-workflow policy. If there is an actual contradiction, preserve safety and this plan's product contract, document the technical resolution, and do not ask the user about routine engineering decisions.
5. The plan is long by design: **continue automatically from one approved milestone to the next**. Avoid artificial pauses, extra planning cycles, or recurring “please test this” messages. The orchestrator should maintain a concise persistent execution ledger so a long session can resume safely.

# 1. Start goal, end goal, acceptance and exclusions

## 1.1 Starting state / entry criteria

- Existing Multi Launcher behavior is preserved; Jump Mode does not yet need to exist in the actual checkout.
- The initial branch and commit are identified and recorded; user work is protected.
- The current app can be built using the repository's documented commands, or an environment blocker is recorded without destroying existing state.
- Potential reuse seams, input ownership, current GUI focus and logical selection mechanics, and native process/screenshot tooling have been inspected enough to permit scoped implementation.
- A baseline screenshot/evidence plan exists for at least the initial dashboard and grid views. Do not spend hours building an unrelated general test framework before implementing the feature.

## 1.2 Required final end state — all mandatory phases

1. Jump Mode is disabled by default in persisted settings and does not alter existing behavior when off.
2. Pressing `Shift+Space` while the Multi Launcher window is foreground activates Jump Mode even if a normal text field currently owns focus, without typing a space or altering that field's existing text.
3. A high-contrast, compact hint is displayed adjacent to each **eligible visible actionable** element in supported UI surfaces.
4. Hint assignment is deterministic for the same ordered target population, uses `A`–`Z`, accepts keyboard letters case-insensitively, prefers short strings, is unique and **prefix-free** so completion cannot be ambiguous, and stays attached to the same logical target during an active session.
5. Typing a complete hint focuses or selects that control, closes Jump Mode, and shows unambiguous selection/focus feedback **without executing the underlying action**.
6. Native keyboard activation works afterward: Enter on buttons/action cards/notes, Space on checkboxes as appropriate, ordinary typing on text fields, arrow adjustments on sliders where supported, and correct popup interaction. Existing action handlers remain authoritative.
7. Escape or repeated `Shift+Space` cancels; Backspace edits partial hints; invalid input cannot trigger unintended actions. Losing application focus safely cancels. Input does not remain stuck or swallow normal keys after exit.
8. The dashboard and visible action grids are covered first; then settings and agreed representative dialogs/complex controls are integrated with consistent modal ownership. Inaccessible background controls never receive hints or execute behind a dialog.
9. No offscreen/hidden targets are presented; no system-wide UI Automation, external-app targeting, mouse-coordinate synthesis, or mandatory global hook is introduced for Jump Mode.
10. A **Codex-owned Windows GUI acceptance harness** exercises the real application in a safe, isolated fixture, makes screenshots and machine-readable evidence, closes only its owned process(es), cleans temporary state, and produces a reviewable evidence index. Automation cannot silently claim success based only on a static screenshot or mock GUI.
11. Targeted Rust tests, relevant GUI acceptance cases, clean formatting and Clippy for changed code, code review, checkpoint commits, best-effort push and final implementation report are complete. No unrelated feature changes or broad architectural cleanup.

## 1.3 Required completion standard

**Mandatory Phase 1** (core engine + dashboard + grids + native harness) and **Mandatory Phase 2** (settings, modal awareness, prioritized dialogs and common controls) must be implemented and verified, or any unachievable item must be clearly marked **blocked/incomplete** with evidence. Do not relabel a blocked requirement as “passed” or silently delete it. Complete independent milestones after a localized blocker when safe. Do not drift into optional enhancements simply because they are interesting.

**Not in scope:** system-wide navigation; arbitrary click coordinates; other desktop applications; offscreen navigation/auto-scroll; mnemonic/fuzzy name search; frequently-used hint optimization; direct execution on hint completion; theme editor redesign; radial-menu rewrite; rearchitecting every plugin; unrelated optimization work; any Multi Manager workspace feature; configuring every possible key combination in this run.

# 2. Locked product decisions (do not re-solicit)

| ID | Decision | Binding requirement |
|---|---|---|
| P01 | Initial leader | `Shift+Space` while Multi Launcher is focused. |
| P02 | Interaction | Completing the hint **only focuses/selects**. An additional Enter or native key activates. |
| P03 | Hints | Automatically assigned, shortest **practical** unique, prefix-free sequences. |
| P04 | Target population | Eligible **visible actionable** controls only. |
| P05 | Integration order | Dashboard and action grids first; settings and dialogs next. |
| P06 | Existing text | Leader works in focused text fields, preserving typed text. |
| P07 | Lifecycle | Immediately exit Jump Mode after successful target resolution. |
| P08 | Dense UI | Use longer unique hints when needed; no hierarchical mode required. |
| P09 | Feature gating | Explicit Experimental Jump Mode setting; disabled by default. |
| P10 | Alphabet | Letters A–Z; input case-insensitive, displayed uppercase. |
| P11 | Leader configurability | Initial fixed `Shift+Space`; preserve a future settings extension path; do not implement a new global hotkey manager. |
| P12 | Initial dashboard controls | Discrete buttons, clickable entries, main search input; complex sliders/dropdowns/editors later. |
| P13 | Hint stability | Stable in active session; deterministic for identical target sets on later activations. |
| P14 | Leader while active | `Shift+Space` cancels the current session, like Escape. |
| P15 | Unsupported dialogs | No hints or activation for blocked background controls; dialog controls added in later milestones. |

# 3. Technical contracts and invariants

## 3.1 Navigation state machine

Model Jump Mode explicitly rather than as unrelated booleans:

- `Disabled`: feature off; no Jump Mode input interception and no active session/overlay.
- `Idle`: feature enabled but not jumping; existing app behavior unchanged.
- `Active`: immutable hint-to-target mapping for the current session, typed prefix, allowed input capture, visible overlay, and a token/generation identifying the current view and modal scope.
- `Resolve`: a completed hint produces a one-shot focus/selection intent consumed by the proper UI owner. The session closes, but target focus/selection must survive the normal input/render cycle. There must be **no execution** here.
- `Cancelled`: clears the overlay and hint prefix, restores prior focus when safe, and returns to Idle. Do not resurrect focus on a closed widget or in a different modal.

The implementer may use a smaller equivalent enum/state representation if it enforces these invariants. Never leave an unowned pending action that Enter could accidentally execute twice. Ensure a fresh Enter keypress **after** selection is required; an Enter event from the hint session must not leak into normal activation.

## 3.2 Target identity and ownership

Represent a target with enough typed information to determine:

- **Logical identity** across frames, independent of pixel coordinates or display text alone.
- **Surface identity** (root dashboard/grid, settings modal, specific dialog instance) and modal priority.
- **Focusable/activatable kind** (text entry, button/action, select row, checkbox, slider, tab, combo, etc.).
- **Current visible rectangle** and any parent clipping; position can change without changing target identity.
- **Supported behavior** for committing focus/selection; do not put plugin-specific execution into the generic hint engine.
- **Eligibility** (enabled, visible, interactive and permitted under current modal).

Prefer existing `egui::Id`/`Response` or action identity in established rendering code, with a surface namespace; do not use note titles, list indices alone, screen coordinates, or strings parsed out of labels as global identities. For repeated identically titled items use the durable record/action identity. If a target disappears, becomes disabled, or its modal owner changes, invalidate it safely. Maintain session hint assignments for targets that remain, and cancel/restart the session if structural changes would make the mapping unsafe; **do not quietly reassign an already displayed hint**.

Do not add an extra generic UI traversal system or duplicate entire egui widget trees. Collect registration data as widgets are rendered through narrowly scoped hooks. Favor bounded visible-target collection only; account for clipping and viewport. Avoid borrowing pitfalls by delivering focus intents at a legitimate UI ownership boundary rather than forcibly reaching into unrelated renderers.

## 3.3 Hint algorithm contract

- Exactly `A`–`Z` (26 symbols), uppercase for display; normalize incoming text case-insensitively.
- Every registered eligible target receives **one** code.
- No code is a prefix of another; no duplicates, no empty codes.
- For zero targets, provide a quiet no-target indicator and cancellation; for one target, a short single-character hint is acceptable.
- Prefer one character when feasible; when >26 visible targets, assign two/longer characters as necessary without ambiguity. A variable-length, prefix-free scheme is preferable if it genuinely reduces typical typing cost, but a deterministic same-length scheme chosen by target count is acceptable only if the implementer explains its tradeoff and tests the practical result. Do not claim the naive scheme is globally minimal.
- Deterministically sort targets using stable surface/visual order and stable logical IDs, not HashMap iteration order. The layout can change between sessions, but assignments must not reshuffle due only to nondeterministic iteration.
- Prefix matching must be linear or better in the **visible target count**, without expensive filesystem/indexer/plugin rebuilds. For typical UI counts, simple bounded iteration is acceptable.
- Typing a valid partial prefix does not select; a completed unique full hint produces exactly one selection event.
- Backspace removes one character; unsupported characters/invalid continuation must never execute another command or modify underlying text. Recommend retaining the prefix and showing subtle invalid feedback, with unit tests for chosen behavior.
- Modifier handling: detect the leader before hint characters; do not treat unrelated Ctrl/Alt/Win shortcuts as hint letters. Account for key repeats and paired key/text events in egui.

## 3.4 Focus and activation contract

Different controls require different focus mechanics:

- **Grid action/notes:** Use existing logical item selection if possible; otherwise introduce a small explicit focused-action state shared with the existing keyboard Enter path. Hint entry never calls `activate_action`/equivalent directly. The **same action handler** used by ordinary Enter/click must be invoked by the subsequent activation. Preserve whether the launcher hides/keeps open and whether action history changes.
- **Dashboard button:** Focus a real egui widget if it supports keyboard activation; otherwise bind one-shot logical selection with a subsequent Enter routed through the same widget/action dispatcher used by normal clicking.
- **Text field/search:** Request true keyboard focus at a safe frame boundary and put the caret into that field. Normal character input after a completed hint must edit that field, without resetting the query or repositioning the window unexpectedly.
- **Checkbox/slider/combo/tabs:** In Phase 2, use native semantics (Space/arrow/Enter) or the smallest verified equivalent; do not simulate mouse coordinates or synthesize fake clicks as production implementation.
- **Destructive commands:** Jumping only focuses; activation uses existing confirmation and permission guardrails; safe fixtures must not run real destructive commands.

**Input conflict invariant:** ordinary grid arrows/numpad, search edit, global show/hide key, radial/pie menu hold-tap logic, and existing modal keyboard ownership must behave exactly as before when Jump Mode is disabled/inactive. Avoid introducing new process-wide Win32 hooks for a foreground-only feature.

## 3.5 Rendering contract

- Draw a subtle but visible `JUMP` state indicator and compact bright hint badges on the dark-green UI; use current theme APIs/color tokens if available.
- Hints are visually attached to the right control without obscuring its essential title/action text. Position inside or immediately adjacent to the control's visible rectangle; clamp to visible panel/window bounds and clipping regions.
- Progressive matching: keep matching hints prominent and dim/hide nonmatching targets; do not block ordinary widgets with an input-intercepting transparent window.
- Overlay is temporary, no permanent layout displacement or extra dashboard column. Show a visible focused/selected state after a completed jump even though the hint layer disappears.
- Handle DPI scaling, window resize, different font sizes, clipping, scrolling and screen edge. Do not require pixel-identical snapshots across machines.
- No unnecessary frame pumping, background polling, or full-screen blur/paint allocation while inactive.

## 3.6 Modal / popup contract

The foremost modal/popup allowed to receive keyboard input defines the active target surface. Inaccessible background widgets cannot register eligible hints or receive focus intents. In Phase 1, unsupported dialogs can suppress Jump Mode entirely. In Phase 2, register supported controls inside the owning modal; opening/closing/changing modal cancels or refreshes state safely. A hint cannot select a widget from a different dialog instance with a coincidentally reused egui ID.

## 3.7 Persistence and safety contract

- Add backward-compatible serialized settings with `#[serde(default)]`/equivalent according to current settings convention; **legacy files must load with Jump Mode off**. Do not rewrite the entire settings file with unrelated defaults or change keys gratuitously.
- Runtime test fixtures must use isolated temporary application data/config paths, not the developer's live notes, macros, pinned queries, bookmarks, clipboard history, or actual filesystem-delete commands.
- Avoid modifying clipboard, launching browsers or external applications, sending global hotkeys, changing audio, or deleting data during the automated demo.
- Avoid changing `AGENTS.md`, agent TOMLs, build profiles, unrelated plugins, or global launcher startup semantics except where a direct proven dependency requires a narrow change.

# 4. Autonomous execution and Git control

## 4.1 New branch creation — **before source edits**

The orchestrator must:

1. Read `AGENTS.md`; run `git status --short --branch`, `git branch --show-current`, `git rev-parse HEAD`, `git remote -v`; record base branch/ref, commit SHA and pre-existing uncommitted state in `docs/plans/JUMP_MODE_EXECUTION.md`.
2. Create a **new, unique branch** based on the *current checkout HEAD*, not by arbitrarily switching to `master`, pulling, or merging: e.g. `feat/jump-mode-<run-date>-<unique-suffix>` based on the actual run date/time. Never reuse/move an existing branch with that name; generate a new suffix on collision.
3. If the tree is clean, `git switch -c <branch>` is appropriate. If the only untracked files are the two **user-provided task input documents**, it is also acceptable to branch in place and explicitly stage only those task documents later. If there are unrelated user changes, **do not stash, clean, reset, overwrite or commit unrelated changes**. Prefer a separate clean Git worktree from the current HEAD on the new branch, in an accessible location, leaving the user's tree unchanged. If using a new worktree, copy the two user-provided Jump Mode plan/kickoff Markdown input documents **only** into the task worktree after creating its branch, ensuring the Goal remains available and can be committed to the task branch. Do not copy unrelated dirty files or change source history. If neither method is safe/available, record a precise blocker and do not modify the user tree.
4. Commit source only on the new task branch. Use the exact branch/worktree root for all subsequent commands. No force-push, destructive history edits, hidden rebases, or auto-merges.
5. Follow the repository's **Active Checkpoint Commit Cadence**: use coherent `M#-A`, `M#-B` markers when a milestone naturally has multiple independently verifiable slices; no noisy per-line commits and no giant accumulated commit.
6. After every completed checkpoint commit, **try one normal push** of that branch (`git push` or `git push -u origin HEAD` if applicable). If unavailable or rejected, leave the commit local, record status, and **continue**. Do not repeatedly retry, ask for credentials, change branch, or fetch/rebase to repair a failed push.
7. Do not create a pull request, merge into `master`, or deploy without a specific user request. The user can review the finished branch and evidence later.

## 4.2 Orchestrator / agents

- Use existing `planner` for scoped, read-only clarification of source ownership at the beginning of a large phase or a genuine mismatch, **not a second open-ended rewrite of this goal**.
- Use `implementer` for bounded write milestones, **sequentially** in one codebase. The orchestrator owns state, milestone transitions, commits, pushes, and final report. Do not run two code-writing implementers simultaneously on shared files.
- Use `reviewer` for meaningful completed phase boundaries or final architecture/regression review; concrete defects return to a narrowly scoped implementer fix. Do not run endless review/fix loops for stylistic preferences.
- Every agent is told the active milestone, dependencies, acceptance evidence and exclusions, and may read only the relevant source. No agent requests manual user verification.
- Never pause at a milestone awaiting user action. If a genuine environment blocker prevents UI automation, continue safe independent unit/integration work and report the blocker accurately rather than inventing screenshots.

## 4.3 Durable progress / report files

Maintain on the feature branch:

- `docs/plans/JUMP_MODE_EXECUTION.md`: initial SHA/branch, agreed scope, stage ledger (`pending | in_progress | complete | blocked`), latest commit SHA, one-line result of verification, known blocker, next step, and push status. Update after each checkpoint; keep it concise.
- `docs/plans/JUMP_MODE_RESULTS.md`: detailed final acceptance matrix, source-change map, key architecture decisions, commands with exits, GUI run IDs, screenshot index, performance comparisons, remaining limitations, final commit and push status.
- `dev/jump_mode/README.md`: repeatable test-harness setup, prerequisites, one command to run, safe fixtures, expected artifacts and cleanup/timeout behavior.
- `dev/jump_mode/evidence/<run-id>/`: **local, retained, normally gitignored** PNGs, JSON/JSONL traces, harness stdout/stderr, scenario results, manifest, and failure artifacts. Do not bloat source-control history with screenshots unless this checkout already has an explicit convention. User must be able to browse this folder **while the session is running**; append named artifacts as soon as each case finishes, not only at the end.

If the worktree/runtime is ephemeral, persist the structured **results report**, harness scripts, and small curated screenshot samples in Git and best-effort push them; preserve and report full local screenshot artifact paths. Where supported, publish the complete evidence through an existing non-destructive artifact channel. Do not claim raw local screenshots are remotely available unless they actually are.

## 4.4 Validation budget / anti-spiral

- Prefer `cargo nextest run` with exact affected test filters; use `cargo test` for the specific target if nextest is unavailable. Use `cargo check`/the correct app binary build when integration changes demand it. Only use broad suites when a concrete cross-cutting modification cannot be covered by focused tests.
- For each checkpoint: one focused compile/test pass after implementation; on failure, one root-cause investigation and up to **two deliberate repair-and-recheck attempts** before recording a blocker or escalating with precise evidence. This is not a command limit on legitimate compilation; it is an anti-spiral rule against endless speculative fixes.
- For GUI E2E milestones: **one nominal run per newly integrated scenario group**; at most two targeted reruns after a demonstrated fix. Do not rerecord every screenshot on every minor code edit.
- Build expensive application variants only when necessary: use cached artifacts, one feature-gated instrumented build for visual tests, and avoid repeated release/profile builds.
- Clippy: ensure no **new** lint warnings from changed code under the repo's established command; if pre-existing warnings exist, record separately without widening scope to fix them.
- Formatting: `cargo fmt --check` at relevant milestones and final.
- No blanket 100% parity benchmarking. Compare basic Jump Mode enabled-but-idle frame overhead and active hint latency to a recorded local baseline; investigate only meaningful, reproducible regressions.
- A genuine blocker must be recorded as such; do not turn the project into an unbounded test-infrastructure repair effort.

---

# 5. Windows visual acceptance harness — explicit product deliverable

This is **required**, not an optional QA afterthought, because Jump Mode is a visual/focus-sensitive feature.

## 5.1 Harness architectural objective

Provide one documented developer command (PowerShell is natural on the target platform) that:

1. Verifies Windows + active interactive desktop/session (do not assume Session 0/headless machines can show windows).
2. Creates a fresh, clearly owned temp fixture directory and predictable launcher data/config containing safe fake notes/action rows and a configured dashboard. Never loads the user's production data.
3. Starts the **real Multi Launcher binary**, instrumented only as necessary for safe fixture selection/QA observation, with Jump Mode enabled via the isolated test config. If the project already has a supported config override, use it. Otherwise introduce the smallest debug/test-only entry boundary, not an unconditional production bypass.
4. Waits for the main window's actual ready condition (foreground eligibility, visible bounds, rendered target registration), using bounded polling on explicit state/Win32 handles or existing QA trace hooks rather than blind fixed sleeps.
5. Sends **realistic keyboard events** for Shift+Space, A–Z hints, Backspace, Escape and Enter. Prefer existing test/Win32 interaction utilities; inspect `src/gui/qa_framebuffer.rs`, radial smoke utilities, existing native test infrastructure. Automating the app through external Win32/UIA *test tooling* is allowed; the production Jump Mode must remain native.
6. Collects a **structured observer trace** of activated mode, surface, assigned target identity/hint, typed prefix, cancel/resolve outcome, focus/selection confirmation and existing command execution outcome. Add a minimal test-only observability seam if needed. Do **not** use screenshots or OCR as the only proof of behavior; do **not** create a parallel fake UI.
7. Captures a **real application window screenshot** for each defined checkpoint. Prefer/extend the existing feature-gated framebuffer hook for reproducible content, and if practical additionally capture the presented native window (explicitly label framebuffer-vs-presented origin). Inspect its manifest semantics and limits; do not claim a CPU framebuffer callback proves GPU presentation. Provide PNGs with human-readable stable names and a JSON index.
8. Executes an explicitly safe confirmation action from fixture data and verifies the correct action handler/path ran **once**, using structured trace; no real file deletes, notes mutation outside fixture, external apps, or global shortcuts.
9. Closes the **owned** app gracefully and waits for exit. On failure/timeout, safely terminate **only the process tree proven to be owned by the harness**, ideally reusing the repository's existing process-containment script; distinguish other user-owned running Multi Launcher instances and leave them alone.
10. Verifies post-cleanup that no owned process remains, then reports results, screenshot paths, temp cleanup outcome, exit code, and any inaccessible environment limitation. Preserve screenshots/logs; remove or retain fixture state according to an explicit safe policy.

## 5.2 Test isolation and collision handling

- Do not bypass single-instance protection or attach to a pre-existing user process without an explicit safe test strategy. Prefer a dedicated test-mode identity or isolated app-data/config path only if the codebase supports it; if a narrowly scoped test-only isolation mechanism is required, implement and document it without changing default production single-instance behavior.
- Use non-destructive fixture commands with verifiable effects, e.g. a test-specific observer event/counter or safe dialog open. The harness must not depend on OSRS data, actual notes, the user's clipboard, or a network service.
- Keep source-controlled harness code readable and minimal; do not add Selenium/browser drivers, OCR, multi-platform orchestration frameworks, or large production dependencies solely for this feature.
- Use unique test run IDs, isolated temp locations, recorded PID+start time, bounded scenario and entire-run timeouts, and `try/finally`-style cleanup.
- Do not simulate a “pass” in environments lacking an interactive GUI. Mark GUI scenarios `NOT_RUN_ENVIRONMENT` with the reason, while still collecting Rust unit-test evidence.

## 5.3 Screenshot catalog (minimum expected named artifacts)

| Screenshot | Required state | Automated assertion |
|---|---|---|
| `01_dashboard_normal.png` | Dashboard before activation | App visible, Jump Mode idle |
| `02_dashboard_hints.png` | Shift+Space, visible dashboard badges | Unique codes, target count >0, no execution |
| `03_dashboard_prefix.png` | Partial hint typed on a multi-character case | Matching targets narrow; no action |
| `04_dashboard_selected.png` | Dashboard button focused after full hint | Overlay off, selected target identity correct |
| `05_dashboard_after_enter.png` | After Enter on a safe dashboard action | Correct action emitted exactly once |
| `06_grid_normal.png` | `note list`/safe fixture action grid | Fixture items visible |
| `07_grid_hints.png` | Active hints over note cards | Visible cards receive unique hints |
| `08_grid_selected.png` | Specific note selected via hint | Query unchanged, no note open yet |
| `09_grid_after_enter.png` | Enter on selected note | Existing safe note-open path invoked once |
| `10_search_focus.png` | Jump to search then type | New text goes into field, old text preserved as intended |
| `11_cancel_escape.png` | Escape after partial hint | No hint overlay, query/action unchanged |
| `12_cancel_leader.png` | Second Shift+Space | Same cancellation outcome |
| `13_dense_hints.png` | More than 26 visible targets | Prefix-free, readable longer hints |
| `14_modal_safety.png` | Active unsupported or supported modal | No background target offered/executed |
| `15_settings_hints.png` | Settings controls integrated (Phase 2) | Correct foreground settings targets |
| `16_dialog_hints.png` | Representative dialog integrated (Phase 2) | Correct dialog targets |
| `17_dialog_selected.png` | Dialog item focused/selected | Correct destination, no premature action |
| `18_feature_disabled.png` | Jump setting off | Shift+Space does not enter Jump Mode |

Screenshots should be captured after actual UI state transitions and/or frame presentation signals, never by assuming a fixed delay is enough. Some screenshots may be split into more descriptive per-dialog case names. Retain a full-window shot and optionally a cropped close-up, but never replace a full screenshot with only a synthetic painted test image.

## 5.4 Required machine-readable evidence

Under each run:

```text
dev/jump_mode/evidence/<run-id>/
  manifest.json                 # branch, SHA, test build, OS/session, fixture location, app PID
  scenarios.json                # statuses: PASS / FAIL / BLOCKED / NOT_RUN_ENVIRONMENT
  events.jsonl                  # target registration, hints, input, resolve, execute evidence
  screenshots/                  # named real GUI captures
  stdout.log
  stderr.log
  process_lifecycle.json        # PID identity, exit/cleanup verification
  summary.md                    # links to screenshots and assertions, updated incrementally
```

Sensitive user data, raw live clipboard data and private filesystem paths must not appear in published screenshot artifacts. Screenshots are review evidence, not a replacement for assertions; trace-only pass without the required available screenshots is a partial result.

---

# 6. Execution ledger format

Create and maintain `docs/plans/JUMP_MODE_EXECUTION.md`, for example:

```markdown
# Jump Mode Execution
- Source branch / starting SHA: ...
- Task branch: ...
- Current HEAD: ...
- Latest evidence run: dev/jump_mode/evidence/...
- Current milestone: M... (...)
- Environment: Windows desktop / whether GUI automation available

| Milestone | State | Commit(s) | Verified | Push status | Evidence / blocker |
|---|---|---|---|---|---|
| M00 | complete | ... | clean branch | pushed / local | ... |
| M01 | in_progress | ... | ... | ... | ... |
...

## Next action
...
```

A milestone is `complete` only when its **specific done criteria and scoped checks** are met. Commit the ledger periodically with coherent work; avoid committing it on every log line or accumulating screenshots into Git history.

---

# 7. Phase map and dependency order

| Phase | Milestones | Exit state |
|---|---|---|
| **P0 — Preparation & safe test foundations** | M00–M01 | New branch and a reproducible fixture/process harness skeleton exist. |
| **P1 — Navigation engine** | M02–M05 | Settings gate, pure hint engine, target/focus API, keyboard state machine and overlay exist. |
| **P2 — First usable end-to-end experience** | M06–M09 | Visible grid and dashboard support, actual GUI screenshots, verified Phase 1 behavior. |
| **P3 — Extended application surfaces** | M10–M13 | Modal-safe settings, representative dialogs and common complex controls, extended GUI screenshots. |
| **P4 — Quality, performance, handoff** | M14–M15 | Tested and reviewed end-to-end result with clear evidence, known limits and branch status. |

The next sections are the **binding handoff contracts**. Before each milestone, check its dependencies and adapt paths to the actual checkout. Each milestone states its outcome, constraints, checks, and artifacts; do not stretch it to include later-phase ideas.

# 8. Detailed implementation milestones

## P0 / M00 — Safe branch creation, baseline and source ownership map

**Objective:** Establish an isolated implementation branch and record exactly how this checkout handles keyboard events, query/grid selection, dashboard widget activation, settings persistence, dialogs, and existing screenshot/test tooling. Make **no feature source changes** yet.

**Prerequisites:** Repository access, `AGENTS.md` readable, an inspectable Git HEAD.

**Likely inspection entry points:** `src/gui/mod.rs`, `src/gui/search.rs`, `src/gui/actions.rs`, `src/dashboard/dashboard.rs`, `src/dashboard/widgets/mod.rs`, `src/main.rs`, `src/settings/model.rs`, relevant tests, `src/gui/qa_framebuffer.rs`, `dev/run_owned_process.ps1`. Their existence and responsibilities must be rechecked locally.

**Work, in order:**

1. Record source branch, SHA, working-tree cleanliness, relevant toolchain, current key event ownership and dominant app/renderer types. If the tree is dirty, use a safe isolated Git worktree where permitted.
2. Create the unique `feat/jump-mode-*` task branch **before** any source changes.
3. Inspect and diagram the smallest required data/control paths: egui event → launcher input router → current surface/grid/dashboard → focused/selected widget → existing action dispatcher. Pay attention to the existing query focus restoration logic and grid arrow selection so Jump Mode does not conflict.
4. Locate modal ownership and the existing viewport/focus behavior. Identify how the dashboard widgets are rendered and how `WidgetActivation` or equivalent routes existing actions.
5. Inspect existing settings backward compatibility patterns and the current Settings UI section organization; identify a concise location for an **Experimental Jump Mode** toggle (not a new top-level settings redesign).
6. Inspect `qa-framebuffer-capture` feature, radial acceptance binaries and ownership scripts for proven screenshot and cleanup patterns; record reuse decisions, including whether native window capture is available.
7. Identify a safe test action/fixture seam without mutating production data.
8. Ensure the supplied `CODEX_GOAL_JUMP_MODE.md` and `CODEX_KICKSTART_JUMP_MODE.md` are accessible in the task checkout (if they arrived as untracked inputs, preserve/copy only them as described above). Initialize `docs/plans/JUMP_MODE_EXECUTION.md` with a stage ledger and a 1–2 page source-ownership contract. Stage independent entries rather than copying the entire repository into documentation.

**Invariants / non-goals:** Do not change user settings, do not push/merge other branches, do not refactor code, do not run full test suites to establish a baseline, do not adopt obsolete plans.

**Verification:** `git status`, `git rev-parse HEAD`, one quick documented build/config discovery if needed; verify the task branch is based on the recorded starting SHA and user changes remain safe.

**Output / done criteria:** New branch exists; source ownership documented with **verified** current names; ledger initialized; test harness route chosen; no accidental user changes. Suggested checkpoint `docs(jump): [M00-A] establish branch and execution baseline` (commit only if the ledger/source map is substantive). One normal best-effort push.

---

## P0 / M01 — Isolated fixture runner and observable baseline

**Objective:** Create the smallest reusable Windows fixture/launcher harness **before** Jump Mode behavior is added. Prove it can safely start a production-path GUI instance, capture at least an idle/dashboard baseline, and close its own process. This is infrastructure for **this feature**, not a general-purpose QA framework.

**Depends on:** M00.

**Ownership / probable scope:** Developer scripts under `dev/jump_mode/`, test fixture data, narrowly scoped existing main/startup QA hooks only if required. Inspect existing `dev/run_owned_process.ps1` and `src/gui/qa_framebuffer.rs` rather than duplicating their large implementation.

**Work:**

1. Add `dev/jump_mode/README.md` defining a single command to run the forthcoming GUI suite, prerequisites, isolated config path, output dir and safety rules.
2. Generate fixture data deterministically: one dashboard with non-destructive buttons, a `note list`-like grid with deliberately distinct records, and a dense grid (>26 actionable items) for later hint tests. Fixture records must have stable IDs and no dependency on real notes/files/network.
3. Establish a robust fixture injection mechanism: existing path overrides if supported, or a narrow gated QA-only configuration route. Assert the fixture is active before tests proceed; abort safely if real data would be used.
4. Implement a process owner wrapper: record PID and start timestamp; log stdout/stderr; bounded readiness wait; graceful close; finally-block forced cleanup only of verified owned process tree; report post-cleanup state. Reuse owned-process PowerShell scripts if appropriate; do not kill another running Multi Launcher.
5. Capture at least one **real** app screenshot in a GUI-capable Windows session. If the QA framebuffer facility can be reused safely, integrate it; otherwise add a simple native capture method. Record origin and pixel dimensions. If graphics interaction is unavailable, produce an accurate environment-blocked result and continue with headless work.
6. Record the baseline screenshot and startup/cleanup results in a first evidence folder. Create a minimal machine-readable `manifest.json` and `scenarios.json`. At this stage no Jump Mode-specific screenshot is expected.

**Invariants / non-goals:** No launcher input rewrites, no added production hotkeys, no fixture writes to the user's profile, no infinite readiness polling, no blanket process kills, no assumption that the existing QA capture path produces presented pixels.

**Focused verification:** Run fixture creation unit checks and one bounded app-launch/capture/close smoke on a real Windows desktop if available. Verify screenshot actually exists, is a valid nonzero-size PNG, and process cleanup completes; do not assert success based on script exit alone.

**Output / done criteria:** Safe fixture, repeatable launcher runner, baseline evidence, cleanup proof and documented one-command invocation. Coherent commits `test(jump): [M01-A] add isolated GUI fixtures and runner`, optionally `[M01-B] capture and clean up owned GUI instance`; each pushed best-effort.

---

## P1 / M02 — Disabled-by-default persisted feature gate and settings UI

**Objective:** Introduce a backward-compatible setting and its small Settings editor control; implement the runtime gate with **zero behavior changes while disabled**.

**Depends on:** M00; M01 fixture preferably available but this milestone can be verified headlessly.

**Ownership / scope:** Actual settings model/default/storage and settings editor paths (possibly `src/settings/model.rs`, `src/settings/defaults.rs`, `src/settings_editor/render.rs`); GUI initialization/state for reading the effective flag. Do not modify other settings sections.

**Work:**

1. Add one clear boolean persisted setting (name selected to match repository convention) to mark Experimental Jump Mode enabled. Default `false` both in code and deserialization of legacy JSON.
2. Add one discoverable toggle in the most context-appropriate existing Settings section; label it **Experimental Jump Mode**, with concise help: `Shift+Space` displays hints; typing a hint selects, Enter activates; available in supported screens only.
3. Ensure toggling on/off applies predictably (immediately if existing Settings lifecycle supports it; otherwise via established save/apply semantics). Disabling while a jump is active must cleanly cancel.
4. Establish one runtime read/accessor or centralized decision so dashboard/grid render paths do not independently decode settings.
5. Document configuration compatibility and verify existing settings round-tripping. Do not silently enable in migrated settings or test outside fixtures.

**Invariants / non-goals:** Do not add a hotkey global listener, configurable hotkey dialog, new settings layout categories, or change unrelated serializations. No hint rendering yet.

**Tests:** New settings omitted-field default-off; explicit true/false serialization round-trip; toggling behavior; legacy config compatibility. Relevant narrow Settings tests only.

**Output / done:** Setting and UI control exist, older JSON loads with flag off, runtime disabled path remains inert. Suggested checkpoints `feat(jump): [M02-A] add backward-compatible experimental setting`; `[M02-B] expose jump toggle in settings`.

---

## P1 / M03 — Pure target model, prefix-free hint assignment and matcher

**Objective:** Implement the independent, testable *pure logic* at the heart of Jump Mode without coupling it to egui painting, Win32, or action execution.

**Depends on:** M02 for enable path; no GUI rendering dependency.

**Ownership / scope:** New cohesive module under the actual GUI/nav namespace (e.g. `src/gui/jump_mode/` if confirmed appropriate); model types for `TargetId`, `SurfaceId`, `Hint`, `HintMap` and selection outcome. Avoid exposing broad public APIs unless needed by actual integration.

**Work:**

1. Define typed stable target identity and concise target metadata suitable for both rendering and focus dispatch; separate session identity from the current coordinate bounds.
2. Implement deterministic ordering and assignment for `0`, `1`, `2`, `26`, `27`, `52`, `100`, and large bounded visible target counts; enforce unique, prefix-free sequences using A–Z only.
3. Implement prefix matching and outcomes: partial, completed target, invalid, empty/no-target, Backspace. Input normalized case-insensitively.
4. Create a table of precise characteristics (number of hints, longest code, mean length for representative sizes) to catch pathological assignment. Prefer shorter average code length without unstable heuristics; if using fixed length, document why and verify UX acceptability.
5. Provide a session snapshot of target IDs and hints; freeze assignments until cancel/resolve. Define behavior when a target is invalidated and when target population changes; never reassign a displayed hint silently.
6. Add tests for collisions, prefix ambiguity, deterministic ordering independent of source map insertion order, varied Unicode target display names, duplicate labels with distinct IDs, and proper cancellation outcomes.

**Invariants / non-goals:** No egui UI, no fake clicks, no widget-specific actions, no user-visible settings additions beyond M02, no offscreen search.

**Verification:** Targeted module unit tests or Nextest filter, `cargo fmt --check`; property-style deterministic tests for synthetic target sets. Do not introduce large property-testing dependencies if a bounded randomized test with an existing crate or deterministic seeds suffices.

**Output / done:** Pure engine API and tests pass; no ambiguous hints; meaningful dense population demonstration in tests. Natural commits `[M03-A] typed navigation model and allocation` and `[M03-B] prefix matching and invariant tests`.

---

## P1 / M04 — Target collection, surface scope and safe focus/selection handoff

**Objective:** Provide an egui-facing integration seam where existing renderers can register actionable **visible** targets and resolve a hint to the appropriate target owner without directly running its action.

**Depends on:** M03.

**Ownership / scope:** Shared GUI integration module and small adapters in actual root GUI/dashboard/grid code. This milestone defines the abstraction; it **does not** register every widget yet.

**Work:**

1. Choose a narrowly scoped registry lifetime tied to the current rendered frame/view. Capture a stable ID, logical surface, eligible `Response`/rect, clipping, enabled state and control kind. Avoid storing stale `egui::Response` or borrow-dependent callback closures across frames.
2. Define active-surface priority: native dialog/popup owner before root dashboard/grid. Suppress background targets when a modal blocks input; no test can activate occluded controls.
3. Establish a safe handoff protocol from navigation resolution to the component that owns actual egui focus or existing selection. Consider a deferred, one-shot intent consumed on the next matching render, with session generation and target identity checks.
4. Ensure invalid/outdated target intents produce no action and clear safely. No user-visible “ghost selection” after a target disappears or dialog closes.
5. Define an **explicit activation boundary**: registration/focus in the generic layer, execution delegated to existing grid/dashboard/dialog action mechanisms later.
6. Confirm the registry costs are bounded/optional when the feature is disabled and that it cannot allocate or rebuild the global command catalog on each frame.

**Invariants / non-goals:** Do not add a new UI tree framework, do not create a separate synthetic mouse dispatcher, and do not modify existing action logic. No complete dashboard integration here.

**Tests:** Registry de-duplication, surface precedence, disabled/clipped exclusion, stale-generation rejection, one-shot intent semantics, duplicate-label distinct-ID targeting.

**Output / done:** Reusable registration and focus/selection contract that other milestones can adopt with small adapters. Checkpoint `[M04-A] register scoped interactive targets`, `[M04-B] resolve focus intents safely`.

---

## P1 / M05 — Leader event capture, Jump Mode lifecycle and visual badges

**Objective:** Make the leader/typing/cancellation state machine functional and render the general-purpose hint overlay over test-registered controls. Establish correctness of keyboard event ownership before integrating full feature surfaces.

**Depends on:** M02–M04.

**Ownership / scope:** Actual root launcher keyboard routing/update and new Jump Mode visual renderer. Inspect preexisting query refocus, numpad and modal input paths.

**Work:**

1. Route `Shift+Space` at the right input priority. Activate only in a focused Multi Launcher viewport and only when enabled. Consume **both** relevant key and possible paired text input so a literal space never gets inserted.
2. If already active, repeated `Shift+Space` cancels. Escape cancels. Backspace removes hint prefix. A–Z typed characters are captured by Jump Mode and do not reach the search query or unrelated widgets. Ignore/reject invalid input safely.
3. Honor Ctrl/Alt/Win key ownership and existing global hotkeys. Prevent auto-repeat/release ordering from causing duplicate toggles. Losing focus or changing modal owner cancels without a ghost activation.
4. On entering mode, take a valid visible target snapshot and initialize deterministic hints. On completion, emit a single focus/selection intent, exit Jump Mode and **consume any same-frame activation input**. On cancel, restore prior focus when safe.
5. Paint visual hint badges and an unobtrusive active-mode indicator using egui painter/layers without resizing underlying layouts. Associate with clipped visible rects, DPI-adjusted dimensions and matching state; keep text readable and badges out of critical labels.
6. Clearly differentiate hints vs focused/selected state. On complete selection, badges disappear but a visible destination indication remains, using existing selection styling if possible.
7. Do not continuously repaint when disabled. Limit repaint demand to active input/visual changes; respect app performance work already present.
8. Add focused unit/integration tests for event ordering: field with text + Shift+Space, Shift release, duplicated key/text event, backspace, Escape, second leader, invalid char, process focus loss.

**Invariants / non-goals:** No broad Windows keyboard hook, no direct action execution, no configurable leader UI in this milestone, no rewriting radial menu hotkey semantics.

**Verification:** Relevant input event and Jump Mode unit tests; basic GUI screenshot from a small test registration or fixture; confirm the actual input path used rather than only invoking private functions directly.

**Output / done:** Leader/hint lifecycle and overlay work on a minimal testable surface; no text corruption; selection never prematurely executes. Commit `[M05-A] route scoped leader and hint keys`, `[M05-B] paint visual hint overlay and focus indication`.

---

## P2 / M06 — Main action grid, search results and note cards

**Objective:** Deliver the flagship Hop-like grid experience: from a populated results view, press leader, type a short hint, select an individual note/action card, then Enter to use the existing execution path.

**Depends on:** M05.

**Ownership / likely scope:** Actual action-grid/list render and keyboard selection methods in GUI (`src/gui/mod.rs`, search/navigation/actions modules as verified), action identity and current query field. Use current grid layout capability and plugin opt-outs as-is.

**Work:**

1. Register **every eligible currently visible card** as a jump target. Detect actual clipping in scroll areas and avoid generating hints for virtualized/offscreen results.
2. Give each target a durable action/item identity; distinguish repeated titles and equal-looking plugin results. Make the hint overlay fit a two-/three-column, tall action grid without covering the title.
3. Implement a `Select` intent that moves logical grid selection to that action and visually marks it. If an existing `selected_index` is the correct owner, reuse it; if the index is unstable, use stable identity resolution and update the existing selection representation at the right time.
4. Preserve the current query string, query caret where safe, plugin search state and result ordering. A hint never synthesizes the note title into query text or performs a new search.
5. Subsequent **fresh** Enter must use the same existing activation implementation as grid keyboard/click, including note-open behavior, use counters, hide/keep-open semantics and any confirmation. Nothing runs when the hint finishes.
6. Verify cases: regular action, note with a duplicate display name, `note list` prefix, 1/10/27+/dense visible cards, selected item scrolled/filtered away during an active session, and query switching before completion.
7. If results update asynchronously, guard focus intents by stable ID + view generation; never select the item that merely reused an old index. Prefer safe session cancel on structural invalidation.

**Invariants / non-goals:** No plugin parser rewrite, no fuzzy search replacement, no new note commands, no forced grid layout for plugins that opt out, no offscreen auto-scroll.

**Tests:** Grid identity/selection and exact-once Enter activation, query preservation, stale target invalidation and prefix views; relevant existing grid/navigation tests.

**Output / done:** `note list` direct keyboard targeting works correctly and is clearly selected before Enter. Screenshot names `06_grid_normal`–`09_grid_after_enter` created by later M08 harness. Checkpoints `[M06-A] register and focus visible grid actions`, `[M06-B] preserve activation and note semantics`.

---

## P2 / M07 — Dashboard discrete action integration

**Objective:** Enable the same navigation model on the existing multi-column dashboard without redesigning it. Support discrete, safe-to-focus buttons and clickable rows first, plus main search input.

**Depends on:** M05, reusable target registry and tested generic focus dispatch.

**Ownership / likely scope:** `src/dashboard/dashboard.rs`, `src/dashboard/widgets/*`, root launcher search input, any actual dashboard action dispatch. Inspect widget `render` trait signature and slot scoping; integrate through the minimal widget-facing registration adapter needed.

**Work:**

1. Register real visible controls from: Recent Commands; Pinned Queries; Todo buttons; Calendar Open/+Add; Bookmarks Refresh/clickable rows; Temp Add/Refresh/Clear all; Timers New Timer/safe buttons; Scratchpad Copy/Append timestamp; Now Playing controls; Recycle Bin controls; system Refresh buttons where supported; main search field. Coverage depends on enabled widgets/visible controls; do not create hints for absent widgets.
2. Bind target identity to slot/widget instance and the control/row inside it to avoid collisions between two copies of the same widget. Different instances with the same “Refresh” label must get distinct hints and destinations.
3. Preserve existing `WidgetAction`/dashboard event routing and mouse-click behavior. **Hint completion selects only**; pressing Enter invokes the existing event. Protect destructive controls (e.g. Recycle Bin Clean, Temp Clear all) via existing confirmations; automated fixtures must never run destructive actions.
4. For dynamically updated panels, ensure badges follow the current rect or session cancels safely when a target disappears. Empty panels do not receive phantom hints.
5. Do not require every dashboard widget to implement a new large trait or duplicate their action routing. Prefer small opt-in helpers/adapters and clear non-registered control fallback until Phase 2.
6. Implement main search-field target: selecting it grants true focus; subsequent typing behaves exactly like ordinary search. Pressing Shift+Space while search already focused must preserve the existing query.
7. Verify keyboard focus and overlay positioning across a 2- and 3-column dashboard, a resized window, a scrolled widget, and at least one duplicate-label `Refresh` case.

**Invariants / non-goals:** No dashboard editor redesign, no cache refresh scheduling changes, no widget layout restructuring, no sliders/dropdowns/multiline editors yet (Phase 2). Do not claim 100% dashboard coverage if some visible controls are not yet instrumented; list exact gaps in ledger.

**Tests:** Widget identity, duplicate labels, disabled controls, safe Button->Enter routing, search focus and preservation. Reuse dashboard test utilities when feasible.

**Output / done:** Representative dashboard actions are visible with hints and work with focus-then-Enter. Safe target coverage inventory updated. Suggested `[M07-A] add dashboard target registration`, `[M07-B] connect keyboard focus and action activation`.

---

## P2 / M08 — Actual end-to-end keyboard driver, screenshots and cleanup verification

**Objective:** Turn M01's skeleton into an autonomous **native GUI acceptance suite** that drives the actual new feature and saves visual/structured evidence, with safe shutdown after each test run.

**Depends on:** M06–M07; M01 fixture/owned-process runner.

**Ownership / scope:** `dev/jump_mode/` test runner and fixture; only narrowly gated `qa` observer hooks in actual app if needed. Prefer existing `qa-framebuffer-capture` and process ownership tools; avoid new production dependency.

**Work:**

1. Implement scripted scenarios for dashboard idle→hints→partial→selected→Enter; grid idle→hints→selected note→Enter; text field preservation; cancellation by Escape and leader; dense hints; disabled feature; stale target/modal safety.
2. Use actual keyboard dispatch to a running Multi Launcher window. The harness must resolve a target hint from structured registered target data (e.g. target ID→hint mapping from observable QA events) rather than assuming letters always mean the same target after UI data changes. This tests the real dynamic hints while remaining deterministic.
3. Persist both **state assertions** and screenshots for each step, immediately available on disk. The assert for “selected” must verify action count is still zero, target ID matches, and query is unchanged; after Enter, verify correct handler invoked once.
4. Capture required catalog `01`–`14` and `18` from §5.3 when the relevant view exists; reserve Phase-2 screenshots `15`–`17` for M13. Do not fake an unrendered screenshot or mark unavailable cases passed.
5. Implement run-wide cancellation/timeout and per-step deadlines; no unbounded polling. Record environment readiness and screen coordinate/DPI metadata. Wait for actual focus/render state, not arbitrary sleeps.
6. Catch errors, record exception and partial evidence, close the app gracefully then own-process clean-up in a guaranteed finally path, confirm no PID identity remains. Never kill a user-owned instance.
7. Write `summary.md` and `scenarios.json` progressively; evidence path and screenshot index referenced in the execution ledger.
8. Produce a small smoke run command that allows `-Scenario Dashboard`, `-Scenario Grid`, or equivalent filters so later milestones don't rerun every screen after each change.
9. Add an index and at least two privacy-safe, fixture-only, representative PNGs to the size-bounded `docs/plans/jump_mode_review/` curated evidence set, commit/push at the phase boundary. Full raw evidence remains local/ignored.

**Invariants / non-goals:** Not an external app automation product, not screenshot-only assertion, not a CI mandate requiring an interactive desktop on every developer machine, not a source-control archive of all PNGs.

**Tests / verification:** One real desktop run per newly implemented scenario group; verify screenshot PNG validity, dimensions, trace events, process cleanup, and that a deliberately invalid fixture causes `FAIL` rather than false `PASS`.

**Output / done:** Single-command, repeatable launch→interact→capture→close suite with actual passing dashboard/grid scenarios and machine-readable evidence (or explicit environment blocker, not a fabricated pass). Checkpoints `[M08-A] drive native jump interactions`, `[M08-B] record screenshots and owned-process cleanup`.

---

## P2 / M09 — Phase 1 stabilization and acceptance gate

**Objective:** Bring the experimentally usable dashboard/grid experience to a known-good checkpoint **before** expanding into dialogs. Resolve concrete cross-surface defects from the first GUI run, not all unrelated launcher bugs.

**Depends on:** M02–M08.

**Work:**

1. Review the Phase-1 acceptance table below. Verify via focused Rust tests and actual M08 GUI evidence that every selected control is focused before execution, and existing query/hotkeys continue to behave normally.
2. Run the feature-off regression: validate Shift+Space and ordinary search/edit behavior do not unexpectedly change when disabled. Validate legacy settings keep flag off.
3. Compare two or three representative screenshot sets for target placement, legibility and selected-state visibility. Note or repair actual collisions/obscured labels; do not turn this into a theme redesign.
4. Inspect the diff for action handler duplication, key-event interception outside active mode, unnecessary allocations and stale intents. Ask read-only reviewer for concrete issues; implement targeted fixes if needed.
5. Update `JUMP_MODE_EXECUTION.md` and `JUMP_MODE_RESULTS.md` Phase 1 section with PASS/FAIL/BLOCKED, links, issue counts, and precise current commit. Continue to Phase 2 without user signoff.

**Exit acceptance (all required where environment supports native GUI):**

- One-character and multi-character hints resolve deterministically and without collisions.
- `note list` exact-note selection does not edit the query and does not launch until a later Enter.
- Dashboard button selection precedes action; existing normal activation follows.
- Search-field focus works; entering Jump Mode while typing does not insert spaces.
- Escape, Backspace, repeated leader, invalid key, focus loss and disable behavior are safe.
- Background controls hidden behind a modal cannot be targeted.
- Native harness saves requested Phase-1 PNGs and cleans owned processes.

**Non-goals:** Do not add arbitrary dialogs or performance optimizations at this gate.

**Output / done:** Phase 1 reviewed and tagged in ledger; one small stabilization commit if actual fixes were needed (`fix(jump): [M09-A] stabilize initial navigation flows`) and best-effort push.

---

## P3 / M10 — Modal ownership and Settings controls

**Objective:** Generalize the same navigation mechanism to the Settings editor and properly enforce topmost modal ownership. Do this without breaking root launcher search or nested popup handling.

**Depends on:** M09.

**Ownership / likely scope:** Actual settings editor render/state, root dialog hosting, Jump Mode registry/context. Existing `TopLevelSettingsSection`/equivalent should be inspected, not replaced.

**Work:**

1. Identify when the Settings dialog is active, its instance identity, and which controls are eligible. Modal priority must prevent dashboard and grid hints from appearing/acting behind Settings.
2. Register the visible Settings tab/category controls and basic buttons, then its visible checkboxes, toggles and text inputs. Use correct native focus, Space for checkbox, Enter for buttons, and query/caret preservation for text entry.
3. Defer dropdowns, sliders and advanced editors to M12 but ensure unsupported controls do not receive nonfunctional hints. If a control is exposed, it must actually be operable after focus.
4. When Settings opens or closes during Jump Mode, cancel/invalidate the old session rather than allowing stale IDs. When a popup is open, target only the active popup or suppress if unsupported.
5. Verify the experimental toggle can be reached through settings without causing Jump Mode to reenter after it has been disabled. Keep UI layout and setting persistence unchanged except for the new Jump Mode toggle from M02.
6. Document the pattern for new modal scopes so subsequent dialogs can adopt it without copy-pasting a separate keyboard router.

**Invariants / non-goals:** No Settings UI redesign, no rewriting other settings ownership or serialization, no unbounded traversal of invisible controls, no action execution upon hint completion.

**Tests:** Modal-owner switching, focus of Settings button and text input, toggle by Space, disabled control exclusion, background blocking, close/reset behavior.

**Output / done:** Settings modal basic controls are jumpable with correct focus/Enter/Space behavior; background remains unreachable. Checkpoints `[M10-A] enforce modal surface priority`, `[M10-B] integrate Settings interactive controls`.

---

## P3 / M11 — Representative application dialogs and popups

**Objective:** Reuse the same target registration/focus API in commonly used first-class dialogs without introducing special-purpose hint engines for each dialog.

**Depends on:** M10.

**Priority coverage (inspect actual UI and choose equivalent screens if moved):**

1. Notes list/editor navigation and note-management buttons (including safe search/filter elements).
2. Todo management dialog: action buttons, selectable items, text entry and safe toggles.
3. Timer/new-timer dialog: buttons and editable numeric/text fields.
4. Paste Sequence dialog: action buttons and selectable sequence rows, preserving current execution/keep-open semantics.
5. One nested popup/confirmation example to prove topmost ownership and cancellation.

**Work:**

- Inventory each dialog's real interactive controls, focus owners, modal container and preexisting keyboard handlers; add the minimal registration hooks.
- For each dialog, register visible actionable elements, use stable IDs that include dialog instance, transfer true focus or logical selection, and reuse its established action method after a distinct Enter/Space.
- Preserve the dialog's normal visibility and lifecycle. Specifically, do **not** accidentally hide the parent launcher or a paste-sequence dialog merely because Jump Mode selected its button. The eventual action may still follow its existing hide/show policy.
- For repeated rows/items, use durable IDs and reject stale selection after sorting/refresh.
- Preserve unsaved user text during activate/cancel; no automatic focus to query outside the current modal.
- Verify nested popup steals target ownership and on close restores an eligible prior focus state or cleanly cancels.
- Document per-dialog supported controls and unsupported controls explicitly instead of claiming blanket support.

**Invariants / non-goals:** No changes to note storage, todo model, timer engine, or paste sequence scheduler; do not “fix” unrelated dialog behavior as part of this feature. No broad plugin API migration.

**Tests:** One meaningful safe targeting/Enter case and cancellation case per dialog; include a nested modal/background-block regression.

**Output / done:** Common dialogs keyboard-navigable via the shared engine with evidence paths reserved for M13. Checkpoints `[M11-A] cover Notes and Todo dialogs`, `[M11-B] cover Timer, Paste Sequence and nested popups`.

---

## P3 / M12 — Complex widgets and MkMacro representative coverage

**Objective:** Extend Jump Mode's *focus contract* from simple buttons/rows to common complex controls and a large custom-dialog example, while staying bound to visible actionable elements.

**Depends on:** M10–M11.

**Control kinds to support where the actual codebase offers them:**

- Checkboxes, toggles/radio groups: focus and Space/arrow selection.
- Slider/numeric input: focus and bounded arrow adjustment, without dragging the mouse; distinguish editable textbox vs slider thumb.
- Combo boxes/dropdowns: focus, Enter/Space opens it, visible popup items become scoped targets or native keyboard arrows, Escape closes safely.
- Tabs/tree/list rows: focus/selection with existing arrow/Enter semantics; avoid requiring hover to interact.
- Multi-line editors: true keyboard focus/caret, keyboard input goes into editor without Jump Mode swallowing post-selection keys.
- MkMacro editor: visible action rows, toolbar buttons, fold/expand headers and safe fields (where available). The user explicitly values this dialog, so it is the larger representative integration surface.

**Work:**

1. Select the minimal set of reusable control-kind adapters required to implement the above, avoiding a universal trait object system without concrete need.
2. Inspect MkMacro's actual list/row ordering, stable action ID, selection/focus and modal/popup flow. Register its visible action rows and common toolbar controls first, then representative properties fields.
3. Ensure Jump Mode **does not start or run a macro** simply by resolving an item hint; execution still requires the established confirmation/button path. Use safe fixtures, with macro playback disabled/not invoked in automation.
4. Preserve existing folding, recording hotkeys, selection state and edit text. A hint should not mutate macro contents merely because an action row became selected.
5. Scope nested menus and dialogs in MkMacro correctly; when a popup is open, do not jump to blocked underlying action rows.
6. Add measured tests for 1–2 controls of each type and for a representative large list. Do not attempt to “support every possible custom widget” with undocumented behavior. Explicitly mark genuinely non-focusable unsupported controls, and if a newly added adapter cannot preserve native behavior, leave it unregistered until corrected.

**Invariants / non-goals:** No new macro commands, no MK recorder/playback refactor, no direct Windows input simulation in production, no offscreen item selection, no uncontrolled extension to external apps.

**Tests:** Control-kind focus then activate; no action-before-Enter; MkMacro selected row preserves macro content; popups block background; size/scroll handling.

**Output / done:** Documented support for common complex control types and functional Jump Mode in representative MkMacro editor areas. Checkpoints `[M12-A] add native complex-control focus adapters`, `[M12-B] integrate MkMacro visible actions and safe controls`.

---

## P3 / M13 — Extended real-GUI scenarios, screenshots and coverage inventory

**Objective:** Extend the M08 harness across Settings and integrated dialogs, assert that control semantics and modal safety work in the actual native application, and leave labeled screenshots for later user inspection.

**Depends on:** M10–M12, M08 harness.

**Work:**

1. Add safe fixture states for Settings, Notes, Todo, Timer, Paste Sequence and MkMacro where possible without using real user data. Avoid script steps with destructive side effects.
2. Capture at least the reserved `15_settings_hints.png`, `16_dialog_hints.png`, `17_dialog_selected.png`, plus named screenshots for one Notes case, one Paste Sequence case and one MkMacro action list if those features are available.
3. Assert that hints map to the intended dialog/surface and cannot select a target behind a modal; check opening/closing a popup cancels stale hint sessions.
4. Confirm native control semantics for checkbox/Space, text focus/editing, slider arrows, dropdown choices, list/row Enter and action button Enter as applicable; no false pass from only checking selection styling.
5. Verify safe scripted execution count = 1 where an action is invoked; verify in tests that macro playback/real clipboard mutations remain unused.
6. Run feature disabled regression again and review view resizing/DPI/high target density visual placement; screenshot is supplementary to structured assertions.
7. Update the final coverage matrix: each supported surface/control kind gets `supported+verified`, `implemented+unverified environment`, `not supported by design` or `blocked`, with evidence references.
8. Ensure every test run cleans up owned app processes and retains reviewable artifacts immediately. Document the selected fixture seed and exact harness command to reproduce.
9. Add selected Settings/dialog PNGs to `docs/plans/jump_mode_review/` without exceeding the small curated-set budget; commit/push at a coherent checkpoint for remote review.

**Invariants / non-goals:** No pixel-perfect screenshot test across multiple machines; no sweeping new test frameworks; no simulated success without an interactive desktop.

**Output / done:** Phase-2 evidence folder contains real labeled screenshots and structured results; all claimed supported control kinds are actually verified. Checkpoints `[M13-A] extend native dialog scenarios`, `[M13-B] publish coverage and screenshot evidence`.

---

## P4 / M14 — Targeted reliability, responsiveness and regression hardening

**Objective:** Remove demonstrable defects discovered in prior stages; confirm ordinary Multi Launcher usability/performance remains intact without initiating another large optimization project.

**Depends on:** M09, M13.

**Work:**

1. Run focused existing tests touching keyboard navigation, query focus, settings serialization, dashboard widget activation, grid/list activation, modal focus, and relevant tested dialogs. Add targeted regressions only for actual changes; do not run all tests as a reflex.
2. Test edge cases: empty targets; same-label buttons; 1/26/27/100 hints; rapid backspace; Shift+Space pressed twice; foreground loss mid-hint; modal opened/closed; dynamic list changes; resize/reflow during a session; disabled control; repeated Enter; simultaneous existing hotkey focus; restored query text; plugin returning duplicate names.
3. Compare a small before/after instrumented observation of baseline frame work **with feature disabled** and enabled-but-idle, and hint-render responsiveness for a dense fixture. Any numbers must include run configuration and sample count. Only optimize if overhead is reproducible/material; don't promise exact historical perf parity.
4. Inspect for memory/state leaks, unbounded registries, frame allocations, duplicate per-widget subscriptions, runaway repaint and stale focus intents. Correct the architectural owner rather than adding sleeps, alternate global hooks or stale wrappers.
5. Verify Windows DPI/multi-monitor coordinate assumptions where practicable; ensure screenshot capture does not permanently take over the user's pointer or desktop. Test dismissal on focus loss.
6. Run `cargo fmt --check`, targeted Nextest filters, one appropriate `cargo check/build` of the affected binary/feature set and Clippy in the repository's established form. Address **new** warnings introduced by the feature; classify unrelated existing warnings separately.
7. A read-only reviewer should inspect actual diff and path-level ownership, with attention to action execution parity, test integrity, settings backward compatibility, process cleanup and active frame overhead. Fix only concrete high-/medium-priority findings within a bounded pass.

**Invariants / non-goals:** No broad compilation-performance effort, cache rewrite, general startup optimization, unrelated Clippy sweep, wholesale application refactor or historical test-campaign revival.

**Output / done:** Focused tests pass, no new Clippy lint warnings, no material avoidable idle cost, and identified defects addressed or honestly reported. Suggested commits `[M14-A] harden input and focus regressions`, `[M14-B] address targeted review findings` only as actually needed.

---

## P4 / M15 — Final acceptance, consolidated evidence and autonomous handoff

**Objective:** Deliver a clean, reviewable, uninterrupted implementation branch and an evidence-based final report that the user can examine whenever convenient.

**Depends on:** All previous milestones or documented blockers.

**Work:**

1. Ensure `JUMP_MODE_EXECUTION.md` matches actual milestone/checkpoint states and HEAD. List incomplete work explicitly; no hidden `pending` marked complete.
2. Ensure `JUMP_MODE_RESULTS.md` contains: mission summary; branch and base SHA; architecture map; list of touched modules; coverage by surface/control type; exact commands and pass/fail/skip; screenshot evidence index with full path; performance observations; unresolved issues; final source and status; next manual-review guidance.
3. Run the smallest final representative **end-to-end** GUI smoke covering dashboard selection+Enter, `note list` selection+Enter, one Settings target, one representative dialog, feature disabled, and cleanup. Do not rerun every previous screenshot series unless changed code invalidated its evidence.
4. Inspect `git status`, staged and unstaged diff, commit history, tracked/untracked evidence, temp/config leftovers; do not include user pre-existing dirty changes or huge screenshot directories in Git commits.
5. Verify retained local screenshots and clear links in results to the most recent passing run. If no GUI desktop was available, record `NOT_RUN_ENVIRONMENT` and never imply visual acceptance was verified; user may choose to run the one-command harness later, but Codex must not ask the user to intervene mid-session.
6. Commit documentation or final targeted fixes as a coherent checkpoint; try one normal best-effort push. Continue even if push fails. Never merge into the base branch.
7. Final response from Codex must say: **what works; what is incomplete; branch; commits; remote push vs local only; exactly where screenshots/results live; tests and commands; risks; one final manual review list**. The report must stand alone without the user reading the full chat history.

**Output / done:** Single feature branch, coherent checkpoint history, safe reproducible harness, comprehensive evidence, clear status and one final user-review handoff. Suggested commit `docs(jump): [M15-A] finalize acceptance and evidence index` if required.

---

# 9. Cross-milestone requirements traceability

Use this table to make sure the implementation does not “finish” while a foundational behavior was forgotten.

| Requirement / observable behavior | Primary milestone | Required proof |
|---|---|---|
| Disabled-by-default, legacy config | M02 | Rust tests + `18_feature_disabled` |
| Leader from root/dashboard/grid and text field | M05–M07 | Input tests + native harness |
| Text preserved, no inserted Shift+Space | M05, M07 | Query before/after trace + screenshot |
| Escape/second leader/Backspace/invalid | M05 | Pure/input tests + native cancel screenshots |
| A–Z, uppercase badges, case insensitive | M03, M05 | Unit tests + hint screenshots |
| Unique/prefix-free deterministic labels, >26 | M03 | Algorithm invariants + `13_dense_hints` |
| Stable hints during active session | M03–M04 | Session unit test + dynamic fixture |
| Hidden/disabled/clipped excluded | M04 | Registry tests + native scenario |
| Focus only, no execution | M04–M07 | Zero execution before Enter, exact once after |
| Grid and `note list` action cards | M06 | GUI `06`–`09` and action trace |
| Dashboard discrete actions/search | M07 | GUI `01`–`05`, `10` |
| Existing action semantics preserved | M06–M07/M11–M12 | Relevant old/new tests |
| Popups and background modal protection | M04, M10 | Modal unit tests + GUI `14` |
| Settings controls | M10 | GUI `15` + focus/Space tests |
| Notes/Todo/Timer/Paste Sequence dialogs | M11 | Dialog smoke + safe event traces |
| MkMacro visible list and common controls | M12 | `MkMacro` scenario + no playback/changes |
| Sliders/checkbox/dropdown/editor semantics | M12 | Native focus and activation assertions |
| Real app, screenshot, close, cleanup | M01, M08, M13 | PNG + manifest + owned PID cleanup JSON |
| Zero surprising idle feature-off overhead | M02, M14 | Disabled behavior test + sampled overhead |
| Coherent branch, small commits, push fallback | M00–M15 | Ledger, git history, status and push record |

# 10. Explicit technical questions Codex resolves autonomously

The product choices above are locked. These are *engineering investigation points*, not questions for the user:

1. What current egui version is used, and which event/focus methods can be reused without a Windows hook?
2. What exactly owns query autofocus and Enter execution? How can focused non-query targets keep the Enter key from being redirected to query or a stale grid selection?
3. Does the existing grid selection track index, action ID, or another stable identity? How are notes and plugin-produced cards represented?
4. What is the dashboard `Widget` render API and its event routing path? Can a small optional target collector be provided through dashboard context or the shared UI helper without touching every widget unnecessarily?
5. How do native child viewport/modal dialogs interact with `egui::Context` and focus? Does each viewport need a per-viewport Jump Mode state or an explicit active surface binding?
6. Can `src/gui/qa_framebuffer.rs` safely capture after Hint Mode transition, and what are its actual manifest constraints? Is a native presented-window screenshot feasible with existing test tools?
7. Can the test harness launch under a separate test configuration without colliding with single-instance behavior? If not, what is the narrowest **test-only** isolating mechanism?
8. Which existing process-containment script best handles GUI processes that normally exit on close? Reuse robust owned-PID cleanup, not unsafe `taskkill` by executable name.
9. Which settings screen and dialogs are implemented using normal egui widgets vs custom controls with logical focus? Choose the minimum adapters required.
10. What is the cheapest valid build/test route with the actual Cargo features and target binaries? Avoid inventing Cargo feature flags or passing non-existent CLI switches.

Record significant decisions once in the ledger/results. Do not use unresolved routine details as a reason to stop and ask for consent.

# 11. Nonfunctional acceptance and stop rules

- **Correctness over coverage claims:** A control without a proven focus-and-activate path must not display a misleading actionable hint. Either complete its integration or mark that control intentionally unsupported with a reason.
- **No auto-execution:** Any hint resolution that directly invokes a command is a correctness failure and must be fixed, even if screenshots look good.
- **No hidden user data mutation:** Fixture safety is a hard gate. The harness must fail safely if it cannot prove isolation.
- **No process kill by name:** No blanket termination of `Multi Launcher`, `multi_launcher.exe`, `WerFault.exe`, etc. Only proven owned PIDs/tree are eligible for cleanup, following existing containment rules.
- **No destructive “success” repair:** Do not use forced Git reset, force push, disabled tests or weakened assertions to achieve completion.
- **No fake screenshots:** If the environment cannot present a native GUI, save honest logs and continue what can be tested headlessly. Do not label a synthetic screenshot as application evidence.
- **No human checkpoints:** Do not ask the user to type keys, take screenshots, run commands, approve a branch, resolve a push failure, or manually verify between milestones.
- **No uncontrolled loops:** After bounded focused attempts on a genuinely blocked milestone, report the blocker in the ledger and work on independent next steps only. If a later milestone requires the blocker, mark its dependency blocked; do not silently drop the requirement or thrash.
- **No feature expansion:** Previously discussed optional product ideas (fuzzy matching, offscreen targeting, mnemonic labels, direct execution, arbitrary external UIA) remain for a separate future request; these are **not** the mandatory P3 modal/dialog integration phase above.

# 12. Final-review contract for the user

After Codex finishes, the user should be able to:

1. Check out/run the new feature branch and enable Experimental Jump Mode in Settings.
2. Press `Shift+Space` on the dashboard and see readable hints, type a target code, see **selection only**, then Enter to invoke a safe action.
3. Search `note list`, press the leader, jump to a note without rewriting the query, and Enter to open it through the established path.
4. Enter Settings and at least representative supported dialogs and navigate visible controls using the same flow.
5. Browse `dev/jump_mode/evidence/<run-id>/screenshots/` and `summary.md` to review actual screenshots captured automatically; view the small curated screenshot set under `docs/plans/jump_mode_review/` on the pushed task branch when available. Inspect the machine-readable pass/fail list without rerunning Codex.
6. Read `docs/plans/JUMP_MODE_RESULTS.md` for full status, commands, limitations, and next priorities.
7. Inspect the feature branch and checkpoint commits without any merge having been performed.

**End of CODEX GOAL.**
