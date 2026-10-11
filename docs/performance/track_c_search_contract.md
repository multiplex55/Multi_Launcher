# Track C C2-A search contract and characterization

Status PASS; 12/29 closed. Base6efe54007057edc5172e38886b3602c46f40bdef (C1-G closure); accepted C1 runtime ccebeef374ec3f8063e264c0c5df60a827db835f. LOCAL ONLY. Planner c0_recon supplied the inspected packet at target/performance/track-c/next-packets/c2-a-prospective.md. Parent owns documents, collector, Cargo, evidence and Git; c0_docs is the sole source writer; c0_boundary_review independently reviews source/method and actual evidence.

## One bounded characterization milestone

Objective: freeze complete ordered search Actions/state and attribute current scoring, eager body ownership, sorting and final materialization before any C2-B optimization. No production ranking, provider scheduling, result replacement, layout or geometry changes.

Ownership: search.rs search_actions and shared merge/materialization boundary; existing query/result-generation owners remain GUI coordinators. Main source scope src/gui/search.rs plus a bounded cfg(test) search observer if useful; gui/mod.rs/render.rs only directly selected fixture setup or test-only receipts; minimal testmodule declaration in gui/mod.rs. Parent must approve any extra source scope before edits.

Current source: local hits clone Action once before stable sort; final scored.into_iter moves Actions. Total required local output clones remain H under full Vec<Action> output. The prospective C2-B hypothesis is smaller sort records and shorter copied-body ownership, not removing a second clone. Catalog candidates precede plugin candidates; usage exactID; descending stable partial_cmp.unwrap_or(Equal), no tie-break. Folder alias Some(None) blocks bookmark fallback. Fuzzy supplied plugin nonmatches can survive with zero score while catalog nonmatches do not. Pending usable output is not complete output.

Required work: preserve frozen C0 eager reference/fixture bodies and add an independent companion local-score/merge reference plus compact explicit provider expectations for gaps. Cover ties/usage/equal casefold labels, duplicate IDs/full args, filters/nohits, zero/negative fuzzy-weight exact override, empty browse, command fallback/provider precedence, resolved exact outputs/query suggestions, pending/revision/catalog mismatch, capacity/manual fallback and provider selection. Capture provider outputs once; never call stateful providers twice for parity. Reuse existing current-query/root CPU-frame oracle, and verify mutable selection/generation versus read-only nonmutation.

Observation: bounded opt-in test-only scopes/counters at responsible sites for visits/filter survivors/scoring/local hits/early body clones, sort inputs/duration, final catalog clones, provider ownership transfer/output moves/count, selected logical copy text/estimated string count. No per-item clocks or extra independent O(N) byte scan. Distinguish measured attempts, structural guarantees and precomputed estimates. Snapshot owner provider invocations are zero; separate controlled ordinary provider test proves invocation behavior. Old Track A/C0 metrics/artifacts keep meanings; new supplementary stage/protocol/collector is distinct. Writer confirms exact static schema/array inventory before parent collector and tests.

Isolation: migrate only selected existing tests using production constructors to TrackCWorkspace plus explicit inert managers, preserving assertions and needed controlled plugins. See prospective packet exactnames. No generic module filters, default plugin discovery, native actions or cleanup tests. Parent audits selected setup before execution.

Verification: stable source/API/protocol handoff; independent source/oracle/isolation/method review; exact safe ordinary tests, Small supplemental and frozen owners; reviewed source commit; exact-source Full supplemental and original-owner comparisons with independent numeric/privacy review. Serial fast-dev, PERF1, ML_SKIP_CLIPBOARD_SYNC1,5warm20retained0excluded. Original search Small5records100samples / Full15records300samples (fivecases100/1k/10k); root query-frame Small1record20 / Full3records60. Preserve original fixtures/signatures. No list/grid campaign or release/native checks. Parent chooses exact ordinary inventory after handoff, not a broad test module.

Acceptance: full independent ordered Action/args/provider state and selection/root assertions; reviewed metric definitions; actual exact-source Small/Full evidence with fixture continuity and all contrary observations. C2-A is characterization only and makes no speedup claim. C2-B needs separate authorization after closure; C2-C and geometry work are outside scope. No tests or C2-A measurements have executed yet.
Production caller audit completed read-only by c0_recon: see track_c_search_callers.md for every shared boundary, direct mutable search trigger and indirect pinned/history/recursive route. It corrects an earlier broad category: action_editor.rs boundary calls are tests, not production callers. This inventory adds no runtime scope or extra test campaign. No C2-A tests/measurements have executed.

