# Codex Kickstart — Desktop Productivity Expansion

Execute `docs/plans/CODEX_GOAL_DESKTOP_PRODUCTIVITY_EXPANSION.md` autonomously in the current Multi Launcher repository. This is an implementation request, not another planning/questionnaire round. The Goal is the feature-specific contract; `AGENTS.md` and applicable local instructions govern engineering and configured agent orchestration.

## 1. Mandatory First Execution — Read, Verify Parent, Create Child, M0-A, Push

**Before editing application code, running implementation builds/tests, or starting M1, read `AGENTS.md`, applicable nested instructions, the entire Goal, and this Kickstart. Then execute the Goal's A0 bootstrap in the single existing checkout.**

### Inspect Git safely

Perform read-only inspection of `git status -sb`, current branch, `HEAD`, index/untracked changes, any merge/rebase state, remotes, upstream/default branch and the preceding Goal/category handoff. Check the queue/execution ledger, not just the current branch. Do not stage or alter user changes during inspection.

### Resolve the exact predecessor

The recorded accepted/frozen predecessor for this continuation is:

```text
parent_branch: fix/studio-usability-root-jump-recovery
parent_tip_sha: 7c49f083474afcdc0eb57eb3e88bbd32561165a8
```

Verify the actual commit and handoff before starting C01. The prior recovery's implementation was accepted; its native gate remained environment-blocked. Do not reopen its completed source repair or assert that the outstanding native flows passed. Its records under `docs/plans/execution/standalone/studio-launcher-usability-recovery/` provide ancestry and inherited limitations, not an additional acceptance campaign for this task.

A newer **explicitly authorized, verified** handoff may supersede this recorded predecessor; record that authority. If records conflict or the required ancestor cannot be found, report a bounded ancestry blocker instead of guessing. Fetch safely when necessary; do not substitute a stale upstream tip for a newer local final commit.

**Every subsequent category starts directly from the immediately preceding category's actual committed `final_tip_sha`. Never return to master/main between categories. Only the first task in a genuinely new chain may default to the up-to-date primary integration branch. This request is a continuation.** For a genuinely new chain with no predecessor, a safe current-HEAD base is allowed only when clearly documented and user changes are protected.

### Create and verify the new category branch

Choose a fresh, unique, idiomatic lowercase kebab-case name using the Goal's category stems. Start C01 from the exact verified parent:

```text
git switch -c <new-category-branch> <parent_tip_sha>
```

The Goal and Kickstart describe **one shared branch per category**, not separate branches for documents, milestones, checkpoints or agents. Even an already-active task branch is not permission to skip the required fresh child branch for this newly authorized category. Do not implement on the predecessor, master or main.

Do not automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite existing work or stage unrelated files. If selecting the required parent is unsafe with the current working tree, preserve it and report the exact blocker. Do not “solve” this by selecting an easier but incorrect base.

Verify that initial child HEAD equals `parent_tip_sha`. Create the category's own **M0-A baseline commit**, preferably a small execution-ledger entry; an intentional empty baseline commit is acceptable. Record `parent_branch`, `parent_tip_sha`, new branch and `baseline_sha`. Verify that:

```text
git rev-parse <baseline_sha>^
```

returns the expected parent tip. The predecessor's final commit is **not** this category's new baseline.

Attempt one ordinary non-force initial upstream push, normally `git push -u origin HEAD`, when the intended remote and permissions are available. Record the true result. Push rejection/unavailability does not block safe local implementation: preserve the local branch/commit and continue without retries, security bypasses or history rewriting. Do not ask the user to create branches or push them manually.

### Single checkout, no worktrees, no intermediate merging

**Never use Git worktrees.** Do not run `git worktree add`, `git worktree move` or `git worktree prune`. Do not create auxiliary checkouts, helper clones, secondary source trees, agent-specific branches or independent branches off a common master base. All branch transitions and source edits occur sequentially in the existing checkout.

