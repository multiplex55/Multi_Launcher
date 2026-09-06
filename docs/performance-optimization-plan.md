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
**Commit:** `544c4c6`

Remove measured blocking/dynamic work from synchronous search without changing `Plugin`. Add an
owned bounded public-IP cache worker with TTL, backoff, timeout, single-flight, repaint and shutdown.
Optimize other plugins only when profiling proves a bottleneck. Preserve ranking/results and avoid
caps.

Planned commit: `perf(search): remove blocking dynamic work from query handling`

## Milestone 4 - Idle, repaint, and worker lifecycle

**Status:** `complete`
**Dependency:** Milestones 1-3 final runtime state.
**Commit:** `9885f20`

Add a small pure state-aware repaint policy while preserving timer, animation, toast, file-search,
MkMacro, overlay and diagnostic schedules. Inventory workers and fix only measured lifecycle or
duplicate-work problems. Avoid broad scheduler/service refactors.

Planned commit: `perf(runtime): reduce unnecessary idle and repaint work`

## Milestone 5 - Cargo and Nextest turnaround

**Status:** `complete`
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
| Browser Tabs | Async single-flight cache | 203.04 us (1,000 populated cached tabs); 1.428 us (`tab clear`) | Discovery is excluded from Criterion; cached filtering/materialization and the static command are measured separately. |
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

## Milestone 4 idle/repaint results

The unconditional dashboard 250 ms repaint was replaced by a pure state-aware policy. Widget
demand is aggregated only from slots in the active dashboard: static and manual-refresh widgets
are event-driven; configurable auto/throttled refresh widgets and diagnostics request a one-second
cadence; running timers, running stopwatches, and the notes-graph animation retain the 250 ms fast
cadence. When `reduce_dashboard_work_when_unfocused` applies, background periodic/animation work
stops and fast time-sensitive demand degrades to one second. A hidden launcher schedules no
dashboard repaint and skips dashboard widget rendering entirely. Dashboard-inactive search/results
views schedule no dashboard repaint.

Specialized schedules remain local: active File Search keeps its existing polling repaint and stops
on terminal events; clipboard preview and Multi Manager reconnect keep their 150 ms schedules;
toasts retain their own egui lifecycle; MkMacro jobs and overlays repaint from completion/runtime
events. The policy does not introduce a global scheduler or async runtime.

### Repaint/idle measurement

The M1 release baseline produced four frames and four dashboard repaint requests per second for 12
successive one-second visible/focused default-dashboard windows. The M4 release build was run three
times from the same isolated warm-filesystem fixture for 15, 12, and 12 seconds, with the log placed
outside the watched data directory. Each run produced the expected startup frame and asynchronous
dashboard-refresh publication, then no `runtime.frames` sample at all: after the sub-one-second
refresh event there was no periodic frame at which the one-second sampler could emit. This is
evidence that the default static dashboard no longer keeps egui awake (4 scheduled frames/s to no
recurring scheduled frames), not a numeric CPU utilization claim.

| State / feature | Final cadence/evidence |
|---|---|
| Hidden launcher | No dashboard schedule; widget rendering is gated off. Deterministic policy test. |
| Dashboard inactive | No dashboard schedule. Deterministic policy test. |
| Visible, focused, static/event-driven dashboard | No recurring schedule; three release traces plus policy test. |
| Visible, focused, auto-refresh or diagnostics | Slow, 1 s. Deterministic aggregation/policy tests. |
| Visible, focused, running timer/stopwatch | Fast, 250 ms, preserving countdown display/completion polling. Deterministic aggregation/policy tests plus timer/stopwatch regressions. |
| Visible, focused, notes-graph animation | Fast, 250 ms. Deterministic aggregation/policy test. |
| Visible, unfocused, reduction enabled | Background demand stops; fast time-sensitive demand becomes 1 s. Deterministic policy test. |
| Visible, unfocused, reduction disabled | Same cadence as focused. Deterministic policy test. |
| Toast | Existing toast-owned animation/lifetime repaint unchanged; toast regressions passed. |
| Active File Search | Existing active-search repaint and terminal-event stop unchanged; File Search regressions passed. |
| Active MkMacro/visual overlay | Existing job/runtime/overlay event repaint unchanged; MkMacro runtime/visual regressions passed. |

