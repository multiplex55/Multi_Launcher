# Track C metric definitions

Track C diagnostics are a fixed-cardinality addition in `performance::track_c`.
The historical `performance::Metric` collector remains at 15 entries and keeps
its existing frame-recording behavior. Track C has 12 named phase counters and
an 8-by-9 event class/origin matrix; it stores no payloads, paths, queries, or
producer-provided labels.

Diagnostics are enabled when `MULTI_LAUNCHER_PERF` is set to `1`, `true`,
`yes`, or `on` (case-insensitive, with surrounding whitespace ignored). The
setting is cached once per process. Timers do not call the clock when disabled.
The internal event channel similarly caches this setting when created: its
normal disabled send path does not classify the event, read the clock, or
maintain observed queue depth. `EventReceiver::queued_depth()` returns `None`
for such a channel. When observation is enabled, depth counts only envelopes
owned by that app channel, separate from registry wake bookkeeping and events
pending before an app registers.

`performance::track_c::snapshot()` returns every phase in enum order with call
count, work units, elapsed nanoseconds (total and maximum), and completed,
error, and abandoned outcomes. The search scoring phase also reports separate
`candidates_scored` and `action_clones` counters; those are zero for unrelated
phases. `snapshot_events()` returns every class/origin
pair, including zero-valued pairs. Concurrent snapshots are approximate and
may straddle updates. `reset()` is intended for controlled runs while writers
are quiescent; it resets both Track C collectors and leaves historical metrics
alone.

| Phase | Timed boundary and work units |
| --- | --- |
| `note.relationship_refresh` | Heavy derived-note refresh from snapshot acquisition through the relationship recomputation; work units are three times the todo-plus-note slice lengths traversed by its three backlink passes. Snapshot failures are errors. |
| `note.mentions_scan` | The mention backlink pass only; units are todo-plus-note slice lengths traversed, including the self-note check that skips the current note. |
| `action.publish_prepare` | Clone/preparation of the currently accepted indexed tail before action publication; units are indexed actions prepared. |
| `action.publish_commit` | Assemble and publish the custom prefix plus indexed tail, then update the action cache and request query refresh; units are resulting actions. |
| `search.score_and_clone_hits` | Current action scoring path, including its interleaved cloning of matching candidates; `work_units` is the examined action-catalog length, `candidates_scored` counts entries that passed filters and ran exact/fuzzy text matching, and `action_clones` counts local Actions copied into hits. Empty-query hit copies contribute clones but no scored candidates. |
| `search.move_results` | Final conversion of scored owned entries into the result vector. This is a move/collection boundary, not clone time; `work_units` is scored entries, and the clone counter remains zero. |
| `root.geometry_cold` | A root list/grid geometry cache miss through construction; units are result rows. Cache hits are excluded. |
| `notes.geometry_cold` | A Quick Notes geometry cache miss through measurement; units are filtered note indices. Cache hits are excluded. |
| `event.enqueue_age` | One sample from successful observed enqueue until dequeue or receiver teardown; work units are one envelope. Teardown before consumption is marked abandoned. |
| `event.drain` | One complete GUI event-drain invocation; work units are events dispatched. Static class/origin handler durations are available separately in the event matrix. |
| `startup.catalog_ready` | Startup indexed catalog completion and custom-first assembly; units are actions in the complete catalog. Failure is recorded as an error. |
| `hotkey.to_first_usable_frame` | A visibility invocation/revision from show publication to the first eligible, visible ROOT frame whose captured result generation is still current. Empty browse/dashboard frames qualify. |

The hotkey phase is CPU-side GUI completion: it ends after the normal root
render path and OCR surface call. It excludes the OCR early-return path and
does not claim that native window presentation or display scanout has occurred.
It is armed while the existing visibility publication gate is held, before
other observers can consume the show revision. Duplicate, stale, hidden,
changed-result-generation, changed-provider-revision, unstable or changed
mutable-catalog-version, provider-pending, refresh/loading, or parked frames do
not complete the sample. The accepted search outcome's mutable catalog
versions are retained with its installed results and compared with current
clipboard, todo, and note versions at frame end.

