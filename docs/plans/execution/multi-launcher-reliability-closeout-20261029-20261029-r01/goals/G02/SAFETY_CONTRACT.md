# G02 safety contract

Status: DESIGN_READY_FOR_M1_A; implementation and native proof pending.
Source: 50608bd417b34854358989a5d4b1325e95293063.
Branch: codex/closeout-g02-table-publication-safety-20261029-r01.
Master: 4a1283e6edd2c245f562a07dbe3dcc6a61061eed.
Bootstrap: 02d0ac64d51655f43d6e958ea551f5bf658ee48d.
Author: /root/g00_planner, Astra/max requested, backend UNVERIFIED; parent persisted returned contract. Authoritative program state is on integration; missing historical receipts are not reconstructed.

## Scope and exclusions

A source-bound, bounded, NEW-FILE-ONLY publication foundation. Existing destinations, unsupported locations and same-source aliases fail closed. G03 may consume it only after full acceptance for a minimal full-file export. No full-file UI, destructive replacement, in-place editing, sort/filter or second engine. Existing compact CSV/TSV/JSON behavior remains unchanged and gains no new safety claim.

## Current source and call-site matrix

| Concern | Current source | Required boundary |
|---|---|---|
| Read admission | request_file_backed_source -> queue_file_backed_source_with_admission -> Work::ScanFile -> scan_table_file | Reuse captured resource admission and accepted source ownership. |
| Native source | open_readonly_source uses FILE_SHARE_READ on Windows; source_identity calls file_operations::native_file_identity | Retain source; verify identity/path association at reservation and commit; actual Windows sharing probes required. |
| Contents | TableFileBackedSource retains Mutex<File>, native identity, source ID, row count and sparse index; scan checks initial/final length; full-cell cursor reads retained file | Length is not identity or a content fingerprint. Use retained source/native freshness; reject unsupported conditions. |
| Last good | accepted_source separate from pending/draft and provisional progress | Failure, refusal, cancellation and stale work preserve accepted data/page/selection. |
| Selection | TableFileRowSelection source-bound and sparse; pages hold bounded previews | Capture source-bound rows and stable columns; never export preview strings as full data. |
| Compact export | request_export_with_basis requires resident document; receipts bind lifetime/source/view/selection/document/operation | New file-backed manifest must preserve compact checks and distinguish file-source from resident document. |
| Completion | TableWorker has one pending job; poll/accept gate results; stale/cancelled work can be discarded | Discarded stage still owns cleanup/quota. Final publication requires current session authority. |
| Compact output | GUI save_receipt/show_overwrite_confirmation validate receipt; absent target persist_noclobber, existing confirmed target save_atomic | Preserve behavior; do not use this route for full-file publication. |
| Atomic helper | Sibling write/sync then replacement; no captured destination identity binding | Reference only; do not modify other users. |
| Temp resources | TableTempWorkspace/TableTempFile own growth, leases and failed-cleanup charges; no publication handoff | Add smallest file/workspace/charge/cleanup-preserving handoff; logical cancellation does not release physical resources. |
| Native effect | file_operations private parent-chain guards, native_file_identity, rename_open_file_no_replace, delete_open_file | Add narrow adapter INSIDE existing owner; preserve callers. |
| Rejected reuse | GuardedReplaceFile/Stage require full &[u8] and buffer verification | No full materialization or replacement transaction import. |

## Immutable manifest and authority

Capture actual session lifetime, draft and accepted source revisions, TableSourceId/native identity, source path and retained Arc<TableFileBackedSource>, view/selection revisions, operation ID, captured source-bound rows/stable columns, resource policy/admission, destination leaf/parent guards/identity and observed absence.

A prepared stage cannot authorize its own publication. Before the irreversible effect, TableUtilitySession must remain open/current and match every applicable identity. Worker may stage bounded output; completion before cancellation observation is not authority to publish. Final gate belongs to session, not rendering code.

## Native reservation and staging

Only absent destinations supported. Refuse existing files/directories/reparse objects, same-source aliases, unsupported native locations and inspection errors. An output appearing between reservation and commit remains untouched.

