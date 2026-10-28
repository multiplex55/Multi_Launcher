# Codex Goal 08 — Lower idle work and configurable live-pin performance

**Companion execution prompt:** [CODEX_KICKSTART_08_IDLE_LIVE_PINS.md](CODEX_KICKSTART_08_IDLE_LIVE_PINS.md)  
**Requested coverage:** #113, #116  
**Program position:** 08 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `perf/idle-live-pins` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish idle-live-pins branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Reduce unnecessary hidden/idle background activity without delaying enabled work, and let the user select a sensible live-pin refresh rate. Keep current behavior as the default, preserve independent pin windows and their resource bounds, and show measured power/CPU tradeoffs honestly.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md), [07 — Launcher readiness, large-result display, and startup responsiveness](CODEX_GOAL_07_LAUNCHER_RESPONSIVENESS.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/pinboard/live.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/pinboard/live.rs)** — `FRAME_INTERVAL; PinLiveFrameSlot; cadence wait/record; MAX_GPU_BYTES`. Live capture has a fixed 34 ms cadence, a bounded latest-frame slot, two frame-pool buffers and a 256 MiB GPU budget. Child publication explicitly avoids waking the launcher root.
- **[src/gui/render.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/render.rs)** — `run_runtime_tick; conditional request_repaint_after calls`. Polling is already conditional in several paths; each remaining wakeup needs an owner/necessity assessment rather than blanket timer removal.
- **[src/hotfolder/runtime.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/runtime.rs)** — `worker loop; next_deadline`. The watcher already blocks on events or the next settle/lease deadline. There is no basis to claim its old fixed-poll cost still applies.
- **[src/thread_reaper.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/thread_reaper.rs)** — `completion-driven supervision`. Worker retirement is already event driven; new idle tuning must not add periodic reaper polling.
- **[src/gui/event_channel.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/event_channel.rs)** — `event delivery and wake ownership`. The channel queues/observes messages but a raw successful send does not wake a viewport. Inspect its use together with the GUI event sink before removing polling.
- **[src/gui/mod.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/mod.rs)** — `EventSinkRegistration::attach_wake; register_event_sink; send_event; send_physical_paste_event`. This is the actual enqueue-then-wake and already-queued-before-attachment owner. Existing tests cover attachment, bursts and disposed sinks; idle fixes must preserve this entire publication-to-wake contract.
- **[src/pinboard/live.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/pinboard/live.rs)** — `WorkerSignal::wait_for_frame_or_control`. The wait_timeout_while predicate recognizes pause/cancel/frame/source state. A rate change needs a predicate-visible configuration revision or equivalent control state; notification alone can resume the old timeout.

## C. Approved functional requirements

1. Measure hidden idle, visible idle, idle with Hot Folder enabled, live pin paused, and live pin actively refreshing. Record process CPU time, wake/repaint counts and relevant owner events; do not infer power savings directly from one wall-time sample.
2. Remove or slow only activity proven unnecessary when no work is due. Preserve timer completion, hotkeys, watcher delivery, clipboard/paste playback, source recovery and real job-completion notifications.
3. Add a persisted global Live Pin refresh preference with choices 5, 10, 15 and 30 updates per second. Default to the current approximately-30 rate (34 ms cadence) when missing or invalid; do not force a higher capture rate.
4. Provide a per-live-pin override using Global or the same rate choices, effective for the pin lifetime. Do not add cross-session pin persistence as part of this goal.
5. Rate changes affect active live sessions promptly through existing control ownership, without creating a new capture worker or resetting source/crop identity. Pause/resume remains authoritative over rate.
6. Retain the bounded latest-frame handoff and CPU/GPU image budgets. Slower cadence must not accumulate stale frames or duplicate retained textures.
7. Keep the launcher root asleep when hidden and only child pins need repaint. A hidden launcher does not imply that an independently visible live pin should stop refreshing.
8. Keep the first useful frame promptly available; apply configured pacing between completed capture cycles, with no catch-up burst after a slow frame, pause or temporary source loss.
9. Expose rate settings through existing settings persistence/search and a pin control/menu that follows the current pin UI. Label the choice as a refresh limit rather than guaranteed delivered FPS.
10. Use notifications/deadlines for new idle logic. Any adaptive policy beyond these explicit rates is deferred; do not silently reduce quality based on uncertain activity heuristics.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Automatic battery/power plans, GPU backend replacement or unlimited frame rates.
- Persisting or restoring pins across sessions.
- Disabling enabled functionality to make idle measurements appear better.
- Global polling/timer rewrites unrelated to a measured wakeup.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Use the existing cadence owner and live session control channel. Propagate a validated interval or rate enum, with missing-setting fallback preserving 34 ms behavior.
2. Audit wake sources with bounded receipts, then change only the owners that wake without useful work. Prefer an event/deadline proof over a wider interval guess.
3. Measure capture versus presentation separately; a frame-rate limiter does not guarantee the source can supply the requested rate.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Identify unnecessary idle wakeups

**Objective:** Create an owner-specific baseline for the selected idle modes.

**Code ownership:**

- src/gui/render.rs
- src/gui/event_channel.rs
- src/performance.rs
- src/hotfolder/runtime.rs
- src/pinboard/live.rs
- src/gui/mod.rs event sink/wake publication

**Implementation tasks:**

1. Record 60-second samples in the five named modes, with identical enabled features and synthetic live source.
2. Correlate unnecessary root repaints/polls to the responsible owner and check its missed-wake prevention mechanism.
3. Choose the bounded fixes and retain a negative-control sample proving enabled work still arrives.

**Behavioral requirements:**

- Only measurement/provenance changes before owner optimization.

**Edge cases:**

