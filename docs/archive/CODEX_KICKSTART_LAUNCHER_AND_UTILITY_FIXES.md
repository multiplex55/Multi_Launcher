# Codex Kickstart — Launcher and Utility Fixes

Execute `CODEX_GOAL_LAUNCHER_AND_UTILITY_FIXES.md` completely and autonomously. Read it from its actual repository location, normally `docs/plans/` if installed there. The Goal is the feature contract; repository `AGENTS.md` and configured agent instructions supply the engineering/orchestration conventions. This Kickstart and the Goal describe one task and one shared branch.

## 1. Mandatory first execution: read, inspect Git, create the exact-tip child, baseline and push

Read `AGENTS.md`, applicable local instructions and the entire Goal before making changes. Then perform read-only Git checks in the **single existing checkout**: `git status -sb`, current branch and `HEAD`, remote/upstream/default-branch information, staged/unstaged/untracked changes, and the immediately preceding Goal/category's authoritative execution handoff.

Before editing application code, running implementation tests/builds or starting M1, execute **M0-A**:

1. Resolve and record the exact `parent_branch` and `parent_tip_sha`. In an existing chain, use the immediately preceding completed Goal/category branch's final tip, even if the current checkout happens to be `master`/`main`. Verify the live ledger and Git object; do not use a remembered, old or guessed SHA. The existing `docs/plans/execution/standalone/productivity-tools-refinement/STATUS.md` may provide the relevant handoff, but inspect any newer records. A terminal LIMITED predecessor retains its verification limitations; confirm its development is finished and its final tip is the intended handoff.
2. Only the first Goal in a genuinely new chain defaults to the up-to-date primary integration branch. Fetch safely when necessary to verify remote-only commits. Do not default subsequent tasks back to `master`/`main`. If handoffs conflict or the required parent is unavailable, report a bounded ancestry blocker. A documented safe current-HEAD starting base is allowed only for a genuinely first task and only without losing or incorporating unrelated work.
3. Choose a fresh, unique lowercase kebab-case branch, such as `fix/launcher-utility-fixes` with a unique suffix if needed. Goal and Kickstart share this branch. Do not reuse the predecessor or an already-active branch as the new Goal branch.
4. Protect all pre-existing user changes. Never automatically reset, hard-pull, stash, rebase, merge, squash, cherry-pick, force-checkout, overwrite or stage unrelated work to make switching possible. If the correct transition is unsafe, preserve state and report the precise blocker rather than choosing a different parent.
5. Create and switch directly from the exact recorded parent: `git switch -c "$task_branch" "$parent_tip_sha"`. Verify the starting `HEAD`. All application work belongs to this new branch.
6. Create this Goal's own **M0-A baseline commit**, preferably a small execution ledger at `docs/plans/execution/standalone/launcher-utility-fixes/STATUS.md`, or use the authoritative queue ledger. An intentional empty baseline is acceptable. Record `baseline_sha` separately and verify its immediate parent equals `parent_tip_sha`. The predecessor's final commit is not this Goal's baseline.
7. Attempt one ordinary non-force initial upstream push, normally `git push -u origin HEAD` after checking the remote. Record the result. If unavailable, denied or failed, keep the local branch/commit and continue safe implementation. No bypasses, history changes or retry loops to make the push succeed.
8. Keep all subsequent A–H checkpoints and review/report commits on this same branch. Commit coherently and attempt normal pushes after checkpoints. No branch per milestone or agent, and no intermediate merges/rebases/squashes/cherry-picks merely to advance the work.
9. At the final handoff record the task branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant checkpoint SHAs, actual push/checkout state and exact `final_tip_sha`. The next Goal must create its own unique child directly from this final tip. If a local final tip was not pushed, it remains the required parent; another machine cannot substitute stale remote state. The cumulative chain is intended for a single final integration PR/merge at the authorized integration boundary.

**Never use Git worktrees.** Do not run `git worktree add`, `git worktree move` or `git worktree prune`, and do not create auxiliary checkouts, helper clones, agent-specific branches or independent parallel branches from a common base. Use one existing checkout with sequential source writers and sequential branch operations. Do not ask the user to create the branch manually.

M0-A is accepted only when the unique active branch, exact ancestry, direct-child baseline, initial push attempt and preservation of user changes are verified and recorded. Capture commit hashes after committing; avoid self-referential ledger-commit loops.

## 2. Mandatory order: A, B, C, GitHub peer review, then D through H

After M0-A, verify the Goal's source locations against the active checkout and execute this sequence:

| Checkpoint | Task / deliverable |
| --- | --- |
| M1-A | **A:** root query works alongside popped-out dialogs; genuine clicks return control to the user |
| M2-A | **B:** localized root-result fade/flicker fix with list/grid navigation preserved |
| M3-A | **C:** default release profile and bounded dependency-feature review |
| M3-B | **C:** localized idle-work reduction and ordinary `cargo build --release` |
| M3-R | Independent A–C peer review, committed and published on GitHub |
| M4-A | **D:** remembered Calculation Pad scratch source closes after successful persistence without a save prompt |
| M5-A | **E:** primary drag on Diagram Viewer preview moves the correct window |
| M6-A | **F:** Diagram Studio pending-change decision stays at the top by host controls |
| M7-A | **G:** bounded details, sortable/filterable typed rows and correct Reveal in Explorer action |
| M7-B | **G:** larger directory/discovery capacity with explicit bounded resource accounting |
| M8-A | **H:** actual Explorer Tabs Shell-baseline identity failure corrected and verified |
| M9-A | Final focused review, factual status, committed exact-tip handoff |

Do not implement D–H ahead of the A–C checkpoint. Do not stop after C. Do not turn M3-R into a new Goal, an approval gate, a branch freeze or a merge prerequisite. The parent must continue directly into D, E, F, G and H after completing/publishing the interim review, subject only to a genuine safety/ancestry blocker. Read-only investigation may prepare later work.

**The user has already authorized the entire A–H Goal. Do not ask whether to continue after a milestone or wait for the user to read the interim review.** A worker returns after its assigned checkpoint; the orchestrator then assigns the next one. A worker's one-milestone scope is not permission for the parent to stop the Goal.

## 3. Use the repository's configured agents

Use the current planner / implementer / reviewer definitions and their configured models/reasoning settings. Do not rewrite those files or substitute remembered model choices as part of this task. Do not require roles/tools unavailable in the environment.

- The **planner** validates only the active milestone's ownership, current source, dependencies, invariants and narrow verification. Do not re-plan the whole Goal each time.
- The **implementer** receives one explicit packet with objective, files/owners, required changes, behavioral constraints, edge cases, non-goals, tests, acceptance and commit boundary. It implements that checkpoint, verifies it and reports back.
- The **reviewer** independently inspects the actual diff and related source, focusing on correctness, regressions, incomplete requirements and meaningful evidence. It does not edit source or expand scope.
- The **parent/orchestrator** owns sequencing, the authoritative ledger, Git, checkpoint commits/pushes, publication and final reporting. It continues the approved work without intermediate user intervention.

Only one source writer may be active at a time; overlapping changes and Git operations are sequential. Read-only investigation/review may be parallelized when safe. Run one Cargo/build/test job at a time; native stateful interaction is serial. Agents do not create separate branches, worktrees or checkouts.

## 4. Execute each bounded checkpoint completely

Use the full Goal milestone as the implementation packet. Inspect enough current code to identify the responsible boundary and existing tests; then implement the smallest complete change through that owner. Preserve the Goal's scope and invariants. Make ordinary engineering decisions without a questionnaire.

Pay particular attention to:

- Root presentation versus any-open-dialog state across query and Jump ownership; do not bypass genuine modals or force focus every frame.
- The root-results-only fade override; selection-scroll changes require evidence of a remaining problem.
- Effective release-profile inheritance, retained panic unwinding, supported decoders and existing renderer/platform behavior. Finding no safe dependency removal is acceptable.
- Hot Folder readiness as an optimization hint, with executor validation and shutdown wakeup preserved.
- Successful recovery persistence before silent scratch close; never use Discard as the silent-close implementation.
- Accepted viewer/host/pass identity for dragging; a stale effect must not drag the launcher.
- One top-level Diagram decision panel preserving the existing transition semantics.
- Folder Size stable generation/index actions, bounded cached presentation and both directory/path limits.
- Explorer Tabs' actual failed native observation, complete prelaunch exclusion baseline and verified target ownership. Removing a check or improving an error message alone does not complete H.

Verify the changed behavior, inspect the diff, commit the coherent checkpoint on the task branch, update the ledger and attempt one normal push. Preserve unrelated changes. Do not weaken assertions, disable meaningful tests or conceal failures. Resolve concrete reviewer findings with new small commits rather than rewriting checkpoint history.

## 5. Publish the interim A–C peer review without stopping

After A, B and C are implemented and their required available checks are finished:

