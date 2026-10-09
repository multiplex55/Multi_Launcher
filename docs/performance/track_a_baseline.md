# Track A runtime baseline (pre-optimization)

Status: G0 frozen at M0-C. These are actual local debug-test measurements, not release GUI latency or speedup claims. Original measurements remain unchanged after optimization.

## Source and host

- Authoritative base: 02399cf0848de2b429dfd9fae80a141ce50d972d; user confirmed current HEAD and waived ZIP comparison.
- Instrumented fixture source: d003b4a35ef3fafd75438493199cb20463378baf; branch performance-optimization. No runtime optimization preceded this capture.
- Windows NT 10.0.19045.0, x86_64 MSVC; Intel64 Family 6 Model 158 Stepping 9, GenuineIntel; 8 logical processors.
- rustc 1.97.1 (8bab26f4f 2026-07-14), LLVM 22.1.6; cargo-nextest 0.9.135.
- Power scheme: Balanced (powercfg /GETACTIVESCHEME).
- CPU model name and native monitor/DPI/refresh: NOT MEASURED; CIM access denied. Headless UI uses fixed 960x640 egui viewport/default scale.
- Profile: debug-test; MULTI_LAUNCHER_PERF=1; ML_TRACK_A_BENCH_MODE=full; Nextest test threads=1. OS/background load is uncontrolled.

## Protocol and reproducibility

See [workload recipe](track_a_workloads.md) and [metric definitions](track_a_metrics.md). The full serial command in the recipe passed six owner tests, 5,032 skipped, in 221.702 seconds. Each scenario has 20 samples; five warmups for UI/pure work, one for index traversal. p50/p95 use nearest rank; max is the slowest sample. Generation, fixture publication, output hashing and assertions are outside timing. Index trees are warm-cache local filesystem workloads. Actions reload includes typed file loading, actual synchronous scan, cache publication and query refresh; it does not measure notify delivery.

[Full bounded machine-readable summaries](track_a_baseline.json) retain exact nanoseconds, every metric, estimated bytes and privacy-safe fixture/output signatures (39 scenarios, about 60 KiB). Nextest immediate-final prints successful output twice; identical JSON lines were deduplicated, not treated as extra samples. Raw console logs and generated trees remain under ignored target/performance.

## Measured timings

All values below are milliseconds, rounded to three decimals. Exact values are in the JSON artifact.

| Scenario | p50 ms | p95 ms | max ms |
| --- | ---: | ---: | ---: |
| history-100-mixed-count-8-no-filter | 0.256 | 0.268 | 0.278 |
| history-100-pins-only-8 | 0.065 | 0.066 | 0.085 |
| history-100-mixed-count-8-rare-filter | 0.299 | 0.375 | 0.542 |
| history-100-mixed-count-50-renamed-missing | 0.245 | 0.275 | 0.284 |
| history-1000-mixed-count-8-no-filter | 3.204 | 5.455 | 5.773 |
| history-1000-pins-only-8 | 0.710 | 1.855 | 1.946 |
| history-1000-mixed-count-8-rare-filter | 3.596 | 4.101 | 4.346 |
| history-1000-mixed-count-50-renamed-missing | 3.167 | 6.500 | 7.193 |
| history-10000-mixed-count-8-no-filter | 28.378 | 39.308 | 39.977 |
| history-10000-pins-only-8 | 7.668 | 12.237 | 13.461 |
| history-10000-mixed-count-8-rare-filter | 122.060 | 143.329 | 145.228 |
| history-10000-mixed-count-50-renamed-missing | 112.528 | 131.420 | 151.994 |
| note-100-idle-refresh-check | 0.743 | 0.799 | 2.236 |
| note-100-draft-after-debounce | 29.002 | 41.883 | 43.569 |
| note-1000-idle-refresh-check | 5.559 | 6.515 | 7.095 |
| note-1000-draft-after-debounce | 331.521 | 368.764 | 375.786 |
| note-5000-idle-refresh-check | 29.254 | 31.860 | 32.027 |
| note-5000-draft-after-debounce | 1512.368 | 1529.377 | 1530.418 |
| quick-notes-100-empty-filter | 38.828 | 43.532 | 44.836 |
| quick-notes-100-sparse-filter | 2.572 | 2.783 | 2.837 |
| quick-notes-1000-empty-filter | 405.648 | 538.330 | 568.939 |
| quick-notes-1000-sparse-filter | 21.763 | 26.562 | 27.833 |
| quick-notes-5000-empty-filter | 2004.788 | 2034.023 | 2068.921 |
| quick-notes-5000-sparse-filter | 106.002 | 122.612 | 122.930 |
| launcher-100-list | 1.797 | 1.954 | 2.304 |
| launcher-100-grid-3-column | 1.669 | 2.572 | 2.708 |
| launcher-1000-list | 15.587 | 16.275 | 16.351 |
| launcher-1000-grid-3-column | 14.186 | 14.941 | 15.056 |
| launcher-10000-list | 155.191 | 157.140 | 157.650 |
| launcher-10000-grid-3-column | 136.690 | 142.395 | 142.657 |
| actions-reload-100-indexed-16-changed | 9.160 | 9.753 | 10.371 |
| actions-reload-100-indexed-16-unchanged | 6.637 | 7.649 | 8.257 |
| actions-reload-1000-indexed-1000-changed | 105.147 | 107.885 | 110.734 |
| actions-reload-1000-indexed-1000-unchanged | 9.298 | 10.206 | 20.607 |
| actions-reload-10000-indexed-10000-changed | 997.298 | 1006.727 | 1019.694 |
| actions-reload-10000-indexed-10000-unchanged | 35.945 | 36.801 | 37.973 |
| index-16-fresh-exhaustion | 1.650 | 3.020 | 3.104 |
| index-1000-fresh-exhaustion | 89.222 | 91.555 | 93.108 |
| index-10000-fresh-exhaustion | 867.009 | 881.198 | 885.056 |

