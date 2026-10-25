# Codex Kickstart 13 — Explore folder sizes with bounded background scanning

Execute **[CODEX_GOAL_13_FOLDER_SIZE.md](CODEX_GOAL_13_FOLDER_SIZE.md)** completely and autonomously. This Kickstart is the execution/orchestration prompt; the Goal is the detailed feature contract. Requested coverage: #52. Execute this pair only.

## 1. Read the complete contracts before acting

Read the repository's `AGENTS.md` and any applicable local instructions, then this entire Kickstart and the entire companion Goal. Follow the configured engineering and planner/implementer/reviewer workflow. The current repository is the source of truth; the Goal's cited revision is planning provenance. These two documents and the checked-out repository provide the requirements; do not reconstruct them from earlier chat.

## 2. Mandatory first execution — fresh branch, baseline commit, initial push

**M0-A must complete before any feature-code edits, builds, implementation tests, benchmark runs or M1 work.** Read-only Git/source checks are permitted first.

1. Inspect `git status -sb`, branch, HEAD, actual default integration branch, remotes/upstream and pre-existing changes. Identify the intended remote and take a normal safe fetch when possible; record unavailability.
2. Select a new unique lowercase kebab-case branch with an appropriate prefix; `feat/folder-size-explorer` is a suggested stem. Check local/known remote collisions and choose a meaningful suffix. Never reuse a prior task branch silently and never ask the user to create it.
3. Normally base it on the up-to-date actual default integration branch (`master` in the reviewed repository; rediscover). Do not reset, hard-pull, stash, rebase, merge or force checkout. If a dirty checkout cannot switch safely, preserve all user work and branch from current HEAD where safe, recording the exception. Report a genuinely unsafe transition as a blocker.
4. Create and switch to the new branch. Create its own initial baseline commit before implementing: a legitimate task ledger/baseline record, or an intentionally empty commit such as `git commit --allow-empty -m "chore: [M0-A] establish folder-size branch baseline"`. Do not stage unrelated pre-existing changes. Record original base SHA and new baseline SHA; use the new baseline for subsequent task diffs and never rewrite it.
5. Attempt one normal non-force upstream push after that commit, for example `git push -u origin HEAD` only when origin is verified as intended and approvals permit. On denied/unavailable push, retain the branch and commit, record the reason and continue locally. Do not loop, bypass protections, or ask the user to fix ordinary push failure.
6. Verify the unique branch is active, baseline exists, base/baseline identifiers and push result are recorded, and pre-existing work is intact. Do all later implementation, tests, checkpoint commits and push attempts on this same new branch. Never silently switch back to the old branch or perform the task on `master`/`main`.

Required predecessor capabilities: **None**. Verify required implementations are already in the selected base, or verify a concrete equivalent. Recommended order alone does not transfer unmerged code. Do not automatically merge, cherry-pick, rebase or inherit a preceding feature branch. Only an explicit user-approved combined execution contract can override the base. A missing hard dependency is a precise blocker, not permission to duplicate it. Perform read-only prerequisite checks early enough to avoid unsafe work.

## 3. Revalidate current owners and orchestrate bounded work

After M0-A, verify the Goal's current source paths, call sites, settings, test targets and partial existing capabilities. Follow the actual implementation owner if earlier compilation work moved a pure engine behind a re-export. Do not add a second implementation to match an older filename.

Use the repository's configured roles when available:

- **Planner:** validate the next checkpoint's ownership, dependencies, invariants and verification scope.
- **Implementer:** implement one explicit checkpoint packet at a time, with concrete tasks and acceptance criteria.
- **Reviewer:** independently inspect meaningful completed changes for correctness, ownership, regressions and scope violations.
- **Orchestrator:** retain progress, enforce scope/deadlines, coordinate verification and own Git commits/pushes.

Parallelize bounded read-only analysis/review where useful. Use one writer for overlapping repository changes; do not allow competing agents to edit shared sources, tests, manifests or Git state. Use only roles/tools the environment actually provides.

