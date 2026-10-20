# Jump Mode Results

Work in progress on `feat/jump-mode-2026-10-19-c7e42`, based on `8f94b003a12853735d1cb4689c6ec4e53c4ace8e`. Checkpoints through `898b733f` are pushed to origin. The default-off setting, pure hint engine, scoped registry, input routing and clipped overlay are implemented. Real native query hint/focus/typing proof passed with zero action execution and verified cleanup. Grid/list integration is verified; dashboard and Phase 2 surface integration remain pending.

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

- Initial rejected GUI run: `dev/jump_mode/evidence/jump_m01_20261019T210335Z_cd3ec49b/summary.md`. Latest accepted run is listed below.
- PNG: `G:\Repos\rust\Multi_Launcher\dev\jump_mode\evidence\jump_m01_20261019T210335Z_cd3ec49b\screenshots\00_baseline_dashboard.png` (900×560).
- Cleanup: same run's `process_lifecycle.json`; original capture receipt retained.
- **Baseline fixture-content FAIL:** screenshot shows an invalid-hotkey conflict banner and empty intended pinned results. A post-run correction explicitly overrides the automated receipt-only acceptance. Real capture and cleanup mechanics succeeded, but this is not an accepted populated-dashboard baseline.
- Fixture-only successful run: `dev/jump_mode/evidence/jump_m01_20261019T210025Z_5dcc013f/`; 32 stable local note records generated and typed validation succeeded.

## Acceptance inventory

| Surface / requirement | Current status |
|---|---|
| M00 branch/source ownership | Complete |
| M01 fixture/runner | Complete: production semantics, accepted actual baseline and owned cleanup |
| M01 populated dashboard baseline | PASS: 232227Z_fc7bc04b, explicit parent visual review |
| M02 setting | Complete: M02-A 3 tests + M02-B real save-owner regression passed |
| M03 pure hint engine | Complete; 10 targeted tests passed, c3549759 pushed |
| M04 registry | Complete; 24 all-Jump tests passed, scoped fmt/diff clean |
| M05 input and overlay | Input/painter verified (33 all-Jump tests); native evidence pending |
| M06–M09 grid/dashboard, keyboard harness and Phase 1 review | Pending |
| M10–M13 Settings, dialogs, complex widgets and extended evidence | Pending |
| M14–M15 performance, final checks and handoff | Pending |

No feature scenario, native keyboard scenario or performance claim is marked passed before execution. A normal-dashboard fixture baseline is curated; no native Jump interaction has been claimed yet.

See [execution ledger](JUMP_MODE_EXECUTION.md) for current ownership and next action.

## M02 checkpoint
Implemented default-off experimental_jump_mode_enabled setting, Launcher Window toggle, editor roundtrip and centralized runtime gate via established committed Save/Apply. cargo nextest run --lib -E 'test(jump_mode)' passed 3 tests (5808 skipped); touched-file rustfmt/diff checks passed. No Jump input interception yet.

## M03 checkpoint
Pure typed hint engine implemented in src/gui/jump_mode.rs. Target generation/surface identity, deterministic prefix-free A–Z leaf expansion, frozen sessions, case-insensitive matching, invalid-input preservation, Backspace/cancel and stale-target reconciliation covered. cargo nextest run --lib -E 'test(gui::jump_mode)' passed 10 tests (5811 filtered). No UI targeting or input interception is claimed yet.

## M04 checkpoint
App-owned frame registry/focus bridge implemented, with public opaque optional dashboard scope and instance/generation namespacing. Final-pass retention, one-shot epoch/generation intents, interactivity/clip/viewport/modal filtering, same-priority non-root ambiguity rejection, real button focus without clicks, logical selection without execution and disabled clearing are tested. Runtime flag has one owner. cargo nextest run --lib -E 'test(jump_mode)' passed 24 tests (5808 skipped); scoped formatting/diff passed. No keyboard interception or overlay yet.

## M02-B discovered persistence gap
The general Settings save coordinator uses define_general_settings_fields! in src/settings_save.rs. The new toggle was omitted, so the editor Save path would drop it despite M02-A serde/editor/direct runtime tests passing. This was repaired at the actual ownership boundary in `470c654a`, with focused apply/publication/rollback coverage. M02 is complete.

## Pre-input CPU baseline
On b151ff00660976aefd4d12b780f9fcefd7aa6dbb (M04), test profile, 100 inert actions, 3-column 960x640 root fixture, 9 warmups and 20 measured frames: mean 159.54 microseconds, median 163.1, nearest-rank p95 213.9, range 97.7–228.5. Timed run_root_list_frame only; output disposal outside measurement. Raw local log: dev/jump_mode/evidence/cpu-baseline/m05-a-root-list-baseline.log. CPU-only observation, no GPU/native claim; later identical disabled/idle/active measurements remain required.

M02-B correction verified: experimental_jump_mode_enabled is now a GENERAL_SETTINGS_FIELDS member. The exact test jump_mode_general_save_acknowledges_and_rolls_back_owned_setting passed (1 passed / 5833 skipped), exercising apply, publication overlay and rollback while preserving an unrelated theme write. The earlier gap is resolved; no input hooks were included in this repair.

