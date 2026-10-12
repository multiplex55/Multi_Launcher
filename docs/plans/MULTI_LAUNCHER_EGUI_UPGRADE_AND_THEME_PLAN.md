# Multi Launcher — egui 0.27 → 0.36.2 Modernization and Performance-First Visual Redesign

**Plan status:** Ready for Codex orchestration. **Source of truth:** the user-supplied `multi_launcher(20261012-203406).zip` snapshot; at execution, verify the chosen repository HEAD corresponds to this snapshot and inspect all differences. **Date:** 2026-10-12. **Execution model:** one uninterrupted, sequential Codex assignment; two distinct internally gated tracks: A = dependency/API migration, B = visual design/theme rollout. No user approval between tracks unless an explicit hard blocker arises. **Primary invariant:** functionality and performance outrank aesthetics.

## 0. Mission, decisions, and rules

Modernize the existing Windows/Rust launcher from egui/eframe 0.27.x (locked 0.27.2 in supplied snapshot) to verified stable 0.36.2, and **then**, immediately in the same orchestration run, substantially improve the launcher’s visual finish through modern minimalist / restrained Fluent-inspired design. Do **not** rewrite the app or discard egui, and do **not** refactor unrelated business logic. Improve the existing UI, not the workflows.

Decisions incorporated from user answers:

- Upgrade directly to **0.36.2** after a preflight version confirmation. If a newer stable exists when Codex runs, document it and prefer the latest stable compatible with the approved migration target, but do not silently move to prereleases.
- Dependency migration and visual redesign happen **back to back**, *not* in a single undifferentiated commit. The user selected both “separate” and “combined”; interpret this as one continuous run with independent milestones, baselines and reversible commits.
- Preserve the existing GPU/backend behavior initially. Benchmark Glow and Wgpu when both can be enabled safely. Keep the fastest reliably supported configuration; do not accept a default backend change by accident.
- Style: minimalist, Windows Fluent-inspired, compact, excellent keyboard readability. No wholesale layout redesign or feature changes.
- Scope: root query launcher/list/grid first, then settings and plugin dialogs. Explicitly **exclude** redesign of native radial-menu surfaces and native cursor overlays unless required for API compatibility. All plugins and existing feature commands continue working.
- Keep System, Dark, Light, Custom; repair actual runtime theme application; add tasteful presets/accent colors; immediate in-dialog preview with Cancel rollback; standardize typography and icons.
- Static corner radii, borders and minimal shadows are welcome. Blur, acrylic, perpetual animations/repaint loops are off. The user also checked “optional advanced graphical effects”: implement **optional, disabled-by-default** effects only if bounded, validated, and maintainable; don't let them delay core modernization.
- Reject meaningful reproducible performance regressions. No default allowance for 5% regression. Benchmark with noise-aware repeated runs and report effect size, baseline variance and confidence; don't claim an improvement based on one sample.
- Dedicated branch, active checkpoint commit cadence, planner / implementer / reviewer delegation, Windows testing, rollback instructions. Keep appearance change isolated from API stabilization. No permanent classic/modern UI toggle is required; Git checkpoint is sufficient as rollback.
- Branch: **`egui-modernization`** (working default because no branch name was supplied). Base it on the code matching the supplied archive. Do not automatically branch from `master` if the supplied snapshot comes from a feature branch; establish SHA and ask only if choosing a base risks losing source-of-truth work. Never destructively reset someone else's changes. Branch creation and push need existing valid remote and permissions; do not assume successful push.

### Absolute hard rules

