# Track C C1-G note/action stage acceptance

Status PASS; 11/29 closed. LOCAL ONLY. Accepted runtime source ccebeef374ec3f8063e264c0c5df60a827db835f; C1-F documentation closure / exact verification HEAD c55c4d3de24fde606b1b3f850c07d450bd05eb49. No runtime changes in this checkpoint.

## Bounded verification milestone

Objective: verify the accepted C1 note/action stage against the original frozen C0 owners and complete semantic oracles. Parent owns evidence, runner, Cargo and local commits; planner c0_recon supplied the read-only packet; c0_boundary_review independently reviews cumulative source and actual evidence. No implementation writer is needed because production source remains unchanged.

Dependencies: C0-D frozen G0 source071243db068835617a054a74fb5734a6dd9f128d; C1-A/B/D/E/F PASS and C1-C measured skip. Preserve all prior artifacts and adverse observations.

Scope: original note heavy-refresh and changed-Actions watcher owners; current note lifecycle/alias units and four owned notes_plugin integration cases. Reuse the already executed ten C1-F action units and non-test library check with exact runtime-source continuity. No additional source optimization, broad suite, release build, geometry, queue, startup or native campaign.

Run serial fast-dev with PERF1, ML_SKIP_CLIPBOARD_SYNC1, exact source SHA and explicit Full mode; five warmups, twenty retained samples, zero exclusions. Exact benchmark names: track_c_benchmark_note_panel_heavy_refresh_owner and track_c_benchmark_actions_reload_owner. Inventory exactly four records/eighty samples: note100/1000/5000 (12todos each), fixture6850096c0be841c8/7d68ec3bd41b2ae8/4324fcfb2ca1f9d2; action20000 fixturec75034d300362287. Existing track_c_collect_owner_results.ps1 does not enforce inventory, so parent and reviewer do so explicitly. Produce track_c_notes_g1.json and track_c_actions_g1.json without overwriting G0 or C1 evidence.

Exact library semantic cases: track_c_oracle_note_refresh_matches_frozen_full_rows_and_revisions; track_c_c1a_open_note_panel_refresh_matches_frozen_rows_and_actual_work; track_c_c1b_note_save_on_close_persists_and_refreshes_backlinks; unchanged_heavy_check_skips_note_snapshot_alias_hash_and_recompute; alias_collision_warning_matches_frozen_reference_for_semantic_edges; alias_warning_query_failure_is_reported_and_add_rename_saves_continue. Integration notes_plugin only: note_link_dedupes_backlinks; note_alias_supports_open_rm_and_labels; extract_tags_skips_code_fences; note_meta_wrap_links_integration_parses_aliases_and_preserves_links_regression. Planner confirmed owned guards; prior accepted reviews cover their isolation. Never select generic constructor or cleanup tests.

Invariants: complete ordered rows and Actions/args; canonical note publication locks/version/debounce/last-good behavior; advisory alias failure does not prevent save; custom-first catalog, last-ID-wins, caller-owned versions, retained equal Arc, latest prefix and stale completion rejection, current query/completion/pins/history/radial coherence. No workers or persistence migration introduced. No asynchronous projection branch exists, so async-candidate invisibility is inapplicable.

Comparison: note work intentionally changed3*(N+12) to N+12 and mentions_scan now means shared classification. Preserve historical G0 metric definitions; add a G1 clarification rather than rewriting artifacts. C0 action counters are output lengths10000:10000:20000:20000:19999:1, not clone receipts. Use matched C1-E/F supplemental evidence for accepted-index timing and actual key-copy work. Compare structural identities after full equality; temporary-path/mutable-version-dependent complete hashes need not equal across runs. Preserve every p50/p95/max and all contrary observations, without application-wide/native/release claims.

Acceptance: exact inventory/profile/source/fixture continuity; full eager semantic assertions pass; scoped checks pass; independent cumulative C1 source and numerical/privacy review CLEAR; report accepted/skipped hypotheses and limitations; local closure commit. No automatic extra test layers.

NOT RUN: note_panel_auto_save::note_panel_auto_saves_on_close lacks owned isolation and uses production construction; inert close/save unit covers the scoped boundary. Complete history integration similarly remains unselected; pure C1-F history units are reused. Native input/display/process readiness NOT RUN/NOT MEASURED. C0-B cleanup incident outcome remains UNKNOWN.
Independent cumulative C1 source/runner review CLEAR: canonical note locks/coherent snapshot/equal revisions, debounce and last-good retry preserved; fused category order remains exact; alias query takes one cache generation and errors remain advisory before save; ID lookup/version/no-op/publication consumer semantics preserved; no new workers/formats/retained C1-C projection. Runner exactsource/isolation/filter/envrestoration approved. Fresh semantic run currentlycompiling; no C1-G test result or benchmark result yet. Rawsemantics-1.log retained.