Native hidden/focus automation and stable process-wide CPU sampling were not credible on this host,
so CPU percentages and native focus-transition frame counts are deliberately not reported. The pure
matrix covers those state decisions, while the repeated release traces establish the actual static
idle outcome. Active feature cadence is asserted deterministically rather than using wall-clock UI
tests.

### Production worker inventory

| Classification | Call sites / ownership | Decision |
|---|---|---|
| Long-lived services | GUI/hotkey runtime, `hotkey::runtime`, mouse-gesture service, MkMacro UIA/runtime/recorder/hotkey services, Multi Manager runtime, dashboard runtime, plugin system-data and IP runtimes | Owned stop/channel lifecycles; dashboard/system/IP shutdown and coalescing were already made deterministic in M2/M3. No M4 change. |
| UI-lifetime workers | File Search coordinator, clipboard-immediate coordinator, MkMacro visual-overlay service, Multi Manager capture/reconnect | Existing cancellation, pending/single-flight state, repaint callbacks, and generation/result ownership retained. No measured duplicate idle work. |
| Short bounded tasks | system actions, sound playback, clipboard transformations, diff scans/file operations, browser-tab refresh, preview/crop/image authoring jobs, launcher-command submissions | Work is demand-triggered and bounded. Browser Tabs already guards refresh with its cache/single-flight flag; diff and image paths suppress stale generations. No M4 change. |
| External-process readers | Ripgrep and Everything stdout/stderr readers | One bounded reader pair per owned child process; cancellation/output bounds already present. No M4 change. |
| Potential repeated spawn reviewed | browser-tab discovery, File Search requests, diff scans, image preview/crop jobs, clipboard immediate execution | Existing single-flight, cancellation, or generation checks prevent duplicate/stale application. No evidence justified replacing them with persistent workers. |
| Test workers | deterministic broker, cancellation, repaint, and worker-lifecycle fixtures under `src`/`tests` | Test-only; excluded from production count and unchanged. |

No new worker was introduced in M4. The audit found no measured duplicate, polling, detached-lifetime,
shutdown, cancellation, or stale-result defect beyond the worker fixes already completed in M2/M3,
so benign bounded/owned workers were intentionally left alone.

Verification completed successfully: `cargo fmt --all --check`; `cargo check`; the 5-test repaint
policy/aggregation selection; the 6-test dashboard registry/diagnostics selection; and 72 visibility,
timer, stopwatch, toast, File Search, and MkMacro integration tests. A release binary was built for
three repeated idle traces. `git diff --check` is recorded after final ledger cleanup.

### Rejected milestone 4 optimizations

- A global scheduler, Tokio runtime, Desktop Interaction service, or MkMacro decomposition: no
  measured issue required the architectural scope.
- Replacing well-behaved bounded workers solely to reduce spawn call count: the audit found existing
  cancellation/single-flight/generation ownership sufficient.
- Reporting process CPU percentages: the host did not provide a stable isolated sampler and the
  application shares OS/graphics activity; repaint cadence is the credible evidence collected.

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

## Milestone 5 Cargo/Nextest results

### Method and measured split

Measurements used the final M4 runtime state on the baseline host. Source-invalidated before and
after `cargo test --no-run --timings` runs used the shared warm dependency cache; Cargo timing HTML
was parsed only for supported unit/section data. Integration-unit durations overlap under parallel
Cargo scheduling and therefore are reported as aggregate linker-work evidence, not wall time. A clean
run used a new `target/m5-isolated-clean` directory rather than deleting the shared cache. The clean
run compiled dependencies and reached application/test linking, but after roughly ten minutes it
filled the disk and failed with MSVC `LNK1318 Unexpected PDB error; FILE_SYSTEM`. The isolated output
was then validated and removed, restoring disk space. No successful clean-build duration is claimed.

Representative edits changed only file timestamps. GUI, MkMacro, and command-handler measurements
ran three `cargo check` iterations each; unit and integration measurements ran three targeted
`cargo test --no-run` iterations each. Cached Nextest inventory was measured three times. Execution timing uses three comparable passing
candidate runs plus one authoritative final-topology run. All medians are the middle of three runs.

