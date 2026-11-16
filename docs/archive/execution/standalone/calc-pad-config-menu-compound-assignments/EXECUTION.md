# Calculation Pad compact UI and compound assignments

## Branch and checkpoints

| Checkpoint | State | Evidence |
| --- | --- | --- |
| M0-A | Complete | Parent `feat/calc-scripting-studio-polish` at `af58c63cef1e01e286e18531b1b7a43257f3589c`. Child `feat/calc-pad-config-menu-compound-assignments` starts from that exact tip. Direct-child baseline `1feeeb02c0b5beb5f880f2ee98c6d9033f014a13` pushed to origin and tracks the matching upstream. |
| M0-B | Complete | Four representative baseline tests passed on the M0-A source snapshot. See below. |
| M1-A | Complete | Config Menu uses retained host effects. Focus, copy, display, compact layout, save, dependency and pop-out checks passed. |
| M1-B | Complete | Ordered Results context actions preserve copy, comparison, navigation, and Inspect bits. |
| M2-A | Complete | Six operators use typed arithmetic and a prior-only target. Physical source bytes and results are preserved. |
| M2-B | Complete | Runtime binding checks, cycle handling, editor assistance, comparison, persistence and loop tests. |
| M3-A | Complete with native smoke unverified | Final focused tests passed. The Windows executable launched, but capture failed before interaction. |

| Checkpoint | Commit |
| --- | --- |
| M0-A | `1feeeb02c0b5beb5f880f2ee98c6d9033f014a13` |
| M0-B | Baseline evidence included with M1-A |
| M1-A | `ff003fc0d6184a518b951e0eb45e905315f3bddc` |
| M1-B | `b444aa2513ee6e7b0a87250eeb66664c2b579f5a` |
| M2-A | `c970b6393e025c95a710a22b8ba859fb9f9f77b0` |
| M2-B | `25c7ba25c975f1d4621f07c05b4b958022441123` |
| M3-A | Commit containing this closeout is the next-Goal base |

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

## Results context menu verification

- `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <14 exact Results and Inspector tests>` passed 13 and failed 1, exit 100 (`m1b-run`). The navigation fixture clicked a disabled command before using a now-closed menu. Isolated runs and the fixture correction are recorded in `m1b-nav-isolated` and `m1b-nav-corrected`.
- The final exact 15-test batch passed all 15, exit 0, with verified cleanup and output drain (`m1b-final-run/result.json`, `stderr.log`). It covers error Copy disabled, row label/value/background navigation, action order, canonical and formatted copy, Copy all source order, vectors, exact-integer Inspect bits, and stale or retired host rejection. Each execution used a 100-second watchdog with `RUST_MIN_STACK=16777216`.

## Compound assignment verification

- The first prepared snapshot passed five exact document tests using the owned libtest binary, exit 0 (`m2a-first-run`).
- `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <nine focused document tests>` passed all 9, exit 0, with verified cleanup and output drain (`m2a-final-run/result.json`, `stderr.log`). The batch covers all six operators, grouped and lazy right-hand sides, conversions, prior-only binding, failure propagation, byte-exact CRLF and Unicode, word operators, fixed-width exact literals and overflow, vectors and quantities. The private expression uses the existing explicit operation's nesting limit.

## Loop and session verification

- The first prepared snapshot passed eight exact domain tests using the owned libtest binary (`m2b-first-run`, exit 0). The editor test filter was corrected to the editor module nested under the dialog for the final batch.
- `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <eleven focused loop, session, comparison, document and editor tests>` passed all 11, exit 0 (`m2b-final-run/result.json`, `stderr.log`). Cleanup and output drain verified.
- Reviewed the cumulative change independently. Two findings were addressed before the final batch. Compound statements are rejected in a loop's iterable expression. Compounds whose only possible prior is conditional on loop execution use runtime binding and forward-reference diagnostics. Their speculative dependency edges cannot create a static cycle. The focused regressions also cover rollback, cancellation, exact arithmetic, frozen comparison rows and final bindings, literal input exclusion, completion order, and byte-exact named save and reopen.

## Final verification and native smoke

- `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <34 focused tests>` passed all 34, exit 0, with verified cleanup and output drain and a 100-second deadline. The exact selector and results are in `m3a-final-run/result.json` and `stderr.log`.
- `rustfmt --edition 2024 --config skip_children=true --emit stdout <changed Rust file>` parsed all eight changed Rust files. `git diff --check af58c63c..HEAD` passed. Evidence in `m3a-format.log`. Compiler warnings in pre-existing code are logged in the build and test receipts. The independent reviewer rechecked the two corrected findings and reported no remaining blockers.
- `cargo build -p multi_launcher --bin multi_launcher --profile fast-dev` exited 0 at source `25c7ba25c975f1d4621f07c05b4b958022441123`. The containment wrapper recorded descendants at root exit and verified cleanup and output drain. Evidence in `m3a-native-build/result.json`. SHA-256 of `target/fast-dev/multi_launcher.exe` is `AE1A73A7455768945B6F924C8152EC605AE04FBF5F358083A0CF10A3BB1D623D`.
- The isolated profile specifies a 620-point docked Pad and disables both Paste Sequences shortcuts (`native/profile.json`). The sandbox launch ran, but its window was absent from the capture surface. The interactive launch exposed the expected window. Initial capture and one refreshed retry both timed out with `window capture timed out: timed out waiting on channel`. Both owned processes closed and were verified absent. See `native/launch.json`, `native/attempt-2-launch.json`, and the corresponding `attempt-1-cleanup.json` and `attempt-2-cleanup.json`. There are no screenshots because the initial capture failed. The smoke ended before the first UI input.

| Windows scenario | Status | Planned input and expected state |
| --- | --- | --- |
| Compact docked and popped-out Config Menu | UNVERIFIED | Open Pad, activate the document, display, tools, Copy all and Dock/Pop Out actions and check reachability and state. |
| Results context actions | UNVERIFIED | Enter `a = 5\na * 4\n1 / 0`, right-click rows 2 and 3, check order, payload, navigation and action gating. |
| Scalar and bounded loop compounds | UNVERIFIED | Enter the operator-chain and range accumulator fixtures and verify final values 16 and 6. |
| Named save and reopen | UNVERIFIED | Save the loop fixture under a disposable name, reopen it and check source and results. |

The temporary diagram test fixture was removed after testing. Its SHA and absence are recorded in `temporary-fixture-cleanup.json`. A fresh test build at this branch still requires the base branch's moved fixture to be made available at its old path.
