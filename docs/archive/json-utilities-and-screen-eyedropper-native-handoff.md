# JSON Utilities and Screen Eyedropper — Native Implementation Handoff

This supplements `json-utilities-and-screen-eyedropper.md` for checkpoints M2-B and M2-C. The active plan and AGENTS.md remain authoritative. The plan matches the inspected source. Planning was read-only: no builds or tests were performed.

## M2-B — Frozen-desktop runtime and reusable capture parking

**Objective:** Create a testable, asynchronous eyedropper runtime that captures once after verified launcher parking, opens a minimal native picker, samples frozen pixels, and publishes `Picked`, `Cancelled`, or `Failed` after native teardown.

**Why / ownership:** Color owns picker state and interaction. `mkmacro::screen` remains the capture/compositor owner. Launcher parking becomes reusable native infrastructure, with feature-specific session identities retained.

### Current-state facts

- `src/mkmacro/screen.rs` exposes `ScreenRect { x: i32, y: i32, width: u32, height: u32 }`, its `validate_capture`, `right`, `bottom`, and `contains` methods; `CapturedRegion { image: RgbaImage, origin: (i32, i32) }`; `CapturedRegion::local_point` and `desktop_point`; `ScreenCaptureBackend::capture(&SearchRegion::Desktop, cancelled)`; and `WindowsScreenCaptureBackend::system`.
- Shared composition uses physical monitor captures, preserves signed origins, validates dimensions, and fills monitor gaps with opaque black. Do not duplicate it.
- `src/screen_draw/capture.rs::ScreenDrawCaptureBackend` adapts the shared backend. Its `DesktopCaptureBackend` and `LauncherVisibilityProbe` traits are Screen Draw scoped.
- `src/screen_draw/launcher_parking.rs::LauncherParkingTransaction` snapshots the native launcher rectangle, moves the visible non-minimized HWND completely outside the signed virtual desktop, and preserves an exact restoration point. It exposes `verify`, `restore`, `repark_after_stale_restore`, and `retain_restore_point_after_fallback_park`. Dropping an `Active` transaction attempts restoration. Dropping after `commit_hidden` does not restore; that deliberate Screen Draw behavior is unsuitable as the eyedropper's default cleanup policy. The transaction currently embeds `ScreenDrawGeneration`.
- `system_launcher_is_capture_safe` requires a valid, visible, non-iconic HWND whose native rectangle does not intersect the capture. Egui visibility alone is insufficient.
- `ScreenDrawController::poll_capture` issues parking, verifies on a later event-loop turn before spawning capture, and implements bounded parking/capture deadlines and cancellation.
- `src/screen_draw/native_canvas.rs` creates a popup over physical capture bounds. `WindowState::cursor_point` uses `GetCursorPos`, avoiding 16-bit message coordinates. `BackingDib` copies RGBA to top-down BGRA and restores the old selected bitmap before deleting GDI resources.
- `src/screen_draw/native_runtime.rs::NativeSessionHandle` owns its native worker nonblockingly; completed threads are joined only after they finish.
- DPI awareness comes from winit rather than repository-owned process initialization. The locally inspected locked winit 0.29.15 implementation defaults to DPI awareness and requests per-monitor V2 with older fallbacks. `src/main.rs` uses normal eframe event-loop creation. No repository manifest override was found.

### Scope

- New color-owned runtime/state/native modules, registered through `src/lib.rs`; names may follow the representation introduced by M2-A.
- Narrow extraction of `src/screen_draw/launcher_parking.rs` into a shared crate module.
- Parking import/type migration in `src/screen_draw/capture.rs`, `src/gui/mod.rs`, `src/gui/render.rs`, `src/gui/screen_draw_toolbar.rs`, `src/gui/command_host.rs`, and `src/visibility.rs` (which already imports parking geometry utilities).
- Relevant existing parking tests and fixtures move with their shared implementation.
- No launcher command/UI integration yet.

### Required changes

1. Extract existing parking rather than copying it. Remove its dependency on Screen Draw's session type while retaining typed ownership. A transaction parameterized by a feature generation type is a suitable narrow option; Screen Draw must continue using `ScreenDrawGeneration`. Preserve current commit/drop/repark behavior.
2. Build color-owned session state with one immutable `Arc<CapturedRegion>`, a typed session identity, hovered capture pixel, and terminal outcome.
3. Expose a nonblocking coordinator that requests parking before capture and requires successful later verification. Inject capture and native factory seams for tests. Actual HWND transaction ownership belongs to M2-C's GUI integration.
4. Spawn shared desktop capture off the GUI thread. Use cancellation and bounded startup failure handling. Associate completions with session identity and ignore stale completions.
5. Create the picker only after capture succeeds. The native worker owns its HWND, pixel backing, message loop, and resource teardown.
6. Keep a minimal physical-pixel popup covering the frozen capture rectangle. Read the pointer through `GetCursorPos`; translate using `CapturedRegion::local_point` or equivalent checked signed arithmetic.
7. Render the original frozen background, pointer target, nearest-neighbor magnifier, and exact center-pixel marker. All previews and selection derive from the immutable snapshot.
8. Use a centered magnifier sampling grid. Clamp individual sample coordinates at edges so the marked center remains the hovered pixel rather than shifting the source window.
9. Mouse movement updates the sample; left-click samples the click's current physical desktop position and accepts; Escape cancels. Reject invalid positions and ignore unrelated input.
10. Publish exactly one terminal outcome only after input is disarmed and native surfaces are destroyed. Initialization failure, message-loop error, channel loss, panic, and shutdown converge on cleanup.
11. End safely with actionable failure/cancellation if display geometry changes, rather than remapping a stale screenshot. Use physical coordinates consistently; do not apply egui's root scale to desktop pixels.
12. Preserve a non-Windows unsupported boundary with a useful error.