Keep each category's checkpoints on its one branch. After committing the preceding category's final increment/report, create the next child from its exact final tip. Do not merge, rebase, squash or cherry-pick intermediate categories to advance. The last branch already contains cumulative history; a single final integration PR/merge belongs only at the approved integration boundary. Do not automatically merge into master/main.

A0/M0-A is a hard prerequisite to implementation. The only nonblocking bootstrap failure is a correctly reported unavailable/rejected push; unsafe ancestry or checkout state must not be ignored.

## 2. Approved Requirements — Do Not Re-Ask

The user selected **Q1 A and Q2 A**, accepted the remaining defaults, and authorized execution. Do not request another questionnaire, scope confirmation or routine implementation choice.

The approved work is the Goal's selected IDs **1–46, 61, 88, 89, and 119**, organized into seven categories:

| Category | Outcome | Milestones |
| --- | --- | --- |
| C01 | Same-session Dock / Pop Out across all requested ordinary editor/utility surfaces, with geometry and focus preservation | M0-B, M1–M4 |
| C02 | Text-based exact programmer arithmetic, separate inspector, comparisons, unit arithmetic, examples/reference | M5–M7 |
| C03 | Preserved Mermaid authoring plus rich visual diagram canvas, library, exports and safe draw.io subset interchange | M8–M11 |
| C04 | Saved ordered folder lists opening as verified tabs in one new identified Explorer window | M12 |
| C05 | Read-only progressive folder size analyzer | M13 |
| C06 | Read-only file lock/user inspector | M14 |
| C07 | Short local silent GIF and video-only MP4 recording; cumulative verification and delivery | M15–M17 |

Each category additionally begins with its own **Cxx/M0-A** branch baseline and push attempt. M17 remains on C07; do not invent an eighth cleanup branch.

Critical product decisions are settled: the pad remains text-first; the programmer inspector is separate; ordinary decimal calculations keep their present arithmetic; `^` remains exponentiation; no exact-decimal mode is authorized. Host moves preserve one live session. Mermaid and rich visual documents have distinct appropriate authority. draw.io support is an honest subset, not full parity. File analysis is not deletion/unlocking. Recording has **no microphone or system audio**. No workspace feature is in scope.

Do not reproduce the whole contract here or invent new product requirements. Read the detailed semantics, acceptance matrix and per-checkpoint tasks in the Goal.

## 3. Reconfirm Owners, Then Use the Existing Agent Workflow

After M0-A, inspect relevant current source, tests and callers to validate the Goal's owner map. The checked-out repository is the source of truth. Adapt renamed paths and ordinary private implementation details without changing the approved behavior or rebuilding an already existing subsystem. Do not require any external planning inputs or conversation reconstruction to proceed.

Use the repository's configured planner, implementer and reviewer definitions under `.codex/` where available. Respect their actual model/settings and capabilities; do not rewrite agent configuration or require unavailable tools/models. If a role cannot be launched, perform its bounded responsibilities sequentially and disclose the limitation; never claim an independent review occurred when it did not.

- **Parent/orchestrator:** Own execution order, ledger, branch selection, Git staging/commits/pushes, verification receipts, dependencies, blocker decisions and final response. Maintain one source writer at a time.
- **Planner:** Read-only validation of the next checkpoint's owner, needed changes, dependencies, invariants, test selection and acceptance. Produce a bounded implementer-ready packet, not a new open-ended initiative. Do not pre-implement or run expensive speculative builds during planning.
- **Implementer:** Execute one explicit checkpoint packet at a time. Modify the appropriate owners, add relevant tests, run the authorized focused checks, and report actual changes/results. Do not independently branch, broaden scope or begin the next checkpoint.
- **Reviewer:** Read-only inspection of the actual diff and relevant callers/tests. Report concrete correctness, compatibility, ownership, scope or missing-evidence findings. Do not redesign working code for style alone or initiate unrelated test campaigns.

