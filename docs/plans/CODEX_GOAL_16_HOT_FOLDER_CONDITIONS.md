# Codex Goal 16 — Hot Folder size, age, and text-content conditions

**Companion execution prompt:** [CODEX_KICKSTART_16_HOT_FOLDER_CONDITIONS.md](CODEX_KICKSTART_16_HOT_FOLDER_CONDITIONS.md)  
**Requested coverage:** #63  
**Program position:** 16 of 17; execute this Goal only.  
**Reviewed source:** [multiplex55/Multi_Launcher, master at c5e10bd](https://github.com/multiplex55/Multi_Launcher/commit/c5e10bdc2a86e54363950a441dc8d105bc869267), October 25, 2026.  
**Status:** Approved implementation contract; no implementation or verification result is implied by this document.

## A0. Mandatory Git branch bootstrap — before all application work

**M0-A is the first execution checkpoint.** Read this entire Goal, its companion Kickstart, the repository `AGENTS.md`, and applicable local instructions. The current checked-out repository is the implementation source of truth. The reviewed commit is provenance; it is not an instruction to reset to that revision.

1. **Read-only safety checks.** Inspect `git status -sb`, current branch, HEAD, actual default integration branch, remote/upstream and pre-existing changes. Read-only source/dependency checks are allowed. Do not edit feature code, run builds/tests/benchmarks, start later checkpoints, or make source modifications before this bootstrap is complete.
2. **Select the base safely.** Normally use the up-to-date tip of the repository's actual default integration branch (`master` in the reviewed repository; rediscover it). Use a normal safe fetch of the intended remote when available. Record local/remote state if fetching is unavailable. Never reset, hard-pull, stash, rebase, merge, or force a checkout to make the transition work.
3. **Preserve dirty user work.** Do not discard, stage or commit unrelated pre-existing changes. If dirty work prevents safely switching to the integration tip, create the fresh branch from current HEAD when that preserves the work safely; record this exception and the original changes. An isolated checkout may be used when consistent with repository instructions. Report a genuinely unsafe transition as blocked rather than solving it destructively.
4. **Create and switch to a unique task branch.** Choose lowercase kebab-case with an appropriate prefix; `feat/hot-folder-conditions` is a suggested stem, not an existing branch to reuse. Check local and known remote names. Add a meaningful unique suffix if needed. Never silently reuse a previous task branch. The new branch must be active for all subsequent implementation, tests, commits and push attempts.
5. **Create that branch's initial baseline commit before implementation.** Prefer a small legitimate execution-ledger/branch-baseline record following project conventions. If none is needed, use an intentionally empty commit, for example `git commit --allow-empty -m "chore: [M0-A] establish hot-folder-conditions branch baseline"`. Do not manufacture a commit from user changes. Record original base SHA and baseline commit SHA. The latter is the comparison baseline for every later task diff; never rewrite it.
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

Allow an existing Hot Folder rule to require a file size range, last-modified age range and/or a literal text-content match before its existing action runs. Keep the current event-triggered workflow and safeguards; make Test Rule explain which conditions passed, failed or could not be evaluated.

### Dependency and branch-base contract

**Required predecessor capabilities:** None.

**Recommended earlier work:** [01 — Bounded developer workflows, build evidence, and focused test feedback](CODEX_GOAL_01_DEV_WORKFLOWS.md), [02 — Faster isolated tests and efficient GUI reference coverage](CODEX_GOAL_02_TEST_EFFICIENCY.md), [06 — Application activity, cancellation recovery, and resource stability](CODEX_GOAL_06_ACTIVITY_CANCELLATION.md), [08 — Lower idle work and configurable live-pin performance](CODEX_GOAL_08_IDLE_LIVE_PINS.md), [15 — Completed macro run summaries and practical starter recipes](CODEX_GOAL_15_MACRO_SUMMARIES_RECIPES.md). Recommended order is not a hard dependency and does not authorize taking files from another branch.

A required predecessor must already be present in the safely selected base, or an equivalent implementation must be verified with concrete source/commit evidence. Running goals consecutively does not make unmerged changes appear in `master`. Do not automatically merge, cherry-pick, rebase, or silently branch from a preceding feature branch. An explicitly authorized future combined execution contract may specify a different base; absent that instruction, retain the default integration-branch rule. If a required capability is missing, report that precise prerequisite as blocked; do not implement a partial substitute or duplicate the predecessor.

After M0-A, revalidate source paths, symbols, tests and current capabilities. Earlier compilation work may move a pure engine into an internal package while preserving its public module API. Follow the real implementation owner/re-export and its current test target; do not put a duplicate engine back into a facade merely to match an older path.


## B. Current codebase findings

These are findings from source inspection at the cited revision, not measured Windows performance results. Confirm the actual execution checkout after the Git bootstrap and adapt locations without changing approved behavior.

- **[src/hotfolder/model.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/model.rs)** — `FilenameFilter; HotFolderRule; evaluate_rule_conditions; SettlePolicy`. Current conditions cover event kind, subfolder/temporary-file policy, filename glob and extensions. Rules are strictly deserialized and have a versioned persisted document.
- **[src/hotfolder/runtime.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/runtime.rs)** — `settling candidates; ready leases; next_deadline`. Watch events become stable identity-bound ready candidates under bounded queues and deadlines. Add content work after cheap admission/settling, not in the watcher ingress callback.
- **[src/hotfolder/execution.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/execution.rs)** — `HotFolderExecutionReceipt; HotFolderExternalFileEvidence; source fingerprinting`. Execution carries native root/parent/file identity, length, modification time, authorization and optional hash evidence through its single-action owner.
- **[src/gui/hotfolder_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/hotfolder_dialog.rs)** — `evaluate_sample_test; start_sample_test; rule editor`. Test Rule currently performs a read-only metadata sample and reports size/modified values; it does not evaluate content conditions.
- **[src/gui/hotfolder_runtime.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/gui/hotfolder_runtime.rs)** — `ConfigurationFingerprint::of_serializable; enable preview/authorization`. Rule configuration fingerprints and revisions gate execution; new conditions must participate in those checks.
- **[src/hotfolder/store.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/store.rs)** — `save_rules; replace_rule; decode document; quarantined rows`. The store uses atomic persistence, revision guards, format versions and quarantines malformed rows without silently replacing the file.
- **[src/hotfolder/execution.rs](https://github.com/multiplex55/Multi_Launcher/blob/c5e10bdc2a86e54363950a441dc8d105bc869267/src/hotfolder/execution.rs)** — `HotFolderExecutionEngine::first_matching_rule; run_next_ready first-rule suppression`. Current arbitration picks the first enabled name/path matching rule and suppresses other candidates before fingerprinting. Checking new predicates only after that choice would let a failing earlier rule incorrectly suppress a later fully matching rule.

## C. Approved functional requirements

1. Add optional inclusive minimum/maximum file size, optional inclusive minimum/maximum last-modified age, and one optional literal UTF-8 contains or does-not-contain condition. Existing filename/extension conditions and all configured new conditions combine with AND.
2. Defaults leave every new condition disabled, preserving existing rules. Display size units and time units explicitly; reject negative values, inverted ranges, overflow and empty content patterns.
3. Age means elapsed time since the file last-modified timestamp, evaluated against one captured wall-clock instant after the file has settled. Unknown or future timestamps cannot satisfy an age condition and produce a clear non-match/not-evaluated reason.
4. Age is a condition on existing create/rename-in/modified events. This goal does not schedule a future run merely because a file gets older, rescan existing folder contents, or change the selected trigger. Explain that behavior beside the age control.
5. Text matching is case-sensitive literal matching, with UTF-8 and optional UTF-8 BOM support. No regex language or archive/binary decoding is required. Handle matches spanning read buffers and validate decoding; unsupported/binary input must never satisfy does-not-contain by default.
6. Use bounded background reads only after metadata/name/age conditions pass. Default content inspection budget: 64 MiB total file bytes, 64 KiB read buffers plus bounded pattern/decoder state, pattern length at most 4096 UTF-8 bytes, and a two-second cooperative processing deadline excluding uncancellable time inside a native read. Report the exceeded limit; do not silently treat a prefix scan as a full negative result.
7. For literal positive/negative conditions, require a complete valid scan under the limit before accepting the condition so a malformed or changing tail cannot turn into a successful text-file match. Check cancellation between reads; never do content work on the GUI or watcher callback.
8. Use the existing rooted native file admission and identity/fingerprint policy. Detect replacement or modification during evaluation, invalidate the candidate and revalidate evidence at the existing dispatch boundary. Do not follow a new reparse target or execute against a stale condition receipt.
9. Extend Test Rule with the same predicate implementation and result categories used by live evaluation. Show observed size/age and per-condition reason, but do not echo matching file contents or execute the action.
10. New conditions affect configuration fingerprints and enable authorization. Editing them invalidates an old preview/authorization exactly as other material rule changes do; no silent auto-enable.
11. Use an explicit compatible document migration: read legacy version 1 without rewriting it and supply disabled new-condition defaults; write a new version only through an intentional successful settings save. Old versions must not silently execute a condition-bearing rule after dropping its conditions. Preserve quarantined data and future-version read-only protection.
12. Do not alter move/copy/rename/external action behavior, loop suppression, throttle/circuit-breaker behavior, journal recovery or existing settle policy. Content inability is a diagnostic skip/unevaluated outcome, not successful execution.
13. Preserve rule order but define the winner as the first authorized, enabled rule whose complete predicates match. An earlier name match that fails size/age/content or is unevaluated must not suppress a later fully matching rule. Rework first_matching_rule/candidate suppression together, and retain exactly-once execution when multiple complete matches exist.
14. Bound condition arbitration across all candidate rules as one job: the 64 MiB read cap and two-second cooperative content deadline are aggregate, not multiplied by up to 256 rules. Share bounded scan/evidence where appropriate, check cheap metadata and content-size admission before expensive full-file fingerprinting, and fingerprint only at the required existing action boundary. Predicate non-match/unevaluated is not an action failure and must not spuriously trip execution circuit breakers.
15. Define unsupported binary input deterministically for this feature: invalid UTF-8 or any NUL byte after the optional BOM is not supported text. Reject NUL in the literal pattern. Test Rule must use the same admitted rooted file-handle/identity reader as live conditions, not an ordinary path read that can follow a different target.

## D. Scope, non-goals and preserved behavior

The requirements above define the complete scope. Preserve unrelated command routing, keyboard shortcuts, focus/visibility ownership, persistence and platform behavior. Extend current architectural owners and prefer existing dependencies. A necessary adjacent change must have a direct correctness or integration reason; new filenames described below are proposals, not claims that they already exist.

- Scheduled aging, processing existing folder contents, multiple ordered actions or batch rule testing.
- Regex text matching, arbitrary encodings, OCR, archive-content rules or parsing structured documents.
- Reading every watched file continuously or logging file contents.
- Relaxing destination, authorization, reparse, collision or recovery safeguards.

No application workspace features, unrelated UI redesign, speculative future capability or general cleanup is authorized. Relevant documentation/help updates belong with the changed behavior.

## E. Technical implementation strategy

1. Separate pure condition configuration/metadata comparison from bounded file-content I/O. Use a typed match/non-match/not-evaluated result with safe reasons so preview and live processing cannot diverge.
2. Keep the watcher responsible for event admission and settling. Evaluate content in the existing execution preparation owner or a narrowly scoped cancellable preparation task under its lease; do not add an independent watcher/polling scheduler.
3. Version the schema conservatively because strict older readers must not ignore new execution constraints. Preserve legacy load behavior and current revision/fingerprint ownership.
4. Extend the current Test Rule worker to use the same immutable rule draft/evidence while preserving editor ID and draft revision stale-result checks.
5. Move full-predicate winner selection before existing duplicate suppression. Share the evaluator with Test Rule and preserve one action for the first complete matching rule; avoid reading/hashing the same file once per eligible rule.

## F. Detailed milestones and tasks

M0-A above is a hard prerequisite. Complete and verify each following checkpoint before beginning a materially different implementation stage. Where a checkpoint records a measured no-change disposition, report that accurately; it is not a speedup or a completed implementation claim.

### M1-A — Define conditions and compatible persisted rules

**Objective:** Add validated typed condition settings with safe legacy defaults.

**Code ownership:**

- src/hotfolder/model.rs
- src/hotfolder/store.rs
- src/hotfolder/capability.rs
- src/gui/hotfolder_runtime.rs

**Implementation tasks:**

1. Introduce size/age/content configuration and pure metadata predicates with AND semantics and explicit units.
2. Implement version-aware legacy decoding and intentional-save upgrade, preserving malformed/future documents and quarantined rows.
3. Include condition settings in rule fingerprints, validation and authorization invalidation.

**Behavioral requirements:**

- Legacy rules behave identically; new invalid configurations cannot become enabled rules.

**Edge cases:**

- Missing fields, old/future version, inverted/overflow ranges, unknown/future timestamps, stale revision, quarantined neighbor.

**Regression constraints:**

- Loading does not rewrite user data or auto-enable rules.

**Verification:**

- Focused model/store round trips and fingerprint/authorization tests using disposable files.

**Acceptance criteria:**

- Legacy behavior, new validation and older-reader fail-closed format behavior are explicit and verified.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(hotfolder): [M1-A] model versioned file conditions` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M1-B — Evaluate stable file contents through the runtime owner

**Objective:** Make content and metadata decisions safely before the existing action boundary.

**Code ownership:**

- src/hotfolder/runtime.rs
- src/hotfolder/execution.rs
- src/file_operations.rs native admission helpers
- proposed shared condition evaluator

**Implementation tasks:**

1. Run cheap metadata predicates after settling and before content inspection; retain one evaluation time/evidence identity.
2. Implement bounded UTF-8 streaming literal matching, cross-buffer patterns, cancellation/deadline checks and explicit limit/encoding outcomes.
3. Tie accepted predicate evidence to rule revision and stable file identity; reject changing files and revalidate at dispatch without bypassing current native safeguards.
4. Update first_matching_rule and run_next_ready suppression to use full predicates under one aggregate arbitration budget. Cheap failures skip to later rules; complete matches preserve current first-winner order. Revalidate authorization and identity before committing that winner.

**Behavioral requirements:**

- Only complete matching candidates reach the existing action owner; unevaluated conditions skip safely with a reason.

**Edge cases:**

- Pattern across buffer/UTF-8 boundary, invalid tail, empty file, permission/sharing failure, cap/deadline hit, source replacement, cancellation before dispatch.

**Regression constraints:**

- Watcher ingress remains cheap; no false negative from a truncated scan and no new uncontrolled thread population.

**Verification:**

- Small deterministic byte-stream/metadata tests, controlled source-change race, and one native rooted-file boundary test.
- Two-rule cases: A name-matches but content/size/age fails and B matches -> B once; both fully match -> A once; A unevaluated -> no false win; many rules share one aggregate read/deadline budget.

**Acceptance criteria:**

- Positive and negative conditions are correct under all bounds and stale/unauthorized candidates cannot execute.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(hotfolder): [M1-B] evaluate bounded stable-file predicates` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-A — Expose rule conditions and explain Test Rule outcomes

**Objective:** Make conditions editable and previewable through existing UI ownership.

**Code ownership:**

- src/gui/hotfolder_dialog.rs
- src/gui/hotfolder_runtime.rs
- existing settings/Jump control registration

**Implementation tasks:**

1. Add grouped optional size, modified-age and literal text controls with validation and clear trigger/limit help.
2. Extend Test Rule to use the shared evaluator, preserving editor draft revision and cancellation ownership.
3. Show per-condition match/non-match/not-evaluated reasons and planned action only when all conditions pass; update live diagnostics using the same categories.

**Behavioral requirements:**

- A sample can be tested safely without enabling a rule or touching its source content.

**Edge cases:**

- Draft edited while scan runs, dialog closed/reopened, content predicate removed, unreadable sample, sample outside root.

**Regression constraints:**

- Existing action selection, enable confirmation, hotkeys and input ownership stay intact; sample file/store remain unchanged.

**Verification:**

- Focused dialog stale-result/read-only tests and a native example with size/age/content pass and non-match cases.

**Acceptance criteria:**

- Test Rule and live evaluation agree and user-visible help prevents interpreting age as a scheduled trigger.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `feat(hotfolder): [M2-A] edit and preview file conditions` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

### M2-B — Verify authorized execution and migration boundaries

**Objective:** Prove integration without expanding the automation surface.

**Code ownership:**

- affected Hot Folder tests
- existing Hot Folder native/perf probe
- Hot Folder help

**Implementation tasks:**

1. Exercise one disposable safe action through create/rename-in and modified events, changing one new condition between runs.
2. Verify prior enable authorization becomes stale after condition edits and duplicate/own-output suppression is retained.
3. Check disabled idle behavior and bounded content inspection, update documentation and capture the condition/test UI.

**Behavioral requirements:**

- New conditions filter existing workflows while unchanged legacy rules retain their semantics.

**Edge cases:**

- Rollback to older binary sees unsupported format, journal recovery present, disabled rule, cancellation near action handoff.

**Regression constraints:**

- No implicit scheduler, existing-file scan or newly authorized external action.

**Verification:**

- Selected model/store/runtime/dialog checks plus one ten-minute-bounded native fixture; final affected first-party lint.

**Acceptance criteria:**

- All three condition types pass shared preview/live tests with correct persistence and native execution guards.

**Commit boundary:** After the above scoped verification succeeds and the diff is coherent, commit as `test(hotfolder): [M2-B] verify conditional rule lifecycle` (adapt wording only if needed to reflect actual work), then attempt one normal push of the task branch. Retain a failed push locally and continue. Do not create an empty implementation checkpoint for an unachieved acceptance criterion.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The boundaries are M0-A and the labeled checkpoints in Section F.

- Commit coherent, scoped and verified increments on the new branch. Stage explicit task-owned paths/hunks; do not sweep unrelated dirty files into a commit.
- The first commit is this branch's M0-A baseline, with original base and baseline SHA retained for provenance. Never substitute an old-branch commit for the new baseline.
- Before each commit inspect status, staged/cumulative diff and relevant evidence. Use each checkpoint's focused verification; do not require an expensive full suite before every commit.
- Attempt one normal, non-force push after every checkpoint when supported. Record successful pushes and locally retained commits distinctly. A push denial never justifies bypassing policy or stopping otherwise available local work.
- Preserve existing commits. Add focused remediation commits if needed; do not squash, amend away the recorded baseline, rewrite history or silently change branches.

## H. Verification and testing strategy

- Use injected time for age tests and controlled streams for read-bound/cancellation tests; no waiting hours or arbitrary sleeps.
- Use one generated UTF-8 sample above the read cap and small boundary fixtures; do not read the user’s private watched data.
- Run targeted model/store/evaluator/runtime/UI tests only. Native fixture uses safe disposable directories/actions and an external execution deadline separate from compilation.
- If a native read cannot be interrupted, keep ownership and a cancelling state until real retirement; report that boundary rather than claiming a two-second hard OS deadline.

Before invoking tests, inspect actual Cargo package/target/features and installed Nextest behavior. Select the narrow target as well as cases, confirm nonzero discovery and preserve command arguments/profiles in evidence. A syntactically valid filter selecting zero tests is not a pass. Reuse integrated developer tooling when present; otherwise use the existing bounded process/harness owners without pretending future helpers exist.

Use an owned, disposable Windows application profile for UI/native checks, preserve user data, open the relevant surface, exercise its controls, capture screenshots/receipts, and close/clean up owned processes. Inspect captured evidence. Do not ask the user to run intermediate commands, take screenshots or perform acceptance checks. If native capability is unavailable, finish unaffected work and report the exact unverified/blocking criterion; do not claim that a headless/unit substitute proved native focus, input or rendering.

Avoid repeated release builds and full-suite runs. Broaden verification only for a concrete failure or shared-boundary risk that the targeted selection cannot cover, and record that reason. Run appropriate affected first-party Clippy once near final review; fix introduced/relevant lints, keep unrelated pre-existing findings separate, and exclude vendored code from cleanup. Re-run only affected checks after a correction.

## I. Risk management and bounded validation

| Risk | Mitigation |
|---|---|
| An older program ignores new constraints and still executes the rule. | Versioned intentional-save migration with fail-closed unsupported-version behavior. |
| Content evaluation is applied to a different file than execution. | Carry root/file identity and revision evidence; reject changes and revalidate before dispatch. |
| Truncation or decoding error makes does-not-contain match. | Only a complete valid bounded scan may accept either content predicate; other outcomes are unevaluated. |

Set separate finite deadlines for build/compilation, test execution and native probes before launching them. Reuse documented project budgets; when no relevant budget exists, a starting ceiling of 45 minutes for one Windows compile and 10 minutes for a selected test process is reasonable, with explicit overrides for a known fixture. These are ceilings, not reasons to keep repeating work until time expires. A timeout is a recorded failure/blocker with owned-process cleanup, not a passing result.

Allow at most two focused remediation attempts for one failing acceptance boundary before recording a precise blocker or one evidence-backed scope adjustment. Continue independent work. Never weaken an assertion, ignore a meaningful failure, reduce required dataset coverage, hide stderr, or report incomplete cleanup as success to fit the budget. Optional benchmarking, unrelated warnings and historical failures cannot expand this Goal indefinitely.

Classify findings as: **feature blocker**, **unrelated pre-existing issue**, or **optional follow-up**. For a blocker report the command/scenario, evidence, attempted remedies, current safe state and smallest necessary external action. Do not ask for routine engineering decisions or ordinary push failure.

## J. Definition of done

- Size, modified-age and literal text conditions work with clear AND semantics and disabled defaults.
- Test Rule uses the same evaluator without side effects.
- Persistence, authorization, file identity, cancellation and existing action guards remain effective.

In addition, all approved functional requirements and checkpoint acceptance criteria must have meaningful evidence; existing affected behavior is preserved; no known feature-related blocker remains; relevant review/lint findings are resolved or accurately classified; and coherent changes are committed with the specified cadence. Report incomplete/blocked requirements explicitly. An investigated performance opportunity with no demonstrated gain must never be labeled a speed improvement.

The final branch is still the new task branch. Original base SHA, M0-A baseline SHA, later commit hashes and actual push states are recorded. Required native/UI evidence is present or its absence is reported as a real limitation; it is never silently replaced by an unsupported pass claim.

## K. Final delivery report

Report completed/blocked checkpoints, meaningful implementation decisions, actual files/owners changed, tests and commands with results, native evidence paths, resource/timing provenance where relevant, and remaining limitations. Include branch name, original base SHA, baseline commit SHA, checkpoint subjects/hashes and actual push status for each. Separate completed code from verified behavior and unavailable validation.

Feature-specific handoff evidence:

- Condition semantics, content limits and age-trigger explanation.
- Migration/authorization compatibility evidence and native fixture receipts.
- Screenshots of rule editing and per-condition Test Rule results.

Stop after this Goal and required verification are complete. Do not merge, publish a release, start another Goal, or perform unrelated cleanup. The user performs their own final manual review after the autonomous work.
