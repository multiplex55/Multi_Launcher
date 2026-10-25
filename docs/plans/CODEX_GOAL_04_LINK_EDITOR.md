# Codex Goal 04 — Faster Windows linking and responsive Neovim rust-analyzer workflows

**Companion execution prompt:** [CODEX_KICKSTART_04_LINK_EDITOR.md](CODEX_KICKSTART_04_LINK_EDITOR.md)  
**Requested coverage:** #125, #126  
**Program position:** 04 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/windows-link-editor` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish link-editor branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Shorten the remaining Windows executable/test-link wait and improve rust-analyzer responsiveness in the user's Neovim workflow using measured, reversible project-scoped settings. Keep normal full-debug and canonical release behavior available, disclose any symbol/cache tradeoffs, and distinguish real editor response measurements from version checks or headless analysis throughput.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [03 — Measured compilation boundaries, dependency cleanup, and test target consolidation](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/Cargo.toml)** — `profile.fast-dev and profile.iteration, lines 269–281`. fast-dev already uses line-table debug information, while iteration already changes optimization/codegen/LTO/incremental settings. These are existing modes, not new optimization proposals, and canonical release has not been replaced.
- **[dev/README.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/README.md)** — `Opt-in reduced-symbol development profile; Profile boundaries and environment`. The guide explains separate profile caches, reduced variable-debugging information, environment overrides, and historical host-specific measurements. It explicitly warns that process wall time is not isolated linker time.
- **[dev/track_b/measure.ps1](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/track_b/measure.ps1)** — `ArgumentList, target-directory resolution and recorded LinkerDuration`. The measurement owner preserves actual arguments/provenance and labels linker duration unmeasured by whole-process wall time. It should be extended or complemented narrowly, not relabeled as a linker profiler.
- **[dev/perf_harness/README.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/perf_harness/README.md)** — `Rust Analyzer external-command guidance, lines 43–57`. Existing documentation separates version output, bounded analysis-stats and actual editor responsiveness. The registered probe runner is not an arbitrary rust-analyzer launcher.
- **[build.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/build.rs)** — `Windows resource compilation`. The application embeds Windows resources with explicit change dependencies; any accepted link configuration must preserve the icon/resource behavior.
- **[dev/track_b/assert-icon-resource.ps1](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/track_b/assert-icon-resource.ps1)** — `existing resource verification helper`. A focused Windows executable resource check already exists and can be reused for accepted linker candidates.
- **[dev/README.md](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/dev/README.md)** — `Repository workflow configuration context`. No tracked .cargo, .config or .vscode configuration was present in the inspected tree. Actual machine-level Cargo flags and Neovim/LSP settings are unknown until the execution environment is inspected; do not assume VS Code or overwrite personal settings.

## C. Approved functional requirements

1. Inspect the actual Windows toolchain, configured linker, effective Rust/Cargo flags, output profiles and PDB behavior before choosing candidates. Inspect the installed Neovim version, active rust-analyzer executable/version and effective LSP initialization/check configuration without exposing unrelated personal settings.
2. Measure an application edit/relink and one selected test-binary link separately from a no-op build. Attribute link/PDB duration only when the actual linker process or an equally valid boundary is observed; otherwise report whole-command evidence and the missing attribution explicitly.
3. Evaluate at most two well-supported link/debug-information candidates using already available supported tools. Make an accepted candidate an explicit project-scoped opt-in; preserve normal full-debug builds, canonical release artifacts, icon resources and current dependency patches.
4. Do not claim a lower-symbol mode retains full variable debugging. Keep a documented full-debug escape hatch and verify the debugging behavior appropriate to each accepted mode. Do not introduce an unsupported linker flag, automatic global environment change or mandatory new tool installation.
5. Deliver a reversible project-scoped rust-analyzer configuration for the actual Neovim integration convention. Prefer focused check-on-save targets/features and measured resource settings; keep semantic completion/navigation and necessary proc-macro/build-script support. If a light check mode omits other-target compiler diagnostics, label that scope explicitly and retain an accessible full/test check mode.
6. Do not disable all diagnostics, test analysis, proc macros or build scripts to manufacture a speedup. Do not overwrite global Neovim configuration or enable repository-local script trust/security settings. Use an existing repository-scoped LSP hook when available; otherwise provide an opt-in adapter/profile and validate it in an owned editor session.
7. Compare shared versus separate Cargo artifact directories only when lock contention is actually observed. Report storage and repeated-build costs before adopting a separate cache; retain explicit mode selection and avoid silently switching existing compiler profiles.
8. Measure an actual controlled Neovim/rust-analyzer session for project-open readiness, a small edit-to-diagnostic interval and a representative completion/navigation request where supported, including coexistence with one terminal build. A version command or analysis-stats run is not proof of interactive improvement.
9. If the actual editor/interactive environment cannot be exercised, finish the safe configuration/documentation work and report editor responsiveness as unverified/incomplete. Likewise, an unsuccessful linker experiment is not a completed speed improvement. No intermediate user-run commands or screenshots are required.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- No compiler-graph/package reorganization, dependency upgrade, production feature change, global editor migration, replacement of Neovim/Neovide, or broader benchmark campaign.
- No compulsory alternative linker, global Rust flags, disabled correctness diagnostics, destructive cache cleanup, personal-config overwrite, or security/trust bypass.
- Do not reinterpret historical Track B timing tables as current-machine evidence, claim GPU/native presentation from headless probes, or require a full Rust suite for tooling configuration.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Treat linker and editor changes as independent opt-in configurations with separate comparison records. Reuse the repository's existing modes, bounded ownership/provenance and resource checks, including any already-integrated developer-workflow improvements.
2. Keep exact resolved configuration in the receipt: linker executable/version, effective relevant arguments, compiler profile/target/features, artifact directory, PDB size, source edit and cache preparation. Observe the responsible phase before attributing a gain.
3. Use official installed-version documentation for Cargo/linker and rust-analyzer options. The current rust-analyzer configuration reference is https://rust-analyzer.github.io/book/configuration.html; its existence does not establish that every option is supported by the user's installed server/client.
4. For Neovim, preserve the actual client integration and scope overrides by the resolved project root. Keep any new adapter/profile under a clearly labeled proposed dev/editor location and avoid implicit execution of repository-local code. Validate effective settings seen by the server, rather than only the text of a configuration file.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Establish attributable link and editor baselines

**Objective:** Identify actual tool/configuration costs and bounded reproducible scenarios before tuning them.

**Code ownership:**

- dev/track_b/measure.ps1
- dev/perf_harness/README.md
- Actual read-only Cargo/linker/Neovim configuration
- Proposed narrow dev/editor or link-observation helper only where existing tooling lacks required evidence

**Implementation tasks:**

1. Record installed executables/versions, effective project/global overrides relevant to this repository and the normal build/edit commands. Avoid collecting unrelated environment or editor content.
2. Choose one application edit/relink and one selected test-binary build, distinguishing preparation and no-op work. Observe the real linker/PDB phase where supported without requiring privileged tracing as a hidden prerequisite.
3. Start an owned Neovim session using the installed integration and a controlled source-edit scenario; record actual LSP startup/readiness and diagnostic/completion boundaries. Preserve the user's files and settings.
4. Set finite budgets before running and retain unavailable measurements with explicit reasons. Do not repurpose the restricted performance-probe runner as an arbitrary command launcher.

**Behavioral requirements:**

- Every baseline identifies the real command, effective settings and measurement boundary; unavailable phases are clearly distinguished from zero cost.

**Edge cases:**

- Different compiler/linker selected by environment, pre-existing warm PDB/cache, absent Neovim/server, noninteractive desktop, diagnostic request cancelled, denied process observation.

**Regression constraints:**

- Read-only personal configuration inspection, exact restoration of controlled edits, no global configuration or security changes.

**Verification:**

- Check receipt consistency against actual invoked process/configuration; run one bounded baseline for each available scenario and no broad suite.

**Acceptance criteria:**

- The baseline is sufficient to choose a concrete candidate or precisely identifies the external measurement block; version-only/editor-headless results are not labeled interactive success.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(dev): [M1-A] record attributable link and editor baselines` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Implement a validated opt-in Windows link/debug mode

