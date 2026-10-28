# Multi Launcher — Defer unfinished Goal 04 editor work and continue at Goal 05

## 1. Explicit user-authorized queue amendment

Execute this amendment to queue `multi-launcher-queue-20261025-eedc19d2` in the repository currently located at `G:\Repos\rust\Multi_Launcher`.

This is a deliberate scope and sequencing decision, not another repair-budget renewal. Stop working on Goal 04 M2-A and M2-B. Preserve the unfinished editor work for possible future resumption, but do not return to it automatically during this queue.

Within user-controlled project instructions, this amendment supersedes the master queue's requirement for a completed `FINAL04` before Goal 05, and supersedes previous `continue.md` or Goal 04 resumption instructions directing more editor repairs. It does not override platform permissions, security restrictions, or the substantive acceptance criteria of other goals.

Record Goal 04 as **PARTIALLY COMPLETED — EDITOR WORK DEFERRED BY USER**:

- M1-A and M1-B: retain their existing accepted checkpoint status and evidence, after verifying the local ledger and committed history.
- M2-A: deferred, implemented in part but not accepted. Preserve the failed observer receipt and all partial work.
- M2-B: deferred / not executed.
- Overall Goal 04 completion and final lint: do not claim either without the evidence actually required. No `FINAL04` is to be manufactured.

This is permission to continue around an excluded, unfinished editor deliverable. It is not permission to call that deliverable tested, complete, or safe to include in the continuation branch.

## 2. Short, read-only reconciliation

Read `AGENTS.md`, the master queue, the durable `QUEUE_STATE.md`, the latest Goal 04 blocked-resume record, and the Goal 05 pair. Use the existing queue manifest and constraint map for the remaining goals; do not repeat the whole planning exercise.

Verify the current branch, HEAD, index, worktree paths, remotes, accepted checkpoint identities, and any active queue-owned processes or source writers. Preserve all unrelated user work. The original Goal 04 checkout must have no active source writer before this handoff. Do not launch simultaneous queue executions.

Known reference identities, which must be checked rather than blindly assumed:

- Goal 04 branch: `codex/windows-link-editor-queue-20261025`.
- Accepted M1-B implementation commit: `ceff1f17fb09545d8b9a6010aae9eca7892a7ea7`.
- Verified final Goal 03 / Goal 04 source base: `6a3d612fcceafe1423341f835cf1f5f2fa317ff5`.
- At preparation of this amendment, GitHub's Goal 04 branch head was `fcecb6cd61a7d8787515284cd6177403834b6e17`; the two commits after M1-B added/updated resumption instructions. This is provenance, not an instruction to reset to that head.

Use the user's latest local report as the source for uncommitted editor work. GitHub does not expose that work or the external diagnostic receipts.

## 3. Preserve the deferred work without carrying it forward

Leave the original Goal 04 branch and checkout intact, including tracked modifications, untracked files, controller/observer scripts, Lua profiles, evidence, and blocked-resume instructions. Record their locations and reuse existing hashes; refresh only genuinely changed inputs.

Do not reset, stash, discard, clean, force-checkout, rewrite history, or commit unfinished M2-A merely to obtain a clean checkout. Do not remove or reconfigure the user's installed editor. Do not rerun the editor scenario or repair its primary-versus-note selector.

Read-only safety reconciliation may confirm existing source-restoration and process-cleanup evidence. It is not authorization for another live editor experiment. If an owned process is still active, use its existing verified shutdown/cleanup mechanism, never a global process-name kill.

## 4. Make an explicit safe handoff around Goal 04

After verifying that M1-B is accepted and that its commit contains the accepted linker work but no unfinished M2-A editor implementation, record:

```text
CARRY04 = ceff1f17fb09545d8b9a6010aae9eca7892a7ea7
BASE05 = CARRY04
FINAL04 = not created; Goal 04 editor scope deferred
```

`CARRY04` means the accepted committed portion of a partially completed goal. It is not a completed-goal marker. Verify that it descends from FINAL03 and includes the accepted Goals 01–03 changes.

