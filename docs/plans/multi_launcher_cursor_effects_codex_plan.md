# Multi Launcher — Cursor Effects & Crosshair Enhancement
## Approved Codex Implementation Plan

**Status:** Approved for implementation; code changes have **not** been made by this document.  
**Source-of-truth snapshot:** current head of the current branch 
**Git:** Implement on the **existing currently checked-out branch**. Do not create or switch branches.  
**Related subsystem:** Existing `mouse` plugin, coordinate HUD, crosshair, mouse settings, and passive Windows overlays.  
**Primary objective:** Add a symmetric crosshair center gap, a genuine screen-color-inverting cursor halo, and a real circular screen magnifier while retaining the original native Windows cursor and existing mouse functionality.  
**Scope limit:** This is *not* a YoloMouse clone or cursor replacement project.

> **Required execution instruction:** **Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

---

# 1. Executive contract

Complete all of the following, subject to the explicit native feasibility gate in §6:

1. **Crosshair clearance:** A persistent `center_gap` preference, default **16 physical pixels**, accepted range **0–128**, measuring the distance from the actual cursor hotspot to the nearest rendered portion of each arm (including its optional contrasting outline). Four arms stay symmetric; thickness and arm length remain independently adjustable. Full-desktop guide lines retain their existing behavior and do **not** inherit the gap.
2. **Genuine inverted halo:** A filled circular area centered on the *actual cursor hotspot* showing current desktop pixels mixed with their color-inverted equivalents. Default radius **60 physical pixels** and inversion strength **0.40**; separately optional thin configurable outline. An ordinary translucent black/white/colored circle is **not** an acceptable success substitute. The original mouse cursor stays native and visible.
3. **Screen magnifier:** A circular lens displaying **real screen content beneath the cursor**, not an upscaled cursor image; adjustable zoom **1.25×–4.0×**, initial **2.0×**, initial diameter **160 physical pixels**, optional border, configurable X/Y offset, centered *or* offset positioning (offset default). Source pixels stay centered on the true cursor hotspot even when the lens itself is offset.
4. **Independent effects:** Existing crosshair, halo, magnifier, and coordinate HUD can be toggled independently; supported combinations must have predictable layering, no recursive views, and no input/focus interception.
5. **Integrated controls:** Extend the **existing** `Mouse Settings` dialog and the **existing** `mouse` command family, preserving current use, transactional settings persistence, session-only enablement, and existing `mouse coords ...` behaviors.
6. **Safe lifecycle:** Lazy native resource acquisition; no added desktop capture if only the original crosshair/HUD is active; no capture/worker cost while all modes are off; reliable disable/shutdown and graceful errors on unsupported desktops.
7. **Platform constraints:** Prioritize normal Windows desktop applications, windowed programs/games, and borderless-fullscreen apps where supported. No game injection, exclusive-fullscreen guarantee, protected-content bypass, system-cursor replacement, or novel global hooks.

**Approved fallback:** If a particular application or desktop cannot be inverted, display a **clearly identified non-inverting contrasting outline** *for the halo* and expose an informative non-spammy status. If capture/magnification is unavailable, disable or pause the lens without disturbing the other modes. Never silently present ordinary tinting as genuine inversion. If the core technologies cannot provide real inversion or real zoom on a normal composited Windows desktop, **do not declare this plan complete**: provide findings, evaluate one bounded alternative, and surface the blocker to the user.

---

# 2. Existing code and ownership map (from October 8 snapshot)

These file paths are implementation *navigation cues*, not a requirement to place every new helper in an existing large file. Inspect the current working tree, its tests, and `AGENTS.md` before editing.

| Area | Current owner / entry points | Required preservation |
| --- | --- | --- |
| Pure geometry & coordinates | `src/coordinate_tool/model.rs`: `PhysicalPoint`, `PhysicalRect`, `MonitorGeometry`, `CoordinateSample`, `CoordinateToolRuntimeState`; signed virtual-desktop physical coordinates | Negative coordinates, synchronized samples, freeze/copy semantics |
| Persisted preferences | `src/coordinate_tool/settings.rs`: `CoordinateToolPreferences`, `CrosshairPreferences`, `normalized()`, serde-defaulting | Old JSON loads; only preferences persist, not enabled states |
| Crosshair renderer | `src/coordinate_tool/render.rs`: `crosshair_geometry()`, `crosshair_bitmap()`, guide rendering | Existing line thickness, arm length, color/opacity, high-contrast outline, unchanged full-screen guides |
| Worker controller | `src/coordinate_tool/controller.rs`: `CoordinateToolController`, `CoordinateRenderFrame`, `CoordinateSurfaceBackend`, `reconcile_worker()`, `SAMPLE_INTERVAL` (16 ms) | One lazily active worker; synchronous stop/join; no polling when disabled |
| Native surfaces | `src/coordinate_tool/native.rs`: Windows DPI-aware sampler, `LayeredSurface`, `WindowsSurfaceBackend`, click-through topmost HWNDs, DIB/`UpdateLayeredWindow` | Never focus or intercept clicks; release HWND/GDI; no unnecessary bitmap uploads |
| GUI adapter | `src/gui/coordinate_tool.rs`: typed command execution, preferences merge/persist, capture-pick admission, settings updates and shutdown | Save transaction before publishing; merge only edited draft fields; preserve clipboard/capture coordination |
| Mouse dialog | `src/gui/mouse_settings_dialog.rs`: `MouseSettingsDialog`, baseline/draft, `egui::Window` + vertical `ScrollArea` | Resizable/scrollable settings, dirty drafts and Apply error handling |
| Launcher command model/parser | `src/commands/model.rs`, `src/commands/parser.rs`, `src/commands/handlers/coordinate_tool.rs` | Validated exact case-insensitive `mouse:...` wiring, deliberate errors, help and typed dispatch |
| Search/discovery plugin | `src/plugins/mouse.rs` | Single `mouse` prefix, compact action inventory, side-effect-free discovery |
| Native smoke coverage | `src/bin/coordinate_tool_smoke.rs`, `src/bin/passive_overlay_smoke.rs`, `tests/follow_mouse.rs` | Existing smoke programs and targeted tests remain usable |
| Wider screen tooling | `src/coordinate_tool/capture.rs`; existing screenshot, OCR, Screen Draw and MkMacro overlays | Respect active capture and existing user input/focus ownership; avoid disturbing other overlays |

**Current critical detail:** `crosshair_geometry()` uses `gap = thickness.max(3)` as part of an internally calculated image extent. This is *not* a true independent distance from cursor center to the nearest arm. Changing just that constant to 16 does not implement the approved semantics; account for stroke thickness and optional contrast-outline expansion.

**Current native architecture:** The coordinate controller already polls approximately every 16 ms and moves transparent layered surfaces. The new effects should integrate with its coherent sample/runtime lifecycle where appropriate, but **their relatively expensive magnification/capture machinery must be lazy and absent from the crosshair-only fast path**.

**Current dependency context:** `Cargo.toml` uses `windows` 0.58 (with Win32 components) and `image` 0.24; use the smallest additional Windows bindings/feature flags necessary. Inspect available Win32 bindings before introducing a dependency or hand-written FFI. Do not impose an additional runtime or framework for a localized feature.

**Preflight actions (orchestrator-owned):**

