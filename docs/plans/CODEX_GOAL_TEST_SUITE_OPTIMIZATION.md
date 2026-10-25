# CODEX GOAL — Test Suite Optimization

**Project:** Multi Launcher (Rust / egui / Windows)  
**Work type:** Test infrastructure, test-only performance and reliability; no product feature changes  
**Authority:** This Goal plus the live checkout's `AGENTS.md`; current code and actual measurements supersede historical records  
**Recommended branch:** a **fresh, unique** `perf/test-suite-optimization`-style branch  
**Target outcomes:** selected warm test build **≥20% faster**; full-suite execution **≥25% faster** (historical ~264 s ⇒ aspirational ~198 s); **all meaningful assertions and coverage preserved**. These are *targets*, not guarantees or reasons to weaken tests.

## A0. Mandatory Git isolation — M0-A, BEFORE any application work

This rule applies independently of the Kickstart. Codex **must** do all of the following **before implementation edits, code builds, test runs, or performance experiments**:

1. **Read-only inspection first:** read `AGENTS.md`, this Goal and the Kickstart. Inspect `git status -sb`, working-tree changes, current branch/HEAD, remote/upstream and actual default integration branch; read-only source inspection is allowed.
2. **Choose a new, unique, task-specific branch** using lowercase kebab-case and an appropriate prefix (`perf/test-suite-optimization`, or a meaningful suffixed variant if it exists). Do **not** reuse a previous task branch merely because it is active.
3. **Safely choose a base:** prefer the newest default integration branch (`master`/`main`, whichever actually exists), using a non-destructive `git fetch` where possible. Do not reset, hard-pull, stash, rebase, merge, force-checkout or delete user changes. If dirty files make moving to that base unsafe, preserve them and create a new branch from the current HEAD if safe; document that base. If no safe branch transition is possible, report a real blocker.
4. **Create and switch** to the fresh branch; verify it is active. Before any implementation, create **its own M0-A baseline commit**. Prefer a minimal task-specific execution ledger if appropriate; otherwise `git commit --allow-empty -m "chore(test-perf): [M0-A] establish test optimization baseline"` is valid. Stage **no unrelated pre-existing changes**. Record original base SHA, M0-A baseline SHA and initial dirty-file inventory.
5. **Attempt an ordinary initial upstream push once**, e.g. `git push -u origin HEAD`. If the push is **denied**, unavailable or approval-blocked, record the exact status, retain local branch/commit and **continue locally without blocking**; no retry loops or bypasses.
6. Perform **all later tests, edits, checkpoint commits and best-effort normal pushes on this new branch**. Do not switch back mid-task, rewrite history, force push, or use an older-branch commit as this task's baseline.

**M0-A acceptance:** fresh branch active; initial commit on it; base and baseline SHAs logged; initial push actually attempted (success/failure recorded); pre-existing work untouched. **Commit:** `chore(test-perf): [M0-A] establish test optimization baseline`.

## A. Objective and user-visible result

Make day-to-day Multi Launcher Rust test iteration meaningfully faster and more reliable while preserving the behavior that the tests protect. Separate three independent bottlenecks: **Cargo compilation/linking**, **individual test runtime**, and **parallel full-suite contention/fixture flakiness**. Deliver a convenient, non-destructive PowerShell workflow for selecting tests, profiling timings and comparing results. Optimize the *proven* expensive work, not merely test source line count.

A developer should be able to run a small impacted target quickly, see progress and final status, and inspect machine-readable timing receipts without launching a full suite every time. Production startup, GUI, hotkeys, settings, plugins, persisted formats and shipped binary/profile behavior **must not change**.

## B. Codebase findings from source inspection

The following **specific** components were identified during source inspection. Recheck paths and implementation details in the live checkout after M0-A rather than treating line numbers as fixed.

