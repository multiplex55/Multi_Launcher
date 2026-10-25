# Structured Nextest result parsing and immutable failed-run replay validation.
# Dot-source from engine.ps1 so all test identity handling shares the inventory
# parser used by the existing test-performance tooling.

function Get-WorkflowResultIdentityKey {
    param([Parameter(Mandatory)][string]$BinaryId, [Parameter(Mandatory)][string]$TestName)
    return $BinaryId + [char]31 + $TestName
}

function Get-WorkflowStructuredResults {
    param(
        [Parameter(Mandatory)][string]$StdoutPath,
        [Parameter(Mandatory)][string]$InventoryPath,
        [AllowNull()]$SelectedIdentity,
        [Parameter(Mandatory)]$Measurement
    )

    $errors = [System.Collections.Generic.List[string]]::new()
    $started = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $attempts = [System.Collections.Generic.Dictionary[string, System.Collections.Generic.List[object]]]::new([StringComparer]::Ordinal)
    $suiteStartedCount = 0
    $suiteTerminalCount = 0
    $suiteActiveCount = 0
    $suiteTerminalStatuses = [System.Collections.Generic.List[string]]::new()
    $inventoryCases = @(Convert-TestPerfInventory -Path $InventoryPath)
    $selectedCases = @($inventoryCases | Where-Object { !$_.Ignored -and $_.FilterStatus -eq 'matches' })
    $selectedCount = [int](Get-TestPerfSelectedCaseCount -Cases $inventoryCases)
    $identityMap = New-TestPerfInventoryIdentityMap -Cases $selectedCases
    $validInventoryKeys = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($case in $selectedCases) { [void]$validInventoryKeys.Add((Get-WorkflowResultIdentityKey -BinaryId ([string]$case.BinaryId) -TestName ([string]$case.TestName))) }
    if ($null -ne $SelectedIdentity) {
        $expectedKey = Get-WorkflowResultIdentityKey -BinaryId ([string]$SelectedIdentity.BinaryId) -TestName ([string]$SelectedIdentity.TestName)
        $expectedInventory = @($selectedCases | Where-Object {
            [string]$_.BinaryId -ceq [string]$SelectedIdentity.BinaryId -and [string]$_.TestName -ceq [string]$SelectedIdentity.TestName
        })
        if ($selectedCount -ne 1 -or $expectedInventory.Count -ne 1) {
            $errors.Add("Validated inventory no longer contains exactly the selected binary-qualified case (selected=$selectedCount, matching=$($expectedInventory.Count)).")
        }
    }

    if (!(Test-Path -LiteralPath $StdoutPath -PathType Leaf)) {
        $errors.Add('Structured result output file is missing.')
    }
    else {
        foreach ($line in [IO.File]::ReadLines($StdoutPath)) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            try {
                $raw = ConvertFrom-Json -InputObject $line -AsHashtable -ErrorAction Stop
                $type = [string](Get-TestPerfField $raw 'type')
                if ($type -eq 'suite') {
                    $suiteEvent = ([string](Get-TestPerfField $raw 'event')).ToLowerInvariant()
                    if ($suiteEvent -ceq 'started') {
                        $suiteStartedCount++
                        $suiteActiveCount++
                    }
                    elseif ($suiteEvent -in @('ok', 'failed')) {
                        if ($suiteActiveCount -le 0) { $errors.Add("Suite terminal event '$suiteEvent' has no matching start.") }
                        else {
                            $suiteActiveCount--
                            $suiteTerminalCount++
                            $suiteTerminalStatuses.Add($suiteEvent)
                        }
                    }
                    else { $errors.Add("Unknown suite event '$suiteEvent'.") }
                    continue
                }
                if ($type -cne 'test') {
                    $errors.Add("Unknown structured message type '$type'.")
                    continue
                }

                $message = Convert-TestPerfJsonMessage -Line $line -IdentityMap $identityMap
                if ($message.IdentityStatus -ne 'resolved') {
                    $errors.Add("Test identity '$($message.TestName)' is $($message.IdentityStatus).")
                    continue
                }
                $caseSeparator = ([string]$message.TestName).IndexOf('$')
                if ($caseSeparator -lt 0 -or $caseSeparator -eq ([string]$message.TestName).Length - 1) {
                    $errors.Add("Test event name '$($message.TestName)' does not contain a binary/case separator.")
                    continue
                }
                $caseName = ([string]$message.TestName).Substring($caseSeparator + 1)
                $key = Get-WorkflowResultIdentityKey -BinaryId ([string]$message.BinaryId) -TestName $caseName
                if (!$validInventoryKeys.Contains($key)) {
                    $errors.Add("Structured event identity '$($message.BinaryId) / $caseName' is outside the selected binary-qualified case.")
                    continue
                }
                if ($message.Event -ceq 'started' -or $message.Status -ceq 'STARTED') {
                    [void]$started.Add($key)
                    continue
                }
                if ($message.Status -notin @('PASS', 'FAIL', 'TIMEOUT', 'IGNORED', 'FILTERED')) {
                    $errors.Add("Unknown terminal test status '$($message.Status)' for '$($message.TestName)'.")
                    continue
                }
                if (!$attempts.ContainsKey($key)) { $attempts.Add($key, [System.Collections.Generic.List[object]]::new()) }
                $attempts[$key].Add([pscustomobject][ordered]@{
                    Event = [string]$message.Event
                    Status = [string]$message.Status
                    DurationSeconds = $message.DurationSeconds
                })
            }
            catch { $errors.Add("Malformed structured output line: $($_.Exception.Message)") }
        }
    }

    $finalCases = [System.Collections.Generic.List[object]]::new()
    foreach ($key in $attempts.Keys) {
        $caseAttemptList = $attempts[$key]
        $caseAttempts = @($caseAttemptList.ToArray())
        $final = $caseAttempts[-1]
        $durations = @($caseAttempts | Where-Object { $null -ne $_.DurationSeconds } | ForEach-Object { [double]$_.DurationSeconds })
        $durationTotal = if ($durations.Count -eq $caseAttempts.Count) { [Math]::Round(($durations | Measure-Object -Sum).Sum, 6) } else { $null }
        if ($null -eq $durationTotal -or $durationTotal -lt 0) { $errors.Add("A terminal duration is missing or invalid for '$($final.Event)'.") }
        $testName = $key.Substring($key.IndexOf([char]31) + 1)
        $binaryId = $key.Substring(0, $key.IndexOf([char]31))
        $finalCases.Add([pscustomobject][ordered]@{
            BinaryId = $binaryId
            TestName = $testName
            Status = [string]$final.Status
            AttemptCount = $caseAttempts.Count
            DurationSeconds = $durationTotal
            LastAttemptDurationSeconds = if ($null -ne $final.DurationSeconds) { [double]$final.DurationSeconds } else { $null }
            Attempts = $caseAttemptList.ToArray()
        })
    }

    $terminalCount = $finalCases.Count
    $startedOnly = @($started | Where-Object { !$attempts.ContainsKey($_) }).Count
    $unexecutedCount = [Math]::Max(0, $selectedCount - $terminalCount)
    $passed = @($finalCases | Where-Object Status -eq 'PASS').Count
    $failed = @($finalCases | Where-Object Status -eq 'FAIL').Count
    $timedOut = @($finalCases | Where-Object Status -eq 'TIMEOUT').Count
    $ignored = @($finalCases | Where-Object Status -eq 'IGNORED').Count
    $filtered = @($finalCases | Where-Object Status -eq 'FILTERED').Count
    $unknown = @($errors | Where-Object { $_ -match 'Unknown|ambiguous|outside the selected|unresolved' }).Count
    $slowCases = @($finalCases | Where-Object { $null -ne $_.DurationSeconds } | Sort-Object -Property @{ Expression = 'DurationSeconds'; Descending = $true }, @{ Expression = 'BinaryId'; Descending = $false }, @{ Expression = 'TestName'; Descending = $false })
    $confirmedFailures = @($finalCases | Where-Object { $_.Status -in @('FAIL', 'TIMEOUT') } | ForEach-Object {
        [pscustomobject][ordered]@{ BinaryId = $_.BinaryId; TestName = $_.TestName; Status = $_.Status }
    })
    $phaseReliabilityErrors = [System.Collections.Generic.List[string]]::new()
    foreach ($propertyName in @('RecordingErrors', 'CleanupErrors', 'RestorationErrors')) {
        $measurementProperty = $Measurement.PSObject.Properties[$propertyName]
        if ($null -eq $measurementProperty -or $null -eq $measurementProperty.Value) { continue }
        foreach ($errorText in @($measurementProperty.Value)) {
            if ($null -ne $errorText -and ![string]::IsNullOrWhiteSpace([string]$errorText)) {
                $phaseReliabilityErrors.Add(('{0}: {1}' -f $propertyName, [string]$errorText))
            }
        }
    }
    $phaseReliable = !$Measurement.TimedOut -and [bool]$Measurement.CleanupVerified -and [bool]$Measurement.OutputDrainVerified -and
        $null -ne $Measurement.ExitCode -and $phaseReliabilityErrors.Count -eq 0
    $suiteComplete = $suiteStartedCount -gt 0 -and $suiteStartedCount -eq $suiteTerminalCount -and $suiteActiveCount -eq 0
    if (!$suiteComplete) { $errors.Add("Suite stream is incomplete (started=$suiteStartedCount, terminal=$suiteTerminalCount, active=$suiteActiveCount).") }
    $complete = $errors.Count -eq 0 -and $phaseReliable -and $selectedCount -gt 0 -and $terminalCount -eq $selectedCount -and $startedOnly -eq 0
    $timeoutCountKnown = $complete -and $failed -eq 0
    $knownTimedOutTotal = if ($timeoutCountKnown) { $timedOut } else { $null }
    $timeoutCountReason = if ($timeoutCountKnown) { $null }
        elseif ($complete -and $failed -gt 0) { 'A final case failure lacks reliable timeout classification; the total timed-out case count is unknown.' }
        else { 'Structured output or execution provenance was incomplete; the total timed-out case count is unknown.' }

    return [pscustomobject][ordered]@{
        SchemaVersion = 1
        Format = 'nextest-libtest-json-plus-0.1'
        Selected = $selectedCount
        Passed = $passed
        Failed = $failed
        TimedOut = $knownTimedOutTotal
        ObservedTimedOut = $timedOut
        TimeoutCountKnown = $timeoutCountKnown
        TimeoutCountReason = $timeoutCountReason
        Ignored = $ignored
        Filtered = $filtered
        Skipped = $ignored + $filtered
        Unexecuted = $unexecutedCount
        StartedOnly = $startedOnly
        Unknown = $unknown
        Complete = $complete
        NativeExitCode = $Measurement.ExitCode
        StageTimedOut = [bool]$Measurement.TimedOut
        PhaseReliable = $phaseReliable
        PhaseReliabilityErrors = @($phaseReliabilityErrors)
        CleanupVerified = [bool]$Measurement.CleanupVerified
        OutputDrainVerified = [bool]$Measurement.OutputDrainVerified
        ExecutionWallSeconds = $Measurement.WallSeconds
        SuiteStartedCount = $suiteStartedCount
        SuiteTerminalCount = $suiteTerminalCount
        SuiteActiveAtEnd = $suiteActiveCount
        SuiteTerminalStatuses = @($suiteTerminalStatuses)
        SuiteComplete = $suiteComplete
        TerminalAttemptCount = @($attempts.Values | ForEach-Object Count | Measure-Object -Sum).Sum
        Cases = @($finalCases)
        SlowCases = $slowCases
        ConfirmedFailures = $confirmedFailures
        ParseErrors = @($errors)
    }
}

