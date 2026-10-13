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

M1-C required review follow-up before any M1-B test acceptance: `Context::default()` embeds viewports. `set_embed_viewports(false)` alone still falls back without a thread-local immediate viewport renderer. Extend the shared lifecycle test adapter with scoped renderer installation/cleanup, child `run_ui` and explicit FullOutput consumption. Preserve the actual production owners; check retained viewport state after final discarded-pass selection, not merely merged output. Execute all nine M1-B regressions, including hidden prompt wake, hidden toolbar and radial registration. Production M1-B source review is closed; this is a known test adapter defect pending M1-C, not a claimed test pass.

Additional concrete hidden-adapter correction: use `Context::run_logic(&RawInput, closure)` and LogicOutput for run_hidden_logic_tick_for_test, matching native dispatch. The prior begin_pass/end_pass adapter activated fonts and UI/plugin state, unlike hidden logic; remove its manual font promotion and test that pending definitions remain pending through hidden logic until a real UI pass. The immediate renderer setter is a Context associated function with no public clear/previous-callback API. Restore the embed flag accurately and establish test-thread lifetime/isolation for the stateless callback; do not claim scoped TLS restoration that the API cannot perform. Final harness review must assess that lifetime contract.

Focused reviewer guidance resolves the renderer lifetime contract: install one stateless callback per test thread using a test-owned TLS Once/Cell, capturing no app/context/fixture. Nested/sequential helpers reuse it; reinstalling during an active callback can panic on egui's immutable TLS borrow and adds avoidable benchmark-frame allocation. The RAII guard restores only each context's embed flag. No competing renderer or later no-renderer assumption on that thread; a test needing different semantics constructs and runs its entire fixture in a fresh thread and propagates panic. Reviewer verified local Rust1.99 libtest thread-per-normal-test source; Nextest process isolation is stronger. Callback lifetime ends with the test thread, not the helper. This is guidance pending implementation/execution/final review.

M1-C font-boundary design review: distinguish staged requests, input-hook submissions and active revisions. Use a context-owned egui Plugin: installation coalesces requests without calling `set_fonts`; `input_hook` submits the final effective request immediately before egui activates fonts; `on_begin_pass` promotes only that submitted revision afterward. This avoids the upstream active A -> queued B -> request A trap: upstream `set_fonts(A)` compares active definitions and leaves queued B intact. Cancellation before activation must retain A's active revision. Hidden `run_logic` invokes neither activation hook. Discarded and immediate-child passes can activate context-global fonts. Bare `Context::begin_pass/end_pass` invokes the input hook but not the plugin begin callback, so genuine low-level harnesses explicitly promote the submitted revision after begin_pass or migrate to run_ui. Cache reads never promote requests. Register before the first relevant pass, initialize fresh contexts without pre-pass font access, and keep definition comparisons at installation; palette changes and atlas growth retain revision. The reviewer confirmed these constraints from cached upstream sources; implementation and tests remain the M1-C writer's responsibility.

Interim actual-source review found registration in App::logic too late: run_ui captures its ordered plugin list before app logic. A first-frame font request plus discard can activate fonts on pass two while the captured begin callback list omits the owner, leaving revision stale. Register during LauncherApp construction before the first run_ui and at standalone context setup, remove per-frame registration, and cover first-frame request+discard through the actual setup contract. TypeId idempotence preserves plugin state but repeated add_plugin still allocates discarded handles. Other requested/submitted/active transitions in the inspected draft were sound; this is interim source evidence, not final acceptance or an executed test.

## M1-D — integration parity packet (read-only planning; execute after M1-C)

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

## M2-A — cache acceptance packet (read-only planning; execute after M1-D)

Objective/ownership: accept migrated `RootListGeometryCache` and `NotesGeometry` correctness, cheap warm reuse, semantic font invalidation and bounded storage. Reuse the reviewed M1-C context font owner; do not introduce a second tracker. Planner rechecked final M1-C state2026-10-13: `src/gui/font_revision.rs::{register, install, active_revision}` is established, including hidden run_logic deferring activation. M1-D retains CommonMark/integration acceptance.

Scope: `src/gui/render.rs` RootListGeometryKey, RootListGeometryCache::{ensure, ensure_grid}, invalidations, eager oracles and Track C observations; `src/gui/notes_dialog.rs` NotesGeometryKey/NotesGeometry, geometry_key, measure_notes_geometry, ensure_notes_geometry, eager oracles/capacity receipts. Shared revision owner changes only for missing acceptance or a demonstrated defect. Existing note_panel markdown analysis tests remain integration evidence.

Current root keys include generation/count, width, pixels per point, button font, wrapping, padding, interaction size, vertical spacing, full-path display, mode, columns, nominal cell width and grid spacing. Alias/presentation changes explicitly invalidate display; replacements change generation. Notes keys include entry/metadata/projection generations, viewport width, DPI, title/small fonts, wrapping, spacing and interaction size. Preserve ordered width expansion from earlier wide rows and original row indices/identities for anchors and menus.

Required changes:

1. Key the active font revision actually measured, never the requested revision; retain existing content/layout factors. Check concrete target metric gaps: .36 text resolution prioritizes `Style::override_font_id`, which inspected root style resolution and Notes title resolution can bypass; plain WidgetText includes `spacing.extra_text_line_spacing`, absent from the inspected root key. Key the fonts/spacing the corresponding widgets actually use. Preserve Wrap/Extend/Truncate distinctions where geometry differs, rather than reducing to a boolean.
2. Consumer acceptance: an in-pass font request leaves current-pass geometry valid; the next actual pass rebuilds using activated fonts; the following unchanged pass reuses. Test no-op/canceled installation without needless rebuilds, using Plugin owner tests as prerequisites rather than duplicating them.
3. Retain the already migrated C2D/C2E/C2F revision assertions and verify their oracles: changed definitions reject otherwise unchanged keys, rebuild once, match eager geometry, then reuse. Final planner recheck found no atlas/texture-atlas/Weak-pointer references in these owners; recipe cause tokens now use FontRevisionReplacement/font_revision_replacement. No further atlas migration is needed. Preserve fixture causes and unchanged result/search state; unrelated Arc identity assertions remain.
4. Retain one current geometry per owner, no revision-indexed history, no warm full-list traversal/shaping. Root vectors stay proportional to current/retained peak fixtures; grid tree/columns proportional to rows/columns; Notes rows/identity map proportional to filtered projection. Preserve replacement overlap accounting and existing partial-capacity receipt limitations; those are not total process memory.