| Area | Actual existing ownership and implications |
|---|---|
| Cargo targets | `Cargo.toml`: Rust edition 2024, explicit `multi_launcher` binary, library, `autobins = false`, many feature-gated smoke/perf binaries (`dev-smoke-binaries`, `perf-harness`), **67 top-level `tests/*.rs` integration source files** (Cargo autodiscovers these), and explicit `[[test]]` suites named `domain` and `plugin_queries`. Avoid turning on unrelated feature-gated binaries during ordinary tests. |
| Existing consolidated targets | `tests/suites/domain.rs` and `tests/suites/plugin_queries.rs` use `#[path = "../..._cases/...rs"] mod ...;` to consolidate side-effect-free test modules. Reuse this convention for a *small measured candidate*, not wholesale regrouping. |
| Profiles | `Cargo.toml`: `[profile.fast-dev]` inherits `dev` with `debug = "line-tables-only"`; `[profile.iteration]` is release-like and **not** the normal test profile. No application/release profile changes requested. `fast-dev` already gives opt-in savings but has a separate cache. |
| Target selection | `dev/README.md` documents `cargo check --lib`, `cargo nextest run --lib -E ...`, `--test domain`, `--test plugin_queries`, exact test selection, and opt-in `--cargo-profile fast-dev`. It cautions that bare test filters may still compile unrelated targets. |
| Measurement utilities | `dev/track_b/measure.ps1` accepts a native command and argument array, records wall time, exit/success, Git/source/fixture identity and environment, writes to an **external** evidence directory, and preserves caller location/environment. `dev/track_b/cargo.ps1` has fixed presets with restricted forwarded profile args. `dev/perf_track_d/developer_loop.ps1` already demonstrates one preparation, no-op and byte-restored source-edit replays; `dev/perf_track_d/summarize.ps1` aggregates records. **Extend/reuse** instead of introducing another generic Cargo launcher. |
| Process containment | `dev/run_owned_process.ps1` already provides a Windows-only owned-process deadline and cleanup/logging for potentially hung commands. Reuse its verified ownership constraints; do not kill unrelated processes or assume the ordinary Track B measurement runner itself has a timeout. |
| Heavy root-query oracle | `src/gui/render.rs`: `track_c2g_oracle_query_sequences_match_usable_cpu_frames` calls `track_c2g_run_record_with_progress` for **four cases at 100 actions** plus `ViewportControls` at **1,000 and 10,000 actions**. `track_c2g_run_frame` performs actual root-frame rendering **and separately** invokes `track_c2g_validate_frame`. The validator reconstructs an independent eager egui list via `track_c_eager_root_list_layout` and checks **all geometry rows** using `assert_root_list_cache_matches_eager`, as well as selected indices, displayed rows, popup owner, action/result equality, state IDs and frame receipts. **Do not remove the independent oracle or the 10,000-action coverage.** |
| Other expensive candidates | `src/gui/watch.rs` contains `track_c_oracle_c3c_frame_recipes_preserve_delivery_and_safety` and event/queue fixtures; this is *not necessarily* a bottleneck until measured. `src/main.rs`, `src/gui/render.rs` and `src/lib.rs` contain numerous in-module tests; moving them carelessly may affect private access and compile cost. |
| Concurrency/fixture examples | `tests/mouse_gestures_service.rs`, `tests/xml_engine_gate.rs`, `src/mkmacro/runtime.rs`, `src/mkmacro/recorder_observer.rs`, and some GUI module tests use waits, threads, temporary paths or global/Windows state. Some use `serial_test`; do not assume a sleep or a serialized test is unnecessary. |
| Historical execution evidence | `docs/plans/POST_QUEUE_NATIVE_ACCEPTANCE_RESULTS.md` reports one earlier 71-binary default-feature full run of **263.841 s**, **6,965 passed / 3 failed / 1 timed out / 48 skipped**, at an **earlier commit and with a specific temporary Nextest configuration**. Its root-query oracle passed standalone in **41.957 s** but timed out at **60.063 s** amid the full suite; the report attributes roughly **25.4 s to validation** versus **4.2 s to rendering** for the 10,000-action portion in a focused run. These are *historical leads*, **not a current comparable baseline**. Prior failures (file-operation restricted directory, radial editor list, universal action catalog) remain unclassified at the current SHA. |
| Instruction authority | `AGENTS.md` requires scoped work, no unnecessary full testing, bounded reviews and an active checkpoint cadence. Historical acceptance documents are reference information, **not inherited task gates**. |

**Hypothesis to test:** the eager oracle's repeated 10,000-row layout work and fixture reconstruction dominate its test time. Possible opportunities include reusing an independent deterministic expected-layout snapshot across frames where inputs are **provably identical**, or caching immutable fixture computation. Such changes require explicit invalidation checks, equality of independent oracle results, and preservation of all frame-specific assertions. Do not blindly cache mutable egui context or replace the eager oracle with the production geometry cache (circular verification).

## C. Approved requirements and measurable contracts

1. **Zero application behavior changes.** Edits belong primarily to tests, test-only (`#[cfg(test)]`) helpers, `dev/` tooling and relevant test-target organization. Any production-path edit requires demonstrable necessity and a separate parity review; otherwise exclude it.
2. **All meaningful cases and assertions remain.** No lowering 10,000 to 1,000, removing scenarios/frames, lowering parameter coverage, replacing behavioral checks with snapshots-only, `#[ignore]`, `--skip`, retries, larger timeout, lower parallelism, disabled debug assertions or conditional bypass *to manufacture a pass or speedup*.
3. **Opt-in tooling.** New profiling/fast-run workflow must not change default `cargo test`, `cargo nextest`, ordinary `dev`, canonical `release`, distributed binaries, persisted settings, or user GUI.
4. **Machine-readable evidence.** User authorizes a dedicated profiling harness. For each measurement capture exit status, target/filter, command, profile/features, source SHA/dirty state, tools/host, wall time, compile-versus-execute phase, per-case timing where available, timeout/failure details and selected test inventory. Missing fields must be explicitly `null`/unknown, not invented.
5. **Controlled measurements.** Same host/toolchain/settings/cache state/features/test set, stable test and compiler parallelism, comparable build invalidation, no simultaneous Cargo compilations. Time a compiled test-only run separately from a `--no-run` build. Distinguish cold artifact population, warm no-op, warm edit rebuild, targeted test execution and full-suite execution; do not pool unlike samples.
6. **Efficient normal workflow.** One documented, simple PowerShell command for small target-based test runs and one for producing/summarizing profiling receipts. Console progress must show which phase/target is active and preserve original Nextest live progress when appropriate; no false progress percentages during compilation.
7. **Reliability safeguards.** Deadline-based waits must use deterministic events/condition variables/channels when feasible, monotonic deadlines otherwise, guaranteed teardown and no process/environment interference. Expected error/timeout assertions must remain.
8. **Success targets (aspirational):** ≥20% reduction in selected *equivalent* warm test-build wall time; ≥25% reduction in equivalent full execution (historic 263.841 s ⇒ ~197.9 s). Count stability and assertion parity are **hard gates**; speed targets are measured objectives, **not permission to keep a regression**. Report missed targets honestly.
9. **History and failures:** retain per-test pass/fail/timeout/ignored classification and historical receipts. A test passing in isolation does **not** cancel a full-suite failure. Do not assert an unrelated existing failure is “pre-existing” without same-SHA evidence.

