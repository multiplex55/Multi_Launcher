# Codex Kickstart — Studio and Launcher Usability

Execute `docs/plans/CODEX_GOAL_STUDIO_LAUNCHER_USABILITY.md` autonomously from beginning to end.

Read the repository's `AGENTS.md` and applicable local instructions, this Kickstart, and the **entire associated Goal** before making changes. The Goal is the feature-specific contract; repository instructions govern engineering and orchestration. Reconfirm current source locations after M0-A. Do not reconstruct requirements from conversation history.

## 1. Mandatory First Execution — Exact Parent Tip, New Child Branch, M0-A, Push

**Before touching application code, running implementation builds/tests, or starting M1, perform the complete bootstrap below in the single existing checkout.** The Goal and Kickstart describe **one shared task branch**, not separate branches.

1. **Inspect Git read-only.** Inspect `git status -sb`, current branch, `git rev-parse HEAD`, default/integration branch, remotes/upstream, staged/unstaged/untracked changes, and previous-Goal records in the active queue/orchestrator/execution ledger.
2. **Identify the required parent.** For a continuing sequence, use the exact `final_tip_sha` of the **immediately previous completed Goal/category branch**. Capture `parent_branch` and `parent_tip_sha`. Verify the preceding work is committed and the SHA agrees with its handoff. The current checkout, an old common base, or `master`/`main` is not a substitute.
3. **Handle remote/local state safely.** If the predecessor exists only remotely, fetch safely and verify its actual tip. If its push failed, the real local final tip remains authoritative. On another machine, block if that commit is absent rather than silently starting from stale remote history. Conflicting/incomplete ancestry records require a precise bounded blocker; do not guess.
4. **Use the primary branch only for a new chain.** Only the first Goal in a genuinely new chain defaults to the verified up-to-date primary integration tip, normally `master` or `main`. A safe current-HEAD starting base is permitted for that first task only when documented and no user work is lost or inadvertently committed. Never return to the primary branch between Goals.
5. **Protect user changes.** Never automatically reset, hard-pull, stash, rebase, merge, force-checkout, overwrite local work, or stage unrelated files to make switching possible. If reaching the required parent safely is blocked by local changes, preserve them and report the exact conflict. Do not choose a different base.
6. **Choose and create one fresh task branch.** Use a unique lowercase kebab-case name with an appropriate prefix; `feat/studio-launcher-usability` is the suggested stem. Execute the equivalent of `git switch -c <unique-task-branch> <parent_tip_sha>`. Verify its initial HEAD equals the required parent. Do not implement on the predecessor, `master`, or `main`.
7. **Create this Goal's M0-A baseline commit.** Use a small task ledger consistent with repository conventions, or `git commit --allow-empty -m "chore: [M0-A] establish studio launcher usability baseline"`. Stage only intended baseline material. Record `baseline_sha` separately from `parent_tip_sha`. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's final commit is not this Goal's baseline; M0-A must be its direct child.
8. **Attempt the initial upstream push.** Make one normal non-force attempt, e.g. `git push -u origin HEAD` when origin is the intended repository. If denied or unavailable, keep the local branch/commit, record the exact failure/unavailable state, and continue locally. Do not retry in loops or bypass security.
9. **Keep every implementation checkpoint on this branch.** This Goal has no internal category branch handoffs. Milestones, agents, and the companion document do not receive their own branches. Commit frequently at the Goal's boundaries; attempt normal best-effort pushes afterward. Do not rewrite prior checkpoints.
10. **Preserve the sequential chain at completion.** Record `final_tip_sha` and instruct the next Goal to create its fresh branch directly from that SHA. No intermediate merges, rebases, squashes, or cherry-picks are permitted merely to advance the sequence. One final integration PR/merge belongs at the approved end-of-chain boundary, not between checkpoints.

### No Git worktrees or alternate working directories

Use **one existing checkout** for all source work and Git operations. Do not run `git worktree add`, `git worktree move`, or `git worktree prune`. Do not create worktrees, auxiliary checkouts, secondary clones, helper clones, or agent-specific clones/branches. Do not parallelize implementation by copying the repository into another directory.

Read-only planning/review may run concurrently when useful. Only one source writer is active at a time. Branch changes, writes, staging, commits, and dependency-changing builds must be coordinated sequentially.

### Bootstrap completion gate

Before delegating M1, verify:

