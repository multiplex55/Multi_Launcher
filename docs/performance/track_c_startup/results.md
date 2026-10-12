# C3-A startup characterization — LOCAL ONLY

PASS;20/29closed. Measured exact clean source `0fbd2f0327badf899e2bdd662616ac6d9d1228d5`, fast-dev, telemetry enabled, one test thread. No production startup reordering or optimization. Five warmups,20 retained samples per recipe, zero exclusions. Native/process/hotkey/display NOT RUN / NOT MEASURED.

The actual binary complete-catalog series and the library ready-catalog component are separate experiments. Do not add them as process latency. The component prepares its complete acknowledged owned catalog before timing, then includes the real Omni constructor and nested default providers/watchers, apps-only settings, inert registration, actual LauncherApp construction/initial empty-query search/transfer, pending publication, exactly one fresh-context root frame and FullOutput disposal. Context allocation belongs to the app-construction phase. Its request receipt begins at in-frame pending consumption and is nested inside the frame, excluding earlier constructor and frame preparation. Post-timer eager validation uses that actual context and captured first-frame viewport; it is not measured or prewarming.

All ordinary12 distinct tests PASS (13 executions including one corrected fixture assertion failure). The initial compile failed before tests; the moved-label oracle issue was corrected. The empty-provider check was corrected to preserve the legitimate exact `query:o list` command fallback while proving zero catalog overlap. Independent source/lifecycle/method/Small/Full identity/numerical/privacy review CLEAR. Binary compilation also compiled the normal production library. Existing unrelated warnings remain unchanged.

Small owners passed in0.079s catalogue/1.716s component; Full in39.813s catalogue/67.248s component. These test-wall times include untimed fixture preparation/validation/teardown and are not owner or request latency. Full has60 original catalogue samples and60 separate component samples; supplemental catalogue attribution repeats the same60 elapsed samples. Small adds20+20 unique samples. All60 Full and20 Small component receipts prove complete acknowledged catalogue, one current accepted+painted PendingQuery/Search frame, Query-only N+3-row cold geometry, and exact ten attempted/created/released callback handlers with no overflow.

## Matched Full G0 catalogue comparison

Times are p50/p95/max milliseconds. G0 source `071243db068835617a054a74fb5734a6dd9f128d`; frozen fixtures/structural identities/revision/work/viewport domains match. All three medians increase, two p95s and one maximum increase. No causal noise or benefit claim.

| Indexed files | G0 | Current | p50 change | p95 change | max change |
|---:|---:|---:|---:|---:|---:|
| 16 | 1.6182/1.7962/2.9391 | 1.6760/1.8915/1.9566 | +3.57% | +5.31% | -33.43% |
| 1000 | 86.4063/100.4915/100.5186 | 86.6610/95.8604/97.4210 | +0.29% | -4.61% | -3.08% |
| 10000 | 860.8644/922.7792/956.3962 | 863.8380/958.9180/1103.8792 | +0.35% | +3.92% | +15.42% |

## Full attribution and separate component

Each cell is p50/p95/max milliseconds. Phase quantiles cannot be added. Worker scan is summed traversal-step time overlapping caller wait, not continuous scan wall time. The unchanged clone loop and full Action equality establish N indexed-body clones; no intercepted clone counter was added, and historical output-cardinality counters are unchanged.

| Scope | 16 | 1,000 | 10,000 |
|---|---:|---:|---:|
| Catalogue coordinator_construct | 0.0227/0.0492/0.0672 | 0.0256/0.0337/0.0395 | 0.0365/0.0757/0.0881 |
| Catalogue submit | 0.0014/0.0023/0.0033 | 0.0016/0.0021/0.0023 | 0.0022/0.0060/0.0075 |
| Catalogue wait | 1.6134/1.8229/1.8821 | 86.0733/95.3379/96.8302 | 859.2459/953.9514/1094.5172 |
| Catalogue take_result | 0.0032/0.0039/0.0041 | 0.0038/0.0044/0.0050 | 0.0040/0.0052/0.0053 |
| Catalogue validate_transfer | 0.0005/0.0006/0.0008 | 0.0005/0.0008/0.0011 | 0.0006/0.0009/0.0664 |
| Catalogue clone_indexed_actions | 0.0065/0.0170/0.0216 | 0.2968/0.4033/0.5823 | 2.8192/4.6067/5.7904 |
| Catalogue wrap_catalog_arc | 0.0003/0.0004/0.0004 | 0.0003/0.0007/0.0008 | 0.0007/0.0027/0.0033 |
| Catalogue overlapping scan steps | 1.5141/1.6729/1.7284 | 85.6644/94.9843/96.4654 | 856.6583/951.3079/1091.0800 |
| Separate ready-catalog whole component | 21.4060/22.6558/22.8270 | 90.9622/99.4739/100.3408 | 731.0757/773.2051/793.3050 |
| Component omni_construct | 4.3099/4.9284/5.0744 | 11.9896/12.7230/14.6426 | 82.2972/90.5966/98.4734 |
| Component apply_apps_only_settings | 0.0197/0.0251/0.0262 | 0.0208/0.0251/0.0263 | 0.0219/0.0335/0.0404 |
| Component inert_manager_register | 0.5372/0.6279/0.6628 | 0.5333/0.5574/0.5786 | 0.5602/0.7052/1.1418 |
| Component launcher_app_construct_and_transfer | 6.9204/7.6612/7.7201 | 11.3445/12.8762/15.3021 | 53.3843/60.4443/66.2449 |
| Component pending_query_publish | 0.0007/0.0010/0.0016 | 0.0010/0.0014/0.0017 | 0.0011/0.0016/0.0017 |
| Component ctx_run_and_fulloutput_drop | 9.3620/10.6982/10.8174 | 66.6231/73.0479/75.0081 | 594.8318/630.5417/641.9371 |
| Component residual | 0.0011/0.0016/0.0017 | 0.0011/0.0015/0.0015 | 0.0013/0.0017/0.0020 |
| Nested pending consumption to usable CPU frame | 1.9923/2.7589/3.1896 | 56.0071/62.4784/62.5959 | 554.0840/584.6513/593.9332 |

