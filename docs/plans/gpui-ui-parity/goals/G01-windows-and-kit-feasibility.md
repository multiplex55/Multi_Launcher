# G01 — Prove the actual Windows frontend foundation

**Status: Validation pending.** G01-A/B startup and focused input are verified, G01-C has native utility and controller evidence, and G01-D built and launched the ordinary release and observed explicit fallback. The global invocation, foreground restoration, hidden-root, passive HUD and display placement criteria still need native evidence. See the [validation log](../VALIDATION_LOG.md).

Read the [master Goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md), [requirements](../REQUIREMENTS.md), [goal register](../GOAL_REGISTER.md) and [validation strategy](../VALIDATION_STRATEGY.md). All work stays on the same dedicated migration branch.


## Objective

Resolve the highest-risk feasibility questions inside Multi Launcher before broad UI ports: one ordinary Windows executable with both dependencies, reliable selected startup, and the native lifecycle this launcher needs. A separate Kit demo can diagnose a problem but does not accept this goal.

Dependencies: G00 branch/source baseline and enough startup/native inventory. May extract the smallest necessary seams before G02’s broader contract work.

## Source and documentation

Inspect Cargo.toml/lock, build.rs/application resources, src/main.rs, src/platform/single_instance.rs, src/platform/app_data.rs, active_window/window_activation, visibility, global hotkeys, utility hosts and existing overlay/native owners. Include src/platform/passive_tooltip.rs, whose independent no-activate/pass-through native lifetime can be preserved without an egui host. Current eframe runs on a worker with Windows any-thread enabled. Do not assume GPUI can copy that arrangement.

Use official Kit installation/getting-started/multi-window/testing docs matching the chosen version. Planning observed 0.10.0; actual version compatibility must be tested and pinned.

## G01-A — Dependency and supported runtime decision

Record current compiler, Windows target, SDK/build tooling, features, lockfile and patches. Introduce the minimal official Kit dependency with aligned GPUI family through the documented re-exports; avoid a separately selected incompatible GPUI crate or optional gpui-fast/JS/WASM expansion.

Inspect duplicate native/link dependencies and existing vendor patches. Keep ordinary egui build behavior and asset/resource handling intact. Choose and document supported GPUI event-loop/thread ownership with the smallest source changes required.

**Evidence:** actual dependency/build diagnostics from affected targets and recorded resolution. **Accept when:** the real manifest/lock resolves and both frontend implementations can be linked into the intended app without a hidden second runtime. Commit the coherent dependency/entry seam separately from further UI work.

## G01-B — Same-binary selection proof

Add only the minimal early selector/host split required for proof, respecting helper entry modes and ownership-before-persistent-load. No selector selects egui; explicit GPUI enters a native Kit shell; explicit egui uses the established eframe path.

Initialize assets and Kit once in the GPUI branch. Ensure the inactive frontend creates no runtime/window/graphics context. Do not instantiate LauncherApp as an invisible engine. If current startup is too coupled, extract the minimum honest service boundary rather than fake engine success.

**Evidence:** build and launch the actual Windows app artifact with all three selections; record binary/hash/profile/source. **Accept when:** ordinary and explicit-egui launches work, GPUI displays and accepts input, and selected-only initialization is supported by source and runtime observations. This is foundation evidence, not full launcher parity.

## G01-C — High-risk native capability probes

Exercise a real global hotkey with repeated summon/hide, editable text and keyboard focus, an independent utility while root is hidden, close/reopen, foreground restoration and relevant no-activate/topmost behavior. Use a representative overlay or existing independent capture/native subsystem to test DPI/monitor coordinate and input-restoration integration early.

Do not require a finished radial designer/Screen Draw studio now. Prove the shared capability or identify a specific unsupported seam and narrow adaptation. Native failures must not be hidden by arbitrary sleeps, duplicate hooks, permanent polling or synthetic focus tricks.

**Evidence:** short actual native scenarios with expected/observed results, cleanup and display environment. **Investigation complete when:** every high-risk capability used by later goals is proven or has a concrete unresolved blocker/dependency. **Accept this checkpoint only when its required capabilities are proven.** Unresolved required capabilities remain Validation pending or blocked and prevent the affected G01 acceptance; continue independent work without implying the blocked capability passed.

#### Pinned Windows backend findings for the implementation

- In `gpui-pre-windows` 0.3.12, app hide and activate are no-ops, and `Window::activate_window` can send synthetic Alt. Use a small HWND adapter and the existing foreground policy. Resolve HWND from the current GPUI window handle, validate ownership and use `SW_HIDE` or `SW_SHOWNOACTIVATE` on the GUI thread after leaving the app/window update. `WM_SHOWWINDOW` can synchronously draw on reveal.
- Retain the root and its input across visibility transitions. Give GPUI the event loop on process main. Keep a separately owned Kit utility window while the root is hidden. Check actual close and process lifetime independently from hide.
- Use the existing `HotkeyTrigger::prepare_listener` and wake channel. Keep receives and bounded native foreground retries off the GUI thread. Validate the current request and external foreground identity before restoring focus. Shut down owned listeners and pending wake work on exit.
- GPUI Windows Popup uses toolwindow/topmost styles, and `focus: false` at creation does not add persistent no-activate behavior. Use the existing native Coordinate Tool HUD adapter for a passive/topmost/pass-through and DPI probe. Keep its enable/shutdown work off the GUI thread and record actual native behavior.

#### Current implementation boundaries

- **G01-C1:** Bind one existing global hotkey listener to the retained root on process main. Accept a wake once, keep blockable I/O off the GUI thread, validate current request and HWND identity, and preserve the retained query on hide/show. Open a separately titled Kit utility. Verify owned-root close and listener shutdown, including when a utility is open.
- **G01-C2:** Exercise the existing Coordinate Tool HUD in the selected GPUI session. Preserve its passive and topmost native styles, validate monitor/DPI conversion and test cleanup. Record the actual display configuration and any unavailable evidence. The probe covers native capability only.

## G01-D — Production packaging and fallback foundation

Build through ordinary cargo build --release at this meaningful dependency/hosting boundary. Verify same-artifact launches from the normal distribution layout and a non-source working directory where supported, preserving canonical data rules. Inspect fonts/icons/resources; do not infer Windows executable identity from a platform-generic icon property.

Exercise clean exit/relaunch and manual egui recovery after a controlled GPUI startup failure. Detailed safe switching and existing-instance behavior is G03; do not claim crash catch-and-hot-swap support.

**Accept when:** the intended Windows distribution supports both entry branches and resource loading and no required runtime-selection or platform capability remains unproven. Record unresolved required platform blockers accurately; recording them does not satisfy G01 acceptance, although independent work may continue.

## Test budget and non-goals

Use targeted compile/build commands as dependency changes require; no full Nextest baseline. Do not rebuild release after every probe if the binary did not change. Headless view tests do not prove the native capabilities above.

Do not redesign the product, add an installer or daemon, drop egui, or make GPUI the default. Keep proof scaffolding bounded and replace/remove it as production views take over.

## Exit and next goal

Reviewer checks actual binary/lifecycle evidence and remaining blocker scope. Update source/component/core/window records. Continue G02–G04; a successful hello-world is not completion of this program.
