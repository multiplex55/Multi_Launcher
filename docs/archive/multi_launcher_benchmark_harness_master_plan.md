# Multi Launcher — Opt-in Benchmark Executables and Safe Native Test Runner

**Implementation plan for Codex**  
**Proposed branch:** `perf-harness` (from the **exact checkout HEAD** when Codex starts)  
**Scope:** infrastructure for making, launching, containing and logging **small, purpose-built performance test executables**. **Do not optimize the application** under this plan.  
**Status:** approved requirements; Codex must inspect its live checkout for final APIs.  
**Starting evidence:** the user's October 18 `multi_launcher(20261018-202248).zip` snapshot and current `perf-track-d` GitHub results informed this plan; **these attachments are for planning only and must not be referenced or required by Codex**.

## 0. Goal and explicit non-goals

Create a reusable, auditable way for Codex to benchmark Windows-specific Multi Launcher code in **short-lived, independently launchable `[[bin]]` test executables**. Each test must have an unambiguous start, finite end, structured metrics, and safe cleanup. Enable Codex to compile/run them under the permissions actually granted to its session. Where native desktop/process permissions fail, report that exact limitation and proceed with separately executable CPU-only checks.

This is **not** another D1–D4 optimization pass. Specifically **do not** change Hot Folder's 80 ms tick, radial rendering, dashboard rendering, query ranking, Rust Analyzer configuration, codegen units, Cargo profiles, production input hooks, or clipboard behavior to improve performance. **Do not** claim that a synthetic/headless number measures live-screen responsiveness. Do not introduce an application-wide profiler, daemon, telemetry upload, always-on UI setting, second hook or permanent benchmarking service. Do not change normal launcher startup or default executable selection.

Successful delivery is: *safe runner with verifiable containment; predictable JSONL/JSON evidence; at least one real production-algorithm CPU probe; opt-in target contracts for a small set of independent probes; documented permission/error semantics; and no default-build overhead*. If the environment cannot permit native GUI runs, honest `blocked_environment` evidence is an acceptable **partial** outcome, not a reason to keep escalating or indefinitely expand scope.

## 1. Verified repository context to reuse

- `Cargo.toml` uses `autobins = false`, default binary `multi_launcher`, and existing `dev-smoke-binaries` gating for `coordinate_tool_smoke`, `passive_overlay_smoke` and `radial_acceptance`. Maintain that contract; do not reintroduce extra default binaries.
- Existing performance artifacts: `docs/performance/track_d/{results.md,native_capability.json,d1_results.md,d2_results.md,d3_owners.md}`, and `dev/perf_track_d/{README.md,developer_loop.ps1,summarize.ps1}`.
- `dev/track_b/measure.ps1` already measures Cargo processes with arguments, exit codes, source provenance, cache-state classification and unique evidence folders; use it for **compiler/build** measurements, not for GUI process containment.
- `dev/run_owned_process.ps1` provides bounded native execution but requires `Get-CimInstance Win32_Process` process inventory; Track D could not use it because Windows denied that query—even the harmless `cargo --version` probe. Preserve this existing runner; add a narrow alternative only if its ownership properties can actually be established.
- `src/gui/hotfolder_runtime.rs` has an 80 ms enabled-driver cadence; it is a *hypothesis*, not a measured CPU regression.
- `src/radial/{geometry,render,bindings,controller}.rs`, `benches/radial_runtime.rs` and `src/gui/track_c_query_observer.rs` already include measurable paths; prefer their true algorithms/observers over a fabricated imitation.
- Dashboard rendering is owned by the existing egui/dashboard widget paths. A headless egui frame measures **CPU construction only**; GPU-present/physical visibility needs native evidence.
- Existing Track D recorded no new production optimization, only a previously available `fast-dev` build advantage. Rust Analyzer's reported 25.94 s database load / 60.07 s headless timeout does not warrant a new RA architecture or another blind trial.
- `AGENTS.md` controls small coherent commits, single writer, reviews and best-effort pushes. This plan adds explicit execution/evidence budgets, not a replacement agent constitution.

Codex must check whether its *current checkout* has diverged from any of those locations/behaviors. Preserve the current implementation and adapt the bounded plan rather than blindly applying stale APIs.

## 2. Strict execution envelope

