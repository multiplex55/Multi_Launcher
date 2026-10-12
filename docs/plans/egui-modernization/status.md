# egui modernization checkpoint ledger

Source contract: [`MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md`](../MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md). Statuses here distinguish recorded preflight evidence from acceptance gates that have actually passed.

| Checkpoint | Status | Current evidence / exit gap |
|---|---|---|
| M0-A — repository and toolchain preflight | `complete` | The Windows MSVC `cargo check --locked --all-targets --offline` passed. Original Nextest diagnostics completed with three classified stale fixture failures; a separate focused test-only correction passed 4/4. Current HEAD is the objective's working baseline; comparison with the separately named archive remains unresolved. No remote builder was configured or supplied. |
| M0-B — golden interaction and render inventory | `in_progress` | Source inventory and isolated fixture prepared; all five full-mode original CPU owners passed (84 cases), independently reviewed. All three original profile builds passed and are preserved; revised helper compiled and generated a provenance-bound third fixture. Native capture failed twice; screenshots, display/GPU timing and native interactions remain unproven. |
| M0-C — dependency/API breakage worklist | `complete` | Planner source-backed worklist and handoffs independently reviewed against cached released sources; accepted. No dependency changes. M0-B native evidence remains outstanding. |
| M1-A — align Rust toolchain and egui dependency graph | `complete` | Independently reviewed graph: one egui 0.36.2 family, aligned wrappers/windowing, explicit Glow. All-target check reaches expected source API errors (621 library / 1,269 library-test); runnable acceptance remains pending M1-C/M2-D. |
| M1-B — app lifecycle migration | `pending` | Not started. |
| M1-C — mechanical egui API compatibility | `pending` | Not started. |
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

- Local-only checkpoints: `f45e3773` (reviewed fixture correction), `2e9f4a9f` (partial native baseline), `de6bcd22` (API plan), `315055aa` (full CPU baseline). No push is authorized.
- CPU session `17135` finished successfully. All five owners passed: list 6 cases, grid 30, notes 42, query 3, startup 3. Every case retained five warmups and twenty samples, with zero exclusions. Independent review checked raw arrays, quantiles and the tracked summary. Native/GPU/display parity is unproven.
- Build session `45005` is live: fast-dev passed in 124.450 s, dev passed in 261.312 s; release is active. Do not restart the live process or run concurrent builds or measurements. Receipts are under ignored `target/egui-modernization/baseline/`.
- The build series began at `de6bcd22`; documentation checkpoint `315055aa` was committed during it. Source, manifest, lockfile and build script remain byte-identical to `f45e3773`; production behavior remains the `72a01bd5` baseline. The build script embeds resources, with no Git-HEAD input.

After the build session succeeds:

1. Record all profile results and preserve the original startup-test executable if needed for matched repeats.
2. Compile the revised fixture helper using the exact completed `build-fast-dev.jsonl` artifacts: launcher `target/fast-dev/libmulti_launcher.rlib`, JSON `target/fast-dev/deps/libserde_json-1c7737647b6b3c79.rlib`, SHA `target/fast-dev/deps/libsha2-33294a4cfa19f33c.rlib`. The canonical-artifact validator fix passed static checks and independent review; compilation remains pending.
3. Generate a new `run-original-003` fixture using the preserved dedicated `original-fast-dev.exe` (SHA-256 `49873783D3BB1C6A57488D432A3EFAAD0088B310936EA2E9B2F55F638CEEFB35`) and helper receipt. Old runs remain preserved with their serializer-provenance limitation.
4. Retry native capture with a fresh unique returned window and explicit `sky.activate_window` before observation. Earlier attempts selected/captured without activation. Do not assume access or success. Both earlier app processes were gracefully terminated. Desktop/access clarification is pending, and the named archive remains unavailable.
5. Begin M1-A after original builds and fixture provenance are saved. The preserved executable permits later original visual capture without waiving M2-D.

M1-A owns only manifest/lock changes: eframe/extras 0.36.2, toast 0.22, CommonMark 0.25, winit 0.30.13, explicit Glow. Direct winit defaults supply the needed feature unification; do not put `winit/default` in a dependency feature list. Image-loader parity remains M1-D work.

Planner clarified that local checkpoint commits and acceptance are distinct. M1-B may remain validation-pending until M1-C closes whole-crate API errors. M1-C may include compiler-required M1-D glue and the intended shared font-revision interface as an M2-A prerequisite. Do not introduce disposable incorrect adapters. M1-D and M2-A retain their behavioral/performance acceptance; run pending M1-B tests after M1-C. Record this rationale in the handoffs when migration begins. M2-D remains the integrated migration gate before styling.

## Baseline build and fixture completion

Session45005 finished successfully: fast-dev124.450s, dev261.312s, release419.488s. The revised helper then compiled and generated run-original-003 with exact artifact/hash binding and typed roundtrips. Original startup-test executable was preserved from the Nextest JSON list (SHA-256611F93C7662DACF38E9F815833A59CD63C464A9EE27EC7176B4217805A771E1E). Prior pending build/helper entries are now superseded. Native activation/capture remains next; M1-A source changes have not begun.

## Current execution state after M1-A

This section supersedes the earlier active handoff's running/pending entries. Original builds, helper compilation and third fixture generation completed in local checkpoint `61fbe4a7`. M1-A graph/API-backlog review passed without findings; all-target compilation is intentionally pending source migration. Preserve checkpoint cadence and enable M1-B tests after M1-C closes the single-crate API backlog.

User explicitly authorized launching, interacting with and screenshotting Multi Launcher for this mission. Run-original-003 activation succeeded after access was granted. Screenshot capture failed with `FrameArrived timed out: timed out waiting on channel`; refreshed unique-window recovery failed with `window capture timed out: timed out waiting on channel`. No screenshot, UI input, native behavior or display-performance pass was obtained. Exact owned PID 21352/path/start-time was checked before graceful taskkill succeeded. Capture remains an external tool limitation; preserved original binaries permit later retries. Continue M1-B while retaining this open M0-B/M2-D evidence gap. Checkpoints remain local only.
