# Codex Kickstart — Calculation Pad Dynamic Vectors and Analysis Dialogs

Execute **`CODEX_GOAL_CALC_PAD_DYNAMIC_VECTORS.md`** from beginning to end. Deliver its dynamic-vector language feature, chart integration and three UI changes; verify the scoped behavior, commit coherent checkpoints and report actual results.

This is an implementation instruction. Do not replace it with another high-level plan, questionnaire or permission loop.

## 1. Read the instructions, then perform M0-A before application work

Read the current repository's `AGENTS.md`, applicable local instructions, actual `.codex` agent configuration and the **entire associated Goal**. The Goal is the feature-specific contract; repository instructions define engineering and orchestration. Historical goals/model choices/native campaigns are not automatically active requirements.

**The first execution checkpoint is M0-A: exact predecessor final tip → fresh child task branch → its own baseline commit → initial normal upstream push → recorded and verified ancestry.** Do this before changing application code, running implementation builds/tests or starting M1.

In the **single existing checkout**:

1. Inspect `git status -sb`, current branch, `HEAD`, remote/upstream/default integration branch, dirty/staged/untracked files and authoritative prior-Goal/queue records. Protect preexisting user work.
2. For every continuing sequence, resolve the **immediately preceding completed/delivered Goal/category branch's exact final commit**. Record `parent_branch` and `parent_tip_sha`. Never select `master`/`main` merely because it is checked out, or use the original common ancestor.
3. The observed predecessor is `feat/calc-pad-and-file-search` at `df62fd792a7a20d5a1c55fb542eddc73dd37bad1`. Its reporting commit follows the implementation checkpoint `17957d3679c814d4caaf679c80827ee290632294` and is part of the delivered tip. Verify that no later completed category/handoff supersedes it. If a later handoff exists, resolve its exact final tip from authoritative records. If ancestry records conflict, report a bounded blocker rather than guess.
4. **Only the first Goal in a genuinely new chain may default to the up-to-date primary integration branch**, unless the user explicitly gives another starting base. This Goal has an identified predecessor. Fetch safely when necessary; a valid local-only predecessor is still the required ancestor. On a different machine, do not substitute stale remote state when the required commit is missing.
5. Choose a **fresh, unique branch**, such as `feat/calc-pad-dynamic-vectors` with a suffix if necessary. **This Goal and Kickstart share ONE task branch.** Agents and checkpoints do not receive separate branches. An existing active task branch does not waive the fresh-child requirement.
6. Switch directly at the verified parent SHA using the equivalent of `git switch -c <new-task-branch> <parent_tip_sha>`. Never reset, stash, hard-pull, rebase, merge, cherry-pick, force-checkout, overwrite user work or stage unrelated files to force a transition. If switching safely is impossible, preserve the checkout and report the precise blocker.
7. Create this Goal's **own M0-A baseline commit**, preferably the minimal task ledger or an intentional empty commit: `chore(calc-pad): [M0-A] establish dynamic vectors branch baseline`. Record `baseline_sha` separately. Verify `git rev-parse <baseline_sha>^` equals `parent_tip_sha`. The predecessor's final commit is not this Goal's baseline.
8. Attempt one normal non-force upstream push, normally `git push -u origin HEAD`. If denied/unavailable/rejected, record it, keep local commits and continue safe local work. No repeated retries, bypasses, history changes or branch changes to repair a push.
9. **Never use Git worktrees, `git worktree add/move/prune`, auxiliary checkouts, secondary/helper clones or agent-specific implementation branches.** Do not repurpose unrelated existing worktrees. All source changes, branch transitions, builds/tests and Git operations use this checkout sequentially.
10. Keep later checkpoint commits on this same branch. At completion, finish reporting commits and read actual final HEAD as `final_tip_sha`. The next Goal must create its child branch **directly from that SHA**. No intermediate merging, rebasing, squashing or cherry-picking merely to advance the chain.

**M0-A is a hard gate.** Verify branch uniqueness/activation, exact parent, direct-child baseline, attempted push or concrete unavailability, protected user work and absence of extra checkouts. Push failure itself does not block safe implementation.

## 2. Confirm the live source and use configured roles

After M0-A, perform the Goal's M0-B read-only confirmation. Verify actual source owners, directly related tests, current format versions and host/reference seams. Adapt moved paths and ordinary Rust details without changing approved behavior.

Use the repository's actual configured capabilities:

- **Planner:** validate dependencies/ownership and hand the next checkpoint to the implementer as a bounded packet with objective, files, tasks, invariants, edge cases, verification and acceptance.
- **Implementer:** make the actual source changes for one checkpoint at a time. Use the configured implementation agent when available; do not silently bypass it.
- **Reviewer:** independently examine the completed cumulative task diff and the relevant surrounding architecture, read-only.
- **Orchestrator:** own task state, scope, serialized verification, Git commits/pushes, bounded remediation and final reporting.

Read the actual model settings rather than imposing historical preferences. At planning time the role files configured planner/reviewer as Sol high and implementer as Terra medium; verify live configuration and preserve it. Do not invent unavailable roles/tools or claim a separate review if none was possible; report the limitation and perform the necessary work through available capabilities.

**One source writer and one build/test owner at a time.** Read-only investigation or review may run in parallel when it improves the work. All roles use the same checkout and branch.

## 3. Execute the complete checkpoint sequence

The Goal's detailed F contracts define scope and evidence:

| Checkpoint | Required result |
| --- | --- |
| M0-A | Correct chained child branch, own baseline, attempted push, recorded identities. |
| M0-B | Live ownership/test selection and bounded admission/work approach confirmed. |
| M1-A | Empty and 1,024-element vectors; safe literal/expression foundation. |
| M1-B | Twelve pure collection functions; shared bounded work/cancellation and required workloads. |
| M2-A | Atomic collection binding exception, loop transaction compatibility, reference/session integration. |
| M2-B | Shared editor discovery, compact results and complete canonical Copy. |
| M3-A | 1,024-point chart domain, empty/one-point behavior and zero-based automatic X. |
| M3-B | Final named-vector selection, compatible physical references, persistence/rename/freshness. |
| M4-A | Separate Sensitivity Tables hosted dialog with retained actions and child wakeup. |
| M4-B | Persisted default-hidden Document usage; adjacent Charts presentation/Close controls. |
| M5-A | Executable in-app examples, linked reference and concise README update. |
| M5-B | Scoped integration/native evidence, independent review, final lint and exact-tip handoff. |

Do not stop after the first working append function or the first few checkpoints. Complete all V1–V6, C1–C2 and U1–U3 requirements unless a genuine bounded blocker prevents it. Do not ask whether to continue after each milestone.

## 4. Preserve these critical implementation boundaries

The following are reminders, not substitutes for the complete Goal:

- **The cap change spans more than one constant.** Empty-vector constructor/parser checks, the 2,048-byte ordinary expression ceiling, private vector work, shared document work, chart capacity and bar-label admission all need consistent handling. Preserve document source/row/storage, range/loop/function and sensitivity sample limits.
- **One collection implementation.** Add the twelve functions to the shared script catalog/typed dispatch and vector domain helpers. Keep `+` element-wise, vectors distinct from sequences, and current numeric type/provenance checks. No separate parser, GUI function registry, mutable reference system or `push` alias.
- **Immutable values with bounded actual work.** Previous physical rows, aliases and worker snapshots cannot change. Account for vector traversals/copies and existing publication paths. Shared storage alone does not prove cheap append. Measure the two-vector 1,024-iteration script and keep cancellation plus sensitivity aggregate budgets connected.
- **Failure publication is precise.** Outside an executing loop, failed evaluated collection updates retain the immediate prior valid binding in the current evaluation; the failed row/its physical reference still fails. Do not revive older source revisions or skip over an already-failed binding. Preserve scalar failed-assignment behavior and loop restore-plus-poison semantics. Propagate collection failure structurally through nested calls and formula wrappers.
- **Full Copy is separate from preview.** Large vector rows may show a preview/count; canonical Copy must contain the complete parseable value. Keep scalar display policy unchanged.
- **Named chart sources follow final bindings.** Preserve existing physical `LineId` selections and saved documents. A variable's producing row can change with zero iterations or branches. Update reference migration, rename/Undo, caches and current-only copy together; do not overwrite historical rows.
- **Charts use 0…N−1 automatic X.** This is an intentional change from 1…N, with matching text/docs/tests. Explicit X and custom labels remain intact. Empty vectors show No data; one-point lines show a marker; mismatches fail clearly. Sensitivity remains limited to 1,000 samples.
- **Sensitivity remains session-owned.** The new companion presents the existing definition, worker and results. Use retained parent-owner receipts and current-basis checks for edits, Generate, Cancel and exports. Opening does no evaluation. Closing retains definition/table; parent close retires the child. Extend existing wake/poll behavior for a visible popped-out child; no timer polling or duplicate worker.
- **UI preferences remain presentation-only.** Usage visibility defaults hidden and persists through display settings without source edits/Undo/evaluation. Charts presentation/Close share a horizontal row and retain owner/pin/host-action semantics.

