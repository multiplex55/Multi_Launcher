$ErrorActionPreference = 'Stop'
$repoRoot = (Get-Location).Path
$evidenceRoot = Join-Path $repoRoot 'docs/plans/execution/multi-launcher-linear-followons-20261030-r01/stages/G07/roles/implementer'
$rawRoot = Join-Path $repoRoot 'target/productivity_expansion/archive_native'
$utf8 = [Text.UTF8Encoding]::new($false)
$head = (git rev-parse HEAD).Trim()
$sourcePaths = @('src/file_operations.rs', 'src/archive/preparation.rs', 'src/archive/mod.rs')
$sourceBefore = [ordered]@{}
foreach ($path in $sourcePaths) {
    $sourceBefore[$path] = [ordered]@{
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        git_blob = (git hash-object -- $path).Trim()
    }
}
$filter = 'test(g07_) | test(=archive::catalog::tests::metadata_catalog_preserves_physical_duplicates_and_never_reads_payloads) | test(=archive::catalog::tests::unsupported_encrypted_unsafe_and_bad_encoding_entries_remain_visible) | test(=archive::plan::tests::physical_duplicate_names_keep_distinct_ids_and_conflict_only_when_both_selected) | test(=archive::plan::tests::windows_case_separator_and_file_ancestor_collisions_are_rejected) | test(=archive::plan::tests::read_only_occupancy_probe_blocks_existing_file_and_is_bounded) | test(=file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture)'
$arguments = @('nextest', 'list', '-p', 'multi_launcher', '--lib', '--features', 'perf-harness', '--cargo-profile', 'fast-dev', '--offline', '--locked', '--message-format', 'json', '-E', $filter)
$cargoExecutable = (Get-Command cargo.exe).Source
$stdoutPath = Join-Path $rawRoot 'g07-list-correction.stdout.json'
$stderrPath = Join-Path $evidenceRoot 'N1_list_correction.stderr.log'
$selectedPath = Join-Path $evidenceRoot 'N1_selected_inventory.corrected.json'
$statePath = Join-Path $evidenceRoot 'N1_list_correction.state.json'
$receiptPath = Join-Path $evidenceRoot 'N1_list_correction.receipt.json'
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $cargoExecutable
$startInfo.WorkingDirectory = $repoRoot
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
foreach ($argument in $arguments) { $startInfo.ArgumentList.Add($argument) }
$started = [DateTimeOffset]::UtcNow
$process = [Diagnostics.Process]::new()
$process.StartInfo = $startInfo
if (-not $process.Start()) { throw 'owned nextest list start failed' }
$directPid = $process.Id
$outTask = $process.StandardOutput.ReadToEndAsync()
$errTask = $process.StandardError.ReadToEndAsync()
$record = [ordered]@{
    phase = 'G07 exact12 nextest list correction'
    base_head = $head
    source_before = $sourceBefore
    command_arguments = $arguments
    filter = $filter
    started_utc = $started.ToString('o')
    cap_seconds = 600
    aggregate_correction_budget_seconds = 600
    prior_correction_command_elapsed_seconds = 0
    direct_pid = $directPid
    direct_process = $process.ProcessName
    direct_executable = $cargoExecutable
    status = 'RUNNING'
    timed_out = $false
    termination_error = $null
    output_drained = $false
    child_tree = 'UNVERIFIED; retained direct process object and no descendant inventory'
    selected_count = $null
}
[IO.File]::WriteAllText($statePath, ($record | ConvertTo-Json -Depth 10), $utf8)
$lastProgress = -15.0
$cap = 600.0
while (-not $process.WaitForExit(500)) {
    $elapsed = ([DateTimeOffset]::UtcNow - $started).TotalSeconds
    if ($elapsed - $lastProgress -ge 15) {
        Write-Output "G07 phase=list elapsed=$([Math]::Round($elapsed,1))s cap=${cap}s selected_count=pending-final-JSON pid=$directPid"
        $lastProgress = $elapsed
    }
    if ($elapsed -ge $cap) {
        $record.timed_out = $true
        try { $process.Kill($true) } catch { $record.termination_error = $_.Exception.Message }
        $retireDeadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
        while (-not $process.WaitForExit(250) -and [DateTimeOffset]::UtcNow -lt $retireDeadline) { }
        if (-not $process.HasExited) {
            $record.status = 'UNRETIRED_STOP_SOURCE_EDITS'
            $record['observed_utc'] = [DateTimeOffset]::UtcNow.ToString('o')
            [IO.File]::WriteAllText($statePath, ($record | ConvertTo-Json -Depth 10), $utf8)
            [IO.File]::WriteAllText($receiptPath, ($record | ConvertTo-Json -Depth 10), $utf8)
            Write-Output ($record | ConvertTo-Json -Depth 10)
            exit 124
        }
        break
    }
}
$drainDeadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
while ((-not $outTask.IsCompleted -or -not $errTask.IsCompleted) -and [DateTimeOffset]::UtcNow -lt $drainDeadline) { Start-Sleep -Milliseconds 250 }
$ended = [DateTimeOffset]::UtcNow
$record['ended_utc'] = $ended.ToString('o')
$record['elapsed_seconds'] = [Math]::Round(($ended - $started).TotalSeconds, 3)
$record['direct_pid_exit'] = $process.ExitCode
if ($record.elapsed_seconds -gt $cap) { $record.timed_out = $true }
if ($outTask.IsCompleted -and $errTask.IsCompleted) {
    $stdout = $outTask.GetAwaiter().GetResult()
    $stderr = $errTask.GetAwaiter().GetResult()
    [IO.File]::WriteAllText($stdoutPath, $stdout, $utf8)
    [IO.File]::WriteAllText($stderrPath, $stderr, $utf8)
    $record.output_drained = $true
    $record['stdout_path'] = $stdoutPath
    $record['stderr_path'] = $stderrPath
    $record['stdout_sha256'] = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant()
    $record['stderr_sha256'] = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()
    try {
        $inventory = ConvertFrom-Json -InputObject $stdout
        $selectedItems = [Collections.Generic.List[object]]::new()
        foreach ($suite in $inventory.'rust-suites') {
            foreach ($testcase in $suite.testcases) {
                if ($testcase.'filter-match'.status -eq 'matches' -and -not $testcase.ignored) {
                    $selectedItems.Add([pscustomobject]@{
                        binary = $suite.binary
                        package = $suite.package
                        name = $testcase.name
                        ignored = $testcase.ignored
                        filter_match_status = $testcase.'filter-match'.status
                    })
                }
            }
        }
        $selected = @($selectedItems.ToArray() | Sort-Object name)
        $expectedNames = @(
            'archive::preparation::tests::g07_captured_quotas_cannot_be_raised_and_refuse_early',
            'archive::preparation::tests::g07_physical_duplicates_bind_the_selected_range_without_reading_payload',
            'archive::preparation::tests::g07_physical_headers_ranges_and_execution_subset_refuse_before_reservation',
            'archive::preparation::tests::g07_preparation_cancellation_and_create_failure_abort_owned_stages',
            'file_operations::archive_io::tests::g07_native_source_holding_and_ancestor_conversion_are_fenced',
            'file_operations::archive_io::tests::g07_native_stages_publish_no_replace_and_retire_exact_ownership',
            'archive::catalog::tests::metadata_catalog_preserves_physical_duplicates_and_never_reads_payloads',
            'archive::catalog::tests::unsupported_encrypted_unsafe_and_bad_encoding_entries_remain_visible',
            'archive::plan::tests::physical_duplicate_names_keep_distinct_ids_and_conflict_only_when_both_selected',
            'archive::plan::tests::windows_case_separator_and_file_ancestor_collisions_are_rejected',
            'archive::plan::tests::read_only_occupancy_probe_blocks_existing_file_and_is_bounded',
            'file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture'
        )
        $actualNames = @($selected | ForEach-Object { $_.name })
        $record['missing_names'] = @($expectedNames | Where-Object { $_ -notin $actualNames })
        $record['unexpected_names'] = @($actualNames | Where-Object { $_ -notin $expectedNames })
        $record['selection_exact'] = $selected.Count -eq 12 -and $record.missing_names.Count -eq 0 -and $record.unexpected_names.Count -eq 0
        $record['selected_count'] = $selected.Count
        $record['selected_names'] = @($selected | ForEach-Object { $_.name })
        [IO.File]::WriteAllText($selectedPath, (ConvertTo-Json -InputObject $selected -Depth 6), $utf8)
        $record['selected_inventory_path'] = $selectedPath
        $record['selected_inventory_sha256'] = (Get-FileHash -Algorithm SHA256 -LiteralPath $selectedPath).Hash.ToLowerInvariant()
        $record['inventory_parse_error'] = $null
    } catch {
        $record['selected_count'] = 0
        $record['inventory_parse_error'] = $_.Exception.Message
    }
}
$record['source_after'] = [ordered]@{}
foreach ($path in $sourcePaths) {
    $record.source_after[$path] = [ordered]@{
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        git_blob = (git hash-object -- $path).Trim()
    }
}
$record['source_paths_changed_after'] = @(git diff --name-only -- $sourcePaths)
$record.status = if ($record.timed_out) { 'TIMEOUT' } elseif (-not $record.output_drained) { 'OUTPUT_UNDRAINED' } elseif ($process.ExitCode -ne 0) { 'LIST_FAILED' } elseif (-not $record.selection_exact) { 'SELECTION_COUNT_FAILED' } else { 'LIST_COMPLETE' }
[IO.File]::WriteAllText($statePath, ($record | ConvertTo-Json -Depth 10), $utf8)
[IO.File]::WriteAllText($receiptPath, ($record | ConvertTo-Json -Depth 10), $utf8)
Write-Output ($record | ConvertTo-Json -Depth 10)
if ($record.status -ne 'LIST_COMPLETE') { exit 1 }
