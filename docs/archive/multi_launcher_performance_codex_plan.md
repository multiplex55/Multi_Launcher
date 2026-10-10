# Multi Launcher — Track A: Runtime Performance & Responsiveness
## Detailed Codex Implementation Plan

**Approved:** October 9, 2026  
**Source of truth:** `multi_launcher(20261009-172730).zip`  
**Branch:** `performance-optimization` (base on matching checked-out source commit; **not blindly on master**)  
**Scope:** **Track A only** — runtime responsiveness, redundant-work removal, UI-thread stall elimination, and conditional coordinate-tool efficiency. **Track B build-speed optimization is excluded.**  
**Supporting artifacts:** `multi_launcher_performance_source_notes.md`, `multi_launcher_performance_codex_start.md`  
**Execution model:** Codex orchestrator + repository planner/implementer/reviewer agents; stage-based commits and pushes; measured performance decisions.

---

# A. Contract, outcomes and non-negotiable rules

## A1. Intended outcome

Improve Multi Launcher's **slow-case responsiveness** by replacing demonstrably redundant per-frame/repaint computations with cheap revision gates, preparing only history and result rows needed for display, moving required path indexing out of the GUI thread, and consolidating mouse-effects presentations. Continue to preserve the current fast paths and avoid adding persistent background overhead.

Work must be measured at the **local subsystem level**. The source establishes performance opportunities, not quantified speedups. The plan contains no promised application-wide percentage gain.

### Success criteria

1. The unchanged-notes-panel check does **not** clone the note cache or sort/hash aliases on every repaint; revision changes still produce correct backlinks and aliases.
2. An eight-row history widget does not clone/resolve the entire history or re-enumerate enabled plugin commands for each invisible entry.
3. Root result-list and grid widgets are prepared only for visible rows plus bounded overscan, with the same absolute-index selection, keyboard navigation and interactions.
4. Quick Notes uses a filtered index projection and effective variable-height virtualization; preview and count operations avoid cloning large note bodies for invisible entries.
5. Reloading changed custom Actions data **does not trigger a full path reindex**; required reindexing is off the GUI thread and stale results cannot overwrite newer configurations or custom actions.
6. Moving cursor effects no longer submit an outdated source immediately before submitting the current source in the same ordinary worker tick. Stationary magnified content still refreshes and filter exclusion ordering is intact.
7. Coordinate sampling/GDI refinements are retained **only when** profiling supports a real improvement without degrading correctness.
8. No changes to existing actions/data formats, query ranking, plugin routes, radial/menu behavior, focus/hotkeys, native lifecycle or failure recovery.
9. A final report contains actual pre/post measurements, supported conclusions, source/commit provenance and unimplemented/conditional items marked clearly.

## A2. Hard compatibility invariants

**Launcher search and activation**

- Exact result membership/order/ranking, displayed label/description, tooltips and action identity.
- Grid/list toggle, columns, folder alias/full path, timer tooltip, pointer selection and right-click context actions.
- Up/down/left/right/page/home/end navigation, selected-row scrolling even when initially off-screen, Enter/Ctrl+Enter/Shift+Enter semantics, selection after changing query, and input focus.
- Radial actions, note and mouse command prefixes, universal actions and dashboard action dispatch unchanged.

**Notes**

- Note cache remains authoritative; persisted markdown, slugs, title/alias lookup, links/tags and backlinks are unchanged.
- External note reloads, saves, appends, removals, renames, settings toggles, todo changes and failed-refresh last-good behavior remain correct.
- Unsaved editor text has its own dirty/debounce lifecycle; a persisted revision cannot replace its invalidation.
- Quick Notes still shows the same title, metadata, preview, hover content and context menu functions; index-sensitive actions target the underlying note, not the filtered row number.

**Filesystem/index**

- Custom actions are before indexed actions, with correct `custom_len`, identity, deduplication and configured max-index semantics.
- No half-published action state, stale completion overwrites, uncontrolled worker/thread growth, or truncated results presented as successful if the indexing scan failed.
- Invalid custom Actions data retains last-good state and its existing diagnostic reporting.

**Mouse / Windows**

- Live pointer sample is the only source for halo and zoom; frozen/last-good HUD samples are not valid effect sources.
- Stationary underlying pixels continue to update in native magnification.
- Exclusions prevent recursive capture (HUD/crosshair/guides, halo/zoom hosts/outlines), including fallback stages, display-topology changes and failure recovery.
- Multi-monitor, Windows DPI changes, window move/resize, clipboard coordinate precision, GDI cleanup and worker shutdown remain correct.
- All four modes off means no effects sampling worker.

**Performance discipline**

- Disabled telemetry has negligible overhead; never log note content, history secrets or filenames beyond established logging rules.
- Prefer shallow/read-only borrowed views and existing revision mechanisms rather than duplicating state.
- A short list must not become slower merely because a large-list fast path was introduced; investigate regression before retaining an optimization.

## A3. Out of scope

- **Track B**: build/linker/cache tuning, consolidation of Cargo integration targets, extraction of crates, test binary architecture, debug symbols or release-build optimizations.
- Search algorithm redesign, command routing redesign, plugin lifecycle refactoring, custom file-format migration or new user-facing functionality.
- Styling/redesign of Quick Notes or launcher layout, changes to current mouse effects, adjustable new refresh-rate settings, new always-on performance dashboard.
- Recursive live watching for *all indexed folders*, broad filesystem indexer replacement, speculative major async framework migration, dependencies introduced solely for convenience.

Only make small, adjacent high-value low-complexity optimizations when clearly measured or required for a specified checkpoint; document each as an extension of that checkpoint.

## A4. Git and baseline policy

The ZIP contains **no `.git` directory or commit metadata**. Its source cannot supply a commit SHA. In the actual repository:

1. Read `AGENTS.md` and `.codex/agents/*.toml`; check `git status --short`, current branch, HEAD, remotes, untracked files, and origin state.
2. Verify the checkout corresponds to the uploaded ZIP (compare `Cargo.toml`, the cited code paths, and the source tree as needed). A material mismatch is a **source-integrity blocker**: do not silently implement against a different revision. Consult the supplied source archive or establish the correct commit first.
3. Preserve existing user work and worktree ownership. Do not reset, stash, overwrite, clean, rebase, force-push or silently move unrelated changes.
4. When on the matching base with a safe worktree, create/switch `performance-optimization`. If the branch exists, inspect it and **continue only if its ancestry and intent match**; never delete or overwrite it automatically.
5. Do **not** `git pull master` as a prerequisite — the approved base is the matching latest source, not an unverified remote master.
6. Commit after a meaningful complete checkpoint, using `perf(scope): [M#-X] ...`, `refactor(scope): ...` or `test(perf): ...` as appropriate; run the checkpoint's relevant validation first, not every test target.
7. **Push each completed checkpoint** to the feature branch with an ordinary push. If remote/permission/push fails, keep the local commit, log the failure and retry after resolving only the actual blocker. Never force push or claim that a push succeeded without confirmation.
8. Keep benchmark reports and planning artifacts in a sensible tracked location (`docs/performance/` and/or `docs/plans/`) in the target repository; these output files are ready to copy. Avoid inadvertently committing real user data or massive generated benchmarks.

## A5. Codex agent contracts

- **Orchestrator:** owns scope/gates, milestone packet definition, progress ledger, sequencing, review integration, commits and pushes. It performs narrow integration edits if appropriate, but does not ask an implementer to recreate the plan.
- **Planner:** check changed ownership boundaries only, propose milestone-local design tradeoffs and precise invalidation/acceptance paths. Existing `.codex/agents/planner.toml` is the primary configuration.
- **Implementer:** receives a specific `M#-X` packet containing changed files, algorithm/ownership, behavior invariants, non-goals, tests and done criteria. It does not autonomously start another milestone.
- **Reviewer:** independent read-only diff review for risky transitions (note revision correctness, history semantics, virtualization/keyboard, worker generations and native presentation ordering). Fix substantive findings before marking checkpoint complete.
- Avoid multiple agents editing the same files concurrently. Allow independent read-only analysis in parallel; serialize overlapping code changes. Use established repository agent configurations/model choices instead of inventing incompatible per-task model settings.