## Supplementary receipt protocol (confirmed)

C2A_SEARCH_RESULT schema1/eager_rank_attribution is separate from frozen C0 TRACK_C_RESULT. Each of five existing cases has 20 retained samples after five warmups; Small uses 100 actions and Full uses 100/1k/10k. Exact fields are enforced by track_c_collect_search_attribution.ps1: 27 unsigned numeric arrays (owner elapsed/sort duration and 25 work/state values) plus two complete/structural identity arrays. Logical String values include Some(empty); UTF-8 byte counts exclude capacity, allocator overhead and RSS. Final scored Action values move; final_local_action_clones is a documented structural zero, not a global clone interceptor.

Captured provider input is one Action in every fixture. Even empty browse first projects three lowercase strings / 79 UTF-8 bytes, then discards the scored provider vector; only nonempty cases merge that provider Action. Empty browse has zero scoring/sort work but materializes every catalog Action with its app-prefixed label; the controlled command cache is empty. Snapshot owner invokes no provider; the ordinary controlled-provider test supplies separate invocation evidence.

Parent synthetic collector check passed one fabricated Small inventory and rejected 19 invalid inputs (metadata, unknown/duplicate/missing fields, integer/identity/sample errors, work/timing violations, missing/duplicate scenarios). Ignored target/performance/track-c/c2-a/synthetic-v1 is parser testing only, never runtime measurement. No C2-A Cargo/tests/benchmarks executed yet. Collector and source remain pending independent review.

Collector review by c0_recon found two gaps and the parent corrected them: fifteen literal fixture signatures are now bound to frozen C0 case/count combinations, and logical String counts are exact (dense/empty 313/3125/31250; sparse 10/9/9). The second synthetic check passed one Small inventory and rejected 20 malformed inventories, including a valid-looking but incorrect fixture signature. These remain fabricated parser checks, not runtime results. Local/browse logical-byte totals currently receive positivity/stability checks only; source assertions against independently derived expected payload totals and independent raw-evidence review are required before acceptance. No byte/allocation benefit is inferred.

Parent additionally derived exact payload totals independently from frozen action_fixture (not runtime measurements): 100 actions dense/sparse/browse 13496/290/13896 UTF-8 bytes; 1k 136657/276/140657; 10k 1372759/276/1412759. Browse adds four ASCII label bytes per catalog entry. Collector now enforces these literal totals as well as exact String counts; v3 synthetic check passed one inventory and rejected 22 malformed inputs, including incorrect byte/string receipts. Independent reconstruction review is pending. The earlier positivity-only limitation above describes the superseded draft collector, not acceptance behavior.

Independent read-only reconstruction by c0_recon is CLEAR: all fifteen frozen signature mappings, all byte totals/String counts above and provider projection 79 bytes match source. No Cargo or file writes were used in that review. Final emitter alignment, safe-fixture source review and actual runtime evidence remain pending.

Preliminary independent reviewer c0_boundary_review found one source visibility defect (private Counter re-exported to parent as Metric); correction was sent to the sole writer. Assigned observer/collector/runner slice otherwise CLEAR: cfg(test)-absent production control flow unchanged, disabled observer avoids payload/clock work, local aggregation avoids per-item TLS/clocks, byte estimates correspond to actual copy sites, and exact serial source-guarded runner restores process environment. This is preliminary only; stable source/test isolation/emitter alignment and actual measurements are still unreviewed. Runner PowerShell parse passed; runner has not executed.

Initial writer handoff added the work-receipt oracle/benchmark and isolated selected constructors, with formatting/diff checks PASS and no Cargo execution. Parent identified that the five timing fixtures alone do not satisfy the approved compact semantic edge coverage and returned that bounded gap to the same sole writer before tests. Final source review is pending those additions. The observer is cfg(test) only; when inactive the test build still performs bounded TLS handle lookups/branches, so rerun frozen owners are source-specific and cannot prove zero measurement overhead relative to historical C0. No production observation path is added.

Further independent review identified two companion-reference mistakes before execution: provider empty exact terms must score zero before alias checks, and only provider label matching trims its extracted term (e.g. double-space g queries). Corrections and compact cases were requested in the new companion, leaving frozen C0 untouched. Review also caught controlled plugins excluded by the new inert helper's empty allow-list; the sole writer must explicitly enable controlled providers (or allow all within the inert inventory), preserving the existing cm/fallback assertions. None of these findings required native action or Cargo execution.

