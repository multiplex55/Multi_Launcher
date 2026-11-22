# G22 — Pinboard, Screen Draw, OCR, and coordinate overlays

**Status: not started.** Checkpoints A–D are not started. Work on the single initiative branch. Preserve no-argument egui and opt-in native GPUI.

## Objective and dependencies

Recreate the complete overlay/capture feature family and its control windows while retaining exact Windows interaction, geometry, cancellation, resource, and capture-ownership behavior. Include static/live pins and Pin List, Screen Draw and toolbar/export flows, OCR selection/results, coordinate capture/HUD, crosshair, click visualization, halo, magnifier, and Mouse Settings.

Requires G00–G04 and G05. **Before G22 implementation, G01 must have investigated and proved the required hard native platform approach. G22 consumes those results; feasibility must not be postponed until this goal.** Any unresolved required G01 capability blocks its dependent surfaces. Coordinate with G18 screenshot/color/crop, G19 macro capture/recording, and G20 radial handoff. Follow [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

G06 owns the Settings shell; G22 owns specialized overlay and Mouse Settings controls and their end-to-end evidence.

## Source seeds and ownership

Baseline: `multiplex55/Multi_Launcher`, `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [Pinboard UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/pinboard.rs), `src/gui/pinboard_windows.rs`, and `src/pinboard/`.
- [Screen Draw toolbar](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/screen_draw_toolbar.rs), `src/gui/screen_draw_restore.rs`, and the native canvas/runtime/overlay files in `src/screen_draw/`.
- [OCR result view](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/ocr_view.rs), `src/gui/ocr.rs`, and `src/ocr/`.
- [Coordinate rendering](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/coordinate_tool/render.rs), its controller/native/settings modules, and [Mouse Settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mouse_settings_dialog.rs).

Independent non-egui native drawing/capture substrates and services may remain shared; this goal does not require rewriting them. No active GPUI session may open an egui viewport, and every egui toolbar/settings/control surface requires a GPUI counterpart. Existing controllers own capture admission, signed physical coordinates, cleanup, and document state. GPUI adapters own frontend windows, control focus, textures, and view events.

## Per-GUI parity scope

Pinboard includes capture, live-source selection, every pin toolbar, Pin List/recovery, Fit/1:1, zoom, resize/move, topmost/click-through, opacity, pause/resume, status/errors, Copy Image, Save As, and closing. Preserve session-only data, shared eight-pin/256 MiB limits, last-completed-frame semantics, source loss/recovery, and desired versus acknowledged native state. Display opacity must not alter exported pixels. Recover controls must remain keyboard reachable.

Screen Draw includes frozen desktop canvas, all existing tools, text, eraser, fading ink, eyedropper, thickness/colors/backgrounds, undo/redo, visibility, toolbar placement/orientation, Drawing/Ghost/Finish, emergency pause, New Capture, export region/destination/background, and screenshot-editor handoff. Preserve Escape precedence, release of pointer/gesture ownership, negative-origin mixed-DPI coordinates, display-change recovery, and export after native windows close. Do not add roadmap stylus/multitouch work.

OCR includes region selection, recognizing/no-text/error states, editable result, Copy All, Re-capture, and Close. Preserve local language/capability errors and transient text lifetime, including editor undo history. Coordinates include desktop/monitor/client spaces, freeze/copy/pick/cancel, last-external-target behavior, passive overlays, and all appearance settings. Mouse Settings enablement stays transient; opening settings does not enable an effect. Include every setting-only rendering path outside `src/gui` in ownership discovery.

## Checkpoints

### G22-A — Bind proven native contracts to actual controllers

**Status: not started.** **Owner:** G22 implementer and shared native controllers.

**Work:** reconcile G01 results with every existing native path; enumerate surface/state ownership; implement selected-frontend adapters and cleanup acknowledgements.

**Invariants:** one capture owner; no duplicate hooks; physical/logical coordinate conversion is explicit; failed cleanup prevents conflicting admission.

**Evidence:** G01 reference, controller tests, representative signed-coordinate fixtures, and one real GPUI-owned overlay lifecycle.

**Done:** every capability has an implemented adapter or explicit blocking dependency; none is silently assumed.

### G22-B — Complete Pinboard and OCR

**Status: not started.** **Owner:** GPUI pin/result adapters; shared capture/OCR services.

**Work:** implement static/live pins, lists/recovery, opacity acknowledgements, clipboard/save, OCR selection/results/editing, and all failures/cancellation.

**Invariants:** capture restoration precedes publication; stale generations cannot replace current pins/results; unavailable sources preserve allowed last-good content.

**Evidence:** focused controller/UI cases and native static/live capture, click-through recovery, opacity, and OCR smoke checks.

**Done:** all Pinboard/OCR parity rows satisfy behavior and Windows acceptance.

### G22-C — Complete Screen Draw and mouse overlays

**Status: not started.** **Owner:** GPUI toolbar/settings adapters and native presentation integration.

**Work:** recreate all drawing tools/modes/export and coordinate/crosshair/click/halo/magnifier controls, plus Mouse Settings and emergency recovery.

**Invariants:** passive surfaces do not steal input; captured press/release stay paired; canceled picks do not copy; display changes preserve recoverable documents.

**Evidence:** production-view tests plus native drawing/ghost/resume/export, physical-coordinate, and mixed-DPI checks.

**Done:** every tool, setting, mode, and recovery interaction is present and verified.

### G22-D — Verify arbitration, cleanup, and fallback

**Status: not started.** **Owner:** G22 implementer; review focuses on native ownership transitions.

**Work:** exercise representative conflicts with screenshot, color, macro, recorder, and radial owners; close/reopen/error paths; independently verify egui after shared changes.

**Invariants:** no orphan overlay/capture resource; inactive frontend initializes nothing; no late result reopens a dismissed surface.

**Evidence:** scoped arbitration tests, native transition records, resource observations, and completed parity rows.

**Done:** applicable platform and UI evidence is complete and checkpoint commits are recorded.

## Focused validation and continuation

Actual tests include `fake_capture_waits_for_both_native_acknowledgements_and_restores_before_commit`, `missing_root_ack_never_dispatches_selection_or_captures_pixels`, `pinboard_repeated_capture_selector_error_restores_once_and_allows_fresh_attempt`, `zoom_uses_signed_physical_monitor_bounds_not_work_area_or_dpi`, and `toolbar_viewport_policy_is_fixed_topmost_and_absent_from_taskbar`. Discover exact compiled filters. Existing `coordinate_tool_smoke`, `passive_overlay_smoke`, and `pin_opacity_smoke` binaries can supply bounded relevant checks after inspection; their existence is not proof of GPUI behavior.

Headless results cannot establish click-through, OS focus, capture exclusion, or monitor geometry. Keep native evidence separately pending until observed. Finish all required surfaces, update the ledger, and continue. If a required platform capability fails, preserve work and report the precise blocker rather than accepting reduced behavior or an egui substitute.
