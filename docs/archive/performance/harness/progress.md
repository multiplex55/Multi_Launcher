# Benchmark harness execution ledger

Baseline: `3a989d56abe1b203902e0c6eb6075ae58f6235ef`; starting branch
`perf-track-d`; clean tracked/untracked status. Created `perf-harness` directly
at that SHA. Origin is `multiplex55/Multi_Launcher`; no history synchronization.

Started 2026-10-18. Absolute budget 260 minutes; stage caps 20/75/95/35/35.
Only benchmark infrastructure is authorized; production optimization is excluded.

## H0-A — complete

Commands: `git rev-parse HEAD`, `git branch --show-current`, `git status --short`,
`git remote -v`, upstream query, `git switch -c perf-harness <baseline>`.
The sandbox initially denied `.git/index.lock`; normal approval allowed branch
creation. No user changes were present. Read current AGENTS.md and master plan.
Cargo metadata before additions: ignored `target/perf-harness/metadata-before.json`.
No native execution or performance metrics yet. Track D reports moved into
`docs/archive/performance/track_d/` in this checkout.

Checkpoint commit/push results are recorded by subsequent ledger entries.

## H0-B — complete

Contract: `contract.md`. Inspected manifest, existing owned runner, Track B
argument-array measurement and archived Track D denial/owner map. Radial's
existing public production paths provide the CPU success candidate; private
worker/widget owners require bounded access inspection before implementation.
H0-A commit `b90182c3` pushed successfully. No measurements claimed.

## Current state

H0–H4 complete. Initial H1-A compile issues resolved in H1-C. Final Clippy
failed on two proven pre-existing lints; no lint-clean claim.

## H1-A — partial (implementation complete; focused test compile pending)

Added nondefault feature, gated runner receipt stub and shared recorder with
strict CLI, bounded/unique output, per-run reservations for nested recorders,
terminal events, atomic summaries, null reasons and four focused tests.
`cargo check --features perf-harness --bin perf_probe_runner` passed exit 0
(17.68 s) before final small review fixes. `rustfmt` and `git diff --check`
passed. Focused Nextest compilation is still running; no tests claimed passed.
H1-C will finish verification against final source. No broad suite or Clippy.

Read-only review found three receipt defects, all corrected: metadata is now
inside the 32 MiB run cap (30 MiB stream allowance); metric sets/units validate
before mutation; returned and persisted failure summaries retain identical
reason/timestamps. Windows reparse paths are rejected. H1-A's 20-minute edit
budget ends here; pending compilation does not justify more recorder features.

Original focused command finished after 7m37s: 4 tests selected, 3 passed,
1 failed, 5768 skipped. Failure was the success test's comparison of canonical
output paths against a noncanonical Windows temporary directory path. Correct
the assertion to canonicalize the expected root; no receipt path escape was
observed. This build started before the final review changes, so final-source
verification remains H1-C. H1-A commit `32014fbd` pushed successfully.

## H1 observations (in progress)

H0-B commit `19bb2fa3` pushed. Source inspection confirmed Hot Folder can use
an internal feature-only driver bridge and isolated `AppDataRoot`; dashboard
needs an inert plugin factory, because ordinary construction installs shared
macro runtime. Radial public geometry/scene APIs need no production bridge.

Required legacy capability attempt (once):
`& ./dev/run_owned_process.ps1 -Executable (Get-Command cargo.exe).Source -Arguments @('--version') -TimeoutSeconds 3 -EvidenceDirectory target/perf-harness/legacy-capability`.
Runner exit 1; exact receipt reason `containment monitor failure: Access denied `;
child exit 0, RootAlive false, PostCleanup `Absent`. Raw evidence is ignored at
`target/perf-harness/legacy-capability/result.json`. No repeated CIM diagnosis.
This does not establish Job Object capability or desktop access.

## H1-B — complete (implementation/check; native capability pending H1-C)

Added checked Windows suspended creation, explicit inherited stdio-handle
list, kill-on-close Job Object assignment before resume, bounded pipe polling,
owned-root and job-empty verification, executable hashing, creation identity,
root CPU time and fixed fixture dispatch. Native API features are enabled only
by `perf-harness`. Non-Windows runner reports unsupported. No new dependency.

Repeated target-specific `cargo check --features perf-harness --bin perf_probe_runner`
ultimately passed exit 0. Initial compilation exposed API/type errors and one
unambiguous mutability correction; subsequent narrow checks covered review fixes.
Only existing vendor warnings remained. Touched Rust formatted; diff inspected.

Independent read-only review confirmed corrections for bounded draining after
cleanup failures, inherited-handle storage lifetime, fixture dispatch, real
descendant accounting and explicit hard-error preservation after deadlines.
Parent additionally corrected default Windows feature leakage and ensured
the batch stops on first denial/unverified ownership. No native fixture has
launched; no containment, desktop or performance metric is claimed yet.