## Exact execution selection (pending)

Three new ordinary tests: track_c2a_oracle_search_shared_owner_matches_work_receipts; track_c2a_oracle_covers_local_filter_identity_usage_and_mode_edges; track_c2a_oracle_covers_provider_precedence_and_snapshot_outcomes. Fourteen existing selections: track_c_oracle_search_shared_owner_matches_frozen_complete_results; track_c_fixture_search_ordinary_path_calls_controlled_provider; track_c_oracle_root_query_reaches_current_usable_cpu_frame; search_replacement_clears_selected_index; read_only_search_uses_launcher_boundary_without_mutating_root_state; manual_query_fallback_does_not_reenter_a_blocked_provider; deferred_fallback_suppression_survives_provider_slot_becoming_free; deferred_snapshot_matches_ordinary_g_prefix_provider_selection; clipboard_modify_root_query_returns_direct_open_modify_first_in_fuzzy_mode; clipboard_modify_root_query_returns_direct_open_modify_first_in_exact_mode; exact_display_match_uses_pre_normalized_query_substring; exact_mode_keeps_plugin_resolved_results_but_filters_query_suggestions; exact_match_filter_keeps_complete_clipboard_modify_actions; qr_resolved_action_bypasses_only_for_qr_query_head. Exact fully qualified filters are recorded in ignored c2-a/run-semantics.ps1. No broad module filter or ignored/native cleanup test is selected.

After ordinary PASS, Small runs three ignored owners serially: track_c2a_benchmark_search_shared_owner_attribution (5 records/100 retained samples), original track_c_benchmark_search_shared_owner (5/100) and track_c_benchmark_root_query_to_usable_cpu_frame (1/20). Source/measurement review precedes local source commit; Full runs those same owners against clean committed source: supplemental15/300, originalsearch15/300, originalrootframe3/60. Separate supplemental observer series cannot substitute for frozen-owner comparisons. No release/native or unrelated suite is authorized by this selection.

The final review found and writer corrected two additional test-only fixture defects before execution: an expected vector moved a provider fixture needed later, and calling update_command_cache erased a manually injected command. The expected vector now clones its fixture; explicit command and CachedSearchEntry vectors stay coherent. Formatting/diff checks passed. Final reviewer confirmation and all Cargo execution remain pending.

Final focused reviewer confirmation is CLEAR for source and all 17 selected ordinary fixtures; all prior findings are resolved. Exact ordinary Nextest invocation has now started (serial fast-dev, process-only isolated environment, fully qualified exact filters). Compilation is in progress; no PASS count or benchmark result exists yet.

## Executed pre-commit verification

Ordinary verification PASS: 17 distinct tests in two exact serial fast-dev runs. Initial run5c6cecf8-8266-4945-a5f1-9cf1d36a7ad6 selected15/15 (build1m57,execution3.053s,5169skipped); parent had misqualified the two pure tests under tests instead of clipboard_modify_exact_filter_tests. Corrected only those two: rundf7ea0be-b5ea-406a-bb0b-fe232f6a13dd PASS2/2 (build.99s,execution.059s,5182skipped). No source correction or redundant rerun of the passing15. Three existing render.rs warnings retained.

Small supplementary owner PASS1/1 runbcfc0ea7-859a-41d4-8d50-dfe93d2817a4 (build.85s,execution7.368s): strict collector accepted5records/100retainedsamples. Small frozen owners PASS2/2 run42359837-c3de-41c1-9d7d-60be835b4f52 (build.83s,execution18.207s):6records/120retainedsamples. These are uncommitted-source smoke runs; raw receipts remain explicitly UNCOMMITTED_SOURCE. The historical generic collector only accepts40hex source; an ignored Small-only adapter widens only that parameter validator, retaining all body validation and raw provenance. Tracked original collector unchanged. Independent numeric/identity/privacy/adapter review pending before local source commit. Full has not run.

Independent Small raw/collected review CLEAR: supplemental145arrays/10timingtriplets/100scalars and original42arrays/18statistics, all11 fixture/structural identities match frozen G0. Original work/viewport domains unchanged. Adapter preserves raw UNCOMMITTED_SOURCE and changes only source-validator+Small guard. Retain separate supplemental fuzzy p95/max5.8138/6.8128ms versus original Small4.9639/5.3691ms; no causal overhead or performance gain is established from these separate smokes. Reviewed sanitized smoke bundles saved as track_c_search_attribution_small.json and track_c_search_owner_small.json. Ready for local source commit, then exact committed Full; C2-A remains IN_PROGRESS.

