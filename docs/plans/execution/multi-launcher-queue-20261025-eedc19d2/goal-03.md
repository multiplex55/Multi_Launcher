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
