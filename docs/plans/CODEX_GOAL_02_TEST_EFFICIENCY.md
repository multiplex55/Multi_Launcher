# Codex Goal 02 — Faster isolated tests and efficient GUI reference coverage

**Companion execution prompt:** [CODEX_KICKSTART_02_TEST_EFFICIENCY.md](CODEX_KICKSTART_02_TEST_EFFICIENCY.md)  
**Requested coverage:** #128, #129, #130, #131, #132, #138  
**Program position:** 02 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/isolated-test-fixtures` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish test-efficiency branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Make representative small tests and expensive GUI reference tests finish sooner and more reliably by removing irrelevant application startup, shared-data interference and redundant work. Preserve the real behavior each test protects, retain meaningful large-list and Windows integration coverage, and report separately the effect on compilation, fixture setup, execution and teardown.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[tests/dashboard_config.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/tests/dashboard_config.rs)** — `new_app, activation_applies_query_override_first, should_show_dashboard_when_empty_query`. The helper constructs the full LauncherApp using PluginManager::new, relative actions/settings paths and unchanged Settings::default. Two small behavioral tests therefore enter broad startup paths.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `new_app_with_settings at 11183; root_list_test_app/root_list_app_at at 12783–12844`. One helper uses broad production-oriented defaults, while the root-list fixture already uses isolated paths, an inert manager, disabled shortcuts and a deliberately dormant unrelated dashboard refresh. The patterns are inconsistent.
- **[src/plugin.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/plugin.rs)** — `PluginManager::new, new_inert_for_test and new_inert_for_test_at, lines 600–644`. The ordinary constructor installs the process-global macro runtime. Inert constructors are pub(crate) and cfg(test); the parameterless helper still opens the current directory, whereas the explicit-root variant uses an empty store. External integration tests cannot simply call these cfg(test) library helpers.
- **[src/gui/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mod.rs)** — `LauncherApp::new, lines 2897–3313`. App construction loads state and constructs dashboard, watchers, editors and runtime owners. Conditional test code does not make the full constructor service-free.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `track_c_eager_root_list_layout, track_c_oracle_root_grid_frame_matches_frozen_eager_geometry, track_c_prepare_root_query_state, track_c2g_oracle_query_sequences_match_usable_cpu_frames`. Reference tests perform real eager egui layout, repeated cold/warm setup and settling frames. The query-sequence oracle also includes 1,000 and 10,000 item viewport cases. Their cost is a measurement candidate, not an established timing result.
- **[tests/clipboard_persistence.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/tests/clipboard_persistence.rs)** — `TEST_MUTEX and history tests`. The tests use a process-local mutex, change CWD and mutate an environment variable. They deliberately avoid the actual clipboard, so they should not be classified as native clipboard tests merely from their names.
- **[tests/watchers.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/tests/watchers.rs)** — `new_app and watcher event tests`. These tests construct real watchers and already use finite event-receive timeouts. Their real-event assertions must be retained rather than replaced by fake notifications.
- **[AGENTS.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/AGENTS.md)** — `Testing Standard and scope-proportionate verification`. Every app test/profile must disable Paste Sequence next/cancel shortcuts before construction. Meaningful tests must not be disabled or weakened to get a passing result, and targeted Nextest is preferred.

## C. Approved functional requirements

1. Baseline a small representative selection containing the two dashboard app tests, one ordinary GUI helper consumer, the affected eager-reference oracle cases and one real watcher/native resource case. Record setup, execution and teardown evidence separately when practical; do not infer bottlenecks from file length alone.
2. Provide an explicit-root, test-only fixture for internal unit tests with deterministic data/settings and only the service owners required by each test. Disable next_hotkey and cancel_hotkey before constructing any app test profile. Preserve production defaults and error reporting.
3. Keep test-only constructors out of the ordinary public production API. Move truly unit-level cases into an owning cfg(test) module when that is the appropriate boundary; genuine cross-crate integration tests continue using the ordinary library with explicit temporary inputs and required real services. Preserve a map of moved test identities and update integrated feature selectors.
4. Use scoped restoration for CWD, environment and process-global test state wherever those APIs cannot yet accept explicit roots. Dispose app/background owners and verify required shutdown before restoring global paths or deleting temporary data.
5. Replace unnecessary sleeps/polling with observable completion, existing event channels, explicit barriers or a locally injectable time input. Retain finite deadlines and real operating-system event checks for tests whose purpose is native timing or watcher delivery.
6. Optimize GUI reference work without deriving expected values from the production algorithm or caching mutable GUI state across unrelated tests. Preserve cold/warm cache invalidation, resize/font/scale changes, focus/navigation/selection and meaningful large-list behavior covered by the original selection.
7. Coordinate only tests that actually compete for a shared Windows resource. Use installed-version-supported Nextest test groups or an appropriate test-only cross-process owner; process-local mutexes alone do not coordinate Nextest processes. Distinguish within-run coordination from independent runner invocations and from the personal running application.
8. Remove duplicated tests/work only after showing that retained behavioral cases cover the same invariant. Maintain default discovery of required correctness coverage; no blanket ignores, reduced assertions, retries-to-green, global serialization or whole-suite timeout inflation.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No application-wide architecture rewrite, broad relocation of inline tests, complete test-framework replacement, or alteration of feature semantics.
- No global error-toast suppression, permanent disabling of watcher/hook/clipboard tests, removal of meaningful large-list cases, or addition of arbitrary sleeps to hide races.
- No dependency/linker/editor tuning or additional user-facing features in this goal. Compilation effects may be recorded but belong to the following optimization goals.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Use current fixtures as the starting point and make the required ownership explicit. Prefer new_inert_for_test_at over its current-directory variant for unit tests. Isolate data before constructors run, including settings, notes/templates and any stores that still resolve relative paths.
2. Introduce the smallest test-support boundary needed, proposed as a cfg(test) GUI support module if no appropriate owner already exists. Service activation must be intentional for a test; ordinary production constructors keep their current behavior.
3. Separate reusable immutable fixture generation from per-case mutable GUI/context state. Share immutable input where safe, retain independent reference calculations, and stop settling frames when a tested readiness condition is reached within a finite frame/deadline budget.
4. Classify resource use from code, not test names. Preserve in-process guards for cargo test compatibility where relevant, add cross-process coordination for Nextest only where necessary, and avoid grouping pure model tests with native desktop work.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Introduce explicit isolated app fixtures and migrate small tests

**Objective:** Small behavior tests construct only the relevant state and cannot accidentally use surrounding user/repository data.

**Code ownership:**

- src/gui/mod.rs test construction seams
- src/gui/render.rs test helpers
- src/gui/command_host.rs test helpers
- src/plugin.rs test-only constructors
- tests/dashboard_config.rs
- Proposed cfg(test) GUI fixture support module

**Implementation tasks:**

1. Capture the bounded representative baseline and list the actual services/data roots each selected fixture touches before changing it.
2. Build an explicit-root fixture around existing inert constructors; disable paste shortcuts before construction and make real watchers/runtime owners opt-in for tests that exercise them.
3. Migrate the two small dashboard app cases and a representative generic render-helper consumer. Keep true integration cases at their proper boundary; do not expose cfg(test) internals as an unconditional production API.
4. Add scoped ownership for temporary data and unavoidable CWD/environment changes, ensuring app/service teardown precedes path restoration and directory removal.
5. Update any integrated feature-test catalog when names/targets move and record the old-to-new case mapping.

**Behavioral requirements:**

- Small fixtures start without unrelated macro hotkey pollers or user-data loads; tests that need a real service request it explicitly.

**Edge cases:**

- Constructor failure, test panic, missing/corrupt synthetic files, CWD restoration failure, background callback retaining a fixture path, integration library without cfg(test).

**Regression constraints:**

- Production settings/hotkey defaults and ordinary constructor behavior remain unchanged; error-notification tests retain real reporting semantics.

**Verification:**

- Run only migrated cases and the fixture's lifecycle checks; prove two independent temporary profiles do not share persisted data and all selected app profiles disable next/cancel shortcuts before construction.

**Acceptance criteria:**

- Migrated behavioral assertions pass, required test identities remain discoverable, fixtures never touch uncontrolled stores, and setup/teardown evidence identifies any remaining real service cost.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `refactor(test): [M1-A] isolate small app fixtures and service ownership` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Replace unnecessary waits with explicit completion

**Objective:** Selected timing-dependent tests finish when the expected event occurs while preserving their actual timing/delivery invariants.

**Code ownership:**

- Affected tests in src/gui/mod.rs and src/gui/render.rs
- tests/watchers.rs
- Existing event-channel and test-clock seams located at execution time

**Implementation tasks:**

1. Use baseline evidence to select a bounded set of slow or flaky waiting paths; classify each wait as modeled time, task completion, genuine OS delivery or redundant settling.
2. Replace modeled elapsed-time waits with a local explicit time input where appropriate and task waits with existing completion channels/barriers; avoid a new application-wide clock framework.
3. Keep real watcher delivery tests on actual filesystem events with finite receive deadlines. Ensure cancellation/shutdown can release a waiting fixture without waiting for its full timeout.
4. Record the event/readiness condition and finite deadline in failure diagnostics instead of silently sleeping or spinning longer.

**Behavioral requirements:**

- Tests complete promptly on success and explain which expected event failed to arrive on timeout.

**Edge cases:**

- Completion before the waiter subscribes, dropped sender, worker panic, cancellation racing completion, unrelated queued event, busy Windows scheduling.

**Regression constraints:**

- Tests intended to prove native delivery still use the native event path; production delays and debounce behavior remain unchanged.

**Verification:**

- Run the changed wait cases plus one timeout/cancellation path; repeat only a previously nondeterministic case up to three times when needed to resolve a concrete remaining race.

**Acceptance criteria:**

- The selected waits no longer pay unnecessary wall-clock delay, cannot hang indefinitely, and still fail when the required event is intentionally absent.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(test): [M1-B] use bounded completion signals in selected tests` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Reduce repeated GUI oracle construction and duplicated proof