| Stage | Time cap | Permitted outcome |
|---|---:|---|
| H0 — Branch / ownership inventory / protocol | 20 min | Confirmed starting state and a written test contract |
| H1 — Structured evidence + handle-owned runner | 75 min | Validated runner or explicitly blocked platform capability |
| H2 — Three small, independent probe targets | 95 min | At least one verified real-algorithm CPU probe; each other target implemented or documented as bounded blocker |
| H3 — Cargo/RA measurement adapters and single-command recipes | 35 min | Reusable, unambiguous commands |
| H4 — Focused verification / handoff / final Clippy | 35 min | Honest completeness matrix and final evidence |
| **Absolute total** | **260 min (4h 20m)** | Stop at cap and publish what works |

**Additional hard limits:**

1. At most **four new opt-in executable targets**: the ownership/launch runner and at most three individual workload probes. No additional one-off binary for every hypothetical future bottleneck.
2. No more than **one shared protocol module** and the minimum necessary scenario adapters. Don't refactor the normal application to make a probe convenient.
3. Per problem: 15 minutes diagnosis, at most **two implementation attempts**, one directly relevant build/test after each; if unresolved, document and move to the next independently executable item. One known compile error may be corrected once beyond this only if the fix is unambiguous and the total deadline remains intact.
4. For each runnable probe, a **single capped smoke batch**: 3 warmups and up to 20 recorded samples, maximum 90 seconds per invocation. Two native launch attempts per executable, including one targeted repair/retry. No long soak, no full-screen automated sweeps.
5. At most **two owned screenshots per GUI probe** (six total), only from a controlled fixture window; never sample unrelated desktop windows. If the screenshot capture route is unavailable, record `screenshot_unavailable` and keep the numeric result separately qualified.
6. Raw log/output limit **32 MiB per run**, no more than **100 MiB total retained** for this plan. Stop logging at the cap and mark a truncated run rather than silently lose data. Track summaries stay small and redacted.
7. Targeted `cargo check` or `cargo nextest` tests only; **no full Nextest run** (the starting HEAD's tests are assumed green). `cargo metadata` once before/after target additions. **One final Clippy invocation**, after all implementation; if it fails on clearly pre-existing lints, record exact owner/evidence instead of fixing unrelated code or repeating Clippy.
8. Never use destructive process-name kills, global desktop input blocking, privilege escalation, user data paths, credential access, broad machine scanning, forced shell changes, admin prompts, external network upload or disabling the sandbox to collect metrics.
9. The plan is not gated on GitHub pushes. Commit locally first, attempt one normal non-force push per checkpoint when permitted, record `pushed`/`local_only`, continue on failure and don't loop to bypass automatic review.
10. **No user-operated actions between checkpoints.** Computer-use permission prompts, actual sandbox/OS denials or required authorization cannot be superseded by the plan. Record the blocked scenario and continue with independent work; surface a single concise final request for any permission the user must explicitly grant.

### Status vocabulary and evidence honesty

Every checkpoint: `pending | in_progress | complete | partial | blocked_environment | deferred`. Every scenario: `success | error | timeout | unsupported | blocked_environment | blocked_desktop | truncated`. Missing values **must be JSON `null` plus a reason**; never encode failed runs as zero latency or zero CPU. Distinguish `cpu_headless`, `native_fixture`, `production_app` and `compile_command` kinds. Never call the first two “real launcher startup” unless the actual binary and presentation path were observed.

## 3. Permissions, authorization and safety contract

**User authorization within the kickoff:** Codex is explicitly authorized to **create, build, launch, relaunch and stop the listed local benchmark binaries and disposable fixture processes** inside the selected repository/owned fixture directories, collect timings, optionally capture images of those fixture windows and save bounded logs. This does *not* authorize running arbitrary executables, modifying unrelated data, granting admin rights, bypassing reviewer denials or using global input against the user's desktop.

**Two different boundaries:**

- **Codex execution approval:** With `sandbox_mode = "workspace-write"`, `approval_policy = "on-request"`, `approvals_reviewer = "auto_review"`, ordinary in-sandbox commands can execute without another user approval; boundary-crossing actions are separately reviewed. **Approve for me is not an elevation or automatic entitlement to access every Windows API.** A denied action is final for that attempt; do not route around it. A human may later select *Ask for approval* and approve the specific action if the UI offers that path.
- **Windows capability:** Windows desktop/session access, process-query rights, Job Object assignment and Native UI/Computer Use permissions are independent. A successful shell command or a trusted Git project **does not** establish safe OS process containment. A direct Windows `Access denied` does not imply an incorrectly formatted Codex prompt.

Do not edit `%USERPROFILE%\.codex\config.toml`, repository `.codex` settings, managed requirements, Windows ACLs, antivirus, UAC or Auto-review policy as part of this task. Do not propose Full Access as the default remedy. If the user's app permits a narrowly approved execution request, use it normally. Computer Use approvals may still require explicit user approval. If blocked, record one exact command/error in the ledger and switch to a safe nonnative measurement.

### Native process ownership requirement

A native runner is allowed **only after** it proves a bounded, handle-based owned-child lifecycle. Prefer Windows **Job Objects** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, no breakaway permission, an explicitly owned process/creation handle and checked assignment. To eliminate the launch-before-assignment race, create a test child suspended, assign it to the job successfully, then resume it. Use native API wrappers already in the project where feasible. Do not use `taskkill /IM`, enumerate unrelated processes, or trust PID alone after process exit. On timeout/error, terminate/close **only the owned job** and verify completion from the handles. Record actual failure if nested job assignment is disallowed. Do not substitute launching a potentially spawning child uncontained to “get the numbers.”

For a GUI probe, additionally assert a test-owned window identity before any native interaction. All input must target only the owned fixture, with no synthetic system-wide input if identity is unknown. If the app/display cannot be controlled from this environment, emit `blocked_desktop` and make no input attempt. The runner must never interfere with another running Multi Launcher instance.

## 4. Required artifact and CLI contract

**Targets** (names may change only to match naming collisions in current checkout):

| Binary | Role | Run target | Evidence label |
|---|---|---|---|
| `perf_probe_runner` | Process creation/timeout/ownership wrapper for exact approved probe commands | Native Windows | `runner_containment` |
| `perf_hotfolder_probe` | Isolated Hot Folder worker idle/disabled scenarios | CPU/owner + optional native worker | `hotfolder_worker` |
| `perf_radial_probe` | Real radial layout/preparation/scene construction with bounded cell counts | CPU; optional owned native fixture when permitted | `radial_cpu` or `radial_native` |
| `perf_dashboard_probe` | Real dashboard/widget egui frame construction under synthetic configurations | CPU headless; optional owned native fixture | `dashboard_cpu` or `dashboard_native` |

All four appear as explicit `[[bin]]` targets behind a new **nondefault** `perf-harness` Cargo feature. `cargo build` and `cargo build --release` without this feature must continue building only `multi_launcher`; existing three `dev-smoke-binaries` remain independently opt-in. Never add all-target startup instrumentation to production.

Preferred minimal CLI semantics for each probe:

```text
perf_<scenario>_probe --scenario <fixed-name> --out <owned-directory> \
  --warmups 3 --samples 20 --timeout-seconds 90 [--screenshot-owned]
```

The runner accepts only an explicit manifest-approved executable path/argv (the controlled target executables, plus purpose-built runner-self-test fixtures under an explicit test-only allowlist), a bounded timeout and output path under an owned evidence root. Do **not** forward strings to `cmd /c` or dynamically evaluate a shell command. Build using Cargo **first**, then invoke the resulting binary through the runner (no repeated Cargo compilation during timed scenario execution). A shell driver may orchestrate this but must not become a second process-ownership authority.

**Per-run output** in a unique ignored `target/perf-harness/<scenario>/<run-id>/` directory:

- `events.jsonl`: start record, one entry per completed sample/error, terminal record. Include event schema version, scenario/measurement kind, monotonic elapsed time, UTC timestamp, requested/actual workload, success/status, and concise error reason. Use explicit flush at terminal and atomic write/rename of finalized metadata where supported.
- `summary.json`: schema version, `baseline_sha` and source dirty-state or diff fingerprint, build profile, rustc/cargo version (when available without extra tool runs), OS/session provenance, fixture/settings hash, actual start/stop and elapsed, `p50/p95/min/max` for valid samples, count of valid/invalid samples, metric units and missing-metric reasons. Include `termination_reason`, `process_containment`, and `screen_visibility_kind`.
- `stdout.log`, `stderr.log` and optionally `owned-01.png` / `owned-02.png`, each size-capped. No arbitrary screen, clipboard, window title, filename, personal path, macro content or stored user settings in source-controlled output.
- `run.json` or equivalent one-run process manifest: executable basename and hash; actual command argv **redacted/normalized**; child PID and verified handle/creation identity where available; job creation/assignment result; exit code; timeout/cleanup verification. On watchdog-forced termination, the **runner** must emit the terminal record even if the child cannot flush its own final event. Never publish raw OS identifiers in Git-tracked summaries unless needed and sanitized.

Only **sanitized summary tables**, benchmark harness source and the tiny protocol fixtures belong in Git. Raw results remain Git-ignored and may be linked by their local path in a final handoff. Do not commit binary executables, PDBs, log streams, screenshots of user's desktop, `.cargo`/`target` artifacts or private machine identifiers.

### Measurement semantics

- CPU time: for a **real owned process**, record GetProcessTimes / elapsed wall, using explicit process handle; label it per-process and specify logical processor normalization (if any). A worker-loop sample counter is *not* CPU percent.
- Idle: distinguish `driver_timeout_wakeups`, `snapshot_publications`, `work_iterations` (if accessible without invasive instrumentation) and CPU usage; do not infer one from another.
- Radial/dashboard: separate wall-clock CPU algorithm/frame construction, input-to-controller acknowledgement (if instrumented) and native present/first-visible-frame time. Do not attribute GPU presentation based on a headless egui pass.
- Compiler/RA: measure the **host command** with Track B's existing command runner or a thin adapter, not a self-measuring GUI executable. Record prep/no-op/edited/cache state and timeout reasons. Do not call a headless Rust Analyzer timeout “interactive lag.”

## 5. Detailed checkpoint instructions

### H0 — Establish an isolated feature branch and bounded owner map (20 min)

**H0-A — Branch and state proof (<= 10 min)**

1. In Codex's actual repository checkout, record `git rev-parse HEAD`, checked-out branch, remotes/upstream and `git status --short`. Do not fetch/pull/merge/rebase to select a different baseline.
2. Require a clean tracked worktree for source changes. If unrelated uncommitted user work exists, do not stash/reset it; record the blocker and request proper isolation instead of incorporating it.
3. Create branch **`perf-harness`** at that exact HEAD (or `perf-harness-2` only if the name already exists; never reset an existing branch). Write branch/base SHA in the execution ledger.
4. Read the current `AGENTS.md`, manifest, Track D outcome and relevant existing smoke tools. No baseline full test suite and no arbitrary native probe.
5. **Commit H0-A:** `docs(perf): [H0-A] record harness branch and ownership baseline` with short `docs/performance/harness/progress.md`.

**H0-B — Single-page instrumentation/fixture contract (<= 10 min)**

1. Map which owners expose safely callable worker/frame algorithms and which are private to the GUI. Identify any existing `cfg(test)` / opt-in instrumentation without cloning app startup code.
2. Create a small scenario table: `hotfolder_disabled`, `hotfolder_enabled_zero_rules`, `radial_8`, `radial_32`, `radial_128`, `dashboard_minimal`, `dashboard_typical`, `dashboard_dense`. For inaccessible owners, mark planned `unsupported` explicitly; do not fabricate an alternate path.
3. Fix the exact baseline workload inputs: synthetic settings and fixtures only; no external folders. Define sample counts and scenario limits as above.
4. **Commit H0-B:** `docs(perf): [H0-B] specify bounded probe scenarios and provenance`.

### H1 — Evidence protocol and safe native runner (75 min)

**H1-A — Build small, self-contained recording primitive (<= 20 min)**

1. Add opt-in `perf-harness` Cargo feature and only the directly needed `[[bin]]` target definitions. Export minimal `#[cfg(feature = "perf-harness")]` library entry points for scenario ownership only when truly necessary; production default build must not compile the probes.
2. Build one shared recorder for the JSONL and summary contract: create unique run directory, start/terminal events, flush, non-success markers, safe bounded values, p50/p95 calculation and guaranteed terminal best-effort report on error/panic. No network listener, global singletons or background daemon.
3. Enforce `--help` side-effect-free behavior; reject unknown scenarios, negative/oversized samples, invalid output roots and unbounded runtime requests **before** creating a process or changing a fixture.
4. Add **2–4 focused tests** using a temporary synthetic output root: successful summary, timeout/error summary, invalid CLI rejection, log-cap/truncation behavior.
5. **Commit H1-A:** `feat(perf): [H1-A] add opt-in structured probe receipts`.

**H1-B — Checked Job Object runner (<= 35 min)**

1. First diagnose the existing runner's *specific* denial once using its documented harmless capability probe; do not repeat broad CIM queries or change OS security settings.
2. Implement a standalone Windows runner using owned process/Job Object handles, checked suspended creation → assignment → resume, bounded wait and enforced owned-tree teardown. Preserve stdout/stderr capture and exit status even for a failed child. Ensure process handles always close on every code path.
3. The native runner accepts only the explicitly registered local probe binaries and scenario args, not arbitrary target executable names; reject unknown/inaccessible paths and inherited execution flags that break containment.
4. If job assignment / resume / process creation is denied, fail closed with `blocked_environment` and the actual Win32 error; never fall back to uncontained spawning. Detect and classify missing interactive desktop separately.
5. Test runner **without launching Multi Launcher** against only controlled fixtures: immediate exit 0; nonzero exit; child that sleeps past deadline; and (if supported) a child that spawns its own owned descendant. Verify no owned child remains after close. One native retry maximum following a concrete fix.
6. **Commit H1-B:** `feat(perf): [H1-B] contain benchmark children with owned job handles` or `docs(perf): [H1-B] record denied native containment boundary` if the environment prevents a trustworthy implementation. Don't mislabel incomplete containment as success.

**H1-C — Protocol/containment final narrow check (<= 20 min)**

1. Perform the exact CLI parsing tests and a short 1–3 second owned fixture run. Confirm `started → sampled/failed → terminal` record ordering, wall-clock/monotonic consistency and cleanup/exit behavior.
2. Capture both approved and rejected execution attempts as separate receipts; no retries with another API solely to avoid a security denial.
3. Inspect the diff and required-features graph. Prevent any default binary or release startup change.
4. **Commit H1-C:** `test(perf): [H1-C] verify recorder and native containment contract` (or an evidence-only checkpoint with precise blocker).

### H2 — Three bounded **independent** benchmark binaries (95 min)

**H2-A — Hot Folder worker probe (<= 35 min)**

1. Build `perf_hotfolder_probe` with strictly synthetic disabled and enabled/no-active-rule scenarios. Use a fresh temporary app-data root and inert external-action adapter, never user's watched folders or actual move/copy/macro actions.
2. Reuse the real Hot Folder driver/service owner where safely accessible. If it cannot be instantiated without substantial production architecture migration, stop after the bounded investigation; don't replicate its logic in a fake loop and call that Hot Folder performance.
3. Measure elapsed wall, actual driver/snapshot counters when available and owned per-process CPU if native containment permits. Record unavailable OS counters as null, not synthetic estimates. One `hotfolder_enabled_one_disposable_root` scenario is **optional** and must use read-only event observation with all actions disabled.
4. Run at most one smoke batch (3 warmups + 20 samples, <= 90 seconds). Do not change the 80 ms interval or hotfolder execution semantics.
5. **Commit H2-A:** `test(perf): [H2-A] add bounded Hot Folder idle probe` (or limitation-only if real owner inaccessible).

**H2-B — Radial preparation probe (<= 30 min)**

1. Add `perf_radial_probe`; run the same production geometry/layout/scene/preparation APIs used by `benches/radial_runtime.rs`, with 8/32/128 synthetic cells and fixed skins. Prefer reuse of a shared current helper; do not port/duplicate the compositor.
2. Warm fonts/assets outside the measured section where the scenario specifies warm caches; retain a separate cold-cache label only if a real cold path can be isolated. Measure CPU preparation and result identity/count validation; no real cursor/hotkey action is needed.
3. Optional native-window surface only if Job Object/desktop control is proven; then distinguish actual present acknowledgement from algorithm time. Do not install a second global hook, steal focus or make unrelated clicks.
4. Run one bounded smoke batch; label `cpu_headless` unless actual native presentation was observed.
5. **Commit H2-B:** `test(perf): [H2-B] expose isolated radial CPU preparation fixture`.

**H2-C — Dashboard egui frame probe (<= 30 min)**

1. Add `perf_dashboard_probe` with minimal, typical and dense **synthetic** dashboard configurations. Use the actual dashboard/widget construction path or safe existing fixture adapter. Do not accidentally render a trivial stand-in panel and present it as application evidence.
2. Use a fixed viewport size/scale and cold/warm layout labels. Benchmark real egui `run_ui` CPU frame construction, plus actual generated shape counts/response invariants. Avoid measuring arbitrary sleep. Require bounded widgets and no network/UIA/system polling side effects.
3. If a test-owned native viewport is demonstrably safe, optionally record frame-present acknowledgement and up to two **owned-window** screenshots. Otherwise report `cpu_headless`, `native_unavailable`; no image required to finish.
4. Run one bounded smoke batch and leave all production widget behavior unchanged.
5. **Commit H2-C:** `test(perf): [H2-C] measure synthetic dashboard frame construction` (or truthful partial if real widget owner unavailable without invasive change).

**Minimum H2 success floor:** At least one of H2-B/H2-C demonstrates real current application computation (not a clone) and emits a valid schema-compliant summary in a sandboxed, noninteractive run. H2-A may be partial if native worker lifetime/permissions cannot be established. This is explicitly *infrastructure proof*, not evidence of a performance improvement.

### H3 — Compiler / Rust Analyzer bridges and reproducibility (35 min)

**H3-A — Reuse existing build telemetry (<= 20 min)**

1. Create a short `dev/perf_harness/README.md` and, only if necessary, **one thin launcher script** that selects an existing Track B command measurement and writes a harness-compatible sanitized summary. Don't rewrite `measure.ps1`, Track B workload editing, process ownership or installed Rust profiles.
2. Document fixed commands for normal dev/fast-dev selected build, targeted Nextest **compilation** and optional RA headless analysis. For RA, record actual executable/version/session/timeout and whether interactive editor response was observed (usually `false`); never call a headless timeout editor latency.
3. Show exactly how a future investigator adds *one new opt-in bin* while keeping `autobins = false` and the `perf-harness` feature gate.
4. One safe `cargo --version` or dry-run measure protocol check is sufficient; no new D1 build-comparison campaign.
5. **Commit H3-A:** `docs(perf): [H3-A] document compiler and RA probe recipes`.

**H3-B — Single-command bounded scenarios (<= 15 min)**

1. Provide copyable **build and invoke** examples on Windows, using exact argv arrays/quoted strings and a unique owned evidence folder. Runner should use direct executable calls, not `cargo run` during the measured interval.
2. Verify command discovery, `--help`, and dry-run recipes. Record which scenarios need desktop access; permit CPU-only runs while GUI is blocked.
3. Update `dev/README.md` with one link to the harness documentation; do not rewrite general developer documentation.
4. **Commit H3-B:** `docs(perf): [H3-B] publish reproducible probe invocation contract`.

### H4 — Focused verification, honesty matrix and handoff (35 min)

**H4-A — Minimum real execution evidence (<= 15 min)**

1. Build only the registered opt-in targets needed for the smoke proof (at least runner + one CPU probe). Record one direct CPU-only run; if allowed, one successful native containment/probe run. Never attempt a broad desktop acceptance campaign.
2. Check start/end markers, sample counts, valid units, records/status, process cleanup and ignored output paths. If a native attempt is blocked, report it and do **not** substitute `Start-Process` without containment.
3. Capture at most two fixture-window screenshots if an owned window exists and capture is authorized; screenshots are optional evidence, not a passing requirement.
4. **Commit H4-A:** `test(perf): [H4-A] record bounded harness execution evidence`.

**H4-B — Narrow regression boundary and final summary (<= 10 min)**

1. Run only focused tests for modified record parser, Job Object cleanup and chosen scenario adapter(s). Verify `cargo metadata --no-deps` identifies only one default app binary, with all new probes gated, old smoke targets retained and no new production dependency.
2. Write a short `docs/performance/harness/results.md`: actual baseline SHA; checkpoint SHAs; exact commands and outputs; supported/unsupported metrics; whether native permissions worked; committed vs raw evidence path; next recommended scenario. No optimization advice unsupported by measurements.
3. Ensure rustfmt for touched Rust paths and `git diff --check`. No broad Nextest, no extra compiler matrix.
4. **Commit H4-B:** `docs(perf): [H4-B] close harness capability matrix`.

**H4-C — Final Clippy and handoff (<= 10 min)**

1. After all source/markdown changes and focused verification, run **one** maintained-target `cargo clippy --workspace --all-targets --offline -- -D warnings`, using the repository's actual toolchain and scope. Do not lint vendored crates as standalone workspace members.
2. If Clippy reports only pre-existing source lints outside touched paths (Track D previously recorded two `let_and_return` findings), report precise errors and baseline evidence **without unrelated fixes or repeat sweeps**; do not claim lint-clean. If new harness-owned lint exists and can be fixed within the checkpoint without an extra full sweep, correct it and run the narrowest possible confirm, recording that one successful narrow run does not prove a clean all-target pass.
3. Update the ledger with the final status and one concise permission/how-to-use handoff. No further source modifications after a successful final Clippy unless required for correctness.
4. **Final commit:** `docs(perf): [H4-C] finalize probe harness and recorded limitations` (best-effort push once).

## 6. Acceptance checklist

- [ ] Codex created `perf-harness` from the exact starting checkout HEAD and preserved user changes.
- [ ] Normal `cargo build` / `cargo build --release` still targets **only `multi_launcher`**; all harness bins are explicitly opt-in, and the existing dev-smoke targets remain gated separately.
- [ ] Direct per-scenario binaries support bounded args, independent execution, deterministic synthetic fixtures and explicit start/stop status.
- [ ] Protocol always distinguishes success, timeout, code error, blocked Windows process API, blocked desktop and unavailable metrics.
- [ ] Raw JSONL/summary/stdout/stderr artifacts exist for successful test runs, with source-controlled sanitized report and hard retention limits.
- [ ] Runner uses owned handles and safely contained child processes before claiming native-run support; otherwise clearly marks that capability unavailable.
- [ ] At least one real application algorithm has been exercised by a CPU-only opt-in executable, with the algorithm's measurable output validated.
- [ ] Hot Folder/radial/dashboard probes are independently executable **or have explicit bounded, source-backed non-support evidence**; no fake native timings.
- [ ] Compiler/Rust Analyzer measurements reuse appropriate external-command scripts; do not claim in-process executable measurements of Rust Analyzer or Cargo linkage.
- [ ] Codex was permitted to attempt local creation/build/run/stop; actual auto-review/Windows restrictions are honestly recorded and never bypassed.
- [ ] Only directly affected tests were run; no full suite; Clippy last with accurate exit/outcome.
- [ ] Small sequential checkpoint commits allow inspection of progress; failures and inconclusive probes did not hold up independent tracks.

## 7. Final handoff format (required)

1. **Git:** starting SHA, created branch, current HEAD, checkpoint map, pushed/local-only state.
2. **Capabilities:** `runner containment` / `CPU headless` / `native desktop` / `owned screenshot` with status and exact evidence location.
3. **Probe table:** binary, scenario, samples, elapsed, CPU/wall interpretation, screenshots and `null` metrics with reason.
4. **Verification:** actual focused commands and results, final Clippy exit, whether any test or metric was skipped.
5. **Next usage:** 3–5 copyable commands to build/run the probes; what environment permissions would be needed *only if* native execution remained blocked.
6. **Stop-point:** explicit declarations of what was **not** optimized (Hot Folder 80 ms, radial/dashboard production code, compiler profiles, Rust Analyzer settings). No follow-up work begins automatically.

---

### References the implementer should inspect locally

`Cargo.toml`; `AGENTS.md`; `dev/run_owned_process.ps1`; `dev/track_b/measure.ps1`; `dev/perf_track_d/README.md`; `docs/performance/track_d/{results.md,native_capability.json,d3_owners.md}`; `src/gui/hotfolder_runtime.rs`; `src/radial/{geometry,bindings,render}.rs`; `benches/radial_runtime.rs`; current dashboard rendering owners and their selected tests. These are orientation points, **not a mandate to inspect unrelated repository areas**.