## Structural observations (20 measured operations)

- Unchanged 5,000-note checks made 20 full snapshots: 572,131,520 estimated copied bytes, 100,000 alias inputs hashed, zero heavy recomputes. Snapshot lock wait totaled 66,200ns; snapshot elapsed (held work) totaled 401,743,700ns. Estimated copied bytes are not allocator measurements.
- 10,000-history mixed count 8 cloned 200,000 input records, resolved 150,020 candidates and enumerated 100,020 plugin catalogs. Pinned-only still cloned 200,000 ordinary history records, resolved 15,400 candidates and enumerated 11,540 catalogs. These reflect existing eager behavior.
- 10,000 launcher results built 200,000 widgets in each list/grid case. Quick Notes 5,000 empty-filter built 100,000 widgets; sparse-filter built 20 but still scanned the backing search/metadata path.
- Changed custom 10,000/indexed 10,000 Actions reloads made 20 complete scans, 420 iterator-next calls and constructed 200,000 indexed actions. Unchanged reloads scanned zero times and retained the Actions Arc/version.
- Output identity assertions cover actual launcher row indices/action IDs, selected first/middle/last identities, Quick Notes original-index edits, ordered history presentation and index membership/order. Actual rendered-subset signatures may intentionally change after virtualization; complete source order and identity invariants must remain covered separately.

## Repeatability gate

A second small-mode root workload passed one owner test, 5,037 skipped, in 1.017 seconds. Both list/grid fixture signatures and actual rendered output/order signatures matched exactly. [Repeat summaries](track_a_baseline_repeat.json) retain exact data. List p50 was 1.797 ms then 1.927 ms, p95 1.954 ms then 3.256 ms; grid p50 1.669 ms then 1.795 ms, p95 2.572 ms then 2.909 ms. Small debug tail samples are visibly noisy; count evidence and output parity are stronger than tiny timing differences. No additional runs were selected to hide that variability.

## Unmeasured coverage and acceptance limits

- Native halo/zoom moving/stationary source submission, display latency, exclusion composition, HUD/GDI and same-HWND move/resize correctness: NOT MEASURED / NOT RUN. Headless egui and fake operations cannot prove Win32 composition. [Native procedure](track_a_native_recipe.md) specifies later checks and separate API/visual outcomes.
- Baseline grid timing covers three columns; other column counts, DPI transitions and live context-menu/scroll interaction are not timed here. M3 requires focused state/geometry tests and records any native smoke limitation.
- History baseline uses mixed five pins, pinned-only and the full generated pin set; an explicit zero-pin and exact-eight-pin timing series is NOT MEASURED. M2 correctness tests must still cover those cases.
- No private note/history/clipboard payloads or absolute temporary paths are committed. No release-build, GPU, input-to-display or general idle-CPU claims follow from these numbers.
- Keep approved M1-M5 order. The measured O(N) snapshots/widgets/catalog work and synchronous index traversal justify those checkpoints. M6 caching remains conditional on native evidence; do not infer its benefit from these CPU fixtures.
