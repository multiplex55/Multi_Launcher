# Codex Goal 01 — Bounded developer workflows, build evidence, and focused test feedback

**Companion execution prompt:** [CODEX_KICKSTART_01_DEV_WORKFLOWS.md](CODEX_KICKSTART_01_DEV_WORKFLOWS.md)  
**Requested coverage:** #119, #124, #127, #133, #134, #135, #139, #140  
**Program position:** 01 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/developer-workflows` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish dev-workflows branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Provide a dependable PowerShell entry point for existing build modes, named feature test selections, failed-test replay, and selected native smoke runs. Each invocation records what actually ran, its result and duration, its build/cache context, and the outcome of owned-process cleanup. Show useful slow-test and comparable-build summaries without making the developer reconstruct commands or confuse compilation with test execution.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** None. Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[dev/track_b/cargo.ps1](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/track_b/cargo.ps1)** — `Assert-NoConflictingCargoArguments and fixed preset dispatch, lines 51–149`. Existing release/debug/lib-test/integration-test presets resolve the repository from the script location, pass native argument arrays, preserve the native exit code, and reject conflicting target or compiler-profile selectors. Named fast profiles currently require direct Cargo commands.
- **[dev/track_b/measure.ps1](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/track_b/measure.ps1)** — `measurement record and Process.WaitForExit, lines 284–462`. Existing provenance includes source SHA, dirty files, command, cache/profile context, fixture hashes, logs, and fresh Cargo timing reports. It has no execution watchdog; whole-process wall time is explicitly not isolated linker time.
- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `fast-dev, iteration, required-features, lines 9–67 and 269–281`. Reduced-symbol and release-like iteration profiles and gated developer binaries already exist. The plan must select these rather than invent equivalent profiles.
- **[dev/run_owned_process.ps1](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/run_owned_process.ps1)** — `Record-Process, Same-Identity, Check-RecordedProcess and cleanup`. The existing Windows wrapper records process identities, deadlines and cleanup results. It depends on process inventory, and its limitations must be preserved honestly when selecting a containment strategy.
- **[dev/perf_harness/README.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/perf_harness/README.md)** — `Compiler and external-command timing; Build once, invoke bounded scenarios`. A checked Job Object runner exists for registered probe executables. It deliberately disallows arbitrary command forwarding and documents environment-blocked outcomes.
- **[dev/jump_mode/README.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/jump_mode/README.md)** — `Phase1Enabled and Phase2 surface selections`. Existing GUI fixtures provide isolated profiles, owned native windows, matching capture receipts, surface selection and bounded cleanup. Fixture-only checks do not establish native UI behavior.

## C. Approved functional requirements

1. Retain every existing helper invocation. Add a small documented workflow facade, proposed at dev/workflows/run.ps1 with a declarative selection catalog, that reuses existing command construction, measurement and native harnesses instead of duplicating them.
2. Expose quick check, full-debug build, fast-dev build/test, iteration application build and production build as explicit choices. Keep canonical release and normal Cargo behavior unchanged; distinguish Cargo compiler profiles from Nextest runner profiles.
3. Named feature selections must specify actual Cargo targets as well as exact cases or filtersets, required nondefault features, platform/resource requirements and available smoke selections. Cover the requested work and common existing areas such as notes, paste sequences, query focus, tables and settings. An unknown feature or empty resolved test selection is a visible failure, never an automatic whole-suite fallback.
4. Record compilation/preparation and test execution separately where supported by the installed tools. Preserve command arrays, tool versions, source/base identifiers, target directory, relevant configuration fingerprints, exit status and bounded logs. Do not collect arbitrary environment values or credentials.
5. After each selected test run, report selected/passed/failed/timed-out/skipped counts and the slowest executed cases. Replay only the originally failed selection with its original targets, features, compiler/runner profiles and meaningful options. Preserve the first failure receipt and link subsequent attempts to it.
6. Every new workflow has a declared stage deadline and verified cleanup of its owned process tree. Use separate default ceilings of 30 minutes for compilation, 10 minutes for a selected test stage and 5 minutes for a selected native smoke after compilation; document a narrower task override where justified. A known legitimate larger workload requires an explicitly recorded budget before launch, not indefinite extension after a timeout.
7. Build trends compare matching host/toolchain, target, feature and compiler-profile configurations and clearly label no-op, edited-source, cache-population and cold/unknown samples. Cache diagnostics explain observed configuration differences; they do not automatically delete caches or promise a root cause unsupported by compiler evidence.
8. Expose existing focused native smoke selections through the same catalog. Isolate all fixture data, disable Paste Sequence next/cancel shortcuts before construction, keep screenshots bound to the owned HWND/process, and return a truthful environment block when native interaction is unavailable. Bound evidence retention using existing repository conventions and preserve original failure summaries.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No launcher runtime changes, general test refactor, dependency upgrade, linker switch or editor configuration change in this goal.
- Do not add the separate SSH live-progress feature #136, a new continuous benchmark service, automatic retries, or an automatic complete-suite gate.
- Do not run arbitrary shell strings, widen the registered performance-probe allowlist, kill processes by executable name, globally clean Cargo caches, or alter personal data/settings.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Start by inspecting installed Cargo/Nextest capabilities and the existing helper call graph. Reuse native Nextest record/replay/rerun and timing/report facilities when the installed version provides the required semantics; retain only the small provenance/selection wrapper needed around them. Choose supported machine-readable results; do not prescribe unverified evolving CLI flags or infer results from colored console text when a structured interface is available.
2. The facade owns selection and stage orchestration. Existing helpers retain their command/provenance responsibilities; a single checked execution owner supplies deadlines and cleanup. Refactor shared helper code only when required to avoid competing process owners or divergent argument handling.
3. Keep the selection catalog declarative and reviewable. Validate it against the current discovered test roster before execution, and use binary/case identities rather than unescaped interpolation when constructing replay filters.
4. Keep reports small and local. Store detailed evidence under an owned ignored output root, with deterministic retention and no deletion outside that root; commit only documentation, catalog entries and small sanitized comparison summaries.


### Ordered program map

The numbering gives the recommended execution order. Each row is a separate Goal/Kickstart pair and requires a separate fresh branch. This map is navigation, not an instruction to automatically run or merge the entire program.

| Order | Goal | Requested coverage | Hard prerequisites |
|---|---|---|---|
| 01 | [Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md) | #119, #124, #127, #133, #134, #135, #139, #140 | None |
| 02 | [Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md) | #128, #129, #130, #131, #132, #138 | None |
| 03 | [Measured compilation boundaries, dependency cleanup, and test target consolidation](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md) | #120, #121, #122, #123, #137 | None |
| 04 | [Faster Windows linking and responsive Neovim rust-analyzer workflows](CODEX_GOAL_04_LINK_EDITOR.md) | #125, #126 | None |
| 05 | [Keep Plugin Settings usable with long descriptions](CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md) | Explicit Plugin Settings horizontal-scrolling request | None |
| 06 | [Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md) | #115, #117, #118 | None |
| 07 | [Launcher readiness, large-result display, and startup responsiveness](CODEX_GOAL_07_LAUNCHER_RESPONSIVENESS.md) | #111, #112, #114 | None |
| 08 | [Lower idle work and configurable live-pin performance](CODEX_GOAL_08_IDLE_LIVE_PINS.md) | #113, #116 | None |
| 09 | [Large CSV files with bounded memory and complete-file operations](CODEX_GOAL_09_LARGE_CSV.md) | Explicit 500 MB+ CSV request; approved complete-dataset processing | None |
| 10 | [Table transformations summaries and JSON/Markdown exchange](CODEX_GOAL_10_TABLE_OPERATIONS.md) | #91, #93, #94 | 09 |
| 11 | [Configure Jump Mode activation and ergonomic hint letters](CODEX_GOAL_11_JUMP_CONFIGURATION.md) | #22 | None |
| 12 | [Redact screenshots with solid fill, blur and pixelation](CODEX_GOAL_12_SCREENSHOT_REDACTION.md) | #74 | None |
| 13 | [Explore folder sizes with bounded background scanning](CODEX_GOAL_13_FOLDER_SIZE.md) | #52 | None |
| 14 | [Preview ZIP contents and extract an explicit selection safely](CODEX_GOAL_14_ZIP_EXPLORER.md) | #53 | None |
| 15 | [Completed macro run summaries and practical starter recipes](CODEX_GOAL_15_MACRO_SUMMARIES_RECIPES.md) | #58, #59 | None |
| 16 | [Hot Folder size, age, and text-content conditions](CODEX_GOAL_16_HOT_FOLDER_CONDITIONS.md) | #63 | None |
| 17 | [Drop data files into an appropriate tool](CODEX_GOAL_17_FILE_TOOL_ROUTING.md) | #1 | 09, 10, 12 |

Development tooling comes first so later work can use focused selection, bounded runs and repeatable evidence. Runtime ownership precedes new long-running utilities. The large-file foundation precedes table operations. File routing comes after the destination contracts it requires. The independent Plugin Settings fix can be executed earlier if desired without changing any dependency.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Establish bounded execution and a common result contract

**Objective:** A selected developer command finishes with a trustworthy result or a bounded failure and cleanup receipt.

**Code ownership:**

- dev/track_b/measure.ps1
- dev/run_owned_process.ps1
- dev/test_owned_process.ps1
- src/bin/perf_probe_runner/windows.rs for read-only ownership-pattern review
- Proposed dev/workflows execution helper only if existing owners cannot supply the required contract

**Implementation tasks:**

1. Inventory actual helper dependencies and supported Windows ownership facilities after branch bootstrap. Preserve existing public parameters and caller behavior.
2. Define one structured invocation/result record extending existing provenance with stage, deadline, timeout reason, owned identities and cleanup certainty.
3. Implement finite waiting and owned-tree cleanup for the workflow command family, using checked ownership rather than process-name scanning. If a containment prerequisite is denied, record the block and do not launch an uncontained substitute.
4. Make log capture and result publication resilient to a failing child, interrupted output and storage errors; a failed recorder or uncertain cleanup must not become success.

**Behavioral requirements:**

- Nonzero child exit, timeout, failed launch and cleanup uncertainty are distinguishable; successful completion requires both command success and required cleanup.

**Edge cases:**

- Short-lived parent with a child, sleeping parent/child, nonzero exit, denied process inventory/ownership, unwritable evidence directory.

**Regression constraints:**

- No unrelated process termination, no arbitrary shell evaluation, no changes to the performance runner's restricted command contract.

**Verification:**

- Use controlled temporary fixture processes to exercise success, exit 7, parent/child timeout and ownership failure. Verify captured arguments containing spaces and filter punctuation remain separate arguments.

**Acceptance criteria:**

- Every fixture produces the correct result and bounded cleanup; existing helper smoke checks still pass; no invocation can report an unbounded wait as a deadline-enforced success.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(dev): [M1-A] add bounded workflow execution receipts` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Expose build modes and resolve feature test selections

