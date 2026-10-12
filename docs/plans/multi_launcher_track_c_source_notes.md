# Multi Launcher — Track C Source Investigation Notes

**Purpose:** Evidence-led runtime responsiveness and performance foundations. These notes are a planning reference, **not** a benchmark result and **not** permission to alter production behavior without checkpoint gates.

**Execution baseline:** The **exact `HEAD` of the branch checked out when Codex begins Track C**. Record `git rev-parse HEAD`, branch, dirty status, Rust/Windows host/toolchain, profile, and target configuration **before** creating `runtime-foundations`. Do not assume `master`, an upstream branch, or earlier performance reports are the starting commit. User-approved operations are local only: no push, merge, reset, rebase, force clean, or automatic fetch/pull. Refuse to overwrite uncommitted changes or an existing conflicting branch.

**Source-review provenance:** Reviewed the latest October 10 user-provided project snapshot. Extracted tree was verified against the provided source bytes for `Cargo.toml`, `src/gui/render.rs`, `src/gui/notes_dialog.rs`, `src/gui/note_panel.rs`, `src/gui/search.rs`, `src/gui/mod.rs`, `src/gui/watch.rs`, `src/performance.rs`, `src/indexer.rs`, and `src/coordinate_tool/controller.rs`. It is not a Git worktree; the originating branch/commit cannot be proven from the snapshot. Codex must inspect the actual starting HEAD and adapt paths/signatures to that checkout. **No Track C measurements or code changes were run in this source review.**

## 1. Preservation baseline: Track A and Track B are already implemented

- The current tree contains Track A's versioned note snapshots and cheap idle gate, dashboard history bounded preparation, variable-height root list/grid/Quick Notes virtualization, bounded index coordinator, generation/config guarded publication, and native cursor-effects refresh ordering.
- It also contains Track B's optional `[profile.fast-dev]` and `[profile.iteration]`, two Windows resource `rerun-if-changed` directives, and target-first build/test documentation. Keep those intact; the canonical `release`, default `dev`, and `test` behavior must not be silently changed.
- The existing `src/performance.rs` opt-in metric collector has 15 bounded metric families (`NoteRefreshCheck`, `NoteSnapshot`, `NoteHeavyRecompute`, `LauncherRowsBuilt`, `QuickNotesRowsBuilt`, `ActionsReload`, `IndexScan`, etc.) plus startup/frame logs. `src/performance/workloads.rs` provides reproducible deterministic Track A builders and ignored owner tests. **Extend rather than replace or corrupt historical metric meanings.**
- The recorded **final** Track A benchmark `docs/performance/track_a_final.json` is an historical reference, with **48 scenarios**, not an automatically comparable Track C baseline. Its synthetic Windows debug-test p95 observations include:

| Workload | Historical final p95 | Separate cold observation | Boundary |
|---|---:|---:|---|
| 10,000-result launcher list, warm frame | 0.876 ms | 646.19 ms measuring 10,000 rows | `render_root_frame`; headless egui CPU |
| 10,000-result launcher grid (3 columns), warm frame | 1.432 ms | 580.78 ms measuring 10,000 cells | `render_root_frame`; headless egui CPU |
| 5,000 Quick Notes, empty filter, warm | 0.665 ms | 40.075 ms at standard viewport | `NotesDialog::ui`; headless egui CPU |
| 5,000 Quick Notes, small viewport, warm | 0.509 ms | 627.73 ms at narrow viewport | one narrow cold geometry rebuild |
| 5,000-note draft after edit debounce | 1,509.04 ms | genuine heavy derived recomputation | `NotePanel::maybe_refresh_heavy_derived` |
| Actions reload, 10k custom/10k indexed, changed | 114.558 ms | not a cold equivalent | typed `WatchEvent::Actions` owner |
| Index config change, 10k indexed publication | 49.520 ms | 10k scan completion p95 1,030.38 ms **off-thread** | publication excludes scan wait |
| History 10k count-8 normal | 0.040 ms | — | already optimized, leave alone |

  Cold timings above are **one observation**, not p50/p95 distributions. Different fixture/profile/commit versions must not be presented as matched before/after comparisons. Native effects/GDI decisions in Track A were already measurement-gated; don't reopen without new evidence.

## 2. Notes/backlinks: verified whole-collection work remains

