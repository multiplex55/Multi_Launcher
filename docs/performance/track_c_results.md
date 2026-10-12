# Track C final results — LOCAL ONLY

The accepted changes improve specific Notes, alias lookup, Action publication and equal-result notification owners. The complete final comparison is mixed: 78/105 original medians, 49/105 p95s and 44/105 maxima are higher than frozen G0. These counts are descriptive, with no significance or application-wide speedup claim. Full per-scenario keys, before/after distributions, ratios, work and identities are published in [track_c_final.json](track_c_final.json) and its hashed [support artifacts](track_c_final/owners_full.json).

## Sources and method

Original branch `Runtime-Responsiveness`, base `bcfd24d226ffb46c85f9f7ce7bdd919466191070`; local branch `runtime-foundations`. Frozen G0 is `071243db068835617a054a74fb5734a6dd9f128d`. Final measurement source is clean committed `18f8cac44f92fa18feee16b81f222953a622059a`; later closure commits change documentation only. The source manifest binds all 662 tracked Rust source files and 15 context/method files. G0 bytes and Track A/B artifacts remain unchanged.

Serial `fast-dev`, Full, Balanced power, telemetry on: five warmups, 20 retained samples, zero exclusions per scenario. Original library9/9 PASS (build1.00s/test1943.129s), startup1/1 PASS (build0.95s/test40.341s), separate equal-reuse1/1 PASS (build1.01s/test149.078s). No replay replaced a first final series. Compilation and execution are reported separately. Original inventory105 records/2100samples, 39 geometry pairs, six width arrays/120 values, 40 age rows/440 individual ages. The companion has nine records/180samples. The inventory and four family comparators pass; independent final source/raw/numerical/privacy review is CLEAR ([receipt](track_c_final/review.json)).

Quantiles use nearest-rank sorted indices9/18/19 for p50/p95/max. Cold cases are distributions over fresh controlled invalidations with warm fonts/OS cache, not a single cold sample or cold-disk startup. Every raw20array remains available. Original output/structure/revision/viewport parity is validated at each owner; Notes intentionally reduce relationship visits from `3*(N+12)` to `N+12`. Historical Actions counters represent output cardinalities, not copy receipts. Geometry fixtures suppress root DashboardAll for safety; the declared fixture difference remains explicit.

## Selected original Full comparisons

All numbers below are milliseconds, `p50/p95/max`. This table is illustrative; [all105 keyed ratios](track_c_final.json) and [raw samples](track_c_final/owners_full.json) include every neutral/adverse case.

| Owner / case | Count | G0 | Final |
|---|---:|---:|---:|
| NotePanel::maybe_refresh_heavy_derived | 100 | 22.3415/23.4536/26.5596 | 8.5973/9.6388/12.3413 |
| NotePanel::maybe_refresh_heavy_derived | 1000 | 229.0207/254.2780/385.6323 | 83.2744/93.6797/101.4922 |
| NotePanel::maybe_refresh_heavy_derived | 5000 | 1153.8690/1182.8377/1255.7501 | 423.6815/488.7143/691.4948 |
| process_watch_events_actions_reload | 20000 | 100.1619/120.6837/135.1611 | 89.0293/92.9215/93.5498 |
| process_watch_events_concurrent_offered_load | 20000 | 544.4073/599.4350/611.4708 | 523.2489/544.9608/560.3057 |
| process_watch_events_finite_burst | 20000 | 341.0464/419.0257/422.8615 | 315.4998/341.0105/357.2681 |
| startup_action_catalog | 16 | 1.6182/1.7962/2.9391 | 1.6890/2.1528/2.1632 |
| startup_action_catalog | 10000 | 860.8644/922.7792/956.3962 | 856.8371/870.2342/872.3232 |
| Root grid cold,3columns | 10000 | 525.7683/541.4192/548.0885 | 534.4693/636.2220/675.6110 |
| Query to usable CPU frame | 1000 | 10.2972/13.2074/17.1924 | 1.4485/2.1185/2.2054 |
| Query to usable CPU frame | 10000 | 9.2657/9.7041/11.8132 | 9.1895/12.4721/14.9246 |

