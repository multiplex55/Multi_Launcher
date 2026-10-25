# Goal 03 — Compile Dependency Boundaries

## M1-A checkpoint: dependency graph and controlled compile baselines

**Status:** M1-A complete. The parent accepted a scoped no-change disposition under Goal 03 Section F and the M1-A acceptance criteria: no capability reduction or version convergence was justified by the bounded graph and consumer evidence. Cargo.toml and Cargo.lock remain byte-for-byte unchanged. This checkpoint establishes reproducible baseline work; it does not claim a compile speedup or complete Goal 03.

### Environment and provenance

The measurements used baseline commit 97066a837497004e71a3f3910304b58d053ff612 on branch codex/perf-compile-boundaries-queue-20261025, Windows x86_64-pc-windows-msvc, Rust/Cargo 1.99.1, the fast-dev profile, default features, and the default target directory. No environment overrides were recorded. The existing checked Track B owner, dev/track_b/measure.ps1, captured each command with an 1800-second compile limit, a 5-second cleanup limit, and the same verified-msvc-vctip auxiliary-process policy.

The provenance record also captures local patch-tree hashes for vendor/eframe, vendor/egui, vendor/epaint, vendor/image, and vendor/egui-toast. No patched crate or toolchain was changed.

### Dependency disposition

The active Windows graph contains two versions with separate consumers. The app uses windows 0.58.0 for its native Windows API surface. windows 0.62.2 is required through accesskit_windows, accesskit_winit, egui-winit, and eframe. The reverse-dependency views therefore do not support converging these versions without changing an upstream API boundary or native behavior. No individual Windows capability was established as safely removable in this bounded pass, so the active feature set remains unchanged.

The image graph also has distinct consumers. image 0.24.9 is used by the app and screenshots 0.8.10. The locally patched image 0.25.6 is used by arboard, eframe, egui_extras, and the app. These paths support retaining both versions to preserve the current screenshot, clipboard, and GUI image behavior. No major-version convergence or broad upgrade was selected.

The graph inventory used cargo metadata --locked --no-deps --format-version 1; cargo tree --locked --target x86_64-pc-windows-msvc -e features and -d; app-scoped normal/build graph views; and reverse views for windows 0.58.0, windows 0.62.2, image 0.24.9, and image 0.25.6. Graph size and duplicate counts were treated as inventory, not compile-time evidence.

### Controlled scenarios

Each scenario used one temporary controlled source edit, one actual target command, and exact original-byte restoration. The app fixtures made behavior-preserving local-binding changes; the test fixture added only an assertion failure diagnostic. The complete measurement receipts, stdout/stderr, provenance, and fixture diffs are retained under:

C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g03-m1a

| Scenario | Controlled edit and target | Cache/artifact binding | Native process wall | Cargo Finished output |
| --- | --- | --- | ---: | --- |
| g03-m1a-small-appleafbodyedit | Split the local calculation in table_utility_session::source_is_inline_editable; app build | Warmed against the immediately prepared original-source app executable, SHA-256 17d1e120…1718d | 47.838126 s | 42.67 s |
| g03-m1a-selected-unittesteditcompile | Add a diagnostic to the assertion in table_parse_bom_unicode_and_whitespace_are_exact; selected lib test-target compile through Nextest binary listing | Warmed against the prepared original-source lib-test executable, SHA-256 b97f6cc9…ab4e4 | 68.480312 s | 1m 02s |
| g03-m1a-large-ownerbodyedit | Bind the existing expression to a local in gui/render::root_selectable_frame_margin; app build | Warmed against the immediately prepared original-source app executable, SHA-256 3579c6db…8db2e | 42.583938 s | 37.39 s |

The temporary fixture and final target-output hashes are preserved with their measurement receipts:

- Leaf scenario: diff SHA-256 5b38b47926ec018e959872bdec16ae5f55b45853c8795090dbda9a4006a43d2d; edited source SHA-256 61be28b6080b85a1a033d0e56772d000966ec7178a75c426a88b8028f41e9d97; app output SHA-256 5981e05d921ab21fd72eeebd14b944af393b1340be4f2012ab143d304a1d7bf3.
- Selected unit-test scenario: diff SHA-256 56793fe2fcadbfa769a3a459ed5c48714cdf5f343595c1747034fa86c5fdea13; edited source SHA-256 d9c6f91e0ea72f28f67aa4edd12c55ebea8e1f23bdef0075124e58e59077e371; lib-test output SHA-256 ebfab6a5ecbcc3a0e632e156264d8e3634a24102857edefabf3b1156a53e4ad3.
- Large-owner scenario: diff SHA-256 a30629df47abafad46f019213489b98ba60fc881eb9a1fc0074eb1445617bc99; edited source SHA-256 2b57bcb286fd34ad2fed8d0bacf28c99865ed090ad6262ed0f6f0558d05ac6de; app output SHA-256 48026d460cb3a77ff0d5a544603c8823bf4e7742c0990931f1d621f929803c06.

