# Codex Kickstart — Calculation Pad Editor Usability

## 1. Read the contracts, then execute M0-A first

Read the current repository `AGENTS.md` and applicable local instructions, then the entire companion **`CODEX_GOAL_CALCULATION_PAD_EDITOR_USABILITY.md`**, then this Kickstart. The Goal is the feature-specific contract; repository instructions govern engineering and configured orchestration. Current task scope takes precedence over historical plans or acceptance campaigns.

**Before editing application code, running implementation builds/tests, or starting M1, establish the fresh chained Goal branch and its M0-A baseline.** Goal and Kickstart describe the same task and share exactly one new branch.

### Mandatory Git bootstrap — independent execution contract

1. Use the **single existing repository checkout**. Read-only inspect `git status -sb`, branch, `HEAD`, remote/upstream/default branch, all staged/unstaged/untracked changes, and the queue/orchestrator/execution handoff for the immediately previous completed Goal/category.
2. Resolve and record `parent_branch` and `parent_tip_sha`. In a continuing sequence, the required parent is that immediately preceding completed Goal/category's exact `final_tip_sha`. A local committed tip remains authoritative if its push failed. Fetch safely if needed to obtain a remote-only required commit and verify it. **Only the first task in a genuinely new chain defaults to the up-to-date master/main integration tip.** A permission to reference master's code is not an execution-parent override.
3. Never infer the parent solely from the current checkout, archived documents, or a stale remote. If records conflict or the required ancestor is unavailable on this machine, report a bounded blocker. Do not choose an older base or restart the chain from master/main. A documented current-HEAD base is allowed for a genuinely first task only when safe and no user work is lost or accidentally committed.
4. Select a fresh, unique lowercase kebab-case task branch with an appropriate prefix; prefer `feat/calculation-pad-editor-usability` if unused. The Goal, Kickstart, checkpoints, implementer, and reviewer do **not** get separate branches.
5. Protect existing user changes. Never reset, hard-pull, stash, rebase, merge, force-checkout, or overwrite work to make the transition possible. Never stage unrelated changes. If selecting the required parent is unsafe, preserve work and report the exact blocker.
6. Create and switch to the new child directly at the recorded SHA, e.g. `git switch -c feat/calculation-pad-editor-usability <parent_tip_sha>`. Verify the active branch begins at the required parent.
7. Create its own **M0-A baseline commit**: a small task ledger consistent with project conventions, or an intentional empty commit. Suggested subject: `chore(calc-pad): [M0-A] establish editor usability baseline`. Record `baseline_sha` separately from `parent_tip_sha`; verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's last commit cannot substitute for this baseline.
8. Attempt one ordinary non-force initial upstream push, such as `git push -u origin HEAD`, when the intended remote/access is available. Record the real outcome or exact unavailability. If rejected/unavailable, preserve local commits and continue safe work without repeated retries or bypasses.
9. Keep all implementation, tests, checkpoint commits, and ordinary push attempts on this branch. One normal best-effort push follows each required checkpoint; a push failure never erases or blocks safe local progress.
10. At completion, record the exact `final_tip_sha`, parent, baseline, significant checkpoint SHAs, actual push state, and checkout state. The next Goal/category must create its child from **this exact final tip**, including any final reporting commit. Do not silently carry uncommitted partial work across Goal boundaries.

**NO GIT WORKTREES.** Do not run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create auxiliary checkouts, helper clones, agent-specific clones, independent branches off master/main, or per-checkpoint branches. All branch transitions, source changes, and commits are sequential in the existing checkout.

Do not merge, rebase, squash, or cherry-pick intermediate Goal branches merely to advance the sequence. Each child inherits its predecessor. The final branch holds cumulative work for a separate approved integration boundary.

Use the authoritative existing task/queue ledger. If none exists, use the Goal's proposed `docs/plans/execution/standalone/calculation-pad-editor-usability/EXECUTION.md`. Record baseline SHA after creating it, and persist it in a later normal update. Resolve `final_tip_sha` after the last commit in the final report/external execution state; do not enter a self-referential amend loop.

M0-A must meet the Goal's explicit ancestry, baseline, push-recording, and user-work-protection criteria before implementation begins.

## 2. Confirm the bounded implementation approach

After M0-A, verify the Goal's actual source locations and relevant current behavior. The checked-out repository is execution truth. Adapt moved symbols or already-completed functionality while preserving every approved user-visible requirement.

The work covers:

- Full-line `//` comments alongside `#`.
- Results presentation filtering without changing physical line numbering or stable IDs.
- Line-number and wrap controls, real horizontal scrolling, and small remembered display preferences.
- Draggable Source/Results dividers in both existing orientations.
- Row-wide **Go to source**, reusing accepted navigation and adding a temporary highlight.
- Focused nested-function verification and directly relevant docs/tests.

Defaults are defined in the Goal: line numbers on, word wrap off, remembered application-level preferences, approximately 50% wide/48% stacked source sizes, and roughly 1.5-second navigation highlighting.

The intended nested example is `x = min(100, min(50, 75))`. The original missing final parenthesis is a typo, not authorization for automatic repair. Existing code already has nested-call infrastructure; first characterize the requested balanced examples. If they pass, keep the parser and add focused coverage/examples. Change parsing only for a demonstrated defect.

Inspect/reuse the named owners in the Goal: `prepare_line`/`declared_names`; physical-line session/evaluation data; `contents_ui`/`editor_ui`/`results_ui`; stable dependency navigation and retained `HostEffects`; the existing settings-save/preference completion paths; and local splitter patterns.

Do not redesign the application, replace the editor/parser, add dependencies, alter document formats/limits, change agent models, repair the full test suite, or adopt historical native-validation machinery.

## 3. Use the configured agents with one writer

Read the actual `.codex/config.toml` and configured role files. Honor their active settings; do not substitute remembered models, uncomment alternatives, or edit agent configuration for this Goal.

- **Planner:** Validate ownership, dependencies, risk, and bounded milestone packets through read-only inspection. Do not pre-implement or run broad verification just to elaborate the plan.
- **Implementer:** Perform the actual source/test changes, one bounded checkpoint at a time, in the shared task checkout.
- **Reviewer:** Independently inspect completed work for correctness, regressions, scope, and missing acceptance evidence.
- **Parent/orchestrator:** Coordinate handoffs, serial verification, source-write ownership, checkpoint commits, push attempts, ledger updates, blockers, and final reporting.

Use actual available configured roles/tools; do not claim a role was run if unavailable. Follow the repository's documented fallback if a role cannot be instantiated, and state that limitation accurately.

For each implementer packet, provide the Goal checkpoint, concrete owner/symbols, required behavior, invariants/non-goals, focused tests, and done criteria. Do not delegate a vague instruction to “improve the editor.”

Only one agent writes source at a time. Read-only investigation/review may run in parallel when safe, but reviewers assess an identified coherent diff/commit and do not race an evolving patch. Agents do not create their own branches or checkouts. Run only one build/test process at a time.

## 4. Execute all checkpoints without intermediate permission stops

Follow this sequence on the same branch:

| Checkpoint | Deliverable |
| --- | --- |
| M0-A | Correct predecessor → fresh child → distinct baseline → initial push attempt |
| M1-A | Consistent slash-comment classification, nested-call characterization, relevant syntax/examples |
| M1-B | Filtered Results presentation preserving physical identity and Copy all |
| M2-A | Line numbers, no-wrap/wrap geometry, compatible display preference persistence |
| M2-B | Usable draggable dividers with independent remembered orientation ratios |
| M3-A | Row-wide Go to source with current/retained navigation and temporary highlighting |
| M4-A | Scoped acceptance, independent review, relevant remediation, committed final-tip handoff |

Use the Goal's nine-part checkpoint specifications. Finish each coherent increment and its required scoped evidence, inspect the diff, commit, attempt an ordinary push, then proceed. Do not stop after comments, after editor controls, or after any intermediate checkpoint to ask whether to continue.

Make ordinary engineering decisions independently. You are authorized to inspect, implement the defined changes, run scoped tests/builds/lint, launch/interact/capture evidence when supported, resolve relevant defects, commit, and attempt normal pushes. The user performs one final manual review after your work.

Stop for a real unresolved requirement, unsafe Git transition, unavailable required ancestor, or consequential action beyond the authorized scope. Do not ask the user to run intermediate commands or supply screenshots.

## 5. Preserve the critical invariants

The Goal contains the full contract. Keep these constraints visible while implementing:

