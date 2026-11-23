# Mouse Effects Overlays

G22 owns `mouse-effects-overlay` and its four Windows modes. `plugins/mouse.rs::MousePlugin` exposes the launcher entries. `commands/parser.rs::parse_mouse_wire` provides the crosshair, click, halo, zoom and effects-off commands. The separate [Mouse Settings](mouse-settings.md) form controls appearance and the current session through the same coordinate controller. The [Coordinate HUD](coordinate-overlay.md) has an independent mode and lifetime contract.

| Criterion | Source-backed behavior |
| --- | --- |
| MOUSE-EFFECT-01 | Crosshair follows the signed physical hotspot. `coordinate_tool/native.rs::WindowsSurfaceBackend` creates a separate crosshair and horizontal and vertical guide HWNDs. Preserve color, width, length, center gap, opacity, contrast and optional monitor guides through cursor movement and topology changes. |
| MOUSE-EFFECT-02 | Click visualization receives admitted button events from the shared physical hook. The native backend reuses its bounded `click_surfaces` pool. Preserve per-button appearance, animation and expiration, signed click position, and clearing on disable or capture suspension. |
| MOUSE-EFFECT-03 | Halo retains its separately shown native host, magnifier child and optional outline. Preserve signed source and cursor centering, output bounds and appearance, existing fallback behavior, and source filtering that excludes owned surfaces. |
| MOUSE-EFFECT-04 | Magnifier retains its own host, magnifier child and optional outline, live source and signed physical monitor bounds. Preserve configured scale, anchored and offset placement, clamped source, and source filtering when coexisting with Halo or other surfaces. |
| MOUSE-EFFECT-05 | Preserve native topmost, pass-through and nonactivating behavior, capture exclusion, exact restoration of the current owner, and teardown for all modes. `mouse effects off` retires the four effects and preserves the independent Coordinate HUD state. |

Target the existing crosshair geometry, click lifetime, halo fallback/coexistence and `zoom_uses_signed_physical_monitor_bounds_not_work_area_or_dpi` tests when these boundaries change. Confirm each actual command and preference route, pointer pass-through, coexistence, suspension and retirement on a controlled Windows desktop. Use owned imagery for live desktop sampling and mixed-DPI or signed-origin monitors when available. Source specified. Native and GPUI verification pending.
