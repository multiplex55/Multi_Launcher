# Track C C2-B delayed search materialization gate

IN_PROGRESS;12/29closed, LOCAL ONLY. Base closured3aecb92faab1ef883cf2cf99ac1b632e028e8fb; accepted eager runtime/search characterization57f384f63de1635525b56e757134e15c76c221dd. Planner c0_recon packet and plugin-heavy addendum saved in target/performance/track-c/next-packets/c2-b-prospective.md. Parent owns documents, collectors, Cargo/evidence/Git; sole implementer owns src/gui/search.rs. No source writes overlap. This is one existing C2-B checkpoint, with baseline harness before the candidate; C2-C and geometry are outside scope.

## Objective and ownership

Catalog filtering/scoring returns absolute catalog indices and scores; shared search_read_only_outcome_from_scored_plugins uses small typed origins for catalog indices versus separately stored owned provider outputs, applies exact-ID usage and unchanged stable descending partial_cmp.unwrap_or(Equal), then copies catalog Actions once in final order and moves provider Actions. No Action in the sort origin enum. Existing complete Vec<Action>/LauncherSearchOutcome APIs, full args, plugin/fallback/pending/revision/catalog/version/deferral and mutable/read-only/root-generation semantics remain unchanged. search_actions and apply_usage_weight each have one production caller. Remove the obsolete production early-owned catalog-hit path when migrated.

The eager implementation already clones each of H matching catalog Actions only once, then moves them. Candidate earlyH→0/final0→H/totalHunchanged. Hypothesis: smaller sort records and shorter copied-body ownership before output. C2-A Full10k denseexact owner5.6613ms/sort.1795ms; densefuzzy397.9052ms/sort.1764ms: sorting is not dominant in these fixed workloads. Smaller records alone do not establish material benefit or allocator savings.

## Bounded order and gates

1. Test-only baseline harness first, no runtime ranking change: a distinct staged attribution protocol with observed final-copy capability and exact identity/payload/work checks. Preserve C2-A collector/artifacts and both eager oracle bodies. Reuse five existing fixture domains; add only one plugin-heavy fixed case (1kcatalog+128captured owned provider Actions, dense query/controlled ties and usage, argsNone/empty/value). Capture provider output once; no provider invocation inside snapshot owner. No extra matrix.
2. Agree exact schema/field inventory with parent before emitter; baseline/candidate must share stage/scopes/sample/timing boundaries. Preserve early-copy metrics; report actual final catalog copy count/UTF8/String payload and total H explicitly. Final provider ownership moves must not be mislabeled catalog clones; empty browse copy work stays separate. Report actual sort-record size without allocation claims. No per-item clocks/TLS or extra timed scan; inactive observer avoids payload/clocks. Observer counters remain bounded, test-only.
3. Exact owned plugin-heavy ordinary oracle and Small stage smoke/source review; local test-harness commit with honest descendant SHA (runtime remains eager57f384f, but build source is its actual new commit). Run exact-source Full staged baseline before runtime refactor. Repeating the same owner matrix is needed to compare the changed attribution protocol and the added provider-heavy case fairly; no broad suite or redundant original-owner baseline campaign.
4. Only then authorize production candidate: typed origins/borrowed scoring/final materialization. Keep track_c_eager_search_outcome and track_c2a_eager_search_outcome immutable. Migrate actual-work validators and obsolete C2-A test harness/emitter where necessary; historical reproduction remains at57f384f and frozen artifacts, not a knowingly failing ignored benchmark. New protocol/stage explicitly documents phase changes. Legacy TrackC SearchScoreAndCloneHits.action_clones0 and SearchMoveResults final materialization including Hcopies must be honest; TrackA meanings untouched.
5. Exact17 accepted search/CPUframe semantic cases plus directly needed new provider-heavy oracle; candidate Small staged owner plus two original owners, review→local candidate commit→exact-source Full same owners and staged baseline comparison. Fivewarmups/20retained/0excluded, serialfast-dev. Candidate staged inventory Small6records120samples / Full16records320samples; original Small5search+1root120samples / Full15search+3root360samples. UNCOMMITTED_SOURCE onlySmall. No fullrepository/release/native campaign.
6. Keep only after full ordered outcome parity, actual early-copy removal/final-copy attribution/unchangedtotalH, smaller sort records and material measured value without material sparse/dense/pending/browse/provider-heavy/root latency regressions. Retain all p50/p95/max contrary cases. Otherwise revert only owned runtime candidate non-destructively and close reviewed SKIPPED(measured gate). No benefit inferred from code shape, capacity estimate or early-clone count alone.

