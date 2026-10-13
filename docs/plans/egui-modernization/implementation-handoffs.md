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

## Checkpoint commits and compilation dependencies

The planner clarified the single-crate compilation dependency. Preserve the exact checkpoint commit cadence, while recording acceptance separately. A coherent local commit may remain validation-pending until later required API adapters permit execution; never label it runnable or accepted prematurely.

- M1-A aligns the graph and inventories known API errors, as the contract permits.
- M1-B owns lifecycle/startup and its test adaptations. Its runtime acceptance can remain pending until M1-C closes unrelated compiler errors.
- M1-C closes compilation across all targets. Include only compiler-required M1-D glue (toast defaults/Ui plumbing, CommonMark constructor/stable IDs, viewport callbacks) and the intended shared font-revision interface replacing removed atlas types. If a small part of M2-A is required early, record it as an M2-A prerequisite implemented here.
- M1-D completes integration behavior and parity: markdown/cache/image formats, toast lifetime/position and native picker ownership.
- M2-A completes font activation, geometry invalidation/reuse, pointer-test migration, bounded storage and comparative cache benchmarks. Earlier interface work does not establish behavioral or performance acceptance.

Use the actual requested/active font-revision boundary with existing layout keys. Never use constant generations, borrowed atlas addresses, unconditional invalidation or full-font hashing as disposable adapters. After M1-C compiles, run pending M1-B targeted tests and record both the implementing and enabling commits without rewriting historical receipts. Each checkpoint records actual checks, remaining compiler/acceptance dependencies and reviewer disposition. M2-D remains the integrated migration gate; styling waits for it.

## M1-B next writer packet: LauncherApp extraction

Dependency: finish and review the focused vendor hidden-input correction first. Reuse the existing partial source edits; do not restart the migration. This is a substep of M1-B, not a new checkpoint. Scope: `src/gui/render.rs`, `src/gui/mod.rs`, `src/gui/mkmacro_dialog/parameter_prompt.rs`, `src/main.rs`, and directly necessary receipt owner methods/tests. Parent owns status/Git/native QA. One source writer.

Current partial state: `RootFrameLogic`, `UiFrameReceiptCandidate`, `root_frame_logic` and `pending_ui_frame_receipt` exist. Macro prompt acquisition/show are split; parameter prompt acquisition/bootstrap state exists. The old `render_root_frame` still contains the mixed update body; `App::update` and startup's non-Result creator remain. Preserve existing changes.

1. Extract the common no-paint runtime owner from the mixed pre-menu preamble: native identity capture, provider generation refresh, capture/overlay/coordinate/color/OCR polling, macro launcher query/command consumption, both prompt acquisition paths, Multi Manager/clipboard runtime/reconnect/save, startup notices, pending query, note debounce and error expiry. Preserve their current relative order. UI/key handling does not belong in this owner.
2. Extract visibility snapshot/reconciliation from outer-rect handling through the menu boundary; return visibility revision/invocation/should-visible through existing frame state. Hidden ticks must not read stale UI bounds. Preserve parking, revision admission and native activation submission.
3. Keep visible `process_watch_events`, background-query refresh, authoring-provider startup and timer/stopwatch/network refresh after the menu, as currently ordered. Invoke the same no-paint background owner from hidden ticks without rendering the menu. Do not move visible watch/action drains ahead of pending-query/prompt routing.
4. `logic` does first-visible-pass bookkeeping, clears previous candidate, captures source state, latches snippet ownership BEFORE routing prompt keys, runs runtime/visibility and stores frame state. Repeated UI passes reuse that state; do not drain/execute/persist again. `logic_only` runs runtime/visibility/background work on EVERY actual hidden tick, independently of visible frame numbers, invalidates paint candidates, and never reads stale keys/drop/time/bounds or opens/completes a visible observer frame.
5. `ui` uses the supplied root Ui. Paint each pass. UI-only pre-menu work includes popup/grid UI maintenance, fresh input traces, simple/parameter prompt show, toolbar, toast, dashboard timing and fresh bounds/drops. First-pass guard input consumption/timing/drop effects. Screen Draw Escape cancellation is visible input handling. Preserve post-watch newly opened snippet latch and opening-frame key consumption before launcher/action-sheet routing; keep dismissal ownership throughout repeated passes and OCR early return.
6. Guard post-paint effects too: deferred result execution, radial authoring/file-dialog intents, preferences, confirmation/prompt responses. Repeat painting without repeating effect consumption, dispatch or persistence.
7. Each actual UI pass replaces the receipt candidate with its paint query/provider/catalog/generation/root gates and actual completion timestamp, only when existing instrumentation requires it. Final `on_ui_frame_complete` takes/revalidates the candidate and publishes once after all discard decisions. OCR early return must use the same noneligible finalization path. No normal-frame query clone or clock when instrumentation is disabled. No GPU/presentation completion claim.
8. Aggregate genuine pending new-child bootstrap intents (simple/parameter prompt and radial placement notice where applicable). Clear bootstrap after successful registration, cancellation or shutdown. Preserve parked ROOT geometry/focus. Replace App::update with the new hooks and startup creator with `Ok(Box::new(app))`; preserve on_exit order.