The app command for the first and third scenarios was:

cargo build -p multi_launcher --profile fast-dev --bin multi_launcher

The selected unit-target command was:

cargo nextest list --list-type binaries-only --message-format json --manifest-path G:\Repos\rust\Multi_Launcher\Cargo.toml -p multi_launcher --lib --cargo-profile fast-dev

That Nextest command listed the selected lib-test binary and compiled it; it did not execute test cases. No full suite, Clippy run, or native manual test was part of M1-A.

Three wrapper preflight stops are recorded separately: one before the leaf scenario and two before the large-owner scenario. All exited before a source write or Cargo invocation and are excluded from the three measured baselines.

The first app-target preparation completed in 156.953430 seconds with CacheState recorded as unknown; its Cargo output showed the app target compiling and finishing in 2m 31s. The selected lib-test target preparation completed in 78.929284 seconds and produced the original-source test executable. Its warmed classification reflects same-profile artifacts, not proof that an earlier exact-source test executable existed: the prior test binary predated the lint-only source corrections. After the leaf scenario changed the app executable, an exact-source app-target preparation was run before the large-owner scenario; it completed in 40.803319 seconds and produced the app executable bound to that scenario.

All three edited-source receipts succeeded with exit code 0, no timeout or runner error, verified cleanup and output drain, and the same verified helper-retirement disposition. Cargo timing reports were not configured. The native wall includes process startup and exit/lifecycle handling; the Cargo Finished line is retained as raw command output, not treated as isolated compiler or linker time. These three targets and edits are not a before/after speed comparison.

### Restoration and remaining artifact state

The original controlled sources were restored and verified against both the provenance snapshots and final hashes:

- src/table_utility_session.rs — 83,352 bytes, SHA-256 6ed3263edebfb4b140b04cb54a66bed17b238c01df0675203fb4efdeeac66d5f
- src/table_utility.rs — 111,718 bytes, SHA-256 ec670d9272091e5ffe9f126bb53b34f130003867c4945f08095667ed4b369419
- src/gui/render.rs — 2,108,423 bytes, SHA-256 61acf78b4fb3f28c06548e11211450e290381921fbfdd9014dff1f594129cd2f

Cargo.toml, Cargo.lock, and build.rs also match their recorded provenance hashes. All controlled source files have clean Git status; the only repository change is this report. No production code, dependency, or test file changed.

The app executable and lib-test executable under target were overwritten by the final temporary-source builds. Their source files are restored, but those outputs must not be treated as original-source artifacts. Any later measurement that depends on either target should first establish a fresh exact-source artifact binding.

### Driver record

The successful large-owner measurement used the preserved external wrapper run-large-ownerbodyedit.ps1; its SHA-256 is 37b4e43fcf2fb7c17a6e11ff8fdddea21b0cb48f1d437567fba2cd8485d9ffc2. Its successful receipt records driver PID 36184 (creation 2026-10-25T20:37:25.8857583Z), root Cargo PID 25744 (creation 2026-10-25T20:37:30.5186237Z), unified exec session 3910, and verified restoration.

Review found a future-wrapper safety improvement: the preserved wrapper’s source write occurs before its measurement try/finally, leaving intervening diff/hash/evidence writes outside that restoration guard. This successful run did reach the restoration finally and verified the original bytes. The immutable used wrapper was retained, and a separate parse-checked, unexecuted future-safe copy was saved at run-large-owner-baseline-future-safe.ps1 (SHA-256 b294147792bb917d9978f42c209e2467ba189c4c3246b807c453532c36f48679). That copy starts an outer try/finally before the first source write and restores the original bytes on any later error. No additional build or test was run for this wrapper review.

## M1-B checkpoint: consolidate the mock hotkey test batch

