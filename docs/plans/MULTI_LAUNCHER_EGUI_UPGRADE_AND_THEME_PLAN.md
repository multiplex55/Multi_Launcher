# Multi Launcher — egui 0.27 → 0.36.2 Modernization and Correctness-First Visual Redesign

Current user desktop policy: **Do not use Computer Use for this goal.** Application screenshots remain requested; use the existing app-native framebuffer capture and ordinary process/build/test tooling, without synthetic desktop mouse/keyboard/window automation. Record native/manual gaps honestly. This clarification overrides any historical desktop-tool procedure.

**Policy amended 2026-10-14:** [PERFORMANCE_ACCEPTANCE_ADDENDUM.md](egui-modernization/PERFORMANCE_ACCEPTANCE_ADDENDUM.md) is authoritative under the user's explicit revised requirements. Continue the existing `egui-modernization` branch, complete BOTH tracks, and accept documented performance debt. Do not restart, reconstruct historical baselines, or ask for permission between tracks. The current status/next action is in `egui-modernization/status.md`; older evidence remains historical. Archive identity remains an honestly recorded gap, not a reason to reset current work.

**Priority:** correctness/data/function; stable responsive interaction; finish migration and theming; low-complexity performance hygiene; comprehensive optimization/benchmarking as future work. Exact original-version performance parity is not required.

## 0. Mission, decisions, and rules

Modernize the existing Windows/Rust launcher from egui/eframe 0.27.x (locked 0.27.2 in supplied snapshot) to verified stable 0.36.2, and **then**, immediately in the same orchestration run, substantially improve the launcher’s visual finish through modern minimalist / restrained Fluent-inspired design. Do **not** rewrite the app or discard egui, and do **not** refactor unrelated business logic. Improve the existing UI, not the workflows.

Decisions incorporated from user answers:

- Upgrade directly to **0.36.2** after a preflight version confirmation. If a newer stable exists when Codex runs, document it and prefer the latest stable compatible with the approved migration target, but do not silently move to prereleases.
- Dependency migration and visual redesign happen **back to back**, *not* in a single undifferentiated commit. The user selected both “separate” and “combined”; interpret this as one continuous run with independent milestones, baselines and reversible commits.
- Retain the explicitly selected, working Glow renderer unless a demonstrated compatibility or serious usability defect warrants change. Defer Glow/Wgpu competition to the post-modernization backlog; do not build backend variants to chase parity.
- Style: minimalist, Windows Fluent-inspired, compact, excellent keyboard readability. No wholesale layout redesign or feature changes.
- Scope: root query launcher/list/grid first, then settings and plugin dialogs. Explicitly **exclude** redesign of native radial-menu surfaces and native cursor overlays unless required for API compatibility. All plugins and existing feature commands continue working.
- Keep System, Dark, Light, Custom; repair actual runtime theme application; add tasteful presets/accent colors; immediate in-dialog preview with Cancel rollback; standardize typography and icons.
- Static corner radii, borders and minimal shadows are welcome. Blur, acrylic, perpetual animations/repaint loops are off. The user also checked “optional advanced graphical effects”: implement **optional, disabled-by-default** effects only if bounded, validated, and maintainable; don't let them delay core modernization.
- Accept documented performance regressions under the revised user policy. Evaluate absolute/practical cost and actual severe usability/resource defects; higher medians or isolated percentiles alone do not block progress. Preserve all historical adverse evidence; further parity-only optimization and comprehensive statistics are deferred.
- Dedicated branch, active checkpoint commit cadence, planner / implementer / reviewer delegation, Windows testing, rollback instructions. Keep appearance change isolated from API stabilization. No permanent classic/modern UI toggle is required; Git checkpoint is sufficient as rollback.
- Continue the existing **`egui-modernization`** branch and chosen live source. Do not reset/rebaseline, branch from master, merge unrelated work or discard changes. Checkpoints remain local-only under the existing user instruction.

### Absolute hard rules

1. Inspect `AGENTS.md`, all applicable repository instructions, git status/log/branch, `Cargo.toml`, `Cargo.lock`, startup flow and GUI before changing code. The actual chosen live checkout is authoritative. Preserve the unresolved historical archive comparison as context; do not reset or reconstruct a baseline to reconcile it.
2. No behavior redesign, command reorganization, altered hotkey dispatch, focus policy, search ranking, view switching, global mouse hook, macro semantics, clipboard privacy, persistence contract, or window placement policy.
3. No panics/unwraps introduced on user input, missing resources, theme config or DPI/renderer reinitialization. No clippy-silencing broad allowances.
4. Fix observed critical correctness defects or severe practical usability/resource regressions; slower historical synthetic benchmarks alone do not block theming. If an upstream architectural incompatibility blocks the direct jump, document it and use intermediate versions as temporary *implementation stepping stones only*, without shipping partially incompatible binaries.
5. Never “repair” test failures by weakening assertions, deleting benchmarks, trimming fixtures or removing coverage. When an old test relies on a removed egui internal, replace it with an equivalent or stronger behavioral invariant.
6. Resolve/apply theme on effective changes; reuse fonts/icons/textures/caches. Avoid unconditional per-frame styling/resource setup, disk IO and expensive native queries. Bounded additional rendering work/allocations are acceptable; zero extra allocation is not a gate.
7. Preserve event-driven idle behavior. Animations must have strict deadlines and cease requesting repaints when invisible, stationary or disabled.
8. Preserve functional cache/layout/Unicode/cursor/output invariants and already integrated justified optimizations. Do not port isolated candidates or broaden dependency forks solely for accepted performance debt. Reuse existing evidence/infrastructure instead of creating new sampling frameworks.
9. Each checkpoint: tests appropriate to scope, reviewer assessment, write results and limitations, commit; push checkpoint when remote/policy allows. Never push an untested broken intermediate checkpoint as “done.”
10. The implementation is not complete until both tracks A and B finish, required verification runs, and final reviewer signs off. A compiling app with unchanged visuals is **not completion**.