**Objective:** An accepted configuration reduces the relevant link/build wait while retaining its declared executable and debugging contract.

**Code ownership:**

- Cargo.toml existing profiles only where a justified change is needed
- Existing developer build-mode helper
- Proposed project-scoped link configuration/adapter
- dev/track_b/assert-icon-resource.ps1
- dev/README.md

**Implementation tasks:**

1. Select a candidate from observed cost and tools already available. Review actual Windows linker support and incompatible defaults before constructing arguments; do not assume historical fast-link options remain valid.
2. Implement the candidate as a named opt-in through the existing workflow convention. Preserve ordinary full-debug and production invocations and avoid leaking flags into unrelated targets/profiles.
3. Build the same application and selected test targets, validate output resources/startup and execute the relevant tests. Verify line/backtrace or full-debug capability appropriate to the candidate's declared symbol mode.
4. Compare equivalent warmed edit/relink scenarios and retain only a justified improvement. If neither of two bounded candidates helps safely, preserve the baseline configuration and record the linking objective unachieved.

**Behavioral requirements:**

- Developers can select the measured faster mode and return to full debugging or canonical release with an explicit command.

**Edge cases:**

- Linker unavailable, stale PDB, incremental-link incompatibility, anti-virus/file locking, missing icon, crash symbolication limitations, feature-gated native binaries.

