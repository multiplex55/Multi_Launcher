# G09 M1 verification

**12/12 selected tests PASS; checkpoint ready for independent review.** Production logic is unchanged (`NO_CHANGE_REVALIDATED` evidence); parent acceptance/integration is pending. Exactly two selected owners and one native pre-read file-handle case were exercised. This is not feature-wide retirement acceptance.

## Exact execution

Package `multi_launcher`, library only, existing `fast-dev` profile, default features, offline and locked. The [exact 12-name filter](roles/implementer/verification/filter.txt) and [selected inventory](roles/implementer/verification/selected-tests.json) contain no ignored cases. No target expansion or correction rerun occurred.

```powershell
$g09Tests = (Get-Content -Raw 'docs/plans/execution/multi-launcher-reliability-closeout-20261029-20261029-r01/goals/G09/roles/implementer/verification/filter.txt').Trim()
cargo nextest list -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E $g09Tests --message-format json
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E $g09Tests --retries 0 --no-fail-fast --status-level pass --final-status-level fail --test-threads 1 --success-output final
```

| Phase | UTC start → finish | Actual elapsed / cap | Exit and evidence |
|---|---|---|---|
| Initial preparation/list | `2026-10-30T07:29:06.7529941Z` → `2026-10-30T07:32:37.8002999Z` | 211.037s / 1200s | Process drained; numeric Cargo exit **unavailable/null** in wrapper; compiler finished, complete JSON contains exactly 12 selected tests. No timeout. |
| Serial selected run | `2026-10-30T07:33:32.2100336Z` → `2026-10-30T07:33:35.8621332Z` | 3.648s / 180s | Cargo exit **0**, 12 passed, 0 failed; Nextest test execution 0.732s; 6439 skipped. No timeout. |

Incremental preparation allowance used: **0/600 seconds**. No rerun, full suite/build, domain target, lint, install or native campaign. The initial script-construction attempt failed before spawning Cargo with Python `LookupError: unknown encoding: utf8-sig`; it was corrected to `utf-8-sig`. The first preparation wrapper did not retain its process handle early enough to obtain a numeric exit; that null remains in the original receipt. The run wrapper retained the handle and captured exit 0. Complete inventory plus the successful exact-source run establish useful verification without a redundant preparation.

[Preparation receipt](roles/implementer/verification/initial-list.receipt.json), [preparation diagnostics](roles/implementer/verification/initial-list.stderr.log), [run receipt](roles/implementer/verification/initial-run.receipt.json), and [full selected run output](roles/implementer/verification/initial-run.stderr.log) retain commands, HEAD, toolchain, input blobs, timestamps, caps and process IDs. No compiler or test failure occurred. Warnings are from unchanged owners: vendored epaint (1 dead-code), egui (1 deprecated constant), image (9 lifetime syntax), table core (3 dead-code), and table session test `replaced_scan_job` (1 unused variable). None were suppressed or fixed.

Toolchain: cargo 1.99.1 (5f94df478 2026-08-27); rustc 1.99.1 (58cf8f9c3 2026-10-13); cargo-nextest 0.9.135 (610eefb88 2026-05-14), x86_64-pc-windows-msvc. Execution HEAD `691645e357d94f240a544996f5a58b532955a345`. [Final source parity](roles/implementer/verification/final-source-parity.json) confirms all ten source/manifest blobs match preparation, run and final working files. [Production parity](roles/implementer/verification/production-parity.json) confirms unchanged non-test sections. Full identity table is in [SOURCE_REVALIDATION](SOURCE_REVALIDATION.md).

## Owner-specific proof matrix

| Owner / boundary | Actual result | What the evidence does not claim |
|---|---|---|
| Reaper actual join | Notifier dropped while worker held: no callback and false reaped probe. After release: real callback, successful join probe, callback outside lock. | Notification alone is not join. |
| Reaper supervisor-spawn failure | Detached worker retains capacity until exit; callback disconnected and reaped probe false before and after release. | Capacity reclamation does not prove a successful join. |
| Duplicate Finder retained worker | Cancel-only drain preserves Cancelled state; refused start changes neither generation nor call count. Coalesced active job is cancelled/closed while held; repeated reopen refuses with unchanged Closed generation. After actual reaped probe + successful reopen: Idle contains no old roots, fresh generation completes with its current root. | Ordinary completed scan leaves a reusable worker; Activity `Unavailable` remains intentional. |
| Screenshot direct cancel | Held injected worker remains Draining/busy; new schedule refuses without generation/call-count advance. Release and owner tick join discard late image and confirm the exact row. | No native capture or apartment cleanup was exercised. |
| Screenshot shutdown | Owner transfers held worker; local busy clears, new schedules remain refused, reaped probe remains false. Release yields actual reaper join; Activity terminal row remains `Unavailable`. | Local ownership transfer and informational row are not join receipts; no fabricated Activity callback. |
| Activity/UI controls | Exact cancellation capability, capped/cleared history late-update fence, visible-only 250ms refresh, safe copied metadata all pass. Live-operation preservation and omission bounds are source-confirmed unchanged. | No full-app, UI pixel, native clipboard or global owner guarantee. |
| Native Duplicate Finder source handle | **PASS**, detailed observation below. | No in-flight `ReadFile`, capture/apartment, blocked-IO cancellation or worst-case latency proof. |

## Native observation and cleanup

[Native receipt](roles/implementer/verification/native-handle.json) is extracted from the passing test's output. Eight identical 128 KiB files (1 MiB total) were created under a unique task-owned `target/productivity_expansion/activity_retirement/g09-native-*` directory. Eight exclusive read opens succeeded before admission. At the real native reader's pre-read gate, fixture index **0** refused specifically with raw Windows error **32**. The same file still returned 32 after cancel/close, while reopen remained `WorkerRetiring` and the real reaped probe was false. After releasing the gate and observing successful join/reopen, all eight exclusive read opens succeeded. Warm restart produced one verified duplicate group, then a second worker join completed and the fixture directory was removed.

The existing production Windows acquisition/RAII path was used, with standard-library `OpenOptionsExt::share_mode(0)` observations; no FFI, feature or policy change. Source acquired before gate; gate before `file.read`. This limited ordering is the native claim.

All blocked fixtures have release-on-unwind guards and bounded normal completion waits. The successful tests drained/joined their controlled work; the supervisor-failure test intentionally proves detached/no-join semantics. [Cleanup](roles/implementer/verification/cleanup.json) at `2026-10-30T07:34:33.1529280Z`: recorded PIDs and cargo/cargo-nextest/rustc/link/multi_launcher process scan empty, fixture root empty. No kill/termination attempt needed. No app, hotkey, clipboard or persistent user fixture was created.

## Compact evidence and final static checks

Full unselected Nextest JSON remains ignored at `target/closeout-g09-m1-20261030-r01/initial-list.all-tests.json`; selected inventory records its SHA-256 and size. The ignored runner is `target/closeout-g09-m1-20261030-r01/invoke-focused.ps1`, SHA-256 `5a8b7e4611aa4b827d3be17e2e466293050a7e35a210ee542dd9f70395e3e534`. Only selected names, compact receipts and bounded diagnostic/run logs are retained under the goal. Local rustfmt used only the three touched Rust files, with `--edition 2024 --config skip_children=true`; no source edits after preparation began. `git diff --check`, scoped path review, source parity and Markdown link resolution are recorded in the final static receipt. No Git mutation by Implementer.
