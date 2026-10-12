# Track C C1-F action ID projection gate

Status PASS;10/29closed. Base1ecdce51c5ef4933bc442d31033c1a9a0b7a77c1, C1-E source508bcf47a006729c064f62adb66609616ccbcaf0. LOCAL ONLY. Parent owns evidence, collectors, Cargo, Git and ledger; c0_docs sole source writer; c0_recon planner and c0_boundary_review independent reviewer. No C1-F tests or optimized measurements yet.

## Reviewed trigger and decision

C1-E Full exactsource20records400samples and complete consumers reviewed CLEAR. At10k changed/index owner medians47.7038/35.8054ms, ID construction6.7579/7.1256ms. At20k owner95.3291/73.6367ms and ID13.8316/14.2689ms. ID attempts20k, retained19,999; selected Action+key construction-copy text4,852,954/4,852,960bytes. Same-sample ID/owner13.62–15.04% custom and16.84–25.70% index. These are material attribution, not removable duration, retained memory or realized gain. KEEP contiguous Arc<Vec<Action>>, search/filter design and synchronous scheduling; GO only bounded catalog-bound ID positions. No tail reuse, segmented catalog or workers.

## Single implementation milestone

Objective: remove eager full Action bodies from the ID projection while preserving all complete consumer semantics. Domain action module owns immutable catalog-bound lookup (Arc<Vec<Action>> plus private ID positions), GUI coordinates publication. Get borrows from its own catalog; last duplicate insertion wins, no exposed positions against unrelated generation, no unsafe/self-reference or general collection compatibility wrapper.

Scope: actions domain; gui mod/search/render/watch and testobserver; dashboard context and command-history/pinned consumers; directly affected pure testfixtures only as necessary. Constructor and update_action_cache both migrate. DashboardContext public concrete field type migration documented. Build local catalog/search/filter/ID candidates, install accepted structures before callbacks, then existing one invalidation/refresh; caller-owned version, watcher no-op/error and worker generation/config acceptance unchanged. Additional Arc lease must release obsolete catalog/projection inside owner interval before callbacks. No historical retained generations, new locks/I/O/workers or persistence migration.

Invariants: full ordered Actions/args, custom-first indexed tail, ID-only pin/history current catalog lookup regardless of saved args, exact fallback/snippet precedence, current query/results/selection/rootgeneration/completion, radial validity, local durable-equal republish versus watcher-equal retention, latestprefix/staleconfig/generation rejection and last-good recovery. Existing independent eager maps remain full expected references, resolved-value assertions cannot weaken to hashes/cardinalities.

Tests: compact domain duplicate ID with distinctNone/empty/nonemptyargs and coherent retained old lookup; six reviewed C1-E consumer/transition/C0reload/originalwake/acceptedindex/equalindex tests; direct pure history/pinned consumers where affected. No broad/native/default-plugin/cleanup tests. Source writer runs no Cargo; parent audits sideeffects and runs exact serial fast-dev filters after independent source review.

Measurement: distinct C1-F optimized stage/protocol and companioncollector. Preserve C1-E artifacts/collector/schema and historicalsource. Reuse exact five cases and100/1k/10k/20k sizes,5warmups20retained0excluded, fixture/fulloutput identities. Deferred completion validation outsidetiming; scannerrequest/wait separate; captureownerobserverbefore validation; no strong fixturecataloglease or perActionclocks. Actual ID insertion/keyclone attemptsN versus retainedN-1, fullActionclones0 and key-only selectedUTF8bytes; never carry old Action+key estimate as optimized work or call text bytes allocator/RAM. Proposed46arrays: old44 removeid_selected_clone_bytes, addid_full_actions_cloned/id_keys_cloned/id_key_selected_clone_bytes; writer confirms before finalemitter/parentcollector.

Verification sequence: stable source/API/protocol handoff; independent source/isolation/method review; exactordinary andSmall+strictcollector; reviewedsourcecommit; exactsourceFull and independentnumeric/gate review. No concurrent Cargo/source changes. Historical frozen C0 owner comparisons remain separate in C1-G.

