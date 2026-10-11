# C2-C — forced-refresh result installation gate

Status: IN_PROGRESS. Accepted source base `c08096cd909b68a3bc1ca1f330dc7b9245141e5c`; C2-B is SKIPPED (measured gate), with the eager search source retained. Thirteen of 29 checkpoints are closed. LOCAL ONLY.

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
