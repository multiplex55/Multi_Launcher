# Codex Kickstart — Productivity Tools

You are the orchestrator for the implementation defined in **CODEX_GOAL_PRODUCTIVITY_TOOLS.md**.

Read the applicable repository instructions and the entire Goal, then execute the authorized work through its checkpoints. Continue autonomously through all eleven features. Do not stop after planning, after scaffolding, or after the first useful subset. The Goal defines the functionality and acceptance criteria; this Kickstart defines execution discipline.

## 1. Mandatory First Execution — Fresh Branch, Baseline Commit, Initial Push

**Before any application edits, builds, test runs, source modifications or implementation checkpoints, complete M0-A below.** Read-only inspection is allowed first. Never assume the user has already created the implementation branch.

### 1.1 Read and inspect without modifying application code

Read:

- The repository's AGENTS.md and applicable instructions in affected directories.
- Any configured planner, implementer and reviewer instructions that apply.
- The entire CODEX_GOAL_PRODUCTIVITY_TOOLS.md.
- This entire Kickstart.

Inspect git status -sb, current branch, HEAD, staged and unstaged changes, untracked files, the actual default integration branch, existing remotes/upstream and relevant branch names. Record pre-existing work so it can be preserved and excluded from task commits. Do not expose credentials embedded in remote URLs or logs.

No implementation agent may start writing source while these checks are in progress.

### 1.2 Select a new unique branch and safe base

Use an idiomatic lowercase kebab-case task name such as **feat/productivity-tools**. Check both local and remote branch names. If occupied, add a meaningful unique suffix; never silently reuse a previously existing task branch.

Unless the user explicitly specifies another base, fetch the verified existing remote normally when permitted, recheck the chosen branch name against refreshed refs before creating it, and prefer the up-to-date default integration branch. If remote uniqueness cannot be checked, record that limit, choose a locally unique suffix, and never force an initial push over an existing remote branch. In the planning inspection that branch was master, but verify the actual current repository.

With a clean checkout, create the new task branch directly from the verified integration tip if convenient. You do not need to reset or advance the user's existing local master/main.

If dirty work prevents safely switching bases, preserve the current working tree and index and create the new branch from current HEAD when safe. Record the actual base SHA and why it was used. Do not automatically stash, reset, hard-pull, rebase, merge, clean, force checkout, discard files or stage pre-existing user work. An unresolved merge or genuinely unsafe branch transition is a bootstrap blocker, not permission to use destructive commands.

Create and switch to the fresh branch before the first task-owned write. Confirm it is active.

### 1.3 Create the branch's own initial baseline commit

The first commit on the new branch must be **M0-A**. Prefer a small task-specific execution record consistent with project conventions. If there is no legitimate tracked change, create an intentionally empty baseline.

Protect an existing staged index: a plain allow-empty commit can still include staged user changes. The documented empty-only form is:

~~~text
git commit --only --allow-empty -m "chore: [M0-A] establish productivity tools branch baseline"
~~~

For a legitimate task-owned record commit, include only that change; inspect the exact commit content. Do not use unrelated edits to manufacture a baseline.

Record both:

- **Original base SHA:** the parent/base from which this new task started.
- **New M0-A baseline commit SHA:** the new task's comparison baseline.

Record the baseline SHA immediately after the commit exists; include it in the next task-owned execution-record update rather than trying to embed a commit's hash into itself. Never amend or rewrite the baseline.

### 1.4 Attempt initial upstream push, then continue locally if necessary

Make one normal non-force upstream push to the intended verified remote, such as git push -u origin HEAD if origin is correct.

If push access is absent, denied by approval policy, or fails, preserve the branch and local baseline commit, record the exact result, and continue implementing locally. Do not enter retry loops, bypass security, replace remotes, force-push or require the user to intervene just to continue local work.

A failed push is not an implementation blocker. Failure to create the required safe branch or local baseline is a real M0-A blocker.

### 1.5 Keep this branch for the entire execution

All later feature implementation, tests, source changes, checkpoints and pushes belong to the new task branch. Do not implement on master/main or silently return to the prior feature branch.

Before releasing the first implementation packet, verify:

- Unique new task branch is active.
- New baseline commit exists.
- Original base and new baseline SHAs are recorded.
- Initial upstream push was attempted when permitted, or its actual unavailability recorded.
- Pre-existing work and index content remain intact and outside the baseline.

For a genuine continuation of this same execution after interruption or context compaction, verify the recorded branch and M0-A baseline instead of creating another branch. An unrelated pre-existing feature branch is not evidence that this task's bootstrap occurred.