- Hidden root with visible child, no active job, deadline due during idle sample.

**Regression constraints:**

- Do not disable watchers, hotkeys or notifications to reduce the sample.

**Verification:**

- One baseline sample per mode; repeat only noisy/inconclusive modes up to three times.

**Acceptance criteria:**

- Each candidate wakeup has a concrete cause and preserved wake-on-work contract.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(idle): [M1-A] attribute background wakeups` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Make unnecessary background work conditional

**Objective:** Remove measured idle work at its owner without delaying real work.

**Code ownership:**

- only owners identified by M1-A
- src/gui/render.rs
- src/gui/event_channel.rs
- src/gui/mod.rs event sink/wake publication

**Implementation tasks:**

1. Replace redundant polling with existing publication/deadline wakeups where safe; cache unchanged read models by revision.
2. Preserve attach-before-notify and lost-wakeup protection, including already-queued work at observer registration.
3. Keep visible-only UI refresh out of closed views and avoid expanding this into a general scheduler migration.
4. Verify the complete publish -> enqueue -> viewport wake path, including attach-after-queue delivery. A channel send by itself is not enough. If measurements implicate synchronous Hot Folder snapshot requests, inspect/reuse the owner-level cached read model rather than duplicating Goal06 work.

**Behavioral requirements:**

- Idle work drops while enabled events retain their delivery semantics.

**Edge cases:**

- Event before subscriber attachment, rapid open/close, worker completion while hidden, app shutdown.

**Regression constraints:**

- No missing timer/Hot Folder/clipboard/capture completion and no unintended root visibility changes.

**Verification:**

- Direct owner delivery tests and a matched idle sample; one enabled-work negative control.

**Acceptance criteria:**

- Measured unnecessary work decreases beyond noise or a no-change finding is justified; functional event delivery passes.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `perf(idle): [M1-B] eliminate redundant owner wakeups` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Add live-pin refresh preferences

**Objective:** Expose bounded rates through the existing capture cadence and settings.

**Code ownership:**

- src/pinboard/live.rs
- src/pinboard.rs
- src/gui/pinboard_windows.rs
- src/settings/model.rs
- settings mapping/save/catalog owners

**Implementation tasks:**

1. Add validated global preference and session per-pin override with current-rate fallback for legacy settings.
2. Update the cadence wait/control path so changes wake a sleeping worker without allocating a second worker or buffering extra frames.
3. Add global settings/search controls and per-pin control labels; preserve pause/resume and source-loss statuses.
4. Make a rate/configuration revision visible in the wait predicate so a setting change recomputes the deadline while waiting even when no new frame arrives; a bare Condvar notify is insufficient.

**Behavioral requirements:**

- Each live pin follows its current chosen limit while independent pin presentation stays responsive.

**Edge cases:**

- Invalid/missing setting, rate change while paused, slow capture, source resize/close, global change under an override.

**Regression constraints:**

- Existing CPU/GPU budgets, latest frame ownership and first-frame behavior remain bounded and correct.

**Verification:**

- Pure cadence tests with injected elapsed time plus settings round trips; inspect 5/15/current-rate modes with a synthetic native source.
- Controlled no-new-frame test: change from slow to fast while the worker is waiting and prove the new deadline takes effect, without treating a control update as a captured frame.

**Acceptance criteria:**

- Rates apply without catch-up bursts or root wake storms; legacy settings preserve current behavior.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(pinboard): [M2-A] configure live-pin refresh limits` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify delivery, resource bounds and user controls

**Objective:** Prove idle gains and pin choices under repeated lifecycle transitions.

**Code ownership:**

- changed owner tests
- existing live-pin/native smoke harness
- related help

**Implementation tasks:**

1. Run a bounded pause/resume/rate-change/source-loss/close sequence and confirm owned frame/worker resources retire.
2. Capture settings and per-pin controls plus one live update and paused state.
3. Compare final idle and active-pin counters against M1-A, report requested versus observed cadence, and update concise help.

**Behavioral requirements:**

- The chosen tradeoff is visible and repeatable without reduced functional reliability.

**Edge cases:**

- Last pin closed while root hidden; two pins with different overrides; parent app exits during capture.

**Regression constraints:**

- No residual capture threads or frame queues, no unrelated settings changes.

**Verification:**

- One final matched mode batch and targeted native smoke; final affected first-party lint.

**Acceptance criteria:**

- User controls, cadence and delivery invariants pass; actual CPU/wakeup outcomes are reported without invented power claims.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(pinboard): [M2-B] verify idle delivery and cadence bounds` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use 60-second idle samples with at most three repeats only for ambiguous modes; separate compilation from execution deadlines.
- Cadence correctness uses an injected clock/controlled worker rather than arbitrary sleeps in unit tests.
- Native validation uses a generated test window with known frame changes, two live pins and owned process cleanup; total smoke execution ceiling ten minutes.
- Retain source-loss, pause, frame-leasing and child-only repaint tests; do not add an all-feature background stress campaign.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| Removing polling loses a completion wakeup. | Verify registration and queued-work ordering with direct owner tests. |
| Rate limiting accumulates stale frames or wakes ROOT. | Retain one latest-frame slot and child-scoped wakes; measure both. |
| A new global preference changes legacy behavior. | Default exactly to existing 34 ms pacing and preserve per-pin pause controls. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- All rate choices and legacy fallback behave as specified.
- Measured unnecessary idle work is addressed without lost enabled events.
- Pin buffers/threads remain bounded and requested/observed refresh behavior is accurately documented.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Before/after idle CPU time and wake/repaint counts by mode.
- Global/per-pin setting defaults and persistence behavior.
- Observed cadence, native screenshots and lifecycle cleanup evidence.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
