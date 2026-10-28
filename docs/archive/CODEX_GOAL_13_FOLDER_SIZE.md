# Codex Goal 13 — Explore folder sizes with bounded background scanning

**Companion execution prompt:** [CODEX_KICKSTART_13_FOLDER_SIZE.md](CODEX_KICKSTART_13_FOLDER_SIZE.md)  
**Requested coverage:** #52  
**Program position:** 13 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/folder-size-explorer` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish folder-size branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

The user can select a folder, see which immediate subfolders and files account for its logical size, drill down through results and inspect the largest files while the launcher stays responsive. Progress, cancellation, exclusions and incomplete totals are explicit, and the utility never modifies scanned files.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/file_search/walkdir.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/file_search/walkdir.rs)** — `WalkDirSearchExecutor; search_filenames_in_directory`. File Search already scans explicit roots with cancellation, progress and inaccessible/skipped counts, but its filtering/ranking model is not a folder-size aggregation engine.
- **[src/file_operations.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/file_operations.rs)** — `ScanCancellation; scan_duplicate_discovery_root_with_progress; inspect_duplicate_discovery_root`. The shared Windows file boundary provides a cooperative cancellation token, validated native root identity and bounded metadata-only enumeration with no-follow behavior. Duplicate enumeration omits rename-proof allocation, but currently returns rows rather than streaming folder aggregates.
- **[src/duplicate_finder/discovery.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/duplicate_finder/discovery.rs)** — `DiscoveryLimits and native discovery orchestration`. Existing discovery explicitly limits roots, entries, directories, depth, retained paths and diagnostics, and reports partial/limited results. Its hashing/grouping and hard-link deduplication are specific to duplicate discovery.
- **[src/duplicate_finder/session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/duplicate_finder/session.rs)** — `DuplicateFinderSession::start_scan, cancel, close and reopen`. An existing worker session uses generations, cancellation and bounded thread-reaper ownership. Closing does not block the GUI, and reopening respects actual worker cleanup.
- **[src/duplicate_finder/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/duplicate_finder/model.rs)** — `ScanCounts; RootScanStatus; KnownFile`. The current file utilities distinguish inaccessible/reparse/cloud exclusions and known native identity from unknown identity rather than treating unknowns as zero-sized files.
- **[src/plugins/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/plugins/table_utility.rs)** — `TableUtilityPlugin and typed action tests`. A compact utility plugin already follows an exact query-prefix, typed open action and searchable command pattern, suitable for a new utility entry point.

## C. Approved functional requirements

1. Provide an explicit folder picker and a dedicated launcher command. Use a proposed foldersize prefix and typed open action after checking the current command catalog for collisions; do not repurpose an existing command.
2. Scan only the explicitly selected root and descendants. Include accessible regular hidden/system entries so a search-specific exclusion list does not silently hide disk consumers. Do not start a computer-wide or recurring scan automatically.
3. Show logical file-byte totals, file/directory counts, immediate-child rows sortable by size/name and a bounded largest-files list. Support root/breadcrumb drill-down, refresh, copy path and explicit reveal in Explorer.
4. Define size semantics in help and UI: count regular-file logical length once per included path. Hard-link aliases therefore contribute per path, and sparse/compressed files may differ from physical allocation. Do not label these values physical disk usage or reclaimable space.
5. Use no-follow traversal for descendant symbolic links/junctions/reparse points. Do not hydrate cloud placeholders or read file contents; skip unsupported/unhydrated entries and report their counts. Resolve/admit the selected root deliberately and report unsupported roots instead of silently scanning a different location.
6. Show scan progress, elapsed time, entries/directories examined, exclusions and bounded diagnostic details. Inaccessible, changed, limited and cancelled results must be visibly incomplete; an unknown/unscanned size is not zero.
7. Keep scanning and large aggregation/sorting work off the UI thread. Coalesce progress and virtualize displayed rows; do not clone or sort the whole tree each frame.
8. Enforce finite directory-node, retained-path, depth, diagnostic and largest-file budgets. Use checked byte totals and report the reason when a budget is reached. Partial totals must not be presented as a complete census.
9. Cancel cooperatively and reject stale generations. Closing must release owned results and arrange worker cleanup without blocking the GUI; reopening/rescanning must not create unbounded workers or allow old results to replace the new selection.
10. Preserve existing File Search, Duplicate Finder, Rename and file identity semantics. This utility is read-only: no delete, move, rename, cleanup recommendations that claim exact reclaimed space, or file-content hashing.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- A disk cleaner, duplicate cleanup, filesystem indexer, continuous folder watcher, physical NTFS allocation analyzer, treemap requirement, automatic cloud hydration, or cross-volume accounting.
- An application-wide scanner framework or migration of existing file tools. Reuse narrow primitives without coupling to duplicate hashing or rename authorization proofs.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Introduce a small domain model/scanner/session plus dialog following existing utility boundaries. Proposed locations are src/folder_size/{mod,model,scan,session}.rs, src/gui/folder_size_dialog.rs and src/plugins/folder_size.rs; verify neighboring conventions before creating them.
2. Reuse native no-follow/root identity primitives and cancellation ownership where appropriate. If aggregation needs an incremental visitor, make a narrow reusable metadata-enumeration extension at file_operations rather than allocating a duplicate-finder snapshot and then copying it into another tree.
3. Aggregate descendant totals upward using stable node identities, retain directory structure under explicit path/node budgets, and maintain a bounded top-files heap. Hard-link per-path semantics avoid an unbounded global identity set and must remain explicitly labeled.
4. Publish immutable generation-tagged snapshots or bounded/coalesced updates. The dialog owns presentation state; the session owns work/results. Account for retained previous/current snapshots in memory limits.
5. Goal 06 is recommended ordering. If its activity/cancellation interfaces are present, register this job there; otherwise use the existing generation/reaper patterns. Do not invent a second global manager or make an unavailable future API an implicit prerequisite.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Build bounded metadata aggregation

**Objective:** A domain scanner produces correct, honestly classified folder totals from a selected root.

**Code ownership:**

- proposed src/folder_size/mod.rs
- proposed src/folder_size/model.rs
- proposed src/folder_size/scan.rs
- src/file_operations.rs — narrowly shared enumeration support only
- src/lib.rs — module registration

**Implementation tasks:**

1. Define root admission, node/file records, logical-size semantics and complete/partial/cancelled/limited/failed statuses before wiring UI.
2. Implement metadata-only traversal with no-follow descendants, checked counters, bounded path/node/diagnostic storage and a bounded largest-files selection.
3. Accumulate direct and recursive totals consistently. Track incomplete child subtrees upward; never convert an unreadable child into a zero-byte completed subtree.
4. Add pure/fake-enumerator tests plus disposable-tree tests for totals, stable sorting, hard-link per-path counting and explicit exclusions. Exercise small injected budgets to prove early stopping without a huge real tree.

**Behavioral requirements:**

- The scanner returns useful totals with clear scope/completeness and does not read file payloads.
- Directory drill-down data and largest-file candidates are bounded and internally consistent.

**Edge cases:**

- Empty folder, zero-length/very large/sparse files, hard links across directories, inaccessible children, deep paths, reparse cycles, cloud placeholders, changing/deleted files and checked-sum overflow.

**Regression constraints:**

- Existing duplicate identity and rename proof behavior is unchanged.
- No source mutation, link following or implicit hydration is introduced.

**Verification:**

- Run the new domain aggregation/budget tests and only directly affected existing enumeration tests if a shared primitive changed.
- On Windows, validate representative no-follow/hard-link cases using a disposable local tree; record privilege-dependent unavailable cases.

**Acceptance criteria:**

- Known fixture totals equal the documented per-path logical sums.
- Every excluded/failed/limited subtree is visible in status/counts and cannot produce a false complete total.
- Injected memory/count limits stop predictably while preserving valid partial data.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(folder-size): [M1-A] add bounded metadata aggregation` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Own background scans and cancellation

