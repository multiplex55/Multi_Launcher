# Multi Launcher — Performance Track D
## Bounded D1 Build/Test, D2 Responsiveness, D3 Idle Efficiency, and Optional D4 Memory Observation

**Plan status:** approved scope; executable by Codex without further user input.  
**Branch to create:** `perf-track-d` from **the exact `HEAD` checked out when Codex starts**.  
**Baseline is immutable:** record the initial commit SHA before creating the branch; do not switch to, fetch, pull, reset to, or rebase onto a named remote branch to choose the baseline.  
**Implementation style:** code-first, one writer, small checkpoint commits, minimal pertinent tests, evidence-backed optimizations, best-effort non-force pushes.  
**Source authority:** Codex's live working checkout at task start. This plan was informed by the currently inspectable repository, including `Cargo.toml`, `AGENTS.md`, the `dev/track_b` scripts, the `dev/run_owned_process.ps1` containment runner, the existing Criterion benches, and the opt-in Track C query observer. Reinspect relevant signatures at the starting HEAD; do not rely on historical branch names or archived plans.  
**Success definition:** a reliable baseline, bounded attempts at the highest-value proven bottlenecks, remeasured improvements where warranted, and an honest report including skipped/no-gain tracks. **Zero accepted optimizations is an acceptable result** if the evidence does not justify changes.

---

## 0. Objective, authority and strict non-goals

The user wants **only a post-expansion performance investigation and bounded optimization pass**:

- **D1 — Build and test compilation (highest priority):** incremental compilation, linker/PDB cost, selected Nextest build time, Rust Analyzer responsiveness, and impact of large module edits.
- **D2 — Application responsiveness (high):** startup/first usable frame, global hotkey-to-visible UI, query-to-results, dashboard rendering, radial opening and submenu switching.
- **D3 — Idle/background efficiency (high):** CPU usage, timer wakeups, memory/handle snapshots, mouse observers/cursor effects and Hot Folder activity.
- **D4 — Memory growth only (low, optional):** a single bounded soak/observation. No general lifecycle/refactor project.

Do **not** add launcher features, redesign UI, change preferences or saved-data schemas, overhaul egui, refactor unrelated modules, migrate dependency versions, add a generic telemetry platform, or re-run a historical feature acceptance program. No benchmarks may be used to weaken keyboard/mouse safety, hotkey priority, clipboard ownership, file safety, shutdown rules, or plugin behavior. No global system configuration changes, persistent environmental tweaks, firewall exceptions, or deletion of users' build caches.

**Optimization is not mandatory.** If a baseline already meets sensible performance expectations, or a candidate fails the acceptance thresholds, record it and move on. Do not create additional milestones to chase diminishing returns.

### 0.1 Existing tools and boundaries to reuse

Inspect the actual current revisions, but begin with these verified owners:

| Concern | Relevant existing ownership/tool |
|---|---|
| Cargo targets and profiles | `Cargo.toml`, existing `fast-dev` and `iteration` profiles, opt-in `dev-smoke-binaries` gating |
| Build measurements | `dev/track_b/measure.ps1`, `dev/track_b/cargo.ps1`, `dev/track_b/test_measure.ps1`, `dev/README.md` |
| Process-safe native runs | `dev/run_owned_process.ps1` and `dev/test_owned_process.ps1` |
| Query observability | `src/gui/track_c_query_observer.rs`, `src/gui/search.rs`, `src/gui/track_c_action_observer.rs` |
| GUI/root and startup | `src/gui/mod.rs`, `src/gui/render.rs`, `src/main.rs`, `src/startup_catalog.rs` if still present |
| Search and radial microbenchmarks | `benches/search.rs`, `benches/omni_search.rs`, `benches/radial_runtime.rs`, `benches/todo_widget_filtering.rs` |
| Radial orchestration | `src/radial/controller.rs`, other directly relevant `src/radial/` owners |
| Hot Folder driver | `src/gui/hotfolder_runtime.rs`; currently known to use an enabled-plugin `DRIVER_IDLE_TICK` of 80 ms; determine actual cost before changing it |
| Input observers / cursor effects | Established mouse-gesture, hotkey and platform owners; locate their live interfaces when D3 begins |
| Existing instructions | Root `AGENTS.md` governs ordinary code quality and authorized best-effort pushes; this task sets narrower measurement/testing limits |

The presence of tooling is **not** a mandate to run every available benchmark. Reuse selectively, extend narrowly, and avoid a second metrics framework.

---

## 1. Bounded execution contract — non-negotiable

### 1.1 Wall-clock and experiment budgets

Use **cumulative active wall-clock** measured from the start of each track, including source inspection, benchmark runs, compilation, review and repairs. Exclude user/Codex session pauses, but **include long build waits**. The orchestrator tracks elapsed budget explicitly in the ledger. Whichever limit is hit first ends the track.

| Stage | Hard active-time cap | Optimization candidates | Experiments per candidate | Native retries |
|---|---:|---:|---:|---:|
| D0 — Branch + bounded measurement scaffolding | **50 min** | None | None | At most one capability probe |
| D1 — Compile/test development loop | **180 min** | **2** | **2** | N/A |
| D2 — Interactive responsiveness | **140 min** | **2** | **2** | One normal attempt + one environmental diagnostic attempt total |
| D3 — Idle/background efficiency | **100 min** | **2** | **2** | One normal attempt + one diagnostic attempt total |
| D4 — Memory observation | **25 min** | **0** (observe only) | One bounded soak | No independent native retry campaign |
| D5 — Final closure | **45 min** | None | Repairs only | None |
| **Entire project** | **540 active minutes (9 hours)** | **6 max** | **12 max** | As above |

