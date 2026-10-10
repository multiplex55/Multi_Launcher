# Multi Launcher — Track A Runtime Performance Source Notes

**Source of truth:** `multi_launcher(20261009-172730).zip` (October 9, 2026 upload)  
**Scope:** Runtime performance and responsiveness only (**Track A**).  
**Status:** Source inspection and implementation hypothesis; **no benchmark or Windows test has been executed as part of this review**.  
**Companions:** `multi_launcher_performance_codex_plan.md` and `multi_launcher_performance_codex_start.md`.

> **Source-control limitation.** The uploaded archive contains repository source and `.codex` agent definitions but not `.git` metadata. Its originating commit SHA, branch and remote state **cannot** be established from this archive. The orchestrator must compare the actual checked-out code with this upload before creating `performance-optimization`; do not assume `master` contains these changes. All paths/line hints below were inspected in the extracted, upload-matching source and may shift as files change.

## 1. Executive assessment

The identified inefficiencies are real code paths that deserve attention, but the magnitude of their runtime cost remains unmeasured. The approach is **instrument → establish baseline → remove redundant work → prove behavior unchanged → compare measurements**. Do not promise a blanket startup-time or FPS improvement.

| Candidate | Verified mechanism | Targeted benefit | Risk / caution |
| --- | --- | --- | --- |
| Notes panel | Full note cache is cloned and aliases sorted/hashed during heavy-derived refresh checks | O(1) unchanged-revision check instead of O(total note content) snapshot work | Cache publication/revision consistency, drafts/backlinks |
| Dashboard command history | Entire history is cloned; entries resolved before `.take(cfg.count)`; plugin command lists can be enumerated per unresolved entry | Resolve only enough rows to display; reuse command lookup | Pinned ordering, filtering on *resolved* label, missing-entry semantics |
| Launcher and notes list | All result rows are constructed inside scroll area, including off-screen rows; notes also clone bodies/build previews | Row-count-proportional rather than result-count-proportional widget work | egui selection/scroll, context menus, variable-height notes |
| Actions watcher/indexer | `WatchEvent::Actions` reload consumes all `index_paths_batched()` output on GUI event processing | No redundant filesystem scan when only actions change; nonblocking genuine reindex | Asynchronous state races and last-good semantics |
| Native mouse effects | Effects `poll_events()` refreshes cached source before worker samples cursor; a subsequent `render()` may present updated geometry | Remove outdated/double source presentations in movement ticks | Magnifier exclusions, stationary video/content updates, failure latching |
| Coordinate sampling/HUD | Active worker samples at 16ms; native sampler queries multiple geometry APIs; HUD draws with new/deleted GDI brush and font | Conditional reduction in API/resource churn | Window bounds can change without HWND change; GDI lifetime |

**Suggested priority:** M0 measurement; M1 notes; M2 history; M3 virtualization; M4 watcher/indexing; M5 mouse effects; M6 conditional sampling/HUD; M7 end-to-end verification.

## 2. Relevant repository constraints

- `AGENTS.md` is substantial and authoritative. It emphasizes source inspection, backward compatibility, scope-proportionate `cargo nextest`, careful Git usage, no gratuitous refactors, and **active meaningful checkpoint commits** (not commit-every-line).
- `.codex/config.toml` enables subagents. `.codex/agents/planner.toml`, `implementer.toml` and `reviewer.toml` are available. Assign **one bounded milestone packet** per implementer; do not overlap edits.
- `Cargo.toml` specifies Rust edition 2024, eframe/egui `0.27`, `egui_extras = 0.27`, Windows API integrations and Criterion benchmarks. This plan does **not** change linkers, test-target layout, debug profiles, dependencies or the workspace architecture: those belong to excluded Track B.
- `src/performance.rs` implements opt-in `MULTI_LAUNCHER_PERF` logging, timing helpers and frame/repaint counts. `src/gui/search.rs` and `src/plugin.rs` already record search phases. Extend rather than build a competing telemetry system.
- Existing `ScrollArea::show_rows()` uses can be studied in `src/dashboard/widgets/notes_recent.rs`, `recent_notes.rs`, `process_list.rs`, `src/gui/file_search_preview_dialog.rs`, and elsewhere. **egui 0.27 `show_rows()` assumes a predictable row height**; it cannot be applied directly to variable-height notes without a compatible layout strategy.
- `src/gui/render.rs` and `src/gui/mod.rs` own main-root rendering, keyboard selection and action dispatch. Virtualization must leave action/selection identity and accessibility behavior unchanged.