Test adapter: compose the SAME logic/UI owners, finalize outside run_ui after its last pass, and keep adapter test-only. M1-C owns general Context::run-to-run_ui plumbing. Do not retain a production alternate update path or inline completion that falsifies discarded-pass coverage.

Required focused behavior: forced discarded pass executes broker/watch/action/persist once; hidden ticks progress requests without paint/stale Escape/CtrlEnter/Tab/drop replay; hidden simple/parameter prompt arms bootstrap without moving/focusing ROOT; existing snippet multiline/tab/Escape and CtrlEnter ownership assertions remain; macro one-shot handoff remains; Track C empty-browse/provider/catalog frame eligibility revalidates final candidate; OCR early exit closes observer once. Source checkpoint may remain implemented/validation-pending until M1-C enables those tests. No styling, cache migration, vendor redesign or full suite.

## M1-C next writer packet: compiler closure

Planner prepared this read-only packet while M1-B remained the sole source writer. Start only after that writer releases ownership and M1-B review corrections are resolved. Objective: all-target compilation on 0.36.2 with equivalent identities, geometry and input semantics. Read the actual current diagnostics and M1-B owner signatures; do not redesign lifecycle or the accepted vendor patch. The M1-A inventory's 621 library / 1,269 library-test errors include duplicates, not unique callsite totals.

