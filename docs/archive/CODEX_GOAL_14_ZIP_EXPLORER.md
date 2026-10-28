# Codex Goal 14 — Preview ZIP contents and extract an explicit selection safely

**Companion execution prompt:** [CODEX_KICKSTART_14_ZIP_EXPLORER.md](CODEX_KICKSTART_14_ZIP_EXPLORER.md)  
**Requested coverage:** #53  
**Program position:** 14 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/zip-explorer` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish zip-explorer branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

The user can inspect a ZIP's entry list, select files or folders and extract only that selection to a chosen destination without launching a separate archive program. Listing and extraction remain bounded, cancellable and clear about unsupported entries, collisions and partial results; existing destination files are never overwritten.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [03 — Measured compilation boundaries, dependency cleanup, and test target consolidation](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md), [13 — Explore folder sizes with bounded background scanning](CODEX_GOAL_13_FOLDER_SIZE.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `Current dependency list`. No ZIP dependency is declared in the reviewed manifest. A maintained parser/decompressor with a minimal feature set is a justified new dependency for this requested capability, subject to verification at execution time.
- **[src/file_operations.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/file_operations.rs)** — `ScanCancellation; native file identity; guarded file operations`. The application already has cancellation, native handle/identity and confined file-operation patterns. These are relevant to destination safety, but the inspected file does not implement ZIP extraction.
- **[src/duplicate_finder/session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/duplicate_finder/session.rs)** — `Generation ownership, cancel/close/reopen and reaper transfer`. An existing background utility demonstrates bounded worker ownership, stale-result rejection and nonblocking cleanup suitable as a lifecycle pattern.
- **[src/hotkey/directory.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotkey/directory.rs)** — `Local scope representations`. Local utility controls can follow existing UI conventions without introducing any new global shortcut for archive work.
- **[src/plugins/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/plugins/table_utility.rs)** — `TableUtilityPlugin`. The application already registers compact utilities through a query prefix, typed open action and normal plugin capability discovery.
- **[src/commands/handlers/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/commands/handlers/table_utility.rs)** — `handle_table_utility`. Existing typed command handlers delegate opening to a host. A new archive opener should use this pattern rather than string dispatch embedded in the GUI.

## C. Approved functional requirements

1. Support ZIP inspection, initially using standard stored/deflated entries and ZIP64 where the selected library can support them under the same bounds. Identify other compression methods and encrypted entries as unsupported instead of attempting guessed decoding or silently omitting them.
2. Opening a ZIP lists entries/paths, file-vs-directory kind, compressed/uncompressed sizes and support/problem status without extracting file payloads. Archive preview means an inspectable entry tree/list; no automatic entry execution or nested-archive recursion.
3. Allow explicit individual-file and folder-subtree selection. Stable identity must include archive revision and entry index, so duplicate names do not collapse into one indistinguishable operation.
4. Show an extraction summary before starting: selected entries, intended destination, declared size estimate, collisions/rejections and configured safety limits. Require an explicit Extract action.
5. Default to a new destination subfolder and allow an explicitly chosen destination. Never overwrite existing files. Refuse conflicting selected output paths, case-insensitive collisions, file/directory ancestor conflicts or existing occupied paths before starting where detectable; recheck at creation/publication to handle races.
6. Treat entry paths as untrusted. Reject traversal, absolute/drive-relative/UNC/device paths, alternate data streams, forbidden/reserved Windows components, ambiguous trailing dots/spaces and unsafe symlink/reparse/device entries. Treat both slash forms correctly and preserve safe relative layout.
7. Confine writes to the admitted destination through a race-resistant no-follow path/handle boundary. A string prefix/canonicalization check alone is insufficient if a parent can become a junction between validation and creation.
8. Stream decompression through bounded buffers and enforce actual per-entry/job output bytes, entry counts and expansion safeguards. Bound central-directory/list metadata before or during parser allocation; post-parse limits alone do not establish a bounded opener.
9. Keep entry metadata/progress/diagnostics, selected-path plans and worker queues bounded. Show oversized/unsupported limits honestly, with no unbounded ReadToEnd of entry payloads and no full decompressed archive in memory.
10. Verify entry integrity before publishing the completed file. Use uniquely owned staging files and no-clobber publication. Cancel/error removes only the current owned partial files and safely removable owned empty directories; retain already completed outputs and report exactly what succeeded.
11. Detect a changed/replaced archive between listing and extraction through pinned handle/identity plus appropriate revision evidence, or revalidate/rebuild the selection plan. Do not apply stale entry indexes to different content.
12. Keep UI work asynchronous, offer progress/cancel, reject stale generations and release handles/temp buffers on completion/close. Do not kill unrelated processes or delete pre-existing destination content.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Archive creation/repacking, 7z/RAR/TAR support, password prompts/cracking, self-extracting executable support, automatic opening of extracted files, recursive extraction of nested archives or an overwrite/merge mode.
- A generic file manager, a new global background-work framework, changing existing Rename/Replace transactions or automatically installing an external archive application.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Introduce an archive domain/catalog/extraction/session with a dedicated dialog and typed plugin opener. Proposed locations are src/archive/{mod,model,catalog,extract,session}.rs, src/gui/archive_dialog.rs and src/plugins/archive.rs; these names are proposals, not existing files.
2. Choose one maintained ZIP implementation after reviewing its primary documentation, supported compression features, ZIP64 behavior and allocation model. Keep features narrow and record the reason for the dependency. Reuse existing image/text utilities only if needed for ordinary UI icons; do not add a second archive stack.
3. Separate listing from a deterministic extraction plan. Normalize and compare destination components using the existing Windows path semantics where applicable; track case-insensitive and ancestor collisions before I/O.
4. Put destination confinement and no-clobber file publication behind a narrow filesystem boundary, reusing existing validated-handle primitives when they fit. Do not make archive code depend on rename-specific authorization tokens or invent a broad migration.
5. Use a session-owned archive handle/revision and bounded plan. If goal-06 activity APIs already exist, integrate the job there; otherwise follow existing cancellation/generation/reaper patterns. Recommended ordering is not a hidden hard dependency.
6. Plan budgets as named finite values with tests that inject much smaller thresholds. Display the effective limits and distinguish declared-size estimates from actual bytes processed.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Parse a bounded catalog and validate extraction plans

**Objective:** ZIP inspection and selection produce a bounded, deterministic plan before any output is written.

**Code ownership:**

- Cargo.toml and Cargo.lock — one justified minimal ZIP dependency
- proposed src/archive/mod.rs
- proposed src/archive/model.rs
- proposed src/archive/catalog.rs
- src/lib.rs — module registration

**Implementation tasks:**

1. Inspect current dependencies and choose a maintained ZIP parser/decompressor with only required features. Confirm pre-allocation bounds for central-directory size/count and ZIP64; add explicit preflight/bounded parsing if the chosen API otherwise allocates unbounded metadata.
2. Model archive revision/entry identity, support status, metadata limits and selection. Keep duplicate entry names distinct.
3. Implement selected-output planning and Windows path validation, including case/ancestor collisions and a no-overwrite policy. Unsafe selected entries must block that plan with an explanation, not disappear silently.
4. Add generated tiny archive fixtures for stored/deflated/ZIP64 as supported, duplicate names, encrypted/unsupported methods, malformed metadata, path attacks and catalog-limit rejection.

**Behavioral requirements:**

- Opening an archive produces a bounded inspectable list with explicit unsupported entries.
- A valid selection maps to one unambiguous confined output plan; invalid selections cannot begin writing.

**Edge cases:**

- Empty archives, huge declared counts, inconsistent ZIP64 records, malformed encodings, duplicate paths, slash/backslash traversal, drive-relative/UNC names, reserved devices, ADS and file/directory collisions.

**Regression constraints:**

- No extraction, execution or destination mutation occurs during listing/planning.
- No unsupported entry is silently represented as a valid empty file.

**Verification:**

- Run pure catalog/path/planner tests on generated inputs; inspect the chosen dependency's actual allocation behavior.
- Use injectable low limits to demonstrate rejection before unbounded allocation, not merely after a large metadata vector exists.

**Acceptance criteria:**

- Catalog/plan storage is bounded and supported fixtures list correctly.
- Every defined unsafe/colliding selected path is rejected with a specific reason.
- The chosen ZIP feature set and unsupported cases are documented.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(archive): [M1-A] add bounded ZIP catalog and extraction planning` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Extract with bounded output and no-clobber publication