## A6. Verification budget

**Default:** local formatting/static inspection plus the **fewest relevant unit/integration tests**, using `cargo nextest`. Cargo target selection must precede test-name filtering when practical. The following are representative syntax patterns, not a demand to run every example on every checkpoint:

```powershell
# Filter within library tests when changes live in src/ modules.
cargo nextest run --lib -E 'test(note_cache)'

# Select an integration target before filtering test names.
cargo nextest run --test notes_plugin
cargo nextest run --test watchers
cargo nextest run --test history

# Scoped fallback (use if Nextest filtering/target behavior differs on host).
cargo test --lib note_cache
```

Before using these, inspect `cargo nextest list`/`cargo test -- --list` if a filter or target is unclear. Do **not** substitute a bare test-name substring that causes Cargo to build all integration binaries. Add focused mocks/egui frame tests for UI and state; use real-Windows smoke tests when native behavior cannot be proved headlessly. Do not run the whole suite after each checkpoint, and do not add redundant tests solely to satisfy a quota. Check long-running builds at a relaxed **10–20 minute cadence when appropriate**, not frantic short polling; this is a polling preference, not permission to leave work running unobserved forever.

**Completion discipline:** each checkpoint must identify code/results, targeted tests attempted/passed/failed, metric evidence and follow-ups; only then commit/push. If a required test cannot run on the execution host, label it **NOT RUN — requires Windows/remote host** and add a real verification action to the next appropriate gate. Do not mark it passed.

---

# B. Measurement design and artifact structure

## B1. Metric families

- **UI latency:** relevant component's p50, p95, maximum duration, plus per-frame widget construction count. Include egui root/notes widget timing only while that surface is active; distinguish frame rate from time spent rendering each widget.
- **Redundant work:** note snapshots and copied-byte estimates; alias hash runs; history candidates copied/resolved; calls to `commands_filtered`; results rows/widgets constructed; index rescans and GUI-thread blocked duration; mouse source presentations and refreshes per tick.
- **Memory and resources:** allocated bytes where a reliable measurement tool is available, otherwise explicitly call them *estimated bytes/clones*; GDI handle count, retained native surfaces; worker count.
- **Correctness:** identical ordering/identity and content hash (privacy-safe), callback behavior, pointer-to-overlay motion latency, stationary magnifier refresh, scroll-to-selection.
- **Reliability:** stale job rejection, last-good retention, watcher errors, cancelled jobs, window/DPI topology and resource tear-down.

## B2. Controlled workload matrix

| Workload | Sizes | Variants / events | Expected observation |
| --- | --- | --- | --- |
| Notes heavy-derived refresh | 100 / 1,000 / 5,000 | short/long markdown; aliases and 0/many backlinks; idle repaints; drafts; external edits | total snapshots/checks and tail latency |
| Dashboard history | 100 / 1,000 / 10,000 | normal/pins-only, empty vs active filter, 8 vs 50 rows, renamed/deleted actions | resolution/build count and frame timing |
| Launcher list and grid | 100 / 1,000 / 10,000 | list, 2–6 grid columns, selection, keyboard, context menu, DPI resize | widget count vs visible rows, p95 render |
| Quick Notes | 100 / 1,000 / 5,000 | large bodies, long wrapped previews, tags/checkboxes/backlinks, search | derived metadata/rows visible |
| Filesystem indexing | 1k / 10k; optionally 100k | custom Actions reload; settings root/limit change; slow/invalid paths; rapid writes | GUI stall, scan duration, stale suppression |
| Cursor effects | halo/zoom/both | moving pointer vs stationary changing video; monitor changes; recover/fallback | source presentations per worker tick, latency |
| Coordinate HUD | HUD-only / all modes | 16ms cadence, DPI change, foreground move/resize, copy | API time, GDI creations, correctness |

Seed test data deterministically. Use temporary data directories and an isolated process for any shared path/env-state fixtures; no private note text, clipboard contents or history payloads in reports. Record sample count, warm-up iterations, OS/CPU/Rust profile, monitor/DPI conditions and all deviations. Run on the same machine and configuration when comparing.

## B3. Measurement gates

- **G0 Baseline:** instrument + capture before-change numbers for each realistically reproducible workload; mark otherwise `NOT MEASURED` and provide a reproducible command/procedure.
- **G1 Per-checkpoint:** code-level count/correctness tests prove specific redundant work removed; collect comparable focused timings where possible.
- **G2 Milestone:** compare p50/p95 and counts; proceed only if behavior is intact, no alarming small-workload regression, and the optimization is justified by measurement or a simple obviously redundant operation.
- **G3 Final:** run selected cross-surface tests and actual Win32 smoke checks, capture same workload matrix and publish a concise before/after + confidence/limitations report.

No mandatory blanket `>=20%` target and no fabricated metrics. A conditional refinement may be skipped if evidence is weak. A suspected regression requires explanation and fix/reversion of that checkpoint before proceeding.

## B4. Reports to maintain in the target repository

- `docs/performance/track_a_baseline.md`: environment, source SHA, telemetry definitions, scenarios, reproducible commands and baseline values or `NOT MEASURED`.
- `docs/performance/track_a_results.md`: rolling milestone metrics, correctness gates, source SHA comparisons and final before/after table.
- `docs/performance/track_a_decisions.md` (optional, only if genuinely useful): notable rejected caching/refactor ideas and evidence.
- Prefer machine-readable, **small, bounded** CSV/JSON summaries only where automation needs them. Do not commit raw traces of unlimited size, personal data or generated index trees.

---

# C. Execution sequence and milestone packets

## Overview / recommended checkpoint cadence

| Phase | Coherent checkpoints | Main gate |
| --- | --- | --- |
| M0 — Measure current behavior | M0-A instrumentation, M0-B fixtures, M0-C baseline | Reproducible baseline exists |
| M1 — Note revision fast path | M1-A publication contract, M1-B refresh rewrite, M1-C regression/measurement | No note-cache copy on unchanged frame |
| M2 — History resolution | M2-A command lookup, M2-B bounded preparation, M2-C behavior proof | Only displayed candidates resolved as needed |
| M3 — Virtualization | M3-A list, M3-B grid, M3-C Quick Notes metadata, M3-D variable-height virtualization and test | Visible-row work; no navigation/menu regressions |
| M4 — Watch/index separation | M4-A reuse index, M4-B worker, M4-C generations/triggers, M4-D tests/measure | No full scan on GUI thread |
| M5 — Cursor refresh order | M5-A event/render contract tests, M5-B one-stage refresh, M5-C native correctness/perf | No stale moving presentation; stationary refresh intact |
| M6 — Conditional sampler/HUD | M6-A measurements, M6-B conditional sampling, M6-C conditional GDI retention | Justified improvement only, or documented skip |
| M7 — Validate and close | M7-A cross-cutting correctness, M7-B final perf report/cleanup | Auditable before/after and reviewed branch |

### M0 — Baseline instrumentation and benchmark harness

**Owner:** `src/performance.rs` and narrow measurement call sites. **Dependency:** verified matching branch + `AGENTS.md`. **Do not** change runtime behavior yet.

#### M0-A — Add opt-in, bounded subsystem metrics

**Implementation**

