# Performance Optimization Execution Ledger

Measurements are evidence gates. Preserve the Typed Command Bus, Action/config/plugin ABI and
search semantics. Do not add a global async runtime or result cap. Prefer deterministic async tests.

## Milestone 1 - Performance measurement foundation

**Status:** `complete`

Established trustworthy runtime, search, idle/repaint, and developer-turnaround baselines with
minimal behavioral change. Real versus cached 10k search is distinct; representative, exact/high
specificity, broad, no-match, command-cache, completion construction and lookup paths are covered.
`MULTI_LAUNCHER_PERF=1` enables startup, first-usable-frame, dashboard, search/plugin, and
frame/repaint diagnostics with negligible disabled overhead.

Verification: `cargo fmt --all --check`, `cargo check`, focused Nextest tests,
`cargo bench --bench search`, affected benches, and `git diff --check`.

Planned commit: `perf: establish runtime and build performance baselines`

## Milestone 2 - Startup and dashboard worker boundary

**Status:** `complete`
**Dependency:** Milestone 1 (`424bc23`, complete).
**Commit:** `a2a1f8d`

Preserve immutable `Arc<DashboardDataSnapshot>` reads. Add one owned coalescing worker behind
typed refresh requests; atomically install snapshots, repaint, and shut down; remove synchronous
startup/UI refresh work; reuse process enumeration. Default/loading dashboard state remains valid
while the launcher becomes usable earlier.

Planned commit: `perf(dashboard): move refresh work off the UI thread`

Verification: `cargo fmt --all --check`, `cargo check`, dashboard runtime unit tests, the
164-test affected Nextest selection, migrated gesture refresh test, release startup/frame measurement,
stale-reference searches, and `git diff --check`.

## Milestone 3 - Search and dynamic-plugin optimization

**Status:** `complete`
**Dependency:** Milestones 1-2 (`424bc23`, `a2a1f8d`; complete).

Remove measured blocking/dynamic work from synchronous search without changing `Plugin`. Add an
owned bounded public-IP cache worker with TTL, backoff, timeout, single-flight, repaint and shutdown.
Optimize other plugins only when profiling proves a bottleneck. Preserve ranking/results and avoid
caps.

Planned commit: `perf(search): remove blocking dynamic work from query handling`

## Milestone 4 - Idle, repaint, and worker lifecycle

**Status:** `pending`  
**Dependency:** Milestones 1-3 final runtime state.

Add a small pure state-aware repaint policy while preserving timer, animation, toast, file-search,
MkMacro, overlay and diagnostic schedules. Inventory workers and fix only measured lifecycle or
duplicate-work problems. Avoid broad scheduler/service refactors.

Planned commit: `perf(runtime): reduce unnecessary idle and repaint work`

## Milestone 5 - Cargo and Nextest turnaround

**Status:** `pending`  
**Dependency:** Milestones 1-4 final state.

Measure compile, codegen/link, integration-target link, scheduling and execution separately, plus
representative incremental edits. Consolidate targets or adjust dependency/profile features only
when justified while preserving isolation and diagnostics. Documentation-only is valid if no safe
material win exists. Finish with full verification and independent review.

Planned commit: `perf(dev): improve cargo and nextest iteration time`

## Final integration and review

After all milestones, inspect for stale/duplicate paths, run the full suite and changed benchmarks,
obtain independent high-reasoning review, resolve findings, record commits/final measurements, and
verify the intended working tree is clean.

## Benchmark inventory

| Target | Current meaning and limitation |
|---|---|
| `search` | Production `LauncherApp::search`; old `search_10k` repeated one query and mostly measured the cache fast path after warmup. It is replaced by explicit real and cached workloads. |
| `omni_search` | Synthetic linear-fuzzy versus FST-subsequence experiment, not production Omni Search latency. |
| `macros_search` | Warm global manager lookup through `search_first_action`; not registered as a Cargo bench and not cold/end-to-end macro work. |
| `todo_widget_filtering` | Standalone old/new helpers over 25k synthetic todos, not dashboard/end-to-end todo latency. |

## Baseline environment and methodology

Collected 2026-09-05/06 on Microsoft Windows 10 Home 10.0.19045 (build 19045), Intel64
Family 6 Model 158 Stepping 9, 8 logical CPUs. Toolchain: rustc 1.97.1
(8bab26f4f 2026-07-14), Cargo 1.97.1, stable `x86_64-pc-windows-msvc`, LLVM 22.1.6.

