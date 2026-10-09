# Track A synthetic workload recipe

Track A workload fixtures and ignored owner benchmarks live under `src/performance/workloads.rs` and the subsystem test modules. Fixture builders are ordinary deterministic tests; headless owner benchmarks are ignored unless opted in before the test process starts.

Run the deterministic builder checks with:

```powershell
cargo nextest run --lib -E 'test(track_a_fixture)'
```

Run the minimum-size smoke workloads with `MULTI_LAUNCHER_PERF` set before Nextest starts. The small mode selects notes 100, actions/results 100, history 100, and an indexed tree of 16 files:

```powershell
$env:MULTI_LAUNCHER_PERF = '1'
$env:ML_TRACK_A_BENCH_MODE = 'small'
cargo nextest run --lib --test-threads 1 --run-ignored ignored-only -E 'test(track_a_benchmark_)' --success-output immediate-final --no-output-indent
```

For full fixture sizes, set the mode to `full` or remove it before starting Nextest:

```powershell
$env:MULTI_LAUNCHER_PERF = '1'
$env:ML_TRACK_A_BENCH_MODE = 'full'
cargo nextest run --lib --test-threads 1 --run-ignored ignored-only -E 'test(track_a_benchmark_)' --success-output immediate-final --no-output-indent
```

Full mode uses notes 100/1,000/5,000, launcher results 100/1,000/10,000 in list and three-column grid, history 100/1,000/10,000 across mixed/pins-only/rare-filter/renamed-and-missing scenarios, and index trees of 16/1,000/10,000 files. UI and pure-operation measurements use five warmups and twenty measured samples; index traversal uses one warmup and twenty samples. Reported p50/p95 use nearest-rank selection, and max is the slowest measured duration.

Each successful ignored test prints bounded `TRACK_A_WORKLOAD` JSON. It contains only scenario and scope labels, fixture counts and estimated bytes, deterministic privacy-safe fixture/output signatures, p50/p95/max nanoseconds, and selected bounded M0 counters. It does not include note or history payloads or absolute temporary paths. Fixture generation, cache/history publication, application construction, iterator construction, output hashing, assertions, and summary output are outside measured intervals. The shared harness checks that per-sample setup does not alter the performance counters.

The owner measurements exercise `NotePanel::maybe_refresh_heavy_derived` for idle and re-armed draft checks, the current history preparation owner, `LauncherApp::render_root_frame(ctx, None)` for list/grid rows, `NotesDialog::ui` for empty and sparse filters, `LauncherApp::process_watch_events` for changed and unchanged synthetic `WatchEvent::Actions` events, and exhaustion of fresh production `IndexBatchIter` instances. The actions event harness writes prepared JSON and enqueues exactly one event outside timing; it measures typed loading, any production index traversal, cache publication, and synchronous query refresh. Its unchanged case checks action `Arc` and version retention. It does not measure watcher notification delivery. Index checks also cover duplicate roots and item caps outside timing. Root selection and Quick Notes edit identities are checked against actual built-row receipts, with output signatures computed outside the timed operation.

These are headless debug-test CPU measurements. Comparable full-mode captures should run serially with `--test-threads 1` to avoid CPU or filesystem contention between owner tests. UI tests use a fixed 960×640 egui viewport and do not measure GPU rendering, display presentation, input latency, or a native window. Index results are warm-cache filesystem timings on the host running the test. Treat them as repeatable structural and local timing evidence, not as release-build or end-user latency claims. Full-mode runs are the M0-C baseline input; do not interpret the smoke-mode numbers as the full-size baseline.
