# egui modernization checkpoint ledger

Source contract: [`MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md`](../MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md). Statuses here distinguish recorded preflight evidence from acceptance gates that have actually passed.

| Checkpoint | Status | Current evidence / exit gap |
|---|---|---|
| M0-A — repository and toolchain preflight | `complete` | The Windows MSVC `cargo check --locked --all-targets --offline` passed. Original Nextest diagnostics completed with three classified stale fixture failures; a separate focused test-only correction passed 4/4. Current HEAD is the objective's working baseline; comparison with the separately named archive remains unresolved. No remote builder was configured or supplied. |
| M0-B — golden interaction and render inventory | `in_progress` | Source inventory and isolated fixture prepared; all five full-mode original CPU owners passed (84 cases), independently reviewed. All three original profile builds passed and are preserved; revised helper compiled and generated a provenance-bound third fixture. Six native fixture runs are recorded: capture failed in runs001–005; run006 used accessibility only and passed prefixed local search plus synthetic Enter activation. Screenshots, display/GPU timing and broader native interactions remain unproven. |
| M0-C — dependency/API breakage worklist | `complete` | Planner source-backed worklist and handoffs independently reviewed against cached released sources; accepted. No dependency changes. M0-B native evidence remains outstanding. |
| M1-A — align Rust toolchain and egui dependency graph | `complete` | Independently reviewed graph: one egui 0.36.2 family, aligned wrappers/windowing, explicit Glow. All-target check reaches expected source API errors (621 library / 1,269 library-test); runnable acceptance remains pending M1-C/M2-D. |
| M1-B — app lifecycle migration | `in_progress` | Implementer owns the sequential lifecycle/startup change after reviewed local M1-A `5536feca`. Focus: hidden runtime progress, visible prompt input order, repeated-pass side effects and retained-paint receipts. Planner confirmed a native eframe hidden-child creation gap; a narrowly documented local eframe 0.36.2 patch is authorized at that integration boundary. Execution of focused tests may depend on M1-C closing whole-crate API errors. |
| M1-C — mechanical egui API compatibility | `pending` | Read-only planner packet prepared and persisted at end of implementation-handoffs.md; execution waits for the sequential M1-B source handoff and review. |
| M1-D — rich text, toasts, image and third-party compatibility | `pending` | Not started. |
| M2-A — geometry/cache migration | `pending` | Not started. |
| M2-B — Windows windowing, renderer and event integration | `pending` | Not started. |
| M2-C — renderer performance comparison and decision | `pending` | Not started. |
| M2-D — Track A final parity gate | `pending` | Not started. |
| M3-A — semantic theme model | `pending` | Not started. |
| M3-B — runtime theme application | `pending` | Not started. |
| M3-C — presets and transactional live theme preview | `pending` | Not started. |
| M3-D — typography, icons and lightweight rendering assets | `pending` | Not started. |
| M4-A — root launcher search field polish | `pending` | Not started. |
| M4-B — result list and grid visual polish | `pending` | Not started. |
| M4-C — UI primitives and standardized surfaces | `pending` | Not started. |
| M5-A — global settings shell and theme editor redesign | `pending` | Not started. |
| M5-B1..n — high-usage plugin dialogs | `pending` | Not started. |
| M5-C1..n — remaining dialogs, messages and secondary windows | `pending` | Not started. |
| M6-A — optional advanced effects as constrained opt-in | `pending` | Not started. |
| M6-B — final visual, accessibility and performance acceptance | `pending` | Not started. |

## M0-A checklist

- [x] Read repository instructions, the full execution contract, the attached goal objective, and the available historical benchmark references.
- [x] Record current branch/SHA, working-tree diff inventory, `Cargo.lock` digest, Rust/Cargo versions, and locked offline dependency-tree outputs.
- [x] Inventory the current egui family, direct `winit` and `raw-window-handle` dependencies/usages, active eframe feature flags, and Cargo patch/git dependency declarations.
- [ ] Reconcile HEAD with `multi_launcher(20261012-203406).zip`; the archive was not present in the attachment's recursive file inventory or repository search, so the comparison is unresolved. The mission objective's current-HEAD baseline is recorded and parent requested archive clarification asynchronously.
- [x] Record original Windows compile diagnostics: `cargo check --locked --all-targets --offline` passed, exit 0, dev profile, 4m11s; compiler warnings are summarized in [baseline.md](baseline.md).
- [x] Complete and classify original test diagnostics: the full no-fail-fast run completed with 6,442 run, 6,439 passed, 3 failed and 41 skipped. The focused fixture-correction run passed 4/4 without changing the original receipt. See [`nextest-status.txt`](receipts/nextest-status.txt) and [`regressions.md`](regressions.md).
- [x] Confirm remote builder applicability: no remote Windows builder is configured or supplied; the local `x86_64-pc-windows-msvc` host is the migration host.
- [x] Save local M0-A checkpoints: inventory `050a922f`, reviewed fixture correction `f45e3773` (four focused tests passed). Original failed full-suite receipts are retained. User directed local-only checkpoints. Archive comparison remains an explicit evidence limitation; current HEAD is the objective-authorized baseline.

