# Opt-in performance harness

Run from the repository root in PowerShell 7. These probes use disposable,
synthetic fixtures under ignored `target/perf-harness`; they do not load personal
settings. See [contract](../../docs/performance/harness/contract.md) and
[results](../../docs/performance/harness/results.md) for measurement limits.

## Compiler and external-command timing

Reuse Track B's direct argument-array command measurement. Its evidence directory
must be outside the source tree. The examples below are recipes, not a new build
comparison campaign. `measure.ps1` uses the checked Job Object owner and defaults
to a 30-minute command deadline; set `-TimeoutSeconds` for the selected command
and inspect its deadline, output-drain, and owned-process cleanup receipt.

```powershell
$cargoExe = (Get-Command cargo.exe).Source
$evidence = Join-Path ([IO.Path]::GetTempPath()) ('MultiLauncher-perf-' + [guid]::NewGuid())
$buildArgs = @('build','--offline','--bin','multi_launcher','--profile','dev','--timings')
$run = & ./dev/track_b/measure.ps1 -Executable $cargoExe -ArgumentList $buildArgs -ScenarioId 'selected-dev-build' -Profile dev -CacheState unknown -EvidenceDirectory $evidence
if (!$run.Succeeded) { throw "Build failed: $($run.ExitCode)" }
```

For fast-dev use `--profile fast-dev` and `-Profile fast-dev`; do not modify Cargo
profiles. Label preparation, warmed no-op, or deliberately edited source runs
accurately with `ScenarioId`, `PreviousCommand`, `SourceFilesModified` and
`CacheState`. An unchanged invocation alone does not prove cache warmth. Preserve
Track B's exit, source fingerprint, command, stdout/stderr and Cargo timings.

Targeted Nextest **compilation only**, through the same measurement call:

```powershell
$testArgs = @('nextest','run','--offline','--lib','--cargo-profile','dev','-E','test(history_prepare_)','--no-run')
$run = & ./dev/track_b/measure.ps1 -Executable $cargoExe -ArgumentList $testArgs -ScenarioId 'selected-test-compile' -Profile dev -CacheState unknown -EvidenceDirectory $evidence
if (!$run.Succeeded) { throw "Test compilation failed: $($run.ExitCode)" }
```

Replace the filter with the actual touched test. `--no-run` prevents execution;
this is not a test-pass receipt. Use `--cargo-profile fast-dev` for that profile.
The older Track D developer loop also edits workloads and executes tests; it is
not required for this focused recipe.

Rust Analyzer is measured as an **external command**, never by a self-measuring
probe or as a proxy for interactive editor latency. A finite version receipt:

```powershell
$raExe = (Get-Command rust-analyzer.exe).Source
$run = & ./dev/track_b/measure.ps1 -Executable $raExe -ArgumentList @('--version') -ScenarioId 'ra-version-only' -Profile external-command -CacheState unknown -TimeoutSeconds 60 -EvidenceDirectory $evidence -FixtureMetadata @{ interactive_editor_observed = $false; session = $null; timeout_seconds = 60; timeout_reason = 'bounded version-only command; interactive editor behavior was not measured' }
if (!$run.Succeeded) { throw "RA version command failed: $($run.ExitCode)" }
```

Optional `analysis-stats .` should use an explicit deadline and record the
actual executable/version/session and timeout reason. The bounded owner does not
apply a command allowlist. Do not claim that command wall time measures editor
responsiveness; interactive response remains a separate observation.

Safe command discovery without compilation:
`./dev/track_b/cargo.ps1 -Preset debug -PrintCommand`.

## Adding a future probe

Keep `autobins = false` and the nondefault feature. A future task may add one
explicit target, subject to its own scope; this task contains exactly four new
targets total:

```toml
[[bin]]
name = "perf_future_probe"
path = "src/bin/perf_future_probe.rs"
required-features = ["perf-harness"]
```

Reuse `perf_harness::{parse_cli, run_recorded}` and an existing algorithm owner.
Keep fixtures synthetic, validate results outside capture, declare missing
metrics as null with reasons, and add any runner registration deliberately.
The runner is an exact allowlist, not an arbitrary command launcher. Normal
debug/release builds retain only `multi_launcher`; old smoke bins retain their
separate `dev-smoke-binaries` gate.

## Build once, invoke bounded scenarios

```powershell
cargo build --offline --features perf-harness --bin perf_probe_runner --bin perf_hotfolder_probe --bin perf_radial_probe --bin perf_dashboard_probe
$env:PERF_HARNESS_BASELINE_SHA = (git rev-parse HEAD).Trim()
$env:PERF_HARNESS_SOURCE_DIRTY = if (git status --porcelain) { 'true' } else { 'false' }
$env:PERF_HARNESS_PROFILE = 'dev'
```

Build first; there is no Cargo execution inside a measured interval. For the
recorded task runs the baseline was the starting checkout SHA, with dirty=true;
for a future run set it to the source revision actually being evaluated. Optional
`PERF_HARNESS_RUSTC_VERSION` / `PERF_HARNESS_CARGO_VERSION` can use already known
toolchain strings. Unprovided tool/session provenance stays null with a reason.

```powershell
& ./target/debug/perf_probe_runner.exe --self-test batch
& ./target/debug/perf_probe_runner.exe --probe hotfolder --scenario hotfolder_enabled_zero_rules --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness
& ./target/debug/perf_radial_probe.exe --scenario radial_32 --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness
& ./target/debug/perf_dashboard_probe.exe --scenario dashboard_typical --warmups 3 --samples 20 --timeout-seconds 90 --out target/perf-harness
```

Each invocation allocates a unique scenario/run directory; do not precreate a
fixture directory. Hot Folder also supports `hotfolder_disabled`; radial supports
`radial_8` / `radial_128`; dashboard supports `dashboard_minimal` / `dashboard_dense`.
Use `--help` on any of the four executables for side-effect-free discovery.
Runner equivalents use `--probe radial|dashboard` with the same scenario args.

The runner uses checked suspended creation, explicit owned Job assignment,
then resume; timeout/error tears down only that job. It accepts registered sibling
executables, fixed scenarios and bounded arguments. No shell command forwarding.
Do not use the hidden fixture modes as benchmark entry points. On Win32 Access
denied preserve `blocked_environment` and stop that attempt. Do not retry via
uncontained process launch, process-name kills, CIM enumeration or elevation.

No current scenario requires desktop interaction. Radial measures layout plus
unprepared scene construction; dashboard measures warm headless egui frames.
Neither measures GPU/native present or first-visible latency. Hot Folder uses
one-second observation windows and all-thread process CPU; counters unavailable
without production instrumentation are null. No owned GUI viewport exists, so
native desktop and screenshot evidence remain unverified (zero screenshots).

Inspect `events.jsonl`, `summary.json` and runner `run.json` plus capped streams.
Exit success requires successful work and verified runner cleanup where used;
read failure reasons, not just a missing/zero metric. The recorder limits each
run to 32 MiB (30 MiB streams) and aggregate evidence/reservations to 100 MiB.
An allocation can fail conservatively when stale reservations remain; inspect
only this owned evidence root before manually archiving old runs. Raw evidence
must remain ignored; commit sanitized summary tables only.