Criterion used its default 3 second warmup and 100 samples in the optimized bench profile. Values
below are Criterion point estimates with confidence intervals. Startup was one warm-filesystem
release run from an isolated temporary working directory containing copied settings with file
logging enabled and no `actions.json`; it is not a cold-start measurement. Frame/repaint cadence
was observed for 12 consecutive one-second windows after the initial window. Cargo values are
Cargo-reported build times. Incremental edits changed only the source file timestamp, then ran
`cargo check`, accurately representing crate invalidation without changing content. The full
Nextest build compiled/linked 127 binaries.

## Runtime baseline

| Metric | Baseline | Method / limitation |
|---|---:|---|
| Settings load | 1.690 ms | Opt-in release trace, warm filesystem. |
| Action load | 0.059 ms | Empty/missing isolated `actions.json`; not representative of populated actions. |
| PluginManager construction | 58.725 ms | Opt-in release trace. |
| Plugin registration | 16.451 ms | Built-ins, isolated configuration. |
| Watcher initialization | 7.560 ms | Opt-in release trace. |
| Dashboard construction | 0.612 ms | Excludes data refresh. |
| Dashboard initial refresh complete | 595.584 ms | Synchronous startup baseline; separate from usability. |
| MkMacro initialization | 0.092 ms | Isolated configuration/store. |
| Action filter metadata | 0.008 ms | Empty action fixture; populated scaling is covered below. |
| Command-search cache | 0.403 ms | Default registered commands. |
| Completion index | 0.794 ms | Default commands, empty custom actions. |
| LauncherApp construction | 661.777 ms | Includes synchronous dashboard refresh. |
| Startup to first update | 989.119 ms | Process entry to first egui `update`. |
| Startup to first usable frame | 989.136 ms | Essential construction complete and first normal input/render frame entered. |
| Representative high-specificity search (500) | 155.57 us | Criterion [154.34, 156.93] us; alternating queries guarantee real work. |
| Representative broad search (500) | 394.45 us | Criterion [390.07, 399.73] us; alternating queries. |
| Representative no-match search (500) | 81.00 us | Criterion [80.60, 81.47] us; alternating queries. |
| 10k real static search | 3.2608 ms | Criterion [3.2293, 3.2940] ms; four-query cycle guarantees no cache hit. |
| 10k cached repeated search | 8.8567 ns | Criterion [8.5273, 9.1958] ns; intentionally warmed identical query/actions. |
| Command-cache lookup (250) | 111.99 us | Criterion [111.30, 112.71] us; alternating queries force real fallback work. |
| Completion construction (500 actions + 250 commands) | 278.10 us | Criterion [275.40, 281.04] us. |
| Completion construction (10k actions + 250 commands) | 3.4988 ms | Criterion [3.4461, 3.5549] ms. |
| Completion lookup (10k index) | 1.1715 us | Criterion [1.1528, 1.1912] us, five-result prefix lookup. |
| Visible/focused dashboard idle cadence | about 4 frames/s and 4 dashboard repaint requests/s | 12 one-second windows after startup, each with 4 frames and 4 measured 250 ms requests. |
| Hidden/unfocused/non-dashboard idle CPU | not credibly measured | The automated run did not manipulate native visibility/focus or provide a stable external CPU sampler. Diagnostics expose those states for Milestone 4. |

## Milestone 2 startup/dashboard results

Measured with a warm release build in a fresh isolated working directory using the same opt-in
instrumentation as the baseline. The measurement log was placed outside the watched data directory
to avoid making the log itself a filesystem-watch input. Values are one representative warm run;
a second run was used to observe frame cadence through refresh completion.

| Metric | Before | After | Change / evidence |
|---|---:|---:|---|
| `LauncherApp` construction | 661.777 ms | 60.130 ms | 601.647 ms faster (90.9%); dashboard I/O is no longer construction work. |
| Startup to first usable frame | 989.136 ms | 355.445 ms | 633.691 ms faster (64.1%). |
| Initial dashboard refresh | 595.584 ms synchronous | 617.225 ms asynchronous | The work itself remains comparable but now starts after the first usable update and publishes one atomic snapshot. |
| Dashboard frame-stall evidence | Refresh blocked startup for 595.584 ms before any usable frame | No refresh-attributed frame stall observed | The cadence window spanning a 628.048 ms async refresh rendered 12 frames in 1,152 ms, then returned to 4 frames/s. M1 instrumentation reports aggregate cadence, so it cannot identify an exact maximum inter-frame gap. |

The removed `search_10k` result is intentionally not reported as real-search baseline: after its
first invocation it measured the cached early return, so its historical number would be misleading.

## Milestone 3 search/dynamic-plugin results

### Measurement method and ranking