Done criteria: complete caller migration with no production owned-Action map; domain/lifetime/consumer parity; coherent atomic installation and bounded currentgeneration retention; actual fullActioncopy elimination; reviewed protocol; completeowner improvement at10k/20k for BOTHpublicationroutes and no material contrarycase regression. Report all p50/p95/max and small/noop/error observations. If marginal/worse, remove candidate via scoped changes/localcommit and close measuredskip; do not reset/discard userwork. C1-G starts only after C1-F accepted or measuredskip closure.

Writer API/protocol confirmed: actions::ActionIdLookup owns exact Arc<Vec<Action>> plus private ID positions, get/len/resolved borrowed iter. DashboardContext field and history/pinned contexts migrate; no extra-file expansion found. Parent approves46arrays, C1F_ACTIONS_RESULT/schema1/stageid_position_projection, old ID Action+key field removed, actual fullActionclones/IDkeys/key-onlyUTF8bytes added; otherstaticmetadata/fixtures unchanged.

Distinct companion track_c_collect_action_projection.ps1 and ignored c1-f/run.ps1 syntaxPASS. Explicitly synthetic positive5records+14negative checks PASS (including nonzeroActionbodyclone, wrongkeycount and missingkeybytes); all fabricated outputs remain ignored and are not measurements. Independent draftcollector/method review requested. Sourcewriter active, no C1-F Cargo/tests/measurements. Full acceptedC1-E artifact/collector untouched. Candidate acceptance remains pending.
Independent draftcollector review identified and parent corrected two definition-level checks: exactN-1retainedIDs for deliberateone-duplicatefixture, and publishingkeybytes>=actualkeycopies for nonemptyIDs. Fresh syntheticv2positive5+16negativePASS; originalsyntheticartifactsretained. Reviewer boundedcorrections CLEAR; finalsource/emitteralignment and actualC1Fdata pending.

Parent targeted test sideeffect audit: specified history_resolution_* tests are pure snapshot/resolution fixtures. Pinned refreshes_when_actions_version_changes uses disconnected DashboardDataCache but initially default PluginManager::new; sole writer instructed to migrate only that selected directlyaffected test to existing new_inert_for_test while changing lookupfixture. No broadplugin-testcleanup, native/defaultdiscovery or fileloaders authorized.
Parent early source inspection: draft ID-map guard initially included oldlookup/catalog destruction. Assigned solewriter to end construction clock after local lookup build, then install allthreeprojections and dispose obsolete lookup in completeowner residual. This preserves C1-E construction-phase meaning while explicitly recording oldcatalog disposal relocation. Fullowner comparison remains acceptance boundary; source still evolving, no C1-F Cargo.

Parent handoff review requested a construction-site cfg(test) receipt for actual ID key-clone/insertion attempts; catalog length alone is not a measured work receipt. Full Action-body copy zero is a structurally verified property of the ID builder, not interception of every Action::clone. The emitted field records that construction guarantee, while key bytes remain precomputed selected logical UTF-8 text. Independent source review also found two stale eager-oracle local references; sole writer is correcting them before any Cargo invocation. No C1-F tests or measurements have executed.

Stable source/method review CLEAR after fixing two stale independent expected-map references, actual construction-site key receipts (including IdActionsProcessed), and pre-estimate association assertions. The existing test-only observer gained one bounded counter in src/gui/track_c_action_observer.rs; no production telemetry expansion. Domain test covers six input attempts/four retained IDs and old/new catalog coherence. Exact10ordinary fast-dev serial execution started; result pending. Parent will also run a narrow non-test lib check because the constructor has cfg(test) receipt code.

Exact ordinary verification PASS10/10, Nextest8a2af4ba-ac1c-4c9a-a288-ea4a68c62d1f, fast-dev serial,5170skipped,build1m38s,execution1.199s; ignored rawordinary-1.log. Three existing render.rs testwarnings retained. No compile/test failures in C1-F. Non-test lib check nowrunning; Small and Full notrun.

