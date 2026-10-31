# G05 final verification — B02 N0

Disposition NATIVE_PROOF_DEFERRED; no N0 source accepted. Existing accepted M1-A and literal B1 foundations remain unchanged, with every production condition refusal retained. N1–N3 NOT_RUN.

| Phase | Cap | Actual result |
|---|---|---|
| Initial selected list |1200s|235.855s, PID29432 exit101, format-placeholder diagnostic; no tests |
| Single correction list |600s aggregate|540.310s, PID27844 exit0; exact4nonignored, full package6504cases |
| Single matching run |180s|22.303s, PID20632 exit100;4run/3pass/1fail; Nextest test interval0.411s; no timeout/retry |

Commands use multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked and four exact test(=...) filters in the [compact inventory](roles/implementer/verification/n0-r1/selected-inventory.json). Run retries0/no-fail-fast. No broad suite/build/lint/app/native GUI/install.

The actual admitted 2MiB regular file was read once through its original retained handle, capped64KiB. GetThreadIOPendingFlag returned no TRUE after70queries; the read completed normally with65536bytes. Polling stopped on completion, reporting sub-millisecond duration floored to0ms, rather than exhausting100ms. No CancelSynchronousIo call or terminal995 occurred. Natural completion is a failed mandatory gate, not cancellation proof or API impossibility. No retry pursued a favorable timing result.

Actual trace: reader native TID16672, test PID34576, source volume12837577387797144463/file IDadc80500000004000000000000000000, SHA256be52904a058b062aac55808cae437e2275ca10a521657b0006ca5005888020c0. Native source identity, write/delete sharing refusal, fresh identity/hash/read and source/fixture cleanup checks passed after actual join. Two fail-closed controls and existing native copy/move fixture passed. This proves only those narrower observations, not pending cancellation/production reader/effect/queue/editor readiness.

See [attempt report](roles/implementer/verification/n0-r1/N0_ATTEMPT.md), [run receipt](roles/implementer/verification/n0-r1/run.receipt.json), and unchanged raw run/diagnostic logs. Full unselected inventory is ignored target storage with relocation/hash receipt; only compact4selection is tracked. Source remained frozen across corrected list/run at SHA2568703eec28b47679fd04bbee28e892dc7984db80247ce73e19ef2305e0b12c29f. Raw Git blob17373039280b9f597853f7c59c9d6ab74edc9ab2 and canonical LF blob1b78081c87f2ead7c0930b63f21a334e82b93835 are both verified; core.autocrlf explains the distinct hashes.

One correction fixed compiler formatting and the reviewer's startup-marker deadlock. The one-shot marker is nonblocking; actual TRUE-pending/995 assertions were preserved. [Static review](roles/reviewer/N0_STATIC_REVIEW.md) is not native acceptance.

The initial receipt was accidentally serialized as PowerShell object text during metadata enrichment. That raw text/hash is retained; a valid receipt was reconstructed from the exact original UTC/PID/exit/hash JSON fields previously read by parent, without inferring lost fractional seconds/timezone. All raw logs remain unchanged. Final metadata receipts parse as JSON; failed command's empty stdout is raw output, not a JSON document.

[Preservation/precheck](PRESERVATION_AND_ROLLBACK_PRECHECK.json) proves exact tested raw source gzip, canonical patch, hashes, empty index and reverse check. Parent restored only src/file_operations.rs to M0; [rollback/cleanup](ROLLBACK_AND_CLEANUP.json) records source parity and patch reapply PASS. Recorded Cargo/test PIDs absent; streams drained, child tree UNVERIFIED due CIM denial. Unique fixture suffix not logged (UNKNOWN), no matching task prefix remains. No unrelated path/PID cleanup or source changes. No source acceptance or condition enablement.