Full supplemental owner PASS1/1 (run62888ed4-3afc-4912-8511-51c76ca3ae49, build.90s, execution63.146s) against committed source57f384f63de1635525b56e757134e15c76c221dd. Strict15records/300retainedsamples; independent435arrays/30timingtriplets/300scalars/source/protocol/15G0fixturestructuraldomains/Smalloverlap/privacy review CLEAR. Dense fuzzy10k owner p50/p95/max397.9052/531.1326/636.5089ms and sort.1764/.2026/.2099ms; sort is a narrow subphase, not a demonstrated dominant cost. Original Full owner run remains in progress; C2-A acceptance is pending.

## Final C2-A acceptance

PASS, independently reviewed;12/29 checkpoints closed, LOCAL ONLY. Source57f384f63de1635525b56e757134e15c76c221dd. Full frozen owners PASS2/2 (run92dc92a4-f4f9-4a98-b087-fb0970025b29, build.86s, root52.139s/search50.131s, total102.282s),18records/360retainedsamples. Review independently verified126arrays/342scalarfields/54timingstatistics and all18 complete/structural IDs, fixture/protocol/work/viewport domains exactly against frozenG0;15supplement overlays and6Small overlaps match. Comparison18rows/108timingvalues also independently verified. Combined Full33records660samples and Small11records220samples;17distinct ordinary tests PASS. All source findings resolved; no further verification required.

Architectural result: unchanged shared search ownership and production ranking, independent eager full-outcome reference, compact semantic edge coverage, actual clone/sort/materialization receipts and owned inert fixtures. Preserve both eager reference bodies in C2-B. Current eager hit copies happen once before sort and final values move; a delayed-materialization candidate still needs H catalog Action copies for H outputs. Measured sorting is a narrow part of these fixtures, especially fuzzy queries. Smaller records alone are not sufficient acceptance evidence.

Reviewed artifacts: track_c_search_attribution_full.json (supplement) and track_c_search_owner_full.json (original owners). Small artifacts remain historical reviewed UNCOMMITTED_SOURCE smokes. Earlier pending paragraphs above are chronological records, superseded by this acceptance.

All contrary observations are retained below. The unchanged ranking algorithm does not explain the timing differences; inactive cfg(test) TLS lookups/branches exist, but their causal contribution has not been established. Headless root-query measurements establish usable CPU-frame receipts, not Win32 input/display responsiveness. Allocator/RSS, release and native tests NOT MEASURED/NOT RUN. The C0 recycle-bin incident deletion outcome remains UNKNOWN; no additional cleanup/recovery/native action was attempted.

### Supplementary actual work and timing (committed Full)
| Case | Actions | Owner p50 / p95 / max ms | Sort p50 / p95 / max ms | Early local clones | Local logical bytes | Browse materializations / bytes |
|---|---:|---|---|---:|---:|---|
| sparse-exact-alias-tie | 100 | 0.1162 / 0.2190 / 0.2348 | 0.0005 / 0.0007 / 0.0010 | 3 | 290 | 0 / 0 |
| dense-exact-empty-term | 100 | 0.0964 / 0.1909 / 0.2324 | 0.0142 / 0.0289 / 0.0395 | 100 | 13496 | 0 / 0 |
| dense-fuzzy-usage-weighted | 100 | 3.9023 / 4.0027 / 4.0667 | 0.0147 / 0.0196 / 0.0216 | 100 | 13496 | 0 / 0 |
| empty-query-browse | 100 | 0.0604 / 0.0842 / 0.1041 | 0.0000 / 0.0000 / 0.0000 | 0 | 0 | 100 / 13896 |
| sparse-exact-pending-provider | 100 | 0.1513 / 0.1977 / 0.1999 | 0.0006 / 0.0009 / 0.0009 | 3 | 290 | 0 / 0 |
| sparse-exact-alias-tie | 1000 | 0.9224 / 1.6278 / 1.6825 | 0.0006 / 0.0009 / 0.0010 | 3 | 276 | 0 / 0 |
| dense-exact-empty-term | 1000 | 0.5587 / 0.7794 / 0.8260 | 0.0269 / 0.0364 / 0.0373 | 1000 | 136657 | 0 / 0 |
| dense-fuzzy-usage-weighted | 1000 | 39.1913 / 42.5440 / 46.1341 | 0.0306 / 0.0356 / 0.0364 | 1000 | 136657 | 0 / 0 |
| empty-query-browse | 1000 | 0.3210 / 0.4290 / 0.6183 | 0.0000 / 0.0000 / 0.0000 | 0 | 0 | 1000 / 140657 |
| sparse-exact-pending-provider | 1000 | 0.9789 / 1.4326 / 1.7614 | 0.0006 / 0.0012 / 0.0019 | 3 | 276 | 0 / 0 |
| sparse-exact-alias-tie | 10000 | 8.9798 / 10.0162 / 10.3636 | 0.0012 / 0.0019 / 0.0020 | 3 | 276 | 0 / 0 |
| dense-exact-empty-term | 10000 | 5.6613 / 7.7991 / 9.6014 | 0.1795 / 0.3239 / 0.3484 | 10000 | 1372759 | 0 / 0 |
| dense-fuzzy-usage-weighted | 10000 | 397.9052 / 531.1326 / 636.5089 | 0.1764 / 0.2026 / 0.2099 | 10000 | 1372759 | 0 / 0 |
| empty-query-browse | 10000 | 2.8437 / 3.6820 / 5.2545 | 0.0000 / 0.0000 / 0.0000 | 0 | 0 | 10000 / 1412759 |
| sparse-exact-pending-provider | 10000 | 8.9243 / 9.2885 / 10.5820 | 0.0011 / 0.0015 / 0.0015 | 3 | 276 | 0 / 0 |