Non-test cargo check --lib --profile fast-dev PASS1m07s. Small ignored benchmark PASS1/1 Nextestbf4b7442-6c21-45ff-8888-f846ec408a88,build0.89s,test7.466s,execution7.481s,5179skipped; strictcollector5records100samples. All five fixture/output identities match C1-E Small. Small owner p50/p95/maxms: changed6.3140/9.9594/10.3636; equal6.1325/11.4704/12.0811; malformed0.3906/0.4871/0.4907; missing0.3611/0.5893/0.6087; index0.3632/0.4452/0.4575. Publishing100actualkeys,99retainedIDs,6382selectedkeybytes,structuralzeroActionbodycopies. No-op/errors0keys/0selectedbytes. SourceUNCOMMITTED_SOURCE; Small is smoke only, no acceptance or speedup claim. Independent actualSmallnumericreview pending, exactsourceFull notrun.

Independent Small evidence CLEAR: every scalar/hash and230arrays exact to raw lines, all75timingtriplets recomputed;100samples pass receipts/nesting/exclusive sums, identities and privacy. Candidate source commit authorized for exact-source Full, acceptance still PENDING. Contrary Small p95/maxms preserved against C1-E Small: changed8.4233/9.5959→9.9594/10.3636; missing0.4931/0.5148→0.5893/0.6087; acceptedindex0.4316/0.4561→0.4452/0.4575. No speedup claim from Small. Source unchanged since ordinary/Small; originalG0/C1Aartifact hashes unchanged; cumulative diff reviewed, no unintended files.

Reviewed candidate source commit ccebeef374ec3f8063e264c0c5df60a827db835f, clean immediately aftercommit. ExactsourceFull started runa4af4ca5-016b-4096-89b1-0f316e60c75a/session45959, build0.87s, ignored rawc1-f/full-1.log; no Full result yet. Source frozen during run, no concurrentCargo. Candidate acceptance remains PENDING,9/29closed.

## Exact-source Full results (reviewed)

Source ccebeef374ec3f8063e264c0c5df60a827db835f; baseline508bcf47a006729c064f62adb66609616ccbcaf0. Nextest a4af4ca5-016b-4096-89b1-0f316e60c75a PASS1/1, build0.87s, test641.278s, execution641.300s,5179skipped. Strictcollector20records400samples;5warmups20retained0excluded. Everyfixture/output signature matches baseline. Runtime includes fixture/oracle work outside handler timing. AllCargo terminal, source unchanged.

Handler timings below are p50/p95/max in milliseconds.

| Case | Actions | C1-E owner | C1-F owner | Median change | C1-F ID construction |
|---|---:|---|---|---:|---|
| custom_changed | 100 | 6.3093/8.0909/10.0437 | 6.5134/7.3434/7.5148 | 3.23% | 0.0540/0.0869/0.0914 |
| custom_equal | 100 | 10.7080/11.9270/12.9692 | 11.0685/11.8410/12.0902 | 3.37% | 0.0000/0.0000/0.0000 |
| custom_malformed | 100 | 0.3901/0.5113/0.5601 | 0.3861/0.5606/0.5945 | -1.03% | 0.0000/0.0000/0.0000 |
| custom_missing | 100 | 0.3345/0.4865/0.5878 | 0.3375/0.3865/0.4280 | 0.9% | 0.0000/0.0000/0.0000 |
| index_accepted_changed | 100 | 0.3929/0.4557/0.4715 | 0.3758/0.4539/0.6624 | -4.35% | 0.0525/0.0627/0.0803 |
| custom_changed | 1000 | 4.9410/5.2145/5.3698 | 4.4555/4.8567/6.2120 | -9.83% | 0.4319/0.4664/0.4680 |
| custom_equal | 1000 | 1.5839/1.8847/2.0314 | 1.5947/1.6693/1.7894 | 0.68% | 0.0000/0.0000/0.0000 |
| custom_malformed | 1000 | 0.4703/0.5447/0.7374 | 0.4804/0.5378/0.6426 | 2.15% | 0.0000/0.0000/0.0000 |
| custom_missing | 1000 | 0.4001/0.5103/0.5641 | 0.4555/0.5277/0.5970 | 13.85% | 0.0000/0.0000/0.0000 |
| index_accepted_changed | 1000 | 3.5229/5.7634/6.6007 | 2.9813/3.4514/4.2919 | -15.37% | 0.4302/0.4821/0.6553 |
| custom_changed | 10000 | 47.7038/56.5249/62.2670 | 43.8495/56.8456/65.4500 | -8.08% | 4.8708/5.5323/6.4084 |
| custom_equal | 10000 | 14.1475/15.3865/20.0922 | 14.2150/16.8506/18.8653 | 0.48% | 0.0000/0.0000/0.0000 |
| custom_malformed | 10000 | 0.5015/0.5806/1.0075 | 0.5333/0.6286/0.7276 | 6.34% | 0.0000/0.0000/0.0000 |
| custom_missing | 10000 | 0.4242/0.5031/0.5939 | 0.4334/0.5593/0.6057 | 2.17% | 0.0000/0.0000/0.0000 |
| index_accepted_changed | 10000 | 35.8054/44.1261/48.9291 | 30.2469/31.6692/31.6753 | -15.52% | 4.7274/5.1211/5.2737 |
| custom_changed | 20000 | 95.3291/102.8852/108.1373 | 84.0773/92.6266/111.3324 | -11.8% | 9.6286/13.0668/13.7298 |
| custom_equal | 20000 | 29.2355/34.9968/44.9636 | 28.1605/29.2733/29.3077 | -3.68% | 0.0000/0.0000/0.0000 |
| custom_malformed | 20000 | 0.5260/0.8406/17.3050 | 0.5177/0.6778/0.7633 | -1.58% | 0.0000/0.0000/0.0000 |
| custom_missing | 20000 | 0.4445/0.6046/0.8943 | 0.4359/0.5782/0.6208 | -1.93% | 0.0000/0.0000/0.0000 |
| index_accepted_changed | 20000 | 73.6367/85.6836/100.9051 | 61.7502/70.9935/71.3449 | -16.14% | 9.6061/11.0454/13.1026 |