**Source:** `src/gui/note_panel.rs::refresh_heavy_derived` around line 1225; `::maybe_refresh_heavy_derived` around 1353; `::backlink_rows_for_note` around 4524; `src/plugins/note.rs::NoteCache` around 145; `::note_cache_snapshot_with_version` around 1134.

Current state:
1. The cheap idle check gates genuine recomputation; it is *not* the remaining target.
2. A legitimate enabled refresh captures a full `(revision, Vec<Note>)` with consistent pairing, loads todos, derives the todo label map, and invokes `backlink_rows_for_note` **three times**: `LinkedTodos`, `RelatedNotes`, `Mentions`.
3. Each helper pass loops all todos and all notes, calls `content_without_fenced_code` per note, examines wiki links/entity references/alias string needles, and creates snippets/display rows. The current panel's searchable content is also reconstructed per invocation. A fused traversal with routing into the three ordered result vectors is the lowest-risk algorithmic experiment.
4. Existing authoritative `NoteCache` already owns `notes`, resolved `links` reverse map, normalized alias/slug/title maps, and lowercase text index. `note_backlink_count` avoids full note clone for simple counts, but **the panel helper's semantics are broader** than `NoteCache.links`: plain string mentions, wiki alias matches, todo references, and snippets matter. Do not substitute the resolved-link map alone for current results.
5. Unsaved/current-draft note content, todo revisions, alias collision behavior, link/source order, reason/category/snippet strings, fenced-code exclusion, failed snapshots, races, and last-good presentation are essential.
6. `NotePanel::alias_collision_warning` around 1189 gets matching slugs via `note_alias_map_snapshot` then clones every note using `note_cache_snapshot()` just to assemble conflicting labels; a versioned lightweight slug/title projection is a candidate low-risk win **if frequently exercised**.
7. `NoteCache::from_notes` normalizes aliases, builds resolved wiki links and maps; any extra reverse index should be built coherently at publication and remain bounded, not recomputed per frame.

**Required oracles/gates:** compare the exact ordered `BacklinkRow` fields, todos, notes and menu paths to an untouched eager reference for: alias collisions/ambiguity, secondary aliases and rename, wiki links versus entity refs versus prose mentions, plain/fenced links, duplicate refs, missing/broken targets, unsaved drafts, large snippets, multi-tab order, failed persisted reload, todo change, disabled→enabled setting. New index work is conditional on fusion's measured residual cost. Preserve versioned snapshot and mutation-lock ordering.

**Relevant existing tests:** `note_cache_publishes_alias_title_body_tag_and_backlink_changes_once`, `note_cache_versioned_snapshot_pairs_revision_and_notes_during_publication`, `open_unsaved_panel_tracks_external_note_and_alias_transitions`, `note_backlink_count_matches_resolved_unique_sources_without_note_clones`, `note_cache_retains_unreadable_reload_then_recovers_without_duplicate_version`, `backlinks_grouping_splits_categories`, `link_menu_*`, `persisted_todo_text_and_note_reference_refresh_linked_rows` (verify names on actual HEAD).

## 3. Action publishing: verified O(N) derived-cache rebuilds

**Source:** `src/gui/mod.rs::publish_custom_actions_with_indexed_tail` around line 1054, `::publish_actions` around 1064; `src/gui/search.rs::update_action_cache` around 154; `src/gui/watch.rs::process_watch_events`, Actions and IndexReady arms.

- Custom Actions-file changes currently clone the entire already-indexed tail into a new Vec, concatenate custom+indexed, replace `self.actions: Arc<Vec<Action>>`, rebuild `action_cache`, `action_filter_metadata`, and `actions_by_id` (the last includes cloned Action values), invalidate results, mark autocomplete dirty, and request a live-query refresh. A separate deferred completion index is rebuilt when its debounce expires.
- Changed reload is synchronous on the GUI event path despite the eliminated filesystem rescan; the final historical 10k+10k ActionsReload p95 is ~115ms. Index completion has a separate cache/query publication stage (~50ms p95 at 10k). These are **distinct paths** and should be timed separately in new baseline.
- `actions_by_id` duplicates and ordering may be semantically significant (collect into `HashMap` gives last-wins by action ID), and consumers (history, pin resolution, radial, plugin catalogs, query) depend on published action identities, versions, and search invalidation. Do not change `Action` public representation or collapse custom/indexed order silently.
- Alternative experiments include minimizing repeated metadata construction, sharing immutable precomputed projections by generation, and preparing heavy pure data away from the GUI owner before atomic commit. **Do not perform I/O or plugin callbacks under the lock**; do not publish a stale projection or allow an older index completion to overwrite the latest custom prefix.
- Identical custom Actions watcher events already do early equality checks; preserve `Arc` identity/version for unchanged input, last-good behavior on missing/corrupt data, and reentrancy rules for local persistence.

