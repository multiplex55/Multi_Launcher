# Codex Goal 06 — Application activity, cancellation recovery, and resource stability

**Companion execution prompt:** [CODEX_KICKSTART_06_ACTIVITY_CANCELLATION.md](CODEX_KICKSTART_06_ACTIVITY_CANCELLATION.md)  
**Requested coverage:** #115, #117, #118  
**Program position:** 06 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/activity-cancellation` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish activity-cancellation branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Show what Multi Launcher is currently doing, make cancelled work release its owned resources promptly, and establish bounded long-session behavior for repeated tool use. The activity view must be useful without becoming a new source of background work. Improve measured cancellation and retention problems at the existing owners; do not replace their execution engines.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [03 — Measured compilation boundaries, dependency cleanup, and test target consolidation](CODEX_GOAL_03_COMPILE_DEPENDENCIES.md), [04 — Faster Windows linking and responsive Neovim rust-analyzer workflows](CODEX_GOAL_04_LINK_EDITOR.md), [05 — Keep Plugin Settings usable with long descriptions](CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `run_runtime_tick; run_background_after_menu`. The root coordinates many existing owners and polls their published completions; visibility and repaint handling are already distinct. Activity reporting belongs at owner publication boundaries rather than inside an unbounded per-frame scan.
- **[src/performance.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/performance.rs)** — `Metric; MetricSnapshot; MetricTimer; snapshot_metrics`. Fixed-memory opt-in counters already describe notes, history, row construction, indexing and cursor work. They are not accurate per-feature process memory or CPU attribution.
- **[src/thread_reaper.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/thread_reaper.rs)** — `ReapPermit; CompletionNotifier; reap_with_callback`. A bounded completion-driven join supervisor already supports off-thread retirement. It supplies a real cleanup boundary that should be reused instead of detaching arbitrary workers.
- **[src/duplicate_finder/session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/duplicate_finder/session.rs)** — `session cancel/close/reopen and generation ownership`. A session implementation already models cancellation, retirement and stale-result rejection; it is a reference pattern, not a scheduler to transplant wholesale.
- **[src/table_utility_session.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/table_utility_session.rs)** — `TableWorker; TableUtilitySession; close; shutdown`. The table has source/view identities and a cancellable worker; close cancels but retains document state. Useful reopen state and unnecessary retention must be distinguished.
- **[src/pinboard/live.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/pinboard/live.rs)** — `PinLiveFrameSlot; frame leases; GPU budget`. Live capture publishes a bounded latest-frame slot and already charges retained image leases. The activity view should expose such genuine accounting without double-counting shared leases.
- **[src/hotfolder/runtime.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/runtime.rs)** — `snapshot at reviewed lines 546–575; GUI caller in run_runtime_tick`. The existing snapshot method sends WorkerMessage::Snapshot and synchronously waits on reply_rx.recv(). Its name does not imply a cheap read model; the activity view must not call this blocking method while painting. This is a source finding, not a measured latency claim.

## C. Approved functional requirements

1. Provide a searchable activity/resource dialog, accessible by a discoverable activity command through the existing command/plugin registration and menu conventions. Show owner/tool, current operation, state, elapsed time, meaningful progress, resource facts and the last bounded error or cancellation outcome.
2. Inspect and cover the owners that actually perform background work in the current checkout: indexing/search, Table/CSV, XML/regex/text processing where asynchronous, file scans/hash/duplicate operations, Rename/Replace, Hot Folder, capture/OCR, macro playback and live pins. Document the precise supported list. A tool with no suitable resource measurement shows unknown or not measured, never a fabricated zero.
3. Use explicit queued, running, cancelling, retired/completed and failed distinctions. Cancel requested is not equivalent to worker exited, temporary files removed, native handles released or input injection stopped.
4. Display process-level memory separately from owner-accounted buffers/entries/handles and clearly label estimates. Do not sum overlapping shared allocations or represent a CPU-side duration counter as a per-feature CPU percentage.
5. Keep observation cheap: immutable/revisioned small snapshots, no file enumeration or synchronous worker request/reply from painting. Refresh visible activity data at most four times per second and stop view-driven refresh when closed; owner completion notifications remain event driven.
6. Activity rows may open the owning tool. Expose Cancel only when the actual owner supports safe cancellation; otherwise link to its existing control. Never force-kill the application, terminate arbitrary threads, or invent a cancellation path around a file transaction.
7. Maintain a bounded session-only history of at most 100 terminal jobs, with Clear and concise copyable diagnostics. Do not persist source content, clipboard values, macro variable values or screenshots as activity history.
8. Measure cancel-to-UI-acknowledgment separately from cancel-to-resource-retirement. For cooperative local CPU/chunked I/O work, target UI acknowledgment within the next usable frame and retirement within one second on the generated acceptance fixture; report unavoidable native I/O boundaries honestly.
9. After cancellation, admit a new operation only when the existing owner can do so safely. Preserve latest-request-wins behavior, immutable generation receipts, transaction atomicity and source identity checks.
10. Exercise repeated open/run/cancel/close cycles and fix confirmed unbounded retention or leaked owned workers/handles within the inspected owners. Stable warm caches are acceptable when their purpose and bound are explicit.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- A global task scheduler or generic job execution framework.
- Whole-application profiler, network telemetry, or exact allocator attribution by feature.
- Forced termination of blocked native threads or unrelated operating-system processes.
- Workspace features, changing automation semantics, and unrelated refactors.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Add a small typed activity snapshot boundary only where existing owners lack a uniform read model. The source owner remains authoritative for operation identity, progress, cancellation and retirement. Any proposed new activity module/dialog must be documented as new; keep the adapter separate from domain execution.
2. Start with an owner/retention map and a few representative measurements. Address known duplicated publication or blocking snapshot work before centralizing anything. Prefer callback/event-based lifecycle receipts and the existing reaper.
3. Use the initial owner inventory to choose a bounded set of concrete cancellation/retention fixes. All inventory owners must have truthful status, but optimization effort follows evidence. Record evaluated paths with no defect rather than manufacturing changes.
4. Later large-file, folder and archive tools can publish the same small activity facts if this goal has been integrated. They must still function correctly without an unmerged activity implementation.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Map activity ownership and establish bounded evidence

**Objective:** Create a precise operation/lifecycle map and reproducible measurements before changing cancellation.

**Code ownership:**

- src/gui/render.rs
- src/performance.rs
- src/thread_reaper.rs
- relevant session/runtime owners

**Implementation tasks:**

1. Enumerate actual background owners and their start, progress, cancel, terminal and retirement boundaries; record which data remain after closing.
2. Reuse current performance/owned-process helpers to record one baseline cancellation and warm-cycle sample for table work, one file scan, one capture/live-pin owner and one native worker boundary.
3. Define the small immutable activity facts and stable operation key; specify owner capacity, history and notification ownership before implementing adapters.

**Behavioral requirements:**

- No execution policy changes yet; source-derived facts and measured facts are kept separate.

**Edge cases:**

- A cancelled operation that is still retiring; owner destroyed before completion; unavailable native session.

**Regression constraints:**

- No raw payload collection or global performance switch enabled solely for the view.

**Verification:**

- Read-only owner audit plus bounded baseline probes after M0-A; inspect receipts for actual native cleanup.

**Acceptance criteria:**

- Owner coverage map and baseline receipts exist; status definitions distinguish cancellation from cleanup; missing native evidence is explicit.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(activity): [M1-A] establish owner lifecycle and resource measurements` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Expose truthful activity snapshots and a lightweight dialog

