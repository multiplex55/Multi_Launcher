# Planner handoffs and execution boundaries

The execution contract remains `../MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md`.
This records the project planner's inspected handoffs, not completed implementation.
One implementer owns one checkpoint at a time; parent owns Git and review gates.
All checkpoints preserve command, persistence, window, hotkey and plugin semantics.
No source changes precede original baseline evidence. No styling precedes M2-D.

## M0-A / M0-B / M0-C

M0-A owns immutable Git/toolchain/lock/dependency receipts. Original source is
`72a01bd5451060d3f6b640b83acd081d7db76e77`; application paths match its parent
`00f2fc53`. The named archive is missing; never claim archive identity. Historical
performance recipes can be read from that parent without restoring deleted docs.
Local Windows MSVC Rust 1.99 is the migration host. Direct toolchain binaries avoid
the failing rustup shim. Record original compile/test diagnostics before upgrades.

M0-B owns golden interaction/screenshot inventory and original measurements. Reuse
`src/performance/{track_c,track_c_workloads}.rs`, including full fixtures: actions
100/1k/10k, notes 100/1k/5k, five warmups and twenty measured samples. Use process
environment `MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`,
`ML_TRACK_C_BENCH_MODE=full`, `ML_TRACK_C_SOURCE_SHA=<source>`,
`ML_TRACK_C_PROFILE=<actual>`. Run measurements serially without concurrent builds.
Exact ignored library owners include `track_c_benchmark_root_list_frame_geometry`,
`track_c_benchmark_root_grid_frame_geometry`,
`track_c_benchmark_quick_notes_dialog_geometry`,
`track_c_benchmark_root_query_to_usable_cpu_frame`; startup binary owner is
`track_c_benchmark_startup_action_catalog_owner`. Preserve sample arrays and oracles.
CPU headless measurements do not prove display/GPU/hotkey latency. Native work uses
an isolated synthetic workspace, current serializer, explicit owned notes path,
clipboard suppression and known process identity; never launch against personal
configuration. Record unsupported DPI/monitor/RDP and real chord checks honestly.

M0-C owns the released API/dependency worklist and explicit lifecycle/cache plan.
Official downloaded metadata confirms eframe 0.36.2 (MSRV 1.95), toast 0.22.0
(egui ^0.36), CommonMark 0.25.0 (egui/extras ^0.36.0, MSRV 1.95). Record feature
parity, once-only side effects, semantic layout revisions and exact test seams.
These are read-only inventory checkpoints; no theme or dependency edits.

## M1-A: dependency graph

Scope Cargo.toml/Cargo.lock and graph receipts. Align eframe/extras 0.36.2 and
released wrappers, retain a single UI egui family, edition 2024, rdev pinned revision,
Windows features and project feature flags. Preserve original Glow explicitly;
modern defaults select Wgpu. Direct winit is used by main's with_any_thread extension;
align its event-loop type or reuse a supported reexport. Verify tree inverse,
duplicates and features, then Windows all-target compiler backlog. This checkpoint
may inventory API errors under the contract; never call it a runnable accepted
migration or push broken code as validated. No aesthetic changes.

## M1-B: lifecycle and startup

Owners: render.rs LauncherApp orchestration, main.rs startup, visibility.rs and
window_manager.rs native frame handles. `render_root_frame` combines background
polling, stale-sensitive input, prompt/toolbar/toast painting, visibility and end
of painted-frame receipts. It is also called directly by extensive tests.

New `logic(ctx, frame)` runs hidden and must not paint or replay stale input events
and time. Extract a no-paint runtime tick/native visibility reconciliation; retain
pending-query/provider/drain order and once-only actions/persistence. UI receives
the integration's root Ui, consumes current input/drop/viewport data and paints
prompt, toolbar, toast, menu, root and dialogs. Audit polling helpers before moving
them. Do not nest a new Context run. End-of-frame usable-query receipts require
actual UI painting. Keep test adapters composing the same owners, not a parallel
implementation. App factory returns Result; preserve startup bounds and on_exit.