**Relevant existing tests:** `actions_watcher_retains_invalid_then_publishes_valid_without_local_double_bump`, `gui_index_accepted_publication_updates_search_history_pins_and_radial_catalog`, `gui_index_equal_completion_preserves_catalog_and_search_state`, `gui_index_completion_merges_the_latest_actions_prefix_after_a_blocked_scan`, `gui_index_failure_retains_last_good_actions_then_recovers`, `gui_index_empty_config_clears_tail_immediately_and_ignores_cancelled_result`.

## 4. Search: avoid eager Action clones while preserving exact ranking

**Source:** `src/gui/search.rs::search_actions` around 356 and `::search_read_only_outcome_from_scored_plugins` around 656; `::apply_usage_weight` around 725; `src/gui/mod.rs::handle_key` around 2600.

- Empty `app` search clones every action; non-empty exact/fuzzy app searches iterate eligible actions, score and **clone every matching Action before sorting**. Plugin results are appended, usage weights applied, and a stable `sort_by` (descending f32 score, equal scores retain insertion order) builds final results.
- Some programmatic/read-only, radial and deferred/provider paths call the same search owner. Cached strings, alias matches, exact text semantics, raw query normalization and provider-pending state matter. An index/score/metadata-only intermediate plan could delay `Action` materialization until order is determined, **but the full final result list may still be required** for navigation and radial semantics; don't arbitrarily return top-K.
- Do not replace `SkimMatcherV2` semantics, change plugin/registration ordering or fuzzy/exact/usage tie behavior, drop args, silently normalize `None` versus `Some("")`, or filter plugin-resolved command results incorrectly. Fuzzy algorithm changes are separate, heavily gated opportunities.
- Benchmarks must distinguish `app` query, no-query action browse, exact/fuzzy, sparse/dense matches, alias and usage ties, plugin mixed ordering, and how search replacement invalidates geometry; measure real query-to-first-frame separately.

**Relevant tests:** `exact_display_match_uses_pre_normalized_query_substring`, `exact_mode_keeps_plugin_resolved_results_but_filters_query_suggestions`, `screen_draw_priority_fixture_requires_normal_app_query_without_read_only_effects`, `clipboard_modify_root_query_returns_direct_open_modify_first_in_fuzzy_mode`, `*_in_exact_mode`, `manual_query_fallback_does_not_reenter_a_blocked_provider`, `search_replacement_clears_selected_index`.

## 5. Cold launcher geometry: preserve egui 0.27's exact geometry

**Source:** `src/gui/render.rs::RootListGeometryCache` near lines 35–570; `::render_root_frame` near 1780, visible-row code around 2568–2750. Existing comprehensive tests around 4177–5390.

