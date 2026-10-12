# Track C baseline provenance

Captured 2026-10-10 before feature branch creation. LOCAL ONLY.

- Initial branch: `Runtime-Responsiveness`.
- Approved initial HEAD: `bcfd24d226ffb46c85f9f7ce7bdd919466191070` (`plan`).
- Initial `git status --short`: empty (clean; no untracked files).
- `runtime-foundations` did not exist. Worktrees: primary checkout at the above HEAD; existing detached Track B baseline at `4221c349acc34beb07d57852b70bfddbed0931ad`, left untouched.
- rustc 1.97.1 (8bab26f4f 2026-07-14), full commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, host `x86_64-pc-windows-msvc`, LLVM 22.1.6.
- cargo 1.97.1 (c980f4866 2026-06-30).
- cargo-nextest 0.9.135 (610eefb88 2026-05-14), full commit `610eefb88762529a316373f4a50f5fd9194c3c35`.
- Active power scheme: Balanced. CIM queries for OS/CPU/RAM/storage were denied. Follow-up read-only .NET/registry inventory reports Windows NT 10.0.19045.0, 8 available logical processors, Intel Core i7-7700K CPU @ 4.20GHz. Physical core count, RAM and storage media/volume details remain NOT MEASURED (volume query also unavailable). Historical Track B host details are context only.

## Preservation inspection

Current source contains the original 15 performance metric families, paired versioned note snapshots, cheap note idle gate, root variable-height geometry cache, Quick Notes metadata/geometry, and IndexCoordinator. Track B fast-dev inherits dev with line-table symbols; iteration inherits release with opt-level 2, 64 codegen units, LTO off and incremental. These profiles and canonical release remain unchanged. Native controller retains stationary-source refresh after a successful unchanged sample.

Both required Track C companion plans are present under docs/plans and have been read. AGENTS.md and all three agent TOMLs were read. Track A final/results and Track B reports remain historical, read-only context.

The orchestrator kickstart was supplied as the attached pasted text and read completely. There is no repository `multi_launcher_track_c_codex_start.md`; the supplied kickstart is the execution contract, without inventing another companion document.

## Verified live owners

| Boundary | Live owner / callers |
| --- | --- |
| Notes | `gui/note_panel.rs`: `maybe_refresh_heavy_derived` gates `refresh_heavy_derived`; three `backlink_rows_for_note` calls; `alias_collision_warning`. `plugins/note.rs`: `NoteCache::from_notes`, `note_cache_snapshot_with_version` own canonical publication. |
| Actions | `gui/mod.rs`: `publish_custom_actions_with_indexed_tail` / `publish_actions` call `gui/search.rs::update_action_cache` then request query refresh. `gui/watch.rs` owns Actions file reload; `gui/indexing.rs::process_index_ready` / `publish_indexed_tail_if_changed` validate generations/config and merge the latest custom prefix. |
| Search | `gui/search.rs`: `search_actions`, `search_read_only_outcome*`, `apply_usage_weight`, `search`; stable score sorting follows catalog then plugin concatenation. |
| Root geometry | `gui/render.rs::RootListGeometryCache::{ensure,ensure_grid}`, `render_root_frame`; exact cumulative list widths, global grid widths and tall-cell spill must remain intact. |
| Quick Notes | `gui/notes_dialog.rs`: `maybe_refresh_derived`, `rebuild_projection_if_needed`, `ensure_notes_geometry`, `measure_notes_geometry`, `ui`; preserve original indices, identity anchors and width-prefix dependency. |
| Startup | `main.rs`: `startup_indexed_actions` waits and acknowledges complete catalog, `startup_action_catalog` assembles custom-first, `spawn_gui` passes it to plugin registration before GUI construction. |
| Events | `gui/state.rs::WatchEvent`; `gui/mod.rs` registry/sink/send_event; `gui/watch.rs::process_watch_events`; `visibility.rs::ViewportWake`. Direct watcher/dashboard/index-notifier sends bypass registry bookkeeping. |
| Observation | `performance.rs` original 15 metrics; `performance/workloads.rs` deterministic fixtures/serial timing. Historical `record_frame` logs usable at update start; Track C needs a separate completed usable-root marker. |

Protected urgent/irreversible variants include ExecuteAction, Recycle, radial dispatch/prepare/deferred/capacity responses, ScreenDraw recovery/emergency, diagnostics and worker completions. No queue policy changes in C0. Root absolute row/menu IDs and Quick Notes original-index/identity anchors remain stable.

## C0-D frozen G0

G0 source is the precise accepted C0-C commit `071243db068835617a054a74fb5734a6dd9f128d`. Source/Cargo files were clean at launch; only parent ledger documentation was dirty. Balanced power was reconfirmed. Independent source/method/output/privacy/29-entry ledger freeze review is CLEAR. G0 is frozen; numerical values must not be edited. The C0-B native verification incident remains an explicit exception with actual deletion outcome UNKNOWN; see `track_c_verification_incident.md`.

All six Nextest processes ran serially using `fast-dev`, default Nextest profile, one test thread and ignored-only exact owner selection. Full library: 9/9 PASS, 5156 skipped, build 0.84s, execute 2474.212s. Full binary startup: 1/1 PASS, 37 skipped, build 0.89s, execute 39.787s. Small repeat1: library3/3 PASS, build0.94s/execute18.587s; startup1/1 PASS, build0.87s/execute0.086s. Repeat2: library3/3 PASS, build0.87s/execute17.216s; startup1/1 PASS, build0.84s/execute0.079s. No failed benchmark test or excluded sample. Three existing lib-test unused-variable warnings are unrelated and unchanged.

