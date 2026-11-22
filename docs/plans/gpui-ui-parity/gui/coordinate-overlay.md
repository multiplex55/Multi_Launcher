# Coordinate overlay

G22 owns `coordinate-overlay`. The native Windows controller already has a GPUI feasibility receipt at `G01-C2-HUD`. The production command adapter, visible HUD, pointer behavior and mixed-monitor checks remain pending. See the [validation log](../VALIDATION_LOG.md).

## Routes and owners

`plugins/mouse.rs::MousePlugin` provides the launcher results and help. `mouse` lists actions and `mouse coords` filters the coordinate controls. Advanced commands parse through `commands/parser.rs::parse_mouse_wire`. The `mouse settings` and `mouse click settings` commands and the Settings search handoff open the separate `mouse-settings` surface. Its HUD checkbox applies to the current session immediately. Its coordinate space, density and signed X/Y offset are draft settings with Apply and Cancel.

`coordinate_tool/controller.rs` owns runtime state and worker lifetime. `coordinate_tool/native.rs` owns the passive HWND and signed physical-coordinate sampling. `coordinate_tool/render.rs` determines content and DPI-scaled layout. `gui/coordinate_tool.rs::CoordinateToolGui` connects typed commands, preferences and clipboard, while the launcher coordinates capture admission, parking, visibility revision and errors. Reuse the platform controller and capture owners through the GPUI adapter.

## Behavior and native acceptance

| Criterion | Required behavior |
| --- | --- |
| COORD-01 | Use the existing HUD enable/disable and coordinate actions, with exactly one controller per active frontend session. Shutdown retires the worker. The HUD is topmost, layered, pointer-transparent and nonactivating. It has no actionable controls of its own. |
| COORD-02 | Sample signed physical desktop, monitor and foreground-client coordinates. Use the last external HWND while an app-owned window is active. In Compact mode show the selected value, monitor and any sampling error. Detailed mode also shows origins, bounds, effective DPI, virtual desktop, frozen state, last successful copy and help. |
| COORD-03 | A frozen sample remains fixed while placement follows the live cursor. Place the HUD within the current monitor work area with the configured signed physical offset and DPI-scaled layout. Validate on both sides of a mixed-scale monitor seam. |
| COORD-04 | Copy uses the current or frozen displayed sample. Only a successful protected clipboard write updates the copied indicator. Pick snapshots the coordinate space at admission and samples the actual pointer down location, independent of freeze. Escape drains the owned input pair and preserves clipboard data. Repeated Pick remains idempotent. |
| COORD-05 | Preserve capture exclusion with pin, color, OCR, Screen Draw, crop and macro capture. A superseding visibility request rejects stale pick effects and restores only its current owner. Turning off the other mouse effects preserves the HUD, freeze, copy status and pick. |

## Verification

Use the applicable library tests for HUD layout, frozen placement, clipboard failure, click-time sample, Escape drain, capture restoration and transient settings. Start with `detailed_hud_reports_origins_freeze_copy_and_help_context`, `freeze_keeps_display_sample_while_hud_placement_tracks_live_cursor` and `pick_copies_click_time_sample_in_the_start_space_even_when_hud_is_frozen` when those boundaries change.

For Windows acceptance, invoke the real Mouse command routes in both frontends. Verify HUD drawing and pointer pass-through over a controlled receiver, startup and teardown, correct focus, and placement at a mixed-DPI monitor seam. Exercise copy, pick and cancel with isolated clipboard data. The G01 proof observed worker start/stop and a 96 × 96 DPI sample. It left visual, hit testing, mixed-DPI and product route evidence open.