## Non-goals and invariants

No result cap/topK/screening/fuzzy/alias/filter/provider-scoring rewrite/no-op geometry/workers/newdependency/persistence/profiles/TrackA/B changes. Catalog before provider concatenation and stable tie order, duplicate exactIDs and full None/empty/nonemptyargs, folderSome(None) precedence, weights<=0 exactoverride and full output/state reference equality remain required. Public callers require no API migration. Provider-heavy onecase proves controlled owned-storage behavior, not general workload representativeness. Native/allocator/display remain NOTMEASURED; C0incident outcomeUNKNOWN.

Current state: test-only baseline harness in progress under the sole source writer; no C2-B measurements executed and no runtime candidate authorized. Exact17 qualified names in accepted C2-A plan/ignored corrected runner; two predicate tests are gui::search::clipboard_modify_exact_filter_tests, not tests.

## Staged baseline protocol

Distinct prefix `C2B_SEARCH_RESULT`, schema1, stage `search_materialization`, materialization `eager` initially; same snapshot owner, fixture, profile, telemetry/observer and five warmups/twenty retained/zero exclusions as C2-A. Exact scalar inventory adds materialization, provider_count and rank_record_bytes; materialization_scope is `catalog_hit_copies_once_staged_provider_owned_moves_browse_separate`. Preserve provider_scope, empty_browse_scope, payload_scope and outcome_scope. The five frozen fixtures and their hashes remain unchanged. One additional `plugin-heavy-dense` at 1k/128 is independently reconstructed and hash-bound before collection.

34 arrays: the 27 numeric C2-A arrays and two complete/structural identity arrays, plus final_local_clone_logical_bytes, final_local_clone_strings, hit_action_clone_total, hit_clone_logical_bytes_total and hit_clone_strings_total. final_local_action_clones now reads an explicit observed receipt, not an emitter literal. Totals combine actual early and final catalog-hit copies; provider ownership moves and empty browse copies remain separately reported. Rank-record size is a type property, not allocation or work savings. Current eager `(Action,f32)` size is 104 bytes on this toolchain; runtime candidate size must be inspected before extending collector acceptance. Strict collector rejects unknown/duplicate/missing fields, wrong hashes/work/payload/source/profile, incomplete matrices and unstable controlled identities. Parser-only fabricated checks are never performance evidence.
Pre-execution independent method review: c0_boundary_review CLEAR for the draft collector/runner and protocol, conditional on replacing all deliberate heavy placeholders with independently reconstructed constants and reviewing stable source/emitter before Cargo. This is not source or evidence acceptance. Timing invariant: provider-storage transformation/allocation, unmatched provider disposal and score-record destruction remain inside the same consumed-snapshot owner interval for eager and candidate; only snapshot capture is setup. Returned full-output lifetime/drop remains matched outside the owner. No candidate-specific storage prebuild or retention across the measured boundary.
Heavy fixture frozen construction: 128 ordered provider rows0..127; IDs track_c_heavy_provider:{i:03}, labels Synthetic provider heavy result {i:03}, desc Captured provider payload row {i:03} for launcher query app; args cycle None/Some-empty/Some(--provider-index {i:03}), counts43/43/42. Expected provider projection14976 logical UTF8 bytes/384 strings; full owned provider payload15816 bytes/469 strings. Catalog payload136657 bytes/3125 strings. Query app, DenseExact, exact=true, fuzzy/usageweights.25, usage8 at catalog and provider indices1/7/13; no aliases/cache and only the controlled provider enabled. Independent pure JS BigInt/Python FNV reconstruction verified heavy literal d5a1cd254a205e24; exact read-only known-folder framing reproduced frozen DenseExact1k control886970aaa9a689a4, with no private paths in committed output. Collector now hard-binds the heavy literal and totals. Parser-only checks PASS1positive/27negative; these are fabricated schema checks, not measurements. Stable source review remains pending.