Retain tests: `root_list_geometry_rebuilds_only_when_layout_or_display_inputs_change`, `root_grid_geometry_reuses_warm_cache_and_tracks_grid_layout_inputs`, `root_list_geometry_matches_eager_selectable_label_layout`, `root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, bounded viewport/absolute-ID tests, `quick_notes_browsing_virtualizes_rows_and_reuses_geometry`, both `track_c_oracle_quick_notes_*geometry` tests and C2D/C2E/C2F recipe oracles. The inspected Notes test expects text-style invalidation while override_font_id is set; migrate to effective target precedence while preserving geometry comparisons, not a stale implementation assertion.

Minimal missing coverage: list/grid/Notes consumer activation -> rebuild -> warm reuse; palette-only changes preserve revision/rebuild/measured-work while rendering changed colors (stripe-color tests alone do not establish this); effective override font size/family and root extra line spacing rebuild/match eager; one large -> small -> mode/filter transition checks capacities, identity-map length and repeated warm stability using current storage observations. Final planner recheck confirms Root ensure/ensure_grid and Notes title bypass override_font_id, Notes small-font key needs to match actual RichText font resolution, and Notes galley_size still collapses wrapping to Wrap/Extend despite a wrap_mode key. Reconcile measurement with actual widgets, including supported Truncate; key changes alone are insufficient.

Before measuring, reconcile benchmark receipt owner labels with the completed migration. Parent observed `C2C_REUSE_RESULT` still naming `egui_Context_run+LauncherApp::render_root_frame` after those APIs were removed. Preserve recipe/schema compatibility where needed, but report the actual current run_ui/lifecycle owner and explicitly document label changes; do not attribute a new measurement to a removed path. Historical baseline labels stay unchanged.

Planner recheck also found stale owner strings in Notes width receipts. Keep historical test names where behavior still matches. The serial recipe in benchmark-results.md is the existing runner; no dedicated Track C CPU wrapper was found. Omit qa-framebuffer-capture and unset ML_QA_FRAMEBUFFER_REQUEST_MANIFEST for all performance runs.

Verification: focused Nextest cache/oracle filters, all-target check after source/harness changes, then serial matched CPU comparisons for81 affected original cases: list6, grid30, Notes42, query3. Startup3 catalog cases are outside this milestone. Set MULTI_LAUNCHER_PERF=1, ML_SKIP_CLIPBOARD_SYNC=1, ML_TRACK_C_BENCH_MODE=full, ML_TRACK_C_SOURCE_SHA to actual measured source, ML_TRACK_C_PROFILE=fast-dev; restore environment afterward. Run each ignored owner separately, adapting this actual original pattern:

```powershell
cargo nextest run --success-output final --locked --offline --profile default --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(track_c_benchmark_root_list_frame_geometry)'
```

Then separately run grid, Quick Notes and query owners. Preserve100/1,000/10,000 actions,100/1,000/5,000 notes, grid columns1/2/3/5/6, width transitions, five warmups/twenty samples. Baseline: tracked receipts/cpu-original-summary.json and ignored baseline/{owner-list.log,track_c_benchmark_root_grid_frame_geometry.log,track_c_benchmark_quick_notes_dialog_geometry.log,track_c_benchmark_root_query_to_usable_cpu_frame.log,cpu-original-results.json}. Attribute production72a01bd5 and test runnerf45e3773 accurately. Match profile/settings/viewport identities; retain outliers; report p50/p95 deltas plus rebuild/work/storage receipts. Failed oracles invalidate timings. Investigate regressions without reducing fixtures/instrumentation. These are CPU-only measurements.

Done: semantic/oracle tests pass, warm reuse/storage bounds demonstrated, affected comparisons recorded and regressions resolved or explicitly explained. No styling, renderer selection, lifecycle redesign, native campaign or unrelated full suite. Planner performed source/test/recipe inspection only, no builds/tests/edits. Evolving M1-C signatures and already-migrated assertions must be confirmed from the final reviewed code.
### M2-A measured-regression diagnostic/correction substep

Trigger: accepted source df27da17 passed list6 oracles but cold medians rose10.8–14.4x and warm2.3–2.5x. Full raw samples and exact binary are preserved; see receipts/m2a-verification.md. Grid/Notes/query comparisons are queued. This substep stays within M2-A; no theme/backend/profile changes.

Objective: demonstrate the dominant cost, correct its actual owner, retain equivalent geometry and the original full benchmark contracts. One writer owns `src/gui/render.rs` and only directly necessary existing test helpers. Start with an ignored test-only diagnostic using current fixtures/setup/oracles/adapter. Parent retains full comparative benchmark execution and Git. No source modifications occurred during planner investigation.

Established facts: fast-dev settings match the original (dev plus line-table debug); do not alter optimization to obtain parity. Root geometry exposes `last_rebuild_nanos`; `track_c::snapshot()` exposes RootGeometryCold duration/calls/work. Existing result rows omit these durations. Cold work is one rebuild/count and warm work zero; row-proportional excess cannot be explained by constant adapter overhead alone. No extra visible-only domain workload was found. Mock Frame/handle/guard and lifecycle completion remain disclosed current owner costs.

Text candidate: original String conversion produced RichText;0.36 produces Text, resolves font/row height plus extra line spacing via font access, builds explicit-line-height layout job, then accesses fonts again for layout/cache. Released0.36 row-height invokes styled metrics including variation-axis processing. Galley eviction can remove offscreen jobs after warmup, as also happened originally; do not retain all galleys to make a benchmark faster. These are candidates until measured.

Ordered work:

1. Label diagnostic counts100/1,000 and disclose a small sample count. Record paired whole-owner elapsed, last geometry rebuild time, RootGeometryCold phase duration/calls/work, rebuild/measured/painted counts and actual adapter pass count. Use current visible setup/cold invalidation. Validate outputs and dispose texture deltas after timing.
2. If geometry dominates, measure aggregate display-string construction, effective font/row-height resolution, job construction, layout/cache/shaping and vector bookkeeping. No per-row logging. Supported galley-cache size observations are sizes, not hit counters.
3. On identical prepared inputs compare supported current WidgetText, equivalent jobs with invariant font/line height resolved once, and layout inside one fonts_mut scope where borrowing permits. Assert equivalent job parameters/eager geometry before comparing costs. Preserve width progression, font precedence/fallback, PPP, line spacing, Wrap/Extend/Truncate, alignment and colors. Switching to RichText alone is not an accepted optimization because it can change metrics.
4. Separately bound mock Frame and immediate-renderer guard for the smaller warm delta. Keep actual logic/UI/finalization inside the owner. No assumed subtraction or timer-boundary change.

Fix boundary: if repeated invariant metric resolution/context access dominates, resolve it once at the existing geometry owner and reuse equivalent supported jobs through its loop. Retain one cache, invalidation and bounded storage. If shaping/cache misses dominate, report evidence before selecting correction. No full-catalog galley cache, shaping disablement, stale jobs, weakened assertions or speculative released-library patch.

Verification: small explicitly labelled fast-dev diagnostic; affected list eager/invalidation/font/spacing/wrap tests; appropriate check/format/diff and bounded review. After an evidence-backed fix, parent reruns unchanged full list6 against original arrays, then the remaining75 cases. Diagnostic counts/sample protocol supplement and do not replace full acceptance. Done requires demonstrated dominant cost, correction tied to it, retained meaningful assertions, passing scoped checks and honest full list comparison. No extra broad suite or styling.

### M2-A layout-dominant branch: standalone shape-plan control

Trigger/evidence: reviewed test-only checkpoint5b58ff7d localizes >97% of the decomposed1,000-row cost to layout; invariant metric/batching variants are inadequate. No production fix. Planner source inspection found epaint0.36 shape_text uses `shaper.shape(buffer, ShapeOptions::new())`; locked HarfRust0.12 constructs ShapePlan when options.plan is absent. FontFace already caches ShaperData and FontsImpl pools buffers. Public reuse is `ShapeOptions::new().plan(Some(&plan))`; shape_with_plan is private. Font-face ownership is separate from ShapePlanKey matching.

Scope/ownership: one implementation writer creates only ignored `target/egui-modernization/m2a/shape-plan-diagnostic/{diagnostic.rs,inputs/,results/}` (plus executable/build receipts there). No production source, Cargo graph, registry writes or upstream patch. Use actual compiled `target/fast-dev/deps/libharfrust-c238bf01df6bc868.rlib`; HarfRust reexports FontRef, so no extra skrifa dependency is needed. Record compiler, source, rlib, input-font, input-run and executable hashes. Parent retains Git/full81 benchmarks; original baseline artifacts remain untouched.

Inputs: current Ubuntu-Light font bytes and representative primary-font Latin portions taken from fixture labels, including distinct Synthetic action indices and long-label Latin portions. Assert no missing glyphs. Clearly label these primary-font runs, not complete epaint fallback segmentation. Do not describe Japanese .notdef glyphs in Ubuntu as the actual fallback path. If genuinely needed, obtain exact synthetic strings/effective font/job metadata through a minimal ignored application exporter using existing setup/display helpers, outside timing, without retaining galleys; obtain parent source authorization first. No epaint archive is needed for this first control.

Protocol: disclosed diagnostic counts100/1,000 and small warmup/sample counts. Load files, FontRef, ShaperData and shaper outside batches. Recycle UnicodeBuffer via GlyphBuffer::clear; preserve boundary flags, guessed script/direction/language, features and default variation instance. Independently prepare equivalent variant states; do not contaminate one with the other's cache state. Compare implicit-plan shape, matching-plan shape and totals including initial plan construction/lookup. Measure ShapePlan construction separately. Compatibility uses the exact0.12 `ShapePlanKey::new(script,direction).language(...).instance(...).features(...)` API and face scope. Validate outside timing: every glyph ID, cluster, flag, advance and offset. Retain arrays/outliers, text lengths/properties and font hash. These controls supplement, never replace, full benchmark acceptance.

Build directly with verified stable rustc: `--edition=2024 -C opt-level=0 -C debuginfo=line-tables-only -C debug-assertions=yes -C overflow-checks=yes -L dependency=target/fast-dev/deps --extern harfrust=target/fast-dev/deps/libharfrust-c238bf01df6bc868.rlib <ignored diagnostic.rs> -o <ignored diagnostic.exe>`. Use checked absolute paths/workdir and verify matching compiler/rlib. Keep dependency artifacts unchanged; no profile manipulation or broad build.

Decision/done: production patch remains unauthorized. Reuse savings must plausibly explain a substantial part of727ms layout using explicit run counts; a merely positive small change is insufficient. If plan cost is material and complete shaping parity passes, next prepare narrowly instrumented actual epaint-run evidence (an ignored source archive may then be justified) before proposing bounded per-face production cache with correct keys/lifetime/eviction tests. If not material, retain evidence and investigate remaining shaping/glyph emission. No engine duplication, approximate geometry, full-catalog galley retention, shaping/hinting disablement or font swaps. Planner verified Ubuntu/Hack/NotoEmoji asset bytes match old/current; font-name equality alone does not identify fallback faces.

### M2-A warm-owner follow-up packet

Source-backed planner packet, queued after the standalone cold probe: add one ignored test-only100-row warm-frame diagnostic using the prepared warm state and existing run_root_list_frame_with_pass_count adapter. No production changes. Optional adapter clocks must preserve call order and original owner stopwatch boundaries; measure begin_root_frame_logic, render_root_ui accumulated across actual passes, finish_ui_frame_receipt, whole owner and residual egui/framework time. Validation and output disposal remain outside timing.

Snapshot existing Metric::LauncherRowsBuilt before/after: its timer spans geometry ensure plus visible result-widget construction. Assert zero warm rebuilds/measured rows, unchanged geometry/IDs, positive bounded painted rows and actual pass count. Emit paired durations, metric calls/work units, painted rows and passes with the established small diagnostic sample protocol. Existing Track C phases do not separately time root logic/finalizer. No duplicate production telemetry owner.

Decision: dominant rows timer means inspect visible result widgets/text; dominant remaining UI means inspect other root UI; dominant logic/finalizer means inspect that owner; large residual means inspect egui begin/end-pass work. Do not revisit the176ns mock-frame/guard probe or unsupported native-handle speculation. This supplements Small control and never replaces Full81 acceptance. Parent authorizes source ownership only after the current writer releases the cold probe; bounded independent review and affected scoped checks remain required.

### M2-A actual-layout attribution packet

Execute after the warm source writer releases and its accepted checkpoint is saved. One writer owns only ignored target/egui-modernization/m2a/actual-layout-probe/{root/,epaint/,receipts/}; parent owns live docs/Git. Create a ROOT archive of that accepted checkpoint and copy exact cached epaint0.36.2. Patch only the archived root manifest to archived epaint, preserving all existing vendor patches/features/profiles/dependency versions. Live checkout and registry remain unchanged. Offline-resolve archive lock once if needed, inspect that only intended source substitution occurred, then locked/offline commands. Record base/archive/source/lock hashes; preserve original/df27 binaries. Reuse root target cache serially where compatible.

Stage1: instrumentation-disabled archived control runs existing ignored track_c_diagnostic_root_list_geometry_cost_breakdown with100/1,000, unchanged setup/owner/assertions. Compare actual owner/phase costs with accepted diagnostics; resolve material provenance/build mismatch before attribution. No job-export/replay framework: preserve real nine-pass preparation/default fonts/Unicode fallback/galley eviction/eager geometry.

Stage2: archive-only opt-in aggregate observation: GalleyCache::layout_internal hit/miss,lookup/miss durations; layout_section/segment_into_runs segmentation duration and actual face run counts; shape_text plan construction versus shaping; layout_shaped_run glyph emission; FontFace::allocate_glyph cache/raster misses and miss duration; Fonts::begin_pass eviction/atlas recreation. No per-glyph clocks or hot-loop logging. A small archive snapshot API plus narrowly derived ignored diagnostic wraps existing preparation/owner/validation without duplicating them; emit aggregates after owner timing.

Plan timing branch explicitly builds the same per-call ShapePlan and supplies ShapeOptions::plan; **no reuse/cache**. Before relying on timings, compare implicit versus explicit shaping on actual runs, preserving flags/properties/features/variation and complete glyph arrays; collect detailed snapshots separately outside timing. Fresh equivalent states for stock disabled, stock observed and explicit-plan observed; report overhead and raw arrays, row/run/cache/raster/atlas counts and existing geometry receipts. Verify rather than assume warmed glyphs. No production patch authorized.

Independent review constraints: reset/enable aggregates immediately before measured owner and freeze/disable immediately afterward, before validation; exclude nine-pass preparation/eager-oracle work. Label every nested timing inclusive or exclusive: cache miss contains layout; section contains segmentation/shaping/emission; emission contains raster misses. Never add overlapping totals. Untimed parity uses fresh equivalent states and ordered actual face identity,text and shaping properties as well as complete glyph arrays. Reviewer accepted the archive approach with these constraints.

Aggregate observations are thread-local to the measured UI owner. Avoid global atomics/trace mutexes that can collect unrelated thread work or distort the probe; do not retain RefCell borrows across measured operations or nested observation callbacks. This is diagnostic scope isolation, not a production telemetry change.

Command shape: verified Cargo nextest run --manifest-path <archive>/root/Cargo.toml --locked --offline --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(track_c_diagnostic_root_list_geometry_cost_breakdown)' --success-output final; Stage2 exact derived test only. Perf enabled,QA feature/request absent,base+patch hashes/native exits/wall times persisted. No full81/grid/backend/application launch. Done: isolate actual cost among cache,plan,shaping,fallback/emission/raster with parity and bounded independent review; production correction requires separate scoped packet.

### M2-A next decision: matched release Small control, then bounded reuse

Stage2 accepted attribution:1,000 cold rows4,047 runs,plan construction333.9284ms/shaping218.6790ms;zero allocation-cache misses/atlas recreation. Warm cases30 galley hits/no shaping. No production patch. First perform canonical-release Small control of the existing root-list owner on original f45e377390284cc1ba6cf474fcd23e008514318e and accepted current source, serially. Original archive under a fresh ignored release-control directory; current live source may compile only while no live source writer is active. No manifest/profile edits; both existing Cargo.toml files use identical canonical release defaults (iteration is separate and is not used). Same100 actions,960x640,scale1000,five warmups/twenty samples,perf1/skipclipboard1,ML_TRACK_C_PROFILE=release,ML_TRACK_C_BENCH_MODE=small,actual SHA,no QA feature/request. Exact ignored track_c_benchmark_root_list_frame_geometry only. Preserve executable/source/lock/compiler hashes,commands,native exits/build and owner wall times,all arrays/outliers; join fixtures rather than owner labels. This supplements rather than replaces Full81 acceptance and cannot waive fast-dev failure or establish native latency.

After release control/review, next independently scoped prototype may reuse plans only in existing ignored epaint archive. Owner is FontFace beside ShaperData; mutable face access from layout_section, field-split immutable shaper/font data from mutable owned plan storage; release face borrow before layout_shaped_run. No unsafe/global tracker/second engine/text or galley cache. Explicit bound eight plans per face with simple replacement. Use public ShapePlanKey with actual script,direction,language,instance,features and matches; actual shaping still uses per-run instance. Face replacement drops plans naturally. No manual approximate keys or borrowed addresses.

Prototype verification: fresh equivalent stock/reuse100/1,000 cold/warm owners,complete ordered run/glyph and eager geometry parity,hits/misses/builds,max entries/run counts,lookup/replacement overhead; incompatible keys,face isolation,eviction/font replacement,valid available variation-instance fixture. Keep unavailable fixtures explicit rather than invent coordinates. No Full81 repeat or production vendoring until actual material savings,bounded ownership and parity are demonstrated and reviewed. Remaining cold shaping and warm UI costs remain visible; theoretical45% plan share is not acceptance.

### M2-A queued warm-row attribution after bounded plan reuse

Read-only planner inspection identifies changed visible-row paths: original SelectableLabel directly lays out/allocates/paints; current Button::selectable traverses AtomLayout, previous-response lookup, class/style resolution, Frame preparation and an owned text copy for widget information. Row scopes use UiBuilder. These are source facts, not measured causes. Accepted warm cases have no layout misses/shaping; avoid repeating cold attribution.

Next smallest step, sequentially after the active archive prototype: subdivide the existing100-row warm LauncherRowsBuilt observation on the actual row loop. Test-only opt-in aggregates for action/display-text preparation, row Ui/scope construction, existing row_ui.add(Button::selectable(...)), and remaining tooltip/context-menu/response work. Retain owner/UI/RowsBuilt totals, stage counts/sums and instrumentation-off/on controls; no per-row logging or clocks in ordinary builds. Preserve visible indices,selection,IDs,rectangles,interaction sense,primitive output and accessibility assertions; require zero geometry rebuilds/measured rows/shaping. Source/API placement must be confirmed by the assigned implementer before editing.

Independent review: scope/with_layout timing can enclose Button timing. Make stages disjoint or explicitly label scope inclusive and never add overlapping values; retain a RowsBuilt remainder for uninstrumented loop/observer work. Bind zero-shaping proof to the existing archived epaint observer if executed there; live cache counters alone do not directly prove it. No new observation framework.

Use the resulting dominant boundary for a focused correction. No legacy hand-painted widget substitution, accessibility removal, retained catalog galleys or styling changes to obtain speed. This packet is queued planning only; no source edits/builds/tests yet and no authorization for overlapping writers.

### M2-A queued production integration: bounded HarfRust plan reuse

Objective: integrate proven per-face plan reuse as a disclosed epaint0.36.2 vendor patch, preserving shaping output/font lifecycle. Partial correction only; no M2-A performance acceptance. Start from stock cached released epaint, never copy the instrumented archive wholesale. Port only capacity8, matching/FIFO helpers, FontFace-owned initially empty Vec<ShapePlan>, field-split shaping method and actual layout caller migration. Public ShapePlanKey uses actual script,direction,language,features,variation instance; misses construct the same plan, oldest evicted at capacity, hits do not reorder. Plans own data/drop with face; no global tracker,text/galley cache,unsafe or app-facing cache API.

Files: new vendor/epaint with released licenses/assets/version/dependency requirements; src/text/font.rs owns cache/shaping; src/text/text_layout.rs gets selected face mutably and releases borrow before glyph emission. Preserve buffer reuse,segmentation,flags,variation ordering/default options. Production method returns GlyphBuffer with no diagnostic arguments/results/modes. Root Cargo.toml adds path patch; Cargo.lock must change only epaint source/checksum with same0.36.2 dependency versions,features and no duplicates. Vendor provenance records full upstream checksum from accepted lock/cache,source/license origin,exact production diff,prototype manifest/evidence,rationale/removal condition. Do not invent checksum from abbreviated planner notes.

Exclude layout_diagnostics.rs,modified fonts.rs,diagnostic reexports/clocks/counters/modes/snapshots/face IDs/root test hooks. One production cache-backed shaping path. Tests remain private cfg(test) at owner: all key components as library specifies,face isolation,capacity/FIFO,recreated face empty,complete implicit/reuse glyph parity for multilingual/default-font runs. Explicit host test uses verified Bahnschrift hash and supported weights for both instance compatibility and full glyph parity; prototype proved compatibility only. No unconditional Windows-font dependency or bundled system asset.

Standalone upstream tests previously resolve uncached benchmark dev-dependencies offline. Do not add shared production test hooks. If necessary use ignored test manifest targeting actual vendored src/lib.rs,mirroring root-locked production dependencies and only genuinely required unit-test dependencies. Independent review confirms upstream fonts.rs cfg(test) requires similar-asserts2 absent from root lock/cache; exact filters still compile it. Omit benchmark-only criterion/mimalloc, but separately resolve this genuine test-only dependency in the ignored manifest, preserve/freeze its lock and provenance, then test offline. Scoped fetch is permitted if required; production/root dependency graph stays unchanged. Do not stub the dependency or suppress upstream tests. Record actual unavailable dependencies/fetch restrictions; never silently change production graph/weaken assertions.

Verification: focused vendor cache/parity tests; root all-target check; directly relevant existing list/grid cache/invalidation/font-activation tests; Notes eager/width transition and track_c2f_oracle_quick_notes_recipe_contract; scoped100/1,000 actual owner diagnostic with QA absent verifies geometry/bound/savings; inspect diff for excluded experimental code. Narrow exact filters chosen from current source. No Full81 yet. Prototype accepted735.5561→385.8893ms cold1,000-row median; remaining shaping211ms/segment27/emission61 and warm gap unresolved. Current writer owns archive warm attribution; this packet is independently reviewed/implementer-ready after the test-only dependency amendment, queued until archive writer release, not concurrent implementation authorization.

### M2-A bounded caller extension: inherit row style at construction

Planner/reviewer source inspection accepts a routine caller correction: in src/gui/render.rs::LauncherApp::render_root_ui pass ui.style().clone() via existing row UiBuilder::style and remove the immediate row_ui.set_style of that same Arc. Ui::new otherwise fetches ctx.global_style,which is immediately overwritten. Pre-return Placer/UiStack/widget/AccessKit container registration does not read Style; clipping before the old overwrite is likewise style-independent. Preserve row ID,with_layout,automatic Button ID,clipping,disabled order,response handling and geometry/text. Do not remove nested layout scopes. Existing eager geometry/viewport/identity/local-style coverage plus matched warm-owner check are proportionate; no extra prototype/egui vendor patch or trivial API test. This targets redundant setup work,not the entire measured0.2905ms setup or0.6879ms Button cost; no saving claimed. Parent explicitly assigned this extension to the same sole live writer before first source freeze/build.

The earlier Button eager label copy is real but not safely deferred as a one-line change: AtomLayout::allocate consumes the text source before response.widget_info; the latter is already correctly lazy for events/AccessKit/debug inspector. No new egui patch or label-lifetime redesign is justified by source alone.

Additional bounded read-only check rules out a parent style-cache reuse hypothesis: released0.36.2 Ui/Style/AtomLayout have no reusable UiStyleCache to inherit; style values/layout atoms are recomputed,text caching remains font/context-owned. Ui::new_child changes automatic-ID seed,parent counter,stack/AccessKit ancestry,sizing/invisibility and registration,with no identified reusable cost benefit or public seed override. Keep current constructor/nested layout. Source inspection is not warm performance evidence.

### M2-A residual cold cost: reviewed archive-only attribution

Current activation: after accepted production checkpoint87e08b32 and writer release, parent explicitly assigned this bounded archive-only packet to sole implementer_m2a. Live production source remains frozen. After accepted plan reuse, recorded shaping remains211.552ms/1,000 rows. Planner inspected cached HarfRust0.12: Shaper::shape_with_plan constructs Scale/FontFuncsDispatch inside the shaping timer, then calls OtShapeContext::shape_internal; Shaper construction is outside that timer. ShaperData already retains tables/cmap,nominal mapping uses cached cmap,horizontal advances use batched metrics,ASCII properties and map_glyphs_fast already have fast paths. No supported switch bypasses normalization/GSUB/GPOS with general output parity. BuiltinFontFuncs lazily initializes per-call charmap/metrics, but source alone does not establish material cost; custom FontFuncs would replace an existing cached/batched path and is unjustified now.

If further cold attribution is needed after active production integration, smallest proposed archive-only measurement is coarse retained-plan timings around FontFuncsDispatch/lazy metric initialization,substitute_default (normalization/masks/mapping),substitute_plan (GSUB/AAT),position (advances/GPOS/fallback),and explicit residual. Same1,000-row owner,warmed glyphs,unchanged profile/fonts,complete existing ordered-glyph/eager parity,disabled observation control,no per-glyph clocks/custom callbacks/cache/full-series rerun. Instrumentation ownership/provenance must be scoped/reviewed before execution; no additional source writer authorized by this paragraph.

Planner implementer-ready refinement (independently reviewed/implementer-ready, held pending production writer release): copy cached HarfRust0.12.0 to actual-layout-probe/harfrust/harfrust-0.12.0 with original manifest/license/full source hashes; archive root patch only,version/dependencies unchanged. An archive-only diagnostic feature enabled through archived epaint's existing HarfRust dependency exposes thread-local aggregate reset/enable/freeze; bridge through existing epaint layout_diagnostics,not a new root dependency. Freeze root/epaint starting archive including current plan/style state before edits. Both observers reset immediately before existing owner and freeze directly afterward before validation/output disposal; preparation excluded. HarfRust clocks switch off/on while epaint stays ReusePlan for both.

Exact functions: src/hb/face.rs::Shaper::shape_with_plan complete call,count and separate Scale::new+FontFuncsDispatch::new setup; src/hb/ot_shape.rs::OtShapeContext::{substitute_default,substitute_plan,position} coarse inclusive phases. These four intervals are disjoint; complete-call remainder includes entry/exit,Unicode properties/clustering/pre/postprocessing,surrounding setup and observer overhead. src/hb/font_funcs.rs::BuiltinFontFuncs::{charmap,glyph_metrics} clocks go only inside actual OnceCell::get_or_init initializer closures,distinguishing FontRef/FontInstance; no per-accessor/glyph clock/counter. Lazy-init times nest within coarse phases and are never added/subtracted again. Keep shaping statements/order unchanged,local per-call timing where practical,no atomics/framework/callbacks/timed run history.

One derived ignored test track_c_diagnostic_root_list_residual_shaping_attribution: only1,000 cold rows,two warmups/five samples per HarfRust off/on mode,fresh equal real nine-pass preparation,unchanged existing owner timer. Full untimed ordered run/glyph and eager parity also compares retained-plan clocks off/on; no snapshots in timed samples. Preserve all ten records/phase arrays/arithmetic/preparation/source/binary/command/environment receipts; compare owner/epaint-shape off/on overhead. Prior4047 runs/103830 glyphs are controls to verify,never forced values. Exact filtered locked/offline fast-dev Nextest with existing perf/skipclipboard/source/profile attribution,QA feature/request absent and explicit existing target settings. No app/full81/unrelated/upstream suite. Done means bounded passing owner/parity/provenance,clear partition/nested-labels and a concrete responsible phase or honest no-dominant-candidate result; it never automatically authorizes production optimization.

## M2-B concrete Windows native parity handoff (read-only planning)

Objective: validate migrated native windows, activation, input and overlays; correct demonstrated migration defects at their existing owners. Execute after M2-A, with M1-C compilation and M1-B lifecycle tests accepted. Keep explicit Glow; backend comparison belongs to M2-C. Confirm completed M1-C viewport callback signatures before editing. This packet is planning evidence, not executed acceptance.

### Ownership and invariants

| Behavior | Existing owner |
| --- | --- |
| Root creation | `src/main.rs::build_viewport_with_icon`, `spawn_gui`: configured size/minimum size/icon/topmost, explicit Glow, Windows `with_any_thread(true)` |
| HWND lifetime | `src/visibility.rs::RootWindowBridge`: synchronized handle/generation, `capture_frame`, `clear`, process/window revalidation; render owner captures frame handle and clears on exit |
| Visibility and focus | `RootViewportCtx`, `apply_visibility_with_focus_intent`, ordered toggle/visibility owners; `src/window_activation.rs::{activate_window_if_current, WindowActivationFence}` |
| Parking and capture | `src/launcher_parking.rs`: signed physical geometry, `compute_capture_safe_parking_position`, transaction/snapshot/restore; visibility owns logical conversion |
| Secondary viewports | QR and Regex immediate viewports; `ParameterPromptUi` immediate/bootstrap; Radial Designer deferred viewport and identity capture/clear; Screen Draw independent immediate toolbar |
| Invocation | `src/hotkey/launcher_invocation.rs::{LauncherInvocationAdapter, LauncherInvocationService}`, `src/hotkey/runtime.rs`, main visibility dispatch |
| Native overlays | `src/radial/native.rs`; `src/screen_draw/{native_overlay,native_canvas,window_layers,geometry}.rs`; `src/mouse_gestures/overlay.rs`; `src/gui/mkmacro_dialog/{visual_overlay,visual_overlay_windows}.rs` |
| Screenshot integration | `src/actions/screenshot.rs`, `src/plugins/screenshot.rs::launch_editor`; Screen Draw capture parking boundary |

Windows root presentation deliberately uses `SW_SHOWNOACTIVATE`; ordered native activation owns focus. `RootViewportCtx` rejects unowned framework Focus. Hiding parks the root offscreen while retaining drawability. Preserve these decisions, generation fences, release/provenance ownership and overlay policies. Do not borrow HWND across threads or introduce stale-handle fallbacks, arbitrary delays, modifier compensation, duplicate hooks or focus workarounds.

Inspection found no tray implementation or root `with_transparent` configuration. Record tray as absent and root opacity as existing behavior; do not add features to satisfy generic checklist wording. Static inspection identified no justified new windowing abstraction.

### Required execution and focused tests

1. Confirm root handle capture/clear, secondary identity/close/reopen and designer identity invalidation still traverse existing owners. Hidden child bootstrap must work with a parked root.
2. Run exact Nextest `test(...)` filters for the existing contracts below. State tests do not prove OS behavior. If these paths change, relevant injected-event integration targets are `gui_visibility`, `focus_visibility`, `trigger_visibility`, `hotkey_events`, `quit_hotkey`; do not label them real-hook evidence.

| Area | Existing test names |
| --- | --- |
| Visibility | `windows_root_presentation_lowers_to_owned_activation_without_framework_focus`; `windows_root_refuses_unowned_framework_focus_without_false_focus_receipt`; `windows_root_activation_from_child_context_targets_root_and_preserves_child_focus`; `simulated_root_wake_requires_requalification_after_replacement_clear_and_hwnd_reuse`; `parking_moves_a_configured_point_outside_the_virtual_desktop`; `hidden_grid_keeps_the_root_drawable_for_radial_preparation` |
| Invocation | `exact_shift_alt_win_end_chord_uses_fake_time_for_tap_and_hold`; `modifier_repeat_does_not_leave_a_phantom_modifier_after_release`; `post_action_trigger_release_acknowledges_once_and_never_reopens` |
| Geometry/layering | `native_rect_and_logical_fallback_conversion_are_checked_signed_and_dpi_aware`; `stale_handle_rearms_bounded_resolution_and_recreation_gets_fresh_identity`; `passive_overlay_remains_topmost_layered_noactivate_and_click_through`; `signed_coordinates_support_negative_monitors`; `native_surface_conversions_are_scale_consistent` |
| Secondary fallback | `qr_viewport_has_scannable_size_and_embedded_escape_cleans_up`; `regex_tester_embedded_shell_focuses_once_and_escape_closes` |

3. Perform one bounded native checklist on the isolated fixture:

- Root show/hide/reopen from background and while focused, placement/topmost, usable query input after activation.
- QR, Regex, parameter prompt, Designer and toolbar creation, intended focus, Escape/X close, reopen, stable owner identity, no unintended root activation. Include prompt creation while root is parked.
- Screen Draw capture/toolbar restore, radial invocation, region selection, one passive crosshair/gesture overlay: layering, click-through versus interactive regions, cancellation and restoration.
- Real keyboard/mouse Shift+Alt+Win+End tap versus hold, release after each, repeat while root/grid focused, ordinary typing afterward to expose sticky modifiers or missed key-up.
- Available monitor/DPI transitions: negative origins, placement/pointer alignment, 100/125/150/200% where available; toolbar/selection coordinates and parking outside the complete virtual desktop.

4. Fix only reproduced failures at the first responsible owner; add focused regression coverage for corrected invariants.

### Acceptance evidence and genuine gaps

Keep separate receipts for automated state/oracles; native HWND identity/generation/bounds/focus/visibility; real input; actual desktop composition/screenshots; hardware/OS availability. Earlier original Sky desktop capture attempts failed. Accessibility-only query/Enter proves that interaction only,not visual/DPI/overlay/GPU parity. Update2026-10-13: M1-D now has accepted renderer-owned ROOT and actual Immediate QR child PNGs, including QR Escape/ROOT-query-focus return; see visual-qa.md and qa-secondary-framebuffer-plan.md. Reuse these scoped owners/evidence; do not create duplicate screenshot infrastructure. Framebuffer pixels still cannot prove desktop composition,click-through or OS focus; native receipt/UIA observations are distinct evidence. QA builds/requests must remain absent from performance runs.

The applied computer-use skill prohibits synthetic Windows-key input; exact chord acceptance needs human/manual evidence. Report mixed-DPI, negative-origin topology, Win11 and remote-session coverage according to actual availability. Mark unsupported cases unavailable and required unobserved behavior pending, never passed. Finish when focused tests pass, available native checks have actual evidence, reproduced defects are resolved and environmental gaps are explicit. Planner performed no edits/builds/tests/native actions.

## M2-C concrete renderer comparison handoff —read-only planning2026-10-13

Execute after M2-A/M2-B acceptance. Objective: choose the fastest compatible measured backend on the available host, preserve native behavior and explicit selection, no styling/cache redesign. Contract requires at least five matched runs per native scenario,prefer ten when variance is substantial.

Verified current state: Cargo enables eframe0.36.2 Glow with defaults off; main.rs::spawn_gui explicitly selects Renderer::Glow. Root lock already contains egui-wgpu0.36.2/wgpu30.0.0; cached30.0.1 must not replace locked30.0.0. Eframe exposes wgpu and wgpu_no_default_features; forwarding eframe/wgpu enables its existing backend without a new direct dependency. Default Wgpu capabilities include DX12/Vulkan/GLES; actual selected backend/adapter must be recorded. Planner found no application GL paint callbacks or direct GL resource consumers. Vendored Wgpu already shares should_show_ui/needs_root_ui_when_hidden/update_logic_only policy; retain that owner.

Required source changes:

1. Add one opt-in application feature forwarding eframe/wgpu, retaining default Glow features/selection until measured decision.
2. Add one-time spawn_gui comparison selection for explicit Glow/Wgpu, rejecting unknown/unavailable values without silent substitution. No persisted setting or per-frame env reads.
3. Build separate release artifacts with matched source/profile/settings: default Glow and opt-in Wgpu. Where useful, compare runtime choices from the dual-enabled binary to separate renderer cost from build-feature footprint. Preserve original release at target/egui-modernization/baseline/bin/original-release.exe and its receipt; never overwrite it.
4. Bind selected renderer and actual adapter once at creation. Glow Painter::new logs GL version/renderer/vendor; Wgpu logs adapter info and CreationContext::wgpu_render_state.adapter.get_info() exposes backend/device/driver. Ensure these reach run logs; if needed use one opt-in startup receipt through existing performance owner, not continuous instrumentation.

Existing performance evidence and limits: src/performance.rs owns startup phases/process elapsed/metric snapshots/runtime.frames; record_frame runs on ROOT UI's first pass. startup.first_usable_frame currently coincides with startup.first_update, so treat that legacy label as CPU update,not displayed/usable-frame proof. IntegrationInfo::cpu_usage accounts UI/render CPU excluding reported vsync wait; Glow/Wgpu accounting differs and is not GPU execution time. No established app metric proves GPU completion,compositor presentation,display latency or GPU memory. QA screenshots/UIA/headless81 cases cannot supply these; do not invent them or rerun headless81 merely to compare renderers.

Parent measurement ownership: serial isolated fixtures/owned processes with explicit Normal window style,actual executable/source/build/settings/fixture hashes. dev/run_owned_process.ps1 supplies containment but defaults Hidden; QA launch helper is provenance reference,not a performance runner. Omit qa-framebuffer-capture and unset its request env for every measured build. Compare preserved original Glow release,migrated Glow release,migrated Wgpu release; identical counts/query/layout/viewport/logging/warmup/observation/launch conditions,alternate backend order,retain every run/outlier. Never time UI automation round trips as app latency.

At least five runs each: startup CPU phases/process-to-first-update; visible and logically hidden idle with fixed warmup/observation and owned PID TotalProcessorTime delta/wall time/working set; ROOT UI frame activity separately from hidden logic where actual counters are available; fixed query/list/grid/Notes interactions through supported routes and existing CPU receipts where emitted; independent QR open/close and retained M2-B compatibility. Attribute binary size/build time/cache state separately. Include telemetry-off process samples to expose instrumentation effects. Any extra renderer CPU samples must be bounded,opt-in and placed at existing frame/receipt owners; no forced repaint loop,GPU wait or general benchmark framework.

Focused verification: default and Wgpu-enabled all-target checks; small selector default/explicit/rejected-unavailable tests; existing relevant lifecycle tests for actual corrections; native ROOT/QR creation/close/reopen,parking/activation and relevant overlays under both backends. No unrelated full suite or synthetic Windows-key chord. Actual physical tap/hold/key-up remains manual.

Done: retain comparable samples/p50/p95,investigate meaningful reproducible regressions,select fastest measured compatible backend with ties/uncertainty explicit,commit decision with renderer explicit. Hardware preparation recheck is recorded in benchmark-results.md; controller properties do not prove actual GL adapter,DPI/topology. Record unsupported mixed-DPI,Win11,remote-session,GPU/display measurements honestly. This packet is source-backed planning only; no backend feature/selector/build/measurement has run.

## M1-D required DDS capability correction

The failing image parity test exposed a concrete upstream admission regression. Both image0.24.9 and0.25.6 hardcode DDS capability reporting false despite their feature-gated DdsDecoder dispatch. Extras0.27 accepts non-SVG URI/MIME and delegates decode; extras0.36 gates file extensions and MIME using reading_enabled, rejecting DDS before the existing decoder. Planner verified the latest released image0.25.10 still has this mismatch; upgrading does not resolve it.

Approved bounded fix: vendor currently locked image0.25.6 with provenance/checksum/licenses and removal condition. At ImageFormat ownership, set DDS reading_enabled to cfg!(feature="dds") and feature-independent can_read to true; keep writing false and decoder unchanged. Update relevant capability assertions. Root Cargo patch must resolve only image0.25, retaining image0.24 application consumers. Do not duplicate loaders or bypass extras gates.

Verify DDS enabled/disabled reporting with focused package unit filters. Add valid tiny DDS bytes to the existing asynchronous extras loader coverage for both file://*.dds extension and image/vnd-ms.dds MIME, plus malformed-input rejection. Confirm graph/feature receipts use the patched image0.25. Batch this correction with the source-backed toast anchor assertion correction before another expensive application test build. Existing failed/interrupted receipts remain classified accurately.

Toast source clarification: RIGHT_TOP anchors area right/top at screen right/top then adds offset; existing positive [10,10] extends beyond right edge and default Area constraint clamps right to viewport edge. The800x600 controlled fixture therefore expects right800/top10, not right790. Preserve production offset/stacking/lifetime behavior; do not change appearance to fit the test.

## M2-A isolated released text-stack candidate — planning packet

Objective: determine whether released HarfRust0.13.3 reduces the retained-plan actual1,000-cold-row owner before extending positioning instrumentation. This is candidate evidence only; production adoption and M2-A acceptance remain separate reviewed decisions.

Compatibility owner: epaint passes Skrifa FontRef directly to HarfRust. Candidate therefore aligns HarfRust0.13.3 with Skrifa0.46.2 and read-fonts0.43.3; preserve font-types0.12.6/once_cell1.21.4 where bounds permit, Skrifa std/autohint_shaping and HarfRust existing std, optional ICU disabled. Planner verified released manifests and used buffer/builder/instance/options/plan-key APIs remain available; mechanical compiler adjustments belong to writer. HarfRust0.14.0 is newer stable but its buffer/shaper API rewrite is outside scope.

Upstream fixes438/447 target lookup dispatch and borrowed shaping temporaries;437 concerns coverage/contextual rules;446 caches metadata mostly outside recorded shape interval but inside owner. Their benefit on this fixture is unknown. Tagged0.13.3 still reparses PairPosFormat2, so issue256 is not a demonstrated fix. A measured released upgrade is preferable to several dependent private backports if worthwhile; no backports or new positioning cache automatically authorized.

One implementer owns a fresh ignored archive candidate; preserve accepted actual-layout archive, its receipts/binaries and all live production files. Freeze source/lock/binary provenance, obtain exact released package checksums/licenses, resolve only aligned text stack, inspect graph/features/diff before build. Use absolute scope/workdir and explicit target path. Parent owns tracked docs/Git. No production changes,profile changes,fixture trimming,0.14 migration,native/Full81/upstream/unrelated suites.

Reuse existing epaint observer/ReusePlan machinery and identical owner/timer envelopes in both versions. Disable HarfRust internal clocks; do not port/extend those diagnostics. Preserve fixture/default-font bytes,segmentation,flags,variations,FIFO and preparation. Persist complete canonical ordered run/glyph snapshots across0.12/0.13.3,omitting only documented process-local face keys. Hashes alone insufficient. Compare snapshots and eager geometry before timing; report any output correction/difference rather than silently accepting it. Snapshot acquisition remains outside timing.

Matched timing: only1,000 cold rows,fresh states,existing nine-pass preparation outside timer,two warmups/five samples per version. Retain all raw owner/epaint-shape arrays,plan/work counts,output identities,source/lock/executable hashes,exact command/env/exit/wall receipts. Verify counts against observed controls,never force expected values. Use exact ignored test/owner and locked fast-dev settings,QA requests absent. Build only single necessary libtest target; initial bounded fetch allowed for exact new packages,thereafter locked/offline. Preserve failed attempts and actual cache/source attribution.

Done: compatible compiled candidate,scoped graph/features,complete cross-version glyph/eager parity and matched raw timings establish benefit or lack of benefit. Review evidence before any live adoption. If unsuitable,return to a bounded responsible-position investigation or separately reviewed narrow upstream438/447 backport; no automatic fallback implementation. Preimplementation reviewer may refine canonical serialization/mode matching before writer activation.

Primary planning sources: https://github.com/harfbuzz/harfrust/blob/0.13.3/Cargo.toml ; https://index.crates.io/sk/ri/skrifa ; https://index.crates.io/re/ad/read-fonts ; https://github.com/harfbuzz/harfrust/blob/main/CHANGELOG.md ; https://github.com/harfbuzz/harfrust/pull/438.patch ; https://github.com/harfbuzz/harfrust/pull/447.patch ; https://github.com/harfbuzz/harfrust/blob/0.13.3/harfrust/src/hb/ot/lookup.rs ; https://github.com/harfbuzz/harfrust/issues/256 . Read-only planner inspection; no measured candidate improvement yet.
Preimplementation reviewer accepted with two required refinements: compare complete actual ROOT geometry/output identity ACROSS versions before timing,retaining vectors/extents/viewport to diagnose any mismatch; same-version eager checks alone are insufficient because Skrifa may alter metrics. Keep raw font bytes/index/tweaks/styles identical and never normalize numeric glyph/geometry differences away. Explicitly remove/disable residual HarfRust timing feature AND epaint bridge/default-font activation in BOTH baseline and candidate copies,not merely clocks-off. HarfRust's required once_cell dependency closure is permitted; unrelated upgrades/features are excluded. Fetch/compile/API compatibility remains unverified until execution. Packet now approved for one isolated archive writer; production adoption remains separate.

Read-only follow-up (no cache authorization): identical-run raw shaping reuse is correct only within unchanged font face/engine and all effective inputs. ShapePlanKey instance is GSUB/GPOS feature-variation selection,not normalized coordinates; different coordinates may share plans but differ glyph positioning. Existing trace lacks explicit normalized instance coords/presence,typed variation bits,font collection index and fixed scale/point-size/font-function/buffer policy declarations. Fixture-specific duplicate-input analysis may use independently verified absent instances and default configurations; canonical traces omitting process-local face keys must not merge collection faces by asset hash alone. Current path has empty features/default scale/point size/custom functions,empty pre/postcontext/defaultcluster policy,raw font-unit outputs. Font size/DPI/rasterization are downstream. This defines a correctness boundary only; no shape-result cache/probe or production implementation authorized.

## M2-A partial production adoption of the reviewed released text stack

Objective: adopt the accepted aligned release stack as a partial correction; M2-A remains incomplete. Evidence checkpoint a36547bf preserves the isolated full glyph/exact geometry parity and five-sample cold timing comparison. One sequential writer owns vendor/epaint/Cargo.toml, root Cargo.lock, vendor/epaint/PATCH.md and a fresh ignored private harness/verification directory. No Rust source edits are expected. Parent owns other tracked docs/Git.

Change HarfRust bound 0.12.0 to0.13.3 and Skrifa0.44.0 to0.46.2, preserving all features/default features. Resolve offline and require the exact accepted read-fonts0.43.3 alignment. Match the minimal graph closure in released-text-stack-candidate/receipts/source-graph-freeze-001.txt: Hrf0.12 disappears; Hrf0.13.3/Skrifa0.46.2/read-fonts0.43.3 enter; font-types0.12.6/once_cell1.21.4 remain. Existing glifo→vello_cpu retains Skrifa0.44/read-fonts0.41; do not upgrade unrelated consumers to remove duplication.

Update PATCH.md with released package checksums/provenance, candidate evidence/limitations, verified plan-key semantics and both local differences (FIFO plan reuse plus dependency bounds). Removal requires compatible upstream epaint with needed behavior/dependencies and focused parity. Preserve stock Cargo.toml.orig/vendor Cargo.lock as package provenance; root lock owns this build.

Copy existing ignored production-plan-reuse-validation/epaint-focused-003/Cargo.toml and its lock into a fresh private harness. Align exact Hrf/Skrifa pins with new root resolution, preserve its live vendor/epaint/src/lib.rs target, existing features and genuine test-only similar-asserts2. Preserve older receipts/lock; do not add inactive optional packages. Freeze source/graph/harness locks and binary identities, with explicit paths/shared-target cache attribution.

Invariants: FIFO8 per-face ownership, public ShapePlanKey matching, actual per-run instance/flags/segmentation/buffer reuse and complete glyph output. Font bytes, profiles, renderer and root rows unchanged. No production diagnostics/clocks/modes/test hooks, shaping-result cache, HarfRust vendoring,0.14 migration, Full81/release/native run.

Required verification:
- Four existing private tests via explicit OR-filter and aligned manifest,locked/offline fast-dev Nextest: shape_plan_key_checks_script_direction_language_and_features; plans_are_face_scoped_fifo_bounded_and_recreated_with_the_face; multilingual_default_font_runs_match_implicit_glyph_output; variable_font_supported_weights_match_implicit_glyph_output. Bahnschrift wght300/700 must actually execute; unavailable return is not acceptance.
- cargo check --all-targets --locked --offline.
- Exact20-case OR-filter from production-plan-reuse-validation/receipts/root-focused-existing-002.receipt.txt; require20 executed cases. Covers list/grid/font activation/Notes/query/output texture disposal.
- Existing ignored track_c_diagnostic_root_list_geometry_cost_breakdown via locked/offline fast-dev lib Nextest,100/1000 cold/warm,two warmups/five samples per cell. Keep all20 records,unchanged owner/oracles/source/profile attribution and QA env exclusions.
- Root/vendor formatting,diff checks,dependency-feature and independent source/result review. Verify unchanged Rust/font assets by hashes. Preserve actual commands/native exits/walls,all failures and source/lock/binary receipts.

Done: required checks pass,minimal graph matches,production remains diagnostic-free,provenance accurate. Commit as partial adoption only. Isolated cold owner390.704→370.182ms does not establish original/warm/release/native parity or pass M2-A. Genuine remaining uncertainties are live build and actual private variable-font execution; routine compiler corrections remain within packet,architectural source changes require reporting to parent before expansion. Planner source inspection complete; writer may execute this bounded packet.

## M2-A next archive-only raw shaping reuse prototype — NOT activated

Hold until released-stack production adoption completes, its source writer releases and checkpoint is saved. This is planning only; no production cache authorized. Accepted baseline trace has3,046 repeated executions/76,260 repeated glyphs among4,047 runs,with identical outputs; these counts are not measured savings.

Objective: measure whether eliminating repeated identical raw shaping reduces the actual1,000-row cold owner. Use a fresh ignored copy of the accepted0.13.3/Skrifa0.46.2 candidate, preserving accepted source/receipts first. FontFace owns reuse state; text_layout owns emission and buffer recycling. Scope only archived epaint font.rs (FontFace/init/shape_for_layout/private tests), text_layout.rs (layout_section/layout_shaped_run/trace slices), existing layout_diagnostics.rs, and one narrowly derived archived root owner/parity diagnostic. No new dependencies,HarfRust patch,raster/galley cache,production files,profile/fixture/font changes,Full81/release/native.

Flow:
1. Initially empty face-owned storage; admit only actual ShaperInstance=None. Every Some(instance),even default coordinates,bypasses unchanged. Never use feature-variation-only ShapePlanKey as result identity.
2. Preserve flags,push_str,property guessing,shaper construction and existing plan selection. Initial lookup immediately before shaper.shape isolates eliminated shaping. Key exact text,resolved script/direction/language,buffer flags and this face. Explicit fixed contract: empty features,no scale/point-size/custom functions,default cluster/replacement policies,empty pre/post context,fresh run-relative clusters. Bypass unsupported buffer policies using actual available APIs/ownership guarantees; report any inaccessible property rather than inventing a getter.
3. Miss runs retained-plan shaping,stores exact complete GlyphInfo/GlyphPosition slices. Hit returns an owned shared handle to arrays plus incoming UnicodeBuffer. No face borrow escapes into glyph emission.
4. Private result carrier has actual GlyphBuffer or stored arrays+UnicodeBuffer; exposes read-only slices and consumes itself to recycle. Miss/bypass calls existing GlyphBuffer::clear; hit clears retained UnicodeBuffer. Do not manufacture private GlyphBuffer state or allocate a replacement buffer on hits.
5. layout_shaped_run reads those slices; scaling,fallback,continuation glyphs,spacing,raster allocation and ordering unchanged. Archive trace observes identical slices on hits/misses. A recreated/replaced face starts empty; no external generation tracker.

Fixed prototype policy (no tuning): at most128 entries per face,LRU; at most1MiB retained key+glyph payload per face; at most4KiB input text and64KiB combined key/glyph payload per admitted entry. Exact-length owned payloads and checked byte accounting before admission. Oversize/overflow bypass without evicting existing entries. Hits update recency without copying arrays; evict until entry and payload limits permit insertion. Metadata additionally bounded by128 entries,reported separately from payload (not claimed total RSS). Returned shared handles may temporarily retain evicted payload; report that lifetime,do not retain beyond run emission.

Focused private tests against archived epaint through copied existing private harness: miss→hit versus forced reference complete glyphs with multilingual/cluster/flag cases; hit→miss and miss→hit recycling/no stale properties/allocation reuse; key text/flags/properties separation,face isolation/recreation; entry/byte bounds,LRU,oversize bypass/accounting; actual variable-instance bypass and existing Bahnschrift extrema parity. No shared production test hooks.

Matched experiment: derive one ignored track_c_diagnostic_root_list_raw_shaping_reuse. Same released stack,binary,plan reuse and observer envelope off/on;1,000 cold rows,fresh states,two warmups/five samples per mode. Raw-result reuse DISABLED during identical real nine-pass preparation in both modes; enable on only immediately before owner,starting empty. Reset immediately before existing owner; freeze immediately afterward. Snapshot validation/disposal outside timer. Before timing require full ordered off/on glyph and exact cached/eager geometry snapshots,retaining accepted canonical reference comparison. Preserve all ten raw owner arrays,requests/hits/misses,actual shape calls/glyphs,bypass reasons,evictions,entries,payload high-water marks. Do not force3,046 trace repeats to equal observed hits.

Use locked/offline fast-dev lib Nextest with exact ignored-owner filter and original performance/source/profile/QA exclusions. Run only focused private contracts and this owner; preserve source/lock/binary provenance and previous evidence. Done requires tests,bounds/recycling/full output parity and raw timing/storage/copy overhead evidence. Independent review required before any production decision. Actual benefit and exact low-level buffer-policy access remain to verify; ordinary equivalent ownership details belong to implementer. Preimplementation reviewer must close genuine policy/key/bounds concerns before activation.

Raw-result prototype preimplementation review: accepted subject to the following concrete policy refinement. HarfRust0.13.3 has getters for cluster level/flags/script/direction/language and is_empty, but no getters for pre/post context or not_found_variation_selector. Admission must therefore be bound to the existing layout-owned proven fresh/cleared lifecycle (FontsImpl take/return → layout_section → consuming carrier.clear),or unknown-policy buffers bypass reuse. Do not silently clear unsupported caller state to gain admission. Both clear variants invoke the same underlying reset,clearing contexts/cluster/replacement properties while retaining allocation. Keep result reuse private to this owner; private tests establish fresh/cleared provenance explicitly. Some(instance) bypass includes face-tweak AND format variations. Payload accounting includes dynamic language storage; metadata and transient shared-handle retention remain separately bounded/disclosed. No other plan findings. Still NOT activated until current adoption checkpoint/writer release.

Production graph clarification from final review: old Skrifa0.44/read-fonts0.41/glifo paths are retained in root Cargo.lock for other-platform or optional consumers. Windows-target reverse trees report nothing for those old packages; do not describe them as active Windows duplicate dependencies. This corrects earlier planning shorthand without changing the accepted minimal lock diff.

Raw-result prototype activation: after independent partial-adoption acceptance and local source checkpoint fc9edf1a, parent explicitly activated sole implementer_m2a on this reviewed archive-only packet and policy refinement. Production source is frozen; no live cache authorized. Preserve previous candidate/archive evidence. Independent source/private-test/parity review must precede bounded owner timing. Parent owns tracked docs/Git.

Raw-result core review findings (prototype in progress): no runtime cache/key/ownership/bounds defect found. Before timing acceptance, strengthen actual miss→hit→forced-native-miss allocation proof using GlyphBuffer glyph_infos().as_ptr() with sufficient established capacity, and compare unknown-context/custom-cluster bypass outputs against identical-policy cache-off references. Empty-buffer/native-variant assertions alone are insufficient. Rerun only affected private contracts; preserve earlier4/4 logs and failures. Owner storage high-water labels must mean MAXIMUM PER-FACE payload/entries/fixed-entry metadata (not sum/RSS),excluding Vec spare capacity/Arc headers/allocator overhead and separately disclosing temporary evicted shared-handle retention. Writer explicitly accepted these corrections; root canonical/parity/timing adapter and its final review remain pending. No owner timing has run.

Raw-result private-proof follow-up: allocation reuse is independently closed at font.rs SHAb1ae2db80863ac539001cfdac95cf1abb586fd1421eb2dae396f3835ac71a2b4 and passing4/4 log8016d99a3b0d5c3c223c9e737496a4ca46840137bf70d3273c352ae066d34451. Unsupported-policy equality alone remains insufficient: configured cache-off reference must first differ observably from default-policy reference, then match cache-on configured output. Reviewer suggests a discriminatory combining-mark cluster fixture and an explicit not-found variation-selector policy/selector or genuinely context-sensitive text. No new font/getter/framework required. Writer is closing this narrow fixture gap; no runtime core defect found, no owner timing authorized.

Raw-result core proof closure: reviewer independently accepted font.rs SHAe9f7a68d7fd9eaee5a5f4067ac5ba8c6cf978c18248fb23be5ce66af69c6a0c7 and private-contracts-raw-cache-006 log SHAada2de8304dc9695b6568e44b677b9ad21578488fc233d5c5887151321bc6474,actual4/4/native0/3.115s command. Combining-mark Characters policy and unknown-provenance explicit not-found-selector replacement each assert configured-reference differs from defaults and matches complete configured cache-on output. Allocation-pointer miss-hit-forced-miss proof remains. No core findings remain; root owner/canonical/source freeze review and parity execution still pending. Timing NOT authorized yet; production untouched.

Raw-result owner review: no-run002 passed (native0,175.549s),but final source must (1) support parity-only phase/early return before warmups/timers,then explicit timing phase regenerating full gate in fresh output directory; (2) bind every warmup/sample output/structural/viewport identity to the accepted off/on/reference parity identities,not only per-mode first sample. Serializer,prep/timer/freeze/counter/storage logic otherwise accepted provisionally. New raw-cache contracts are referenced only by their own module/tests,so fence that module with all(test,default_fonts) and clear with cfg(test); no ordinary-dependency helper needed. Started003 was intentionally cancelled before test execution for concrete source corrections,with partial logs preserved; verify its owned descendants terminal before subsequent compilation. No timing authorized.

Raw-result final adapter source/protocol review: both owner findings independently closed at root4039af65…a439/font6a8c35a1…a79f. Phase defaults parity and rejects other values; empty output directory required; parity uses no outer owner clock and returns before warmups/samples. Timing regenerates full canonical gate and every warmup/sample checks the accepted parity output/structural/viewport tuple. Test-only cfg fencing verified. No-run004 passed native0/58.142s (no bodies executed); exact binary/source freeze pending. Cancelled003 receipt records tool session6857 interrupt exit1,subsequent Get-Process cargo,rustc empty,no PID/ancestry/native Cargo completion claim. Source/protocol provisionally accepted; actual parity-only execution after binding review,and independent canonical parity review before timing still required.

## M2-A partial production raw-result reuse proposal — NOT activated

Objective: integrate the accepted raw-result core as a partial correction; M2-A remains incomplete. Live source fc9edf1a already has the aligned0.13.3/0.46.2 stack and independent FIFO8 plans. Accepted archive freeze002 manifest4f4ce0c6092301d159edd33c0adaff436e563b012d30062eaa22f6f1c32cda44 binds prototypefont/text_layout and the completed private/fullcanonical/timing proof. Parent is checkpointing evidence; proposal review precedes writer activation.

One writer owns only vendor/epaint/src/text/font.rs, vendor/epaint/src/text/text_layout.rs and vendor/epaint/PATCH.md,plus fresh ignored harness/receipts. No app/dependency/feature/font/renderer/profile changes. Do NOT copy archive files wholesale. Port only typed ShapeBufferProvenance, RawShapeCacheKeyRef, bounded entry/storage/accounting and LayoutShapeBuffer carrier; no public reexports,diagnostic references/counters/clocks/modes/capture args/verification exports/runtime test-helper module or production enable switch.

Initialize each face empty. Keep128 LRU entries/1MiB retained key+glyph payload/4KiB text/64KiB per-entry payload,checked text/language/complete glyph accounting and oversize rejection before eviction. Hits update recency without copying arrays. Preserve actual instance construction from both face-tweak and format coordinates; every actual Some(instance),including default normalized coords,bypasses raw reuse while FIFO8 plan selection remains unchanged.

Admission checks BEFORE buffer population: known layout provenance, initially empty buffer and default cluster policy. Unknown policy bypasses regardless of apparent emptiness; do not clear hidden context/replacement state to admit. Key exact text,resolved script/direction/language/flags per face; fixed local contract remains empty features,no scale/point-size/custom functions,empty context/default buffer policies. No plan-key substitution. Raw lookup stays immediately before retained-plan shape; owned hit carrier keeps immutable complete arrays and incoming buffer,miss/bypass retains real GlyphBuffer. Consuming clear recycles original allocation and releases arrays; no face borrow during emission.

Only runtime caller is layout_section. layout obtains new/cleared buffer from FontsImpl::take_shape_buffer,per-run carrier.clear retains that lifecycle,and layout returns it through return_shape_buffer. Mark ONLY this proven path LayoutOwnedClean. Preserve layout_shaped_run emission,fallback,spacing,continuation,scale/raster order. Two existing private calls adapt deliberately; keep raw hits from concealing existing plan-key/FIFO assertions (unknown provenance can force legitimate bypass without a runtime toggle).

Tests: keep four existing private plan/font contracts; port four raw contracts under cfg(test) with no runtime observer dependence: raw_cache_key_covers_resolved_components_and_is_face_scoped; raw_cache_enforces_lru_payload_and_oversize_limits; raw_cache_hit_miss_recycling_and_unknown_buffer_bypass_match_reference; raw_cache_bypasses_variable_instances_and_preserves_extrema_outputs. Replace archive counters with carrier variants/private state/accounting/direct test-only reference shaping. Preserve complete glyph structs,multilingual/cluster/flags,LRU/overflow/oversize/face recreation,allocation-pointer recycling,discriminatory hidden-policy/custom-cluster checks,format AND face-tweak instance bypass and actual Bahnschrift300/700. Add one small pre-populated-buffer bypass case to the existing contract: preserve original append/shaping semantics and cache entries; build the identical reference buffer/input sequence rather than assuming concatenated-text clusters.

Verification: fresh ignored copy of production-adoption-validation/epaint-focused-004/Cargo.toml targeting LIVE vendor source,same pinned dependencies/features/separate lock; explicit four existing+four raw OR-filter,exact8 executed. Then locked/offline all-target check; exact20-root OR-filter from production-adoption-validation/receipts/root-focused-20-001.receipt.txt,quoted as one argument; unchanged existing ignored track_c_diagnostic_root_list_geometry_cost_breakdown,100/1000coldwarm,two warmups/five samples each,all20records. Production raw caches naturally participate during existing prep; no reset hook to imitate archive experiment. Correct current source/profile attribution,QA requests absent,exact binary/lock/source receipts,root/vendorfmt/diff,unchanged dependencies/assets and independent source/results review. Search for archive symbols and stale direct-buffer assumptions. No Full81/release/native yet.

PATCH.md documents ownership/key/fixed contract/admission,bounds/recycling proof,memory exclusions,accepted prototype source/evidence/removal condition while retaining prior release/FIFO provenance. Done requires8private/20root/alltarget/20diagnostic checks and review with intended normal runtime path/no experimental machinery. Prototype373.0338→227.9875ms is diagnostic support,not original/warm/release/native parity. Retained maxima175833payload/14336shallowmetadata perface and1132348 cumulativecopied bytes are distinct from total heap/RSS. No architecture blocker identified; live port correctness/normal-prep measurements remain to prove.

Production raw-result integration activation: independent reviewer accepted the preceding proposal as implementer-ready without corrections. After local prototype evidence checkpoint0857af6a, parent activated sole writer /root/implementer_m2a on exactly that bounded production packet. This supersedes NOT activated above; no other scope changes. Parent owns tracked general docs/Git. Source-ready review runs alongside required focused checks; acceptance and source checkpoint remain pending. User local-only checkpoints and application launch/screenshot authorization persist; .gitignore matches tracked blob9f6b156df3bd5dc178cc9c55bf1539cbc8b7830c and unrelated Workspaces/ remains untouched.

Production integration review in progress: initial private-cache-tests-010 passed8/8,including actual Bahnschrift300/700 format/face-tweak bypass; initial test-only compile failures preserved. Reviewer found no runtime defect or leaked experimental machinery at font CD21EA2A499B6E25006EE0681DF216905CA945D353A06C8B12949503775EAC08/textlayout0537FF82CD641E7BD866A47142612026F80A83133FD219674B1CC767B261A2E3. Two narrow findings remain: prepopulated-buffer fixture must use deliberately claimed LayoutOwnedClean to independently exercise is_empty rather than short-circuit Unknown; PATCH must state fixed ShapeOptions/context/replacement contract. Writer will finish already-running all-target invocation,correct test/prose,rerun affected private proof and refresh hashes. Initial command receipt uses filter placeholder; exact-command addendum required. Root20/20-record normal-preparation diagnostics/final review/checkpoint remain pending; no performance acceptance.

Production raw-result integration accepted: both source findings closed at finalfontD31D…f887/layout0537…a2e3/PATCH197D…836d; private0118/8 inclactualBahnschrift,root00220/20,finalalltarget003,fmtdirectbins and unchanged20-recorddiag pass. Parent/reviewer independently verified15manifestrows,bothbinaries,exactargvJSON1CF349…E9F6 and sourceA86C…9C1D; all20records correct,100cold/warm14.3379/2.1737ms,1000cold/warm213.2354/2.1353. Reviewer independently checked actual3ownedfilediffwhitespaceexit0,closing inheritedreceipt's oldscopenote. No findings remain;writerreleased,localcheckpointnext. See receipts/m2a-production-raw-shaping-reuse.md. Partialonly;originalperformanceandM2-Astillopen. Planner read-only nextremainingcostproposal underway;no newsourcewriteractivated.

Local production raw-result checkpoint9b7006cd saved after final acceptance. Only unrelated Workspaces/ remained untracked at commit; no push. Sole source writer released. Planner nextremainingcostproposal remains read-only/notactivated; M2-A open and all latergates/TrackBstillrequired.

## M2-A archive post-section layout attribution proposal — NOT activated

Objective: identify the first still-unmeasured post-section responsibility in the remaining cold owner gap, without another cache or production optimization. Checkpoint9b7006cd is source of truth. Accepted raw experiment reduced native shaping to49.3760ms while owner remained227.9875ms; live normal-preparation diagnostic cold1000owner213.2354ms/warm2.1353ms still lacks original parity. Do not subtract historical medians to infer current phases.

Construction: fresh ignored target/egui-modernization/m2a/post-shape-layout-attribution/root from TRACKED9b7006cd including existing vendor packages/manifests/lock; preserve uninstrumented inventory first. Existing root epaint path vendor/epaint remains valid. Do not begin from old cache-toggle prototype or replace production core. One writer owns only archived root/vendor/epaint/src/text/{layout_diagnostics.rs,mod.rs,font.rs,fonts.rs,text_layout.rs} and archived root/src/gui/render.rs plus fresh private receipts/harness. Parent owns trackeddocs/Git. No live source,dependency,feature,font,renderer,profile or cache-policy change. No epaint lib.rs change needed (text already public).

Port ONLY accepted observational hooks: existing counters/traceDTOs/reset/freeze/canonicalprojection in layout_diagnostics; moduleexposure in text/mod; plan/native-shape/raw-cache/glyph-allocation counters and traceinputs in font; galleyhit/miss/miss-time/pass/eviction/atlas observations in fonts; section/segmentation/emission/trace hooks in text_layout. Current production raw-result admission/LRU/bounds/carrier and FIFO8 semantics remain authoritative. No verificationexports/runtime-testhelpers/engine-selection branches. Observation presets may retain names ReusePlan/TraceReusePlan ONLY if observation-only. Remove raw_shape_reuse_enabled/setter/preparationdisables/cachemodebranches; rawreuse is alwaysenabled duringprep/parity/everyowner. Reset/freeze observations never clears facecaches.

New clocks: FOUR aggregate intervals in text_layout::layout, no perrow/glyph clocks. Inclusive post_section starts immediately after fonts.return_shape_buffer(shape_buffer) and ends after galley_from_rows completion. Three disjoint children wrap existing calculate_intrinsic_size, rows_from_paragraphs and galley_from_rows calls; galley interval includes existing positioning/tessellation/rounding. Preserve exact operationorder/moves/return behavior. Derive post_section_other=inclusive-intrinsic-rows-galley with explicit checked nested arithmetic; never add parent tochildren. Remainder includes overflowcharacter handling,alignment/justification/setup/observeroverhead. Print existing sameowner galley/section/segmentation/emission/nativeShape/geometry counts/times, not historical subtraction. New clockflag controls ONLY these4intervals; existing observerenvelope identicaloff/on.

Owner: derive one ignored track_c_diagnostic_root_list_post_shape_layout_attribution,1000coldrows. Call LIVE track_c_prepare_root_list_state(...,cold=true) unchanged with actualninepasses and observersdisabledduringprep; rawreuse participatesnormally. Do not import prototype with_plan_warmup/oldrun_owner. Afterprep reset/enablecounterortrace,setnewclockflag,run existing run_root_list_frame_with_pass_count with unchangedownertimer; stop timer and immediatelyfreeze before track_c_validate_root_list_state/canonicalcomparison/disposal. Freshidenticalstates,2warmups/5samples PERclockmode,samebinary. Require matchingactualwork/cachecounts/identities,not old3046hits/1001misses; verify layoutcallcounts ratherthanassuming1000.

Protocol: diagnostic-specific ML_M2A_POST_SHAPE_PHASE=parity|timing defaultsparity, rejectsother; fresh emptyoutputdir/create-newfullsnapshots eachinvocation. Reuse accepted rawarchive nested canonical_snapshot/assert_same_snapshot and COMPLETE orderedrun/glyph/exactgeometryprojection,adaptonlyenclosingstateplumbing. Parity-only freshnormalprepclocksoff/on owners writefullsnapshots and compareboth acceptedcanonicalreference/eachother; validatecounts/canonicalidentitytuple; no reportedownertimingseries/warmups/samples. Freeze source/lock/binary/commands/env/snapshotidentities, STOP forindependentactualsource/parityreview. Timing onlyafterexplicitparentauthorization,unchangedreviewedsource/binary,newfreshdir; regeneratefullcanonicalgatefirst,then2warmups5sampleseachmode traceoff. Everywarmup/samplebindsaccepted output_identity/structural_signature/viewport_receipt tuple and logicalwork/cacheobservations. Preserveall10rawsamples/newphasecounts/times/nestedarithmetic/offonoverhead; excludevalidation/disposalfromowner.

Verification: locked/offlinefast-devlibNextest exactignorednewowner onarchive'srootmanifest, performance/clipboard/source/profileenv and QAexclusions asaccepted. Source/no-run/privatebindingreview precedesactualparity; source/actualparityreview precedestiming. Run onlythisdiagnostic+embeddedparity; noFull81/fullsuite/release/native. Freezebeforecopy/afterhooks sourceinventory,locks/fonts/binaryprovenance and allfailures. Done: trustworthy off/onmeasurements identify/excludepostsection materialcost withcompleteparity/normalprep/currentproductionsemantics and independentreview. Smallresult isvalidexclusion; noautomaticfurtherinstrumentation/optimization. Genuineuncertainties currentdistribution/clocks-overhead only. Preimplementationreviewrequired; NOTactivated.

Post-section proposal preimplementation review accepted with no required corrections. Live timing boundaries match; disjoint children/checked residual preserve arithmetic. GalleyCache miss time is another inclusive owner and must not be added to post_section. max_rows==0 early return means actual post_section call count must be reported/matched, not forced to1000. No builds/writes/activation by reviewer. Parent will activate exactly this archive-only packet after plan checkpoint.