## 1. Evidence map from provided code

**Verified 2026-10-12 archive findings** (paths are relative to repo root):

| Path / symbol | Observed | Direct action |
|---|---|---|
| `Cargo.toml` | `edition = "2024"`; `eframe = "0.27"`, `egui_extras = "0.27"`, `egui-toast = "0.13"`, `egui_commonmark = "0.15.0"`; direct `winit = "0.29"`, `raw-window-handle = "0.6"` | Upgrade version family; resolve transitive compatibility, duplicate egui trees and native window handle conversion |
| `Cargo.lock` | egui and eframe 0.27.2 in supplied snapshot | Baseline dependency/SBOM; track exact post-migration versions |
| `src/gui/render.rs` | `impl eframe::App for LauncherApp` implements `fn update`; `RootListGeometryCache`, separate list/grid paths, geometry telemetry, text/atlas-based cache invalidation | Migrate to new app lifecycle while retaining temporal order of input, logic, paint and side effects; preserve geometry validity |
| `src/gui/notes_dialog.rs` | font texture atlas identity used in notes/layout caching and related tests | Redesign invalidation consistently with main list; test font/style changes |
| `src/gui/mod.rs` | app structure, toasts, visible state, panels, startup state, root geometry, theme-editor flags | Avoid app-wide rebuild and lost state; central theme manager integration |
| `src/main.rs` | `build_viewport_with_icon`, `NativeOptions`, `run_native` | Explicit backend selection and startup/window behavior tests |
| `src/settings/model.rs` | `ThemeMode`, `ThemeColor`, `ColorScheme`, `ThemeSettings` with System/Dark/Light/Custom and named presets | Extend compatibly; serde defaults and migration; no settings data loss |
| `src/gui/theme_settings_dialog.rs` | draft and persistence plus preview; `Apply`/`Cancel` concepts | Preserve draft semantics while adding coherent live runtime preview with guaranteed cancel rollback |
| `src/gui/regex_tester_dialog.rs` | `ctx.set_visuals` found in test routines | Don't confuse test-only visuals changes with production theme application |
| `src/gui/mkmacro_dialog`, `src/gui/radial_editor`, `src/gui/note_panel.rs` | complex custom editors, secondary viewports, graphics/textures and repaints | Migration and visual pass must preserve editor workflows and cost |
| `src/gui/mouse_settings_dialog.rs`, `src/gui/file_search_dialog.rs`, `src/gui/coordinate_tool.rs` | timed repaint paths | Audit idle repaint frequency and visibility guards |
| `src/visibility.rs` and Win32 overlay modules | native window integration, viewport lifecycle, input/hotkey interaction | Window visibility/focus/monitor regression suite |
| `docs/performance/track_a_*`, `docs/performance/track_c_*` | performance recipes, data, scripts and evidence | Reuse contract and update measured evidence, never alter originals |
| `.codex/agents/planner.toml`, `implementer.toml`, `reviewer.toml` | project-specific agents | Delegate according to AGENTS.md orchestration |

**Known upstream changes:** eframe 0.34 replaced `App::update` with `App::logic` / `App::ui` and changed default renderer to Wgpu; current egui family 0.36.2 requires Rust 1.95+. Verify details against pinned version docs and actual compiler diagnostics instead of coding against remembered APIs. Sources: https://github.com/emilk/egui/blob/main/crates/eframe/CHANGELOG.md ; https://docs.rs/egui/0.36.2/egui/ ; https://docs.rs/eframe/0.36.2/eframe/ ; https://github.com/emilk/egui/releases/tag/0.36.2 .

## 2. Orchestration, branch and Active Checkpoint Commit Cadence

Use an active, milestone-based commit cadence throughout this task. Break milestones into coherent implementation checkpoints (`M0-A`, `M1-A`, `M1-B`, etc.). Commit after each meaningful subsection rather than waiting for a whole phase. Prefix each commit message with the checkpoint (`M1-B: migrate root geometry atlas invalidation`). Maintain `docs/plans/egui-modernization/` with `status.md`, `dependency-audit.md`, `migration-decisions.md`, `baseline.md`, `benchmark-results.md`, `visual-qa.md`, `test-matrix.md`, `regressions.md`, and `handoff.md`. Checkpoints should include SHA, touched files, commands, pass/fail results, reviewer disposition, screenshots or UI notes, blockers and follow-up. Do not overwrite historic Track C metrics.

**Branch guard:** inspect actual status/branch/HEAD before significant work and commits. Continue current modernization history. Initial archive comparison/fetch/branch-creation procedures are historical, not repeated prerequisites. Preserve unrelated changes and source identities; no push without new authorization.

**Role ownership:** planner gives bounded coherent handoffs; one implementer writes each coupled milestone; reviewer evaluates correctness/compatibility/maintainability against the revised acceptance policy. Reviewers must not restore superseded parity requirements. Review coherent changes, not separate receipt/parser/listing/document approval cycles. Run independent file work in parallel **only where conflicts and behavioral coupling are low**. Sequential migration gates are mandatory; do not parallelize skinning before migration is validated.

**Progress cadence:** publish a brief, actionable update after each milestone, showing commit SHA, test status, benchmark status, next checkpoint, risks. No artificial sleeping/polling for completion; avoid redundant expensive builds. Favor `cargo check` and targeted tests between major milestones, full test suite at gates.

