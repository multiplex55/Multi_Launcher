# Codex Kickstart — Calc Scripting and Studio Polish

Execute `CODEX_GOAL_CALC_SCRIPTING_STUDIO_POLISH.md` autonomously from beginning to end. The user approved **Q1: A, Q2: A — proceed**. Do not repeat the questionnaire or ask for another approval to begin.

This Kickstart and its Goal are one task, using **one shared fresh branch**, not separate branches for the documents, milestones, or agents. The Goal contains the detailed feature contract. This prompt governs how to execute it.

## 1. Mandatory first execution — read instructions, resolve ancestry, create the child branch, baseline, and push

Before application edits, implementation builds/tests, or M1:

1. Read the repository's `AGENTS.md`, any applicable local instructions, this Kickstart, and the **entire** `CODEX_GOAL_CALC_SCRIPTING_STUDIO_POLISH.md`. Treat the Goal as the authoritative feature-specific contract and repository instructions as the engineering/orchestration contract. Historical plans and old native acceptance campaigns are reference only unless this Goal explicitly adopts them.
2. Inspect read-only Git state in the **single existing repository checkout**: `git status -sb`, current branch, `HEAD`, remotes/upstream/default branch, index, untracked/modified files, and any active queue/orchestrator/ledger handoff. Inventory user changes before choosing a parent. Do not edit feature code, run implementation tests/builds, or create later checkpoints yet.
3. Determine whether this is a genuinely new chain or the next Goal/category in an existing sequence. **For a continuing sequence, the required base is the immediately previous completed Goal/category branch's exact `final_tip_sha`**, regardless of the current checkout's branch. Verify that predecessor's intended changes are committed. Record `parent_branch` and `parent_tip_sha`. Fetch safely when necessary to obtain the exact ancestor. Never substitute an older remote tip for a locally completed but unpushed predecessor.
4. **Only the first Goal of a genuinely new chain may default to the up-to-date primary integration branch**, usually `master`/`main`, unless explicitly overridden. Safe fetch/inspection is allowed, automatic pull/merge/rebase is not. A safe current-HEAD initial base is allowed only when genuinely starting a new chain, documented, and preserving all user work. The Goal's reference commit identifies inspected source, **not** the required execution parent. If active ancestry records conflict or the required ancestor is unavailable, report a bounded blocker rather than guessing a base.
5. Select a **fresh, uniquely named, idiomatic branch**, preferably `feat/calc-scripting-studio-polish` with a unique suffix as needed. An already-active task branch is not permission to skip the child branch for a newly started Goal. If this exact Goal is being resumed after interruption, verify its existing recorded baseline/checkpoints and resume that same branch instead of resetting or creating a second baseline.
6. **NO GIT WORKTREES.** Do not run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create auxiliary checkouts, helper clones, secondary working directories, agent-specific clones/branches, or independent branches from a common integration base. All write work and branch transitions happen sequentially in the current checkout. This computer has one active build/test lane.
7. Protect user changes. Never automatically reset, stash, hard-pull, rebase, merge, force-checkout, overwrite, or stage unrelated changes to manufacture a safe transition. Do not commit through an unsafe index/unresolved merge. If the required parent cannot be selected safely, preserve existing work and report the precise blocker; do not silently choose another parent.
8. Create and switch to the child branch **directly at the recorded parent SHA**, for example `git switch -c <unique-task-branch> <parent_tip_sha>` with verified actual values. Verify the new branch is active and starts at that SHA. Do not implement on `master`, `main`, or the predecessor.
9. Create this Goal's own **M0-A baseline commit**. Use a small task-ledger entry, or an intentional empty baseline when appropriate: `chore(studio): [M0-A] establish calc scripting branch baseline`. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals the recorded parent tip. The predecessor's last commit is not the new baseline. Stage no unrelated user files.
10. Attempt one ordinary initial upstream push, normally `git push -u origin HEAD`, when access/permissions allow. Do not force-push or bypass security. Record the exact outcome, including when permission prevents an attempt. A denied/unavailable push does not block safe local implementation; retain the local branch/commit and continue without retry loops, branch changes, pulls, or history rewriting.
11. Reuse the active authoritative queue ledger. For standalone execution, use `docs/plans/execution/standalone/calc-scripting-studio-polish/INTEGRATION_STATUS.md`. Record parent, active branch, baseline, checkpoints, actual verification, preserved user changes, and push state. Do not create competing ledgers. After commits, record their actual identities externally or at the next ordinary ledger update; do not amend a commit to try to place its own SHA inside itself.