Before source changes, targeted Nextest cases were run with `--status-level all` on the same host and
warm debug dependencies. Those case durations include test-process setup/teardown, so they rank
blocking behavior but are not presented as Criterion-equivalent latency: Processes measured
1.033-1.067 s per query case, SysInfo 1.000-1.061 s, and the cold Volume name lookup 0.481 s.
Network cases measured 0.060-0.074 s, Shell 0.034-0.086 s, Missing 0.068 s, and Mouse Gestures
0.012-0.021 s including harness overhead. Public IP was ranked worst/unbounded by direct source
inspection because each matching query performed a real blocking HTTP request without an explicit
client timeout; deliberately no real network measurement was made. Browser Tabs already used an
asynchronous single-flight cache (its existing isolated cache test includes a fixed 500 ms wait),
and Layout had no isolated pre-existing timing test; both were measured below without changing
their implementations.

The final Criterion run exercises `PluginManager::search_filtered` with one enabled built-in and a
warmed production system snapshot. Values are point estimates from the default 3 s warmup and 100
samples. The IP provider is covered by deterministic blocked/failing fake-provider tests rather
than a real HTTP benchmark.

| Plugin/path | Before classification | Final query cost | Decision |
|---|---|---:|---|
| IP public address | Unbounded blocking HTTP in `search` | Provider I/O is absent from `search`; fake provider may remain blocked while search returns | Dedicated bounded worker/cache added. |
| Processes | 1.033-1.067 s cases; `System::new_all` per search | 51.876 us | Shared immutable system snapshot/background refresh added. |
| SysInfo | 1.000-1.061 s cases; system/disk enumeration per search | 2.511 us | Shared immutable system snapshot/background refresh added. |
| Volume name lookup | 0.481 s cold case | 3.242 us | Existing five-second intent retained; refresh moved to shared worker. |
| Network | Existing refresh-rate cache; 0.060-0.074 s test cases | 4.097 us | Existing cache sufficient; unchanged. |
| Browser Tabs | Existing async single-flight cache | 2.115 us (`tab clear` cached/control path) | Existing cache retained; unsafe UI Automation discovery excluded from Criterion. |
| Shell | Routed file work only for explicit subcommands | 1.861 us (`sh`) | Unchanged-fast; existing behavior retained. |
| Layout | Explicit config access | 30.905 us (`layout`) | Unchanged-fast; cache staleness/complexity not justified. |
| Mouse Gestures | Static root query; file loads only for explicit inspection subcommands | 3.435 us (`mg`) | Unchanged-fast; existing gesture database behavior retained. |
| Missing | Explicit maintenance scan | 273.91 us | Intentionally synchronous rare work; cache invalidation complexity not justified. |

### Search-core comparison

No search-core allocation/materialization change was retained. The identical M1 workloads remained
within a variable host envelope and did not identify a stable core bottleneck attributable to this
milestone. In particular, the no-match sample regressed while other unchanged paths improved, so
these values are recorded without claiming a core-search speedup.

| Workload | M1 baseline | M3 final |
|---|---:|---:|
| Representative high-specificity real | 155.57 us | 155.43 us |
| Representative broad real | 394.45 us | 380.81 us |
| Representative no-match real | 81.00 us | 82.541 us |
| 10k real query cycle | 3.2608 ms | 2.8706 ms |
| 10k cached repeat | 8.8567 ns | 7.7080 ns |
| Command-cache lookup | 111.99 us | 101.30 us |

### Architecture and lifecycle

`IpPlugin` now owns a capacity-one channel worker and one cached last-good value. Success is fresh
for five minutes; failures retain the last-good value and back off for 30 seconds. The Reqwest
client has a two-second connect timeout and three-second total request timeout. Repeated stale
queries are single-flight, and drop signals and joins the worker. Processes, SysInfo, and Volume
share one lazily started capacity-one system-data runtime owned by `PluginInternalServices`; it
publishes an immutable snapshot and refreshes on demand at most once per five seconds off the search thread, avoiding duplicate startup enumeration.

A built-in-only generation/repaint notifier invalidates the launcher's current cached query and
wakes egui after cache publication. `Plugin::search`, dynamic-library vtables, the Typed Command Bus,
commands, filters, ranking, completion, and result cardinality are unchanged. The dashboard's
synchronous process enumerator remains only inside the Milestone 2 dashboard worker; launcher query
handling has no live process/system/disk/public-network discovery path.

### Tests and verification

Deterministic channel-controlled tests cover blocked-provider non-blocking search, single-flight,
public-IP TTL, failure backoff, last-good retention, repaint, idle shutdown/join, system snapshot
publication/coalescing, and cached Processes/SysInfo/Volume materialization. The former IP integration
test no longer contacts the public internet.

