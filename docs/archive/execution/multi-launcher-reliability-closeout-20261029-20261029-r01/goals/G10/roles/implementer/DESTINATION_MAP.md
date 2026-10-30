# G10 parent-ready destination map

This packet contains proposed content only. All writes are confined to this Implementer role directory. Parent alone materializes authoritative files, commits/pushes/imports, obtains cumulative independent review and records PR facts. No final acceptance is asserted.

Links inside proposal files are intentionally relative to their **intended destination**, generally the execution run root; they are not expected to resolve from `proposals/`. The static validator materializes an in-memory destination view before checking them. The [machine-readable map](destination-map.json) names exact repository destinations, operation, preimage hash and append-only event text. Do not copy a mutable fragment as an entire Goal or MEGA file.

| Proposal | Intended repository destination | Parent application |
|---|---|---|
| [PROGRAM_STATUS.md](proposals/PROGRAM_STATUS.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/PROGRAM_STATUS.md` | `replace_file` |
| [BRANCH_MAP.md](proposals/BRANCH_MAP.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/BRANCH_MAP.md` | `replace_file` |
| [ACCEPTED_CHECKPOINTS.md](proposals/ACCEPTED_CHECKPOINTS.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/ACCEPTED_CHECKPOINTS.md` | `replace_file` |
| [FOLLOW_ON_CATALOG.md](proposals/FOLLOW_ON_CATALOG.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/FOLLOW_ON_CATALOG.md` | `replace_file` |
| [DEPENDENCY_MAP.md](proposals/DEPENDENCY_MAP.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/DEPENDENCY_MAP.md` | `replace_file` |
| [INTEGRATION_MANIFEST.md](proposals/INTEGRATION_MANIFEST.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/INTEGRATION_MANIFEST.md` | `replace_file` |
| [PR_SUMMARY.md](proposals/PR_SUMMARY.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/PR_SUMMARY.md` | `replace_file` |
| [MANUAL_REVIEW_CHECKLIST.md](proposals/MANUAL_REVIEW_CHECKLIST.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/MANUAL_REVIEW_CHECKLIST.md` | `replace_file` |
| [FINAL_HANDOFF.md](proposals/FINAL_HANDOFF.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/FINAL_HANDOFF.md` | `replace_file` |
| [RISKS_AND_DECISIONS.md](proposals/RISKS_AND_DECISIONS.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/RISKS_AND_DECISIONS.md` | `replace_file` |
| [G10_PROGRESS.md](proposals/G10_PROGRESS.md) | `docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G10/PROGRESS.md` | `replace_file` |
| [MEGA_HEADER.md](proposals/MEGA_HEADER.md) | `docs/plans/reliability-feature-closeout-20261029/MEGA_GOAL.md` | `replace_between` |
| [MEGA_GOAL_MATRIX.md](proposals/MEGA_GOAL_MATRIX.md) | `docs/plans/reliability-feature-closeout-20261029/MEGA_GOAL.md` | `replace_between` |
| [MEGA_PROGRESS.md](proposals/MEGA_PROGRESS.md) | `docs/plans/reliability-feature-closeout-20261029/MEGA_GOAL.md` | `replace_between` |
| [G00_MUTABLE_PROGRESS.md](proposals/G00_MUTABLE_PROGRESS.md) | `docs/plans/reliability-feature-closeout-20261029/goals/G00-baseline-and-continuation/GOAL.md` | `replace_between` |
| [G03_MUTABLE_PROGRESS.md](proposals/G03_MUTABLE_PROGRESS.md) | `docs/plans/reliability-feature-closeout-20261029/goals/G03-table-full-file-operations/GOAL.md` | `replace_between` |
| [G10_MUTABLE_PROGRESS.md](proposals/G10_MUTABLE_PROGRESS.md) | `docs/plans/reliability-feature-closeout-20261029/goals/G10-residual-defect-triage-integration/GOAL.md` | `replace_between` |

## Exact materialization protocol

1. Read the complete proposals and preserve other writers' edits. For a full-file replacement, require the recorded SHA-256 of **raw destination bytes** or expected absence. If it differs, stop that replacement and reconcile the actual parent change rather than overwrite it.
2. For `replace_between`, locate the exact start/end anchors, normalize CRLF/CR to LF, encode as UTF-8 and verify the section SHA-256, and replace only that range. The start anchor is included; the end anchor and all subsequent events are retained. The three MEGA ranges are disjoint. Current product priorities/dependencies and all immutable charter text remain unchanged.
3. Append `append_event_if_absent` only when absent. Do not delete, rewrite or duplicate existing event logs. BRANCH_MAP, ACCEPTED_CHECKPOINTS and RISKS proposals preserve the prior bodies under explicit historical headings.
4. Review the in-memory/static receipts, materialize on the parent G10 review branch, run the parent-owned final diff/source/config check and obtain independent cumulative review. Only accepted documentation is eligible for integration.
5. Resolve final G10/PR/head/base/push/check/review fields from actual operations. No proposed file names the unknown commit containing itself. The prepared manual checklist is a single file linked by PR/final handoff.

`docs/plans/continue.md`: **NO_CHANGE**. Its two existing live links resolve to the orchestrator and current run status; the queue notice is historical. No continuation proposal is needed.

[Provenance audit](PROVENANCE_AUDIT.md), [static checks](STATIC_CHECKS.md), and [role record](ROLE_RECORD.md) describe the bounded work and remaining parent gates.
