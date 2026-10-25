# CODEX KICKSTART — Execute Test Suite Optimization

You are the autonomous Codex orchestrator for **Multi Launcher**, a Rust / egui / Windows desktop application. Your task is to implement the **test-suite optimization Goal**, not alter application functionality. This Kickstart governs **execution and agent coordination**; the Goal governs **technical scope, measurable requirements, milestone acceptance and regression safeguards**.

**Required authoritative file:** `CODEX_GOAL_TEST_SUITE_OPTIMIZATION.md`  
**Repository authority:** `AGENTS.md` and current checked-out code  
**No human checkpoints:** execute and verify autonomously to the defined stop boundary.

## 0. Mandatory first execution: fresh Git branch + own baseline commit + initial upstream push

**Before any application implementation, edit, Cargo build, test, benchmark or profiling run**, do the following. These rules stand independently of the Goal:

1. Read the repository's `AGENTS.md`, applicable local instructions, **this entire Kickstart**, and **the entire `CODEX_GOAL_TEST_SUITE_OPTIMIZATION.md`**. Perform **read-only** repository inspection: `git status -sb`, current branch, `HEAD`, dirty files, upstream/remotes and default integration branch (`master` or `main`, whichever is actually used). Read-only code inspection is allowed before branching.
2. Choose a **new**, uniquely named lowercase kebab-case task branch such as `perf/test-suite-optimization` or a meaningful unique suffixed variant if that branch already exists. **Never reuse an old or currently active task branch.**
3. Prefer the safely fetched, up-to-date tip of the actual default integration branch. Preserve all uncommitted user work. **Never** reset, hard-pull, stash, rebase, merge, force checkout or force push over it. If a dirty checkout makes the preferred base unsafe, create a fresh branch from current HEAD instead **if that is safe**; record the actual base. If neither transition is safe, stop and report a genuine blocker without destructive repair.
4. **Create and switch to the new branch**, then immediately create its own **M0-A initial baseline commit**, before all implementation or measurements. A small task ledger record is preferable if appropriate; an intentionally empty `git commit --allow-empty -m "chore(test-perf): [M0-A] establish test optimization baseline"` is permitted. Do not stage unrelated pre-existing changes. Record the **original base SHA** and **new baseline commit SHA** as separate identifiers.
5. **Attempt the first normal upstream push** (`git push -u origin HEAD`, or equivalent). If denied or unavailable, record its real status and **continue implementation locally without retry loops or security bypasses**.
6. Confirm the task branch remains active. **Every subsequent test, source edit, checkpoint commit and push attempt must run on that new branch**—not `master`, `main` or the previous feature branch. Never change back mid-task.

**M0-A is a hard gate**: finish the new branch, baseline commit and initial push attempt before commencing M0-B or other implementation. The existing default branch and any uncommitted user changes must remain intact.

## 1. Validate the Goal against the live repository

After M0-A, inspect the **current local repository HEAD** and confirm the actual location/shape of:

- `Cargo.toml`, the existing `fast-dev` profile and test targets;
- `dev/README.md`, `dev/track_b/measure.ps1`, `dev/perf_track_d/`, `dev/run_owned_process.ps1`;
- `tests/suites/domain.rs`, `tests/suites/plugin_queries.rs`, the top-level integration test files;
- `src/gui/render.rs` C2G query oracle and its eager geometry comparison;
- any relevant fixture/concurrency tests ranked by actual timings.

Treat `AGENTS.md` as engineering/orchestration guidance and the Goal as the feature-specific contract. Adapt stale paths or names to what exists **without silently changing the scope**. Use only the live checkout for implementation; do not depend on unavailable prior artifacts. Historical Nextest results mentioned in the Goal are **investigation clues, not proof of this branch's baseline**.

## 2. Use the project's configured agents judiciously

The project has a planner/implementer/reviewer orchestration pattern; use it where those roles are available and helpful.

- **Planner (read-only):** validate module ownership, Windows build/test tool availability, exact selected-test selectors, benchmark fairness, likely slow paths, safe milestone sequence and test coverage invariants. Return a bounded milestone packet; do not initiate unrelated roadmap work.
- **Implementer (single writer at a time):** perform one checkpoint at a time, commit coherent verified increments, and keep code/test changes limited to the Goal's ownership. Do not run multiple agents writing the same source concurrently.
- **Reviewer (independent/read-only where possible):** examine key boundaries after C2G changes and any integration-target regrouping; verify assertions/cardinality/fixture independence, production parity, results accuracy, and scoped-diff cleanliness. Do one substantive final review rather than repeated ceremony.
- **Orchestrator (you):** own Git safety, execution ledger, budgets, command orchestration, evidence, cleanup, checkpoint commits, pushes, agent handoffs and final status.