1. Read root `AGENTS.md`, `.codex/agents/*.toml`, the approved plan, then the relevant files above.
2. Record `git branch --show-current`, `git status --short`, and `git log -1 --oneline`; do not checkout, rebase, reset, restore, clean, or force-push.
3. Compare important interfaces to the October 8 snapshot. The archive defines intended baseline/behavior; the **current checked-out branch and its intentional user edits** are the actual files to modify. Reconcile deviations without blindly overwriting newer changes.
4. Establish whether the current branch already contains partial halo/zoom work; adapt to it instead of duplicating it.
5. Make a short architectural decision note identifying native rendering approach, event/worker ownership, and where the two new effect backends live. Keep this note proportionate; avoid an unrelated broad design document.

---

# 3. Approved user-facing product behavior

## 3.1 Crosshair center gap

- Expose **Center gap (physical pixels)** in the existing Crosshair section; editable **0–128**, default **16**.
- Semantics are distance from cursor hotspot to the **closest colored or outline pixel** of every one of the four radial arms. A 16-pixel setting is **16 pixels per direction**, not a total 16-pixel hole diameter. Use a precise, well-documented pixel-coordinate convention for odd/even thickness to avoid asymmetry or cursor overlap.
- Preserve independent arm length (existing 2–256), thickness (1–16), color, opacity, and contrasting outline. A larger gap grows total bounding extent; it does **not** shorten the arms.
- Enforce symmetry in physical desktop coordinates including monitors left/above primary (negative coordinates) and mixed DPI.
- Setting `0` brings strokes as close to hotspot as geometrically possible without accidental negative spans, overlaps, or array out-of-bounds.
- Optional virtual-desktop guides are their own full-length guide lines with their original behavior and should not acquire a gap.
- The native cursor must remain unchanged and accessible.

## 3.2 Halo effect

- Render a **filled circle** showing the *original* desktop scene with a controllable fraction of RGB inversion, rather than a solid overlay. Externally visible result must remain recognizable at 40% inversion.
- Define inversion mathematically in a consistent linearized-or-display RGB working model (document the choice) so the strength has testable semantics. If operating on normalized display RGB, each channel `c_out = (1 − s) * c_original + s * (1 − c_original)`, with `s ∈ [0,1]`. Confirm any Windows color matrix API's row/column and additive-offset convention **with native tests**, not merely a plausible-looking matrix.
- `s=0` means unchanged desktop inside circle; `s=1` means full inversion; `s=0.4` the approved default, **not 40%-opaque paint over screen**.
- Default radius 60 physical pixels. Choose a bounded practical adjustment range, document it, normalize bad settings (suggest 8–256 px). Optional thin color/thickness border; default border style unobtrusive and independently toggleable. Border is *not* allowed to masquerade as inversion.
- Follow actual `GetCursorPos` hotspot with responsive movement; avoid extra smoothing that lags the click target.
- Respect the Windows system pointer; do not draw a surrogate pointer or hide it.
- At screen boundaries, clip correctly. Do not wrap/reflect pixels or display stale frames from the other monitor.

## 3.3 Zoom lens

- Circular lens displays a scaled window of **live desktop content around the true cursor**. Initial zoom **2×**, diameter **160 physical pixels**, user range **1.25×–4×**. Choose a safe practical diameter range (suggest 64–480 px), normalize invalid/non-finite values and report limits in settings.
- Position modes: `Offset` (default) or `Centered`; offset mode uses configurable signed physical X/Y deltas (suggest default `+120, +80`, provided it places lens beyond the cursor hotspot and is clamped/flipped sensibly near screen edges). The lens may move independently, but its magnified *source* always centers on the actual cursor hotspot.
- Optional distinct outline/border. The lens should not include a magnified/duplicated copy of the native cursor unless there is a compelling documented platform limitation; it must **not replace or move the native cursor**.
- Centered mode can cover underlying click target visually but must remain **click-through**. Offset is preferred to keep normal cursor interaction visible.
- Scale content clearly. Avoid blur from repeated scaling or low-resolution intermediate surfaces; keep reasonable edge handling when source pixels fall outside monitor/virtual-desktop bounds.
- Prevent the lens from seeing itself or feedback from sibling overlays.

## 3.4 Coexistence and draw order

- `Crosshair`, `Halo`, `Zoom`, and `Coordinate HUD` enabled states remain independent. They can coexist, except for a **specific documented native incompatibility** discovered in the feasibility gate.
- Prefer the following layering semantics: underlying desktop → halo transformation of original content → optional halo border → zoom lens (using original content, not re-zoomed halo) → original crosshair and coordinate HUD (visible/readable and not copied into the lens). The native system cursor is always handled by Windows, not replaced.
- Avoid overlapping windows that oscillate in topmost order every frame. Z-order adjustments should be deterministic and minimal.
- For overlapped centered effects, the lens may occlude the halo's interior; this is an explicitly understandable composition consequence, not a failure of the halo. Ensure independent disable produces correct final appearance.
- No effect should capture keyboard/mouse input or activate a window. Escape and other existing input flows should not be intercepted.

## 3.5 Persistence versus runtime

- Persist **appearance** in the existing `CoordinateToolPreferences` inside the canonical Settings file: center gap; halo style; zoom style/position. Preserve legacy JSON compatibility and unknown content unrelated to this feature, consistent with current Settings behavior.
- **Do not persist any of the effect enable flags**. When Multi Launcher is restarted, HUD, crosshair, halo and zoom default to off for that session. Existing RuntimeState freeze/copy data is never persisted.
- In the dialog, new appearance fields use the current draft/Apply behavior; enabled checkboxes act immediately for this session. Failed Apply retains dirty user values and displays the error. External commands that change saved fields should merge properly with a dirty dialog draft using the established changed-fields-only commit pattern.
- A malformed/old saved preference object must normalize cleanly and load with safe defaults; no manual migration or destructive reset.

## 3.6 Command contract

Preserve current `mouse` plugin. Add the following **canonical queries**, backed by typed `mouse:...` actions (the implementation should follow the existing `src/commands/parser.rs` exact-token parser, not invent a parallel interpreter):

```text
mouse crosshair gap <0..128>
mouse halo toggle
mouse halo on
mouse halo off
mouse zoom toggle
mouse zoom on
mouse zoom off
mouse effects off
```

- Existing `mouse`, `mouse settings`, `mouse coords ...`, `mouse crosshair ...`, help, and the existing one-shot coordinate pick must remain valid.
- `mouse halo` and `mouse zoom` should give a discoverable short action list, as `mouse crosshair` already does; `mouse` should provide useful concise inventory. Additional advanced styles can remain GUI-only unless a very small command addition improves consistency.
- Reject invalid values, excess tokens, NaN/Infinity, and lookalike prefixes without triggering effects or settings changes.
- `mouse effects off` means **crosshair + halo + zoom off**, while the independent coordinate HUD can remain active; it must not cancel/alter an in-progress coordinate pick, copy anything, or mutate the saved appearance settings.
- Do not register default global hotkeys. Users may bind existing launcher commands/actions normally.

## 3.7 Mouse Settings dialog

Existing `Mouse Settings` remains one **resizable, scrollable egui window** with four clearly separated sections:

1. **Crosshair** — existing controls + Center Gap.
2. **Cursor Halo** — enabled-for-session; radius; true inversion strength; optional outline/color/thickness; visible availability/fallback status.
3. **Cursor Magnifier** — enabled-for-session; centered/offset selection; zoom ratio; diameter; offsets; optional border; error/availability status.
4. **Coordinate Display** — existing controls, functionally unchanged.