Actual key-copy/insertion attempts on publishing routes are100/1000/10000/20000, with99/999/9999/19999retainedIDs. Precomputed associated logical key-only UTF-8 bytes are6382/63082/630082/1260082. Full ID Action-body copies are structurally zero by the shared constructor; this is not a global clone interceptor. No-op/error receipts remain0keys/0keybytes with retained catalog/version. Search/filter/completion/result cardinalities and full Action values/args remain equal. No per-action clocks, extra byte scan in owner, worker or historical-generation retention introduced.

Parent recommendation: accept the bounded ID projection, subject to independent final review. Both routes improve complete-owner medians at10k/20k (8.08%/11.80%custom;15.52%/16.14%index), with20k p95 improvements9.97%/17.14%. This is owner-only evidence, not an application-wide or native-display speedup. Old catalog disposal moved from assembly into lookup replacement inside the owner; ID construction timing alone is not the acceptance boundary.

Contrary evidence remains:10kcustom p95+0.57%,max62.2670→65.4500ms;20kcustom max108.1373→111.3324ms;100index max0.4715→0.6624ms;100custom/equal medians+3.23%/+3.37%;1kmissing median0.4001→0.4555ms;10kequal p9515.3865→16.8506ms. Small contrary observations above remain. All20rows retained; causes of run-to-run variability are not proven. No universal p95/max improvement claim; unchanged no-op/error branches still execute no projection work. The material large-catalog median gains and explicit body-copy removal support the narrow hypothesis without adding concurrency risk.

Native input/display, allocator bytes/RSS, end-to-end stale-result time and deferred completion rebuild are NOT MEASURED here. Request/scanner waits stay separate. Original frozen C0Full note/action comparison remains C1-G work.

Final independent review CLEAR and ACCEPT: all920arrays/300nearest-ranktriplets/460scalarfields exact to rawFull; all20baseline and5Small identities match. All400samples satisfy work/nesting/exclusive sums/version/Arc/consumer/privacy invariants. Qualified acceptance: large-catalog complete-owner gains on bothroutes and structural body-copy removal outweigh retained mixed tails for this bounded gate. This is not proof that every latency statistic improves, nor an explanation of variability. No source findings or additional rerun required. Parent accepts same disposition. C1-F CLOSED PASS,10/29closed; nextC1G executes only after localclosurecommit.