1. Inspect existing `performance::enabled`, `Timer`, `record_frame`, search timings and tracing filters. Reuse the same environment flag. Decide whether per-component aggregated counters or trace spans are more helpful; keep the disabled path constant/cheap.
2. Define stable metric names/tags for `note.refresh_check`, `note.snapshot`, `note.alias_hash`, `history.prepare`, `history.resolve`, `history.catalog_build`, `launcher.rows_built`, `quick_notes.rows_built`, `actions.reload`, `index.scan`, `coordinate.sample`, `effects.refresh_source`, `effects.present_source`, and `hud.gdi_create`. Use short enums/static strings where practical; do not allocate strings per idle frame.
3. Add instrumentation at the true work-owner boundaries, not generic `update()` timing alone. Count actual snapshots and iteration/batch counts; time mutex hold separately from post-lock work when feasible.
4. Bound metric aggregation memory; avoid per-frame log spam, blocking I/O on UI thread, per-action filenames and unbounded cardinality. Disable any verbose per-tick native event output in production.
5. Define reset/snapshot of counters for controlled tests where useful. Add tests showing disabled path has no observable metric side effects and enabled counters are correct.

**Verification:** focused `src/performance.rs` tests, one library `cargo nextest --lib` filter, simple startup/idle smoke. **Acceptance:** every metric has an owner/definition; zero private payloads; baseline implementation behavior unchanged; logging opt-in.  
**Suggested commit:** `perf(instrumentation): [M0-A] add bounded runtime performance counters`.

#### M0-B — Build deterministic, isolated workloads

**Implementation**

1. Create helper builders or focused Criterion/egui test harnesses that produce seeded synthetic note caches (aliases, backlinks, todo links, long bodies), history entries/pins, launcher actions and directory trees without accessing the user's real data.
2. Implement at least the 100/1k/5k note, 100/1k/10k history, and 100/1k/10k result scenarios, with edge variants listed in B2. For index scans, start with a small temp root and an optional larger stress variant. Native mouse test seams should use the existing fake operations rather than direct Windows calls for CI-like tests.
3. Record constructed work size and approximate bytes so measurements can be reproduced; explicitly separate data generation time from the target operation.
4. Ensure isolation of global paths (`ML_NOTES_DIR`, existing `TEST_MUTEX` conventions), cache state and history files. Preserve tests' deterministic ordering and cleanup.
5. Use an egui `Context` frame harness where needed to count built rows and simulate selection/scroll. A screenshot or manual smoke can complement but not replace deterministic identity assertions.

**Verification:** run the fixture/harness tests, inspect generated output sizes, one small scenario without profiling. **Acceptance:** stable seed yields stable identities/counts; no real profile writes, no test-global cross-contamination.  
**Suggested commit:** `test(perf): [M0-B] add synthetic runtime workload fixtures`.

#### M0-C — Record G0 baseline, freeze acceptance expectations

**Implementation**

1. Capture source SHA, test host and profile, telemetry settings, warmup steps, repeat count, and current behavior of every scenario. Include optional native runs when available. Mark unavailable native/window measurements unmeasured rather than inventing them.
2. Record **current rendered action order and note/history snapshot signatures** without sensitive data; establish baseline selected index/scroll/context action behavior.
3. Capture idle and active CPU/worker/GDI measurements for mouse modes, including all-off state; count native source submissions per moving and stationary tick.
4. Produce `docs/performance/track_a_baseline.md` and initialize the results table; explicitly list which experiments will be performed later on the actual Windows machine.
5. Review baseline for unexpected cost distribution. Reorder checkpoints only when justified by evidence; keep approved M1–M5 goals and document any change.

**Verification:** reproduce one scenario twice and confirm stable ordering and a plausible timing spread; do not rewrite an unmeasured baseline after optimization. **Acceptance:** every improvement has a reproducible comparison or is marked pending/unmeasured.  
**Suggested commit:** `docs(perf): [M0-C] record Track A baseline and verification gates`.

### M1 — Eliminate unnecessary note snapshots and alias hashing

**Owner:** `src/plugins/note.rs`, `src/gui/note_panel.rs`; relevant tests. **Dependencies:** M0.

#### M1-A — Make cache publication revision contract explicit and testable

**Implementation**

1. Trace note cache publication through `refresh_cache`, note save/append/delete, bulk updates, file watcher and failed loads. Identify every mutation that can change note content, alias/slug, tags or backlinks.
2. Codify revision semantics: successful materially changed published content increments exactly when intended; unchanged reload does not; errors/rolled-back edits do not; external refresh eventually changes revision after publication.
3. Audit `publish_note_cache`'s `Mutex` and `NOTE_VERSION` ordering. Provide an **internally consistent versioned snapshot helper** only if needed for recomputation. Never tag one revision onto another revision's note data during concurrent publication. Consider locking briefly to read metadata/version, then clone only on a true refresh; do not add another mutable data store.
4. Test note alias rename/removal, second alias (where relevant), slug move, backlinks, changed body, equal cache reload, failed save/reload and concurrent writer/reader behavior at a deterministic seam. Verify the existing link-menu revision cache remains correct.
5. Decide explicitly whether `last_alias_map_hash` can be deleted or replaced with a more precise cheap metadata revision. Keep the old hash until tests establish that the new guard covers all relevant invalidation.

**Verification:** relevant `src/plugins/note.rs` unit tests and `tests/notes_plugin.rs` as required. **Acceptance:** authoritatively versioned metadata, unchanged refresh is a no-op; failures preserve old revision/state; race test cannot pair inconsistent data/version.  
**Suggested commit:** `refactor(notes): [M1-A] formalize note cache revision publication`.

#### M1-B — Move the cheap check before expensive derived work

**Implementation**

1. In `NotePanel::maybe_refresh_heavy_derived`, first compare `note_version`, `todo_version` and local dirty/debounce/forced-refresh state. Avoid `note_cache_snapshot`, alias gathering/sorting and heavy recomputation when all inputs are unchanged.
2. In `refresh_heavy_derived`, take the note snapshot only after confirming a genuine refresh is needed, with the correct consistency contract established in M1-A. Derive linked todos/related notes/mentions and labels as before; do not alter backlink grouping, order or text.
3. Preserve the 250ms heavy edit debounce, `backlinks_enabled` handling, settings transitions and force refresh paths. **Important:** note panel content may be unsaved; persisted version cannot be the sole trigger for local edits.
4. Remove redundant `last_alias_map_hash` state/function only after all callers/tests are migrated, or retain a cheap per-revision alias guard if semantically required. Avoid redundant full-panel-content hashes for empty idle work if a reliable dirty generation already exists, but keep that secondary optimization optional and measured.
5. Add lightweight counters/test hooks showing (a) unchanged frames trigger zero cache clones and (b) one change triggers one meaningful rebuild rather than repeated recomputation.

**Verification:** targeted panel derived/backlink tests, edited unsaved content, todo-link change, alias rename, settings enable/disable, external reload with open editor. **Acceptance:** no O(total notes) copying on unchanged check; derived output bit-for-bit/row-for-row equivalent; no starvation of pending debounce.  
**Suggested commit:** `perf(notes): [M1-B] gate heavy note refresh on revisions`.

#### M1-C — Prove notes behavior and record G1 comparison

**Implementation**

1. Compare synthetic small/medium/large note check counts and timings to M0. Report before/after snapshot count, clone bytes estimates, lock hold and p50/p95 check time with enough samples.
2. Exercise external create/edit/delete/rename while panel is open and with links/aliases; verify cache version and both backlink views update. Test loading missing/invalid note files and last-good state.
3. Regression-test unsaved drafts, checkbox edits, note-save-on-close, todo changes, alias collisions, link-menu cache and panel visibility. Use existing tests before writing broader new ones.
4. Resolve reviewer findings; keep evidence in results report and commit only coherent changes.