Delegate only when it adds useful independent coverage. Do not invent unavailable tools/agents, wait for manual help, or block on optional agent opinions.

## 3. Execute dependency-ordered milestones

Follow every checklist and acceptance criterion in the Goal. Suggested execution order:

| Checkpoint | Action | Advance only when |
|---|---|---|
| **M0-A** | Create fresh branch, new baseline commit, initial push attempt | Git provenance and worktree safety verified |
| **M0-B** | Add small profiling/Nextest receipts harness using existing utilities | JSON timings and owned failure/timeout paths tested |
| **M0-C** | Collect focused build/run baseline and optional **single** full baseline | Test roster, source identity, time budgets, timings and ranking recorded |
| **M1-A** | Document/make fast target-based local commands with phase/progress output | Exact library and integration selections really run |
| **M1-B** | Compare existing `dev`/`fast-dev` opt-in choices | Fair paired data; no default/release changes |
| **M2-A** | Optimize the expensive C2G oracle's **test-only** repeated work | Four 100-result recipes, 1k and 10k viewport cases and independent checks remain |
| **M2-B** | Optimize up to five *measured* additional slow tests | Correctness and speed evidence per retained candidate |
| **M3-A** | Run **one** small, reversible target-consolidation A/B | Case inventory preserved; measured benefit or revert |
| **M3-B** | Optional single test-only compile hotspot improvement | Only if strongly supported by timings and regression-safe |
| **M4-A / M4-B** | Fix *measured* fixture/isolation or needless waits | Deterministic teardown/waits, unchanged semantics |
| **M5-A** | Compare matched before/after outcomes, one bounded candidate full run maximum | Traceable scoreboard, failure/timeout truth, coverage parity |
| **M5-B** | Independent review, scoped format/lint, final commits and handoff | Acceptance complete or precisely classified blocker |

Do not create more milestones just because additional test cleanup is available. Optional M3-B and M4 changes may be skipped **with evidence**; do not manufacture edits to justify milestone labels.

## 4. Profiling and benchmark discipline

This task includes explicit authorization to write a **dedicated test profiling harness and collect machine-readable timing data**. Reuse known project measurement and owned-process utilities where safe instead of adding competing generic infrastructure.

**Distinguish and record** test compile/link build time, test execution time, per-test wall duration, baseline failures/timeouts/ignored, warm no-op, controlled edited-build and cold profile population. Keep commands, source SHA, active profile/features, tool versions, Windows host/CPU, relevant env/parallelism and cache state in receipts. Use versioned JSON; prefer supported Nextest machine reporters, verifying installed CLI options before invoking them. Raw outputs should stay under unique external/temp evidence directories, not be committed with sensitive local paths. Source-controlled findings should be compact and privacy-safe.

**Fairness requirements:** compare equivalent Cargo targets and selected test names on the same Windows host/profile/features/parallelism; prebuild test binaries to time execution separately. Use three warm selected-build samples when practical, label preparation separately, and use a byte-restored edit fixture with final SHA verification if recompilation is being measured. A new `fast-dev` cache is not directly comparable to an already warm `dev` cache. Record both successful and unsuccessful timed commands; do not count failures as improvements.

**Whole-suite limit:** at most **one current unfiltered baseline and one unfiltered final run**, with a bounded owned process and `--no-fail-fast` where supported. Prefer prebuild/listing before running; never execute repeated full suites during the per-checkpoint optimization loop. If a full baseline is too costly, unavailable or cannot be compared fairly, explicitly mark the **full-suite 25% target unmeasured**. The historic ~264-second run is not an equivalent current baseline. Do not change timeouts, disable test cases, lower parallelism, add retries or alter the test inventory to manufacture a speedup.

**Other limits:** one/two narrow Nextest groups per checkpoint plus at most one causally justified repaired rerun; at most five additional slow-test candidates, one integration-grouping A/B, three wait substitutions, and no repeated expensive release build. Use process-owned deadlines and cleanup; do not kill unowned Windows processes, change unrelated system settings, or remove the user's Cargo cache.

## 5. Hard regression and test-integrity constraints