The 5k Notes owner reduces p95 from1182.8377 to488.7143ms; its maximum remains691.4948ms. The 20k Actions owner p95 falls120.6837→92.9215ms. The 1k original query owner improves, while the 10k query p95 rises9.7041→12.4721ms. Three-column cold grid10k p95 rises541.4192→636.2220ms. Startup has no accepted optimization:16-item p95 rises1.7962→2.1528ms, whereas10k falls922.7792→870.2342ms; do not infer a startup algorithm improvement from those fluctuations. The largest original p95 ratio is1.771792: Quick Notes100, empty text, cold geometry (exact viewport/key in JSON),11.3147→20.0473ms. Warm grid6columns10k p95 rises2.2277→3.4775ms. Search exact-empty-term1k p95 rises0.6556→1.0339ms. Long adverse observations are retained without selective reruns.

## Separate equal-result reuse domain

Compare only against its frozen eager-invalidation baseline source `76793267b7902282456e52588986b2effa992bbf`. Complete Action equality for equal nonempty results and unchanged raw query preserves result generation and geometry, while provider/state updates still occur. It neither screens results nor changes ranking. All nine elapsed comparisons are in JSON. Equal notification p95 at100/1k/10k is6.8294→1.2326,59.1624→2.0785,613.9868→10.1354ms; geometry rebuilds becomezero. Changed notification still rebuilds:100 p95 rises6.2063→6.4197ms and10k576.5696→579.0713ms. Cached100 p95 rises1.0457→1.2607ms. Baseline has no comparison/installation observer, so those new clocks remain unpaired; no fabricated zeros or subtraction of quantiles.

## Accepted architecture and measured skips

Accepted: one traversal creates all three note relationship vectors; one-lock alias warning lookup avoids full-map/full-note snapshots; catalog-bound ID positions preserve last duplicate winner without ID Action-body clones; complete equal-result reuse retains geometry only under the reviewed guard. Bounded opt-in telemetry, deterministic owners and the private shared startup readiness owner provide observation/testing. Startup registration readiness, queue FIFO/scheduling, eager search materialization and eager list/grid/Quick Notes geometry remain the accepted algorithms.

Eight measured skips remain closed: C1-C relationship projection (first publication/retention tradeoff); C2-B materialization (required copies unchanged, mixed tails); C2-D list reuse (four of six fixed prefix p95 gates fail); C2-E grid reuse (warm1k gate fails); C2-F Quick Notes reuse (nine of32 gates fail); C3-B startup (required wait dominates); C3-D coalescing (no proven semantic transport whitelist); C3-E drain budget (sustained-starvation trigger unestablished). Favorable rejected results remain in their phase artifacts. A skip does not prove optimality.

## Copy estimates and historical domains

C1-D historical5k duplicate warning lookup26.4706/31.4618/31.8468→1.4829/1.5952/1.6159ms is a lookup clock, not Save/native latency. Selected old map payload1,412,636bytes plus existing note snapshot estimate28,593,807bytes are avoided by the canonical label query. See [accepted evidence](track_c_c1d_evidence.json).

C1-F historical20k ID projection selected Action-plus-key text4,852,954/4,852,960bytes becomes key-only1,260,082bytes,20k insertion attempts/19,999retained IDs and structurallyzero ID Action-body copies. See [evidence](track_c_c1f_evidence.json). These are logical selected estimates, not allocator traffic, global clone counts or RSS. Rejected C1-C retained-capacity estimate30.68MB/replacement61.36MB is not an accepted cache. No retained standalone relationship or layout projection was added.

[C2G causal query](track_c_query_observation/full_v2.json), [C3A component](track_c_startup/component_full.json) and [C3C actual event frames](track_c_events/full_frames.json) retain their original source SHAs and reviewed source continuity. They are historical companion measurements, not final-SHA executions. C2G preserves intent/acceptance/paint correlation; C3A includes readiness and release receipts with overlapping scan/wait clocks; C3C retains finite/bounded load and long frames (finite/producer p95346.4083/686.1509ms), with zero observed producer progress during the reducer and no sustained-starvation proof. Never add their nested/overlapping clocks to original owners or infer an app-wide response percentage.

## Native and verification limits