1. Inspect `AGENTS.md`, all applicable repository instructions, git status/log/branch, `Cargo.toml`, `Cargo.lock`, startup flow and GUI before changing code. The archive is the functional source of truth; repository HEAD at run-time must be reconciled against it.
2. No behavior redesign, command reorganization, altered hotkey dispatch, focus policy, search ranking, view switching, global mouse hook, macro semantics, clipboard privacy, persistence contract, or window placement policy.
3. No panics/unwraps introduced on user input, missing resources, theme config or DPI/renderer reinitialization. No clippy-silencing broad allowances.
4. Stop and fix any objectively measured severe regression before continuing to theming. If an upstream architectural incompatibility blocks the direct jump, document it and use intermediate versions as temporary *implementation stepping stones only*, without shipping partially incompatible binaries.
5. Never “repair” test failures by weakening assertions, deleting benchmarks, trimming fixtures or removing coverage. When an old test relies on a removed egui internal, replace it with an equivalent or stronger behavioral invariant.
6. No unconditional per-frame `set_visuals`, `set_fonts`, icon decoding, theme cloning, SVG rasterization, texture loading, settings IO, native Windows querying or fresh dynamic allocation added to hot paths.
7. Preserve event-driven idle behavior. Animations must have strict deadlines and cease requesting repaints when invisible, stationary or disabled.
8. Preserve Track A/Track C cache/performance optimizations; treat their invariants as contracts. Reuse existing docs/performance scripts, telemetry and workload recipes rather than inventing uncorrelated benchmarks.
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

**Branch guard:** `git status --short`; `git branch --show-current`; `git rev-parse HEAD`; `git remote -v`; fetch remote. If the archive omits `.git`, identify equivalent worktree commit by file hashes or best documented basis. If HEAD differs from archive, capture exact divergence and preserve archive content as precedence for feature behavior. Never cherry-pick or merge unrelated changes silently. Create `egui-modernization` from correct validated baseline. If already exists, verify ownership/history before using. Push only where safely configured; report inability to push but continue local changes.

**Role ownership:** planner maps migration and tests before files touched; implementer makes narrowly scoped changes; reviewer reviews API compatibility, performance contracts, persistence, native behavior and checkpoint evidence. Reviewer can demand fixes. Run independent file work in parallel **only where conflicts and behavioral coupling are low**. Sequential migration gates are mandatory; do not parallelize skinning before migration is validated.

**Progress cadence:** publish a brief, actionable update after each milestone, showing commit SHA, test status, benchmark status, next checkpoint, risks. No artificial sleeping/polling for completion; avoid redundant expensive builds. Favor `cargo check` and targeted tests between major milestones, full test suite at gates.

## 3. Gate M0 — source reconciliation, baseline, inventory (no dependency changes)

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

### M2-A — Geometry/cache migration (critical)

- Open `RootListGeometryCache` in `src/gui/render.rs`; document each invalidation key, measured geometry unit, list/grid mode, col count, font atlas, font family/style, DPI, presentation prefix and filters. Reconcile against Track C geometry/synthetic fixture assumptions.
- Implement a stable font/layout revision or explicit signature in place of incompatible `Arc::ptr_eq` atlas identity. Include all factors that can alter text metrics: pixels_per_point, font definitions, font sizes, text wrapping, available width, row density, theme typography if changed. Do **not** invalidate for a color-only change.
- Extend similarly to `src/gui/notes_dialog.rs` layout and markdown caches. Existing tests relying on pointer identity must become semantic cache validity tests: same inputs reuse; font/DPI/text width changes invalidate; color-only theme changes preserve geometry; reused cache never paints stale text or clips selected result.
- Benchmark cold/warm list, large grid, quick notes and result handling with the original Track C harness. Quantify regression and determine whether it comes from egui layout cost, new cache key, text shaping or instrumentation. Don't mask overhead with fixture downsizing.
- Preserve narrow cache ownership, bounded memory capacity and storage receipts. Avoid mutex contention and new full-list traversals.
- **Exit:** cache correctness, no repeated needless remeasurement, measured performance parity. **Commit:** `M2-A: make font and layout cache invalidation egui-0.36-safe`.

### M2-B — Windows windowing, renderer and event integration

