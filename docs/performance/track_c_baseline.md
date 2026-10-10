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

## Measurement state

G0 is NOT FROZEN. C0-B instrumentation checks passed with the explicit native verification incident recorded in `track_c_verification_incident.md`. C0-C's shared support, all actual owner oracles and Small benchmark smoke passed scoped checks and independent source/output review. Actions and startup fresh-process repeats retain fixture/structural identities; startup raw complete IDs legitimately vary with owned paths. No G0 benchmark distribution has been measured and no performance result is borrowed from Track A/B. C0-D next freezes matched full-mode observations on the precise accepted C0-C commit; the native recipe is explicitly NOT RUN. No source optimization is authorized before that freeze.