**Hard gate:** M0-A must establish the unique active child branch, exact-parent ancestry, direct-child baseline, and truthful initial-push outcome before any application work. An unavailable push is nonblocking; an unresolved unsafe parent/checkout is not.

All later implementation, tests, checkpoint commits, and pushes remain on this same branch. This Goal has **no internal category branch splits**. Do not create a branch per checkpoint, agent, or Markdown document.

At completion, capture the actual post-final-commit `final_tip_sha`. The **next Goal must create its child branch from this exact tip**, not from `master`/`main` or another old ancestor. No intermediate merge, rebase, squash, or cherry-pick is needed or permitted merely to advance the sequence. Preserve cumulative history for one later approved integration boundary. If the next machine lacks an unpushed ancestor, it must block until that commit is available.

## 2. Confirm the current source and approved scope

After M0-A, execute M0-B. Recheck the Goal's source ownership against the current correctly based checkout. Adapt moved paths and already-implemented portions without changing user requirements. Use the actual current source/tests/call sites, not assumptions from historical plans. The Goal is self-contained; do not require the user to reconstruct the conversation.

The approved feature areas are:

- **Editable Mermaid flowcharts:** ordinary comments preserved through visual edits, `graph` headers, semicolon-separated statements, chained supported arrows, RL/BT directions. Preserve the existing separate non-flowchart preview families; this is not full Mermaid conformance.
- **Calculation Pad:** sequential reassignment; optional inputs/watchlist; toggleable completion and hints; theme-aware highlighting; Find/Replace; synchronized selection; display-format controls; smart editing; folding/navigation; lazy conditional expressions; collection statistics; bounded ranges; and actual multi-line `for` blocks in the current calculation language.
- **QR Generator:** join the shared Dock / Pop Out host while retaining one session, payload, correction level, cached result, correct focus, and explicit exactly-once clipboard/file effects.

Use the Goal's language semantics and ordinary engineering defaults. Do not reduce multi-line loops to collection expressions only. Do not add a Python runtime, REPL, functions/imports, workspace features, plotting, unrelated plugins, or a GUI rewrite. The earlier brainstorming list is not an implementation backlog for this Goal.

Key preservation points: exact integer and unit semantics; contextual programmer operators; physical `rN` references; legacy saved documents; source-only opt-in recovery; current-revision comparison/Inspector/dependency snapshots; one source/undo owner; retained-pass side-effect acceptance; no dropped Mermaid comments; independent main-query focus.

## 3. Use the repository's configured agents

Inspect and follow `.codex/config.toml` and the actual `.codex/agents/planner.toml`, `implementer.toml`, and `reviewer.toml` configurations. Preserve their configured models, reasoning levels, permissions, and local instructions; this Goal does not authorize changing agent models.

- **Planner:** Validate the next checkpoint's source owners, dependencies, invariants, test boundaries, and executable handoff. Read-only inspection should stop when the implementer has enough information. Do not create a second large plan or pre-implement the work.
- **Implementer:** Use the configured implementation agent for actual source changes, one bounded checkpoint packet at a time. The packet includes the objective, files/owners, existing facts, required tasks, behavior, edge cases, regression constraints, focused tests, done criteria, and commit boundary from the Goal.
- **Reviewer:** Independently inspect the affected diff against the original requirements and acceptance criteria. Focus on correctness, incomplete migrations, stale-state bugs, data loss, regressions, and invalid tests. Review is read-only and scoped, not an opportunity to redesign unrelated systems.
- **Orchestrator:** Own sequencing, the single write/build/test lane, active ledger, checkpoint decisions, Git commits/push attempts, bounded remediation, and final reporting.