- **No production/user-visible feature changes:** preserve launcher activation, GUI focus, settings, command and plugin behavior, Windows interactions, hotkeys, persisted formats and shipped binaries. This is not a performance refactor of application runtime.
- **Do not weaken tests:** keep the C2G oracle's four 100-action cases and **1,000/10,000** viewport cases, each step and independent eager geometry/reference comparison, expected-result equality, provider and popup/scroll checks, negative-path coverage, and relevant row/cardinality/state assertions. Optimize repeated test-only computation **without making production output validate itself**.
- **Do not hide failures:** no `#[ignore]`, skipped patterns, silently modified thresholds, elevated timeout, blanket serialization, lower suite parallelism or toggled assertions for convenience. Maintain real Windows COM/native safety where present.
- **Compilation grouping is experimental:** moving an integration file changes Nextest target identity and sometimes process isolation. Before and after, record a full test-name inventory/mapping. Use the existing grouped suite pattern and move only 2–4 demonstrated side-effect-free test files. Revert if improvement is weak or assertions/test discovery/isolation differ.
- **Existing unrelated failures:** preserve receipts and classify them. An isolated pass of a previously timed-out oracle does not establish full-suite reliability. Repair only regressions caused by this task; document unrelated product defects separately.

## 6. Autonomous operation, small commits and safe pushes

Once the M0-A gate is complete, perform ordinary design decisions, implement, verify and proceed without stopping for user approval. Only ask a question for a truly unresolved safety-critical or destructive choice. The user will perform **one optional manual review after final delivery**, not intermediate testing or screenshots. Native GUI interaction is not required merely because this is a Windows app: the task's proof is in tests, test profiles and profiling receipts.

Follow the active checkpoint commit cadence: verify each cohesive increment, inspect its diff, commit on the **new task branch** with subjects identifying `M#-X`, attempt a best-effort normal push, record success/failure, and proceed locally even if a push fails. Stage no unrelated pre-existing files and never amend/squash or rewrite the baseline. Keep baseline/original-base SHAs distinct. Do not start Clippy after every checkpoint: run the appropriate first-party lint at the final review boundary, excluding vendored cleanup.

If a test fails, first establish whether it is a regression, baseline failure, unsupported environment or harness defect. Investigate only the directly relevant owner; apply one bounded correction and one exact retest if justified. If the repair would require a broad refactor or changing production behavior, stop that subtask, report the evidence and continue independent completed checkpoints where safe. Do not turn a non-green full run into an endless qualification campaign.

## 7. Acceptance gate and stop boundary

The Goal is complete **only** when:

1. New task branch and **its own M0-A baseline commit** exist, an upstream push was attempted, and all implementation/checkpoint commits remain on it.
2. The profiling workflow reports honest source-bound timings and status, and focused everyday commands work with simple PowerShell usage.
3. The C2G oracle still tests all original cases/sizes with a truly independent expected result; directly affected tests pass, and any retained optimizations have verification evidence.
4. Changed target/test roster and meaningful regression assertions are preserved. No known task-introduced flake, runaway test, process leak, user data mutation, profile default change or product behavior regression remains.
5. Before/after warm-build and execution scorecards disclose equivalent measurements and limitations. The **20% / 25% improvement thresholds are goals, not pass/fail permission to cheat**. A measured miss is a valid honest result; a silent metric substitution is not.
6. Final scoped reviewer, format, relevant checks and first-party Clippy are completed or accurately recorded as blocked, and all retained changes/receipts are committed on the fresh task branch.

**Stop at this boundary.** Do not proceed to speculative compiler tuning, unrelated feature work, test rewrites, or historical acceptance initiatives.

## 8. Required final response to the user

Provide a concise, concrete final report with:

- **Git:** old branch, actual base SHA, **fresh branch name, M0-A baseline SHA**, subsequent checkpoint SHAs, initial/final push results, preserved dirty-work disposition;
- **Milestones:** complete/skipped/blocked with important implementation decisions and exact files changed;
- **Performance:** baseline versus after for the selected **warm test build**, oracle/slow tests, and full **execution** if comparable; wall seconds, median/sample count, percentage and source/profile/feature/parallelism; label historic ~264 s as historical when it cannot be paired;
- **Coverage:** original/mapped current test roster and preserved assertions, particularly 10,000-result oracle; observed PASS/FAIL/TIMEOUT/SKIP counts, including unexplained full-suite failures;
- **Verification:** exact commands genuinely executed and results, bounds reached, first-party lint/reviewer status, JSON receipt locations and one-line PowerShell usage;
- **Limitations:** genuine remaining risks and a brief final manual-review note only if needed.

Do not claim completion for any command, benchmark, UI test or cleanup you did not actually execute. Give the user the results now when execution ends—do not promise asynchronous later work.