## 4. Implement the Goal in order

The implementation checkpoints after M0-A are:

1. **M1-A — Build bounded metadata aggregation.** A domain scanner produces correct, honestly classified folder totals from a selected root.
2. **M1-B — Own background scans and cancellation.** Folder scans progress asynchronously with deterministic replacement and cleanup.
3. **M2-A — Add explorer UI and launcher integration.** The user can navigate and compare actual scan results through an accessible, responsive utility.

Read the full checkpoint in the Goal before delegating or implementing it. Complete its behavior, directly relevant tests, evidence and scoped review; inspect the diff and commit at the stated boundary before materially different work. Update execution state with completed, in-progress or blocked status. A written implementation is not a completed checkpoint until required acceptance evidence exists.

Feature-specific execution directions:

- Implement folder-size aggregation, not a disk cleanup feature.
- Do not silently inherit File Search exclusions or Duplicate Finder's hard-link accounting.
- Reuse current lifecycle primitives; goal 06 is recommended ordering, not a hidden hard dependency.

## 5. Continue autonomously and preserve behavior

Make routine Rust, UI, error-handling and test decisions from the current code and approved requirements. Reuse existing abstractions, defaults and persistence paths. Do not broaden the Goal into unrelated refactors, new application workspace features, speculative tools or global cleanup.

Do not stop after each milestone to ask whether to continue. Do not require the user to run intermediate commands, exercise the UI, provide screenshots or resolve a routine compiler error. Build/launch/interact/capture/close the relevant UI using the existing owned harness when supported, and inspect the evidence yourself. Request intervention only for a genuine unresolved requirement, unavailable essential capability or consequential action beyond the authorization.

## 6. Verify proportionately with finite deadlines

Follow Goal Sections H and I. Inspect installed tool support and current package/target/filter/features before invoking tests; confirm nonzero selected cases. Preserve command arguments, profiles, results and first failure evidence. Separate compilation time from test/native execution time. Use controlled clocks/events instead of arbitrary waits when feasible.

Run focused unit/integration tests and only the relevant native smoke. Avoid repeated release builds, default full-suite runs, broad cache deletion and open-ended benchmarking. Broaden checks only when a concrete affected boundary or failure warrants it, explaining why. Use finite owned-process deadlines and cleanup; a timed-out, empty, incomplete or unverified run is not a pass.

Allow at most two focused repair attempts per failing acceptance boundary before reporting a precise blocker or justified adjustment. Continue unaffected checkpoints. Never weaken assertions, disable meaningful tests, hide failures, or call a headless check native evidence. Distinguish source hypotheses, measured gains, no-change findings and unachieved objectives.

## 7. Commit checkpoints and attempt normal pushes

Use the project's active checkpoint commit cadence. The first commit was M0-A; later subjects follow the Goal's labels. Commit coherent verified increments on this same branch, staging task-owned changes only. Preserve user history and dirty work. Attempt one normal best-effort push after each commit, keep failed pushes locally and record actual results. Never force-push, rewrite/squash the recorded history, delete branches or change branch to solve a push failure.

## 8. Final review and correct stopping boundary

Review the cumulative diff against the new baseline; resolve feature-related review findings, remove obsolete paths created by this work, and run the appropriate affected first-party Clippy pass near the end. Exclude vendored code from unrelated cleanup and rerun only checks affected by corrections. Inspect final Git status.

Report completed/blocked checkpoints, important decisions, changed files/owners, actual verification commands/results, native screenshot/evidence paths, measured outcomes and limitations. **Report the branch name, original base SHA, M0-A baseline commit SHA, all checkpoint commit hashes and actual push status.** Clearly separate implemented, verified and unverified items.

Stop when this Goal and its required validation are complete, or at a genuine documented blocker. Do not merge, release, automatically start the next Goal, or continue optional optimization. The user will perform their final manual review afterward.