## 2. Source Authority and Reconciliation

The current checked-out implementation branch is the source of truth.

The Goal's findings were inspected at **04e51606c2e1862e262454f424ed576bb4bc0814** on jump. At inspection, master was **0cb3dabf148dc720ed560626c32ef596b5e8ff9a**, and jump was 46 commits ahead. These are planning provenance, not a request to use jump as the branch base.

After M0-A:

1. Recheck the relevant current modules, symbols, callers, tests and data ownership.
2. Adapt stale paths/seams while preserving the Goal's functional requirements.
3. Record meaningful differences in the execution record.
4. Do not merge/cherry-pick the unrelated newer branch or recreate its entire historical verification program merely to match planning notes.

The Goal and this Kickstart are self-contained. Do not require the user to reconstruct requirements from earlier conversation or provide an old source package. Proposed modules in the Goal are proposals, not assertions that those files already exist.

The user request and current Goal define scope. Historical plans, acceptance matrices and testing campaigns are reference material unless this Goal explicitly adopts a particular part.

## 3. Execution Sequence and Agent Ownership

Follow the repository's real orchestration capabilities. Use configured roles where available:

- **Planner:** Validate a bounded checkpoint's current ownership, approach, dependencies and verification.
- **Implementer:** Change only the assigned checkpoint and run its scoped checks.
- **Reviewer:** Independently inspect the relevant changes for concrete correctness, regression and scope findings.
- **Orchestrator:** Own task state, source reconciliation, delegation, verification evidence, Git boundaries and final reporting.

Use a single writer for shared repository state. Read-only investigation/review can be parallel; overlapping source changes, test migrations and command/GUI registration work cannot. Do not run simultaneous agents that all modify src/gui/mod.rs, command routing, plugin registration or settings ownership.

If specialized agents are unavailable, perform the same bounded responsibilities with available tools. Do not block because a particular role/tool name is absent.

### Checkpoint order

| Stage | Required result |
|---|---|
| M0-A | Fresh branch, its own baseline commit, initial best-effort upstream push. |
| M0-B | Reconcile source, discover actual tests, set bounded verification and fixture approach. |
| M1-A/B | Settings catalog/search, then exact-control navigation with draft preservation. |
| M2-A/B | Guarded target/reference repair, then file/folder UI and maintenance integration. |
| M3-A | All five Clipboard Modify operations through commands, UI and saved pipelines. |
| M4-A | Searchable Paste Sequence snippet/history pickers with concealment and reference semantics. |
| M5-A | Numbered screenshot markers in retained history, preview and exports. |
| M6-A/B | Delayed capture lifecycle, then countdown/target/visibility integration. |
| M7-A/B | Image comparison domain, then imgdiff plugin/dialog. |
| M8-A/B | Actual owned pin translucency, then controls/reset/static/live evidence. |
| M9-A/B | Revision-safe note replacement, then UI/undo/save integration. |
| M10-A/B | CSV/TSV domain/output, then csv plugin/dialog. |
| M11-A/B/C | Duplicate discovery/identity, streamed verification, then dff UI/actions. |
| M12-A | Focused final review, integration/lint/evidence, documentation and handoff. |

Do not release a checkpoint until its required predecessor exists. If an external/native blocker affects one feature, document it and finish independent authorized work where safe. Never silently remove that feature from the completion report.

### Implementer handoff packet

Give each implementer the relevant Goal checkpoint plus:

1. Current branch and new baseline SHA.
2. Exact objective and architectural owner.
3. Verified current files/types/callers and base differences.
4. Required behavior and edge cases.
5. Relevant invariants and explicit non-goals.
6. Earlier dependencies that are complete.
7. Specific tests/native checks and their deadlines.
8. Acceptance criteria and commit boundary.

The implementer must not reconstruct the whole project from a broad narrative or re-plan unrelated milestones. Ordinary private naming, Rust ownership decisions and compiler-driven adjustments belong to the implementer.

## 4. Hard Rules During Implementation

### Preserve the functional contract

Deliver all eleven selected requirements. Treat the detailed C1–C11 requirements as binding.

In particular:

- Settings search navigates through existing owners without saving, activating controls, discarding drafts or enabling disabled plugins. Do not promise exact field discovery for opaque external DLLs or break their interface.
- Repair includes **folders**, original-record validation and the specified exact active preferences/references. Changing only a displayed path is insufficient when path-keyed associations would become stale.
- Clipboard line transforms extend the existing typed pipeline and preserve old configuration/names/undo.
- Paste Sequence pickers retain alias/fingerprint references. Do not turn them into stored clipboard bodies or leak concealed snippets through filtering.
- Marker preview and exported pixels come from the same retained annotation state.
- A scheduled screenshot is not a successful capture. Cancellation, foreground-at-deadline, selection timeout and owned cleanup must be correct. Normal launcher hide must not suspend/cancel the countdown. New delayed/cancel actions return an explicit GUI-required error headlessly, without an immediate capture or external fallback.
- Use **imgdiff** for image comparison; keep the existing diff path grammar intact.
- Pin opacity is real display translucency, not a destructive pixel transform or tint over an opaque background. Static/live exports remain unchanged.
- Note replacement operates on the live unsaved source, uses existing save/discard behavior and supports a single undo operation.
- **csv** handles CSV and TSV, keeps strings intact, uses correct quoting and explicit JSON shape, and never overwrites its source.
- **dff** is the required duplicate-file plugin prefix. It verifies exact duplicates and is **finder-only**: no delete/recycle/keeper automation/hard-link replacement. Identity-unverified content matches are labeled separately and excluded from verified independent-copy totals.

Do not expand into workspaces, unrelated UI redesign, broad settings/plugin/command frameworks, global ID migration, a new clipboard service, a spreadsheet editor or automatic filesystem cleanup.

### Keep work responsive and owned

Do not put traversal, hashing, large parsing/diff work, long joins or countdown sleeps on the GUI frame path. Use existing workers/cancellation/reaper patterns with bounded delivery and generation checks. Closing/reopening a dialog must not apply an earlier session's results.

New utility registration must not start a worker, watcher, clipboard read, drive scan or timer during ordinary application startup or search.

Respect exact native window/process ownership. Never target the launcher root or an arbitrary HWND for pin changes. Never kill every process by executable name during verification.

### Use existing architecture

Reuse the current typed command/parser/host/bus routes, GUI Panel/focus/render/close bookkeeping, plugin enablement, Help and keyboard/Jump ownership. Verify the complete end-to-end route, not only a domain module.

Avoid duplicate sources of truth, string-based side dispatch and a second persistence owner. A focused CSV parser/writer dependency may be added if justified by the actual current graph; do not upgrade unrelated libraries.

## 5. Checkpoint Commits, Pushes and Persistent State

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The Goal already specifies those boundaries. Begin with the new M0-A baseline, then commit each coherent completed checkpoint on the same task branch.

Before each commit:

1. Confirm the expected branch.
2. Inspect scoped checks and the real diff.
3. Confirm that no unrelated/pre-existing work entered the index.
4. Stage only task-owned changes. Avoid blanket add operations over a dirty repository.
5. Use a descriptive subject with the checkpoint label.
6. Record the commit SHA and actual checks/evidence.
7. Attempt a normal best-effort push when permitted; record success or local-only status and continue.

Do not squash, amend, rebase, rewrite, force-push, discard user commits or silently switch branches. Use small remediation commits for later fixes.

Use the Goal's execution record or an already suitable repository ledger. Track pending, in_progress, complete, blocked and verification-limited states distinctly. Record base/baseline/branch, current checkpoint, completed commits, actual push results, selected tests, evidence paths and blockers.

Measure committed task changes against the **new baseline**, not against an old feature-branch commit. Inspect any remaining uncommitted work against the recorded starting state so user changes are not mistaken for task output.

Do not generate redundant new plans or restart the initiative after context compaction. Read the ledger, verify actual Git state, and continue the next incomplete authorized checkpoint.

## 6. Bounded Verification

Follow Section H of the Goal. Verify the installed environment rather than copying commands blindly.

### Automated checks

- Discover actual nextest selectors and targets after M0-A, restricting enumeration as well as execution to relevant --lib or --test targets. Confirm nonzero intended test selection.
- Prefer exact module/test/binary selections and meaningful behavior assertions.
- Preserve current nextest default/CI profiles; use the opt-in bounded task profile.
- Use a terminating per-test timeout, not merely a SLOW warning. Default is 60 seconds; narrowly justified native tests may use 120 seconds.
- Bound each selected test batch to 10 minutes of execution. Bound compilation separately; the initial build/check ceiling is 30 minutes.
- Keep retries at zero for ordinary validation. If group interference is plausible, rerun the exact failed test once in isolation and retain both outcomes.
- Run one final combined selected suite for changed behavior; do not automatically run the entire repository test suite.
- Inspect return codes, logs, counts and real assertions. A zero-match filter, timeout, skipped run or isolated pass after a group failure is not a clean pass.
- Use UTF-8 logs without ANSI color where practical for SSH review.