**Objective:** A named workflow selects exactly the intended build mode and Cargo/test scope.

**Code ownership:**

- dev/track_b/cargo.ps1
- Cargo.toml for existing mode lookup
- Proposed dev/workflows/run.ps1 and selection catalog

**Implementation tasks:**

1. Implement the facade and catalog using current paths and discovered test identities; include a command-preview mode that does not compile or execute tests.
2. Select Cargo targets before case filters, preserve native argument arrays, and reject conflicting selectors or empty matches.
3. Expose existing fast-dev and iteration choices explicitly without changing release defaults or confusing Nextest -P with a compiler profile.
4. Associate each selection with resource/platform notes and a stage budget; leave actual resource-group policy to the test-isolation goal or already-integrated configuration.

**Behavioral requirements:**

- Preview shows executable, argument array, target/profile, discovered selection and budgets; running a selection uses that same resolved request.

**Edge cases:**

- Unknown or renamed tests, zero matches, paths with spaces, incompatible mode/target combinations, absent Nextest, forwarded options after an argument separator.

**Regression constraints:**

- Existing release/debug/lib-test/integration-test commands remain valid; no implicit all-targets/all-features expansion.

**Verification:**

- Validate the catalog against a discovered current roster; compare previews for one library and one integration selection; execute one small representative selection through the bounded owner.