Keep changes within the Goal. Do not absorb File Search, new chart types, two-input sensitivity, nested/typed lists, framework migration, workspace features, general optimization or unrelated cleanup.

## 5. Verify autonomously within the Goal's bounds

Follow Goal H, using actual test names and narrow Nextest library/module filters. Reject zero-match filters as verification. Extend meaningful existing tests; migrate assertions only for intentional behavior changes. Do not delete/ignore/weaken coverage to obtain passing results.

Serialize builds/tests and use conservative build jobs where needed. Cached focused test batches have a 100-second execution deadline; initial compilation is separately bounded to 900 seconds using available process ownership. Reuse compilation/native binaries where appropriate. Do not repeatedly run release builds or the full repository suite.

Perform one controlled vector/chart workload measurement pass and, only for a concrete issue, one focused optimization/retest pass. For the same failure, use initial diagnosis plus at most two focused correction/retest cycles before documenting a precise blocker. Ordinary compiler feedback should be fixed autonomously; a new bounded attempt needs new concrete evidence.

Build/launch the app when the environment supports it. Execute the Goal's compact native flows for generated charts, Manual freshness, Sensitivity Tables, usage visibility, dock/pop-out/close and launcher query focus. Capture relevant evidence. Isolated app/test profiles must disable Paste Sequence `next_hotkey` and `cancel_hotkey` before construction while leaving production defaults unchanged.

If native interaction is unavailable, make at most two materially different bounded environment attempts, state exactly what remains unverified and continue safe independent checks. Never label native acceptance passed from source reasoning or a headless image. The user performs their own review at the end; do not make them run intermediate commands, supply screenshots or unblock routine verification.

Run one independent final review and an appropriate final scoped first-party Clippy pass. Resolve task-introduced issues; record unrelated preexisting findings and avoid vendored cleanup. Broaden testing only for a concrete remaining risk, not because more tests exist.

## 6. Commit checkpoints and maintain truthful state

**Use the project's active checkpoint commit cadence and the Goal's task-specific commit boundaries.** M0-A is the first baseline. Every later checkpoint remains on the same fresh task branch; no per-checkpoint/per-agent branches or intermediate merges.

Before each commit, inspect status, actual diff and staged paths. Commit only coherent task changes, preserving preexisting work. Use descriptive stage-tagged subjects. Attempt one ordinary non-force push per checkpoint; retain local commits and continue if pushing fails.

Use the authoritative execution ledger, or `docs/plans/execution/standalone/calc-pad-dynamic-vectors/STATUS.md` if no queue ledger exists. Track requirement/checkpoint status, real evidence, Git identities and push state. Do not mark a checkpoint complete merely because source was edited.

At a genuine blocked/exhausted stop, follow `AGENTS.md`'s tracked stalled-Goal convention when safe. Record failed/unrun checks, expected/actual behavior, attempts and remaining budget, ancestry/dirty state, preserved partial work and smallest next decision. Do not escape a blocker through an unauthorized worktree, side branch, different ancestor or weakened requirement.

## 7. Final report and correct stopping boundary

Report implementation, automated verification, native verification and Git delivery separately. Missing required code is partial/blocked; implemented code with unavailable Windows evidence is native-acceptance-limited; rejected pushes mean local-only Git delivery.

Include:

- V/C/U outcome-and-evidence matrix and concise user-visible changes.
- Completed checkpoints, relevant paths and important decisions, including actual expression/work limits and chart-reference migration.
- Actual commands/results, bounded performance evidence, native outcomes/screenshots and explicit unverified scenarios.
- Remaining limitations and a short final manual-review checklist.
- **`parent_branch`, `parent_tip_sha`, task branch, `baseline_sha`, significant checkpoint SHAs, actual `final_tip_sha`, upstream/push state and clean/dirty checkout status.**
- Confirmation that no worktrees, auxiliary checkouts/clones, per-agent implementation branches or intermediate merges/rebases/cherry-picks were used.

Finish all reporting commits **before** reading final HEAD. Do not chase a self-referential commit hash in tracked files; publish the true final tip in the authoritative external handoff/final response.

End with:

> The next Goal must create its fresh child branch directly from `<final_tip_sha>`, not from master/main or an earlier common base. This tip contains the cumulative chain. Final integration remains a separate approved boundary.

Stop when this approved feature and scoped verification are complete, or at a documented bounded blocker. Do not merge into `master`/`main`, start another Goal or introduce an unnecessary approval round.
