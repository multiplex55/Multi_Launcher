# Goal — Preserve Folder Diff mutation exclusion during rescan cancellation

Status PROPOSED / NOT_EXECUTED. Priority P1. Discovering role G01 reviewer, requested Astra/max, backend UNVERIFIED. Finding G01-R1. CONTINGENCY ONLY: first use G01 permitted correction; if resolved there mark RESOLVED_IN_G01 and do not schedule another task.

## Evidence and owner

At 8d183f663b212f8a707ac16a4c056eef5fe641ff, src/diff/folder_runtime.rs424-427 cancels then clears the operation. OperationHandle.cancel only sets a flag; copy/recycle observe between items. Ordinary refresh/watch calls prepare_rescan; plan_mutation/dispatch use mutation_active as exclusion. High-confidence source regression, no native overlap reproduced. Existing tests cover retained feedback and report classification, not pending cancellation.

## Objective and scope

Existing FolderRuntime owner blocks a second mutation until terminal acknowledgement. Allowed src/diff/folder_runtime.rs, src/gui/diff_dialog/mod.rs, owned tests/receipts. Keep executor API, H0 authority, recycle/dirty guards, generation/view/roots. No joins, native identity, atomicity, generic retirement, new cancel UI, watcher redesign or unrelated fixture repairs.

## M1-A

1. Revalidate current source and safe branch; evidence SHA is not a reset target.
2. Retain operation during same-comparison rescan cancellation; matching old-generation terminal can release exclusion.
3. Reject stale feedback and rescan; preserve normal valid adoption and explicit comparison invalidation.
4. Deterministic owned pending-operation fixture: same-root rescan, second dispatch refused, terminal delivered, gate releases without stale result/rescan. Wrong acknowledgement cannot release a different active operation.
5. One focused review, coherent checkpoint commit and normal best-effort parent push.

## Verification and acceptance

One library Nextest list/preparation and focused run of lifecycle case and directly affected rescan/generation/dispatch controls. Require nonzero inventory. Incremental preparation600s, built execution180s; no domain/full suite/native test. Exact commands/count/time/exit/source/cleanup receipts required. Done: gate held through pending cancellation, matching terminal releases, stale view effects refused, normal feedback passes.

If bounded owner correction needs broader architecture, preserve blocker, exclude unsafe M1-B; independently accepted M1-A may integrate. Roll back only task edits. Parent owns Git/acceptance; no automatic merge. Requested planner/reviewer Astra/max, implementer Astra/xhigh; record actual configuration/backend. Execute only after explicit activation if R1 remains unresolved. No tests/commits executed by authoring pair.

## Resolution

RESOLVED_IN_G01 at6c6812f03d6e6ea4b7d3f2810b0834456bd5e14a; independent named confirmation2026-10-30 00:15:14 UTC. Contingency is closed; do not schedule or execute this pair. Historical source finding above remains evidence.
