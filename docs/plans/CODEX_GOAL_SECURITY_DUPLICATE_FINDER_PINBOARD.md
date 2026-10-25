# Codex Goal — rustls Security Patch, Stable Duplicate Finder, Responsive Pinboard

**Scope:** One bounded remediation goal covering the three issues below; independent, testable checkpoint commits on **one newly created task branch**.  
**Project:** Multi Launcher (Rust 2024, Windows, eframe/egui with Glow).  
**Primary contract:** Fix the named issues; preserve unrelated application behavior.  
**Companion prompt:** `CODEX_KICKSTART_SECURITY_DUPLICATE_FINDER_PINBOARD.md`.

## A0. Mandatory Git branch bootstrap — M0-A, before all application work

The **first execution checkpoint** is **M0-A: create and switch to a new, unique task branch, make a new baseline commit, and attempt the initial upstream push**. This requirement is not satisfied by being on an existing feature branch. There must be no feature-code edits, builds, test executions, or implementation checkpoints before M0-A.

1. **Read only:** Read repository `AGENTS.md`, this entire Goal, and the companion Kickstart. Inspect `git status -sb`, current branch, `git rev-parse HEAD`, configured remotes/upstreams, actual default integration branch (`master`/`main` or other), and all existing modified/untracked files. A safe `git fetch` to inspect the remote default tip is permitted; do not destructively manipulate the checkout.
2. **Select base safely:** Normally use the up-to-date default integration branch if the checkout is clean and doing so will not lose or displace user changes. Never automatically reset, hard-pull, stash, merge, rebase, force-switch, or discard anything. If dirty state makes switching bases unsafe, preserve it and create the fresh branch from current `HEAD` when safe; explicitly record this deviation. If no safe branch transition exists, report a genuine blocker instead of taking destructive action.
3. **Create/switch branch:** Select a **new**, idiomatic, lowercase kebab-case branch with a suitable `fix/` prefix; suggested `fix/rustls-duplicate-finder-pinboard`. Check local/remote names; append a meaningful unique suffix if taken. Do not reuse an older task branch. Branch creation is Codex's responsibility, not the user's.
4. **Create baseline commit on the new branch:** Prefer a small, legitimate task-specific tracked execution ledger if compatible with project conventions; otherwise use an intentionally empty baseline commit (e.g. `git commit --allow-empty -m "chore: [M0-A] establish remediation branch baseline"`). Never stage unrelated pre-existing work to fabricate a commit. Record **original base SHA** and **new baseline commit SHA** separately. All subsequent diffs are measured from this new baseline.
5. **Attempt initial push once:** `git push -u origin HEAD` (or equivalent non-force push when the remote/upstream is known and authorized). If denied, offline, or unavailable, record the actual reason, retain the local branch and commit, and **continue implementation locally; the initial push is nonblocking**. No bypasses, authentication retry loops, or force-pushes.
6. **Stay on this branch:** All changes, focused tests, later commits, and ordinary best-effort pushes must occur on this newly created branch. Do not silently return to the previous branch.

**M0-A acceptance:** New branch exists and is active; base and baseline SHAs are recorded; baseline commit belongs to the new branch; initial upstream push was actually attempted and its outcome recorded; all pre-existing user work is intact/unstaged. **Commit boundary:** `chore: [M0-A] establish remediation branch baseline`.

## A. Goal and desired outcome

Address the following independently recognizable defects in the current Multi Launcher codebase:

1. **Security:** Remove exposure to [RUSTSEC-2026-0285 / GHSA-2mjx-qc3c-rqvc](https://rustsec.org/advisories/RUSTSEC-2026-0285.html). The inspected `Cargo.lock` resolves `rustls 0.23.40`, within the affected `>=0.23.13, <0.23.45` range. Resolve **all active rustls instances** to **0.23.45 or later** with the narrowest compatible dependency update. No custom TLS protocol patching.
2. **Duplicate Finder dialog:** Stop the dialog from growing vertically across idle/repaint frames until it fills the application viewport. The default and user-selected window sizes must settle and remain stable when content and viewport dimensions are stable, while remaining resizable and allowing access to scan controls, results, diagnostics, and the footer.
3. **Screenshot Pinboard:** Improve sluggish movement/drag responsiveness of the independent pin windows after capture, and increase the visibly slow live-pin refresh rate. Diagnose drag responsiveness separately from capture throughput; remove confirmed bottlenecks rather than making indiscriminate scheduling changes. For an active, small, capable source, aim for **about 30 FPS** rather than the current built-in ~10 FPS ceiling; deliver latest frames without unbounded work or materially worse idle cost.

**User-visible result:** Secure transport dependency; Duplicate Finder stays the intended size; captured pins feel responsive to native window movement; live pins refresh materially faster where source/compositor capacity allows. No new settings, hotkeys, plugin commands, or UI redesign are requested.

## B. Codebase findings from the attached current-source inspection

These findings are **starting points**, not a substitute for Codex checking its live checkout after M0-A.

| Area | Located code and verified behavior | Implications |
|---|---|---|
| Security dependency | `Cargo.toml` declares `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`; there is **no direct `rustls` declaration**. `Cargo.lock` currently has `rustls 0.23.40`. Reverse dependencies include `reqwest`, `hyper-rustls`, `tokio-rustls`, `quinn`, `quinn-proto`. | Prefer a targeted `Cargo.lock` resolution update; do not pin a gratuitous new direct dependency or broadly upgrade networking. |
| Duplicate UI | `src/gui/duplicate_finder_dialog.rs` owns `DuplicateFinderDialogState::show_with_jump_mode`. `egui::Window` uses `.resizable(true)`, `.default_size((1040.0, 700.0))`, `.min_size((620.0, 420.0))`, `.max_size(ctx.content_rect().size())`. | Maintain resize, min size as viewport permits, and existing close/focus ownership. |
| Duplicate sizing feedback candidate | In `show_with_jump_mode`, outer `ScrollArea` uses `body_max_height = (ui.available_height() - footer_reserve).max(0.0)` and `.max_height(body_max_height)`. `show_status` derives root-status/diagnostic caps from `ui.available_height()`, and `show_results` is capped at `body_max_height * 0.4`. | Height budgets depend on available space inside an auto-layout window; inspect egui's retained sizing passes and identify whether these calculations create gradual size feedback. This is a **hypothesis, not a proven root cause**. |
| Duplicate ownership and tests | Existing Windows test `dff_dialog_response_flow_cancels_drains_restarts_and_uses_current_verified_actions()` in the same file exercises constrained viewports, footer stability, scrolling, cancellation and current-row actions. `src/gui/duplicate_finder_smoke.rs`, `src/bin/duplicate_finder_smoke.rs` provide an owned native fixture; rendered from `src/gui/render.rs` and hosted via the existing panel/command infrastructure. | Extend real layout and interaction tests; avoid touching duplicate-detection algorithms or worker lifecycle without evidence. |
| Pinboard orchestration | `src/gui/pinboard.rs` owns the pin-capture state machine, root parking, shared rectangle selector, hide/restore, capture commit and presenter calls. `src/pinboard.rs` owns limits, pin identity and session lifetime. `src/plugins/pinboard.rs` exposes capture/list/live commands. | Capture ownership must remain exclusive and safe; UI improvements do not change selection or capture semantics. |
| Native pin windows | `src/gui/pinboard_windows.rs`: `PinWindowPresenter::{show,poll}`, `pin_viewport_builder`, `render_pin_viewport`, `ensure_pin_texture`, `lookup_owned_pin_window`. Presenter uses `ctx.show_viewport_deferred(...)` for independent native child windows; native lookup/geometry and worker state are synchronized with root. Static pin textures are retained, and live textures are updated by sequence. | Investigate what costs/requests run on native drag vs GUI frame. Preserve HWND, PID, lifetime and root/child ownership validation; do not invent a second window system. |
| Live frame throttle | `src/pinboard/live.rs`: `const FRAME_INTERVAL: Duration = Duration::from_millis(100)`, `FrameCadence`, `native::run`, two-buffer frame-pool coalescing; test `frame_cadence_caps_completed_frames_to_ten_per_second()`. | There is a **confirmed 10 FPS maximum at the capture worker**. A rate increase alone may be insufficient if preparation, GPU readback, texture uploads or paint are slower; measure each stage. |
| Live display delivery | `src/gui/pinboard_windows.rs`: `LiveDisplayPreparation`, `spawn_live_display_preparation`, per-pin child wake callback, `ensure_pin_texture`. Conversion to egui `ColorImage` happens in a worker; the child viewport consumes the prepared latest sequence. Upload retirement is limited by `MAX_RETIRED_LIVE_UPLOADS`. `PinWindowPresenter::poll` has 16 ms work-pending and 100 ms retired-worker repaint scheduling; these are **not automatically evidence of a 100 ms active-live repaint throttle**. | Keep expensive conversion off UI, target child-viewports only, avoid root-wide busy repaint or unbounded queued image copies. Do not change 100 ms retirement cleanup merely because the live capture limit is 100 ms. |
| Existing pin tests | `src/gui/pinboard_windows.rs` tests `pinboard_live_presenter_reuses_texture_and_charges_retained_uploads()` and related identity/visibility cases. `src/gui/pinboard_windows_smoke.rs` with `src/bin/pin_opacity_smoke.rs` provides guarded native Glow/Windows presentation evidence for generated pins. `src/pinboard/live.rs` contains frame-cadence/fake-backend tests and a Windows native source fixture using `tests/fixtures/pinboard_live_source.ps1`. | Reuse these seams. A generated frame test **does not** prove WinRT/OS capture throughput, and headless UI layout tests **do not** prove actual native drag responsiveness. Report evidence by type. |

The user's phrase **"duplicate folder dialog"** is interpreted as the application's **Duplicate Finder** dialog: this is the corresponding actual user-facing dialog found in the source. If the live checkout instead has a distinct duplicate-folder dialog, Codex must verify the match through its UI/command registration before making changes; do not silently alter an unrelated UI.

## C. Approved functional requirements and engineering defaults

### C1. Security resolution

- Keep network features and TLS backend functioning. Upgrade only the necessary resolved packages; `rustls >=0.23.45` is the security floor for the affected 0.23 series.
- Prefer `cargo update -p rustls --precise 0.23.45` **if supported by the currently resolved dependency graph**; inspect before and after with `cargo tree -i rustls` and the lockfile. If a newer compatible patched 0.23 version is already resolved in the live tree, keep it.
- Change `Cargo.toml` or peer transitive packages only if their actual constraints make a safe lock-only update impossible; justify each related change. Keep `reqwest` `blocking` + `rustls-tls` unchanged unless absolutely required for the patch.
- Verify no affected copy survives; ensure the fix is captured in the tracked lockfile. Do not promise that the advisory enables handshake forgery: upstream describes a TLS 1.3 encryption-level protocol-validation defect while handshake transcript authentication remains intact.

### C2. Duplicate Finder sizing and interaction

- With no input and no content change, **neither the dialog height nor its top/bottom edges may drift upward/downward over successive frames** after normal initial egui convergence.
- Preserve an initial usable size near the existing 1040×700 logical points when space permits. Allow user-driven resize/reposition; when resized, preserve the user-chosen size during subsequent repaints, scans, diagnostics expansion, text wrapping, and scrolling. If the main viewport shrinks, obey its bounds and keep a reachable UI.
- Keep `Start`, `Cancel`, `Close` accessible; keep long roots, diagnostics, and result lists inside bounded, usable scroll regions; maintain virtualized result rows and approved result actions. No fullscreen creep as a surrogate for showing overflowing content.
- Preserve Jump Mode target registration, modal/window ownership, `first_ui_pass` side-effect fencing, selection/revalidation and worker cancellation/draining semantics.

### C3. Pin movement and live display

- **Static pins must move promptly with the native OS drag operation**; live pins should also move promptly while capturing, and a paused live pin must not spuriously interfere with dragging. Do not reset native positions or introduce repeated delayed jumps. Reopening the pin list must not disrupt existing pin positions.
- Capture completion should not leave expensive synchronous capture/presentation work on the main UI path longer than necessary; any measured root/pin window blocking during drag should be reduced at its actual ownership boundary.
- Live pins should have a bounded **~30 FPS target (roughly 33–34 ms cadence)** when unpaused, visible, and fed sufficiently frequent source frames on capable hardware. Source changes and frame presentation remain asynchronous and latest-frame-oriented. A slower source or overloaded compositor may run slower; do not fabricate frames or claim a guaranteed 30 FPS.
- Paused, hidden/unavailable, terminated or unchanged sources must not generate unnecessary continuous work. Preserve pause/resume, resize/source-invalid notifications, exact generation fencing and cleanup.
- Retain per-session limits (8 pins, 256 MiB CPU shared budget; existing GPU budget), two-frame pool, bounded retired uploads, click-through recovery, topmost/default window behavior, opacity, scale modes, Copy/Save, closed-window cleanup, and source-window identity checks.
- Do not introduce a user-facing FPS slider or capture quality setting in this goal.

## D. Scope and non-goals

**In:** Targeted dependency resolution; dialog sizing/layout bug and its tests; measured pin-window drag/presentation bottleneck and live refresh throughput; minimal test/fixture instrumentation needed to verify the above; new branch/commits and evidence.

**Out:** New duplicate-finding algorithms, search features or destructive file actions; a general egui redesign; new workspace behavior; a pinboard rewrite, different capture backend, GPU architecture migration or hardware acceleration feature; new plugins/hotkeys/configuration; unrelated dependency sweeps, general compilation optimization, hundreds of test fixes or full test-suite stabilization. No modified tracked user data in smoke tests. No unapproved external window input or screen captures.

**Do not:** Replace native child viewports, bypass `PinSourceWindowIdentity`/HWND guards, relax capture exclusivity/clipboard privacy, solve performance by leaving the UI continuously repainting when idle, or disable window resizing to mask the sizing defect.

## E. Technical implementation strategy

1. **Separate cause classes.** Security is dependency resolution; dialog is egui layout; pin responsiveness is native child-window lifetime/event scheduling and live capture/prepare/upload throughput. Do not couple their implementations.
2. **Prove geometry drift before fixing.** Use the existing dialog's egui test-context fixture to capture stable window rect over many frames and across snapshots/viewport sizes. Examine whether `ui.available_height()` is fed back into retained outer window size. Correct the layout's height budget at the owning boundary using stable finite geometry rather than content-derived growth; avoid a global egui style change or fixed, unresizable window.
3. **Diagnose pin drag separately.** Compare a static pin, an active live pin, and a paused live pin. Time native movement/input responsiveness, `PinWindowPresenter::show/poll`, child rendering/texture updates, lock waits and repaint requests. Identify whether sluggishness is an event-loop cost, repeated native inspection, capture/texture pressure, or geometry/builder updates. Optimize only observed hotspots; native compositor movement must remain authoritative.
4. **Raise live cadence safely.** Start with the known 100 ms `FRAME_INTERVAL` and its test; choose a bounded ~33–34 ms cadence if tests/measurements show the pipeline can keep up. Retain latest-frame coalescing, backpressure and memory reservations; avoid accumulating CPU frames or GPU uploads. Improve worker or repaint bottlenecks only if measured and directly related.
5. **Verify at layers.** Unit tests prove exact cadence, geometry stability, and ownership; generated child-viewport tests prove frame arrival/painting and lifecycle; optional owned native smoke establishes Windows behavior when an interactive graphical session is available. Keep evidence types distinct.

## F. Detailed milestones and tasks

### M0-A — Create and baseline fresh Git branch

**Objective:** Establish a safe, isolated branch and baseline before any implementation.  
**Owner:** Orchestrator (Git only; see A0).  
**Tasks:** Execute A0 in order; check changed/untracked files and intended remote; write down base/baseline SHAs and push status without modifying the app.  
**Edge cases:** Dirty working tree, no remote, existing suggested branch, no safe checkout.  
**Regression constraints:** No lost/staged unrelated user work; no changes on primary/previous branch.  
**Verification:** `git status -sb`, active branch, `git log -1`, branch ancestry, push result.  
**Acceptance:** All A0 criteria.  
**Commit:** `chore: [M0-A] establish remediation branch baseline`; best-effort push.

### M1-A — Patch affected rustls resolution

**Objective:** Eliminate the pinned vulnerable `rustls` resolution without unnecessary networking churn.  
**Owner:** `Cargo.lock`; inspect `Cargo.toml` and reverse dependency paths before considering other files.

**Tasks:**

1. On the new branch, inspect `rustls` versions in the live lockfile and reverse dependency tree; record baseline package/version and why it's included.
2. If affected, request the narrowest patched 0.23.x compatible resolution, starting with `cargo update -p rustls --precise 0.23.45`. If constraints reject the patch, identify the specific blocker and make the smallest required additional package/manifest adjustment; no wholesale `cargo update`.
3. Inspect the lockfile diff for unintended version churn; verify one/all resolved rustls instances are outside the vulnerable range. Preserve reqwest TLS backend and features.
4. Prefer local metadata/lockfile and targeted compiler checks. If compilation is costly, do the full first-party targeted compile once during the final integration checkpoint rather than rebuilding the entire application repeatedly.

**Edge cases:** Dependency already patched in the live checkout; multiple Rustls major copies; resolver/offline failure; transient registry access. An already-secure lockfile should be documented as verified rather than changed gratuitously.

**Regression constraints:** No TLS fallback to native TLS; no reduction of verification/certificate validation; no vendored rustls edits.

**Verification:** `cargo tree -i rustls --locked` (and disambiguated versions if more than one), lockfile inspection, `cargo metadata --locked --format-version 1 --no-deps` as applicable; later targeted compile/HTTPS regression smoke if a safe existing test exists. Run an existing `cargo audit` only when available without adding a dependency or broadening scope; do not interpret absence of unrelated advisories as a new requirement.

**Acceptance:** No affected rustls version remains; `Cargo.lock` captures the patched graph; required features still resolve; diff is minimal and explicable.  
**Commit:** `fix: [M1-A] resolve patched rustls security dependency`; normal non-force push attempt.

### M2-A — Fix Duplicate Finder height creep with geometry regression proof

**Objective:** Stabilize dialog height without losing usable scroll areas or user resizing.  
**Owner:** Primarily `src/gui/duplicate_finder_dialog.rs`; edit `src/gui/render.rs` or shared UI only if evidence establishes the true ownership boundary.

**Tasks:**

1. Extend the existing egui-context dialog test helpers with a test that records the **actual window rect** (or egui retained area rect) per frame rather than only footer button rectangles. Reproduce the growth with repeated no-input frames; record initial convergence vs ongoing drift. Test both empty/idle and crowded snapshots; a stable 1180×860 viewport and small 620×420 viewport; varying viewport height and DPI as supported by test context.
2. Inspect outer scroll max-height and `ui.available_height()` dependency within `egui::Window`; isolate the cause. Bind body/footer allocation to stable bounded available dimensions that do not cause window-size feedback. Preserve `.resizable(true)` and the application's ordinary window sizing conventions.
3. Keep results virtualization, selected-root scroll, root-status and diagnostics visibility, and the fixed scan/close footer. Verify resizing up/down or moving via UI/raw-input test where feasible, and that the chosen size remains stable afterward. No artificial full-height `default_size` or forced window reset each frame.
4. Check Jump Mode `register_target`, owner/current input gating and single-pass side effects stay unchanged. Tests must use current egui 0.36 APIs rather than invented method names.

**Edge cases:** First few egui measurement passes; `ScrollArea` first-show viewport changes; roots/status/diagnostics content growth; tiny viewport; long paths and wrapped text; user-directed size changes; closing/reopening.

**Regression constraints:** Do not alter duplicate hash/scan workers, root selection semantics, action revalidation, cancellation, existing error presentation, or the real folder picker.

**Verification:** Target the `dff_dialog_` test family plus new sizing regression; extend existing geometry test rather than cloning expensive full-flow fixtures. On GUI-capable Windows, one owned Duplicate Finder fixture open/stable/resize screenshot series through existing `duplicate_finder_smoke` if its driver supports it safely; never require manual mid-task intervention.

**Acceptance:** After brief egui convergence, repeated no-input frames exhibit no monotonic vertical growth; user resize persists; footer and scrolling controls stay visible/reachable; existing targeted flow passes.  
**Commit:** `fix: [M2-A] stabilize Duplicate Finder window geometry`; best-effort push.

### M3-A — Diagnose and correct pin-window movement latency

**Objective:** Make captured native pin windows responsive to dragging independently of live capture rate.  
**Owner:** `src/gui/pinboard_windows.rs`, possibly focused calls in `src/gui/pinboard.rs`/`src/gui/render.rs` only if measured. Reuse `src/gui/pinboard_windows_smoke.rs` or relevant unit tests.

**Tasks:**

1. Record a **small bounded before-fix baseline**: static captured pin drag, live pin drag, paused live pin drag, capture-complete-to-pin-ready time, and frame/render/lock or native lookup hotspots. Instrument transiently behind tracing/feature-gated test code where needed; avoid permanent high-volume logs.
2. Check `PinWindowPresenter::show/poll`, root hidden-state bootstrap, native `lookup_owned_pin_window`, `pin_viewport_builder`, child `render_pin_viewport`, static texture reuse and geometry observations. Determine which path is causing delayed drag updates, blocked native message dispatch, or position jitter. Do not assume the live worker's 100 ms frame throttle is responsible for static drag.
3. Fix the confirmed responsible behavior with the smallest ownership-correct change: e.g. avoid expensive repeated or synchronous work in an input-sensitive event path, unnecessary redundant geometry/window-builder updates, or blocking locks. Retain authoritative native window position and exact lifetime/identity validation; no broad Win32 hooks absent hard evidence.
4. Ensure static texture is still prepared/reused once and that inactive pin windows do not spin the root continuously. Confirm close/retire, opacity, click-through, always-on-top, resize, and independent multi-pin movement.
5. Repeat the same bounded measurements/fixture interactions to show improvement; record the environment/hardware and any uncertainty. If instrumentation shows **no code-owned drag defect**, do not invent one; preserve the evidence and narrow to proven bottlenecks.

**Edge cases:** Two or more pins, quick consecutive captures, high-DPI/secondary monitor, foreground changes, minimized launcher, topmost/click-through, source paused or closed, native handle recycling, last-pin cleanup.

**Regression constraints:** Never weaken process/lifetime/native-window ownership checks, pass-through recovery, capture hide/restore transaction, or live-session cancellation. Don't make main launcher stay open simply to move a pin.

**Verification:** Related `pinboard_windows` presenter/visibility tests; bounded generated native pin smoke (`pin_opacity_smoke`) for actual child window readiness/movement when GUI is supported; if simulating drag, require exact owned child, foreground, hit-test, no foreign overlay/held input, and fail closed on unsafe environments. A `SetWindowPos` position test is useful for geometry but **is not** evidence of pointer-drag latency.

**Acceptance:** A static pin moves smoothly without repetitive delayed position updates on a supported native desktop; active/paused live pins do not visibly stall native movement; before/after evidence identifies a real code path improved; all identity/interaction invariants remain intact. For unsupported GUI environments, distinguish proven unit behavior from unverified native responsiveness.  
**Commit:** `fix: [M3-A] reduce screenshot pin movement latency`; best-effort push.

### M4-A — Improve active live capture/display cadence safely

**Objective:** Remove the demonstrable 10 FPS capture limit and keep frame delivery responsive, bounded and idle-efficient.  
**Owner:** `src/pinboard/live.rs`, `src/gui/pinboard_windows.rs` only for demonstrable preparation/presentation bottlenecks, focused tests in these modules.

**Tasks:**

1. Instrument the stages **source frame signaled → latest frame read/converted → published sequence → prepared `ColorImage` → child repaint/texture delta** using cheap bounded counters/timings in a test-only harness or short diagnostic session. Measure live source throughput before changing scheduling; distinguish source-limited vs app-limited.
2. Change the existing capture cadence from `FRAME_INTERVAL = 100 ms` to a conservative ~33–34 ms target **when active**; update `frame_cadence_caps_completed_frames_to_ten_per_second()` to an accurate descriptive test of the new cap with deterministic virtual durations. Avoid increasing 16 ms action-polling or 100 ms retired-preparation cleanup frequency without direct evidence.
3. Confirm worker `wait_for_frame_or_control` and `FrameCadence` still coalesce pending frames and respect cancellation, pause/resume and source close; no spin when no frames. Keep two-buffer latest-frame semantics and discard stale generations.
4. If measured preparation/render costs prevent improvement, optimize **only** the identified stage using the existing worker, latest-sequence and texture-reuse ownership: no image queue expansion, excessive clone, repeated upload of unchanged sequence, or synchronous per-frame encoding/copy on the UI thread. Preserve `MAX_RETIRED_LIVE_UPLOADS`, exact reservations/byte accounting, and per-pin child wake rather than blanket root repaint.
5. Use generated fake frames with a controlled 30 Hz producer to verify material throughput above the former 10 FPS cap and correct latest-frame progression. On a healthy native Windows/Glow session with a sufficiently active small source, target roughly 20–30 visibly delivered FPS after stabilization; do not require that from a slow, minimized or protected source. Record generated vs actual OS-source evidence separately.

**Edge cases:** Very large capture region under memory pressure, multiple live pins, pause/resume race, source resize/unavailable/closed, renderer retaining older texture handles, hidden/suppressed viewport, cancellation during wait, slow GPU/remote desktop.

**Regression constraints:** All safety, budget, generation and window invariants in C3. Do not circumvent the GPU/CPU budgets to achieve FPS. Unchanged/stopped sources stay quiet and worker threads retire promptly.

**Verification:** Updated `frame_cadence_` test(s), `worker_signal_` test(s), generated live-source tests in `src/pinboard/live.rs`, and `pinboard_live_presenter_reuses_texture_and_charges_retained_uploads()`. Assert texture reuse, child-only repaint, pause/resume freshness, bounded retained uploads, and no runaway worker wakeups. Targeted compile/nextest only.

**Acceptance:** Hardcoded 100 ms active-live ceiling eliminated; ~30 Hz source can produce/display more than 10 distinct frames per second in a capable bounded fixture, while all lifecycle/budget tests pass and no idle busy loop appears.  
**Commit:** `perf: [M4-A] increase bounded live pin frame cadence`; best-effort push.

### M4-B — Owned native pinboard validation and final polish (only if environment supports it)

**Objective:** Verify production-path child-window movement, live presentation and pixel/lifecycle correctness after both pin checkpoints.  
**Owner:** Existing smoke/support modules and narrow fixes needed to close related failures; do not build a new universal GUI automation framework.

**Tasks:**

1. Reuse `src/gui/pinboard_windows_smoke.rs`/`pin_opacity_smoke` for real Glow child windows with owned/generated source. Capture a short stage-by-stage log and **screenshots only of the owned fixture surfaces** if available (initial static, moved static, active live frame before/after). Preserve existing protection against clicking/sampling foreign windows; avoid reading unrelated screen contents.
2. Reuse native live-source smoke in `src/pinboard/live.rs` with `tests/fixtures/pinboard_live_source.ps1` only when the Windows interactive desktop permits it and safe ownership checks pass. It validates actual WinRT source capture independently of synthetic generated-frame fixtures. No manual user interaction required during execution.
3. Run a **bounded** capture→pin appears→move/resize→live update→pause/resume→close scenario, preserving click-through, topmost, opacity and Copy/Save behavior. Confirm no residual processes/windows; logs must identify what was directly proven vs not run.
4. If native GUI is impossible over an SSH-only or locked session, record a clear **environmental GAP**, not a pass. Do not repeatedly attempt focus/input injection or ask the user to participate mid-task. Generated/unit coverage must still be completed.

**Edge cases:** Foreign-window occlusion, user input active, unsupported WinRT capture, minimized/remote desktop, DPI.  
**Regression constraints:** Never weaken visual/input ownership checks to force smoke success.  
**Verification:** One owned smoke run per supported scenario (one remediation rerun if a directly related defect is corrected); log actual timing, visible frame changes and clean shutdown.  
**Acceptance:** Native behavior is verified where possible; any environmental gap is explicit; no unowned input/screenshot actions or lingering owned process.  
**Commit:** If meaningful source/test improvements are required, `test: [M4-B] verify responsive native pin presentation`; otherwise do not create an artificial non-baseline commit.

### M5-A — Focused integration, review and handoff

**Objective:** Confirm three fixes coexist and stop at the agreed boundary.  
**Owner:** Orchestrator/reviewer; source changes only for directly identified regressions.

**Tasks:**

1. Review diff from the **new M0-A baseline** and identify each change's owner. Ensure Cargo changes are small, dialog effects localized, and pinboard changes do not affect unrelated macros/overlays/launcher focus.
2. Run formatting for changed Rust files and one relevant first-party compile (`cargo check --locked -p multi_launcher --bin multi_launcher`, or equivalent confirmed target). Run targeted nextest filters for affected tests, not the entire thousands-test suite.
3. Run a single first-party Clippy pass **at the end** if practical in the declared budget. Exclude vendored crates/unrelated lint cleanup. Fix only introduced lint problems; record unrelated preexisting warnings/failures separately.
4. Confirm memory cleanup, native fixture termination, no temporary captured user content in tracked files, correct lockfile, and Git state. Resolve bounded reviewer findings without rewriting earlier checkpoint commits; use a narrow remediation commit if needed.

**Verification:** `git diff <M0_A_BASELINE_SHA>...HEAD --stat` plus detailed diff, focused test receipts, no affected rustls copies, `cargo check` result, final `git status -sb`, commit log/push results.

**Acceptance:** Requested three outcomes delivered, evidence or explicit environmental gaps recorded, no feature-related blockers, clean intentional diff, all checkpoint changes committed on the new task branch.  
**Commit:** `fix: [M5-A] close remediation verification findings` **only if necessary**; best-effort push.

## G. Active checkpoint commit cadence

**Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** M0-A baseline is mandatory and first. Commit verified, coherent work at M1-A, M2-A, M3-A, and M4-A (with M4-B/M5-A only for material subsequent changes). Where a stage genuinely needs multiple coherent implementations, add stage-tagged subcheckpoints (e.g., `M3-B`) without ballooning scope; do not commit every tiny edit.

- Before every commit, review staged diff and perform that checkpoint's focused checks. Do not stage unrelated pre-existing files.
- Make **one normal non-force push attempt after each checkpoint** when supported. Push failure is logged and **does not block** implementation or local commits.
- Never squash/rewrite existing commits, force-push, switch back to the original branch, or incorporate unrelated source changes into this task.
- Record **branch name, original base SHA, M0-A baseline SHA, all checkpoint commit SHAs and individual push outcomes**.

## H. Focused verification strategy

**Core security checks:** `cargo tree -i rustls --locked` and exact lockfile version inspection; targeted metadata and compile. Optional `cargo audit` only if already available; do not turn this into a full audit remediation campaign.

**Recommended focused Rust checks on Windows** (adjust test filters to actual test names added or changed):

```powershell
cargo nextest run --locked -p multi_launcher --lib -E 'test(dff_dialog_)'
cargo nextest run --locked -p multi_launcher --lib -E 'test(frame_cadence_) or test(worker_signal_)'
cargo nextest run --locked -p multi_launcher --lib -E 'test(pinboard_live_) or test(pinboard_live_presenter_)'
cargo check --locked -p multi_launcher --bin multi_launcher
# Final, if supported and within budget:
cargo clippy --locked -p multi_launcher --lib --bin multi_launcher -- -D warnings
```

If a nextest selector differs from the installed version or a test was renamed, inspect `cargo nextest list` and use the exact current selector. If `--lib` filters omit a module's relevant test target, include that explicitly. Never report a command as passed without running it.

**GUI validation:** Extend actual egui window-rect tests in `src/gui/duplicate_finder_dialog.rs`. Reuse existing `duplicate_finder_smoke` and `pin_opacity_smoke` binaries only with `--features dev-smoke-binaries`, safe owned fixtures and supported graphical session. `native_live_pin_owned_window_smoke()` is a separate OS-capture test, not a prerequisite to claim generated frames work. Record before/after timings using the same environment and input method.

**Evidence:** Small text/JSON performance receipt with static vs live drag observations, source/preparation/presentation frame counters/timings, known gaps and their causes; bounded screenshot evidence of owned UI if supported. Don't add bulky recordings, sensitive screenshots, or broad benchmark frameworks to the repository.

## I. Risk management, budgets and blockers

| Risk | Mitigation |
|---|---|
| Cargo resolver/network error | Try one safe targeted resolution; inspect offending constraints; preserve intended lockfile; no blind dependency-upgrade loops. |
| Window geometry symptom without proven cause | Measure actual window rect across sustained repaint frames; fix owner; don't mask by disabling resize or artificially constraining full app. |
| Pin dragging confused with low live FPS | Compare static, live and paused; measure native event, paint, lookup and main-thread costs independently. |
| Higher FPS raises CPU/GPU pressure | Target a bounded ~30 Hz **active** rate; retain coalescing, reservations, retained-upload caps, low idle wake rate and error surfaces. |
| WinRT/Glow/SSH desktop unavailable | Distinguish headless/unit PASS from native GUI GAP. Avoid unsafe automation or demanding interim manual review. |
| Very slow existing test suite / long rebuilds | Select targeted nextest and one necessary compile; avoid release builds, broad regression suites and looped retries. |
| Unrelated historical test failures | Log as pre-existing/unrelated with evidence; do not fix them under this goal. |

**Bounded execution:** Start with short baseline traces (e.g., 20–30 seconds per pin movement/source class); run each directly affected test group once after implementation, and permit at most one focused remediation/retest cycle per actual feature-related failure before escalating a concrete blocker. Native smoke sessions should have a bounded duration (approximately 90 seconds per scenario **after build**, except an existing harness with an established shorter limit); ensure cleanup. Compile time can reflect Windows cache state; do not start repeated release builds. Do not spend hours trying to make an unsupported remote GUI become interactive.

A true blocker is failure to obtain a patched dependency, inability to remove height drift, an unsafe/incorrect pin change, or a directly failing required regression. An unrelated warning, absence of Windows desktop access, or an optional optimization is **not** permission for a new project. Record the smallest next decision if the blocker cannot be fixed within bounds.

## J. Definition of done

- [ ] M0-A completed on a **fresh unique branch** with recorded base/baseline SHA and initial upstream push outcome.
- [ ] No active resolved `rustls` copy in `>=0.23.13, <0.23.45`; minimal tracked dependency changes and verified reqwest rustls feature resolution.
- [ ] Duplicate Finder height no longer creeps with repaint frames; user resize persists, small viewport/footer/results/diagnostics remain usable, current scan and Jump Mode interactions pass.
- [ ] Static pin movement is materially more responsive on a supported native Windows session, with actual cause/fix/measurement documented; no snapping, ownership bypass or new idle churn.
- [ ] Active live pin throughput is no longer hard-capped at 10 FPS; higher bounded cadence works with capable generated source and correct latest frame delivery, while pause/resume, cancellation, GPU/CPU budgets and cleanup remain correct.
- [ ] Relevant targeted tests/compile and appropriate end-only lint review have factual outcomes. Native evidence is reported as PASS, FAIL or environmental GAP—not assumed.
- [ ] All meaningful code checkpoints committed/pushed best effort on same new branch; no unrelated user work changed; no feature-related blocking defects remain.

## K. Final delivery report Codex must produce

Produce a concise, evidence-based final handoff with:

1. **Git:** actual branch; starting/default base and SHA; **M0-A baseline SHA**; every labeled checkpoint SHA and push status; final Git status. Note any dirty-checkout deviation.
2. **Security:** original and final rustls versions; reverse dependency path; exact changed dependency files; advisory coverage confirmation; build outcome.
3. **Dialog:** diagnosed source of vertical drift; geometry/layout correction; test viewport sizes and number of repeated frames; resize/diagnostics/footer results; owned UI screenshots if obtained.
4. **Pin movement:** measured baseline vs final static/live/paused window behavior and diagnosed blocking path; exact responsive movement proof and limitations.
5. **Live refresh:** before/after producer, capture/prepare/display frame cadence; source type (generated vs OS); CPU/memory/cancellation consequences and actual test results.
6. **Checks:** commands, test counts/pass/fail/skip and timings, native smoke result and screenshots/log paths, review/lint outcome. Don't claim unexecuted checks succeeded.
7. **Residual items:** only relevant open blockers/environmental gaps, with smallest feasible next step; suggested final **single** manual review at end if useful.

**Stopping rule:** Stop once this security fix and the two scoped UI/performance corrections are implemented and bounded verification is complete. No optional follow-on work or unrelated refactoring.