| Metric | Before | Final | Evidence / interpretation |
|---|---:|---:|---|
| All Rust test binaries | 127 | 70 | 124 top-level integration targets became 67 explicit targets; package lib/main/auxiliary binaries account for the other three. |
| Integration targets represented in Cargo timing units | 121 | 64 in the measured candidate | `mock_ctx.rs` no longer links as an empty executable. The final topology has 67 integration targets after three MkMacro suites were re-isolated from ordinary-harness contention. |
| Source-invalidated `cargo test --no-run --timings` | 11m52s | 9m03s measured 64-target candidate | Same warm-cache host, 23.7% candidate wall-time reduction; final 67-target Cargo timing was not rerun after the disk/PDB failure. The earlier M1 baseline was 11m59s. |
| Aggregate integration timing-unit work | 5,310.06 s | 3,767.68 s | 29.0% less aggregate work; units overlap in wall time. |
| Median integration timing unit | 46.58 s | 55.69 s | Expected per-umbrella cost increase; total fan-out still falls materially. |
| Nextest build for inventory after topology invalidation | 12m31s | 8m39s | Authoritative final 70-binary topology, 31.0% wall-time reduction. |
| Cached Nextest inventory/scheduling | not comparably repeated | 2.779 s median | 3.155/2.731/2.779 s total command times; Cargo itself reported 1.34/1.00/0.99 s. |
| Full Nextest execution | 86.812 s, 2,960 executed | 42.730 s final, 2,976 executed | Comparable 67-binary candidate runs were 37.616/42.181/44.246 s (42.181 s median); the authoritative 70-binary run was 42.730 s. Test count and host load changed since M1, so this is regression evidence rather than a topology-only attribution. |
| GUI timestamp-only `cargo check` | 6.50 s M1 single run | 11.440 s median | 11.440/11.341/13.504 s; host variance prevents an optimization claim. |
| MkMacro timestamp-only `cargo check` | 6.44 s M1 single run | 17.811 s median | 14.097/17.811/25.569 s; the M1 file differed, so this is final-state characterization only. |
| Command-handler timestamp-only `cargo check` | not measured | 13.717 s median | 17.242/9.149/13.717 s. |
| Unit-test-only rebuild/link | not measured | 32.186 s median | 50.441/32.186/23.858 s after touching `src/file_search/test_fixtures.rs`. |
| Umbrella integration-test rebuild/link | not measured | 39.958 s median | 48.043/26.884/39.958 s after touching one source in `plugin_queries`. |
| Incremental release build | 2m05s M1 | 2m40s | Final source-invalidated `cargo build --release`; no release-profile change was made. |

The measured 64-target candidate exposed a 22.90 s normal-library unit with 13.44 s frontend and 9.46 s codegen in the successful
after timing report. Test units did not expose finer sections on this stable toolchain; its 64
reported integration durations include frontend/codegen/link. The longest was the `domain` umbrella at 125.44 s,
followed by existing isolated plugin/platform tests at roughly 78-94 s. This is why the integration
aggregate is not mislabeled as pure linker CPU time.

### Test topology and isolation

Cargo automatic test discovery covers every top-level stateful test. `plugin_queries` groups simple,
side-effect-free plugin parsing/search tests under `tests/plugin_cases`; `domain` groups pure
configuration, parsing, diff/search algorithms, model, and fake-backend tests under
`tests/domain_cases`. Normal Nextest parallelism remains enabled. No assertions or meaningful tests
were removed. The post-review inventory lists 2,984 tests; the final 70-binary suite runs 2,977 with
7 skipped.

Sixty-five stateful files stay as independent integration binaries. The isolation audit retained
separate processes for current-directory and environment mutation; shared stores and files; global
execution/event/window hooks and static overrides; Windows hotkey, mouse, UI Automation, screenshot,
and window state; spawned commands and external processes; watcher timing; audio threads; and
launcher/GUI lifecycles. Representative protected groups include bookmarks/favorites/folders,
clipboard/history/notes/snippets/todo/tempfile/timer stores, visibility/hotkey/window-manager tests,
MkMacro authoring/store/launcher integration, mouse-gesture service/database/UI, shell/recycle/sound,
and watcher/screenshot tests. Combining these under the standard Rust harness would allow parallel
tests to share process globals, so a larger umbrella or one giant target was rejected.