**Regression constraints:**

- No global RUSTFLAGS/toolchain replacement, loss of supported resources or silent reduction of the default debugging contract.

**Verification:**

- One paired candidate comparison, focused icon/resource check, one bounded responsive launch/close and selected test execution. Inspect debug/backtrace evidence for the supported symbol mode.

**Acceptance criteria:**

- The opt-in mode works and has measured benefit with honest phase attribution; default modes remain usable and tradeoffs are documented. No-gain results do not count as a completed performance objective.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(build): [M1-B] add a validated opt-in Windows link mode` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Configure project-scoped Neovim rust-analyzer behavior

**Objective:** The editor uses an explicit, reversible configuration that reduces unnecessary checks/lock contention while preserving useful diagnostics and navigation.

**Code ownership:**

- Actual Neovim LSP configuration seam when repository-managed and available
- Proposed dev/editor project profile and adapter
- dev/README.md
- Existing build-mode/target selection metadata

**Implementation tasks:**

1. Inspect how the installed Neovim configuration starts rust-analyzer; use its supported root-scoped settings mechanism and preserve unrelated user options. Do not invent a VS Code settings file as the primary deliverable.
2. Define a lightweight project check mode using actual application/library targets and needed features, alongside the existing full/test-check path. Keep proc macros, necessary build scripts and semantic assistance functional.
3. Measure whether a separate artifact directory actually resolves observed contention. Adopt it only as an explicit option with recorded disk/cache costs; otherwise keep the existing shared cache.
4. Provide a reversible adapter/profile application path and verify the server's effective initialization/check settings in an owned editor session. Do not enable exrc/trust or silently modify a global configuration file.
5. Update documentation and any integrated mode selector so the editor profile cannot accidentally conflict with the chosen compiler profile or use a different unreported server.

**Behavioral requirements:**

- Typing/checking uses the declared scope, full/test diagnostics remain deliberately available, and the configuration is confined to this project.

**Edge cases:**

- Older Neovim/server option schema, multiple attached clients, client overriding server defaults, proc-macro/build-script data, moved package targets, duplicate caches, absent real editor configuration.

**Regression constraints:**

- Completion/navigation and relevant error reporting remain functional; no global editor rewrite or blanket disabling of analysis.

**Verification:**

- In an owned session, observe effective settings, introduce and restore a harmless error in the selected target, request completion/navigation and run the explicit test/full-check path. Confirm overrides do not apply to an unrelated temporary project.

**Acceptance criteria:**

- The actual client accepts the scoped profile, reports the controlled error and preserves semantic assistance/full-check access; configuration scope and restoration are verified. If the actual client is unavailable, mark this validation incomplete.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(editor): [M2-A] add scoped Neovim rust-analyzer modes` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Compare editor responsiveness and finalize tooling guidance

