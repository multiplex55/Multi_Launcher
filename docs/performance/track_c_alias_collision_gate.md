# Track C C1-D alias-warning cost gate

Status PASS;8/29 closed. Base8b6b8b9d487bae3154d5304f35bef099291f1019; LOCAL ONLY. Planner packet inspected; sole source writer c0_docs. Parent owns this report, ledger, collectors, execution and Git. Ordinary3/3, Small1/1 and exact-source Full1/1 PASS, independently reviewed. Narrow production query implemented and verified; source2c35a2e4f2a9a61fd81ab41e71716c2d34128b3f, optimized ordinary7/7,Small1/1,Full1/1 PASS with independent source/numerical/privacy/gate CLEAR.

## Objective and scope

Characterize complete existing alias warning semantics and accepted Add/Rename lookup cost before deciding whether a narrow under-one-lock labels query earns a production change. Only cfg(test) instrumentation/fixtures are initially authorized in note_panel.rs, with a narrowly justified cfg(test) note.rs seam if required. No production API, cache projection, persistence or error policy change yet. No action-publication source changes.

NotePanel::alias_collision_warning clones the complete alias map; only conflicting non-self candidates trigger a full note snapshot. Its only caller warn_alias_collision is invoked when Add/Rename is accepted; typing/frame/Remove do not invoke it. Warning is advisory and does not block metadata persistence. The panel owns toast/message composition; CACHE owns canonical alias/note identity. A future query would return only conflict labels under one lock; no new cached index or title-query migration.

## Invariants and oracle

Trim input and preserve its display case; Unicode lowercase lookup includes primary and secondary aliases. Exclude all exact current draft-slug entries, including changed/unsaved slug. Deduplicate candidate slugs, but preserve duplicate matching note entries and canonical source order. Display uses the conflicting note's original nonblank primary alias (trim only tests blankness), title, slug and existing punctuation. Freeze full warning strings with a test-only independent fixture reference. Compact controls: blank/self-only, primary/secondary/Unicode, ambiguity/order/duplicates, changed draft slug and exact formatting.

Existing snapshot wrappers silently return empty on lock poison. Characterize honestly without poisoning global CACHE. If a production query is later justified, approved C1-D requires fallible query errors distinguished from successful no-conflict and diagnosed while save remains advisory. Do not change this path during characterization.

## Finite measurement

Small100 unique/duplicate/ambiguous; Full same three cases at100/1k/5k. Five warmups/twenty retained per case/zero exclusions, fast-dev/PERF1, serial parent Cargo. Owned TrackCWorkspace before note environment lock, explicit inert app, cache publication guard, all owners dropped before workspace. Fixture/reference/estimates prepared outside timers; complete warning validation after every timed sample. No native/default plugin/cleanup side effects.

Attribute actual owner duration, alias-map snapshot calls/entries/selected cloned-byte estimates, full-note snapshot calls/entries/estimated bytes, filtering/formatting and returned bytes, invocation counts. Existing NoteSnapshot lock wait/hold may be reported only if isolated soundly; otherwise disclose NOT MEASURED. No O(N) test-only byte-estimate scan inside owner that manufactures latency. Hooks fixed, bounded, opt-in and clock-free disabled. Timers overlap; estimates are not allocator traffic or RAM. Distinct C1D_ALIAS_RESULT protocol, static/hash/numeric metadata only, exact source/profile/fixture and20-value arrays.

## Verification and decision

Writer returns exact tests/symbols/protocol/timer definitions and diff without running Cargo or committing. Parent/reviewer inspect source/method and owned side effects. Run compact new correctness test and only directly relevant existing alias tests; Small validates protocol then reviewed source commit precedes exact-source Full. No broad note module/native/manual matrix. Parent owns parser and numeric/privacy review.

If action latency is material, obtain bounded implementation handoff for fallible one-lock labels query in note.rs and warning caller in note_panel.rs; preserve output/advisory persistence and add deterministic coherent-generation/error tests. If cost is infrequent/immaterial, close reviewed measured SKIP retaining existing production. C1-D is not complete until actual characterization and review support that decision. C1-E read-only packet exists but implementation remains behind C1-D.
## Independent methodology review