- Unique task branch is active and started at the exact required predecessor SHA.
- `parent_branch` and `parent_tip_sha` are recorded.
- A distinct `baseline_sha` exists and its immediate parent is `parent_tip_sha`.
- Initial push result is recorded; rejection has not erased local progress.
- No pre-existing unrelated change was staged or lost.
- No extra checkout, worktree, or independent implementation branch was created.

Only then begin code work.

## 2. Maintain One Concrete Execution Record

Use the active authoritative queue ledger if one exists. Otherwise create one compact execution ledger under `docs/plans/execution/standalone/studio-launcher-usability/`, following the Goal.

Track checkpoint states, important decisions, commands/results, evidence paths, scoped blockers, commit SHAs, and actual push state. Record the initial user-dirty paths so they cannot be mistaken for task changes later.

Capture a commit's SHA after committing and record it in the next ledger update or the external handoff/final report. Do not create an endless chain of bookkeeping commits to record a commit's own hash inside itself. Compute the final handoff tip after the last commit.

Historical goals, reports, and harness procedures are reference material unless explicitly adopted by this Goal. Do not revive earlier full-suite or native-acceptance campaigns.

## 3. Use the Configured Planner / Implementer / Reviewer Workflow

Re-read the actual role configuration under `.codex/`. The inspected named roles are:

| Role | Configuration observed | Responsibility |
| --- | --- | --- |
| Orchestrator | `gpt-6.1-sol` / medium in `.codex/config.toml` | Own state, dependencies, sequencing, Git, verification budgets, and final handoff. |
| Planner | `gpt-6.1-sol` / high in `.codex/agents/planner.toml` | Validate the bounded approach and issue explicit checkpoint packets. |
| Implementer | `gpt-6-luna` / max in `.codex/agents/implementer.toml` | Complete one assigned checkpoint at a time. |
| Reviewer | `gpt-6.1-sol` / high in `.codex/agents/reviewer.toml` | Independently review the actual cumulative diff and directly affected behavior. |

Use those named roles when the runtime exposes them. Current repository configuration takes precedence over this observation. Do not edit model settings, assume unnamed agents inherit the same configuration, or invent unavailable tools/roles. If delegation is unavailable, perform the same bounded responsibilities sequentially.

Every implementation handoff must include the Goal checkpoint's objective, current-state facts, architectural owner, exact scope, ordered changes, behavior, edge cases, invariants, non-goals, dependencies, focused verification, acceptance criteria, and commit boundary. Do not hand an implementer only “improve Diagram Studio” or the entire feature list.

Parallel read-only analysis is useful; competing source writes are prohibited. The orchestrator owns commits unless the repository explicitly delegates that responsibility. An implementer reports its changes and actual verification, then stops until assigned the next checkpoint.

## 4. Execute the Goal in Dependency Order

Follow the detailed packets in Goal section F:

| Checkpoint | Required result |
| --- | --- |
| M0-A | Correct new child branch, direct-child baseline, initial push attempt, recorded ancestry. |
| M1-A | Deliberate main-query focus beside ordinary tools through the shared ownership boundary. |
| M1-B | Real query editing/navigation/activation and dashboard access while tool drafts remain intact. |
| M2-A | Diagram preview visibility, discoverable zoom, and resizable source/inspector/render allocations. |
| M2-B | Bounded offline Mermaid adapter proven on Windows with mindmap, sequence, and flowchart. |
| M2-C | Ten additional rendered template families, truthful editability/status, exact source save/export. |
| M2-D | Separate synchronized Diagram viewer with independent camera and safe native lifecycle. |
| M3-A | Calculator mixed-base literals, remainder/modulo, and tested examples/reference. |
| M3-B | Calculation Pad popout/dock using one session and the existing evaluation cache. |
| M3-C | Optional default-off last-source recovery through safe settings/persistence owners. |
| M4-A | Note-owned custom Markdown table builder with named columns and one prepared undoable edit. |
| M4-B | Save Changes on note close, explicit successful-save authority, and failure-preserves-draft behavior. |
| M5-A | Dashboard collapse persistence separate from safety tokens, plus correct save toast. |
| M6-A | Focused integration, native smoke, final lint/review, committed delivery, exact-tip handoff. |

Complete checkpoint-specific verification before accepting a checkpoint. Small remediation commits may follow; do not rewrite prior checkpoints.

If M2-B encounters a genuine renderer blocker, preserve the evidence and continue independent M3/M4/M5 work when safe. Do not pretend M2-C/M2-D generic rendering is complete or substitute nonrendering template text. A later dependency failure must not stall every unrelated improvement.

## 5. Enforce These Important Implementation Boundaries