See [baseline.md](baseline.md), [dependency-audit.md](dependency-audit.md), [migration-decisions.md](migration-decisions.md), and raw command outputs under `receipts/`.

## Active execution handoff

- Branch: `egui-modernization`; checkpoints stay local by explicit user instruction. Latest local commit: M1-B accepted vendor substep `dac00f8d`; M0-B native QA follow-up `713a5298`; latest fully accepted migration checkpoint: M1-A `5536feca`. Original preflight/baseline checkpoints: `050a922f`, `f45e3773`, `2e9f4a9f`, `de6bcd22`, `315055aa`, `61fbe4a7`. No push is authorized.
- M1-B source writer `/root/implementer_m1b` completed the lifecycle extraction, four original review corrections and the hidden radial registration correction, then released source ownership. Independent production source review is closed. Nine focused regression definitions remain unexecuted; their immediate-viewport adapter also needs correction in M1-C. Prior `/root/implementer` remains interrupted. Parent owns this ledger, visual QA and Git. No overlapping source writers.
- M1-B vendor substep is complete and independently accepted. LauncherApp extraction is source-reviewed, implemented/validation-pending: whole-crate API errors and a concrete immediate-viewport test adapter defect must close in M1-C. Vendor-only M1-B checkpoint `dac00f8d` is saved; local app checkpoint preparation is underway. No runnable migration or native parity acceptance. M1-C and M1-D read-only packets are persisted; neither implementation has started.

### Baseline evidence retained

All five full-mode original CPU owners passed: list6, grid30, notes42, query3, startup3 (84 cases), five warmups/twenty samples per case, zero exclusions. Independent review checked raw arrays, quantiles and tracked summary. CPU evidence does not prove native display/GPU/hotkey latency. See [benchmark-results.md](benchmark-results.md) and `receipts/cpu-original-summary.json`.

All original profile builds completed: fast-dev124.450s, dev261.312s, release419.488s. These are single observed existing-cache build durations, not statistical clean-build measurements. Original executables and exact library/startup-test artifacts remain preserved under ignored `target/egui-modernization/baseline/`. Production behavior remains the `72a01bd5` baseline; the reviewed `f45e3773` changes only stale test fixtures. See [baseline.md](baseline.md) and `receipts/original-build-times.json`.

The revised fixture helper compiled, generated provenance-bound runs003/004 and passed initial typed roundtrips. Run004 additionally records JSON-only controlled-placement overrides separately from the original helper manifest. User explicitly authorizes launch, interaction and screenshots for Multi Launcher. Six fixture runs have produced no screenshot. Run005 verified native query entry through accessibility-only observation; run006 additionally verified select-all, prefixed local search and synthetic Enter activation. Other selection/navigation and native interactions remain unproven; indexed Settings clicking could not obtain input geometry. Runs003/004 activated successfully after authorization, but capture and refreshed-window recovery timed out; run005 still failed capture after verified input. Exact owned processes were gracefully closed and exit confirmed. Native visual/display/GPU checks remain unproven; this is an open M0-B/M2-D evidence gap, not a waiver. See [visual-qa.md](visual-qa.md) and ignored per-run receipts. Do not repeat unchanged capture attempts without a meaningful recovery condition.

### M1-A accepted graph

Reviewed local checkpoint `5536feca` aligns eframe/extras0.36.2, toast0.22.0, CommonMark0.25.0, winit0.30.13 and raw-window-handle0.6.2, retaining explicit Glow/platform/accessibility features. All graph commands passed. Windows all-target check reaches expected source migration errors (621library/1269library-test); dependency alignment is accepted, runnable migration is not. Image0.25 decoder parity remains M1-D. See [m1a-api-errors.md](m1a-api-errors.md).

### M1-B vendor boundary accepted — local checkpoint dac00f8d