Next: `cargo build --offline --features perf-harness --bin perf_probe_runner`,
four recorder tests plus runner parser tests, then one `--self-test batch`.

## H1-C — complete

H1-B `ab243ce4` pushed. Final-source verification:

- `cargo nextest run --offline --features perf-harness --lib --bin perf_probe_runner -E 'test(perf_harness::tests::) | binary(perf_probe_runner)'`:
  first run 3/4 passed; exposed actual serde `radial8` vs required `radial_8`.
  Fixed explicit serialization names for all three radial scenarios; retained
  assertion. Added two pure allowlist/bounds runner parser tests. Final run
  **6 passed**, 5768 skipped, build 1m27s. H1-A verification is now complete.
- `cargo build --offline --features perf-harness --bin perf_probe_runner`:
  exit 0, 2m51s.
- `target/debug/perf_probe_runner.exe --self-test batch`: exit 0, 3.86s;
  one native batch, no repair/retry. Exit 0, exact exit 23, two-second timeout,
  inherited descendant all had assigned/resumed owned jobs and verified cleanup.
  Descendant receipt records successful root exit, job total 3, active 2 at
  root exit, followed by verified empty job. No process inventory was used.
- Unregistered probe command: exit 2 before child creation; separate ignored
  `target/perf-harness/cli-rejection.json` receipt. `git check-ignore` confirms it.
- Checked all four event streams: started/terminal ordering, monotonic elapsed,
  valid UTC bounds. Raw root contains 29 files / 50,977 bytes at this checkpoint.

Raw fixture receipts: `target/perf-harness/runner_self_test/run-1792360843518494200-0`
(exit 0), `run-1792360843867080400-0` (exit 23),
`run-1792360844189831100-0` (timeout), `run-1792360846590963000-0`
(descendant). Successful ownership is confirmed; native desktop visibility and
screenshots remain untested. Root CPU is GetProcessTimes per-process time,
unnormalized and quantized (observed 0/15.625 ms), not worker CPU or latency.
All production algorithms, profiles and smoke gates remain unchanged.

## H2-A — complete

Added separately gated `perf_hotfolder_probe` and a feature-only private-owner
bridge. Fresh fixture creation rejects preexisting directories; all rules,
roots, candidates, leases, completed runs and external dispatches remain zero.
The production driver, watcher/executor owners and 80 ms cadence are unchanged.
One bounded sleep per observation window avoids adding a 100 Hz harness poller.
Added owned-process CPU helper and explicit optional toolchain/session nulls.

Verification passed:
`cargo check --offline --features perf-harness --bin perf_hotfolder_probe`;
`cargo nextest run --offline --features perf-harness --lib -E 'test(perf_harness_idle_fixtures_validate_and_reap_without_external_dispatch) | test(successful_run_writes_ordered_events_summary_provenance_and_fixture_hash)'`
(2 passed, 5771 skipped); `cargo build --offline --features perf-harness --bin perf_hotfolder_probe`
(1m30s); side-effect-free `--help`; touched-file rustfmt; `git diff --check`.
An initial positional Nextest filter selected zero tests, then the corrected
explicit expression above passed. No broader tests were run.

One batch per scenario, each command:
`target/debug/perf_probe_runner.exe --probe hotfolder --scenario <name> --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness`.
Both returned exit 0 with verified owned-job cleanup; 20 valid / 0 invalid CPU
samples each. Baseline/profile/dirty provenance supplied; Cargo version reused
from the existing owned capability receipt; Rust version/session remain null
with reasons. No further Hot Folder smoke runs are needed in this task.

| Scenario | CPU child run ID | Elapsed s | Window p50/p95 ms | CPU p50/p95 ms |
|---|---|---:|---:|---:|
| hotfolder_disabled | run-1792361736248888800-0 | 23.502 | 1000.713 / 1001.020 | 0 / 0 |
| hotfolder_enabled_zero_rules | run-1792361787594119500-0 | 23.061 | 1000.706 / 1000.887 | 0 / 0 |

Raw records live below `target/perf-harness/<scenario>/`; matching runner run
IDs are `run-1792361735060378200-0` and `run-1792361786472204400-0`.
CPU is all probe threads during the observation windows, quantized by Windows;
zeros do not establish zero cost. Wall values are observation intervals, not
latency. Driver wakeup/publication/work counters are null: production counters
are not instrumented. No visibility/GPU metrics or screenshots were collected.
No optimization conclusion follows from these infrastructure smoke samples.

## H2-B — complete