1. Record the committed A–C code tip. Have the configured reviewer independently review `baseline_sha..abc_tip_sha` against A–C requirements and verification evidence.
2. Resolve substantive findings and rerun only affected checks. Record the final exact `abc_reviewed_tip_sha` after corrections and the reviewer follow-up.
3. Write `docs/plans/execution/standalone/launcher-utility-fixes/PEER_REVIEW_ABC.md`, or its authoritative queue equivalent. Clearly label it an automated independent peer review. Include reviewed SHAs, scope, findings/dispositions, checks actually performed and unverified native/build limitations. State explicitly that D–H remain active.
4. Commit and attempt to push the report/status. Reuse the PR for this task branch, or create one draft PR for the **entire A–H Goal** against the intended final integration branch if none exists and GitHub access is available. Publish the peer-review summary as a PR comment with the exact reviewed commit and a commit-pinned report link. Use the available GitHub connector or `gh`; supply multiline `gh` bodies through a temporary UTF-8 file with `--body-file`. Do not assign a named human reviewer, send unrelated notifications, merge or create a separate A–C branch.
5. Record the actual publication URL and pushed commit. Do not confuse a local report or failed push with GitHub publication. If publication is blocked by credentials/network/permissions, retain and commit the report, record `publication_pending` with the exact reason, and continue D–H; reconcile at a later ordinary checkpoint if access changes. Do not claim the full delivery is complete while the publication remains pending.
6. **Immediately start M4-A/task D on the same branch. Do not wait for user approval, a human review reply, PR approval, CI unrelated to this task, or merging.**

This is the user's requested interim visibility point. It must neither terminate nor pause the remaining Goal.

## 6. Perform proportional verification and keep moving

Follow the Goal's exact acceptance scope. Prefer directly affected Nextest cases and `fast-dev` where suitable, meaningful state/interaction tests, diff inspection and a few focused native checks. Confirm selectors match tests. Disable Paste Sequence next/cancel hotkeys in test fixtures before app construction as `AGENTS.md` requires; do not change production defaults.

Run ordinary **`cargo build --release` once at the end of C**, before the interim review. Preserve the Goal's build limitation if the Windows target/toolchain is unavailable; a different-target build does not prove the Windows EXE. Repeat a release build only when a concrete correction or later change invalidates required evidence. No automatic full-suite execution, benchmarks, exhaustive profiling, distribution campaign or historical native acceptance protocol.

Keep focused pure-test execution around a 100-second upper bound excluding compilation, using existing timeout/process tools. Split or narrow slow selections instead of inventing nextest flags or interrupting legitimate compilation. Native waits must use bounded existing deadlines. Do not run concurrent expensive Cargo jobs.

Build/launch/interact with relevant UI and capture concise evidence where supported. Do not require the user to run commands, perform intermediate tests or supply screenshots. At final review perform an appropriate first-party Clippy pass; fix task-related findings and record unrelated pre-existing findings without vendor cleanup.

Use the Goal's bounded correction budget. If a Windows desktop/capture backend, target toolchain or GitHub service is unavailable, record exactly what is unverified or pending and complete all remaining safe work. Do not falsely mark acceptance passed, repeatedly recreate failed native environments, or stop after C merely because publication/native evidence is unavailable. A real unsafe change, ancestry conflict or defect blocking safe dependent work warrants a precise blocker report.

## 7. Maintain truthful progress, commits and final status

Use one authoritative execution ledger. Keep task letters A–H and checkpoint IDs visible. Record implementation state, actual commands/results, native evidence, reviewed SHA and GitHub publication status separately. Preserve the user-directed checkpoint cadence:

> Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.

Every substantial coherent increment receives a descriptive checkpoint commit and ordinary best-effort push. Preserve local commits after push failures and continue. Do not force-push, reset, rebase, merge, squash or discard prior work to manufacture a clean result.

At a genuine stop, follow `AGENTS.md`'s concise tracked stalled-Goal report requirement under `docs/plans/execution/standalone/launcher-utility-fixes/stalls/` or the owning queue's equivalent. Record actual branch/HEAD/push state, verified/missing behavior, the concrete failure and attempts, preserved work and the smallest necessary next action. Do not use historical acceptance requirements to invent a stop.

At M9-A, independently inspect the final change set, incorporating the existing A–C review and concentrating on D–H and later corrections. Resolve feature-related findings and rerun only affected checks. Confirm the complete Goal's Definition of Done, report publication, intended files, ancestry, checkout and push state.

The final response must include A–H results; checkpoints; key decisions/files; actual tests/build/UI evidence; A–C review/report/PR URLs and reviewed SHA; remaining limitations; task branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, significant commits and exact `final_tip_sha`; actual push and checkout state; and one final compact manual check list if useful.

State explicitly: **The next Goal must create a fresh child branch from this exact `final_tip_sha`, not from `master`/`main`. No Git worktrees, auxiliary checkouts/clones, independent checkpoint branches or intermediate branch merges were used.** Capture the exact final HEAD after the last intentional commit; do not create endless commits to write a commit's own SHA into itself.

Stop only when the full A–H work and required delivery/verification are complete, or all feasible safe work is done and a genuine remaining blocker is precisely documented. Do not continue into optional enhancements after that boundary. **Task C and the interim peer review are never the planned stopping point.**