**Verification:** narrow `--lib` note panel tests; `--test notes_plugin` and only extra tests exercising touched integration behavior. **Acceptance:** corrected cache refresh and no semantic changes; report has actual metrics or `NOT MEASURED` reason.  
**Suggested commit:** `test(notes): [M1-C] lock down note revision and refresh regressions`.

### M2 — Prepare only dashboard history entries that are needed

**Owner:** `src/dashboard/widgets/command_history.rs`, optionally `src/dashboard/dashboard.rs`, `src/gui/render.rs`, `src/gui/search.rs`. **Dependencies:** M0.

#### M2-A — Reuse command catalog/lookup per relevant refresh

**Implementation**

1. Document current `resolve_action()` **precedence**: snippets, `actions_by_id`, plugin commands, processes, favorites, notes, clipboard, todo, snippet edit/remove, legacy opaque clipboard. Be careful with action `args` and duplicates.
2. Inspect the already-existing `LauncherApp.command_cache` and how `update_command_cache()` refreshes it when enabled plugins/settings change. Prefer presenting this authoritative command slice/lookup through `DashboardContext` over rerunning `PluginManager::commands_filtered()` per unresolved history entry. If a map is justified, prepare it at the same invalidation boundary and preserve the current first-match behavior for duplicate `(action_id,args)` keys.
3. Take one `DashboardDataSnapshot` per history preparation, not per `resolve_action` call. Use borrowed/`Arc` data where possible. Avoid copying all notes/processes/favorites just to search them.
4. Provide a pure resolver function taking an explicit resolution context so tests can compare its output with the previous implementation for ordinary and legacy actions.
5. Cover enabled/disabled plugin transitions, command rebuilds, action rename/delete, snippets with prompted fields, missing pins, favorite/process/note/todo changes and opaque clipboard compatibility. **Do not** cache a command map forever without relevant invalidation.

**Verification:** command-history unit tests, narrow plugin command-cache tests. **Acceptance:** no per-entry plugin command enumeration; unchanged data can reuse lookup safely; resolution precedence identical.  
**Suggested commit:** `perf(history): [M2-A] reuse one command resolution catalog`.

#### M2-B — Stream candidates until enough filtered rows exist

**Implementation**

1. Preserve pinned-only mode: never even request/clone regular history when `show_pinned_only` is true. In mixed mode, sort pinned rows by reverse timestamp and then stream unpinned history in its current deque order.
2. Match pins by `action_id` plus `args` (the existing equality semantics), not label/timestamp. Avoid naive `Vec::contains` comparisons on large histories; a `HashSet` of existing identity pairs may be appropriate if filtering and memory measurements justify it.
3. Prepare candidates incrementally: resolve, apply current filter to **resolved** label and query, retain match, stop when `cfg.count` matches have been assembled. Never take first N candidates before filtering. Respect count UI range, including test construction outside UI clamp if relevant.
4. Avoid locking global history during plugin resolution or egui calls: safely clone a bounded batch of input records and release the lock, or isolate filtering/preparation under a short snapshot protocol. Optimize the common empty-filter/8-row case while handling sparse matches correctly.
5. Preserve fallback saved presentation for history, missing-label/unpin affordances for unavailable pins, pin toggling and timestamps; a changed pin must appear on subsequent render without a stale cache.
6. Add counter assertions for: (a) pinned-only does not read history, (b) eight rows with no filter resolve no more candidates than needed, (c) rare match can scan beyond eight to produce eight valid rows, (d) plugin catalogs are not regenerated during that scan.

**Verification:** widget pure tests covering pinned, filtered, missing, duplicate, empty and 10k synthetic records. **Acceptance:** less preparation work without changing visible results or action execution.  
**Suggested commit:** `perf(history): [M2-B] bound history resolution by visible matches`.

#### M2-C — Revalidate history semantics and compare performance

**Implementation**

1. Use the baseline workload with 100/1k/10k entries, 0/8/many pins, empty/rare filters, count 8/50 and enabled/disabled plugin changes.
2. Compare visible ordered `DisplayEntry` identities, labels, missing flags, timestamps and query overrides against the old implementation. Exercise pin/unpin, action rename, deleted note or snippet, and legacy clipboard entries.
3. Record resolved entries, plugin catalog enumerations, clones and p50/p95 preparation time. Investigate small-data regressions rather than claiming raw latency improvements from counters alone.
4. Reviewer checks cache invalidation and accidental fallback behavior changes; resolve material findings.

**Verification:** history widget tests and `cargo nextest run --test history` where relevant. **Acceptance:** stable semantics plus measured reduction in redundant work.  
**Suggested commit:** `test(history): [M2-C] prove command history parity and savings`.

### M3 — Virtualize launcher and Quick Notes lists

**Owner:** `src/gui/render.rs`, `src/gui/mod.rs`, `src/gui/notes_dialog.rs`, `src/plugins/note.rs`; note exact UI ownership before modifying. **Dependencies:** M0; M1 preferred for note data.

#### M3-A — Virtualize the root *list* by visible rows

**Implementation**

1. Capture current list geometry under `ScrollArea::vertical`, `scale_ui`, widths, `SelectableLabel` height, tooltip and folder alias formatting. Choose a stable row-height presentation consistent with current egui 0.27 visual behavior; do not silently change padding or row hit targets.
2. Use `show_rows` or an equivalent viewport-based range calculation with a bounded overscan. Loop only over the visible **absolute action indices**; clone a result action only when needed for a visible widget or deferred activation.
3. Preserve timer tooltip generation and `attach_result_context_menu` for every visible row; do not execute a context action in a closure after its borrowed `Action` is invalidated. Keep tracing/action dispatch hooks and all `DeferredActivation` structures intact.
4. Make keyboard-driven selection of an initially off-screen result request/compute an appropriate scroll location **before** the selected row is required to produce a response. Avoid depending exclusively on `scroll_to_me()` for a widget that has not been built this frame.
5. Handle zero results, one result, search changes, filtered category changes, variable window widths and list scaling. Ensure final selected index is stable under a query update; avoid stale scroll offsets on new result sets.
6. Add a fake egui test that measures built row count at 10k results and verifies selection/scroll and action identity for first/middle/last rows.

**Verification:** `src/gui/render.rs` navigation tests and targeted context menu/action tests. **Acceptance:** `rows_built` grows with viewport height/overscan, **not** with 10k result count; list behavior unchanged.  
**Suggested commit:** `perf(launcher): [M3-A] virtualize visible list results`.

#### M3-B — Virtualize root *grid* by complete rows

**Implementation**

1. Derive row count from `self.results.len()` and `self.query_results_layout.cols.max(1)` safely, including an incomplete final row. Use row boundaries to map viewport rows to absolute result indices; retain current per-cell `col_width`, 44px cell height, 8px/6px spacing and `scale_ui` behavior.
2. Use egui row virtualization with stable height and `Grid::new("query_results_grid")` / equivalent layout only for visible complete rows. Preserve left-to-right order, aligned columns and unused space in final row without creating selectable empty actions.
3. Preserve exact selected index and keyboard column arithmetic in `src/gui/mod.rs`, including moving between incomplete rows and scrolling selection into view. Explicitly test scenarios with 1, 2, 3, 5 and 6 columns and `N % cols != 0`.
4. Attach context menus only to actual rendered actions, maintaining IDs, pointer hit targets, radial add/action sheet choices and activation source. Test a first-row vs far-last-row mouse activation and selection.
5. Recompute virtualization metrics and scroll mapping after changing window width, DPI, `list_scale`, grid/list mode and resolved column count. Avoid visible jitter due to inaccurate spacer height.
6. Count visible cells created and assert bounded overhead when 10k results are present.

**Verification:** grid/list navigation equivalence test and new viewport tests. **Acceptance:** current grid layout and all keyboard/action behavior preserved; off-screen cells do not build widgets.  
**Suggested commit:** `perf(launcher): [M3-B] virtualize launcher grid by rows`.