Commands completed successfully:

- `cargo test plugins::ip::tests --lib -- --nocapture` (3 passed).
- Focused system-data/process/sysinfo/volume/notifier unit filters (9 passed total).
- `cargo nextest run --test ip_plugin --test processes_plugin --test sysinfo_plugin --test network_plugin --test volume_plugin --test shell_plugin --test mouse_gestures_plugin --test missing_plugin` (29 passed).
- `cargo nextest run --test ranking --test plugin_routing --test plugin_exact_match --test query_autocomplete --test plugin_commands --test web_search_prefix` (30 passed).
- Final combined run of all 14 targets above: 59 passed, 0 skipped.
- `cargo test plugins::browser_tabs::imp::tests --lib -- --test-threads=1` passed both legacy cache tests; they share global state and interfere under concurrent execution.
- `cargo fmt --all --check` and `cargo check` passed.
- `cargo bench --bench search` passed after replacing an unsafe Browser UI Automation discovery workload with the deterministic `tab clear` cached/control path.
- Stale blocking-path searches found public HTTP only in the worker provider and `System::new_all` only in the system worker plus the dashboard-worker helper.
- `git diff --check` passed (line-ending conversion warnings only).

### Rejected milestone 3 optimizations

- Search-core temporary/allocation refactors: rejected because unchanged Criterion workloads did not
  show a stable bottleneck and added complexity would not be evidence-backed.
- Result caps or render virtualization: rejected because no broad-render bottleneck was established
  and all logical results must remain available.
- Replacing Network or Browser Tabs caches: rejected because their existing cached paths measured in
  low single-digit microseconds and already avoid repeated discovery.
- Caching Shell, Layout, Mouse Gesture inspection, or Missing maintenance results: rejected because
  measured explicit-query costs did not justify stale-data and invalidation complexity.
- Benchmarking live Browser UI Automation discovery: rejected after it caused a Windows access
  violation in the first benchmark attempt; the deterministic cached/control path is the valid
  repeatable workload.

## Developer baseline

| Metric | Baseline | Method / limitation |
|---|---:|---|
| Fully warm no-change `cargo check` | 0.93 s | One run after a preceding 10.38 s source rebuild; dependencies warm. |
| Incremental common GUI edit | 6.50 s | Touched `src/gui/render.rs`, then `cargo check`. |
| Incremental MkMacro edit | 6.44 s | Touched `src/gui/mkmacro_dialog/action_editor.rs`, then `cargo check`. |
| Nextest compile/link | 11 min 59 s | Source-invalidated profile, 127 binaries. An earlier equivalent build took 14 min 03 s, showing host variance. |
| Full Nextest execution | 86.812 s | 2,960 tests: 2,959 passed, 1 failed, 7 skipped. |
| Incremental release binary build | 2 min 05 s | Source-invalidated `cargo build --release --bin multi_launcher`; dependencies warm. |
| Bench-profile build | 3 min 13 s | First `cargo bench --bench search` after changes; partially warm release dependencies. |
| Clean verification | not run | `cargo clean` would destroy shared/user build cache merely to manufacture a value. Use isolated target storage in Milestone 5. |
| Dependency/codegen/per-binary link split | not available | These wall-time runs expose aggregate profile time. Milestone 5 should use Cargo timings or isolated targets rather than inventing a split. |

The full Nextest failure was `tests/logging.rs::writes_log_file` (buffered file content did not
contain `test`). It reproduced alone. Milestone 1 does not modify logging initialization or that
test, so this is recorded as an existing/environmental verification issue rather than hidden or
reclassified as a pass.

## Rejected optimizations

- Treating the prior `search_10k` result as real search: rejected because it measures the valid
  repeated-query cache path after warmup.
- Treating `omni_search` as a production optimization comparison: rejected because its FST
  subsequence implementation is an isolated experiment, not the production Omni Search path.
- Making runtime changes from the baseline alone: deferred to later milestones; this milestone
  changes measurement boundaries only.

## Commit record

| Milestone | Commit | Subject |
|---|---|---|
| 1 | `424bc23` | `perf: establish runtime and build performance baselines` |
| 2 | `a2a1f8d` | `perf(dashboard): move refresh work off the UI thread` |
| 3 | pending | `perf(search): remove blocking dynamic work from query handling` |
| 4 | pending | `perf(runtime): reduce unnecessary idle and repaint work` |
| 5 | pending | `perf(dev): improve cargo and nextest iteration time` |