Create a separate, clean Git worktree from this exact commit for Goal 05, on a fresh, unique, idiomatic branch such as `fix/plugin-settings-scroll-queue-20261025`, adding a suffix if required. Do not switch the dirty original checkout. Do not use a moving branch tip or copy unfinished editor files into the new worktree.

Fulfill Goal 05's M0-A exactly once: create its small legitimate baseline record or explicitly empty baseline commit, record base and baseline SHAs, then make one normal best-effort upstream push. A denied push does not block safe local work. No Goal 05 implementation or tests precede this bootstrap.

If the selected commit cannot be verified as the accepted clean boundary, report the exact discrepancy once. Do not fix it by incorporating partial M2-A, resetting the original checkout, or assuming master contains the cumulative work.

Use absolute paths to the verified original `docs/plans` directory when supplying plans to agents. Record the distinct control checkout, execution worktree, evidence root, and plan root. Keep stale Goal 04 continuation instructions inactive; this amendment controls the next action.

For Goals 06–17, resume the existing cumulative rule: each new branch starts from the previous completed goal's verified final commit. Preserve functional dependencies, including Goal 10 requiring Goal 09 and Goal 17 requiring Goals 09, 10, and 12. Do not merge into master/main.

## 5. Execute Goal 05, not another tooling project

Read and execute:

- `docs/plans/CODEX_GOAL_05_PLUGIN_SETTINGS_SCROLL.md`
- `docs/plans/CODEX_KICKSTART_05_PLUGIN_SETTINGS_SCROLL.md`

Goal 05 is the existing bounded Plugin Settings fix: contain long descriptions within a usable dialog and provide horizontal plus vertical scrolling while preserving filtering, toggles, persistence, Settings Search navigation, and Jump Mode control identity.

Follow its single implementation checkpoint and its original scoped build, test, native UI, and final-lint requirements. Reuse the existing application verification facilities. Do not introduce another editor profile, language-server observer, telemetry subsystem, or general verification framework to complete this UI change.

Use the original/default supported build workflow unless the accepted opt-in linker mode is already an appropriate verified choice. Editor optimization is not a prerequisite for building or verifying Goal 05.

Preserve the already-approved configured-agent fallback: use the configured implementer when available, or the authorized parent as the sole writer when dispatch is exhausted. Label parent review honestly; never claim it is independent review. Do not bypass actual permissions or retry unavailable dispatch indefinitely.

## 6. Keep verification honest and proportional

Do not repeat successful Goal 01–03 tests, Goal 04 linker benchmarks, or the failed live-editor campaign merely to justify this handoff. Reuse evidence only where the code, artifacts, configuration, and scope actually match. Old binaries-metadata paths are not proof of old executable contents; prepare/bind the targets that Goal 05 actually needs.

Goal 05 and later goals retain all mandatory functional, regression, safety, process-ownership, and native acceptance gates. This amendment does not authorize skipping arbitrary failures or weakening assertions. It also does not reset general repair budgets.

A short preservation audit and clean-worktree bootstrap are administrative handoff work. Do not turn deferral into a new multi-checkpoint project or require Goal 04's missing editor acceptance before proceeding.

## 7. Durable status and autonomous continuation

Record this amendment and the deferral decision in the existing external queue ledger. Preserve historical entries and hashes; append the changed authority and current next action rather than overwriting the old evidence. Record CARRY04, BASE05, Goal 05 branch/worktree, baseline, and push result.

Queue summaries must distinguish completed goals from the user-deferred editor scope. At the end, report completion of the revised active queue with Goal 04 editor work deferred—not "all 17 original goals complete." Do not automatically schedule a retry of M2-A/M2-B or restart them when the remaining goals finish.

After Goal 05 is accepted, committed, and pushed or retained locally after a normal push failure, continue Goals 06–17 under the existing master and this one explicit exception. Do not request intermediate manual testing or approval. Stop only at the revised queue's completion or a genuine remaining safety/correctness/permission blocker.

Begin with the short reconciliation, preserve Goal 04, create the clean Goal 05 worktree and baseline, and implement the Plugin Settings fix. Do not respond with another proposal to fix the editor observer.