The Goal contains the full specification. These are the decisions most likely to be lost during delegation:

- **Query availability is not blanket keyboard permission.** Ordinary windows may stay open while the query receives deliberate focus. Actual focused controls own keys; active capture/destructive/modal operations remain protected. Fix the shared owner, not only the named tools.
- **Diagram Studio already has view modes, zoom, and its own native window.** Extend/reveal them. The new popout is a second viewer sharing the existing document/render data.
- **New Mermaid families need actual rendering.** The current custom parser and template-creation validation only accept a flowchart subset. Make template creation capability-aware, preserve the visual editor, and keep generic source sessions exact. Separate displayed-preview/export/banner status from native graph editability. Revalidate same-frame source edits before image export and freeze the matching result before picker interaction.
- **Keep the renderer small and offline.** The Goal identifies `mermaid-rs-renderer` 0.3.1 as the initial pinned library candidate plus a minimum in-memory SVG rasterizer. Qualify the actual Windows integration and required template corpus. Do not claim full Mermaid.js parity, add an automatic browser backend, or pretend a timeout kills an in-process thread.
- **Calculation Pad already caches and supports sqrt.** Preserve and demonstrate its revision cache. Add missing syntax and useful documentation rather than replace the engine.
- **Mixed-base arithmetic keeps existing scalar semantics.** Diagnose integer-literal precision loss; preserve the separate exact `u128` conversion path. Numeric prefixes must not consume identifier budgets. Preserve existing bare `mod` variables when adding call syntax and verify mixed `%`/`*`/`/` grouping against the Goal. Arbitrary-precision arithmetic is outside scope.
- **Native hosts must preserve accepted-frame effects.** Carry host identity and accepted root/child pass provenance. One authoritative session; no source loss from same-frame edit plus Save/Close or duplicate effects after discarded passes. Resolve Calculation Pad Copy from accepted current results, not a stale draw-time string.
- **Remember-source is optional and default-off.** Recovery is separate from named documents, lazy, bounded, atomic, and serialized with Discard/disable clearing. It must not resurrect discarded content or make an unsaved draft falsely clean.
- **Table insertion belongs to the originating note.** Capture identity/source/caret, validate dimensions, and apply one prepared edit. Query focus or another opened note must not retarget it.
- **Save Changes closes only on positive save success.** Handle write failure and overwrite/Save as New decisions explicitly; add conflict cancellation and preserve original note identity on failed Save as New. Repair the related auto-save-on-close failure path while preserving explicitly configured close-without-saving behavior when both existing options are disabled.
- **Dashboard folds are presentation state.** Keep target/token renewal for stale-action protection; preserve folds through separate identity/preferences. Stage touched Save and new toast/preference effects at accepted root completion; `first_ui_pass` alone is insufficient. The layout sketch is not a substitute for the real main dashboard.
- **Respect scope.** No workspace features, broad parser/search redesign, unrelated cleanup, vendor edits, or speculative enhancement.

## 6. Operate Autonomously and Keep Work Reviewable

You are authorized to inspect, implement the Goal, make routine engineering decisions, run targeted checks, build/launch/interact with the app when supported, capture appropriate evidence, fix relevant regressions, commit, and attempt normal pushes.

Do not stop after each milestone to ask whether to continue. Do not ask the user to run commands, perform intermediate tests, or provide screenshots. The user performs one final review after your execution.

Resolve ordinary Rust ownership, module placement, helper naming, UI sizing, error wording, dependency feature selection, and test organization from the actual code and Goal. Ask for intervention only for a genuinely non-inferable consequential decision, an unavailable required external resource, or a destructive action that lacks authorization.

A failed push is not such a blocker; keep the successful local commit and continue. A compiler error is normal implementation feedback; fix the first meaningful cause and rerun the narrow affected command.

Progress updates should explain what is complete, what was learned, and the next uncertainty being resolved. Do not substitute plans or promises for implementation.

## 7. Verify Proportionately and Honestly

Use Goal section H's focused checks and section I's budget. Select exact targets/tests; do not automatically run the full repository suite or repeated release builds.

