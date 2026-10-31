$ErrorActionPreference='Stop'
$repoRoot=(Get-Location).Path
$evidenceRoot=Join-Path $repoRoot 'docs/plans/execution/multi-launcher-linear-followons-20261030-r01/stages/G06/roles/orchestrator'
$profileRoot=Join-Path $repoRoot 'target/linear-followons-20261030-r01/G06/profile'
$stopPath=Join-Path $profileRoot 'NATIVE_STOP_REQUEST'
$utf8=[Text.UTF8Encoding]::new($false)
function Save-Record($record,$name) { [IO.File]::WriteAllText((Join-Path $evidenceRoot $name),($record|ConvertTo-Json -Depth 8),$utf8) }
$build=Get-Content (Join-Path $evidenceRoot 'BUILD_RECEIPT.json') -Raw | ConvertFrom-Json
$exePath=Join-Path $repoRoot 'target/fast-dev/multi_launcher.exe'
if($build.status -ne 'BUILD_PASSED' -or $build.timed_out -or -not $build.output_drained -or (Get-FileHash $exePath).Hash.ToLowerInvariant() -ne $build.exe_sha256) { throw 'exact-source build unavailable' }
if((git rev-parse HEAD).Trim() -ne $build.source_head -or @(git diff --name-only -- src crates Cargo.toml Cargo.lock .codex).Count) { throw 'source changed after build' }
$settings=Get-Content (Join-Path $profileRoot 'settings.json') -Raw | ConvertFrom-Json
if($null -ne $settings.paste_sequence.next_hotkey -or $null -ne $settings.paste_sequence.cancel_hotkey -or $settings.index_paths.Count -ne 0 -or $settings.enabled_plugins.Count -ne 0) { throw 'unsafe fixture settings' }
if(Test-Path -LiteralPath $stopPath) { throw 'unexpected prior native stop request' }
$info=[Diagnostics.ProcessStartInfo]::new()
$info.FileName=$exePath
$info.WorkingDirectory=$profileRoot
$info.UseShellExecute=$false
$info.RedirectStandardOutput=$true
$info.RedirectStandardError=$true
[void]$info.Environment.Remove('ML_QA_FRAMEBUFFER_REQUEST_MANIFEST')
$process=[Diagnostics.Process]::new()
$process.StartInfo=$info
$started=[DateTimeOffset]::UtcNow
if(-not $process.Start()) { throw 'owned app launch failed' }
$appPid=$process.Id
$outTask=$process.StandardOutput.ReadToEndAsync()
$errTask=$process.StandardError.ReadToEndAsync()
$record=[ordered]@{ phase='native_jump'; source_head=$build.source_head; executable=$exePath; exe_sha256=$build.exe_sha256; profile=$profileRoot; started_utc=$started.ToString('o'); native_cap_seconds=180; cleanup_cap_seconds=60; app_pid=$appPid; supervisor_pid=$PID; status='RUNNING'; paste_next_cancel_disabled_before_construction=$true; qa_capture_env_removed_for_child=$true; input_authority='Only supported sky-returned unique owned Window may receive input; process HWND metadata is not a target'; child_tree='UNVERIFIED'; termination_error=$null; forced_termination=$false; output_drained=$false }
Save-Record $record 'NATIVE_STATE.json'
try { $record['input_idle']=$process.WaitForInputIdle(5000) } catch { $record['input_idle_error']=$_.Exception.Message }
if(-not $process.HasExited) {
    $process.Refresh()
    $record['reported_main_window_handle']=$process.MainWindowHandle.ToInt64()
    $record['reported_main_window_title']=$process.MainWindowTitle
}
Save-Record $record 'NATIVE_STATE.json'
$lastProgress=-15
while(-not $process.WaitForExit(250)) {
    $elapsed=([DateTimeOffset]::UtcNow-$started).TotalSeconds
    if($elapsed-$lastProgress -ge 15) { Write-Output "G06 phase=native_jump elapsed=$([Math]::Round($elapsed,1))s cap=180s owned_pid=$appPid"; $lastProgress=$elapsed }
    if((Test-Path -LiteralPath $stopPath) -or $elapsed -ge 180) { break }
}
$cleanupStarted=[DateTimeOffset]::UtcNow
$record['native_phase_ended_utc']=$cleanupStarted.ToString('o')
$record['native_elapsed_seconds']=[Math]::Round(($cleanupStarted-$started).TotalSeconds,3)
$record['native_budget_reached']=($record.native_elapsed_seconds -ge 180)
$cleanupDeadline=$cleanupStarted.AddSeconds(60)
if(-not $process.HasExited) {
    try { $record['close_main_window_requested']=$process.CloseMainWindow() } catch { $record['close_main_window_error']=$_.Exception.Message }
    if(-not $process.WaitForExit(10000)) {
        try { $process.Kill(); $record.forced_termination=$true } catch { $record.termination_error=$_.Exception.Message }
    }
    while(-not $process.WaitForExit(250) -and [DateTimeOffset]::UtcNow -lt $cleanupDeadline) { }
}
if(-not $process.HasExited) {
    $record.status='UNRETIRED_STOP_SOURCE_EDITS'
    Save-Record $record 'NATIVE_STATE.json'
    Save-Record $record 'NATIVE_RETIREMENT.json'
    Write-Output ($record|ConvertTo-Json -Depth 8)
    exit 1
}
$record['app_exit_code']=$process.ExitCode
while((-not $outTask.IsCompleted -or -not $errTask.IsCompleted) -and [DateTimeOffset]::UtcNow -lt $cleanupDeadline) { Start-Sleep -Milliseconds 250 }
if($outTask.IsCompleted -and $errTask.IsCompleted) {
    [IO.File]::WriteAllText((Join-Path $evidenceRoot 'native.stdout.log'),$outTask.GetAwaiter().GetResult(),$utf8)
    [IO.File]::WriteAllText((Join-Path $evidenceRoot 'native.stderr.log'),$errTask.GetAwaiter().GetResult(),$utf8)
    $record.output_drained=$true
    $record['stdout_sha256']=(Get-FileHash (Join-Path $evidenceRoot 'native.stdout.log')).Hash.ToLowerInvariant()
    $record['stderr_sha256']=(Get-FileHash (Join-Path $evidenceRoot 'native.stderr.log')).Hash.ToLowerInvariant()
}
$ended=[DateTimeOffset]::UtcNow
$record['retired_utc']=$ended.ToString('o')
$record['cleanup_elapsed_seconds']=[Math]::Round(($ended-$cleanupStarted).TotalSeconds,3)
$record.status=if($record.output_drained) { 'RETIRED' } else { 'OUTPUT_UNDRAINED' }
Save-Record $record 'NATIVE_STATE.json'
Save-Record $record 'NATIVE_RETIREMENT.json'
Write-Output ($record|ConvertTo-Json -Depth 8)