All agents use the **same checkout**. Read-only reasoning/review may overlap safely, but never run competing writes, branch switches, commits, or independent builds/tests. Do not invent unavailable agents/tools or claim delegation that did not happen. If a configured role cannot be invoked, record that capability limitation and use available tooling within repository policy; do not install or redesign agent orchestration for this task.

## 4. Execute checkpoints in order without stopping for routine approval

Follow this sequence on the shared branch:

| Checkpoint | Outcome |
| --- | --- |
| M0-A | Verified chained branch, direct-child baseline, initial push outcome. |
| M0-B | Current ownership/baseline and focused verification scope. |
| M1-A | Approved bounded editable-flowchart syntax. |
| M1-B | Comment-safe visual round trips and actual RL/BT behavior. |
| M2-A | Sequential binding/shadowing and comparison/dependency updates. |
| M2-B | Lazy conditions, ranges, and collection statistics. |
| M2-C | Bounded multi-line loop parsing/execution. |
| M2-D | Responsive current-revision execution and consumer integration. |
| M3-A | Find/Replace, physical-line navigation, synchronized selection. |
| M3-B | Theme-aware highlighting, completion, and real call signatures. |
| M3-C | Safe source folding and optional smart editing. |
| M4-A | Optional source-backed input controls and output watches. |
| M4-B | Pad-local precision-safe display and copy formatting. |
| M4-C | Backward-compatible preferences/document view metadata. |
| M5-A | QR shared-host migration and lifecycle integration. |
| M6-A | Focused verification, independent review, docs, final-tip handoff. |

Read each complete checkpoint in the Goal before delegation. Complete its required work and focused verification, inspect the actual diff, commit the coherent increment, attempt a normal push, update the ledger, and **continue**. Do not stop after Mermaid, the language engine, or any individual editor feature merely to ask whether to proceed.

Keep an explicit requirement-status matrix for MMD-01…05, CP-01…16, and QR-01. “Implemented,” “automated verified,” and “native verified” are separate fields. Do not mark a later checkpoint verified merely because its prerequisite compiles.

Routine engineering choices are yours to resolve. Ask for intervention only for a genuine consequential/destructive authorization issue or an irreducible requirement blocker. Do not ask the user to run commands, test controls, capture screenshots, create branches, or troubleshoot push failures during execution. The user will review at the end.

## 5. Apply bounded verification, not a new regression campaign

Use Goal section H and the repository's current testing instructions. Discover actual test names with `cargo nextest list`; run focused `--lib` filters and directly affected integration/host/settings tests. A filter that executes zero tests is not a pass. Do not invent a test target from a feature's name.

Keep selected test execution batches bounded to **100 seconds after compilation** using supported executor controls. Do not impose that runtime cap on cold compilation or invent Nextest timeout flags. On a hang, capture the failing test identity, terminate its process tree, preserve/drain available output, and diagnose that case rather than repeatedly restarting everything. Lower build concurrency only for demonstrated machine constraints, and distinguish it from test execution concurrency.

A concurrency-only failure gets one individual/serial diagnostic rerun. Establish whether it is a feature regression, legitimate behavior change, stale test, or unrelated environment issue. Fix the owner; never weaken assertions, delete tests, inflate production stack size, or mark a failing batch green because one isolated rerun passed.