Independent review added two mandatory constraints. The existing open snippet
prompt routes and latches keyboard ownership BEFORE plugin/macro/pending-query
processing. Preserve that visible pre-runtime input phase, and keep its ownership
for the entire frame even if Enter/Esc closes the prompt. Hidden logic must not
replay that input. A simple runtime-first/UI-input-second extraction changes order.

The target native integration enables two passes and invokes both logic and UI
inside the run_ui callback for each pass. Once-only work therefore needs explicit
discard/repeated-pass handling, not only separation between methods. Add a forced
request_discard test proving one action/persistence publication. Usable-query
completion must not count a discarded/unpainted pass; UI-body end alone does not
prove retained final paint. Inspect the supported pass/frame APIs when implementing.

Hidden prompt presentation also needs an explicit supported lifecycle path:
poll_macro_prompt and ParameterPromptUi::show currently combine request acquisition
and viewport creation. Glow can skip UI when root and every existing descendant is
hidden/minimized, so a new pending prompt cannot rely solely on a UI callback.
Separate acquisition/presentation as needed and test this case while preserving
root parking/focus policy; never paint in logic or introduce synthetic focus input.

Tests must cover hidden worker/visibility progress with no stale key replay,
visible logic-to-UI once-only effects, and existing prompt/focus/visibility/query
observer contracts. Verify supported Windows focused/hidden toggle and prompt Esc.
No native focus workaround or style changes.

## M1-C / M1-D: mechanical APIs and integrations

M1-C owns changed widget/paint/UiBuilder/Frame/Margin/CornerRadius/menu/text APIs
across gui and settings_editor. Use diagnostics, preserve points/pixels, geometry,
IDs, accessibility, canvas clips, selection and virtualization. Tests migrate
Context::run to the supported root-Ui harness without weakened assertions. Compile
all targets and run touched geometry/editor/menu tests. No broad replacement that
silently changes geometry and no style polish.

M1-D owns CommonMark, toast and images. CommonMarkViewer 0.25 uses new() without
the old segment id: retain stable per-segment identity with scoped UI. Reuse its
cache and preserve markdown/task/link/code/selection behavior. Toast 0.22 show
accepts Ui; preserve stacking, duration, anchor, logging and layer order. Match
existing PNG/JPEG/BMP/ICO/GIF/TIFF loader coverage: direct image0.24 features do
not unify with extras image0.25. Retain lazy decode/upload and picker ownership.
Run touched notes, toast_log, crop/QR/file-preview tests and supported native smoke.

## M2-A: semantic geometry/cache identity

Owners: existing RootListGeometryCache and NotesGeometry plus a small shared
font/layout revision boundary. Root keys already include generation/count, width,
PPP, FontId, wrapping, padding, interaction size/spacing, full paths, grid columns
and nominal cell metrics. Notes keys include entry/metadata/projection generation,
width/PPP/title/small font, wrapping, spacing and interaction size. Preserve them.
Both currently own TextureAtlas Arcs and use ptr_eq; 0.36 exposes a borrowed atlas.
Never replace this with borrowed address identity.

Use a stable cheap semantic font configuration/layout revision shared by both
caches. Application font installation updates it exactly once for effective
metrics changes; palette changes preserve it. No per-frame byte hashing, atlas
cloning, per-row mutex or full-list traversal. Atlas growth alone is not geometry
change. Preserve bounded cache memory, extents, scroll anchors, popup IDs and eager
oracles. Rewrite pointer tests to prove same configuration reuse, definition/family
replacement invalidation, width/PPP/font-size/grid-spacing invalidation and
palette-only reuse, without stale glyphs. Preserve Track C2D/C2E font replacement
recipes/signatures or explicitly document schema changes.

The revision must represent fonts actually active in the measured pass, not merely
a pending set_fonts request. Confirm target activation timing, route all test and
benchmark font replacement through the same boundary, and test request during the
current pass, rebuild with effective fonts on the next pass, then warm reuse.

