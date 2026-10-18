# D2 responsiveness: partial environment coverage

**No measured production optimization met the acceptance gate.** Product
behavior is unchanged. Instrumentation changes are test-only.

| Category | Actual evidence | Status |
|---|---|---|
| Process startup / first visible frame | No safe native launch | BLOCKED_ENVIRONMENT |
| Hotkey to usable visible launcher | No owned native fixture/input | BLOCKED_ENVIRONMENT |
| Query to usable CPU root frame | Synthetic catalogs, 5 warmups +15 measured per size | INTERNAL_BASELINE_ONLY |
| Dashboard useful frame / steady cost | No native fixture; no directly reusable bounded dashboard benchmark | BLOCKED_ENVIRONMENT |
| Radial invocation / submenu presentation | No native fixture/controller trace | BLOCKED_ENVIRONMENT |

Existing `dev/run_owned_process.ps1` with harmless `cargo --version` returned
runner exit 1: `containment monitor failure: Access denied`. The cargo root
exited 0 and its recorded identity was verified absent after cleanup. One
diagnostic exact-current-PID CIM query also returned access denied. Computer
Use initialization/window inventory succeeded; **Computer Use itself is not
reported unavailable**. Required descendant containment is unavailable, so no
GUI fixture was launched and no input or screenshots were taken. Native
allowance is exhausted, no alternate runner bypass attempted. D2-C dashboard
and radial timing remain unmeasured; Criterion would measure a different
component and is not used as a presentation substitute. No controller trace
identifies a radial microbenchmark bottleneck, so no radial bench was compiled.

| Catalog actions | CPU p50 | CPU nearest-rank p95 | Samples |
|---:|---:|---:|---:|
| 100 | 1.0947 ms | 4.2427 ms | 15 |
| 1,000 | 1.6240 ms | 2.9170 ms | 15 |
| 10,000 | 6.2323 ms | 6.7161 ms | 15 |

Source `78f5b16c23d2f461918bab5216954bbd59351410`, fast-dev, default
features, egui 960x640 / scale 1.0, synthetic exact-prefix action fixture,
fresh query geometry with warm fonts. Exact ignored test:
`gui::render::tests::track_c_benchmark_root_query_to_usable_cpu_frame`.
`MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`,
`ML_TRACK_C_MEASURED_SAMPLES=15`, `ML_TRACK_C_BENCH_MODE=full`;
provenance SHA/profile switches set. Runner restores overrides; test workspace
isolates CWD/settings/notes and restores its environment. Full command wall
89.11 s includes setup outside timed owners. Existing independent output and
revision/usable-frame assertions passed. Per-row fixture signatures, raw timing
numbers and command are in d2_query.json. Screenshot index: empty (0/16).

D2-D ranking: the 10,000-item query has the largest observed internal p95
(6.7161 ms), followed by 100 items (4.2427 ms), then 1,000 (2.9170 ms). The
nonmonotonic tails show why one small sample batch is not attribution. At 15
samples nearest-rank p95 equals the maximum. Instrumentation overhead was not
separately measured; mark `instrumented_only`. Unoptimized fast-dev results are
not release latency. No native before/after, production critical-path split,
or adjacent native guardrail is available. Thus no specific root-cause change
is justified by these measurements: D2-E–H `SKIPPED_INSUFFICIENT_EVIDENCE`,
zero candidates/variants, not a claim that all UI paths are already fast.

Historical startup component probes use a separate fixed 20-sample loop, not
the shared helper. They were not run beyond Track D's 15-sample ceiling. No new
native executable, external metrics framework or production timer was added.