**Acceptance criteria:**

- Known selections resolve correctly; invalid selections fail before execution; preview and execution metadata agree.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(dev): [M1-B] add explicit build modes and feature test selections` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Report slow cases and replay original failures

**Objective:** Selected test runs produce actionable summaries and faithfully reproducible failed-test reruns.

**Code ownership:**

- Proposed dev/workflows result/report/replay helpers
- Existing Nextest result configuration if present in the execution base

**Implementation tasks:**

1. Consume a machine-readable format supported by installed Nextest; separate compiler failure from test failure and execution time from build time.
2. Produce counts, slowest executed cases, failed case identities and evidence links while preserving native failure status.
3. Create a replay request from the original resolved request and failed case identities, preserving profile/features/targets and relevant options; save each replay as a new attempt.
4. Reject stale or incompatible replay metadata with an actionable explanation; preserve the original failure and never automatically retry until green.

**Behavioral requirements:**

- A replay selects only the original failed tests, and reports changes in source/tool configuration instead of pretending environments are identical.

**Edge cases:**

- Build failure before test discovery, crash or timeout with partial results, no failed cases, duplicate names in different binaries, malformed/truncated result data.

**Regression constraints:**

- A successful replay does not overwrite the first failure; skipped/ignored/unexecuted cases are never counted as passes.

**Verification:**

- Use small controlled result fixtures for two binaries with the same test name, partial failure and timeout; run one deliberate tiny failing fixture through capture/replay when the existing harness supports isolation.

**Acceptance criteria:**

- Counts and slow-case order match actual records; replay preserves the required options and binary identity; first-failure evidence remains available.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(dev): [M2-A] summarize selected tests and replay failed cases` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Compare build records and explain cache differences