1. **Physical lines remain authoritative.** `rN` refers to physical source line N, including comments/blanks. Filter only the Results UI. Keep source bytes, line IDs, evaluation/dependency collections, and source-containing Copy all output intact.
2. **Errors and zero remain visible.** Blank/comment placeholders disappear; incomplete expressions and unavailable formatting are not silently treated as blank success.
3. **One editor and one history owner.** Keep the TextEdit and session-owned undo/redo. Gutter/wrap/split/navigation do not dirty the document or change evaluation revision.
4. **Use real text geometry.** Wrapped continuations, Unicode, CRLF, trailing newlines, horizontal offset, and clipping must agree with the actual galley.
5. **Publish only accepted effects.** Guard preference, divider, navigation, focus, and highlight changes against discarded/sizing passes and retired hosts. Reuse current document/incarnation/LineId checks.
6. **Focus once; highlight temporarily.** Result navigation must not repeatedly steal focus or later clear a selection the user made. Preserve root-query interactivity with a popped-out Pad.
7. **Save preferences through the existing owner.** Preserve unrelated settings/drafts, coalesce pending changes, and avoid writes on every drag frame. Do not alter saved-pad or recovery formats.
8. **Keep existing actions.** Select for comparison, Copy, Inspect bits, dependency navigation, pending close decisions, dock/popout, and recovery behavior remain functional.

## 6. Perform bounded, meaningful verification

Use the Goal's targeted Nextest selections, settings tests, shared-host checks, and small UI smoke. Validate that test filters select runnable cases. Restrict the target to the needed library/integration target; a test-name filter alone does not prevent unrelated tests in that binary from compiling.

Start with no more than two Cargo build jobs and one test process, with no concurrent agent builds. Use the Goal's **100-second selected test-execution batch limit, excluding compilation**, with separately bounded binary preparation and supported metadata reuse/owned-process timeout. Inspect current tooling; do not invent CLI flags, create a general testing framework, or change global profiles.

Prefer exact newly changed tests during iteration. Rerun an apparently contention-related failure once individually and report both results. Fix actual feature regressions; do not weaken assertions, ignore meaningful cases, or absorb unrelated old failures into this Goal.

For tests/profiles constructing the application, disable Paste Sequence `next_hotkey` and `cancel_hotkey` before construction as required by `AGENTS.md`; leave production defaults and error reporting alone.

When supported, build one suitable binary, launch the app with the existing isolated profile mechanism, and perform the Goal's compact docked/popout smoke. Cover gutter/wrap/scroll, both divider orientations, value/error Go to source and highlight expiry, existing result actions, query focus, and preference restart behavior. Capture useful evidence. Do not require repeated release builds or a pixel-diff campaign.

If native interaction is unavailable, record the exact limitation after the bounded attempts in the Goal. Automated evidence does not justify claiming native interaction passed. Continue safe independent work and report limited/unverified acceptance accurately.

Perform one appropriate final first-party Clippy pass, address findings caused by this Goal, and record unrelated warnings. Avoid vendored cleanup and repository-wide formatting churn. Full-suite tests, benchmarks, and broad regression sweeps are not default requirements.

Use no more than two evidence-driven correction/recheck rounds for the same persistent blocker. If exhausted, preserve progress and follow the current `AGENTS.md` stalled-goal reporting policy. Do not create an endless native-debugging or test-remediation loop.

## 7. Commit, review, and hand off the real final tip

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The companion Goal already defines those boundaries; follow them.

Before every commit, confirm the task branch, inspect staged paths, and exclude pre-existing/unrelated work. Use descriptive `<type>(calc-pad): [M#-X] <description>` subjects. M0-A is mandatory and separate from the predecessor's last commit. All later checkpoints stay on the same branch.

Attempt one ordinary push after each required commit. Retain local commits and continue on failure; record what actually reached the remote. Do not force-push, reset, rewrite history, or switch bases to repair a push.

After implementation, obtain the configured independent review, resolve feature-related findings, and rerun only affected checks. Inspect the entire Goal diff and final repository state. Complete the Goal's Definition of Done; use explicit limited/blocked status when required evidence or functionality is missing.

The final report must include:

- Overall status; completed checkpoints; requirement-level implemented/automated/native coverage.
- Key decisions and relevant changed files.
- Actual build/test/lint/review results, case counts, isolated reruns, and UI evidence.
- Remaining defects, environmental limitations, and optional concise final user-review steps.
- `parent_branch`, `parent_tip_sha`, current branch, `baseline_sha`, checkpoint SHAs, exact `final_tip_sha`, actual pushed/local-only state, and clean/dirty status.
- The explicit next-task instruction: **Create the next Goal's fresh branch directly from this `final_tip_sha`; do not return to master/main.**
- Confirmation that no Git worktrees, auxiliary checkouts/clones, agent branches, or intermediate integration merges were used.

Stop at the completed feature or a precisely documented bounded blocker. Do not begin unrelated enhancements, merge the final branch, or continue into another Goal without the appropriate instruction.