function Get-WorkflowReplayFilterExpression {
    param([Parameter(Mandatory)]$Failures)
    $failureRows = @($Failures)
    if ($failureRows.Count -eq 0) { throw 'Replay requires at least one confirmed failed test identity.' }
    $expressions = [System.Collections.Generic.List[string]]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($failure in $failureRows) {
        $binaryId = [string]$failure.BinaryId
        $testName = [string]$failure.TestName
        if ([string]::IsNullOrWhiteSpace($binaryId) -or [string]::IsNullOrWhiteSpace($testName) -or
            $binaryId.Contains("`n") -or $binaryId.Contains("`r") -or $testName.Contains("`n") -or $testName.Contains("`r")) {
            throw 'Replay contains a malformed binary-qualified test identity.'
        }
        $identityKey = Get-WorkflowResultIdentityKey -BinaryId $binaryId -TestName $testName
        if (!$seen.Add($identityKey)) { throw 'Replay contains duplicate failed test identities.' }
        $binaryPattern = [regex]::Escape($binaryId).Replace('/', '\/')
        $testPattern = [regex]::Escape($testName).Replace('/', '\/')
        $expressions.Add("binary_id(/^$binaryPattern`$/) & test(/^$testPattern`$/)")
    }
    if ($expressions.Count -eq 1) { return $expressions[0] }
    return '(' + ($expressions -join ') | (') + ')'
}