Planner confirmed released eframe0.36.2 hidden logic applies commands only to existing native viewports and cannot create a new macro prompt child. The local patch retains ROOT parking/focus and introduces default-compatible native hooks for logic-only dispatch, hidden child bootstrap, opted-in input invalidation, and CPU UI-frame completion after all discard decisions. Both Glow/Wgpu use the same scheduling policy while retaining hidden-root GPU guards. This is a disclosed local patch atop released0.36.2; it does not establish GPU presentation completion or native parity. See [vendor patch provenance](../../../vendor/eframe/PATCH.md).

Initial partial review found a P1: clearing pending events alone could leave held input stuck or replay pointer activation. The accepted correction clears stale retained keys/modifiers/pointer/scroll/files/events/frame deltas and cancels drag at the actual opted-in logic-only boundary, preserving absolute InputState.time, viewport/focus metadata and returned viewport commands. Default-false apps retain upstream behavior. Existing raw_input_hook cannot identify that dispatch boundary without duplicating native policy.

Parent inspected actual log `target/egui-modernization/m1b-vendor-tests.log`: initial texture-delta harness failure is preserved, then the corrected focused offline package run passed4/4, exit0. Tests exercise actual EpiIntegration held-key and held-pointer visible/hidden-release/resumed-UI transitions, no delayed action, hidden event/drop discard, and close/title command retention. Reviewer found P1 resolved with no new findings. Root locked offline no-deps metadata also passed. Temporary vendor workspace manifests/locks were removed; build outputs are ignored. No test session remains active.

### M1-B next acceptance dependency

The current source separates runtime, visibility, supplied-root UI and retained-pass completion; startup returns Result and explicitly selects Glow. Five focused test definitions cover forced discard action/history once, repeated hidden requests/stale input, hidden simple and parameter child bootstrap, and hidden-transition observer closure. Their unused FullOutputs are explicitly consumed. Writer formatting/diff checks passed. Parent inspected `target/egui-modernization/m1b-check-final.log`: scoped library check still exits101 with614 broad API errors. Tests are definitions only, not executed or accepted yet; M1-C must enable them and migrate remaining historical harness calls.

Independent source review identified four required corrections: missing ROOT scheduling after newly armed hidden child bootstrap (P1), skipped hidden-UI background/watch/provider/timer work (P1), skipped independent Screen Draw toolbar presentation/lifetime (P1), and pending query intents surviving already-hidden ticks (P2). The writer implemented bounded bootstrap-edge repaint scheduling, once-per-frame hidden background processing, toolbar registration with first-pass action/persistence guards, and explicit RootHidden cancellation. Four additional regression definitions exercise runtime prompt acquisition scheduling, logical-hidden background work, toolbar registration across discarded passes, and pre-hidden observer cancellation across both dispatch paths. Direct stable-toolchain formatting and diff checks passed; no build or test was rerun. Independent re-review is pending before the local app checkpoint. Reviewer previously found no further concrete snippet/final-candidate/OCR/startup issue and accepted the limited original native QA documentation; this does not waive M1-C execution or native migration acceptance.

Complete LauncherApp logic/logic_only/ui/on_ui_frame_complete extraction and startup Result factory using the planner packet. Preserve prompt input precedence/latch, visible after-menu watch ordering, every-hidden-tick runtime progress, once-only side effects across discarded passes, hidden new-child intent lifecycle, and final retained-paint receipts with accurate timestamps and no disabled-path instrumentation cost. Existing shutdown/parking/focus semantics remain authoritative. Do not introduce a parallel production update path.

Final focused re-review accepted all production corrections, including radial presentation on every hidden UI pass with actions first-pass-only and background -> radial -> timer order preserved. The forced-discard regression now checks retained viewport title, exposing a concrete harness requirement: default egui contexts embed viewports, and disabling embedding alone still falls back without a thread-local immediate renderer. M1-C must install a scoped immediate-viewport test renderer, run/consume child UI output, restore thread-local state, and then execute all nine lifecycle regressions. Until then, empty ROOT shape and child registration assertions are known adapter failures, not passing evidence. Production source review is closed; compilation, tests and native candidate acceptance are pending.

Checkpoint commits and acceptance are distinct for this single-crate migration. M1-B source may be implemented with focused tests validation-pending until M1-C closes remaining API errors; record actual implementing/enabling commits and results honestly. M1-C may include compiler-required integration glue and the intended shared font revision interface, without disposable false adapters. M1-D and M2-A retain behavioral/performance acceptance. M2-D remains the integrated migration gate before any styling.
