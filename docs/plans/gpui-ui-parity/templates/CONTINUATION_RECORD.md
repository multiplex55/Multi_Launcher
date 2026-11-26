# Continuation: <goal / checkpoint / UTC timestamp>

**Template only — unfilled.** State: **Pending**. Use this for a durable handoff or genuine stop; it is not a reason to stop ongoing authorized work.

## Current authority and checkout

- Required methodology: [EXECUTION_STRATEGY.md](../EXECUTION_STRATEGY.md). Read and follow it before resuming or delegating.
- Current objective and controlling instructions:
- Existing checkout path, task branch and actual base:
- Local HEAD / upstream HEAD / last accepted checkpoint:
- Git state at report preparation: staged, unstaged, untracked, merge/rebase operation:
- Active single writer / build owner / native controller:
- Source/configuration changes since the prior handoff:

## Completed and in progress

- Verified work with evidence/review links:
- Implemented but unverified work:
- Current files/symbols and preserved partial work:
- GUI/requirement IDs still pending:
- Prior evidence requiring selective revalidation:
- Last command/result and owned processes still active:

## Blocker, only if genuine

- State: In progress / Blocked / Paused / Explicitly deferred:
- Exact missing capability, authority, dependency or unresolved decision:
- Error/check/command and expected versus actual result:
- Attempts already made, diagnostics and known cause versus hypothesis:
- Cleanup/output drain and fixture state:
- Independent authorized work completed or still available:
- Smallest safe next action and required authority, if any:
- Required stall-report path when stopping:

Ordinary compiler errors and review fixes are continuation work, not automatic user blockers. Do not discard partial changes or claim missing native checks passed.

## Resume instructions

1. Read current task status, this record, the active checkpoint packet, [EXECUTION_STRATEGY.md](../EXECUTION_STRATEGY.md), and applicable AGENTS and agent configuration. Include the strategy and relevant requirements in each agent handoff.
2. Confirm checkout/branch/HEAD and preserve intentional user changes.
3. Resume: <exact next bounded action>.
4. Run only: <necessary focused checks and remaining native evidence>.
5. Review/commit: <next coherent checkpoint and owner>.
6. Continue with: <next authorized goal/checkpoint>.

## Commit and push status

- Local checkpoint/report commit:
- One normal push attempt and actual result:
- Unpushed work and next ordinary opportunity:
- User-facing progress summary:

A documentation-only stop commit is not an accepted implementation checkpoint. No worktree creation, reset/force-push or retry loop to repair a failed push. Follow [VALIDATION_STRATEGY.md](../VALIDATION_STRATEGY.md).