Stable baseline source pre-execution review: remaining source/method CLEAR; one ordinary-test index assertion found before execution (provider_input_actions is receipt index7, index8 is provider_filter_rejections). Implementer assigned correction7=128/8=0 plus literalheavyhash assertion outsideclock. No runtime source algorithm change; both eageroracle bodies and fixture-signature function confirmed unchanged. Execution/evidence acceptance remains pending.

## Reviewed baseline Small execution

Exact ordinary3 PASS, run f8fbe451-0ebb-4324-b631-cd0b19c57c75, fast-dev build1m53s/execution.748s; new heavy + C2-A work receipt + frozen C0 complete result oracle. Staged ignored owner1 PASS, run f444f9ee-b6d6-4d62-bdc1-8ea204e5964a, build.88s/test8.779s/execution8.794s; six records120 retained samples, five warmups each/zero exclusions. Source UNCOMMITTED_SOURCE at clean-source base d3aecb92 plus reviewed test-only search.rs changes. Same three existing render-test warnings; no new warnings. Both sessions terminal; source unchanged during each run.

| Case | N/P | Owner p50/p95/max ms | Sort p50 ms |
|---|---:|---:|---:|
| sparse-exact-alias-tie | 100/1 | 0.1240/0.1726/0.2025 | 0.0006 |
| dense-exact-empty-term | 100/1 | 0.1075/0.1768/0.1848 | 0.0141 |
| dense-fuzzy-usage-weighted | 100/1 | 3.9942/4.7236/5.2458 | 0.0183 |
| empty-query-browse | 100/1 | 0.0493/0.0744/0.0981 | 0.0000 |
| sparse-exact-pending-provider | 100/1 | 0.1390/0.2175/0.2273 | 0.0006 |
| plugin-heavy-dense | 1000/128 | 0.7791/0.9048/0.9127 | 0.0723 |

Strict collector accepted all34 arrays and fixed schemas/hashes/payload/work; independent evidence review CLEAR:204 arrays/4080 values,138 scalars,12 nearest-rank timing triplets and all five C2-A shared non-timing arrays/complete and structural identities. Corrected assertion and literal heavyhash verified. Sanitized track_c_search_materialization_baseline_small.json retains REVIEWED_UNCOMMITTED_SMOKE status and UNCOMMITTED_SOURCE; it is not committed Full evidence. Eager sort-record type104bytes; earlyH/final0/totalH catalog copies, provider moves separate; no allocator/RSS/native/display measurement. No performance benefit claim, no runtime candidate authorization. Commit and exact-source Full baseline remain gated on evidence review.

Independent reviewer cleared the baseline harness commit after ordinary3/3 and Small6records120samples. Exact-source Full baseline16records320samples remains mandatory before runtime candidate authorization. This is one C2-B checkpoint, still IN_PROGRESS;12/29closedLOCALONLY.

Baseline harness committed locally as1bc5204e9107e3137830a7f9994f9484936a62c8; sourceunchangedafterreview, cleanpostcommit. Exact-source Full staged owner running (run195c3800-442e-41ff-8d52-ad345c124244) with16records320retainedsamples expected. Full results not yet accepted; no runtimecandidateauthorized.

## Accepted committed Full baseline

Source1bc5204e9107e3137830a7f9994f9484936a62c8; exact clean source during run. One staged owner PASS, run195c3800-442e-41ff-8d52-ad345c124244, fast-dev build.86s/test63.256s/execution63.274s. Sixteen records320retainedsamples; fivewarmups/twentyretained/zeroexclusions. Strict collector and independent evidence review CLEAR:544 arrays10880values,368scalars,32nearest-rank timing triplets; all15C2Asharednon-timingarrays/fixture and sixSmalloverlays unchanged. Sanitized track_c_search_materialization_baseline_full.json statusC2_B_EAGER_BASELINE_REVIEWED. Baseline accepted before separate runtime candidate handoff. Same three existing render-test warnings. No runtime candidate authorized.

