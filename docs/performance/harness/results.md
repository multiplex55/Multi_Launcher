# Benchmark infrastructure results

Baseline `3a989d56abe1b203902e0c6eb6075ae58f6235ef`, clean `perf-track-d` checkout;
created `perf-harness` directly from it, without pull/rebase/merge. All checkpoint
commits were pushed normally. See [execution ledger](progress.md) for corrections,
exact command history and final verification; [usage](../../../dev/perf_harness/README.md)
for copyable commands. Measurements are infrastructure smoke evidence, not an
optimization comparison. Source/profile provenance is dev, dirty=true at capture;
Rust version and desktop/session inventory are null with explicit reasons.

| Checkpoint | Commit |
|---|---|
| H0-A baseline | b90182c3 |
| H0-B contract | 19bb2fa3 |
| H1-A recorder | 32014fbd |
| H1-B owned runner | ab243ce4 |
| H1-C proof | fa5a9e15 |
| H2-A Hot Folder | 1056f1dc |
| H2-B radial | 748230f2 |
| H2-C dashboard | c33e6b81 |
| H3-A external recipes | b00087cf |
| H3-B usage | 3a3a0dbb |
| H4-A execution and ownership repair | ddd10f8f |
| H4-B capability matrix | 7b29bb19 |

Later H4 commits and final HEAD are recorded in the ledger and Git history.

| Capability | Outcome / evidence |
|---|---|
| Windows owned Job containment | Verified suspended creation → assignment → resume, deadline teardown, ActiveProcesses=0. `target/perf-harness/runner_self_test/` |
| Owned root CPU | GetProcessTimes supported; quantized values, no logical processor normalization. Hot Folder receipts below |
| CPU headless | All 8 real-owner scenarios, 20 valid / 0 invalid each |
| Legacy CIM runner | blocked_environment: `containment monitor failure: Access denied `, exit 1; attempted once, `target/perf-harness/legacy-capability/` |
| Native desktop / GPU present | Unverified; no owned GUI viewport was created. Containment does not establish desktop capability |
| Owned screenshots | 0 collected; no viewport, physical visibility unmeasured |
| RA interactive / analysis | Unmeasured; no new headless analysis trial. Recipe records external-command scope and missing deadline/session reasons |
| Default build gates | Before/after metadata confirms one ungated app, 3 old dev-smoke bins, exactly 4 new perf-harness bins; no dependency/profile changes |

Raw JSONL/JSON/logs stay ignored under `target/perf-harness/<scenario>/<run-id>/`.
No process IDs, private paths, screenshots or raw streams are committed. All runs
used 3 warmups, 20 recorded samples, timeout 90 seconds. Every workload receipt
has start + 20 samples + terminal, a deterministic fixture hash, units and null
reasons. Each row below names the child/direct workload run, excluding the runner
wrapper's separate whole-process sample. Reported elapsed includes setup/warmup.

| Binary / scenario | Run ID | Elapsed | Wall p50 / p95 ms | Actual output |
|---|---|---:|---:|---|
| hotfolder / disabled | run-1792361736248888800-0 | 23.502 s | 1000.713 / 1001.020 | 20 observation windows |
| hotfolder / enabled_zero_rules | run-1792361787594119500-0 | 23.061 s | 1000.706 / 1000.887 | 20 observation windows |
| radial / 8 | run-1792362314906006800-0 | 18.174 ms | 0.02810 / 0.05090 | 8 cells / 20 primitives |
| radial / 32 | run-1792362315197214300-0 | 16.182 ms | 0.04010 / 0.07095 | 32 / 68 |
| radial / 128 | run-1792362315325060000-0 | 44.658 ms | 0.18535 / 0.28426 | 128 / 260 |
| dashboard / minimal | run-1792363308041892800-0 | 69.686 ms | 0.35515 / 0.593335 | 1 widget / 18 shapes |
| dashboard / typical | run-1792363308210060100-0 | 105.690 ms | 1.00300 / 1.207725 | 4 / 64 |
| dashboard / dense | run-1792363308399074100-0 | 119.750 ms | 2.00095 / 2.268355 | 9 / 138 |

Hot Folder uses the production driver with fresh app-data and no roots/rules,
watch activity or external dispatch. Per-window all-thread process CPU p50/p95
is 0/0 ms at Windows counter resolution; this does **not** show zero cost. The
wall values are observation intervals, not response latency. Driver wakeups,
publications and work-iteration metrics are null (production counters are not
instrumented). Both wrapper runs verified owned Job cleanup:
`run-1792361735060378200-0`, `run-1792361786472204400-0`.

