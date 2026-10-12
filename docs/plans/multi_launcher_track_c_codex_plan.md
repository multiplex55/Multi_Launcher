# Multi Launcher — Track C Codex Implementation Plan

**Track:** Runtime Responsiveness & Performance Foundations  
**Approved approach:** Comprehensive, focused performance engineering; measure first, preserve behavior, no application-wide rewrite.  
**Operational source of truth and baseline:** the **exact HEAD of the currently checked-out branch at execution time**.  
**New branch:** `runtime-foundations`, created from that HEAD after provenance/worktree checks.  
**Git policy:** local checkpoint commits only; **never** push, merge, rebase, reset, fetch/pull automatically, clean unknown files, or change another branch/worktree.  
**Implementation checkpoints:** **29** across **C0 (4), C1 (7), C2 (8), C3 (6), C4 (4)**. A conditional investigation may conclude `SKIPPED — NOT JUSTIFIED` with retained evidence; don't force speculative modifications.

Companion documents:
- `multi_launcher_track_c_source_notes.md` — mechanism-specific source audit, existing test owners, final Track A historical observations, limitations.
- `multi_launcher_track_c_codex_start.md` — orchestrator, branch, subagent and execution contract.

## 0. Non-negotiable execution contract

1. **Record exact provenance first.** From repository root run `git status --short`, `git branch --show-current`, `git rev-parse HEAD`, inspect `.git` worktrees if necessary, and verify expected Track A and B code/profile presence. Record commit/branch and changed files in `docs/performance/track_c_baseline.md` before creating the branch. Branch `runtime-foundations` **from the current HEAD**, not from remote `master`. If there are unrelated dirty changes, if the branch name is occupied by incompatible work, or if source differs materially from the approved architecture, **do not destroy changes**; report the blocker and choose a non-destructive resolution. Known user-placed untracked Track C plan files may be preserved across safe branch creation and committed only on the new branch. Do not silently reinterpret old archive paths/SHAs as the live source.
2. **Read repository contracts**: `AGENTS.md`, `.codex/config.toml`, `.codex/agents/{planner,implementer,reviewer}.toml`, relevant owner source/tests, current `docs/performance/track_a_final.json`, Track A M7 report and Track B reports. Old reports are **historical**, not current measurement baselines.
3. **Preserve behavior, not just API shapes.** Search result order, stable ties, action identity, exact `args` including `None` vs empty, plugin/provider precedence, pinned/history/radial results, note alias resolution/snippets/unsaved drafts, UI coordinates/scroll/selection/menu ownership, startup catalog completeness, hotkeys, window manager and native cursor effects, async worker cancellation and last-good semantics, persistent data, version increments, and shutdown ownership must remain unchanged.
4. **No new semantic shortcuts.** Don't truncate results, hide sources, skip tests, switch to fixed-height widgets, alter fuzzy ranking, lower safety settings, replace the thread coordinator, run plugin callbacks under locks, or publish stale data to make performance look better.
5. **Gate every optimization:** inspection → isolated fixture/oracle → frozen baseline → minimal change → targeted tests → exact-profile before/after comparison → independent review → local checkpoint commit. Measurements showing regression or negligible gain trigger revert/skip with rationale. If a risky feature cannot be made correct within the bounded scope, retain existing implementation and document why.
6. **Use source-level work units as strong evidence.** Track bytes/notes copied, notes/todos visited, fenced-code normalizations, candidates scored/materialized, geometry rows/cells measured, cache rebuilds, action projection constructs, event queue age/processed count, startup milestones. Time alone is noisy on Windows. Do not call a single cold observation a cold p95.
7. **Profile identity matters.** For comparable CPU/UI results use the **same Windows host, target directory, source fixture, profile, viewport/fonts/DPI, sample protocol and enabled telemetry**. Serial ignored performance owners, `MULTI_LAUNCHER_PERF=1` at process creation; Track C's `ML_TRACK_C_BENCH_MODE` (or equivalent documented fixture mode). Keep `cargo nextest --cargo-profile fast-dev` series separate from default-profile baselines. Use `cargo build --profile iteration --bin multi_launcher` for routine binary compile. Canonical release settings remain untouched.
8. **No measurement leakage:** no real clipboard data, private notes, absolute paths, HWND/titles, desktop readback or credentials in committed metric files. Raw native logs and generated source trees remain under ignored `target/performance/track-c/` or an owned temporary directory; publish only sanitized aggregates + stable synthetic identities.
9. **Testing cadence:** prefer targeted `cargo check --lib`, selected `cargo nextest run --lib -E 'test(<owner>)'`, and smallest relevant integration executable. On slow remote builds do implementation/review/fixture work first, then one compile/test gate. Check long-running commands at sensible **10–20 minute** intervals instead of repeated busy polling; never run overlapping Cargo jobs against the same target directory. Full suite only if a concrete cross-cutting correctness risk justifies it and the run budget is explicitly recorded.
10. **Agent ownership:** the parent Codex orchestrator owns checkpoint gating/ledger/commits. Delegate nonoverlapping read-only investigation to planner/reviewer as appropriate; **one implementation writer at a time for overlapping source**. Reviewer checks source and measured evidence after each risky stage. Parent compares actual diffs and commits only verified intended files. Do not parallelize expensive benchmarks on the same host.
11. **Status vocabulary:** `PASS`, `SKIPPED (measured gate)`, `FAIL`, `IN_PROGRESS`, `NOT RUN`, `NOT MEASURED`. Don't claim Windows-native visual latency from fake APIs, or responsiveness from isolated headless CPU timing. A skipped optional optimization satisfies a checkpoint only when the measured hypothesis, risk rationale and reviewer decision are recorded.
12. **Scope boundaries:** Track A/B runtime and compilation work stay intact. Explicitly exclude wholesale crate/workspace decomposition, broad global-state refactor, rust-analyzer tuning, dependency updates, cursor/GDI reoptimization, UI redesign, persistence-schema changes, and any Git network operation. The separate optional SSD experiment is advisory and nonblocking.

## 1. Working outputs and baseline protocol

Create these new, additive files under `docs/performance/`:

- `track_c_checkpoints.md` — the 29-row live ledger: owner, state, tests run and totals, provenance SHA, compared profile, reviewer disposition, decision/skip, local commit, recorded limitations.
- `track_c_baseline.md`, `track_c_baseline.json` — frozen source-matched G0; match fixture signatures and host/environment.
- `track_c_workloads.md`, `track_c_metrics.md` — exact commands/inputs/units and opt-in measurement definitions.
- `track_c_results.md`, per-stage bounded JSON summaries (e.g. `track_c_notes_g1.json`, `track_c_search_g1.json`) — G1 data and unfavorable cases.
- `track_c_native_recipe.md`, `track_c_handoff_notes.md`, `track_c_final.json` — native protocol, checked decisions and final provenance.

Reference the existing owner harness `src/performance/workloads.rs` and frozen `docs/performance/track_a_final.json`; **never overwrite historical files or silently change existing Track A scenario definitions**. Keep baseline scenario records immutable after freeze, even when a later methodology needs an extra, versioned supplemental scenario.

### Measurement design

**Windows host observations:** OS, CPU logical/physical count, RAM, drive type, power mode, rustc/cargo/nextest versions, commit SHA, profile, target type, egui effective style/font/pixels-per-point, ignored-test thread count and app settings. Record inability to query a value instead of estimating.

**Matched workload classes:**

| Class | Small / medium / large | What to measure and protect |
|---|---|---|
| Notes legitimate heavy refresh | 100 / 1k / 5k | full note snapshot+todo loading+three categories, alias collisions and exact ordered row payload |
| Notes quick-collision lookup | 100 / 1k / 5k | unnecessary note clones, ambiguity and correct display strings |
| Action reload and indexed publication | 100 / 1k / 10k custom/indexed; 20k combined | typed load vs assemble vs cache/index build vs synchronous query; equality/last-good; separate off-thread index time |
| Search | 100 / 1k / 10k actions; sparse/dense queries | `app` exact/fuzzy, aliases/usage/ties/providers; candidate visitation/clones/sort time; full output identity |
| Launcher cold geometry | 100 / 1k / 10k list/grid | cold *distribution* when feasible, measured galley count, warm frames, selected first/middle/last; width/font/mode changes |
| Quick Notes cold geometry | 100 / 1k / 5k | standard and narrow viewport; projection/metadata preparation vs geometry vs painting; anchor, menus and draft behavior |
| Startup | 16 / 1k / 10k indexed | process→complete catalog→plugin registered→hotkey ready→first usable frame, plus first query completeness |
| Event-loop bursts | quiet/short burst/sustained | FIFO handling and urgent events, queue depth/age, frame time, repaint/wake gaps, one-event heavy handlers |
| Native smoke | real Windows if available | real hotkey→usable result, selected action, startup/index readiness, closing/shutdown; no raw private output |

