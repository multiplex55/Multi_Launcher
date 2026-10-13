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

Command shape: verified Cargo nextest run --manifest-path <archive>/root/Cargo.toml --locked --offline --cargo-profile fast-dev --lib --run-ignored ignored-only -E 'test(track_c_diagnostic_root_list_geometry_cost_breakdown)' --success-output final; Stage2 exact derived test only. Perf enabled,QA feature/request absent,base+patch hashes/native exits/wall times persisted. No full81/grid/backend/application launch. Done: isolate actual cost among cache,plan,shaping,fallback/emission/raster with parity and bounded independent review; production correction requires separate scoped packet.

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