1. Migrate high-count widget APIs using cached released sources: `DragValue::clamp_range` to `range`, `ScrollArea::id_source` to `id_salt`, `ComboBox::from_id_source` to `from_id_salt`, keeping the same salts. Replace `close_menu` with `close` or `close_kind(UiKind::Menu)` according to enclosing ownership. Use `MenuBar::new().ui` and supported `Popup` builders with existing IDs and memory/boolean state. Preserve context-menu ownership rather than treating every open popup as a context menu. Use `Tooltip` APIs with equivalent delay, anchor and disabled behavior; fixed-position menus remain popups.
2. Migrate child allocation to `new_child(UiBuilder...)` / `scope_builder`, preserving max rect, layout, salts, auto-ID allocation, clip and visible-only work. Panels and viewport callbacks receive `&mut Ui`; use `ui.ctx()` only for Context APIs. Preserve viewport IDs. Use `Frame::NONE`, `CornerRadius` and explicit `Margin` conversions; integer APIs must not silently truncate dynamic/fractional dimensions. Choose `StrokeKind` according to old stroke bounds rather than one blanket replacement.
3. Replace `SelectableLabel` with supported `Button::selectable` consistently in production, cached measurement and eager oracles, preserving sense, identity and padding. `Style::wrap_mode` retains None/default versus Wrap/Extend/Truncate. `WidgetText::into_galley_impl` now takes `(ctx, style, TextWrapping, FontSelection, Align)`. Use `fonts_mut` for mutable shaping/metrics. Context global style APIs must preserve the intended current scheme; change both schemes only where intended. Replace copied-text output with `ctx.copy_text`. Adapt AccessKit from actual diagnostics while retaining IDs, roles, actions and assertions.
4. Implement the intended shared font-revision prerequisite now, with context-scoped ownership accessible to both root cache and NotesGeometry. Route all application, test and benchmark font installation through it. Compare definitions only on installation if needed, never per frame; distinguish requested revision from fonts active in the measured pass, honoring next-pass activation. Cache keys read cheap active revision plus their existing metric/content keys. Palette changes do not bump it. Remove obsolete atlas Arc/ptr_eq dependencies without constant generations, borrowed addresses, full-font hashing, image-size proxies or temporary adapters. Preserve font-replacement cases and assertions. M2-A retains invalidation/oracle/performance acceptance.
5. Include only compiler-required M1-D glue: default Toast style, `show(&mut Ui)` in the UI owner, argument-free CommonMark constructor with old segment IDs retained through scoped UI, and actual cache API adjustments. Keep image decode/cache owners. Extras image0.25 features do not unify with direct image0.24; M1-D retains format/markdown/toast/cache parity acceptance.
6. Convert `Context::run` harnesses to `run_ui`: closure receives Ui, panel callers use it and Context-based dialogs use its context. Use `begin_pass` / `end_pass` only for genuine low-level tests. Preserve RawInput viewport/screen/DPI and widget IDs. Launcher lifecycle harness composes M1-B's same owners and finalizes once outside `run_ui`, never once per discarded pass or through a parallel production path. Headless tests that do not apply texture deltas must explicitly consume FullOutput with `drop_without_applying_deltas`; bare ignored outputs can panic under the new output lifecycle (already observed and corrected in vendor tests). Make pending M1-B lifecycle tests executable here.

Verification: iterate `cargo check --lib` for library blockers, then `cargo check --all-targets` for bin/test/bench closure. Run the narrow touched GUI/oracle/popup/selection/editor tests and pending M1-B lifecycle tests, not the full suite. Inspect diff and stale API references. Done: all targets check, relevant focused tests pass, M1-B acceptance is recorded honestly with implementing/enabling commits, fonts/integration prerequisites remain attributed to later behavioral acceptance, no styling or disposable adapters. Genuine uncertainties are compiler-discovered residual calls, resolved AccessKit/cache APIs, fractional margin semantics and old popup ownership.

## M1-D — integration parity packet (read-only planning; execute after M1-C)

M1-C required review follow-up before any M1-B test acceptance: `Context::default()` embeds viewports. `set_embed_viewports(false)` alone still falls back without a thread-local immediate viewport renderer. Extend the shared lifecycle test adapter with scoped renderer installation/cleanup, child `run_ui` and explicit FullOutput consumption. Preserve the actual production owners; check retained viewport state after final discarded-pass selection, not merely merged output. Execute all nine M1-B regressions, including hidden prompt wake, hidden toolbar and radial registration. Production M1-B source review is closed; this is a known test adapter defect pending M1-C, not a claimed test pass.

Objective: accept CommonMark, toast and image behavior after compilation closes, retaining the existing parsing, delivery and texture-cache owners. M1-C supplies compiler-required CommonMark scoped IDs, Toast defaults/UI call and test harnesses; these do not establish parity. M2-A retains font/cache correctness and performance acceptance. No styling, lifecycle redesign, broad image migration or full-suite expansion.

Scope: `src/gui/note_panel.rs` (`show_markdown_fragment`, `markdown_cache`, `mark_content_changed`, `render_preview_range_plain`, `handle_markdown_links`), `src/gui/mod.rs` (`push_toast`, `add_toast`, toast initialization), `src/gui/render.rs` (root toast UI), `src/gui/mkmacro_dialog/image_preview.rs` (`decode_job`, `show_sized`), QR/crop/screenshot surfaces and decoder features in Cargo files.