**Objective:** Folder scans progress asynchronously with deterministic replacement and cleanup.

**Code ownership:**

- proposed src/folder_size/session.rs
- src/thread_reaper.rs — reuse only unless a demonstrated shared defect requires change
- existing goal-06 activity interface if present

**Implementation tasks:**

1. Implement one bounded session worker or equivalent existing executor integration with generation tags, cooperative cancellation and a latest-request policy.
2. Coalesce progress so rapid enumeration cannot fill an unbounded event queue. Keep retained result snapshots under the defined storage budget.
3. Cancel on close/replacement; transfer the owned worker to the existing reaper without a GUI-thread join. Show stopping/busy until ownership is actually released.
4. Integrate available activity reporting with state/counts only; avoid repeatedly reading process-wide resource counters for each progress event.
5. Use deterministic gates to test cancellation during enumeration, old completion after a new request, close/reopen and reaper admission failure.

**Behavioral requirements:**

- The launcher remains interactive while scanning; cancellation stops new work and reports actual state.
- A newer root/refresh cannot be overwritten by an older worker's progress or completion.

**Edge cases:**

- Slow/blocking native enumeration, rapid rescan/cancel, close before first result, exhausted cleanup capacity, worker error and shutdown during scan.

**Regression constraints:**

- No worker, progress queue or old tree grows without a bound.
- Owned process/thread cleanup follows existing conventions and never kills unrelated processes.

**Verification:**