## 3. M0 initial preflight record (no repeated baseline prerequisite)

These initial tasks are retained as project context. Reuse their completed evidence; do not recreate the original baseline or reset current work. Unresolved archive/native/hardware coverage is recorded as a qualification gap under the addendum, not a new parity campaign or automatic Track B blocker.

### M0-A — Repository and toolchain preflight

- Read `AGENTS.md`, project roadmap/active instructions, 2026-10-12 attachment basis and existing test/benchmark documentation.
- Confirm Rust/Cargo versions (`rustc -Vv`, `cargo -V`, `rustup show`) including the remote Windows builder; upgrade toolchain to Rust >=1.95 if needed using project-approved mechanism, and record cost. Ensure feature flags and edition 2024 remain intact.
- Save `git status`, SHA, branch, diff inventory, dependency lock snapshot, `cargo tree -d`, `cargo tree -i egui` and `-i eframe` (where available). Inventory all egui-related crates, direct `winit`/`raw-window-handle` usages, optional renderer feature flags, patches and git dependencies.
- Determine whether the supplied archive is ahead/behind HEAD. Maintain isolated worktree if current branch is dirty.
- Run original compilation/test diagnostics on the supported Windows host. A baseline that fails must be classified as preexisting rather than rewritten to look green.
- **Exit:** reproducible baseline, migration host, actual source revision and toolchain documented. **Commit:** `M0-A: record egui migration preflight`.

### M0-B — Golden interaction and render inventory

- Enumerate root launcher flow: hotkey tap → visibility → search focus → type → list results → arrows/enter/escape → grid toggle → hide. Capture screenshot/recording at representative DPI and theme; note window coordinates and size.
- Enumerate settings editor, theme dialog, note editor, MKMacro, file search, radial editor, QR, regex, tooltip/toast, secondary viewports, modal focus, error dialogs, system tray behavior.
- Validate multi-monitor: primary/secondary, mixed DPI, taskbar placement, RDP/remote desktop (if feasible), minimized/hidden/always-on-top, and hover selection. Record “not tested” honestly.
- Inventory every `set_visuals`, `set_fonts`, `texture_atlas`, `load_texture`, `request_repaint_after`, `show_viewport*`, explicit font galley, low-level painter, glyph cache, `Context::run`, backend and native handles. Save with path:line reference and owner.
- Capture baseline: idle CPU and paint/repaint rate while hidden/visible, hotkey-to-visible latency, first usable frame, query updates, scroll throughput on small/large result sets, notes reflow, result geometry cold/warm metrics, memory, GPU footprint and release/debug build time. Use repeated, matched fixtures and track sample counts, warmups, noise, CPU power/profile, Windows scaling, hardware and build SHA. UI metrics may require fresh instrumented hooks; isolate measurement probes behind test/debug flags.
- Do not rely on screenshots alone for performance. Save command lines, settings, datasets, JSON/CSV receipts and failure-only logs.
- **Exit:** reproducible baseline and visual acceptance checklist. **Commit:** `M0-B: capture golden behavior and performance baseline`.

### M0-C — Dependency/API breakage worklist

- Study official changelogs and compare 0.27→0.28→…→0.36 at migration seams. List removed/renamed APIs and calls with file paths, prioritize by compile blockers and runtime consequence.
- Identify compatible **released** versions of `egui-toast` and `egui_commonmark` using crates.io/docs.rs; confirm their egui dependency bounds. If no compatible version exists, evaluate a minimal adapter/fork with feature parity, licensing and maintenance notes. Do not mix multiple egui versions for UI types.
- Determine if direct `winit` is genuinely required or can be replaced with updated eframe reexports without functional change; document transitive version path.
- Investigate `Fonts::texture_atlas` identity changes; devise cache generation/invalidation strategy based on explicit layout inputs, font configuration revision and DPI, not ephemeral borrow address. Separate shape cache from texture cache identity.
- Draft the expected `App::logic` vs `App::ui` division, respecting input event order and state mutation. Keep side effects once per frame; no double action execution.
- **Exit:** executable migration backlog with owners and mitigations. **Commit:** `M0-C: map breaking egui APIs and dependency compatibility`.

## 4. Track A — dependency upgrade and behavior-preserving API migration

### M1-A — Align Rust toolchain and egui dependency graph

- Make a targeted `Cargo.toml` change to egui family `0.36.2`: `eframe`, `egui_extras`, and compatible interop crates; only introduce a direct `egui` dependency where justified.
- Pin renderer explicitly: first reproduce the old backend path (typically Glow; verify from original effective feature set). If modern eframe features require `default-features = false`, explicitly choose needed windowing/platform/input/accessibility features, not a cargo-feature blanket disabling needed capabilities. Confirm feature unification via `cargo tree -e features`.
- Resolve `egui-toast`, `egui_commonmark`, winit, raw-window-handle, images, clipboard, icon formats and `rdev` conflicts case by case. Preserve important `windows` and native API dependencies.
- Update `Cargo.lock` intentionally. Run `cargo tree -d` and demonstrate only one compatible egui major/minor family is used by widgets that share types.
- Build with Windows target and the same features as shipping release; log all incompatibilities. Do not combine UI style changes.
- **Exit:** correct dependency graph and known compiler errors inventoried. **Commit:** `M1-A: align egui 0.36.2 dependency family`.

### M1-B — App lifecycle migration