**Benchmark protocol:** generate deterministic fixtures outside the measured interval, use controlled startup state (owned temporary data dirs, inert plugin manager where supported, `ML_SKIP_CLIPBOARD_SYNC=1` if relevant), five warmups/twenty measured samples for pure/UI operations (unless a different bounded protocol is justified), one or more cold samples with actual invalidation in each repetition and explicit cold sample count, output generation/hash after each timed iteration, nearest-rank p50/p95/max. For slow native/cold campaigns use fewer bounded repetitions with a `NOT MEASURED` p95 label rather than inventing a distribution. Compare equivalent complete data/projection fingerprints separately from viewport-only receipt hashes.

**Performance decision rule:** A change must preserve semantic parity and demonstrate an attributable reduction in work or repeatable latency improvement beyond observable noise at a relevant scale. No global percentage quota. Latency wins may justify a small additional memory cost if quantified. No improvement and high risk → revert or skip. Benchmark owners should not leak gigabytes of fixtures across repeats or mutate process globals between parallel tests.

## 2. Checkpoint ledger — 29 contracts

| ID | Objective | Gate nature |
|---|---|---|
| C0-A | Provenance, owners and invariants | Required |
| C0-B | Low-overhead observation/latency instrumentation | Required |
| C0-C | Representative fixtures, semantic oracles and benchmarks | Required |
| C0-D | Freeze Track C baseline and native plan | Required |
| C1-A | Characterize full note relationship semantics | Required |
| C1-B | Fuse redundant note/todo relationship traversals | Measured implementation |
| C1-C | Conditional reverse relationship indexing | Optional measured gate |
| C1-D | Lightweight alias collision/secondary note projections | Optional measured gate |
| C1-E | Action catalog preparation and cost characterization | Required |
| C1-F | Conditional atomic/reusable action projections | Optional measured gate |
| C1-G | Notes/action semantic and timing acceptance | Required |
| C2-A | Search owner baseline and exact result oracle | Required |
| C2-B | Two-phase score/retain/materialize search | Measured implementation |
| C2-C | Conditional candidate-screening search improvement | Optional measured gate |
| C2-D | Reuse cold list geometry safely | Optional measured gate |
| C2-E | Reuse cold grid geometry safely | Optional measured gate |
| C2-F | Reuse cold Quick Notes layout/metadata safely | Optional measured gate |
| C2-G | Integrate query-to-visible timing and invalidation | Required |
| C2-H | Search/geometry independent parity and comparison | Required |
| C3-A | Characterize startup readiness critical path | Required |
| C3-B | Conditional safe startup improvements | Optional measured gate |
| C3-C | Instrument queue wait, frame-burst load and wakeups | Required |
| C3-D | Conditional coalescing of idempotent notifications | Optional measured gate |
| C3-E | Conditional bounded event processing/fairness | Optional measured gate |
| C3-F | Event/startup race, shutdown and Windows acceptance | Required |
| C4-A | Focused cross-surface integration gate | Required |
| C4-B | Native end-to-end responsiveness validation | Required evidence / limits |
| C4-C | Complete matched G0/G1 comparison | Required |
| C4-D | Final review, local commits and handoff | Required |

---

# C0 — Provenance, measurement and frozen baseline (4 checkpoints)

### C0-A — Establish exact source owners, safe branch and behavior inventory

**Goal:** prove the starting checkout and the boundaries of every optimized subsystem before editing behavior.

**Tasks**
1. Run read-only Git and environment inventory (`git rev-parse HEAD`, branch, status, `git log -1`, `rustc -Vv`, `cargo -V`, `cargo nextest --version`; gather Windows host/power without writing private identifiers). Confirm `Cargo.toml` contains Track B profiles; `src/plugins/note.rs` versioned snapshot; `RootListGeometryCache`, Quick Notes geometry cache, `IndexCoordinator`, native effects refresh phase, and 15 metric families still exist. Do not assume source notes' line numbers remain valid.
2. Inspect live `AGENTS.md` and agent TOMLs. Map exact callers and existing tests for `refresh_heavy_derived`, `backlink_rows_for_note`, `NoteCache::from_notes`, `publish_actions`, `update_action_cache`, `search_actions`, root list/grid `ensure`, Quick Notes geometry, startup catalog, event queue.
3. Document startup invariant: `startup_indexed_actions` completes before `OmniSearchPlugin`/`VirtualDesktopPlugin` catalogs; document event variants whose delivery must never be delayed/coalesced (action execution, recovery/emergency, radial dispatch/control, worker completion). Document GUI geometry root-IDs and note identity invariants.
4. Record baseline SHA and exact dirty state **before** creating `runtime-foundations`. Create and check out the new branch only if safe. No branch switch to master, fetch or reset. Create the 29-row ledger with all `PENDING`, timestamped provenance and explicit local-only policy.

**Verification and acceptance:** `git status`, branch/HEAD and scope inspection recorded; no source changed other than new plan/ledger files; parent and planner sign off on owner inventory. Stop safely on provenance/dirty-state conflict rather than overwriting it. **Suggested local commit:** `docs(perf): [C0-A] record runtime-foundations provenance and owners`.

### C0-B — Bounded opt-in responsiveness telemetry

**Owners:** `src/performance.rs`, existing performance helper modules, narrow boundary call sites in `src/gui/render.rs`, `src/gui/search.rs`, `src/gui/watch.rs`, `src/main.rs`, and notes/action owners only as needed.

**Tasks**
1. Define a minimal set of Track C phases: `note.relationship_refresh`, `note.mentions_scan`, `action.publish_prepare`, `action.publish_commit`, `search.score_candidates`, `search.materialize`, `root.geometry_cold`, `notes.geometry_cold`, `event.enqueue_age`, `event.drain`, `startup.catalog_ready`, `hotkey.to_first_usable_frame` (names may be adapted; provide a stable table). Instrument **actual owner boundaries**, not a detached miniature algorithm.
2. Reuse Track A fixed atomic/perf infrastructure where possible. For high-cardinality, dynamic or cross-thread traces, prefer a bounded `cfg(test)`/opt-in recorder with static phase IDs, never strings/paths. Keep normal non-opt-in path essentially a cached boolean + no timers/work.
3. Measure event enqueue/receive count/age using a safe envelope or sidecar only after checking `WatchEvent` clonability, existing `EventSinkRegistration` queue counters and all sender paths. Preserve send failure, wake ownership and no-events-before-owner behavior. Do **not** add an unbounded per-event trace.
4. If measuring hotkey→first usable frame, associate an invocation generation/timestamp without altering gesture recognition. Record start when launcher invocation is accepted; mark end on the actual first root frame with current query/result generation, not on a repaint request; never count a hidden or still-loading grid as usable.
5. Unit-test overflow, reset, disabled overhead, duplicate/late completion, failure statuses and frame-lifecycle association. Record per-class work counters (notes visited, rows measured, action copies, candidates scored, queue depth) separately from time.

**Tests:** `cargo nextest run --lib -E 'test(performance::tests)'` plus newly named `track_c_metric_*`, and a focused event/launcher smoke test; format/diff checks. **Acceptance:** existing metric names/meanings unchanged, no private payload storage, no background reporter, no unexpected native actions. Reviewer approves cost and lifecycle. **Commit:** `perf(metrics): [C0-B] add bounded responsiveness owner probes`.

### C0-C — Deterministic baseline fixtures and independent reference oracles

**Owners:** `src/performance/workloads.rs` extension or `src/performance/track_c_workloads.rs` (test-only), and minimal `#[cfg(test)]` accessors local to note/search/render/watch/startup owners.