**Status:** M1-B is complete and accepted after a separate structured parent review; configured Reviewer dispatch was unavailable. The existing five pure cases and five mock hotkey/quit cases now share the `pure` integration-test target. This is a test-target ownership change only; production code, dependencies, features, Cargo manifest entries, and test behavior are unchanged. It does not complete Goal 03.

### Migration and target ownership

Moved `tests/hotkey_events.rs` and `tests/quit_hotkey.rs` to `tests/pure_cases/` and included them from `tests/suites/pure.rs`. The suite owns one private `MockCtx` module, and both moved modules import it through `super::mock_ctx`; the support implementation and its other users are unchanged. All four hotkey cases, the quit-loop case, and their assertions remain present and nonignored. The existing five pure cases are also included in the baseline and candidate selections.

The target map in `dev/README.md` now uses `--test pure` filters for `hotkey_events::` and `quit_hotkey::`. A metadata-only `cargo metadata --locked --no-deps --format-version 1` inventory reports 65 test targets in the package, keeps `pure` at `tests/suites/pure.rs`, and has no separate `hotkey_events` or `quit_hotkey` test target. The selected-group inventory therefore changes from three binaries to one. No claim is made that old executable files were physically deleted from `target`.

The moved case identities are:

| Old target / case | New `pure` case |
| --- | --- |
| `hotkey_events` / `launcher_and_quit_hotkeys_toggle_flags` | `hotkey_events::launcher_and_quit_hotkeys_toggle_flags` |
| `hotkey_events` / `zero_key_events_toggle_visibility` | `hotkey_events::zero_key_events_toggle_visibility` |
| `hotkey_events` / `shift_capslock_does_not_trigger` | `hotkey_events::shift_capslock_does_not_trigger` |
| `hotkey_events` / `win_modifier_triggers` | `hotkey_events::win_modifier_triggers` |
| `quit_hotkey` / `quit_hotkey_exits_main_loop` | `quit_hotkey::quit_hotkey_exits_main_loop` |

The source-backed compatibility review remains applicable: hotkey events are supplied directly to local trigger state, visibility uses its queue-only mock path, and the quit worker is signalled and joined before assertions. These tests do not start native hooks or use input injection, foreground-window ownership, shared files, or process-global mutable fixtures. The normal Nextest concurrency and default profile were retained.

### Controlled selected-group comparison

Both sides used the accepted M1-A HEAD on Windows x86_64-pc-windows-msvc, Rust/Cargo 1.99.1, the `fast-dev` Cargo profile, default features, and the ordinary Nextest profile/concurrency. To compare the same work, both selected groups contained the five existing pure cases plus the same five hotkey/quit cases. The controlled source edit added the same assertion diagnostic messages to one trigger assertion and one quit-worker assertion; the original source bytes were restored afterward.

| Scenario | Selected targets / prepared binaries | Edited compile outputs whose hashes changed | Compile process wall | Inventory process wall | Ten-case run process wall |
| --- | --- | --- | ---: | ---: | ---: |
| Baseline | `pure`, `hotkey_events`, `quit_hotkey` / 3 | 2 (`hotkey_events`, `quit_hotkey`) | 11.829834 s | 1.310770 s | 0.786528 s |
| Candidate | `pure` / 1 | 1 (`pure`) | 11.921350 s | 1.104362 s | 0.799487 s |

The inventories on both sides selected exactly ten nonignored cases, once each. The baseline run reported `10 tests run: 10 passed, 0 skipped` across three binaries; the candidate reported the same result across one binary. Whole-command run wall differs by 0.012959 s (about 1.6%); Nextest's own test-summary duration was 0.048 s baseline and 0.049 s candidate. This is no material execution regression for this single bounded pair. The compile process wall was 0.091516 s higher on the candidate, so the measurement shows no elapsed compile-time speedup. The evidenced compile-work change is the selected binary/output reduction from three/two edited executables to one/one. These are native process wall measurements, not linker durations.

The original-target preparation selected three test targets and took 170.385264 s; the candidate exact-source preparation selected only `pure` and took 13.134966 s. These preparations have different target sets and are outside the controlled edited-source interval, so their durations are recorded but are not compared as a performance result. The candidate preparation bound the one `pure` executable (SHA-256 `cc6c6e3806e5815ea7393e845abf5154e1f4903cb9452e866fe6b0625762aa1a`, 9,414,656 bytes) to hashes for the suite, all four `pure_cases` modules, and `tests/support/mock_ctx.rs`. The controlled candidate compile then changed the `pure` executable to SHA-256 `0c22ec7259188d32db72567a3624c37b3904476dae999f74174048715da68ea2` for the two diagnostic messages. The source files were restored exactly; that output remains fixture-built and must not be treated as an executable bound to the restored source. A later measurement that needs an exact-source candidate artifact must prepare one first.

