# Cursor effects native feasibility

Status: M1-C core feasibility gate passed. Native inversion, circular composition
and offset zoom were observed in composed desktop readbacks; the user confirmed
physical click-through and live updates with the pointer stationary.

## Candidate and ownership

Evaluate the Windows windowed Magnification API in an isolated
`tools/cursor_effects_smoke` package before production effect integration.
Use opaque layered, no-activate, click-through hosts, elliptical window regions,
1x blended display-RGB inversion for halo and 2x normal-color cursor-centered
source for the lens. Exclude both hosts and feature-owned sibling overlays using
local magnifier filters. Keep initialization, HWND ownership and teardown on the
message-loop thread. Production integration, if proven, will be lazy and owned
by the existing coordinate worker; crosshair/HUD must not initialize magnifiers.

Microsoft's documentation describes the required mechanisms but is not proof
that this desktop supports their combined behavior:

- [Magnifier host and control](https://learn.microsoft.com/en-us/windows/win32/winauto/magapi/magapi-intro)
- [Desktop source rectangles](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowsource)
- [Color effects](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetcoloreffect)
- [Local exclusion filters](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowfilterlist)
- [Region ownership transfer](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowrgn)

## Evidence required

- Actual circular filled 0/40/100% inversion over black, white and RGB scene
  patches; 40% maps black to 102, white to 153 and red to (153,102,102),
  allowing documented SDR rounding tolerance. Outside the circle is unchanged.
- Actual doubled desktop checker/text content at cursor-centered source,
  including an offset destination, with native pointer preserved.
- No sibling or self feedback; live scene update with stationary cursor.
- Click delivery beneath hosts, focus preservation, independent disable,
  repeated recreation and final resource teardown.
- Signed monitor coordinates tested where available; unsupported configurations
  and inconclusive capture methods recorded explicitly.

## Observations

October 8, 2026: x86_64 Windows toolchain and Computer Use desktop inventory
available. Initial inventory alone established no native effect acceptance.

The isolated harness subsequently built without warnings and its one matrix
test passed. Independent native source review found a missing child redraw;
checked `InvalidateRect` was added. Parent inspection found reentrant mutable
state access; guarded `RefCell` callbacks and posted close requests fixed it.
The rebuilt harness and matrix test passed; no production native code changed.

First native launch through Computer Use timed out awaiting app approval;
inventory confirmed no fixture windows. A normal shell launch then initialized
`MagInitialize`, created both magnifier hosts/children/regions, applied the
40% matrix and configured both six-HWND exclusion filters. Its cursor sampler
returned `GetCursorPos: Access is denied (0x80070005)`, and Computer Use could
not see any fixture windows. This was not an interactive-desktop proof. That
owned fixture process was stopped; cleanup cannot be claimed from this forced
termination.

The first unsandboxed automatic permission review timed out; its explicitly
permitted single retry succeeded. The interactive fixture initialized all
resources without the cursor access error. Its scene was returned by Computer
Use (title unexpectedly blank) and the accessibility tree exposed all controls.
Pixel capture failed with `FrameArrived timed out`, then `window capture timed
out` on the one recovery. An Exit-button click failed with `coordinate input
geometry is unavailable`. A fresh accessibility focus observation followed by
Escape successfully reached the scene's handler and exited with code 0:
foreground HWND preserved, two effect hosts created and destroyed successfully,
all four fake overlays destroyed. Those observations establish keyboard exit
and this teardown only; they do not establish visual composition or click-through.
Forwarding `WM_NCCREATE` to `DefWindowProcW` repaired the scene's missing title.
The next interactive launch reported a visible valid HWND, title `Cursor Effects
Smoke Scene`, window rectangle `(60,50..1290,840)` (1230x790), and client rectangle
`(0,0..1214,751)`. Pixel capture still timed out after one recovery. Capturing the
existing project Explorer window also timed out; the issue is not isolated to
the fixture. Accessibility and keyboard input remain usable.

F5 diagnostics observed actual cursor `(674,445)`, halo source/destination
`[614,385..734,505]`, lens source `[634,405..714,485]`, destination
`[714,445..874,605]` in offset mode. Advisory desktop-DC readback was
`(90,131,169)` at a point outside the static palette; this does not establish
composed output. F7 destroyed only the lens while the halo remained on, then
recreated the lens and refreshed both exclusion lists. Each keyboard operation
reported unchanged foreground identity. Both effects are left enabled in the
interactive fixture for the requested manual observation; Escape exits it.

### Composed desktop readback

The retained fixture now provides opt-in F12 bounded desktop readback using
`BitBlt(SRCCOPY | CAPTUREBLT)` into a top-down DIB, checked `GdiFlush`, and BMP
output. This desktop's resulting pixels include the native magnifier output;
ordinary Computer Use window capture still times out. Readback is proof tooling,
not a production capture backend. Independent review found no substantive issue
in its bounded allocation, GDI ownership, flushing or BMP output.

Actual artifacts are under `target/cursor-effects-smoke/observations` (ignored
generated output). The capture stamps were `1791495842532627900` (40%),
`1791495911449164900` (100%) and `1791495927601388800` (0%). Each has the
`readback-<stamp>-context-320x280.bmp`, `-halo-120x120.bmp`,
`-lens-destination-160x160.bmp` and `-lens-source-80x80.bmp` files.

At cursor `(668,434)`, halo bounds were `[608,374..728,494]`, lens source
`[628,394..708,474]`, and offset destination `[708,434..868,594]`.
Direct inspection showed circular inversion with scene text inside it, an offset
circular lens with enlarged text, and white fake guide lines absent inside both
effects while present outside. In overlapping regions the lens retained ordinary
source colors rather than the transformed halo colors.

Python standard-library BMP comparisons of the three context captures found:

- 10,965 pixels changed between 0% and 100%, with none outside the halo rectangle.
- All changed pixels matched `100% = 255 - input` per channel exactly.
- All corresponding 40% pixels matched `round(0.2 * input + 102)` exactly.
- The scene background `(31,34,42)` became `(108,109,110)` at 40% and
  `(224,221,213)` at 100%; an outside green corner stayed `(0,255,0)`.
- All 17,692 interior lens samples (radius 75 within the 160-pixel destination)
  matched the 80-pixel source at `(floor(x/2), floor(y/2))` exactly at 0%.
  Lens content stayed unchanged across halo strengths.

These are real composed-pixel observations, not just matrix unit tests or API
return values. They establish actual partial inversion and actual 2x zoom of the
cursor-centered source in offset mode. A further centered-mode capture
`1791496314182263100` showed circular enlarged scene text at the cursor; its
overlapping source readback is not used for the exact source comparison above.
Native logs reported preserved foreground identity after capture and placement
changes. The fixture is left at default 40% halo with an offset lens.

### Current gate outcome

On October 8, 2026, the user explicitly confirmed that clicks reach controls
beneath the halo/lens and content updates inside the effects while the pointer
is still. These are user-performed native observations, not automated click
results: Computer Use clicking fails with `coordinate input geometry is
unavailable`. Together with the composed pixel evidence, observed independent
disable/recreation, focus preservation and resource teardown, they pass M1-C's
core feasibility gate. Proceed with the Windows Magnification API for M2–M5;
no alternate capture backend is needed based on these results.

After the user's checks, Escape closed the same interactive fixture with exit
code 0. Its recorded button operations preserved foreground identity through
independent toggles, recreation and placement changes. Final cleanup reported
9 effect hosts created and 9 destroyed, and successful destruction of all four
reference overlays. This is actual fixture lifecycle evidence; final production
integration still needs its own scoped lifecycle and interaction verification.

Still not established: full controlled black/white/RGB swatch acceptance;
sustained non-recursion/flicker acceptance; actual production overlay exclusion;
multi-monitor/negative-coordinate/mixed-DPI visual acceptance and performance.

Actual standalone checks (all successful):

```powershell
cargo fmt --manifest-path tools/cursor_effects_smoke/Cargo.toml -- --check
cargo build --manifest-path tools/cursor_effects_smoke/Cargo.toml --target-dir target/cursor-effects-smoke
cargo test --manifest-path tools/cursor_effects_smoke/Cargo.toml --target-dir target/cursor-effects-smoke
git diff --check
```

One standalone matrix test passed; build emitted no warnings. Native source
review found no remaining substantive safety/resource issue after checked child
invalidation. The fixture is a proof tool, not the integrated production backend.

G08 now has native pixel evidence for real inversion; Z01 has native exact 2x
source evidence; Z08 has native sibling/self-filter evidence from the captured
combination. Their broader final integration acceptance remains pending,
including live updates and sustained non-recursion. No alternate compositor has
been justified by these results.

## Crosshair checkpoints verified before this gate

M1-A `b021b87f`, M1-B `60689d88`, test-helper remediation `9998849a`.
Formatter/diff checks passed. The first grouped run exposed an over-counting
negative-ray test helper; after bounding its ray, the rerun passed 25 tests
(4,940 unrelated tests skipped). Three compiler warnings are in untouched
`src/gui/render.rs`.

Actual successful grouped command:

```powershell
cargo nextest run --lib --no-fail-fast -E 'test(coordinate_tool::settings::tests::) | test(coordinate_tool::render::tests::crosshair) | test(coordinate_tool::render::tests::guide_geometry) | test(gui::coordinate_tool::tests::preference_updates_persist_transactionally_before_runtime_publication) | test(gui::coordinate_tool::tests::draft_commit_merges_only_edited_fields_into_the_latest_settings_transaction) | test(gui::coordinate_tool::tests::crosshair_gap_command_updates_the_live_frame_without_restarting_the_worker) | test(gui::mouse_settings_dialog::tests::) | test(commands::parser::mouse_command_parser_tests::) | test(commands::handlers::coordinate_tool::tests::) | test(plugins::mouse::tests::)'
```

## M5-C production integration observation — first run

On October 8, the actual production controller/backend was exercised through
`target/debug/coordinate_tool_smoke.exe --cursor-effects-auto` on the interactive
desktop. This finite fixture changes only its own controller, never synthesizes
input, moves the pointer or accesses the clipboard. PID 8204 ran 12 named stages
with pointer fixed at `(14,632)`, near the monitor's left edge. Raw BMP readbacks
and native diagnostics are retained under ignored `target/coordinate-tool-smoke`.

Observed successes:

- Halo source `[-46,572..74,692]`, radius60, native scale1. On 2,491 static client
  pixels inside the visible circle, every 40% pixel matched `round(.2*c+102)` and
  every 100% pixel matched `255-c`; 4,150 static outside pixels stayed unchanged.
  Background `(248,248,248)` became `(152,152,152)` and `(7,7,7)`.
- Offset zoom used actual source `[0,592..54,672]`, exact native factor2, host
  `[54,632..214,792]` and child `[106,632..214,792]`. The missing off-monitor
  source was masked rather than fabricated. All 10,685 compared interior lens
  pixels sampling visible client content matched their native source coordinates
  exactly. 580 differences in the broader comparison were confined to native
  window-border/shadow pixels; these are not claimed as exact client evidence.
- Centered and fractional1.7/163px stages rendered real content with the logged
  native transforms and monitor-clipped source. All-four stationary captures
  changed 1,282 pixels in a lens region while cursor/source geometry and the
  cheap-render counter stayed unchanged (7 renders).
- Effects-off and repeated disable preserved the visible HUD, released effect
  hosts/ring and reported both effects Disabled. Foreground stayed `0xce0e16`.
  Exit0, one sampler/backend, 429 samples, 11 renders, one backend shutdown,
  zero owned windows remaining. Counts are observations, not an FPS benchmark.

**Integration gate remains open:** all-four capture
`desktop-readback-all-four-default-guides-1791506639919.bmp` shows source guide
feedback inside the lens despite seven correct filter HWNDs. A red vertical
line appears at lens center x134 (true guide x14), and a magnified horizontal
line near y712 (true guide y632). At `(134,748)`, zoom-only is `(248,248,248)` but
all-four is red `(255,0,0)`. The HUD drawing above the lens is an intentional
composition layer and is distinct from this source feedback defect.

The bounded next check recreates effects after cheap guides are already visible,
to distinguish filter/presentation timing from `UpdateLayeredWindow(ULW_ALPHA)`
compatibility. Actual corrected composed pixels are required before closing M5-C.
### Exclusion timing diagnostic

A second finite production run (PID28120, stationary cursor `(2,842)`) added a
final stage with guides already visible before effect recreation. It exited0:
one sampler/backend, 571 samples, 14 renders, one shutdown, zero owned windows.
`desktop-readback-recreated-effects-after-guides-1791507227540.bmp` shows normal
source content without magnified guide/crosshair/HUD feedback. The same direct
per-pixel-alpha surfaces are effectively excluded when already presentable;
there is no evidence requiring a renderer or global capture-affinity change.
The native repair will present cheap surfaces first and invalidate filters only
on presentation/backing transitions, without clearing failure latches.
Original logs are preserved as `native-run-1.log` and `native-run-2.log` beside
raw BMPs. The first run's observed defect remains required to pass after repair.
### Production repair validation — third run

PID27384 held the cursor at `(7,634)` through 16 stages. Presenting cheap
surfaces before filtering fixed initial all-four guide feedback: zero red
pixels in 4,203 lower-lens interior samples. The stationary repeat changed
665 pixels in that region without cursor/source movement. Foreground remained
`0x2e0e70`; exit0, one sampler/backend, 592 samples, 15 cheap renders, one
shutdown, zero owned windows remaining. Log: `native-run-3.log`.

The additional radius20 magenta-outline test exposed the same presentation
ordering problem for a newly shown ordinary ring. Hidden backing preparation
was insufficient: the physical ring had 126 magenta pixels, and its magnified
feedback had 264 magenta pixels in the selected lens interior. Capture:
`desktop-readback-all-four-magenta-halo-ring-1791508190675.bmp`.
M5-C remains open until rings are presented at their live destinations before
exclusions are refreshed and corrected actual pixels pass. No alternate
capture backend is justified; the cheap-surface ordering fix passed natively.

### Final production gate — fourth run (passed)

After staging both auxiliary rings visibly at validated live destinations
before filtering, PID31416 completed all16 stages with cursor fixed at
`(1280,720)`. Native zoom source was `[1240,680..1320,760]` at exact2x;
halo sources remained cursor-centered. In 7,449 lower-lens interior pixels,
initial all-four guides produced zero red feedback pixels and the new
radius20 magenta ring produced zero magenta feedback pixels. The actual
physical ring contained212 magenta pixels, so this confirms exclusion rather
than an absent ring. The same stationary lens region changed4,032 pixels
between repeat captures. Direct composition and circular real-content zoom
were inspected in the raw BMP, including text and the animated scene marker.

Foreground stayed `0x1f10e40`. Effects-off preserved HUD and disabled both
effects. Exit0, one sampler/backend, 817 samples, 15 cheap renders, one
backend shutdown and zero owned windows remaining. No fixture process remains.
Logs: `native-run-4.log`; final captures:
`desktop-readback-all-four-default-guides-1791509018058.bmp`,
`desktop-readback-all-four-stationary-repeat-1791509019853.bmp`,
`desktop-readback-all-four-magenta-halo-ring-1791509032461.bmp`.

The earlier native guide/ring defects are resolved. The production native
core gate is complete together with first-run exact inversion/zoom evidence
and the user's physical click-through/stationary-update confirmations.
Remaining unperformed checks are mixed-DPI/multiple-monitor visual transitions,
exclusive fullscreen/protected/game content, a sustained performance campaign,
and unrelated picker/Screen Draw/OCR workflows. Signed geometry is unit-tested;
these remaining environment checks are not represented as native passes.
API-success protected black pixels are not heuristically diagnosed. The
supported target is the ordinary composited Windows desktop.