**Tasks**
1. Reuse Track A fixture builders (`note_fixture`, `action_fixture`, index trees and stable signatures) but give Track C scenario names/seed IDs distinct identity domains. The actual production owners must be invoked: one note-panel heavy refresh; actual `search_read_only_outcome` with plugin fixture; `render_root_frame`/`NotesDialog::ui`; `process_watch_events` action publish; real index completion via injected scanner; actual startup stage seam; real event drain.
2. Create **untimed, test-only eager reference paths** before performance modifications. Notes: exact ordered `BacklinkRow` fields for three categories and todo label map; search: full ordered `Action` including optional args and provider/pending state (not hash-only); geometry: actual egui 0.27 eager Widget `Response` rects/content bounds/scroll and popup owner; actions: custom-first indexed tail, maps/duplicate-ID winners and version stamps.
3. Use separate fixture builders for `100/1k/5k notes`, `100/1k/10k results`, `custom+indexed20k`, `16/1k/10k index roots`, and an explicitly bounded event burst. Include alias collisions, Unicode, very long/wrapped labels, grid1/2/3/5/6 cols, wide-before-narrow Quick Notes, sparse `app` query, tied fuzzy score and disabled plugin, unsaved note, failed save, stale scan completion.
4. Test cold path by invalidating **genuine production geometry/projection state** and timing subsequent owner work; test warm path without invalidation and assert zero full measurement. Layout-related input (font atlas/scale/width/style) must be settled before measured repeats. For headless text rendering, account for egui frame animation settling; no artificially fixed height.
5. Audit fixture isolation: use unique owned tmp directory and stable synthetic data, avoid HOME/USERPROFILE/CODEX_HOME mutation, avoid native worker startup where not needed, restore all thread-local/global state and avoid overlapping serial performance tests.

**Tests:** new `track_c_fixture_*`, eager oracle smoke, smallest ignored benchmark owners with explicit mode/profile. **Acceptance:** complete output identities stable across repeat runs; no caching/algorithm changes yet; timer starts after fixture creation, ends before output hashing and logging; cold/warm work clearly separated. **Commit:** `test(perf): [C0-C] add Track C workload owners and eager oracles`.

### C0-D — Freeze G0 current-source baseline and manual Windows recipe

**Tasks**
1. On **the precise C0-C commit**, run full serial opt-in tests using the documented Windows toolchain, power mode, profile, viewport and benchmark protocol. For cold repeated trials generate independent invalidations outside timing and record cold rebuild observations as distributions *only* when sample count supports them; if a single observation, label it accurately.
2. Capture output signatures, p50/p95/max, note/alias work, candidate counts/action clones, action cache work, cold geometry costs, event queue age/lag, startup complete-catalog readiness where available. Report `NOT MEASURED` for native/display cases until performed. Do not borrow Track A final observations to fill missing Track C columns.
3. Repeat selected small cases and verify fixture and full result/projection signatures. Preserve genuine timing noise; don't select the best rerun. Freeze `docs/performance/track_c_baseline.{md,json}`. Never edit frozen values; new scenarios later get a separate versioned baseline appendix.
4. Prepare `track_c_native_recipe.md`: owned Windows data dir/process, synthetic catalog and test hotkey, focus handoff, real first usable frame marker, note panel, Quick Notes, radial/menu, native window close, no private capture. Distinguish API, visible composition, input-to-display and GPU effects. Ensure no user-data modifications.
5. Parent/reviewer verify 29-entry ledger, profiles, test isolation, direct owner invocation, baseline source commit and unmodified Track A/B files. Only then release C1 implementation.

**Acceptance:** comparable G0 locked, explicit valid/invalid comparisons, tests pass; native unavailable outcomes explicitly recorded. **Commit:** `perf(baseline): [C0-D] freeze Track C owner measurements`.

---

# C1 — Notes, relationships, and action publication (7 checkpoints)

### C1-A — Freeze exact note relationship contract and count its work

**Owners:** `src/gui/note_panel.rs::refresh_heavy_derived`, `backlink_rows_for_note`, `src/plugins/note.rs`, todo owners and the C0 oracle.

**Tasks**
1. Inventory every route to `refresh_heavy_derived`: persisted notes/todos/setting revisions, force refresh, debounce/edit, save/reopen, link-menu. Record input ownership and when missing/poisoned cache must retain old rows.
2. Write an independent **unoptimized** eager note-row oracle (or freeze original helper under `cfg(test)`) that compares full row title/badge/updated/snippet/reason/note slug/todo ID, three tab orders, TODO label map, and captured persisted revision. Do not rewrite oracle while developing the optimized algorithm.
3. Count notes/todos visited, fenced-code transformations, per-tab passes, temporary content allocation (estimate vs actual bytes distinguished), and frequency of full snapshots in G0. Measure a real open draft after debounce at 100/1k/5k, external note reload and persisted todo change. Record relative cost of full snapshot versus row computation.
4. Choose smallest viable fix; prefer fusing three independent passes before adding indexed state. Reviewer documents exactly which matching rules are not represented by `NoteCache.links`.

**Tests:** focused `note_panel` related/mention/link tests, note publication/race tests and G0 owner; **no production optimization in this checkpoint**. **Acceptance:** eager reference trusted and work attribution documented. **Commit:** `test(notes): [C1-A] characterize relationship-refresh contract`.

### C1-B — One-pass exact related-note and linked-todo refresh

**Tasks**
1. Replace three calls to `backlink_rows_for_note(..., tab, ...)` with a private single traversal that constructs **three separate vectors** in the original per-category source order. Compute `note_reference_needles` and the current draft's `content_without_fenced_code` once per refresh, not once per tab. For each note/todo, perform the minimum equivalent parsing/matching and route eligible rows preserving tab classification, precedence, snippets and reason text.
2. Preserve source traversal order in each category, including todo-versus-note order and tie rules. Do not deduplicate a row unless the eager helper did. Keep `LinkedTodos` and `Mentions` behavior for TODO rows consistent with the original, including any intentional overlap.
3. Apply derived rows/todo label map only once after complete successful recomputation; maintain `last_notes_version`, `last_todo_revision`, settings, draft debounce, `heavy_recompute_requested`, and retry flags exactly. A failed note snapshot or todo read must retain last-good behavior.
4. Add fixture cases: 0/1/100/1k/5k notes, ambiguous aliases, primary/secondary alias transitions, broken wiki links, mentions embedded in/omitted from fenced blocks, nested markdown, duplicate refs, Unicode snippets and persisted todo links; compare full row structs and revision stamps, not only counts.
5. Measure visited-note counts, transforms and allocations as well as heavy-refresh p50/p95. Revert if fusion makes common cases slower without demonstrable heavy-work reduction or alters output; do not sacrifice correctness for a microbenchmark.

**Tests:** targeted `cargo nextest run --lib -E 'test(gui::note_panel::tests::) | test(note_cache_)'` **only if build/test budget permits**, preferably a narrower named set first; `--test notes_plugin` and `--test note_panel_auto_save` on accepted source. Independent reviewer checks exact oracle, unsaved draft/alias/todo order, failed snapshot and semantics. **Acceptance:** complete exact parity, reduced redundant traversals/normalization, repeatable improvement or documented no-change revert. **Commit:** `perf(notes): [C1-B] fuse relationship refresh with exact parity` (or measured skip record).

### C1-C — Conditional versioned reverse relationship projection

**Trigger:** only if C1-B residual full-scan cost is still material for realistic note counts **and** a bounded index can represent the exact semantics without excessive invalidation/memory cost.

**Tasks**
1. Analyze whether canonical note revisions plus unsaved draft overlay can map all wiki/entity/alias/prose mention rules; contrast existing `NoteCache.links` with `backlink_rows_for_note` by counterexample. Enumerate alias collision/rename invalidations and snippets requiring source text.
2. If justified, prototype an immutable `Arc` versioned relationship projection built once per successful note cache publication in `NoteCache::from_notes` or a clearly owned stage. Preserve coherent `(revision, projection)` pairing under cache lock; no snapshot of one generation paired with another. Avoid constructing large per-frame indexes; do not store redundant full content unnecessarily.
3. Keep todo-related indexing **separately revision-keyed** to the todo owner. For unsaved edits use current draft content on top of persisted projection without publishing draft data into global cache. Fallback eager recompute for unsupported/ambiguous cases with exact receipts.
4. Race tests: note publication interleaved with read, reload/save, alias rename/collision resolution, TODO mutation, disabled→enabled backlinks, failed cache construction. No lock held through I/O or plugin callbacks; never publish a partial index.
5. Compare warm edit, first publication, memory footprint, p95 and unchanged-lookup cases. Reviewer may mark **SKIPPED (semantics/cost gate)** and retain fused-pass improvement if full index isn't convincingly better.

**Tests:** note cache publication and snapshot race cases, linked TODO/related notes/mentions full oracle. **Acceptance:** exact fidelity and measured net value including publication costs. **Commit:** `perf(notes): [C1-C] add versioned relationship projection` or docs-only gate commit.

### C1-D — Eliminate secondary full-note copies in alias/collision display

**Source:** `NotePanel::alias_collision_warning` and `NoteCache` title/alias/slug lookups.