The immutable controlled receipts, raw Nextest metadata/output, binary hashes, diffs, and source-restoration records are under:

`C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g03-m1b`

The main result records are `baseline-controlled-roster-retry1/baseline-controlled-result.json`, `candidate-controlled-roster-retry1/candidate-controlled-result.json`, `candidate-preparation/candidate-prepared-input-binding.json`, and `target-discovery-checked/candidate-target-discovery-summary.json`. The checked metadata-only discovery receipt took 0.098255 s and invoked no build or test.

### Restoration and driver history

The controlled baseline restored the original top-level files byte-for-byte: `tests/hotkey_events.rs` returned to 3,518 bytes / SHA-256 `8e1c17b328a25054f068d0a83cc529edc2132de92bac88052f907ebc986fafe3`, and `tests/quit_hotkey.rs` to 2,001 bytes / SHA-256 `1ca47aa5fbbc3a58e0ee7d83a67ef1f261ebe22992df7ce26c9697d5d9e6d530`. After the permanent move, the candidate fixture backups were 3,477 bytes / SHA-256 `ee83c02a58508e17739bda9ba10791b8adf955d7940022bf9a2402582fa42573` and 1,960 bytes / SHA-256 `ca41cadb34156cf30d696cb62e4041d5f200dcb01a1106dd4952bf3716bcb437`; both were restored to those exact bytes. The candidate wrapper also verified that the entire permanent migration working-tree status matched its pre-fixture snapshot.

The first baseline controlled-driver attempt (unified exec session 85192) failed before a measured command started: a PowerShell byte-array collection conversion caused its restoration path to write truncated source bytes. The failed driver and receipt are retained unchanged. The parent recovered the two files from the filtered committed blobs only after verifying each recovered hash and length, with no Git reset, checkout, or history operation. The corrected baseline driver then completed compile, inventory, and run successfully. One candidate-preparation preflight receipt also records an initial incorrect Stage classification (`provenance` instead of `compile`); it has `ProcessStarted: false` and was corrected before the successful one-target preparation. Neither failed wrapper attempt is included in the controlled timing comparison.

No full suite, native input test, Clippy run, cache reset, or follow-up benchmark repeat was performed. The scoped results meet the M1-B selected-target, exact-case, restoration, target-reduction, and execution-parity criteria; later Goal 03 checkpoints remain outside this report.

## M2-A checkpoint: extract the pure table core

**Status: COMPLETE / ACCEPTED.** The explicit user fixture-only renewal superseded the earlier exhausted allowance. The first renewed focused contract run passed; all original mandatory M2-A criteria now have scoped evidence. A separate structured parent review accepted the checkpoint under the existing configured-Reviewer dispatch fallback; this is not independent review. Goal 03 remains incomplete until M2-B's app-edit comparison and final Clippy gate.

### Ownership change

Moved the one pure table implementation and its existing 23 inline tests into the internal `crates/multi_launcher_table_core` package. The package has only `csv` and `serde_json` dependencies and is `publish = false`. The root package now depends on it by path and no longer declares a direct `csv` dependency. `src/table_utility.rs` remains the public application module and re-exports the core API; application callers retain the same types and functions. The root workspace and default members contain exactly `multi_launcher` and `multi_launcher_table_core`; no session, dialog, plugin, or UI ownership moved.

The `tables` catalog selection now names the core lib target, its moved exact test, and `crates/multi_launcher_table_core/src/lib.rs`. Fast-dev test preparation accepts only the fixed root/core package set, while inventory and execution remain metadata-driven. Mixed library discovery prepares both packages and records both identities without claiming one package for the aggregate. Source fingerprints include the core implementation and manifest. Existing selection names and per-selection receipts are retained. The README records the facade, package command, workspace defaults, and target-preparation/inventory deadlines.

Cargo metadata reports both packages as workspace members and default members. Original-source preparation produced the root and core library test binaries. `cargo tree --locked -p multi_launcher_table_core -e normal` contains no `eframe`, `egui`, `windows`, or `winapi` packages. The 23 core cases remain nonignored and cover parsing, CSV policy/export, limits, projection, cancellation, and source-bound selections.