- Preserve the current `egui::ScrollArea` behavior in compact launchers and small viewports; settings content may scroll vertically, but it must **not expand the window or force the parent launcher to grow**.
- Add short tooltips for pixel radius, inversion strength, offset, scale, and centered mode.
- Include small **Reset to defaults** controls for the new effect sections (and for the new gap field if appropriate). They edit the draft and require Apply, consistent with existing settings.
- Show immediate feedback when toggling modes, a compact active-effects indicator, and a non-repeating local error/warning if the engine cannot honor an effect. Do not display an infinite series of toast notifications.
- Keep existing dirty-draft, reopen/close, and save-on-Apply semantics.

---

# 4. Non-goals / hard safety limits

- **No** replacement of system cursor image, Windows cursor APIs, or hotspot.
- **No** cursor asset/theme manager; no RuneScape cursor package import; no xBRZ upscaler; no flames, plasma, trails, scripting, pulsing, or general animation framework.
- **No** global input hooks, graphics DLL injection, anti-cheat interaction, privileged APIs, protected-content circumvention, or exclusive-fullscreen guarantees.
- **No** changes to radial menu, MkMacro action authoring, clipboard snippets, workspaces, application launch/search ranking, or other features except small necessary integrations.
- **No** second independent high-frequency cursor sampler when the existing controller can own the lifecycle cleanly.
- **No** release build/performance campaign after each checkpoint.
- **No** heuristic approximate colored overlay passed off as inversion; the user explicitly requested a real color flip.
- **No** silent error swallowing that leaves apparently enabled but nonfunctional effects.
- **No** source control branch creation, automatic branch switching, push, history rewrite or squash.

---

# 5. Native architecture guidance (directional, not mandated implementation)

## 5.1 Preferred first candidate: Windows Magnification API

Investigate a **windowed magnifier control** (not a system-wide fullscreen magnifier) for both the `halo` and `zoom` surfaces:

- `MagInitialize` / `MagUninitialize`, `WC_MAGNIFIER`, `MagSetWindowSource`, `MagSetWindowTransform`, `MagSetColorEffect`, and `MagSetWindowFilterList`.
- At **1.0× scale**, a circularly clipped, correctly positioned magnifier with a configurable color matrix may provide *genuine blended pixel inversion* for the halo without a bespoke continuous screen-capture loop.
- At **2.0× scale**, a second circular magnifier using normal colors may provide the zoom lens with a cursor-centered source rectangle.
- A topmost, no-activate, click-through layered host with a circular region (if viable) may yield the requested shape. Validate the interaction among `WS_EX_LAYERED`, `WS_EX_TRANSPARENT`, circular clipping/regions, display affinity, DWM, and the embedded magnifier **on the real target desktop**. Do not claim this combination works merely because each API exists independently.
- `MagSetWindowFilterList(MW_FILTERMODE_EXCLUDE)` can exclude *sibling* overlay HWNDs from magnified content; the magnifier's own window is automatically excluded according to Microsoft documentation. Confirm the filter list functions with the actual crosshair, guides, HUD, and both magnifier hosts. Avoid excluding arbitrary third-party windows or the entire application without a reason.
- Verify proper startup/shutdown thread affinity, HWND ownership, safe native handle cleanup and all Win32 return values.
- Check the available Windows crate bindings/features first; add only necessary bindings or a **narrow, safe wrapper** if official APIs are absent from currently generated bindings. Keep `unsafe` localized and documented.

## 5.2 If that approach fails the feasibility gate

Evaluate **one bounded alternative** rather than implementing many competing capture engines:

- Use a local rectangle around the cursor obtained from a suitable Windows desktop capture mechanism (e.g., Windows Graphics Capture, Desktop Duplication, or justified GDI capture), then apply per-pixel inversion/resampling and present in clipped, click-through window(s).
- Prove crop correctness, frame latency, performance, overlay exclusions, protected/black frames, and multi-monitor behavior before integrating; avoid broad library adoption merely to get a circular tinted window.
- If considering `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, understand that it can also hide your overlays from screenshots/streaming/recording, and works only under documented OS/DWM conditions. Prefer **local magnifier exclusion** where possible; do not globally exclude overlays without considering the user's screen-sharing needs.
- When an exact effect cannot be supported on a desktop or device, **report that precisely** and use the approved contrasting-outline fallback for halo. Do not change settings to claim genuine inversion still works.

## 5.3 Compositor and performance requirements

- Build only the native surfaces needed for active modes. Current cheap crosshair + HUD remain on their original bitmap/position-only path, with **no desktop readback/magnifier initialization** unless halo/zoom require it.
- The existing `CoordinateToolController` worker already samples the cursor every 16 ms. Share its `CoordinateRenderFrame`/consistent sample where reasonable, but avoid a giant always-present heavy renderer. Consider a feature-owned, optional native effect-surface helper *owned by* the existing worker; keep controller, pure rendering calculations and Win32 resources separate.
- Update position/source when cursor moves; update transformed content as required when the desktop changes under a stationary cursor. If a backend provides a live magnifier control, confirm it actually updates without moving the mouse. Avoid excessive busy-spinning.
- Keep approximately **60 FPS as a target, not a promised guarantee**. Use measurements on the target PC before making performance claims. Idle work when all modes off must be zero; when only crosshair/HUD on, there must be **no new screen capture**.
- Handle negative virtual-desktop origins and different monitor scales. Source rectangles use **physical desktop coordinates**; clip/clamp against monitor/virtual-desktop bounds without integer overflow. When lens follows between monitors, recreate/reconfigure native resources if necessary but never leak old surfaces.
- Keep transparency/click-through guarantees even in failure/recovery states. Avoid per-frame topmost flag churning, unnecessary bitmap reallocation and a second perpetual polling thread.

## 5.4 References for feasibility research

These are **API documentation**, not proof the desired circular desktop composition works in this codebase:

- [Windows Magnification API overview](https://learn.microsoft.com/en-us/windows/win32/winauto/magapi/magapi-intro)
- [MagSetColorEffect](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/api/magnification/nf-magnification-magsetcoloreffect)
- [MagSetWindowSource](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowsource)
- [MagSetWindowTransform](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowtransform)
- [MagSetWindowFilterList](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowfilterlist)
- [SetWindowRgn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowrgn)
- [SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)

---

# 6. Five implementation milestones with active checkpoint commits

**Milestone order:** Crosshair gap + early native feasibility → typed effect state and common native surface → inverted halo → circular screen magnifier → integrated UI/commands and final targeted verification.

**Recommended checkpoint count:** 12. The identifiers are deliberate logical checkpoints, not a requirement to commit before code is coherent. See §7 for precise commit/verification rules.

## M1 — Crosshair clearance and early native proof

### M1-A — Persist true center gap and correct crosshair geometry

**Objective:** Implement a real per-arm center gap and verify mathematical layout independent of Win32.

**Owner:** `src/coordinate_tool/settings.rs`, `src/coordinate_tool/render.rs`; update only the nearest existing tests.

**Tasks:**

1. Introduce `CrosshairPreferences.center_gap` with serde/default handling, default 16 physical pixels and clamp 0–128. Ensure pre-feature JSON continues to deserialize.
2. Refactor `crosshair_geometry()` into a single unambiguous distance convention measured from the *cursor hotspot* to the closest visible colored **or contrasting-outline** pixel. Do not reuse the existing `gap = thickness.max(3)` definition as if it were per-arm clearance.
3. Compute bounding bitmap extents and center placement with checked arithmetic. Avoid inconsistent center shifts caused by odd/even thickness or the optional 1-pixel outline.
4. Ensure both positive- and negative-axis arms preserve the configured independent arm length. Keep `guide_geometry()` unchanged except if a shared helper must be adjusted for compilation.
5. Test gap=0, 1, 16, 128; odd/even thickness; outline on/off; all four directions; image sizes and transparency; normalization from out-of-range and legacy JSON.
6. Preserve existing appearance fields in the round-trip tests and ensure runtime flags do not enter serialized preferences.

**Acceptance:** The four inner visible stroke boundaries are equally far from the hotspot, and no optional outline encroaches into the specified clearance; lengths/thickness retain current semantics; guides unaffected.

**Suggested commit:** `feat(mouse): [M1-A] add symmetric crosshair center clearance`.

### M1-B — Expose crosshair gap in existing settings/command flow

**Objective:** Make the new crosshair gap user-configurable without creating a second settings surface.

**Owner:** `src/gui/mouse_settings_dialog.rs`, `src/gui/coordinate_tool.rs`, `src/commands/{model,parser,handlers/coordinate_tool}.rs`, `src/plugins/mouse.rs`, relevant tests.

**Tasks:**

1. Add labeled Center Gap control with 0–128 bounds next to existing Thickness/Arm Length; tooltip states **per-arm distance in physical pixels**.
2. Extend `apply_draft_preferences()`'s changed-fields-only merging logic; modified gap persists transactionally with current settings, while failed writes retain the draft and previous live preference.
3. Implement validated canonical `mouse crosshair gap <pixels>` typed command end-to-end; retain all existing commands, help and discovery conventions.
4. Verify enabled live crosshair updates immediately after a successful Apply or exact command; the worker should not be restarted unnecessarily.
5. Test invalid syntax, case insensitivity, out-of-range values, changed field merge, settings dialog with small viewport, and existing controls.

**Acceptance:** Existing HUD/crosshair functionality is intact; cursor gap can be changed from GUI and launcher; settings remain scrollable.

**Suggested commit:** `feat(mouse): [M1-B] expose crosshair gap in settings and commands`.

### M1-C — Native feasibility gate for real circular inversion and magnification

**Objective:** Validate **on Windows** whether the preferred native composition can deliver the exact two user-visible effects *before* building full configuration and permanent integration.

**Owner:** Short-lived feature-isolated proof harness (extend an existing native smoke binary only if sensible); a small text record of findings, e.g. `docs/plans/cursor_effects_feasibility.md`. Do not reshape production controllers for a speculative backend.

**Tasks:**

1. Create a bounded manual/debug-only proof that shows current desktop content under a **circular, click-through** magnifier host at the physical pointer with **1× transform and true inversion**. Verify both black and white test regions and readable text, not merely changing a hardcoded UI circle color.
2. Demonstrate **adjustable partial inversion strength** (0%, 40%, 100% test values). Confirm math and API matrix orientation through pixel evidence. Show original native cursor on top/unmodified.
3. Demonstrate a second circular lens at 2× whose source follows the cursor even when the destination lens is offset.
4. Verify exclusion of all feature-owned overlay surfaces (including crosshair/HUD/other lens) from each magnifier. Test actual scene movement and stationary cursor over moving content to detect stale pixels.
5. Confirm click-through, no keyboard focus, negative-coordinate monitor support or recorded current limitation, shape correctness and resource teardown (repeat on/off), and whether layered/circular clipping is genuinely supported.
6. Record approach decision, observed shortcomings, tested Windows/display environment and exact evidence; do not speculate about exclusive-fullscreen support.
7. If the preferred approach fails a key point, evaluate **one** bounded alternative from §5.2 and test it. If neither can deliver *genuine* effects on a normal desktop, stop the halo/zoom milestones, give the orchestrator a concrete **blocker report**, and ask the user for scope/technology direction instead of faking success.

**Acceptance gate:** A native Windows proof visually demonstrates an actual partly inverted *filled circle* and actual magnified *desktop* pixels, a working circle mask and at least one strategy for preventing recursive feedback; dependencies/limitations are documented. A mere colored alpha surface does **not** pass.

**Checkpoint note:** Commit a useful, bounded proof harness and findings when they establish a reusable decision. Do **not** commit disposable experimental binaries/large dumps or a meaningless checkpoint if no coherent work is retained.

**Suggested commit:** `test(mouse): [M1-C] validate native cursor inversion and zoom approach`.

---

## M2 — Shared effect model, runtime and native lifecycle

**Prerequisite:** M1-C's core feasibility gate passes. If a different proven backend was selected, tailor implementation details accordingly; preserve the user contract.

### M2-A — Define preferences, runtime toggles and independent effect state

**Objective:** Have **one typed canonical preference model** and **one runtime state model** for HUD, crosshair, halo and zoom, with no UI/native implementation dependency.

**Owner:** `src/coordinate_tool/settings.rs`, `src/coordinate_tool/model.rs`, controller interfaces; appropriate module export in `src/coordinate_tool/mod.rs`.

**Tasks:**

1. Introduce `HaloPreferences` and `ZoomPreferences` with safe serde defaults and explicit normalization. Suggested fields:
   - Halo: radius=60, inversion_strength=.40, outline_enabled, outline_color, outline_thickness.
   - Zoom: mode=`Offset`, zoom_factor=2.0, diameter=160, signed X/Y offset (suggest +120,+80), outline_enabled/color/thickness.
2. Use existing color/offset structs where they fit semantics, rather than inventing ambiguous duplicate data types. Limit large offsets/radii and non-finite scale/strength.
3. Extend `CoordinateToolPreferences` and `RawCoordinateToolPreferences` without breaking legacy/partial JSON or unrelated preferences. Serialization must contain **appearance only**.
4. Extend `CoordinateToolRuntimeState` with independent session-only `halo_enabled` and `zoom_enabled` values, plus a single way to disable crosshair/halo/zoom while preserving coordinate HUD/capture.
5. Preserve existing freeze, last copied value, current vs placement sample behavior. Effects always track **live current sample**, never the frozen HUD display.
6. Extend controller tests for mode combinations and idle/active transitions with a fake backend. Do not create a second sampler thread or force a capture engine into pure unit tests.

**Acceptance:** Legacy settings load; runtime and persisted state stay separate; invalid values normalize; modes toggle independently and the all-effects-off action retains HUD.

**Suggested commit:** `feat(mouse): [M2-A] model independent halo and magnifier preferences`.

### M2-B — Implement lazy native effect surface/resource boundary

**Objective:** Add a reusable effect renderer capable of owning the Windows magnifier/capture resources when and only when needed.

**Owner:** `src/coordinate_tool/native.rs` or a small, focused `src/coordinate_tool/native_effects.rs` module; `src/coordinate_tool/controller.rs` for clean activation/shutdown. Keep Win32 handle lifetimes RAII where practical.

**Tasks:**

1. Isolate native effect resources behind one internal interface. Avoid making `CoordinateSurfaceBackend` do desktop readback for each crosshair render. A reasonable interface takes a `CoordinateRenderFrame` plus active preferences and returns a concise status; use ownership patterns already established in the controller.
2. Create/destroy magnifier runtime and relevant HWNDs lazily, based on session toggle(s). Preserve same-thread native init/use/uninit requirements and worker stop/join behavior. Ensure partial initialization failure cleans up handles.
3. Maintain passive topmost/no-activate/transparent hit-testing; ensure no global hotkeys or native input hooks are added. Avoid blocking the main egui thread on expensive capture operations.
4. Establish stable source-window exclusion and layering support. Include crosshair, full guides, HUD and both effect hosts as needed; avoid recursive frames and readback of another effect's pixels.
5. Integrate display-change, DPI-change, monitor switch and `WM_*` invalidation with existing polling/event pump. Preserve negative desktop coordinate math and sensible edge handling.
6. Ensure effect initialization errors do not tear down a **working** crosshair/HUD. Surface one stable per-effect status instead of error-toast storms.
7. Add tests for resource states with injected fake backend/factory: 0 enabled→0 native effect surfaces; crosshair-only→no capture; halo+zoom→shared ownership; disable one→other remains; disable last→dispose; shutdown and repeat toggles→no leaks/worker remnants.

**Acceptance:** Native overhead is zero with only cheap modes, renderer lifecycle is bounded, independent modes survive errors, and overlay surfaces never become interactive.

**Suggested commit:** `feat(mouse): [M2-B] add lazy native cursor-effects lifecycle`.

---

## M3 — True blended inversion halo

### M3-A — Halo computation, shape, and genuine color transformation

**Objective:** Produce an actual blended inversion result from live desktop pixels, with a precise circular clip and optional outline.

**Owner:** New pure helpers near `src/coordinate_tool/render.rs` and native effect renderer; tests isolate transformation math.

**Tasks:**

1. Define and unit-test the blending operation independently of Windows. For `s=0.0`, output=input; `s=1.0`, output=invert(input); `s=0.4`, output is the specified mix (test black/white/gray/colored channels, avoiding overflow/clamping errors).
2. Express the transformation in the selected native renderer (color matrix or pixel pipeline) and **compare to controlled visual evidence**. Watch for channel permutation, alpha premultiplication and color-space differences.
3. Implement filled circular silhouette; outside radius must be fully unaffected. Halo should remain centered on **hotspot** despite DPI and thickness. If an overlay region's physical outline has jagged edges, accept a simple crisp edge first; edge blur/soft animation are out of scope.
4. Draw optional user-selectable outline separately from the actual inverted interior. If outline isn't enabled, there must be no stray border.
5. Retain the native pointer and avoid shift to hardware cursor hotspot or duplicated cursor image.
6. Check movement over light/dark backgrounds, white/black and saturated test swatches, moving windows, multi-monitor borders and zero/100 percent strength.

**Acceptance:** User visibly sees a **partially inverted recognizable live scene** inside the halo and unchanged content outside, not a tint; shape/radius/strength controls behave predictably.

**Suggested commit:** `feat(mouse): [M3-A] render genuine blended-inversion cursor halo`.

### M3-B — Availability, fallback and halo error recovery

**Objective:** Make unsupported capture/composition a contained state instead of corrupting the desktop overlay lifecycle.

**Owner:** Native surface status, controller error reporting, focused availability tests; GUI display hook may be minimal here, finished in M5.

**Tasks:**

1. Distinguish available full inversion, degraded contrasting-outline fallback, paused/unavailable, and off states. The fallback has **no inversion**, must say so, and must retain radius/optional border settings as practical.
2. Do not continually retry broken capture in a tight loop; use bounded recovery on meaningful environment changes or explicit reenable. Keep errors non-spammy.
3. Verify stop/cleanup/restart after display mode/DPI change, monitor detach, RDP/lock/secure desktop transition where testable, backend init failure, and extreme but normalized preferences.
4. Confirm halo failure leaves crosshair, HUD and coordinate picker unaffected.
5. Ensure passive overlay remains non-activating and not a blocker for screenshots, OCR, Screen Draw or MkMacro when those systems acquire their own overlays. Record any unavoidable interaction with screenshots/recording.

**Acceptance:** Failures are honest, localized, and recoverable; no frozen large tinted window or zombie active-loop remains.

**Suggested commit:** `fix(mouse): [M3-B] isolate halo failure and fallback states`.

---

## M4 — Real cursor-centered screen magnifier

### M4-A — Correct magnification source/destination geometry

**Objective:** Calculate source and destination rectangles for all supported lens settings and monitor placements without requiring a native window.

**Owner:** Pure coordinate/placement helpers in `src/coordinate_tool/{model,render}.rs` or narrowly named helper, settings normalization/tests.

**Tasks:**

1. Implement centered or offset lens placement from signed physical cursor coordinates; respect user X/Y offsets in Offset mode, with a monitor-aware clamping/flip strategy to keep the lens onscreen near the edges. Document precedence if the lens cannot remain fully visible.
2. Calculate the native desktop source rectangle from the **cursor hotspot**, magnification ratio and diameter. The source remains cursor-centered even when the drawn lens is at a different X/Y. Test this explicitly.
3. Keep correct scale and aspect ratio when source rect size cannot be fractional: choose consistent pixel rounding and avoid zoom jitter near boundaries.
4. Handle source reading at/near desktop edges: clip/sample safely without unsigned conversion overflow or cross-monitor teleportation. Prefer a clear policy (clamp source to available desktop; if necessary expose limited edge region) rather than invalid Win32 rectangles.
5. Cover negative-coordinate secondary monitors, different monitor work areas, lens larger than target monitor, odd/even lens sizes, 1.25/2/4 magnification, offset directions, centered mode and extreme but normalized preferences.

**Acceptance:** Math is deterministic and accurate; a lens drawn at (+offset) still magnifies the pointer's location, not pixels beneath the destination lens.

**Suggested commit:** `feat(mouse): [M4-A] calculate monitor-safe circular lens geometry`.

### M4-B — Native lens rendering and simultaneous-effect behavior

**Objective:** Implement circular, readable live magnification with independent enablement and predictable interaction with halo/crosshair/HUD.

**Owner:** Feature-owned Windows effect renderer + `CoordinateToolController` integration; minimal settings adapter glue.

**Tasks:**

1. Instantiate (or reuse the M2 shared infrastructure for) a native magnifier at configured zoom, circle diameter and position, with a normal-color source from the actual cursor hotspot.
2. Keep native pointer unchanged; confirm magnified view excludes overlay windows as specified. Do not accidentally show the halo as a second expanding halo inside zoom or copy the lens into itself.
3. Support live source refresh both while cursor moves and (where supported) while the underlying desktop animates without cursor movement; test for stale frame and excessive CPU use.
4. Add optional lens outline and independent centered/offset transitions. Confirm border doesn't intercept mouse clicks or cover HUD text unexpectedly.
5. Test all combinations (halo+zoom, crosshair+zoom, all three, all four including HUD). Establish/validate stable draw order; no frames flickering with z-order reordering.
6. Verify lens-only mode correctly activates existing worker and lens off while halo/HUD still on does not stop the shared worker prematurely. Stopping the last mode joins and releases native resources.
7. On native magnifier failure, stop/disable/park lens gracefully and show concise status while other effects continue. Do not silently substitute cursor-image scaling.

**Acceptance:** True underlying screen content is magnified in a circular lens, independently selectable and not recursive; existing modes and foreground input work normally.

**Suggested commit:** `feat(mouse): [M4-B] render click-through cursor magnifier`.

---

## M5 — Integration, polish and focused verification

### M5-A — Expand the Mouse Settings dialog without layout regressions

**Objective:** Expose controls and statuses for both new effects in the current UI, retaining current editing conventions.

**Owner:** `src/gui/mouse_settings_dialog.rs`, `src/gui/coordinate_tool.rs`, directly associated tests.

**Tasks:**

1. Add Crosshair / Cursor Halo / Cursor Magnifier / Coordinate Display sections in that order to the existing resizable vertical-scroll window.
2. Bind enable-for-session checkboxes to typed adapter/controller operations for immediate action; bind appearance controls to the **draft**, not directly to disk.
3. Add radius, strength, optional outline + color/thickness, zoom factor, diameter, offset mode, signed offsets, optional outline + color/thickness, per-section Reset to defaults, help tooltips and concise effect-active labels.
4. Extend `apply_draft_preferences()` changed-fields-only merge for every new preference, retaining unrelated concurrent edits and dirty drafts when write fails. Do not retroactively persist runtime enabled flags.
5. Explicitly reflect native availability/fallback status; never use a generic `Enabled` checkbox to imply real inversion is functioning when only contrast-outline fallback is running.
6. Test 400×220 or similarly compact egui viewport scrolling, no auto-growing parent window, no forced horizontal overflow, dialog close/reopen, reset/apply, and independent session controls.

**Acceptance:** No settings need a new dialog; all fields can be reached at compact sizes, and changes follow established persistence semantics.

**Suggested commit:** `feat(mouse): [M5-A] expose cursor-effects controls in Mouse Settings`.

### M5-B — Typed mouse commands, help, discovery and effect-off control

**Objective:** Make the new effects usable instantly from launcher search, aliases and existing command actions.

**Owner:** `src/commands/{model,parser,handlers/coordinate_tool}.rs`, `src/gui/coordinate_tool.rs`, `src/plugins/mouse.rs`, relevant command bus/host tests; update README command description only as needed.

**Tasks:**

1. Extend `CoordinateToolCommand` with typed operations for halo toggle/on/off, zoom toggle/on/off, all-effects-off; avoid string-only handler special cases.
2. Extend exact `parse_mouse_wire` and `mouse` search discovery. Preserve current case insensitivity, bounded argument grammar and `Invalid` handling. Add `mouse halo` and `mouse zoom` discovery queries.
3. Ensure all operations pass through existing GUI/headless contract and single settings/runtime owner. Never make plugin `search()` toggle effects.
4. Add or update `mouse help`, `mouse crosshair help`, and new effect-specific guidance; keep action inventory useful rather than flooding search results with every style field.
5. Implement `mouse effects off` as independent crosshair/halo/zoom disable with coordinate HUD and coordinate pick untouched. Confirm no new default global hotkeys.
6. Cover command parsing, dispatch, typed host mocks, plugin discovery, malformed tokens and end-to-end smoke where reasonable.

**Acceptance:** All approved commands work as actual launcher actions and do not regress old commands. `mouse effects off` is an immediate, safe escape hatch.

**Suggested commit:** `feat(mouse): [M5-B] wire halo zoom and all-effects-off commands`.

### M5-C — Final targeted verification, regression cleanup and documentation

**Objective:** Close the feature with evidence for core native capabilities and preserved behavior, without an unnecessary repository-wide qualification campaign.

**Owner:** Focused tests, native smoke harness, concise help/README and plan completion notes, with orchestrator/reviewer coordination.

**Tasks:**

1. Run focused unit/integration tests for `coordinate_tool` settings/model/render/controller/native adapters, mouse plugin, typed parser/handler, Mouse Settings dialog and relevant passive overlay smoke. Fix **new** warnings/test failures where attributable to this feature.
2. Run one **appropriately scoped** compile/check/Nextest verification pass at this milestone; do not run repetitive full test suites at each preceding checkpoint. See §9.
3. Perform native Windows acceptance using the checklist in §8. Distinguish actual results from unperformed manual tests. If environment cannot display native windows, do **not** claim native acceptance passed.
4. Examine interactions with screenshot, OCR, Screen Draw, MkMacro coordinate picker, window focus, launcher show/hide, multi-monitor and video capture. Validate only impacted interfaces; do not resurrect historical unrelated broad acceptance suites.
5. Record approximate FPS/latency/resource behavior under halo-only, zoom-only, both, and crosshair-only using whatever existing profiling approach is available. If 60 FPS cannot be attained, report conditions/measurements rather than inventing a benchmark.
6. Verify all enabled effects are off at new app session, persisted appearance survives relaunch, active masks are genuinely click-through, and last-disable is resource-clean.
7. Request independent reviewer attention for native handle lifetime, recursion exclusion, native cursor preservation, settings migration, accidental future capture when idle and command compatibility. Remediate **substantive** defects with traceable follow-up commits.
8. Write concise user-facing instructions: settings sections, `mouse ...` commands, supported Windows modes, the difference between true inversion/fallback and screenshot/streaming limitations.
9. Keep final report compact but concrete: implemented behavior; evidence/limitations; changed files; tests actually run; checkpoint hashes; known unsupported setups.

**Acceptance:** Core behavior and compatibility gates in §8 are verified or accurately reported as requiring native confirmation; unresolved nonoptional core functionality cannot be labeled delivered merely because Rust tests compile.

**Suggested commit:** `test(mouse): [M5-C] verify cursor effects and document limitations`. Follow-up defect-specific remediation commits are preferable to quiet history rewriting.

---

# 7. Active checkpoint commit cadence — binding workflow

**Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.** The list below is the concrete requested cadence. The parent orchestrator must ensure the Git history shows visible, coherent development progress.

| ID | Boundary | Suggested subject |
| --- | --- | --- |
| M1-A | Crosshair gap model + pure geometry/tests complete | `feat(mouse): [M1-A] add symmetric crosshair center clearance` |
| M1-B | Gap GUI, commands and persistence wired | `feat(mouse): [M1-B] expose crosshair gap in settings and commands` |
| M1-C | Native feasibility harness/findings retained (if meaningful) | `test(mouse): [M1-C] validate native cursor inversion and zoom approach` |
| M2-A | Persistent halo/zoom models + runtime flags | `feat(mouse): [M2-A] model independent halo and magnifier preferences` |
| M2-B | Lazy native-effect resources and worker lifecycle | `feat(mouse): [M2-B] add lazy native cursor-effects lifecycle` |
| M3-A | Genuine circle inversion and math/shape coverage | `feat(mouse): [M3-A] render genuine blended-inversion cursor halo` |
| M3-B | Non-spammy fallback, status, recovery | `fix(mouse): [M3-B] isolate halo failure and fallback states` |
| M4-A | Magnified source/target placement math | `feat(mouse): [M4-A] calculate monitor-safe circular lens geometry` |
| M4-B | Native zoom lens + combined-effect behavior | `feat(mouse): [M4-B] render click-through cursor magnifier` |
| M5-A | User settings, draft merge, availability UI | `feat(mouse): [M5-A] expose cursor-effects controls in Mouse Settings` |
| M5-B | Full typed commands + help/discovery | `feat(mouse): [M5-B] wire halo zoom and all-effects-off commands` |
| M5-C | Focused integrated verification and documentation | `test(mouse): [M5-C] verify cursor effects and document limitations` |

**Rules:**

- Commit after **each meaningful coherent subsection** rather than allowing independently understandable work to accumulate. Do not commit every tiny edit, a pure WIP, or artifacts from dead-end probes.
- Before a checkpoint commit, inspect `git status`, `git diff`, staged files, and adjacent references; avoid accidentally including pre-existing user edits. If dirty baseline includes user's modifications, distinguish them explicitly and stage only task-owned files/hunks.
- Use subjects formatted **`<type>(<scope>): [M#-X] <clear description>`**. Optional short commit body explains architecture, compatibility and deliberate limitations.
- **Do not run expensive full verification before every commit.** A type/formatter check or tiny targeted test can be enough to keep a checkpoint coherent. Run substantive targeted verification at the designated integration gate(s), especially M1-C native proof and M5-C final checks.
- If later native smoke/tests/reviewer uncover defects, use a *new*, descriptive remediation commit referencing the relevant stage, e.g. `fix(mouse): [M4-B] exclude halo surface from zoom capture`.
- Do not squash/rewrite checkpoint history unless user explicitly requests. Do not push without being asked.
- A meaningful shared architectural change may justify a slight boundary adjustment; document the reason and retain the visible checkpoint cadence.
- Existing user changes and the current branch's history must remain intact. The provided October 8 archive is a baseline reference, not permission to reset newer work.

---

# 8. Acceptance and regression matrix

Use these identifiers in unit/integration tests and final manual test notes. They are **behavioral requirements**, not an order to create 42 unrelated test files. Avoid duplicating existing tests that already prove an invariant.

| ID | Scenario and expected observation | Best evidence |
| --- | --- | --- |
| G01 | Existing/legacy settings JSON loads with gap=16 and safe new-effect defaults | Unit serde |
| G02 | Gap 0,1,16,128: equal distance to each **visible stroke including outline** | Pure geometry/bitmap tests |
| G03 | Arm lengths/thickness unchanged by changing gap | Pure geometry |
| G04 | Odd/even thickness, outline, all directions remain centered on hotspot | Pure geometry |
| G05 | Existing guides remain continuous and independent of gap | Render regression |
| G06 | Gap GUI Apply persists; exact command updates active display | Adapter/command tests + native spot |
| G07 | White, black, gray and colored source pixels invert at 0/40/100% strength | Pure transform + native swatches |
| G08 | Halo is filled circular **inverted desktop content**, not tinted paint | **Native visual must-pass** |
| G09 | Outside halo circular region, desktop remains unchanged | Native visual |
| G10 | Halo stays centered on system cursor hotspot; native cursor unchanged | Native manual |
| G11 | Halo radius, inversion strength and border follow saved preferences | Unit/native |
| G12 | Border off leaves no border; border on respects color and thickness | Render/native |
| G13 | Halo fallback clearly states **no inversion** and remains a contrasting outline | Error-state unit/native |
| G14 | Re-enabling halo after recoverable failure doesn't create extra HWNDs/loops | Fake lifecycle/native |
| Z01 | Lens magnifies real underlying screen content, not a resized pointer | **Native visual must-pass** |
| Z02 | Lens 1.25×, 2×, 4× produces consistent scaling | Pure geometry/native |
| Z03 | Offset lens samples location under true pointer rather than offset destination | Pure geometry/native |
| Z04 | Centered lens remains click-through over target controls | Native manual |
| Z05 | Lens diameter and border preferences are applied | Render/native |
| Z06 | Mixed-DPI/multiple monitor + negative desktop coordinates | Geometry + native manual |
| Z07 | Cursor at all four monitor edges keeps lens usable without corrupt readback | Pure geometry/native |
| Z08 | No recursive lens/halo/crosshair/HUD in lens content | **Native must-pass** |
| Z09 | Stationary cursor above changing text/content remains live if backend supports it | Native manual |
| C01 | HUD, crosshair, halo and zoom toggle independently; combinations work | Fake controller + native |
| C02 | All effects off disables crosshair/halo/zoom, **retains HUD** | Controller/command tests |
| C03 | Only HUD/crosshair active causes **zero new desktop capture work** | Fake backend instrumentation |
| C04 | Last active mode off joins worker and frees native resources | Controller/native tests |
| C05 | Mouse Settings enabled states reset across new app session; appearance survives | Unit/adaptor/native |
| C06 | Saved settings update merges only changed draft fields; failed writes preserve draft | Adapter/dialog tests |
| C07 | Dialog scrolls in small viewports; no parent auto-width growth | egui UI test/manual |
| C08 | Commands are case insensitive/exact; bad args have no side effects | Parser/plugin tests |
| C09 | `mouse`, `mouse halo`, `mouse zoom`, help discover valid actions | Plugin tests |
| C10 | Cursor effects do not interfere with click/drag/keyboard, window focus or Escape | Native manual |
| C11 | Coordinate pick still captures exact release and preserves clipboard on cancel | Existing capture tests + native spot |
| C12 | Effects survive resizing display/moving monitors, or fall back clearly | Native manual |
| C13 | On unsupported content capture, no repeated errors and other modes survive | Fake native error/manual |
| C14 | No unexpected changes to Screenshot/OCR/Screen Draw/MkMacro overlay actions | Direct regression spot checks |
| C15 | Run on windowed/borderless content where supported; exclusive fullscreen not promised | Native compatibility notes |
| C16 | CPU/resource use returns to baseline after all effects off; repeated toggles leave no leaks | Lifecycle tests/native observation |
| C17 | Native capture limitations and screenshot/streaming visibility disclosed | Documentation/manual |
| C18 | No click-through regression even when halo/zoom window partially initialization-fails | Native error injection |

**Must-pass native gates:** G08, Z01, Z08. The source-derived unit test suite alone cannot prove them. If native device/environment is unavailable, explicitly mark them **not executed** rather than “passed”; final completion must reflect that limitation.

---

# 9. Testing strategy tailored to slow Windows builds

The user's Windows build/test cycle can be expensive. Make the most substantial code changes before performing full targeted compilation. Use `cargo nextest` for test execution where present, but do not force repeated expensive rebuilds across closely related checkpoints.

## 9.1 Lightweight checks as work progresses

- `cargo fmt --all -- --check` when enough changes warrant it; `git diff --check` before commits.
- Pure geometry/matrix unit tests in the nearest existing test module when feasible; do not force separate test binaries if they add large compilation overhead.
- Optional narrow `cargo check --lib` only when native API integration reaches a meaningful compile gate, not after every edit.
- Planner read-only exploration should **not** trigger cargo builds.

## 9.2 Substantive compilation/test gates

1. **M1-C:** Native visual proof of Windows composition is an **early feasibility gate**; run its small smoke harness manually on Windows, without building the entire GUI suite if avoidable.
2. **After M2-B or M3-A as necessary:** One narrow Rust compile/test gate for new native bindings and pure effect logic; an extra run is justified only for a concrete compiler/runtime issue.
3. **M5-C:** One focused check and one targeted Nextest expression covering coordinate model, settings, render, controller, adapter, mouse dialog, parser/plugin/handler and a relevant native smoke binary, using actual workspace target names discovered from `cargo nextest list` or repository metadata. Example **illustration only** (adjust patterns to live test names):

```powershell
cargo fmt --all -- --check
cargo check --lib --bin multi_launcher
cargo nextest run --lib -E 'test(coordinate_tool) | test(mouse_settings) | test(plugins::mouse) | test(mouse_command)'
```

The test filter above is **not evidence** of actual test names; adjust it to the checked-out tree and supplement with separately named tests when the filter misses them. Avoid claiming successful tests if they matched zero cases.

## 9.3 Native acceptance

- Use a controlled on-screen checkerboard/palette and a text editor/browser UI with white, gray, black and colored elements. Capture observation carefully because capture-exclusion features may change what screenshots show.
- Verify crosshair default gap vs new gap, halo visible inverse (0/40/100%), zoom readable and cursor-centered, all combinations, multiple monitors including negative coordinates, 100/125/150% Windows scaling when available, very small settings dialog, repeated on/off and shutdown.
- Check focus and click-through (clicking buttons underneath lens/halo), global hotkey/launcher visibility, capture picker, Screen Draw, OCR and normal screenshot on the target system.
- Test standard composited windowed desktop first. Record outcomes for borderless games only if possible; exclusive fullscreen/protected capture limitations are not core failure conditions.
- Consider a simple timestamped visual performance note (mode, monitor, approximate refresh, CPU/GPU) if available; don't create or run a major new benchmark suite.

## 9.4 Long-running commands / wake-up cadence

- The orchestrator should avoid repeated polling of long Rust builds/tests. If delegating remote or asynchronous checks, **wait roughly 10–20 minutes between status checks** for genuinely long-running verification when it is possible to do so safely, rather than polling every few seconds. This is a *check cadence*, not a deliberate delay before reporting a known result.
- Inspect logs/process state only as needed. Do not start overlapping duplicate builds merely because an earlier build is still running.
- Repair specifically attributable compilation/test errors, then rerun the narrowest relevant target; do not restart a full suite reflexively.

---

# 10. Review contract and remediation policy

The independent reviewer should explicitly investigate:

- Is the 16-pixel gap actually measured from the hotspot to **all rendered edges**, including the outline, or merely an internal blank width?
- Is actual underlying desktop color inversion implemented? Can the screenshot/visual proof distinguish 40% inversion from 40% black opacity?
- Is magnified screen content tied to true cursor source coordinates in offset mode?
- Is the circle truly clipped? Are overlapped overlays (zoom, halo, crosshair, guide lines, HUD) excluded from native magnification input?
- Are all Win32/COM/graphics handles, regions, GDI objects and magnifier runtime resources owned and released correctly on the appropriate thread?
- Can the user interact through every overlay? Is focus/no-activate preserved during initialization/errors?
- Is old JSON compatible, transient enabled state never serialized, and dirty draft merge transactional?
- Is `mouse effects off` unambiguous, and does it preserve the coordinate HUD/pick?
- Is capture completely absent in crosshair-only mode? Does native initialization stay lazy?
- Are errors contained and honestly labeled, and are manual native checks distinguished from mock test passes?

Report substantive findings ordered by severity; have **one source-writing agent** remediate; use descriptive follow-up commits. Do not re-run historical unrelated repository qualification campaigns.

---

# 11. Final deliverables and definition of done

The orchestrator must provide:

1. Working native **crosshair gap**, genuine inversive cursor halo and genuine screen magnifier within the supported Windows desktop conditions; independent controls, predictable combinations and session-only enable states.
2. Updated Mouse Settings sections with proper scroll/resize behavior, defaults, Apply semantics and availability/fallback indicators.
3. Extended typed `mouse` command family, accurate command help/discovery and preserved old commands.
4. Backward-compatible preferences and focused tests with stated results; native visual evidence that satisfies must-pass gates or explicitly documented blocker/exception.
5. A **linear, understandable checkpoint history** with proposed `M1-A` … `M5-C` commits or an explained slight boundary adaptation. No automatic branch changes, pushes, squashes or history rewrites.
6. Concise final report with **actual commit hashes**, paths changed, test commands/outcomes, native tests actually performed, known platform limitations and any remaining unverified item.

**The project is not complete merely because settings sliders or translucent circles exist.** Actual desktop pixel inversion and magnification, non-recursion, click-through behavior and resource cleanup are core acceptance conditions.

## Execution ledger — October 8, 2026

- Baseline: clean `mouse-improvements`, HEAD `af09178e39d4c7a50fb7fefe5be2e266f8ea4e52`; current code matches the scoped plan baseline.
- Ownership: planner read-only handoffs; one implementer writes one checkpoint at a time; parent inspects/stages/commits and maintains this ledger; reviewer performs bounded independent review.
- Commit boundaries: retain the M1-A through M5-C boundaries and subjects in §7. No branch change or push.
- Native decision: evaluate windowed Windows Magnification API first in an isolated proof. Production effect resources will only be introduced after observed circular inversion, real zoom and exclusion pass M1-C.
- Verification budget: lightweight formatter/diff checks for M1-A/B, focused behavioral tests authored there and run together at the compilation gate; isolated native proof at M1-C; subsequent scoped checks per §9.
- M1-A: complete; parent inspected geometry and corrected raw-arm bound and zero-gap subtraction with implementer. Formatter/diff checks and grouped targeted tests passed.
- M1-A checkpoint: `b021b87f`.
- M1-B: complete; parent diff inspected, formatter/diff checks and grouped targeted tests passed. Typed gap command and field-only transactional merge integrated.
- M1-C: retained isolated harness implemented and reviewed; standalone build, 1 matrix test, formatter/diff checks pass. Native startup/keyboard/independent lens disable and resource teardown observed. Added bounded F12 composed desktop readback: actual 0/40/100% inversion and exact 2x offset zoom proved from native pixels, with captured sibling/self exclusion. Click-through and stationary live-update confirmation remain pending; production effects remain gated. See `cursor_effects_feasibility.md` for evidence and limits.
- M1-B checkpoint: `60689d88`.
- Grouped A/B Nextest: first run 19 passed, 1 test-helper failure, 5 not run due to fail-fast. Negative rays now stop at the bitmap edge. Rerun with `--no-fail-fast`: **25 passed**, 4,940 unrelated tests skipped; compile produced 3 existing warnings in untouched `src/gui/render.rs`. Filter covered settings, crosshair/guide rendering, transactional adapter and worker update, dialog, exact parser, handler and mouse plugin.
- M2-A, M2-B, M3-A, M3-B, M4-A, M4-B, M5-A, M5-B, M5-C: pending.
- Desktop access: ordinary Computer Use pixel capture times out, but the native fixture's bounded desktop readback includes composed output. Physical click automation fails because coordinate geometry is unavailable; requested user confirmation of click-through and stationary updates is pending.
- Gap independent review: no substantive findings. Native harness review's child-redraw finding resolved; parent reentrancy and scene-creation fixes applied before native evaluation.
- Test-helper remediation checkpoint: `9998849a`.

---

# 12. Quick orchestrator handoff summary

**Orchestrator:** Follow repository `AGENTS.md` and the separately supplied `multi_launcher_cursor_effects_codex_start.md`. Keep original branch and unrelated edits. Delegate planner (read-only), implementer (one well-scoped checkpoint at a time), reviewer (read-only). Perform M1-C native feasibility early. Then execute M2→M3→M4→M5 in dependency order, committing coherent checkpoints. Keep builds sparse and targeted; use slower wake-ups for genuinely long tests. Do not hide missing native verification or weaken the user's real-inversion requirement.