**Objective:** The expensive selected GUI correctness tests retain independent reference coverage with less repeated setup and layout work.

**Code ownership:**

- src/gui/render.rs eager-layout helpers and Track C correctness tests
- Existing performance workload fixtures only as required by those tests

**Implementation tasks:**

1. Measure the chosen cold/warm list, grid and query-sequence tests with the isolated fixture; identify repeated construction, fixture generation, settling and reference layout that proves the same state more than once.
2. Reuse immutable inputs and invariant reference calculations only within a safe scenario lifetime; rebuild every value whose dependencies change. Keep fresh mutable context/state for cases where independence matters.
3. Replace fixed settling loops only when an explicit observed state establishes readiness. Preserve failure diagnostics when readiness is not reached within the finite budget.
4. Create a concise coverage mapping for retained cases: cold/warm results, columns, resize/font/scale invalidation, focus/navigation/selection and the existing large-list scenario. Do not substitute hashes/counts for the behavioral oracle they previously supplemented.
5. Remove redundant work or duplicate cases only when the retained independent assertions prove the original invariant; keep benchmark-only sampling separate from ordinary correctness checks.

**Behavioral requirements:**

- Failures still identify the affected geometry/input invariant, and high-cardinality behavior remains meaningfully tested.

**Edge cases:**

