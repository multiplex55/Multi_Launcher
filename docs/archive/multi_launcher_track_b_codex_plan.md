# Multi Launcher — Track B: Rust Compilation & Build-Time Optimization
## Detailed Codex Execution Plan

**Authoritative baseline:** `HEAD` of the **currently checked-out branch** at the start of execution in the real repository. Record its branch name and SHA. **Do not fetch/pull/switch to `master` to establish a base.**

**Implementation branch:** `build-optimization`, created from that HEAD, or safely resumed only if already owned by this initiative. **All commits local; do not push or merge.**

**Source notes:** `multi_launcher_track_b_source_notes.md` (planning evidence to verify against the live source).  
**Kickstart:** `multi_launcher_track_b_codex_start.md` (orchestrator instructions).  
**Priority:** **Build compilation and Nextest compilation first**; user is **not** requesting immediate rust-analyzer/Neovim configuration changes.

---

# 0. Mission, scope, invariants

## 0.1 Problem definition and success

Reported reality: after changing Rust source files or switching branches, a **warm-dependency** production release build takes approximately **six minutes** on a Windows machine with **32 GB RAM** and previously recorded **eight logical processors**. The reported linker is “LLVM” and Rust version “latest”; treat these as unverified until querying the host. A truly unchanged/no-op build has **not** been measured and must not be conflated with an after-edit rebuild.

**Desired outcome:** shorten the normal **edit → check/build → targeted Nextest → run** loop, preserve complete test coverage, retain a high-quality unchanged **canonical production release**, and publish reproducible before/after evidence. Optimize by reducing *work*, not by hiding targets or weakening tests.

Expected outputs of this initiative:

1. A reliable build-time benchmark harness + frozen baseline with clear source SHA/environment/caches and separate **compile**, **codegen/link**, **test execution** and **no-op** measurements.
2. Low-risk build-script invalidation and explicit build-target workflows.
3. Measured development/test and release-like **named iteration** profiles, preserving standard `release` behavior.
4. A fast targeted Nextest workflow and, **only when proven valuable**, safe consolidation of appropriate integration targets.
5. Conditional experiments with linker, sccache, job count and dependency-feature pruning; a measurement-based decision on whether crate extraction merits a separate architecture project.
6. A final report with correct comparison methodology, accepted/rejected experiments, resource tradeoffs, full functionality/coverage gates and local commit references.

**No fixed improvement percentage is mandatory.** Six minutes should be reduced where feasible, but the plan makes **no timing promise** without benchmark evidence.

## 0.2 Hard rules

1. **Source-of-truth rule:** the currently checked-out repository `HEAD` is the baseline; inspect actual `git status`, `git branch --show-current`, and `git rev-parse HEAD`. Historical plans and prior report SHAs are background context only. The source inventory in the companion notes must be verified against current HEAD before implementation.
2. **Git safety:** no switch to or pull from `master`, no rebase/force-push, no reset/clean/stash of user work, no deletion of branches or other worktrees. If there is a conflicting existing `build-optimization` branch, inspect provenance before proceeding; no overwrite. Local checkpoint commits **only**; **no `git push`** or automated merge.
3. **Canonical release preserved:** do not alter `[profile.release]` / effective release optimization/codegen/LTO defaults without explicit new user approval. Prefer separate named profiles and shell-local experimental flags. The normal `cargo build --release --bin multi_launcher` remains the production authority.
4. **No behavior loss:** no application features removed, test assertions weakened, plugins disabled, Win32 bindings trimmed without proof, shortcuts/routes/data formats changed, or test binaries made undiscoverable.
5. **Track A preservation:** do not modify the existing runtime optimization algorithms, tests, result semantics or their historical performance reports. Use narrow smoke checks after toolchain/profile changes; leave any runtime regression investigation separate unless this project caused it.
6. **Compilation focus:** rust-analyzer and rustaceanvim **configuration changes are out of immediate scope**. Do not edit editor settings, disable IDE analysis, or spend time tuning analyzer startup. An optional future-work note is permitted.
7. **Measurement gates:** avoid speculative new caches, excessive `RUSTFLAGS`, broad feature removal, and wholesale crate refactoring. Retain high-complexity changes only if supported by fair wall-clock measurements and correctness gates.
8. **Cache safety:** never run `cargo clean` in the user's normal target directory, delete historical benchmark evidence, or delete unowned files. Cold tests must use a **dedicated isolated target directory**. Do not perform simultaneous measurement builds that contend for the same target/cache.
9. **Bench isolation:** don't modify source files in the user's active working tree merely to generate benchmark edits. Use a disposable owned worktree/copy with private test files and restore only benchmark-owned edits. Avoid host-wide environment or security-service mutations.
10. **Review and testing:** follow repository `AGENTS.md`; use only relevant targeted `cargo nextest` checks while iterating. Run expensive broad compile/discovery benchmarks only at their designated gates, not after every tiny checkpoint.
11. **No invented provenance:** record actual current SHA, exact environment and measured outcome. `NOT MEASURED`, `NOT RUN`, `SKIPPED (measurement-gated)` are valid results; do not imply passing Windows native behavior from headless compilation.

## 0.3 Technical starting points confirmed by source inspection (recheck at HEAD)

| Location | Starting observation | Direct consequence |
| --- | --- | --- |
| `Cargo.toml` | Edition 2024; 58 runtime deps; no explicit Cargo profiles | Measure inherited defaults; add named profiles only after B0 |
| `build.rs` | `embed_resource::compile("Resources/windows.rc", embed_resource::NONE)` with no rerun metadata | Benchmark build-script rerun; narrowly track `windows.rc` and icon |
| `Resources/windows.rc` | References `Resources/Green_MultiLauncher.ico` | Both files must trigger resource rebuild |
| `src/bin` | `coordinate_tool_smoke`, `passive_overlay_smoke`, `radial_acceptance` | Compare ordinary default release vs explicit launcher-only |
| `src/bin/radial_acceptance.rs` | ~40k lines | Potential unrelated binary compilation cost; keep acceptance functionality |
| `tests` | 65 top-level integration targets plus named `domain` and `plugin_queries` grouped suites | Need target-aware Nextest commands and linker fanout analysis |
| `tests/suites/domain.rs`, `plugin_queries.rs` | Module aggregation via `#[path]` | Precedent for selective grouping if justified |
| `src/lib.rs`, `src/main.rs` | Shared large app library plus launcher binary | Need distinguish crate recompilation from binary linking |
| `AGENTS.md`, `.codex/agents/*` | Existing orchestration, scoped testing and safety instructions | Use planner/implementer/reviewer and local meaningful commits |
| `dev/run_owned_process.ps1` | Existing owned-process test runner | Apply only to native checks when changing linker/profile |
| `docs/performance/track_a_*` | Completed runtime performance campaign | Protect, do not remeasure unrelated runtime milestones indiscriminately |

## 0.4 Explicit exclusions

- No rust-analyzer / Neovim / rustaceanvim settings changes now.
- No blanket Cargo dependency upgrade, Windows-rs rewrite, UI feature migration, plugin removal, feature-gate disabling, or runtime search/render optimization.
- No forced consolidation of all 65 test files, suppression of failing tests, or reduction of test coverage.
- No compulsory new external tools or global compiler flags; no automatic editing of user/system `~/.cargo/config.toml`.
- No crate split unless B4 analysis makes a **strong documented case** and the extracted component is small and behavior-preserving; high-risk broad reorganization is deferred to a separate approved initiative.

---

