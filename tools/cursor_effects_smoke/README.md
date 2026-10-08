# Cursor Effects Magnification API smoke harness

This is a small, standalone Windows GUI proof for the cursor-effects feasibility gate. It does not
depend on or modify the production coordinate-tool renderer.

Run it from the repository root with:

```powershell
cargo run --manifest-path tools/cursor_effects_smoke/Cargo.toml --target-dir target/cursor-effects-smoke
```

The ordinary scene window is titled **Cursor Effects Smoke Scene**. Its buttons and keyboard
shortcuts can toggle the independently created halo and lens, set halo strength to 0%, 40%, or 100%,
switch the lens between cursor-centered and offset destination, recreate the native surfaces, toggle
the fake HUD/crosshair/guide overlays, and inspect the current state. F5 logs a desktop-DC pixel
sample at the cursor while the pointer remains over the scene; F6 toggles the halo, F7 toggles the
lens, F8 cycles strength, F9 switches lens placement, F10 toggles reference overlays, and Escape
closes the harness. Focus the scene first, then move the pointer over a color patch and press F5 to
record an observation without moving the pointer to an Inspect button. F12 performs a one-shot
desktop `BitBlt(SRCCOPY | CAPTUREBLT)` readback around the cursor, lens destination, cursor source,
and a bounded context rectangle that contains the halo and offset lens.

The program writes concise diagnostics to stdout and `%TEMP%\MultiLauncherCursorEffectsSmoke.log`.
It logs the signed physical cursor/source/destination rectangles, selected matrix, filter HWNDs,
foreground before/after explicit operations, and create/destroy counts. A desktop `GetPixel` sample
is explicitly advisory: whether that DC includes composed magnifier output depends on the desktop
composition path. F12 saves unmodified desktop readback BMPs and RGB samples under
`target/cursor-effects-smoke/observations`; GDI may still omit layered `WC_MAGNIFIER` output, so
API success alone is not proof. Inspect whether the saved pixels contain the composed effect; if
they do, the image can support the native review. Compare with actual on-screen output as well.

The color matrix follows the row-vector layout shown in Microsoft's [MagSetColorEffect documentation](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetcoloreffect): RGB slopes occupy the diagonal and the affine strength term is in the last row. At 40%, the intended operation is `out = 0.2 * input + 0.4 * 255` per RGB channel; the harness logs expected swatch values so visual orientation can be checked against the actual desktop.

The harness uses one UI/message-loop thread for `MagInitialize`, all magnifier HWND operations, the
16 ms cursor timer, and `MagUninitialize`. The scene marker changes while stationary to expose stale
content. The magnifier hosts are separately titled, topmost, circularly region-clipped, layered,
no-activate, and click-through. A filter list excludes the four named fake feature overlays and
both magnifier hosts from each magnifier. The magnifier control is created without
`MS_SHOWMAGNIFIEDCURSOR`, so Windows draws the original native cursor.

Starting the harness only shows that the APIs returned success. It does **not** establish that
circular clipping, live composition, partial inversion, feedback exclusion, focus preservation, or
teardown worked on a target Windows desktop. Those outcomes require a visual/manual observation and
must be recorded separately in the feature feasibility note. Exclusive-fullscreen and protected
content are outside this smoke test.