- Test root viewport, secondary dialogs, independent UI surfaces, always-on-top, native hwnd uses, multi-monitor DPI transitions, screen geometry, focus ownership, tray interactions, transparency and closing/reopening windows.
- Pay special attention to `src/visibility.rs`, `src/main.rs`, `src/gui/radial_actions.rs`, `src/gui/qr_dialog.rs`, `src/gui/regex_tester_dialog.rs`, `src/gui/screen_draw_toolbar.rs`, `src/gui/mkmacro_dialog/parameter_prompt.rs`, and `src/gui/radial_editor/mod.rs`.
- Validate native hotkey path using real keyboard and mouse: Shift+Alt+Win+End chord, quick tap vs hold semantics, no missed key-up, no sticky modifiers and no focus theft. Confirm grid/window remains togglable while in focus.
- Preserve native overlays (screen capture/region selection/mouse crosshair/radial) and their OS handle layering, even though they are out of visual-redesign scope.
- Inspect mixed DPI 100/125/150/200%, negative desktop origin, Win10/Win11 if available, and render API capability. Mark unavailable hardware cases as such.
- **Exit:** Windows-native regression checklist passes. **Commit:** `M2-B: validate native windows, input hooks and viewport lifecycle`.

### M2-C — Renderer performance comparison and decision

- First measure 0.36.2 with **explicit Glow**, identical release profile/feature set and deterministic fixtures compared to old Glow baseline.
- When safe, instrument a Wgpu build with equivalent behavior; compare binary size, cold startup, first usable frame, hotkey-to-visible, steady idle CPU/GPU, warm query latency, result scrolling and viewports. Repeat >=5 runs (prefer >=10 where variance is substantial), annotate outliers rather than silently excluding.
- Evaluate GPU fallback, software renderers, remote session compatibility, high DPI and transparency. Ensure the alternative doesn't break native Win32 overlay interactions or impose high compile times unexpectedly.
- Choose a backend supported on target environments, prioritizing reliable interaction and no measurable meaningful regression; if neither matches baseline, profile and fix. Keep selected renderer explicit in config and document why.
- **Exit:** committed backend decision with reproducible results, without changing visual design. **Commit:** `M2-C: select measured egui 0.36 renderer`.

### M2-D — Track A final parity gate (hard gate before skins)

- Run fmt, clippy under repository standards, `cargo check` all targets, `cargo nextest run` (where project supports it), targeted benches and the Windows manual test matrix. Record skipped tests accurately.
- Verify settings files, plugins, bookmarks, usage counts, global invocation, search, note, mouse, Multi Manager, MKMacro, radial trigger semantics, file search, quick notes, clipboard, regex and utility commands.
- Diff visual baselines: only unavoidable framework-rendering deltas are allowed here. Do not prematurely “fix” by changing font size/color/layout unless necessary for pixel correctness.
- Compare performance with M0-B; profile and repair regressions. Gather numbers for both visual frames and hidden idle. Reviewer must explicitly sign the performance and behavioral gate.
- **STOP CONDITION:** Do not start Track B with failed critical tests, repeated crashes, fundamentally broken event ordering, or unmitigated meaningful performance regression. Fix first. Minor documented non-critical upstream rendering differences may proceed.
- **Exit:** stable modernization checkpoint suitable as rollback to pre-styling state. **Commit:** `M2-D: accept functional egui 0.36 migration and performance gate`.

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
- **Exit:** readable typography/icons without extra hot-path allocations. **Commit:** `M3-D: unify typography and cached icon resources`.

### M4-A — Root launcher search field polish

- Prioritize high-visibility main launcher: deliberate root background, soft but legible search-surface boundary, search icon alignment, search text readability, placeholder and selection, shortcut hint if already in UI (do not add behavior).
- Preserve precise existing controls and event ordering, focus on launch/hotkey, IME input, caret position, keyboard navigation and query clear. Avoid padding changes that desynchronize measured geometry.
- Ensure selection/focus states and hover respond without animation loops. Keep root window size/position and user-selected dimensions unchanged.
- Measure initial frame cost, typing response and paint primitive count before/after.
- **Exit:** root query visual finish improved; all keyboard semantics unchanged. **Commit:** `M4-A: polish compact launcher search surface`.

### M4-B — Result list and grid visual polish