# 1. Orchestration, benchmark protocol and progress evidence

## 1.1 Agent ownership

The **parent Codex orchestrator** owns branch state, measurement schema, milestone sequence, test budget, review integration, local commits and reporting. Assign narrow read-only analysis to the repo's **planner** and **reviewer**; implementation checkpoints to the existing **implementer** agent(s), with **one source writer at a time**. Do not create overlapping agents running `cargo` into the same target directory. The parent may run measurements after implementation to maintain like-for-like conditions.

A checkpoint packet must include: `stage`, `base SHA`, `owner files/callers`, `existing behavior`, `required edits`, `constraints`, `tests`, `benchmark comparison`, `acceptance`, `review trigger`, `commit subject`. Read `AGENTS.md` and agent TOMLs as the first work item and adapt paths/ownership to live HEAD.

## 1.2 First-execution Git protocol

From the real repository root, before modifying files:

```powershell
$baselineBranch = git branch --show-current
$baselineHead   = git rev-parse HEAD
$worktreeStatus = git status --short
Write-Output "Track B baseline branch: $baselineBranch"
Write-Output "Track B baseline HEAD: $baselineHead"
Write-Output $worktreeStatus
```

- Record `HEAD`, including its containing branch; do not infer source SHA from older Track A logs.
- If the current worktree is safe, create `build-optimization` **from that exact HEAD**: `git switch -c build-optimization`. If already on the correct branch, proceed without switching. If branch exists on another worktree, investigate ownership; do not force-swap or silently delete it. An optional dedicated worktree may be used when branch constraints require it, but never mutate an existing owned worktree without permission.
- Existing uncommitted user work is not part of the committed `HEAD` baseline; preserve it, isolate changes and do not stage it. If dirty source would invalidate controlled measurements, move the initiative into a fresh owned worktree at the recorded HEAD without affecting those files.
- Subsequent commits are **local only**. Never run `git push` or merge into `master`.
- In `docs/performance/track_b_baseline.md`, record both `baselineBranch` and `baselineHead`, and exact source tree/worktree facts. Keep the reports tracked separately from bulk raw outputs.

## 1.3 Baseline experimental design

**Fair conditions:** same host, same toolchain, same target profile, same exact command/flags, same representative source edit, controlled dependency cache state, and no simultaneous competing build. Capture Windows OS/version, CPU/logical processors, 32GB memory verified, physical disk type/free space, power scheme, Rust/Cargo/Nextest versions, actual linker path, `RUSTFLAGS`, `RUSTC_WRAPPER`, `CARGO_TARGET_DIR`, relevant Cargo user/repo configuration and build scripts.

**Do not confound:** (a) truly unchanged build, (b) cached dependencies with changed source, (c) branch switch, (d) clean output dir, (e) first build of a new Cargo profile. Distinguish compiler codegen, build script, resource compiler and linking. Use `cargo --timings` and `-vv` evidence where appropriate.

Baseline matrix — measure before any optimization:

| ID | Scenario | Example command or owner | Separate measurements |
| --- | --- | --- | --- |
| BL-01 | Unchanged production launcher | `cargo build --release --bin multi_launcher --timings` repeated identically | no-op wall time, Cargo freshness, build-script execution |
| BL-02 | Launcher after small source edit | same explicit release command | final crate codegen/link, wall time |
| BL-03 | Launcher after large module edit | same explicit release command | which crates invalidated, codegen/link duration |
| BL-04 | Default full release targets | `cargo build --release --timings` | difference vs explicit single `--bin` |
| BL-05 | Small targeted library test | `cargo nextest run --lib -E 'test(history_prepare_)'` | build and execution separately |
| BL-06 | One integration target | `cargo nextest run --test history` | compile/link vs test execution |
| BL-07 | Grouped integration target | `cargo nextest run --test domain` | actual suite wall and link cost |
| BL-08 | Broad integration target discovery/build | `cargo nextest list` or bounded `cargo nextest run --tests` if needed | distinct binary count, compile/link wall; no mandatory broad execution |
| BL-09 | `cargo check --lib` | basic check | compiler diagnostics cost excluding LLVM/codegen/link |
| BL-10 | Branch switching | disposable controlled pair of commits | cache invalidation, final rebuilt targets, wall |
| BL-11 | Fresh isolated target directory | exact release command with `CARGO_TARGET_DIR` scoped to a temporary owned path | total cold dependency build; run once if disk/time permit |

**Baseline edit fixtures:** use a scratch clone/worktree with identical source + initially populated caches for controlled mutation. Small change should be in a representative leaf/internal Rust file; large change should touch a representative substantial GUI/plugin module. Make a byte-preserving original backup and restore only the benchmark-owned file using an explicit `try/finally`; assert no user modifications, extra diffs or partially restored edits. Name the exact files chosen only after inspecting current code. Source changes used solely for measurement are not committed into the feature branch.

**Workload repetition:** for expensive release builds use enough runs to distinguish outliers (minimum 2 comparable repeats where feasible; 3 is preferred), avoiding gratuitous 6-minute rebuilds; record every observation and reason for any incomplete samples. For inexpensive no-op and check commands use 3–5 trials. Quote median and ranges; do not use a single “best” run as the before/after result. When a workload takes unusually long, checkpoint captured result and avoid repeating it unless the comparison is necessary.

**Artifact policy:** write `docs/performance/track_b_baseline.md`, `docs/performance/track_b_results.md`, and a compact `docs/performance/track_b_checkpoints.md`. Store raw `--timings` HTML, terminal traces, ETW/build process captures, test listing and potentially identifying paths in ignored `target/track-b/` or a private temporary directory. Never commit entire target trees, user source paths, credentials or private environment-variable values. The only environment data in committed reports should be non-sensitive and relevant.

**Stage acceptance classes:** `PASS` — verified; `FAIL` — verified failure; `NOT RUN` — unavailable/not executed; `NOT MEASURED` — metric absent; `SKIPPED` — conditional experiment rejected by evidence. Each milestone must explicitly identify its status.

---

# 2. B0 — Provenance, profiling and frozen pre-change baseline

**Mandatory. No compiler/profile edits in B0.** This phase prevents tuning the wrong bottleneck.

## B0-A — Verify current code/host and choose non-destructive benchmark ownership

**Ownership:** `.codex`, `AGENTS.md`, `Cargo.toml`, `build.rs`, `src/bin`, `tests`, Windows host; tracked docs in `docs/performance/`.

1. Run the Git protocol above; ensure branch points to initial HEAD and record exact SHA. Confirm clean working tree or isolate initiative from pre-existing changes without stashing or discarding user state.
2. Enumerate targets using `cargo metadata --no-deps --format-version 1` and inspect `Cargo.toml`. Verify bin names, selected library, 65 top-level integration files, grouped suites, bench targets and any newly added `[[bin]]` / `[[test]]` / `required-features` in live HEAD.
3. Inspect `build.rs`, `windows.rc` and referenced icon/other resource files, including any environment-variable dependencies of resource compilation. Inventory any repository/global `.cargo/config.toml` and wrapper/compiler flags **without logging secrets**.
4. Capture `rustc -Vv`, `cargo -V`, `cargo nextest --version`, `rustup show active-toolchain`, `where.exe link`, `where.exe lld-link`, `where.exe rust-lld` when present, and actual link executable from verbose build output. LLVM version is not a linker identity.
5. Capture CPU logical threads, physical/logical core details where available, RAM (expected 32GB), real storage medium, free space, Windows Defender scanning/build-folder status **read-only**, and power plan. Never disable security software globally.
6. Document current normal developer workflow: commands for release, test and branch switching; identify one small leaf file and one larger module to use in an isolated scratch-worktree edit protocol. Establish that benchmark scripts can't accidentally touch production data or shared `target/` when destructive.
7. Define a lightweight PowerShell runner under `dev/track_b/` (or docs-referenced local script) that records source hash/profile/command, timing and exit code. Do not install a telemetry service or add production runtime logging.