**Objective:** The final report establishes what improved in the real workflow and how to choose or revert each mode.

**Code ownership:**

- Project-scoped link/editor configuration and measurement helpers
- dev/README.md
- Existing sanitized performance summaries

**Implementation tasks:**

1. Run equivalent baseline/candidate editor scenarios for readiness, edit-to-diagnostic and a representative semantic request, including one concurrent terminal build when contention is part of the proposed gain.
2. Separate first cache population from warmed use, record server/client versions and settings, and account for extra artifact storage. Do not infer GUI responsiveness from analysis-stats throughput.
3. Document concrete supported commands/configuration application and reversion, symbol limitations, check scope and required tools using the installed versions actually verified.
4. Review all changed configuration for global leakage, incompatible flags and stale target names; remove rejected candidates and preserve factual blocked/no-gain outcomes.

**Behavioral requirements:**

- Users can choose a supported mode with clear expectations for symbols, diagnostics, cache usage and measured response.

**Edge cases:**

- Noisy editor startup, background indexing, same source revision with different caches, incomplete LSP event logs, link improvement coupled with a worse edit/diagnostic cycle.

**Regression constraints:**

- Current canonical release/debug workflows remain available; neither missing measurements nor failed native/editor checks become passing claims.

**Verification:**

- One paired comparison for each available scenario, with at most three repeats only if noise prevents interpretation. Finish with relevant script/configuration checks and a focused application/test check if linker settings changed.

**Acceptance criteria:**

- Accepted link and editor changes have explicit achieved/unachieved evidence and reversible instructions. Both requested speed objectives must be supported before the whole goal is marked complete; absent environment or no-gain outcomes are reported precisely.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `docs(dev): [M2-B] document measured link and editor workflows` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use at most two justified link candidates and one scoped editor profile with a measured cache option. Avoid an open-ended matrix of linkers, profiles and editor settings.
- Set a 30-minute deadline per selected compile/link command, a 5-minute deadline per editor readiness/diagnostic scenario, and a 2-minute post-build application smoke deadline. Adjust a known legitimate requirement before the run with a recorded reason, not by silently restarting stalled work.
- Use one baseline/candidate comparison per scenario plus at most three paired repeats if variance is unresolved. Record warm/cold preparation separately and keep existing target directories.
- Validate actual effective flags and client settings, meaningful executable/resource behavior, declared debug-symbol capability and an explicit diagnostic/semantic request. Configuration text or a successful version command alone is insufficient.
- No full test suite is automatic. Use the affected selected binary/tests, resource check and one owned startup/shutdown smoke, then end-only first-party lint/review as applicable.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| A faster linker or smaller PDB silently weakens debugging or executable compatibility. | Keep the mode opt-in, verify resources/startup and the stated symbol behavior, and preserve full-debug/release escape hatches. |
| Editor tuning appears fast because it stopped checking useful code. | Declare check scope, retain semantic features and full/test checks, and verify an actual controlled error plus completion/navigation. |
| A separate rust-analyzer cache reduces lock stalls but duplicates excessive work/storage. | Adopt only after an observed contention comparison; report disk/population costs and keep an explicit shared-cache mode. |
| The execution host cannot observe the user's actual Neovim workflow. | Complete safe scoped configuration and controlled-session checks where possible, but leave real editor responsiveness explicitly unverified rather than substituting analysis-stats or user intervention. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Any accepted link configuration is project-scoped, reversible, compatible with the application and honest about its symbol tradeoffs.
- The Neovim rust-analyzer profile is supported by the installed client/server, confined to this project, and retains meaningful diagnostics/navigation plus a full-check path.
- Equivalent measurements establish the achieved link and editor improvements; unsupported environments or absent gains remain clearly incomplete.
- Documentation describes exact verified modes, cache costs, effective settings, reversion and limitations without altering global defaults.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Report actual linker, Rust/Cargo, Neovim and rust-analyzer versions; effective configurations, artifact/PDB sizes and any storage tradeoff.
- Provide separate application/test-link and editor readiness/diagnostic/semantic timing receipts, native/resource/debugging evidence, accepted/rejected candidates and every unverified claim boundary.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