### Invariants

- No second desktop compositor or live `GetPixel` sampling.
- Subtract signed desktop origin before unsigned indexing; right/bottom capture edges are exclusive.
- Selection is exact independently of overlay/magnifier rendering.
- HWND/DIB/DC creation and destruction occur on the owning native thread.
- Worker teardown never blocks egui updates.
- Screen Draw's exact-rectangle restoration, committed parking, cycle identity, and stale-repark semantics remain intact.
- Runtime performs no clipboard operations or HEX/RGB/HSL calculations.

**Non-goals:** Screen Draw controller/runtime redesign, drawing tools, history, continuous capture, additional color formats, launcher query restoration.

**Dependencies:** M2-A's shared color representation is available for picked RGB. Runtime must not require a plugin instance.

### Tests

- Signed origin mapping, negative X/Y, top-left/bottom-right pixels, and out-of-bounds rejection.
- Exact RGBA-to-RGB selection, frozen ownership, and no recapture during movement.
- Magnifier center/edge extraction with the sampled pixel remaining the marked center.
- Accept, cancel, failure, stale completion, repeated request, and teardown ordering.
- Fake capture proves capture starts only after parking verification.
- Native factory failure/channel closure releases session ownership.
- Preserve existing parking tests, particularly `active_transaction_restores_exact_snapshot_on_drop`, `committed_transaction_stays_parked_on_drop`, `explicit_restore_is_idempotent`, `intersection_uses_signed_half_open_coordinates`, `capture_safety_requires_valid_visible_non_iconic_offscreen_window`, and `failed_stale_repark_keeps_exact_restore_point_for_later_cleanup`.

**Verification:** Run new runtime library-test and extracted parking module filters through targeted Nextest. After choosing module names, `cargo nextest run --lib launcher_parking` is appropriate if that remains the module name. A Windows-target compile/check is justified for new FFI. Do not run the entire suite.

### Done criteria

- Shared compositor supplies the only snapshot.
- Tests prove park → verify → capture → native session ordering.
- Exact pixel and centered magnification work across signed layouts.
- Success/cancel/error terminate once after cleanup.
- Parking migration preserves behavior and targeted tests pass.
- Commit boundary: `feat(color): [M2-B] add frozen-desktop screen eyedropper runtime`.

**Genuine uncertainties:** Native sanity must verify inherited process DPI context on the actual desktop. If a pre-existing external DPI context constrains initialization, handle that at the picker's native boundary; do not introduce speculative application-wide DPI changes.

## M2-C — Typed command and launcher lifecycle integration

**Objective:** Make `color pick` start the runtime, protect parking ownership, restore launcher state after cleanup, and route selection through ordinary color results without writing the clipboard.

**Why / ownership:** Typed commands initiate the feature. Color runtime owns session behavior. `LauncherApp` owns query/results and coordinates exact native restoration with ROOT visibility authority.

### Current-state facts

- `src/plugins/color_picker.rs::ColorPickerPlugin::search` generates three `clipboard:` actions from a color. Before M2-A, `color pick` is rejected by HEX parsing.
- Normal activation follows `parse_command` → `CommandBus` → feature handler → host.
- Explicit routes require updates in `src/commands/model.rs::{Command, Command::domain, Command::kind_name}`, `src/commands/parser.rs::parse_action`, `src/commands/bus.rs`, `src/commands/host.rs::{CommandHost, blanket implementation}`, `src/commands/mod.rs`, `handlers/mod.rs`, and `headless.rs`.
- `ScreenDrawCommandHost` demonstrates a narrow feature host. `handle_screen_draw` returns `VisibilityPolicy::Keep`, retaining lifecycle ownership in the feature.
- `src/gui/command_host.rs::dispatch_command_invocation_with_history` may apply query overrides before dispatch. `command_accepts_query_override` already excludes several UI-owned families.
- `src/gui/render.rs` polls features before normal ROOT visibility reconciliation.
- ROOT currently suppresses generic offscreen placement specifically when `screen_draw_launcher_parking` owns geometry. Adding a color transaction without updating that ownership check is insufficient.
- ROOT uses `VisibilityRevision`, `RootWindowBridge`, `VisiblePlacementPolicy::PreserveCurrentGeometry`, and ordered native activation. Simply setting `restore_flag` can apply configured position/size and race newer visibility requests.
- Screen Draw restoration lives in `restore_screen_draw_launcher_exact` / `publish_screen_draw_launcher_restore`; its feature-specific resume/recovery behavior must not be imported into color wholesale.
- `LauncherApp::report_error_message` is the established error surface.