CLEAR for implementation/source review with narrow definitions: owner timing is alias_collision_warning lookup/message construction, excluding persistence/toast/render. Add calls only for nonempty accepted input; accepted Rename can pass blank input, which exits before snapshots. Unique means no other conflicting slug and zero full-note snapshot; conflict branches must exercise one/multiple other slugs while retaining duplicate note entries. Existing PERF1 NoteSnapshot hold includes the existing O(N) byte-estimate traversal plus cloning, not raw clone time. New byte estimates must be precomputed for the controlled canonical generation, with constant-time actual call/entry receipts. Deterministic oracle parity does not prove the current two independent snapshots coherent. Historical lock failure after successful map read can produce an empty-label warning, not universally None; no global poison test. These requirements forwarded to sole writer before execution.

Parent ignored c1-d/run.ps1 syntax PASS, adapted exact-source/Small-only-uncommitted guards from accepted C1-C runner; no execution. All Cargo terminal. Final emitter protocol/source review and exact test selection pending.
Parent pre-execution read-only audit selected existing regressions: plugins::note::tests::note_alias_resolution_is_case_insensitive_and_supports_all_aliases builds only local NoteCache from synthetic Note values. duplicate_alias_resolution_is_ambiguous uses test_plugin_with_notes with a local Arc/Mutex cache, empty in-memory TEMPLATE_CACHE and watcher=None; plugin.search("note open Shared") only returns Action values, never activates actions or loads files. These two exact tests plus the new compact warning oracle are the intended ordinary selection, pending final source review. The broader open_unsaved_panel transition test is unnecessary for this test-only characterization and is not selected. No tests executed by this audit.
## Draft reporting protocol

Writer confirmed parent static schema1/stage alias_collision_characterization/owner NotePanel_alias_collision_warning with source/profile/mode/case/count, fixture and warning scalar hashes,5warm/20retained/zero, and explicit lookup/message/invocation/selected-byte/legacy-error scopes.14arrays: owner_ns, lookup_calls, format_calls, format_ns, alias_map_snapshot_calls, alias_map_snapshot_key_entries, alias_map_snapshot_slug_entries, alias_map_selected_clone_bytes, note_snapshot_calls, note_snapshot_entries, note_snapshot_estimated_clone_bytes, note_snapshot_lock_wait_ns, note_snapshot_lock_hold_ns, warning_output_bytes. Full warning equality is checked every sample outside owner timing; scalar signature encodes complete Option<String>, not just output length. No warning text/alias/path in report.

Parent ignored collector syntax PASS; explicitly synthetic three-record positive+four negative checks PASS (missing hash,extra lookup,unique branch full snapshot,unknown payload). Synthetic outputs marked is_synthetic/purpose and never measurement evidence. Collector strict metadata/known fields/source/counts/20UInt64 values, constant controlled work, branch receipts and nested timer bounds, nearest-rank four timing distributions. Independent draft protocol review dispatched; final source alignment and actual logs remain pending. No C1-D Cargo/tests run.
Independent draft collector review found one concrete gap: stable impossible alias-map zero/count receipts could pass. Parent added known nonempty-fixture checks key entries>0, slug entries>=keys and selected logical bytes>0. Fresh explicitly synthetic v2 positive+six negative cases PASS, including all-zero map and slug-count-less-than-key-count corruption. Originals retained ignored; final emitter alignment/source/actual measurements still pending. No source or performance conclusion follows from parser checks.
## Final source review — bounded corrections assigned

Parent and independent reviewer found the same two required fixes before any Cargo: initial benchmark bodies were tiny one-paragraph notes, underrepresenting the accepted multi-KB Track C corpus (~25MB at5k); retain exact3/9 matrix and reuse accepted seeded body shape while changing only alias/identity fields. Compact correctness fixture can stay short. Second, alias-map wrapper computed an unconditional key_entries Option used only under cfg(test), introducing production receipt work/unused binding; preserve original clone/empty-on-poison behavior with cfg(test)-only observation. Sole writer assigned both; no execution authorized until focused delta rereview.

