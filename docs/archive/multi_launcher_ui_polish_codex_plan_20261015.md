# Multi Launcher — UI Polish, Theme Catalog, Live Hotkeys, and Result Flicker

**Implementation plan / Codex execution contract**  
**Reference snapshot:** `multi_launcher(20261015-133847).zip` (October 15, 2026)  
**Archive SHA-256:** `1c8628388a97af9658b2922fd3797db669de36f9de1049141415fa8`  
**Status:** Approved requirements; implementation pending.  
**Scope:** Windows / Rust / eframe-egui application. No implementation has been performed by the author of this plan.

## 0. Binding decisions — do not re-request clarification

All recommendations in the October 15 questionnaire were approved. The resulting behavioral contract is:

1. **MkMacro**: master Macro Properties fold; individually collapsible Identity, Activation, Playback, Recording Toggle, and Reusable Signature sections; Expand All / Collapse All; Focus Steps; compact selected-macro summary. Default properties **collapsed** for a new preference profile, but thereafter honor the last saved layout. Persist the *presentation* settings across macro changes, dialog close/reopen and application restart, separately from serialized macro definitions. Preserve the existing step-block folding implementation.
2. **Curated palettes**: target **54 built-in definitions** (48 dark, 6 light), keeping all six existing presets and IDs; emphasize OLED black, charcoal, slate, blue-black and tinted dark surfaces with visibly distinct accents; unique human-readable names, grouped selectors, swatches/previews, and readable text. Do not populate the user's saved-preset map with every built-in theme up front.
3. **Launcher header**: independent `Show Launcher Icon` and `Show Launcher Title` settings under Launcher Window & Appearance, both on by default. Remove all brand-row space when both are off; do not touch the native title-bar/taskbar icon or menu bar. Apply after Save without restarting.
4. **Typography**: Overall Text Size under Theme Settings > Text and links, **75%–200%**, default **100%**, with reset. Live theme preview, Cancel restores committed theme. Apply across standard egui text roles/surfaces; keep Query Scale and List Scale independent. Associate text size with saved named palettes; switch palettes restores their saved size. Provide explicit **Save As Palette** / **Update Palette** actions so a preview/edit does not silently overwrite a named palette. Do not expand this phase to per-role advanced size controls or separately rendered native radial/cursor overlays.
5. **Hotkeys**: after pressing Settings **Save**, launcher shortcut and radial shared tap/hold, enabled state and threshold become effective in the running process; do not apply on every keystroke in the settings textbox. Show failures, preserve the previously working binding on failure, preserve startup and radial routing semantics, and keep runtime and persisted settings consistent.
6. **Query-list blink**: use the two supplied recordings as reference. Find and fix the responsible rendering/layout behavior, including stationary short lists and long three-column results; do not add a cosmetic 'disable effects' switch or periodic repaint as a workaround. No speculative root-cause assertion based only on a video.
7. **Quality**: `rustfmt` + targeted `cargo clippy` and Nextest at relevant checkpoints; no newly introduced lint warnings and no new blanket `allow`. Fix existing warnings in touched code when practical; log unrelated inherited warnings as baseline debt, without widening the refactor. Verify meaningful native Windows UI behavior.
8. **Workflow**: dedicated new branch from the **matching current HEAD**; no automatic master reset/rebase/pull. Planner / Implementer / Reviewer roles. Active Checkpoint Commit Cadence with sequential writes and checkpoint commits. No push or history rewriting without explicit user permission. Slow Windows builds/tests may take many minutes; avoid tight polling.

## 1. Repository facts observed in the uploaded snapshot

These are **read-only source observations**, not evidence of successful runtime fixes:

- `Cargo.toml` uses **eframe 0.36.2**, with local `[patch.crates-io]` overrides for `egui`, `eframe`, `epaint`, `image`, and `egui-toast`. Preserve Glow and the vendored patches; an egui migration is **not** part of this plan.
- `AGENTS.md` prioritizes ownership boundaries, targeted verification, sequential write agents, meaningful reviews, and no automatic full-suite testing. `.codex/agents/{planner,implementer,reviewer}.toml` already exists.
- `src/gui/mkmacro_dialog/macro_properties.rs::show` lays out the existing Record Toggle, Name, Description, Enabled, Folder, Hotkey, Hotkey Scope/Matcher, a Playback Settings `CollapsingHeader`, and Reusable Signature editor. `src/gui/mkmacro_dialog/mod.rs` around the main editor's `StripBuilder` uses a properties `ScrollArea` with available height reduced by `step_table::MIN_TABLE_VIEWPORT_HEIGHT` and the runtime inspector. `src/gui/mkmacro_dialog/folding.rs::FoldState` folds **macro action/structural steps**, not property groups.
- `src/gui/theme_settings_dialog.rs` defines `CURATED_PRESETS` and `curated_scheme` with exactly six built-ins: `graphite-dark`, `graphite-light`, `slate-dark`, `slate-light`, `ocean-dark`, `ocean-light` (preserve IDs). Selecting a curated palette currently inserts it into `draft.named_presets` under a collision-safe `curated.{id}` name; manual color edits copy to `custom_scheme` instead of editing the original saved scheme.
- `src/settings/model.rs`: `ThemeSettings.named_presets: HashMap<String, ColorScheme>` and `ThemePreferences.typography_scale: f32` (normalizer already clamps to 0.75–2.0). `src/gui/theme/mod.rs::metrics_from_preferences` already uses the scale, updates standard egui text sizes and maintains style identities. The missing pieces are an editor control and per-saved-palette association.
- `src/gui/theme_settings_dialog.rs::ThemeSettingsDialogState` already implements a draft, preview, Cancel, and conflict-checked `Settings::update` path. Extend this; do not replace it with an independent theme publishing system.
- `src/gui/render.rs::render_root_ui` unconditionally renders `paint_brand_badge` plus a `"Multi Launcher"` label. Its toast anchor and existing root UI tests use brand/header geometry. Changes must update the anchor and tests when branding is hidden.
- `src/settings_editor/{state.rs,mapping.rs,render.rs,save.rs}` owns general settings field staging and save/application. `src/settings/model.rs::Settings` owns persisted settings.
- **Concrete hotkey wiring defect:** `src/settings_editor/save.rs::apply_saved_settings` calls `crate::request_hotkey_restart(new_settings)` in the library. `src/lib.rs::request_hotkey_restart` is a **no-op**, while the working channel sender is a separate `src/main.rs::request_hotkey_restart`. The runtime loop's `restart_rx.try_recv()` branch is in `src/main.rs`. Merely making a notification arrive is not enough to satisfy failure rollback and acknowledgment requirements.
- `src/hotkey/runtime.rs::HotkeyTrigger::start_listener` polls keyboard state for the legacy route and does not return a registration `Result`; `src/hotkey/launcher_invocation.rs` owns the more complex shared/radial path. Do not assume all failures are OS `RegisterHotKey` failures; detect unsupported legacy key mappings and shared hook/route startup errors appropriately.
- `src/gui/render.rs` owns `RootListGeometryCache`, `root_grid_cell_response`, `root_grid_row_color`, and the `ScrollArea::vertical().show_viewport` query results. There are existing geometry tests, including `root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, `root_list_and_grid_geometry_follow_consumer_font_activation`, and `root_grid_geometry_reuses_warm_cache_and_tracks_grid_layout_inputs`. The `qa-framebuffer-capture` feature and `src/gui/qa_framebuffer.rs` are available for visual diagnostics if useful.
- `src/lib.rs` contains historical crate-wide `allow` directives. The project may have pre-existing Clippy debt and expensive Windows builds. Do not erase broad allowances as an incidental change.

## 2. Git, baseline, and checkpoint contract

**Preflight — before modifying source**

1. Read `AGENTS.md`, `Cargo.toml`, and this plan. Inspect `git status --short`, `git branch --show-current`, `git rev-parse HEAD`, and relevant files. Preserve any uncommitted work without reset/stash/discard unless separately authorized.
2. Confirm the repository contents match the provided October 15 snapshot **for the affected files**. A zip snapshot does not prove a Git commit ID; never invent one. If a different checkout has moved ahead, prepare a concise diff of the affected modules and reconcile safely. If the divergence changes accepted behavior or ownership materially, stop and ask for the correct baseline.
3. From the verified **current matching HEAD**, create a new branch such as `ui-polish-20261015` (append a safe suffix if it already exists). Do not switch to, fetch/rebase from, or pull `master` automatically. Do not push until authorized.
4. Record baseline project version, Windows target/toolchain, and any **already observable** format/Clippy warnings that would prevent a strict clean gate. Do not run extensive benchmarks or a full suite merely to set a baseline.
5. Save plan / kickoff copies under project `docs/plans/` *if useful and approved by repository conventions*, without clobbering existing documents. The milestone list below is the ledger; mark complete only after relevant checks pass.

**Active Checkpoint Commit Cadence**

- One implementer writes at a time; follow each meaningful milestone with its own Conventional Commit, using the label in the commit body (e.g. `Checkpoint: UP-HK`). Split a genuinely large milestone into adjacent A/B commits only when the first part is coherent, compiling, and independently reviewable.
- For each: inspect diff; format; targeted Clippy; targeted Nextest; independent Reviewer for high-risk architectural changes; then commit. Preserve unrelated work. On failure, fix or explicitly block; do not commit a knowingly broken intermediate state as 'complete'.
- Use the repo's existing `.codex` agents: high-reasoning Planner for bounded preparation/adjustments, Implementer for each single milestone, read-only Reviewer to inspect meaningful risky diffs. Never have parallel implementers editing overlapping files.
- Report milestone progress periodically, preferably on completion or substantive blocker. Prefer process completion signals over polling; if polling of a lengthy remote test is unavoidable, check **roughly every 10–20 minutes**, not every few seconds. Do not interrupt a sound compile merely because it takes 15–20 minutes.
- Never require exact historical performance parity. Prevent avoidable repeated work and serious regressions; a small UI rendering cost may be acceptable for the new look. A future benchmark is a separate task.

## 3. Ordered implementation milestones

### UP-HK — Runtime-safe, no-restart launcher hotkey reconfiguration

**Objective:** Fix the GUI-to-main runtime communication defect and make Save produce an acknowledged, safe hotkey/tap-hold reconfiguration. Highest functional risk; do this first.

**Owners / relevant code:** `src/settings_editor/save.rs`, `src/lib.rs`, `src/main.rs`, `src/hotkey/runtime.rs`, `src/hotkey/launcher_invocation.rs`, `src/settings_editor/{state.rs,mapping.rs}`, `src/settings/model.rs`; directly related tests in `src/main.rs`, `tests/hotkey_events.rs`, `tests/settings_editor.rs`, and `tests/trigger_visibility.rs`.

**Required work**

1. Trace current Settings Save, the `lib.rs` no-op, the binary's `RESTART_TX` and `EVENT_TX`, the main loop `restart_rx.try_recv()` block, and shared radial route handoffs. Record every existing valid side effect during reconfiguration: launcher/quit/help triggers, Screen Draw, radial enablement, macro reservations, native controller, tooltip preferences, and pending handoff generation.
2. Define one **library-visible, runtime-installed reconfiguration boundary** with typed request, identity/generation, wake-up, and explicit `Result` completion/acknowledgment. The main runtime owns the receiving endpoint. Remove/replace the no-op and avoid a second global implementation with the same logical responsibility. Use the existing channel architecture where appropriate; do not make `lib` depend on `main`.
3. Decouple *settings have been written* from *new hotkey is active*. Prefer preparation/validation before persistent commit, followed by an acknowledged runtime transition; alternatively implement explicit rollback of both runtime and persisted hotkey-related settings if the runtime fails. Preserve concurrent `Settings::update` semantics and unrelated fields in the persisted file. A screen-wide save toast must not claim full success until the runtime operation has succeeded.
4. Validate parsed key chord *and its availability in the selected route*, not just `parse_hotkey != None`. The legacy polling listener only maps a limited key set; unsupported chords must be rejected with a useful error instead of being accepted but nonfunctional. Validate conflicts with quit/help, macro reservations, Screen Draw and radial direct triggers using existing validators.
5. Reconfigure the old and new routes in a **failure-safe order**. Maintain/restore the old usable launcher hotkey if hook installation, route handoff, or listener preparation fails. Avoid any state where both routes accidentally activate a single press. Preserve cancellation of an in-flight hold and stale generation suppression; prevent double toggling on release.
6. Ensure transitions work in **both directions**: shared tap/hold on/off, radial on/off, changed hold threshold, modified launcher binding, and unchanged settings. No full application restart; no unexpected radial opening while grid is visible.
7. Integrate errors into existing GUI `UiErrorEvent`/toast paths. Bound any acknowledgment wait and avoid blocking the GUI or deadlocking the main loop. If the runtime owner is unavailable, report failure and leave old effective settings rather than lying about success.
8. Write deterministic tests around the typed request/response and saved/effective state; add Windows manual tests for actual input handling. Keep other settings behavior unchanged.

**Critical invariants:** short tap toggles launcher on release (including when focused); hold exceeding configured threshold opens radial where enabled; release after hold does **not** toggle grid; current grid visible stays untouched by hold; Screen Draw and exclusive owner routing remain authoritative; quit/help remain functional; failed key change leaves old binding functional and not overwritten in persisted settings.

**Scoped verification:** formatting; targeted hotkey/editor tests (e.g. `cargo nextest run -p multi_launcher -E 'test(hotkey) | test(launcher_invocation) | test(trigger_visibility)'`), and a relevant main binary test group, with a focused Windows smoke test. Strict Clippy of touched targets if baseline permits. Do not run all GUI tests simply because `main.rs` changed.

**Done when:** both settings visibly apply after Save without relaunch; error path preserves old binding **and** coherent disk/runtime state; no duplicate trigger event; relevant tests and Reviewer findings resolved.

**Checkpoint:** `fix(hotkey): apply launcher and radial invocation settings live` (body `Checkpoint: UP-HK`).

### UP-MK — MkMacro property folding, Focus Steps, and persistent UI preferences

**Objective:** Make macro-step modifications take most of the editor's central column on demand, without losing editing capabilities or changing the macro format.

**Owners / relevant code:** `src/gui/mkmacro_dialog/{mod.rs,macro_properties.rs,step_table.rs,signature_editor.rs,window_matcher_editor.rs}`, settings storage via `src/settings/model.rs` and `src/settings/store.rs`, dialog construction in `src/gui/mod.rs`; MkMacro test modules in the dialog and `tests/mkmacro_authoring.rs` / `tests/mkmacro_store.rs`.

**Required work**

1. Add an explicit typed **Macro Properties presentation state** separate from `folding.rs::FoldState` (which must remain dedicated to structural steps). Proposed groups:
   - **Identity:** Name, Description, Enabled, Folder.
   - **Activation:** Hotkey, Hotkey Scope, active-window matcher/window picker.
   - **Playback:** Speed, random delay, random position offset; migrate existing `Playback Settings` fold control into the unified group state.
   - **Recording:** document-wide Record Toggle and any related diagnostic; must be usable even if no macro is selected.
   - **Reusable Signature:** the current signature editor for inputs/outputs.
2. Add master `Macro Properties` collapse/expand, individually collapsible group headers, and `Expand All` / `Collapse All`. Clicking these controls changes **only UI state**. No `mark_dirty`, no macro authoring undo entries, no `MkMacroDocument` schema bump.
3. Implement `Focus Steps` at the top of the center editor pane. When active, hide all expanded property bodies and show a small, informative selected-macro strip (name, enabled state, optional folder/hotkey labels). Switching back restores the prior per-group expansion state, without losing it. The master collapsed view should also retain a compact summary. Do not use the same egui memory IDs as action/step fold states.
4. For first-ever use, initialize properties **collapsed**. After a user changes the layout, restore that last-used preference on selection, dialog reopening, and a new app process. Determine whether to persist master-open, group-open map and Focus Steps independently; avoid ambiguity by documenting precedence: Focus Steps effective-hide > master-open > group-open.
5. Persist preferences in an independent, serde-defaulted UI settings structure under settings (`settings.json`/typed settings), **not** in MkMacro macros/folders. Prefer one typed field of `Settings` (or a dedicated UI-pref store already in the actual checkout) rather than piggybacking transient draft dirty tracking. Persist on meaningful changes/close, not every painted frame; use the atomic `Settings::update` path and avoid overwriting concurrent settings edits.
6. Rework `macro_properties::show` into section functions as appropriate to keep borrow scopes, `selected_macro_mut` handoff, hotkey capture, and matcher/Windows picker safe. Preserve hotkey errors and diagnostics; do not let a collapsed capture mode retain an invisible capture prompt indefinitely. Expand the necessary group for validation errors when useful without overriding persistent user preferences unexpectedly.
7. In `mod.rs`, ensure the existing `ScrollArea` shrinks to the visible properties content and the `step_table::show` viewport reclaims height. Honor `step_table::MIN_TABLE_VIEWPORT_HEIGHT`, the independently opened runtime inspector, resize/minimum bounds and center-column width. Do not create a second scroll container around the step table or force unnecessary viewport changes every frame.
8. Preserve macro selection, selected step, range selection, edit dialogs, document conflict/dirty logic, undo/redo, structural step folding, recording state, and keyboard navigation when panels open/close or macros switch.
9. Add tests: default folded state; expand/collapse semantics; Focus Steps enter/exit restores group choices; preferences survive serialization and simulated dialog creation; old settings load with defaults; no document dirty revision on view toggle; recorded toggle works with no selected macro; step table gets larger drawable height and does not lose its selected-step identity.

**Critical invariants:** no persisted change to `MkMacroDocument`; no interference with `folding.rs::FoldState` or macro list folder-collapse state; folder assignment, hotkey editing, window matcher, playback, and signature editing all work when groups expand; no stuck key-capture after folding.

**Scoped verification:** format, targeted `mkmacro_dialog` unit tests, `tests/mkmacro_authoring.rs` where relevant, Clippy on changed code, one manual MkMacro window resize/selection/edit/save scenario.

**Done when:** default collapsed layout gives visibly more room for row modification; toggles and Focus Steps restore faithfully after relaunch; macro saved output is identical if only folding controls were used.

**Checkpoint:** `feat(mkmacro): persist collapsible properties and focus-steps layout` (body `Checkpoint: UP-MK`).

### UP-BR — Independent main launcher branding visibility

**Objective:** Allow users to hide the icon and/or `Multi Launcher` text over the query field, with no leftover blank row when both are hidden.

**Owners / relevant code:** `src/settings/model.rs`, `src/settings_editor/{state.rs,mapping.rs,render.rs,save.rs}`, `src/gui/mod.rs` (runtime LauncherApp fields/init), `src/gui/render.rs::render_root_ui`, theme brand renderer in `src/gui/theme/mod.rs`; existing UI geometry/toast tests in `src/gui/render.rs`.

**Required work**

1. Add serde-defaulted persisted bools `show_launcher_icon` and `show_launcher_title` (names may follow current Settings convention) defaulting to `true`; test legacy JSON default behavior. Keep this separate from Windows-native window/icon settings.
2. Expose both switches together under the existing **Launcher Window & Appearance** settings section, not Theme Settings. Stage them in `SettingsEditor`, map in `from_settings` and `to_settings`, and immediately publish the committed values into `LauncherApp` on successful Save.
3. Render each header element conditionally. When icon and title are both off, skip the brand horizontal row entirely (no label, icon allocation, inner/outer vertical spacing, or placeholder). When only one is on, size it naturally. Keep the menu bar and query editor.
4. Recalculate header/toast anchor without assuming `brand.response.rect` exists. Use the greatest relevant bottom of menu, optional branding, and query, and continue to respect inline error placement. Update tests that currently assume the brand text and mesh always exist; retain tests for the default legacy-on mode.
5. Verify all four combinations and settings persistence across restart, with query focus, narrow window dimensions, non-default Query Scale and DPI scaling. Confirm no resizing/surprise query overlap when hiding the row.

**Critical invariants:** native title-bar text and taskbar icon stay unchanged; File/Settings/Help remain visible; no startup behavior changes; default appearance is identical to current version.

**Scoped verification:** targeted settings and root header/toast geometry tests plus a short Windows visual smoke. No unrelated GUI refactors.

**Done when:** icon-only, title-only, both, and neither work immediately after Save and after relaunch, and the query moves up when neither is shown.

**Checkpoint:** `feat(launcher): make query header icon and title optional` (body `Checkpoint: UP-BR`).

### UP-PA — Expand and organize curated theme palettes

**Objective:** Replace a six-item preset dropdown with a useful, carefully differentiated **54-palette curated library** without breaking custom theme persistence.

**Owners / relevant code:** `src/gui/theme_settings_dialog.rs::{CURATED_PRESETS,section_presets,curated_scheme}`, `src/gui/theme/mod.rs`, color model `src/settings/model.rs::ColorScheme`. A bounded `src/gui/theme/presets.rs` or equivalent is appropriate for the expanded static catalog.

**Curated palette catalog (54 unique human labels; names indicative but use these unless a collision is found)**

| Family | Dark / light | Curated palette names |
|---|---|---|
| **OLED / True Black** | 12 dark | Obsidian Lime; Obsidian Ember; Obsidian Amber; Obsidian Gold; Obsidian Cyan; Obsidian Teal; Obsidian Azure; Obsidian Ruby; Obsidian Rose; Obsidian Violet; Obsidian Silver; Obsidian Ice |
| **Charcoal / Graphite** | 12 dark | Graphite Dark (**existing**); Charcoal Mint; Carbon Orange; Forge Yellow; Night Steel; Smokestack Gold; Gunmetal Cyan; Ink Cobalt; Ash Burgundy; Onyx Orchid; Coal Blush; Carbon Frost |
| **Midnight / Slate** | 12 dark | Slate Dark (**existing**); Navy Neon; Starless Green; Abyss Orange; Midnight Lantern; Midnight Turquoise; Ocean Dark (**existing**); Blue Hour; Dusk Raspberry; Storm Plum; Twilight Ice; Steel Lantern |
| **Tinted / Atmospheric Dark** | 12 dark | Forest Terminal; Moss Ember; Espresso Gold; Rust and Ember; Midnight Copper; Merlot Rose; Aubergine Frost; Arctic Graphite; Vaporwave Dark; Synthwave Noir; Jungle Neon; Infrared Night |
| **Light / Accessible Counterparts** | 6 light | Graphite Light (**existing**); Slate Light (**existing**); Ocean Light (**existing**); Parchment Amber; Frost Lime; Cloud Violet |

**Required work**

1. Keep the six existing *IDs and underlying schemes* compatible (existing `graphite-...`, `slate-...`, `ocean-...` IDs). Name the other 48 with stable lowercase kebab-case IDs. Do not rename a user's previously saved `curated.{id}` entries or overwrite a colliding user-defined key.
2. Move the growing preset catalog to a compact **data-driven definition/recipe registry**. Each preset defines a purposeful background family and distinct accent treatment; do not just mutate one accent channel while retaining incompatible selection/hover/link colors. Ensure background, panel, inactive/hover/active, text, hyperlink, selection and semantic tokens are coherent.
3. Distinguish **true black** (`#000000`) backgrounds from near-black and dark tinted backgrounds, and make accents recognizable at a glance (green/lime, orange/amber/yellow/gold, teal/cyan, electric blue, red, violet, pink, silver/white). Keep foreground text legible and widget boundaries visible; guard accidental alpha issues and theme/color mismatch.
4. Redesign `section_presets`: retain `Saved palette` selection; provide a grouped browse control / lightweight gallery for curated entries (group tabs/categories and swatch cards or grouped combo with visible swatches). Within ordinary dialog widths the catalog must be navigable without excessive scrolling or causing dialog layout jitter. Use egui style/theme APIs already in use; maintain keyboard navigation and focus.
5. Preview curated colors as a draft on selection, respecting the existing live preview/Cancel/save contract. Do not populate the saved map until the user explicitly chooses a curated preset under existing semantics (and never bulk-seed all 54). If selection uses the historical collision-safe `curated.{id}` insertion, preserve its collision protections; ensure repeat selection is idempotent.
6. Preserve legacy System, Dark, Light, and Custom modes, saved-palette selection, accent override, independently edited scheme colors, and existing preview update flow.
7. Write data-level tests for exact 54 count; 48 dark / 6 light grouping; unique IDs/labels; stable six old IDs; distinguishable palettes; coherent readable contrast; valid serde and collision-safe curated selection; no unintended map changes when merely browsing.