**Tasks**
1. Baseline collision-warning cost for unique/duplicate/ambiguous alias, 100/1k/5k notes, and confirm actual invocation frequency while authoring aliases. Measure full note snapshots and estimated copied bytes separately from label formatting.
2. If material, create a small under-one-lock query (`note_conflicting_display_labels` or equivalent) returning only necessary display strings for given alias+excluded slug, using existing canonical maps and note order. Avoid copying all note content. Keep original alias ambiguity warning exact, including label rendering, duplicates, case insensitivity and unsaved note slug.
3. Preserve poisoned-lock/error policy and notification behavior. Do not silently replace an existing warning with an empty/no-warning success on lock failure. Test snapshots during note rename and version publication.
4. Consider similarly narrow secondary note-title lookups **only after** inspecting call frequency and prior indexes; no broad rewrite of note APIs.

**Tests:** new alias collision parity and version tests plus relevant `note_panel`/`note_cache` tests. **Acceptance:** output parity and demonstrated clone/work reduction, or skip if infrequent/immaterial. **Commit:** `perf(notes): [C1-D] use lightweight alias collision projection` or gate decision.

### C1-E — Freeze exact action publish projections and identify redundant work

**Owners:** `src/gui/mod.rs::publish_custom_actions_with_indexed_tail`, `::publish_actions`, `src/gui/search.rs::update_action_cache`, WatchEvent Actions/IndexReady and `src/completion.rs`.

**Tasks**
1. Measure independently (a) typed Actions file load, (b) copying indexed tail, (c) assembling custom+indexed `Arc<Vec<Action>>`, (d) generating `CachedSearchEntry` / filter metadata, (e) populating `actions_by_id`, (f) invalidating completion/search, (g) actual same-query refresh. For index worker completion exclude scanner duration from publication; keep the request/wait boundary separately.
2. Snapshot full semantics in a test-only oracle for custom-first indexed order, same-action IDs and optional args, ID lookup duplicate winner, current query results, history/pin resolution, radial dynamic catalog validity, completion index, equality/no-op identity/version and stale worker completion.
3. Profile changed payload vs semantically equal file vs malformed/missing file vs new indexed completion at 100/1k/10k/20k. Include a queued newer custom change while a scan is blocked. Preserve old `Arc` on no-op and on failure.
4. Make an explicit keep/change decision about immutable shared projections versus whole-Vec compatibility; **never** assume changing `Arc<Vec<Action>>` to a segmented structure is easy or required.

**Tests:** focused `gui_index_*`, `actions_watcher_retains_invalid_then_publishes_valid_without_local_double_bump`, search invalidation cases and real C0 publication benchmark; no production change yet. **Acceptance:** heavy work attributed and exact comparison oracle approved. **Commit:** `test(actions): [C1-E] characterize catalog publish boundaries`.

### C1-F — Conditional action catalog publication improvement

**Trigger:** G0 shows material synchronous publication cost with a safe bounded plan that preserves all consumer semantics.

**Tasks**
1. Prefer the narrowest change: reuse unchanged immutable tail/metadata, avoid duplicate `Action` body construction for `actions_by_id` where possible, and/or build pure projections off-thread using a bounded owner-coordinated job. Keep `self.actions` public semantics stable unless a comprehensive consumer audit supports a stronger change.
2. Stage an immutable candidate containing ordered catalog and search/filter/ID projections tagged with **both** current custom-actions publication identity and index generation/config; build without holding GUI state locks. On commit, revalidate current custom payload, indexed tail, relevant folder aliases, enabled plugins and search generation; newer state wins, stale candidates discarded.
3. Atomically install all accepted derived structures, then perform **one** correct search/completion/radial version transition; preserve plugin synchronous command-resolution precedence. Reentrant and unchanged publications must not duplicate version bumps or retarget popup/selected results.
4. Bound pending/result slots and cancellation; no new unbounded worker or I/O on UI owner. Failure retains complete last-good catalog and all related derived lookups. Background prep must not race with resource drops or post-shutdown wakeups.
5. Focused races: external Actions edit during blocked index, root/cap change, malformed/empty custom, same-id replacement, persisted save followed by watcher event, enabled/disabled plugin, stale background projection arriving after newer completion. Compare complete ordered outputs and `Arc`/version expected transitions to eager oracle.
6. Measure wall time of changed Actions watcher event and separate accepted-index publication. If work simply moves off-thread but creates longer user-visible stale results, record the latency and reject/adjust. Reviewer may select `SKIPPED` if present implementation is safer or improvements are marginal.

**Tests:** existing `gui_index_*`, relevant watcher integration targets, history/pins/radial tests plus controlled race channels. **Acceptance:** measured GUI latency win, exact semantic parity and bounded concurrency with shutdown ownership. **Commit:** `perf(actions): [C1-F] reduce atomic catalog publish cost` or documented gated skip.

### C1-G — End-to-end notes/actions stage comparison and regression gate

**Tasks**
1. Run the corresponding final, serial, exact-profile C0 note and Actions owner workloads on the **accepted C1 source commit**, keeping seed/fixture/order signatures and sample counts unchanged. Record remaining full note snapshots, candidate processing, bounded worker waiting and release-ready publication costs.
2. Compare full note row content, note revision/draft/todo contracts, action catalog/cache/duplicate IDs, menu selection, search refresh, radial dynamic action behavior, last-good and stale completion across changed/error/recovery cases. Preserve original failed-results data even if later tuning is better.
3. Verify no new global workers, no file format migration, no Track A notes idle regression, no changed note-cache publication locks, and that a discarded async action candidate is invisible to all consumers.
4. Reviewer audits C1 source and metrics. If C1-C/D/F were skipped, record measured gating evidence; don't count a skipped implementation as a speedup.

**Tests:** narrow library owners and integration `notes_plugin`, `note_panel_auto_save`, `history`, relevant watchers/domain when touched. **Acceptance:** G0/G1 report `track_c_notes_g1.json`, `track_c_actions_g1.json` (or explicit `NOT MEASURED`), changed source snapshot, reviewer resolution. **Commit:** `test(perf): [C1-G] verify note and action responsiveness`.

---
# C2 — Search and cold UI geometry (8 checkpoints)

### C2-A — Establish complete search/ranking and invalidation oracle

**Owners:** `src/gui/search.rs::search_actions`, `::search_read_only_outcome_from_scored_plugins`, `::apply_usage_weight`, `src/gui/mod.rs` query and result-generation owners, `src/gui/render.rs` selection.

**Tasks**
1. Enumerate every caller of `search_read_only_outcome`, `search_read_only_outcome_without_providers`, `search_read_only_outcome_with_plugin_snapshot`, ordinary `search()`, radial pinned queries, and action execution that recursively changes the query. Identify which paths are read-only and which mutate current results or geometry generation.
2. Create an *immutable eager oracle* that captures complete ordered `Action { label, desc, action, args }` values, not only IDs. Test no query, `app` prefix, exact/fuzzy mode, usage weighting, duplicate IDs, ties, special result ordering, plugin/provider deferral and failure fallback. Do not use a different sort to define the expected behavior.
3. Capture baseline at 100/1k/10k actions with sparse and dense matches and at least one combined plugin result set. Count candidates scanned/scored, matched Actions cloned, strings allocated (estimated), sort input length, provider call count, output length, result generation invalidations, first-frame geometry work.
4. Distinguish typed input→search results from search results→cold layout and first usable frame. A query with a partially pending provider must not falsely count as complete.

**Tests:** `search_replacement_clears_selected_index`, `exact_display_match_uses_pre_normalized_query_substring`, `exact_mode_keeps_plugin_resolved_results_but_filters_query_suggestions`, `manual_query_fallback_does_not_reenter_a_blocked_provider`, `clipboard_modify_root_query_returns_direct_open_modify_first_in_fuzzy_mode` and exact companion, plus new `track_c_search_oracle_*`. **Acceptance:** exact test-only G0 reference, full output order/tie-state assertions and separate search/geometry measurements. **Commit:** `test(search): [C2-A] freeze ordered search oracle`.

### C2-B — Two-phase scoring and delayed Action materialization