`mock_ctx.rs` is retained under `tests/support` solely through `#[path]` by visibility tests and does
not build as an empty standalone executable. A separate mock-context crate was rejected: its
consumers are already isolated and the tiny helper has no independent dependencies or measured
compilation bottleneck.

### Slow tests, logging defect, and feature/profile audit

A full 67-binary candidate `--status-level slow --final-status-level slow` run reported no tests above Nextest's slow
threshold and completed in 44.795 s. Existing watcher/process/timeout tests were left realistic;
there was no deterministic seam that justified replacing their waits, and execution was dominated
by process startup/concurrency rather than a small set of slow bodies.

The earlier `logging::writes_log_file` failure was legitimate. The host's `RUST_LOG=warn` suppressed
its `info!` event, the test slept for 100 ms instead of owning a flush boundary, and both tests tried
to install a process-global subscriber when run by the ordinary Rust harness. Logging initialization
now returns its `WorkerGuard`; `main` retains it for application lifetime and tests drop it to flush.
Each logging case executes its initialization in a fresh child test process and removes only the
ambient `RUST_LOG`, preserving the two original test identities and behavior. Focused logging tests
and the final full suite pass.

`cargo tree` and `cargo tree -e features --depth 2` were audited against source usage. Reqwest already
uses only blocking Rustls; Syntect already disables defaults and selects syntax/theme/parsing/fancy
regex; image already selects PNG/JPEG/BMP; Rodio already selects only WAV. Eframe/Winit, screenshots,
RFD, Rdev, Sysinfo, and the requested Windows API features all have direct GUI/platform/runtime
consumers. RFD/Eframe/Sysinfo defaults include cross-platform or broader flags, but target gating and
lack of an isolated measured win did not justify compatibility risk. No dependency or feature was
changed. Test-profile debug information was retained; machine-specific linker/job configuration and
a global incremental/profile workaround were rejected to preserve full diagnostics and portability.

### Milestone 5 verification

Successful commands: `cargo fmt --all --check`; `cargo check` (11.29 s); focused logging Nextest
(2 passed); `cargo nextest list` (2,983 tests, 70 binaries); an authoritative final full Nextest run (2,976
passed, 7 skipped), plus three comparable 67-binary candidate runs and a slow-status pass; `cargo build --release` (2m40s); `cargo tree --depth 1`; and `cargo tree -e features --depth 2`. `git diff --check` is recorded after the final
ledger edit. Both umbrellas also passed under ordinary `cargo test` (100 domain and 111 plugin-query tests). The required clean isolated attempt and two subsequent no-run attempts are explicitly
recorded as failed due to the temporary disk-exhaustion/PDB condition rather than claimed as passes.

## Independent-review remediation

**Status:** `complete`

The independent review found eight substantive gaps. System Controls, Tempfiles, Layouts,
Scratchpad, and Command History now send automatic refresh/load/save work through widget-owned,
capacity-one workers. Requests are single-flight, completed snapshots wake egui, and dropping a
widget closes and joins its worker. A deterministic blocked-loader test proves request submission
does not wait for the provider and that duplicate work is coalesced. A release fixture containing
all five widgets reached the first usable frame in 481.484 ms; this is one warm-host regression
sample, not a general maximum-frame-time claim.

Browser Tabs now owns an injectable capacity-one worker and immutable cache instead of detached
threads and a process-global snapshot. Publication increments `PluginSearchUpdates` and invokes its
repaint callback, so a cached launcher query reruns after discovery. Channel-controlled tests cover
nonblocking cold search, single-flight refresh, publication generation/repaint, populated filtering,
and owned shutdown without sleeps. Windows UI Automation's `FindAll` API exposes no hard timeout or
cancellation handle: shutdown is deterministic after an in-progress call returns, but cannot
preempt a UIA call that is itself hung. Criterion keeps native discovery disabled only in its
injected fixture and measures 1,000-tab cached filtering/materialization separately from the static
clear command: 203.04 us and 1.428 us point estimates respectively (20 samples, one-second warmup,
two-second measurement).