function New-WorkflowRecordingContext {
    param([Parameter(Mandatory)][string]$EvidenceDirectory)
    $configPath = Join-Path $EvidenceDirectory 'nextest-user-config.toml'
    $config = "[experimental]`nrecord = true`n`n[record]`nenabled = true`n"
    [IO.File]::WriteAllText($configPath, $config, [Text.UTF8Encoding]::new($false))
    $stateRoot = Join-Path ([IO.Path]::GetTempPath()) ('mlnx-' + [guid]::NewGuid().ToString('N').Substring(0, 16))
    [IO.Directory]::CreateDirectory($stateRoot) | Out-Null
    return [pscustomobject][ordered]@{
        ConfigPath = $configPath
        ConfigSha256 = (Get-FileHash -LiteralPath $configPath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
        StateDirectory = [IO.Path]::GetFullPath($stateRoot)
        EnvironmentOverrides = @{
            NEXTEST_EXPERIMENTAL_RECORD = '1'
            NEXTEST_EXPERIMENTAL_LIBTEST_JSON = '1'
            NEXTEST_USER_CONFIG_FILE = [IO.Path]::GetFullPath($configPath)
            NEXTEST_STATE_DIR = [IO.Path]::GetFullPath($stateRoot)
        }
        CleanupVerified = $false
        CleanupError = $null
    }
}

function Remove-WorkflowRecordingContext {
    param([Parameter(Mandatory)]$Context)
    try {
        if (Test-Path -LiteralPath $Context.StateDirectory -PathType Container) {
            $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
            $statePath = [IO.Path]::GetFullPath([string]$Context.StateDirectory)
            if (!$statePath.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Nextest state directory escaped the system temp root.' }
            Remove-Item -LiteralPath $statePath -Recurse -Force -ErrorAction Stop
        }
        $Context.CleanupVerified = !(Test-Path -LiteralPath $Context.StateDirectory)
        if (!$Context.CleanupVerified) { throw 'Nextest private state directory remains after cleanup.' }
    }
    catch {
        $Context.CleanupVerified = $false
        $Context.CleanupError = $_.Exception.Message
    }
}

function Get-WorkflowManagedFilePath {
    param([Parameter(Mandatory)][string]$Path)
    $fullPath = [IO.Path]::GetFullPath($Path)
    if (![OperatingSystem]::IsWindows()) { return $fullPath }
    if ($fullPath.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) { return $fullPath }
    if ($fullPath.StartsWith('\\', [StringComparison]::Ordinal)) { return '\\?\UNC\' + $fullPath.Substring(2) }
    return '\\?\' + $fullPath
}

function Get-WorkflowArchiveIdentity {
    param([Parameter(Mandatory)][string]$Path)
    $fullPath = [IO.Path]::GetFullPath($Path)
    $stream = $null
    $sha = $null
    try {
        $stream = [IO.FileStream]::new((Get-WorkflowManagedFilePath -Path $fullPath), [IO.FileMode]::Open, [IO.FileAccess]::Read,
            [IO.FileShare]::Read, 65536, [IO.FileOptions]::SequentialScan)
        $length = $stream.Length
        $sha = [Security.Cryptography.SHA256]::Create()
        $digest = $sha.ComputeHash($stream)
        return [pscustomobject][ordered]@{
            Path = $fullPath
            ByteLength = $length
            Sha256 = [Convert]::ToHexString($digest).ToLowerInvariant()
        }
    }
    finally {
        if ($sha) { $sha.Dispose() }
        if ($stream) { $stream.Dispose() }
    }
}

function Copy-WorkflowArchive {
    param(
        [Parameter(Mandatory)][string]$SourcePath,
        [Parameter(Mandatory)][string]$DestinationPath
    )
    $source = [IO.Path]::GetFullPath($SourcePath)
    $destination = [IO.Path]::GetFullPath($DestinationPath)
    if ($source.Equals($destination, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Recording archive staging and publication paths must be distinct.'
    }
    $sourceIdentity = Get-WorkflowArchiveIdentity -Path $source
    if ($sourceIdentity.ByteLength -le 0) { throw 'Nextest produced an empty portable recording archive.' }

    $sourceStream = $null
    $destinationStream = $null
    $destinationCreated = $false
    try {
        $sourceStream = [IO.FileStream]::new((Get-WorkflowManagedFilePath -Path $source), [IO.FileMode]::Open, [IO.FileAccess]::Read,
            [IO.FileShare]::Read, 65536, [IO.FileOptions]::SequentialScan)
        $destinationStream = [IO.FileStream]::new((Get-WorkflowManagedFilePath -Path $destination), [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write, [IO.FileShare]::None, 65536, [IO.FileOptions]::SequentialScan)
        $destinationCreated = $true
        $sourceStream.CopyTo($destinationStream)
        $destinationStream.Flush($true)
    }
    catch {
        $copyError = $_.Exception.Message
        if ($destinationCreated) {
            try { [IO.File]::Delete((Get-WorkflowManagedFilePath -Path $destination)) }
            catch { throw "Archive publication failed ($copyError), and the partial destination could not be removed: $($_.Exception.Message)" }
        }
        throw
    }
    finally {
        if ($destinationStream) { $destinationStream.Dispose() }
        if ($sourceStream) { $sourceStream.Dispose() }
    }

    $destinationIdentity = Get-WorkflowArchiveIdentity -Path $destination
    if ($destinationIdentity.ByteLength -ne $sourceIdentity.ByteLength -or $destinationIdentity.Sha256 -cne $sourceIdentity.Sha256) {
        try { [IO.File]::Delete((Get-WorkflowManagedFilePath -Path $destination)) }
        catch { throw "Published archive identity did not match its staging file, and the mismatched destination could not be removed: $($_.Exception.Message)" }
        throw 'Published archive byte length or SHA-256 differs from the private staging file.'
    }
    return [pscustomobject][ordered]@{
        Source = $sourceIdentity
        Destination = $destinationIdentity
        ContentMatches = $true
    }
}

function Stage-WorkflowReplayArchive {
    param([Parameter(Mandatory)]$Request, [Parameter(Mandatory)]$RecordingContext)
    if ($Request.Action -cne 'Replay' -or [string]::IsNullOrWhiteSpace([string]$Request.ReplayRecordingPath) -or
        $null -eq $Request.ReplaySource -or [string]::IsNullOrWhiteSpace([string]$Request.ReplaySource.RecordingSha256)) {
        throw 'Replay has no validated original archive identity to stage.'
    }
    $stagingPath = Join-Path ([string]$RecordingContext.StateDirectory) 'replay-input.zip'
    $copy = Copy-WorkflowArchive -SourcePath ([string]$Request.ReplayRecordingPath) -DestinationPath $stagingPath
    if ($copy.Destination.Sha256 -cne [string]$Request.ReplaySource.RecordingSha256) {
        try { [IO.File]::Delete((Get-WorkflowManagedFilePath -Path $stagingPath)) }
        catch { throw "Replay archive staging identity differed, and the private copy could not be removed: $($_.Exception.Message)" }
        throw 'Private replay archive copy does not match the immutable original recording hash.'
    }

    $arguments = @($Request.RunCommand.ArgumentList | ForEach-Object { [string]$_ })
    $recordingIndex = [Array]::IndexOf([string[]]$arguments, '-R')
    if ($recordingIndex -lt 0 -or $recordingIndex + 1 -ge $arguments.Count -or
        $arguments[$recordingIndex + 1] -cne [string]$Request.ReplayRecordingPath) {
        try { [IO.File]::Delete((Get-WorkflowManagedFilePath -Path $stagingPath)) }
        catch { throw "Replay archive command did not reference the validated original, and the private copy could not be removed: $($_.Exception.Message)" }
        throw 'Replay command did not reference the validated original portable recording.'
    }
    $arguments[$recordingIndex + 1] = $stagingPath
    $Request.RunCommand.ArgumentList = $arguments
    if ($Request.Phases.Count -ge 4) { $Request.Phases[3].Command = $Request.RunCommand }
    $runtimeIdentity = $copy.Destination
    $Request.ReplaySource | Add-Member -NotePropertyName RuntimeRecording -NotePropertyValue ([pscustomobject][ordered]@{
        PathScope = 'private Nextest state'
        FileName = [IO.Path]::GetFileName($stagingPath)
        Sha256 = $runtimeIdentity.Sha256
        ByteLength = $runtimeIdentity.ByteLength
        CleanupVerified = $false
    }) -Force
    return [pscustomobject][ordered]@{
        PathScope = 'private Nextest state'
        FileName = [IO.Path]::GetFileName($stagingPath)
        Sha256 = $runtimeIdentity.Sha256
        ByteLength = $runtimeIdentity.ByteLength
        CleanupVerified = $false
    }
}

function New-WorkflowNextestStoreCommand {
    param(
        [Parameter(Mandatory)][string]$CargoExecutable,
        [Parameter(Mandatory)][ValidateSet('list', 'export')][string]$Operation,
        [string]$RunId,
        [string]$ArchivePath
    )
    $manifestPath = [IO.Path]::GetFullPath((Join-Path $script:WorkflowRoot 'Cargo.toml'))
    $arguments = @('nextest', 'store', $Operation, '--manifest-path', $manifestPath)
    if ($Operation -eq 'export') {
        if ([string]::IsNullOrWhiteSpace($RunId) -or [string]::IsNullOrWhiteSpace($ArchivePath)) { throw 'Store export requires a resolved run ID and archive path.' }
        $arguments += @($RunId, '--archive-file', [IO.Path]::GetFullPath($ArchivePath))
    }
    return [pscustomobject][ordered]@{
        Executable = [IO.Path]::GetFullPath($CargoExecutable)
        ArgumentList = $arguments
        WorkingDirectory = $script:WorkflowRoot
        Started = $false
    }
}

function Get-WorkflowRecordedRunId {
    param([Parameter(Mandatory)][string]$RunStdoutPath, [Parameter(Mandatory)][string]$RunStderrPath, [Parameter(Mandatory)][string]$StoreListPath)
    $runOutput = (@(Get-Content -LiteralPath $RunStdoutPath -Raw -ErrorAction Stop) + @(Get-Content -LiteralPath $RunStderrPath -Raw -ErrorAction Stop)) -join "`n"
    $guidPattern = '(?i)(?<![0-9a-f])([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(?![0-9a-f])'
    $runIds = @([regex]::Matches($runOutput, $guidPattern) | ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() } | Sort-Object -Unique)
    if ($runIds.Count -ne 1) { throw "Nextest output did not expose exactly one recorded run UUID (found $($runIds.Count))." }
    $storeText = Get-Content -LiteralPath $StoreListPath -Raw -ErrorAction Stop
    $runPrefix = $runIds[0].Substring(0, 8)
    if (!$storeText.Contains($runIds[0]) -and !$storeText.Contains($runPrefix)) {
        throw 'The isolated Nextest store listing does not contain the run UUID reported by the selected run.'
    }
    return $runIds[0]
}

function Test-WorkflowReplayReceipt {
    param([Parameter(Mandatory)][string]$Path)
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Original failed-run receipt is missing: $Path" }
    try { $receipt = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop }
    catch { throw "Original failed-run receipt is malformed: $($_.Exception.Message)" }
    if ($receipt.SchemaVersion -ne 1 -or $receipt.Action -cne 'Run' -or $receipt.Status -cne 'FAIL') {
        throw 'Replay requires a supported immutable failed Run receipt; successful or prior Replay receipts are not accepted.'
    }
    if ($receipt.Request.Mode -cne 'fast-dev-tests' -or [string]::IsNullOrWhiteSpace([string]$receipt.Request.SelectionName)) {
        throw 'Original receipt does not describe one supported selected test workflow.'
    }
    if ($receipt.Request.CatalogSha256 -cne (Get-WorkflowCatalogSha256)) {
        throw 'Original receipt catalog fingerprint is stale; replay is refused because the selected test contract may have changed.'
    }
    $currentSourceFingerprint = Get-WorkflowSourceFingerprint
    $sourceChanged = [string]$receipt.Request.SourceFingerprintSha256 -cne $currentSourceFingerprint
    $cargoPath = Get-WorkflowCargoPath
    $nextest = Get-WorkflowNextestIdentity
    if ([string]$receipt.Request.CargoExecutable -cne $cargoPath -or
        [string]$receipt.Request.NextestTool.Path -cne $nextest.Path -or [string]$receipt.Request.NextestTool.Sha256 -cne $nextest.Sha256) {
        throw 'Original Cargo or Nextest tool identity changed; replay is refused.'
    }
    if ($null -eq $receipt.Results -or !$receipt.Results.Complete) { throw 'Original failed-run receipt has incomplete or untrusted structured results.' }
    $failures = @($receipt.Results.ConfirmedFailures)
    if ($failures.Count -eq 0) { throw 'Original failed-run receipt contains no confirmed failed test identities.' }
    if ($failures.Count -ne 1) { throw 'This facade selection is expected to resolve to one exact case; ambiguous multi-case replay is refused.' }
    $identity = $receipt.SelectedIdentity
    if ($null -eq $identity -or [string]$failures[0].BinaryId -cne [string]$identity.BinaryId -or
        [string]$failures[0].TestName -cne [string]$identity.TestName) {
        throw 'Original failure identity does not match the exact selected binary and case.'
    }
    $recording = $receipt.Recording
    if ($null -eq $recording -or $recording.Status -cne 'PASS' -or [string]::IsNullOrWhiteSpace([string]$recording.ArchivePath) -or
        [string]::IsNullOrWhiteSpace([string]$recording.ArchiveSha256)) {
        throw 'Original run has no verified portable Nextest recording.'
    }
    try { $archiveIdentity = Get-WorkflowArchiveIdentity -Path ([string]$recording.ArchivePath) }
    catch { throw "Original portable recording cannot be read: $($_.Exception.Message)" }
    if ($archiveIdentity.ByteLength -le 0) { throw 'Original portable recording is empty.' }
    $archiveHash = $archiveIdentity.Sha256
    if ($archiveHash -cne [string]$recording.ArchiveSha256) { throw 'Original portable recording hash changed; replay is refused.' }
    return [pscustomobject][ordered]@{
        Receipt = $receipt
        Failures = $failures
        ArchivePath = [string]$recording.ArchivePath
        ArchiveSha256 = $archiveHash
        SourceChanged = $sourceChanged
        OriginalSourceFingerprintSha256 = [string]$receipt.Request.SourceFingerprintSha256
        CurrentSourceFingerprintSha256 = $currentSourceFingerprint
    }
}