| Case | N/P | Owner p50/p95/max ms | Sort p50 ms |
|---|---:|---:|---:|
| sparse-exact-alias-tie | 100/1 | 0.1228/0.1416/0.1482 | 0.0006 |
| dense-exact-empty-term | 100/1 | 0.0985/0.1299/0.1823 | 0.0124 |
| dense-fuzzy-usage-weighted | 100/1 | 3.9416/4.5223/4.8793 | 0.0150 |
| empty-query-browse | 100/1 | 0.0510/0.0715/0.1239 | 0.0000 |
| sparse-exact-pending-provider | 100/1 | 0.1577/0.1935/0.2192 | 0.0007 |
| sparse-exact-alias-tie | 1000/1 | 0.9820/1.2832/1.7191 | 0.0007 |
| dense-exact-empty-term | 1000/1 | 0.5562/0.6162/0.6359 | 0.0252 |
| dense-fuzzy-usage-weighted | 1000/1 | 39.3400/43.3123/53.4920 | 0.0284 |
| empty-query-browse | 1000/1 | 0.3121/0.7223/0.7304 | 0.0000 |
| sparse-exact-pending-provider | 1000/1 | 0.9648/1.2202/1.3895 | 0.0006 |
| sparse-exact-alias-tie | 10000/1 | 9.5660/10.1932/10.2011 | 0.0012 |
| dense-exact-empty-term | 10000/1 | 5.8547/6.5496/6.6072 | 0.1778 |
| dense-fuzzy-usage-weighted | 10000/1 | 399.1597/408.5767/454.5630 | 0.1751 |
| empty-query-browse | 10000/1 | 3.0195/3.8371/5.5074 | 0.0000 |
| sparse-exact-pending-provider | 10000/1 | 9.3387/10.2716/11.8337 | 0.0012 |
| plugin-heavy-dense | 1000/128 | 0.6982/0.9244/1.0155 | 0.0687 |

All measured tails retained, including fuzzy1kmax53.4920ms, browse1kp95.7223ms and pending10kmax11.8337ms. Baseline characterization only, no speedup claim. Eager104-byte sort-record type, earlyH/final0/totalH copies; provider and empty-browse work separate. No allocation/RSS/native/display measurements. Both benchmark sessions terminal; do not poll99143 or83121.

Full staged eager baseline accepted independently; baseline evidence commit next. Runtime candidate may be separately dispatched only after that local commit, against its exact HEAD and with the already defined source ownership/oracle/work/timing/keep-or-revert gates. C2-B remains IN_PROGRESS,12/29closedLOCALONLY.

Baseline evidence committed locally230f2996380619610305ac7a13ab214b38cca41e; cleanpostcommit. Separate runtimecandidate sourcehandoff authorized against that HEAD, solewritersearch.rs, fullgates asabove. Parentownscollector/docs/Cargo/Git. Original frozen search work-counter vector slot2 is early SearchScoreAndCloneHits.action_clones, expectedH→0 in candidate (not totalActioncopies); SearchMoveResults now owns finalHcatalogcopies, separately exposed in stagedschema. Other originalwork/identity fields and TrackA meanings remain stable. Candidate source/measurement review pending; C2-B IN_PROGRESS12/29closed.

Candidate protocol/runners pre-source method reviewCLEAR: deferredearly0/finalH/totalH withmatchingpayload, final_scored_entries_movedP(vseagerH+P), browsezerosseparate; exact18semanticfilters/serialguards/envrestore valid. Original Smalladapter byte-equivalent to previouslyreviewedC2Aadapter. Deliberatecandidate-record-size placeholder mustbe replaced onlyafteractualstabletypeinspection; no source/evidenceapprovalimplied. Runtimewriteractive; noCargo.

Candidate implementation boundary decision: original frozen owner work_counters stays fourfields (score.work_units,score.candidates_scored,score.action_clones,moved.work_units). The thirdfield becomes0truthfully; finalphase action_clones=H is asserted internally and fullyreported in stagedschema, avoiding unnecessary fifthfield/protocolshapechange. Do not claim fulloriginalworkvectorparity; compare stablefields and explicitlyaccount changedearlycopyphase. No additionalActioncopy reduction inferred.

Parent inspected candidate sorttype ScoredActionSource::{Catalog(usize),Provider(usize)} plusf32, noActionpayload. Fixed64btargetexpected24bytes; emitteruses actualsize_of, so Smallstrictcollector willconfirm orreject that expectation. Deferredcollector bound24/eager104; parser-only2positives(eager+deferred)/6newdeferrednegativechecksPASS, previous27baseline negatives retained. These are fabricatedparserchecks, not compiler size measurements or runtime evidence. Originalfourslotownerworkprotocol retained.