Shared system-data readiness is now `Option`-backed. Cold Processes, SysInfo, and named-process
Volume searches request refresh but emit no fabricated data; last-good snapshots remain available
after publication. Channel-controlled coverage exercises cold state, publication,
generation/repaint notification, and refreshed reads. Dashboard `All` refresh creates one
`System` value for both process actions and system status instead of enumerating twice, and the
obsolete self-enumerating dashboard helper was removed. Calendar is explicitly slow-periodic while
focused and becomes event-driven when unfocused-work reduction applies; deterministic policy
coverage asserts both transitions.

Cargo automatic integration-test discovery is restored. The two pure umbrellas remain explicit,
with source modules under `tests/domain_cases` and `tests/plugin_cases`; the shared mock context is
under `tests/support`. Stateful top-level tests are auto-discovered, no targets are duplicated, and
the final inventory lists 2,984 tests across 70 binaries (the previous 2,983 plus the new
background-loader lifecycle test).

Final-HEAD warm startup measurement from an isolated fixture: plugin manager 60.419 ms, plugin
registration 17.665 ms, `LauncherApp` construction 74.782 ms, first usable frame 393.764 ms, and
dashboard initial publication 496.153 ms. These values include the final system/IP/Browser Tabs
worker construction and supersede the post-Milestone-2-only evidence for final-state regression
assessment.

Final remediation verification passed: `cargo fmt --all --check`; `cargo check`; focused Browser
Tabs, system-data, dashboard-cache, widget, repaint-policy, and SysInfo tests; `cargo nextest list`
(2,984 tests, 70 binaries); `cargo nextest run --no-fail-fast` (2,977 passed, 7 skipped in 40.776 s
after a 7m39s rebuild); corrected Browser Tabs Criterion workloads; `cargo build --release`
(2m05s); `git diff --check`; and stale-path searches. The first full-run attempt stopped during
test compilation on a moved test-only `Arc`; the ownership was corrected and the authoritative
rerun passed completely.

### Second independent-review remediation

**Status:** `complete`

The follow-up review identified six remaining boundary and race issues. Browser Tabs now records
the last filter for which `recalc_each_query` forced discovery, so launcher notification requery and
Plugin Home rendering of the same actual query consume the published snapshot without starting a
discovery loop. A channel-controlled enumeration-count test covers repeated same-filter queries and
the next distinct filter.

Dashboard Volume collection and Windows enumeration now run outside widget rendering. Volume uses
the shared capacity-one widget loader; the Windows plugin owns a capacity-one enumeration worker,
publishes immutable snapshots, and signals `PluginSearchUpdates`. Browser Tabs, Query List, Pinned
Query Results, Window List, and Windows Overview observe that shared generation and immediately
invalidate their local materialized-result caches after asynchronous publication. The remaining
widget plugin-search calls are static command materialization or read caches/off-thread providers.

Scratchpad load/save results carry their storage path and stale results are ignored after a settings
path change; a controlled path-change/edit race proves an old load cannot overwrite current text.
Background refresh submission now advances scheduling state immediately, while an explicit request
that arrives in flight remains pending for one follow-up. Deterministic tests cover both combined
scheduler/loader behavior and generation invalidation.

Dropping an active widget loader, Browser Tabs cache, or Windows cache no longer joins provider work
on the egui thread; a small reaper owns the join. Production widget providers are bounded, including
the power-plan query, which kills `powercfg /L` after three seconds. Windows UI Automation remains
the sole hard limitation: `FindAll` exposes no cancellation/timeout handle, so a hung OS call cannot
be preempted, but plugin removal itself remains nonblocking and only one owned call can be active.
Channel-controlled coverage proves an active generic loader can be dropped without waiting and that
blocked Windows enumeration is nonblocking, single-flight, and notifier-driven.