For application fixtures, set Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` before constructing the app, as required by `AGENTS.md`. Do not change production defaults or globally suppress error toasts.

Build and launch the Windows application when the environment supports it. Prefer the existing `fast-dev` build profile and reuse the build until a relevant change requires another. Exercise UI-1…UI-8 from the Goal and capture representative screenshots/action logs using safe fixture data. Exact arithmetic, filesystem effects, and clipboard content require corresponding assertions/observations, not just a screenshot. Retain evidence paths and actual build/commit identity.

No automatic full test suite, repeated expensive release builds, benchmark campaign, historical native qualification matrix, or candidate-cycle process. Broaden only when a concrete touched shared boundary makes narrow verification insufficient, and record why.

Perform one appropriate final first-party Clippy pass and touched-file formatting checks. Resolve introduced findings. Preserve/report unrelated existing findings and exclude vendored crates from unrelated cleanup. Never mute warnings globally just to claim a clean result.

For a repeatedly unresolved validation/native/environment failure, use an initial diagnosis plus at most two focused remediation rounds, then record a precise blocker/limitation. Compilation fixes remain normal development; this budget prevents unproductive repetition. Continue independent safe work where possible, but never claim the blocked requirement is done.

## 6. Commit and push each coherent checkpoint

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The Goal already defines those boundaries and stage-tagged subject suggestions.

The mandatory first commit is this Goal's M0-A baseline, directly descended from `parent_tip_sha`. All subsequent checkpoints remain on its branch. Before committing, verify branch/HEAD/status, inspect the diff and staged path list, and stage only intended task changes. Commit coherent verified increments; use small remediation commits rather than rewriting history.

After a required commit, make one ordinary best-effort push using the configured upstream or a permitted `git push -u origin HEAD`. Retain local progress and continue after failure. Never force-push, squash/amend/rebase checkpoints, pull/merge merely to repair a rejected push, or create checkpoint/agent branches. A failed push is not a request for the user to intervene.

Track `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint subjects/SHAs, `final_tip_sha`, actual upstream/push outcome, and clean/dirty status. If a final documentation commit moves HEAD, report the resulting actual tip, not the earlier implementation SHA.

## 7. Final review, blockers, and truthful status

At M6-A, have the configured reviewer inspect the completed affected diff against the Goal. Resolve feature-related findings with bounded fixes and revalidate the affected checks. One scoped review plus fix verification is sufficient; do not restart design or open unrelated cleanup work.

Confirm all approved requirements, migration invariants, examples/help, focused verification, native evidence where possible, and Git ancestry. Unsupported native interaction is **unverified**, not passed. Use:

- **COMPLETE:** All required implementation/acceptance checks pass, including native behavior when claiming it verified; no known feature blocker remains.
- **LIMITED:** Implementation and available automated checks pass, but required native/environment verification is unperformed.
- **PARTIAL/BLOCKED:** Requested implementation or feature-related acceptance remains incomplete.

On a genuine exhausted-acceptance/blocked stop, follow `AGENTS.md` and Goal section I. Create a concise tracked report in the active queue's `stalls/`, or `docs/plans/execution/standalone/calc-scripting-studio-polish/stalls/<UTC-timestamp>.md`, and update `INTEGRATION_STATUS.md` when safe. Record the exact failed/unrun check, expected versus actual behavior, attempts, evidence, branch/parent/tip, preserved partial work, actual push state, and smallest safe next action. A documentation-only stop commit is not an accepted implementation checkpoint. If tracked reporting is unsafe, use the existing external ledger first.

Do not claim native acceptance from source inspection, claim a command ran when it did not, or hide an unmet requirement behind a broad “all features implemented” statement. Do not discard partial work or roll back unrelated changes.

## 8. Final response and mandatory next-Goal handoff

Provide the Goal's section K report: completion status; user-visible changes; completed checkpoints and unmet requirement IDs; actual decisions/deviations; significant files; commands/tests/results; review/lint outcomes; native evidence and unverified interactions; limitations; and short optional final-human-review steps.

Include exact Git identity:

```text
parent_branch:
parent_tip_sha:
branch:
baseline_sha:
checkpoint_commits:
final_tip_sha:
upstream_and_push_state:
working_tree_state_and_preserved_user_changes:
```

State explicitly, substituting the actual final SHA:

> The next Goal must create its fresh child branch directly from `<final_tip_sha>`, not from `master`/`main`. No Git worktrees, auxiliary checkouts, agent-specific clones, or intermediate branch merges were used.

Qualify that statement truthfully if any workflow violation occurred. Record the same handoff in the authoritative ledger/final report; do not silently carry uncommitted partial work into the next category. A remote push failure must distinguish local-only commits from successfully published ones.

Stop after this Goal and its required reporting. Do not implement unrelated ideas, begin another Goal, or promise later/background work.