### Compile and behavior evidence

The controlled compile used the same one-line assertion-diagnostic fixture as M1-A's `table_parse_bom_unicode_and_whitespace_are_exact` case. Core preparation was outside the interval. The original prepared core lib-test executable was bound by identity, path, source, manifests, lockfile, SHA-256, and byte length before the guarded edit; the source was restored in an outer `finally` and verified byte-exact afterward.

| Item | Result |
| --- | --- |
| M1-A root lib-test compile baseline | 68.480312 s; original test executable SHA-256 `b97f6cc9…ab4e4` |
| M2-A core lib-test diagnostic compile | 7.176044 s; 1,420,800-byte output SHA-256 `0c5dff1d279f3a0fd963cec2c972c286388d0245a382861ba0bddd3255bfab14` |
| Core original-source prepared executable | 1,420,800 bytes; SHA-256 `6c3b5768977cd102848d9e7527dffe26cfe793d0b32b948b7c968865fee61d09` |
| Restored core source | 111,718 bytes; SHA-256 `ec670d9272091e5ffe9f126bb53b34f130003867c4945f08095667ed4b369419` |
| M2-A core inventory and run | Exactly 23 nonignored cases; 23 passed, 0 skipped, from the same diagnostic-built executable after source restoration |

This is about an 89.5% lower compile wall for the independent core lib-test target than the M1-A root lib-test baseline. The target intentionally changed to the extracted package; this is not an app-build speed claim and does not bind the edited executable to restored source.

The root lib-test target was prepared once (229.213874 s) and its prepared metadata selected exactly the two existing `table_utility_session` cases: `table_session_latest_source_wins_across_close_and_reopen` and `table_session_stale_parse_cannot_accept_an_inline_edit`. Both passed; 6,323 other root lib cases were filtered. The first run command stopped before test execution because Nextest's experimental JSON output mode requires `NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1`; that failed receipt is preserved and the same two cases then passed with the required scoped override.

The actual `tables` workflow `Run` passed its one exact parser case and published its Nextest recording. Its receipt identifies the `multi_launcher_table_core` package and binary, with kind `lib`. The actual library `Discover` passed all five catalog library selections: `tables` resolves to the core binary, while `notes`, `paste`, `query-focus`, and `settings` resolve to the root binary. Its combined prepared metadata contains exactly the core and root lib binaries. Discovery's aggregate package and target fields are null, with the package list carrying both identities. This intentionally does not execute the unrelated selected cases.

### Historical workflow-contract blocker and authorized resolution

The four focused contract invocations and receipts are retained under `g03-m2a/workflow-contracts` in the external evidence directory below. The first two failures were repaired within the existing owner: explicit null/count handling for an absent fast-dev target selector, then a case-sensitive selector assertion so Cargo `-p` is not confused with Nextest `-P`. The third failure was an unnecessary root-first ordering assumption in the new test; with parent approval, only that assertion was adjusted to compare the exact sorted two-package set while preserving duplicate/cardinality and per-selection identity checks. Production catalog order remains core-first. The single adjusted run still failed:

| Attempt | Owner PID / creation time | Outcome |
| --- | --- | --- |
| `g03-m2a-workflow-contracts_20261025T213124715Z_0b286d4664db47c1b8c0b0523bee28dc` | 27460 / 21:31:24.9736592Z | `cargo.ps1:176` precondition rejected an invocation because an absent package list was counted as one element; corrected with the scoped null/count guard. |
| `g03-m2a-workflow-contracts_20261025T213201595Z_d8ebef9a0f0146af862cae64725a5f11` | 27000 / 21:32:01.8234859Z | `test_run.ps1:234` treated Nextest `-P` as Cargo `-p`; corrected to a case-sensitive selector check. |
| `g03-m2a-workflow-contracts_20261025T213306604Z_d8aee00db09247da97a5a9b8e6111ea2` | 36104 / 21:33:06.8372751Z | New assertion expected root-first ordering; parent approved the exact sorted-set assertion adjustment, with production ordering unchanged. |
| `g03-m2a-workflow-contracts-adjusted_20261025T213616974Z_7e5b1491013540819752abef68fc79a8` | 20832 / 21:36:17.2124391Z | `test_run.ps1:642` failed with `Cannot index into a null array.` |