**Tasks**
1. Refactor only the action-catalog path to score/filter using **borrowed actions and stable absolute catalog indices** instead of cloning every match before sorting. Model candidate origin as `CatalogIndex(usize)` versus `PluginOwned(usize)` (or equivalent) so plugin-provided Actions retain their original lifetime/order; avoid full long-lived self-borrows or sharing temporary references across asynchronous boundaries.
2. Preserve exact/fuzzy checks, `split_action_filters`, alias logic, `CachedSearchEntry` semantics, full match score, `fuzzy_weight`, usage weight (by exact action ID), and the comparator's original `partial_cmp(...).unwrap_or(Equal)` behavior. Keep the **original concatenation order** between app and plugin candidates and stable sort semantics for ties; no `sort_unstable`, `select_nth_unstable`, or top-K cut unless separately proven equivalent for *all* consumers (default: not approved).
3. After final stable ordering, materialize only the ordered result Actions once, retaining exact `Action.args`, label/desc, and original blank-query `app ` label transformation. Preserve provider_revision/catalog_versions/pending state and deferred results even if the Action list is empty.
4. Profile allocation/copies for sparse/dense queries, include empty browse and plugin-heavy queries. Recognize that an all-match output still must materialize all rows under the unchanged API; report that limitation instead of claiming constant-time search.
5. Add unit/property fixtures for tied fuzzy scores, duplicate aliases, usage ties, equal case-folded labels, `None` versus empty args, no result, stale provider completion, disabled providers, and history/radial command resolution. Compare complete result vec and state directly to eager oracle.

**Tests:** `cargo nextest run --lib -E 'test(gui::search::tests::) | test(exact_display_match_) | test(clipboard_modify_root_query_)'` as scoped subset, plus new oracle; specific `plugin_routing` and radial search integration only if touched. **Acceptance:** complete parity plus fewer eager clones/allocations in a representative dense/sparse case and no materially worse output latency. Independent reviewer checks comparator/tie and plugin-state lifetimes. **Commit:** `perf(search): [C2-B] defer action materialization until ordered`.

### C2-C — Conditional candidate screening and no-op result replacement

**Trigger:** after C2-B measured result, only if remaining dominant work is demonstrably unnecessary scanning/invalidation and the proposed optimization can preserve full output.

**Tasks**
1. First inspect `search()`'s `last_results_valid`/`last_search_query` and every `invalidate_root_list_results` call. Test whether semantically identical complete results are routinely reinstalled and cause expensive cold geometry work. If so, compare full ordered candidate identities, labels/args and display-relevant metadata to avoid a genuine **no-op** generation bump; never infer equality from result length or unchecked hash alone.
2. Profile exact-mode cached lowercase fields versus fuzzy-mode repeated label/desc scoring. If safe, reuse generation-keyed precomputed normalized strings/feature filters. For fuzzy candidate screening, preserve matcher score equivalence; substring/prefix heuristics that eliminate valid fuzzy matches are **not** allowed.
3. Any independent searchable index must refresh with the same accepted action catalog and aliases; no unbounded stale search cache across plugin enablement, custom Actions reload or indexed completion. Do not change provider result completeness or radials using read-only search snapshots.
4. Capture no-op versus replaced result-generation work, cold geometry rebuild counts and complete output comparison across empty/identical/reordered/renamed/same-count replacement. Keep app-specific command dispatch and `query:*` special filters correct.

**Tests:** search oracle + `root_list_geometry_rebuilds_only_when_layout_or_display_inputs_change`, current user input tests, `gui_index_equal_completion_preserves_catalog_and_search_state`. **Acceptance:** proven unnecessary work reduction without stale data, otherwise `SKIPPED` with measured finding. **Commit:** `perf(search): [C2-C] avoid redundant search invalidation` or decision-only commit.

### C2-D — Conditional incremental cold launcher **list** geometry

**Owners:** `src/gui/render.rs::RootListGeometryCache::ensure`, result generation and display invalidation owners.

**Tasks**
1. Establish baseline cold geometry **distribution** for first open, query replacement with 5–20% shared ordered prefix, identical results, last-item text edit, first-item width expansion, toggle full paths, wrap change, font atlas replacement, pixels-per-point changes, viewport width changes and 10k rows. Compare to existing eager `SelectableLabel` response oracle.
2. Trace why `row_x_offsets`/`row_response_x_offsets`/`content_width` accumulate effects from earlier wide/no-wrap rows. Map the minimum changed suffix under each key and whether reused prefix geometry can be **proven exact**. Record candidate cost of hashing/storing display strings versus saved galley work.
3. If valuable, cache bounded identity/display measurements keyed by explicit action result generation **plus** precise layout environment (font texture atlas identity retained, font style, pixels/point, available width, actual wrap, padding, spacing, full-path display and alias revision). Reuse previously verified prefix measures only when presentation-equivalent; recalculate every affected suffix including x origin/width and total scroll extent.
4. Do not reuse layout from different font texture atlases/widths, or assume a wide row only changes itself. Preserve selected-row synthetic scroll-to-center, absolute stable widget IDs, max clip extents, negative x offset and popup owner (disjoint offscreen row retained and closed when invalid).
5. Keep the existing warm visible-row iterator and bounded repaint counts. Do not create widgets for all rows to measure geometry and do not introduce fixed `ScrollArea::show_rows` heights.
6. Verify first/middle/last selected row and real click/navigation after partial reuse, including zero/single and newline/wrap/unbounded label stress. If existing accurate geometry requires O(N) and no safe cache scheme meets the measurable gate, **skip** and keep current geometry untouched.

**Tests:** `root_list_geometry_matches_eager_selectable_label_layout`, `root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection`, `root_list_geometry_rebuilds_only_when_layout_or_display_inputs_change`, `root_list_context_menu_owner_stays_with_its_absolute_row_offscreen`, related keyboard/deferred activation. **Acceptance:** exact response/content rectangles and selected/menu identity plus lower p95 cold rebuild in realistically shared results, no warm regression. Critical independent geometry review. **Commit:** `perf(ui): [C2-D] reuse valid cold list geometry` or documented measured skip.

### C2-E — Conditional incremental cold launcher **grid** geometry

**Dependency:** C2-D must be accepted or explicitly skipped first; do not edit the same geometry owner concurrently.

**Tasks**
1. Profile `RootListGeometryCache::ensure_grid`: full `grid_cell_widths/heights`, global column maxima/minima, nominal width, row extents, `grid_visual_max_end_tree`, horizontal content extent, stripe layout and `44` outer allocation/`8,6` spacing for each columns count (1/2/3/5/6), including incomplete final row.
2. Prove how changed cell width can change a **global column width and positions of other columns**, while tall response content may spill past logical row boundaries. Never assume changes are local to a row, and don't drop far-away tall rows from interval-tree queries.
3. If the measured value justifies it, introduce measured-cell reuse for unchanged `Action` display/text and style key; rebuild global maxima/row geometry/interval tree as needed. A selection/generation change must not retain a popup at the wrong absolute index. Keep stripe alternating by absolute row, clipped click hit areas, synthetic offscreen selection scroll, and width/height exactness.
4. Compare cold full geometry, cold partial delta, warm grid frame and memory footprint at 100/1k/10k. If recomputing the aggregate dominates despite cached text galleys, revert rather than complexity creep.
5. Avoid altering the existing rendering backend, label visual design, wrap semantics, total result count, `QueryResultsLayout`, grid columns or menu contents.

