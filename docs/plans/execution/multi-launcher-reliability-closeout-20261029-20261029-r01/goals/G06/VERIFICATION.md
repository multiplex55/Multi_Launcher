# G06 verification

Baseline docs only, no Cargo/native run. Source checkpoint caps:1200s prep,180s run,600s aggregate incremental,1–2 actual-error reruns. Nonzero focused library Nextest/fast-dev/offline/locked; no unrelated domain target/full suite/build/install. App fixtures disable Paste Sequences next/cancel hotkeys before construction. Native keyboard tooling potentially available via sky; exact-source binary/profile isolation must be established before native claims.

## M1 editor/transaction and M2 focused proof — 2026-10-30

**Final result: 25/25 PASS, no ignored cases, 6435 unrelated cases skipped.** Nine new `g06_jump_` tests plus sixteen specified existing/extended controls. This is a source checkpoint awaiting independent review and parent acceptance, not whole G06/native acceptance.

The [selected-only inventory](roles/implementer/verification/m1/selected-tests.json) lists every actual selected name. The handoff's Settings focus test module was stale: its actual name is `settings_editor::render::settings_shell_tests::jump_mode_focuses_real_settings_controls_and_later_egui_keys_activate_them`. The initial list selected 24; a cached selector-only correction confirmed 25 before execution. No selected-case claim is based only on an expected count.

| Phase | UTC start → end (2026-10-30) | Wall seconds / cap | Exit | Result |
|---|---|---:|---:|---|
| Initial preparation | 04:02:43.8872998 → 04:06:11.4103955 | 207.519 /1200 | 0 | Compiled; 24 selected, stale module selector found |
| Selector correction | 04:07:11.9959944 → 04:07:15.5735151 | 3.573 /600 aggregate | 0 |25 selected; source unchanged |
| Initial run | 04:07:26.8608578 → 04:07:33.3722712 | 6.507 /180 |100 |24 PASS,1 fixture failure;3.560s test execution |
| Fixture correction preparation | 04:08:02.4945399 → 04:09:26.5285091 |84.030 /596 remaining |0 |25 selected; only test client lifetime changed |
| Corrected run |04:09:51.9205784 →04:09:58.6547705 |6.726 /180 |0 |25 PASS;3.538s test execution |

Aggregate incremental preparation 87.603s of 600s; one actual-error correction run. No cap hit or renewed budget. `cargo 1.99.1 (5f94df478 2026-08-27)`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Nextest 0.9.135 (`610eefb88762529a316373f4a50f5fd9194c3c35`), host `x86_64-pc-windows-msvc`.

Exact commands, full filter expressions, timestamps, process IDs, source/manifest Git blobs and exits are in each [phase receipt](roles/implementer/verification/m1/). Commands use `cargo nextest list/run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E <recorded expression>`; list uses `--message-format json`, run uses `--retries 0 --no-fail-fast --status-level pass --final-status-level fail`. Execution was bounded by the recorded wall cap, including cached startup. No other Cargo commands beyond version inspection were run.

First meaningful failure is preserved verbatim in [run-01.stderr.log](roles/implementer/verification/m1/run-01.stderr.log): `settings_save::tests::g06_jump_stale_prepared_draft_cannot_overwrite_concurrent_preferences`, `src/settings_save.rs:1819:34`, `called Result::unwrap() on an Err value: Disconnected`. Its cancellation, returned stale-baseline error and persisted concurrent `AB` alphabet assertions had already passed. The moved sole client dropped when the worker returned. Retaining a cloned fixture sender keeps the final “no Apply message” assertion meaningful; no assertion or production behavior was weakened. The corrected file's blob is `443d33aaa82cf3fad6eca2559ccab4163ebc469b`; all other recorded source/manifest blobs are unchanged between runs. Final success log: [run-02-fixture.stderr.log](roles/implementer/verification/m1/run-02-fixture.stderr.log).

Warnings are unchanged owner warnings: nine from vendored `image`, three table-core dead-code warnings (`SCANNER_CANCELLATION_INTERVAL`, `scanner_cancellation_checkpoint`, `StreamRecord.end_offset`), and unused test variable `replaced_scan_job` in untouched `src/table_utility_session.rs:3521`. No warning points at a G06 change. No fixes or warning suppression were made outside scope. Git emitted the repository's LF→CRLF conversion notices; `git diff --check` passes.