The event matrix attributes envelopes using a static class derived from the
`WatchEvent` variant and a static origin assigned at the internal producer or
registry fanout. For each pair it reports successful enqueues, dequeues, failed
sends, abandoned queued envelopes, queue-age total/maximum, and reducer-handler
call count plus handler-time total/maximum. Handler timing begins immediately
after dequeue and uses an RAII guard spanning the complete reducer arm, including
error and early-continue paths. Public raw sender registration retains its
existing `std::sync::mpsc::Sender<WatchEvent>` signature; the app-owned receiver
facade returns the original plain `WatchEvent` values and keeps envelope
metadata private. Native presentation remains outside these measurements.

## C1-A attribution supplement

C1-A adds a distinct observer-enabled actual NotePanel UI series; it does not redefine frozen C0 or Track A metrics. See `track_c_notes_contract.md` for selected construction-point String estimates, overlapping timer boundaries, legacy NoteSnapshot completion semantics and test-constructor isolation correction. `track_c_c1a_evidence.json` retains all exact-source Full observations and the original-owner continuity rerun. Native display and allocator traffic remain unmeasured.

## C1-B phase scope transition

The table above describes frozen G0/C1-A traversal scopes. From accepted C1-B source `b6361dd6cb4fd7ce4ce534fda1e9c34dddaf2ee1`, `note.relationship_refresh` records actual fused traversal work N+T, replacing 3*(N+T). The existing `note.mentions_scan` phase now spans shared normalization and classification for all three relationship categories, with N+T visits including the skipped self-note. It is no longer an isolated mentions-pass timer. Do not compare its elapsed time directly to the old isolated phase. Track A and snapshot metric meanings remain unchanged; frozen artifacts and the old C1-A collector are immutable.

The named test observer records one actual fused traversal, N note visits and T todo visits. Shared needle construction and fenced projection receipts establish reduced work; selected String lengths/capacities do not measure allocator traffic, retained memory or peak RAM. Nested timer boundaries overlap. Exact-source Full and original-owner observations are retained in `track_c_c1b_evidence.json`; see `track_c_notes_fusion.md` for matched comparison limits.

## C1-C test-only projection gate

C1C_NOTE_RESULT schema1/stage prepared_projection is separate from frozen G0/C1-A/C1-B UI series. Exact-source7af1dec31f25ede1c41a5605062df4d26f9a427d fast-dev/PERFtrue/observerfalse reports18 timing arrays,15 work/revision arrays and2 output-ID arrays with5warm/20retained/zero exclusions. External owner wall totals include matched inputs/row-map installation/disposal; legacy nested timers have slightly different drop boundaries and are not matched comparisons. Local canonical publication excludes global locks/revision/I/O; publication-plus1/5 sums are paired before quantiles and do not represent actual draft/TODO transition charges. Break-even is a linear local model; nonpositive owner savings have no finite K. Unchanged panel gate is separately measured with zero heavy work. Memory is partial logical/capacity including old+new overlap, excluding HashMap buckets, encoded paths and allocator/process overhead. Raw postings include duplicates/self; fallback is still a full ordered scan. Same-sample full eager comparison precedes report hashes; external updated timestamps legitimately yield2 structural hashes. Production indexing SKIPPED; no Track A or frozen metric meaning changes. See track_c_c1c_evidence.json and track_c_notes_projection_gate.md.
## C2-A supplementary search attribution

C2A_SEARCH_RESULT schema1/eager_rank_attribution is a distinct test-only opt-in stage. Owner elapsed includes observer activation/release and shared captured-snapshot search; sort_ns is sort only. Twenty retained samples after five warmups, zero exclusions, source/profile/fixture bound. Numeric receipts distinguish actual local visits/filter passes/score attempts/hits/early copies and logical copied UTF-8/String values; provider input/filter rejections/lowercase projections/retained moves; command fallback copies; sort input/final scored moves; empty command-cache/catalog materialization; output and typed outcome. Final catalog clone zero is structural evidence of current map-and-move, not an intercepted global clone counter. Optional empty Strings count as values, not allocations. Two identity arrays separate complete state from ordered structural output. Inactive cfg(test) observer still has bounded TLS lookups/branches, whose overhead has not been isolated; production cfg(test)-absent ranking is unchanged. No historical TrackA/C0 field is redefined. See track_c_search_contract.md and strict track_c_collect_search_attribution.ps1; future C2-B must use a new stage and honest actual final-copy attribution rather than redefine eager metrics.

