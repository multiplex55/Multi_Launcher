# Track C handoff notes — initial state

This is a live orchestration note, not the final Track C report. It records C0-A's initial provenance and immediate gates. Track C remains in progress.

## Initial provenance

- Initial branch: `Runtime-Responsiveness`.
- Approved starting source: `bcfd24d226ffb46c85f9f7ce7bdd919466191070` (`plan`).
- Initial `git status --short`: clean (empty).
- Local feature branch: `runtime-foundations`, created from the approved starting source. At C0-A start, its HEAD is still `bcfd24d226ffb46c85f9f7ce7bdd919466191070`.
- A detached Track B baseline worktree at `4221c349acc34beb07d57852b70bfddbed0931ad` was observed and left untouched.
- Toolchain: rustc `1.97.1` (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, `x86_64-pc-windows-msvc`, LLVM 22.1.6); cargo `1.97.1` (`c980f4866`, 2026-06-30); cargo-nextest `0.9.135` (`610eefb88762529a316373f4a50f5fd9194c3c35`). Active power plan was Balanced. OS, CPU/RAM, and storage details could not be queried and remain NOT MEASURED. See [`track_c_baseline.md`](track_c_baseline.md) for the full provenance record.
- The current working tree contains the parent-owned baseline note plus this C0-A documentation work. The initial-clean statement above refers to the pre-branch inventory, not the current status.

## C0-A owner and behavior inventory

The approved plan and source notes were read in full. Read-only planner reconnaissance verified these current source boundaries and test seams; parent/planner source inventory sign-off is PASS.

| Area | Verified owner and flow |
|---|---|
| Note refresh and cache | `src/gui/note_panel.rs`: `NotePanel::{maybe_refresh_heavy_derived,refresh_heavy_derived,alias_collision_warning}` and `backlink_rows_for_note`; `src/plugins/note.rs`: `NoteCache::from_notes`, `note_cache_snapshot_with_version`, `note_backlink_count`. The panel's relationship semantics exceed the resolved wiki-link reverse map: preserve prose/wiki/alias/todo relationships, category order and exact row strings. |
| Action publication and index acceptance | `src/gui/mod.rs`: `publish_custom_actions_with_indexed_tail`, `publish_actions`; then `src/gui/search.rs::update_action_cache` and `request_background_query_refresh`. `src/gui/indexing.rs::{process_index_ready,publish_indexed_tail_if_changed}` validates desired config/generation and retains the latest custom prefix. Preserve custom-first order, duplicate-ID winner, unchanged `Arc` identity and last-good publication. |
| Search | `src/gui/search.rs`: `search_actions`, `search_read_only_outcome`, `search_read_only_outcome_without_providers`, `search_read_only_outcome_with_plugin_snapshot`, `search_read_only_outcome_from_scored_plugins`, `apply_usage_weight`, and `search`. Preserve the complete ordered result list, stable score ties, provider/pending state, plugin precedence and `Action.args`. |
| Root list/grid geometry | `src/gui/render.rs::RootListGeometryCache::{ensure,ensure_grid}` and `render_root_frame`. Preserve egui 0.27 variable geometry, global grid columns, bounded visible painting, absolute result/widget identity, keyboard/mouse targets and offscreen popup ownership. |
| Quick Notes geometry | `src/gui/notes_dialog.rs::{maybe_refresh_derived,rebuild_projection_if_needed,ensure_notes_geometry,measure_notes_geometry,ui}`. Preserve filtered original indices, note-identity anchors and popup ownership, draft deferral, last-good snapshots and variable-width/wrap behavior. |
| Startup catalog | `src/main.rs::{startup_indexed_actions,startup_action_catalog,spawn_gui,main}`. `startup_indexed_actions` must finish the complete catalog before plugin registration; OmniSearch and VirtualDesktop receive the complete initial catalog. |
| Event delivery | `src/gui/state.rs::WatchEvent`; sender registration/accounting and `send_event` in `src/gui/mod.rs`; `src/gui/watch.rs::process_watch_events`; `src/visibility.rs::ViewportWake`. Preserve delivery/order for `ExecuteAction`, emergency recovery, radial dispatch/control and worker completion (`IndexReady`). No urgent or irreversible event may be dropped or reordered. |
| Performance observation | `src/performance.rs` has 15 existing metric families; `src/performance/workloads.rs` contains test-only deterministic workload builders. Extend without changing historical metric meanings. |

Exact existing test names verified during reconnaissance include:

- Performance: `performance::tests::{diagnostics_switch_accepts_only_explicit_truthy_values,subsystem_metrics_are_opt_in_bounded_and_resettable,subsystem_metric_atomics_aggregate_concurrent_samples}`.
- Notes: `note_cache_versioned_snapshot_pairs_revision_and_notes_during_publication`, `backlinks_grouping_splits_categories`.
- Geometry: `root_list_geometry_matches_eager_selectable_label_layout`, `root_grid_geometry_matches_settled_grid_cells_and_global_extents`, `root_grid_tall_cell_spill_stays_visible_without_intermediate_rows`, `quick_notes_eager_geometry_oracle_records_real_variable_rows`, `quick_notes_reuses_metadata_and_keeps_sparse_original_indices`, `quick_notes_defers_drafts_and_keeps_last_good_candidate_until_recovery`, `quick_notes_scroll_anchor_prefers_identity_then_nearby_survivor`.
- Event delivery: `event_sink_attaches_wake_to_work_already_in_queue`, `event_sink_enqueues_before_waking_and_handles_bursts`, `disposed_or_dead_event_sinks_are_not_called_again`, `event_sink_delivers_work_emitted_before_first_owner_registers`.
- Actions/index: `actions_watcher_retains_invalid_then_publishes_valid_without_local_double_bump`, `gui_index_accepted_publication_updates_search_history_pins_and_radial_catalog`, `gui_index_equal_completion_preserves_catalog_and_search_state`, `gui_index_completion_merges_the_latest_actions_prefix_after_a_blocked_scan`, `gui_index_failure_retains_last_good_actions_then_recovers`, `gui_index_empty_config_clears_tail_immediately_and_ignores_cancelled_result`, `gui_index_startup_transfer_keeps_the_acknowledged_catalog_without_resubmitting`.
- Startup binary: `startup_index_returns_the_complete_catalog_and_acknowledges_before_transfer`, `startup_index_skips_empty_roots_and_propagates_scan_failures`.

No tests or builds were run for this documentation-only scaffold. No runtime, test, profile, or fixture code was changed. Parent/planner accepted the owner inventory. C0-A is PASS, with its local commit resolved by the parent after committing these files.

## Next gates

1. Parent reviews and signs off the owner map and exact 29-row ledger, then records the local C0-A commit SHA.
2. Complete C0-B bounded instrumentation and C0-C fixture/oracle work with their own focused checks and review.
3. Only after C0-B and C0-C are accepted, run C0-D on the exact accepted source and freeze matched G0 measurements and the native recipe.
4. **No C1, C2, or C3 optimization begins until C0-D has a complete frozen baseline and independent method/source review.**

G0 is NOT FROZEN. Track C tests and benchmarks have NOT RUN; Track A/B measurements remain historical context and are not substitutes for current G0 values.