- Migrate `src/gui/render.rs` `impl eframe::App` from `update` to the 0.36.2 lifecycle required by the current API. Carefully specify which operations happen in `logic`, which in `ui`, and why. Do not assume `logic` and `ui` each execute once per visual frame without verifying the docs.
- Preserve input processing before query/result selection, plugin message consumption, visibility transitions, persistence debouncing, low-power repaint scheduling and shutdown/drop cleanup. Use a per-frame operation ledger if necessary to detect double-running side effects.
- Update `src/main.rs` native app creator, `NativeOptions`, viewport builder, icon semantics, app construction and errors. Preserve existing launch dimensions and position across dialogs and root surface.
- Audit all `Frame` accesses and change any removed native handle/access method according to official API. Preserve window decorations/transparency behavior.
- Test launcher tap toggle while focused, hidden and with radial enabled; key release state and focus restoration; ensure no regression of known tricky keyboard chord.
- **Exit:** app launches and basic UI actions work on real Windows. **Commit:** `M1-B: migrate eframe lifecycle with event-order parity`.

### M1-C — Mechanical egui API compatibility

- Migrate changed `Ui`, `Painter`, `Layout`, `Margin`, `Stroke`, `Rounding`/`CornerRadius`, `Frame`, menus, `TextEdit`, color/opacity, fonts, scroll area, docking-ish panels and viewport API invocations. Use `cargo check` feedback to identify all call sites.
- Avoid mass search/replace that changes visual geometry or `Id` stability without review. Where signatures changed, keep actual dimension semantics in points vs pixels, and evaluate rounding conversions explicitly.
- Preserve accessibility/input behavior, selection highlight, drag handling, context menu close and nested menu focus. Honor stable widget `Id`s in dynamically repeated groups and editors.
- Preserve image/text scaling and primitive scissor/clip behavior. Repair imports and warnings narrowly.
- Do not polish. Compare source screenshots for visible deltas introduced by API replacements and record expected unavoidable deviations.
- **Exit:** no compile errors in main/bench/test compilation; targeted tests pass. **Commit:** `M1-C: adapt egui widgets, geometry and viewport APIs`.

### M1-D — Rich text, toasts, image and third-party compatibility

- Upgrade or adapt `egui-toast` and `egui_commonmark` with exact behavior: toast stacking/duration/anchor, note markdown syntax and links, code fences, text selection, scrolling and memoized parsing.
- Audit any crate bringing its own independent egui version; avoid bridging incompatible `egui::Context` and widget types.
- Verify texture creation, lazy loads, image previews, clipping, screenshots and QR images. Preserve existing cache lifetimes; avoid texture reupload per frame.
- Run preview, note markdown, toast and error notification smoke tests. Verify drag-and-drop and native file picker ownership/focus.
- **Exit:** integrations compile and behavioral parity demonstrated. **Commit:** `M1-D: port egui integrations and image surfaces`.

### M2-A — Geometry/cache correctness closeout

- Keep actual integrated source and inspect relevant cache/atlas/revision ownership; preserve correct invalidation, virtualization, wrapping, Unicode, cursors and full output. Palette-only changes must preserve text geometry; typography changes invalidate deliberately.
- Validate the actual live provisional closed-presentation guards with targeted affected tests and a Windows all-target compile. Reuse valid unchanged lifecycle/input/cache/layout/vendor evidence; fix concrete failures only.
- Existing release ROOT/list regressions are accepted performance debt. Preserve raw measurements, source/profile identities and original failed dispositions. Do not initiate compact/ASCII/shaping/allocation/row-storage experiments or additional benchmark matrices solely to recover original speed.
- Preserve isolated candidates in the future backlog without porting them. Do not rebuild historical baselines for documentation completeness.
- **Exit:** chosen live source has necessary compilation/correctness evidence, no known critical cache/geometry defect, and the former performance blocker is `accepted_with_documented_regression`. Unexecuted checks remain unexecuted. **Commit:** `M2-A: close live cache and presentation correctness`.

### M2-B — Functional Windows integration

- Preserve hotkey tap/hold, visibility, input ownership, once-only actions, hidden-root child/prompt creation/dismissal, focus restoration and window placement.
- Use available desktop and automated/source evidence for show/hide, typing/navigation/Enter/Escape, list/grid/scroll, representative notes/plugins and true idle distinguished from active timers/effects. Reuse existing synthetic isolated-workspace infrastructure.
- Run the smallest useful smoke checks. Do not endlessly retry unavailable desktop captures/elevation/profiling. Record exact unobserved physical hotkey/focus/monitor/DPI/native scenarios and required manual QA; environmental gaps permit safe continued implementation, without inventing passes.
- **Exit:** available functional evidence reviewed, demonstrated critical defects fixed, qualification gaps explicit. Accepted synthetic performance differences are not reopened. **Commit:** `M2-B: stabilize Windows integration and record QA gaps`.

### M2-C — Retain working renderer

- Inspect actual configuration. Currently eframe defaults are disabled, Glow is enabled, and NativeOptions explicitly selects `Renderer::Glow`.
- Retain this working renderer unless a demonstrated compatibility or serious usability defect requires change. No new backend feature/variant/build solely for benchmark parity.
- Reuse completed renderer evidence if any; otherwise record “Existing renderer retained; comparative performance study deferred.” The previous multi-run Glow/Wgpu competition is future backlog work.
- **Exit:** explicit renderer decision documented. **Commit:** include the decision in the coherent migration stabilization checkpoint.

### M2-D — Functional migration acceptance before theming

