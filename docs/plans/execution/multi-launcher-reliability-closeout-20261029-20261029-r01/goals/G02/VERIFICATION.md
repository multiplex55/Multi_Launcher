# Verification

M0-A was documentation/planning only. M1-A is **DEFERRED_WITH_DESIGN / NATIVE_NOT_PROVEN / UNACCEPTED**. The final selected run is **15 passed / 4 failed / 19 selected**, not a passing foundation. M1-B was not started; G03 remains blocked.

## Commands, selection and environment

Evidence directory: [roles/implementer/verification/m1a](roles/implementer/verification/m1a). Each `*.receipt.json` records the exact command, HEAD, per-file Git blob identities, timestamp, elapsed time, cap, owned PID, exit and output-drain result. The compact [selected inventory](roles/implementer/verification/m1a/selected-inventory.json) contains all 19 names: 13 application-library and 6 core-library tests, all non-ignored, including all 9 new `table_publication_` cases. The raw 6,510-name Nextest inventory remains ignored under `target/closeout-g02-m1a-20261029-r01`; hashes and paths are in [cleanup.receipt.json](roles/implementer/verification/m1a/cleanup.receipt.json).

```text
cargo nextest list -p multi_launcher -p multi_launcher_table_core --cargo-profile fast-dev --lib --offline --locked --color never -E "test(table_publication_) | test(resource_pool_leases_share_usage_and_release_on_last_owner) | test(partial_temp_append_failure_keeps_charge_until_owned_file_is_removed) | test(failed_temp_deletion_keeps_a_retryable_charge_until_removed) | test(cloned_snapshot_handles_keep_each_allocation_charged_after_session_drop) | test(pending_job_releases_its_resource_leases_before_confirmed_activity_retirement) | test(file_page_failure_and_cancellation_preserve_the_last_good_page) | test(file_page_completion_from_closed_lifetime_is_ignored_after_reopen) | test(table_session_draft_edits_revoke_queued_export_receipts) | test(table_file_save_refuses_to_replace_the_opened_source) | test(table_file_save_requires_confirmation_and_publishes_the_captured_output)" --message-format json
```

Execution uses the identical packages/profile/targets/offline/locked/filter, replaces `nextest list` with `nextest run`, and replaces `--message-format json` with `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`; exact expanded commands are in both run receipts. No full-domain target, broad suite, Clippy, independent build, installation or native GUI experiment.

Toolchain observed: cargo-nextest 0.9.135 (`610eefb88`, 2026-05-14), rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), cargo 1.99.1 (`5f94df478`, 2026-08-27), `x86_64-pc-windows-msvc`. Existing fast-dev cache. All times below are 2026-10-29 EDT (UTC-04:00).

| Phase / receipt prefix | Start–end | Elapsed / cap | Exit / result |
|---|---|---|---|
| `list` | 20:45:37.590–20:47:38.328 | 120.733s / 1200s | 101; E0277 fixture `FileReadError` lacks Debug |
| `correction-list` | 20:48:20.455–20:51:44.667 | 203.964s / 600s aggregate | 0; 19 selected, 9 new |
| `run` | 20:52:25.889–20:52:28.900 | 3.008s / 180s | 100; 15 passed, 4 failed |
| `native-correction-list` | 20:54:07.217–20:54:37.612 | 30.392s / 396s remaining | 101; E0277 existing RootError is not StdError |
| `native-correction-list2` | 20:55:05.499–20:56:31.295 | 85.792s / 365s remaining | 0; same 19 selected, 9 new |
| `native-correction-run` | 20:56:53.994–20:56:56.963 | 2.966s / 180s | 100; 15 passed, 4 failed |

Aggregate correction preparation: **320.148s / 600s**. No timeout, retry loop or budget renewal. The one affected correction rerun is consumed; parent acknowledged stopping without automatically spending the unused time.

## Actual failures and proof limit

Initial fixture correction matches existing `FileReadError::Cancelled/Failed` variants instead of requiring Debug with unwrap. Production error types were not changed. Initial native failure was the lossy `the reported file could not be inspected`. The localized adapter correction now retains the existing internal `RootAdmission.directories` and backend message, and normalizes Table's std-canonicalized verbatim path with the existing native helper. Its compiler feedback was corrected by formatting the existing RootError at that adapter; no guard/assertion was relaxed.

Final first responsible boundary: `NativeTablePublication::reserve` -> local `hold_parent` -> `NativeRenameScanBackend::admit_root` -> `open_native_object` -> `CreateFileW`:

```text
could not admit the selected root: CreateFileW failed: Access is denied. (os error 5)
```

Fixture class: `tempfile::tempdir()` beneath observed `C:\Users\Jay\AppData\Local\Temp\`, with `source.csv` and new `output.csv`/`appeared.csv` in the same task-owned directory. Source admission canonicalizes the path; the adapter normalizes it to DOS spelling. Unique fixture leaf and failing ancestor were not emitted by the test, so no exact ancestor is asserted. The existing native traversal opens each drive-to-parent component with `FILE_READ_ATTRIBUTES`, `FILE_SHARE_READ | FILE_SHARE_WRITE` (no delete sharing), `OPEN_EXISTING`, `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`; metadata-only admission does not request enumeration. The OS access denial is proven; ACL, sandbox, sharing or another environmental cause is **not established**. The next diagnostic should retain existing `BackendFailure.path`, rather than infer the denied component or reduce guards.

Four failures at stage reservation: exact new-file publication/identity transfer; appeared-output preservation; source/parent/stage substitution fence; and manifest-revision/chunk refusal. They occur before CREATE_NEW, so none proves its later native effect. Two native-named refusal cases pass, but an early parent refusal can satisfy generic error assertions; they do not prove successful publication or all later identity checks. The incompatible-writer test's direct source-open assertion did run. All three deterministic resource cases and all ten existing controls passed. Deterministic tests prove quota edge, partial-write charge retention, injected seal/publish failure and exact-capability cleanup/retry/accounting; they are not native failure proof.

## Static checks, warning ownership and cleanup

Scoped rustfmt and `git diff --check` succeeded. Search finds the native adapter used only by the private publication module/tests; no UI or normal session caller. Only the five approved source files plus G02 own receipts changed. The final run source hashes still match the cleanup check; Cargo.toml/Cargo.lock hashes remain unchanged. Parent-owned ledgers/role files were preserved.

Warnings observed are outside new behavior: vendored epaint `Unknown`, egui deprecated EPSILON, image lifetime syntax (9); existing core scanner constant/function and `StreamRecord.end_offset`; existing session test `replaced_scan_job`. No warning fix or suppression added beyond the explicit private future-caller module annotation.

All recorded owned process IDs had exited at cleanup. Process command-line enumeration via CIM was denied; direct `Get-Process` checks of recorded PIDs succeeded and found none. No wrapper, service or persistent test helper created. TempDir fixtures unwind on failure; failed native cases created no stage. Passing resource tests check owned cleanup and charge retirement. No arbitrary external directory cleanup was attempted. Raw inventories remain ignored with SHA-256 references; tracked evidence is selected-only plus bounded logs/receipts.

Exact resume package: [GOAL](roles/implementer/follow-on/native-parent-admission-proof/GOAL.md) and [KICKSTART](roles/implementer/follow-on/native-parent-admission-proof/KICKSTART.md). No source integration eligibility is claimed.