Stable runtimecandidate review inprogress. Pre-execution reviewer+parentfound FinalScoredEntriesMoved incorrectlystillH+P; approveddeferredscope requires actualPproviderActionmoves becauseHcatalogActions are nowcloned. Writerholdingconsolidatedfix forobserver+validator; SearchMoveResults.work_units andoriginalfourfieldshape remainfulloutputH+P. NoCargoexecutedcandidateyet; no performanceclaim.

Assigned receiptfixcompletedandindependentlyverified: FinalScoredEntriesMoved actualP, validatorP(empty0); SearchMoveResults fullH+Pwork/finalHclones andoriginalfourfields intact. Source/isolationreview CLEARforexecution, bothimmutableoracles parent+reviewerchecked unchanged. Candidate exact18semanticrunstarted atrevieweduncommittedsearch.rs/base230f2996, fast-dev (rawcandidate-semantics.log). No measuredcandidateacceptanceyet.

Candidate first semanticrun751b5bc4-5b55-4c8b-ad7f-20d3c9191068:17/18PASS, rootCPUframeoracle FAIL at render.rs6414 because fixturevalidator stillrequires earlyphasecatalogclonesH afterowner legitimatelymovedcopies tofinalphase. Build1m58/execution2.708s; rawfailurepreservedcandidate-semantics.log, all17searchsemanticspassed. AuthorizedadjacentTEST-ONLY rootvalidator migration (track_c_validate_root_query_frame): assert early0/finalH and preserve rootlocal-copyreceiptH/sevenfieldshape; no geometry/runtime/output/timingchange. Source scope nowsearch.rs plus thatsingle render.rstesthelper; solewriterunchanged. Afterfocusedreviewrerunonlyfailedrootoracle; no redundant17rerun orbroadsuite. ParentoriginalcomparisonrootworkvaluesremainH and SearchScoreearlyclonefield0explicit.

## Reviewed candidate Small execution

All18 distinct semantic cases PASS: initial run751b5bc4-5b55-4c8b-ad7f-20d3c9191068 had17PASS/1root stale-metric FAIL; after reviewed test-only root receipt migration, targeted root rerun ac21a1be-076d-455e-aa66-75d96bcdace3 PASS1/1 (build46.29s/execution.870s). Runtime search source unchanged across those runs; failure retained. Same three existing render-test warnings.

Staged1/1 PASS run06f9562f-fe81-431e-8718-db5e6b8134cf build.89s/test8.834s/execution8.850s, sixrecords120samples. Original search+root2/2 PASS runf673892c-1071-4a0c-a431-68cbd43ad70c build.85s/root11.693s/execution19.028s, sixrecords120samples. Strict collectors accepted; actualcompiler sortrecord24bytes confirmed, early0/finalH/totalH, providerPmoves; rootsevenfieldcopyreceiptH preserved. All source UNCOMMITTED_SOURCE/Small/fast-dev; committed Full pending.

| Staged case | N | Eager baseline p50/p95/max ms | Candidate p50/p95/max ms |
|---|---:|---:|---:|
| sparse-exact-alias-tie | 100 | 0.1240/0.1726/0.2025 | 0.1252/0.2157/0.2250 |
| dense-exact-empty-term | 100 | 0.1075/0.1768/0.1848 | 0.1027/0.1943/0.2496 |
| dense-fuzzy-usage-weighted | 100 | 3.9942/4.7236/5.2458 | 3.8756/3.9376/3.9616 |
| empty-query-browse | 100 | 0.0493/0.0744/0.0981 | 0.0736/0.1145/0.1204 |
| sparse-exact-pending-provider | 100 | 0.1390/0.2175/0.2273 | 0.1261/0.1637/0.3416 |
| plugin-heavy-dense | 1000 | 0.7791/0.9048/0.9127 | 0.7562/0.9782/1.0888 |

