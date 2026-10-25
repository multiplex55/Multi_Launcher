# Codex Goal 15 — Completed macro run summaries and practical starter recipes

**Companion execution prompt:** [CODEX_KICKSTART_15_MACRO_SUMMARIES_RECIPES.md](CODEX_KICKSTART_15_MACRO_SUMMARIES_RECIPES.md)  
**Requested coverage:** #58, #59  
**Program position:** 15 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/macro-summaries-recipes` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish macro-summaries-recipes branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
6. **Attempt the initial upstream push once.** After the baseline commit exists, perform one normal non-force push to this task branch when an appropriate remote and platform approvals permit, for example `git push -u origin HEAD` when `origin` is verified as the intended repository. If denied/unavailable, record the real reason and keep working locally. Do not retry in a loop, bypass protections, or require the user to resolve ordinary push failure.
7. **Verify the bootstrap and remain on this branch.** Record branch, original base, baseline SHA, pre-existing work and initial push result in the execution ledger. A ledger created by the baseline commit can record its resulting SHA immediately afterward and include that update in the next legitimate checkpoint; do not create recursive commits merely to record their own hashes. Do not silently return to the prior branch.

### M0-A acceptance and commit contract

**Objective:** A unique active task branch with its own committed baseline and an honest initial push result.

**Code ownership:** Git state and an optional task execution ledger only; no application code or test execution yet.

**Tasks:** Complete steps 1–7 above, check the dependency contract below read-only, and keep existing user work untouched.

**Behavior after this checkpoint:** All later feature work and verification happen on the new branch and are measured against the new baseline commit.

**Edge cases:** Missing remote/upstream, denied fetch/push, existing branch name, dirty checkout, stale local integration tip, missing prerequisite or unsafe transition.

**Regression constraints:** No user change staged/lost, no destructive Git command, no force push/history rewrite, and no implementation on `master`, `main` or the old feature branch.

**Verification:** Inspect branch/HEAD, baseline commit, worktree/staged diff, base provenance and actual push output. No application build is needed to prove bootstrap.

**Acceptance:** Unique branch is active; baseline commit exists on it; original base and baseline SHA are recorded; push was attempted or its policy/access unavailability is recorded; unrelated changes are preserved. If branch/baseline cannot be established safely, stop with a precise blocker. Push failure alone is nonblocking.

**Commit boundary:** The first commit is the M0-A baseline. Every later checkpoint stays on this new branch; normal best-effort push follows each commit.


## A. Goal and desired outcome

After a macro completes, fails or is stopped, show an understandable bounded summary of what ran and where elapsed time went. Provide useful starter macros through the existing template workflow so users can create independent editable examples without automatically executing them or assigning shortcuts.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md), [11 — Configure Jump Mode activation and ergonomic hint letters](CODEX_GOAL_11_JUMP_CONFIGURATION.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/mkmacro/runtime.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/mkmacro/runtime.rs)** — `RuntimeSnapshot; CompletedStep; apply_execution_event; run_one`. Snapshots already contain run identity, terminal state, start/finish times, root/callee step maps, last completion and diagnostics. These are current-run projections; they do not provide a bounded history with per-step elapsed aggregates.
- **[src/mkmacro/executor.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/mkmacro/executor.rs)** — `ExecutionEvent; RunControl; StepStarted/StepFinished/StepFailed; frame events`. ExecutionEvent declares step/frame events and Paused/Resumed variants, but the reviewed production paths do not emit those pause variants. Manual control transitions and breakpoint pause must be instrumented at their authoritative owner. Repeated calls/loops also need occurrence identity.
- **[src/gui/mkmacro_dialog/runtime_inspector.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mkmacro_dialog/runtime_inspector.rs)** — `Runtime Inspector rendering and debug value formatting`. An existing inspector displays state/variables; normal execution and clipboard-enabled debug redaction have deliberate privacy boundaries.
- **[src/mkmacro/templates.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/mkmacro/templates.rs)** — `MkMacroTemplateCatalog; load_template_catalog; plan_template_instantiation`. Templates are on-demand portable independent packages. Instantiation remaps identities and clears all copied macro hotkeys.
- **[src/gui/mkmacro_dialog/package_ui.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mkmacro_dialog/package_ui.rs)** — `PackageModal::Templates; open_templates; Preview Copy`. The UI already has New from Template and preview-copy flows; built-in recipes should use this entry point.
- **[src/mkmacro/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/mkmacro/model.rs)** — `MkAction; MkProcessPayload; MkParameter; WaitUntil`. Typed text, process, window, wait, prompt/parameter and launcher actions already support useful examples without a new automation language.
- **[src/mkmacro/executor/frame.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/mkmacro/executor/frame.rs)** — `enter_instruction; instruction repeat/retry phases`. StepStarted/StepFinished describe instruction visits; repeated action attempts can occur inside one visit. Breakpoint entry pauses the control directly before publishing BreakpointHit. Summary counts and pause durations must follow these actual semantics.

## C. Approved functional requirements

1. Publish one terminal summary for each admitted root run, including normal, debug, partial/from-selection, hotkey, launcher and Hot Folder origins. Keep runtime ownership and external-run receipts unchanged; classify preview runs visibly so they cannot be mistaken for real stored playback.
2. Show macro name/identity at admission, run ID, real invocation source and mode, partial/from-selection descriptor, completion/stopped/failure status, wall-clock timestamps for display, monotonic admitted-to-terminal elapsed duration, instruction-visit executed/skipped/failed counts and a safe failure location. Capture admission time before preparation and distinguish optional execution-start/preparation duration so pre-start cancellation and compilation failure still have valid timing.
3. Aggregate instruction-visit counts and elapsed duration by macro plus step while respecting frame instances and loops. A visit may contain repeats/retries; label counts as visits unless actual attempt events are separately recorded. Never infer attempted/successful action counts by multiplying repeat settings. Show slowest retained steps and action categories including waits; elapsed duration is not CPU time.
4. Measure user/debug pause time at authoritative RunControl/runtime/manual/breakpoint transitions; the declared but un-emitted Paused/Resumed variants are insufficient. Label caller timings inclusive of nested calls and do not sum overlapping caller/callee durations as total run time. Handle panic, failed preparation and pre-start cancellation without inventing execution start or success.
5. Retain session-only history up to 100 runs with a total metadata budget of 4 MiB and at most 1024 per-step aggregate rows per retained run. Evict oldest completed summaries as needed. If detailed aggregation is capped, display omitted/truncated detail counts and call the slow-step list slowest retained steps. Keep total run counters accurate.
6. Expose history/summary from the existing MkMacro dialog with filtering by macro/outcome, inspect details and copy a concise metadata-only report. Do not persist history across restarts in this goal.
7. Do not collect text typed, clipboard contents, prompt answers, screenshots, OCR output, variable values or full command arguments in summaries. Preserve clipboard-enabled redaction for error details and avoid leaking payload-bearing diagnostics through the new copy/report path.
8. Add at least three carefully documented built-in recipes using current actions: parameterized form filling into an explicitly selected target; open a chosen file/folder with existing typed process/launcher actions; wait for a chosen application state with timeout and clear success/failure feedback.
9. Recipes remain read-only catalog definitions until Preview Copy/instantiate creates a normal independent draft. No automatic run, shortcut registration, destructive file handling or hard-coded user paths. All copied hotkeys remain cleared through the current import mechanism.
10. Each recipe explains purpose, required target/inputs, which fields to customize, expected outcome and cancellation/timeout behavior. Validate package versions, stable recipe IDs and all referenced steps/assets. Existing user templates, saved macros and import semantics remain unchanged.
11. RuntimeOrigin currently distinguishes Stored from RecordingPreview only. Thread a small truthful invocation-source and subset descriptor from toolbar, launcher, hotkey and Hot Folder entry owners into admission; preserve preview tickets and external owned-run identity. An unknown API caller must be labeled unclassified, not guessed from current foreground state.
12. Use a bounded diagnostic whitelist for history/copy: failure kind, fixed safe reason, macro/step IDs and action category. The existing failure presenter clones full context and is not safe to reuse for history. Test secret sentinel payloads in ordinary non-clipboard failures as well as clipboard-enabled failures.
13. Built-in recipes must use capabilities that actually interpolate today: retain a fixed supported executable/launcher action and parameterize validated arguments. Window matchers and executable program fields are not currently interpolated; describe required target customization in the copied draft instead of promising unsupported dynamic targets.
14. Enforce aggregate byte/row/string limits while the active summary is being accumulated, not just after terminal publication. Run success is determined by the authoritative terminal result; a StepFinished event alone cannot establish successful playback when later control-flow validation fails.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Data-driven batch macro execution, expression language or conditional breakpoint features.
- Persistent execution audit logs, raw input recording or analytics upload.
- A separate template store/browser bypassing current package planning.
- New process/file primitives solely to make a recipe more elaborate.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Accumulate bounded timing/summary metadata in the existing runtime event owner using a monotonic clock abstraction for tests. Publish immutable terminal records; do not make GUI frames the timekeeper.
2. Maintain occurrence timing until each frame/step completes, then fold it into bounded aggregates. Label nested inclusive timing and preserve skipped versus executed semantics.
3. Extend the existing template picker to expose built-in examples alongside user templates. Reuse plan_template_instantiation and its identity/hotkey safety rather than hand-copying structures.
4. Keep recipes and run summaries as separate coherent checkpoints within one MkMacro productivity goal; neither should alter core playback semantics.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Capture bounded terminal run summaries

**Objective:** Create truthful terminal and timing data without changing execution behavior.

**Code ownership:**

- src/mkmacro/runtime.rs
- src/mkmacro/executor.rs
- proposed bounded summary types near runtime owner
- src/mkmacro/executor/frame.rs
- actual invocation/hotkey/command/Hot Folder entry owners

**Implementation tasks:**

1. Define immutable summary/aggregate types and clock seam; trace the real event ordering for frames, calls, waits, loops, pause and cancellation.
2. Accumulate occurrence timings and total counters at the runtime owner; publish exactly one summary at the admitted root terminal boundary, including failed preparation.
3. Enforce run/detail/byte bounds, redact payload-bearing diagnostics and expose dropped-detail metadata.
4. Capture a small source/subset descriptor and timing origin at admission; cover preparation failure and pre-start stop. Instrument real pause transitions and label instruction visits separately from action repeat/retry attempts. Do not rely solely on unused pause event variants.

**Behavioral requirements:**

- Every admitted terminal run has an inspectable metadata summary with well-defined durations.

**Edge cases:**

- Same step repeats, recursive/repeated callee, stopped step, panic, clock wall-time change, failed preparation, zero executed steps.

**Regression constraints:**

- Input release, runtime state, normal/debug variable privacy and Hot Folder completion receipts retain existing semantics.

**Verification:**

- Injected-clock event-stream tests for success/failure/stop and nested timing; bounded-history and redaction tests.
- Test pre-start stop, compile failure, manual and breakpoint pauses, repeat/retry within one instruction visit, and sensitive non-clipboard diagnostic sentinels.

**Acceptance criteria:**

- Exactly-once terminal publication, correct counts/timing labels and hard metadata caps pass without storing user payloads.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(mkmacro): [M1-A] capture bounded run summaries` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Present completed runs in the existing macro UI