C2-B staged eager baseline accepted at source1bc5204e9107e3137830a7f9994f9484936a62c8, fast-dev. Distinct C2B_SEARCH_RESULT stage search_materialization:34arrays (32numeric+2identity), explicit eager/earlyH/final0/totalH, provider moves/browse separate; rank_record_bytes104 is type-size only. Small6records120samples/Full16records320samples include one fixed1k/128provider case, hashd5a1cd254a205e24, fullpayload15816/469 and projection14976/384. Independentreview and C2Asharednon-timingcontinuity CLEAR. This is baseline characterization within IN_PROGRESS C2-B, not optimization or native/allocator evidence. Fulltimings and scopes are in track_c_search_materialization_gate.md and baseline_full.json; historicalC2A/TrackA metric meanings unchanged.

## C2-D rejected incremental-list series

C2D_REUSE_RESULT schema1 is separate from eager C2D_LIST_RESULT. Candidate measures actual suffix galley calls plus commit count, formatted/compared ordered text and miss/environment/atlas reasons. Complete owner includes installation, formatting/comparison, old-value disposal and actual CPU frame. Candidate observer enabled; eager observer disabled, overhead not subtracted. Partial checkpoint String lengths/capacities and six geometry-vector capacities include simultaneous old/new overlap, counting shared full-prefix geometry once, excluding transient WidgetText/galley allocation, control blocks, allocator/RSS. Four of six required prefix p95 comparisons fail; candidate rejected, eager runtime restored. Historical baseline/C0/C2C/TrackA metrics unchanged; no accepted runtime speedup. See track_c_list_geometry_gate.md and reviewed rejected candidate_full.json.


C2-E reviewed Full baseline: source a098d685857cb2c4e7c47e90b20aad4b3c6c838d, fast-dev27recipes/540samples,1PASS841.096s owner/841.133s Nextest;5warmups20retained0exclusions. Independent459arrays9180entries486scalars135triplets540statehashes CLEAR;108Small arrays/144scalars match. Ordinary12unique and non-testlibcheck PASS. N10k/3cols fullcold owner550.2842/708.8718/929.4093ms, cell543.4424/700.7973/918.6308ms; shared20% shrink481.0998/514.6584/534.1384ms; warm1.3691/1.6541/1.6915ms. All tails/columns preserved in baseline_full.json and grid gate. Runtime eager unchanged; GO one separately gated experiment, not KEEP. C2-E IN_PROGRESS;15/29closed. Native/allocator/productionfrequency NOT MEASURED. LOCAL ONLY.


C2-E SKIPPED(measuredgate). Candidate1936c89b32545058c8f3ab069b4cf2d59b61b1ff:13ordinary/non-testlibcheckPASS; Small9/180 andFull27/540PASS728.789test728.830exe. Independent567arrays11340entries486scalars135triplets540statehashes/297baselinecommon/144Smallarrays144scalars CLEAR.31/32frozengatesPASS; warm1kp951.4944→1.6729ms (+11.9446%) exceeds5% despite20%shrink10kp50/p95481.0998/514.6584→355.2499/391.5868ms. All favorable/contrary tails retained, no waiver/noise claim. Partialold/newgeometry+presentation peakmax4,977,280B, notRSS/allocator. Exact5e/a098eager render restored SHA08AA135652DABF2534807C37BB21E163515D72DA6188E19D9F8572D69127099F; reuse identical-source priorverification, no redundantCargo. AcceptedC2C retained; no grid optimization accepted.16/29closed, nextC2F. Native/productionfrequency NOTMEASURED. See grid gate and reviewed rejected candidate_full.json. LOCALONLY.