Every child inherited process-scoped MULTI_LAUNCHER_PERF=1, ML_SKIP_CLIPBOARD_SYNC=1, ML_TRACK_C_PROFILE=fast-dev, ML_TRACK_C_SOURCE_SHA=071243db068835617a054a74fb5734a6dd9f128d and ML_TRACK_C_BENCH_MODE=full (small for repeat checks). Prior process values were restored in finally; user/machine environment was unchanged. Five warmups precede twenty retained measurements per scenario, with nearest-rank ceil(p*n) quantiles and zero exclusions. Setup, eager references, hashing and validation are outside named owner timers. Notes sizes count N note documents plus twelve todos; original three-pass visits are 3*(N+12).

`track_c_baseline.json` retains all 105 Full records (102 library, three startup), 2100 owner samples, 39 geometry cold/warm pairs, six arrays/120 actual width receipts, forty queue-age rows/440 delivered ages, complete/structural/viewport/revision receipts, host/toolchain, actual run IDs, commands and both eight-record Small repeat appendices. Parent strict inventory and repeat identity checks PASS. Independent final output/privacy/freeze review CLEAR. All105Fullrecords/2100samples,120widths,440ages andbothSmallrepeatappendices were verified; no remaining substantive findings. Small rounds match complete identities for all seven non-startup scenarios, structural/fixture/revision/work receipts for all eight; startup raw identities vary with owned paths after full raw equality is checked in each sample. Both timing rounds are retained.

Selected Full G0 observations below are milliseconds. All exact numeric samples and remaining scenarios are in JSON; these are baseline observations without an improvement claim.

| Actual owner / fixture | p50 ms | p95 ms | max ms |
| --- | ---: | ---: | ---: |
| Note heavy refresh, 5000 notes +12 todos | 1153.869 | 1182.838 | 1255.750 |
| Root list 10000, geometry cold / warm fonts | 556.567 | 656.888 | 752.389 |
| Root list 10000, geometry warm | 1.001 | 1.114 | 1.446 |
| Root grid 10000, one column, geometry cold | 528.793 | 548.480 | 875.809 |
| Quick Notes 5000, horizontal cold | 533.385 | 548.450 | 554.538 |
| Quick Notes 5000, horizontal warm | 0.684 | 0.727 | 0.750 |
| Shared dense fuzzy search 10000 | 397.930 | 405.954 | 412.789 |
| Query event through eligible CPU frame 10000 | 9.266 | 9.704 | 11.813 |
| Actions reload, custom10000 + retained tail10000 | 100.162 | 120.684 | 135.161 |
| Concurrent finite offered load, reducer elapsed | 544.407 | 599.435 | 611.471 |
| Finite burst with stale-index gate, reducer elapsed | 341.046 | 419.026 | 422.861 |
| Startup catalog, 10000 indexed +3 custom | 860.864 | 922.779 | 956.396 |

Concurrent delivered age aggregate: 280 values, p50/p95/max268.463/454.059/514.922ms. Finite gated aggregate: 160 values, 152.360/257.248/296.054ms. Finite age includes deliberate setup-gate residence and is not steady-state production lag. Concurrent elapsed sums actual EventDrain intervals, excluding explicit spawn/barrier/join but including active scheduling and recorder contention. Completion debounce can refresh suggestions during longer queue drains; complete hashes can therefore differ across owners despite equal ordered-action structures. Drain-minus-handler time includes real deferred work, not solely observation overhead. Observer capacities16/32 are test bounds, not production queue caps.

Geometry-cold distributions use genuine invalidation after fonts/atlas settle; warm distributions retain geometry. Startup uses a new coordinator over warm OS cache and proves complete indexed catalog acknowledgement/assembly, not process/plugin/display readiness. Search phase clone counters exclude empty-browse command-cache/catalog cloning; Actions counters describe output lengths, not allocation counts. Raw complete identities can include revisions/owned paths; normalized structure and actual viewport identities remain separate.

Native process/hotkey/plugin/display/GPU latency, pre-owner registry residence, allocation/memory, alias-only cost, first-use disk and application-wide improvement remain NOT MEASURED. Native protocol is NOT RUN because this session lacks native desktop control. No historical Track A/B numbers fill these gaps. Fast-dev observations do not establish release behavior.

Reproduce on the exact source using the environment above and `commands` in JSON: first command Full library, second Full startup; third Small library then second Small startup, repeated twice serially. Use new ignored log paths and preserve every run; never overwrite frozen measurements. Strict parsing uses `track_c_collect_owner_results.ps1 -LogPaths <lib,bin> -ExpectedSource <SHA> -ExpectedMode Full -OutputPath <new.json>`; exact inventory uses `track_c_verify_baseline_inventory.ps1 -CollectionPath <collection.json>`. Parser and inventory outputs deliberately remain pending review until independently accepted. New scenarios after freeze require a separate versioned appendix.

Raw original logs remain under ignored target/performance/track-c/c0-d/. Reviewer caught an age-row serialization sort defect before freeze; explicit numeric sample sorting and0..19 assertions corrected the collector, and candidate-v2 was regenerated from unchanged originals. Old candidate is superseded, not used as frozen evidence; no benchmark rerun was required. Protocol/fixture/source/profile meanings and historical Track A/B evidence remain unchanged.