- Replace inconsistent per-row colors, borders, disabled states and text sizes with semantic tokens in `src/gui/render.rs` and relevant drawing helpers. Use discreet selection fill/edge, category metadata contrast and hover indication; no per-cell heavy shadows.
- Preserve result ordering, action labels, key navigation, quick-launch shortcuts, ID stability, visible-row virtualization, drag/swap controls and action counts. No expensive painter work for offscreen entries.
- Audit grid/list geometry cache after any spacing/padding change. Recompute only on effective layout revision, not palette toggles; test 0/1/10/1k/10k results or realistic fixture sizes already in Track C.
- Verify scroll anchoring, wrap and truncation; measure UI paint/gather and geometry cold/warm duration; compare to M2-D before accepting.
- **Exit:** main launcher has coherent list/grid hierarchy, no altered usability or performance. **Commit:** `M4-B: restyle list and grid without virtualization regressions`.

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
- Validate 100/125/150/200% DPI, minimum usable window size, viewport restoration/position and keyboard tab/escape behavior.
- **Exit:** system feels cohesive at settings level and no setting value changes unexpectedly. **Commit:** `M5-A: polish settings and theme editor surfaces`.

### M5-B — High-usage plugin dialogs

- Inventory visual inconsistency in notes, clipboard, MKMacro, Multi Manager, file search, radial editor (the **egui editor UI only**, not native radial skin), mouse settings and regex tester. Prioritize visible frequency and reuse.
- For each dialog: standardize title/header, option hierarchy, labels, input background/stroke, high-contrast selection, separators, button placement, notification display, scroll reachability and content padding. No data model or command behavior modifications.
- Preserve large editing canvases, macro image capture overlays, regex highlights, notes markdown syntax and file preview. Do not break overlays/highlight palettes that have domain-specific meaning; create controlled semantic token mappings where appropriate.
- Track dialogs in a checklist by path, QA outcome and screenshot pair. Commit every 1–3 related dialogs rather than batching dozens into opaque diffs (`M5-B1`, `M5-B2`, ...).
- **Exit:** core plugin experiences visually consistent, tested and performance-safe. **Commits:** `M5-B1..n: unify [dialog group] presentation`.

### M5-C — Remaining dialogs, messages and secondary windows

- Cover less-used utilities: JSON formatter, QR, calendar, screenshot/image crop, color picker, aliases/bookmarks, recovery, confirmations, notifications/toasts and standalone viewports.
- Audit that secondary viewport theme inheritance is correct on creation and subsequent OS-mode/draft changes. Ensure close/reopen and settings dialogs don't reset parent window position.
- Maintain toast positioning, stacking and lifecycle. Avoid deeply layered shadows, subtle text under low contrast, or inaccessible action states.
- Create surface coverage ledger enumerating all dialog source files, mark migrated/intentional exception/deferred with reason. Do not claim full coverage without validating list.
- **Exit:** broad design consistency with no behavioral change. **Commits:** `M5-C1..n`.

### M6-A — Optional advanced effects as constrained opt-in

- User allowed optional advanced graphical effects. Limit to safe static or bounded features: optional modest elevation shadow, highlight/focus accent or bounded 100–150ms transitions **only if** measurements verify no meaningful regression and no continuous idle repaints. Avoid acrylic/blur/translucent composition initially.
- If implementing transitions, add `enable_visual_motion` default false, respect reduced-motion Windows setting when feasible, schedule repaints only for active visible transition windows, no global timer/polling, and test that idle repaint stops.
- Prefer skipping any effect that needs invasive native window handling, multi-pass rendering or significant shader pipeline changes. Document deferred effect ideas rather than coding them.
- Test low-power laptop, Wgpu/Glow selected backend, and RDP. Keep advanced features under feature/runtime switch as appropriate, default off.
- **Exit:** either a small fully validated optional improvement or explicit justified omission, never a hidden performance tax. **Commit if implemented:** `M6-A: add bounded optional visual effects`.

### M6-B — Final visual, accessibility and performance acceptance

