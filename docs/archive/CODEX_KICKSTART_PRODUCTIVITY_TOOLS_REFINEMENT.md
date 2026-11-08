# Codex Kickstart — Productivity Tools Refinement

Execute `docs/plans/CODEX_GOAL_PRODUCTIVITY_TOOLS_REFINEMENT.md` autonomously. If the file is stored elsewhere in this checkout, locate that exact filename and read it in full. The companion Goal is the authoritative feature contract; `AGENTS.md` and applicable local instructions govern engineering and orchestration.

This Kickstart and the Goal describe **one task and one shared fresh branch**. Do not implement the application until the mandatory M0-A sequence below is complete.

## 1. Read instructions, then run M0-A before all application work

Read `AGENTS.md`, applicable local instructions, the entire Goal, and this Kickstart. Then perform only read-only Git/ledger inspection until the correct parent is established.

1. Inspect `git status -sb`, current branch, `HEAD`, remotes/upstream, staged/unstaged/untracked changes, and the active queue/orchestrator/execution records. Identify any existing active or paused task and preserve its work.
2. Resolve `parent_branch` and `parent_tip_sha`. For a continuing sequence, use the **exact recorded final commit of the immediately previous completed Goal/category branch**, verified against Git. Safely fetch a remote-only predecessor when necessary. Do not assume the current checkout is the correct parent; do not use an old common base or stale remote tip.
3. **Only the first Goal in a genuinely new chain** defaults to the up-to-date primary integration branch, normally `master`/`main`. A documented safe current-HEAD base is allowed for that first task only if no user changes are lost or inadvertently committed. Never restart an existing chain from the integration branch.
4. If ancestry records conflict, the exact ancestor is unavailable, or the predecessor's status is unresolved, report a bounded blocker. Do not guess a SHA or silently declare a paused predecessor complete.
5. Select a new, unique lowercase kebab-case branch, preferably `feat/productivity-tools-refinement` with a meaningful suffix if occupied. Use one branch for the Goal/Kickstart pair; checkpoints and agents do not get branches.
6. **Use the single existing checkout. NO GIT WORKTREES.** Never run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create auxiliary checkouts, secondary clones, agent-specific clones, or independent branches from a common base. Read-only agents may investigate/review in parallel; writes and Git changes are sequential.
7. Protect pre-existing changes. Never reset, hard-pull, stash, rebase, merge, force-checkout, discard files, or stage unrelated changes to manufacture a clean transition. If switching to the required parent is unsafe, stop that transition and document the precise blocker.
8. Create and switch directly from the verified SHA: `git switch -c <new-unique-branch> <parent_tip_sha>`. Verify its initial HEAD exactly equals the parent.
9. Create the branch's own **M0-A baseline commit**, using a small task ledger consistent with the repository or an intentional empty commit: `git commit --allow-empty -m "chore: [M0-A] establish productivity tools refinement baseline"`. Do not include pre-existing unrelated work. Record `baseline_sha` separately; verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`.
10. Attempt one ordinary non-force upstream push, normally `git push -u origin HEAD`, when the configured remote/environment allow it. If denied or unavailable, retain the local branch/commit, record the actual failure, and continue safe local work. Do not retry in a loop or bypass restrictions.
11. Record `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, initial push outcome and checkout state in the existing authoritative execution ledger. A standalone ledger may live at `docs/plans/execution/standalone/productivity-tools-refinement/STATUS.md`.
12. Only now begin application source verification, implementation builds/tests, or M1.

The M0-A baseline must be a **direct child of the prior Goal's final tip** for a continuing sequence. Do not treat the predecessor's last commit as this Goal's baseline.

All later checkpoints stay on this one branch. This Goal's milestones are feature increments within one approved task, not category-branch handoffs.

At completion, record `final_tip_sha`. **The next Goal must create its fresh child branch directly from this exact final tip**, including local-only commits if pushing failed. If execution moves to another machine, the exact ancestor must be available there; never substitute stale remote state. Do not merge, rebase, squash or cherry-pick intermediate goal branches merely to advance the queue. The last branch of the approved chain contains all prior work for a single final integration PR/merge at the approved boundary. Do not perform that final integration as part of this Kickstart.

## 2. Confirm the real source after bootstrap

Use the actual checked-out code and the Goal's source map. Locate moved symbols rather than inventing paths. Reconfirm the relevant current behavior and targeted test ownership; adapt internal details without changing the approved functionality.

Important starting findings to validate:

- Calculation Pad already has prefixed scalar literals. The binary-first problem crosses quantity dispatch and the `b` unit alias.
- Existing `%`, Euclidean `mod(a,b)`, and a bare variable named `mod` have distinct compatibility requirements.
- Vector integration extends typed values, named variables and physical-line `rN` references. No new `ans`, statistics, matrices or broad calculator replacement is required.
- Reference help is ROOT-only, and some content sits outside its scroll body; the Pad's own pop-out is not the requested independent help pop-out.
- Inspector width controls currently edit a target while the grid reads the working mode.
- ZIP catalogue/planner accept Deflate while the writer rejects it. Do not merely remove the guard or copy compressed bytes.
- Visual marquee paint ordering, fixed viewer height and click-only minimaps are concrete narrow UI changes. Clipboard, owned labels and manual connector routes require durable model integration.
- Explorer UIA uses legacy activation before requesting a window. Keep its finite-timeout preflight when correcting activation.
- Recorder reads the shared window cache without requesting its cold-start refresh. The encoded region crop already exists.
- Recycle's unrestricted `rec` prefix claims `record`; repair that provider, preserving general search semantics.

Do not assume source inspection or mocked tests establish native Windows success. Do not import historical full-regression matrices or past environment blockers as current gates.

## 3. Use the configured agent workflow

Read the repository's existing `.codex/config.toml` and role files. Preserve their current models, reasoning levels and permissions. The inspected configuration includes planner, implementer and reviewer roles; do not rewrite their settings or substitute remembered older defaults.

Use available roles proportionately:

- **Planner:** Validate a bounded checkpoint's architectural ownership, dependencies and exact handoff.
- **Implementer:** Own one explicitly assigned packet, make its code changes and run focused checks.
- **Reviewer:** Independently inspect completed changes and directly relevant surrounding code.
- **Orchestrator:** Own branch/ledger state, serial write scheduling, scope, verification, commits, pushes, blocker handling and the final report.

Do not require unavailable tools or roles. If a role is unavailable, fulfill its responsibilities through the capabilities actually present and record any material review limitation.

Give each implementer the packet's objective, exact source owners, tasks, behavior/edge cases, invariants, non-goals, dependencies, tests, acceptance and commit boundary. Do not make it reconstruct the entire Goal. Only one agent writes source at a time; do not run competing edits or separate Git histories. Parallelize bounded read-only analysis/review when useful.

Run one build/test/native job at a time on the shared machine. Avoid redundant builds from multiple agents.

## 4. Execute the Goal's checkpoints in order

Do not begin M1 until M0-A has established the correct fresh branch, direct-child baseline and initial push outcome.

| Milestone | Checkpoints | Deliverable behavior |
| --- | --- | --- |
| M0 | A | Verified ancestry, new branch, baseline and best-effort push |
| M1 | A–C | Scalar fixes/aliases, typed vectors, searchable independent reference window |
| M2 | A | Immediate checked Inspector modes, bit grouping and named ranges |
| M3 | A–D | Deflate extraction, default/bulk selection, safe ZIP creation domain and UI |
| M4 | A–E | Visual navigation/fonts/viewer, clipboard/keys, owned labels, persistent bends, templates |
| M5 | A–B | Timeout-capable Explorer UIA and saved-folder picker |
| M6 | A–B | Cold-cache window discovery and accurate recording-region indicators |
| M7 | A–B | Command-only help and Recycle prefix boundary |
| M8 | A | Focused native evidence, final review, lint and exact final-tip handoff |

For each checkpoint:

1. Validate its immediate assumptions against current source.
2. Implement exactly that packet with existing architectural owners.
3. Add/update the smallest useful behavioral tests.
4. Run focused verification and fix task-caused failures.
5. Inspect the diff for unintended files, stale paths, duplicate ownership and unrelated changes.
6. Record code/test/native status accurately. Native checks consolidated at M8 remain pending until executed.
7. Commit the coherent increment on this Goal's branch using its checkpoint label.
8. Attempt one normal push, retain local work on failure, and continue.

Do not stop after each checkpoint for permission. Do not create a branch for every milestone or merge intermediate branches. If an actual external blocker affects one feature, record it and continue independent packets where safe; never pretend the blocked behavior is satisfied.

## 5. Make the approved defaults concrete

Apply the Goal's detailed requirements instead of reopening routine decisions:

- Contextual aliases preserve identifiers and source. Infix `mod` matches `%`; existing function modulo remains distinct.
- Vectors are bounded finite dimensionless 1-D values with element-wise arithmetic, scalar scaling and explicit dot/cross products, integrated into variables/`rN`/copy/persistence.
- Reference search stays pinned above a wholly scrolling body; help has independent host state and does not mutate Pad content.
- Inspector changes width through checked conversion; explicit reinterpretation handles requested truncation. Bit layout grouping is separate from copied-number formatting.
- New archives default all physical members selected without bypassing unsafe-entry validation. Create mode accumulates sources, resolves archive-path conflicts, writes a new output through staging and never silently overwrites.
- Diagram text is owned and persistent; custom connector bends survive save/reopen, history, viewer and supported export. Do not satisfy those requirements with temporary screen-only geometry.
- Folder bookmarks copy their current paths into Explorer lists; no new foreign-key schema.
- Display/selected-region indicators use frozen physical region bounds and explicit lifetime ownership. Preserve Window mode's OS indicator; do not freeze its initial rectangle while the captured window moves/resizes. A platform-owned WGC border may remain when documented borderless capability/consent is unavailable; the accurate selected-region app indicator is still required.
- Fix the specific Recycle query boundary; keep fuzzy search and legitimate resolved provider results.

The Goal contains exact examples, negative cases and compatibility rules. Do not expand into matrices, additional archive formats, full draw.io parity, a font system, a packaging overhaul, workspace features, or unrelated refactors.

## 6. Work autonomously and preserve existing behavior

You are authorized to inspect and modify the required code, run targeted builds/tests, launch/interact with the app when supported, capture evidence, review/fix regressions, create checkpoint commits and attempt normal pushes within this contract.

Do not ask the user to run intermediate commands, provide screenshots, manually verify each stage, or approve routine internal choices. The user will perform one final review after the work is finished.

Do not treat a first compiler error, a correctable test failure, a failed push, an unrelated Clippy finding or an unavailable optional tool as an automatic reason to pause the whole Goal.

Only seek intervention when an important non-inferable requirement, inaccessible required resource, unsafe ancestry transition, or consequential destructive operation cannot be handled within existing authorization. First finish the concrete safe work available, then explain the exact blocker and its source.

Preserve existing hotkeys, command IDs, user documents, settings/recovery, host focus/close behavior, file integrity and directly affected features. Use current project abstractions. Do not suppress errors, weaken assertions, mark relevant tests ignored or claim unrun checks passed.

## 7. Bounded verification

Follow section H of the Goal:

- Prefer targeted Nextest selections and the cached development profile; verify filters select actual cases.
- Use one owned build/test job at a time. Native stateful checks run serially.
- Keep compilation separate from test execution limits. Aim for focused pure-test batches near or below 100 seconds; narrow/split a slow batch instead of inventing flags or changing global settings.
- Set test-app Paste Sequence `next_hotkey = None` and `cancel_hotkey = None` before construction, preserving production defaults.
- Verify parser/vector semantics, archive integrity/lifecycle, diagram persistence/input ownership, catalogue generations and relevant host stale-event behavior at their lowest useful layer.
- Use a consolidated supported-Windows session for the six native flow groups: Calculation/Inspector, ZIP, visual diagrams, Explorer Tabs, Recorder and command discovery. Recorder coverage includes a short start/stop from a cold-cache window selection, then Show region and a short selected-display-region recording.
- Capture screenshots or equivalent receipts when available, and inspect actual file/recording/tab outcomes where screenshots are insufficient.
- Use the initial native/environmental attempt plus at most two evidence-driven corrective attempts for the same cause. Do not repeat without a new hypothesis.
- Perform one independent final review and one appropriate final first-party Clippy pass. Fix new feature-related findings; report unrelated existing findings separately and exclude vendored cleanup.
- Do not automatically run the entire test suite, repeated release builds, historical acceptance campaigns, pixel-golden tests or speculative benchmarks.

Compilation and task-caused failures must be corrected. Bounded verification prevents redundant diagnostics, not implementation completion.

If a native environment or required capability is unavailable, preserve the exact evidence, complete independent work and report the affected checks as unverified. Do not replace them with a claim that source review proves runtime success. Do not mark the entire Goal COMPLETE while mandatory functional/native acceptance remains missing.

## 8. Checkpoint and stall records

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

Follow the Goal's 21 checkpoint packets, beginning with M0-A. Use descriptive `<type>(<scope>): [M#-X] <behavior>` commit subjects. Do not accumulate several substantial increments into one final commit or rewrite prior commits to conceal remediation.

Before every commit inspect branch, HEAD, staged paths and upstream. Stage only task-owned changes. Keep `parent_branch`/`parent_tip_sha` for ancestry, `baseline_sha` for cumulative diff review, and `final_tip_sha` for handoff. Attempt one ordinary best-effort push after each checkpoint; failed pushes are recorded and do not block safe local continuation.

Follow `AGENTS.md` for a genuine blocked/deferred/paused stop: create one concise factual tracked report under the appropriate `docs/plans/execution/.../stalls/` path when safe. Include the exact checkpoint, timestamp, branch/HEAD/upstream, expected/actual result, failed or unrun check, attempts, cleanup and preserved work, missing acceptance and smallest safe next action. Keep the existing operational ledger authoritative. If a reporting commit is unsafe, use the existing external ledger until it can be reconciled.

Do not generate additional planning documents, competing ledgers or repetitive stall reports for ordinary in-progress work.

## 9. Final review and truthful delivery

When implementation is ready:

1. Review the cumulative diff from `baseline_sha` and check every Goal requirement.
2. Confirm structural changes are connected to their real runtime/data paths: vector references/copy, archive streaming/publication, visual label/route persistence, shared window discovery and accepted host effects.
3. Resolve feature-related independent reviewer findings and rerun only affected checks.
4. Finish the required native flows, targeted final checks and lint review within the budget.
5. Commit intentional final help/evidence/report changes; inspect clean/dirty state and actual push outcome.
6. Verify parent ancestry and record the exact final tip after the last intentional commit. Avoid a commit loop trying to write a commit's own SHA into itself.

Report:

- COMPLETE, LIMITED or BLOCKED with exact reasons and missing requirement IDs.
- Completed milestones/checkpoints and important implementation decisions.
- Relevant files/components changed.
- Actual commands/tests, results and native evidence; distinguish unverified behavior and platform limitations.
- Current branch, `parent_branch`, `parent_tip_sha`, `baseline_sha`, checkpoint SHAs, `final_tip_sha`, actual push state and checkout state.
- Remaining issues, if any, and a concise final manual-review checklist.
- This explicit handoff: **the following Goal must create its fresh child branch directly from this Goal's `final_tip_sha`, not from `master`/`main`**.
- Confirmation that no Git worktrees, auxiliary checkouts/clones, agent branches or intermediate branch merges were used.

A failed push alone does not invalidate complete verified local work. Missing mandatory native/functional acceptance must remain visible in the final status.

## 10. Stop at the approved boundary

Stop after the requested behavior, focused validation, final review, checkpoint commits and final-tip handoff are complete. Do not begin another Goal, merge to the integration branch, redesign unrelated systems, or continue into optional enhancements.

If a genuine blocker prevents complete acceptance, leave all safe work preserved and committed where possible, provide the precise bounded blocker report, and stop without claiming completion.

