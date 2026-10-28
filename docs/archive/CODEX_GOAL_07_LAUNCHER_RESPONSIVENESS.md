# Codex Goal 07 — Launcher readiness, large-result display, and startup responsiveness

**Companion execution prompt:** [CODEX_KICKSTART_07_LAUNCHER_RESPONSIVENESS.md](CODEX_KICKSTART_07_LAUNCHER_RESPONSIVENESS.md)  
**Requested coverage:** #111, #112, #114  
**Program position:** 07 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/launcher-responsiveness` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish launcher-responsiveness branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Reduce avoidable work between launcher invocation and a usable query field, smooth first display and geometry changes of large results, and reach useful input sooner with large personal datasets. Preserve exact focus/visibility behavior and use matched measurements rather than source-size guesses.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [03 — Measured compilation boundaries, dependency cleanup, and test target consolidation](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md), [04 — Faster Windows linking and responsive Neovim rust-analyzer workflows](CODEX_GOAL_04_LINK_EDITOR.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/visibility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/visibility.rs)** — `VisibilityRevision; complete_first_usable_frame_at; RootWindowBridge`. Existing hotkey metrics correlate visibility revision, invocation and a retained painted result generation; they describe CPU-side usability, not compositor presentation or proof that a native typed character reached the query.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `pending_show_query_focus_*; query_focus_session_*; root list geometry; run_runtime_tick`. A substantial focus ownership model and virtualized root list already exist. Recent note/Jump Mode and show-query focus protections must not be replaced by unconditional focus requests.
- **[src/gui/track_c_query_observer.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/track_c_query_observer.rs)** — `Observer; Receipt; GeometryWork`. Bounded opt-in query receipts already distinguish completed usable frames from abandoned or superseded intent.
- **[src/gui/search.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/search.rs)** — `LauncherApp search and result publication`. Search owns query generations, caching and result updates; investigate measured allocation and invalidation paths here before moving work to an unrelated parallel system.
- **[src/gui/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mod.rs)** — `LauncherApp::new; startup timer phases`. Construction measures dashboard, watcher, macro, action/filter cache, command cache and completion-index phases and initializes many data owners.
- **[src/main.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/main.rs)** — `spawn_gui; GuiStartupServices; startup_index_coordinator_for_owner`. GUI/service startup and acknowledged indexing ownership already have dedicated boundaries.

## C. Approved functional requirements

1. Define and measure three distinct experiences: process startup to useful input, hotkey invocation to accepted query typing on an eligible launcher surface, and query/geometry change to a retained usable CPU frame. Label actual native presentation evidence separately.
2. Warm launcher show must focus the query according to the current ownership policy, including passive notes and Jump Mode. Existing active modal/editor/native child owners continue to retain input; no unconditional focus steal or injected-key workaround is allowed.
3. Cover first query, first large result display, list/grid switch, resize, font-size/DPI changes and returning to an existing result list. Preserve ranking, result identity, selected entry, scroll anchoring, click/hint geometry and Enter activation.
4. Use deterministic synthetic personal datasets containing many actions, notes, snippets and history entries; do not modify or copy the user’s real data to make performance fixtures.
5. Reuse existing virtualization, immutable catalogs and query generations. Optimize measured repeated layout, cloning, filtering, metadata construction or cache invalidation; do not present file splitting alone as a speedup.
6. If deferring startup work, retain a truthful loading state and prevent stale or incomplete catalogs from being treated as final. Core invocation/input and essential settings must be valid before reporting readiness.
7. An operation superseded by typing, hiding, a new invocation or data revision must not publish stale results or complete a latency sample for the replacement operation.
8. Keep production metrics opt-in and bounded. Benchmark overhead and discard-state events must not change the user-visible timing path.
9. Freeze workload, build profile, data hashes and measurement definitions before candidates; report distributions and actual work counts. A measured CPU-frame improvement does not establish native keyboard or GPU latency improvement.
10. Address each of the three requested areas with a measured candidate where an avoidable cost exists. A measured no-change disposition is allowed for an already efficient path, but it must be explicit and must not be reported as a new speedup.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Changing search ranking, scope behavior, command syntax, default themes or data formats.
- Replacing egui, changing window ownership policy, or redesigning the plugin system.
- General crate/module reorganization already owned by the compilation goals.
- Unrelated features, full-suite qualification campaigns or speculative background preload.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Keep latency ownership with the existing visibility and query observers. Add only missing timestamps/receipts needed to distinguish focus acceptance from CPU layout.
2. Select one coherent hotspot per requested path before edits. Preserve existing independent eager-layout reference coverage and use the improved fixtures if already integrated.
3. Treat startup as readiness phases: essential settings/ownership first, safely deferred optional catalogs second. Any deferred publisher must be generation-bound and wake the correct owner once.
4. Use existing synthetic CPU and native harness entry points; avoid adding a second application startup or input simulator.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Freeze workloads and readiness definitions

**Objective:** Establish trustworthy baselines for the three requested paths.

**Code ownership:**

- src/performance/track_c.rs
- src/gui/track_c_query_observer.rs
- src/visibility.rs
- existing perf/native harnesses

**Implementation tasks:**

1. Inspect current metrics and identify exactly where query focus becomes eligible and where native typing is accepted.
2. Create small/default/large deterministic dataset recipes and capture cold-start, warm-show, first-list, resize and grid-switch baselines.
3. Record CPU-frame metrics separately from native visibility/focus receipts; identify the first responsible hotspot for each path.

**Behavioral requirements:**

- No behavioral optimization until baseline definitions and workload provenance are fixed.

**Edge cases:**

- Superseded invocation, hidden root, modal active, interrupted startup and unavailable interactive desktop.

**Regression constraints:**

- Never count dropped/superseded requests as successful low-latency samples.

**Verification:**

- One baseline batch; up to three samples per comparable scenario only if needed to resolve variance.

**Acceptance criteria:**

- Every requested area has a reproducible baseline and a documented candidate owner or evidence-backed no-change finding.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(launcher): [M1-A] baseline readiness and large-result workloads` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Improve invocation-to-query readiness

