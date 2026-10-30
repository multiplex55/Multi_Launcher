# G05 B2 native proof boundary

Planner-authored findings returned 2026-10-30; parent persisted. Requested Astra/max; configured Sol/high unchanged; actual backend UNVERIFIED. Source tip 37512cb9de1e6a9246890cfd9908fbe55f946d3d; accepted integration source 4999a47c68a9e4e5b8531cf31e018eda3eefda4f, receipt 93029e71.

Disposition: NATIVE_PROOF_DEFERRED; no B2 implementation or native test. Preserve accepted M1-A and corrected B1 (22/22; conditions blob 0559e8f9cc77430b210ab7949967ab8582cd6c99). This is an unresolved proof method, not API impossibility or a runtime failure. G02 OS5 supplies no evidence here.

## Owners and first unresolved boundary

`src/file_operations.rs`: ScanCancellation is AtomicBool; hash_reader performs synchronous Read and only checks between calls. prepare_hotfolder_source admits root/parent/file and hashes before returning GuardedReplaceFile. fingerprint_hotfolder_file drops that guard and returns copied evidence. execute_hotfolder_native_action_windows reopens/verifies before effects. open_native_replace_file requests DELETE/read and denies write/delete sharing; an extra held DELETE guard conflicts with reopening. Native owner blob b7d67bf4d49f81a8c4492991a95892a8dda4835b.

`src/hotfolder/conditions.rs`: LiteralSet/LiteralScan/LiteralEvidence retain exact-set identity and bounded complete-stream semantics. model.rs composes typed outcomes; execution.rs CandidateConditionEvaluation/HotFolderOperationPort own candidate binding. Actual queued starts belong to gui/hotfolder_dispatch.rs PendingLauncherAction, complete_launcher_actions, invoke_registered_action_from_catalog and dispatch_mkmacro; MkMacro activation/completion must also be bound.

No current repository fixture establishes a regular-file Read actually pending, subsequently cancelled and terminally retired. Atomic cancellation cannot interrupt an already blocked Read. Microsoft distinguishes request from completion and warns cancellation is driver-dependent; overlapped ReadFile may itself block. A pending anonymous-pipe probe can establish narrower mechanism ownership, not admitted regular-file acceptance. Cached read, pre-cancel, blocked parser, timeout return or dropped receiver cannot replace that proof.

- [Cancellation guidance](https://learn.microsoft.com/en-us/windows/win32/fileio/canceling-pending-i-o-operations)
- [CancelSynchronousIo](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelsynchronousio)
- [CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)
- [ReadFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-readfile)

## Closure

Persist [complete continuation](follow-on/g05-native-read-retirement/GOAL.md), preserve all FILE_CONDITIONS_NOT_EVALUATED gates and accepted foundations. No source, manifest, Cargo, app, native probe or manual-user test for this closure. No M1-B-wide, condition-to-effect, editor/Test Rule, queued-start or whole-G05 acceptance. Documentation checks: source identifiers, scope, links and staged paths. Parent owns Git, integration and authoritative ledger. Distinct additional findings: NO_ACTION; remaining work is G05 continuation, not a new initiative.