**Scope:** Typed command files above, `ColorPickerPlugin`, `src/gui/command_host.rs`, `src/gui/mod.rs`, `src/gui/render.rs`, a focused color GUI lifecycle module, relevant command fake hosts, and targeted tests.

### Required changes

1. Add typed color-pick intent and a stable internal wire action such as `color:pick`. Route explicitly through parser, bus, feature handler, and narrow host. Headless execution returns a clear UI-required error.
2. Expose the action through plugin `commands()` and exact `color pick` search, preserving M2-A conversion and settings.
3. Return a neutral immediate outcome for query/results/visibility; session initiation controls capture parking. Record activation once using existing conventions. Exclude this family from premature query-override reclassification.
4. Add one launcher-owned color session/controller and one exact parking owner. Snapshot query/selection before initiation; leave them unchanged during startup.
5. Make repeated picker activation a deterministic no-op while active. Reject color picking while Screen Draw capture/native state owns launcher parking. Prevent Screen Draw start/new-capture/resume from obtaining parking while color owns it. Do not automatically close existing Screen Draw sessions.
6. Apply runtime parking requests through the shared transaction. Set logical hidden state and `last_visible` together, clear restoration requests, then verify on a later turn.
7. Extend ROOT's existing hidden-geometry ownership/reconciliation boundary to recognize the color transaction. Generic visibility processing must not overwrite its capture-safe position.
8. Poll runtime events through the focused lifecycle module. ROOT wake/repaint delivery remains functional while logically hidden.
9. After native teardown, explicitly restore the exact snapshot. Use existing visibility revision/focus intent/ordered activation infrastructure. Preserve restored geometry instead of applying follow-mouse/static placement.
10. Respect newer external ROOT visibility requests during restoration. Delayed feature completion must not overwrite a newer hide/show request. If launcher-show interaction arrives while picker ownership is active, cancel the picker and complete cleanup before handing ROOT presentation back.
11. On `Picked`, normalize RGB through M2-A and set equivalent `color #rrggbb` query, invalidate/recompute ordinary results, move cursor to the end, and focus appropriately. Do not activate any resulting clipboard action.
12. On `Cancelled`, preserve saved query/selection. On failure, preserve query/clipboard, restore launcher ownership, and report an actionable diagnostic.
13. Retain the restoration point if native restoration fails so later cleanup/recovery can retry. Do not drop ownership merely because the picker completed.
14. Application exit requests picker shutdown and releases resources without a blocking UI join.

**Invariants:** One parking owner; typed routing; no automatic clipboard write; ordinary color conversion remains the sole output owner; exact geometry restoration; newer visibility requests retain authority; startup/cancel/failure allow reopening.

**Non-goals:** A second color result UI, persisted picked-color settings, broad ROOT/Screen Draw recovery redesign, new capture hooks or arbitrary waits.

**Dependencies:** Completed M2-A and M2-B. Implement sequentially after M2-B is reviewed and committed.

### Tests

- Parser/discovery/handler/bus route and UI-only headless error.
- Query override cannot bypass/reclassify color picking.
- Duplicate activation creates no additional capture/native owner.
- Screen Draw/color ownership conflicts are rejected without state mutation.
- Picked results enter ordinary search and produce HEX/RGB/HSL actions.
- Selection alone performs no clipboard write.
- Cancel/failure restore exact geometry and preserve query; failure permits successful reopening.
- ROOT reconciliation preserves active parking and respects newer hide/show requests.
- Native outcome is not applied before teardown.
- Existing architecture tests remain meaningful: `typed_bus_has_no_legacy_or_wildcard_fallback` and `launcher_activation_has_one_parser_and_no_raw_protocol_router` in `tests/domain_cases/command_bus_architecture.rs`.
- Directly affected `src/gui/render.rs` Screen Draw regressions: `screen_draw_capture_failure_restores_launcher_and_reports_diagnostic`, `screen_draw_exact_restore_ignores_configured_position_and_size`, and `screen_draw_restore_error_does_not_publish_logical_visibility`.

**Verification:** Targeted Nextest filters for new color command/lifecycle tests and listed affected architecture/Screen Draw tests. Use the planned final native sanity pass for real focus, exact pixel selection, Escape, repeated reopening, and available monitors. No broad historical acceptance harness.

### Done criteria

- `color pick` discovers and dispatches through the typed bus.
- Picker and Screen Draw cannot compete over restoration.
- ROOT does not overwrite active capture parking.
- All terminal outcomes restore/release ownership safely.
- Selection reaches ordinary color results without clipboard mutation.
- Targeted tests pass.
- Commit boundary: `feat(color): [M2-C] integrate screen picking with the color command`.

**Genuine uncertainties:** None requiring a product decision. Reconcile concrete M2-A/M2-B type names before implementation; naming does not alter these ownership requirements.