## D. Scope, non-goals and behavioral invariants

**In scope:** a small Windows PowerShell profiling workflow, test selection documentation, data-driven improvements to the expensive oracle, a bounded set of measured slow tests and fixtures, *one* compile-target consolidation experiment, narrow deterministic synchronization fixes, and before/after reporting.

**Out of scope:** changing user commands, GUI/hotkeys/plugins, Rust application architecture, dependency upgrades, release tuning, replacing Nextest, rewriting all ~70 test binaries, blanket serialization or parallelism changes, new GUI test harnesses, large-scale test migration, running opt-in ignored performance campaigns, fixing unrelated product defects, deleting tests, manipulating real user files/settings, or Clippy cleanup in vendored crates.

**Preserve:** `dev`/`release` semantics; exact assertions and diagnostic value (including 10k oracle case); Windows process and COM safety; test target access to environment and relative fixtures; GUI fixture state transitions; plugin routing; clipboard/global state isolation; default Nextest behavior and user-specific configuration. Avoid `target` cleanup, environment modification outside a scoped child process or force-killing unnamed processes.

## E. Technical strategy and measurement rules

- **Optimize in this order:** (1) high-cost test-only validation/fixture work, (2) developer selection workflows, (3) reproducible compile/link experiments, (4) proven concurrency/wait issues. This limits churn and yields useful wins even when compile consolidation fails.
- **Proof before change:** use targeted test-name listing and machine timings to establish the current cost and inventory; the historical result is a triage clue only.
- **Harness implementation:** prefer `dev/test_perf/profile.ps1`, `dev/test_perf/summarize.ps1`, `dev/test_perf/README.md` (Codex may choose equivalent idiomatic names) calling existing Track B measurement/owned-process utilities when compatible. Use PowerShell 7 / Windows; construct native `ArgumentList` arrays, never evaluate concatenated shell commands. Emit unique runs under `$env:TEMP/Multi_Launcher/test-perf` (or another verified external evidence location), with compact tracked redacted summaries only. Validate any JSON/JUnit reporter options against installed `cargo nextest run --help`/config documentation **before** using them; do not assume an unsupported flag. A machine-readable per-test reporter (e.g. supported JUnit) is preferable; if parsing stable text, ship parser fixtures/tests and label limitations.
- **Metrics:** source SHA/dirty status; test full name and binary target; feature flags; compiler/Nextest/profile/CPU/OS; intended parallelism; run type (cold/warm/no-op/edited/execution); elapsed wall time; compilation/link evidence if separately observable; test duration and result; timeout/retry facts; skipped/ignored count; process cleanup status. Use stable units and schema version; do not record secrets or full private environment. Distinguish process wall time from pure linker time.
- **Fair samples:** selected warm-build comparisons should use **three comparable samples where cheap**, median as primary and range as dispersion; prepare both sides before measuring, reproduce a controlled, exactly restored edit when necessary, verify original bytes/SHA afterward. One cold sample may be descriptive only; do not deliberately erase a user's target cache. Compare full execution on matched normal parallelism/default-feature configuration, **same test inventory and no hidden exclusions**.
- **Full-suite budget:** no repeated full suite during development. Allow **at most one unfiltered current-baseline full run and one unfiltered post-change run** (both with `--no-fail-fast`, if supported, and an explicit same Nextest profile/config); first confirm build, ownership containment and a practical ~15-minute per-invocation deadline, with separately bounded compilation if needed. If current baseline cannot safely/affordably be run, treat 263.841 s as **historical only** and report overall gain **unmeasured**, not 25% proven. No third full run for chasing failures. Do not quietly increase the 60-second oracle timeout or lower parallelism to avoid an otherwise observed timeout.
- **Targeted budget:** usually one or two exact Nextest slices per checkpoint plus at most one specific repaired failure rerun. One isolated oracle run and at most one adjacent contention slice per relevant change. At most one small target-grouping A/B experiment, reverting it if no clear win or any regression. Failed tests remain visible in results.
- **Exit/cleanup:** each bounded invocation must record actual process exit or explicit timeout/cleanup. Retain raw evidence outside the source worktree; do not mark a timeout as success. Preserve the user's data and unowned processes.

