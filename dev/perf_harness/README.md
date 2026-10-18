# Opt-in performance harness

Run from the repository root in PowerShell 7. These probes use disposable,
synthetic fixtures under ignored `target/perf-harness`; they do not load personal
settings. See [contract](../../docs/performance/harness/contract.md) and
[results](../../docs/performance/harness/results.md) for measurement limits.

## Compiler and external-command timing

Reuse Track B's direct argument-array command measurement. Its evidence directory
must be outside the source tree. The examples below are recipes, not a new build
comparison campaign. `measure.ps1` has no watchdog; do not use it as native
process containment or claim it enforces a timeout. Use only finite compiler or
version commands, or a separately proven bounded command owner.

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
$run = & ./dev/track_b/measure.ps1 -Executable $raExe -ArgumentList @('--version') -ScenarioId 'ra-version-only' -Profile external-command -CacheState unknown -EvidenceDirectory $evidence -FixtureMetadata @{ interactive_editor_observed = $false; session = $null; timeout_seconds = $null; timeout_reason = 'version-only command; Track B has no watchdog' }
if (!$run.Succeeded) { throw "RA version command failed: $($run.ExitCode)" }
```

Optional `analysis-stats .` requires a separately proven finite process owner,
an explicit deadline, captured actual executable/version/session and timeout
reason. This session did not run it: the legacy CIM-based owner returned Access
denied, and the new runner intentionally permits only registered probes. Do not
pass RA through that allowlist or launch an unbounded analysis to fill a table.
RA analysis timing and interactive response remain unavailable, with reasons.

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
