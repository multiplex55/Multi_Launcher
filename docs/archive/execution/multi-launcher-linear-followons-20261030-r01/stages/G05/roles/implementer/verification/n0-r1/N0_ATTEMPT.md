# G05 N0 — native pending regular-file read attempt

Status: NATIVE_PROOF_DEFERRED / NOT_ACCEPTED. This attempt did not observe a native pending read and did not establish terminal cancellation status 995. No source acceptance is claimed.

## Change

Added one Windows/perf-harness test in src/file_operations.rs:
file_operations::tests::g05_native_pending_regular_read_cancellation_retires_owned_source.

The test creates a dense 2 MiB fixture beneath target/productivity_expansion/hotfolder_native, admits it through the existing watch-root observation and prepare_hotfolder_source path, and moves the complete GuardedReplaceFile into one reader thread. The reader seeks before the marked phase, performs one synchronous ReadFile of at most 64 KiB through the admitted original handle, uses no OVERLAPPED structure, and returns the guard only after the native call is terminal. The controller polls GetThreadIOPendingFlag for at most 100 ms and calls CancelSynchronousIo only after observing TRUE. It joins the reader before checking retained identity and write/delete sharing refusal, dropping the first guard, freshly admitting and hashing the source, performing one uncancelled read, and retiring/removing the fixture. The marker is synchronization only; it does not count as pending evidence. Contents are not logged.

The initial list compilation found one format-placeholder error. The single localized correction fixed the diagnostic and changed the start marker to an unbounded channel so a marker wait timeout cannot block the reader before its terminal result. The correction did not change the pending-observation or cancellation gate.

No production code, dependencies, configuration, or FILE_CONDITIONS_NOT_EVALUATED gates were changed.

## Selected inventory

The corrected Nextest JSON reported exactly four selected, nonignored cases:

- file_operations::tests::g05_native_pending_regular_read_cancellation_retires_owned_source
- file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture
- hotfolder::execution::tests::hotfolder_condition_metadata_conditioned_execution_stays_refused
- hotfolder::capability::tests::enabled_condition_rules_fail_closed_until_the_shared_evaluator_is_installed

The complete package inventory had 6,504 cases; the full unselected JSON was moved under ignored target storage. The compact inventory, relocation receipt, and raw-log hashes are in this directory.

## Native observation and exact first failure

The run used Nextest run ID b2045430-bb69-4a25-9dba-50e52977f645. The G05 test observed pending_observed=false after 70 successful flag queries; the read completed normally with 65,536 bytes and no terminal HRESULT. The reported pending window is 0 ms because Duration::as_millis floors sub-millisecond durations; the polling loop stopped when the single read completed. CancelSynchronousIo was not called. No retry was made.

The exact first failing assertion was:

GetThreadIOPendingFlag did not observe TRUE within the single 100 ms window; no retry was attempted (terminal HRESULT: None)

The required 995 terminal cancellation was therefore not observed. These measurements do not show that cancellation is impossible; they show this one admitted regular-file read completed naturally on this run.

The post-read retirement checks passed: original identity remained held; writer and delete attempts were both refused with sharing violation; the source remained present while guarded; fresh admission retained the same file identity and SHA-256; a fresh 65,536-byte read succeeded; source cleanup and TempDir::close both returned success. A follow-up filesystem check found no directory with the unique case-g05-pending-read- prefix remaining.

The test output recorded process ID 34576, reader native thread ID 16672, volume serial 12837577387797144463, file ID adc80500000004000000000000000000, and source SHA-256 be52904a058b062aac55808cae437e2275ca10a521657b0006ca5005888020c0. Fixture size was 2,097,152 bytes; the marked and fresh read caps were each 65,536 bytes. The ephemeral tempfile suffix was not logged; the fixture root and unique prefix are recorded above, and cleanup was confirmed.

## Verification

Initial list preparation:

- Exact command was cargo nextest list -p multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked --message-format json with the four exact test(=...) filters above.
- Direct PID 29432 exited 101 after 235.855 seconds (1,200 second cap). Child tree UNVERIFIED; redirected output was drained. The first diagnostic was the format-placeholder error. Receipt and raw logs are list.receipt.json, list.stdout.json, and list.stderr.log.

Single correction list preparation:

- Same exact list command and filter.
- Direct PID 27844 exited 0 after 540.310 seconds (600 second cap). Child tree UNVERIFIED; redirected output was drained. The JSON inventory selected exactly four nonignored cases. Receipt and raw stderr are correction-list.receipt.json and correction-list.stderr.log; selected-inventory.json retains the compact selection. Full JSON is in the ignored target location identified by full-list-relocation.receipt.json.

One selected run:

- Exact command was cargo nextest run -p multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked --retries 0 --no-fail-fast with the same four exact test(=...) filters.
- Direct PID 20632 exited 100 after 22.303 seconds (180 second cap). Nextest reported 4 tests: 3 passed, 1 failed, and 6,500 skipped. Child tree UNVERIFIED; redirected output was drained. No process was killed. Raw output SHA-256: stdout e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855; stderr 66dd937e9c4c3df917274dd90b245f82d674c4a74579a68d07c1c004ad1bbe62. See run.receipt.json and run.stdout.log/run.stderr.log.

Base HEAD was 0c6f2b0624be4c43a3d69e455072a04f08d79f2d; base src/file_operations.rs Git blob b7d67bf4d49f81a8c4492991a95892a8dda4835b. Final task source SHA-256 is 8703eec28b47679fd04bbee28e892dc7984db80247ce73e19ef2305e0b12c29f; Git blob 17373039280b9f597853f7c59c9d6ab74edc9ab2. The pre-correction candidate SHA-256 was 5b0640e04b4abd8b031d19c9f49167e1d6fc02f0b7732750b632ba48b5194d6a and the author-reconstructed Git blob is retained in list.receipt.enrichment-error.txt; the valid repaired receipt retains the original working SHA256. candidate-source.patch.gz preserves the final task diff; the gzip round-trip SHA was verified and patch/raw-source gzip hashes and exact round-trip checks are recorded in the parent PRESERVATION_AND_ROLLBACK_PRECHECK.json.

## Boundary

N0 stops here. No retry followed natural completion, and no N1, N2, N3, UI activation, or source acceptance was attempted. Parent verified preservation and restored only the task-authored src/file_operations.rs change to B2 M0; source parity and patch reapply checks passed. Keep all condition-bearing rules refused.

Evidence clarification: original source Git blob17373039280b9f597853f7c59c9d6ab74edc9ab2 is the raw unfiltered working-file identity. Canonical LF Git blob under core.autocrlf=true is1b78081c87f2ead7c0930b63f21a334e82b93835. Raw SHA256 remains8703eec28b47679fd04bbee28e892dc7984db80247ce73e19ef2305e0b12c29f and is preserved byte-exact in candidate-file_operations.rs.gz. The malformed enriched initial receipt is retained as list.receipt.enrichment-error.txt; validJSON repaired from original precise UTC fields previously read by parent, without changing raw logs or test results.