Hold verified ancestry through staging and publication. Stage on destination filesystem via resource owner; retain the original identity-bearing file/handle through effect. Reopening a remembered stage path is not sufficient proof. Use existing handle-bound no-replace operation, never check-then-path-rename or MoveFileExW replacement.

Write bounded chunks; charge BEFORE growth, including actual partial-write ownership. Finish buffered output and flush/sync while retaining native publication ownership. Close handles only after outcome and cleanup obligations are established.

## Resources, cancellation and recovery

One resource owner retains staging bytes, source/cursor/selection leases and failed cleanup charges for as long as workers/results own them. Outcomes distinguish refusal before staging, staged unpublished, cancellation before publication, published with exact output identity, and cleanup/reconciliation required.

Cancellation before publication refuses; cancellation after successful effect cannot undo it or replace its truthful published receipt with a generic cancelled result. Never retry an ambiguous publication blindly.

Cleanup targets the owned object, not whatever later occupies its path. Failure retains identity-bound cleanup capability AND quota in the existing cleanup mechanism. Close or stale-result discard cannot erase that obligation. Refuse another publication until unresolved receipt/cleanup is reconciled.

No source or pre-existing output is mutated. No cross-file transaction, arbitrary crash recovery or power-loss durability claim follows from rename. If required recovery cannot be retained/reconciled at this bounded owner, STOP rather than silently passing the limitation to G03.

## Rejected alternatives

- Collect all source/output into RAM or export display-page previews.
- Treat compact overwrite/save_atomic as identity-safe full-file publication.
- Path-only rename after an existence check.
- Duplicate native FFI in Table Utility or create a generic filesystem framework.
- Release quota on cancellation request or create a second global cleanup queue.
- Enable controls while publication/cleanup proof is pending.

## Exact proof and acceptance

Use synthetic task-owned fixtures and controlled stream/failure injection. Native cases execute actual Windows adapter, not only fake callbacks.

1. Source: ordinary writer/replacement conflicts with retained read admission; incompatible pre-existing writer refused; changed identity/path/session/source revision refuses staging/commit and preserves last-good state.
2. Destination: absent target publishes exact bytes; appeared target preserved; existing file/directory, same-source alias, unsupported/reparse location refused; parent relocation/substitution cannot redirect effect.
3. Streaming: bounded chunks; exact quota edge and one-byte-over rejection; short writes handled; partial write, flush/seal/publish failure never exposes partial final output or overwrites another object.
4. Lifetime: cancellation before staging or before commit refuses; source/view/selection/operation/lifetime changes reject stage; close/shutdown/stale discard retain cleanup; completed effect keeps published receipt despite later cancellation.
5. Recovery: successful cleanup releases charges after retirement; injected failure retains stage identity/charge/record; retry acts only on owned object; unresolved state blocks publication; publication transfers user output and releases only temporary charges that ceased to apply.
6. Compatibility: accepted page/selection/last-good preserved; compact receipt revocation/copy/export/confirmation behavior unchanged.

Record platform/source SHA, selected inventory, exact outcomes/output and source hashes, cleanup/resource facts. Unsupported native proof is NATIVE_NOT_PROVEN and prevents ACCEPTED_COMPLETE.

## Milestones, verification and gate

M1-A: reservation/resource/native seam. M1-B: current-session commit, cancellation, recovery, shutdown and stale-result integration. M2-A: focused deterministic and real Windows proof. M3-A: review and exact acceptance.

Use focused Nextest preparation/list and run; localized reruns only for actual failures. Targets multi_launcher --lib and multi_launcher_table_core --lib, existing fast-dev cache. Prefix new cases table_publication_. Include directly affected existing resource/session controls named in planner record; no broad GUI/table/native/repository suite. Standard preparation cap 20 minutes, built execution 180 seconds; corrections 10 minutes. Source checkpoints may remain unaccepted until their relevant proof.

G03 stays blocked until G02 ACCEPTED_COMPLETE. A compiled seam, docs receipt or fake-native PASS cannot satisfy it.

Closure amendment2026-10-30: design is not accepted implementation. M1-A donor f879a2a8 failed native verification15/19; G02 DEFERRED_WITH_DESIGN/NATIVE_NOT_PROVEN; M1-B not started; G03 blocked. See VERIFICATION, stall report and canonical follow-on.