Final-HEAD verification passed: `cargo fmt --all --check`; `cargo check`; focused Browser Tabs,
Windows, scratchpad, loader/scheduler and generation-invalidation tests; `cargo nextest list`
(2,990 inventoried tests across 70 binaries); `cargo nextest run --no-fail-fast` (2,983 passed,
7 skipped in 46.790 s after a 5m36s rebuild); `cargo build --release` (2m04s); render-path and
stale-reference searches; and `git diff --check`. The corrected Criterion workloads measured the
1,000-tab populated cached filter/materialization path at 214.47 us and the separately static clear
command at 1.164 us (100 samples, default warmup).

The final isolated warm release sample measured plugin manager construction at 58.016 ms, plugin
registration at 17.111 ms, `LauncherApp` construction at 36.685 ms, first usable frame at
352.856 ms, and dashboard initial publication at 517.348 ms. This single warm-host sample supersedes
the earlier remediation sample for final-HEAD regression evidence; it is not a cold-start claim.

### Third independent-review remediation

**Status:** `complete`

Browser Tabs now remembers a bounded set of distinct forced filters instead of toggling one last
filter. Launcher requery and alternating Plugin Home/dashboard consumers therefore stabilize after
each filter's first discovery rather than creating a notification loop. Plugin search publications
also carry a provider source generation. Browser Tabs, Windows, pinned engine widgets, and shared
system-data consumers observe only their own source; manual Query List mode ignores unrelated global
publications. Plugin Home caches materialized results by plugin, mode, query, and source generation,
with clipboard/layout/shell mutation versions used for their file-backed sources.

The Layouts worker snapshot now includes the complete `LayoutStore` and config-file existence state.
Automatic active-layout selection and all subsequent widget mutations use that published store;
`layouts.json` is opened only by the background load operation rather than during rendering.

Browser Tabs and Windows workers recheck shutdown after the provider returns and before mutating a
snapshot or notifying consumers. Channel-controlled drop-while-blocked tests release the provider
after UI ownership is gone and prove no generation or repaint is published. Production Browser UIA
and Windows enumeration also use process-wide single-flight guards, preventing plugin reloads from
accumulating concurrent OS enumerations. UIA remains unpreemptible when an individual Windows call
itself hangs; the guard deliberately prevents replacement instances from starting another call.

Final verification passed: `cargo fmt --all --check`; `cargo check`; focused notifier, manual
refresh, Plugin Home, Layouts, Browser Tabs, Windows, Shell, and Windows integration tests;
`cargo nextest list` (2,997 tests across 70 binaries); `cargo nextest run --no-fail-fast`
(2,990 passed, 7 skipped in 43.703 s); corrected Browser Tabs Criterion workloads; `cargo build
--release` (2m00s); stale render-I/O/reference searches; and `git diff --check`. Criterion measured
the populated 1,000-tab cached filter/materialization path at 206.35 us and the separate static clear
command at 1.0965 us.

The final isolated warm release sample measured plugin manager construction at 56.529 ms, plugin
registration at 16.617 ms, `LauncherApp` construction at 60.327 ms, first usable frame at
372.122 ms, and dashboard initial publication at 549.592 ms. This is a single warm-host regression
sample, not a cold-start or maximum-frame-time claim.

### Fourth independent-review remediation

**Status:** `complete`

Layouts now uses a typed, generation-tagged operation queue for refresh, save, duplicate, rename,
import read/write, and export. Every mutation reloads the current on-disk store in the worker before
merging and saving, so command or external edits are preserved; monotonically applied results prevent
an older refresh snapshot from replacing a completed mutation. Plugin Home now submits owned plugin
handles to a capacity-one background loader and caches results by source plus a two-second TTL. Cold
rendering never invokes plugin search, mutable plugins without explicit generations cannot remain
permanently stale, and dynamic-library guards travel with outstanding plugin work.

Manual Browser Tabs, Windows, Windows Overview, and pinned-query widgets record the source generation
at an explicit refresh and consume exactly one later publication to materialize the completed
snapshot. Publications without an outstanding request, including unrelated or later same-source
work, do not arm manual refresh. Browser Tabs forced discovery is now coalesced by a fixed two-second
cooldown rather than remembered per filter, so more than 32 simultaneous query consumers cannot
thrash discovery.