- Font atlas/context changes, cached stale expected geometry, scale/width changes, scroll animation, zero/one result, offscreen selection, mutable fixture reuse across cases.

**Regression constraints:**

- Reference expectations do not call the production calculation under test; passing results cannot be manufactured by changing golden values to the new output.

**Verification:**

- Run only affected correctness oracles and their directly related geometry/input cases; compare baseline and candidate execution with equivalent source/profile/data. Use one paired comparison and at most three repeats if variance obscures the result.

**Acceptance criteria:**

- All retained invariants pass with an explicit coverage mapping, and measured selected-run evidence shows which setup/execution cost improved. If no improvement is established, report that objective as unachieved rather than complete.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(test): [M2-A] reduce repeated GUI reference setup` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Coordinate native resources and finalize the scoped test contract

**Objective:** Native tests avoid preventable resource contention without slowing unrelated tests or hiding failures.

**Code ownership:**

- Proposed .config/nextest.toml if no current configuration exists
- Selected native clipboard/hotkey/capture test fixtures
- dev/README.md
- Any already-integrated feature test catalog

**Implementation tasks:**

1. Inspect installed Nextest configuration support and classify the actual selected resource users. Add narrowly matched groups/concurrency limits and necessary test-only ownership guards; retain pure clipboard-persistence/model tests outside native groups.
2. Document the scope of each guard, including whether it coordinates one Nextest invocation or multiple runners. Never claim a test group excludes a personal running app.
3. Verify group matching against the discovered roster and retain existing ignored/native opt-in rules for genuine environment requirements.
4. Compare selected-run results with the baseline, document moved/removed duplicate-case mappings and remaining measured costs, and independently review fixture lifetimes and assertions.

**Behavioral requirements:**

- Conflicting native cases are coordinated; unrelated model/GUI-state tests retain ordinary concurrency and native failures remain visible.

**Edge cases:**

- Overlapping resource families, renamed tests escaping a filter, parallel independent runners, unavailable interactive session, failed cleanup after a native timeout.

**Regression constraints:**

- No global single-thread setting, blanket retries, broad ignores or suppression of registration errors; finite process ownership remains required.

**Verification:**

- Validate roster-to-group assignments; run a small competing selected subset under ordinary concurrency and verify its owner lifecycle. Do not exercise personal clipboard/input just to demonstrate grouping.

**Acceptance criteria:**

- The intended resource users match their policy, independent tests remain parallel, lifecycle checks pass and the report distinguishes improved time from merely changed organization.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(runtime): [M2-B] coordinate selected native resources and document test costs` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use the affected case roster and fixture coverage map as the verification boundary. A full suite is not automatic; broaden only for an evidenced shared-fixture consumer that cannot be covered by the mapped selection.
- Separate build preparation from timed execution. Default compilation ceiling is 30 minutes and each selected execution batch is at most 10 minutes; set narrower per-test deadlines from the actual behavior before launching.
- Use one baseline/candidate comparison plus at most three repeats when noise or a reproduced race remains. No repeated release builds or cache deletion.
- Verify actual absence of unwanted background owners/data access using existing fixture observations or small targeted counters, not by assuming an inert plugin manager makes the entire app inert.
- End with relevant first-party lint/review checks. Any unexecuted native proof or failure to establish the requested timing improvement remains explicitly incomplete.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| A faster fixture stops testing the production behavior it was meant to protect. | Preserve owner-level behavioral assertions and a small genuine integration path for each crossed boundary; distinguish pure unit cases from required real-service tests. |
| Shared mutable GUI or global state makes test results order-dependent. | Share only immutable fixture inputs, use fresh contexts when necessary, restore global settings with guards and finish background owners before teardown. |
| Resource grouping is mistaken for complete isolation. | Document its process/run scope, validate matching selectors, and retain explicit roots and ownership protocols for resources beyond a single invocation. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Selected small tests use isolated, intentionally scoped fixtures; every touched app profile disables paste next/cancel shortcuts before construction.
- Changed waits and fixture teardown are bounded and preserve actual event/timing semantics.
- Selected GUI correctness coverage remains independent and meaningful at large size, with no unexplained removed assertions/cases.
- Evidence establishes the achieved time/reliability improvements; any requested improvement lacking evidence or a verified environment remains clearly marked incomplete.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report original/new test identities, affected fixture/service ownership, resource groups, removed redundant work and retained invariant coverage.
- Provide equivalent baseline/candidate setup/execution/teardown results, exact selections and profiles, actual repetition counts, cleanup/native status and unresolved limitations.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
