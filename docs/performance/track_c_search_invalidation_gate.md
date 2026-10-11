# C2-C — forced-refresh result installation gate

Status: PASS (KEEP conditional equal-result reuse). Accepted source base `c08096cd909b68a3bc1ca1f330dc7b9245141e5c`; C2-B is SKIPPED (measured gate), with the eager search source retained. Fourteen of 29 checkpoints are closed. LOCAL ONLY.

The first milestone adds test-only characterization, with no production search or geometry change. `search()` already skips valid same-query work; equal index completion already preserves search state. Exact-mode cached lowercase fields already exist. A new search index or fuzzy screening rule is not justified by current evidence: the dense fuzzy fixture matches every row, and sparse exact fixtures do not establish unnecessary fuzzy work.

The prospective opportunity is a forced provider-generation refresh that computes the same complete ordered `Vec<Action>` but reinstalls it and invalidates geometry. The actual root render path observes provider generation, invalidates search validity, and calls `search()`. Characterize that path through the existing controlled notification seam, using inert owned providers and isolated synthetic catalogs.

## Bounded baseline milestone

The sole source writer owns the test module in `src/gui/render.rs`. Parent owns this plan, protocol/collector, evidence, Cargo and Git. A tiny additional test observer requires an identified missing metric and parent approval. No runtime optimization, dependency, profile, persistence, worker, native provider or layout algorithm changes.

Use synthetic catalog sizes 100/1k/10k, one controlled provider, dense exact query, settled fonts and prior geometry. Three recipes:

- Valid cached repeat: call the existing same-query search fast path, then render; no provider invocation or geometry invalidation.
- Equal provider notification: advance controlled provider generation, then render through the production refresh path; preserve full provider output.
- Changed provider notification: change the final provider Action's description or args without changing result count, notify, then render; require replacement.

Fixture setup and the changed provider input are outside timing. Notification, actual refresh/frame and relevant disposal are inside the owner. Use five warmups and twenty retained samples with zero exclusions. Record complete output/provider/catalog state separately from result generation, rebuilds and viewport receipts. Generation retention would be an intentional candidate change, never hidden inside claimed full receipt parity. Record provider calls, geometry work/timing and complete output identity. Agree exact emitter/schema and fixture hashes before execution; strict collectors reject incomplete inventories and source/profile/identity/work mismatch. Raw logs stay ignored.

Synthetic notifications establish mechanism and conditional economics; they do not establish production refresh frequency. No claim that provider updates routinely return unchanged output without evidence.

## Conditional runtime milestone

Only after reviewed committed-source baseline evidence, consider avoiding invalidation for a computed nonempty same-query result whose entire ordered Action vector equals the installed vector. Use complete `Action::PartialEq`, including args `None` versus empty. Preserve every provider/catalog/pending/deferral/validity/suggestion/selection/layout update.

Popup cleanup is a separate required effect of current invalidation. A future candidate must preserve it through a narrow render-owned operation while retaining equal-result geometry. Existing display/style/font/DPI/width/full-path/alias invalidations remain effective. Keep empty-query, changed-query, explicit command replacement and radial restore behavior unchanged.

Accept only meaningful owner/frame savings without material changed-result regression and with complete state/popup semantics. Preserve all p50/p95/max and comparison/disposal costs. If conditional value is insufficient, revert owned runtime changes and close SKIPPED (measured gate). Do not turn this checkpoint into C2-D geometry reuse.

## Verification

Audit inert fixtures and source before running exact serial fast-dev tests. First execute the new ordinary baseline recipe contract and directly relevant existing root/search state tests. Then Small smoke, independent review, local harness commit and exact-source Full characterization. Candidate tests, if authorized later, cover equal/reordered/renamed/args-only/same-count/pending/display-invalidated outcomes and popup cleanup without a benchmark cross-product. No broad suite, release build or native campaign.

## Agreed characterization protocol