Browser Tabs, Windows, public-IP, and shared-system workers serialize shutdown with the complete
snapshot/publication/notification transaction through a dedicated lifecycle mutex. Snapshot locks
are released before repaint callbacks, avoiding reentrant deadlock, while Drop either observes the
committed notification or suppresses the publication before returning. UI Automation remains
unpreemptible inside an individual Windows call; process-wide single-flight still bounds it to one
active production enumeration.

Final verification passed: focused deterministic tests; `cargo fmt --all --check`; `cargo check`;
`cargo nextest run --no-fail-fast` (2,994 passed, 7 skipped, 3,001 total across 70 binaries in
37.877 s); the Browser Tabs Criterion benchmark; `cargo build --release` (2m05s); stale render-I/O
searches; and `git diff --check`. Criterion measured the populated 1,000-tab cached filter path at
193.51–203.11 us and the static clear command at 1.0779–1.1231 us, with no statistically detected
regression.

### Fifth independent-review remediation

**Status:** `complete`

Plugin ownership now uses one slot per plugin, with the originating dynamic-library handle stored only
on that slot (built-ins store no library handle) and a monotonically increasing instance epoch.
Plugin Home includes that epoch in request/result identity and replaces its background executor and
cache when an instance changes. A blocked old instance therefore cannot populate or block the
replacement instance, while unrelated DLLs are no longer retained by every outstanding plugin
handle. Plugin settings and mouse-gesture settings use nonblocking write acquisition and show a
clear busy diagnostic instead of waiting behind Plugin Home's background read.

Layouts now bounds its typed operation queue at 32 entries, preferentially discards a redundant
queued refresh under pressure, and records the latest submitted UI mutation generation. Older FIFO
results cannot roll back the visible snapshot or status while a newer edit is queued; success and
error status is published only from the worker result. Every mutation still reloads and merges the
current store at execution time.

The internal built-in refresh service now assigns exact tickets to Browser Tabs, Windows, public-IP,
and shared-system refreshes. Search callers observe whether they scheduled or joined current work;
dedicated manual widgets and Query List await only those exact tickets, consume completion once, and
perform no follow-up for a fresh cache. Query List derives relevant asynchronous sources from the
routed command head. Superseded tickets cannot publish, increment generation, or repaint, preventing
stale instance work from satisfying current consumers. The public Plugin trait and dynamic-plugin ABI
remain unchanged.

Verification passed: focused deterministic settings-frame, Plugin Home epoch-reload, Layouts FIFO,
ticket join/fresh/unrelated/stale-publication, and widget-consumption tests; `cargo fmt --all -- --check`;
`cargo check`; `cargo nextest run --no-fail-fast` (3,004 passed, 7 skipped, 3,011 total across
70 binaries in 41.987 s after compilation); `cargo build --release` (2m07s); the Browser Tabs
Criterion benchmark; stale-path/render-I/O searches; and `git diff --check`. Criterion measured the
populated 1,000-tab cached filter path at 199.06–213.36 us and the separate static clear command at
1.1047–1.1414 us, with no statistically detected regression.

### Sixth independent-review remediation

**Status:** `complete`

Built-in refresh tickets now live in one mutex-protected per-source registry. Scheduling atomically
returns either a new ticket or the exact joined ticket; provider searches return those transaction
tickets directly through an internal capture boundary instead of looking up active state after
search. Publication, cancellation, resolution, and supersession use the same state. Channel-send
failure, provider shutdown, plugin reload/drop, and supersession resolve waiters and notify repaint,
while stale workers cannot publish. The explicit `tab:cache` rebuild now participates in the same
ticket and notification protocol. Query List receives tickets directly from the routed searches.

Layouts classifies results as snapshot, mutation, import-read, or export. Only snapshot-bearing
results participate in mutation revision suppression; export and import-read success/errors remain
independently deliverable in heterogeneous FIFO sequences. Cancelling the export dialog returns an
explicit not-queued outcome, so the UI never reports a queued export that does not exist.