Native NOT RUN; human-perceived first-open, OS input-to-display, GPU composition, cold-disk/process startup and allocator/RSS are NOT MEASURED. Browser-only controls cannot provide native observation. Production root DashboardAll includes recycle querying even with dashboard disabled, so launch is HOLD under the campaign no-probe restriction. [Native acceptance](track_c_native_acceptance.json) records future separately authorized prerequisites. The C0-B unexpected cleanup attempt outcome remains UNKNOWN; no subsequent probe/recovery/cleanup was attempted.

C3F actually executed12 focused race/shutdown tests; C4A actually executed6 integrations plus production lib/bin check. Their exact sources, corrected failures, lifecycle evidence and historical reuse are in [C3F](track_c_startup_event_verification.json) and [C4A](track_c_cross_surface_verification.json). No full suite, release build or native campaign was substituted. Persisted Action/Note/settings formats, plugin precedence, full args, result order, Track A behaviors, profiles and dependencies are preserved.

## Reproduction

At clean measurement commit18f8cac44f92fa18feee16b81f222953a622059a in a separately selected checkout, use [serial runner](track_c_run_final.ps1) stages OriginalLib, Startup, EqualReuse with new ignored log paths and the [source manifest](track_c_final/source.json); no concurrent Cargo/writer. Runner enforces source hashes and restores environment. Apply [collector](track_c_collect_owner_results.ps1), [105inventory](track_c_verify_final_inventory.py), [Notes/Actions](track_c_compare_c1_final.py), [search/geometry](track_c_compare_search_geometry.py), [queue](track_c_compare_queue_controls.py), [startup](track_c_compare_startup_catalog.py), and the separate [equal collector](track_c_collect_search_equal_reuse.ps1) per [frozen protocol](track_c_final_protocol.json). Original logs remain ignored locally; their hashes/run IDs are in final JSON. No replay is necessary for this acceptance.

Pure fixture reconstruction reproduces9a958b83b7e7345f and matches the exact final1k root-list cold key. This checks signature only, not timing/fonts/display. [Receipt](track_c_final/representative_reproduction.json).

Representative serial PowerShell commands (new paths, clean measured checkout; supply a copy of the published manifest):

```powershell
$trackFinalSha = '18f8cac44f92fa18feee16b81f222953a622059a'
$trackFinalManifest = 'target/performance/track-c/reproduction-source.json'
foreach ($trackStage in @('OriginalLib','Startup','EqualReuse')) {
    & ./docs/performance/track_c_run_final.ps1 -Stage $trackStage -Source $trackFinalSha -SourceManifest $trackFinalManifest -LogPath ('target/performance/track-c/reproduce-' + $trackStage + '.log')
    if ($LASTEXITCODE) { throw 'Stop after failed measurement' }
}
& ./docs/performance/track_c_collect_owner_results.ps1 -LogPaths @('target/performance/track-c/reproduce-OriginalLib.log','target/performance/track-c/reproduce-Startup.log') -ExpectedSource $trackFinalSha -ExpectedMode Full -OutputPath target/performance/track-c/reproduce-owners.json
python -B docs/performance/track_c_verify_final_inventory.py --input target/performance/track-c/reproduce-owners.json --baseline docs/performance/track_c_baseline.json --source $trackFinalSha --output target/performance/track-c/reproduce-inventory.json
& ./docs/performance/track_c_collect_search_equal_reuse.ps1 -LogPath target/performance/track-c/reproduce-EqualReuse.log -ExpectedSource $trackFinalSha -ExpectedMode Full -OutputPath target/performance/track-c/reproduce-equal.json
```

For family comparison, preserve the seven collection fields and filter only `records`:96 search/geometry/query,2 `process_watch_events_concurrent_offered_load`/`process_watch_events_finite_burst`,3 `startup_action_catalog`,4 `NotePanel::maybe_refresh_heavy_derived`/`process_watch_events_actions_reload`. Run each linked comparator with `--input`, `--source`, `--output`, and `--mode Full` for geometry/queue/startup; C1 additionally takes `--baseline`. These filters and commands are recorded in the local ignored `c4-c` collection packet. Use PowerShell7; do not reinterpret a comma-joined external string as its `LogPaths` array.