Distinct `C2C_REFRESH_RESULT`, schema1, stage `forced_refresh_invalidation`, installation `eager_reinstall`; owner `LauncherApp::search+egui_Context_run+LauncherApp::render_root_frame`. Fixture `synthetic_action_catalog_controlled_provider`, query `app_empty_term_exact`, viewport960x640, warm egui font atlas, exact app prefix/default list. Profile fast-dev, lower-case small/full, telemetry enabled, five warmups/twenty samples/zero exclusions. Notification is inside owner before frame; input mutation is setup outside owner. Geometry uses existing test observations with no equality observer.

Sixteen numeric arrays: elapsed_ns, result_count, provider_calls, provider_generation_delta, result_generation_delta, geometry_rebuilds, geometry_measured_rows, geometry_measured_cells, geometry_ns, painted_rows, search_valid, search_pending, provider_deferral, catalog_versions_stable, selected_index, suggestion_count. Separate seven-field catalog receipts retain accepted-present/accepted clipboard/todo/notes/current clipboard/todo/notes; four-field provider receipts retain before/accepted/current/render-observed generations. Three fixed16hex identity arrays retain complete semantic state, full ordered Actions, and visible viewport separately. Every array has twenty samples.

The semantic identity hashes actual query, last query, full suggestions, selection and pending/valid/deferral/catalog-stability after full assertions; revisions and geometry remain in separate receipts. Cached expected trigger/generation/rebuild deltas0, notified1; each notified frame measures all N+1 result rows. geometry_ns is zero for no rebuild rather than stale prior timing. FullOutput lifetime is matched through validation.

Fixture signature uses seed0x4352465245534831/domain track-c2c-forced-refresh-fixture, N+1, case, catalog identity, exact query/settings, provider identity/config and initial/effective provider Actions. Initial args Some-empty changes to Some-changed for the changed case. Independent constants pending stable source; draft collector intentionally refuses collection until all nine bindings are established. AST syntax passed; no parser fabricated checks or runtime measurements yet. Independent method/protocol review CLEAR, source/isolation/evidence acceptance pending.
Independent pure JS BigInt/Python fixture reconstruction CONTROL PASS, catalog1kidentity9a958b83b7e7345f. Allninefixturehashes and fullActionidentities agree; no nativemetadata/API or emitter-derived values. Collector hardbindings populated; actualstable source framing review stillrequired. Draftparser inventoryshadowbug corrected beforeexecution. Fabricatedparserchecks PASS1positive/9negative (missingcase, duplicate/unknownfield, wrongfixture/output/source, badtuple, outsidegeometryinterval, UInt64overflow); these are not runtime performance measurements. Suggestioncountbinding pendingactualstablefixturestate.

Stable source source/isolation/timing/oracle/emitterreview CLEAR after adding explicitpostclockautocomplete0 and setup/accepted/currentcatalogreceiptassertions. Exact2ordinary(newthree-recipecontract+existingrootqueryoracle) nowrunning; no benchmarkexecutedyet. Actualsuggestion_count5 andfull5strings are independentlyasserted. Complete semanticstateFNVidentities independentlyreconstructedbyJS/Python, hardboundincollector alongsideall9fixture/6ActionIDs. Finalfabricatedparserchecks1positive11negativePASS, includingwrongstate/suggestions; runtimeevidencepending. Sourceproductionunchanged(rendercfgtestadditionsonly).

Firstordinaryrun535ec1a0-aede-4326-aae9-8c86ff9b0bc7 build2m09/execution1.742s: existingrootoraclePASS, newcontractFAILsetup actualsuggestions[]expected5. Failurepreserved. Rootcausefixtureleft120mscompletiondebouncependingafterupdate_action_cache; nineheadlessframesdonotguaranteeelapsed120ms. Assignedtest-onlysetupcorrectionusingexistingrebuild_completion_index_now beforeinitialsearch, withsettled-index/dirty/scheduleassertions. No arbitrarydelay, expectationweakening, runtimechange, orhashregeneration. Afterfocusedreviewrerunonlyfailedcontract; benchmarksnotexecuted.