Plugin Home carries both plugin-instance and widget-configuration epochs. Configuration changes move
at most one active loader into one retiring slot and immediately provide a replacement loader; a
second outstanding retirement is represented as a visible pending state rather than spawning more
workers. Old results are discarded before current configuration materialization. Dynamic plugin
slots record their originating DLL path, and same-path reload is deferred while an owned handle pins
that slot. Settings displays the deferred path and retry guidance. This bounds retained DLLs and
worker/reaper ownership to one active plus one retiring operation per widget. Windows UI Automation
still cannot be preempted inside an individual OS call.

Verification passed: deterministic completion-before-return, disconnected-send, joined cancellation,
shutdown/reload, supersession, explicit tab-cache notification, heterogeneous Layouts FIFO, blocked
`on_config_updated`, repeated retirement, and DLL deferral tests; `cargo fmt --all -- --check`;
`cargo check`; `cargo nextest run --no-fail-fast` (3,011 passed, 7 skipped, 3,018 total across
70 binaries in 40.959 s after compilation); `cargo build --release` (2m11s); Browser Tabs
Criterion; stale-path audits; and `git diff --check`. Criterion measured the populated 1,000-tab
cached filter path at 203.43–213.80 us and the separate clear command at 1.1250–1.1763 us, with no
statistically detected regression.

### Seventh independent-review remediation

**Status:** `complete`

Browser Tabs, Windows, Public IP, and shared system-data workers now wrap their complete worker
lifetime with serialized terminal cleanup. Provider panics are contained, the exact active ticket is
cancelled, local in-flight state is cleared, and stale publication remains excluded by the existing
publication lock. A disconnected request channel marks that provider runtime terminal, so repeated
search frames neither allocate new tickets nor repeatedly notify repaint. Public IP shutdown now
uses the same bounded owned-reaper pattern as the other UI-facing providers, allowing plugin drop to
return before an active bounded HTTP lookup completes.

`BackgroundLoader` now distinguishes an empty result queue from permanent worker disconnection,
clears `in_flight`, and exposes a one-shot failure. Plugin Home reports the failure, replaces a
failed active executor, retires failed old executors, and preserves the one-active/one-retiring
ownership bound while allowing a reloaded plugin instance to proceed.

Pinned Commands settings no longer calls `load_favs` from an egui settings frame. The settings
context carries the already-owned dashboard favorites snapshot, and action-choice materialization is
pure over that snapshot.

Verification passed: 12 focused deterministic panic, disconnected-channel, nonblocking-drop,
Plugin Home recovery, and settings-snapshot tests; `cargo fmt --all -- --check`; `cargo check`;
`cargo nextest run --no-fail-fast` (3,021 passed, 7 skipped, 3,028 total across 70 binaries in
44.006 s after compilation); `cargo build --release` (2m19s); render-I/O and stale-state audits; and
`git diff --check`.

### Rejected milestone 5 optimizations

- One giant integration target or consolidation of stateful tests: rejected because ordinary
  `cargo test` runs tests from one binary in a shared process and may run them concurrently.
- Standalone `mock_ctx` crate: rejected because removing its accidental empty binary captures the
  benefit without another crate boundary.
- Dependency/feature churn: rejected where direct consumers exist and no isolated measured win beat
  compatibility and maintenance risk.
- Reduced test debug info, machine-specific linker selection, or committed job limits: rejected to
  preserve diagnostic/backtrace capability and repository portability.
- Replacing sleeps in watcher/process/timeout coverage: rejected because the slow-status run found no
  slow-body concentration and no deterministic seam justified weakening integration realism.

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
| 3 | `544c4c6` | `perf(search): remove blocking dynamic work from query handling` |
| 4 | `9885f20` | `perf(runtime): reduce unnecessary idle and repaint work` |
| 5 | `707459e` | `perf(dev): improve cargo and nextest iteration time` |
| Review remediation 1 | `e6f8baf` | `fix(perf): resolve final review findings` |
| Review remediation 2 | `4596f72` | `fix(perf): harden async refresh lifecycles` |
| Review remediation 3 | `bb10a3e` | `fix(perf): stabilize async cache invalidation` |
| Review remediation 4 | `7c0daf3` | `fix(perf): harden async cache ownership` |
| Review remediation 5 | `6b63ea7` | `fix(perf): make async completion request-specific` |
| Review remediation 6 | `5116bbc` | `fix(perf): close async ticket and reload races` |