- Keep one cached build profile, initially `fast-dev` unless current repository conventions require another.
- Use targeted `cargo nextest` runs with `--no-fail-fast`, selected package/target, and confirmed nonzero test matches.
- Before constructing any new app/test profile, disable Paste Sequences next/cancel shortcuts in that fixture as AGENTS requires. Preserve production defaults and real error reporting.
- Treat source/evaluation/render counters as evidence for unchanged-input caching; do not infer cache correctness solely from perceived speed.
- Reuse existing retained-frame and immediate-viewport seams to test same-frame edits, discarded passes, host migration, and single delivery.
- Use meaningful table/math/template example corpora and actual save-failure tests. File existence, labels, or helper-only tests cannot substitute for behavior through the production path.
- Build/launch the Windows app and carry out the Goal's bounded native smoke session when possible, including screenshots of the actual affected UI.
- Headless lifecycle tests do not prove native focus, OS X behavior, or multi-monitor interactions. If unavailable, record the exact unverified checks and environment limitation; do not ask the user to fill the gap mid-plan.
- Perform one final first-party Clippy cleanup/check, excluding dependencies and unrelated vendored cleanup. Report preexisting unrelated failures separately.
- Perform one independent cumulative review, fix substantive findings, and rerun only affected verification.

Do not weaken assertions, delete meaningful tests, suppress errors globally, or change acceptance criteria to manufacture success. A justified behavioral change may require updating an obsolete blanket-dialog assertion while retaining its underlying no-background-activation protection.

Budget each concrete failing behavior to an initial scoped pass and at most two focused remediation/revalidation rounds. Keep native interaction to the stated smoke flows, normally 30–45 minutes excluding compilation. Broaden only when evidence identifies a specific uncovered regression risk; record the reason.

## 8. Commit and Push Each Accepted Checkpoint

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

The Goal already defines the boundaries and suggested subjects. After each coherent verified checkpoint:

1. Confirm task branch and current HEAD.
2. Inspect working/staged diff and intended path list.
3. Stage only task-owned changes.
4. Commit the checkpoint with a descriptive labeled subject.
5. Record the SHA and verification result.
6. Attempt one normal task-branch push.
7. If push fails, retain the commit, record failure and actual visibility, and proceed.

No per-checkpoint branches, no agent branches, no intermediate merging, no history rewriting, and no worktrees. Do not accumulate the whole feature into one final commit.

## 9. Handle Real Blockers Without Stalling Unrelated Progress

Separate:

- A feature defect blocking a required behavior.
- A missing environment capability preventing a particular verification.
- An unrelated preexisting failure.
- An optional improvement outside scope.

Use the Goal's bounded renderer fallback rule and verification budget. Preserve working increments and continue independent accepted work. Do not enter open-ended library comparisons, full regression campaigns, or repeated native harness cycles.

When a substantial actual stop is unavoidable, follow AGENTS' tracked stalled-goal report convention under `docs/`. Record the affected checkpoint, truthful status, branch/HEAD and parentage, exact failed/unrun command or interaction, expected/actual behavior, attempted corrections, remaining uncertainty, preserved partial changes, and smallest next action. Use the existing authoritative ledger; do not create competing progress systems.

A documentation-only blocker commit must not be presented as successful implementation acceptance. Commit/report only when safe, and make the normal best-effort push attempt.

## 10. Final Review, Report, and Exact-Tip Handoff

After implementation:

1. Check every Goal requirement and checkpoint acceptance condition.
2. Review the complete `baseline_sha..HEAD` diff for missed paths, duplicated owners, stale compatibility code, unrelated files, and accidental churn.
3. Resolve feature-related findings and complete the required focused verification.
4. Confirm actual native evidence and identify anything unverified.
5. Commit all intended task work or explicitly document unresolved changes. Inspect final branch/upstream and working-tree state.
6. Compute `final_tip_sha` **after the last commit** and verify the chain descends from the required parent.
7. Record the exact tip in the final report and authoritative handoff. Do not silently carry uncommitted partial work into the next category.

The final response must include:

- Completed milestones/checkpoints and concrete resulting behavior.
- Important implementation decisions and files changed.
- Actual test/build/lint commands and outcomes.
- Native interactions and evidence paths.
- Remaining feature blockers, limitations, and unverified checks.
- `parent_branch`, `parent_tip_sha`, current task branch, `baseline_sha`, significant checkpoint SHAs, `final_tip_sha`, actual push state, and checkout state.
- This explicit instruction: **“The next Goal must create its fresh branch directly from <final_tip_sha>, not from master/main.”**
- Confirmation that no worktrees, auxiliary checkouts/clones, independent agent branches, or intermediate branch merges were used.
- A brief final user review checklist.

Use **Complete** only when the Goal's Definition of Done is satisfied. Otherwise give a precise Partial/Blocked or implementation-versus-native-verification status.

Stop when this approved Goal and required validation are complete. Do not proceed into extra features, repository-wide cleanup, an unapproved merge, or another category.