#### M3-C — Cache Quick Notes projection and lightweight row metadata

**Implementation**

1. Separate **all note entries** from a filtered projection of original entry indices. Build/rebuild that projection when `entries`, `search`, search index or relevant settings/data revisions change; do not use the projection index as `edit_idx` or deletion target.
2. Refactor `short_preview()` to normalize/truncate at 120 Unicode characters **without first concatenating the full note**. Preserve heading/`Alias:` line exclusion, whitespace collapsing and ellipsis semantics. Compare on synthetic Unicode, blank/large, CRLF and long-line inputs.
3. Introduce a count-only `note_backlink_count(slug)` API for the metadata path; it should read the backlink map without looking up/cloning linked `Note` bodies. Keep `note_backlinks(slug)` unchanged for callers needing objects. Regression-test parity with the previous count, including missing slugs and duplicate relationships.
4. Build lightweight stable row metadata: display name, slug, formatted tags, backlink count, checkbox count, truncated preview, original entry index and any presentation flags needed. Cache by a correct entry revision plus settings that affect display; avoid rebuilding all metadata on an unchanged frame.
5. Keep expensive checkbox count/long content scans out of per-frame render; on a changed note, invalidate only the affected row when possible (or rebuild on a published notes revision if that is simpler). Verify external and local edits invalidate the right cache.
6. Preserve Quick Notes context menu actions, hovered preview text, template/todo submenu contents and exact index identity. Full content should be cloned only on edit/open/copy actions that genuinely need it, not merely to paint a row.

**Verification:** Quick Notes preview/count tests, note backlinks tests, index/filtered-row identity tests. **Acceptance:** invisible notes are not cloned every frame; metadata is up to date; preview strings match baseline.  
**Suggested commit:** `perf(notes-ui): [M3-C] prepare lightweight filtered note rows`.

#### M3-D — Virtualize Quick Notes while preserving variable heights

**Implementation**

1. Measure current Quick Notes presentation height per note for normal and wrapped metadata/preview under narrow/wide windows and different DPI/text scales. `show_rows` fixed-height usage is **not automatically correct** for this UI.
2. Implement an accurate or conservatively estimated variable-height viewport projection. Acceptable approaches include measured-height cache with prefix-sum offsets, stable height buckets and local measurement, or another documented algorithm that preserves scroll/hover/click geometry. Do not switch the UI to a new compact fixed-height design merely to simplify virtualization.
3. Include modest overscan to avoid pop-in while scrolling; ensure cached offsets/heights invalidate on width, text scale, font, visibility toggles, tags/preview changes or row insertion/deletion. Preserve scroll anchor when live metadata changes, rather than jumping unexpectedly to the top.
4. Render the current note rows with the existing title/meta/preview and context menu commands, passing original `entries` index to callbacks. Confirm that clicking the last filtered row selects the right note even if most original rows were hidden.
5. Preserve both-axis scroll semantics if current content can require horizontal space; avoid phantom scrollbar heights or clipped popups. Test scroll-to-bottom and viewport resize.
6. Measure 100/1k/5k note scenarios at different viewport heights. If variable-height logic is too costly/unstable, use an explicitly documented safer bounded approach, report the tradeoff, and **do not falsely mark full virtualization complete**.

**Verification:** synthetic egui frames + direct manual Windows UI tests (scrolling, hover, right-click, edit/delete). **Acceptance:** only visible/overscan Quick Notes rows built; stable scroll geometry; no regression to content/actions.  
**Suggested commit:** `perf(notes-ui): [M3-D] virtualize variable-height note browsing`.

#### M3 milestone review / measurement gate

Compare M0/M3 grid/list/notes results: widgets built per frame, total cloned actions/notes, preview characters scanned, p50/p95 render time, memory allocation estimate when available, and small-list overhead. Run a focused root keyboard + universal context menu smoke, not the entire repository. Any selected-index or popup identity regression is a **blocking failure**.

### M4 — Separate custom Actions reload from indexed-path scanning

**Owner:** `src/gui/watch.rs`, `src/gui/mod.rs`, `src/indexer.rs` and event/coordinator state. **Dependency:** M0; can be implemented after M3 or earlier if baseline shows clear UI stalls. **High-risk:** asynchronous ownership and stale result publication.

#### M4-A — Reuse current indexed actions for Actions-only reloads

**Implementation**

1. In `WatchEvent::Actions`, retain existing typed-load/error/unchanged handling (`load_actions_typed`, `LoadState`, `actions_persistence_diagnostic`). Its current early return on identical custom action data is intentional.
2. Change the **changed custom actions** branch to reuse the existing indexed tail from `self.actions[self.custom_len.min(self.actions.len())..]`, as the local `update_custom_actions()` path already does. Do not run `index_paths_batched` in this event handler.
3. Prefer one internal routine for merging current custom actions + current indexed actions, so watcher/local save and eventual index completion share action-cache publishing logic. Avoid duplicating `custom_len` bookkeeping or version bump behavior.
4. Preserve `publish_actions()` semantics: custom segment first, indexed segment after, updates to `Arc<Vec<Action>>`, action ID/search caches and appropriate query refresh. Preserve the single correct `actions_version()` bump on externally changed data; avoid local-save/watch double bumps.
5. Add an injectable indexer counter/test seam proving that changing custom action data makes **zero path scan calls**. Assert that indexed action order/IDs and configured max counts are unchanged, even with malformed/missing custom JSON and same-content watcher bursts.

**Verification:** `src/gui/mod.rs` existing Actions-watcher tests, `tests/watchers.rs`, `tests/watcher_failures.rs`. **Acceptance:** a custom reload never traverses index roots, returns promptly, and retains indexing and persistence semantics.  
**Suggested commit:** `perf(actions): [M4-A] reload custom actions without path rescans`.

#### M4-B — Introduce a bounded off-thread scan coordinator

**Implementation**

1. Identify existing event-channel/wake and worker patterns (`src/gui/state.rs` `WatchEvent`, `register_event_sender_with_wake`, existing indexer batches). Select **one job coordinator** with an explicit owner in the app lifecycle; do not create a thread for every notify event.
2. Define a request shape with `generation: u64`, roots, max items, and enough config identity to determine whether the result remains valid. Define completion/error event(s) carrying generation and either a complete indexed `Vec<Action>` or failure status. Avoid unbounded queues for index batches; prefer local worker aggregation with a bounded final-result message if behavior permits.
3. Move the traversal of `IndexBatchIter` to a worker. Check supersession/cancellation every batch (the default batch size is 512) and before publication, and cap indexes by the existing max. Do not block the GUI thread waiting for worker results or doing `JoinHandle::join` in root-render path.
4. Preserve `walkdir`, canonical-path dedup, filename/action label/desc and max count semantics. Handle errors explicitly: either retain the last complete valid index or document a deliberate bounded partial-results policy approved by tests; **default preference is last-good index on an unsuccessful scan**.
5. Register completion through the established app event sink and wake mechanism, then apply on the GUI owner thread only. Maintain ownership of worker shutdown/cancellation and ensure app exit cannot strand uncontrolled filesystem threads. Observe repository one-data-directory-instance ownership rules.
6. Add deterministic tests with a fake scanner that can stall, fail, complete in a chosen order and report scan counts. A synthetic directory smoke should assert that indexing itself is off the GUI thread.

**Verification:** coordinator test target/library unit tests, one indexer integration/synthetic fixture; review sender lifetimes. **Acceptance:** long scan no longer blocks rendering; bounded active worker and queues; no partial bad publication; no leaked thread/resources.  
**Suggested commit:** `perf(indexer): [M4-B] move indexed-path scans to bounded worker`.