Independent stable boundaries otherwise CLEAR: WORKLOAD then NOTES lock, inert app before publication, all owners before workspace drop; no native/persistence/default discovery in owner. Full independent Option<String> and ordered Unicode/duplicate/changed-slug receipts; matching canonical generation before/after each sample. Actual map calls/key counts observed, exact-equal-map slug/selected-byte estimates outside timing. Existing NoteSnapshot reset/collection outside owner, enabled hold includes estimate scan+clone. Format timer after snapshot includes filter/materialization/cloned-note disposal/final message. Disabled hooks clock/alloc-free, counters bounded. Final14-array/static emitter matches corrected collector. No larger semantic matrix needed; only assigned deltas await review.
Assigned deltas applied and writer paused: benchmark now starts with accepted track_c_workloads::note_fixture(count).values, retains seeded multi-KB bodies, changes only case alias/slug metadata. Canonical normalization additionally incorporates aliases embedded in those bodies; expected alias map/byte estimates are derived from the captured canonical fixture, not raw override-only fields. Alias snapshot wrapper preserves original Result clone/empty-on-poison and computes observer key cardinality only under cfg(test). Parent focused inspection and diff whitespace check PASS; rustfmt two files PASS per writer. Independent focused delta review pending, no C1-D execution yet.
Focused independent delta review CLEAR for ordinary then Small. Canonical normalization appends seeded-body aliases while retaining explicit primary metadata, namespace-specific queries avoid filler collisions; Unique current-only, Duplicate two rows/one slug, Ambiguous two distinct slugs. Source/protocol boundaries CLEAR; no production optimization authorized.

Parent session46924 is the sole active Cargo: fast-dev ordinary exact selection alias_collision_warning_matches_frozen_reference_for_semantic_edges, note_alias_resolution_is_case_insensitive_and_supports_all_aliases, duplicate_alias_resolution_is_ambiguous. Raw ignored c1-d/nextest-ordinary.log; source UNCOMMITTED_SOURCE/base8b6b8b9d487bae3154d5304f35bef099291f1019. All source writers paused. Compilation started; no result accepted yet. After terminal PASS only, run Small ignored owner, collect/review, source commit then exact-source Full.7/29 closed; C1-D IN_PROGRESS.
## Ordinary and Small actual execution

Ordinary session46924 terminal exit0; run3815ed64-c2c6-419d-aa31-eba569e03a5f, fast-dev build2m06s/execution0.181s,3/3PASS,5170skipped. New compact warning oracle plus two audited pure alias-resolution tests genuinely executed. Only three pre-existing render-test warnings; no new warning. No source changes after execution.

Small terminal exit0; run0217ed5f-9dc6-4a2c-a2d8-fb37eda22726, build0.89s/test4.562s/execution4.579s,1/1PASS,5172skipped. Three cases/60 retained samples. Final-field collector accepted actual log; independent numerical/privacy review CLEAR: all42arrays×20,12 timing triplets, every scalar/raw correspondence, branch call/count/estimate receipts and nested timing bounds. Exact UNCOMMITTED_SOURCE/fast-dev/PERF1/observertrue,5warm/20retained/zero. Published smoke preserves honest uncommitted attribution and reproducible collector; no retroactive committed-source claim.

|100-note case|Owner p50/p95/max ns|Map keys/slugs|Selected map bytes|Existing snapshot estimated bytes|Warning bytes|
|---|---|---|---|---|---|
|unique(current-only)|92400/107700/213500|209/302|28740|0|0|
|duplicate(one other slug/two rows)|571500/627900/698300|212/304|29002|559180|217|
|ambiguous(two other slugs)|457000/588800/746800|212/306|29090|559160|209|

These are controlled lookup/message owner timings, not Add/Rename save/toast/frame latency or an optimization. Full nine cases/180samples and conditional production-query decision remain pending. All Cargo terminal; writers paused;7/29 closed. Parent preparing test-only source commit.
Test-only source/collector/reviewed smoke committed0a88c34dddf53f8609adf9dbab65b8a6ec158b1a; clean immediately after commit. Parent sole Cargo session85790 now runs exact-source Full ignored owner, run9771be86-3dc5-4f19-913e-607a1a999c04, build0.86s, raw c1-d/nextest-full.log. Nine cases/180 retained samples expected. All source writers paused; no production query authorized. Preserve session until terminal; collect with committed track_c_collect_alias_collision.ps1, ExpectedSource0a88c34dddf53f8609adf9dbab65b8a6ec158b1a/ExpectedModeFull/freshoutput, then independent numeric/privacy and cost gate. C1-D IN_PROGRESS;7/29closed.
## Exact-source Full baseline (numeric/gate review pending)