## F. Dependency-ordered milestones and checkpoints

### M0-A — Branch bootstrap (mandatory hard prerequisite)

**Objective:** establish isolated, pushed-if-possible task history. **Ownership:** Git and minimal task ledger only. **Tasks:** execute **A0** exactly, inspect state read-only before any changes, create fresh branch + baseline commit, attempt upstream push, record original base / baseline SHA. **Behavior/edge cases:** maintain pre-existing dirty work even if branching from current HEAD; no destructive checkout. **Regression constraints:** no application/test changes before bootstrap. **Verification:** `git status -sb`, `git branch --show-current`, `git rev-parse HEAD`, log and push result. **Acceptance:** every A0 condition satisfied or a genuinely unsafe Git transition reported. **Commit:** `[M0-A]` baseline first.

### M0-B — Build the minimal profiling harness and test inventory

**Objective:** gather comparable, machine-readable evidence with no product effect.  
**Ownership:** `dev/test_perf/` (new), `dev/track_b/measure.ps1` / `dev/run_owned_process.ps1` **reuse by invocation first**, `Cargo.toml`, `dev/README.md`, existing Nextest configuration only read-only.  
**Tasks:**
1. Discover real `cargo`, `rustc`, `cargo nextest` versions/flags; inventory unfiltered default-feature test targets and cases using supported Nextest listing; count skipped/ignored and record current source SHA.
2. Implement `profile.ps1` with bounded `-Mode` (e.g. `SelectedBuild`, `SelectedRun`, `FullRun`, `Inventory`), target/filter/profile choices, explicit execution receipts, no implicit feature-gated binaries, and per-phase console progress. Reuse Track B for successful bounded build wall timing and the owned runner for bounded executions when safe; if composition would be unsafe, implement the smallest *owned* wrapper without changing those general utilities.
3. Produce versioned structured JSON plus a summary of slowest 10–20 test cases; failure/timeout rows must remain in output, along with source and process-cleanup attribution. Confirm status and strict argument passing; no destructive cleanup. Test parser and timeout accounting with **fake, short-lived owned fixtures** rather than a full suite.
4. Document a paste-friendly PowerShell example resolving `Cargo.toml` from the current repository (or a fixed script-root path), including explicit `--lib` and `--test <name>` targeting, live phase output and read-only summaries. Do not introduce a global Nextest config change.

**Behavior/edge cases:** errors, zero matches, nonzero exits, spaces in paths, interrupted runs, absence of Nextest, unavailable reporter fields. **Regression constraints:** no application code, no user file reads beyond necessary repo inspection, no cache deletion. **Verification:** dry/list and small known exact test; fake success/failure/timeout parser fixture; valid JSON schema and process cleanup. **Acceptance:** one reproducible per-test receipt and one build receipt with honest status and source identity; helpers remain opt-in. **Commit:** `test(perf): [M0-B] add bounded Nextest profiling harness`.

### M0-C — Establish the bounded baseline and bottleneck ranking

**Objective:** know where time is spent, not just how many tests exist.  
**Ownership:** profiler receipts and tracked concise results ledger; no production source.  
**Tasks:** sample selected `--lib --no-run` test build (normal `dev`, and/or controlled opt-in `fast-dev`), selected integration target build, a handful of representative fast exact tests, the root-query oracle standalone, and a full test inventory. Record prepared/warm no-op/controlled edit comparisons separately; use Track D's byte-restored edit pattern. If available and safe, use **one** bounded current default-feature full run after prebuilding and with unchanged normal parallelism; otherwise state why it was skipped and retain the historical 263.841 s only as context. Rank 10–20 slowest by actual observed cost, noting censored/failed timeouts separately.

**Behavior/edge cases:** source may have failures; compute duration for failures as observed, do not discard them; negative/unknown metrics remain unknown. **Regression constraints:** one full baseline max, no global flags/profile changes, no installation/upgrades. **Verification:** source/profile/target equality checks, duplicate-run detection, sorted report, restored edit hashes. **Acceptance:** useful bottleneck list, selected-build metric, source/version/environment evidence, classified full baseline status. **Commit:** `docs(test-perf): [M0-C] record bounded baseline and ranked costs`.

### M1-A — Make everyday selection and progress fast and predictable

**Objective:** developers stop paying broad-build cost for a one-case change.  
**Ownership:** `dev/test_perf/`, `dev/README.md`, optional narrowly scoped `dev/track_b/cargo.ps1` docs (do not relax its safety guard without need).  
**Tasks:** provide stable recipes for `cargo check --lib`, exact `cargo nextest run --lib -E 'test(...)'`, scoped `--test` targets and `--cargo-profile fast-dev`; a simple helper to show build/execute phase and preserve Nextest's native case/progress display. Warn explicitly that filtering **cases** without selecting a **target** may compile unrelated targets, and that a new profile needs an initial cache population.