## M05-A input checkpoint
The first real target is the root query editor. Early leader/hint capture precedes widgets, repeats and paired text are handled once, and capture survives discarded passes. Actual-root tests preserve query/caret, prevent a focused button from activating on the leader, and suppress same-frame modified Enter through active, completion and cancellation transitions. Focus/owner loss cancels and releases held guards. Paste/IME cannot mutate the query while hints own input.

Final cargo nextest run --lib -E 'test(jump_mode)': 29 passed / 5809 filtered, exit 0. Touched-file rustfmt and diff checks pass. No visual overlay or native Jump keyboard scenario is claimed yet. Bounded input review found activation and held-key defects; all were repaired with focused regressions.
## M05-B painter checkpoint
Current-pass clipped badges, accepted-prefix styling, active/invalid/no-target feedback and real-focus destination outline are implemented in jump_mode/paint.rs. Frozen IDs cannot borrow stale-generation geometry. Root pass finalization validates/reconciles before paint and QA capture; full-frame registry retention is unchanged. No widget focus or action behavior is owned by the painter.

cargo nextest run --lib -E 'test(jump_mode)': 33 passed / 5809 filtered, exit 0. Touched-file rustfmt --check and diff check pass. Actual native overlay screenshot remains required before M05 completion.
## M01-B accepted production baseline
Latest accepted summary: [232227Z_fc7bc04b](../../dev/jump_mode/evidence/jump_m01_20261019T232227Z_fc7bc04b/summary.md). Actual PNG: [normal dashboard](jump_mode_review/00_baseline_dashboard.png), 900x560, 171568 bytes. Parent inspected populated list/search cards and all four expected tag buckets without warnings. Explicit baseline_visual_review.json records the visual assessment; scenarios and manifest were advanced from automated content-BLOCKED to PASS based on that assessment, without modifying original capture receipt, pixels or lifecycle.

Fixture verification now invokes production load_notes, NotePlugin queries, typed pinned-query config and the pure complete hotkey-route validator. Three fixture tests pass; list/search each return32 stable IDs and four underscore tags each count8. Canonical notes/template paths are confined before environment update or plugin creation; child uses both ML_NOTES_DIR and ML_NOTE_TEMPLATES_DIR. Fixture-only unfocused throttling is disabled and CtrlAltF10 validates cleanly. Warning run232004Z_28069510 remains FAIL.

Owned run command: `pwsh -NoProfile -ExecutionPolicy Bypass -File dev/jump_mode/run.ps1 -CaptureTimeoutSeconds 45 -GracefulCloseSeconds 6`. Process23360 closed gracefully, exit0, owned_tree_gone, no forced cleanup. Scoped formatting, PowerShell/evidence parsing and diff checks pass. No Jump interaction is proved by this baseline.

All future test profiles now disable Paste Sequence Enable Next Shortcut and Enable Cancel Shortcut, per user steering: both optional hotkeys are None and the verifier rejects enabling either. The accepted baseline predates that instruction; its original artifacts are preserved. Normal application defaults remain unchanged.

Future-shortcut constraint verified by fixture-only run jump_m01_20261019T232417Z_6468abe4: 3 tests passed, generation/production semantic verification exit 0; both persisted shortcut fields are null. No native run was requested in this fixture-only check.
## M05-C native query proof
[Query hint screenshot](jump_mode_review/00_query_hints.png) and run `jump_m05_query_probe_20261020T000051Z_b5d58182` prove native Shift+Space, actual active map, map-derived A focus handoff, zero action activation, and normal X typing afterward. Retained receipt identifies pass9; completion independently reports actual query editor focus. PID40556 exited0 gracefully; owned tree verified gone. Parent inspected structured trace/cleanup and matching-hash actual pixels. Both test shortcuts persisted null; no warning/toast appears.

Earlier native runs remain FAIL: too-short2s owned-window readiness (no input), pre-handoff query-focus requirement (leader only), PowerShell automatic Matches overwrite (leader and real PNG, no hint). Corrected bounded condition poll, focus-stage contract and candidate variable at their owners; regression checks accompany the fixes. No action-card/dashboard/native activation coverage is claimed yet; M06 onward remains required.
Final Rust verification: `cargo nextest run --cargo-profile fast-dev --features dev-smoke-binaries,qa-framebuffer-capture --lib -E 'test(jump_mode_observer_recognizes_the_real_registered_query_text_entry_and_hint)'` passed1/5868skipped. Combined filter previously passed58 with this one setup failure; exact corrected test now passes. Four fixture tests, persisted PowerShell helper regression, production semantic validation, parser, scoped rustfmt and diff checks passed.
## M06 grid/list checkpoint
Visible actual list/grid responses now register result-generation/index identities, including equal-looking actions with distinct arguments. Hint completion changes the existing logical selection and keeps query focus/caret; no action is queued. FreshEnter retains existing action semantics.
`cargo nextest run --cargo-profile fast-dev --features qa-framebuffer-capture --lib -E 'test(jump_mode)'`:38passed/5834skipped. New actual-root tests prove list/grid selection, fullActionargs, zero execution at completion/sameframeEnter, exactly once afterfreshEnter, query/caret preservation, forceddiscard, clipped/offscreen exclusion, generation cancellation and popupbackground exclusion. Scopedrustfmt/diff pass. M08 provides real note-list/native catalog evidence.