| Original owner/case | N | C2-A eager Small p50/p95/max ms | Candidate p50/p95/max ms |
|---|---:|---:|---:|
| query_event+egui_Context_run+LauncherApp::render_root_frame / app_prefix_exact_match_default_list / fresh_query_geometry_cold_warm_fonts | 100 | 2.7044/3.5360/3.7020 | 2.7236/3.3037/3.6162 |
| LauncherApp::search_read_only_outcome_with_plugin_snapshot / exact-alias-collision-usage-tied / fresh-query-sparse-provider-result | 100 | 0.1373/0.3499/0.3816 | 0.1373/0.1871/0.2045 |
| LauncherApp::search_read_only_outcome_with_plugin_snapshot / exact-empty-term-usage-weighted / fresh-query-dense-results | 100 | 0.1359/0.2121/0.2542 | 0.1273/0.1910/0.1949 |
| LauncherApp::search_read_only_outcome_with_plugin_snapshot / fuzzy-mode-usage-weighted / fresh-query-dense-results | 100 | 3.9363/4.9639/5.3691 | 3.9661/4.5680/4.7454 |
| LauncherApp::search_read_only_outcome_with_plugin_snapshot / empty-query-browse / fresh-query-complete-catalog | 100 | 0.0804/0.1040/0.1186 | 0.0523/0.0919/0.1329 |
| LauncherApp::search_read_only_outcome_with_plugin_snapshot / exact-alias-collision-provider-pending / fresh-query-pending-provider-snapshot | 100 | 0.1877/0.3402/0.3746 | 0.1382/0.2917/0.3093 |

All timing tails retained. Small results are mixed, including adverse browse/provider-heavy tails and some lower fuzzy timings; no noise attribution or optimization acceptance. Strict original work shape remains4fields with third earlyphasecopiesH→0; root7values unchanged with explicitfinalphase attribution. Parent original comparison initially used an unqualified owner literal, corrected to LauncherApp::search_read_only_outcome_with_plugin_snapshot; parser-only comparison correction, no source/Cargo rerun. Independent review pending before candidatecommit and matched Full.

Independent Small evidence review CLEARforcandidatecommit: staged204arrays138scalars12timingtriplets +original42arrays114scalars6timingtriplets(18scalarstats); allrawvalues, sixstagedbaseline and sixeagerC2Aoriginal overlays and comparisonsverified. Rootsevenvaluesunchanged/searchthirdcounteronlyH→0, actual24byte record andearly0/finalH/totalH/Pmoves confirmed. Mixedtails retained. Originalcomparison G0fields nowexplicitg0_modeFull/context_only_fixture_matched_full_distribution; docsSmalltableuses matchedC2ASmall only. Sanitizedcandidate_small bundle REVIEWED_UNCOMMITTED_CANDIDATE_SMOKE_PENDING_FULL_GATE. This clearscommit, not optimizationacceptance. ExactsourceFullsameownersrequirednext.

## Exact-source candidate Full execution

Candidate source `ac7e43db2762f44d6398e2101607938ba9866239`, fast-dev, clean source throughout both serial runs. Staged1/1 PASS runed2acd2d-d9dd-4e8f-9145-2890155a1db9 build.84s/execution64.061s; original2/2 PASS run5623373f-ad78-409d-b73e-399c6526c20b build.85s/execution104.938s. Full34records680samples, fivewarmups/twentyretained/zeroexclusions each. Strict collectors and comparisons accepted; independent review pending. No native/display/allocator measurements.

| Staged case | N | Eager p50/p95/max ms | Candidate p50/p95/max ms |
|---|---:|---:|---:|
| sparse-exact-alias-tie | 100 | 0.1228/0.1416/0.1482 | 0.1234/0.2220/0.2414 |
| dense-exact-empty-term | 100 | 0.0985/0.1299/0.1823 | 0.1146/0.1441/0.1444 |
| dense-fuzzy-usage-weighted | 100 | 3.9416/4.5223/4.8793 | 3.9046/4.3440/4.4221 |
| empty-query-browse | 100 | 0.0510/0.0715/0.1239 | 0.0598/0.0837/0.1013 |
| sparse-exact-pending-provider | 100 | 0.1577/0.1935/0.2192 | 0.1335/0.1959/0.2373 |
| sparse-exact-alias-tie | 1000 | 0.9820/1.2832/1.7191 | 0.9334/1.4133/2.0480 |
| dense-exact-empty-term | 1000 | 0.5562/0.6162/0.6359 | 0.5459/0.5979/0.7560 |
| dense-fuzzy-usage-weighted | 1000 | 39.3400/43.3123/53.4920 | 39.3576/47.7149/64.7441 |
| empty-query-browse | 1000 | 0.3121/0.7223/0.7304 | 0.2883/0.4888/0.5783 |
| sparse-exact-pending-provider | 1000 | 0.9648/1.2202/1.3895 | 0.9437/1.3726/1.4679 |
| sparse-exact-alias-tie | 10000 | 9.5660/10.1932/10.2011 | 9.2284/9.8544/10.4433 |
| dense-exact-empty-term | 10000 | 5.8547/6.5496/6.6072 | 5.3034/5.9507/6.1130 |
| dense-fuzzy-usage-weighted | 10000 | 399.1597/408.5767/454.5630 | 400.4924/440.4177/468.7663 |
| empty-query-browse | 10000 | 3.0195/3.8371/5.5074 | 2.8626/3.9897/4.5841 |
| sparse-exact-pending-provider | 10000 | 9.3387/10.2716/11.8337 | 9.1084/11.2072/11.3707 |
| plugin-heavy-dense | 1000 | 0.6982/0.9244/1.0155 | 0.7096/1.0505/1.1524 |