**Objective:** Make summaries useful without replacing the live runtime inspector.

**Code ownership:**

- src/gui/mkmacro_dialog/runtime_inspector.rs
- src/gui/mkmacro_dialog/mod.rs
- src/gui/mkmacro_dialog/toolbar.rs

**Implementation tasks:**

1. Add a completed-run view/selector near the inspector, with status, duration, counts, slowest retained steps and concise copied report.
2. Keep current live run identity separate from selected historical summary, and preserve a summary when its macro/step is edited or deleted later.
3. Add keyboard/Jump controls and clear-history action; cap formatted text and update only on summary revisions.

**Behavioral requirements:**

- Users can understand completed, failed and cancelled runs without exposing runtime variable values.

**Edge cases:**

- No history, same macro run twice, deleted macro, truncated detail, historical selection while a new run starts.

**Regression constraints:**

- Normal playback remains normal; inspector debug state is not enabled merely to produce summaries.

**Verification:**

- Focused summary-view tests and one generated native run/stop/failure screenshot sequence.

**Acceptance criteria:**

- Displayed data matches summary ownership and copied report respects privacy/caps; UI does not poll unnecessarily.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(mkmacro): [M1-B] show completed run results` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Provide starter recipes through template copies

**Objective:** Ship useful validated examples using existing package semantics.

**Code ownership:**

- src/mkmacro/templates.rs
- src/gui/mkmacro_dialog/package_ui.rs
- src/mkmacro/model.rs as reference
- proposed checked-in built-in recipe data

**Implementation tasks:**

1. Author the three required examples with typed parameters, finite waits and customization descriptions; inspect current action support rather than inventing variants.
2. Add a built-in source section/filter to the current template picker with preview and independent copy semantics.
3. Validate recipes using current package/compiler/import validation, ensure stable built-in identity and fresh copied identities, clear all copied shortcuts.
4. Keep executable names and target matchers compatible with current interpolation support; require target edits in the independent copy where needed, and test argument quoting without invoking a shell.

**Behavioral requirements:**

- Selecting a recipe creates an editable independent draft only after preview confirmation; no automation runs on import.

**Edge cases:**

- User template with same name, unsupported future recipe version, missing asset, repeated copies, path with spaces/quotes, missing target window.

**Regression constraints:**

- User template persistence and macro IDs/hotkeys remain untouched.

**Verification:**

- Package validation/import tests for all recipes; mock-backend execution of intended success and timeout paths; one native copy/customize/run example using disposable targets.

**Acceptance criteria:**

- All examples validate, copy safely, explain prerequisites and demonstrate the stated workflow without destructive effects.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(mkmacro): [M2-A] add practical built-in recipes` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify shared macro workflows and document usage