**Tests:** `root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, `root_grid_viewport_builds_complete_bounded_rows_with_absolute_ids_and_click_targets`, `root_grid_geometry_reuses_warm_cache_and_tracks_grid_layout_inputs`, `root_grid_popup_owner_retains_complete_row_without_retargeting`, list regression suite. **Acceptance:** exact eager oracle under all layouts and a documented material cold improvement or a gated skip. Independent source/cold evidence review. **Commit:** `perf(ui): [C2-E] reuse grid measurements with settled geometry parity` or gate decision.

### C2-F — Conditional cold Quick Notes layout and metadata reuse

**Owners:** `src/gui/notes_dialog.rs::measure_notes_geometry`, `::ensure_notes_geometry`, `::maybe_refresh_derived`, metadata/projection and anchor owners.

**Tasks**
1. Baseline separately: note-cache snapshot, metadata preparation, lowercase search index construction, search-only filtered projection, full cold geometry and warm viewport paint. Use 100/1k/5k notes at standard 640-pt and small 180-pt viewport, varying actual available row width. Test case-only query yielding identical complete filtered projection.
2. Inspect `width_before`→`prefix_width`/`width_after` dependency. A wide early header or preview can change wrap for every following preview. Determine which prefix can be safely reused and where a viewport change must invalidate all row measurements.
3. For genuine content-only changes with stable width/font/style/metadata, reuse the independent `NoteRowMetadata` and geometry for unchanged source notes by identity and revisions, but **only** if the `prefix_width` and preceding layout state match exactly. Rebuild affected suffix, content extents and `row_by_identity` in a single consistent candidate; preserve no partial install on failed/raced note revision.
4. Preserve filtered original-entry indices, unsaved editor freeze, new-note sentinel, search projection identity, failed-save/draft retention, horizontal scroll offset, first-visible identity+intra-row anchor remap and near survivor fallback after deletion, bottom reachability and retained offscreen popup owner.
5. Compare every rendered header/preview/separator and full `ScrollArea` content extent against actual eager widgets after font/scale/width/wrap changes, including very long Unicode title and mixed CRLF/metadata preview.
6. Maintain bounded warm widget counts, prevent broad note clone/metadata scans during scrolling, and record cold layout work count. If no meaningful cold cost reduction is possible without risking variable-height correctness, skip geometry rewrite and retain proven warm virtualization.

**Tests:** `quick_notes_eager_geometry_oracle_records_real_variable_rows`, `quick_notes_browsing_virtualizes_rows_and_reuses_geometry`, `quick_notes_scroll_anchor_prefers_identity_then_nearby_survivor`, `quick_notes_popup_owner_stays_with_identity_offscreen_then_closes_on_removal`, `quick_notes_reuses_metadata_and_keeps_sparse_original_indices`, `quick_notes_defers_drafts_and_keeps_last_good_candidate_until_recovery`, `quick_notes_publishes_a_successful_edit_after_the_editor_closes`. **Acceptance:** metadata+projection+geometry parity and real cold improvement or measured skip. **Commit:** `perf(notes-ui): [C2-F] reuse valid Quick Notes cold layout` or decision.

### C2-G — Integrate query-to-first-visible and cold invalidation observations

**Tasks**
1. Wire the C0 invocation-generation timer across query edit (real `TextEdit`/input), search output, accepted result publication, geometry rebuild, subsequent egui render and first usable actual viewport frame. Determine a consistent boundary for query text when provider work is pending; avoid reporting incomplete results as completed.
2. Record exactly when a new query unnecessarily triggers two rebuilds, when select-navigation consumes a cold layout, and which result changes are due to a plugin/catalog change rather than user typing. Maintain Track A `LauncherRowsBuilt` work-unit meaning.
3. Add controlled workload: type 1, 2, 3, 5 chars in quick sequence; alternate prefix `app`/plugin; change filter but preserve result members; resize at 100/1k/10k results; traverse selected first→middle→last and open/close context menu. Include headless interaction tests with settled egui frames and optional real native input latency smoke.
4. Ensure background query updates cannot publish stale results, close another context menu or overwrite newer search input. Preserve native radial separate visibility, tap/hold gesture operation and deferred plugin fallback semantics.

**Tests:** search+root geometry suites; GUI query focus/keyboard navigation, `screen_draw_priority_fixture_requires_normal_app_query_without_read_only_effects`, `deferred_activation_from_results_*`, relevant radial query tests. **Acceptance:** consistent end-to-end phase accounting and preserved live behavior; no tool/API submission confused with GPU visibility. **Commit:** `perf(ui): [C2-G] trace query through usable viewport`.

### C2-H — Search and cold-geometry comparison/reviewer acceptance

**Tasks**
1. Run final C2 owners on exact accepted source with G0-matched fixture/profile/viewport/font. Tabulate search p50/p95/max and candidate clones, list/grid/Quick Notes cold *and* warm costs, exact measured/reused row counts, output identities and first-usable-frame evidence. Keep unsuccessful candidates' results visible as separate rows.
2. Independently verify all eager-oracle rects and full result ordering, popup lifecycle, selection, scrolling/horizontal overflow, first/middle/last clicks, plugin query search and no-results fallback; compare notes/Actions state after related C1 changes.
3. Confirm that a headless low p95 did not mask a first-open or per-keystroke cold stall, and that a result-cache change did not create false `last_results_valid` behavior. Review all conditional skips for substantiation.
4. If a source change has a mixed result, retain only where measured user-perceived benefit dominates and independent reviewer accepts risk; otherwise revert that subchange without undoing unrelated accepted ones.

**Tests:** focused `src/gui/search.rs` and `src/gui/render.rs` tests, full `gui::notes_dialog::tests`, limited plugin/radial integration; original Track A warm owner for regression comparison only in same profile. **Acceptance:** comparison files `track_c_search_g1.json`, `track_c_geometry_g1.json`, consistent G0 hashes, reviewer clears C2. **Commit:** `test(perf): [C2-H] verify search and cold geometry`.

---

# C3 — Startup critical path and bounded GUI event fairness (6 checkpoints)

### C3-A — Measure startup completeness and first-usable-frame latency

**Owners:** `src/main.rs::startup_indexed_actions`, `::startup_action_catalog`, `::spawn_gui`, `src/performance.rs`, relevant `src/gui/mod.rs` startup/install and plugin consumer constructors.

**Tasks**
1. Trace startup order: settings/recovery, custom Actions load, `IndexCoordinator::submit`/`wait_for_completion`, constructing complete custom-first/indexed catalog, plugin registration, global hotkeys, creation of GUI worker, first enabled root frame. Record which phases are on critical path vs parallel independent work; don't infer from log order alone if timers overlap.
2. Capture actual index traversal for 16/1k/10k synthetic files, warm-cache vs first-use where feasible, complete expected catalog ID/order, `OmniSearchPlugin` FST and `VirtualDesktopPlugin` behavior on first query, and one usable-frame event after plugin readiness. Use owned process/data root; no real user folder enumeration.
3. Confirm baseline includes the known intentional wait **before plugin startup**. Produce a dependency diagram identifying any truly independent initialization that can safely move or be cached without changing the initial plugin/action state.
4. Gather Windows native startup/hotkey-to-first-frame data with bounded instrumentation where available; no UI flash or missing-results shortcut. Any inability to observe actual display latency must be recorded as a limitation.

**Tests:** existing `main.rs` startup catalog and transfer cases, `gui_index_startup_transfer_keeps_the_acknowledged_catalog_without_resubmitting`, plugin initial-search cases, synthetic index fixture. **Acceptance:** measured startup critical path and complete first-query oracle before any migration. **Commit:** `test(startup): [C3-A] characterize first usable frame and catalog readiness`.

### C3-B — Conditional critical-path startup improvement

**Trigger:** C3-A demonstrates a material wait outside the required catalog-ready barrier that can be shortened safely. Do **not** remove index readiness by default.

**Tasks**
1. Prefer low-risk dependency-graph changes (defer strictly optional diagnostics or noncritical UI resource work, avoid redundant query/geometry preparation, reuse existing ready catalog or measure one-time caches). Use the existing coordinator, no second indexing worker.
2. If considering async-first-frame before indexing, require a complete design to update immutable `OmniSearchPlugin` and `VirtualDesktopPlugin` catalogs/FST without a missing startup query, stale action execution, duplicate action counts or newly observable intermediate state. Require explicit critical independent reviewer approval and separate ABI/API/ordering tests. **Default decision:** keep pre-GUI completed index and skip if no proven parity path.
3. Preserve initial error propagation, singleton data-directory ownership, startup recovery, hotkey registrations (tap/hold), first usable grid contents and native window focus. Any background stage must have bounded pending generations, single publication owner, nonblocking shutdown, no post-exit wake and last-good semantics.
4. Compare full process→first-ready/hotkey latency and CPU work against G0 at 16/1k/10k; extra memory/worker cost and no-results flashes are regressions.

**Tests:** `startup_action_catalog` unit/integration and `gui_index_startup_*` plus actual native startup smoke if available. **Acceptance:** improved *complete* first-usable-frame latency and identical initial plugin results or `SKIPPED (readiness contract)` with evidence. **Commit:** `perf(startup): [C3-B] shorten verified startup critical path` or measured gate decision.

### C3-C — Characterize event queue aging, bursts and repaint fairness

**Owners:** `src/gui/watch.rs::process_watch_events`, `src/gui/mod.rs::register_event_sender_with_wake`, `::send_event`, `EventSinkRegistration` accounting and `ViewportWake`; root `render.rs` update call.

**Tasks**
1. Inventory **every** `WatchEvent` variant and sender, including `Actions`, `IndexReady`, notes/todos/bookmarks/folders, `ExecuteAction`, radial dispatch/prepare/resolve, recovery/emergency, virtual desktops and independent background work. Classify per-event idempotence, ordering/acknowledgement obligations, and cost. Do not assume file notifications can be blindly collapsed: actions version increments and last-good publishing may depend on their order.
2. Add opt-in bounded queue arrival/consume counters and queue-age samples for high-level event classes (no user content). Observe the existing pre-owner buffer capacity separately from the active `std::sync::mpsc::channel` queue; do not conflate them.
3. Deterministic channel-gated test sends bursts and sustained producers, including a slow Actions event, urgent recovery/radial dispatch, background IndexReady and new data publication. Record per-frame drained count, elapsed handler work, lag until next repaint, number of `ViewportWake` requests and whether queued count reaches zero; do not rely on sleeps for race correctness.
4. Determine whether starvation/visible frame stalls are **real**, whether handler duration dominates queue size, and whether prioritization can preserve FIFO semantics. Do not edit drain policy until independent review accepts the characterization.

**Tests:** existing `send_event`/registration/wake tests around `src/gui/mod.rs` 5k range, watcher tests, emergency recovery/radial tests and new `track_c_event_queue_*` serial deterministic cases. **Acceptance:** event-type delivery matrix and bounded burst G0 p50/p95/worst queue age. **Commit:** `perf(events): [C3-C] measure GUI event backlog and handler cost`.

### C3-D — Conditional coalescing of proven-idempotent notifications

**Trigger:** measured duplicate lightweight events account for meaningful queue/handler cost, and per-variant semantics support coalescing without losing required state transitions.

**Tasks**
1. Propose an **explicit whitelist** only for level-triggered “read latest state” notifications that are provably coalescible (e.g. some file rescan requests). Do **not** coalesce `ExecuteAction`, clipboard/recycle operations, radial dispatch, gesture/hotkey invocations, emergency recovery, errors/diagnostics, capacity acknowledgements, pending worker response receipts or any action with irreversible effect.
2. Preserve arrival ordering relative to noncoalesced events: coalescing must not move a later state update before an earlier action. Prefer one coalescing marker and on-demand latest-state read rather than dropping arbitrary FIFO events; document exactly where version increments and diagnostics remain truthful.
3. Retain bounded owner-lifetime slots and registration teardown. Prove no lost wake/queued accounting imbalance, even when send fails, owner is replaced, event is consumed during producer send, or a newer worker generation publishes after shutdown.
4. Test simultaneous watchers, missing/invalid/changed/unchanged actions, empty roots, note revision, stale background index completion; ensure correct last-good and current custom/indexed order.
5. Measure quiet and burst cases; if cost is dominated by expensive single events rather than duplicates, mark **SKIPPED** and avoid extra state machine complexity.

**Tests:** new deterministic coalescing race tests plus `actions_watcher_*`, `gui_index_*`, radial dispatch and emergency recovery tests as relevant. Independent concurrency reviewer must inspect actual event ordering and sink lifecycle. **Acceptance:** fewer redundant operations with complete observable event semantics or explicit gated skip. **Commit:** `perf(events): [C3-D] coalesce safe latest-state notifications` or decision.

### C3-E — Conditional bounded event-drain policy and urgent-work fairness

**Trigger:** C3-C shows a sustained flood causing measured frame starvation, and C3-D's targeted coalescing is inadequate.

**Tasks**
1. Define an explicit per-frame **work budget** (number of events and/or monotonic elapsed time) with a fairness guarantee. Compare moving heavy handler work off the GUI owner first if one event dominates; a drain cap alone cannot fix one 115ms synchronous publication.
2. Preserve FIFO-sensitive actions, stable `execute` ordering, `ViewportWake` scheduling on deferred remainder, and appropriate treatment of high-priority emergency/radial close. If adding priority classes, prove by deterministic receipt trace they cannot overtake earlier state dependencies or starve ordinary events.
3. Do not implement a busy repaint loop or high-frequency polling; request a repaint only when work remains, with bounded wake coalescing. Preserve shutdown/owner revocation, accurate queued counters, cancellation and no after-drop callbacks.
4. Test bursts bigger than cap, continuously arriving producers, paused UI, slow handler, nested event emission, malformed persisted payload, latest index completion and emergency recovery, no duplicate dispatch under retried repaint.
5. Measure p50/p95/worst query/keyboard frame latency and age distribution with and without policy at equal workload; record maximum queue depth and completed work. Reject budget policy if it introduces input/control latency regressions or broken ordering.

**Tests:** isolated `track_c_event_budget_*`, existing `EventSinkRegistration` tests, relevant `src/gui/watch.rs` plus representative actual egui frame/radial/notes. Critical reviewer gate. **Acceptance:** verified bounded frame work, no starvation or data loss, material responsiveness benefit; otherwise `SKIPPED` with no behavior change. **Commit:** `perf(events): [C3-E] bound GUI event work with fairness` or gate decision.

### C3-F — Deterministic startup/event race and native smoke acceptance

**Tasks**
1. Compose startup/queue changes with Track A index coordinator. Test blocked initial scan, startup failure, foreground hotkey during initialization, root/cap changes, stale `IndexReady`, action watcher while scan blocked, shutdown while pending, queued radial dispatch and note save while UI is busy. Verify current generation/config, complete catalog and latest custom prefix, no stale user-visible result.
2. Assert no lock is held across disk I/O/egui callbacks, no worker spawned per event, no unbounded queue/trace introduced; verify drop/reaper lifecycle and no callback after viewport destruction.
3. Run actual Windows smoke with owned temporary data and carefully scoped hotkey (if accessible). Verify hotkey toggles grid, query caret/focus, first complete action catalog, radial tap/hold, note browser open, and orderly exit. Distinguish native API success from visible composition/latency; do not claim not-run cases.
4. Compare C3 startup and event fairness G1 with C0 G0 on identical host/profile/fixture; retain any tail-latency regressions, startup slow-root worst case, CPU overhead and skipped C3-D/E decisions.

**Tests:** selected `main.rs` startup tests, `gui_index_*`, `actions_watcher_*`, event sink/radial tests, owned Windows smoke if available. Independent reviewer checks full startup/dispatch correctness and measurement honesty. **Acceptance:** `track_c_startup_events_g1.json`, documented native evidence/limits, event matrix unchanged. **Commit:** `test(perf): [C3-F] verify startup and GUI event fairness`.

---

# C4 — Final cross-surface validation and handoff (4 checkpoints)

### C4-A — Exact-source focused regression matrix

**Tasks**
1. Freeze implementation state and enumerate changed owners. Run the **smallest sufficient** focused tests for notes, action publish, search, root geometry, Quick Notes, startup, event registration, IndexCoordinator cancellation, history/radial and native cursor smoke if touched. Tests should include failure and stale-result recovery, not just happy path.
2. Verify Track A/B invariants: idle note refresh takes zero full snapshots when unchanged; history normal-count preparation stays bounded; warm list/grid/Quick Notes paint stays viewport-bounded; index request remains off-thread and generation-safe; native stationary source refresh unaffected; `[profile.fast-dev]`/`[profile.iteration]` plus canonical release unchanged; no test target removed.
3. Verify output compatibility: all production `Action`/note/persistence serializations unchanged, saved settings stable, plugin flags and hotkey window-state behavior unchanged. Validate safe fallback to eager code if an optional optimization was rejected.
4. Recheck new source for accidental unbounded metric cardinality, deadlocks/reentrant locks, unsafe stale Arc reuse, failure swallowing, and all conditional checkpoints. Do not expand into unrelated whole-suite tests without explaining why.

**Tests:** `cargo check --lib`, targeted `cargo nextest run --lib -E ...`, selected `--test notes_plugin`, `--test history`, `--test domain`, watcher/plugin/radial targets per changed scope; `git diff --check`, changed-file format. **Acceptance:** tests/results recorded with exact counts and SHA; no unresolved critical reviewer issue. **Commit:** `test(perf): [C4-A] complete focused cross-surface regression`.

### C4-B — Native end-to-end responsiveness and cleanup protocol

**Tasks**
1. Use `track_c_native_recipe.md` with authorized interactive Windows session and **owned temp settings/data**. Never run native hotkeys against real user data or take private screenshots. Establish exact root process/GUI HWND ownership, startup/catalog complete, a known synthetic action and enabled state; then exercise root hotkey (tap/hold), list/grid search typing, selected actions/context menu, note open/Quick Notes, optional settings change and close.
2. Record process start→index-ready→plugin-ready→usable frame/hotkey→visible state timings separately, with p50/p95 only when sampled sufficiently; include CPU/API versus actual displayed behavior qualifiers. Verify no stale results, blank grid flash, stray radial window, lost input focus or prematurely disposed GUI worker.
3. Capture bounded event flood responsiveness using only synthetic/owned changes and explicit cancellation; observe protected urgent behavior, no runaway repaint, stable worker/thread/resource count where tooling permits, and deterministic quit/cleanup.
4. If desktop control unavailable, run headless deterministic owners but label native `NOT RUN` and give precise manual commands/instructions. Never manufacture PASS or missing display latencies.

**Acceptance:** native smoke PASS where available, failures corrected/retested or honestly recorded; no observable behavior regression. **Commit:** `test(native): [C4-B] verify live launcher responsiveness` (or docs-only NOT RUN with acknowledged limitation).

### C4-C — Final comparable performance report, including unfavorable results

**Tasks**
1. On final accepted production-source SHA run the exact G0-matched Track C owners serially in the same profile/fixture environment. If a source change affects the benchmark harness, explicitly separate the changed scenario domain and avoid bogus before/after ratios. Preserve original G0 files.
2. For every scenario publish source SHA, fixture/hash, profile, sample/warmup counts, p50/p95/max, work units, real cold observations/distribution, output identities, and Windows/native limitations. Include G0→G1 ratio only where exact metric boundary is comparable.
3. Summarize grouped action catalog/notes/Search/geometry/startup/event latency and overall interaction response, not a fabricated app-wide percentage. Highlight regressions and neutral changes, and annotate `SKIPPED` experimental gates.
4. Report added cache memory/cold preparation cost and any loss of warm performance, plus human-perceived first-open timings. Separate background scan time from GUI publication, cached Nextest test execution from compilation, and native API submission from actual visual frame.
5. Reviewer reproduces at least one representative size/signature, confirms no biased favorable reruns, and checks all 29 checkpoints' evidence. Output `track_c_final.json` and `track_c_results.md` with no private payloads.

**Acceptance:** comparable final report with evidence for both wins and limitations; independent reviewer clearance. **Commit:** `perf(report): [C4-C] finalize runtime foundations comparison`.

### C4-D — Close 29-checkpoint ledger and prepare local-only handoff

**Tasks**
1. Audit every checkpoint C0-A…C4-D. Record `PASS`, `SKIPPED (measured gate)`, `NOT RUN`, etc. with exact local commit, tests, performance evidence, reviewer and unresolved limitation. Check 29 unique IDs and no remaining unjustified `PENDING`.
2. Inspect cumulative `git diff <recorded_base_sha>..HEAD --stat` and `git diff --check`, relevant source changes, `Cargo.toml` profiles, `Cargo.lock`, archive-free documentation, new performance files and untracked artifacts. No Track A/B metric rewriting, test hiding, runtime UI redesign, accidental dependency change or unexpected output under tracked files.
3. Ensure all generated benchmark input/raw logs/private locators remain ignored. Commit source, test and public sanitized documentation only; never commit machine-private notes/clipboard/paths/large source-tree copies.
4. Verify baseline-to-feature lineage, local branch `runtime-foundations`, clean intended worktree, no automatic Git network action, and no active background cargo/native fixture processes started by the campaign. Keep optional SSD comparison as a future **separate** proposal with no guaranteed gain.
5. Produce short user-readable summary: what improved at which scale and profile, what did not, decisions skipped and why, functionality checks, how to reproduce measurements, known native limits, base/full final HEAD, local-only status. Do not invent closing commit SHA inside a file being created by that commit; resolve with `git rev-parse HEAD` in the final orchestrator handoff.

**Acceptance:** 29 closed checkpoints, complete reviewed reports, no hidden failures or unsupported claims, local commits only, source/working tree preserved. **Suggested commit:** `docs(perf): [C4-D] close runtime-foundations handoff`.

---

## 3. Mandatory semantic acceptance matrix

| Domain | Must remain unchanged | Required evidence |
|---|---|---|
| Notes | same resolved wiki links, aliases/ambiguity, todo categories, snippets/reasons/order, unsaved edits, last-good retries, exact revisions | full eager-row oracle, version tests, editor save/fail paths |
| Actions | custom-first indexed order, `args`, duplicate-ID precedence, `Arc` identity on no-op, version bumps, live query/command completion/history/radial | full Action+lookup comparison; IndexReady race, watcher and pin tests |
| Search | exact/fuzzy, alias matching, usage and stable tie order, plugin/command priority, pending/fallback, full result list | eager ordered Action/state oracle; mixed provider regression |
| Root list/grid | settled egui response/visual extents, variable-height row and column global widths, tall overflow, stripes, cursor selection, scroll and popup identity | existing actual-eager oracles + live click/keyboard tests |
| Quick Notes | title/meta/preview geometry, filtered original indices, horizontal width propagation, anchor survival, offscreen popup, drafts | actual eager widget oracle, insertion/deletion/removal/reopen tests |
| Startup | complete initial indexed plugin catalogs/FST, hotkey readiness, single-instance ownership, recovery, no blank usable frame | startup source contract, exact initial catalog + native smoke |
| Event loop | all urgent effects delivered once, ordering, sink queued counters and `ViewportWake`, no starvation or stale worker publication | bounded deterministic burst receipts; actual frame timing |
| Existing performance work | cheap note idle, viewport-only paint, bounded index coordinator, native effects, Track B profiles | relevant Track A/B scoped regressions and source diff |

## 4. Scoped test recipe (templates; verify actual live target and test names)

```powershell
# Before any work; capture outputs to Track C provenance.
git branch --show-current
git rev-parse HEAD
git status --short
rustc -Vv
cargo nextest --version

