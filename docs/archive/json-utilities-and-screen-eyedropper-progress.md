# Goal A execution record

Active plan: [JSON utilities and screen eyedropper](json-utilities-and-screen-eyedropper.md).
Implementation branch: `json-and-eye-dropper`. Initial working tree was clean.

| Checkpoint | Status | Evidence |
| --- | --- | --- |
| M1-A shared JSON transformations | complete | Shared ordered parser; strict JSON and CM compatibility tests pass. |
| M1-B JSON commands | complete | Typed intents, plugin registration, routing and tracked dialog; 13 focused tests pass. |
| M1-C JSON utility | complete | Valid-only initialization, editable buffer, transforms, errors and explicit copy; 5 state tests pass. |
| M2-A shared color conversion | complete | Pure RGB/HEX/HSL owner reused by plugin; 7 focused tests pass. |
| M2-B frozen desktop picker | complete | Signed frozen model, native runtime, generalized parking; 21 focused tests pass. |
| M2-C launcher integration | complete | Typed picker command; revision-aware ROOT ownership/restore; 29 scoped tests pass. |
| M3-A qualification and documentation | complete | Architecture guards/build pass; README updated; native attempt limited by UI capture/discovery. |
| Independent review | complete | Cumulative review through `82dcb13e`; no substantive findings. |

## Decisions

- Preserve JSON object ordering locally; do not change global `serde_json` features.
- Reuse the MkMacro virtual-desktop compositor and generalize existing capture-safe parking narrowly.
- Sequential source writers; checkpoint commits owned by the orchestrator.
- Preserve existing Clipboard Modify operation IDs and color result generation.

## Verification

A full repository suite is not required by this goal.

- M1-A: targeted library Nextest run passed 8 tests covering `json_transform::` and operation ID serialization; exact Clipboard Modify JSON parity test passed separately (1 test). Formatting and diff whitespace checks passed.
- M1-B: `cargo nextest run --lib json_utility` (11 passed); `cargo nextest run --lib tracked_openable_panels_count_as_any_panel_open shared_catalog_handle_preserved_across_reload` (2 passed). Formatting and diff whitespace checks passed.
- M1-C: targeted `gui::json_utility_dialog` state tests (5 passed); formatting and diff whitespace checks passed. Shared JSON/CM transformation code unchanged.
- M2-A: `cargo nextest run --lib -E 'test(color::tests::) | test(plugins::color_picker::tests::)'` (7 passed); formatting and scoped diff whitespace checks passed.
- M2-B: `cargo nextest run --lib -E 'test(color_pick::) | test(launcher_parking::tests::)'` (21 passed on final source: 9 picker/runtime, 12 existing parking tests). Windows FFI compiled; formatting and diff whitespace checks passed. Capture timeout/cancel retires blocked capture without allowing stale results to open an overlay; native completion waits teardown.
- M2-C: final scoped Nextest run passed 29 tests (13 command/lifecycle, 9 color runtime/model, 4 color plugin, 3 Screen Draw recovery). Formatting and diff whitespace checks passed. Tests cover exact restore, ordered activation, teardown suppression, newer show/hide intent, retry after restore failure, duplicate activation and bidirectional ownership conflicts.
- M3-A: `cargo nextest run --test domain -E 'test(command_bus_architecture::)'` (3 passed): exhaustive typed routing, canonical activation parser and protocol-free handlers.
- M3-A: `cargo build --bin multi_launcher` passed on Windows. Final formatting/diff checks passed. No full repository suite was run.

## Independent review

The independent reviewer inspected the cumulative implementation through `82dcb13e`: shared JSON/CM behavior, typed routing, dialog state, color conversion parity, signed frozen pixel sampling, DPI/native resource cleanup, parking migration and ROOT restoration races. No substantive findings or stale competing paths were identified. No remediation commit was necessary.

## Native verification limitation

Attempted a small Windows sanity pass with an isolated settings/data directory under `target/goal-a-native-smoke`, fixed launcher geometry and only the relevant plugins enabled. The built application started and remained responsive. Computer Use window screenshot capture failed with `FrameArrived timed out`; the required reselect/activation retry also timed out. Accessibility text remained available for the Explorer anchor, but neither configured hotkey attempt exposed the launcher in the UI tool's window inventory. The isolated process was stopped without changing normal launcher settings/history.

Consequently native pixel selection, Escape, repeated opening, focus restoration and actual multi-monitor/DPI interaction were **not verified** in this environment. They remain a manual follow-up on an interactive desktop. Signed coordinates, frozen sampling, magnifier edges, cancellation/teardown, exact restoration and newer show/hide intent have targeted automated coverage. This limitation does not represent a passing native smoke test.