**Targeted verification:** build metadata enumeration; PowerShell benchmark runner self-test using a cheap command; manifest readable; diff/status check.  
**Done when:** source SHA, target list, toolchain/linker, host constraints and safe benchmark environment documented; no production behavior changed.  
**Review:** planner/parent inspection of Git/cache safety; reviewer optional.  
**Commit:** `docs(build): [B0-A] record HEAD provenance and build benchmark ownership`.

## B0-B — Benchmark harness and accurate compile/link/test separation

1. Write a repeatable command runner with scenario ID, command arguments, env override scope, current directory, timestamp, elapsed wall time, exit status, stdout/stderr location and Cargo `--timings` report path. Confirm PowerShell exit code handling (`$LASTEXITCODE` for executables) and do not mark failures as success.
2. Add a *cache-state ledger*: warmed/clean/branch-switched, target dir, previous command, number of source files modified, flags that change rustc fingerprints. Record whether the invocation did compilation, linked a binary or was completely fresh.
3. Where cargo emits `target/cargo-timings/cargo-timing.html`, capture/copy a versioned report to ignored evidence storage before the next run. HTML is human-readable; don't promise stable JSON parsing of Cargo timing HTML. Optionally capture `--verbose` output or process telemetry to estimate link time.
4. For Nextest, separate build/list/preparation time from test execution. If supported by installed nextest, use `--timings`; otherwise record independent wall time for (a) build with no run, (b) test execution after it is built. Confirm Nextest command support on the actual version rather than guessing flags.
5. Provide smoke-test fixtures: no-op two consecutive identical commands; one bounded fake build exit zero and one exit nonzero; make sure commands with spaces and arguments are recorded accurately and output files are not overwritten.
6. Keep benchmark report output bounded; no raw compiler path lists or huge duplicated command logs in versioned Markdown; no private project files outside the repository.

**Targeted verification:** PowerShell harness self-test; dry-run metadata retrieval; one inexpensive `cargo check --lib --timings` if the host cache allows.  
**Done when:** run records are auditable and test duration is not mislabeled compile duration; evidence is never overwritten.  
**Review:** independent review of the benchmark runner's measurement and failure semantics if nontrivial.  
**Commit:** `test(build): [B0-B] add reproducible build timing harness`.

## B0-C — Freeze baseline on the real Windows host

1. Run BL-01..BL-10 with a conservative cost budget; run BL-11 once only if disk space and iteration time permit. If any scenario cannot be safely reproduced, write **NOT MEASURED** and exact reason.
2. For BL-02/03 record **recompilation graph/fingerprints**: were dependencies rebuilt, did `build.rs` execute, did `multi_launcher` library rebuild, how much time in final crate compilation vs linking? Do not treat `cargo check` alone as a release compile benchmark.
3. For BL-04 record which binary targets are actually compiled. Distinguish expensive auxiliary `radial_acceptance` from main application and its dependencies. Compare explicit main-bin release to default build only after equivalent edits/cache states, not sequentially warmed/no-op artifacts.
4. For BL-05..08 capture discovery/test counts and commands. Keep a baseline roster of tests by binary and test case, including ignored tests, for B3 parity. Avoid executing the full suite solely to get names if `cargo nextest list` suffices.
5. For BL-10 test branch-switch invalidation with two known commits in a disposable owned worktree, capturing code changes/cargo targets and warm vs cold cache status; never switch main working branch or lose local modifications.
6. In the baseline report document the top bottleneck(s) and prioritized *hypotheses*: build script rerun, recompilation of shared lib, unnecessary binary targets, link fanout, cache fragmentation, dependency features, job contention. Link every claim to a run/trace, not a guessed percentage.
7. Freeze baseline artifacts/checksums before B1. No retrospective editing of baseline numbers to make later results look better; add a correction note instead if a measurement flaw is found.

**Verification:** repeat at least one no-op and one edited build scenario; compare whether outputs/target sets match; reviewer validates measurement labels.  
**Done when:** baseline build-time table is complete to feasible scope and highlights the actual limiting stages; reports separate no-op/edit/branch switch/cold.  
**Commit:** `docs(build): [B0-C] freeze Track B compilation baseline`.

---

# 3. B1 — Low-risk target and resource compilation improvements

## B1-A — Constrain Windows resource build-script invalidation

**Owners:** `build.rs`, `Resources/windows.rc`, `Resources/Green_MultiLauncher.ico`; any relevant build-script tests or docs.

1. Reconfirm `embed_resource::compile` call and all files referenced from resource script, and whether the build dependency prints its own `rerun-if-changed` values.
2. Add explicit lines in `build.rs` for each true resource input, at minimum:

   ```rust
   println!("cargo:rerun-if-changed=Resources/windows.rc");
   println!("cargo:rerun-if-changed=Resources/Green_MultiLauncher.ico");
   ```

   Preserve existing `embed_resource::compile(..., embed_resource::NONE)` argument/options and behavior.
3. Verify via Cargo verbose output/fingerprints that a normal unrelated source edit **does not cause an extra resource-script run**, while modifying `windows.rc`, modifying/restoring icon in a controlled fixture, and changing `build.rs` itself correctly trigger recompilation/rerun. Restore only owned scratch changes, not others' data.
4. Build `--bin multi_launcher` and verify icon/resource embedding using a Windows resource inspection or a narrow runtime/tool test; retain the original artifact's icon identity/metadata.
5. Compare like-for-like source-edited build time and build-script resource compiler invocation count to B0. If the change improves invalidation but not wall time noticeably, record that accurately and retain for correctness/efficiency if harmless.

**Verification:** `cargo build --bin multi_launcher` and relevant release target at designated measurement gate; fingerprint/script count tests.  
**Done when:** unrelated source changes do not rerun resource compilation, input changes still update icon, feature functionality preserved.  
**Review:** parent diff check.  
**Commit:** `perf(build): [B1-A] narrow Windows resource build dependencies`.

## B1-B — Select main launcher target and make fast command recipes

**Owners:** `README.md`, `dev/README.md`, optional opt-in `dev/track_b/*.ps1` wrappers, not core Rust application behavior.

1. Document exactly what normal Cargo default build compiles in this repository (library + default binaries), and establish the recommended production executable-only command:

   ```powershell
   cargo build --release --bin multi_launcher
   ```

   Keep the canonical `cargo build --release` available where all binary targets are intentionally requested; never remove auxiliary source files.
2. Add a lightweight reproducible script/command alias for release launcher build, debug launcher build, narrow `--lib` test build and one integration target, with arguments forwarded safely. Do not silently change the production packaging command to omit required distribution artifacts if distribution actually needs the other binaries.
3. Distinguish **compile** operations from `cargo run`, test discovery and test execution. Document that `cargo build --release --bin multi_launcher` still must compile the shared library on relevant source changes.
4. Validate the three existing smoke/acceptance binary targets remain directly buildable (`cargo check --bin ...` for cheap compile checks; full native executables only when needed). Do not use `required-features` to hide them unless explicitly asked later.
5. If useful, add `default-run = "multi_launcher"` **only** if it improves a real developer command and after verifying it does not incorrectly suggest `cargo build` selects only the app. It is *not* an alternative to `--bin`.
6. Capture target count and after-edit wall time, comparing equivalent `default` vs `--bin` commands from B0; where unrelated targets were already fresh, state reduced target selection but not a measured speedup.