Current-source semantics PASS10/10 across2binaries (sixlib/fournotes_plugin), Nextest196e6b79-e431-426c-9dc5-d3adf2c3045d,fast-devserial,build1m29s,execution1.358s,5193skipped. Three prior render testwarnings unchanged. OriginalFulltwoowner run nowlive Nexteste2d331fe-d5a8-44f1-9b8f-14b489299502/session66807,build0.89s; rawc1-g/full-1.log. Exactsourcec55c4d3de24fde606b1b3f850c07d450bd05eb49; sourcefrozen. NoFullresultyet.

## Actual verification and matched G0/G1 comparison

Exact-source Full PASS2/2, Nextest e2d331fe-d5a8-44f1-9b8f-14b489299502,build0.89s,noteowner74.539s,actionowner42.455s,execution117.005s,5178skipped. Strictcollection exactly4records80samples. Source remains c55c4d3de24fde606b1b3f850c07d450bd05eb49; runtime ccebeef unchanged. All four fixture identities and all eighty structural-output IDs match frozen G0. Full eager comparisons pass before hashing. Both G1 artifacts retain every raw sample, complete-state ID, structural ID, revision and work receipt; independent numeric acceptance pending.

All timings are handler p50/p95/max in milliseconds; all samples retained.

| Owner | Items | Frozen G0 | Accepted C1 source G1 | Median change |
|---|---:|---|---|---:|
| NotePanel::maybe_refresh_heavy_derived | 100 | 22.3415/23.4536/26.5596 | 8.7669/12.2860/13.4603 | -60.76% |
| NotePanel::maybe_refresh_heavy_derived | 1000 | 229.0207/254.2780/385.6323 | 81.5589/88.0257/92.4449 | -64.39% |
| NotePanel::maybe_refresh_heavy_derived | 5000 | 1153.8690/1182.8377/1255.7501 | 412.7649/531.2783/581.9063 | -64.23% |
| process_watch_events_actions_reload | 20000 | 100.1619/120.6837/135.1611 | 87.7558/94.1866/94.8153 | -12.39% |

Note work is112/1012/5012 versus G0 three-pass336/3036/15036; shared classification visits equal fused traversal counts. Actions work remains20000 and six output lengths10000:10000:20000:20000:19999:1. Those original action counters do not measure clone work. C1-F supplementary accepted-index/copy evidence retains its own schema/source. No historical metric is relabeled as an allocator or native measurement.

Contrary continuity observation: current5000-note p95/max531.2783/581.9063ms exceed earlier accepted C1-B original-owner431.9527/510.6336ms, while the current median412.7649ms is close to earlier414.4422ms. Both distributions remain preserved. All current p50/p95/max improve versus frozen G0, but no cause is assigned to tail variation and no claim of monotonically improving runs is made. C1-F mixed custom/small/no-op/error tails and all C1-C modeled publication/memory disadvantages remain in their original reports.

C1-A/E characterize contracts; C1-B fused relationships, C1-D one-lock alias-label query and C1-F catalog-bound ID positions are accepted. C1-C production relationship index remains SKIPPED (measured gate), with both favorable replay and unfavorable publication/memory evidence. No skipped implementation is counted as a speedup. C1-G introduces no runtime changes. Native input/display, release-profile timing, actual process readiness, allocator/RSS and deferred completion latency remain outside these headless fast-dev owner measurements.

Final independent source/numerical/privacy review CLEAR; C1-G ACCEPTED. Fourrecords80samples,28arrays76scalarfields exact acrossraw/combined/splits,12quantiles recomputed, all80structuralidentities/protocols matchG0. Note100/1k/5k p50 improvements60.76%/64.39%/64.23%;Actions20k12.39%. Allp50/p95/max improveagainstG0, qualifiedbyhigher5knote tailsversusC1-B andC1-Fadverserows retained. Currentsemantics10/10+originalowners2/2 verified; earlieraction10units/libcheck reusedwithsourcecontinuity. No additionalverificationrequired. Source unchanged, allCargo terminal,11/29closedLOCALONLY; C2-A starts onlyafterlocalclosurecommit.