Pre-rerun review caught the first setup correction at the historical root helper instead of C2C; no Cargo ran with that misplacededit. Writerremoved thosefiveownedlines fromtheoldhelper and insertedthem onlyintrack_c2c_prepare_state. Parentverifiedoldhelpertext matchesbaselinec08096cd anddiffisoneC2Cadditionhunk; propercall6665/indexSome/deadlineNone/bothdirtyfalse. Focusedexecutionclearancepending, failedlogretained.

## Small baseline execution

Both distinct ordinary tests pass: first run535ec1a0 had existing root oracle PASS/new fixture FAIL; targeted corrected contract runfa0feec8-143d-4955-b38b-7dd32632bccd PASS1/1, build50.49s/execution1.909s. Failure and pre-execution misplaced-fix finding remain recorded. Source runtime and old root helper unchanged.

Small run0573cd92-a4b3-4317-9bd7-42e7ba353726 PASS1/1, build.97s/test35.333s/execution35.349s; three records/sixty samples, sourceUNCOMMITTED_SOURCE/fast-dev. Strict collector accepts full fixture/Action/state bindings, provider/catalog receipts and expected work. Independent numerical review pending.

| Case, N100 | Owner p50/p95/max ms | Geometry p50/p95/max ms | Rows rebuilt | Provider calls |
|---|---:|---:|---:|---:|
| cached_repeat | 1.1839/1.4750/1.7782 | 0/0/0 | 0 | 0 |
| equal_provider_notification | 5.7444/7.8128/7.8995 | 4.4143/6.1275/6.3428 | 101 | 1 |
| changed_provider_notification | 5.4170/6.1095/6.3745 | 4.2505/4.8604/5.0064 | 101 | 1 |

These characterize different baseline recipes; subtracting cached owner time does not isolate geometry savings. No optimization or production prevalence claim. Full committed-source baseline required before authorizing a runtime candidate. C2-C remains IN_PROGRESS,13/29closedLOCALONLY.
Independent Small numerical/source/privacy review CLEARforbaselineharnesscommit:3records63arrays1260values66scalars6nearest-ranktriplets exactraw/JSON. Allcatalog/providerreceipts, fullboundsemantic/ActionIDs, stableviewport22paintedrows, delta0vs1/geometry0vs101rows/providercalls0vs1 checked; geometrywithinownerall60samples. Sanitized baseline_small statusREVIEWED_UNCOMMITTED_EAGER_BASELINE_SMOKE. SourcecommitthenexactFull9/180required, no runtimecandidateauthorized.

## Exact-source Full eager baseline

Source `76793267b7902282456e52588986b2effa992bbf`; run436e1f10-a726-4f92-918f-e68ce04bf49d PASS1/1, build0.91s/test247.613s/execution247.629s. Nine recipes/180 retained samples, five warmups each, zero exclusions; strict collection accepted. Independent numerical/source/privacy review CLEAR:189arrays3780values198scalars18nearest-ranktriplets, all identities/state/work/receipts and three Small non-timing overlaps. No runtime candidate authorized before this baseline evidence commit.

| Case | N | Owner p50/p95/max ms | Geometry p50/p95/max ms |
|---|---:|---:|---:|
| cached_repeat | 100 | 0.8968/1.0457/1.0683 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 100 | 5.4254/6.8294/7.8781 | 4.2747/5.4406/6.3347 |
| changed_provider_notification | 100 | 5.2966/6.2063/6.3695 | 4.1892/4.9759/5.1680 |
| cached_repeat | 1000 | 0.9461/1.2278/1.2695 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 1000 | 54.1812/59.1624/64.0101 | 52.3214/57.2215/62.1682 |
| changed_provider_notification | 1000 | 55.7053/110.3065/121.4695 | 54.2169/107.7522/119.4659 |
| cached_repeat | 10000 | 0.9593/1.2887/1.5005 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 10000 | 548.5311/613.9868/629.3620 | 541.6434/606.5904/619.3513 |
| changed_provider_notification | 10000 | 547.8612/576.5696/615.9344 | 541.4550/570.0935/608.8313 |