## Small context-only series

N16 Small versus G0 Full is context only, not the matched Full comparison above. All18 distinct Small timing distributions follow; catalogue elapsed is shared exactly with its supplement.

| Scope | p50/p95/max ms |
|---|---:|
| Catalogue whole owner | 1.6895/1.9409/2.0905 |
| Catalogue coordinator_construct | 0.0236/0.0579/0.0777 |
| Catalogue submit | 0.0015/0.0025/0.0028 |
| Catalogue wait | 1.6297/1.8520/2.0270 |
| Catalogue take_result | 0.0033/0.0038/0.0042 |
| Catalogue validate_transfer | 0.0005/0.0005/0.0012 |
| Catalogue clone_indexed_actions | 0.0083/0.0168/0.0168 |
| Catalogue wrap_catalog_arc | 0.0003/0.0004/0.0005 |
| Catalogue overlapping scan steps | 1.5176/1.7120/1.9017 |
| Separate ready-catalog whole component | 21.8801/25.0571/26.0354 |
| Component omni_construct | 4.5427/5.2112/5.2620 |
| Component apply_apps_only_settings | 0.0201/0.0249/0.0254 |
| Component inert_manager_register | 0.5382/0.6665/0.6886 |
| Component launcher_app_construct_and_transfer | 6.9919/7.7565/7.7581 |
| Component pending_query_publish | 0.0006/0.0010/0.0015 |
| Component ctx_run_and_fulloutput_drop | 9.7187/11.5619/13.2778 |
| Component residual | 0.0012/0.0014/0.0014 |
| Nested pending consumption to usable CPU frame | 2.0619/2.9118/2.9600 |

## Limits and reproduction

One fixed owned root/store set per25sample series; global snapshots/caches and OS filesystem cache are warmed across repetitions. Full real-path Action equality precedes prefix normalization. Raw Action/payload identity is constant within a fixed root; geometry/viewport hashes are per-real-root, not claimed equal across roots. Untimed teardown waits for final callback-handler release after app/provider/coordinator destruction; this does not join detached native notify threads. Failure preserves owned paths/environment in its failing child. No generic plugin reload, main/spawn_gui/run_native, desktop action, real hotkey or recycle operation ran. No allocator/RSS, cold-disk, first-process, production prevalence or application-wide percentage claim.

For measurement, use a clean checkout of the measured commit, set process-local `MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`, `ML_TRACK_C_PROFILE=fast-dev`, `ML_TRACK_C_SOURCE_SHA=0fbd2f0327badf899e2bdd662616ac6d9d1228d5`, and `ML_TRACK_C_BENCH_MODE=small` or `full`; restore prior environment afterward. Run serially:

```powershell
cargo nextest run --bin multi_launcher --cargo-profile fast-dev --test-threads 1 --no-fail-fast --success-output immediate --run-ignored only -E "test(=tests::track_c_benchmark_startup_action_catalog_owner)"
cargo nextest run --lib --cargo-profile fast-dev --test-threads 1 --no-fail-fast --success-output immediate --run-ignored only -E "test(=gui::render::tests::track_c3a_benchmark_ready_catalog_restricted_omni_first_cpu_frame)"
```

Use `track_c_collect_owner_results.ps1` for original catalogue records, `track_c_compare_startup_catalog.py` for matched frozen G0 comparison, and `track_c_collect_startup_results.py --stage startup|component` with the original records or frozen component bindings. Each collector refuses an existing output. Strict complete schemas, typed integers, nearest ranks, source/profile/fixture joins, phase bounds, Query-only geometry and callback inventories are frozen in protocol.json;17/21/28 original/supplement/component synthetic controls PASS. Collected JSON status strings remain untouched; companion reviews record acceptance. All favorable/adverse arrays are retained in original_*, attribution_*, component_* and comparison_* JSON; execution, verification and review files record actual runs and retained failures.

C3-B remains separately gated; this checkpoint provides characterization, not a startup speedup.