Verify `root_list_geometry_*`, `root_grid_geometry_*`, Track C2D/C2E oracle tests,
`track_c_oracle_quick_notes_dialog_matches_frozen_eager_geometry` and horizontal
width transition oracle. Compare full matched cold/warm owners including
`track_c2d_benchmark_root_list_cold_geometry_recipes`,
`track_c2e_benchmark_cold_root_grid_geometry` and notes/query owners. No theming yet.

## M2-B / M2-C / M2-D: migration gates

M2-B owns native compatibility and real Windows checks: HWND generation, parking,
activation, secondary viewport creation/close, negative origins/DPI, focus, hooks,
tap/hold and native overlays. Run focused existing tests and supported native
smoke; unsupported hardware/chords stay unrun. No native surface redesign.

M2-C owns measured explicit backend choice. Compare original Glow, migrated Glow
and safe Wgpu with matched profiles/settings/fixtures and repeated native startup,
idle/query/viewport/GPU measurements. Keep CPU owners separate. Fix reproducible
meaningful regressions before accepting a renderer.

M2-D requires contract fmt/lint/check/all-targets, automated suite, matched owners,
supported Windows matrix and reviewer behavioral/performance signoff. Record all
unrun checks and rendering deltas. This is the migration-only rollback checkpoint;
failed critical behavior or performance gates prevent Track B.

## Track B handoffs

Each row is a sequential checkpoint with the contract's acceptance/verification
requirements. Existing model and behavioral tests remain the source of truth.

| Checkpoint | Ownership, required change and proof |
|---|---|
| M3-A | settings/model.rs and focused gui/theme tokens; retain modes/colors/named_presets and additive serde defaults/clamps. Separate palette/layout identities. Test legacy/partial JSON, custom extremes, preset fallback, mode resolution. |
| M3-B | ThemeRuntime/startup/successful settings publication and OS-mode events; apply on effective changes, use M2-A revision, no frame IO/theme clones. Test no-op/hidden/secondary viewports and zero palette-only geometry rebuilds. |
| M3-C | theme_settings_dialog.rs plus ALL app close routes: open snapshot, draft preview, successful Apply accept, failed save retain draft, Cancel/X/Esc/panel dismissal rollback. Resolve rollback against current OS mode; define external-change conflict. Preserve SettingsEditor mapping's current.theme copy. Test all exits and failed_save_keeps_draft_dirty_and_returns_before_publication. |
| M3-D | Typography/icon initialization using existing licensed resources; one install per change, deliberate geometry revision, no generic asset framework. Test clipping/fallback/cache invariants and supported DPI. |
| M4-A | render.rs query finish; preserve ID, IME/caret/focus/navigation, prompt precedence and root dimensions. Compare first-frame/query cost and primitive counts with M2-D. |
| M4-B | Semantic list/grid styling with stable ordering/IDs/popups/anchors and visible-only work. Preserve geometry inputs; rerun eager oracles/warm-cache tests/full Track C owners. |
| M4-C | Small native-widget helpers for headings/forms/status/focus used by root/dialogs, stable IDs and compact showcase state checks; no component framework. |
| M5-A | settings_editor render/state/mapping/save and theme shell; preserve sections/values/save-cancel/concurrent themes. Existing settings_editor and preview tests plus minimum size/DPI QA. |
| M5-B1..n | Sequential 1–3 related high-use dialogs per commit: notes, clipboard, MKMacro, Multi Manager, file search, radial egui editor, mouse, regex. Preserve canvas/highlight/worker semantics; path ledger, focused tests, screenshot pair and idle/performance disposition per group. |
| M5-C1..n | Actual remaining inventory: JSON/QR/calendar/crop/image/aliases/bookmarks/recovery/prompts/timers/snippets/toasts/secondary windows. Record migrated/exception/deferred per path and reason; verify inheritance and parent position. |
| M6-A | Prefer explicit justified omission of optional effects. Any accepted effect is measured, default off, bounded and ceases idle repaint; no native acrylic/blur. |
| M6-B | Original vs M2-D vs themed matched distributions, required final suites/native checks/persistence/rollback and independent review. Deliver versions/backend/SHAs/surface ledger/screenshots/unrun checks/rollback commits with honest limits. |