# Fast compile and an ordinary focused library test.
cargo check --lib
cargo nextest run --lib -E 'test(gui::notes_dialog::tests::)'
cargo nextest run --lib -E 'test(root_list_) | test(root_grid_)'
cargo nextest run --lib -E 'test(gui_index_) | test(actions_watcher_)'
cargo nextest run --lib -E 'test(note_cache_) | test(note_backlink_count)'
cargo nextest run --lib -E 'test(history_prepare_)'

# Target FIRST, then case filter. Keep --cargo-profile fast-dev series separate.
cargo nextest run --test notes_plugin
cargo nextest run --test history
cargo nextest run --test domain -E 'test(indexer_)'
cargo nextest run --cargo-profile fast-dev --lib -E 'test(gui::search::tests::)'

# Opt-in serial Track C measurements (examples only: final owner filters must exist).
$env:MULTI_LAUNCHER_PERF = '1'
$env:ML_TRACK_C_BENCH_MODE = 'small'
cargo nextest run --lib --test-threads 1 --run-ignored ignored-only -E 'test(track_c_benchmark_)' --success-output immediate-final --no-output-indent
# Record exact SHA, profile, fixture and work units for full matched run.
$env:ML_TRACK_C_BENCH_MODE = 'full'
cargo nextest run --lib --test-threads 1 --run-ignored ignored-only -E 'test(track_c_benchmark_)' --success-output immediate-final --no-output-indent
Remove-Item Env:MULTI_LAUNCHER_PERF -ErrorAction SilentlyContinue
Remove-Item Env:ML_TRACK_C_BENCH_MODE -ErrorAction SilentlyContinue