#### M4-C — Add generation rejection and correct refresh triggers

**Implementation**

1. Map all changes that invalidate indexed actions: application startup, configured indexed roots, `max_indexed_items`, an explicit refresh operation if available, and any supported settings reload. **Inspect actual call sites** first; do not assume `update_paths()` alone schedules indexing today.
2. On each necessary reindex request, advance a monotonically increasing generation; capture exact root/limit config and coalesce rapid changes. Decide explicit ordering when the user edits custom Actions data during a scan.
3. On completion, reject all mismatched generations/configs and anything delivered after shutdown. On accepted success, merge **the most recently published custom actions** with newly indexed paths (never restore a stale custom segment captured with the job). On failure, keep the latest good indexed paths and expose an appropriate diagnostic/retry policy without per-frame error spam.
4. Ensure a custom-actions reload **while a reindex is pending** updates custom results immediately and preserves the old index until the new index is valid. Ensure a succeeding scan does not erase newer custom edits.
5. Verify startup timing/readiness: a first scan is allowed in background if last-good/empty indexed entries are not yet available; do not declare stale cached results current. Track loading/ready/failure status internally only if required for correct behavior. Do not add a new user-facing UI unless essential to prevent misleading data.
6. Cover watcher duplicate events, changed roots mid-scan, changed max-items mid-scan, error-after-some-batches, inaccessible directories, concurrent completion, rapid writes, and app shutdown. Consider whether actions-version bumps should represent accepted index changes, and test no double bump from an unchanged result.

**Verification:** event-coordinator tests plus `watchers`, `watcher_failures`, relevant `src/gui/mod.rs` tests. **Acceptance:** stale scan cannot overwrite current indexed config or newer custom actions; last-good survives failures; necessary refreshes occur exactly as designed.  
**Suggested commit:** `fix(indexer): [M4-C] reject stale scans and preserve live actions`.

#### M4-D — Benchmark stalls and regression-test the full watcher path

**Implementation**

1. Run changed/custom Actions reload cases with 1k/10k indexed synthetic files and optional slow/large directories; capture max/p95 GUI-event duration, scan count and scanner queue counts.
2. Confirm no per-frame/blocking disk traversal or expensive `join` has migrated to another UI-path function. Keep any direct index APIs for non-GUI workflows if still needed.
3. Exercise invalid JSON/missing file recovery, duplicate events, local save + watcher, root reorder, symlink/canonicalization, max cap and partial scan error behavior.
4. Verify when the index is updated, results/search cache, dashboard action IDs, history lookups and radial action catalog observe the proper version and result ordering.
5. Review with the independent reviewer, especially concurrency and merge points. Record throughput/stall numbers and ensure unchanged Actions reload still has no action-cache churn.

**Verification:** relevant watcher/integration tests, synthetic scan benchmark, Windows manual slow directory smoke. **Acceptance:** old GUI-thread rescans eliminated and observable behavior correct under racing changes.  
**Suggested commit:** `test(indexer): [M4-D] verify nonblocking reload and stale-job recovery`.

### M5 — Consolidate moving cursor-effect presentations

**Owner:** `src/coordinate_tool/controller.rs`, `src/coordinate_tool/native.rs`, `src/coordinate_tool/native_effects.rs`; optional narrow presentation interface. **Dependencies:** M0 telemetry; M4 unrelated. **High-risk:** Win32 magnification capture and failure lifecycle.

#### M5-A — Specify and test the single-tick presentation contract

**Implementation**

1. Diagram actual worker tick: `recv_timeout(16ms)` → `backend.poll_events()` → sample live cursor → `render()` on changed/forced/retry. Confirm `poll_events()` calls `effects.poll_visible_sources()` using cached geometry **before** new sample is available; `render` can then reconcile current geometry.
2. Add one fake-backend or fake-operations test that counts per-tick `refresh_visible_source` and `present_live_source` for halo and zoom independently. Explicitly identify ordinary moving tick, stationary changing underlying content, invalid sample, toggles, topological refresh and failed native presentation.
3. Record baseline native source-submission/invalidations counts with M0 metrics and identify whether a repeated present/refresh is truly caused by the old cached-source poll. Avoid asserting that GPU work doubles; these counters describe API submissions only.
4. Write invariants into test names/fixtures: moving tick uses latest successful sample once; stationary active magnifier still refreshes every appropriate tick; disabled/hidden effects never refresh; invalid sample never reuses frozen or placement sample; filter list must include all visible overlays before capture.
5. Inspect halo fallback separately from native Magnification child: preserve its own outline/refresh behavior, and ensure failures in one effect do not inadvertently hide a sibling.

**Verification:** targeted `native_effects` lifecycle tests and `controller` fake worker tests. **Acceptance:** tests capture duplicate-path regression before production rewrite and define semantics for sample failure/topology.  
**Suggested commit:** `test(mouse): [M5-A] characterize moving and stationary effect refreshes`.

#### M5-B — Reorder pump/sample/presentation to avoid outdated source refresh

**Implementation**

1. Separate event processing/topology signals from presentation. In `WindowsSurfaceBackend`, do not automatically present cached effect source in `poll_events()` before the current live sample is known **if that same tick will present updated geometry**.
2. Sample cursor and determine frame invalidation. Consolidate native effect work into a controlled per-tick phase that first handles cheap surfaces, outline staging, exclusion-filter rebuild and topology state, then refreshes/presents the correct current effect source at most once per requested visible effect in normal movement. Retain existing `CursorEffectsRuntime::reconcile()` ownership of native sessions/failures and only restructure its call protocol as needed.
3. On a *stationary* tick, the absence of a changed HUD/crosshair frame must **not** skip refreshing visible magnified content: refresh from the current successful validated live geometry or equivalent safely proved cached geometry without a second stale submission.
4. On sample error, never fall back to `placement_sample`/frozen HUD to drive effects. Pause/hide invalid geometry correctly; preserve retry/pause state and no background refresh on hidden/unavailable effects.
5. Preserve filter-input ordering: stage visible cheap/outline geometry first, install updated exclusion lists before a native magnification source update, restack appropriately, and do not break halo/zoom co-existence or topology/DPI notifications.
6. Prefer an explicit worker tick API (`pump_events` / `reconcile_frame` / `refresh_stationary_sources` or a small flags/result contract) only if it actually clarifies correctness; do not build an elaborate rendering scheduler.
7. Keep changes localized and add assertions that moving ticks submit no old-coordinate source. Verify that `effects_status` publication still reflects latest worker-side errors and that last mode disabled still stops/joins worker.

**Verification:** tests from M5-A and affected `native_effects` regressions; when available run `coordinate_tool_smoke`/`cursor_effects_smoke` on Windows. **Acceptance:** no redundant stale moving submission; current geometry and exclusions correct; stationary updates and all lifecycle semantics preserved.  
**Suggested commit:** `perf(mouse): [M5-B] present each active effect once per worker tick`.

#### M5-C — Native effect reliability, perf report and reviewer gate

**Implementation**

1. Exercise movement latency and source counts for halo-only, zoom-only, both effects, crosshair/HUD additions and system cursor stationary over changing video/animated page.
2. Re-run or adapt existing lifecycle tests covering `stationary_poll_refreshes_only_visible_requested_effects_without_reconciling`, `filter_input_invalidation_refreshes_once_without_cursor_movement_churn`, `topology_refresh_rebuilds_complete_filter_lists_and_live_only_refreshes_visible_surfaces`, zoom clipping and stationary refresh failure latches. Old tests may need updates to observe the new phase while preserving intent; never weaken their behavioral assertion.
3. Test topology invalidation while moving between monitors/DPI scales, fallback halo with zoom, paused/clipped sampling, disable/re-enable and shutdown; inspect available GDI/Magnification native error status.
4. Measure before/after submissions/tick, p95 cursor motion latency, CPU/GPU usage where available and stationary update correctness. If fewer API calls do not reduce CPU/GPU, state that honestly and retain the change only if it simplifies correctness or removes harmful work.
5. Obtain independent review of capture-exclusion ordering and all failure paths. Resolve findings before accepting.