Read-only investigation may run in parallel when safe, but it must converge before implementation. Do not have competing edits, test-fixture changes or simultaneous Git operations in the shared checkout. Source edits and review-driven fixes remain sequential. All roles share the same checkout; no worktrees, helper clones or independent implementation branches.

For each checkpoint handoff include: objective, files/owners, concrete tasks, expected behavior, important edges, regression constraints, focused verification, acceptance criteria and commit boundary. The Goal already supplies these fields; refine only live-code details that genuinely need refinement.

## 4. Execute the Sequence and Maintain Persistent State

Use the existing authoritative queue ledger when present; otherwise use:

```text
docs/plans/execution/standalone/desktop-productivity-expansion/EXECUTION.md
```

Track category/current branch, ancestry/baseline, checkpoint status, selected-feature coverage, per-surface host coverage, relevant evidence, attempts and remaining blockers. Do not create a separate planning document for every agent or maintain multiple conflicting ledgers.

Begin with M0-B's bounded baseline/host inventory after C01/M0-A. Then follow the Goal in order. Establish shared hosting before migrating many tools, exact integer primitives before inspector UI, and rich document authority/persistence before canvas/export additions. Carry current worker/session identities through host migrations instead of copying state into new windows.

Complete coherent code and its focused checks before the next subsection; commit at the stated checkpoint boundary. If a listed checkpoint has several substantial natural increments, use small labeled same-branch subcommits. Do not accumulate a whole multi-tool migration into one huge final commit. Do not create a branch per checkpoint.

At each category handoff:

1. Finish or accurately freeze the category at a coherent committed state, applying Section 7's blocker rules. Preserve explicit native gaps rather than declaring them passed.
2. Record tests/review/evidence and any dependency limitation, commit the final task/reporting increment, then obtain the actual final HEAD.
3. Record the category's `final_tip_sha`, actual push state and checkout state in the authoritative receipt/report.
4. Create the next category's fresh branch **directly from that exact tip**, make its new M0-A baseline, verify its parent and attempt its initial push before implementation.

A previous local commit is the required parent even when pushing it failed. If execution moves to a different machine without that commit, stop at that bounded ancestry blocker instead of using the older remote branch.

Do not stop after each checkpoint to ask whether to continue. Proceed through the authorized sequence while dependencies are sound. Do not ask the user to run tests, open windows, capture screenshots or validate intermediate milestones. The user's manual review is after delivery.

## 5. Preserve the Important Behavioral Boundaries

Use the Goal's full preservation contract. Pay particular attention to:

**Input and windows:** global launcher toggle/hold behavior, actual focused editor, Root Jump ownership, native Alt+Space/Escape, accepted UI passes, generations, pending dirty decisions, prompt parentage, root-hidden work and monitor/DPI recovery. A host move is not a close, reopen, restart or action replay.

**Documents and settings:** exact source, old calculation meaning, version-1 diagram readability, source/visual authority, history and selection, settings draft vs committed values, unknown-data preservation, atomic output and collision safety. Do not auto-save simply because a window moved.

**Workers and effects:** exactly-once command/clipboard/runtime effects, stable requests/target identity, bounded queues, cancellation and stale-result retirement. Keep traversal, native enumeration, capture conversion and encoding off the GUI thread. Do not add idle permanent services for unopened tools.

**Native safety:** Explorer input targets only the verified owned operation; folder scans stay read-only; file lock inspection never terminates/unlocks; recorders never silently retarget, capture audio or publish unfinalized output. Existing screenshots, Pinboard, Screen Draw, Paste Sequences and ZIP extraction must retain their protections.

Do not treat a useful reference implementation as a reason to copy another entire subsystem. Reuse the actual domain/platform boundary and migrate affected callers deliberately. New dependencies/features need a concrete requirement, minimal enabled scope and reviewed lockfile changes. No unrelated framework upgrade or vendor cleanup.