**Objective:** Check summaries and recipes through actual existing entry points.

**Code ownership:**

- affected macro tests
- existing native macro harness
- MkMacro help

**Implementation tasks:**

1. Exercise toolbar, direct invocation and an external-owned run with the same summary boundary; verify terminal status appears once.
2. Run the small recipe/summary native smoke, inspect screenshots and restore disposable targets.
3. Update the relevant help with session history limits, timing interpretation and recipe customization.

**Behavioral requirements:**

- The two additions are discoverable and consistent across macro entry paths.

**Edge cases:**

- Close/reopen dialog during playback, app shutdown, cancelled preview and an external run stopped by its owner.

**Regression constraints:**

- No new global hotkeys, no payload persistence and no weakening of existing runtime tests.

**Verification:**

- Targeted runtime/template/dialog selection only; final first-party lint for changed targets.

**Acceptance criteria:**

- Requested summary and recipe workflows pass with bounded evidence; known environment limits are reported.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(mkmacro): [M2-B] verify summaries and starter workflows` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use deterministic fake time/event tests for elapsed accounting; reserve native validation for actual UI/target/shortcut ownership.
- Keep native examples confined to generated local windows and disposable files. One smoke execution budget of ten minutes, separate from compile time.
- At most two focused remediation attempts per failing owner before a precise blocker. Do not run all macro recording/image/OCR suites unless a changed boundary actually affects them.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Nested call times are double-counted or loops overwrite prior occurrences. | Track frame occurrences and explicitly label inclusive durations; fold counts only at terminal boundaries. |
| Summary diagnostics bypass existing clipboard privacy. | Use safe metadata-only fields and test redaction of failure/copy paths. |
| Recipe import registers hotkeys or overwrites user templates. | Use current preview-copy/identity remapping with cleared copied hotkeys and separate built-in source IDs. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- All required run outcomes and entry origins produce one bounded summary.
- Timing, truncation and privacy semantics are explicit and tested.
- Three practical recipes validate and instantiate independently with useful help.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Summary field/timing/privacy contract and memory bounds.
- Recipe list, customizable inputs and validation/native outcomes.
- Screenshots of completed, failed/stopped and recipe-preview UI.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