Cached controls rebuild zero rows; both notified cases rebuild N+1 rows and invoke the provider once. Full ordered Actions, semantic state, revision receipts and eager visible geometry are checked separately. Contrary changed-N1000 p95/max110.3065/121.4695ms remains reported. These are baseline costs, with no candidate speedup or production-frequency claim.

## Authorized single candidate milestone

Baseline evidence commit `b466969fa27c15f3902b0b1dd804a019405e890f` is clean, independently reviewed and precedes this authorization. One writer owns search.rs and the small render popup facade/C2-C tests. Parent owns protocol/collector/evidence/Cargo/Git. Candidate retains only nonempty same-raw-query full ordered Action equality; all provider/catalog/pending/validity/selection/suggestion/layout updates remain. Drop incoming Actions inside installation owner, queue existing popup cleanup separately without invalidating retained geometry. Existing invalidated display/style/font/width keys remain invalid. Empty browse, changed query, cached fast path, explicit command replacement and radial restore retain existing behavior. No fuzzy screening/index or C2-D layout changes.

Scoped cfg(test) installation observer is approved for actual comparison_calls/comparison_ns/comparison_input_results, equal_reuses/discarded_incoming_actions, replacements/installed_actions and installation_ns. Input results means vector length, not actual visited comparisons or string bytes. Installation includes comparison/disposal/popup queue/invalidation; no per-item timing or extra payload traversal. Disabled observer does not time. Ranking still materializes full output as before; no allocation/process-RAM claim.

Distinct candidate prefix C2C_REUSE_RESULT, schema1/stage forced_refresh_equal_reuse/installation full_action_equal_reuse/geometry_scope scoped_installation_observer. Preserve baseline collector/artifacts. Existing21 arrays plus8observer arrays, unchanged fixture/full Action/state bindings and3recipes/9matrix5+20. Cached compare/reuse/replace0; equal notified compare1,inputN+1/reuse1/discardN+1/resultgen0/rebuild0; changed notified compare1,inputN+1/replace1/installN+1/resultgen1/rebuild1. Observer timings measured only in candidate; baseline fields are absent, not retrospectively measured zeros. Four nearest-rank triplets owner/geometry/comparison/installation. Collector requires complete inventory, actual source/profile, identities/receipts/work, comparison<=installation<=owner and disjoint geometry+installation<=owner.