**Objective:** A validated plan can write selected files without escaping or overwriting the destination.

**Code ownership:**

- proposed src/archive/extract.rs
- src/file_operations.rs — narrowly reusable destination/handle primitives only
- proposed src/archive/model.rs — extraction results

**Implementation tasks:**

1. Admit the chosen destination and enforce no-follow confinement for every created parent/file with a race-resistant native boundary. Reject destination substitutions instead of trusting an earlier string check.
2. Stream one selected entry at a time into a unique create-new staging file within the admitted destination. Enforce actual output/entry/job limits before each write and validate integrity before publication.
3. Publish with a true no-clobber operation, preserving existing files even if a race occurs after preflight. Never implement overwrite avoidance as exists() followed by a replacing write/rename.
4. On cancel/error, close handles and remove only owned partial staging artifacts whose identity is still valid. Keep completed output files and record per-entry success/failure/cancellation; never recursively remove the chosen destination.
5. Add fault-injection tests for short/corrupt data, limit exhaustion, output collisions/races, write failure, changed archives and cancellation after some entries finish.

**Behavioral requirements:**

- Only selected safe entries are extracted, preserving their relative directories.
- Completed outputs remain available after cancellation and partial files are distinguished/cleaned without deleting user data.

**Edge cases:**

- Disk full, permission failure, destination junction substitution, path becoming occupied, CRC mismatch, reported sizes smaller than actual expansion, huge single entry, cancellation within decompression and a new file appearing during cleanup.