- Establish adequate chosen-source Windows compilation and targeted correctness evidence. Run relevant formatting and Clippy checks under repository standards, with broad automated regression at this meaningful stabilization or final gate, not after each small edit; classify preexisting/environment failures without deleting coverage.
- Review existing functionality and available smoke checks, including persistence/plugins/input/child lifecycle. Fix known critical defects. Document missing environmental/manual coverage honestly and continue safe work.
- Accepted benchmark regressions are documented debt. Do not require original parity, new large matrices or profiling before proceeding.
- Preserve an identifiable migration-only rollback source checkpoint before theme changes. Reviewer assesses THIS revised policy.
- **Exit:** adequate compilation/correctness, no known critical functional defect, accepted performance debt and QA gaps recorded. **Commit:** `M2-D: accept functional migration with documented performance debt`.
- Immediately begin M3-A and complete Track B without another approval request.

## 5. Track B — theme engine, skins and visual polish (begins immediately after M2-D)

### Design doctrine

**Goal:** impressive visual fit-and-finish without perceived sluggishness. Maintain keyboard-first compact launcher, existing information architecture and functional order. Standardize the visual language across surfaces; do not turn every control into a large card, enlarge every row, or animate the whole app. Use a controlled, tasteful hierarchy: surface, raised panel, border, accent, text strong, text muted, selection, hovered, pressed, disabled, warning, error, success. Use semantic tokens, not scattered raw color literals. Keep all effects optional and inert when disabled.

**Reference style:** Windows Fluent-inspired *without* requiring Acrylic/Mica APIs or translucency. Dark charcoal / navy neutrals, clear focus rings, soft geometric radii, spare borders, deliberate icon alignment, balanced spacing, compact density. Respect OS contrast and system theme changes.

### M3-A — Semantic design system model

- Define a centralized `ResolvedTheme` (suggest `src/gui/theme/` or equivalent) built from persisted `ThemeSettings`, OS mode, optional preset/accent settings and a theme revision. Separate persisted user data from derived egui style and runtime caches.
- Model palette semantic tokens (background root/panel/input/elevated/popover, foreground primary/secondary/disabled, border subtle/strong, accent/hover/selection, semantic success/warn/error, focus); typography token scale; spacing and corner radius tokens; stroke thickness; static shadow; icon metrics; density compact default.
- Retain/extend existing `ThemeMode`, `ThemeColor`, `ColorScheme` and `ThemeSettings` in `src/settings/model.rs`. New serde fields need safe defaults, readable naming, range clamps and backwards-compatible load for existing JSON. Do not remove `named_presets` or mutate meaning of existing saved colors silently.
- Define identity/version scheme: palette-only changes should repaint but preserve text layout caches; typography/scale changes should invalidate geometry exactly once. Avoid auto-recreating full style each frame.
- Add unit tests for settings migrations, round-trip serde, empty/unknown presets, numeric extremes, transparency/clamping, and System mode transitions.
- **Exit:** design tokens owned centrally with tested backwards compatibility. **Commit:** `M3-A: introduce compatible semantic theme model`.

### M3-B — Runtime theme application layer (fix existing gap)

- Audit app lifecycle for theme load and changes. Build resolved visuals on startup and on **change events**: saved theme revision, draft-preview revision, OS dark/light mode transition, or user setting changes.
- Apply `egui::Style`, `egui::Visuals`, spacing and text styles in the proper context, with per-viewport consistency. Do not globally override text metrics unintentionally; avoid repeated `Context::set_style` or `set_visuals` every frame.
- Ensure System follows Windows theme (with clear behavior if detection unavailable), Dark/Light reliably override and Custom uses user palette. Make all root result list/grid panes and settings dialog reflect changes.
- Define stable event handling and a `ThemeRuntime`/revision cache; detect no-op settings modifications. Do not clone the entire `ColorScheme` during every frame.
- Verify no plugin/editor calls `ctx.set_visuals` during regular runtime and inadvertently overwrites the app theme. Scope test-only style manipulation correctly.
- Test app start, hide/show, reopen theme settings, OS mode flip, secondary viewport creation, dialog close and theme persistence across restart.
- **Exit:** all four modes genuinely affect rendered UI, no per-frame style churn, no cache invalidation on color-only switches. **Commit:** `M3-B: wire settings into event-driven egui styling`.

### M3-C — Theme preview, presets and safe user editing

- Extend `src/gui/theme_settings_dialog.rs` with clear current mode, named presets and accent selection. Keep persisted custom schemes compatible. Start with curated light/dark preset pairs (e.g. graphite, slate, ocean), each accessible and subtle; avoid overabundant skins and bright multicolor noise.
- Introduce preview transaction: dialog opens by copying persisted settings to draft; edits update transient runtime preview immediately; Apply persists atomically then accepts; Cancel/close/Esc restores exactly the pre-preview effective theme (including OS-follow state); failed persistence should keep draft and display clear error without claiming save success.
- Preview must work while launcher is visible, including result list/grid and a mock widget showcase inside dialog. Any external settings reload while dialog open must have documented conflict behavior rather than silently discarding edits.
- Handle rapid slider/color tweaks with events and bounded repaints; don't hammer disk or generate new GPU textures on every tiny color change.
- Keep button order, shortcut behavior, and accessibility sensible. Add unit/integration tests for Apply/Cancel/close, no-op preview, serialization and OS theme changes during preview.
- **Exit:** intuitive, reversible, correct preview/persistence. **Commit:** `M3-C: add presets and transactional live theme preview`.

### M3-D — Typography, icons and lightweight rendering assets