Draft separate collector AST syntax PASS; fabricated parser control1positive/18negative PASS under ignored reuse-synthetic-v1. These fabricated values validate rejection behavior only, not performance evidence. Independent source/protocol review before Cargo. Ordinary tests cover full equality and each changed Action field/order/None-empty, changed/empty query, equal pending capacity state/resume, offscreen popup cleanup and prior display invalidation. Small and exact-source Full required, then independent keep/revert gate. C2-C remains open,13/29closed. Native NOT RUN; production equal-refresh frequency unmeasured. LOCAL ONLY.
Independent candidate collector/protocol review CLEAR:29arrays24numeric+2tuples+3IDs, all fixed bindings/inventory/work states/UInt/receipt/privacy gates and four timing distributions. Source review must confirm installation and geometry intervals disjoint and disposal included. Candidate source/ordinary/Small/Full evidence pending. A test-only force-eager production branch was explicitly disallowed; frozen baseline remains source767/evidence, candidate tests exercise actual runtime behavior.
Early independent installation-boundary review CLEAR: full ordered equality/same-query/nonempty gates, all state updates, timed incoming/old vector disposal and disjoint later geometry, existing popup queue delegation/no invalidated-key restoration. Nested observer sibling visibility correction identified and assigned before Cargo. Final source/tests/emitter and execution review pending; migrated fixture contract name track_c2c_oracle_forced_refresh_reuse_contract, candidate benchmark track_c2c_benchmark_equal_refresh_reuse.
Final independent source/method/fixture-safety review CLEAR for targeted execution: GUI-scoped observer API visibility corrected, no production bypass, full Action/state/eager validation and29arraymapping aligned;8comparison/8replacement semantic arithmetic and offscreenpopup assertions corrected before Cargo. Source writer idle. Parent exact9ordinary fast-dev RUNNING session79759 rawreuse-semantics.log; no candidate runtime evidence accepted yet.
Candidate ordinary runa76c22df-6750-415e-ac28-f74cebc25e49 PASS9/9,5182skipped, fast-dev build2m01/execution6.505s. Fournew/migrated C2C tests plus frozen search/rootquery, listinvalidations, gridpopup and inert equal-index completion all genuinely executed. No candidate failure; same3preexistingwarnings. Small owner RUNNING session54855/runcb45b816-2b8b-40c4-b5cd-e904da5e04ee build.89s, rawreuse-small.log; measurements pending. Allsourcewritersidle, LOCALONLY.

## Candidate Small smoke (uncommitted)

Run cb45b816-2b8b-40c4-b5cd-e904da5e04ee PASS1/1; build.89s/test33.694s/execution33.714s, three records/60samples, sourceUNCOMMITTED_SOURCE/fast-dev. Strict collection accepted; independent87arrays1740values66scalars12timingtriplets numerical/source/privacy review CLEAR for sourcecommit only.

| Case N100 | Owner p50/p95/max ms | Geometry p50/p95/max ms | Comparison p50/p95/max ms | Installation p50/p95/max ms |
|---|---:|---:|---:|---:|
| cached_repeat | 0.8969/1.4499/1.5379 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 1.0547/1.3731/1.5359 | 0.0000/0.0000/0.0000 | 0.0067/0.0090/0.0106 | 0.0215/0.0271/0.0280 |
| changed_provider_notification | 5.3594/6.4577/7.1446 | 4.1659/4.9669/5.0271 | 0.0073/0.0122/0.0152 | 0.0239/0.0333/0.0444 |

Equal notify comparisons1/input101/reuse1/discard101 with generation0/rebuild0; changed notify comparisons1/input101/replace1/install101/generation1/rebuild1; cached all0. Complete semantic output identities remain bound. Full baseline N100 owner cached0.8968/1.0457/1.0683, equal5.4254/6.8294/7.8781, changed5.2966/6.2063/6.3695ms. Candidate equal benefit is conditional; changed p95/max and cached-control tails are adverse and retained. Small alone cannot establish acceptance.

Smallreview also confirms unaffected cached/changed viewport IDs match baseline; equal retains cached viewportIDe68b38d1cdff4e1a instead of rebuilt47d1e9ff6b1551bf intentionally. Sourcecommit readinessCLEAR; exact-source Full9/180 and independentgate required, C2Cstillopen13/29closed.

## Exact-source Full candidate and gate

Candidate source `da838c6d59cc5a4172c3a2471398a673dd0188f9`; runfd82a608-b3ae-434c-9cfd-79c5aab580d9 PASS1/1, build.86s/test229.099s/execution229.131s,9records180samples,zeroexclusions, fast-dev. Strict collector accepted. Independent261arrays5220values198scalars36triplets evidence/source/privacy review CLEAR; KEEP recommended and parent accepted.