## 3. M1 — Notes cache/version and derived-state checks

### Observed code

- `src/plugins/note.rs:~152`: `NoteCache` owns notes, tags, backlinks map, full-text index, aliases and slug/title maps, guarded by a `Mutex`.
- `src/plugins/note.rs:~1004–1038`: `refresh_cache()`, `publish_note_cache()`, `bump_note_version()`, `note_version()`, `note_cache_snapshot()`, `note_alias_map_snapshot()`.
- `publish_note_cache()` builds the new cache, compares `guard.notes` against the new set, publishes when changed, and increments `NOTE_VERSION`. `note_cache_snapshot()` clones **all** cached `Note` values, including content, under the cache mutex.
- `src/gui/note_panel.rs:~1206`: `refresh_heavy_derived()` obtains `note_version()`, `todo_version()`, hashes current content, clones notes, sorts/hashes aliases and only afterwards checks whether recomputation can be skipped.
- `src/gui/note_panel.rs:~1303`: `maybe_refresh_heavy_derived()` computes `alias_map_hash(&note_cache_snapshot())` on a refresh-check path, including when there was no note change. It also separately hashes panel content.
- `src/gui/note_panel.rs:~4366`: `alias_map_hash()` gathers the primary alias/slug pairs, sorts them and hashes them. Note: the index also supports multiple aliases, so replacing this hash needs a correct understanding of which changes matter.
- `src/gui/note_panel.rs:~1323`: `refresh_link_menu_targets_if_needed()` is **already keyed on `note_version()`**. Reuse that precedent.
- `src/plugins/note.rs:~1130–1390`: save, append and delete paths publish the changed cache. Background file reload also calls `refresh_cache()`; inspect the exact watcher/coordinator when validating all mutations.

### Recommended architectural seam

1. Verify every successful published mutation/reload that affects backlink/alias results increments the existing revision, and every failed/unchanged operation leaves it unchanged. Add tests before removing the alias hash guard.
2. Use the cheap `note_version()` / `todo_version()` / panel-local dirty or edit marker to decide if a cache snapshot is needed. No full note clone, alias sort, or back-link rebuild in unchanged frames.
3. When a refresh is actually needed, take a consistent note snapshot and revision, deriving metadata **after** dropping the cache lock. Avoid publishing a cache snapshot tagged with the wrong version if a writer races a reader; choose a lock-synchronized snapshot or validated read/retry, not an unexamined double-read.
4. Keep the 250ms heavy-derived edit debounce, settings-conditional backlinks, forced recomputation, panel-local unsaved edits and todo revisions behaving as before. Do not tie unsaved editor changes exclusively to a persisted-note revision.
5. Consider cheap immutable metadata/`Arc` publication *only* if measurement or lock contention justifies it. Do not add a second mutable notes source of truth.

### Validation anchors

`src/plugins/note.rs` cache tests near `~3322–3405`; `src/gui/note_panel.rs` link-menu revision tests near `~6863–6930`, backlink tests near `~7693–7840`, save/slug invalidation near `~8322`, and `tests/notes_plugin.rs`, `tests/note_panel_auto_save.rs`.

**Uncertainty requiring a test:** whether all desired user-visible refreshes (external edits, title/alias and backlink changes, watcher recovery) are covered by `NOTE_VERSION` across every persistence path. Do not infer that from the single `publish_note_cache()` call alone.

## 4. M2 — Dashboard command-history preparation

### Observed code

