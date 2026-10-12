# M0-B test and interaction matrix

Status: **PARTIAL; fixture generation and the root-list Track C owner passed. Other Track C owners are still running, and no visual/native interaction passed. Two native launch attempts are documented below; neither produced a screenshot or valid native measured result.** This matrix separates automated/headless assertions from observations that require the actual Windows app.

## Automated and headless evidence

| Owner / behavior | Test target and command | State |
|---|---|---|
| Root-list geometry owner (cold/warm) | `cargo nextest run --success-output final --locked --offline --profile default --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(track_c_benchmark_root_list_frame_geometry)'` with `MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`, `ML_TRACK_C_BENCH_MODE=full`, `ML_TRACK_C_SOURCE_SHA=f45e3773`, `ML_TRACK_C_PROFILE=fast-dev` | `PASS`; one test covered six fixture cases. Raw samples are preserved in `target/egui-modernization/baseline/owner-list.log`; no aggregate performance conclusion is recorded here. |
| Root grid cold frame geometry | Same command, filter `test(track_c_benchmark_root_grid_frame_geometry)` | `IN PROGRESS`; currently executing in the parent-owned serial owner run. |
| Quick Notes geometry | Same command, filter `test(track_c_benchmark_quick_notes_dialog_geometry)` | `QUEUED` in the parent-owned serial owner run. |
| Query-to-usable CPU root frame | Same command, filter `test(track_c_benchmark_root_query_to_usable_cpu_frame)` | `QUEUED` in the parent-owned serial owner run. |
| Startup action catalog owner | Use the same env and `cargo nextest run --success-output final --locked --offline --profile default --cargo-profile fast-dev --bin multi_launcher --run-ignored ignored-only -E 'test(track_c_benchmark_startup_action_catalog_owner)'` | `QUEUED`; run separately from library owners. |
| Settings/action fixture serializer and load round-trip | `build_fixture_helper.ps1` compiled and ran `prepare_native_fixture.rs`; the generator checks `Settings::load`, re-reads typed actions and emits file hashes | `PASS` for fixture generation/round-trip only; manifests are preserved in `run-original-001` and `run-original-002`, each with 1,000 actions and 100 notes. This is not a native UI pass. |

Run each remaining Track C owner serially after the current owner process exits; do not run a broad `cargo nextest run` in parallel with a measurement. The root-list owner passed and its complete log is preserved at `target/egui-modernization/baseline/owner-list.log`; the grid owner is running, while notes, query and startup are queued. Preserve raw `TRACK_C_RESULT` lines and complete logs under the ignored run directory. Track C owners include their own five warmups and twenty samples; the startup owner must remain a separate test process from library owners.

The original default-environment baseline Nextest invocation failed two telemetry assertions with actual `None` vs expected `Some(1)`, stopped early, and skipped 41 tests. The no-fail-fast diagnostic then completed with 6,442 tests run, 6,439 passed, 3 failed, and 41 skipped in 134.496 seconds. The three failures were classified as stale test fixtures: two watch tests retained old telemetry/equality expectations; one plugin test expected 13 commands while the current built-in catalog contains 20 (13 original commands plus three halo, three zoom and one effects-off command). The focused test-only correction passed its four targeted tests; it did not rewrite the original full-suite result. Preserve the original logs and keep these findings separate from M0-B evidence and production-regression conclusions.

## Native/manual matrix

| Surface or behavior | Required observation | State |
|---|---|---|
| Process/profile isolation | Owned CWD/settings/actions, `ML_NOTES_DIR`, template path, unique single-instance root, one owned process | `NOT MEASURED`; manifests and process receipts document fixture setup and process identity, but UI-driven data isolation was not exercised |
| Root startup and usable display | First actual visible frame distinguished from CPU completion receipt | `NOT MEASURED`; run 002 root-window discovery returned, but capture approval and the `FrameArrived` retry timed out. Internal startup log values do not establish a displayed frame |
| Automated native hotkey invocation/focus | Supported sky Computer Use actions with a fresh observation before each action; focused/hidden show/hide and query focus | `NOT MEASURED`; run 001 F12 registration failed, run 002 logged successful `Ctrl+Alt+F10` registration but no chord or focus interaction was observed. Physical/hardware chord validation is tracked separately and remains `NOT RUN` |
| Search and list | Supported sky Computer Use actions with fresh observations; fixture query, result order, hover, selection, arrows, Enter on query-set action, Escape | `NOT RUN`; no input action was observed |
| Grid mode | Toggle, selected result identity, populated grid and return to list | `NOT RUN` |
| Settings/theme | Open/close, current mode, controls, focus and persisted fixture-only settings | `NOT RUN`; not covered by the root-only empty-plugin fixture unless reachable independently |
| Notes | Load generated Markdown, preview/edit and reflow; all writes remain in fixture notes | `NOT RUN`; notes/plugin scenarios require a separate explicit enabled-plugin fixture |
| File search | Open/filter/cancel using fixture root only | `NOT RUN`; requires a separate explicit plugin/surface fixture |
| MKMacro and prompt | Inspect ordinary editor or prompt only; do not run macro/capture/window picker | `NOT RUN`; requires a separate explicit plugin/surface fixture |
| QR/regex and secondary viewport | Open/close and focus if reachable without using external data | `NOT RUN`; requires a separate explicit plugin/surface fixture |
| Toast/error and dialogs | Observe layering and close behavior if generated naturally; no forced errors yet | `NOT RUN`; common plugin scenarios require a separate explicit plugin/surface fixture |
| Radial editor/runtime | Runtime, global item inputs and radial hooks disabled; editor is optional and must be opened without enabling them | `NOT RUN` |
| Screen Draw/MKMacro overlays and native canvas | Explicit native overlay exclusion; separate compatibility workflow | `NOT RUN` |
| Primary/secondary monitor and DPI | Record actual monitor, scale, position and viewport; no unsupported display claimed | `NOT RUN` |
| RDP/alternate OS/taskbar layouts | Record only actual available configuration | `NOT RUN` |
| Ordinary shutdown/resource cleanup | Wait for the retained exact process to exit before fixture cleanup | `NOT RUN`; run 001 was gracefully terminated by exact PID after its failed sandbox attempt, not through the ordinary app close path |

## Evidence format

Every row gets its own status and evidence path. A screenshot may prove a visible composition only; it does not prove latency, focus ownership, hotkey timing, renderer identity, texture lifetime or resource cleanup. Preserve failures and outliers. Use `NOT MEASURED` where an attempted scenario ran without a valid metric, and `NOT RUN` where the scenario was not attempted. Record supported sky-driven native smoke separately from physical/hardware validation; the exact physical Win-key chord remains `NOT RUN` unless physically exercised, and an automated key sequence does not count as hardware validation.