**Verification:** focused Rust tests and manual Windows native effect smoke (document any not run). **Acceptance:** no recursion, stale effect content or disabled/stationary regressions; measured source call reduction or documented reason.  
**Suggested commit:** `test(mouse): [M5-C] verify cursor source ordering and recovery`.

### M6 — Conditional coordinate sampler and HUD resource efficiency

**Owner:** `src/coordinate_tool/native.rs`, `src/coordinate_tool/controller.rs`, tests. **Dependencies:** M0, ideally M5 stabilized. **This milestone has explicit skip gates**: only M6-A is mandatory investigation; M6-B/C depend on evidence.

#### M6-A — Measure sampling and HUD cost before changing it

**Implementation**

1. Instrument `WindowsSampler::sample()` substeps separately: cursor position, virtual-desktop metrics, monitor geometry/monitor DPI and foreground-client geometry; record active-mode dependent costs and approximate call frequency.
2. Instrument `LayeredDib::draw_hud()` GDI brush/font creation, redraw count, and overall redraw p50/p95. Record GDI object count over extended activity to identify whether a leak or avoidable churn exists (currently allocation/release occurs on changed HUD redraws).
3. Determine whether existing 16ms timeout is excessive for HUD-only, crosshair-only, halo/zoom and frozen modes. **Do not** change refresh cadence merely to lower CPU when it would degrade pointer tracking or magnifier content refresh.
4. Reproduce monitor/DPI changes, foreground window moving/resizing without HWND changes, delayed `sample_for_copy()`, inactive/no-mode state, screen topology and worker shutdown.
5. Add a decision entry: implement M6-B only if safe topology caching materially reduces work; implement M6-C only if GDI churn is meaningful. If neither is justified, record `SKIPPED (measured)` and proceed to M7. Avoid manufacturing a percent gain.

**Verification:** focused native timing hooks, Windows manual sampling/COPY smoke. **Acceptance:** valid measurements/limitations and explicit implementation decisions.  
**Suggested commit:** `docs(perf): [M6-A] profile coordinate sampling and HUD resources`.

#### M6-B — **Conditional** stable display-metadata reuse with reliable invalidation

**Only execute if M6-A supports this change.**

**Implementation**

1. Cache only facts whose validity interval is defensible: virtual-desktop bounds, per-monitor identity/work area and effective DPI, possibly behind a topology generation. Invalidate on display/DPI notification, monitor identity change, or other known signals; add a conservative fallback for missed notifications.
2. Keep `GetCursorPos` fresh. Keep foreground-client/window geometry fresh (or use a *proven* event-based invalidation strategy that accounts for moving/resizing the **same HWND**); never let `sample_for_copy()` return stale client-relative data.
3. Preserve physical-coordinate conversions across negative monitor origins, differing scale factors and monitor switching. Avoid race between topology events and a cached sample's monitor info.
4. Apply caching only for modes needing metadata where safe; do not change the visible preference schema or copy command format. Verify setup/teardown and no persistent worker after all modes off.
5. Compare API counts and p95 tick time with the M6-A baseline. If no benefit or correctness uncertainty persists, revert this checkpoint and report that the refinement was not retained.

**Verification:** sampler tests for topology/move/resize, actual Windows DPI and copy smoke. **Acceptance:** correct fresh coordinates and meaningful reduced native sampling overhead.  
**Suggested commit:** `perf(mouse): [M6-B] reuse safe monitor metadata across samples`.

#### M6-C — **Conditional** retain compatible HUD GDI resources safely

**Only execute if M6-A supports this change.**

**Implementation**

1. Make an explicit owner/RAII wrapper for the HUD background brush and font (or attach them to existing `LayeredDib`), avoiding `CreateSolidBrush/DeleteObject` and `CreateFontW/DeleteObject` for every compatible redraw.
2. Key fonts/brushes to drawing configuration, especially font size and DPI. On change, create replacements, select/deselect correctly and **never delete a GDI object still selected into a live DC**.
3. Keep `LayeredDib` error handling and `Drop` consistent: restore original selected resources, clean up new handles once, recover after failed creation/select/present, no stale font/brush handle after DIB recreation.
4. Retain `SetBkMode`, text color, alpha-channel fill and existing visual appearance. Verify no flicker/incorrect font on DPI and font-size changes.
5. Measure created/deleted object counts and steady-state p50/p95 HUD redraw time. If churn was inconsequential, consider not retaining more complex resource ownership.

**Verification:** focused GDI ownership tests where possible, Windows GDI handle monitoring during extended HUD activity. **Acceptance:** no use-after-free, leaks, resource-count growth, appearance change or degraded performance.  
**Suggested commit:** `perf(mouse): [M6-C] retain compatible HUD drawing resources`.

### M7 — Integrated verification, review, report and handoff

**Owner:** orchestrator for coordination; reviewer for cross-boundary risk; implementers only for confirmed fixes. **Dependencies:** completed M1–M5, M6 measured and done/skipped explicitly.

#### M7-A — Run a bounded cross-surface regression and native acceptance matrix

**Implementation**

1. Review all diffs against the original code and source notes. Confirm notes revisions, dashboard history resolution, rendered result identities, action indexing and native mouse lifecycle have a single owner each. No stale dead logic or duplicate caches remain.
2. Run targeted unit/integration tests that span changed boundaries. Suggested affected targets include `notes_plugin`, `history`, `watchers`, `watcher_failures`, `note_panel_auto_save`, `note_panel_scroll`, and narrow `src/gui/render.rs`/`native_effects.rs` library filters. **Only run those implicated by actual changes**, and group them to avoid repeated linking.
3. On the Windows machine, manually check the selected-row navigation to far-offscreen results in grid and list, context menus/universal action/radial-add, Quick Notes search+edit/delete at deep positions, note external update and backlink refresh, action-file edits with large indexed roots, moving/stationary halo/zoom, DPI/monitor transitions and coordinate copy while a window moves/resizes.
4. During native tests check errors, GPU/CPU, Win32 resource handles, worker shutdown/all-off state and reduced instrumentation mode. Confirm no new always-on load.
5. Ensure error and cancellation paths are covered (invalid note file, corrupt actions JSON, inaccessible indexed path, scan supersession, magnifier unavailable, partial renderer failure). Compare behavior with baseline fixtures; any difference requires documented user-approved scope or a correction.
6. Independent reviewer validates critical concurrency/UI identity/native ordering seams. Resolve substantive findings; do not transform this gate into unrelated full-repository regression work.

**Verification:** narrow target commands and Windows smoke ledger with PASS/FAIL/NOT RUN. **Acceptance:** no unresolved material regressions in changed surfaces; every native-only check has evidence or explicit caveat.  
**Suggested commit:** `test(perf): [M7-A] complete targeted runtime regression gates`.

#### M7-B — Reproduce post-change benchmarks and conclude

**Implementation**

1. On the same host/profile/fixtures as M0, rerun every reproducible scenario and capture before/after p50/p95/max, counters and resource usage. Include negative/no-change results honestly; note unavailable runs.
2. In `docs/performance/track_a_results.md`, report **measured** performance in a table: operation, workload, baseline, final, change, confidence/caveats, pass/fail. Do not sum independent wins into a claimed overall speedup. Distinguish source submissions from real GPU cost.
3. Include correctness evidence and reviewer outcomes; identify any M6 work intentionally skipped, unresolved risks, lower-priority follow-ups, and high-value low-complexity improvements found/implemented.
4. Verify no Track B code changes were accidentally introduced. Inspect `git diff`, `git status`, tracked benchmark output sizes and ignored files; preserve user changes.
5. Commit finished reports and final targeted fixes as a meaningful checkpoint, push feature branch, and hand off a concise summary: branch + baseline SHA + HEAD, checkpoint commits, measured results, tests run/not run, risk notes and suggested PR review order.

