# G05 continuation — native read retirement and source ownership

Status: REQUIRED_CONTINUATION / NOT_EXECUTED. Priority P1. Origin: G05 B2 NATIVE_PROOF_DEFERRED, 2026-10-30. Companion: [KICKSTART](KICKSTART.md). Planner-authored continuation, parent persisted; required remaining scope, not a distinct initiative.

## Objective and evidence

Establish reproducible Windows proof that an actual pending read on an admitted regular source can be cancelled and terminally retired. Only then implement a bounded retained-source adapter and effect handoff. Starting foundation: corrected B1 944f9d0ef4ed5ceee54c9a3d255c40e9a220494d integrated as 4999a47c68a9e4e5b8531cf31e018eda3eefda4f, plus accepted M1-A. Verify full current refs and relevant blob parity; never import initially rejected B1 alone.

High-confidence observed source: AtomicBool ScanCancellation; synchronous hash_reader; prepare_hotfolder_source preliminary hash; fingerprint_hotfolder_file drops guard; built-ins reopen; DELETE/read source handle denies write/delete sharing. Not established: controlled regular-file pending-read cancellation/retirement proof or conforming native adapter. Windows cancellation APIs provide counterevidence to impossibility; missing supported proof does not establish impossibility. Read official [guidance](https://learn.microsoft.com/en-us/windows/win32/fileio/canceling-pending-i-o-operations), [CancelSynchronousIo](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelsynchronousio), [CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex), [ReadFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-readfile).

## Ownership and scope

Existing Hot Folder region/tests in src/file_operations.rs, connected through src/hotfolder/execution.rs::HotFolderOperationPort; only necessary B1 visibility changes. Reuse native guards and effects. No duplicate FFI/reader/authority system, generic I/O framework, driver/service install, OS-policy change, arbitrary user-folder fixture, forced process/thread termination, detached reader, whole-file buffer, weakened sharing or persistent polling.

## Ordered checkpoints and required behavior

1. Proof before production migration: controlled supported regular-file fixture must establish read entered pending native operation, cancellation after that point, actual terminal status, and every thread/request/handle/buffer retired before completion or dependent effect. Fresh subsequent operation succeeds without stale cancellation. Use owned disposable target/productivity_expansion/hotfolder_native fixtures following m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture. A pipe-only probe can separately prove narrower device mechanics, never this gate. Cached success, pre-cancel, blocked parser or timeout-only evidence insufficient. If gate cannot be proved, retain refusal and record exact missing facility; no speculative probe campaign.
2. Only after independent proof acceptance: start one monotonic candidate-wide budget before acquisition. Include preliminary hash in 8 MiB / 64 KiB chunks / two seconds; feed B1 and compute fingerprint in one pass. Validate settled root/parent/file identity, length, mtime, genuine EOF and final binding. Retain admitted source or verifiably transfer to existing effect body; no conflicting guard plus reopen. Preserve unconditioned behavior, source/output safeguards, journal/dedupe/rate limit/cancellation/completion owners.
3. Following required owner migration: complete_launcher_actions/invoke_registered_action_from_catalog/dispatch_mkmacro verify current rule/revision/epoch/control/source at actual invocation/activation, using existing authorization/completion. Hold request, invalidate evidence, prove zero invocation/activation and exactly one correlated terminal outcome.
4. Resume original G05 editor/Test Rule, final safety proof/review and controlled enablement. No gate removed by checkpoints 1–3 alone. Original Goal/Kickstart and SAFETY_CONTRACT remain binding.

## Verification, acceptance and rollback

Targeted multi_launcher library Nextest, fast-dev, offline/locked; nonzero selected inventory and exact tested blobs. Initial prep 1200s, execution 180s, aggregate incremental prep 600s, 1–2 localized actual-error reruns. No full suite/build, domain target, install or broad lint. Native proof includes successful regular-source admission, sharing/substitution refusal, pending-read cancellation/retirement/resource release and retained-source actual built-in effect without reopen conflict. Request is not terminal retirement; outstanding resources forbid success/action start.

Accept only proved independently reviewed checkpoints. Parent commits and normally pushes; failed prototypes are not integrated. Preserve accepted M1-A/B1 and all gates on unsuccessful attempt. Requested planner/reviewer Astra/max, implementer Astra/xhigh; record configured and actual backend separately. One writer, parent Git/status. No user manual tests between milestones.
