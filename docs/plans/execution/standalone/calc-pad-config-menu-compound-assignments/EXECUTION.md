# Calculation Pad compact UI and compound assignments

## Branch and checkpoints

| Checkpoint | State | Evidence |
| --- | --- | --- |
| M0-A | Complete | Parent `feat/calc-scripting-studio-polish` at `af58c63cef1e01e286e18531b1b7a43257f3589c`. Child `feat/calc-pad-config-menu-compound-assignments` starts from that exact tip. Direct-child baseline `1feeeb02c0b5beb5f880f2ee98c6d9033f014a13` pushed to origin and tracks the matching upstream. |
| M0-B | Complete | Four representative baseline tests passed on the M0-A source snapshot. See below. |
| M1-A | Complete | Config Menu uses retained host effects. Focus, copy, display, compact layout, save, dependency and pop-out checks passed. |
| M1-B | Pending | Results context actions. |
| M2-A | Pending | Typed compound assignment. |
| M2-B | Pending | Loop, editor, session and comparison integration. |
| M3-A | Pending | Focused review, verification and Windows smoke. |

All checkpoints use the original checkout and its child branch.

## Baseline verification

Local evidence root is `target/calc-pad-config-menu-evidence`. The commands ran under `dev/run_owned_process.ps1`. Individual receipts include command arguments, elapsed time, process identities, output drain and cleanup.

- `cargo nextest run --lib --cargo-profile fast-dev -E <baseline selector> --no-run` exited 101. The base branch's `src/diagram/drawio.rs` test uses `include_bytes!` on an old path under `docs/plans/execution`. The tracked fixture now lives under `docs/archive/execution`. Evidence in `m0b-build/result.json` and `stderr.log`. Cleanup and output drain verified.
- Rebuilt with `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <corrected exact baseline selector> --no-run` using a temporary byte-identical copy of the archived fixture. Nextest exited 0 and emitted the baseline binary. The containment wrapper reported an indeterminate process identity at PID 18480. The post-run lookup identified an existing `svchost` at that PID. The wrapper verified output drain and recorded all compiler and linker identities absent. Evidence in `m0b-build-fixture`.
- Ran `target/fast-dev/deps/multi_launcher-b694ac7f5b3ac11b.exe --exact <four full test names>` against the baseline binary with a 100-second deadline. Sequential physical bindings, loop accumulation, canonical row and summary copy, and retained display preferences passed (4 passed, 0 failed, exit 0). Cleanup and output drain verified. Exact test names and arguments are in `m0b-run/result.json`, with counts in `m0b-run/stdout.log`.

## Config Menu verification

- `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <eight exact UI tests>` initially passed 6 and failed 2 (exit 100, `m1a-run`). The editor focus failure led to retained focus-origin capture on menu activation. The retained Copy all test reached its payload assertion with `RUST_MIN_STACK=16777216` (`m1a-copy-isolated`). Its expected string was updated to match the existing canonical formatter. Effect count and discarded-pass assertions passed on rerun.
- The same Nextest command with 18 exact UI tests and `RUST_MIN_STACK=16777216` passed 17 and failed 1 (exit 100, `m1a-correction-run`). The failure was `programmer_inspector_real_pad_selection_and_retained_bit_insert`, at the Ctrl+A replacement assertion before the Config Menu action. Running that exact test on the older executable `multi_launcher-6780e572eb679e65.exe` reproduced the identical source and expected value (exit 101, `m1a-inspect-prior-binary`). This cached executable has no recorded source SHA. The selection failure is retained for reporting with its existing assertions.
- Compilation receipts are in `m1a-build` and `m1a-correction-build`. Both compiler exits were 0. Test receipts contain exact selectors and verified process cleanup and output drain. The execution watchdog was 100 seconds per batch.
