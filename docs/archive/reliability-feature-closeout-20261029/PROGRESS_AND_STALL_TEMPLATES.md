# Goal and program progress / stall templates

## Required authoritative program index

```text
PROGRAM_ID: multi-launcher-reliability-closeout-20261029-<RUN_ID>
MASTER_START: <actual fetched sha>
INTEGRATION_BRANCH: <actual branch>
INTEGRATION_LAST_ACCEPTED_SHA: <sha>
ACTIVE_GOAL: Gxx / checkpoint or NONE
ACTIVE_WRITER: <role agent id / NONE>
NEXT_EXACT_ACTION: <one safe step>
LAST_PUSHED: <branch SHA and actual push status>
```

| Goal | Branch/source base | Checkpoints done | Verification proof | Decision (`ACCEPTED_COMPLETE`, `PARTIAL_ACCEPTED`, `DEFERRED_*`, `NO_CHANGE_REVALIDATED`) | Integrated accepted SHA | Next action |
|---|---|---|---|---|---|---|
| G00 | — | — | — | PENDING | — | — |

## Role provenance

Requested model/effort; `.codex` configured pin; observed backend/effort if runner exposes a receipt (else UNVERIFIED); agent/context ID; assigned sources; authored files; who persisted read-only outputs; independent-review status; number of iterations; no claim of other roles' work.

## Real stall report (one event, no repeated unchanged reports)

- Program/goal/milestone, timestamp, status: `DEFERRED_DEPENDENCY`, `DEFERRED_WITH_DESIGN`, `NATIVE_PROOF_DEFERRED`, `FAILED_VERIFICATION`, `BLOCKED_EXTERNAL`.
- Source baseline, actual HEAD, worktree/branch/upstream/dirty or operation state.
- Expected vs actual owner behavior; first diagnostic or unsupported tool/API; scope and urgency.
- Commands actually attempted and results/time caps; commands **NOT_RUN** and why.
- Accepted files and patches vs excluded/unverified changed files, temp/user-data cleanup and active process/child states.
- One attempted bounded recovery, if any, and why further retries are not useful.
- Exact next safe action; whether dependent goals must skip and independent goals may proceed.
- Commit and normal-push receipts; no fabricated GitHub visibility.

## Follow-on index

Finding ID, source/role/model, evidence link, severity/confidence, accepted/deferred/no-action reason, canonical GOAL/KICKSTART links, dependencies, needed future tests/native proof, branch and user authorization status. Every actionable recommendation gets a complete pair; speculative ideas can become research-spike pairs. No recursive queue explosion.
