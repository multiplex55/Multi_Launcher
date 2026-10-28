# Codex Goal 03 — Measured compilation boundaries, dependency cleanup, and test target consolidation

**Companion execution prompt:** [CODEX_KICKSTART_03_COMPILE_DEPENDENCIES.md](CODEX_KICKSTART_03_COMPILE_DEPENDENCIES.md)  
**Requested coverage:** #120, #121, #122, #123, #137  
**Program position:** 03 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/compile-boundaries` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish compile-dependencies branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Reduce the work required after representative small edits and when compiling a selected group of tests. Use measured Cargo target/dependency boundaries, remove demonstrated unnecessary enabled capabilities, and consolidate genuinely compatible tiny integration binaries. Preserve application features, existing public module contracts and all meaningful test coverage; show which compiler/link work actually disappeared.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/lib.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/lib.rs)** — `public module declarations`. Most application domains and the GUI are built through one library. Filtering cases within its test target does not itself partition compilation of that target.
- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `autobins, explicit developer binaries, test targets and fast-dev/iteration profiles`. Developer binary gating and faster opt-in profiles already exist; two named integration suites already consolidate selected side-effect-free tests. Broadly recreating these features would duplicate completed work.
- **[tests/suites/domain.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/tests/suites/domain.rs)** — `path-based domain test modules`. The repository already has a concrete grouping pattern for related test files. Other top-level tests are still independently auto-discovered; the inspected tree contains 67 such .rs files, which must be re-counted at execution.
- **[tests/suites/plugin_queries.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/tests/suites/plugin_queries.rs)** — `path-based plugin test modules`. Plugin-query cases are already grouped, so additional grouping must target remaining compatible cases and avoid duplicate automatic discovery.
- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `dependency features at lines 69–225 and patch.crates-io at 283–291`. Many dependencies already have narrowed features. Image 0.24 and 0.25 are explicitly retained for distinct existing paths and format compatibility, and several GUI/image crates are locally patched.
- **[Cargo.lock](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.lock)** — `duplicate image, windows, windows-sys and other package versions`. Multiple generations exist, but a lockfile count alone does not establish active Windows compile cost, compatibility, or safe removability.
- **[src/table_utility.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility.rs)** — `TableDocument, TableProjection, parser/export types and inline tests`. This inspected engine uses standard-library types plus csv/serde_json and has no crate:: or GUI/Windows import in the reviewed source. It is one concrete candidate for a narrow independent compilation boundary; its actual dependency graph and benefit still require measurement.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `crate::table_utility imports`. The session and GUI consume the engine through its module API. Preserving that root module contract would limit migration churn if the engine becomes an internal package.
- **[build.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/build.rs)** — `main`. Resource rebuilding already has explicit rerun-if-changed entries for the resource script and icon; broad build-script rewriting is not supported by the inspected evidence.

## C. Approved functional requirements

1. Establish separate representative scenarios for a leaf implementation edit, a selected-test edit/build and a large-owner edit. Record the actual source diff, Cargo target/profile/features, active Windows dependency graph, compiler versions, cache history and timing meaning. Compare equivalent warmed states; preparation is not an edited-build sample.
2. Optimize existing target selection before broad structural changes. Preserve all developer-bin feature gates and established fast-dev/iteration/release semantics.
3. Audit enabled dependency features and duplicate versions against real call sites, reverse dependencies, local patches and supported formats/platform behavior. Remove or narrow only capabilities shown unnecessary; do not force incompatible versions to converge or upgrade unrelated packages to reduce a count.
4. Consolidate a small coherent group of remaining tiny side-effect-free integration binaries where measured build/link overhead warrants it. Retain meaningful separate boundaries for native resources, CWD/environment mutation and independent process behavior.
5. Create at most one initial narrow internal compilation package when evidence justifies it, using the table engine as the first concrete candidate to inspect. Preserve the public multi_launcher::table_utility module contract with a deliberate re-export or equivalent stable boundary; move its implementation/tests rather than compiling a duplicate second copy. A different owner requires a recorded source-backed justification.
6. Keep selected engine tests executable without pulling the whole GUI/application dependency graph when the new boundary is accepted. Preserve ordinary application builds and default discovery of required correctness tests; update already-integrated feature selectors and developer commands.
7. Changing source-file layout alone is not an accepted compilation improvement. Required evidence includes work/target changes and equivalent before/after timings for both a representative small application edit and selected-test compilation.
8. Keep only behavior-preserving candidates with demonstrated benefit or a clearly established reduction in requested work. An investigated candidate with no gain may be rejected and documented; failure to achieve a requested speed objective remains unachieved/blocked, not silently marked complete. Individually incompatible duplicate versions may remain with a concise reason.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No application-wide crate split, unrelated feature removal, general source reorganization, wholesale API redesign, all-dependency upgrade, vendored cleanup, or lockfile churn unrelated to accepted changes.
- No new user-facing workspace features, change to canonical release optimization, alternative-linker rollout or editor settings; linker/editor work belongs to Goal 04.
- Do not disable correctness tests, hide them behind opt-in features, globally serialize them, delete caches, or use a small no-op timing as proof of a meaningful edit-build speedup.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Use existing Track B measurement/provenance and any integrated Goal 01 helpers. Discover actual installed Cargo graph/timing commands, then distinguish dependency, first-party code generation and link costs rather than attributing the whole wait to a large file.
2. Apply dependency/feature reductions and test-target consolidation as separate coherent changes. Their evidence and rollback are then understandable without conflating multiple candidates.
3. For a justified core package, keep its dependency direction toward pure data/algorithms. Application sessions, GUI, native APIs and global startup owners remain in the application package. Preserve module consumers through the existing root API while making the core tests addressable directly.
4. Use an explicit before/after discovery map to ensure that moved test files are neither lost nor compiled twice. Update test selections after names/targets change, and retain a small real application integration test of the new boundary.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Measure active compile costs and narrow unnecessary dependency work

**Objective:** Establish trustworthy scenarios and apply only source-justified dependency/feature reductions.

**Code ownership:**

- Cargo.toml
- Cargo.lock
- dev/track_b/measure.ps1 and existing measurement docs
- Direct call sites for candidate dependencies

**Implementation tasks:**

1. Capture the active native-target dependency/feature and duplicate-version graph, installed tools and one bounded baseline per required scenario. Preserve existing caches and record source restoration exactly.
2. Trace the most consequential candidate feature/duplicate back to real consumers and local patches. Review image formats, Windows COM/API requirements and optional developer-target needs before changing anything.
3. Apply a small justified reduction, retaining default production behavior and needed optional harness capabilities. Re-resolve the lockfile using the repository's normal supported toolchain.
4. Record retained duplicates whose convergence would require unrelated major migrations; stop expanding that audit once the high-cost candidates have a supported disposition.

**Behavioral requirements:**

- Application capabilities and supported formats remain available while any removed compile work is explained by the graph.

**Edge cases:**

- Target-specific dependencies absent from the current host graph, dev-feature unification, patched crates, feature names used by generated code, incompatible major API generations.

**Regression constraints:**

- No removal justified solely by duplicate counts; no broad version upgrade, production feature loss or accidental harness deactivation.

**Verification:**

- Compare the active graph before/after; compile the affected targets and run the narrow behavior tests for any narrowed feature or changed dependency. Do not rerun unrelated format/network/native matrices.

**Acceptance criteria:**

- Every dependency change has a traced consumer/compatibility rationale and passes affected checks; retained duplicates are justified, and the baseline provenance is reproducible.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(build): [M1-A] narrow demonstrated dependency compile work` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Consolidate compatible tiny integration targets