**Verification:** `cargo metadata`, script help/argument checks, targeted launcher and smoke-bin `cargo check`, documented commands run.  
**Done when:** fast command workflows are accurate and executable, all binaries remain available, expected selected targets verified.  
**Commit:** `docs(build): [B1-B] add target-specific Cargo build recipes`.

## B1-C — Validate cache fidelity and integrate low-risk gains

1. Re-run controlled no-op, small-edit and launcher-only release scenario with B1-A/B accepted. Record build script invocations and final crate link work; do not claim faster rebuilds from unrelated warmed dependencies.
2. Confirm Cargo prints `Fresh`/relevant target is not rebuilt for an unchanged invocation; if not, trace mtime, environment, `RUSTFLAGS`, target-dir variations and build script fingerprint until root cause is known.
3. Check for unwanted tracked manifest/lock changes, altered icon resources or changed output path/package behavior. Confirm aux binaries remain independently buildable.
4. Reviewer assesses scope: no dependency feature change, no standard release profile change, no test-file migration. Correct findings before moving forward.
5. Record timings, codegen/link breakdown and whether B1 merely avoids rebuilding unnecessary resource/other bins or also improves an edited launcher build.

**Verification:** narrow launcher and resource checks, same benchmark runner as B0.  
**Done when:** B1 comparison and behavior gate are documented without overclaim.  
**Commit:** `docs(build): [B1-C] verify targeted build and resource cache behavior`.

---
# 4. B2 — Separate fast development/test profiles from canonical production release

**Dependency:** B0 baseline and accepted B1; profile experiments may create entirely new artifacts and therefore need first-cache-population separated from warmed runs. **Do not** edit canonical `[profile.release]` settings in this initiative.

## B2-A — Establish debug/test symbol policy with a full-debug escape hatch

**Owners:** `Cargo.toml`, command documentation in `dev/README.md`; no editor config.

1. Inspect actual effective Cargo dev/test profile settings and native CodeLLDB requirements. User rarely uses variable inspection, but full debugging must stay possible on demand.
2. Compare debug-information candidates **one at a time**, such as `debug = "line-tables-only"` vs current dev/test defaults, and optionally `debug = 0` for dependency crates using `package."*"` only if test debug needs permit. Avoid disabling assertions/overflow checks for test runs.
3. Prefer new named profiles initially (`[profile.fast-dev] inherits = "dev"`; `[profile.fast-test] inherits = "test"`) to avoid involuntary changes to everyone's default build. If measurements show value and no required usability loss, consider changing `[profile.dev]`/`[profile.test]` deliberately with reviewer evidence. Example **candidate, not mandated config**:

   ```toml
   [profile.fast-dev]
   inherits = "dev"
   debug = "line-tables-only"

   [profile.fast-test]
   inherits = "test"
   debug = "line-tables-only"

   [profile.debugging]
   inherits = "dev"
   debug = 2
   ```

   Verify supported syntax/version on actual Cargo. Avoid assuming inheritance automatically makes caches compatible with default profile; profile names and differing debug info can cause independent builds.
4. Benchmark standard `cargo check --lib`, `cargo build --bin multi_launcher`, and targeted `cargo nextest run --lib` against profile-equivalent operations (`cargo build --profile fast-dev --bin multi_launcher`, `cargo nextest run --cargo-profile fast-test --lib -E ...`). Distinguish first cache fill from repeated incremental checks; include changed/unchanged source and test targets.
5. Exercise a representative panic stack trace, test failure location and optional CodeLLDB session with `[profile.debugging]`, marking debugger-variable inspection **NOT RUN** if unavailable rather than claiming a full-debug pass.
6. Record binary/PDB size, CPU/memory/compile/link time, assertion behavior, and any difference to normal Windows test execution. Do **not** treat a reduced `.pdb` as proof of matching debugger functionality.
7. Retain the simplest measurable winner. If named profiles are slower after accounting for cache separation, document the outcome and avoid adding unneeded profiles.

**Verification:** targeted `cargo nextest` for a representative library module, targeted debug launcher compile, optional manual debugger acceptance.  
**Done when:** no runtime feature loss, tests retain safety checks, full-debug command documented, numbers support accepted changes.  
**Review:** reviewer for profile semantics/debuggability.  
**Commit:** `perf(cargo): [B2-A] add measured fast debug and test profiles` (or docs-only rejected experiment).

## B2-B — Introduce a release-like iteration profile without touching release

**Owner:** `Cargo.toml`; docs/workflow commands.

1. Add one named candidate `[profile.iteration]` **inheriting release**, experimenting with lower LLVM optimization/codegen and incremental compilation. Starting candidate:

   ```toml
   [profile.iteration]
   inherits = "release"
   opt-level = 2
   codegen-units = 64
   lto = "off"
   incremental = true
   ```

   These values are a **hypothesis**, not the final mandatory settings. `lto = "off"` matches ordinary non-LTO behavior when no explicit release LTO is set. Confirm effective settings and actual compiler invocations rather than assuming.
2. Keep `cargo build --release --bin multi_launcher` as the canonical production command. Iteration command:

   ```powershell
   cargo build --profile iteration --bin multi_launcher
   ```

   Its executable is a **different build profile**, with possible runtime and binary-size differences. Do not ship/rename it as the canonical release binary.
3. Measure initial profile cache population separately, then no-op and realistic source edits; compare B0 release and iteration only for like-for-like changes on the same code. Count rebuilt dependency crates and final link time; measure peak memory and `target/iteration` size.
4. Include a quick real Windows launch/application smoke for iteration binary and one representative CPU-sensitive Track A runtime scenario if needed to assess gross runtime cost, **without** reimplementing Track A benchmarks. No need for whole-app p95 proof when the executable is explicitly only for iteration.
5. Vary one parameter at a time where evidence suggests: `opt-level = 1` vs 2, `codegen-units = 16/64/128`, `incremental = true/false`, LTO when enabled by effective host config. A single full factorial grid is not required; avoid an expensive combinatorial experiment.
6. Preserve standard release behavior, debuggability of optional full-debug profile and correctly named target output. Document recommended daily usage and the need for final canonical release validation before distribution.

**Verification:** targeted build both profiles, low-cost launcher native smoke, same fixture repeated per profile and run log.  
**Done when:** `iteration` reproducibly builds faster for normal edit rebuild or is rejected and removed; release settings and behavior unchanged.  
**Review:** profile/behavior review mandatory.  
**Commit:** `perf(cargo): [B2-B] add measured release-like iteration profile` (only if accepted).

## B2-C — Tune incremental and codegen/work-unit tradeoffs selectively