Read-only diagnosis confirmed that `ConvertFrom-Json -AsHashtable` creates case-sensitive ordered hashtables. The receipt writes `SelectedIdentity` with `PackageName`, `BinaryName`, and `TestName`, while the failing fixture uses the lowercase member `selectedIdentity` and lowercase property labels (`packageName`, `binaryName`, `testName`). The exact minimal future fixture correction is to index the actual case-sensitive `SelectedIdentity` key and mutate those exact PascalCase identity keys; preserve each stale-identity rejection. The parent exhausted this contract boundary after the two repairs and one scope adjustment. At that historical stop, no further contract edits or reruns were authorized and the self-test had not passed. The explicit user renewal and successful resolution below supersede that stop.

Configured independent Reviewer dispatch failed once at the agent thread limit; no retry or replacement was launched. The authorized parent fallback performed the separate structured acceptance review after the configured Implementer completed the renewed correction. Only the fixture changed during renewal; the other ten preserved task files matched their prior hashes, so successful core, root-session, graph, and actual-workflow evidence remains valid without repetition.

The fixture indexes the serialized `SelectedIdentity` container and exact `PackageName`, `BinaryName`, and `TestName` keys. It verifies each key already exists, mutates its intended stale value, writes the altered receipt, reads it back, checks unchanged key cardinality and exact persisted value, and retains each `STALE_DISCOVERY` rejection assertion. This prevents a false pass from adding differently cased duplicate keys.

The one renewed checked invocation of `dev/workflows/test_run.ps1` passed with exit 0 in 19.122982 seconds, under the authorized 600-second execution / five-second cleanup deadlines. Receipt: `fixture-renewal/g03-m2a-workflow-contracts-fixture-renewal_20261025T223312415Z_4dbe885984464227bd336513e509df72/measurement.json`. Stdout was 91 bytes and stderr zero; actual file lengths, bytes read, and bytes recorded matched. Cleanup and output drain were verified, with no timeout or runner/recording/restoration error. The parent independently queried all 15 recorded PID-plus-creation identities with a bounded CIM query: all were absent. No second correction or rerun was needed. All earlier failures and receipts remain preserved. No passing benchmarks/tests, core preparation, or native campaign was repeated.

### M2-A evidence and source bindings

The external evidence root is:

`C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g03-m2a`

Key evidence paths beneath it are `metadata/g03-m2a-workspace-metadata_20261025T213348302Z_0bfb4fcdc3e042999dee23b5d7095790/measurement.json`, `graph/g03-m2a-core-locked-graph_20261025T213732223Z_cf415880d0c44ab7996b364081e4d3ee/measurement.json`, `core-preparation/g03-m2a-core-original-preparation_20261025T213400728Z_312e4f7441f24c17b5c39b072b97022d/measurement.json`, `root-session/g03-m2a-root-session-preparation_20261025T213750113Z_cf7de0f663f14975bd9da40e5ce45c4b/measurement.json`, `core-selected-unittesteditcompile.driver-result.json`, `core-diagnostic-tests/`, `root-session/`, `workflow-tables-run/`, and `workflow-library-discover/`. All command receipts retain owned process identities, cleanup and output-drain results.

Principal source hashes at the end of this checkpoint are: `crates/multi_launcher_table_core/src/lib.rs` `ec670d9272091e5ffe9f126bb53b34f130003867c4945f08095667ed4b369419`; `crates/multi_launcher_table_core/Cargo.toml` `be8c5d3f6e3c75551aca9dc4485f20293237dcad2f9c9c99f256147f1fd94319`; `src/table_utility.rs` facade `b1524ebb2ee16daa1444742bb9a77572cbf664fc7f34e755107421e25183ed7b`; root `Cargo.toml` `9c8bd78bc59ceea6662a274400e420af5ed00a67332bbed14a81f59c4472f32f`; `Cargo.lock` `3eef4cc515234d5316e623855cbf4906d3f0da208890dc6cf1b4de451ad68320`; `dev/workflows/catalog.json` `7927081d2c662f8721809e04980bcc57b257ea49ad3b18a7b9c3d88f610128e5`; `dev/workflows/engine.ps1` `2324eaad8bfd5d5a0fff97ebd2909079773ca296c2727201c353c05be2ffe4d4`; `dev/workflows/test_run.ps1` `57398f9544aa91049df971059b12600190ad00ed2088b36d845918f15ae53343`; and `dev/track_b/cargo.ps1` `cdb278bf8f58a8e9642872dc6e33d1f2b7345823390722c3e1ae09af678f418a`.
