# Jump Mode Results

Work in progress on `feat/jump-mode-2026-10-19-c7e42`, based on `8f94b003a12853735d1cb4689c6ec4e53c4ace8e`. M00 checkpoint `03e42b69` was pushed to origin. No Jump Mode runtime behavior is implemented yet.

## Architecture and current inventory

- Existing cwd-based AppDataRoot and single-instance identity isolate the fixture without changing production startup.
- `dev/jump_mode/run.ps1` generates a fresh profile, builds the actual app, requests the existing gated ROOT framebuffer capture and owns process shutdown.
- `dev/jump_mode/fixture_generator/src/main.rs` is a dev-only root-package binary, preserving the root lockfile/vendor patches. Typed fixture data uses an explicit notes-only plugin allowlist, empty executable actions/index/plugin directories, local note records and explicit ML_NOTES_DIR.
- ROOT framebuffer evidence reports CPU UI completion before GPU presentation. It is not claimed to prove presented-window pixels.
- One reviewed cleanup failure path was corrected: process inventory/logging failure no longer skips retained-handle root shutdown. Read-only targeted rereview confirmed repair.

## Recorded verification

| Command or check | Outcome |
|---|---|
| Initial Git status, branch, SHA and remotes | Clean master at recorded SHA; unique feature branch created |
| `pwsh -NoProfile -File dev/jump_mode/run.ps1 -FixturesOnly` | Original nested helper failed registry resolution; corrected root-package helper later passed 2/2 tests and typed generate/verify |
| `pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -CaptureTimeoutSeconds 45 -GracefulCloseSeconds 6` | Actual app built/launched/captured/closed; fixture-content acceptance FAIL |
| `Get-CimInstance` within normal sandbox | Access denied; escalated own-process identity lookup succeeded |
| Actual baseline process cleanup | PID 2128, graceful close, exit 0, verified owned tree gone |

The first nested helper did not inherit root patches/lock; it was replaced by a root-package development binary. One incorrect Rust import was corrected after compiler feedback. These were harness implementation failures, not missing GUI environment.

## Evidence

- Latest real GUI run: `G:\Repos\rust\Multi_Launcher\dev\jump_mode\evidence\jump_m01_20261019T210335Z_cd3ec49b\summary.md`.
- PNG: `G:\Repos\rust\Multi_Launcher\dev\jump_mode\evidence\jump_m01_20261019T210335Z_cd3ec49b\screenshots\00_baseline_dashboard.png` (900×560).
- Cleanup: same run's `process_lifecycle.json`; original capture receipt retained.
- **Baseline fixture-content FAIL:** screenshot shows an invalid-hotkey conflict banner and empty intended pinned results. A post-run correction explicitly overrides the automated receipt-only acceptance. Real capture and cleanup mechanics succeeded, but this is not an accepted populated-dashboard baseline.
- Fixture-only successful run: `dev/jump_mode/evidence/jump_m01_20261019T210025Z_5dcc013f/`; 32 stable local note records generated and typed validation succeeded.

## Acceptance inventory

| Surface / requirement | Current status |
|---|---|
| M00 branch/source ownership | Complete |
| M01 fixture/runner | Scaffold implemented, corrections under scoped verification |
| M01 populated dashboard baseline | Incomplete: correct fixture content/readiness still needs proof |
| M02 setting | Complete: M02-A 3 tests + M02-B real save-owner regression passed |
| M03 pure hint engine | Complete; 10 targeted tests passed, c3549759 pushed |
| M04 registry | Complete; 24 all-Jump tests passed, scoped fmt/diff clean |
| M05 input and overlay | Pending |
| M06–M09 grid/dashboard, keyboard harness and Phase 1 review | Pending |
| M10–M13 Settings, dialogs, complex widgets and extended evidence | Pending |
| M14–M15 performance, final checks and handoff | Pending |

No feature scenario, native keyboard scenario or performance claim is marked passed before execution. No curated PNG has been published as accepted feature evidence.

See [execution ledger](JUMP_MODE_EXECUTION.md) for current ownership and next action.

## M02 checkpoint
Implemented default-off experimental_jump_mode_enabled setting, Launcher Window toggle, editor roundtrip and centralized runtime gate via established committed Save/Apply. cargo nextest run --lib -E 'test(jump_mode)' passed 3 tests (5808 skipped); touched-file rustfmt/diff checks passed. No Jump input interception yet.

## M03 checkpoint
Pure typed hint engine implemented in src/gui/jump_mode.rs. Target generation/surface identity, deterministic prefix-free A–Z leaf expansion, frozen sessions, case-insensitive matching, invalid-input preservation, Backspace/cancel and stale-target reconciliation covered. cargo nextest run --lib -E 'test(gui::jump_mode)' passed 10 tests (5811 filtered). No UI targeting or input interception is claimed yet.

## M04 checkpoint
App-owned frame registry/focus bridge implemented, with public opaque optional dashboard scope and instance/generation namespacing. Final-pass retention, one-shot epoch/generation intents, interactivity/clip/viewport/modal filtering, same-priority non-root ambiguity rejection, real button focus without clicks, logical selection without execution and disabled clearing are tested. Runtime flag has one owner. cargo nextest run --lib -E 'test(jump_mode)' passed 24 tests (5808 skipped); scoped formatting/diff passed. No keyboard interception or overlay yet.

## M02-B discovered persistence gap
The general Settings save coordinator uses define_general_settings_fields! in src/settings_save.rs. The new toggle was omitted, so the editor Save path would drop it despite M02-A serde/editor/direct runtime tests passing. This is being repaired at the actual ownership boundary, with focused apply/publication/rollback coverage. M02 is not complete until that passes.

## Pre-input CPU baseline
On b151ff00660976aefd4d12b780f9fcefd7aa6dbb (M04), test profile, 100 inert actions, 3-column 960x640 root fixture, 9 warmups and 20 measured frames: mean 159.54 microseconds, median 163.1, nearest-rank p95 213.9, range 97.7–228.5. Timed run_root_list_frame only; output disposal outside measurement. Raw local log: dev/jump_mode/evidence/cpu-baseline/m05-a-root-list-baseline.log. CPU-only observation, no GPU/native claim; later identical disabled/idle/active measurements remain required.

M02-B correction verified: experimental_jump_mode_enabled is now a GENERAL_SETTINGS_FIELDS member. The exact test jump_mode_general_save_acknowledges_and_rolls_back_owned_setting passed (1 passed / 5833 skipped), exercising apply, publication overlay and rollback while preserving an unrelated theme write. The earlier gap is resolved; no input hooks were included in this repair.