1. Analyze why the six-minute rebuild persists under standard release: is final library LLVM codegen or link dominant? Use B0 timing report and `-vv` to distinguish incremental reuse possibilities from dependency recompilation.
2. Explore a small **orthogonal** candidate set for **named iteration and fast-test profiles only**: incremental enabled/disabled; codegen units 16, 32, 64 as promising; debug symbols if link dominates. Avoid applying conflicting `RUSTFLAGS` during baseline comparisons or letting one trial pollute the next trial's build cache.
3. For each candidate record cold first use, warmed after small source edit, warmed after large module edit, selected Nextest build and target size. Report improvements **on the workload** rather than summing independent savings as an overall speedup.
4. Monitor CPU/memory saturation on the eight-logical-core/32GB host. An eight-job release build and a single high-memory LLVM step may be slower under system paging; quantify rather than assume a higher job count helps.
5. Sanity check sample runtime behavior on the resulting **iteration** executable. If faster compile trades unacceptably slow UI startup or input responsiveness, roll back the candidate and document the tradeoff. No canonical release comparison based on mixed profiles.
6. Keep minimum viable profile variants (prefer **one** iteration profile + optional fast-test/full-debug) and remove redundant candidate names. Ensure Cargo.lock, rustup toolchain and test source remain unchanged except deliberate profile manifest diff.

**Verification:** reproducible command table and narrow launcher execution; appropriate `cargo nextest --cargo-profile` test run if modifying test profile.  
**Done when:** a measured, simple profile configuration is selected or no change accepted with evidence.  
**Review:** reviewer compares manifest/profile changes to user constraints.  
**Commit:** `perf(cargo): [B2-C] select efficient incremental and codegen settings`.

## B2-D — Preserve production release/runtime and document workflows

1. On a host with the accepted profiles installed, run **canonical release** and **iteration** in separate output directories; record actual flag differences and wall times. Ensure `--release` continues to use its original effective settings and binaries retain resource icon/Windows functionality.
2. Test debug information escape hatch (`cargo build --profile debugging --bin multi_launcher`; for test debugger, use the supported profile Nextest flag). Preserve breakpoint/source line maps in the full-debug profile; if test execution cannot run a GUI, mark native smoke NOT RUN.
3. Validate environment variable `CARGO_PROFILE_*` overrides cannot unintentionally make the canonical release path use iteration values; document user-visible result directories and exact commands.
4. Update `README.md` / `dev/README.md` with a compact decision tree for **quick check**, **fast iteration binary**, **narrow Nextest**, **debugger**, and **production release**. No false claims of runtime binary identity.
5. Freeze B2 report: build/edit timing, symbols/PDB size, cache disk footprint, observed startup smoke, remaining concerns, and rejected profile configurations.

**Verification:** `cargo metadata`, targeted profile build/test and one production launcher native smoke appropriate to profile changes.  
**Done when:** profiles are documented and verified with no production release regression.  
**Commit:** `docs(build): [B2-D] verify release and iteration profile separation`.

---

# 5. B3 — Reduce Nextest compilation/linking work without reducing coverage

**Dependency:** B0 baseline inventory, B2 stable profiles. Prefer **target-selection workflow** gains first; any test consolidation is conditional on measured linker fanout.

## B3-A — Audit test-target fanout, discoverability and isolation

**Owners:** `tests/*.rs`, `tests/suites/domain.rs`, `tests/suites/plugin_queries.rs`, `tests/domain_cases/`, `tests/plugin_cases/`, `Cargo.toml`, documentation. **Read-only initially.**

1. Capture Cargo test-target inventory: 65 auto-discovered top-level targets and two grouped suites in inspected source; re-enumerate live HEAD to confirm count. Determine `cargo test --tests --no-run` vs Nextest listing or suitable installed Nextest commands needed to compile each target, and verify exact target names from `cargo metadata`.
2. Save a full test roster before changes: target executable name, fully qualified test names, `ignored` status, platform condition, global-state/integration constraints, intended category and approximate executable size/compile/link time. Don't interpret source module count as test-case count.
3. Inspect each candidate's environment mutation (`set_var`, `current_dir`), registry, filesystem, COM, UI HWND/hotkey hooks, serial/global mocks, process spawning, `CARGO_BIN_EXE_*`, native search and test-only shared statics. Mark candidates `groupable` vs `isolate` vs `unknown`; unknowns remain separate.
4. From `--timings` and artifact sizes determine whether linking 67 integration executables is a **material** total cost for broad builds, and how often broad builds are used vs targeted ones. A large target count alone does not authorize a migration.
5. Identify at most one safe pilot grouping area (likely pure-domain or plugin-query style) and list the exact source files/migration plan. Avoid mixing GUI/native/global-state tests solely to reduce target count.
6. Check naming conventions and `#[path]` module imports in current grouped suites; map old logical test names to new qualified names ahead of any move.

**Verification:** baseline Nextest listing/metadata and source audit.  
**Done when:** full roster and classification exists, linker-vs-execution contribution is measured, and any pilot has objective go/no-go criteria.  
**Review:** mandatory scoped review of isolation classification.  
**Commit:** `docs(test): [B3-A] inventory integration test build fanout and isolation`.

## B3-B — Provide efficient, repeatable targeted Nextest commands

1. Create scripts/docs for narrow test builds using **Cargo target selection first**:

   ```powershell
   cargo nextest run --lib -E 'test(history_prepare_)'
   cargo nextest run --test history
   cargo nextest run --test notes_plugin
   cargo nextest run --test domain -E 'test(indexer_)'
   cargo nextest run --test plugin_queries -E 'test(weather_)'
   ```

   The last filters are examples; validate actual case names first and avoid command recipes that accidentally select zero tests. Use the real `--lib` and `--test` flags, not bare test substrings for build-target selection.
2. Provide a `dev/track_b/test-targets.ps1` or similarly simple optional helper that validates target names/selection and prints/executed commands without guessing. Do not build a whole new test runner or replace Nextest.
3. Compare a filtered-by-name but unconstrained Nextest invocation to explicit `--lib` / `--test` target selection **under equivalent cold/warm artifact states**, measuring compiled/linked target count and real wall time. Don't ascribe the savings to a faster compiler if fewer targets were requested.
4. Validate that relevant native and integration tests can still be selected explicitly, including `coordinate_tool_smoke` as a binary compile target. Preserve ability to invoke the full suite deliberately.
5. Document a workflow: first `cargo check --lib` (when sufficient), then exact narrow Nextest, only then broader related tests. Make sure the quick recipes don't stop developers from running affected integration tests.

**Verification:** run a subset of new commands and confirm nonzero expected discovered test counts; compare timed build-target selection.  
**Done when:** fast paths work, test executable selection is explicit, and functionality/coverage is unchanged.  
**Commit:** `docs(test): [B3-B] add targeted Nextest build recipes`.

## B3-C — **Conditional** one test grouping pilot

**Prerequisite:** B3-A proves meaningful integration-executable link fanout and finds a genuinely safe set of independent tests. If not, **SKIP** with evidence, preserving all current integration targets.

1. Choose a **small** set of groupable top-level test files. Create or extend an explicit `[[test]]` aggregator, following `tests/suites/plugin_queries.rs` or `domain.rs` conventions only when the changed test names/paths and mocks are compatible.
2. Keep module source files under an appropriate non-top-level folder and reference via stable `#[path]` modules. Prevent double compilation by moving test source out of auto-discovered `tests/*.rs` into a group-only directory. Preserve all behavior and code, including `#[cfg(windows)]` and ignored test conditions.
3. Fix only import/module path differences, original test fixture ownership and test-specific global state affected by grouping. Do not convert integration tests to unit tests merely for speed; do not combine tests requiring separate binary-level process semantics.
4. Generate a coverage mapping: every old test name (including ignored) maps to exactly one discovered new test. Compare source test count and Nextest's resolved test inventory before/after; preserve test capabilities and CI workflow selection. If test identifiers change, update controlled script/CI filters and document map.
5. Run the newly grouped suite and old dedicated behavioral acceptance tests where they still exist; confirm no new cross-module global-state conflicts, test-order assumptions, leaked environment changes or native UI/COM side effects.
6. Compare compile/link wall time and executable count on the same host/profile against B3-A, including a no-op and small-edit build. Inspect that a larger aggregate test binary isn't causing worse incremental rebuild cost for frequent module edits.
7. If the net benefit is unconvincing or coverage isolation fails, **revert this pilot's owned changes** without erasing any unrelated user files; mark `REJECTED` with evidence.