Use a small number of focused remediation rounds. Do not enter endless test/benchmark loops or broaden validation once the concrete risk is resolved.

### Native UI checks

When the environment supports a Windows interactive desktop:

1. Create an owned isolated profile and synthetic fixture data using the existing helpers where appropriate.
2. Build/reuse the intended binary with only needed QA features.
3. Launch, wait for observable readiness, interact with the actual controls, and verify resulting state/output.
4. Capture a small useful screenshot/evidence set.
5. Shut down normally, then clean up only the retained owned process tree after the bounded deadline.

Use 90-second scenarios by default, up to 180 seconds for a justified multi-window scenario, and bounded cleanup. Keep the whole native evidence batch within the Goal's 20-minute post-build budget.

A root framebuffer capture does not prove that another application is visible behind a translucent pin. Use real composited evidence for that requirement. A countdown screenshot does not prove capture/history correctness. Inspect the actual operation result.

If native interaction is unavailable, record NOT_RUN_ENVIRONMENT/BLOCKED_NATIVE accurately, continue all independent work and disclose the remaining gap at final handoff. Do not ask the user to run intermediate checks or fabricate a passing result.

### Final lint and review

Perform the final first-party Clippy pass after implementation, scoped to the actual changed package/targets/features. Exclude vendored and unrelated cleanup. Use focused reruns after real fixes, not repeated whole-project lint cycles.

Have an independent reviewer inspect the cumulative change against the new baseline and directly relevant surrounding code. Review especially repair persistence, hidden-source projection, capture lifecycle, native opacity, note undo, table data preservation and DFF verification.

Fix concrete feature-related findings and revalidate the affected part. Do not weaken tests or acceptance criteria to finish.

## 7. Autonomy and Blockers

Proceed through the complete plan without asking whether to continue at each checkpoint. The user has authorized routine source inspection, required code changes, targeted builds/tests, fixture generation, application launch/interaction, screenshots, scoped repairs, commits and best-effort pushes for this execution.

Do not require intermediate manual testing, screenshots or commands from the user. The user will review after Codex has finished.

Make ordinary decisions from the Goal, current source, existing tests and project conventions. Do not ask about private helper names, routine Rust ownership, minor layout or equivalent implementation details.

Stop for intervention only when a genuinely non-inferable consequential decision, required unavailable access, unsafe branch transition or required destructive action prevents authorized progress. Do not bypass tool or approval restrictions.

For a blocker, record:

- The exact affected checkpoint/requirement.
- Concrete evidence and the actual error.
- Bounded remedies attempted and their results.
- Current branch/commit and safe local state.
- Independent work that can continue.
- The smallest necessary next decision or environmental capability.

A failed push alone does not block implementation. A pre-existing unrelated failing test does not authorize a broad repair campaign. A native verification gap must not be disguised as complete acceptance.

## 8. Completion and Final Handoff

Finish when every applicable Goal acceptance criterion has passed and required task changes are committed, or when real remaining blockers have been documented after independent work is complete.

Before reporting completion:

1. Check all eleven feature rows against implementation and evidence.
2. Verify relevant compatibility and no known feature-related blocking defects.
3. Resolve concrete reviewer findings.
4. Inspect formatting, scoped lint, the baseline-to-HEAD committed task diff and Git status. Report preserved pre-existing working-tree/index changes separately; do not clean them merely to make the repository appear pristine.
5. Confirm no fixture process, stale dialog worker, countdown or owned pin remains from testing.
6. Confirm the new task branch, base SHA, baseline SHA, final HEAD and real push status.
7. Update Help/documentation and the execution record with actual results.

Final response must include:

- Completed checkpoints and feature behavior.
- Important implementation decisions and source files by owner.
- Actual tests/builds/lint/native checks, counts and outcomes.
- UI evidence paths and what they establish.
- Branch name, original base SHA, new baseline SHA, checkpoint hashes/subjects, final HEAD, upstream and push/local-only status.
- Any incomplete requirement, genuine blocker, known limitation or unverified native acceptance.
- A short final review guide for the user, after autonomous work is finished.

Do not claim that code, tests, screenshots, pushes or acceptance passed unless they actually did. Do not stop at a vague “ready for testing.” Do not begin optional extra features after this goal is satisfied.

**Begin now by reading the applicable instructions and the entire Goal, then perform M0-A: safe fresh branch → new baseline commit → best-effort initial upstream push → recorded branch/base/baseline.**

