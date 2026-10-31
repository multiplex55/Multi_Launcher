$ErrorActionPreference = 'Stop'
$repoRoot = (Get-Location).Path
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'Cargo.toml'))) { throw 'not repository root' }
$evidenceRoot = Join-Path $repoRoot 'docs/plans/execution/multi-launcher-linear-followons-20261030-r01/stages/G06/roles/orchestrator'
$utf8 = [Text.UTF8Encoding]::new($false)
function Save-Record($record, $name) {
    [IO.File]::WriteAllText((Join-Path $evidenceRoot $name), ($record | ConvertTo-Json -Depth 8), $utf8)
}
$head = (git rev-parse HEAD).Trim()
if (@(git diff --name-only -- src crates Cargo.toml Cargo.lock .codex).Count) { throw 'source not frozen' }
$arguments = @('build','-p','multi_launcher','--bin','multi_launcher','--profile','fast-dev','--offline','--locked')
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = (Get-Command cargo.exe).Source
$startInfo.WorkingDirectory = $repoRoot
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
foreach ($argument in $arguments) { $startInfo.ArgumentList.Add($argument) }
$started = [DateTimeOffset]::UtcNow
$process = [Diagnostics.Process]::new()
$process.StartInfo = $startInfo
if (-not $process.Start()) { throw 'owned build start failed' }
$buildPid = $process.Id
$outTask = $process.StandardOutput.ReadToEndAsync()
$errTask = $process.StandardError.ReadToEndAsync()
$record = [ordered]@{ phase='normal_app_build'; source_head=$head; command_arguments=$arguments; started_utc=$started.ToString('o'); cap_seconds=600; direct_pid=$buildPid; child_tree='UNVERIFIED: prior CIM denial; only owned root handle used'; status='RUNNING'; timed_out=$false; termination_error=$null; output_drained=$false }
Save-Record $record 'BUILD_STATE.json'
$lastProgress = -20
while (-not $process.WaitForExit(500)) {
    $elapsed = ([DateTimeOffset]::UtcNow - $started).TotalSeconds
    if ($elapsed - $lastProgress -ge 15) { Write-Output "G06 phase=normal_app_build elapsed=$([Math]::Round($elapsed,1))s cap=600s pid=$buildPid"; $lastProgress=$elapsed }
    if ($elapsed -ge 600) {
        $record.timed_out = $true
        try { $process.Kill($true) } catch { $record.termination_error=$_.Exception.Message }
        $retireDeadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
        while (-not $process.WaitForExit(250) -and [DateTimeOffset]::UtcNow -lt $retireDeadline) { }
        if (-not $process.HasExited) {
            $record.status='UNRETIRED_STOP_SOURCE_EDITS'
            $record['observed_utc']=[DateTimeOffset]::UtcNow.ToString('o')
            Save-Record $record 'BUILD_STATE.json'
            Save-Record $record 'BUILD_RECEIPT.json'
            Write-Output ($record | ConvertTo-Json -Depth 8)
            exit 124
        }
        break
    }
}
$drainDeadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
while ((-not $outTask.IsCompleted -or -not $errTask.IsCompleted) -and [DateTimeOffset]::UtcNow -lt $drainDeadline) { Start-Sleep -Milliseconds 250 }
$ended = [DateTimeOffset]::UtcNow
$record['ended_utc']=$ended.ToString('o')
$record['elapsed_seconds']=[Math]::Round(($ended-$started).TotalSeconds,3)
$record['direct_exit']=$process.ExitCode
if ($record.elapsed_seconds -gt 600) { $record.timed_out=$true }
if ($outTask.IsCompleted -and $errTask.IsCompleted) {
    [IO.File]::WriteAllText((Join-Path $evidenceRoot 'build.stdout.log'),$outTask.GetAwaiter().GetResult(),$utf8)
    [IO.File]::WriteAllText((Join-Path $evidenceRoot 'build.stderr.log'),$errTask.GetAwaiter().GetResult(),$utf8)
    $record.output_drained=$true
    $record['stdout_sha256']=(Get-FileHash (Join-Path $evidenceRoot 'build.stdout.log')).Hash.ToLowerInvariant()
    $record['stderr_sha256']=(Get-FileHash (Join-Path $evidenceRoot 'build.stderr.log')).Hash.ToLowerInvariant()
}
$record.status = if ($record.timed_out) { 'TIMEOUT' } elseif (-not $record.output_drained) { 'OUTPUT_UNDRAINED' } elseif ($process.ExitCode -eq 0) { 'BUILD_PASSED' } else { 'BUILD_FAILED' }
$exePath=Join-Path $repoRoot 'target/fast-dev/multi_launcher.exe'
if ($record.status -eq 'BUILD_PASSED' -and (Test-Path -LiteralPath $exePath)) {
    $record['executable']=$exePath
    $record['exe_sha256']=(Get-FileHash $exePath).Hash.ToLowerInvariant()
    $record['exe_length']=(Get-Item $exePath).Length
}
$record['source_paths_changed_after']=@(git diff --name-only -- src crates Cargo.toml Cargo.lock .codex)
Save-Record $record 'BUILD_RECEIPT.json'
Save-Record $record 'BUILD_STATE.json'
Write-Output ($record | ConvertTo-Json -Depth 8)
if ($record.status -ne 'BUILD_PASSED') { exit 1 }
