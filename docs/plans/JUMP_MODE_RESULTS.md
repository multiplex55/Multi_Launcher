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
| M02 setting | Complete; 3 targeted tests passed, checkpoint f3215251 pushed |
| M03–M05 engine, registry, input and overlay | In progress / pending |
| M06–M09 grid/dashboard, keyboard harness and Phase 1 review | Pending |
| M10–M13 Settings, dialogs, complex widgets and extended evidence | Pending |
| M14–M15 performance, final checks and handoff | Pending |

No feature scenario, native keyboard scenario or performance claim is marked passed before execution. No curated PNG has been published as accepted feature evidence.

See [execution ledger](JUMP_MODE_EXECUTION.md) for current ownership and next action.

## M02 checkpoint
Implemented default-off experimental_jump_mode_enabled setting, Launcher Window toggle, editor roundtrip and centralized runtime gate via established committed Save/Apply. cargo nextest run --lib -E 'test(jump_mode)' passed 3 tests (5808 skipped); touched-file rustfmt/diff checks passed. No Jump input interception yet.

## M03 checkpoint
Pure typed hint engine implemented in src/gui/jump_mode.rs. Target generation/surface identity, deterministic prefix-free A–Z leaf expansion, frozen sessions, case-insensitive matching, invalid-input preservation, Backspace/cancel and stale-target reconciliation covered. cargo nextest run --lib -E 'test(gui::jump_mode)' passed 10 tests (5811 filtered). No UI targeting or input interception is claimed yet.
