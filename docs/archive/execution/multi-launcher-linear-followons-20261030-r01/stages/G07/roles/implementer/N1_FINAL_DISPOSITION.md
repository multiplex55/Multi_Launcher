# G07 N0 verification disposition

Status: **FAILED_VERIFICATION / NATIVE_PROOF_DEFERRED**. The prepared candidate was frozen and remains unaccepted. After evidence preservation, the parent restored the current workspace to the M0 baseline. No matching test run or native test execution occurred.

## Frozen source

Base HEAD: `d62ac109d27c2810ba097c04cae562185ee2c0f7`.

| Path | Captured candidate SHA-256 | Candidate Git blob |
|---|---|---|
| `src/file_operations.rs` | `5f84de133eb13f246d206cac3a41fe5e5bca15f557e571953dd77c1b07439800` | `0f362f2994c8decbf2f0c88b36ac6e9360f3b386` |
| `src/archive/preparation.rs` | `553495e03d202f4065f81910b17f861a75066fa1ce5e00867626668093384836` | `8c657ec60557ac37252ff457184f7bb37d0c5d0f` |
| `src/archive/mod.rs` | `35d434dad9c45353f0f6b311cbccff42cd469ca48ed826ff4786e5a537041098` | `ba84fd5ca01d55bee053a1c927cc0165612efb6c` |

All three candidate hashes match the last list supervisor's before/after receipt. The parent verified candidate gzip preservation and patch reverse-check, then restored `src/file_operations.rs` and `src/archive/mod.rs` to M0 and removed the task-created preparation module. At role handoff before the B7 closing commit, workspace HEAD was `d62ac109d27c2810ba097c04cae562185ee2c0f7`; current `file_operations.rs` SHA-256 is `b88bd57c493406acbdd39681782637890cc0334e02053fb57ced024a53bd814a` (baseline blob `b7d67bf4d49f81a8c4492991a95892a8dda4835b`), `src/archive/preparation.rs` is absent, and `src/archive/mod.rs` is baseline SHA-256 `6122c11351ad17ae64a109a8b21743d038968f5228a7761dd82cfcec0a97528a` (blob `267a87a87552d63c5f8c67d76448a43c01eeccee`).

## Bounded list attempts

All commands used the same exact packet filter and `cargo nextest list -p multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked --message-format json`. The commands retained direct .NET process handles and drained both streams. Child trees remain **UNVERIFIED**.

1. Initial list: PID `34332`, start `2026-10-31T03:36:13.3537605+00:00`, elapsed `150.544s` / cap `1200s`, direct exit `101`, streams drained. Stdout was empty (SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`); stderr SHA-256 `408ad652f42a1f4b0ed50cdbbc88020d801218d1aa4cb275773680d4b5797220`. First compiler diagnostics were E0364 test-helper visibility, E0433 missing `SelectionTarget`, three E0599 Windows metadata trait-scope errors, and E0277 conversion of `BackendFailure`.
2. Corrected list: PID `4140`, start `2026-10-31T03:40:08.3711019+00:00`, elapsed `114.438s` / cap `600s`, direct exit `101`, streams drained. Stdout was empty (same SHA-256); stderr SHA-256 `fdfe1b1776961e22c0ca5fa1e7596c0110f79e2dfce7ccb5169f44638a0230aa`. First remaining compiler diagnostic was E0282 for the archive directory-chain vector.
3. Final correction list: PID `39128`, start `2026-10-31T03:42:50.4047648+00:00`, effective cap `485.5s` after `114.438s` prior correction-command time, supervisor elapsed `486.248s`, timeout disposition, direct exit reported `-1`, streams drained. `Process.Kill(true)` was invoked and returned without exception; descendant status remains UNVERIFIED. Stdout was empty (same SHA-256); stderr SHA-256 `46348c252a8d518e3209fc8b8aeba82c132720135747fa6a1973d63f0f771b5e`. Stderr ended after `Compiling multi_launcher` with warnings and no reported compiler error before the cap.

The remaining correction budget calculation was `600.000s - 114.438s = 485.562s`; the final effective cap was set to `485.5s`. Configured/effective sum was `599.938s`. Recorded supervisor elapsed including bounded stop/drain was `600.686s` across the two correction attempts, `0.686s` beyond the 600s aggregate allowance (`0.748s` beyond the configured/effective sum). No claim is made that the child compiler tree was independently absent.

## Selection and fixtures

No Nextest JSON inventory was emitted, so the packet's exact 12 nonignored names were not established and the matching run was not started. The empty compact arrays in the list receipts are explicitly nonqualifying (`stdout` was empty), not evidence of zero tests in a completed inventory. No current G07 native endpoint behavior was observed; do not infer OS32, alias behavior, or any other native result from this attempt.

The owned `target/productivity_expansion/archive_native` root contained only the three empty list stdout files at final inspection; the G07 native tests did not execute and created no test fixtures.

Known direct PIDs `34332`, `4140`, and `39128` were absent on exact-PID checks after their owned supervisors drained. Byte-exact stderr gzip copies and ignored raw copies are retained; `RAW_LOG_PRESERVATION.json` maps the original capture paths and hashes. No source acceptance claim is made.