**Verification:** roster equivalence, selected target Nextest execution and timed compile/list.  
**Done when:** no lost/disguised tests, zero functional difference, meaningful build-time win or documented skip/revert.  
**Review:** mandatory independent review of isolation and test mapping.  
**Commit:** `refactor(test): [B3-C] group independently safe integration targets` (**only if retained**; otherwise docs-only checkpoint).

## B3-D — Prove full test discoverability and finalize target-layout decisions

1. Re-enumerate integration targets/tests with same installed Nextest version and same feature set. Compare B3-A original roster to final in a documented old→new identity map, including ignored/Windows-gated cases and test executables that intentionally remain isolated.
2. Ensure no missing test cases or duplicate accidental compilations. Preserve CI/manual ability to target moved tests and existing scope-focused test scripts; update names only when the actual grouping changed them.
3. Compare a **representative broad integration compilation** and a frequent targeted test build before/after. Separate test execution time, compiler work and link work. Use selected actual test cases rather than running broad native acceptances just to reach a count.
4. Reviewer checks that no blanket `autotests=false`, `required-features`, ignored-test changes, or warnings hiding important coverage were introduced.
5. Record how many test executables remain and quantify benefit, noting when targeted commands solve the issue without aggregation. Update docs to communicate when to run exact targets and when comprehensive build validation is justified.

**Verification:** Nextest listing/diff and focused execution of changed grouping, unchanged scopes selected as appropriate.  
**Done when:** every logical test retained/discoverable, no accidental native isolation loss, build fanout measurements auditable.  
**Commit:** `test(build): [B3-D] verify test target coverage and linking parity`.

---

# 6. B4 — Conditional linker, compiler cache, parallelism and dependency experiments

**Guard:** B4 is evidence-led. No B4 tool becomes a hard developer dependency unless a later user request explicitly approves it. Prefer isolated experimental env/profile/target dirs; current production release remains canonical.

## B4-A — Rank codegen/link and external-cache bottlenecks

1. Revisit B0+accepted B1/B2/B3 timing output. Rank actual sources of wall time: Rust crate compile, LLVM codegen, final MSVC linker, build resource compilation, proc macros, dependency fanout, test executable link fanout, target-dir file I/O, and cache instability.
2. Use `--timings` and supported `-vv` output to identify expensive crate(s) and compiled/link targets. Don't derive exact link time from an uncalibrated Cargo HTML bar if it includes multiple rustc phases; supplement with process-timing evidence when needed.
3. Record the host's actual link command/executable and toolchain identity. User guessed “LLVM”; confirm or falsify with evidence before selecting an alternative.
4. Decide which following experiments are justified. When link consumes little of the incremental wall time, B4-B should be **SKIPPED**, not performed out of curiosity.
5. Document predicted cache impact and verification needed for each selected experiment, then run one at a time; no overlapping build sources or flag soups.

**Verification:** read-only timing/profile interpretation and reviewer fact check.  
**Done when:** ranked bottlenecks + rationale for running/skipping each B4 experiment.  
**Commit:** `docs(build): [B4-A] rank compiler, linker and cache bottlenecks`.

## B4-B — **Conditional** alternative Windows MSVC linker trial

**Run only when B4-A shows material linking cost.**

1. Confirm available linker(s), MSVC libraries/toolchain integration, native resource/manifest compatibility, PDB/debug output and the command Cargo actually runs. LLVM in `rustc -Vv` **does not** identify the Windows linker.
2. Pick a supported alternative, such as LLD (`rust-lld`/`lld-link`) *when available*, configured in a **temporary shell-local environment** or isolated Cargo config, not global machine settings.
3. Compare the same `cargo build --release --bin multi_launcher` source edit under current vs alternative linker, using separate controlled cache state and a fair reproducible sequence. Capture wall, link time, PDB size, memory and warnings.
4. Validate binary loads and executes on Windows, the embedded icon and resources survive, native win32/COM/overlay functionality behaves, and debugger artifact generation remains acceptable. Use `dev/run_owned_process.ps1` to contain native smoke where appropriate; do not kill unrelated processes.
5. On any unresolved LNK errors, missing symbols, native crash, bad PDB or compatibility concern, revert experimental config and report **REJECTED**. If retained, keep it opt-in/documented until a separate explicit approval to make it the default.

**Verification:** both linker builds with same native smoke; reviewer inspects configuration/ABI risk.  
**Done when:** measured outcome and behavior parity; no compulsory linker change.  
**Commit:** docs/opt-in config only if justified: `perf(linker): [B4-B] document measured Windows linker choice`.

## B4-C — **Conditional** sccache trial and branch-cache comparisons

**Run only if dependencies/non-incremental compilation or branch-switch rebuilds offer plausible hit reuse.**

1. Detect whether `sccache` is installed and compatible; do not silently install external tools or change system PATH. If installation would be required, document the manual step and mark unavailable until explicitly authorized.
2. Use shell-local `RUSTC_WRAPPER` pointing to the verified executable and a private cache root/size. Do not mix with other experiments; isolate target directory as needed to avoid artificial cache-invalidation fingerprints.
3. Clear/record **only the experiment's private sccache cache** for a first use; leave user-wide caches intact. Measure miss population, unchanged command, branch switching and repeat builds of dependencies where actual cache reuse occurs.
4. Use `sccache --show-stats` (verify flags on installed version) to record hit/miss/unsupported codegen counts and sizes. Interpret compilation hit savings against the added wrapper overhead; do not claim direct reuse of incremental crate work.
5. Retain only opt-in instructions if outcomes improve the user's branch-switch/edit workflow. If wrapper conflicts with native builds or offers no practical gain, remove it from default commands and document **SKIPPED/REJECTED**.

**Verification:** controlled cache tests + a successful release application build on both baseline and experiment.  
**Done when:** actual hit rates and wall-time deltas justify or reject use.  
**Commit:** `docs(build): [B4-C] record sccache experiment and safe setup`.

## B4-D — **Conditional** job count, memory pressure and filesystem study

1. If B0/B4-A shows CPU starvation or memory pressure, benchmark `--jobs`/`CARGO_BUILD_JOBS` variants suited to **eight logical processors** and 32GB RAM, e.g., 4, 6, 8; don't run a broad search or assume largest is best.
2. Measure compile concurrency, compiler RAM growth, page faults/swap, disk queue, and interference with editing tools/build caches. Distinguish cold parallel dependency compilation from a single final-codegen bottleneck.
3. Evaluate whether test linking is serial or parallel and whether too many simultaneous test executables compete for CPU/RAM. Adjust only **documented opt-in invocation recipes** initially; do not lock global jobs settings without clear benefit.
4. If disk is slow, document the evidence and optional relocation to faster storage in a user-owned directory (not an automatic move/clean); ensure resulting path is stable across branches and doesn't consume excessive disk.
5. Do not disable Defender/AV or unrelated background processes, and do not terminate active compiler processes owned by other jobs.

**Verification:** same workload and cache state with bounded job-count variations, complete resource evidence.  
**Done when:** selected job count and storage guidance are measured, or original settings retained.  
**Commit:** `docs(build): [B4-D] record measured compile parallelism choices`.