# Routine non-test GUI build; keep canonical release separate.
cargo build --profile iteration --bin multi_launcher
cargo build --release --bin multi_launcher
```

Do not treat the example `test(track_c_benchmark_)` filter as evidence that owners already exist: **C0-C must create them and verify case names**. The default profile and fast-dev are distinct output/cache directories. Also, ordinary `cargo check`/Nextest commands may rebuild unexpectedly after changing profile or worktree path; report true build versus execution time.

## 5. Ownership handoff and progress format

For each checkpoint, parent orchestrator supplies a bounded packet to an implementer with: exact source SHA; owner files/types/callers; objective and non-goals; architecture invariants; smallest acceptable code surface; known owner tests; new fixture/oracle required; matched benchmark command; reviewer questions; done/revert/skip conditions; proposed subject. The planner may do **read-only** reconnaissance of a future nonoverlapping owner; do not run two implementation writers against `src/gui/mod.rs` or `src/gui/render.rs` simultaneously.

**Parent ledger example:**

```text
C2-D | IN_PROGRESS | owner: root list geometry | source: <SHA>
G0: 10k cold p95 ... (n=...), warm p95 ...
Source diff: list geometry only; all eager row/clip/selection oracles PASS
G1: ...; skipped alternative: ...
Reviewer: PASS / changes needed; remaining native: NOT RUN
Commit: <local SHA after verification> | Remote: NO PUSH
```

**Milestone stop/go:** do not begin C1 until G0 is frozen; do not launch C2 geometry writer until C2 search and related C1 publication decisions are settled; do not change C3 event policy before C3-C queue characterization; do not run C4 final metrics during ongoing compilation or active test writers. If any high-risk feature lacks a proven safe path, document and skip it rather than holding the entire project hostage to that refactor.

**Final definition of success:** fewer observable end-user stalls in the selected, reproducible heavy workloads **without changing the launcher users already have**. An honest data-backed skip is preferable to an unverified architectural change. Finish with the actual final `git rev-parse HEAD`, full base, test evidence, benchmark limitations, and local-only handoff.