- Select native Windows-friendly font stack using installed/system-available fonts or bundled license-cleared assets **only if truly necessary**. Preserve fallback for symbols, Unicode, emoji and existing glyphs. Do not distribute unlicensed fonts.
- Define coherent typographic hierarchy (search input, primary result label, secondary metadata, dialog section title, controls, helper/error text). Keep sizes compact with legible contrast.
- Reuse egui fonts and font data initialization once; avoid per-frame font definition reinstallation. Typography changes trigger deliberate list/grid/notes cache revision.
- Standardize icons across launcher/result rows, toolbar actions, dialog controls; reuse existing icon assets and textures. Cache decoded resources; avoid per-result image decode and per-frame uploads. Avoid icons that depend on incompatible private egui APIs.
- Validate clipping, ellipsis, tall glyphs, keyboard underline/focus ring, high DPI, CJK/non-Latin scripts if relevant. Add snapshot/geometry tests where feasible.
- **Exit:** readable typography/icons with reused resources and bounded rendering cost. **Commit:** `M3-D: unify typography and cached icon resources`.

### M4-A — Root launcher search field polish

- Prioritize high-visibility main launcher: deliberate root background, soft but legible search-surface boundary, search icon alignment, search text readability, placeholder and selection, shortcut hint if already in UI (do not add behavior).
- Preserve precise existing controls and event ordering, focus on launch/hotkey, IME input, caret position, keyboard navigation and query clear. Avoid padding changes that desynchronize measured geometry.
- Ensure selection/focus states and hover respond without animation loops. Keep root window size/position and user-selected dimensions unchanged.
- Use available lightweight responsiveness evidence where practical; retain keyboard/focus checks. No original-egui benchmark campaign is required for this styling checkpoint.
- **Exit:** root query visual finish improved; all keyboard semantics unchanged. **Commit:** `M4-A: polish compact launcher search surface`.

### M4-B — Result list and grid visual polish

- Replace inconsistent per-row colors, borders, disabled states and text sizes with semantic tokens in `src/gui/render.rs` and relevant drawing helpers. Use discreet selection fill/edge, category metadata contrast and hover indication; no per-cell heavy shadows.
- Preserve result ordering, action labels, key navigation, quick-launch shortcuts, ID stability, visible-row virtualization, drag/swap controls and action counts. No expensive painter work for offscreen entries.
- Audit grid/list geometry cache after any spacing/padding change. Recompute only on effective layout revision, not palette toggles; test 0/1/10/1k/10k results or realistic fixture sizes already in Track C.
- Verify scroll anchoring, wrapping, truncation, stable IDs and virtualization. Assess bounded rendering cost against migration-only evidence where readily available; do not reopen original-egui parity.
- **Exit:** coherent list/grid hierarchy, preserved behavior/virtualization and no observed severe usability defect; bounded extra rendering work is acceptable. **Commit:** `M4-B: restyle list and grid without virtualization regressions`.

### M4-C — UI primitives and standardized surfaces

- Create small reusable UI helpers (not a giant component framework): section heading, secondary label, subtle divider, consistent form row, actionable button variants, focus outline, empty state, optional subpanel frame and semantic status. Avoid large abstraction layering in immediate-mode rendering.
- Use tokenized egui native widgets rather than custom painting for every button; custom widgets only when standard API cannot deliver needed appearance.
- Define sizes, spacing, corner radius, hit targets, tooltip placement and accessibility contracts; preserve stable `Id` for dynamic content.
- Ensure no per-frame theme serialization/cloning. Write component tests for hover, active, disabled, focused and invalid states; snapshot a small showcase UI.
- **Exit:** reusable low-overhead primitives used by both root and dialogs. **Commit:** `M4-C: standardize low-overhead UI primitives`.

### M5-A — Global settings shell and theme editor redesign

- Polish navigation, section headings, descriptions, checkbox/switch alignment, help text, validation, focus rings, save/cancel semantics, scroll areas and spacing in `src/settings_editor/*`, `src/gui/theme_settings_dialog.rs` and common settings components.
- Maintain existing settings section structure and grouping; no feature removal. Avoid unnecessarily expanding dialogs or moving frequently used controls.
- Ensure theme editor itself always reflects preview, supports color contrast safety and readable light/dark components. Show original vs draft where practical without expensive extra viewports.
- Validate available DPI/minimum size, viewport restoration/position and keyboard tab/Escape; record unavailable DPI/hardware scenarios as manual QA follow-up.
- **Exit:** system feels cohesive at settings level and no setting value changes unexpectedly. **Commit:** `M5-A: polish settings and theme editor surfaces`.

### M5-B — High-usage plugin dialogs

- Inventory visual inconsistency in notes, clipboard, MKMacro, Multi Manager, file search, radial editor (the **egui editor UI only**, not native radial skin), mouse settings and regex tester. Prioritize visible frequency and reuse.
- For each dialog: standardize title/header, option hierarchy, labels, input background/stroke, high-contrast selection, separators, button placement, notification display, scroll reachability and content padding. No data model or command behavior modifications.
- Preserve large editing canvases, macro image capture overlays, regex highlights, notes markdown syntax and file preview. Do not break overlays/highlight palettes that have domain-specific meaning; create controlled semantic token mappings where appropriate.
- Track dialogs in a checklist by path, QA outcome and screenshot pair. Commit every 1–3 related dialogs rather than batching dozens into opaque diffs (`M5-B1`, `M5-B2`, ...).
- **Exit:** core plugins are visually consistent, behaviorally tested and responsive, with documented bounded rendering costs. **Commits:** `M5-B1..n: unify [dialog group] presentation`.

### M5-C — Remaining dialogs, messages and secondary windows