A candidate is one specific root-cause hypothesis with an isolated changed code/configuration boundary. Trying a different implementation of the same candidate consumes its **second** experiment; moving to another subsystem consumes another candidate slot. No extra candidate is granted because a previous one failed. If a tool cannot finish within the remaining budget, **stop, mark `blocked_environment` or `inconclusive`, commit the evidence and move on**.

Per-candidate cap: D1 **55 min**, D2 **35 min**, D3 **30 min**, inclusive of verification and remeasurement. **One correction attempt** for a failing directly affected test or failing harness is allowed per checkpoint; a second independent failure means record and move on unless the current diff would be left noncompiling (then revert only this candidate's changes safely). No loops of speculative repairs. One bounded read-only reviewer pass per accepted candidate; no continuous reviewer ping-pong.

### 1.2 Benchmark sampling budgets

- **D1:** 3 ordinary warmed/no-op command samples per selected command; at most 2 controlled edit/replay samples per selected profile/target; at most **one** isolated cold-cache build across D1, **only if it fits the remaining budget**. Never wipe or overwrite an existing target directory to manufacture cold conditions. At most **three** selected build/test command families in the primary comparison; optional Rust Analyzer is one separate limited observation.
- **D2:** up to **5 warmups + 15 measured interactions per scenario**, using fixed synthetic fixtures. At most **6 scenario families** in the baseline; an accepted candidate remeasures its directly affected scenarios plus **one** adjacent guardrail scenario. One independent confirmation batch only for close/noisy decisions.
- **D3:** up to **4 controlled idle scenarios** initially, **120 seconds measured each** after a **30-second settling period**; for a promising change, one matched before/after **300-second** confirmation in the affected scenario plus one guardrail scenario. Do not sample dozens of settings combinations.
- **D4:** a **single 15-minute** session (plus warmup/start/cleanup within its 25-minute stage), samples at **30-second** intervals. No multi-hour soak, no repeated stress cycles, no restart marathon.
- **Screenshots:** up to **16 small owned-window screenshots** total across D2/D3 and final evidence, captured only where a visual artifact helps attribute a timing or confirm functional UI. No blanket full-desktop collection; do not include user data.
- **New native executable harnesses:** **one** reusable opt-in performance probe target maximum, and **one** external launcher/driver script family maximum, unless an existing smoke binary already meets the need. Harness files are instrumental to these tracks only, not a new product feature.
- **Additional tests:** only tests covering the changed code and its direct integration boundary. No pre-optimization full suite. No speculative fuzzing or exhaustive regression matrices.

These are maxima, not targets. Do less whenever data answers the question sooner. If a trustworthy existing measurement already covers a scenario on the same machine/source/toolchain/config, reuse it as context, but do not present a historical number as a matched current-branch baseline.

### 1.3 No-progress, stop, keep-or-revert rules

Stop investigating an individual suspected hotspot and record `NO_GAIN` / `INCONCLUSIVE` when any of these occurs:

1. The allocated per-candidate time or two implementation experiments are exhausted.
2. Two matched measurement attempts cannot distinguish the change from natural variation.
3. The bottleneck is external/environmental, and one ordinary plus one diagnostic attempt do not establish an actionable project change.
4. Correctness tests fail after one bounded repair, or preserving behavior would require an unrelated architectural rewrite.
5. The subsystem is already below the meaningful absolute-impact threshold or contributes a negligible share of the full user interaction.

**Accept a change only when all conditions hold**: (a) sound root cause; (b) directly affected functional tests pass; (c) matched performance evidence meets the defined track threshold in **two consistent comparison batches** where practical; (d) no material regression in direct guardrails; (e) no input/UI/background safety invariant is weakened. If not, revert **only this task's isolated uncommitted candidate edits** after confirming the exact diff; do not use `git reset --hard`, delete caches, or disturb pre-existing/user work. A rejected experiment still gets a short ledger entry and can receive a docs-only checkpoint commit, not a forced code change.

### 1.4 Improvement thresholds (guardrails, not aspirations)

Use medians for broad performance decisions, p95 for tail latency, and paired before/after samples from a consistent host/toolchain/config. Report raw values, sample counts and noise. These default thresholds deliberately avoid micro-optimizing already-fast paths:

- **D1 edited build/test:** accept a **≥15% median improvement AND ≥10 seconds saved** on the actual edited path. `cargo check` may qualify at **≥20% AND ≥5 seconds**. A confirmed substantial *developer workflow* improvement that doesn't change production behavior may be documented even when not merged. No-op builds must not regress by >15% or >2 seconds unless a compelling independent gain outweighs it and is explicitly documented.
- **D2 GUI/hotkey/radial:** accept a **≥12% p95 reduction AND ≥3 ms absolute reduction** in a meaningful user-visible interaction; **startup** needs ≥10% AND ≥20 ms. Query microbenchmarks must show ≥15% AND ≥5 µs, and be connected to a meaningful query/user path before accepting code complexity. Other measured primary user interactions must not regress by >10% p95 or >5 ms without explicit rejection/rollback.
- **D3 idle:** accept a **≥25% reduction** in measured wakeups or CPU-time rate **AND** an absolute reduction of ≥100 wakeups/minute or ≥60 ms CPU time/minute. No degradation of responsiveness, watcher latency, hotkey behavior or shutdown safety. If idle CPU is already tiny, record `NO_GAIN` and avoid complexity.
- **D4:** observation only; record private bytes/working-set/handles and slope after warmup. A sustained rise of >25 MiB over the observation **and** >1 MiB/minute after warmup is a follow-up **signal**, not proof of a leak or authorization to fix it in this track.

If variance exceeds the apparent benefit (e.g., effects reverse across independent runs), say `INCONCLUSIVE`. Never claim an exact percent improvement from incomparable host/profiles/caches. Don't “fix” benchmark results by weakening representative workloads.

### 1.5 Required evidence discipline

For every measurement set, record:

- **Source fingerprint:** `git rev-parse HEAD`, working diff hash or recorded one-candidate patch, Rust/rustc/cargo/Nextest/Rust Analyzer versions where applicable, Cargo profile/flags, feature set, binary SHA, relevant settings fixture hash.
- **Host fingerprint:** Windows version, CPU logical processors, RAM, power/battery mode if readable, DPI/monitor conditions for GUI, app process architecture. Gather identifiers useful for reproducibility; avoid serial numbers/usernames.
- **Scenario:** fixture size/data pattern, command, environment overrides, cache classification, warmups and repetitions, timed start and stop semantics, success/failure, output path.
- **Metrics:** raw observations as small anonymized CSV/JSON, median/p95/range, comparison percent and absolute change, regression guardrails, acceptance/rollback reason.
- **Artifact handling:** detailed raw traces/dumps in a user-owned, locally ignored evidence directory (`target/perf-track-d/` or an explicitly verified writable external scratch path). **Commit small sanitized summaries, metadata and aggregate metric tables** under `docs/performance/track_d/`; never commit huge logs, private file paths, clipboard contents, full desktop captures, profiles with personal arguments or user screenshots.
- **Timing integrity:** distinguish `compilation`, `link/PDB`, `test execution`, `process launch`, `first usable frame`, `hotkey admitted`, `window visible` and `render/present` as separate phases; do not label one as another. Mark unavailable metrics `unmeasured` instead of inventing numbers.

Performance instrumentation must be opt-in, bounded-buffer/RAII or dedicated harness-only, quiet when disabled, and removable/maintainable. If probes themselves cost >5% of the observed metric, either correct measurement or report `instrumented_only`, not a runtime win.

---

## 2. Autonomous execution and Git protocol

### D0-A — Create **the new branch** before implementation or benchmarks

**Owner:** parent/orchestrator, no implementation agent yet.  
**Maximum:** 10 minutes, within D0.

1. In Codex's **current repository/worktree**, run `git rev-parse --show-toplevel`, `git rev-parse HEAD`, `git branch --show-current`, `git status --short`, `git remote -v` and `git branch --list 'perf-track-d*'`. Record the exact starting SHA and status; never assume the starting branch is `master` or an older cleanup branch.
2. If clean and `perf-track-d` is available, create it with `git switch -c perf-track-d` **at that exact current HEAD**. If occupied, use the first available `perf-track-d-2` or `perf-track-d-3` (three tries total); never delete, reset or overwrite an existing branch.
3. If the initial worktree has uncommitted user changes, **do not overwrite, stage or carry them into performance commits**. Prefer creating a separate clean `git worktree add -b perf-track-d <safe-sibling-path> <captured-HEAD>` and continue there. Verify that the new worktree points to the captured HEAD; keep all existing files/worktrees intact. If safe isolation fails, stop and report an initial blocker rather than trashing user work.
4. Verify `git rev-parse HEAD` is **exactly equal** to the recorded starting SHA and the new branch is active. Do not run pull/rebase/merge just to improve recency.
5. Do not perform a baseline full Nextest run; **assume existing unit tests pass at the starting HEAD** as the user directed.
6. Record branch name, immutable baseline SHA, current worktree, active clock start, branch creation method, and any existing user changes in `docs/performance/track_d/progress.md` and `baseline_manifest.json`.

**Done:** branch created at the exact original HEAD, user changes preserved, D1–D4 budgets instantiated. No source optimization yet. If branch cannot be created, this is the **only** startup blocker that warrants stopping the job.

### D0-B — Code-first measurement runner and evidence contract

**Owner:** one implementer; bounded read-only planner review optional.  
**Maximum:** 25 minutes; no unrelated architecture search.

1. Inspect existing `dev/track_b/measure.ps1`, `dev/track_b/cargo.ps1`, `dev/run_owned_process.ps1`, Criterion benches and Track C observer. Decide which can be reused unchanged.
2. Introduce only missing orchestration under `dev/perf_track_d/` (PowerShell 7 scripts and a tiny aggregation/fixture tool if needed). Prefer standard-library PowerShell/.NET timing, process counters, ETW/performance-counter facilities when already available, and Criterion's existing output. Avoid new production dependencies.
3. Runner must use argument arrays (no stringly evaluated command), write unique run IDs, log actual exit codes, and always restore temporary env/CWD/owned fixture files in `finally`/RAII. Never delete `target/debug`, `target/release`, existing caches or all temp data.
4. Create a small stable summary schema (`version`, `track`, `scenario`, `baseline_sha`, `candidate_sha_or_diff`, `profile`, `settings_hash`, `host`, `samples`, `metrics`, `limitations`, `decision`). Small aggregate JSON/CSV are source controlled; raw logs remain ignored.
5. If a custom executable becomes necessary, **add at most one explicit `[[bin]]`** using the existing `dev-smoke-binaries` required-feature mechanism, or reuse an existing smoke binary. Confirm plain `cargo build`/`cargo build --release` continue to select **only** `multi_launcher` as an application binary. Do not create an ungated `src/bin/` file.
6. Verify runner syntax/argument handling and one **short, harmless** dry run, no full Cargo compilation just to test the logger. Abort harness expansion at 25 minutes; an incomplete optional harness is not a reason to delay D1.

**Done:** reproducible minimal runner, privacy/ownership constraints enforced, evidence directories known. **Commit:** `chore(perf): [D0-B] add bounded measurement runner` (if no code needed, commit the ledger/recipe only).

### D0-C — Define owned, reproducible scenarios and baseline governance

**Maximum:** remaining D0 time (15 minutes target).

1. Define **synthetic** action catalogs/query sets, dashboard fixture, radial rings/menus, watched folders and dummy Windows target/control names. Use deterministic seeds and clearly marked fixture roots; no real home-directory or clipboard content. Pin the fixture schema/hash.
2. Establish event definitions for D2 (process start/first usable frame, physical-input observed/usable frame, query admission/result ready, radial request/first actionable ring, submenu request/first actionable submenu). Where existing observers measure only a portion, distinguish them.
3. Set matched workloads, environment capture, neutral screenshot names, and expected stable behavior before any optimization code. Prevent benchmarking state from modifying persistent user settings.
4. Populate the `progress.md` checkpoint index and decisions table. Prefer one writer and sequential commits. Verify `git diff --check` and only directly affected harness checks.

**Done:** unambiguous scenario manifests and reproducibility fingerprints. **Commit:** `docs(perf): [D0-C] define controlled track-d workloads`. Push best-effort and move into D1 whether or not all optional native metrics are available.

---

## 3. D1 — Build and test compilation (**hard limit 180 minutes**)

**Goal:** reduce actual edit→feedback time, not merely achieve an impressive clean release build. **Prioritize D1 over D2/D3.** Existing `fast-dev` and `iteration` profiles are already documented and must be treated as previous experiments, **not automatically replaced**. Maximum 2 code/recipe candidates, 2 implementation variants per candidate, 55 minutes/candidate.

### D1-A — Baseline real developer workflows (no source optimization)

**Target 25 minutes.** Use the existing Track B process measurement script where possible. Measure only:

1. **Developer loop A:** `cargo check --lib` after a controlled single-file edit; record no-op separately.
2. **Developer loop B:** a selected library Nextest test **build versus execution** (explicit `--lib` / `--no-run` or a selected stable test), plus its actual execution time; record the exact case name. Do not use an unfiltered Nextest command.
3. **Developer loop C:** `cargo build --bin multi_launcher` after a controlled local edit; distinguish compile and link if timings support it. If a release comparison is justified and fits budget, perform **one** opt-in `iteration`/release evidence pair; **do not** rerun a giant release campaign by default.

Sampling: 3 no-op warmed runs and at most two controlled edits/replays per family, always with hashed restored source; no extra profiles unless a clear bottleneck is isolated. Existing developer caches remain intact. A separately isolated cold-cache build is permitted **once** only when it is important enough to justify its disk/time cost. `cargo --timings` may help identify expensive units; do not use nightly unstable compiler flags as a prerequisite.

**Output:** baseline command table with wall seconds, compilation/link attribution or `unavailable`, cache/flags, summary JSON/CSV, and dominant cost fraction. **Done:** at least two relevant build families measured OR environmental blocker recorded. **Commit:** `bench(perf): [D1-A] record developer-loop baseline`.

### D1-B — Attribute incremental/link/PDB and Rust Analyzer cost

**Target 25 minutes; no more than two expensive probes beyond D1-A.**

1. Inspect timings and build graph to rank top three measured owners by **critical-path time**, not merely total crate count. Separate Rust compiling, codegen/LLVM, Windows link/PDB, proc macro/codegen, test binary link, build scripts and no-op overhead where measured. File/module size alone is not proof.
2. Inspect the latest Rust Analyzer binary availability (`rust-analyzer --version` or configured executable). If present, run **one** bounded headless/LSP request sequence on a synthetic or already-open file, capturing startup/diagnostics-completion and timeout. Do not install it, edit Neovim config, or change user-specific global IDE settings. If not available, mark `RA_NOT_MEASURED` and continue.
3. Check whether a change in a large module (e.g. root GUI, startup or input coordinator) invalidates more downstream units than a small leaf-module edit; use already established source-edit fixtures or one strictly restored example, not a sweep of every file.
4. Produce **ranked candidate sheet**: suspected root cause, evidence source, expected absolute savings, risk, proposed limited code boundary, acceptance thresholds, and whether the older `fast-dev` / `iteration` profiles already address it.

**Done:** the two highest-value *actionable* candidates selected or no-change decision justified. **Commit:** `docs(perf): [D1-B] attribute compile and linker bottlenecks`.

### D1-C — Candidate 1: one proven high-cost build/test owner

**Target ≤55 minutes inclusive of D1-D remeasurement.**

1. Select **one** candidate from D1-B, such as removing a specific high-fanout dependency in a frequently edited module, correcting an unnecessarily rebuilt test target, or a measured low-risk build recipe improvement. Do **not** split the entire GUI into crates, broadly change dependency features, or migrate the linker without controlled and attributable evidence.
2. State an exact pre-change hypothesis and touched file list in the ledger. Implement the smallest complete improvement, optionally via a scoped profile/helper flag only for developer builds.
3. Verify directly affected compile and the narrow relevant test. Max **one repair** if needed. Never modify production release semantics solely to accelerate development.
4. Do not commit as a performance **win** until D1-D decides keep/revert. Preserve a checkpoint with premeasurement/hypothesis in the ledger while the candidate is uncommitted.

### D1-D — Paired measurement and accept/reject candidate 1

1. Replay **only** its corresponding D1-A workload with an equivalent edit/caching state, 2 matched edited-run samples + applicable no-op guardrail. Record all raw per-sample observations and differences; one extra independent confirmation only if a nearly threshold result could be noisy.
2. Apply D1's ≥15% **and** ≥10s (or check ≥20%/≥5s) improvement rule, preservation of behavior and no material no-op regression.
3. **Accept:** commit exact source and evidence: `perf(build): [D1-D] reduce measured <owner> cost`. **Reject:** revert only these task-owned uncommitted source changes, record `NO_GAIN` or `INCONCLUSIVE`, commit evidence as `docs(perf): [D1-D] record rejected build candidate`.
4. No third implementation approach. If no actionable candidate was identified, D1-C/D become one recorded `SKIPPED_NO_CANDIDATE` decision, without extra code.

### D1-E — Candidate 2: next independent measured owner (optional)

**Target ≤55 minutes inclusive of D1-F.** Only start if an independent actionable bottleneck survives D1-D and enough track budget remains. Do not reinterpret D1-E as an opportunity to optimize the same failed candidate endlessly.

1. Select the second-ranked D1-B item; check it still dominates after accepted candidate 1.
2. Implement one direct improvement, allowing a single bounded alternative and narrow tests, with no unrelated rewrite.
3. Stop immediately if estimated savings are below absolute-impact threshold or if consumed track budget makes confirmation impossible.

### D1-F — Paired measurement and accept/reject candidate 2

Repeat D1-D only for candidate 2; compare against the **current pre-D1-E checkpoint** and give a cumulative view versus original D0 baseline, ensuring profile/cache are comparable. Preserve no-gain evidence. **Commit:** `perf(build): [D1-F] ...` or `docs(perf): [D1-F] record outcome`.

### D1-G — Close D1 and freeze developer-workflow advice

**≤15 minutes or remaining budget.**

1. Summarize each measured command, before/after absolute time, % change, sample noise, accepted/rejected candidates, remaining compiler/linker hotspots, effects on `fast-dev` / `iteration`, and any Rust Analyzer observation.
2. Update only directly relevant `dev/README.md` or a new Track D recipe; **preserve** documented profiles unless measured evidence warrants a narrowly scoped change. Any new profile is opt-in and must not silently alter canonical release.
3. Record `D1_COMPLETE`, `D1_NO_GAIN` or `D1_PARTIAL_ENVIRONMENT`; cap remaining D1 work. **Commit:** `docs(perf): [D1-G] close build-test findings`.
4. Move to D2 **even if D1 found zero wins**.

---

## 4. D2 — Application responsiveness (**hard limit 140 minutes**)

**Goal:** identify and improve at most **two** impactful UI/runtime latency bottlenecks. The D2-A/B/C/D/I targets plus two inclusive 35-minute candidate windows total **140 minutes**; skip candidate 2 if earlier work consumes its budget. Reuse opt-in `track_c_query_observer`, existing radial benches and real native app instrumentation where possible. Do not spend the entire track building a general UI automation system.

### D2-A — Add only missing opt-in end-to-end receipts / safe harness

**Target ≤15 minutes.**

1. Inspect existing startup timing, query observers, GUI frame boundaries, radial controller transitions and existing smoke executables. Define which metrics are **internal** (admission→ready) versus actually user-perceived (hotkey→first actionable visible frame). Do not claim a root-frame timestamp proves compositor display.
2. Add only missing opt-in event stamps (lightweight bounded ring or event log under a perf-run flag), with monotonic clock and invariant identifier. No user query text/window title/paste content in output. With the flag absent, no steady-state allocations or polling should result.
3. If native interaction measurements require it, create/reuse **the one allowed opt-in performance EXE** or minimal PowerShell owner driver. **Explicitly gate any new binary** via existing `dev-smoke-binaries` and `autobins=false`; prefer test fixture HWND/PID markers and existing containment script. All synthesized input must target the owned fixture or an independently verified app HWND/PID; never type into unrelated foreground apps. All keys/mouse Down have paired releases even on failure; never block the user's global input or modify user clipboard.
4. Screenshots are optional proofs of correct synthetic GUI state; limit to owned launcher/fixture bounds and use the global 16-image cap. One native run and one environmental diagnostic retry maximum; an access-denied input/cursor/desktop boundary becomes `NATIVE_NOT_RUN` and D2 continues with deterministic measures.
5. Minimal compilation/smoke of changed observer or harness only. **Commit:** `chore(perf): [D2-A] add bounded UI latency receipts` (or docs-only if existing instrumentation suffices).

### D2-B — Baseline startup, hotkey visibility, and query responsiveness

**Target ≤20 minutes.** For up to **three** scenarios:

1. **Startup:** process launch→first usable launcher/root frame; include separate startup-load and first-render intervals if instrumented. Record p50/p95 and any frame stalls; use an isolated synthetic settings/data root, avoid real indexing directories.
2. **Hotkey:** validated launcher activation event→first usable visible root frame, including already-running hidden-to-visible, hotkey contention/short-tap only when fixture permits. Preserve configured launcher hotkey and existing input ownership.
3. **Query:** fixed empty, short, long and prefix query stimuli over deterministic action catalogs (e.g., 100 and 5,000), query admission→results accepted→first usable result frame. Reuse Track C counters if applicable; record actual number of results and provider readiness, not only matcher nanoseconds.

Use ≤5 warmups +15 measured samples per scenario. Run one targeted existing Criterion query benchmark only if it directly isolates a discovered issue, not automatically all benches. Record stable p50/p95 and whether presentation timing was measured natively or inferred internally. **Commit:** `bench(perf): [D2-B] baseline startup hotkey and query latency`.

### D2-C — Baseline dashboard frames and radial/menu switching

**Target ≤20 minutes.** Up to **three** scenarios:

1. Dashboard first useful frame and steady-frame CPU/render cost for one **small** and one **realistic** deterministic widget configuration (do not open all dialogs or enable every expensive plugin). Use existing layout/editor conventions; record whether data providers were already warmed.
2. Radial invocation→first actionable ring with a **small** and **representative** menu, and parent→child submenu transition. Record internal controller/scene phases and actual visible-frame evidence if supported; distinguish overlay/topmost failure from slow rendering.
3. Optional one related radial Criterion case (existing `benches/radial_runtime.rs`) only if the controller trace indicates geometry, scene or projection as dominant; otherwise skip microbench compilation.

Use same sampling rules, and at most 8 of the total screenshots in the D2 baseline phase. **Commit:** `bench(perf): [D2-C] baseline dashboard and radial latency`.

### D2-D — Attribute latency and choose at most two candidates

**≤10 minutes.** Rank user-perceived workloads by p95 **absolute milliseconds** and percentage attributable to owned code, including frequency of occurrence. Name two actionable hotspots with exact ownership; examples of **hypotheses to measure, not assumed defects**: repeated query projection/allocation, repeated dashboard widget state work, unnecessary radial scene construction, or redundant startup work. If no hotspot exceeds the D2 acceptance floor, write `NO_GAIN_EXPECTED` and skip D2-E–H.

**Commit:** `docs(perf): [D2-D] rank actionable UI latency`.

### D2-E — Candidate 1: smallest root-cause responsiveness fix

**≤35 minutes INCLUDING D2-F.** Change one measured critical path only, reuse current caching/invalidation semantics, and preserve visible output, deferred search consistency, hotkey semantics, dashboard state, and radial event ordering. Add/update one focused regression when an invariant changes; no unrelated state-machine migration. If the best fix would require a broad owner rewrite, explicitly defer it.

### D2-F — Measure candidate 1, regression gate, keep or revert

Repeat its measured scenario and **one adjacent** direct guardrail with matched synthetic fixtures; use p95, median and outliers. Require D2 ≥12% and ≥3ms p95 improvement (startup ≥10% and ≥20ms; query microcase ≥15% and ≥5µs plus end-to-end benefit), direct tests pass and no meaningful >10% / >5ms adjacent p95 regression. **Commit source + metrics on acceptance; otherwise revert own uncommitted candidate source, commit rejection note.** One reviewer read-only pass at most.

**Commit:** `perf(ui): [D2-F] improve measured <owner> latency` or `docs(perf): [D2-F] record no-gain candidate`.

### D2-G — Candidate 2 (optional)

Only if another high-value independent bottleneck survives and time remains. Use the same **≤35-minute inclusive D2-H** budget and no more than two implementation variants. Do not attempt to improve already-faster low-cost micro paths just to produce a second commit.

### D2-H — Compare candidate 2 and decide

Use identical before/after direct scenario + guardrail evidence and the same keep/reject rule; if rejected revert only owned uncommitted changes. **Commit:** `perf(ui): [D2-H] ...` or `docs(perf): [D2-H] record outcome`.

### D2-I — Close UI responsiveness track

**≤5 minutes or remaining budget.** Produce a 1-page summary table across the five core categories: startup, hotkey visibility, query, dashboard, radial/submenu. Include internal-vs-native measurement distinction, screenshots index, absolute baseline and final p50/p95, accepted changes and no-gain/untested paths. Keep native limitations explicit and preserve synthetic fixture scripts. **Commit:** `docs(perf): [D2-I] close responsiveness findings`. Proceed to D3 regardless of outcome.

---

## 5. D3 — Idle and background efficiency (**hard limit 100 minutes**)

**Goal:** avoid meaningless idle wakeups/CPU work and growth of background services without slowing useful events. D3-A/B/G plus two inclusive 30-minute candidate windows total **100 minutes**; preserve the track ceiling even if a fixture needs correction. At most two candidates, two variants each, no blanket rewrite of the scheduler/hotkey infrastructure.

### D3-A — Add bounded idle probes only where necessary

**≤10 minutes.** Inspect existing background owners (Hot Folder driver, mouse observations, cursor effects, watcher/snapshot publish, radial idle schedulers). Use process CPU time / wall rate, private bytes/working set, handle/thread count and bounded event/tick counters. Introduce a trace counter only if external OS sampling cannot identify cause. **No unconditional periodic performance thread** in production. All measurements should be tied to an owned PID and isolated settings fixture; do not use arbitrary foreground apps or inspect private file contents. **Commit:** `chore(perf): [D3-A] record idle owners and counters`.

### D3-B — Run the four-scenario idle baseline

**≤20 minutes**, **30 seconds settle + 120 seconds observation each**:

1. Launcher hidden, all optional mouse/cursor effects and Hot Folder disabled.
2. Launcher hidden, Hot Folder plugin enabled but **zero enabled rules**; retained disabled rules in an isolated test root if relevant.
3. Launcher hidden, exactly **one enabled test rule** watching an owned empty directory, with no destructive action and no real filesystem events; measure quiet baseline/event latency separately as needed.
4. Launcher visible but idle (dashboard shown), with mouse observations/cursor effects toggled **one at a time only where safe**; compare with matching hidden scenario, not a factorial matrix.

Record CPU seconds/minute, wakeups/minute if obtainable, snapshot/publication counts, idle UI frame/wake rate, process private bytes/working set, handle/thread counts and owned watcher count. If OS wake-up count permissions fail, use explicit bounded owner counters rather than inferring them from CPU alone. Never assert that `DRIVER_IDLE_TICK` is bad without measuring the cost and necessary responsiveness. **Commit:** `bench(perf): [D3-B] baseline idle and background rates`.

### D3-C — Candidate 1: most expensive confirmed idle owner

**≤30 minutes INCLUDING D3-D.** Prefer changing a measured idle-only scheduling/snapshot boundary without touching business rules or input hook priority. A **potential** candidate is Hot Folder idle work when zero rules are enabled; measure before choosing. Consider event-driven sleeps or reduced expensive snapshot computation only if existing request/event wake contract and shutdown guarantees can remain intact. No new extra worker, duplicate global mouse hook or high-frequency timer.

### D3-D — Verify D3 candidate 1

Run directly affected targeted tests and one matched **300-second** before/after rate confirmation plus one guardrail (e.g., enabled owned watcher event processing or hotkey activation). Accept only if ≥25% and ≥100 wakeups/min or ≥60ms CPU/min reduction; no degradation of watcher responsiveness or shutdown correctness. If not meaningful, revert own changes and mark `NO_GAIN`. **Commit:** `perf(idle): [D3-D] ...` or evidence-only no-gain.

### D3-E — Candidate 2: independent background owner (optional)

**≤30 minutes INCLUDING D3-F.** Candidate can concern mouse observation, cursor effects, redraw scheduling, or another measured idle owner. Must be separately attributable after candidate 1; avoid large UI changes and all user-visible effect redesigns. Preserve enabled effects' responsiveness and idle disabled behavior.

### D3-F — Verify D3 candidate 2

Matched affected rate and one direct guardrail, same keep-or-revert thresholds and focused tests. Never turn background optimization into application-wide keyboard/mouse event refactoring. **Commit:** `perf(idle): [D3-F] ...` or evidence-only no-gain.

### D3-G — Close D3 and document remaining idle costs

**≤10 minutes.** Record breakdown by hidden/no-rule/one-rule/visible idle scenario, before/after, absolute CPU-time and wake reductions, observed memory/handle/thread counts, any trade-offs/limitations, and deferred non-actionable items. **Commit:** `docs(perf): [D3-G] close background-cost investigation`.

---

## 6. D4 — Optional memory-growth observation (**hard limit 25 minutes; not a required optimization**)

This user reports no memory-growth problem and has explicitly deprioritized long-running stability testing. D4 **must not** grow into repeated launch/hide cycles, broad hook handoff tests, overlay shutdown refactors or a leak-hunting campaign.

### D4-A — Gating decision

**≤5 minutes.** Run only if D1–D3 are complete/recorded, at least 20 minutes of active budget remains for the single memory session and native process ownership is reliable. Otherwise mark `SKIPPED_OPTIONAL`, cite why, and proceed to D5. No extra setup runs to make D4 possible. **Commit:** combine decision with D4-B or D5-A; no fake milestone commit.

### D4-B — One 15-minute owned-process memory observation

**≤20 minutes including cleanup.** Launch owned app with isolated synthetic settings (or reuse safely owned running fixture), settle briefly, then sample private bytes, working set, handle and thread count every **30 seconds for 15 minutes**, at most one cycle of 5 show/hide transitions if they are already supported by the harness without risk. Compare start-after-warmup to final and compute simple slope; include whether caching/GC/font atlas allocation could explain growth. **Do not** inject destructive actions, write user data or switch global desktops. Release owned process and confirm it exited. A material upward trend becomes a **future follow-up issue**, not permission to chase a leak now.

**Output:** brief observed memory chart/table, raw anonymized sample CSV, `PASS_OBSERVATION` / `POTENTIAL_GROWTH` / `NOT_RUN_ENVIRONMENT`, and clear noncausal limitations. **Commit:** `bench(perf): [D4-B] record bounded memory observation` if performed; otherwise document the skip in D5-A.

---

## 7. D5 — Integration, review and closeout (**hard limit 45 minutes**)

### D5-A — Reconcile source, evidence and behavior

**Target ≤15 minutes.**

1. Inspect entire diff relative to the exact D0 starting SHA and compare accepted code against its owning subsystem only. Ensure temporary edit fixtures restored by hash and no foreign/user artifacts staged.
2. Review key compatibility guarantees: no changed launcher hotkeys/UI behavior, no changed JSON configuration/default semantics, no degraded shutdown or input ownership, no new default bin targets, no unbounded instrumentation when disabled, no uncaught runtime logging of private data.
3. Run **only the directly affected integration tests** and minimum compilation checks that cover the accepted modifications; do not re-run already-passing isolated test groups gratuitously. For D1-only profile/tool changes use CLI metadata/profiles or a targeted selected compile instead of GUI tests. For D2/D3 source changes run the relevant `--lib`, `--bin multi_launcher` or exact integration target names; rerun one failed assertion once after a narrow repair.
4. **Do not run full Nextest by default.** If the accepted changes alter genuinely shared behavior across several subsystems and focused tests cannot credibly validate it, permit **one** full `cargo nextest run --workspace` **at this final checkpoint only**, only if the remaining 45-minute closure budget permits; no full-suite repair loops. If it cannot finish on budget, mark deferred rather than extend scope. Initial baseline is assumed green.
5. Check the new perf harness remains opt-in and `cargo metadata` still shows exactly one un-gated app binary. Confirm native owned-process cleanup and final Git status.

**Commit:** `test(perf): [D5-A] verify accepted performance changes` (or evidence/ledger only if no code changes).

### D5-B — Publish concise final benchmark report and future backlog

**Target ≤15 minutes.** Build `docs/performance/track_d/results.md` and `progress.md` plus small anonymized metrics files. Include:

- exact branch/start SHA/final SHAs, accepted and rejected experiments, one normal push attempt status per commit;
- 4-track summary (including skipped optional D4), workload description, machine/toolchain/profile/fixture hashes, p50/p95 and absolute gains, confidence/variance, benchmark overhead caveats, native screenshot index if any;
- unchanged behavior verification and test commands/outcomes, environmental limitations, unverified assumptions;
- **at most five** follow-up backlog items ranked by estimated user benefit, **not** additional work in this plan;
- branch status for local-only versus pushed commits, build/test recipe and how to reproduce **selected** benchmarks.

If accepted changes produce no meaningful gain, state `No measured optimization met the acceptance threshold` prominently and consider the project successfully bounded. Do not invent a success percentage. **Commit:** `docs(perf): [D5-B] finalize track-d results`.

### D5-C — Final maintained-target Clippy / formatting

**Remaining ≤15 minutes**, the **last automated verification**.

1. Run `cargo fmt --check` / touched-file `rustfmt --check` as appropriate **before** Clippy; keep a controlled list of changed Rust source to avoid vendored formatting.
2. Run **one** maintained-crate Clippy sweep on the selected package/targets (`cargo clippy --workspace --all-targets -- -D warnings` where correct under the actual workspace graph). **Exclude vendored crates as workspace targets;** dependencies may still compile and emit their own warnings. Do not lint unrelated manually vendored source. If Clippy's known build/toolchain cost exceeds remaining budget, document `NOT_RUN_BUDGET` rather than claim success.
3. For a new owned lint in modified code, perform one bounded correction and rerun the **same focused Clippy** once. No broad cleanup of preexisting unrelated warnings and no post-Clippy full Nextest. If changes require direct changed-test revalidation after a lint correction, do so **before** the final successful Clippy command and inside the same D5 budget.
4. If all required checks pass, make the final commit (if needed) `chore(perf): [D5-C] finish maintained-code lint`; otherwise record precise unfinished validation without extending track. Attempt one best-effort normal push. Stop.

### 7.1 Final handoff: one-shot user review

After D5, report final outcomes in one concise handoff, explicitly separating **implemented and measured**, **implemented but not native-validated**, **measured/no gain**, **environment blocked**, and **optional skipped**. Do not ask user to run tests during milestones. User performs **one** manual review after autonomous work is complete.

---

## 8. Milestone and checkpoint matrix

Every completed checkpoint must update `docs/performance/track_d/progress.md` with status, time consumed, experiment count, touched files, metrics/evidence, tests, commit SHA and push result. Checkpoints without a code change still commit meaningful measurements/decisions, not empty commits. One source writer; optional bounded read-only planner/reviewer. No concurrent Cargo processes using the same target directory.

| Stage | Checkpoints | Gate for moving on |
|---|---|---|
| D0 | A branch; B runner; C scenarios | Exact initial HEAD preserved; minimal evidence contract established |
| D1 | A baseline; B attribution; C/D candidate 1; E/F candidate 2; G summary | ≤180 minutes, ≤2 candidates, accepted gains measured or no-gain recorded |
| D2 | A opt-in probe; B startup/hotkey/query; C dashboard/radial; D triage; E/F candidate 1; G/H candidate 2; I summary | ≤140 minutes, ≤2 candidates; no invasive GUI redesign |
| D3 | A instrumentation; B baseline; C/D candidate 1; E/F candidate 2; G summary | ≤100 minutes, ≤2 candidates; idle gain measured or no-gain recorded |
| D4 | A skip/run decision; B one memory observation | Optional, ≤25 minutes, **no optimization scope** |
| D5 | A focused verification; B report; C Clippy last | Complete or honestly bounded partial handoff, no hidden retries |

**No additional D6, D7 or “one more pass” stage.** Any reopened track would require a future task, not this plan.

---

## 9. Example evidence rows and decision language

### Measurement row format

| Checkpoint | Scenario | Baseline | Candidate | Delta | Samples | Decision | Reason |
|---|---|---:|---:|---:|---:|---|---|
| D1-D | Edited selected lib-test compile | *record seconds* | *record seconds* | *record %/seconds* | 2 paired | `KEEP` or `NO_GAIN` | Real measured critical path |
| D2-F | Radial first actionable ring p95 | *record ms* | *record ms* | *record %/ms* | 15 each | `KEEP` or `INCONCLUSIVE` | Native or internal semantics noted |
| D3-D | Hot Folder zero-rule idle wake/min | *record rate* | *record rate* | *record rate/%* | 300 sec paired | `KEEP` or `NO_GAIN` | Watcher latency guardrail retained |
| D4-B | Memory private-byte slope | *after warmup* | *15-min final* | *MiB/min* | 30-sec samples | `OBSERVED` | No leak claim without stronger evidence |

Replace placeholders with actual values only. Never fabricate any sample, test result, screenshot or speedup.

### Stop-record language

`D2-G SKIPPED_NO_CANDIDATE: After the highest-impact user-visible path was addressed, all remaining measured p95 costs are below the configured ≥3 ms absolute-impact floor; proceeding to D3. No additional GUI changes.`

`D3-D NO_GAIN: Candidate reduced timer wakeups in a synthetic owner test, but matched whole-process CPU time changed by less than the noise floor; reverted task-owned code and kept observation artifacts.`

`D4 SKIPPED_OPTIONAL: D1–D3 exhausted the active-work budget; current application has no user-reported growth symptom; no leak conclusion asserted.`

---

## 10. Absolute termination conditions

The orchestrator must terminate this task when any of the following is true:

- D0–D3 completed or explicitly closed `NO_GAIN`/`BLOCKED`, D4 performed or skipped, and D5 reported.
- The **540-minute active-work ceiling** is reached; spend any remaining action on safe teardown, a minimal ledger and local commit/push attempt, not further experiments.
- Environment blocks safe native input/owned-process control after one normal and one diagnostic attempt; native-dependent scenarios are marked unavailable, not retried repeatedly. Continue source-level D1/other D3 where possible.
- No accepted optimization candidate can pass the predefined improvement/guardrail threshold within its two variants, or safe rollback cannot be guaranteed.
- User data/branch/worktree safety would be violated by continuing; preserve current local commits, explain exact blocker, and stop without force operations.

**Final deliverable is an evidence-backed, bounded performance improvement attempt—not an ever-expanding benchmarking project.**