- `src/dashboard/widgets/command_history.rs:~50`: `CommandHistoryWidget` owns filter text, asynchronously loaded pins and widget settings. Default `count` is **8** (configurable 1–50 in settings UI).
- `~126–282`: `resolve_action()` calls `ctx.data_cache.snapshot()`, checks snippets, `ctx.actions_by_id`, then calls `ctx.plugins.commands_filtered(ctx.enabled_plugins)` for unresolved actions; afterwards checks processes, favorites, notes, clipboard, todos and snippet editing/removal, plus opaque legacy clipboard literals.
- `~324–384`: `render()` clones all history via `with_history(|h| h.iter().cloned().collect())`, **including pinned-only mode**; resolves pinned and unpinned entries before applying `.filter(...).take(self.cfg.count)`.
- Pins are sorted newest-first for mixed mode; unpinned history follows existing `VecDeque` order, excluding pinned identities. `HistoryPin::eq` uses action ID **and args**, not saved timestamp or label.
- `src/dashboard/dashboard.rs:~30` exposes `DashboardContext` with `actions_by_id`, plugin manager and revision fields (`actions_version`, `fav_version`, `notes_version`, `todo_version`, `clipboard_version`, `snippets_version`, etc.). `src/gui/render.rs:~1879` constructs this context.
- `src/gui/search.rs:~153–179` already builds `LauncherApp.command_cache` via `plugins.commands_filtered(...)` on command updates; investigate whether the dashboard can reuse that authoritative catalog or a lookup derived on the **same** revision, rather than keep another per-frame plugin enumeration.
- `src/history.rs:~110+` exposes `with_history()`, an `RwLock` read-access helper, and writes history under a separate transaction lock.

### Recommended architectural seam

- Build a **single per-refresh resolution context** containing borrowed or cheap-`Arc` dashboard snapshots and command lookup. Preserve fallback/preference ordering *exactly* (snippets → actions → commands → processes → favorites → notes → clipboard → todos → snippet special routes → legacy clipboard).
- Key any long-lived lookup by all relevant state changes, including enabled plugin set and plugin command-catalog updates. A context already holding the current command slice may be simpler and safer than inventing a new revision counter.
- When pinned-only, avoid reading/cloning the ordinary history at all.
- Traverse candidates in the current pinned/unpinned order and stop after **`count` filtered matches**, not after `count` raw entries. Filtering uses resolved labels and query text, so entries may require resolution before rejection; there is no safe blanket shortcut that filters by stale saved labels.
- Do not hold `HISTORY` read locks while calling plugin code, filesystem APIs or rendering; clone only needed candidate data in a bounded step or use a safe read-only preparation model.
- Preserve `missing` marking for pins versus fallback executability for ordinary history and special opaque clipboard/snippet semantics.

### Validation anchors

`src/dashboard/widgets/command_history.rs` existing tests from `~480`, `tests/history.rs`, command-cache invalidation tests in `src/gui/mod.rs` and source-owned plugin query tests. Add focused tests for pinned-only zero history reads, counts, filtered/renamed targets, duplicate identities and zero plugin-catalog enumerations on unchanged frames.

## 5. M3 — Grid, list and Quick Notes virtualization

### Observed code

- `src/gui/render.rs:~1898–2050`: under `ScrollArea::vertical().show(...)`, `scale_ui` chooses a grid or list. The grid loops `0..self.results.len()` with `egui::Grid`, cloning each action, formatting label and description, building a `SelectableLabel`, attaching context menus, and scrolling selected responses into view.
- The list loops all results similarly; it additionally determines folder alias/full-path display and timer tooltip text. Both paths use absolute `self.selected` result indices and preserve deferred clicks/actions.
- `src/gui/mod.rs:~2570–2645` handles keyboard selection/navigational math, including column-based movement. `src/gui/render.rs:~9597` already has a keyboard navigation parity test.
- `src/gui/notes_dialog.rs:~364–637`: Quick Notes iterates all `self.entries` under `ScrollArea::both()`, filtering against a prebuilt search index but cloning every matching `Note`. For every visible *and invisible* match it calculates meta text, `note_backlinks(...).len()`, checkbox count, preview and extensive context menu actions.
- `src/gui/notes_dialog.rs:~49`: `short_preview` concatenates **all** eligible lines before limiting to 120 characters. `checkbox_count()` also scans whole note content.
- `src/plugins/note.rs:~583`: `note_backlinks(slug)` resolves slug entries by scanning `cache.notes` and cloning entire `Note` bodies. This is particularly unnecessary when the caller needs only the count.
- `NotesDialog` stores `entries`, `index`, `edit_idx`, `text`, `search` and template manager state. Its editing identity is an **original-entry index**, not a filtered row position.