- `RootListGeometryKey` keys result generation/count, available width, font/atlas, wrap, padding, interaction size, spacing, path-display mode and grid parameters. `invalidate_results` advances generation, invalidates cache and queues correct popup cleanup.
- `ensure` loops **all results**, computes display text and egui galleys, and updates row tops, x offsets, response widths and full content extent. Not just heights: a wide/no-wrap item changes later content widths and x origins.
- `ensure_grid` also evaluates every cell to preserve **global column minima/maxima, row tops/height, tall-cell visual spill, stripes, incomplete row bounds, and content extents**; cached `grid_visual_max_end_tree` enables disjoint tall overflow visibility.
- Virtual painting already builds bounded visible/overscan rows and retains at most an offscreen popup owner. This **warm virtualization is a solved problem**; do not replace it with fixed `show_rows`, clamp widths, truncate labels, cap result count, or break scrolling.
- Candidate approaches: exact generation-aware reuse when results overlap, safe layout checkpoint/prefix caching, recognizing unchanged display-text subsets, or a bounded no-op invalidation avoidance. Any incremental scheme must correctly recompute all **affected suffix** geometry and global grid columns; unchanged full extent must be provable. Background galley measurement likely depends on egui `Context`/font state, so do **not** assume it can move to a worker.
- Critical geometry oracles already exist: `root_list_geometry_matches_eager_selectable_label_layout`, `root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, `root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection`, `root_grid_viewport_builds_complete_bounded_rows_with_absolute_ids_and_click_targets`, `root_list_context_menu_owner_stays_with_its_absolute_row_offscreen`, `root_grid_popup_owner_retains_complete_row_without_retargeting`, and cache invalidation tests. Keep/add tests for empty, 1/2/3/5/6 cols, giant text/newlines, DPI/fonts, narrow/wide, selected first/middle/last, offscreen menus, mixed scroll, keyboard/mouse clicks.

## 6. Cold Quick Notes geometry and metadata

**Source:** `src/gui/notes_dialog.rs::measure_notes_geometry` around 311, `::maybe_refresh_derived` around 861, `::rebuild_projection_if_needed` around 979, `::ensure_notes_geometry` around 1011, `::ui` around 1297.

- The notes dialog caches revision-validated lightweight `NoteRowMetadata`, an ordered filtered projection of **original indices**, and geometry mapping note identities to projected rows. It handles editor deferral, last-good on failed/raced snapshots, anchor remapping, horizontally wide content, and one retained popup.
- Cold geometry does a full sequential measure of the filtered projection. A wide earlier header/preview expands `prefix_width` for subsequent rows; preview text wraps under viewport width, so incremental reuse depends on the **prior width state**, not just per-note stable identity.
- Historical cold 5k time varied dramatically by viewport: ~40ms standard versus ~628ms narrow. Focus on invalidation patterns, case-only equivalent projections, viewport changes and metadata changes; prefer exact reuse of unaffected prefix/segments with correct state over speculative lazy heights.
- Preserve original-index callbacks, identity anchor and fallback on removed notes, unsaved-editor freeze, new-note sentinel, note revision race handling, sorting, aliases, titles, meta labels and menu ownership.
- Existing tests include `quick_notes_scroll_anchor_prefers_identity_then_nearby_survivor`, `quick_notes_popup_owner_stays_with_identity_offscreen_then_closes_on_removal`, `quick_notes_publishes_a_successful_edit_after_the_editor_closes` and actual eager geometry tests in module `gui::notes_dialog::tests`.

## 7. Startup: catalog readiness is a correctness constraint

**Source:** `src/main.rs::startup_indexed_actions` around 885, `::startup_action_catalog` around 935, `::spawn_gui` around 955, `::main` around 1109; `src/performance.rs::record_frame` and startup Timer logs.

- Startup submits an `IndexCoordinator` request then **waits for acknowledged complete indexing before plugin registration and the GUI**. That is deliberate, not necessarily an accidental defect.
- `OmniSearchPlugin` and `VirtualDesktopPlugin` are constructed with complete catalog snapshots; startup cannot be made asynchronously empty without an explicit, verified consumer refresh path and unchanged first-query behavior.
- The coordinator already supports bounded pending/results/cancel, config-generation rejection and GUI ownership transfer. Reuse it; do not create another worker architecture.
- First usable frame and hotkey semantics depend on plugin readiness, single-instance/data-directory ownership, watcher initialization and gesture/radial behavior; measure separately: process start→index ready, hotkey received→visible/useable grid, initial query ready, and first frame. An apparently faster first frame missing catalog results is a regression.
- Prefer reducing other strictly independent startup work, caching safe immutable initialization, and measuring true user-perceived critical-path latency. Make catalog-readiness deferral a high-risk, optional gate only if a complete consumer parity mechanism is proven.

## 8. GUI events: bounded work must not break control-plane delivery

**Source:** `src/gui/mod.rs::new` around 1746 creates `std::sync::mpsc::channel()`; `src/gui/watch.rs::process_watch_events` around 51 drains `while let Ok(ev) = self.rx.try_recv()`; root update calls it from `src/gui/render.rs` around 2257. `EventSinkRegistration`/`event_consumed` bookkeeping lives in `src/gui/mod.rs`.

- There is no explicit `max-events/frame` or elapsed-work budget on this drain. Event variants include high-priority action execution, radial dispatch, emergency recovery, indexing completions and ordinary file-change notifications. Not every event is safe to reorder, discard or coalesce.
- The channel is unbounded; registration/pre-registration mechanisms and `ViewportWake` are separate. A bounded per-frame drain could improve fairness during sustained event floods, but cannot lose wakeups, starve high-priority safety events, violate FIFO effects, or make file watcher versions skip required publications.
- Characterize real bursts and queue age with low-overhead counters **before** choosing mitigation; instrument enqueue/consume separately from expensive handlers. Coalesce only explicitly identified idempotent notifications and preserve observable last-good, version-bump, diagnostics, and latest-custom-prefix behavior. Use deterministic channels/barriers for race tests, never arbitrary sleeps to prove timing.
- Native Windows responsiveness proof needs a controlled, owned process/hotkey, visible frame and realistic events; a headless egui frame is only CPU/dispatch evidence.

## 9. Measurement requirements and optional hardware study

- **Track C new baseline** on exact feature-branch base HEAD before optimization. Freeze raw JSON and human-readable protocol; never overwrite `docs/performance/track_a_*` or `track_b_*` baselines.
- Keep `MULTI_LAUNCHER_PERF=1` opt-in; use `ML_TRACK_C_BENCH_MODE=small|full` or a clearly specified equivalent; bounded metric-cardinality and no private note/clipboard content/path/HWND capture.
- Include content/position/result-order signatures, original-ID and complete-projection signatures **separate from bounded rendered viewport receipts**. Track A deliberately changed rendered-receipt semantics; do not fake equality or rewrite frozen benchmarks.
- Cases: 100, 1,000, 5,000 notes; 100, 1,000, 10,000 actions/results, 20k combined catalogs; cold/warm geometry; first/middle/last and mixed-case queries; action publication changed/unchanged/equal; transient note-save/failure/stale snapshot; startup/index 16/1k/10k roots; event quiet/burst/sustained.
- Timings: p50/p95/max from controlled samples, plus actual *work units*, lock/queue delay, cold single observations or cold distributions explicitly distinguished; record sample counts, toolchain, source SHA, power, target/profile, fixture hashes, GC/OS interference and exclusions. Cross-profile comparisons are invalid without matched controls.
- Native: real launcher hotkey/first-frame, stable/no stale query, pane usability, Windows focus/target consistency, radial invocation, unload/shutdown, owned process and clean resource teardown. Mark `NOT RUN` / `NOT MEASURED` explicitly if unavailable.
- Keep tests/builds practical on Windows: prefer `cargo nextest run --lib -E 'test(...)'`, `--test domain`/specific targets, `cargo check --lib`, and opt-in ignored owner tests serial when collecting timings. `--cargo-profile fast-dev` is a separate profile and **must not be mixed** with a default-profile baseline. Normal builds: `cargo build --profile iteration --bin multi_launcher`; canonical release only at final acceptance.
- **Optional separate SSD experiment** after Track C core acceptance: the verified B host stores source/target on a mechanical HDD; test source/target relocation only in isolated paths with comparable caches and available capacity, preserve user data/config and Track B profiles. Do not infer gains or block Track C on it.

## 10. Recommended stage ordering and stop conditions

1. **C0 measurement/oracles first.** Require complete reproducible cold + warm baseline; do not treat historical Track A single cold observations as a new baseline.
2. **C1 correctness-preserving collection optimizations.** Fuse the three backlink passes first; only add indexes if repeat measurements justify complexity. Benchmark action publication separately from index traversal.
3. **C2 search before geometry.** Compare exact ordered Actions, args, score ties, provider state. Geometry may benefit more from avoiding unneeded result-generation invalidations than from changing egui layout algorithms.
4. **C3 measure before queue/startup restructuring.** Preserve startup catalog completeness and urgent event ordering. If risk outweighs benefit, document a gated skip.
5. **C4 independent reviews, focused regressions, controlled native checks, final comparable report.** A skipped speculative checkpoint can be correct and complete if evidence and rationale are explicit.

**Explicit exclusions:** broad crate/workspace split, rust-analyzer tuning, a second Track B profile campaign, unmeasured cursor/GDI caching, large renderer rewrite, replacing fuzzy ranking, changing persistence schemas, changing global hotkeys/radial controls, opportunistic API/dependency upgrades, disabling tests, and automatic Git pushes/merges.