- Cover less-used utilities: JSON formatter, QR, calendar, screenshot/image crop, color picker, aliases/bookmarks, recovery, confirmations, notifications/toasts and standalone viewports.
- Audit that secondary viewport theme inheritance is correct on creation and subsequent OS-mode/draft changes. Ensure close/reopen and settings dialogs don't reset parent window position.
- Maintain toast positioning, stacking and lifecycle. Avoid deeply layered shadows, subtle text under low contrast, or inaccessible action states.
- Create surface coverage ledger enumerating all dialog source files, mark migrated/intentional exception/deferred with reason. Do not claim full coverage without validating list.
- **Exit:** broad design consistency with no behavioral change. **Commits:** `M5-C1..n`.

### M6-A — Optional advanced effects as constrained opt-in

- User allowed optional advanced graphical effects. Limit to safe static or bounded features: optional modest elevation shadow, highlight/focus accent or bounded 100–150ms transitions only with bounded practical cost and no continuous idle repaints; accepted timing differences alone are not a veto. Avoid acrylic/blur/translucent composition initially.
- If implementing transitions, add `enable_visual_motion` default false, respect reduced-motion Windows setting when feasible, schedule repaints only for active visible transition windows, no global timer/polling, and test that idle repaint stops.
- Prefer skipping any effect that needs invasive native window handling, multi-pass rendering or significant shader pipeline changes. Document deferred effect ideas rather than coding them.
- Use the selected Glow backend and available host; record unavailable hardware/RDP coverage. Keep expensive effects disabled/deferred and skip invasive work.
- **Exit:** either a small fully validated optional improvement or explicit justified omission, never a hidden performance tax. **Commit if implemented:** `M6-A: add bounded optional visual effects`.

### M6-B — Final visual and functional acceptance

- Review root, theme/settings editor, common plugin/secondary surfaces, System/Dark/Light/Custom, keyboard/contrast and available DPI. Complete the planned surface coverage; document intentional exceptions and unobserved scenarios.
- Run relevant functional/automated checks, broad regression at a meaningful final/stabilization gate, and a bounded themed responsiveness/true-idle smoke. Preserve strict behavioral and persistence/Cancel/restart/theme-revision tests.
- Use readily available lightweight comparisons to the migration-only checkpoint. Comprehensive original/migration/themed statistics, backend competition and unavailable native hardware coverage are FUTURE work, not a completion prerequisite.
- Reviewer evaluates actual functionality, architectural safeguards, documented accepted debt and qualification limits. Fix observed critical defects; do not reopen exact parity or rewrite adverse evidence.
- **Exit:** both migration and substantial visual modernization implemented and reviewed; actual tests, accepted regressions, native/manual gaps and remaining release blockers reported separately. **Commit:** `M6-B: finalize themed functional and visual acceptance`.

## 6. Performance hygiene and future measurement catalog

### Future measurement catalog (not a modernization prerequisite)

The following existing workload catalog is retained for the separate future performance task. Immediate validation is the bounded functional/responsiveness/true-idle scope in the addendum. Do not automatically run this complete matrix.

1. Clean cold process launch → interactive first frame; warm launch after caches.
2. Global hotkey → visible and input-ready, including focused-visible toggle and long-hold/radial coexistence.
3. Repeated keystrokes in empty and populated search, hot query and backspace; display update latency.
4. Idle hidden, idle visible (no pointer activity), settings editor open but untouched: repaint count / CPU / GPU.
5. Root list scrolling and select over 10/100/1000/large entries, cold/warm geometry, list-to-grid switch.
6. Note markdown editing/open/refresh, quick-note metadata and preview.
7. File search, large result lists, tooltip/toast rendering.
8. Theme mode/preset switch, color-only change, typography change, rapid preview edits, Cancel rollback.
9. Secondary viewports, mixed DPI drag, monitor changes, software/remote GPU path if available.
10. Incremental Windows compile, debug build, release build and cargo nextest; report developer-workflow effects separately from runtime effects.

### Current performance acceptance

- Documented regressions are acceptable. Consider absolute time, frequency, practical interaction and realistic large catalogs; no universal percentage threshold or exact statistical match is required.
- Block only demonstrated critical correctness or serious usability/resource defects: crash/hang/data loss, broken input/hotkey/focus/lifecycle, repeated visible freezes, sustained true-idle busy work, unbounded growth or functionally broken virtualization/geometry. Explain the actual observation and severity.
- Preserve all adverse samples and historical old-policy failures. Source796 ROOT debt is accepted under the revised user requirements, subject to actual live correctness closeout; isolated candidates remain deferred.
- Record unavailable observations and manual QA follow-up; continue safe implementation. Source/headless evidence is not a native pass. Full comparative statistics belong to the future backlog.

### Functional coverage inventory

Use relevant existing tests/smokes and preserve coverage. The performance column names future workloads, not additional parity gates. Unavailable native/hardware observations remain explicit qualification gaps.

| Surface | Automated | Manual Windows | Performance | Migration gate |
|---|---|---|---|---|
| Main root/search | query/navigation/event tests | hotkey, focus, position | startup/typing | A and B |
| List/grid | geometry/golden cache | scroll/selection/DPI | Track C large fixture | A and B |
| Theme runtime | serde/mode/revision tests | light/dark/system/custom | switch/idle | B |
| Theme preview | Apply/Cancel/error tests | instant preview/rollback | rapid tweak/idle | B |
| Notes | layout/atlas contracts | markdown/edit/scroll | render/reflow | A and B |
| MKMacro | existing tests | editor/actions/capture | idle/editor | A and B |
| Multi Manager | existing tests | window operations | latency | A and B |
| File search | existing tests | preview/results | large results | A and B |
| Radial editor | existing tests | authoring viewport | idle preview | A and B |
| Native radial/menu | hotkey tests | tap/hold/sticky | invocation | A only (no redesign) |
| Cursor/overlays | existing tests | crosshair/capture/multimonitor | CPU/GPU idle | A only |
| Toast/secondary viewport | render/widget tests | layering/focus/close | idle | A and B |
| Settings migration | serde/recovery tests | old config opens/saves | load time | A and B |

