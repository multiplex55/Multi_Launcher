param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('list', 'run', 'correction-list', 'corrected-run')]
    [string] $Phase
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path 'G:\Repos\rust\Multi_Launcher').Path
$evidenceRoot = Join-Path $repoRoot 'docs\plans\execution\multi-launcher-linear-followons-20261030-r01\stages\G02\roles\implementer\verification\m1a-current'
New-Item -ItemType Directory -Path $evidenceRoot -Force | Out-Null

$filter = 'test(table_publication_) | test(resource_pool_leases_share_usage_and_release_on_last_owner) | test(partial_temp_append_failure_keeps_charge_until_owned_file_is_removed) | test(failed_temp_deletion_keeps_a_retryable_charge_until_removed) | test(cloned_snapshot_handles_keep_each_allocation_charged_after_session_drop) | test(pending_job_releases_its_resource_leases_before_confirmed_activity_retirement) | test(file_page_failure_and_cancellation_preserve_the_last_good_page) | test(file_page_completion_from_closed_lifetime_is_ignored_after_reopen) | test(table_session_draft_edits_revoke_queued_export_receipts) | test(table_file_save_refuses_to_replace_the_opened_source) | test(table_file_save_requires_confirmation_and_publishes_the_captured_output)'
$cargoVerb = if ($Phase -in @('list', 'correction-list')) { 'list' } else { 'run' }
$capSeconds = if ($cargoVerb -eq 'list') { 600 } else { 180 }
$arguments = [System.Collections.Generic.List[string]]::new()
$arguments.Add('nextest')
$arguments.Add($cargoVerb)
$arguments.Add('-p')
$arguments.Add('multi_launcher')
$arguments.Add('-p')
$arguments.Add('multi_launcher_table_core')
$arguments.Add('--cargo-profile')
$arguments.Add('fast-dev')
$arguments.Add('--lib')
$arguments.Add('--offline')
$arguments.Add('--locked')
$arguments.Add('--color')
$arguments.Add('never')
$arguments.Add('-E')
$arguments.Add($filter)
if ($cargoVerb -eq 'list') {
    $arguments.Add('--message-format')
    $arguments.Add('json')
} else {
    $arguments.Add('--retries')
    $arguments.Add('0')
    $arguments.Add('--no-fail-fast')
    $arguments.Add('--status-level')
    $arguments.Add('pass')
    $arguments.Add('--final-status-level')
    $arguments.Add('fail')
}

$sourcePaths = @(
    'src/file_operations.rs',
    'src/table_utility_session.rs',
    'src/table_utility_session/publication.rs',
    'crates/multi_launcher_table_core/src/lib.rs',
    'crates/multi_launcher_table_core/src/resource.rs'
)
$sourceIdentities = foreach ($relativePath in $sourcePaths) {
    $absolutePath = Join-Path $repoRoot $relativePath
    $blob = (& git -C $repoRoot hash-object -- $relativePath | Out-String).Trim()
    [ordered]@{
        path = $relativePath
        git_blob = $blob
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $absolutePath).Hash.ToLowerInvariant()
    }
}
$head = (& git -C $repoRoot rev-parse HEAD | Out-String).Trim()
$branch = (& git -C $repoRoot branch --show-current | Out-String).Trim()
$status = @(& git -C $repoRoot status --short)

$stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$stdoutPath = Join-Path $evidenceRoot "$Phase.stdout.log"
$stderrPath = Join-Path $evidenceRoot "$Phase.stderr.log"
$receiptPath = Join-Path $evidenceRoot "$Phase.receipt.json"
Remove-Item -LiteralPath $stdoutPath, $stderrPath, $receiptPath -Force -ErrorAction SilentlyContinue

$startUtc = [DateTimeOffset]::UtcNow
$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = (Get-Command cargo.exe).Source
$startInfo.WorkingDirectory = $repoRoot
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
foreach ($argument in $arguments) { $startInfo.ArgumentList.Add($argument) }
$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo
if (-not $process.Start()) { throw 'Could not start the owned Cargo Nextest process.' }
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()
$cargoPid = $process.Id
$seenPids = [System.Collections.Generic.HashSet[int]]::new()
[void] $seenPids.Add($cargoPid)
$processTreeInventoryErrors = [System.Collections.Generic.List[string]]::new()
$timedOut = $false
while (-not $process.WaitForExit(250)) {
    try {
        $snapshot = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
        $changed = $true
        while ($changed) {
            $changed = $false
            foreach ($child in $snapshot) {
                $childPid = [int] $child.ProcessId
                if ($seenPids.Contains([int] $child.ParentProcessId) -and $seenPids.Add($childPid)) {
                    $changed = $true
                }
            }
        }
    } catch {
        # The Cargo process and its root PID remain authoritative if process
        # inventory is unavailable to this user session.
        $processTreeInventoryErrors.Add($_.Exception.Message)
    }
    if (([DateTimeOffset]::UtcNow - $startUtc).TotalSeconds -ge $capSeconds) {
        $timedOut = $true
        try { $process.Kill($true) } catch { }
        $process.WaitForExit()
        break
    }
}
$process.WaitForExit()
$stdout = $stdoutTask.GetAwaiter().GetResult()
$stderr = $stderrTask.GetAwaiter().GetResult()
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($stdoutPath, $stdout, $utf8)
[System.IO.File]::WriteAllText($stderrPath, $stderr, $utf8)
$endUtc = [DateTimeOffset]::UtcNow
$livePids = @()
try {
    $livePids = @((Get-CimInstance -ClassName Win32_Process -ErrorAction Stop) |
        Where-Object { $seenPids.Contains([int] $_.ProcessId) } |
        ForEach-Object { [int] $_.ProcessId })
} catch { $processTreeInventoryErrors.Add($_.Exception.Message) }
$exitCode = if ($timedOut) { $null } else { $process.ExitCode }
$receipt = [ordered]@{
    phase = $Phase
    verb = "cargo nextest $cargoVerb"
    command_arguments = @($arguments)
    filter = $filter
    head = $head
    branch = $branch
    source_identities = @($sourceIdentities)
    git_status_short_at_start = $status
    started_utc = $startUtc.ToString('o')
    finished_utc = $endUtc.ToString('o')
    elapsed_seconds = [Math]::Round(($endUtc - $startUtc).TotalSeconds, 3)
    cap_seconds = $capSeconds
    cargo_pid = $cargoPid
    observed_process_tree_pids = @($seenPids | Sort-Object)
    process_tree_inventory_status = if ($processTreeInventoryErrors.Count -eq 0) { 'observed' } else { 'limited' }
    process_tree_inventory_errors = @($processTreeInventoryErrors | Select-Object -Unique)
    live_owned_pids_at_drain = $livePids
    timed_out = $timedOut
    exit_code = $exitCode
    stdout_path = $stdoutPath
    stdout_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant()
    stderr_path = $stderrPath
    stderr_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()
    output_drained = $true
    supervisor_pid = $PID
}
$receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $receiptPath -Encoding utf8
Write-Output ($receipt | ConvertTo-Json -Depth 8)
if ($timedOut) { exit 124 }
exit $process.ExitCode