**Behavior/edge cases:** no matching tests must be surfaced rather than celebrated as a pass; zero progress during build cannot be fabricated; standard exit status propagated. **Regression constraints:** untouched default commands and profiles; no shell alias/global configuration required. **Verification:** exact known lib case, exact known integration case, expected nonzero and no-match fixture. **Acceptance:** user-facing documented commands run only intended targets and show meaningful stages; no slower default dev loop. **Commit:** `docs(test-perf): [M1-A] add focused local test workflows`.

### M1-B — Test runner tuning, strictly opt-in and benchmarked

**Objective:** choose a defensible local baseline profile/concurrency without masking bugs.  
**Ownership:** profiling workflow/options; `Cargo.toml` existing `fast-dev` read-only unless demonstrated need.  
**Tasks:** measure existing `dev` vs `fast-dev` at matching warmed states and representative exact targets; keep the faster configuration optional and report PDB/symbol loss tradeoff. Inspect CPU contention and Nextest job/thread settings but do not lower standard full-run parallelism to make tests pass. Offer a bounded local profile only if an actual comparison supports it; no speculative `[profile.*]` variants or persistent Nextest policy change.

**Behavior/edge cases:** alternate profile's first cache warmup can be slower; mismatch cannot be scored. **Regression constraints:** test semantics and normal execution unaffected. **Verification:** paired equivalent measurements and recorded flags. **Acceptance:** explicit default recommendation and evidence, or reason to keep existing profile unchanged. **Commit:** `chore(test-perf): [M1-B] document measured runner choices` (omit if no tracked delta; report no-change checkpoint).

### M2-A — Optimize the costly C2G query oracle without weakening it

**Objective:** lower expensive **test-only** validation while retaining a genuinely independent oracle.  
**Ownership:** `src/gui/render.rs` `track_c2g_oracle_query_sequences_match_usable_cpu_frames`, `track_c2g_run_frame`, `track_c2g_validate_frame`, `track_c2g_expected_state_for_query`, `track_c_eager_root_list_layout`, `assert_root_list_cache_matches_eager`, fixture signatures and supporting `#[cfg(test)]` helpers.  
**Tasks:**
1. Profile **preparation, actual rendered frame, eager validation, state/hash/serialization, teardown** at 100/1,000/10,000 actions with low-disturbance instrumentation; use established progress markers outside rendering.
2. Isolate redundant immutable fixture work: investigate reusing precomputed expected `Vec<Action>` and/or independent eager-layout/reference geometry for successive frames **only when actual inputs (`actions`, width, egui style, scale, aliases, context-relevant state) are proven equivalent**. Where cache invalidation is complex, prefer a simpler algorithmic improvement to the independent test oracle. Do **not** use the production geometry cache as the expected value.
3. Preserve each original case/step and full action/result equality; verify every expected row geometry (not only visible rows), overscan, selected row, popup ownership, provider supersession, frame state/identity, screen-size variation and 10k cardinality. Maintain mismatch diagnostics. Add focused proof that changed width/scale/content invalidates expected layout and that an intentionally wrong row/popup state is still detected (controlled negative fixture if safe).
4. Compare standalone execution and one small mixed-contention group under normal parallelism. If results only improve isolation and still time out at full load, report contention separately rather than changing the timeout.

**Behavior/edge cases:** mutable egui font cache/styles, changed query/result list, context switching, popup/scroll animation, UTF-8 widths, stale eager reference. **Regression constraints:** `track_c2g_benchmark_gui_query_to_usable_cpu_viewport` stays ignored/opt-in and its measurement semantics remain sound; default user code untouched. **Verification:** exact C2G oracle, one optional adjacent geometry oracle, validator-negative proof, fixture/step/cardinality assertions, before/after timing. **Acceptance:** all original C2G cases and 10k assertions pass, expected representation remains independent, meaningful measured runtime improvement or documented unsuccessful candidate (reverted). **Commit:** `test(gui): [M2-A] reduce independent query oracle overhead`.

### M2-B — Optimize only the next measured expensive tests

**Objective:** capture low-risk runtime wins beyond one test, without broad refactor.  
**Ownership:** **only** the top-ranked tests/modules from M0-C (possible candidates: `src/gui/watch.rs` C3C recipe, GUI test fixtures, `src/mkmacro/*`, `tests/mouse_gestures_service.rs`; these are *candidates*, not a mandatory edit list).  
**Tasks:** select up to **5** concrete expensive test implementations from the measured top 10–20; for each, identify dominant fixture setup, repeated file/JSON creation, redundant loop validation, overexpensive dependent allocation or actual required wait. Share immutable fixtures within a test **only** when isolation and assertions remain equivalent; prefer smaller algorithmic overhead over reducing workload size. Preserve per-case data independence, test names and parallel behavior.

**Behavior/edge cases:** shared mutable state can introduce order dependence; parallel tests must not reuse a single temp location unsafely. **Regression constraints:** no change to production code, test workload/cardinality or user config. **Verification:** one exact pre/post run per owner, summary of change and assertion mapping, randomized/interleaved rerun only if concurrency is relevant. **Acceptance:** each retained edit passes exact tests and has an observable or well-reasoned cost reduction; otherwise revert and note it. **Commit:** `test(perf): [M2-B] streamline measured slow fixtures` (split into `[M2-B1]`/`[M2-B2]` only for independent owners).