## 7. Failure, rollback and reporting protocol

**Checkpoint rollback:** M2-D is the hard migration-only recovery point; use its commit to compare regressions with final styled UI. M0-B is the pre-upgrade recovery point. Keep all checkpoint commits, do not squash before user review. Corrective fixes go on the same feature branch with separate labeled commits. Never force push or destroy original configuration/data. Use additive serde migrations and verified backup/recovery behavior.

**Blockers:** fix demonstrated critical functional/data/input/lifecycle failures and observed severe usability/resource problems. Slower historical benchmarks alone do not block Track B. Environmental access gaps are documented manual QA follow-up; do not endlessly retry unchanged tools. Continue safe implementation and begin Track B immediately after the functional migration checkpoint.

**Final handoff must include:** SHA and branch; exact egui and renderer versions; toolchain; dependency changes; API migration summary; cache invariants; added theme capabilities; list of styled surfaces; available original/migration/final evidence with unmeasured entries and deferred comprehensive benchmarking; tests actually run; unrun Windows scenarios; limitations; screenshots or clear QA references; opt-in visual effects behavior; rollback commits; documented known issues; follow-up wishlist clearly separated from completed work. Summarize improvements and risks without exaggeration.

## 8. Suggested commands (adapt to repo and Windows shell)

```powershell
# Read repository instructions before running this sequence. Do not blindly overwrite work.
git status --short
git branch --show-current
git rev-parse HEAD
git remote -v
rustc -Vv
cargo -V
cargo tree -i egui
cargo tree -d
cargo fmt --all -- --check
cargo check --all-targets
cargo nextest run
cargo build --release
```

For the separate future benchmark task, use existing Track C PowerShell recipes and their documented fixture contracts from `docs/performance`; run matched baseline and candidate builds with same host, flags and workload. For backend feature comparison, consult `cargo tree -e features` and eframe 0.36.2 feature list before specifying features. Never assume `cargo nextest` exists on an agent or that Windows-specific tests can run on Linux.

## 9. Targeted source-review notes and common migration traps

- The archive may not include `.git`; do not infer branch names from archive paths.
- Different `winit` versions may coexist transitively; only directly manipulate native handles through correct types. Avoid unsafe transmute and downcasts.
- `Arc` pointer identity of font texture atlases is not guaranteed under 0.36.2; convert tests to semantic layout revision contracts rather than pointers.
- Use event-driven theme application. Stateful runtime widget style can be changed outside `update`/`ui` but needs proper viewport and thread semantics under eframe.
- `eframe::App::logic` vs `ui` is not a trivial signature substitution. Review every early return and visibility action to avoid skipping state updates while hidden.
- Style effects may affect hit boxes, truncation, cache invalidation and automatic window resizing. Mark geometry changes explicitly and test.
- Renderers and DPI can subtly change font antialiasing even with identical theme: measure geometric/behavioral parity, not impossible pixel-perfect equality.
- Theme “Custom” and persisted named presets currently exist; do not create a second independent config system or silent overriding defaults.
- `src/gui/theme_settings_dialog.rs` already has draft/apply and color previews; evolve rather than wholesale replacement.
- Watch hot paths: `src/gui/render.rs`, `src/gui/notes_dialog.rs`, `src/gui/note_panel.rs`, overlays and editor live previews; don't add theme locks or resource decoding there.
- Keep related source-level test changes adjacent to implementation; record why each expected pixel/layout threshold moved, if any.
- Preserve project preferences: no unrelated workspaces feature changes, no mouse commands escaping `mouse` namespace, no radial/hotkey toggle behavior changes.

## 10. Execution checklist

Current evidence and ownership are authoritative in `egui-modernization/status.md`; checked migration gates retain the documented native/manual QA gaps and accepted performance debt.

- [x] M0-A preflight and branch
- [ ] M0-B golden baseline and screenshots
- [x] M0-C API/dependency worklist
- [x] M1-A dependency graph
- [ ] M1-B lifecycle and startup
- [x] M1-C widget APIs
- [x] M1-D toast/markdown/texture integrations
- [x] M2-A cache migration and Track C compatibility
- [x] M2-B Win32 / viewport behavior
- [x] M2-C retain working renderer; comparison deferred
- [x] M2-D **migration-only gate** accepted
- [x] M3-A semantic theme model
- [x] M3-B actual runtime application
- [x] M3-C live preview and presets
- [x] M3-D typography and icons
- [x] M4-A root search polish
- [x] M4-B list/grid polish
- [x] M4-C common UI primitives (scoped evidence and native qualifications in status ledger)
- [x] M5-A settings and theme editor
- [x] M5-B plugin dialogs incrementally (B1–B8 source accepted; native gaps documented)
- [x] M5-C remaining dialogs/secondary viewports (C1–C24 source accepted; M6 corrections/qualification active)
- [x] M6-A optional invasive effects/motion explicitly omitted with justification in final receipt
- [ ] M6-B final functional / visual / bounded responsiveness signoff
- [ ] `docs/plans/egui-modernization/handoff.md` complete and checkpoint branch pushed if allowed

**Done means BOTH migration and substantial visual improvements are implemented and reviewed, with actual functional checks, accepted performance debt, native/manual gaps and any release-blocking defects reported honestly. Comprehensive performance optimization remains a future task.**