Session85790 terminal exit0; run9771be86-3dc5-4f19-913e-607a1a999c04, source0a88c34dddf53f8609adf9dbab65b8a6ec158b1a, fast-dev build0.86s/test118.799s/execution118.813s,1/1PASS,5172skipped. Committed collector accepted nine cases/180 retained samples/126arrays/36timing triplets into ignored full-candidate.json. No source changes during/after run. All Cargo terminal; writers paused. Actual numerical/privacy and conditional gate review pending.

|Count/case|Owner p50/p95/max ms|Selected map bytes|Existing full snapshot estimated bytes|
|---|---|---|---|
|100 unique|0.0950/0.1641/0.2043|28740|0|
|100 duplicate|0.5794/0.6691/0.7130|29002|559180|
|100 ambiguous|0.4645/0.5968/0.6074|29090|559160|
|1000 unique|0.8759/1.0737/1.2867|282990|0|
|1000 duplicate|4.6168/5.2103/6.4112|283252|5707485|
|1000 ambiguous|4.5559/5.6104/6.1441|283340|5707465|
|5000 unique|6.2566/6.9821/7.3732|1412374|0|
|5000 duplicate|26.4706/31.4618/31.8468|1412636|28593807|
|5000 ambiguous|26.1068/26.7714/28.1801|1412724|28593787|

5k conflict NoteSnapshot hold median13.54–13.56ms includes existing enabled O(N) estimate scan and cloning; format median5.99–6.13ms includes cloned-note filtering/materialization/disposal and final message. Do not sum independent quantiles to infer omitted phase cost. Unique still clones the entire map but takes no full-note snapshot. Invocation remains accepted Add/Rename only; no typing/frame/native/save latency claim. Selected logical estimates are not allocator traffic or process RAM.

Parent sees a potentially justified narrow under-one-lock labels query: about30MB selected copying per conflict and26ms direct cost at5k, without any proposed retained index. Independent reviewer/planner assess gate and precise handoff before any production authorization. Full source output parity continues exact;7/29closed, C1-D IN_PROGRESS.
## Reviewed baseline and production GO

Independent exact-source Full numerical/privacy review CLEAR: every scalar, all126 arrays and36 nearest-rank timing triplets match raw output; controlled branch receipts, positive estimates, output bytes and per-sample timer containment are valid. Small/Full100 fixture/warning identities and controlled work match. Published track_c_c1d_baseline.json preserves source0a88c34dddf53f8609adf9dbab65b8a6ec158b1a and baseline-only attribution.

Parent, planner and reviewer agree GO for one bounded production milestone: fallible conflict-label query in note.rs under one CACHE lock, canonical source order/duplicate entries/exact display preserved, captured revision receipt, no retained index or full-map/full-note copies. About26ms and30MB selected temporary copied payload at5k is material despite accepted-action-only frequency; this differs from the retained projection rejected in C1-C. No measured production gain yet.

Sole writer c0_docs authorized for note.rs/note_panel.rs at exact0a88c34 HEAD. GUI diagnoses Err explicitly while accepted Add/Rename mutation/save remain advisory. Coherent-publication channels and scoped one-shot failure tests required; global cache poisoning forbidden. Frozen baseline oracle/protocol remain unchanged, optimized benchmark gets distinct companion schema/collector and same3/9 corpus. Parent owns docs/collectors/Cargo/Git. Production source review, exact tests, Small, source commit and matched Full remain required; C1-D IN_PROGRESS,7/29closed, LOCAL ONLY.

## Optimized query companion collector

Parent added track_c_collect_alias_query.ps1, distinct stage single_lock_conflict_labels,24arrays, strict static/hash/numeric-only schema and Small3/Full9 inventory. Fifteen query/message/work/revision arrays plus nine actual legacy snapshot zero controls. Independent draft collector review found candidate cardinality assumption: Duplicate has one canonical deduplicated slug and two source-row labels. Corrected expected candidate visits to unique1/duplicate1/ambiguous2; writer confirmed. Remaining parser review CLEAR pending final source/emitter alignment. Ignored explicitly synthetic positive3records+eight negative checks PASS; fabricated data are never measurement evidence. Query lock wait+hold contained by query time and query+message contained by owner; controlled work constant, revisions may vary across fresh publication fixtures. No production tests or benchmarks executed yet.

