# G02 follow-on — establish native parent admission and publication proof

Status: **PROPOSED / NOT_EXECUTED**. Discoverer `/root/implementer`; continuation of the current G02 proof gap. G02 remains **DEFERRED_WITH_DESIGN / NATIVE_NOT_PROVEN**; M1-A source is **UNACCEPTED**, M1-B unstarted and G03 blocked. Companion: [KICKSTART](KICKSTART.md).

## Objective and evidence

Establish why the existing native parent-chain owner refuses the task-owned publication fixture, and obtain genuine Windows proof of the existing private M1-A seam without weakening its ancestry, source, stage or no-clobber guarantees. This is not permission to enable a public/session/UI caller.

Observed on Windows x86_64-pc-windows-msvc, branch `codex/closeout-g02-table-publication-safety-20261029-r01`, HEAD `24f3c13f812a8e1654c490c15729142c721e469b`; exact tested dirty-source blobs in [final run receipt](../../roles/implementer/verification/m1a/native-correction-run.receipt.json). Parent may subsequently checkpoint this source on the donor; revalidate that checkpoint rather than assuming an unrecorded SHA or importing it as accepted. Master `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`, bootstrap `02d0ac64d51655f43d6e958ea551f5bf658ee48d` are provenance, not reset instructions.

Two selected runs each returned exit100 with 15 passed and 4 failed. Final first native error:

```text
could not admit the selected root: CreateFileW failed: Access is denied. (os error 5)
```

Failing cases reserve a stage before any write: `table_publication_native_new_file_preserves_source_and_transfers_exact_identity`, `table_publication_native_occupied_same_source_and_appeared_outputs_are_untouched`, `table_publication_native_source_parent_and_stage_cannot_be_substituted`, and `table_publication_manifest_revisions_and_bounded_chunks_refuse_without_output`. See [verification](../../VERIFICATION.md) for full counts, commands, timings and limits.

Confidence: high that the adapter cannot demonstrate ordinary success in this execution context; high that refusal is before CREATE_NEW in existing parent admission; unknown which ancestor or host policy causes access denial. Counterevidence: read-only Table source admission and all ten existing controls plus three deterministic resource cases pass. Two native-named generic-refusal cases pass but can return early at the same parent boundary, so they do not disprove the gap. No source corruption or successful unsafe publication observed.

## Existing owners and scope

- `src/file_operations.rs`: `NativeTablePublication::reserve::hold_parent`, `NativeRenameScanBackend::admit_root`, `open_native_object`, existing `BackendFailure { path, message, kind }`. Preserve existing native helper ownership; no duplicate FFI or publisher.
- `src/table_utility_session/publication.rs`: only the private reservation fixture and exact native tests needed to establish the same contract. The admitted source retains its Arc and original handle; do not replace it with page strings or a reopened substitute.
- Existing core resource owner remains unchanged unless a concrete native failure demonstrates a directly necessary ownership correction; require an explicit bounded parent handoff for that additional change.
- Own G02 source/verification/risk/role records. No parent/global ledger edits by implementer. No table UI, worker lifecycle, parser, dependencies, configuration, manifest, FileSearch fixture or generic native infrastructure migration.

Current fixture class is `tempfile::tempdir()` below observed `C:\Users\Jay\AppData\Local\Temp\`, containing `source.csv` plus requested absent output in the same directory. The random directory leaf was not logged. Native parent admission requests only FILE_READ_ATTRIBUTES, shares READ|WRITE but denies DELETE, opens existing components with OPEN_REPARSE_POINT|BACKUP_SEMANTICS and retains the full verified chain. The detailed backend message is now preserved, but its existing `path` is not yet included in the adapter diagnostic. Do not presume an ACL, sandbox or sharing cause without evidence.

## Bounded milestones

1. **D1 — exact diagnosis and minimal owner correction.** Revalidate the donor source, preserve the failing ancestor using existing BackendFailure.path at the local adapter, and reproduce only this native boundary on explicitly task-owned fixtures in a context that supports the requested read/identity handles. If the host cannot support that operation, record the exact access restriction and stop; do not automatically elevate, change ACLs or substitute path-only publication. Correct only a demonstrated local API/path/ownership defect. Keep all native assertions and preserve compact behavior. Parent checkpoints a coherent source/evidence result if useful; it remains unaccepted until proof.
2. **D2 — native proof and disposition.** Use the same selected test inventory after the correction, prove actual absent-output bytes/identity transfer, appeared-target preservation, source/parent/stage guards and exact cleanup, then obtain one bounded independent review of the resolved issue. A passing M1-A checkpoint only enables a fresh M1-B planner handoff; whole G02 requires its remaining lifecycle proof and acceptance.

## Required invariants and tests

Retain original source/parent/stage identity handles; no existing/same-source/unsupported destination writes. Charge growth before write, preserve charge on partial failure, flush/sync before effect and transfer output only after successful handle-bound no-replace rename. Failed cleanup retains native capability and quota in the existing pool; do not add a second queue or release ownership merely to make a test pass. No public production caller, full-output Vec, path reopen as identity proof, path rename fallback, relaxed Windows sharing or ignored/removed tests.

Use the 19 selected names in [selected-inventory.json](../../roles/implementer/verification/m1a/selected-inventory.json): nine new prefix cases plus ten directly affected existing controls, application and core library targets only. Resource module is `resource::resource_tests`. Record which paths actually reach stage creation/publish; generic error assertions alone do not prove a particular refusal. Native tests must call the actual Rust Windows adapter; mocks do not close the gate.

## Verification budget and done/defer boundary

Future execution requires explicit parent dispatch; this package grants no automatic budget renewal. Proposed bounded new dispatch: one offline/locked fast-dev preparation/list, cap600s, then one selected run cap180s. At most one localized correction within that same preparation allowance; no full domain, full suite, Clippy, dependency installation or historical harness. If host support requires a changed execution permission/context, establish that specific requirement through normal authorization before native effects. Do not spend unused time just because it remains.

Done: nonzero exact inventory, successful actual native publication and guard cases, truthful ownership/accounting evidence, no scoped regressions, reviewed exact source identity, cleanup and parent checkpoint. Defer: access denial remains unlocalized/unsupported, required ownership cannot be retained, budget exhausted or another material owner expansion is required. In any defer state preserve the first exact cause, source hashes and safe next step; source remains donor-only, G02 deferred and G03 blocked.

## Roles, commits and rollback

Parent orchestrator owns Git, deduplication, accepted integration and progress. One implementer writer; user-requested Astra/xhigh, configured role and backend attribution recorded separately. Planner/reviewer use the user-requested assignments when explicitly dispatched; backend provenance stays UNVERIFIED unless independently available. No config edits to satisfy model labels.

Use one coherent diagnostic/correction checkpoint followed by proof/docs if necessary, with parent-owned normal push attempts. No history reset or import of unaccepted donor source. Rollback/default: leave all full-file publication callers disabled and preserve the previously accepted application behavior; parent can keep or exclude the entire donor source checkpoint. Do not delete others' edits or retrofit claims onto earlier runs.

Canonicalized by parent after independent documentation review. Preserved donor f879a2a880d59e941486241e652299dabc002365; source remains UNACCEPTED. Role-authored original retained under roles/implementer/follow-on. Parent catalog priority P1; no execution or budget renewal authorized by publication of this pair.