**Objective:** The developer can compare equivalent builds and identify observable causes of cache churn.

**Code ownership:**

- dev/track_b/measure.ps1
- dev/perf_track_d/summarize.ps1
- Proposed workflow comparison view/helper

**Implementation tasks:**

1. Reuse existing records and fresh Cargo timing artifacts; add stable fingerprints for relevant configuration and tool versions without dumping environment secrets.
2. Group comparable scenarios and show whole-command changes alongside available crate/timing details; mark incompatible, missing and noisy samples explicitly.
3. Explain observed target-directory, profile, feature, toolchain and Rust-flag differences. Preserve unknown causes when available evidence cannot establish invalidation ownership.
4. Apply bounded evidence retention and document how to reproduce one comparison without deleting existing caches.

**Behavioral requirements:**

- A warm no-op cannot be presented as an edited-build improvement; aggregate command timing cannot be labeled linker timing.

**Edge cases:**

- Old record schema, missing timing file, stale unchanged Cargo report, different host/profile, interrupted build, cache population.

**Regression constraints:**

- No automatic environment rewrite or cache removal; original measurement provenance remains readable.

**Verification:**

- Check comparison output using controlled compatible/incompatible records; capture one representative baseline/candidate or unchanged repeat and verify its labels against actual commands.

**Acceptance criteria:**

- Reports distinguish comparable results from unavailable comparisons and accurately identify recorded configuration differences.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(dev): [M2-B] compare build timings with cache provenance` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M3-A — Connect focused native smoke selections and document use

**Objective:** One selected tool can be opened, exercised, captured and closed through its existing safe harness.

**Code ownership:**

- dev/jump_mode/run.ps1
- dev/jump_mode/README.md
- dev/README.md
- Proposed workflow selection catalog

**Implementation tasks:**

1. Map supported existing smoke/surface selections to their actual harness entry points, reusing fixture generation, owned-window validation and cleanup.
2. Separate fixture-only, headless and native receipts; request only the selected surface and a small number of meaningful captures.
3. Document build modes, selection previews, slow reports, replay, evidence locations, deadlines and environment-blocked outcomes with exact current commands.
4. Review the cumulative diff and remove duplicate orchestration or unsafe fallback paths introduced by this goal.

**Behavioral requirements:**

- The chosen surface runs autonomously when supported, and a noninteractive environment produces a truthful blocked/native-not-run result.

**Edge cases:**

- Unavailable interactive desktop, harness compile failure, mismatched capture receipt, already-running personal launcher, close timeout.

**Regression constraints:**

- All fixtures use isolated data and disabled paste shortcuts; only owned processes/windows receive input or cleanup.

**Verification:**

- Run one fixture-only selection and one focused native selection when supported, with at most three captures and a five-minute post-build deadline; verify shutdown and retained evidence.

**Acceptance criteria:**

- Documented entry points select only their intended work; native success requires matching receipts and cleanup; any unavailable native proof is clearly incomplete.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `docs(dev): [M3-A] connect targeted smoke workflows and document evidence` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use fixture-level PowerShell/record parsing checks and one small actual library selection plus one integration-selection preview. Do not run the complete Rust suite to validate developer tooling.
- Run one representative build comparison; use no more than three additional paired samples only if variation prevents interpretation. Existing caches remain intact.
- Run the process-owner negative fixtures before relying on deadlines for long commands. Enforce separate compilation, selected-test and native deadlines defined in requirements.
- Use one selected native surface only; reuse its built executable when valid. Report unsupported native verification honestly and do not ask the user to perform intermediate checks.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| A wrapper hides compiler failures or selects more tests than requested. | Preserve native exit codes, validate the resolved roster, reject zero matches and record target/filter/profile separately. |
| Timeout handling leaves children alive or kills unrelated processes. | Prove owned identity/tree cleanup with negative fixtures; fail closed on uncertain ownership without process-name kills or uncontained fallback. |
| Trend reports imply unsupported speedups or expose local secrets. | Compare compatible contexts, retain explicit unknowns, fingerprint selected settings, and avoid collecting arbitrary environment values. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Existing build modes and named selections are accessible through documented bounded commands with verified argument handling.
- Slow-test summaries, original-option replay and compatible build comparisons work with actual supported result formats.
- Deadline/cleanup negative cases pass, and selected native smoke evidence is either actually verified or explicitly reported as incomplete/environment-blocked.
- No application behavior or default production build configuration changed.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- List supported named selections and build modes, exact invocation examples, result schema/evidence locations and budgets.
- Include one real selected-run receipt, replay verification, comparison example, and native evidence status; distinguish tools implemented from performance improvements that later goals must measure.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