Parent in-progress source observations: new query is crate-internal, formatter private in note owner; GUI warning composition remains separate. Query actual revision is asserted against pre/post fixture revision; label bytes sum individual labels before separators and output bytes cover full warning. Zero controls use existing alias observer and NoteSnapshot counters, derived alias payload estimates multiplied by actual observed calls. Added static candidate_scope=map_slug_entries_including_current and note_scope=ordered_cache_rows_scanned to emitter/collector; fresh explicitly synthetic v2 positive+eight negative checks PASS, originals retained. Advisory test audit uses inert app and owned temporary note files; injected one-shot lookup errors and Add/Rename save paths do not activate native/default plugins. Stable source review and actual Cargo still pending.

## Stable production source review

Writer paused after two-file production/query/test/emitter migration; rustfmt PASS, no Cargo. Independent bounded review found no production/protocol/ownership defect. Parent found one new owner-unit fixture error, confirmed by reviewer: parsed comma-separated metadata creates separate Éclair and 東京 aliases, so querying the combined literal cannot match. Sole writer assigned only the query-literal correction to ÉCLAIR, preserving Unicode casefold and two-candidate expectations; production parser/frozen GUI oracle unchanged. Review otherwise CLEAR: coherent one-lock old/new generation reads, exact duplicate/order/display, advisory explicit errors and unchanged Add/Rename save statements, no snapshots/body clones/retained state, scoped nonpanicking race hook/release guard, owned failure-test I/O, clock-free disabled hooks and actual24array zero/work receipts. Exact ordinary seven tests sufficient; no broader publication matrix requested. Actual ordinary/Small pending after focused correction verification.

Focused correction verified: new plugin Unicode query is ÉCLAIR, unchanged parser/expected candidate count; rustfmt --check two files PASS and diff whitespace PASS. Independent conditional source review now CLEAR. Parent sole Cargo session18146 running exact seven ordinary fast-dev tests via ignored query/run.ps1, source UNCOMMITTED_SOURCE/base0a88. Raw query/nextest-ordinary.log. All writers paused; compilation started, no test result accepted yet.

Ordinary first attempt session18146 terminal exit1 before tests: compilerE0689 at note_panel.rs expected_labels_returned.saturating_sub, missing integer type on test-only benchmark binding. No ordinary tests executed; raw log preserved. Sole writer assigned explicit u64 annotation matching query label counter; no production semantic change. Existing render unused-variable warning also emitted. All Cargo terminal until writer correction; rerun same seven with fresh log only.

Explicit u64 benchmark annotation verified, rustfmtcheck PASS. Parent sole Cargo retry session5524 running same seven exact tests, fresh query/nextest-ordinary-retry.log, same UNCOMMITTED_SOURCE/base0a88. Writers paused; compilation pending. Original failed log retained.

## Optimized ordinary and Small execution

Ordinary retry session5524 terminal exit0, run d794f7f5-179f-4a2b-8511-678470aae5ed, fast-dev build2m00s/execution0.330s,7/7PASS,5169skipped. Exact semantic oracle, query owner parity, coherent old/new publication, injected-error Add/Rename persistence, two alias resolution regressions and existing versioned snapshot publication test genuinely executed. Only three existing render-test warnings. Original compiler-failed attempt retained; explicit u64 benchmark binding fixed before retry. No source changes after retry.

Small terminal exit0, run2e146fe7-868c-4178-8099-b9427d4da91c, build0.87s/test4.551s/execution4.566s,1/1PASS,5175skipped. Three100-note cases/60samples collected with companion parser. Owner p50/p95/max ns: unique8800/11100/11900; duplicate42000/46100/60300; ambiguous43600/53400/58300. Actual canonical candidate visits1/1/2, note visits0/100/100, returned labels0/2/2, label bytes0/163/158, outputbytes0/217/209. All nine legacy snapshot controls zero. This remains uncommitted-source smoke, not exact-source Full or user-visible timing. Independent raw/numeric/privacy/hash continuity review pending; all Cargo terminal, writer paused. C1-D IN_PROGRESS,7/29closed.