**Objective:** Reduce redundant integration binary build/link overhead without introducing shared-state coupling or losing tests.

**Code ownership:**

- Cargo.toml explicit test targets
- tests/suites/domain.rs
- tests/suites/plugin_queries.rs
- Selected remaining top-level tests/*.rs
- Any integrated feature-selection catalog

**Implementation tasks:**

1. Choose one coherent initial batch of no more than eight small targets using measured build/link cost and source inspection of globals, CWD, environment, native resources and lifecycle assumptions.
2. Move the selected cases under the established suite pattern or a clearly named proposed suite. Ensure Cargo no longer auto-discovers a second top-level copy.
3. Preserve assertions and test purpose; update module/target names, helper visibility and selectors only as needed by the move.
4. Generate a before/after test-discovery map and record which binary count/dependency-link steps disappeared.

**Behavioral requirements:**

- Developers can still select the same behavioral groups, and required cases execute once under their documented target.

**Edge cases:**

- Duplicate test names across old targets, relative fixture paths, cargo test shared-process semantics, binary-specific environment, native globals and accidental double discovery.

**Regression constraints:**

- No grouping of incompatible stateful/native tests solely for fewer binaries; no coverage deletion or hidden ignored cases.

**Verification:**

- Compare case inventory and run only the moved group plus any directly affected fixture tests. Measure its selected compilation once against the equivalent baseline.

**Acceptance criteria:**

- Every moved case has a retained identity mapping, runs once, and passes; the selected grouping has measured or directly evidenced lower redundant build/link work without a material execution regression.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(test): [M1-B] consolidate compatible small integration targets` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Establish one measured independent core compilation boundary

**Objective:** A selected pure engine can be checked and tested independently of unrelated GUI/application compilation.

**Code ownership:**

- src/table_utility.rs as first inspected candidate
- src/lib.rs
- src/table_utility_session.rs and src/gui/table_utility_dialog.rs consumers
- Cargo.toml and Cargo.lock
- Proposed narrowly scoped internal core package

**Implementation tasks:**

1. Revalidate the candidate engine's dependency direction and public types on the execution base. Confirm the extraction addresses measured selected-test or small-edit work; reject a broad split undertaken merely for file size.
2. Create the smallest internal package owning the existing implementation and its meaningful unit tests, with only required dependencies. Preserve root module/API consumers through an intentional stable boundary.
3. Migrate test ownership without include/path tricks that compile the same implementation twice. Keep application session/native/UI responsibilities outside the core package.
4. Update direct package/test selection and existing developer workflow metadata. Preserve default required-test discovery or provide an explicit documented equivalent in the normal workflow.
5. Check a representative application consumer and the extracted engine tests, including its error/cancellation boundaries that cross the application interface.

**Behavioral requirements:**

- A targeted core test invocation avoids the GUI dependency graph; the application exposes the same feature behavior and compatible module API.

**Edge cases:**

- Private helpers referenced by sibling tests, type identity changes, feature leakage, duplicate source compilation, default Cargo package selection changing after package introduction, later goals encountering relocated owners.

**Regression constraints:**

- One implementation owner, unchanged persistence/command behavior, no new production framework or broad public API migration.

**Verification:**

- Inspect the selected core target's graph; run its tests and one real session/UI integration selection. Measure a controlled core edit and its selected-test compilation with the same toolchain/profile/cache preparation.

**Acceptance criteria:**

- The core is genuinely independently addressable, required tests remain part of the documented normal workflow, application consumers compile/pass, and measured results justify keeping the boundary. Otherwise reject the candidate and report the speed objective still unresolved.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `refactor(build): [M2-A] isolate a measured core compilation boundary` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify edit-build improvements and document the new target map

**Objective:** The accepted changes have clear measured benefits and an accurate developer path for future feature work.

**Code ownership:**

- dev/README.md
- Existing build measurement summaries and selection catalog
- Cargo.toml/Cargo.lock and cumulative touched call sites

**Implementation tasks:**

1. Repeat the representative leaf/application edit and selected-test compilation scenarios in an equivalent warmed state; separate preparation, no-op, code generation, link observations and execution.
2. Check for changed feature/profile/target configuration that could invalidate the comparison and report unavailable phase measurements explicitly.
3. Document target/package selection, preserved public module ownership, any moved test identities and retained duplicate-dependency reasons.
4. Review the full graph/source diff for duplicated implementation, unintended default-target changes, unsupported dependency claims and unnecessary formatting churn. Keep only accepted candidates.

**Behavioral requirements:**

- The next goal can locate the true implementation owner and run the narrow relevant tests without rediscovering the build graph.

**Edge cases:**

- Noisy timings, antivirus/background activity, cold new package cache, different compiler flags, a test rename breaking replay, unresolved runtime integration evidence.

**Regression constraints:**

- Canonical release behavior and production feature coverage remain unchanged; wall time is not mislabeled isolated linker duration.

**Verification:**

- One baseline/candidate comparison per required scenario, with at most three paired repeats only if variance prevents a conclusion; one relevant application build/check and the affected tests, then final first-party lint/review.

**Acceptance criteria:**

- Both requested small-edit and selected-test objectives have explicit achieved/unachieved evidence. Completion requires demonstrated accepted improvements and preserved functionality; an audit-only result or unavailable comparison is not reported as the performance goal complete.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `docs(build): [M2-B] record compile improvements and current target ownership` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Build graphs and test inventories are read-only evidence, not test passes. Compile and execute the actual moved/affected targets before accepting an implementation checkpoint.
- Allow one baseline/candidate comparison for each representative scenario and at most three paired repeats when noise is material. Limit structural exploration to one initial core boundary and at most two justified candidate approaches; do not turn failure to gain speed into an unlimited package redesign.
- Use a 30-minute compilation ceiling per selected command and a 10-minute ceiling for the affected execution batch, with any justified larger known requirement recorded before launch. Keep preparation outside measured edit intervals.
- Use existing caches and preserve dirty user work. Controlled fixture edits must be exactly restored and verified, with no git reset, cargo clean or branch rewriting as benchmark cleanup.
- Validate affected formats/platform features when dependencies change, and one genuine application consumer of the accepted core boundary. A complete repository test suite is required only if an evidenced interface impact cannot be covered by these selections.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| A structural split increases build overhead or merely moves cost. | Measure equivalent small-edit and selected-test cases, inspect actual dependency work, and reject the candidate if benefit is not established. |
| Dependency convergence removes format/API capability or breaks a local patch. | Trace reverse dependencies and patches, retain incompatible generations with a reason, and test only the directly affected capability boundaries. |
| Target consolidation loses default discovery or creates shared-process coupling. | Map every moved case, validate one-time discovery and both process assumptions, and keep resource-dependent targets separate. |
| Future table work assumes the old implementation path. | Preserve the public module contract and document the actual relocated owner; later goals must revalidate source locations after their own branch bootstrap. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Required selected correctness coverage is retained and discoverable, and every accepted dependency/target/boundary change passes its focused verification.
- Representative small application edits and selected-test compilation have recorded equivalent before/after evidence showing accepted improvements; missing gains remain honestly unresolved.
- Retained dependency duplicates have supported compatibility reasons and no unnecessary package/API changes remain.
- The current developer target map and module ownership are accurate for downstream work.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report changed dependency features/versions with rationale, retained duplicate groups, old/new test targets and the extracted owner if accepted.
- Show separate preparation/no-op/edit/test-compile results, host/tool/profile/cache provenance, actual graph/work changes and any objective that did not achieve a measurable improvement.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