**Verification:** repeat benchmark(s) using identical parameters, reviewer acknowledges results, verify remote push. **Acceptance:** auditable report with source provenance and no guessed percentages.  
**Suggested commit:** `docs(perf): [M7-B] publish runtime before-after analysis`.

---

# D. Milestone-by-milestone acceptance and regression matrix

| Check | M0 | M1 | M2 | M3 | M4 | M5 | M6 | M7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Baseline captured/same test seed | Required | Compare | Compare | Compare | Compare | Compare | Compare | Final rerun |
| Note alias/external reload/backlinks/unsaved drafts | Observe | **Gate** | — | **Gate** | Adjacent | — | — | Final |
| Dashboard history ordering/pins/filter/legacy actions | Observe | — | **Gate** | — | Adjacent action versions | — | — | Final |
| Launcher grid/list selection/scroll/context menus | Observe | — | — | **Gate** | Action result update | — | — | Final |
| Quick Notes variable-height/filtered index correctness | Observe | Revision dependency | — | **Gate** | — | — | — | Final |
| Watcher invalid custom JSON/index configuration races | Observe | — | — | — | **Gate** | — | — | Final |
| Magnifier stationary video/recursion/error recovery | Observe | — | — | — | — | **Gate** | Adjacent if needed | Final |
| Coordinate copy with moved/resized same HWND and DPI | Observe | — | — | — | — | Adjacent | **Gate if changed** | Final |
| Telemetry off, worker zero when effects off, no private data | Required | Check | Check | Check | Check | Check | Check | Final |

## D1. Targeted test scenarios the orchestrator must preserve

**Notes:** equal-note reload no revision; changed alias/title or slug increments version; cache load failure preserves committed state; external edit reflected in backlinks; unsaved editor changes immediately mark dirty and respect debounce; toggling backlinks changes derived state; Quick Notes `short_preview` exactly matches sample baseline for Unicode, headings and whitespace; backlink count equals old API.

**History:** pinned-only never clones history; duplicate pins match by `(action_id,args)`; newest pinned first in mixed mode; unpinned order matches original; filter matches resolved label and query (not stale saved label); missing pin is marked while missing ordinary history keeps saved runnable fallback; prompted snippet and opaque clipboard compatibility.

**Root/UI:** 10k action list/grid renders bounded widgets; all absolute selected indices activate the correct action; selection of an item offscreen reliably scrolls; last partial grid row has no fabricated actions; context-menu hover/click identity is correct; changed font/DPI/columns invalidates height/scroll calculations.

**Watcher:** changed custom Actions with huge index performs no scan; rapid edited actions retain newest custom state; scan A finishing after scan B is rejected; root/max-item changes invalidate generation; failed scan preserves last good; successful scan merges with live current custom; app shutdown stops worker without GUI wait/deadlock.

**Mouse:** single moving worker tick does not refresh old location then present new; stationary active magnifier continues refresh; sample failure never uses last-good or frozen sample for effect source; hidden/disabled effect is no-op; halo fallback/zoom coexist; exclusion lists updated before potentially recursive source capture; effects recover after supported topology/toggle invalidation; Windows GDI handle count stable if HUD caching is changed.

## D2. Checkpoint packet template (required for each implementer handoff)

```text
Checkpoint: M#-X — <short title>
Base commit: <SHA>; target branch: performance-optimization
Objective: <observable completed behavior>
Files/ownership: <source paths and relevant existing calls>
Current source facts: <confirmed facts, not guesses>
Dependencies: <which earlier checkpoints must be present>
Required implementation: <numbered concrete steps>
Invariants/non-goals: <what must not change>
Targeted tests: <exact or validated commands/fixtures>
Performance comparison: <which G0 counters/scenarios apply>
Review: <reviewer required? which boundaries>
Definition of done: <objective pass/fail statements>
Suggested commit: <stage-tagged subject>
Deliverable: changed files, test results, metrics, deviations
```

The planner may refine a checkpoint packet for actual code changes **without** reopening approved scope or re-planning the entire initiative. Give the implementer enough detail to code without rediscovering architecture.

## D3. Checkpoint completion ledger (maintain in repository)

| Checkpoint | Code ready | Targeted tests | Reviewer | Baseline comparison | Commit SHA | Push confirmed | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| M0-A | — | — | optional | setup | — | — | — |
| M0-B | — | — | optional | fixtures | — | — | — |
| M0-C | — | — | optional | G0 | — | — | — |
| M1-A/B/C | — | — | critical | G1 | — | — | — |
| M2-A/B/C | — | — | important | G1 | — | — | — |
| M3-A/B/C/D | — | — | critical | G1 | — | — | — |
| M4-A/B/C/D | — | — | critical | G1 | — | — | — |
| M5-A/B/C | — | — | critical | G1 | — | — | — |
| M6-A/B/C | — | — | as applicable | decision gate | — | — | record SKIPPED |
| M7-A/B | — | — | critical | G3 | — | — | — |

Do not collapse all M1/M2/M3/M4/M5 checkpoints into one commit just because their table row is condensed above. Maintain actual individual checkpoint status and commit IDs in the live ledger.

# E. Implementation decisions and stop conditions

1. **Never claim a speedup without measurement.** Redundant-source removal is a justified reason to try a change, not proof of wall-time improvement.
2. **Never sacrifice data consistency.** If a cache invalidation key cannot cover external edits or a race, fix ownership at the source boundary or abandon that specific cache.
3. **Never fake virtualization.** Skipping drawing but still cloning/preparing every off-screen row is not a meaningful M3 completion.
4. **Never hide scan stalls behind a misleading iterator name.** Draining `IndexBatchIter` on the GUI thread remains synchronous; async workers must actually own traversal.
5. **Never turn event pumping into unnecessary 16ms UI redraws.** Stationary magnification content updates should remain worker-owned; cheap UI should not repaint just to preserve video magnification.
6. **Never optimize coordinates with stale geometry.** If monitor/window invalidation cannot be proved, prefer correct live queries and leave M6-B skipped.
7. **No unrelated workflow migration.** Developer build/test speed belongs to Track B, notwithstanding targeted-test efficiency during Track A.
8. **No destructive Git recovery.** Branch conflicts, changed worktree, missing remote or mismatched source are blockers to resolve explicitly, not permission to reset, force or overwrite.
9. **Stop when the milestone is proved.** Do not run unrelated global tests, create new agent loops or broaden benchmarks unless specific failures demand it.

# F. Final orchestrator handoff requirements

At the end, return a report with:

1. Base SHA and final SHA on `performance-optimization`; remote push confirmations or clearly stated blockers.
2. One concise line per checkpoint, with actual commit hash, tests performed and what changed.
3. A before/after table for **notes, history, root rendering, Quick Notes, watcher/indexing, mouse effects**, and conditional coordinate HUD work; include `NOT MEASURED`/`SKIPPED` explicitly where appropriate.
4. Correctness outcome for every high-risk gate (external notes, filter/history, off-screen selection, stale indexing, native magnification).
5. Proven wins, unexpected regressions resolved, considered-but-rejected changes and remaining risks.
6. A statement that Track B (Cargo/linker/test-target optimization) was not modified.
7. Any manual Windows checks still needed before merge — not disguised as automated passes.

**Go/no-go:** The branch is ready for human PR review only when the M7 results are auditable, accepted major risk gates are passed or transparently qualified, and unrelated data/behavior changes are absent.