## B4-E — **Conditional** dependency feature audit

**Run only if cargo timings identify expensive dependency builds that can plausibly be reduced without feature loss.**

1. Use `cargo tree -e features`, `cargo tree -d`, Cargo.lock, code references, and relevant target-specific windows dependencies to identify feature union/duplicate versions and expensive crates. Do not assume `Cargo.lock`'s 710 records are all on the active build path.
2. Prioritize **reversible manifest-only** proposals: potentially unneeded default features on `eframe`, `rfd`, `reqwest`, `image`, `windows`, OCR/audio libraries or duplicate transitive feature activation. **Verify each against current feature usage**, including Windows-only APIs, panic/link/COM code and smoke binaries; never disable a feature just because a static grep misses dynamic usage.
3. Propose one dependency-feature change at a time, with before/after compilation graph, exact runtime functionality gate, Cargo.lock diff review and Windows smoke cases. Preserve pinned Git revision `rdev` unless a measured/behavioral reason requires otherwise.
4. Compare clean/cached dependency builds and incremental after-edit builds. A dependency prune may only improve cold build; report that accurately. Revert if typechecking/native behavior breaks or a dependency upgrade becomes necessary outside scope.
5. Prefer leaving expensive-but-required dependencies untouched rather than introducing subtle optional compilation gates. No blanket dependency upgrades.

**Verification:** targeted cross-feature binary and existing integration checks, direct Win32/GUI/native scenario if affected.  
**Done when:** every retained manifest change has proven feature parity and measured compile benefit, or proposal is rejected.  
**Review:** mandatory for any dependency/feature diff.  
**Commit:** `perf(deps): [B4-E] trim measured unused dependency features` only for accepted changes; otherwise docs-only decision.

## B4-F — **Analysis only unless separately justified:** crate extraction decision

1. Use cumulative timing reports to identify whether changing one GUI/plugin file causes rebuild of the entire shared library and many dependent test/binary artifacts. Determine if a **small stable and mostly pure** domain boundary exists with minimal cycles and few Windows interfaces.
2. Estimate avoided recompilations vs new crate boundary overhead, cross-crate visibility/API churn, new target cache costs, test migration and maintenance. Compare representative source edits and dependency graph for possible subcrates without implementing architecture first.
3. Produce a concise decision: **REJECT/DEFER** extraction by default; only consider a tiny proof of concept if estimated benefit is large and the necessary code/API changes are narrow and behavior-preserving. A general workspace split of Multi Launcher is outside the approved scope and must be proposed separately to the user.
4. If a bounded proof is approved, do it as its own reviewed checkpoint/branch-internal commit with exact before/after test and build gains. If not expressly approved, stop at recommendations; do not autonomously start crate extraction.

**Verification:** architecture/timing comparison, no code change by default.  
**Done when:** an evidenced decision exists; no speculative large refactor.  
**Review:** independent architecture review of rationale.  
**Commit:** `docs(build): [B4-F] assess crate-extraction cost and payoff`.

---
# 7. B5 — Final verification and comparable build-performance report

## B5-A — Confirm compilation workflows, coverage and production functionality

1. Audit the cumulative diff from initial `HEAD` against the final `build-optimization` branch. Confirm that no Track A runtime code was changed, no data formats were modified, and all Cargo build/test target names are intentional.
2. Validate canonical Windows **production** executable compilation with current effective release settings: `cargo build --release --bin multi_launcher`. Check that Windows icon/resources remain embedded and startup can load in an owned bounded native smoke. If toolchain/profile changed link behavior, exercise only the directly implicated native features (hotkey, window behavior, overlay, plugin load) and document exact evidence.
3. Validate other executables remain available (`cargo check --bin coordinate_tool_smoke`, `cargo check --bin passive_overlay_smoke`, `cargo check --bin radial_acceptance` or equivalent commands supported on host). Do not run long acceptance scenarios solely to prove each binary compiles; native smoke when changes actually affect linking or resource presentation.
4. Compare full Nextest discovered tests/ignored tests and target-to-test mappings against B3-A. Verify all logical cases remain reachable and selected representative grouped/native tests pass; no weakened tests, hidden targets or zero-selection happy-path scripts.
5. Confirm optional full-debug profile works as documented, standard test/debug assertions remain enabled where required, and iteration-profile executable is clearly distinct from release. Where CodeLLDB manual debugger validation is unavailable, report NOT RUN instead of “passed”.
6. Validate reproducibility of the new scripts on Windows PowerShell, including paths with spaces, environmental restoration, nonzero exit code propagation, and no unexpected dependencies on editor configuration/sccache/linker.
7. Independent reviewer checks risky changes: profile semantics, link toolchain, test grouping/coverage, cache scripts and Windows resources. Fix material findings before final reporting.

**Verification:** focused production binary, scoped tests affected by changed build features, test roster parity, owned native smoke when relevant.  
**Done when:** production functional equivalence and test coverage are supported; every not-run test is identified.  
**Commit:** `test(build): [B5-A] verify production release and Nextest coverage`.

## B5-B — Rerun matched workloads and publish accepted gains

1. On the same baseline Windows host/toolchain and source-edit fixtures, rerun the B0 baseline matrix for accepted changes, including **real small edit**, **real large edit**, **no-op**, **branch switch** and **targeted Nextest compile/link**. Do not merely run each unchanged command twice and label second result an incremental-edit win.
2. Record after data with the exact initial HEAD SHA, feature branch HEAD SHA, Cargo profile, target list, environment, storage/cache state, selected flags, median/range of runs, and exact timing artifacts. Explain failed/incomplete runs.
3. Use one per-scenario table: `scenario | baseline seconds | final seconds | percent/ratio where fair | compiler/link contribution | correctness gate | caveat`. Do not add speedups across separate operations or overgeneralize one iteration profile's speed to canonical release.
4. Report peak RAM, target directory size, linker/process behavior and cache miss/hit changes where measured. Include one distinct no-op result and explicit release-default/launcher-only comparison.
5. Keep a rejected-experiments table: alternative linker (if tried), sccache, jobs, dependency features, grouping, crate extraction. Record measured or safety reason for rejection; do not label a skipped experiment “failed”.
6. If an accepted change no longer improves a repeated workload or introduces a release regression, inspect/resolve; revert only the initiative-owned offending diff safely, then rerun the exact gate. The standard production profile remains unchanged by contract.
7. Avoid rewriting historical Track A results or comparing against older application code; build comparisons are from **current branch HEAD baseline** to Track B output.

**Verification:** independent reviewer checks methodology/claims and provenance; repeat representative strong result to screen for noise.  
**Done when:** a trustworthy report establishes actual user-relevant build/test wins, plus resource and compatibility tradeoffs.  
**Commit:** `docs(build): [B5-B] publish verified compilation before-and-after results`.

## B5-C — Final local commits, checkpoint summary and developer handoff

1. Keep `docs/performance/track_b_baseline.md`, `track_b_results.md` and `track_b_checkpoints.md` accurate; include local commit SHA per meaningful checkpoint and final branch HEAD. Do not fabricate completed stages where evidence was unavailable.
2. Review staged diff (`git diff --cached`), overall `git status` and `git diff <base>..HEAD` to confirm only owned build/docs/test-layout files changed. No secret logs, raw build trees, resource binaries accidentally modified or unrelated human work committed.
3. Ensure scripts are documented with recommended commands for **quick check**, **fast targeted Nextest**, **release-like iteration executable**, **full debugging**, **canonical production release** and **deliberate comprehensive verification**. Rust-analyzer/Neovim changes should remain deferred as requested.
4. Produce a readable final answer: what changed, per-stage commits, exact before/after 6-minute scenario result (or NOT MEASURED), test discoverability parity, narrow verification, native status, rejected experiments, lingering bottlenecks and highest-value next priority.
5. **Do not push. Do not merge.** Leave the local branch ready for user-controlled inspection/push/PR.