Independent optimized Small review CLEAR: every scalar/all72arrays×20/15nearest-rank triplets/raw correspondence/privacy and branch/timer receipts verified. Baseline100 fixture/warning hashes and output sizes exact; nine legacy snapshot controls zero. Unicode/type corrections focused-verified, no behavior change. Published track_c_c1d_query_small.json with honest REVIEWED_UNCOMMITTED_QUERY_SMOKE attribution. Parent comparison confirms track_c1d_notes/eager_warning/fixture_signature/output_signature source functions unchanged from0a88. Cumulative diff inspected, no obsolete warning snapshot/GUI formatter path. Source/collector/baseline/smoke/docs ready for coherent local production commit; exact-source Full remains required before C1-D closure.

Production source/collector/baseline/reviewed smoke committed2c35a2e4f2a9a61fd81ab41e71716c2d34128b3f; working tree clean immediately after commit. Parent sole Cargo session15043 running exact-source Full, run25ac9eb7-e254-4615-aa70-2a7ded3b01dc, build0.87s, nine cases/180retained expected, raw query/nextest-full.log. Writers paused; no Full result accepted yet. C1-D remains IN_PROGRESS,7/29closed.

## Optimized exact-source Full (review pending)

Session15043 terminal exit0; run25ac9eb7-e254-4615-aa70-2a7ded3b01dc, source2c35a2e4f2a9a61fd81ab41e71716c2d34128b3f, fast-dev build0.87s/test115.807s/execution115.820s,1/1PASS,5175skipped. Companion collector accepted9cases180samples/216arrays/45timing triplets. No source change during/after run; all Cargo terminal, writers paused. Raw query/nextest-full.log; ignored full-candidate.json. Independent numerical/privacy/identity and final performance gate pending.

|Count/case|Baseline owner p50/p95/max ms|Query owner p50/p95/max ms|
|---|---|---|
|100 unique|0.0950/0.1641/0.2043|0.0081/0.0115/0.0142|
|100 duplicate|0.5794/0.6691/0.7130|0.0421/0.0488/0.0491|
|100 ambiguous|0.4645/0.5968/0.6074|0.0430/0.0559/0.0671|
|1000 unique|0.8759/1.0737/1.2867|0.0125/0.0175/0.0242|
|1000 duplicate|4.6168/5.2103/6.4112|0.2824/0.3166/0.4835|
|1000 ambiguous|4.5559/5.6104/6.1441|0.2835/0.3996/0.4153|
|5000 unique|6.2566/6.9821/7.3732|0.0184/0.0204/0.0218|
|5000 duplicate|26.4706/31.4618/31.8468|1.4829/1.5952/1.6159|
|5000 ambiguous|26.1068/26.7714/28.1801|1.4709/1.6128/1.7966|

Owner boundaries are comparable lookup/message work on matched fixtures/profiles. Phase subdivisions differ: baseline format includes cloned-note disposal; new query includes under-lock candidate/ordered-note lookup and label construction, followed by GUI join/message. Do not compare those subdivision timers as equivalent. Query returns labels without retaining an index; all old map/full-note snapshot call/entry/selected-byte/wait/hold controls zero. Selected logical bytes avoided are not allocator or RAM measurements. These are accepted-action owner results, not persistence/toast/frame/native/application-wide latency.

## Final acceptance

C1-D PASS,8/29closed. Independent exact-source Full review CLEAR/PASS: every scalar/all216arrays×20/45nearest-rank triplets/raw correspondence and all9 baseline fixture/warning hashes/output sizes verified. Actual successful one-query/revision/candidate/note/label receipts, all nine zero snapshot controls and timing containment valid. Every comparable owner p50/p95/max improves across all9scenarios. Parent accepts narrow coherent one-lock query: exact output/advisory save semantics, explicit failure diagnosis, no full-map/full-note copies, no retained state/publication overhead. Published track_c_c1d_evidence.json preserves all arrays and baseline comparisons. No further tests needed for closure; native/display/allocator/RAM remain NOT MEASURED. Next checkpoint C1-E action publication characterization, no action optimization preauthorized. LOCAL ONLY.
