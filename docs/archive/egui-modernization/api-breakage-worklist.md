# M0-C: executable egui 0.36.2 API worklist

Status: inspected migration backlog, not implementation or validation. Scope is
the original 0.27.2 application and downloaded released crate sources. No Cargo,
application, Git, or native-input changes were made to prepare this document.
Use this with [implementation-handoffs.md](implementation-handoffs.md), preserving
its checkpoint gates. Checkpoints remain local only under current user direction.

## Evidence and dependency boundary: M1-A

Cached registry root on this host:
`C:/Users/Jay/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
The paths below are relative to that root; source versions are explicit.

| Owner / current call sites | Confirmed target fact | Required work |
|---|---|---|
| `Cargo.toml`: eframe/extras 0.27 | `eframe-0.36.2/Cargo.toml:53` defaults enable Wgpu; Glow is optional. Extras 0.36.2 requires egui 0.36.2. | Align the UI family to 0.36.2. Start with explicit Glow and retain required accessibility, fonts, links and platform capabilities; inspect feature unification rather than accepting the new default. Keep later Wgpu comparison in M2-C. |
| Direct winit 0.29; `src/main.rs:986` uses `EventLoopBuilderExtWindows::with_any_thread` | Eframe manifest `:266` requires winit 0.30.13 with `rwh_06`. `eframe/src/epi.rs:26` reexports builder/attributes, not the Windows extension trait. | Keep a matching direct winit 0.30 dependency unless a verified reexport eliminates the trait import. Preserve GUI worker thread construction and `with_any_thread(true)`. |
| Direct raw-window-handle 0.6; `src/visibility.rs:382`, `src/window_manager.rs:633` | Eframe requires raw-window-handle 0.6.2. `eframe/src/epi.rs:701` still implements `HasWindowHandle` for Frame. | Retain current typed handle extraction. A new raw FFI/window lookup workaround is unnecessary. Validate runtime HWND lifecycle in M2-B. |
| Toast 0.13 | `egui-toast-0.22.0/Cargo.toml` requires egui 0.36, defaults off. | Use released 0.22.0, no fork/second egui graph. API adaptations belong to M1-D. |
| CommonMark 0.15 | `egui_commonmark-0.25.0/Cargo.toml` requires egui/extras 0.36.0 and Rust 1.95. Defaults enable `load-images` and `pulldown_cmark`. | Use released 0.25.0; preserve parser/image capabilities actually used. No macro/syntax-highlighting/network feature expansion without existing need. |
| Direct image 0.24 with png/jpeg/bmp/ico/gif/tiff | `egui_extras-0.36.2/Cargo.toml:123` uses image 0.25.6 with defaults off. Eframe enables only image 0.25 png. | Existing image 0.24 features do not enable image 0.25 decoders. Audit CommonMark image loading separately from existing explicit 0.24 decoding. Enable only needed 0.25 formats if loader parity requires them; do not upgrade all image consumers blindly. |

Keep edition 2024, profiles, pinned rdev revision, windows API features and
`unstable_grab` unchanged. Inspect `cargo tree -i egui`, `-i eframe`, `-d` and
`-e features` after resolution. Duplicated image/windows versions alone are not
duplicate egui-type failures. Capture Windows all-target compiler diagnostics;
do not claim M1-A creates a runnable app while known API errors remain.

## Lifecycle, input and side-effect ledger: M1-B

Confirmed target interfaces: `eframe-0.36.2/src/epi.rs:49` app creation returns
`Result<Box<dyn App>, Box<dyn Error + Send + Sync>>`; `:167` defines
`logic(ctx, frame)` and `:182` defines `ui(&mut Ui, frame)`. Glow-enabled `on_exit`
retains its optional Glow context; the non-Glow-only build signature differs.
Update `src/main.rs:992` creator and preserve `src/gui/render.rs:3790` shutdown.

Important implementation detail: `eframe/src/native/epi_integration.rs:288`
calls both logic and ui inside the `Context::run_ui` callback. Egui
`src/context.rs:794` can repeat that callback for discarded layout passes.
Logic is once before each UI callback, not inherently once per integration
frame. Hidden `update_logic_only` uses `run_logic`, retains pending raw input,
and produces no paint output. Hidden logic sees stale last-pass events/time.

The current owner is `LauncherApp::render_root_frame` in
`src/gui/render.rs:2085`; `App::update:3786` delegates to it. Its tests invoke the
same owner directly. Split that owner coherently, without a second production
execution path:

| Current order / operation | New responsibility and invariant |
|---|---|
| Query observer begins; cache popup maintenance; native Frame capture (`:2088-2160`) | Separate tick bookkeeping from paint-pass bookkeeping. HWND publication and visibility wake/reconcile must remain possible while hidden. Paint/popup cleanup cannot run in hidden logic. |
| Snippet prompt keyboard precedence (`:2166`) before query/root actions | Visible fresh-input handling only. Preserve prompt > Escape cleanup/action sheet > launcher query priority. Never replay Enter/Escape/Tab from hidden stale input. |
| Provider generation refresh, worker/capture/runtime polling (`:2172-2240`) | No-paint runtime work progresses while hidden. Audit each called helper for nested UI first; keep operations once per admitted tick/frame despite discarded passes. |
| `macro_parameter_prompt.show`, Screen Draw toolbar, toast show | UI only; these exist inside the current mixed preamble. Hidden prompt requests may update state/register viewport intent and request the relevant repaint, but must not paint in logic or stall until an unrelated root paint. Preserve modal/prompt focus ownership. |
| Pending query, note debounce, queued timer/macro messages | Preserve one acceptance/consumption per request. Retain ordering against provider refresh and watch events; do not execute actions again on repeated UI passes. |
| Dropped files, unstable_dt, root inner/outer bounds (`:2249-2314`) | Read fresh visible input/geometry only. Hidden ticks must not repeat file drops, advance dashboard timing using stale dt, or overwrite placement with stale bounds. |
| Visibility revision inspect and ordered restore/parking (`:2315-2470`) | No-paint coordination must remain active on hidden wake. Preserve revision gates, focus intent, HWND generation and placement policy; no synthetic input, polling loop or arbitrary delay. |
| Menu, then `process_watch_events` (`:2580`), then root/dialog content | Maintain event/query/action ordering and drain exactly once. A forced earlier drain is a documented behavior migration requiring tests, not a mechanical substitution. |
| OCR early exit (`:3300`) and final observer/first-usable completion (`:3650-3750`) | Finish every opened observer boundary once. Report usable frame only after eligible actual painting, never from hidden logic or an abandoned pass. |

Use an explicit small operation ledger or equivalent ownership to distinguish
an integration tick, visible frame and repeated paint pass. A visible frame
counter alone cannot deduplicate hidden ticks; a pass number alone cannot
deduplicate repeated side effects. Tests should exercise a discarded second
pass, hidden worker/event progress, no stale prompt submission/drop replay,
and visible logic+UI single action consumption. Preserve existing query
observer, prompt and visibility integration tests. No native input success is
inferred from these headless tests.

## Widget and viewport APIs: M1-C

Target source references: `egui-0.36.2/src/containers/panel.rs:422,1212` panels
accept `&mut Ui`; `src/ui.rs:209,2194` provides `new_child(UiBuilder)` and
`scope_builder`; `src/widget_text.rs:758` still has `into_galley_impl`, now with
`TextWrapping`; `src/painter.rs:380,406` rectangle/stroke APIs require
`StrokeKind`. These are concrete API adaptations, not styling permission.

| Current call sites | Required adaptation / invariant |
|---|---|
| `render.rs` root TopBottomPanel/CentralPanel and `render_root_frame` test entry | Thread the integration root Ui through panels. Migrate `Context::run` fixtures to `run_ui` and pass `ui.ctx()` only where an API still needs Context. Never recursively start a second root pass. |
| `diff_dialog/{mod,folder_table,text_view}.rs`, `mouse_gestures_dialog.rs:1033,1114`: child_ui | Use UiBuilder with equivalent rect/layout/Id/clip ownership. Preserve virtualized row hit boxes and nested scroll geometry. |
| `notes_dialog.rs:523`, root geometry text measurement | Convert bool/width wrapping into explicit TextWrapping; use `fonts_mut` for layout/metrics methods requiring mutable FontsView. Preserve eager-oracle equivalence and vertical alignment. |
| `image_crop_editor.rs`, `note_panel.rs`, MKMacro frame/painter calls | Convert radius/margin types and explicit stroke placement without silently moving borders or shrinking hit targets. No geometric polish in Track A. |
| `data_recovery_dialog.rs:613`, `file_search_preview_dialog.rs:303,309`, `mouse_gestures_dialog.rs:1268`: copied_text output | Use Context copy command API with unchanged copied content and action ordering. Search all remaining output-field uses after migration. |
| `visibility.rs:2448,2478`; `radial_editor/mod.rs:2662`; QR/regex/MKMacro parameter prompt viewports | Target `context.rs:4062,4116` viewport callbacks receive `&mut Ui`, not Context. Adapt callback root panels and derive Context as needed; preserve stable ViewportId, close lifecycle and thread ownership. |
| Extras TableBuilder/StripBuilder in file search and MKMacro | Follow resolved API diagnostics while preserving column widths, stable row IDs and visible-row work. |

Compile all application/test/bench targets once the bounded adapters are in
place. Run focused changed GUI tests between repairs; full suites remain at
the adopted M2-D/M6-B gates. Preserve meaningful tests, not obsolete API syntax.

## Toast, markdown and image integration: M1-D

`egui-toast-0.22.0/src/lib.rs:190` takes `show(&mut Ui)`;
`src/toast.rs:21` adds `style: ToastStyle` to Toast. Adapt existing struct
construction sites using default style unless existing semantics require more.
Keep `src/gui/mod.rs:251` push/log boundary, RIGHT_TOP anchor at `:1832`, duration,
stacking, error visibility and `tests/toast_log.rs`. Render from UI, never logic.

`egui_commonmark-0.25.0/src/lib.rs:103` has argument-free `new()`; `:236` show
still accepts Ui/cache/markdown. Current `src/gui/note_panel.rs:3928` supplies a
per-segment ID to old new(). Preserve that identity with scoped UI rather than
discarding it. Retain the cache field at `:907`, markdown links/code/task/custom
segments and selection/scroll behavior. Check `clear_scrollable` at `:1690`
against the resolved backend cache API before adapting. Preserve lazy texture
creation and existing explicit decode owners in QR/crop/file preview/MKMacro.
Verify image formats through actual resolved 0.25 features, not 0.24 assumptions.

## Semantic font activation and geometry: M2-A

Current cache coupling: `render.rs:254,414,438,541,565` stores/compares font atlas
Arcs; `notes_dialog.rs:436,453,547,1328` does likewise. Target
`epaint-0.36.2/src/text/fonts.rs:813` FontsView exposes definitions but no public
revision counter or texture_atlas method. Fonts itself exposes a borrowed atlas
at `:762`; borrowing its address is not a safe generation replacement.

Use one explicit application font/layout revision boundary shared by root and
notes. Preserve existing width/PPP/font/spacing/padding/grid/result/projection
keys. Palette-only changes repaint without revision or geometry rebuild.
Font family/definitions, typography, extra line spacing and other effective
metric changes must invalidate geometry. Do not hash/compare font bytes, clone
atlas images, or traverse all rows just to check validity each frame.

Activation order matters: `egui/src/context.rs:2106` set_fonts queues definitions
for the next pass and compares TTF data expensively. Track requested versus
active font revision so old-font geometry cannot be tagged as new and reused
after activation. Apply font/style changes at a defined pass boundary; test a
font request made during a pass and its next-pass activation. No-op settings
must avoid reinstalling fonts. Independent viewport PPP remains a cache input.

Migrate pointer-identity expectations in Track C2D/C2E and notes tests into
semantic invalidation assertions. Keep font-change recipe coverage, frozen
eager geometry, capacity receipts, popup ownership and full-sized fixtures.
Exact focused seams include `root_list_geometry_rebuilds_only_when_layout_or_display_inputs_change`,
`root_grid_geometry_reuses_warm_cache_and_tracks_grid_layout_inputs`,
`track_c2d_oracle_root_list_recipes_match_frozen_eager_geometry`, and
`track_c_oracle_quick_notes_horizontal_width_transition_matches_frozen_eager_geometry`.
Run matched existing list/grid/notes/query ignored benchmarks after behavior
tests; retain five warmups/twenty samples and original fixture signatures or
explicitly document a necessary receipt-schema migration.

## Completion and remaining uncertainties

M0-C is complete when this source-backed backlog and ownership are reviewed;
it does not assert compilation, native parity, GPU performance or theme work.
M1 checkpoints implement interfaces; M2-A proves cache semantics; M2-B validates
actual Windows behavior; M2-C decides renderer; M2-D is the migration-only gate.
Only then begin event-driven theme runtime/transactional preview and polish.

Still to confirm during implementation: exact compiler-discovered call sites,
resolved image-loader format parity, buried UI in polling helpers, and the
smallest sound activation/once-only ledger implementation. These are bounded
implementation checks, not reasons to broaden into command/native-policy
refactors. Unsupported real chord injection, monitor/DPI/RDP and GPU cases
remain unrun unless supported evidence is captured; never substitute synthetic
CPU receipts for those outcomes.
