# Multi Launcher — Screenshot Pin Input-Shield Acknowledgement Repair

**Status:** Approved bug-fix implementation plan; code-first, no further user clarification required.  
**Priority:** High — `Capture Screenshot Pin` currently fails before region selection can begin.  
**Execution authority:** Current checked-out repository and its `AGENTS.md`. Do **not** rely on uploaded archives, historical source snapshots or a particular branch name.  
**User-visible error to eliminate:** `screen selection failed: Rectangle input shield coverage acknowledgement failed`.

> **Success is functional, not cosmetic:** Capturing a Screenshot Pin must succeed when the user's Windows desktop supports the feature, while the complete selection mouse Down/drag/Up is prevented from interacting with underlying applications. **Do not bypass the shield acknowledgement, let the picker become active with incomplete coverage, or merely suppress the error message.**

## 1. Bounded scope and desired behavior

### Must fix

1. Selecting **`pinboard` → Capture Screenshot Pin** establishes its Windows input shield(s) and lets the user select a rectangle normally, rather than immediately displaying the acknowledgement failure.
2. The shield acknowledgement reports **exactly which invariant fails** when it cannot arm: Win32 API/stage, operation ID, monitor index, requested and observed screen bounds, visibility, and relevant extended styles; show an actionable concise message in the UI and richer privacy-safe data in the diagnostic log.
3. If the failure is caused by inconsistent DPI/coordinate spaces, window positioning, visibility, window styles, or initial Win32 message ordering, **correct the actual cause** and make the verification coherent across normal startup and display-topology repair.
4. **Safety invariant:** A click/drag/release used to select the rectangle never selects text, clicks a button, activates controls, or sends an unintended mouse transaction to the app underneath; no shield remains after successful capture, Escape, initialization error, monitor change or shutdown.
5. Existing Screenshot Pin image capture, session-only pin windows, drag coordinates, multi-monitor behavior, pin management, screenshot editor, general OCR, Screen Draw and MkMacro rectangle selectors retain their established behavior.

### Outside scope

- No new screenshot plugin, generic input-hook rewrite, alternative capture backend, desktop-wide input blocking, extra Windows service, settings migration, UI redesign, broad performance audit or unrelated file changes.
- Do not add a second system-wide `WH_MOUSE_LL` hook or call `BlockInput` as a workaround for a broken input shield.
- Do not remove validation, change a failed prerequisite into success, accept coordinates that permit uncovered physical pixels, or silently fall back to a click-through picker.
- No lengthy benchmark campaign, new infrastructure framework, broad refactor of shared overlays, or repeated full test-suite runs.

## 2. Source-derived diagnosis and owner map

The reported text is emitted by the **real source** at the rectangle shield verification step in `src/gui/mkmacro_dialog/visual_overlay_windows.rs`, currently near the end of `NativeOverlayRenderer::show` (around the `GetWindowRect` + `shield_covers_monitor` loop). The current flow:

1. `displays()` obtains monitor rectangles from `EnumDisplayMonitors` and `GetMonitorInfoW`.
2. `input_shield_plan` in `src/gui/mkmacro_dialog/visual_overlay.rs` requests **one physical-monitor-sized input shield per display** for a rectangle picker (`RectanglePurpose::PinboardCapture` among other purposes).
3. `show_input_shields` calls `CreateWindowExW` with a nonactivating layered top-level popup, sets alpha to `1` (not zero), uses `SetWindowPos(HWND_TOPMOST, ... SWP_SHOWWINDOW)`, calls `ShowWindow`, and performs initial paint. The input-shield WndProc owns left Down, Move, Up and `SetCapture`/`ReleaseCapture` semantics.
4. `NativeOverlayRenderer::show` then loops over shields. For each HWND, it obtains `GetWindowRect`, `IsWindowVisible` and `GetWindowLongPtrW(GWL_EXSTYLE)` and requires **exact equality** between the observed rectangle and the monitor bounds, plus `WS_EX_LAYERED`, `WS_EX_NOACTIVATE`, `WS_EX_TOPMOST` and the **absence** of `WS_EX_TRANSPARENT`. It emits the same generic error for all failures.
5. Only **after all shields pass** does the code drain pre-acknowledgement events, sample startup left-button state and mark the rectangle input stream ready. `refresh_shield_coverage` separately repeats a very similar strict test during topology changes. Both need coherent behavior.
6. The Pinboard GUI wraps this native error as `screen selection failed: ...` via the existing capture-state reporting path (`src/gui/pinboard.rs`).