### M3-A — Controlled integration-test target consolidation experiment

**Objective:** determine whether fewer small test binaries actually save Windows compile/link time.  
**Ownership:** `Cargo.toml`, `tests/suites/*.rs`, and at most **2–4** low-risk small top-level `tests/*.rs` files chosen from the build profile.  
**Tasks:** inventory Cargo targets and test names first. Using existing `domain`/`plugin_queries` pattern, move a small set of **side-effect-free** standalone targets into a suite under `tests/<subdir>/` using `git mv` and `#[path]` declarations (top-level `tests/*.rs` are auto-discovered: merely including them in a suite would double-run tests). Update any relative fixture paths, target selectors, and docs; keep Cargo autobuild/feature behavior explicit. **Do not** globally disable test autodiscovery or require enumerating every target. Benchmark same set built as standalone versus grouped with controlled warm edit, matching features/profile/cache. Check total executable size and link/build wall time. Retain only if reproducible benefit with no isolation/inventory regression; otherwise revert experiment, preserve benchmark result.

**Behavior/edge cases:** module-path prefix and test-binary identity change Nextest filters, binary-local globals, `env!("CARGO_BIN_EXE_...")`, `include_str!` locations and process isolation. Exclude targets relying on those semantics. **Regression constraints:** no omitted, duplicated or newly ignored test; no loss of independent process isolation for stateful tests; no changed default feature set. **Verification:** before/after test roster identity mapping, exact moved tests, representative unrelated target, controlled compilation samples. **Acceptance:** case count and assertion coverage preserved; measurable warm-build improvement; all moved tests pass; otherwise revert and record no-go. **Commit:** `refactor(tests): [M3-A] consolidate proven safe small targets` **only if accepted** (otherwise `docs(test-perf): [M3-A] record consolidation no-go`).

### M3-B — Reduce *demonstrated* test-only compile overhead (optional bounded follow-on)

**Objective:** address one proven compile/link hotspot not solved by M3-A.  
**Ownership:** only compile-heavy `#[cfg(test)]` support in `src/gui/render.rs`, `src/main.rs`, `src/lib.rs`, or M0-ranked test targets.  
**Tasks:** inspect Cargo timings/compiler-unit evidence for duplicated large test-only helpers, redundant test-target dependencies, or excessive monomorphization. Make **at most one** local extraction/deduplication with no change to public APIs, production build graph, test access to private fields or test discovery. If no obvious low-risk, measured opportunity, **skip this checkpoint**; do not restructure the large GUI owner as a general refactor.

**Behavior/edge cases:** `#[cfg(test)]` differs across library and integration test compilation; a helper moved to integration code may lose private access; shared modules may force more recompilation. **Regression constraints:** application and distribution compilation unchanged. **Verification:** `cargo check --lib`, affected `--no-run`, exact case tests, test listing, post-build timings. **Acceptance:** verified test-only savings without behavior change or checkpoint explicitly skipped. **Commit:** `test(build): [M3-B] reduce measured test-only compilation` if retained.

### M4-A — Fixture isolation, setup and teardown reliability

**Objective:** improve run-to-run reliability and remove avoidable setup cost in **measured** tests.  
**Ownership:** ranked fixture tests and their test-only helpers; candidate locations `tests/mouse_gestures_service.rs`, `tests/xml_engine_gate.rs`, `src/mkmacro/runtime.rs` or affected GUI fixtures if profiling implicates them.  
**Tasks:** investigate shared environment variables, temporary directories, native handles, mock data stores, exclusive input/clipboard/global state and worker lifecycle **only** for failures or expensive setup actually observed. Use RAII/unwind-safe teardown, unique per-case temp roots, explicit ownership and channel/barrier handshakes; use `serial_test` only for a demonstrated shared global invariant, not as a blanket acceleration strategy. Preserve existing negative-path assertions.

**Behavior/edge cases:** panic/timeout/failed worker initialization, Windows COM/thread affinity, server listener still alive on teardown, ownership of external processes, concurrent fixtures. **Regression constraints:** no user-file mutation or test suppression; no production threading changes. **Verification:** exact tests with repeated short runs and one contention slice as justified; assert cleanup/no lingering owned process. **Acceptance:** original behavior still asserted and fixture cleanup deterministic; no new shared mutable global. **Commit:** `test(fixtures): [M4-A] isolate measured concurrent fixtures` (skip if no supported problem).

### M4-B — Replace unnecessary wall-clock sleeps, without false synchronization

**Objective:** eliminate latency or flakes from proven arbitrary waits.  
**Ownership:** only waits identified and costed in M0-C/M4-A (e.g. test wait helpers in `tests/mouse_gestures_service.rs`, test code under `src/gui/*`, not the entire app).  
**Tasks:** prefer channel/Condvar/event notification with bounded `Instant` deadlines over blind sleep or polling; when a real OS tick/debounce is behavior under test, retain real time and a bounded wait. Ensure synchronization semantics survive native hooks, COM, UI thread and panic. Test fail-fast paths and teardown. Stop after at most **three** proven wait replacements.