- Compare final UI to original screenshot matrix and M2-D migration-only baseline across root, theme editor, settings, common plugin dialogs, light/dark/custom, DPI and monitor locations. Check high contrast and warning/error/disabled semantics. Make the application clearly more polished without sacrificing density.
- Performance comparisons must include three states: original 0.27 baseline, new 0.36 with old look (M2-D), and new 0.36 with full theme (final). Separate API/backend effects from skinning effects. Repeat matched tests and report median, spread, sample count and % delta; do not claim “no regression” when confidence is inadequate.
- Run complete Windows manual regression and automated suites. Ensure 0 idle style work and idle repaint behavior; inspect allocated heap at steady state, image texture uploads, GPU/cold launch; performance regressions get profiled and fixed.
- Check persistence with old settings files, newly saved themes, cancel preview, application restart, OS mode flip and layout cache revision consistency.
- Reviewer independently samples high-risk workflows and evaluates metrics and behavioral parity. If acceptance fails, use `git bisect`/checkpoint isolation to narrow issue; do not mass revert unrelated changes.
- **Exit:** signed-off functional and aesthetics gate. **Commit:** `M6-B: finalize visual migration validation`.

## 6. Performance protocol and acceptance gates

### Required measured scenarios

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

### Performance judgment

- Primary rule: **no meaningful reproducible regression**. No arbitrary unconditional “under 5% is okay.”
- Record multiple comparable samples and baseline noise. Flag any statistically distinguishable degradation in latency, visible UI input responsiveness, allocation, or idle behavior even if absolute delta looks small; triage effect sizes and measurement artifacts.
- Treat a new constant idle repaint loop, repeated per-frame texture upload, unbounded layout cache growth, lost virtualization, or broken hotkey latency as automatic failures irrespective of median headline numbers.
- Whenever platform-specific test isn't executable on available agent, report it “not run” and give a precise reproducible manual validation script. Never invent Windows pass results from Linux `cargo check`.
- Keep output receipts and command logs in `docs/plans/egui-modernization/measurements/` or an appropriately ignored artifact directory. Record actual environment and checksum.

### Test matrix minimum

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

**Blockers:** Missing Windows toolchain, incompatible third-party egui wrappers, changed behavior in native event/viewport handling, or >noise runtime regressions must trigger root-cause analysis and workaround. If truly unsolved, produce an honest blocker report and the maximal green, reviewable code state. Do not fake a completed theme rollout. Because the user expressly requests back-to-back work, once migration gate passes, proceed to Track B **without requesting another confirmation**.

**Final handoff must include:** SHA and branch; exact egui and renderer versions; toolchain; dependency changes; API migration summary; cache invariants; added theme capabilities; list of styled surfaces; old vs migration-only vs final benchmark table; tests actually run; unrun Windows scenarios; limitations; screenshots or clear QA references; opt-in visual effects behavior; rollback commits; documented known issues; follow-up wishlist clearly separated from completed work. Summarize improvements and risks without exaggeration.

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

For benchmarks, use existing Track C PowerShell recipes and their documented fixture contracts from `docs/performance`; run matched baseline and candidate builds with same host, flags and workload. For backend feature comparison, consult `cargo tree -e features` and eframe 0.36.2 feature list before specifying features. Never assume `cargo nextest` exists on an agent or that Windows-specific tests can run on Linux.

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

- [ ] M0-A preflight and branch
- [ ] M0-B golden baseline and screenshots
- [ ] M0-C API/dependency worklist
- [ ] M1-A dependency graph
- [ ] M1-B lifecycle and startup
- [ ] M1-C widget APIs
- [ ] M1-D toast/markdown/texture integrations
- [ ] M2-A cache migration and Track C compatibility
- [ ] M2-B Win32 / viewport behavior
- [ ] M2-C backend benchmarking and decision
- [ ] M2-D **migration-only gate** accepted
- [ ] M3-A semantic theme model
- [ ] M3-B actual runtime application
- [ ] M3-C live preview and presets
- [ ] M3-D typography and icons
- [ ] M4-A root search polish
- [ ] M4-B list/grid polish
- [ ] M4-C common UI primitives
- [ ] M5-A settings and theme editor
- [ ] M5-B plugin dialogs incrementally
- [ ] M5-C remaining dialogs/secondary viewports
- [ ] M6-A bounded optional effects (if accepted by evidence)
- [ ] M6-B final performance / functional / visual signoff
- [ ] `docs/plans/egui-modernization/handoff.md` complete and checkpoint branch pushed if allowed

**Done means both modernization and visual improvements are completed, reproducible, performance-validated, and reviewable.**