### Recommended architectural seam

1. Virtualize launcher **list** at stable row-height boundaries. Render/context-menu/tooltip calculations only for visible rows plus modest overscan. Keep `self.selected` indices absolute and request scroll offset for an off-screen selected row before relying on a `Response::scroll_to_me()`.
2. Virtualize launcher **grid by complete rows**, `row = absolute_index / cols`; map visible rows back to `[row*cols..min((row+1)*cols,len)]`, preserving spacing, partial last row, width/scaling and selection math.
3. For Quick Notes, build a lightweight filtered projection of original-entry indices whenever entries/search/settings change. Cache stable metadata such as short preview, checkbox and backlink counts appropriately. Note `ScrollArea::show_rows()` assumes fixed heights; retain variable-height presentation by height-estimation/measurement with offset mapping, grouping/bucketing, or another correctness-preserving approach. Do not silently replace the UI with compact fixed-height rows without a separate explicit decision.
4. Preserve Quick Notes context menu actions, wrap-links, edit/delete original indexes, focus, hover preview and search behavior, resizing and DPI/scale changes. An optimization that reduces widgets but breaks off-screen keyboard selection is not successful.
5. Add `note_backlink_count()` (or equivalent) under the cache lock that reads the backlink-slug vector length only, with tests that it equals the old API's count in relevant cases; retain `note_backlinks()` for callers needing actual `Note` objects.
6. Optimize bounded preview generation without altering the currently displayed short normalized text, heading/alias exclusion, Unicode truncation boundary, or whitespace semantics.

### Validation anchors

`src/gui/render.rs` keyboard navigation tests, `tests/note_panel_scroll.rs`, `src/gui/notes_dialog.rs` existing preview/count and wrap-links tests, note cache/backlink tests, existing dashboard `show_rows` examples. Add synthetic large-dataset widget counts and context-menu/edit-identity tests.

## 6. M4 — Watcher reload and path-indexing thread safety

### Observed code

- `src/gui/watch.rs:~156–212`: `WatchEvent::Actions` loads the custom action file with typed persistence handling. Invalid/missing data retains the last-good state. An unchanged custom action list is already skipped.
- A **changed** custom list triggers `indexer::index_paths_batched()` and consumes **every** batch in the event handler; only then calls `self.publish_actions(custom,indexed)`.
- `src/indexer.rs:~1–157`: `IndexBatchIter` walks roots via `walkdir`, canonicalizes files, deduplicates on canonical path, respects a max count and emits default-sized batches of 512. Creating batches is not asynchronous. Traversal errors return from the iterator; be careful about current partial-results/error behavior versus the proposed last-good policy.
- `src/gui/mod.rs:~1022–1055`: `update_custom_actions()` already reuses the indexed tail of `self.actions` via `skip(self.custom_len)`, then republishes merged action state. `publish_actions()` updates `custom_len`, `Arc<Vec<Action>>`, action search/ID caches and background query refresh.
- `src/gui/mod.rs:~1532–1580`: `update_paths()` updates `index_paths` along with plugin settings. Confirm actual reindex triggers before changing them; do not assume all desired triggers are present.
- `src/gui/mod.rs:~4434–4495`: action watcher tests verify last-good state, successful reload and no unwanted extra action-version bump.

### Recommended architectural seam

- For changed custom actions, preserve current indexed tail and publish the custom segment promptly, without any disk-path enumeration.
- Create a **single bounded/coalescing reindex coordinator** only for startup, configured-root / max-item changes and explicit refresh operations that genuinely require new indexing. Prefer an existing event-sink/channel pattern and place filesystem traversal on a worker thread.
- Snapshot (roots, max items, generation) for each job; check supersession/cancellation at bounded points; send completion or failures back to the GUI owner, not mutated GUI state directly from the worker.
- On arrival, apply a result only if its generation **and configuration** match the latest request, and merge it with **the current custom actions**, not the custom set captured when the scan began. Preserve last-good index on scanner failure; preserve invalid-actions persistence last-good behavior.
- Keep one data-directory owner, no unbounded worker/thread creation, no blocking wait/`join` on the GUI rendering path; document orderly shutdown. Preserve canonicalization, duplicate handling, traversal semantics and max-item behavior except explicit error-hardening.

