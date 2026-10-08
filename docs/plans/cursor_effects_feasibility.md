# Cursor effects native feasibility

Status: native visual proof pending fixture inspection. This note does not establish acceptance.

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

### Current gate outcome

M1-C retained harness/source checks are coherent, but **native visual acceptance
is awaiting user observation**. The parent requested confirmation of a visible
partly inverted circle, true cursor-centered zoom, circular clipping and absence
of overlay feedback. Production M2–M5 remains pending. This is an observation
access limitation, not evidence that Magnification API failed its appearance
contract; switching capture backends without such evidence is not justified.

Not executed/established: controlled 0/40/100% native swatches; circular pixels
outside/inside bounds; actual doubled screen content; stationary live content;
click-through; recursive feedback; actual production overlay exclusion;
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

G08, Z01 and Z08 remain **unexecuted**. API success and the pure matrix test
do not satisfy circular rendering, actual pixel inversion, real zoom or
non-recursion acceptance. No alternate compositor has been justified by these
results: desktop access failed before either backend could be evaluated.

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