**Regression constraints:**

- No overwrite and no write outside the admitted destination.
- Output budgets apply to actual produced bytes; declared ZIP metadata is not trusted as enforcement.

**Verification:**

- Run focused extraction tests in disposable directories; assert outside sentinel files and pre-existing destination files remain byte-identical.
- On Windows, test path/case/native-confinement behavior, including a deterministic substitution gate where practical.

**Acceptance criteria:**

- All unsafe/racing writes fail closed and all existing sentinels survive.
- Byte/count limits stop output before exceeding their configured ceilings.
- Integrity failures never publish a successful final file; owned partial cleanup and partial-result accounting pass.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(archive): [M1-B] stream selected entries with confined no-clobber writes` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-C — Own asynchronous listing and extraction lifecycles

**Objective:** Archive work remains responsive, cancellable and bounded across close/reopen and replacement.

**Code ownership:**

- proposed src/archive/session.rs
- src/thread_reaper.rs — reuse only
- existing goal-06 activity interface if present

**Implementation tasks:**

1. Implement generation-tagged background listing/extraction with bounded progress coalescing and one current job/controlled replacement policy.
2. Retain or revalidate the archive revision for extraction; invalidate selection on archive replacement or catalog refresh.
3. Track reading/planning/extracting/stopping/complete/partial/error states and actual processed counts. Cancellation must not be reported as cleanup complete while the worker still owns files.
4. Close and reap owned work without a GUI-thread join; preserve the extraction result report long enough for the user to see partial outputs.
5. Test old completion after replacement, close mid-file, reopen while stopping and exhausted reaper capacity using deterministic gates.

**Behavioral requirements:**

- The UI can request cancellation promptly and sees truthful stopping/partial status.
- Old archive results cannot overwrite a new catalog or run against a changed selection.

**Edge cases:**

- Slow disk/decompression, repeated cancel/open, close before listing completes, archive changed while selected and application shutdown.

**Regression constraints:**

- No unbounded detached threads, messages or retained plans.
- Cleanup affects only this session's handles/staging artifacts and never unrelated processes.

**Verification:**

- Run bounded session/lifecycle tests and one generated moderate archive cancellation observation.
- Inspect owned staging/worker counts after completion and close; do not infer zero leaks merely from a closed window.

**Acceptance criteria:**

- All state transitions and stale-generation fences are tested.
- Cancel/close eventually release owned files, buffers and workers or report a precise genuine external blocker.
- Progress and result retention are bounded.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(archive): [M1-C] manage cancellable ZIP work sessions` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Expose archive preview and selective extraction

**Objective:** The full user workflow is available through the launcher with clear selection, destination and results.

**Code ownership:**