### Validation anchors

`src/gui/watch.rs`, `src/gui/mod.rs` watch tests, `tests/watchers.rs`, `tests/watcher_failures.rs`, `src/indexer.rs` and event registry tests. Add a deterministic fake/in-memory indexer seam for races, rapid changes, stale completions, error retention, and custom edits arriving during a scan.

## 7. M5 — Mouse magnifier refresh ordering

### Observed code

- `src/coordinate_tool/controller.rs:~10, ~330–424`: a **single worker exists only while HUD/crosshair/halo/zoom has an active mode**. It waits with a `16ms` timeout, `backend.poll_events()` first, then `sampler.sample()`, then conditionally `backend.render(frame)` if forced, changed or retry required.
- `src/coordinate_tool/native.rs:~2327–2367`: `WindowsSurfaceBackend::poll_events()` pumps Windows messages, refreshes `self.effects.poll_visible_sources(...)` on the non-topology-invalidated path using **cached** successful geometry, then restacks. `render()` updates crosshair and HUD, refreshes exclusion filters and calls `effects.reconcile(...)` with **current** sampled geometry.
- `src/coordinate_tool/native_effects.rs:~217–440`: `CursorEffectsRuntime::reconcile()` updates native effects and caches validated live sources; `poll_visible_sources()` updates currently visible requested sources from the cache and does not reconcile.
- This creates a **plausible two-submission moving-tick path**: poll cached geometry followed by reconcile/present of new geometry when it differs. This is *not* evidence of 2x GPU work; native Windows may coalesce invalidations.
- Stationary animation is intentionally refreshed by `poll_visible_sources()`, even when `last_frame` is unchanged and normal rendering is skipped.
- Renderer distinction is critical: `current_sample` is the only valid effect source. `displayed_sample` may be frozen for HUD, and `placement_sample` can be last-good after a sampling failure. **Never** give either stale sample to halo/zoom.
- Filter-list input includes HUD, crosshair, guide and sibling effect/outline windows; staging outlines before installing magnifier exclusions prevents recursive capture.

### Recommended architectural seam

Keep a clear worker tick sequence: **pump native messages/topology → obtain live cursor sample → determine changed frame and effect refresh requirements → update cheap surfaces/exclusions as necessary → present each requested visible effect at most once per ordinary tick**. If no frame change, refresh visible effect sources using the current *validated* live source or preserved equivalent only when valid. Avoid stale geometry, but do not suppress stationary content refresh. If the backend interface changes, keep a deterministic fake backend for tests.

Preserve failure latches/retry transitions, disabled/hidden behavior, halo fallback, zoom clipping pause/recovery, multi-monitor/DPI events, native resource teardown and magnifier exclusion order.

### Validation anchors

`src/coordinate_tool/native_effects.rs` especially tests `stationary_poll_refreshes_only_visible_requested_effects_without_reconciling` (~2462), `filter_input_invalidation_refreshes_once_without_cursor_movement_churn` (~2240), `topology_refresh_rebuilds_complete_filter_lists_and_live_only_refreshes_visible_surfaces` (~2398), `zoom_clipping_geometry_pauses_without_stale_refresh_and_recovers...` (~2600), and `stationary_refresh_failure_is_latched...` (~2685); controller worker fake tests; Windows-only `src/bin/coordinate_tool_smoke.rs` and `tools/cursor_effects_smoke`.

## 8. M6 — Conditional coordinate sampling and GDI optimizations

### Observed code

- `src/coordinate_tool/native.rs:~160–195`: `WindowsSampler::sample()` calls `GetCursorPos()`, then `sample_at()`.
- `sample_at()` obtains virtual-desktop bounds via `GetSystemMetrics`, monitor geometry via `MonitorFromPoint`, `GetMonitorInfoW` and `GetDpiForMonitor`, and foreground-client geometry via `GetForegroundWindow`, `GetClientRect`, `ClientToScreen` and related APIs.
- A different foreground HWND is not the only invalidation trigger. The same window can move, resize or change client bounds. Overly aggressive caching would produce incorrect client-relative coordinate copies.
- `src/coordinate_tool/native.rs:~430–520`: `LayeredDib::draw_hud()` creates and deletes a `CreateSolidBrush` and `CreateFontW` object on changed redraws. The DIB/DC itself already uses an owning lifecycle; retaining compatible resources is a relatively narrow possible improvement.
- While all four modes are disabled, the controller has **no sampling worker**; there is no proven always-idling coordinate thread problem to fix.