**Objective:** Connect small owner snapshots to a useful visible activity view.

**Code ownership:**

- src/gui/mod.rs
- src/gui/render.rs
- src/commands/model.rs
- src/commands/host.rs
- src/performance.rs
- proposed activity domain/dialog modules

**Implementation tasks:**

1. Implement adapters at existing publication boundaries with generation-safe terminal updates and bounded session history.
2. Add command/menu entry, search/filter, source-tool navigation, visible-only refresh and clear history controls using existing UI and Jump Mode conventions.
3. Label real buffer accounting, estimates, process totals and unknown data distinctly; attach cancel controls only to supported owner operations.
4. For Hot Folder specifically, publish or reuse a bounded cached snapshot at the authoritative owner before adding the activity adapter. Do not synchronously request-and-wait from the activity view or create a second incompatible cache if a prior change already fixed this boundary.

**Behavioral requirements:**

- Visible jobs explain their current work; closing the dialog stops its refresh demand.

**Edge cases:**

- Immediate completion before first paint; duplicate notifications; owner close/reopen; resource numbers unavailable.

**Regression constraints:**

- The view cannot block on a worker, trigger scans, overwrite payloads, or wake hidden root windows for each captured frame.

**Verification:**

- Adapter tests for event order and history bounds; one GUI open/filter/close check; compare idle counters with view closed before/after.