**Objective:** Remove measured work on the warm show/typing path while preserving focus ownership.

**Code ownership:**

- src/visibility.rs
- src/gui/render.rs
- src/gui/mod.rs
- src/window_activation.rs
- src/main.rs only if owner evidence points there

**Implementation tasks:**

1. Apply the narrow owner-level change identified by M1-A; reuse visibility/query generation receipts.
2. Preserve show/hide ordering, foreground ownership, current query text, passive-note behavior and modal ownership.
3. Keep hotkey, UI/menu and existing command entry paths equivalent where they share the same activation owner.

**Behavioral requirements:**

- Eligible warm show accepts typing promptly and exactly once; modal input remains with its owner.

**Edge cases:**

- Rapid show-hide-show, held hotkey, focus stolen by another app, note editor ownership, pending modal, alternate Jump chord after integration.

**Regression constraints:**

- No synthetic focus/key injection, delayed stray refocus, or hidden-query mutation.

**Verification:**

- Target focus/visibility tests including passive notes and Jump enabled/disabled; one owned native hotkey/typing smoke.

**Acceptance criteria:**

- Matched measurements improve the targeted work beyond observed noise or demonstrate the path already meets the frozen budget; focus/typing evidence is separate and passing.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(launcher): [M1-B] shorten eligible query activation` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Smooth first display and geometry changes

**Objective:** Eliminate measured redundant large-list/grid preparation without changing result semantics.

**Code ownership:**

- src/gui/render.rs
- src/gui/search.rs
- root list/grid geometry owner and directly related tests

**Implementation tasks:**

1. Reduce measured repeated row metadata/layout/cloning work and make cache invalidation depend on the actual font, width, query and catalog revisions.
2. Keep selection and hit-test geometry synchronized with retained painted results across resize and mode changes.
3. Retain a representative eager reference calculation and meaningful cold/warm/large-list behavior coverage.

**Behavioral requirements:**

- Large result display and geometry updates remain responsive with consistent keyboard/mouse targets.

**Edge cases:**

- Empty/single result, very long Unicode labels, selected off-screen entry, changing columns, font/DPI revision mid-query.

**Regression constraints:**

- Ranking, activation, row identity, scroll and Jump target ownership stay correct.

**Verification:**

- Relevant root-list/grid oracle recipes plus matched 1k/10k synthetic result samples; do not rerun all historical oracle matrices.

**Acceptance criteria:**

- Geometry and navigation references agree; measured work reduction is attributed to the changed owner.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(results): [M2-A] reduce first-display and geometry work` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Improve startup with large personal catalogs

**Objective:** Shorten measured startup readiness while retaining correct initial data publication.

**Code ownership:**

- src/gui/mod.rs LauncherApp::new
- src/main.rs GuiStartupServices/spawn_gui
- actual catalog/store owners implicated by measurements

**Implementation tasks:**

1. Reduce duplicate loading/copying or defer clearly optional work only where M1-A proves it blocks readiness.
2. Publish ready/loading/error states through existing owners and reject completions from retired startup generations.
3. Update developer measurement notes and run a single matched final workload set; include a concise native startup screenshot/typing receipt.

**Behavioral requirements:**

- The launcher reaches meaningful input sooner; asynchronously completed catalogs update safely without losing query intent.

**Edge cases:**

- Large corrupt file, missing directory, load failure, first query before catalog publication, shutdown during deferred work.

**Regression constraints:**

- Persistence recovery, existing defaults, startup registration and core command availability remain intact.

**Verification:**

- Targeted initial-catalog and cancellation tests plus one synthetic large-profile startup/native smoke.

**Acceptance criteria:**

- All three request areas have measured final outcomes; catalog correctness and query usability pass; no unbounded new worker or cache exists.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(startup): [M2-B] reduce large-catalog readiness work` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use the same build profile and fixture hashes for baseline/candidate; do not mix release and debug timings.
- Cap performance exploration to the three requested paths, one candidate batch per checkpoint and at most three matched repeats for variance. Stop optional profiling once the selected invariant and effect are established.
- Native smoke budget: one normal desktop profile and one large synthetic profile, bounded by the owned-process wrapper; compilation deadline is separate from the execution deadline.
- Run the selected visibility/query/layout/startup tests, then an appropriate first-party Clippy pass at the end. A missing native session must be reported as a verification blocker, not simulated as proof.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Earlier timestamps manufacture an apparent readiness improvement. | Freeze readiness definition and require accepted native typing evidence separately from CPU receipts. |
| Cached layout survives an invalid font/DPI/width revision. | Use actual cache keys and retain geometry/reference tests. |
| Deferred startup loses initial catalog or query intent. | Explicit loading state, generation-bound publication and first-query-before-ready tests. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Hotkey readiness, large-result display and startup each have a specific measured disposition.
- Existing focus and navigation invariants hold with Jump enabled and disabled.
- Any no-change or environment-blocked result is explicit; no unmeasured speed claim appears.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Matched workload and timing/work-count table for all three request areas.
- The readiness definition and native-vs-CPU evidence distinction.
- Changed cache/loading ownership and directly affected regression evidence.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