### Recommended decision gate

First record per-tick native sampling time, per-mode work, HUD redraw frequency, GDI handle counts and p95 pointer-to-display latency. Then:

- Cache *stable topology facts* on reliable display/DPI/topology invalidation, optionally per-monitor. Continue sampling dynamic client/window position correctly.
- Scope expensive foreground geometry to the modes/commands that require it **only if** copy semantics stay immediate and correct.
- Retain HUD brush/font with RAII ownership across compatible frames; on changes, deselect prior to `DeleteObject`, replace safely, and never hold a deleted object selected into a DC.
- If measurement shows small costs or correctness risk outweighs gain, **skip an implementation sub-checkpoint** and record why in the report. M6 investigation is approved, not permission to overengineer.

## 9. Measurement plan and definition of success

| Scenario | Sizes / variants | Essential signals |
| --- | --- | --- |
| Note panel idle checks + edit | 100 / 1,000 / 5,000 notes; include long content, aliases, backlinks, todo links | calls/s, cache snapshots, cloned bytes (estimated), lock hold, p50/p95 checks, heavy recomputes |
| History widget render | 100 / 1,000 / 10,000 history; 0/8/50 pin/filters; renamed/missing targets | plugin catalog builds, entries resolved, total clones, render p50/p95 |
| Main list and grid | 100 / 1,000 / 10,000 matches; list, multi-column grid, navigation & selection | widgets built/frame, p50/p95 frame time, selected scroll accuracy |
| Quick Notes | 100 / 1,000 / 5,000 notes; short/long, varied wrap heights | preview chars scanned, backlink clones/count calls, rendered note rows, edit identity accuracy |
| Actions watcher/indexer | 1k / 10k / optionally 100k indexed files, slow/delayed trees | UI-thread stall p95/max, reindex count, scan duration, stale results rejected |
| Mouse effects | halo alone, zoom alone, both; moving/static background; monitor changes | native source submissions/tick, sample→presentation latency, CPU/GPU where available, stationary refresh |
| Coordinate HUD | HUD alone/effects combinations; window move/resize; DPI changes | sampling time, GDI create/delete per update, handle counts, copied coordinate accuracy |

Use synthetic content with seeded, repeatable data; avoid real note text in metrics/logs. Capture environment (OS build, CPU, monitor/DPI layout, power state, display refresh, Rust profile), sample counts, warmup procedure and caveats. Compare before/after using the **same workload on the same machine** and confidence-minded p50/p95/max results rather than a single stopwatch pass. Distinguish cold initialization from steady-state frames and from triggered reload stalls.

**Successful end state:** confirmed reduction in redundant work and/or problematic tail latency, unchanged action results/order/hotkeys/focus, no stale note or indexed state, correct mouse effects and OS resources, no worsening of small workload experience. No global percentage promise is justified.

## 10. Scope exclusions and implementation cautions

**Explicitly excluded (Track B):** cold/warm Rust build profiling, linker/Cargo profile changes, consolidation of 65-ish integration binaries, crate extraction/workspace split, and release optimization. Continue using **targeted** build/test commands to avoid wasting iteration time, but do not turn runtime optimization checkpoints into build-system work.

Also excluded: fuzzy-search algorithm redesign, UI visual redesign, changed note serialization or actions formats, new mouse effects, new user-facing profiling GUI, speculative always-on telemetry, automatic recursive watching of all indexed roots, full indexing rewrite, and unrelated feature work.

**Known limits of this review:** no live Windows runtime measurements, no compiled test result, no GPU analysis, no Git revision proof. A source-level work pattern establishes an opportunity, not a measured speedup. The orchestrator must resolve factual source differences discovered on the real branch, document them, and preserve behavior before proceeding.