## 6. Focused Verification and Evidence

Use Goal H's exact scope and native flows, adapted to live test names. Prefer pure tests, owner/state tests and targeted `cargo nextest` runs. Confirm nonzero matches. Use cached `fast-dev` builds where appropriate, distinguish compilation from test execution, and record literal commands/results rather than a generic “tests pass.”

Do not automatically run the complete repository suite, every feature/smoke binary, repeated release builds or broad performance campaigns. Do not edit nextest TOML/Cargo profiles, disable failing tests, relax meaningful assertions, or conceal warnings to obtain a green result. A full-suite run requires a concrete reason tied to the changes, recorded before expanding verification.

Keep formatting scoped. Perform final first-party Clippy/review at M17, excluding unrelated vendored cleanup. Fix introduced relevant findings; classify historical unrelated findings with evidence. Do not assume the prior 15 Clippy findings still describe the current change without checking what the actual run reports.

Build/launch/interact with the real application when supported. Use existing native tooling and disposable fixtures; capture sanitized screenshots and actual GIF/MP4 artifacts as evidence. Test-only app profiles must set Paste Sequences `next_hotkey = None` and `cancel_hotkey = None` before construction; do not change production defaults or globally suppress errors.

For shared hosting, every named surface needs a state/wiring check; native evidence may sample ordinary families as specified in H.4, but Notes/Markdown/MkMacro/Settings and other distinct lifecycle owners cannot be declared covered by one Calculation Pad screenshot. Check actual state and side-effect counts, not appearance alone.

For Explorer, verify the target window, tabs and paths. For lock inspection, use a known disposable held-file helper. For recording, independently inspect/play encoded files, elapsed time/dimensions and **zero audio streams**. Fake-backend success proves state logic, not native capability. An Unsupported message is an error-path check, not successful implementation of the intended supported-target workflow.

Do not invent multi-monitor, reader, encoder, UI interaction or test results. A missing native setup must be reported as unverified. If a legitimate non-UI native check can run while the visual observer is unavailable, record its narrower result without relabeling the full UI flow passed.

## 7. Bounded Validation — Continue Useful Work Without Hiding Gaps

Keep separate implementation, automated-check, native-check, review and delivery status. Use the Goal's states and budgets rather than inheriting an older initiative's acceptance procedure.

For a concrete feature failure, allow initial validation plus at most two focused correction/revalidation cycles. Diagnose the responsible owner, fix the relevant defect and rerun only invalidated checks. If the same blocker persists, record it; do not reset the budget by renaming stages, generating new plans or creating extra branches.

For unavailable native environment/observer capability, use at most one supported recovery/reprobe after the initial capability check. If the same unchanged environment failure has already been established, preserve that finding rather than re-probing once per category. Do not bypass permissions, repeatedly restart services/hosts, invent helper access or require the user to repair the environment during implementation.

**Authorized continuation:** when implementation, focused tests and review are sound but native checks are environment-blocked, commit/freeze the category with status **Implemented — native verification pending**, then continue independent authorized work from its exact final tip. Preserve the missing native flows in the ledger and final report. Do not call the category or the overall Goal Complete. Dependent work may reuse this reviewed implementation with the disclosed native limitation; it may not ignore a known shared correctness defect.

A true feature blocker stops its dependent work. Independent categories may continue only when the committed state is coherent, the missing behavior is explicitly documented, and unsafe active code is not silently inherited. Missing required implementation, failing correctness tests, data-loss behavior or an encoder/tab operation that never works on a supported target are not merely observer/environment limitations.

Unrelated baseline failures, unrelated lint warnings and optional enhancements must not expand the Goal or prevent safe progress. Push failures never trigger resets, rebases or repeated retries.

For an actual Blocked/Paused/exhausted-acceptance stop, write the concise tracked report required by `AGENTS.md`, using the existing queue `stalls/` directory or:

```text
docs/plans/execution/standalone/desktop-productivity-expansion/stalls/<UTC-timestamp>.md
```

Include current category/checkpoint/status, exact ancestry and preparation-time HEAD/upstream, verified/missing behavior, failed/unrun command or native step, evidence, attempts and remaining budget, cleanup, checkout/index state, preserved work, dependencies and the smallest safe next action. Separate hypotheses from proven causes. Do not include personal payloads or uncontrolled logs.

A safe reporting commit is allowed and gets one ordinary push attempt; it is not an accepted implementation checkpoint. When tracked reporting is unsafe, record in the existing external ledger and preserve the checkout instead of discarding work.

## 8. Active Checkpoint Commits and Pushes

**Use the project's active checkpoint commit cadence and the task-specific boundaries in the Goal.** The mandatory first commit of every category is its own M0-A baseline directly atop the recorded predecessor tip.

Only the parent/orchestrator stages and commits. Before each commit verify branch/HEAD, intended staged paths, diff and focused check results. Preserve unrelated user files; never indiscriminately stage everything. Use stage-tagged descriptive commit subjects such as `feat(calc): [M5-B] integrate programmer syntax without scalar regressions`.

Commit locally first. Attempt one ordinary push after each checkpoint when supported. On rejection/unavailability, record the exact outcome and continue locally without permission bypass, force-push, history rewrite or retry loop. Later checkpoints may each make their normal single attempt. Report which commits actually reached the remote; do not imply GitHub visibility for local-only commits.

Do not merge intermediate branches, squash checkpoints, rewrite accepted history or create new branches merely because a new agent is taking over. Only the seven explicit category handoffs create branches.

## 9. Final Review, Actual Tip Receipt and Stop Boundary

At M17, audit every selected ID and required host surface against implementation and evidence. Have the configured reviewer inspect the cumulative feature changes and relevant boundaries. Resolve substantive task-related findings within the bounded budget, then rerun only affected checks. Report lack of independent review honestly if the role is unavailable.

Determine status using Goal J:

- **Complete:** all required implementation and acceptance, including mandatory native behavior, passed with no feature blocker.
- **Implemented — native verification pending:** implementation/tests/review are sound, but specified native gates could not run in the environment.
- **Partial / Blocked:** some required implementation or correctness is missing. Identify exact IDs/checkpoints and dependencies.

Push status is separate. Local-only commits can still be implemented/verified, but cannot be claimed pushed.

Commit the final tracked report/documentation first, then read the actual HEAD. Record `final_tip_sha` in the external queue receipt or an ignored receipt under `target/desktop-productivity-expansion/`, and in the final response. A following category/task may record its parent SHA in tracked history. **Do not create recursive commits just to write a commit's own hash inside itself.** If a final reporting commit changes HEAD, hand off that newer commit, not the earlier implementation tip.

The final response must include:

1. Seven-category status table and completed checkpoints, with selected-feature omissions explicit.
2. Important decisions, changed owners/files and actual verification/evidence, clearly separating run/passed/failed/unrun checks.
3. Every category's branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, actual push result and checkout state.
4. The actual cumulative final tip and this next-task instruction: **Create a fresh child branch directly from this exact `final_tip_sha`, not master/main; create M0-A, attempt push, use one checkout, no Git worktrees and no intermediate branch merging.**
5. Remaining limitations, concise blocker next action where needed, and a compact end-of-task manual review checklist. Do not claim checks passed because the user can test them later.

Confirm no worktrees, auxiliary checkouts/clones or intermediate category merges were used. Preserve any unrelated user changes and report them rather than claiming a clean checkout falsely.

Stop when the approved work and required validation are complete, or at a precisely documented bounded stop with preserved progress. Do not continue into unselected utilities, unrelated cleanup, another historical recovery, or an automatic integration merge. Do not ask whether to proceed after each checkpoint; this Kickstart already authorizes the full bounded sequence.
