# Track D selected measurements

Reuse `../track_b/measure.ps1` unchanged. Pass executable arguments as arrays;
inspect `Succeeded` and `ExitCode`, never interpret an unsuccessful command as
a build timing. Use `$env:TEMP/Multi_Launcher/perf-track-d` for unique raw run
directories (the existing runner requires evidence outside the worktree). The
existing runner fingerprints Git, fixtures, flags and actual target directory,
restores environment/location in finally, and uses ProcessStartInfo.ArgumentList.

```powershell
$r = & ./dev/track_b/measure.ps1 -Executable cargo.exe `
  -ArgumentList @('check', '--lib', '--offline', '--timings') `
  -ScenarioId 'd1-check-initial' -Profile dev -CacheState unknown `
  -EvidenceDirectory (Join-Path $env:TEMP 'Multi_Launcher/perf-track-d') `
  -BaselineSha cbe104abc10ff03af2fd9897e88a4a331a7f4197
$r | Select-Object ScenarioId, WallSeconds, ExitCode, Succeeded
```

Run Cargo sequentially. Offline mode establishes cache availability without
network retries. Distinguish preparation/failure from warmed and edited samples.
For an edit replay, retain exact original file bytes, append a harmless comment,
run the selected command, and restore original bytes in finally; verify SHA256.
Never remove existing target directories. Use `../run_owned_process.ps1` only
after isolated settings and process identity/cleanup are established. Native
input requires a verified owned HWND/PID; the script alone does not establish
safe GUI input ownership.

Source-controlled summary schema: version, track, scenario, baseline_sha,
candidate_sha_or_diff, profile, settings_hash, host, samples, metrics,
limitations, decision. Unavailable metrics are null with an explanation.
No raw command logs, personal paths, query text or screenshots in summaries.

Track D's current-checkout results are in
`docs/performance/track_d/d1_results.md`. Existing fast-dev edited launcher
builds saved 15–21 seconds in two controlled replays; no-op median grew by
0.229 seconds. This remains an opt-in debug-information tradeoff. Profile
settings and release semantics are unchanged. Use `developer_loop.ps1 -Family
app-build -Profile fast-dev` for the selected comparison; it performs one
preparation, three no-ops, and two byte-restored comment edits. Do not treat
preparation as an edit speedup. `summarize.ps1` requires explicit UTC start/end
bounds so unrelated sessions cannot be pooled.

For the isolated Track C root-query CPU-frame test, set
`ML_TRACK_C_MEASURED_SAMPLES=15`, `ML_TRACK_C_BENCH_MODE=full`,
`MULTI_LAUNCHER_PERF=1`, `ML_SKIP_CLIPBOARD_SYNC=1`, and provenance switches
`ML_TRACK_C_SOURCE_SHA` / `ML_TRACK_C_PROFILE=fast-dev` in the measurement
runner's EnvironmentOverrides. The sample override affects only the shared
`measure_owner` / `emit_samples` test helper; dedicated historical protocols
with their own loops are unchanged. Default shared-helper sampling remains 20.
Use the exact ignored test
`gui::render::tests::track_c_benchmark_root_query_to_usable_cpu_frame` with
Nextest `--run-ignored only --nocapture --cargo-profile fast-dev --lib -E`.
This is synthetic CPU usability evidence, not native visibility or release
performance. No clipboard or native input is needed.