**Behavior/edge cases:** missed notifications, lost wakeups, false ready signals, early cancellation, independent worker backlog. **Regression constraints:** do not remove necessary scheduling windows or fake timers to get speed; no changed production delays. **Verification:** targeted repeated runs under equivalent parallelism, expected timeout negative test, all threads/owned children retired. **Acceptance:** same assertions, stable bounded waits, real execution savings or documented reason to keep a wait. **Commit:** `test(sync): [M4-B] replace proven unnecessary waits` (omit if no retained edit).

### M5-A — Before/after verification and quantitative scorecard

**Objective:** establish honest gains and guard against lost regression protection.  
**Ownership:** profiling harness/results only; no new product implementation.  
**Tasks:** repeat exactly comparable selected-build/edit and oracle runs; use same host/toolchain/profile/feature flags/target selection/cache state and summary method. Compare test roster **name-by-name**, including intentional M3 moved-target mapping, disabled/ignored state, number of cases/steps/fixtures, 10k case, and assertion obligations. Perform **at most one** final default-feature unfiltered full Nextest run, only after selected verification and prebuild; same settings as M0-C full baseline, if available. Capture fail/timeout/skip sets and durations. No full-suite rerun to chase unrelated failure, no secret skipped lists, no changed timeout/parallelism. Calculate:

`improvement_% = 100 * (baseline_seconds - candidate_seconds) / baseline_seconds`

Compute separately for warm selected build and full **execution** excluding compilation; only use equivalent completed baseline/candidate runs. For timeouts, compare classifications and censored durations rather than claiming throughput gains from failure. Record ranges/limitations and aspirational targets versus outcomes.

**Behavior/edge cases:** missing baseline; full run non-green; inherited failures not proven baseline; parallel load noise; invalid comparison across Nextest profiles. **Regression constraints:** don't enlarge timeouts or claim isolated test passes resolve full failures. **Verification:** JSON schema/reports validated, exact tests green where relevant, classification diff reviewed. **Acceptance:** reproducible before/after table or explicit *unmeasured* fields, no silent loss of coverage, all task-related tests pass, evidence remains traceable. **Commit:** `docs(test-perf): [M5-A] record verified runtime and build comparison`.

### M5-B — Independent final review and clean handoff

**Objective:** finish with demonstrably safe, scoped and documented improvements.  
**Ownership:** all task diffs, brief results ledger, `dev/test_perf/` documentation.  
**Tasks:** reviewer inspects for production behavior changes, weakened tests, lost/duplicated cases, stale CLI examples, invalid timings, leaked processes and benchmark pollution. Run scoped `cargo fmt --check`, the minimal affected test/build checks, and **one final first-party Clippy pass** appropriate to changed maintained targets (`cargo clippy -p multi_launcher --all-targets --profile fast-dev -- -D warnings`, adapting actual project flags). Do not clean vendored warnings, run a speculative release build or restart the full suite after lint. Resolve only task-related reviewer/lint findings, commit small remediation if needed, and attempt normal pushes. Final report must state which checks were actually run.

**Behavior/edge cases:** Clippy on full targets may incur compile cost; run once near end, not after each checkpoint, and document environmental inability honestly. **Regression constraints:** unrelated product code, default workflows and user state untouched. **Verification:** diff against M0-A baseline, SHA inventory, first-party lint/format and final Git status. **Acceptance:** required changed-owner tests pass; coverage equivalence preserved; known task-related blockers absent; results correctly classified; every retained delta committed on branch, final push attempt status known. **Commit:** `chore(test-perf): [M5-B] complete review and handoff` (or small specifically labeled remediation commit).

## G. Active checkpoint commit cadence

> **Use the project's active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**

First commit is **M0-A on the newly created branch**, never the original base commit. Commit each cohesive verified checkpoint, favoring `M0-B`, `M0-C`, `M1-A`, `M2-A`, `M2-B`, `M3-A` (only if beneficial), `M4-A/B` (only if warranted), `M5-A`, `M5-B`. Omit optional no-change commits; record skips in the ledger. Inspect diffs and run checkpoint-appropriate checks before each commit. Do not amend/squash/rebase existing history. Attempt a best-effort regular push after each retained commit; failure is recorded but **never stops local work**. Preserve unrelated user changes and stage only intended files. Report **original base SHA, new M0-A baseline SHA, each later commit SHA and actual push result**.

## H. Testing and evidence matrix

| Gate | Minimum proof | What must **not** be inferred |
|---|---|---|
| Harness | Short synthetic owned success/failure/timeout + one exact library and integration listing/run; validate JSON schema | Parser output alone is not a real test pass |
| Selected build | Exact `--lib`/`--test` prebuild, warm no-op and controlled edit, profile matched | Cold profile population is not an edited-build improvement |
| Oracle | `cargo nextest run --lib -E 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames)'` with controlled profile; adjacent geometry check if touched | Never drop 10k case or independent eager geometry to speed it up |
| Consolidation | Exact moved tests and before/after case inventory; source/target selector mapping | Changing test binary identity can change process/global-state isolation |
| Fixtures | Direct affected cases plus one mixed-contention check when justified | Isolated pass does not settle full-suite concurrency |
| Final | Same matched selections; one bounded full-suite candidate run if feasible; first-party formatting/Clippy at end | The historical 263.841s run is **not** a current-SHA measured baseline |

