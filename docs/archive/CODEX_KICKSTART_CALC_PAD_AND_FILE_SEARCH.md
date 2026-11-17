# Codex Kickstart — Calculation Pad Expansion and File Search Metadata Filters

Execute **`CODEX_GOAL_CALC_PAD_AND_FILE_SEARCH.md`** from start to finish. Implement its thirteen core features, preserve existing behavior, verify the affected paths, commit coherent checkpoints, and deliver a factual final report. This is an implementation instruction, not a request to produce another high-level plan or questionnaire.

## 1. Read instructions, then perform M0-A before all application work

Read the repository's `AGENTS.md`, applicable local instructions, actual `.codex` agent configuration, and the **entire Goal** before changing anything. The Goal is the feature-specific implementation contract; repository instructions supply engineering/orchestration rules. Historical goals, test matrices, model choices, and acceptance campaigns are not active requirements unless this Goal explicitly adopts them.

The first execution checkpoint is **M0-A**. Before application edits, implementation builds/tests, or M1, perform the following bootstrap in the **single existing repository checkout**:

1. **Inspect read-only:** `git status -sb`, current branch, `HEAD`, remotes/upstreams, default integration branch, staged/unstaged/untracked changes, and authoritative previous-Goal/queue records. Protect all preexisting user work. Merely being on an existing task branch does not satisfy this new Goal's fresh-branch requirement.
2. **Identify the required exact parent:** for a continuing sequence, use the **final committed tip of the immediately preceding completed Goal/category branch**, even when the current checkout is on `master`/`main`. Record `parent_branch` and `parent_tip_sha`; verify the commit and prior completion records. Fetch safely if the required ancestor exists only remotely. A preceding push failure does not invalidate the actual committed local tip. On another machine, block until that commit is available instead of selecting a stale remote base.
3. **First-chain exception only:** only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch, normally `master`/`main`, unless the user explicitly supplies another base. A safe first-task current-HEAD exception must be explicitly documented and preserve unrelated work. Never return to the original integration branch between Goals. If records conflict or the parent is unknown, report a precise bounded ancestry blocker; do not guess.
4. **Choose one fresh, unique child branch:** use an idiomatic name such as `feat/calc-pad-and-file-search`, adding a suffix if necessary. **This Kickstart and its Goal share ONE task branch.** All thirteen features and all checkpoints stay on it; this Goal has no internal category-branch handoffs. Agents, milestones, and documents do not get separate branches.
5. **Switch safely from the exact parent SHA:** after safety checks, use the equivalent of `git switch -c <new-task-branch> <parent_tip_sha>`. Verify the new branch initially equals the required SHA. Never automatically reset, stash, hard-pull, merge, rebase, cherry-pick, force-checkout, or overwrite user work to make the transition succeed. If switching safely is impossible, preserve the checkout and report the blocker instead of choosing a different parent.
6. **Create this Goal's M0-A baseline commit:** use the minimal task execution ledger or an intentional empty commit such as `chore: [M0-A] establish calc-pad-and-file-search branch baseline`. Record `baseline_sha` separately. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. Do not call the predecessor's final commit the new baseline or stage unrelated/preexisting files.
7. **Attempt the initial normal upstream push:** ordinarily `git push -u origin HEAD`, only to the intended repository/branch. Never force-push. If denied, unavailable, or rejected, record the exact state and continue safe local implementation. No repeated retries, permissions bypass, branch changes, or history rewrites to repair a push.
8. **Prohibit extra working copies:** no Git worktrees, `git worktree add/move/prune`, auxiliary checkouts, secondary/helper clones, or per-agent feature branches. Do not repurpose or modify unrelated existing worktrees. All editing, branch transitions, builds, and commits use the current checkout sequentially.
9. **Record the handoff contract:** track `parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, checkpoint SHAs, verification, and actual push state. At completion, record the actual final `HEAD` as `final_tip_sha`. The following Goal must branch **directly from this exact final tip**, not from `master`/`main` or an older ancestor. Do not merge/rebase/squash/cherry-pick intermediate Goal branches merely to advance the chain. Final integration is a separate approved boundary, not part of this instruction.

**M0-A is a hard gate.** Confirm the unique child branch, exact parent, direct-child baseline, push attempt/concrete unavailability, protected user changes, and absence of extra working copies before implementation. Push failure itself is not a blocker to safe local work.

## 2. Validate the current source and use the configured agents

After M0-A, execute M0-B against the actual checked-out source. Confirm the Goal's named owners, relevant callers/tests, and existing capabilities. Adapt moved paths and ordinary Rust details without changing the approved behavior. Do not require conversation history or reconstruct the task from past discussions; the Goal is self-contained.

Use the repository's configured planner, implementer, and reviewer roles when available:

- **Planner:** validate architecture/dependencies and prepare a bounded packet for the next checkpoint, using its objective, ownership, tasks, behavior, edge cases, regression constraints, verification, acceptance, and commit boundary. Do not replace the Goal with a speculative redesign.
- **Implementer:** make the actual source changes for one bounded packet at a time. Do not silently have the orchestrator bypass an available implementation-agent role.
- **Reviewer:** independently inspect the completed task diff and relevant surroundings read-only; prioritize correctness, compatibility, lifecycle, missing requirements, and test validity.
- **Orchestrator:** coordinate state, evidence, verification, checkpoint commits, normal pushes, bounded remediation, and final reporting.

Use the actual configured model/settings; do not change agent models based on historical instructions. Do not invent unavailable tools or agents. When a role cannot be invoked, explicitly note the limitation and perform the necessary work through available capabilities rather than pretending a separate review occurred.

**One writer and one build/test owner at a time.** Read-only investigation/review can be parallel when safe, but no competing source edits, branch changes, commits, or builds. All agents use this one checkout.

## 3. Execute the Goal's complete dependency order

Follow the detailed checkpoint contracts in Goal Section F:

| Stage | Outcome |
|---|---|
| M0-A / M0-B | Correct chained branch/baseline, live-source ownership, and scoped verification baseline. |
| M1-A / M1-B | C20: shared logical/physical source mapping, comments, multiline expressions, editor integration. |
| M2-A / M2-B | Versioned options/recovery/run identity and C24 Manual calculation. |
| M3-A / M3-B | C25: bounded exact decimal/fraction arithmetic, display, canonical copying. |
| M4-A | C21: all ten numeric helpers with real signatures and executable examples. |
| M5-A / M5-B / M5-C | C18 functions, editor support, and C27 blocks/break/continue. |
| M6-A / M6-B | C22: typed deterministic date/time operations and presentation. |
| M7-A / M7-B | C26: complete Extended-source lifecycle, usage UI, bounded capacity check. |
| M8-A / M8-B | C14: binding-aware atomic rename and preview/editor interaction. |
| M9-A / M9-B | C6: typed input definitions, validation, and source-backed controls. |
| M10-A / M10-B | C4: non-destructive sensitivity execution, panel, persistence, exports. |
| M11-A / M11-B | C5: line/bar/scatter models, image export, dockable Charts companion. |
| M12-A / M12-B / M12-C | F3: typed predicates, backend filtering before caps, filter UI/preferences. |
| M13-A / M13-B | Linked documentation/examples, integration acceptance, independent review, final handoff. |

Do not stop after the foundational milestones or after the first working feature. Complete the entire approved scope unless a genuine bounded blocker prevents it. Do not ask whether to continue after each checkpoint.

The Goal already resolves the intended defaults and optional exclusions. Do not add two-variable sensitivity grids, recursion, statement-bodied functions, live clock functions, date pickers, creation-date search, workspace features, framework migration, or unrelated optimization. Conversely, do not silently remove one of the thirteen required core features because it is harder than expected.

## 4. Preserve the central correctness boundaries

Read the full feature contracts rather than implementing from this summary. These are high-risk integration checks, not substitutes for the Goal:

- **Source and types:** source remains authoritative; existing valid documents, physical `rN` references, fixed-width integers, unit arithmetic, and floating-point semantics are preserved. New exact numbers require explicit constructors and bounded typed arithmetic. Do not add unguarded string splitting for comments or use text replace-all for rename.
- **State and freshness:** Manual mode must stop implicit evaluation, not merely hide automatic results. Current/stale/run identities apply to every observer and asynchronous completion. A rename updates related metadata in the same one-undo transaction; retained stale source ranges cannot edit or navigate current text.
- **Persistence and capacity:** update all relevant session/store/recovery/worker/editor limits and version readers together. Old v1/v2 documents and legacy recovery remain readable; unknown future versions are not rewritten. Never truncate source or make deserialization/allocation unbounded.
- **Analysis and hosts:** sensitivity evaluation uses immutable overrides, not repeated live edits. Charts read committed data, disclose staleness, and follow the existing utility-host/first-pass action pattern. Main launcher focus remains usable while companions are popped out. Image copying renders the chart only.
- **File Search:** resolve relative date windows once at submission and compare semantic UI criteria for dirty state. Filter before accepted-result limits. The Goal explicitly uses WalkDir for metadata-filtered filename searches that would otherwise use Everything; disclose the backend tradeoff. Keep the content-scan size cap separate, and never treat unknown metadata as zero.

Reuse actual domain/session/host owners. Proposed module names in the Goal are proposals, not claims that those files already exist. Do not introduce parallel sources of truth, duplicate evaluators, unnecessary frameworks, or broad dependency upgrades.

## 5. Verify autonomously within the stated budget

Run the narrowest relevant tests/build checks for each checkpoint, inspect its diff, and record actual results. The Goal's Section H defines the affected automated families, native scenarios, and limits.

Prefer targeted Nextest library/module filters and the real `domain` target for File Search cases. Discover actual test names and reject zero-match filters as verification; path-included editor tests live beneath `calculation_pad_dialog::editor`. Serialize test/build execution, use conservative build jobs where necessary, and isolate a failed test when interference is plausible. For constructed app/test profiles, disable Paste Sequence `next_hotkey` and `cancel_hotkey` before construction, as `AGENTS.md` requires; never change production defaults to quiet tests.

Keep cached focused test batches within the Goal's 100-second execution deadline, with separately bounded compilation. Use at most the initial diagnosis and two focused corrective/retest cycles for the same failure, unless new evidence justifies another explicitly bounded check. Do not repeatedly run the whole suite, rebuild release, or expand into unrelated test/lint repair.

Build and launch the app when supported; perform the actual native scenarios for input controls, rename, sensitivity/charts, dock/pop-out/focus, Extended persistence, and File Search filters. Capture appropriate evidence. Do not require the user to run commands, provide screenshots, or perform intermediate verification. Their manual review comes at the end.

Headless tests do not prove Windows focus/clipboard/window behavior. If native execution is unavailable, make at most two materially different bounded environment attempts, record exactly what is unverified, and continue safe independent verification. Never label native acceptance passed from inferred behavior or a noninteractive screenshot.

Do not delete/ignore/weaken meaningful tests, suppress feature-related failures, or lower acceptance requirements to obtain a green result. Preexisting unrelated failures and vendored warnings are recorded separately; they are not authorization for broad cleanup.

## 6. Commit coherent checkpoints and maintain truthful progress

Use the project's active checkpoint cadence and the Goal's task-specific boundaries. M0-A is the first baseline; all later checkpoints remain on the **same new task branch**. Before a dependent checkpoint, commit the preceding coherent verified increment. Use descriptive stage-tagged subjects such as `feat(calc-pad): [M10-A] add non-destructive sensitivity evaluation`.

Inspect status, full intended diff, and staged paths before every commit. Never stage unrelated/preexisting user changes. After each commit attempt one normal non-force push when supported. Keep local commits and continue after push failure; record actual visibility instead of implying GitHub received them.

Maintain the existing authoritative execution ledger, or the Goal's standalone status file when no queue exists. Track every requirement as pending/in progress/implemented/verified/blocked, distinguishing code completion from evidence. Do not create competing progress records.

At a genuine bounded stop, follow `AGENTS.md`'s tracked stalled-goal reporting convention when safe. State the exact failing/unrun check, expected/actual behavior, attempted remedies, budget, current ancestry/dirty state, preserved partial work, and smallest safe next action. Do not carry uncommitted partial work silently into another Goal or create a side branch/worktree to sidestep it.

## 7. Final review, status, and exact-tip handoff

Once the implementation stages are complete, run the configured independent reviewer over the task diff, resolve substantive feature-related findings through the implementer, rerun affected checks, and complete the scoped first-party Clippy/final verification required by the Goal. Do not rewrite history or expand into optional additions.

Report these dimensions separately: implementation for all thirteen IDs; automated verification; native verification; and Git delivery. A required unimplemented feature is partial/blocked. An implemented feature with unavailable Windows evidence is native-acceptance-limited, not fully verified. Push failure alone means local-only delivery, not invented implementation failure.

Your final response must include the Goal's outcome/evidence matrix, completed checkpoints, relevant changed paths, substantive decisions, actual checks/results, native evidence and unverified scenarios, remaining limitations, and a short final manual review checklist.

Also include **`parent_branch`, `parent_tip_sha`, current task branch, `baseline_sha`, significant checkpoint SHAs, actual `final_tip_sha`, actual push/upstream state, and clean/dirty checkout status**. Finish any tracked reporting commit before reading final `HEAD`; do not create a self-referential sequence of commits trying to embed a commit's own hash. Put the true final tip in the final response/authoritative external handoff.

Confirm that no Git worktrees, auxiliary checkouts/clones, per-agent implementation branches, or intermediate branch merges/rebases/cherry-picks were used for this workflow. End the handoff with:

> The next Goal must create its fresh child branch directly from `<final_tip_sha>`, not from master/main or an earlier common base. This tip contains the cumulative chain. Final integration remains a separate approved boundary.

Stop after the approved work and truthful handoff, or at a documented bounded blocker. Do not merge into `master`/`main`, start another Goal, or request an unnecessary approval round.