| Case | N | Baseline owner p50/p95/max ms | Candidate owner p50/p95/max ms | Candidate geometry ms | Comparison ms | Installation ms |
|---|---:|---:|---:|---:|---:|---:|
| cached_repeat | 100 | 0.8968/1.0457/1.0683 | 0.8993/1.3244/1.4427 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 100 | 5.4254/6.8294/7.8781 | 1.0624/1.5061/2.0117 | 0.0000/0.0000/0.0000 | 0.0070/0.0116/0.0152 | 0.0225/0.0411/0.0538 |
| changed_provider_notification | 100 | 5.2966/6.2063/6.3695 | 5.4312/6.1136/6.1443 | 4.1724/4.8042/4.9287 | 0.0073/0.0112/0.0158 | 0.0246/0.0412/0.1723 |
| cached_repeat | 1000 | 0.9461/1.2278/1.2695 | 0.9179/1.0817/1.2003 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 1000 | 54.1812/59.1624/64.0101 | 1.4970/1.7662/1.9410 | 0.0000/0.0000/0.0000 | 0.0559/0.0813/0.0834 | 0.1888/0.2198/0.2642 |
| changed_provider_notification | 1000 | 55.7053/110.3065/121.4695 | 53.4526/54.8293/56.7278 | 51.7845/53.2793/54.6230 | 0.0536/0.0959/0.1058 | 0.1920/0.2648/0.2896 |
| cached_repeat | 10000 | 0.9593/1.2887/1.5005 | 0.9674/1.1438/1.2367 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 | 0.0000/0.0000/0.0000 |
| equal_provider_notification | 10000 | 548.5311/613.9868/629.3620 | 6.8325/8.4798/10.8200 | 0.0000/0.0000/0.0000 | 0.6625/0.9261/1.1008 | 2.3079/2.7082/3.8020 |
| changed_provider_notification | 10000 | 547.8612/576.5696/615.9344 | 542.4872/596.9982/606.7477 | 535.4350/589.4474/599.0594 | 0.6184/0.8587/1.1495 | 2.3680/2.7176/3.0065 |

Equal notifications invoke provider1 and compare N+1 complete Actions, discard incoming N+1 and reuse installed vector; result generation/rebuild/galley rows0. Changed notifications compare N+1, install N+1, result generation/rebuild1 and measure N+1rows. Cached all observer/work0. Full semantic/Action state remains independently bound; equal viewport generation retention is intentional. Changed10k p95 rises576.5696→596.9982ms, while median/max fall; cachedN100p95/max rise1.0457/1.0683→1.3244/1.4427ms. These remain adverse observations with no invented cause.

## Acceptance decision

C2-C PASS, KEEP the candidate source `da838c6d59cc5a4172c3a2471398a673dd0188f9`. Independent reviewer and parent accept the bounded per-refresh tradeoff: equal owner median savings80.42%/97.24%/98.75% at100/1k/10k, with real generation/rebuild/galley-row work0. Complete Action/state/provider/catalog/eagergeometry semantics, queued offscreen popup cleanup, deferred provider resume and independent display invalidation pass. No added runtime cache, normalized-string index or fuzzy screening; C2-B measured SKIP remains intact.

The decision explicitly retains changed10k p95+3.54%(+20.4286ms), changed100median+2.54%, and cached100p95/max+26.65%/+35.05%(+.2787/.3744ms). Changed10kmedian/max fall0.98%/1.49%. This is an engineering acceptance of conditional savings with recorded downsides, not proof of universal nonregression or a causal noise explanation. Do not subtract phase quantiles or fabricate baseline observer timings as zero. Native/display/allocator/RSS/real equal-refresh frequency remain unmeasured.

Verification completed: candidate9ordinary/Small1/1/Full1/1 allPASS; baseline2distinctordinary/Small1/1/Full1/1 retained. Source/method/fixture/collector/privacy and numerical reviews CLEAR; final cumulative diff inspected, baseline artifacts/frozen oracles preserved, no further tests requested. All Cargo terminal and source writers idle. Fourteen of29closed; next checkpoint C2-D list geometry (prospective read-only packet only so far), then C2-Egrid in order. LOCAL ONLY.