Radial measures production layout plus **unprepared** scene construction;
fixture allocation and result validation are outside capture. Text shaping,
prepared assets, full compositing and GPU presentation are excluded. Dashboard
measures real warm egui `run_ui` / `Dashboard::ui` construction at 1280x720 / scale
1; pinned slots deliberately show their empty placeholder. It loads no user
favorites or plugin files and performs no input. Direct radial/dashboard process
CPU is null (no owned process CPU handle supplied); wall algorithm/frame time is
measured. Native present/visibility is null for every headless row.

## Focused verification

All four target-specific `cargo check --offline --features perf-harness --bin
<name>` and executable builds passed. No full Nextest run. Scoped passing tests:

```text
cargo nextest run --offline --features perf-harness --lib --bin perf_probe_runner -E 'test(perf_harness::tests::) | binary(perf_probe_runner)'
# H1-C: 6 passed, 5768 skipped
cargo nextest run --offline --features perf-harness --lib -E 'test(perf_harness_idle_fixtures_validate_and_reap_without_external_dispatch) | test(successful_run_writes_ordered_events_summary_provenance_and_fixture_hash)'
# H2-A: 2 passed, 5771 skipped
cargo nextest run --offline --features perf-harness --bin perf_radial_probe radial_fixture_hash_and_scene_invariants_hold_for_supported_counts
# H2-B: 1 passed
cargo nextest run --offline --features perf-harness --bin perf_dashboard_probe dashboard_fixtures_render_expected_safe_topologies_and_reject_existing_paths
# H2-C: 1 passed
```

The first native self-test batch proved exit 0, exit 23, 2-second timeout and
contained descendant teardown. Read-only review found that the hidden direct
descendant fixture entry point needed explicit Job identity proof before spawn;
the actual batch was contained. H4-A repaired this with a collision-checked
named Job, explicit membership/limit checks and a query handle closed before
spawn. A focused rejection test passed (1/1). Targeted runner check/build passed.
The second and final native batch returned 0 in 4.12 s; all four cleanup checks
passed. Raw runner_self_test IDs: `run-1792363814902126000-0` (success, exit 0),
`run-1792363815338391700-0` (error, exit 23), `run-1792363815707410200-0`
(timeout, verified empty), `run-1792363818154312500-0` (root exit 0, total 3,
active at root exit 2, expected error for remaining descendants, then empty).
Reserved probe/guard exit 3 now preserves blocked_environment and captured
stderr. There was no denied Job API in these batches; the legacy denial was not
retried. Unknown executable/scenario/oversized arguments reject before mutation;
all four `--help` calls return 0; Track B dry-run prints `Started: false`.

Metadata captured once before/after; `autobins=false`, default feature selection,
existing gates and dependencies remain intact. Touched-file rustfmt and diff
checks passed. H4 audited all eight receipts for monotonic/UTC order, 22 events,
20 valid / 0 invalid samples and hashes; raw audit is
`target/perf-harness/h4-receipt-audit.json`. Retained evidence is below 0.5 MiB,
far below the 100 MiB aggregate cap; no raw file is tracked. Final Clippy outcome
is appended before handoff. Non-Windows fallback helpers were source-reviewed;
no cross-platform build was available or claimed.

Final single maintained-target invocation:
`cargo clippy --workspace --all-targets --offline -- -D warnings` returned
**101**, with `clippy::let_and_return` at `src/paste_sequence/playback.rs:597`
and `src/gui/command_host.rs:420`. Both files have an empty diff against baseline
`3a989d56`; `git show` confirms the same bindings/returns there. These match the
previous Track D report. Existing vendored image warnings also appeared. Raw log
and exit: `target/perf-harness/final-clippy.log`, `final-clippy-exit.txt`.
No unrelated source fix or repeat Clippy. The prescribed command uses default
features, so gated harness targets were checked/tested separately above; this
failed sweep does not establish lint cleanliness of all targets/features.

No production optimization was attempted: Hot Folder's 80 ms scheduling,
radial/dashboard production algorithms, launcher behavior, Cargo profiles and
Rust Analyzer settings are unchanged. Feature-only adapters expose existing
owners. A next investigation can select one existing scenario with an explicit
comparison hypothesis and source/profile; no follow-up campaign starts here.