**Verification:** final stage ledger, documentation links/paths, `git status --short`, diff scope review.  
**Done when:** cleanly reviewable local feature branch and honest benchmark handoff.  
**Commit:** `docs(build): [B5-C] close Track B with build recipes and checkpoint ledger`.

---

# 8. Measurement/compliance acceptance matrix

| Requirement / gate | Establish in | Must be rechecked in | Success definition |
| --- | --- | --- | --- |
| Current branch `HEAD` is authority | B0-A | Every checkpoint | Baseline SHA recorded; no master switch/pull |
| Safe worktree/cache state | B0-A/B | All experiments | No unowned modifications or `cargo clean` on primary cache |
| No-op truly fresh | B0-C | B1/B2/B5 | No unplanned compile; reason documented if not |
| Source edit rebuild documented | B0-C | B1/B2/B5 | Same edit and cache state; consistent timing data |
| Windows icon/resource validity | B0-A | B1-A, B5-A | Resource edits trigger, unrelated edits don't; icon embedded |
| Canonical release unchanged | B0-C | B2/B4/B5 | Release profile flags and runtime feature behavior preserved |
| Named iteration safe | B2 | B5 | Distinct profile, reproducible compilation and documented runtime tradeoffs |
| Test safety assertions preserved | B0 | B2/B3/B5 | No silent assertion/debug safety reduction |
| Complete Nextest logical roster | B3-A | B3-C/D, B5-A | Every test maps once; ignored/platform tests accounted |
| Native/global-state test isolation | B3-A | B3-C/D, B5 | No unsupported grouping |
| Toolchain/cache experiment opt-in | B4 | B5 | No compulsory external tools or global RUSTFLAGS |
| User-focused build gains | B0-C | B5-B | Real edited-release, test build and branch-switch comparisons |
| Rust-analyzer configuration unchanged | Scope lock | B5 | No editor settings diff |
| Track A/runtime untouched | B0 source | B5 | No accidental unrelated code/data changes |
| Local-only Git checkpoints | B0 | Each commit/B5 | Recorded SHA, **no push/merge** |

## 8.1 Risk gates that halt specific changes

- **Resource changes fail to update launcher icon:** repair B1-A or revert; no “performance success” based on skipping needed resource compilation.
- **Test grouping loses one case or changes native/global-state semantics:** reject B3-C and retain standalone target. One missing test invalidates claimed coverage preservation.
- **New iteration profile is quick but fails runtime smoke:** reject profile or document use restriction; don't silently substitute for production release.
- **Linker produces native crash/PDB/resource defect:** immediately revert opt-in trial and document it; no default linker switch.
- **Dependency pruning disables Windows API/plugin code:** revert particular feature change; no hidden gating of required behavior.
- **Baseline compromised by profile/cache differences:** relabel unsupported comparison, rerun feasible fair scenario; never invent percentage.
- **Unsafe branch/worktree state:** pause changes and report factual Git conflict; don't reset user work.

## 8.2 B0–B5 checkpoint ledger template

| Checkpoint | State | Change / experiment | Tests/metrics | Reviewer | Local commit SHA |
| --- | --- | --- | --- | --- | --- |
| B0-A | pending | HEAD/host inventory | — | — | — |
| B0-B | pending | Measurement harness | — | — | — |
| B0-C | pending | Frozen baseline | — | — | — |
| B1-A | pending | Resource rerun control | — | — | — |
| B1-B | pending | Targeted build recipes | — | — | — |
| B1-C | pending | Low-risk verification | — | — | — |
| B2-A | pending | Debug/test symbols | — | — | — |
| B2-B | pending | Iteration profile | — | — | — |
| B2-C | pending | Incremental/codegen tuning | — | — | — |
| B2-D | pending | Release-profile separation | — | — | — |
| B3-A | pending | Test isolation/roster | — | — | — |
| B3-B | pending | Targeted Nextest recipes | — | — | — |
| B3-C | conditional | Grouping pilot | — | — | — |
| B3-D | pending | Test roster/build parity | — | — | — |
| B4-A | pending | Bottleneck ranking | — | — | — |
| B4-B | conditional | Alternative linker | — | — | — |
| B4-C | conditional | sccache | — | — | — |
| B4-D | conditional | Job count/storage | — | — | — |
| B4-E | conditional | Dependency feature audit | — | — | — |
| B4-F | decision only | Crate-extraction evaluation | — | — | — |
| B5-A | pending | Production + coverage gates | — | — | — |
| B5-B | pending | Final matched benchmarks | — | — | — |
| B5-C | pending | Local handoff | — | — | — |

**Total:** 23 checkpoints, including explicitly conditional experiments/decisions. `SKIPPED` or `NOT MEASURED` can be a correct evidence-based outcome for a conditional stage; do not treat that as failure to complete Track B.

## 8.3 Minimal per-checkpoint implementer handoff

```text
Checkpoint: B#-X — <concrete objective>
Baseline source: <exact initial HEAD SHA and branch>
Current working branch: build-optimization
Files/ownership: <exact current source files and relevant callers>
Existing behavior: <brief source-confirmed pre-state>
Required changes: <ordered implementation steps from plan>
Invariants: <release behavior, tests, Win32, data, branch safety>
Non-goals: <no editor config, no runtime optimization, no scope creep>
Benchmark: <BL-ID and fair cache/edit/profile conditions>
Verification: <target-specific Cargo/Nextest commands + assertions>
Reviewer: <required or optional with reason>
Done when: <objective conditions>
Commit subject: <type(scope): [B#-X] description>
Output: <diff, timings, test status, remaining uncertainty>
```

**Branch coordination:** If implementer cannot use host-linked Windows shell/real repo, it should report missing results and provide validated commands, **not** substitute Linux builds for Windows MSVC timings. The orchestrator should continue safe non-native analysis without claiming unavailable tests succeeded.

---

# 9. Final report format required from Codex

```text
TRACK B — FINAL STATUS
Starting branch + initial HEAD SHA:
Implementation branch + final local SHA:
Host/toolchain/actual linker/profile information:
No-op build (baseline → final):
6-minute after-edit release scenario (baseline → final):
Branch-switch compile scenario (baseline → final):
Targeted Nextest compilation (baseline → final):
Broad Nextest binary build (baseline → final):
Chosen fast iteration profile and cache/size tradeoff:
Tests before/after (counts, mapping, ignored/platform coverage):
Windows resource/icon verification:
Standard release functionality/performance preservation:
Conditional experiments accepted/rejected/skipped:
Source code, artifact and benchmark caveats:
Checkpoints and local commit hashes:
No remote push or merge performed:
Next recommended build improvements:
```

**Quality bar:** The project is complete when the original HEAD is traceable, the edited-build bottleneck was actually measured, at least the safe build/target/profile opportunities were investigated with controlled comparisons, application/test semantics are intact, and the branch is locally reviewable. A truthful result of “fewer targets and faster iteration profile, but canonical release still takes six minutes because of final LLVM codegen” is preferable to misleading speedup figures.
