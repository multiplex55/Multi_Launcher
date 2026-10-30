# Source revalidation

Full G02 Goal/Kickstart read by parent; actual Table/source/page/export/atomic helper and table-core ownership to be revalidated by planner. No historical unsafe patch import; missing historical reports remain limitations.

## M1-A implementer source checkpoint

Implementer read the full G02 Goal/Kickstart, safety contract and approved planner packet against HEAD `24f3c13f812a8e1654c490c15729142c721e469b`, branch `codex/closeout-g02-table-publication-safety-20261029-r01`. Master `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`; bootstrap `02d0ac64d51655f43d6e958ea551f5bf658ee48d`. No historical source imported.

Revalidated actual `scan_table_file`/retained `TableFileBackedSource`, actual session revisions and source-bound row/column resource owners, `TableTempFile` growth/cleanup leases, and native parent-chain/create/identity/rename/delete helpers. The original resource cleanup queue retained a remembered path and charge but lacked an owned native cleanup capability. The existing native CREATE_NEW helper inspected the new handle before transferring cleanup ownership. Parent approved additive adapters at these two existing owners.

Current changes are confined to the private session publication module and its module declaration; the existing core resource owner plus mechanical re-export; and a crate-private adapter inside `file_operations.rs`. The resource owner now transfers an identity-bearing native capability into its existing pending-cleanup map on cleanup failure, preserving byte and metadata charges and blocking new publication while cleanup is active. The native CREATE_NEW split transfers ownership before fallible inspection; old replacement callers preserve their prior access/sharing/inspection behavior.

The private manifest retains the actual source Arc, source/native identity, path, lifetime, draft/accepted source, view, selection and operation revisions, source-bound rows/columns and captured resource admission. The stage has a 64 KiB append boundary, seals before no-replace handle publication and has no normal production caller. No GUI, parser, engine, dependency, configuration or persisted schema changes. M1-B owns worker/session cancellation, close, stale completion and durable-in-session recovery integration.

Source identities for each preparation/run, including the fixture compiler correction and native-reservation correction, are recorded in `roles/implementer/verification/m1a/*.receipt.json`. Test selection uses the actual existing `resource::resource_tests` module; the planner's `resource::tests` spelling was approximate. Selection intent and existing assertions are unchanged.

Final disposition: M1-A donor source **UNACCEPTED**; G02 **DEFERRED_WITH_DESIGN / NATIVE_NOT_PROVEN**. Final preparation compiles, but four native cases still refuse at existing parent-chain CreateFileW with OS error 5. No exact denied ancestor or host cause is inferred. The own-role native-parent-admission Goal/Kickstart preserves a bounded resume; no M1-B caller or additional source fix was started after stop.