| Original owner settings | N | C2-A eager p50/p95/max ms | Candidate p50/p95/max ms |
|---|---:|---:|---:|
| app_prefix_exact_match_default_list | 100 | 2.6725/3.3772/3.6081 | 2.7454/3.6289/3.6590 |
| app_prefix_exact_match_default_list | 1000 | 9.7605/15.9550/18.2350 | 9.7231/15.8381/17.0509 |
| app_prefix_exact_match_default_list | 10000 | 9.5686/12.0263/13.0745 | 9.7878/17.4238/17.4612 |
| exact-alias-collision-usage-tied | 100 | 0.1334/0.2022/0.2255 | 0.1497/0.5348/0.7020 |
| exact-empty-term-usage-weighted | 100 | 0.1019/0.2011/0.2214 | 0.1370/0.3709/0.3725 |
| fuzzy-mode-usage-weighted | 100 | 3.9878/4.4022/4.4444 | 3.9402/4.3577/6.0595 |
| empty-query-browse | 100 | 0.0508/0.0892/0.0928 | 0.0472/0.0684/0.0716 |
| exact-alias-collision-provider-pending | 100 | 0.1483/0.2844/0.2984 | 0.1511/0.2899/0.3041 |
| exact-alias-collision-usage-tied | 1000 | 0.9478/1.1923/1.2509 | 1.0821/2.0040/2.1186 |
| exact-empty-term-usage-weighted | 1000 | 0.6043/0.9442/1.0963 | 0.5925/0.9811/1.2673 |
| fuzzy-mode-usage-weighted | 1000 | 40.4428/44.3953/53.5078 | 39.5299/51.6367/54.3215 |
| empty-query-browse | 1000 | 0.3152/0.4144/0.4168 | 0.3728/0.7961/0.8434 |
| exact-alias-collision-provider-pending | 1000 | 0.9464/1.1493/1.1612 | 0.9658/1.5671/2.0214 |
| exact-alias-collision-usage-tied | 10000 | 9.0339/9.4896/9.5386 | 9.2339/13.7824/15.2510 |
| exact-empty-term-usage-weighted | 10000 | 6.2561/7.0044/8.0240 | 5.4005/6.7647/6.8158 |
| fuzzy-mode-usage-weighted | 10000 | 400.9361/412.8564/422.6879 | 395.3902/529.5281/618.1502 |
| empty-query-browse | 10000 | 3.0433/3.6730/3.7152 | 2.8579/3.6863/4.1387 |
| exact-alias-collision-provider-pending | 10000 | 8.9361/10.5393/11.3298 | 9.6458/13.7168/17.5472 |

Every sort subphase and matched G0 original-owner p50/p95/max remains in track_c_search_materialization_candidate_full.json. No timing exclusions, noise attribution, size-only benefit, total-copy reduction, or application-wide claim.

Independent Full review CLEAR: staged544arrays10880values368scalars32triplets; original126arrays2520values342scalars18triplets. All34 fullorderedidentity/revision/viewport/work domains preserved with intended phasecounterchanges. Final disposition SKIPPED(measured gate): narrow denseexact10k gains do not offset materially worse fuzzy/root/sparse/pending/provider-heavy tails. Sort-record size24vs104 and deferredcopyphase are genuine properties, totalHcopies unchanged; no allocation/native claim. Restore candidate-owned search.rs and render.rs receipt migrations exactly to230f2996380619610305ac7a13ab214b38cca41e; retain test-only eager harness and all immutableevidence. Source restoration verified by exact Gitblob identity; no additional measurement round needed for gate decision.