Coverage includes raw invalid preservation/canonicalization/defaults/sample/catalog, exact-local/subset-global conflicts and no global registration, final merged preparation rejection, owned-field apply/ack overlay/conditional rollback, stale baseline rejection, live-before/after/failure acknowledgment, pending draft preservation/reopen, actual egui text/Reset/Cancel response behavior, saved fallback and existing controller/query/Notes/focus/release gates. These use deterministic controllers/egui frames and task-owned temporary settings with injected runtime acknowledgment channels. The integration test file's three mapping calls were mechanically migrated; the domain target was NOT_RUN.

Cleanup: all owned preparation/run processes exited and output was drained. TempDir/IsolatedWorkspace fixtures retire through the existing test ownership; no persistent profile, app process, native hook, clipboard fixture or GUI launch was created by this checkpoint. Full unselected inventories remain only in ignored `target/closeout-g06-m1-20261029-r01/`, with hashes/references in [artifacts.json](roles/implementer/verification/m1/artifacts.json). Only selected names, bounded diagnostics and receipts are tracked. Static checks cover scoped diff, exact final source hashes, links and process retirement. Native M3/output/focus proof is **NOT_RUN**, with [setup guidance](roles/implementer/NATIVE_M3_SETUP.md) for the parent after review.

## R1 final-resolver correction — 2026-10-30

Review found that the base-validator/library proof did not cover effective Paste Sequences bindings in main. Original `b708e38a` remains unaccepted until the named correction review; the earlier25 passes remain historical evidence. Parent explicitly authorized `src/main.rs` and `--lib --bin multi_launcher` from `5c00e367c88e90e7897b5ccd9e537618d886eb1a` to test the actual final-resolver owner.

**Corrected result: 29/29 PASS**, comprising all original25, one new editor case and three direct `resolve_hotkey_route_candidate_with_macro` cases. [Selected inventory](roles/implementer/verification/r1/selected-tests.json):26 library and3 binary; no ignored cases,6499 skipped. Direct cases cover effective custom Next/Cancel and saved Start conflicts; disabled Jump/plugin/control, absent required modifiers, optional-default omission and no Jump global registration. Explicit snapshot bindings avoid native playback/store discovery; new cases construct no application.

| Phase | UTC start → end | Wall seconds / cap | Exit |
|---|---|---:|---:|
| R1 preparation |2026-10-30 04:26:17.4468273 →04:30:38.8877505 |261.429 /512 |0|
| R1 run |2026-10-30 04:30:57.1087214 →04:31:04.0212659 |6.908 /180 |0|

Run test execution3.891s. No R1 compiler/test failure or rerun; cumulative incremental preparation87.603+261.429=349.032s of600s, remaining250.968s unused. No budget renewal. Same toolchain and filter union as above, with the necessary binary target and four new prefix cases. Full commands/source+manifest blobs are in [prepare-r1.receipt.json](roles/implementer/verification/r1/prepare-r1.receipt.json) and [run-r1.receipt.json](roles/implementer/verification/r1/run-r1.receipt.json); full output in [run-r1.stderr.log](roles/implementer/verification/r1/run-r1.stderr.log).

Warnings remain in untouched owners: the prior vendor image/table-core/test warnings, plus three unused variables (`response`, `slot`, `results`) in `src/gui/table_utility_dialog.rs:1672`,1599,1583 from compiling the binary's non-test library dependency. No R1 warning or suppression. No standalone build, domain/full suite, native launch, clipboard/hook probe, dependency/config or Git change.

Exact final-source equality, scoped diff/link checks and process retirement are in [cleanup.json](roles/implementer/verification/r1/cleanup.json). Full unselected inventories remain ignored with hashes in [artifacts.json](roles/implementer/verification/r1/artifacts.json). Source remains pending parent checkpoint and R1-only confirmation. Native M3 is NOT_RUN; binary test preparation does not establish a launched application artifact.