- proposed src/gui/archive_dialog.rs
- proposed src/plugins/archive.rs
- src/plugins_builtin.rs
- src/commands/model.rs
- src/commands/parser.rs
- src/commands/host.rs
- src/commands/handlers — proposed archive handler if required
- src/gui/command_host.rs
- src/gui/mod.rs
- src/gui/render.rs
- README.md and current help

**Implementation tasks:**

1. Register a proposed zip utility prefix after checking for collisions and route it through typed command/host handling. Open via file picker and keep any explicit-path opener bounded and reusable.
2. Build a virtualized searchable/sortable entry list or tree with stable selection identity, folder-subtree selection, supported/problem status and declared sizes.
3. Present the destination/selection summary, no-overwrite collision errors, effective limits and explicit Extract/Cancel actions. Keep output progress and final completed/skipped/failed/cancelled details copyable within a bounded report.
4. Use standard keyboard/Jump registration for discrete controls where the utility pattern supports it, without adding new navigation coverage elsewhere.
5. Run one native session over generated archives: inspect, select a subset, choose a new destination, extract, verify files, attempt a collision and cancel another job. Capture preview, summary and partial-result states; document supported ZIP features.

**Behavioral requirements:**

- The user can inspect before writing, extract only the selected entries and understand every rejected or partial result.
- Existing destination files are never replaced by a UI convenience action.

**Edge cases:**

- No selection, folder selection containing unsupported entries, duplicate visible names, narrow window, cancelled picker, changed archive, existing destination and no free space.

**Regression constraints:**

- Opening/listing never executes contained content or automatically extracts.
- Other launcher commands/file tools retain behavior and plugin enable semantics.

**Verification:**

- Run focused command/plugin/dialog-state coverage plus the catalog/extract/session selections.
- Use one bounded Windows smoke with disposable fixtures, verify extracted bytes/names and unchanged sentinels, and inspect remaining owned staging artifacts.

**Acceptance criteria:**

- Native preview, subset extraction, no-overwrite feedback and cancellation match the contract.
- All selected outcomes are accounted for and no unsafe entry escapes the destination.
- Help and final evidence clearly state compression/encryption/ZIP64 and limit support.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(archive): [M2-A] add ZIP preview and extraction dialog` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use generated archives only: tiny cases for parsing/path safety/integrity and one moderate compressible archive for actual-output limits and cancellation. Never test by extracting an unknown user archive into user directories.
- Separate dependency/application compilation from execution. After the necessary build, allow up to 12 minutes for targeted catalog/extraction/session/command checks and up to 12 minutes for one native smoke. At most two focused remediation attempts; a failed confinement/overwrite test is a blocking defect.
- Verify central-directory allocation bounds, actual decompressed byte limits and destination sentinels explicitly. A passing list preview does not establish extraction safety; a cancelled window does not establish cleanup.
- Use disposable settings with Paste Sequence next/cancel shortcuts disabled. Final first-party lint work remains scoped and end-only; no default full suite, global clean or unrelated dependency upgrade.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| ZIP path tricks or a destination junction race writes outside the chosen root. | Validate Windows components and enforce race-resistant no-follow confinement at each write/publication boundary; test outside sentinels. |
| A parser allocates unbounded central-directory state or decompression exceeds declared sizes. | Enforce pre/during-parse metadata caps and actual streaming output budgets with injectable-limit tests. |
| No-overwrite is implemented as a check-then-write race. | Use create-new staging and true no-clobber publication, with a deterministic collision race test. |
| Cancellation cleanup deletes pre-existing or newly user-created files. | Track exact owned staging identities, preserve completed outputs and avoid recursive destination cleanup. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- ZIP listing, explicit selection, extraction preview, no-overwrite execution and partial-result reporting work.
- Path confinement, output/metadata bounds, integrity and cleanup have passing targeted evidence.
- Background work and UI remain bounded with truthful stopping states.
- Supported/unsupported formats and actual verification limits are documented.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report the final prefix, ZIP library/version/features, effective limits and supported compression/encryption/ZIP64 behavior.
- Provide native preview/extraction screenshots, extracted-fixture verification and no-overwrite/confinement/cancellation results.
- List partial-output cleanup policy, remaining platform limits, commits and push status.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
