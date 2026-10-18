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