**Acceptance criteria:**

- Every inventoried supported owner has truthful status; terminal rows are capped; no hidden view polling or raw content appears.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(activity): [M1-B] add bounded owner activity view` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Improve cancellation at the responsible owners

**Objective:** Reduce measured delayed cleanup without weakening job or transaction ownership.

**Code ownership:**

- src/thread_reaper.rs
- src/table_utility_session.rs
- src/duplicate_finder/session.rs
- measured affected session/runtime files

**Implementation tasks:**

1. Fix the measured longest cooperative cancellation boundaries using cancellation checkpoints, bounded chunks and owner notifications.
2. Ensure stale results and exports cannot publish after cancel/close; preserve worker permit and temporary-file ownership until retirement.
3. Remove blocking GUI joins or redundant polling only where the observed owner proves safe event-driven retirement; retain transactional commit boundaries.

**Behavioral requirements:**

- Cancel is acknowledged promptly and the UI clearly reports any remaining retirement before another conflicting operation starts.

**Edge cases:**

- Cancellation during native I/O, export publication, result acceptance, repeated cancel, or application shutdown.

**Regression constraints:**

- No detached unlimited threads, unsafe thread termination, partial successful export, or dropped held-input release.

**Verification:**

- Targeted affected-owner tests with controlled worker barriers; repeat the same baseline cancellation fixture once, allowing two repair iterations.

**Acceptance criteria:**

- Cooperative acceptance fixture meets the recorded acknowledgment/retirement budget or has a precisely identified external blocker; no stale publish or orphaned owned resource remains.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(jobs): [M2-A] release cancelled work through owner retirement` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify long-session bounds and native presentation

**Objective:** Prove the new view and changed lifecycle paths stay bounded across reuse.

**Code ownership:**

- affected resource owners
- existing native smoke/performance harnesses
- activity help/documentation

**Implementation tasks:**

1. Run a deterministic sequence of 30 warm open/run/cancel/close cycles for the changed owners, with disposable data and an upper elapsed deadline.
2. Measure worker/handle/cache/temporary-file counts after retirement; distinguish allocator retention from live object growth.
3. Fix any feature-related growth, capture activity states including cancelling and retired, update concise user help and document real remaining platform limits.

**Behavioral requirements:**

- Repeated operations return to known bounded states and accurate activity rows.

**Edge cases:**

- Shutdown with active jobs; close/reopen while prior generation retires; missing source file mid-cycle.

**Regression constraints:**

- Caches needed for existing fast reopen behavior are bounded rather than indiscriminately cleared.

**Verification:**

- One bounded native smoke and one repeated-cycle run; relevant first-party lint at final review only.

**Acceptance criteria:**

- Owned resources retire, history stays capped, cancellation results are measured and screenshots show real UI states; no unsupported leak or speed claim is made.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(activity): [M2-B] verify cancellation and session resource bounds` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use only selected owner tests and the activity dialog smoke; no full suite or repository-wide memory campaign.
- Cycle validation: 30 cycles, a ten-minute execution ceiling excluding compilation, and at most two focused remediation/recheck attempts.
- Measure private bytes/working set and owned handles/threads separately; compare post-warm-up trends instead of requiring the allocator to return every byte to Windows.
- Record toolchain, dataset, visible/hidden state, operation identities, cancellation request and actual retirement times. Native input/capture evidence requires an interactive Windows session; mark unavailable evidence blocked.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| The activity observer itself adds wakeups or blocking calls. | Read only bounded owner-published snapshots, cap visible refresh, and compare hidden idle work. |
| UI cancellation hides a still-live worker. | Track a retirement receipt and reaper ownership separately from requested cancellation. |
| Shared buffers appear multiple times in memory totals. | Show owner-accounted quantities and process totals separately; label incomplete estimates. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Activity view covers the documented owner list with bounded history and truthful values.
- The changed cancellation paths have measured responsiveness and actual cleanup evidence.
- Repeated changed-owner cycles show bounded live resources; pre-existing unrelated findings remain separate.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Owner coverage table and unsupported measurements.
- Before/after cancellation timings and repeated-cycle resource counts.
- Screenshots for running/cancelling/retired activity and any blocked native evidence.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