- Run focused session generation/cleanup tests with controlled worker gates, not sleeps.
- Observe one bounded generated-tree scan and cancellation; distinguish GUI responsiveness from actual completion of a slow OS call.

**Acceptance criteria:**

- Stale updates are ignored and cancel/close/reopen transitions are tested.
- No blocking GUI join or unbounded detached worker is introduced.
- Result and worker ownership is released on completed close/cleanup.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(folder-size): [M1-B] add cancellable background scan sessions` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Add explorer UI and launcher integration

**Objective:** The user can navigate and compare actual scan results through an accessible, responsive utility.

**Code ownership:**

- proposed src/gui/folder_size_dialog.rs
- proposed src/plugins/folder_size.rs
- src/plugins_builtin.rs
- src/commands/model.rs
- src/commands/parser.rs
- src/commands/host.rs
- src/commands/handlers — proposed folder-size handler if current convention requires it
- src/gui/command_host.rs
- src/gui/mod.rs
- src/gui/render.rs
- README.md and current help

**Implementation tasks:**

1. Register a distinct utility prefix and typed command through existing parsing/dispatch paths. Add the dialog to normal visibility/close lifecycle without changing unrelated focus policy.
2. Build folder selection, progress/cancel/refresh, breadcrumb navigation, sortable immediate children and a bounded largest-files view using virtualized rows and stable IDs.
3. Display logical-size/hard-link semantics and incomplete/exclusion indicators near totals. Provide copy path/reveal only on a current selected path, handling disappearing entries.
4. Reuse existing keyboard/Jump registration where supported by standard utility controls; do not launch a separate navigation expansion project.
5. Run one native session selecting a generated tree, drill down/up, sort, inspect a large-file row, cancel/refresh and close/reopen. Capture complete and incomplete views and update concise help.

**Behavioral requirements:**

- Useful totals appear without freezing input, and drill-down preserves the selected root context.
- The user can distinguish a complete scan from a partial lower bound and see why entries were skipped.

**Edge cases:**

- No selected root, invalid/root-changed selection, empty/partial results, a very wide/deep name, small window, disappearing reveal target and reopening after cancellation.

**Regression constraints:**

- Existing file utility commands and selected files remain untouched.
- UI rendering does not initiate a rescan or clone/sort all results every frame.

**Verification:**

- Run focused plugin/parser/host and dialog-state tests plus the scanner/session selection.
- Perform one native generated-tree smoke, capture sorting/drill-down and incomplete-state evidence, and inspect counts against the known fixture.

**Acceptance criteria:**

- The command opens the utility and all requested browse/progress/cancel operations work.
- The native fixture matches expected totals and exclusion/completeness labeling.
- No known worker/result leak or cross-tool behavior regression remains within touched paths.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(folder-size): [M2-A] expose explorer navigation and scan status` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use deterministic small domain fixtures for accounting and injected bounds, session gates for lifecycle, and one generated disposable Windows tree for native identity/reparse behavior. Do not scan the user's full drive for testing.
- Separate build time from execution. After compiling required targets, allow up to 10 minutes for the targeted scanner/session/command selection and up to 10 minutes for one native smoke plus cancellation. Use at most two focused remediation attempts.
- Run one bounded larger synthetic-tree observation to check peak retained state and idle UI cost; stop at known budget thresholds and record actual counts/limits. Do not claim a measured memory reduction without a comparison.
- Use isolated application settings with Paste Sequence next/cancel shortcuts disabled. Broaden tests only if a specific shared-enumeration modification creates a concrete adjacent risk; no default full suite or release loop.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Logical totals can be mistaken for physical disk allocation or space freed by deletion. | Label the metric and hard-link per-path rule explicitly; provide no physical/reclaimable-space promise. |
| A directory tree or progress stream can consume unbounded memory despite metadata-only scanning. | Budget nodes/path bytes/snapshots/diagnostics and top files, coalesce updates, and test injected limits. |
| A filesystem change or reparse point redirects a scan or makes totals falsely complete. | Use validated root/no-follow primitives, generation checks and visible partial/exclusion status; avoid file payload reads. |
| Cancellation returns visually while a worker remains blocked in an OS call. | Expose stopping/busy honestly, retain ownership and use the bounded reaper instead of detaching or blocking UI. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Explicit-root logical totals, largest files, sorting and drill-down are implemented and documented.
- Memory/progress/worker lifetimes are bounded with tested incomplete/cancel states.
- The utility is read-only and preserves existing file-tool semantics.
- Native and targeted evidence support the actual completion claims.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report the final command prefix, size/hard-link/exclusion semantics and concrete resource limits.
- Include known-fixture totals, partial-state and drill-down screenshots, and cancellation/cleanup evidence.
- Identify any unsupported root/platform cases and the checkpoint/push results.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