### Key uncertainty — do not claim a root cause before measuring it

**Potentially important hypothesis:** `GetWindowRect` is documented as DPI-virtualized, while `GetMonitorInfoW.rcMonitor` may be used as a physical-monitor coordinate source. Strict equality is unsafe **if** those paths run in different effective DPI coordinate contexts. Mixed DPI, non-primary monitors, signed negative origins and thread DPI-awareness changes are worth inspecting. This is *not proven* to be the user's failure. Other candidates are visibility, z-order/style readback, window creation timing, and a window API returning an error. Investigate **which exact condition is false** before modifying behavior.

Official Windows API references (informative, not instructions to add dependencies):
- [GetWindowRect — DPI virtualization](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect)
- [SetWindowPos — topmost placement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- [IsWindowVisible — visibility style, not proof of unobscured input](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible)

### Directly relevant files (verify paths in actual checkout)

| Area | Owners | Reason |
|---|---|---|
| Native shield construction, acknowledgement and topology repair | `src/gui/mkmacro_dialog/visual_overlay_windows.rs` | Primary defect boundary; failure text, `shield_covers_monitor`, Win32 style/rect probes, error cleanup. |
| Rectangle picker plan and lifecycle | `src/gui/mkmacro_dialog/visual_overlay.rs` | All rectangle purposes share the shield contract; `input_shield_plan`, start/cancel and event ownership. |
| Shared selector and Pinboard error handoff | `src/gui/mkmacro_dialog/visual_capture_workflow.rs`, `src/gui/pinboard.rs` | Pinboard starts selection, displays the error, parks/restores launcher correctly. |
| Native owned regression harness | `src/gui/mkmacro_dialog/visual_overlay_rectangle_smoke.rs`, `src/bin/passive_overlay_smoke.rs` | Existing test-owned EDIT/BUTTON receiver verifies no leaked Down/Up/click, completion and cleanup; **developer binary is opt-in**. |
| Cargo smoke opt-in | `Cargo.toml` | `passive_overlay_smoke` has `required-features = ["dev-smoke-binaries"]`; do not change ordinary default build target policy. |
| Existing test names | `rectangle_shield_acknowledgement_rejects_missing_invisible_or_transparent_coverage`, `rectangle_shield_native_events_preserve_rapid_transaction_and_signed_positions`, `rectangle_startup_baseline_is_taken_after_coverage_acknowledgement`, `rectangle_guard_*`, `shield_plan_preserves_negative_origins_mixed_sizes_and_offsets` | Direct tests to extend, not a reason to rerun unrelated suites. |

## 3. Mandatory Codex execution protocol

- **Code-first:** Read `AGENTS.md`, `git status -sb`, the direct native owner and adjacent tests. Do not spend a milestone collecting exhaustive baseline metrics; then implement checkpoint by checkpoint.
- **Single writer, bounded reviewer:** One implementer modifies the source. Optional independent read-only reviewer after the first diagnostic and after the core repair; fix concrete findings only. Do not repeatedly restart planning, open-ended exploration, or another generic Windows overlay redesign.
- **Frequent small commits:** Commit coherent checkpoints labeled `M1-A`, etc. after direct tests, `git diff --check`, and a limited diff review. Avoid empty/WIP commits or one giant final source commit.
- **Best-effort push:** Follow current `AGENTS.md`: commit locally first, attempt **one** normal push after each checkpoint if remote/permissions permit; never force-push, switch branches, reset or retry denied pushes in a loop. Failure to push does not stop implementation.
- **Autonomous execution:** Codex may build/run its own test instance of Multi Launcher, create test-owned Win32 windows and fixture controls, take safe screenshots and stop **only** processes it created. It must not ask the user to execute tests, manually capture screenshots, or approve intermediate milestones. Use temporary fixtures, never personal app data.
- **Validation budget:** Per checkpoint normally one focused Nextest slice, **at most two focused slices** if truly crossing a second owner. One native interactive smoke session at the repaired-feature milestone, **one diagnostic attempt plus one corrected retry** only if there is new useful evidence. If native desktop access is denied (e.g. `GetCursorPos` access denied), record `not_run_environment` with the exact stage; do not treat that as the shield defect or claim a native PASS. Use a no-injection native probe / deterministic tests if possible.
- **Failure budget:** For the *same* unchanged failure signature, at most **two substantive corrective iterations**, each with one narrow recheck. If still unresolved, log the root reason, keep the fail-safe active, record the exact blocker and stop expanding scope. Do not mask it with sleeps, tolerance or skipped tests.
- **Test order:** Focused tests only during M0–M3. One full `cargo nextest run --workspace` may run at **M4-A** (one repaired rerun maximum), not during implementation. Complete formatting/help/evidence and non-lint commits before **M4-C**, the **only Clippy phase**. Clippy applies to maintained application crates; ignore vendored crate lint output and never alter vendor source solely to satisfy lint. Final successful Clippy invocation is the **last automated verification**; immediately report and stop.

## 4. Milestone / checkpoint overview

| Milestone | Checkpoints | Goal | Acceptance gate |
|---|---|---|---|
| **M0** | M0-A | Minimal source baseline and reproduction path | Owner, applicable tests and reproduction steps known. |
| **M1** | M1-A, M1-B | Per-shield diagnostics and bounded root-cause evidence | Exact failing invariant can be distinguished; no unsafe picker arming. |
| **M2** | M2-A, M2-B | Narrow underlying Win32 fix plus unified coverage/cleanup | Shield is correctly placed, verified and safe to use. |
| **M3** | M3-A, M3-B | Owned native and adjacent integration proof | Pin capture completes without underlying click events; negative/cancel cases protected. |
| **M4** | M4-A, M4-B, M4-C | One final Nextest, tidy report/docs, Clippy last | Honest final verification + small committed patch series. |

---

## M0 — Confirm source ownership and baseline quickly

### M0-A — Exact failure path, toolchain and fixture selection

**Implementer tasks**

1. Confirm the active checkout, current HEAD, clean/dirty working tree, Windows target/toolchain and current `AGENTS.md`. Respect existing uncommitted user edits.
2. Verify the exact producer of the string `Rectangle input shield coverage acknowledgement failed`; trace only the call from `pinboard capture` through native `show` and back to Pinboard UI. Confirm shared consumers, the `refresh_shield_coverage` path, and current smoke-test entry points.
3. Record a small diagnostic matrix of possible failing conditions: `GetWindowRect` API failure, rectangle mismatch/DPI context, `IsWindowVisible == false`, missing `WS_EX_LAYERED`, missing `WS_EX_NOACTIVATE`, missing `WS_EX_TOPMOST`, unexpected `WS_EX_TRANSPARENT`, missing/incorrect HWND or z-order. The matrix is a development aid, not a massive report.
4. Confirm the smoke harness is opt-in (`cargo run --features dev-smoke-binaries --bin passive_overlay_smoke -- --rectangle-input-isolation`); no need to run it until the diagnostic/code fix is ready.

**No broad test run, no Clippy, no baseline performance work.** Begin M1-A promptly.

**Done:** Failure boundary and tests located with no speculation labeled as fact. No commit required solely for reading source; use a brief progress entry if it serves the active task.

---

## M1 — Give the failure a precise cause

### M1-A — Typed per-shield acknowledgement report (without weakening validation)

**Goal:** Replace a single ambiguous boolean/error with a narrow, diagnostic account of which invariant failed. Keep exactly the same gate until the root cause is established.

**Implementation tasks**

1. Introduce a compact typed probe/result or equivalent in the native shield owner, used by both initial startup acknowledgement and topology revalidation. Capture one stable snapshot per shield: operation ID, shield HWND, monitor index, expected monitor bounds, returned screen rectangle, query success/errors, `IsWindowVisible`, extended-style word and individual checked/missing/unexpected style flags.
2. Capture the Win32 failure code **immediately when an API fails**, not after other Win32 calls that could overwrite `GetLastError`. For `GetWindowLongPtrW`, distinguish a genuine zero result/failure per API semantics rather than report an unrelated stale last error.
3. Include effective awareness/DPI data **only where it illuminates a mismatch**: current thread/window DPI awareness context, monitor DPI and the coordinate-space assumptions of `EnumDisplayMonitors` and `GetWindowRect`. No process-wide DPI setting changes in this checkpoint.
4. Create explicit reasons, for example `window_rect_query_failed`, `rect_mismatch`, `window_not_visible`, `layered_missing`, `noactivate_missing`, `topmost_missing`, `unexpected_transparent`, `missing_shield`. Preserve the existing `Platform` failure classification and event/lifecycle semantics.
5. Log compact detailed diagnostics through the application's existing `tracing` path (no HWND-dependent sensitive text, no screenshot, no clipboard, no user window titles/paths). The user-facing error should identify the monitor and category concisely, e.g. `Screen selection could not start: shield on monitor 2 has a position/size mismatch`. Do not dump huge raw Win32 details into the one-line launcher status toast.
6. Ensure **failure still closes partial shields and never marks rectangle input ready**. No new retry timers or bypass paths.

**Focused tests**
- Extend existing shield-ack test to assert a specific reason for each missing style/visibility/rect/failure case.
- Verify signed negative monitor bounds, missing shield count and that a rejected acknowledgement never reaches the input-ready state.

**Commit:** `fix(pinboard): [M1-A] report failed shield acknowledgement invariants`.

**Done:** On the user's observed failure class, diagnostics can discriminate the responsible condition; behavior remains fail-closed.

### M1-B — One bounded reproduction/probe and root-cause decision

**Goal:** Use the new diagnostics to select **one justified fix**, not try arbitrary Windows workarounds.

**Implementation tasks**

1. If an interactive Windows desktop is available, launch only a test-owned overlay/receiver. Exercise initial shield creation/acknowledgement **without synthetic mouse input first**, and retain the single failing probe snapshot if it fails. If the existing full smoke harness requires `GetCursorPos`, prefer a tiny *reused* no-input shield probe at the same native owner so desktop approval problems do not obscure geometry information.
2. Run a single owned `pinboard capture` launch path only if the environment allows it, using an isolated app-data location and no user data. Record whether the shield fails before input arming and the exact diagnostic category. If the owned app cannot be accessed, record `not_run_environment` after at most one diagnostic/retry; proceed with a source-grounded, deterministic path if the evidence supports it.
3. Classify the result: coordinate mismatch and differing DPI context; visibility/position initialization issue; style readback/topmost issue; Win32 query error; or unrelated concrete finding. Review Microsoft API semantics and the call ordering for **that** result only.
4. Write a **short decision note** in the ledger or final commit message: *observed condition → specific root-cause hypothesis/evidence → repair boundary*. Avoid expanding into nearby screenshot or mouse systems.

**Focused verification:** Same shield probe test or one owned no-input native probe. Avoid full screenshot capture/injection at this diagnostic checkpoint. **Do not perform open-ended native retries**.

**Commit:** `test(pinboard): [M1-B] reproduce shield readiness failure with exact diagnostics` only if a reusable native probe/test or meaningful source change was created. Otherwise record evidence in ledger; don't manufacture a commit.

**Done:** The repair is evidence-driven. If access blocks native reproduction, the report explicitly differentiates reproducible source conditions from unverified desktop assumptions.

---

## M2 — Fix the root cause and align both shield-coverage checks

### M2-A — Correct shield bounds, style or initialization at the native owner

**Goal:** Make the exact failed invariant true in reality while retaining strict, complete physical-monitor coverage.

**Conditional implementation rules**

- **If DPI/rect mismatch:** Establish a consistent **physical-pixel coordinate convention** for monitor enumeration, `CreateWindowExW`/`SetWindowPos`, observed bounds and native pointer/mouse coordinates. Inspect effective thread/window awareness first. Use existing project-aware Win32 DPI helpers or a narrowly scoped RAII DPI awareness context when justified; restore the previous context. Avoid globally changing the process/winit/egui DPI awareness or comparing logical pixels directly with physical pixels. Verify true physical pixel coverage, not merely scaled numerical equality.
- **If observed rectangle differs for a specific window style:** Determine whether invisible nonclient borders, window sizing or a real wrong position are responsible. Compare windows using the appropriate API/coordinate convention **without introducing a tolerance that leaves real screen gaps**. Keep Win32 popup shield hit-testable over the entire intended monitor.
- **If invisible or style/topmost issue:** Correct the ordering/arguments to `CreateWindowExW`, `SetLayeredWindowAttributes`, `SetWindowPos`, `ShowWindow` or Z-order operations as supported by collected probe data. A success return from one positioning call is not by itself sufficient acknowledgement; confirm its resulting state. Avoid arbitrary sleeps. A single bounded, message-driven readiness acknowledgement is acceptable only if evidence shows an actual transient window-manager transition.
- **If Win32 API error or desktop/session issue:** Handle and report the actual API error; correct invalid handles/thread ownership or unsupported desktop initialization when feasible. Fail safely when the environment truly cannot create input-isolating windows; don't bypass policy.

**Shared requirements**

1. Apply the fix to the **shared startup and `refresh_shield_coverage` verification boundary**. Do not create one relaxed check for startup and a different conflicting strict check for monitor changes.
2. Preserve `one shield per physical monitor`, monitor gaps, negative origins, mixed DPI, transparent visuals above the nontransparent shields, `WM_MOUSEACTIVATE` noactivation, alpha `1` hit-testability and verified topmost/exclusive input ownership.
3. Keep `GetWindowRect`/visibility/style readback and failure handling deterministic; the screen selection must not be armed until every shield is ready and the baseline is taken.
4. Avoid pin-only bypasses of shared rectangle selection. Adjust shared code only where required to fix the common boundary; other selection purposes must not regress.

**Focused tests**
- Add/extend geometry/DPI coordinate tests for 100%, 125%, 150% assumptions and multiple monitors with negative coordinates, using synthetic monitor rectangles when multiple real monitors are unavailable.
- Verify exact coverage acceptance and actual missing coverage failure. Keep original `rectangle_shield_acknowledgement_rejects_missing_invisible_or_transparent_coverage` negative cases; no loosening of style flags or overlapping holes.
- Test the corrected probe semantics for both startup and topology-refresh use.

**Commit:** `fix(pinboard): [M2-A] correct native shield coverage acknowledgement`.

**Done:** Under supported conditions a valid fully covering shield passes both startup and refresh; missing/invisible/transparent/incomplete shields remain rejected.

### M2-B — Preserve startup arming, transaction integrity and teardown

**Goal:** Ensure the repair cannot reintroduce the original click-through problem or trap mouse input after failure.

**Implementation tasks**

1. Verify ordering: install all shield HWNDs → set correct attributes/z-order → verify full coverage → drain pre-acknowledgement input → capture held-button baseline → expose picker as Ready. No native event from before Ready can start selection.
2. Keep `WM_LBUTTONDOWN`, held drag/`WM_MOUSEMOVE`, and `WM_LBUTTONUP` consumed by the native shield; preserve `SetCapture/ReleaseCapture`, signed client-coordinate conversion and the existing `RectangleNativeInput` state transitions.
3. Validate rapid down/up, initial launch click still held, mouse release across monitors, interruption (`WM_CAPTURECHANGED`), Escape, display hotplug/topology change, failed partial install, replacement picker start, close while held and application exit. Preserve the existing deferred-release cleanup semantics.
4. Ensure that a shield failure (including a topology refresh failure) **cannot leave orphan windows or a captured button**, and that the next unrelated click reaches its normal destination once cleanup is complete.
5. Ensure errors are surfaced once, with no duplicate toast loop or repeated failed capture start on ordinary GUI frames.

**Focused tests:** `rectangle_shield_native_events_preserve_rapid_transaction_and_signed_positions`, `rectangle_shield_interruption_retains_owned_transaction_until_release`, `rectangle_guard_*`, `rectangle_startup_baseline_is_taken_after_coverage_acknowledgement`, and one new fake-native failure/cleanup case, with a **single combined slice** when practical.

**Commit:** `fix(pinboard): [M2-B] preserve guarded picker arming and release cleanup` (only if actual code/tests changed; otherwise cite existing passing tests and no artificial commit).

**Done:** Successful or failed picker lifetime neither leaks an input shield nor lets the selection gesture fall through.

---

## M3 — Prove real behavior and adjacent compatibility

### M3-A — Native smoke on an owned receiver only

**Goal:** Prove a valid selection becomes active and correctly absorbs mouse input on an accessible Windows desktop.

**Native execution command (the developer binary is opt-in):**

```powershell
cargo run --features dev-smoke-binaries --bin passive_overlay_smoke -- --rectangle-input-isolation
```

**Implementation/verification tasks**

1. Prefer the existing production-path smoke harness; it already owns harmless EDIT/BUTTON receiver controls, checks native mouse Down/Up/commands, unchanged text selection, rectangle result, held-Escape teardown and next ordinary click. Do not rewrite it or create a second large harness.
2. First verify native shield readiness **without injecting input**. Only after a valid input-shield acknowledgement, run the already-owned guarded gesture and record exact expected/actual rectangle, receiver click counts and absence of unexpected selection. All activity must stay in Codex-owned fixtures, never the user's real desktop apps.
3. Where possible include one monitor at non-100% scale or secondary monitor, but do not make multi-monitor hardware a hard prerequisite; use synthetic tests when absent. Keep run bounded (one native smoke + at most one justified rerun after a code fix).
4. Confirm the original error is no longer produced for a valid desktop and that an intentionally failing shield still returns a precise error *before input*. Prove the follow-up ordinary click reaches the owned receiver after capture/cancel.
5. If `GetCursorPos` or computer-use access is denied before any native fixture/injection (as occurred in an earlier Codex environment), record exact Win32 failure and `not_run_environment`, stop trying different desktops/privileges, and rely on the no-input readiness probe and focused deterministic tests. **Do not label input pass-through verified**.
6. Capture one or two non-sensitive screenshots if the owned native fixture reaches a stable state. Screenshots are evidence, not a pixel-perfect regression gate.

**Commit:** `test(pinboard): [M3-A] verify native rectangle input isolation` only if tests/fixture changed; otherwise reference the test run in the ledger.

**Done:** Native smoke passes where supported; otherwise the unverified scope is clearly documented without hiding a failed real check.

### M3-B — Confirm Pinboard command integration and direct regressions

**Goal:** Verify the actual launcher command's behavior and avoid damage to other screenshot/rectangle functions.

**Implementation tasks**

1. Exercise `pinboard` → **Capture Screenshot Pin** through the actual command/GUI state route (owned instance if possible, deterministic GUI state tests otherwise). Confirm successful region selection creates exactly one image pin and cancellation creates none.
2. Verify `Show Screenshot Pins`, pin close/reopen and main launcher visibility still work. Preserve screenshot editor/`ss`, OCR, MkMacro region selection and Screen Draw shared selector ownership.
3. Verify no pending selection remains on failed shield acknowledgement; retrying a corrected capture on the next user invocation is allowed without restarting Multi Launcher.
4. Ensure the final user-visible failure message includes actionable diagnostics when appropriate but does not expose private clipboard/image/window text.

**Focused verification:** At most two directly affected slices combining Pinboard command capture and shared selector compatibility. Reuse M2 passing unit tests rather than rerunning a huge legacy screenshot suite.

**Commit:** `test(pinboard): [M3-B] protect pin capture and shared selection routes` if meaningful changes exist.

**Done:** User-facing screenshot command works with input shielding, with cancellation and adjacent screenshot actions intact.

---

## M4 — Final evidence, full tests and Clippy last

### M4-A — One full Nextest pass after the functional fix

1. Once M1–M3 complete and direct focused tests pass, run **one** `cargo nextest run --workspace` on the supported Windows toolchain. Do not run `--all-features` by default just to pull in opt-in smoke targets.
2. If a genuine failure is introduced by the repair, inspect the first root cause, fix the narrow code/test seam and confirm with one focused rerun. Then allow at most **one** full-suite rerun after that correction. Do not spiral into unrelated old tests or performance benchmarking.
3. If full Nextest cannot run for an environment reason, report the exact command/failure and preserve already verified focused evidence. Do not mislabel environment or vendor warnings as maintained-code test failures.

**Commit:** `test(pinboard): [M4-A] close full-test regression findings` only if source/tests changed; no empty checkpoint commit.

### M4-B — Documentation, cleanup and checkpoint summary **before Clippy**

1. Retain detailed per-shield failure diagnostics because they will help distinguish later multi-monitor or DPI regressions. Ensure they are bounded and only log on failed init/refresh or at explicit debugging level, not every frame.
2. Remove temporary debug code, sleeps, duplicate probe helpers, accidental formatter churn and unsafe test-only bypasses. Keep production path readable. Update relevant help text only if the new error wording or user workflow actually changed.
3. Ensure touched maintained source is formatted, `git diff --check` passes, Windows build paths remain correct and the existing developer smoke target stays feature-gated. Finish ordinary compile/test checks, fixture cleanup, screenshots and documentation **now**, not after Clippy.
4. Write a small result section in a checkpoint note/ledger: exact failing invariant (if observed), changes made, the two places reusing the fix, scoped/native/full test outcomes, remaining environmental limitations. Record local commit IDs; do not create lengthy progress diary.
5. Commit all non-lint final changes. **No Clippy yet.**

**Commit:** `docs(pinboard): [M4-B] finalize shield diagnostics and repair evidence` when there are real changes.

### M4-C — Final maintained-crate Clippy gate; STOP afterwards

1. **Run Clippy only here**, not during earlier checkpoints. Use the actual maintained workspace package(s), exclude vendored workspace members if any and do not modify vendored source to address third-party lints. Typical command for this repository:

   ```powershell
   cargo clippy --workspace --all-targets -- -D warnings
   ```

2. If project-owned Clippy warnings occur, fix them without broad redesign. Use at most two meaningful correction passes per unchanged class of warning; focused correctness rechecks and `cargo fmt` for lint fixes must happen **before** the final clean Clippy command.
3. Commit any lint-only changes, then run the **final successful Clippy command as the last automated verification**. Do not format, test, edit sources, start a feature, run benchmarks or update tracked files afterward.
4. Report results directly, including any unresolved lint diagnostic and its status, not a false green label.

**Commit:** `chore(clippy): [M4-C] clean shield repair lints` only if actual maintained code changes. The last Clippy pass follows the final lint commit.

---

## 5. Acceptance checklist

- [ ] `pinboard` → Capture Screenshot Pin no longer returns a generic shield-ack error on supported Windows desktop; if it fails legitimately the **specific invariant** is visible and logged.
- [ ] All input shields cover **every physical monitor** in the correct coordinate space, including negative-origin/mixed-DPI monitors. No actual uncovered physical pixels are accepted.
- [ ] No rectangle picker is armed before shield acknowledgement and startup-button baseline.
- [ ] Mouse Down, drag and Up do **not** reach underlying EDIT/BUTTON controls; an ordinary later click works after cleanup.
- [ ] Escape, held mouse button, capture lost, monitor topology changes, partial initialization failure and app exit clean up the native owner without orphan windows or stuck capture.
- [ ] `refresh_shield_coverage` and first startup follow one coherent, strict policy.
- [ ] Valid Pinboard capture creates a pin; failed/cancelled selection creates none; screenshot editor, OCR, MkMacro and Screen Draw retain their input semantics.
- [ ] Focused tests passed; actual Windows smoke outcome is recorded honestly; one final full Nextest is run when feasible; final maintained-crate Clippy run occurs last.
- [ ] Commits are small/labeled, best-effort pushed once each, with no branch resets, no manual user verification requests and no unrelated refactors.

## 6. Stop conditions and final report

**Stop extending the fix** once the actual failed invariant is identified and corrected, focused tests cover both acceptance and negative cases, the owned native probe/smoke has produced its bounded result, and final verification is complete. If accessible native behavior remains broken after the bounded repair budget, **do not bypass the input shield** or report the problem solved; retain precise diagnostics and report the blocker.

The final Codex report must contain: current branch/HEAD; checkpoint commit hashes and push statuses; exact observed failure category (or explicitly unobserved), explanation of actual root-cause correction, proof that startup/refresh share the fix, focused test commands/counts, optional native smoke and screenshot evidence, full Nextest outcome, final Clippy command and result, and any remaining genuine issues. Never ask the user to test or approve mid-run; after reporting, stop.

**Final directive:** Code first, evidence-driven Win32 fix, small commits, no input-safety regression, bounded tests, full Nextest only at the end, Clippy truly last.