**Non-goals:** downloading community themes; remote gallery; user-customizable preset tags; global Windows accent integration; repaint animation; a larger theme engine migration.

**Scoped verification:** theme catalog/selection tests and one UI layout smoke at narrow and wide dialog widths. No need for an application-wide performance benchmark.

**Done when:** 54 distinct, reliably selectable palettes are visible, previews are usable, old palettes/user colors survive, and serialization behavior remains stable.

**Checkpoint:** `feat(theme): expand curated palettes and grouped previews` (body `Checkpoint: UP-PA`).

### UP-TX — Overall typography size persisted per named palette

**Objective:** Expose already-existing typography scaling in Theme Settings and bind that size to saved palette choices without breaking the historical `named_presets` format.

**Owners / relevant code:** `src/settings/model.rs::{ThemeSettings,ThemePreferences,ColorScheme}`, `src/gui/theme/mod.rs::{resolve_theme,metrics_from_preferences,ThemeRuntimePlugin}`, `src/gui/theme_settings_dialog.rs::{ThemeSettingsDialogState,render_theme_primary_sections,section_text_and_links,section_presets,save_to_path}`, root text scaling tests in `src/gui/render.rs`.

**Required work**

1. Preserve the installed egui text-style runtime as the sole owner of standard font metrics. `ThemePreferences::MIN_TYPOGRAPHY_SCALE` and `MAX_TYPOGRAPHY_SCALE` already equal 0.75 and 2.0; reuse them. No new font loading, no multiplying an already-applied style, no reallocation or reinstallation of fonts on every frame.
2. Place an **Overall Text Size** slider (75%–200%, sensible increments) plus a visible **Reset to 100%** action **inside `Text and links`**. The default must reproduce the October 15 sizes. Show the current percentage numerically.
3. Extend theme persistence to associate a normalized `f32` scale with each named palette. **Recommended migration**: add a serde-defaulted sidecar `HashMap<String, f32>` keyed by the existing `named_presets` names. Keep `named_presets: HashMap<String, ColorScheme>` unchanged so old JSON files and tests do not require wholesale enum/wrapper migration. If an alternative typed representation is chosen, prove transparent deserialization of all legacy `ColorScheme` values and no data loss.
4. Implement a deterministic **legacy scale migration/fallback**: old JSON without a per-palette scale still reads; the previously saved global `preferences.typography_scale` remains visually effective until the user explicitly chooses or updates another palette. Existing named palettes receive sensible inherited defaults instead of silently resetting the user's old font size. Curated themes start at 100% unless explicitly saved differently. Reject/clamp NaN, infinities, or out-of-range values at the effective theme resolver boundary (and UI on save) without destructively rewriting raw configuration on mere load.
5. On selecting a named palette, restore both color scheme and associated type scale. On editing colors/scale, preview a **draft copy**. Add `Save As Palette` (new name with collision validation) and `Update Palette` (explicit opt-in to replace selected name's saved colors/scale). General Apply/Save should not silently overwrite an existing named palette; a modified unsaved theme may be saved as Custom if that matches current editor semantics. Keep mode switching sane.
6. Handle Reset/Cancel carefully: Reset section resets size to 100%; full Theme Reset must retain pre-existing named colors *and their scale sidecar data* when preserving saved palettes; Cancel must restore last committed style even after multiple live slider/palette previews. Applying with a stale dialog must reject conflicting edits using existing `Settings::update` baseline comparison.
7. Ensure standard egui roles (Heading, Body, Small, Button, Monospace) scale coherently across launcher query, results, Settings, Theme editor, MkMacro properties/step editor, Notes, other dialogs and menus. Audit local `RichText::size` or absolute text-size uses in the directly affected standard egui surfaces, fixing those that prevent the user-requested overall scale. Avoid forcing separate native radial/cursor overlays to inherit fonts this phase.
8. Preserve the separate Query Scale and List Scale consumers. Verify combined factors produce deliberate, non-compounding sizing and valid row heights; do not make typography scale rewrite those settings. Check the theme identity/cache invalidation so a scale change refreshes text/layout exactly as required, but not infinitely on every frame.
9. Add tests: historical settings deserialize unchanged; legacy preset with non-default global scale remains legible; 100% is baseline; named palette A at 85% / palette B at 140% restore independently; Save As/Update are explicit; collisions reject; preview/Cancel restores; full reset preserves saved scale map; font/style identity and geometry changes stabilize; no accidental double-scaling.

**Critical invariants:** no loss/rename of saved colors, no automatic overwrite of selected palette, stable font families and style slots, no new native overlay dependency, no query-list text clipping at 75% or 200% without expected overflow handling.

**Scoped verification:** targeted theme/unit persistence, `root_list_and_grid_geometry_follow_consumer_font_activation`, and one native theme/size cycle on MkMacro + launcher. Format and scoped Clippy.

**Done when:** each named palette restores its own text size across app restarts, Cancel is true rollback, and legacy settings retain the previous effective text size.

**Checkpoint:** `feat(theme): persist font scaling with saved palettes` (body `Checkpoint: UP-TX`).

### UP-FL — Eliminate bottom-edge query-list blinking at its source

**Objective:** Reproduce and eliminate the result-list/grid visual flicker reported in two October 15 screen recordings, without suppressing theme styling or incurring idle repaint loops.

**Owners / relevant code:** primarily `src/gui/render.rs::{render_root_ui,RootListGeometryCache,root_grid_cell_response,root_grid_row_color}` and any proven upstream style/viewport owner; optional `src/gui/qa_framebuffer.rs`; targeted renderer tests already present in `src/gui/render.rs`. The uploaded evidence files are `20261015-1344-30.9923877.mp4` (short results) and `20261015-1346-09.9124008.mp4` (three-column results). These files may need to be separately made available in Codex's checkout.

**Diagnostic-first procedure — DO NOT preselect a fix**

1. Reproduce *both* reference cases: stationary pointer/query with approximately two visible results and a long three-column grid; test a dark curated palette and current default, at 100% scale. Confirm whether the effect is a pixel boundary, result row, scrollbar, toast layer, scroll offset, background fill or another visibly identified region.
2. Compare frames while input and data are stationary; if available, use opt-in `qa-framebuffer-capture` rather than continually writing screenshots during every draw. Record clip rects, viewport inner/content dimensions, scroll offset, `root_list_geometry` generation/key/rebuild count, visible row indices, and style identity **only when diagnostic mode is enabled**.
3. Trace `ScrollArea::vertical().max_height(...).show_viewport(...)`, top pixel alignment, `scale_ui(list_scale)`, `ui.expand_to_include_rect`, cached row visual bottoms, row stripes, and `root_grid_cell_response` fixed-height button allocation. Look for inconsistent rounding, clipped overhang, geometries that alternate by one pixel, scroll-bar threshold oscillation, changing row bounds or unnecessary repaint demand. Treat these as investigation leads, **not confirmed causes**.
4. Reduce the reproducer to the first responsible boundary and correct that owner's layout/paint behavior with a minimal change. Do not introduce a permanent FPS timer, blanket `request_repaint` loop, scrolling hack, opaque overlay concealer or theme-effect opt-out. Avoid invalidating all root geometry on idle frames.
5. Verify across short/long grids, list mode, empty results, selected/unselected rows, scrolling to bottom, very tall/wrapped labels, 2–4 columns, 75%/100%/200% overall type size, existing Query/List scales, narrow/wide windows and 100%/125%/150% display scaling where feasible.
6. Add a deterministic regression for the diagnosed root cause, e.g. stable geometry/visible rows/content bounds and scroll offset on settled consecutive frames, or stable framebuffer pixels if this is a paint-only issue. Existing tests already cover row geometry and warm-cache reuse; extend the smallest relevant test rather than building a new general visual QA framework.
7. Capture before/after video or quantitative stationary-frame comparison on **native Windows**. If the effect can only be reproduced under the actual GPU backend, keep the manual requirement explicit and avoid a spurious passing unit test claiming the native issue was solved.
8. Confirm idle request-repaint frequency and CPU behavior do not materially worsen. A small unavoidable one-time repaint after a theme/size change is fine; the app need not achieve exact pre-theme benchmark numbers.

**Critical invariants:** result click targets, keyboard navigation, selection persistence, scroll-to-selected, context menus and popup-owner row retention, theme highlight colors, virtualization/caching and Query/List Scale all remain correct.

**Scoped verification:** focused renderer tests (`root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, `root_grid_geometry_reuses_warm_cache_and_tracks_grid_layout_inputs`, `root_list_and_grid_geometry_follow_consumer_font_activation`) plus native Windows reproducer. Only widen test scope when failure evidence makes it necessary.

**Done when:** the stationary two-result and long-grid reference scenarios no longer blink for repeated observation, the cause and patch are explained, targeted tests pass, and no idle repaint regression was added.

**Checkpoint:** `fix(gui): stabilize result viewport at the bottom edge` (body `Checkpoint: UP-FL`).

### UP-QA — Focused integration, final Clippy gate, and handoff

**Objective:** Validate interactions among the six changes, resolve directly related lint debt and produce a trustworthy final report; no broad redesign or default comprehensive test sweep.

**Required work**

1. Run `cargo fmt --all -- --check`. On the project's Windows builder run strict Clippy on directly affected library/binary/targets first, for example `cargo clippy -p multi_launcher --lib --bin multi_launcher -- -D warnings`. If the repository-wide strict lint check is already clean or inexpensive, run `cargo clippy -p multi_launcher --all-targets -- -D warnings`; otherwise distinguish *baseline inherited* warnings from new/touched-file warnings. Report command, toolchain and outcomes precisely. Do not add `#![allow(clippy::...)]` at crate level to greenwash warnings.
2. Run only an acceptance-aligned Nextest union: relevant theme/settings, root renderer geometry, hotkey/radial routing, MkMacro authoring tests, plus Windows manual checks. Prefer test filters over full `cargo nextest run`. If a newly changed shared abstraction has more consumers, add only the necessary adjacent tests and explain why.
3. Check migration / round trip of old settings JSON, saved named palettes, new typography sidecar and UI preferences; ensure independent settings writes merge and existing macro files are unchanged by view-only actions.
4. Execute compact end-to-end matrix:

   | Scenario | Expected result |
   |---|---|
   | Open MkMacro with new preferences | Properties initially collapsed, step table large |
   | Expand group; enable Focus Steps; disable Focus Steps; relaunch | Prior group choice returns and persists |
   | Hide title only, icon only, then both | Query header updates immediately; no blank row when both off |
   | Pick OLED green and amber/orange palettes | Coherent color/contrast; saved themes untouched until chosen/saved |
   | Save palette A at 85%, palette B at 140%; switch; relaunch | Each restores its own overall size |
   | Preview 200% then Cancel | Committed theme and text size fully restored |
   | Change launcher chord and tap/hold mode; Save | New chord effective without app restart, old deactivated |
   | Attempt unsupported/conflicting chord | Actionable error, old binding still works, saved settings coherent |
   | Stationary short and long query grids, scroll to bottom | No bottom-edge blink at multiple sizes/DPIs |
   | Run scoped Clippy/tests | No new warnings or directly related regressions |

5. Have the Reviewer inspect changed architecture, persistent migration, cross-component state publication, and actual test assertions. Address high/medium findings in the relevant milestone; avoid speculative future-proofing.
6. Inspect `git status`, `git diff --check`, recorded checkpoint commits and remaining user changes. If an integration fix genuinely crosses milestones, make one small final coherent commit with reason (e.g. `test(gui): cover combined theme and launcher controls`), else do not create a no-op commit.
7. Report: branch and base SHA (actual from Git), checkpoint subjects/hashes, files and behavior changed, tests/Clippy commands actually run, native observations, outstanding risks, and any pre-existing lint debt. **Do not claim tests ran if not executed.** Do not push without user authorization.

**Done when:** every acceptance scenario has supported evidence or a clearly identified platform-only validation item; all changed behavior has relevant passing verification; no unresolved severe review issues; branch is ready for the user's merge decision.

## 4. Dependencies and rollback

- **UP-HK** can be implemented first independently; its runtime delivery contract must be confirmed before any other settings save changes rely on it.
- **UP-MK** and **UP-BR** both add settings fields; integrate sequentially to prevent overlapping `settings/model.rs` and `settings_editor` edits. Persisted UI settings must use `#[serde(default)]` to allow old installs to start.
- **UP-PA** establishes the curated palette catalog and selection UX; **UP-TX** then extends palette scale persistence and explicit update semantics. Both must preserve the original six IDs and existing saved values.
- **UP-FL** is scheduled after theme/typography because final text size affects layout. Its root cause may exist earlier; if diagnosis conclusively shows a pre-existing defect, still fix only the proven boundary.
- **UP-QA** consolidates validation, not a license for a broad rewrite.
- Each checkpoint is independently revertible where feasible. When rolling back a data-format addition, new serde-defaulted fields must not corrupt old settings; document any explicit backward-compatibility limitation. Do not use `git reset --hard` or force-push to 'roll back' someone else's work.

## 5. Explicit out-of-scope items

- Replacing egui/eframe 0.36.2, dropping local vendor patches, or migrating the graphics backend.
- Exact old-frame-time / compilation-time parity or a new benchmark project.
- Advanced independent typography settings per text role, native radial menu fonts, or cursor-overlay fonts.
- Theme downloads, generative/unbounded random colors, synchronization/cloud accounts.
- MkMacro step-block folding changes, macro execution/runtime behavior or document schema migration.
- A generic UI-wide animation disable switch to conceal flicker.
- A repository-wide removal of historical Clippy suppressions and unrelated refactors.
- Automatic Git push, reset/rebase/force-push, master update, or destructive cleanups.

## 6. Reporting template for every checkpoint

```text
Checkpoint: UP-XX — <title>
State: pending | in_progress | complete | blocked
Base commit: <SHA>
Result commit: <SHA, subject>
Changed code: <modules and behavior>
Architecture/compatibility: <relevant boundaries and migrations>
Targeted verification: <exact commands, pass/fail and runtime>
Clippy: <exact command, introduced/pre-existing warnings>
Windows manual evidence: <when relevant>
Reviewer: <findings/resolutions>
Known blockers or risks: <concrete items or none>
Next checkpoint: <ID>
```