Required changes:

1. Keep argument-free `CommonMarkViewer::new().show(ui, cache, text)` within scoped UI preserving `note_seg_{cache_id}` identity. Retain one cache per panel, width constraints, preprocessing, task/collapse/link ownership and panic fallback. Read-only `show` avoids a second checkbox mutation owner. Target `prepare_show` installs extras loaders once per cache and deactivates link hooks; preserve these boundaries and `mark_content_changed` invalidation/`clear_scrollable`. Do not adopt upstream's hidden/buggy `show_scrollable` strategy.
2. Explicitly preserve image0.25 decoder features while retaining image0.24 consumers. Baseline extras0.27 shared image0.24 features with application decoders and `screenshots-0.8.10`, which enables defaults. Equivalent preserved0.25 features are `bmp, dds, exr, ff, gif, hdr, ico, jpeg, png, pnm, qoi, tga, tiff, webp`. Current0.25.6 lock evidence contains png/tiff dependencies without jpeg/gif/webp dependencies; direct0.24 features do not unify. Use a clearly named feature boundary, without broad defaults adding AVIF/network/SVG/animation. Static extras image loader strips GIF/WebP frame suffixes; animated extras loaders are unnecessary for baseline static parity.
3. Preserve root toast placement `.anchor(RIGHT_TOP, [10.0, 10.0])`, default `__toasts` foreground group/top-down stacking, configured durations/icons/progress/error text and exactly-once `append_toast_log` through `push_toast`. Toast0.22 enqueues into context memory, decrements unhovered `unstable_dt`, pauses on hover and removes expired entries on the next show. Do not substitute wall-clock lifetime or render root toasts in children.
4. Preserve path-keyed plain-note preview textures/bounded resizing, MKMacro background decode/upload outside egui data lock, QR raster/texture reuse with NEAREST sampling, and crop/screenshot geometry. No per-frame decode/upload or duplicate loader/cache/delivery owners.

Tests: reuse markdown `horizontal_markdown_fragments_shrink_vertically_with_long_links`, `split_preview_long_unbroken_content_does_not_expand_dialog`, `markdown_analysis_is_reused_until_content_changes`, `programmatic_content_replacement_invalidates_cached_markdown_analysis`, `preview_preprocess_preserves_markdown_blocks_and_raw_html`, `disabled_interactive_checkboxes_do_not_mutate_during_preview_render` and link-routing tests. Reuse MKMacro `decode_allocates_only_thumbnail_rgba`, corruption/redecode/invalidation tests, QR `qr_editor_refreshes_exact_source_and_reuses_unchanged_generation_and_texture`/PNG roundtrip, affected crop clipping tests and `tests/file_drop.rs::dropping_file_opens_add_dialog`.

Add missing migration protection only: two identical markdown fragments retain distinct stable scroll/widget IDs across repeated rendering/content changes; actual extras loader decodes small JPEG/GIF/WebP/PNG fixtures with bounded asynchronous readiness and remaining formats are enabled at the0.25 boundary; controlled toast frames verify ordering/root anchor/configured expiration/hover pause through public rendering hooks. `tests/toast_log.rs` protects logging, not lifetime.

Verification/done: filtered Nextest for touched behavior, all-target check after dependency changes, image0.25 feature-tree inspection, diff inspection, and focused native smoke for rich preview/link/code/selection/scroll, toast/error, images/drop and note/QR picker cancel/return focus. Record actual evidence and unrun native checks. Genuine uncertainty: picker ownership/focus requires actual OS observation; existing rfd calls lack explicit parents, so add plumbing only for a demonstrated migration defect through the existing native owner. Planner performed source/cache inspection only, no builds/tests/edits.