Use supported Nextest syntax established with local `--help`. Example default-feature library and integration selectors:

```powershell
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E 'test(=gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames)'
cargo nextest run -p multi_launcher --test plugin_queries --cargo-profile fast-dev -E 'test(help_plugin::search_returns_help_action)'
cargo nextest run -p multi_launcher --test dashboard_config --cargo-profile fast-dev
```

For a *full* run, use the real supported `cargo nextest run --workspace --cargo-profile fast-dev --no-fail-fast` invocation with an explicitly recorded effective config/profile/features **and matching pre/post run conditions**. Do **not** inherit a historical ignored `target/.../nextest.toml` without verifying that it exists, is safe, and is part of this comparison. Do not run historical ignored benchmark modes as if they were ordinary tests.

## I. Risks, bounded escalation and stop conditions

| Risk | Mitigation / stopping rule |
|---|---|
| Full test suite takes too long or hangs | At most one base and one final full invocation, each with owned-process deadline and immediate evidence. No third attempt; classify any incomplete measurement. |
| Windows-only or missing tools | Prefer native Windows host for actual benchmarks; no fabricated Linux-equivalent speed claims. Short install/config diagnostic, then report blocker rather than installing new toolchains without cause. |
| Timing comparisons are biased | Pin profile/flags/target host; separate build/execution and cold/warm/edited phases; use median where repeatable; do not compare unmatched jobs. |
| Expensive eager oracle accidentally weakened | Preserve 10k step, all geometry and frame assertions; verify independent expected values and negative detection; review source diff. |
| Test grouping changes process isolation | Group only pure tests; compare case count/target mapping, fixture paths and concurrency; revert if doubtful or benefit unproven. |
| Fixing unrelated previously failing tests expands scope | Record each discrepancy and cause. Repair only if newly introduced or caused by test isolation work. Separate product defects into follow-up notes. |
| Cargo profiles or test config silently change product behavior | New choices opt-in; no default manifest profile/release changes; no blanket timeout/ignored/parallelism adjustments. |
| Repeated failed experiments consume unlimited time | Limit to one consolidation candidate, five runtime-test candidates, three wait replacements, one specific remediation rerun per checkpoint; revert no-go experiments. |

**Escalation:** identify exact failed command/test and stage, effect on Goal acceptance, one plausible root-cause remediation and smallest possible follow-up. Stop optimization when further work requires a broad production refactor, unrelated defect campaign, loss of assertion strength or unsafe OS mutation. Never demand manual user participation during Codex execution; present only the final handoff and any genuine blocker.

## J. Definition of done

- Mandatory unique fresh branch, **new M0-A baseline commit and attempted upstream push** actually completed; original base and baseline IDs retained.
- Useful opt-in profiling workflow and machine-readable timing receipts exist, with documented targeted commands, failure and timeout accounting and process safety.
- The C2G oracle retains all four 100-case recipes and both large `ViewportControls` scenarios (including **10,000**); coverage, independent expected geometry, selector/popup/provider and state/frame checks remain meaningful and pass.
- Selected further test/runtime/compile/wait changes are **evidence-backed**, narrowly scoped, committed or explicitly reverted/skipped.
- Test roster and assertions are preserved (allow only documented module/target renames from validated M3 experiment); no introduced flaky/forever tests, unowned child processes, unintentional production modifications or default profile changes.
- Targeted verification, final diff/review and appropriate first-party lint/format are completed or precisely reported as blocked; full-suite findings are recorded truthfully.
- Selected-build 20% and full-execution 25% goals are evaluated **only if equivalent pairs exist**; actual improvements and unmet/unmeasured targets are reported without optimistic claims.

## K. Final Codex delivery format

In one concise factual handoff, include:

1. **Git:** original branch/base SHA; new unique task branch; M0-A baseline SHA; checkpoint commit SHAs; upstream push attempts, final branch/status; confirmation unrelated work preserved.
2. **What changed:** milestones completed/skipped, exact touched paths, rationale for retained vs reverted optimization experiments, confirmation product behavior unchanged.
3. **Performance:** table for selected warm build, selected execution/oracle, and complete execution (**source SHA**, command/config, baseline, after, delta %, sample count, cache/parallelism). Clearly label missing/unmatched measurements and historical 263.841 s context.
4. **Coverage and reliability:** before/after case/target roster and mapping, preserved 10k oracle and assertions, test PASS/FAIL/TIMEOUT/SKIP counts; distinguish new regressions, unresolved full-suite failures and known historical observations. Include owned process cleanup status.
5. **Verification:** exact commands, result/exit codes, time budgets and any skipped full run; first-party Clippy/reviewer result; paths to profiling JSON/summary receipts and any short, user-runnable PowerShell commands.
6. **Limitations/manual review:** only actual unresolved issues; no request for intermediate human testing. Stop at this Goal, not an open-ended performance campaign.