### Frozen-owner G0 to C2-A timing (fast-dev)

These are comparable owner/fixture domains at their recorded sources, not an optimization gain attributable to C2-A. Keep every row, including adverse tails.
| Owner / case | Actions | G0 p50 / p95 / max ms | C2-A p50 / p95 / max ms |
|---|---:|---|---|
| query-to-usable-CPU-frame | 100 | 2.6449 / 3.4087 / 3.4110 | 2.6725 / 3.3772 / 3.6081 |
| query-to-usable-CPU-frame | 1000 | 10.2972 / 13.2074 / 17.1924 | 9.7605 / 15.9550 / 18.2350 |
| query-to-usable-CPU-frame | 10000 | 9.2657 / 9.7041 / 11.8132 | 9.5686 / 12.0263 / 13.0745 |
| sparse-exact-alias-tie | 100 | 0.1230 / 0.1491 / 0.1501 | 0.1334 / 0.2022 / 0.2255 |
| dense-exact-empty-term | 100 | 0.1248 / 0.2610 / 0.3434 | 0.1019 / 0.2011 / 0.2214 |
| dense-fuzzy-usage-weighted | 100 | 3.9204 / 5.4625 / 6.1973 | 3.9878 / 4.4022 / 4.4444 |
| empty-query-browse | 100 | 0.0399 / 0.0735 / 0.0749 | 0.0508 / 0.0892 / 0.0928 |
| sparse-exact-pending-provider | 100 | 0.1245 / 0.1728 / 0.2725 | 0.1483 / 0.2844 / 0.2984 |
| sparse-exact-alias-tie | 1000 | 0.9550 / 1.0786 / 1.2709 | 0.9478 / 1.1923 / 1.2509 |
| dense-exact-empty-term | 1000 | 0.5510 / 0.6556 / 0.9172 | 0.6043 / 0.9442 / 1.0963 |
| dense-fuzzy-usage-weighted | 1000 | 39.4503 / 42.7475 / 45.1321 | 40.4428 / 44.3953 / 53.5078 |
| empty-query-browse | 1000 | 0.2762 / 0.5702 / 0.8117 | 0.3152 / 0.4144 / 0.4168 |
| sparse-exact-pending-provider | 1000 | 0.9707 / 1.3354 / 1.6586 | 0.9464 / 1.1493 / 1.1612 |
| sparse-exact-alias-tie | 10000 | 9.4264 / 9.7786 / 9.9183 | 9.0339 / 9.4896 / 9.5386 |
| dense-exact-empty-term | 10000 | 5.9666 / 6.7493 / 12.8722 | 6.2561 / 7.0044 / 8.0240 |
| dense-fuzzy-usage-weighted | 10000 | 397.9302 / 405.9538 / 412.7889 | 400.9361 / 412.8564 / 422.6879 |
| empty-query-browse | 10000 | 3.0210 / 3.8272 / 6.1540 | 3.0433 / 3.6730 / 3.7152 |
| sparse-exact-pending-provider | 10000 | 9.4191 / 10.8161 / 11.0793 | 8.9361 / 10.5393 / 11.3298 |