H2-A `1056f1dc` pushed. Added only the gated `perf_radial_probe` manifest target
and its bin source. Reuses public production `layout_menu` and `build_scene`
as in `benches/radial_runtime.rs`; no production source changed. The last layout
argument is minimum scale 0.25 (corrected the contract's earlier angle label).
Fixture hashes cover serialized actual menus and the exact layout constants.
Validation outside capture checks cell identity/order/count and finite geometry.

Passed `cargo check --offline --features perf-harness --bin perf_radial_probe`;
`cargo nextest run --offline --features perf-harness --bin perf_radial_probe radial_fixture_hash_and_scene_invariants_hold_for_supported_counts`
(1 passed); `cargo build --offline --features perf-harness --bin perf_radial_probe`;
`--help`, touched-file rustfmt and `git diff --check`. Existing vendor warnings only.

Direct command once per scenario:
`target/debug/perf_radial_probe.exe --scenario <name> --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness`.
All returned 0; each has 20 valid / 0 invalid samples and 22 ordered JSONL events.

| Scenario | Run ID | Run elapsed ms | Construction p50/p95 ms | Cells/primitives |
|---|---|---:|---:|---:|
| radial_8 | run-1792362314906006800-0 | 18.174 | 0.02810 / 0.05090 | 8 / 20 |
| radial_32 | run-1792362315197214300-0 | 16.182 | 0.04010 / 0.07095 | 32 / 68 |
| radial_128 | run-1792362315325060000-0 | 44.658 | 0.18535 / 0.28426 | 128 / 260 |

Raw evidence: `target/perf-harness/<scenario>/<run-id>/`. These `cpu_headless`
samples measure layout plus **unprepared** scene wall construction in dev,
excluding fixture creation, output validation, text shaping, asset preparation,
compositing, GPU and physical presentation. Process CPU and visibility are null
with reasons. This proves the H2 minimum real-algorithm CPU floor; it is not an
optimization comparison or live responsiveness measurement.

## H2-C — complete

H2-B `748230f2` pushed. Added gated dashboard bin and a feature-only adapter
at the existing Dashboard owner. Uses real `Context::run_ui` / `Dashboard::ui`,
fixed 1280x720 points / scale 1, and 1/4/9 synthetic safe widget slots.
The inert plugin factory loads no plugin files or global macro runtime;
pinned-command slots deliberately render the empty placeholder. No events,
network, system polling, clipboard reads, native viewport or presentation.

Passed `cargo check --offline --features perf-harness --bin perf_dashboard_probe`;
`cargo nextest run --offline --features perf-harness --bin perf_dashboard_probe dashboard_fixtures_render_expected_safe_topologies_and_reject_existing_paths`
(1 passed); `cargo build --offline --features perf-harness --bin perf_dashboard_probe`;
`--help`, touched-file rustfmt and `git diff --check`. A module visibility compile
error was corrected at the feature-only owner boundary; no default path changed.

One direct invocation per scenario:
`target/debug/perf_dashboard_probe.exe --scenario <name> --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness`.
All exit 0, 20 valid / 0 invalid samples, 22 ordered events, fixture hashes.

| Scenario | Run ID | Run elapsed ms | Frame p50/p95 ms | Widgets/shapes |
|---|---|---:|---:|---:|
| dashboard_minimal | run-1792363308041892800-0 | 69.686 | 0.35515 / 0.593335 | 1 / 18 |
| dashboard_typical | run-1792363308210060100-0 | 105.690 | 1.00300 / 1.207725 | 4 / 64 |
| dashboard_dense | run-1792363308399074100-0 | 119.750 | 2.00095 / 2.268355 | 9 / 138 |

Raw `target/perf-harness/<scenario>/<run-id>/`. CPU construction wall duration
only, after 3 warm-up frames. Own-process CPU, native present and session are
null with reasons. No screenshot. No optimization or comparison conclusion.

Read-only review identified a hidden runner fixture ownership defect: direct
internal descendant invocation could spawn without proving membership in our
owned Job. Actual recorded batches were contained. H4-A will add explicit job
identity proof before descendant spawning and use the remaining repair batch;
no unsafe direct descendant invocation was executed.

## H3-A — complete

H2-C `c33e6b81` pushed. Added `dev/perf_harness/README.md` recipes that reuse
Track B command telemetry for dev/fast-dev builds, Nextest compilation-only,
and external RA version measurement. No new script/target/dependency.
Track B has no deadline: optional RA analysis remains unavailable here rather
than launching an unbounded command or weakening the probe allowlist. Its
external evidence-directory rule is explicit; no new outside-tree write run.

Safe dry run `./dev/track_b/cargo.ps1 -Preset debug -PrintCommand` printed the
exact cargo build argv and `Started: false`; no compiler or benchmark campaign.
The combined inspection shell returned 1 because a later optional rg search
had no match, not because the dry run failed. No RA analysis/version launched.
Documented future gated target addition without adding a fifth bin.

## H3-B — complete

H3-A `b00087cf` pushed. Published build-once/direct-run examples, exact fixed
scenario names, provenance environment, unique ignored output allocation,
metric meanings, caps and fail-closed permission behavior. Added one dev/README
link. No new shell/process owner. All four executable `--help` commands returned
0 without workload execution; Track B `-Preset debug -PrintCommand` returned 0
and `Started: false`. `git diff --check` passed. Existing H2 smoke evidence is
reused; no scenario batch repeated. All current workload scenarios are headless;
native presentation and screenshots remain unverified.

## H4-A — complete

H3-B `3a3a0dbb` pushed. Reused H2's one-batch-per-scenario CPU/native evidence;
no workload reruns. Repaired the concrete hidden-descendant ownership gap with
a collision-checked named Job only for that fixture. Explicit membership and
KILL_ON_CLOSE/no-breakaway limits must verify, and the temporary query handle
closes before spawn. Parent Job handle remains noninheritable. Bare fixture
arguments reject before spawn; non-Windows unsupported helpers remain present.
Registered probe/guard reserved exit 3 preserves blocked_environment and stderr.
Read-only independent Windows review found no remaining containment blocker.

Passed targeted `cargo check --offline --features perf-harness --bin perf_probe_runner`;
`cargo nextest run --offline --features perf-harness --bin perf_probe_runner descendant_modes_without_owned_job_are_rejected_before_spawn` (1 passed);
`cargo build --offline --features perf-harness --bin perf_probe_runner`;
touched-file rustfmt and `git diff --check`. No unrelated validation or Clippy.

One remaining authorized repair batch:
`target/debug/perf_probe_runner.exe --self-test batch` returned 0, 4.12 s.
All four roots assigned while suspended, resumed afterward, verified cleanup:

| Case | Raw runner_self_test run ID | Status / root exit | Outcome |
|---|---|---|---|
| exit-zero | run-1792363814902126000-0 | success / 0 | verified empty |
| nonzero | run-1792363815338391700-0 | error / 23 | exact failure retained |
| deadline | run-1792363815707410200-0 | timeout / 1 | owned Job terminated and empty |
| descendant | run-1792363818154312500-0 | expected error / 0 | total 3, active at root exit 2; teardown verified empty |

Descendant's error intentionally reports a root that left descendants, even
though cleanup succeeded. Captured stdout confirms descendant started; no guard
failure or Access denied. This was native batch 2; no further attempts. Existing
CIM denial remains final. No native viewport, screenshots or desktop input.

## H4-B — complete

H4-A `ddd10f8f` pushed. Added sanitized results table/checkpoint map and updated
contract dispositions. `cargo metadata --offline --no-deps --format-version 1`
captured once after additions in `target/perf-harness/metadata-after.json`.
Compared to the single before capture: dependencies identical; default features
empty; only `multi_launcher` ungated; three old dev-smoke targets unchanged;
four new bins all require perf-harness. Profiles/Cargo.lock unchanged.

Reused passing scoped protocol, CLI, adapters and target-specific checks; the
only new behavior's rejection test and actual owned teardown passed in H4-A.
No full suite, default-build matrix, pixel campaign, or repeated workload batch.
Audited all 8 workload receipt sets: 22 ordered JSONL events each, consistent
monotonic/UTC order, success, 20 valid / 0 invalid, fixture hash. Saved ignored
`h4-receipt-audit.json`. Retained raw evidence below 0.5 MiB, max file below
40 KiB, none tracked; root remains ignored. Source review/diff confirms real
owners, feature-only seams and no default algorithm/profile changes. Touched
Rust rustfmt and cumulative diff checks passed; non-Windows stubs inspected,
no cross-platform compilation claimed. Independent dashboard review clear.
Final single maintained-target Clippy is the only remaining check.

## H4-C — complete (known baseline lint failures recorded)

H4-B `7b29bb19` pushed. After source/docs and all other checks, ran exactly once:
`cargo clippy --workspace --all-targets --offline -- -D warnings`.
Exit **101**: `clippy::let_and_return` at
`src/paste_sequence/playback.rs:597` and `src/gui/command_host.rs:420`.
Both paths are unchanged against baseline (empty path diff), and `git show
3a989d56:<path>` confirms the identical let/return code. Archived Track D also
records these two findings. Existing vendor/image warnings appeared. No unrelated
lint fix or repeat Clippy; default-feature maintained-target sweep is not a
claim that every gated harness target is lint-clean. Their scoped check/test
results remain separate. Raw log/exit under ignored target/perf-harness.

All required checkpoints completed within the total and stage budgets. No OS
privilege/security changes, broad process enumeration retry, personal data,
GUI interaction, screenshots, full Nextest or optimization campaign. Native
Job containment works; desktop/visibility remains unverified. Legacy CIM and
RA-analysis limitations are recorded, not worked around. No follow-up starts.
Final documentation-only checkpoint commit receives one normal push attempt;
its resulting HEAD and pushed state are reported in the final handoff.
