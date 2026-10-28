#requires -Version 7.0
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'engine.ps1')

function Assert-WorkflowResultEqual {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -cne $Expected) { throw "$Label expected '$Expected', got '$Actual'." }
}

function Assert-WorkflowThrows {
    param([scriptblock]$Action, [string]$Label)
    $threw = $false
    try { & $Action | Out-Null } catch { $threw = $true }
    if (!$threw) { throw "$Label was expected to fail closed." }
}

function New-WorkflowResultInventory {
    param([Parameter(Mandatory)][object[]]$Cases)
    $suites = [ordered]@{}
    foreach ($case in $Cases) {
        $binaryId = [string]$case.BinaryId
        if (!$suites.Contains($binaryId)) {
            $suites[$binaryId] = [ordered]@{
                'binary-id' = $binaryId
                'package-name' = [string]$case.PackageName
                'binary-name' = [string]$case.BinaryName
                kind = [string]$case.Kind
                testcases = [ordered]@{}
            }
        }
        $suites[$binaryId].testcases[[string]$case.TestName] = [ordered]@{
            ignored = $false
            'filter-match' = [ordered]@{ status = 'matches' }
        }
    }
    return ConvertTo-Json -InputObject ([ordered]@{ 'rust-suites' = $suites }) -Compress -Depth 10
}

function New-WorkflowResultEvent {
    param([string]$Target, [string]$TestName, [string]$Event, [double]$DurationSeconds = 0.01)
    $name = 'multi_launcher::{0}${1}' -f $Target.Replace('-', '_'), $TestName
    $status = switch ($Event) { 'ok' { 'pass' } 'failed' { 'fail' } 'timeout' { 'timed_out' } default { $Event } }
    return ConvertTo-Json -InputObject ([ordered]@{
        type = 'test'; event = $Event; name = $name; exec_time = $DurationSeconds
        nextest = [ordered]@{ status = $status; time = [ordered]@{ exec_time = $DurationSeconds } }
    }) -Compress -Depth 6
}

function New-WorkflowResultSuiteEvent {
    param([Parameter(Mandatory)][ValidateSet('started', 'ok', 'failed')][string]$Event)
    return ConvertTo-Json -InputObject ([ordered]@{
        type = 'suite'; event = $Event; nextest = [ordered]@{ run_id = 'fixture-run-id' }
    }) -Compress -Depth 5
}

function New-WorkflowResultMeasurement {
    return [pscustomobject]@{
        TimedOut = $false; CleanupVerified = $true; OutputDrainVerified = $true
        ExitCode = 0; WallSeconds = 1.25
        RecordingErrors = @(); CleanupErrors = @(); RestorationErrors = @()
    }
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('mlnx-result-fixture-' + [guid]::NewGuid().ToString('N').Substring(0, 12))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
try {
    $inventoryPath = Join-Path $testRoot 'inventory.json'
    $stdoutPath = Join-Path $testRoot 'stdout.jsonl'

    $duplicateCases = @(
        [pscustomobject]@{ BinaryId = 'multi_launcher::same::lib'; PackageName = 'multi_launcher'; BinaryName = 'same'; Kind = 'lib'; TestName = 'duplicate' },
        [pscustomobject]@{ BinaryId = 'multi_launcher::same::test'; PackageName = 'multi_launcher'; BinaryName = 'same'; Kind = 'test'; TestName = 'duplicate' }
    )
    Set-Content -LiteralPath $inventoryPath -Value (New-WorkflowResultInventory -Cases $duplicateCases) -Encoding utf8
    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target same -TestName duplicate -Event failed),
        (New-WorkflowResultSuiteEvent -Event failed)
    ) -join "`n") -Encoding utf8
    $duplicate = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $duplicate.Selected 2 'Duplicate-name selected count'
    Assert-WorkflowResultEqual $duplicate.Complete $false 'Duplicate binary identity is incomplete'
    Assert-WorkflowResultEqual $duplicate.ConfirmedFailures.Count 0 'Ambiguous identity is not replayable'
    if ($duplicate.Unknown -lt 1) { throw 'Duplicate binary identity must be reported as unknown.' }

    $multiCases = @(
        [pscustomobject]@{ BinaryId = 'multi_launcher::slow'; PackageName = 'multi_launcher'; BinaryName = 'slow'; Kind = 'lib'; TestName = 'case_slow' },
        [pscustomobject]@{ BinaryId = 'multi_launcher::fast'; PackageName = 'multi_launcher'; BinaryName = 'fast'; Kind = 'lib'; TestName = 'case_fast' }
    )
    Set-Content -LiteralPath $inventoryPath -Value (New-WorkflowResultInventory -Cases $multiCases) -Encoding utf8
    $retryEvents = [System.Collections.Generic.List[string]]::new()
    $retryEvents.Add((New-WorkflowResultSuiteEvent -Event started))
    $retryEvents.Add((New-WorkflowResultEvent -Target slow -TestName case_slow -Event started))
    $retryEvents.Add((New-WorkflowResultEvent -Target slow -TestName case_slow -Event failed -DurationSeconds 0.45))
    $retryEvents.Add((New-WorkflowResultEvent -Target slow -TestName case_slow -Event ok -DurationSeconds 0.2))
    $retryEvents.Add((New-WorkflowResultSuiteEvent -Event ok))
    $retryEvents.Add((New-WorkflowResultSuiteEvent -Event started))
    $retryEvents.Add((New-WorkflowResultEvent -Target fast -TestName case_fast -Event ok -DurationSeconds 0.01))
    $retryEvents.Add((New-WorkflowResultSuiteEvent -Event ok))
    $retryOutput = $retryEvents -join "`n"
    Set-Content -LiteralPath $stdoutPath -Value $retryOutput -Encoding utf8
    $retries = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    if (!$retries.Complete) { Write-Host ('Retry fixture summary: ' + (ConvertTo-Json -InputObject $retries -Depth 12 -Compress)) }
    Assert-WorkflowResultEqual $retries.Complete $true 'Terminal retry result complete'
    Assert-WorkflowResultEqual $retries.Selected 2 'Retry selected count'
    Assert-WorkflowResultEqual $retries.Passed 2 'Retry final statuses counted once'
    Assert-WorkflowResultEqual $retries.Failed 0 'Recovered retry is not a final failure'
    Assert-WorkflowResultEqual $retries.TerminalAttemptCount 3 'Retry attempts retained separately'
    Assert-WorkflowResultEqual $retries.SlowCases[0].TestName 'case_slow' 'Slow case ordering'
    Assert-WorkflowResultEqual $retries.SlowCases[0].AttemptCount 2 'Slow case retry count'
    Assert-WorkflowResultEqual $retries.SlowCases[0].DurationSeconds 0.65 'Slow case total execution duration'
    Assert-WorkflowResultEqual $retries.SuiteStartedCount 2 'Multiple suite starts retained'
    Assert-WorkflowResultEqual $retries.SuiteTerminalCount 2 'Multiple suite terminals retained'
    Assert-WorkflowResultEqual $retries.SuiteTerminalStatuses.Count 2 'Suite terminal statuses kept separate'

    $singleCase = @([pscustomobject]@{ BinaryId = 'multi_launcher::lib'; PackageName = 'multi_launcher'; BinaryName = 'multi_launcher'; Kind = 'lib'; TestName = 'timeout_case' })
    Set-Content -LiteralPath $inventoryPath -Value (New-WorkflowResultInventory -Cases $singleCase) -Encoding utf8
    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event started),
        (New-WorkflowResultSuiteEvent -Event ok)
    ) -join "`n") -Encoding utf8
    $startedOnly = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $startedOnly.Complete $false 'Started-only output is incomplete'
    Assert-WorkflowResultEqual $startedOnly.StartedOnly 1 'Started-only event count'
    Assert-WorkflowResultEqual $startedOnly.Unexecuted 1 'Started-only event is not terminal success'
    Assert-WorkflowResultEqual $startedOnly.Passed 0 'Started-only event never counts as passed'

    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event timeout),
        (New-WorkflowResultSuiteEvent -Event failed)
    ) -join "`n") -Encoding utf8
    $timeout = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $timeout.TimedOut 1 'Structured case timeout count'
    Assert-WorkflowResultEqual $timeout.ObservedTimedOut 1 'Observed case timeout count'
    Assert-WorkflowResultEqual $timeout.TimeoutCountKnown $true 'Complete stream makes timeout total known'
    Assert-WorkflowResultEqual $timeout.ConfirmedFailures.Count 1 'Timeout is a replayable confirmed failure'

    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event failed),
        (New-WorkflowResultSuiteEvent -Event failed)
    ) -join "`n") -Encoding utf8
    $genericCaseFailureMeasurement = New-WorkflowResultMeasurement
    $genericCaseFailureMeasurement.ExitCode = 7
    $genericCaseFailure = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement $genericCaseFailureMeasurement
    Assert-WorkflowResultEqual $genericCaseFailure.Complete $true 'Complete generic failure stream remains structurally complete'
    Assert-WorkflowResultEqual $genericCaseFailure.Failed 1 'Generic failure remains a case failure'
    Assert-WorkflowResultEqual $genericCaseFailure.TimedOut $null 'Generic case failure leaves timeout total unknown'
    Assert-WorkflowResultEqual $genericCaseFailure.ObservedTimedOut 0 'Generic case failure has zero observed timeout events'
    Assert-WorkflowResultEqual $genericCaseFailure.TimeoutCountKnown $false 'Generic case failure does not claim a known timeout total'
    if ([string]::IsNullOrWhiteSpace([string]$genericCaseFailure.TimeoutCountReason)) { throw 'Generic case failure must explain the unknown timeout total.' }

    foreach ($errorProperty in @('RecordingErrors', 'CleanupErrors', 'RestorationErrors')) {
        $unreliableMeasurement = New-WorkflowResultMeasurement
        $unreliableMeasurement.ExitCode = 7
        $unreliableMeasurement.$errorProperty = @("injected $errorProperty failure")
        $unreliableResults = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath `
            -SelectedIdentity $null -Measurement $unreliableMeasurement
        Assert-WorkflowResultEqual $unreliableResults.ParseErrors.Count 0 "$errorProperty leaves structured output parseable"
        Assert-WorkflowResultEqual $unreliableResults.Failed 1 "$errorProperty retains the confirmed failing case"
        Assert-WorkflowResultEqual $unreliableResults.Complete $false "$errorProperty makes phase results incomplete"
        Assert-WorkflowResultEqual $unreliableResults.PhaseReliable $false "$errorProperty marks phase provenance unreliable"
        Assert-WorkflowResultEqual $unreliableResults.PhaseReliabilityErrors.Count 1 "$errorProperty reliability evidence retained"

        if ($errorProperty -eq 'RecordingErrors') {
            $replayRefusalReceipt = [pscustomobject][ordered]@{
                SchemaVersion = 1
                Action = 'Run'
                Status = 'FAIL'
                Request = [pscustomobject][ordered]@{
                    Mode = 'fast-dev-tests'
                    SelectionName = 'tables'
                    CatalogSha256 = Get-WorkflowCatalogSha256
                    SourceFingerprintSha256 = Get-WorkflowSourceFingerprint
                    CargoExecutable = Get-WorkflowCargoPath
                    NextestTool = Get-WorkflowNextestIdentity
                }
                Results = $unreliableResults
                SelectedIdentity = [pscustomobject]@{ BinaryId = 'multi_launcher::lib'; TestName = 'timeout_case' }
                Recording = [pscustomobject]@{ Status = 'PASS'; ArchivePath = 'unused'; ArchiveSha256 = ('a' * 64) }
            }
            $replayRefusalPath = Join-Path $testRoot 'unreliable-recording-failed-run.json'
            Set-Content -LiteralPath $replayRefusalPath -Value (ConvertTo-Json -InputObject $replayRefusalReceipt -Depth 18) -Encoding utf8
            $replayRefusedForUnreliableResults = $false
            try { Test-WorkflowReplayReceipt -Path $replayRefusalPath | Out-Null }
            catch {
                if ($_.Exception.Message -notlike '*incomplete or untrusted structured results*') { throw }
                $replayRefusedForUnreliableResults = $true
            }
            if (!$replayRefusedForUnreliableResults) { throw 'Replay accepted a complete-looking failed run whose process-owner recording failed.' }
        }
    }

    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event ok)
    ) -join "`n") -Encoding utf8
    $missingSuiteTerminal = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $missingSuiteTerminal.Complete $false 'Missing suite terminal is incomplete'
    Assert-WorkflowResultEqual $missingSuiteTerminal.SuiteComplete $false 'Truncated suite stream is incomplete'
    Assert-WorkflowResultEqual $missingSuiteTerminal.TimedOut $null 'Incomplete suite has unknown timeout total'
    Assert-WorkflowResultEqual $missingSuiteTerminal.ObservedTimedOut 0 'Observed timeouts remain distinct from the unknown total'
    Assert-WorkflowResultEqual $missingSuiteTerminal.TimeoutCountKnown $false 'Incomplete suite does not claim a known timeout total'

    Set-Content -LiteralPath $stdoutPath -Value '' -Encoding utf8
    $genericFailureMeasurement = New-WorkflowResultMeasurement
    $genericFailureMeasurement.ExitCode = 7
    $genericFailure = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement $genericFailureMeasurement
    Assert-WorkflowResultEqual $genericFailure.TimedOut $null 'Generic process failure has unknown timeout total'
    Assert-WorkflowResultEqual $genericFailure.ObservedTimedOut 0 'Generic process failure has zero observed timeout events'
    Assert-WorkflowResultEqual $genericFailure.TimeoutCountKnown $false 'Generic process failure does not claim a known timeout total'

    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event started)
    ) -join "`n") -Encoding utf8
    $outerTimeoutMeasurement = New-WorkflowResultMeasurement
    $outerTimeoutMeasurement.TimedOut = $true
    $outerTimeout = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement $outerTimeoutMeasurement
    Assert-WorkflowResultEqual $outerTimeout.TimedOut $null 'Outer execution timeout has unknown case timeout total'
    Assert-WorkflowResultEqual $outerTimeout.ObservedTimedOut 0 'Outer execution timeout has zero observed case timeout events'
    Assert-WorkflowResultEqual $outerTimeout.TimeoutCountKnown $false 'Outer execution timeout does not claim a known case timeout total'

    Set-Content -LiteralPath $stdoutPath -Value '{broken-json' -Encoding utf8
    $malformed = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $malformed.Complete $false 'Malformed output is incomplete'
    if ($malformed.ParseErrors.Count -eq 0) { throw 'Malformed output must preserve its parse error.' }

    Set-Content -LiteralPath $stdoutPath -Value (@(
        (New-WorkflowResultSuiteEvent -Event started),
        (New-WorkflowResultEvent -Target multi_launcher -TestName timeout_case -Event mystery),
        (New-WorkflowResultSuiteEvent -Event failed)
    ) -join "`n") -Encoding utf8
    $unknown = Get-WorkflowStructuredResults -StdoutPath $stdoutPath -InventoryPath $inventoryPath -SelectedIdentity $null -Measurement (New-WorkflowResultMeasurement)
    Assert-WorkflowResultEqual $unknown.Complete $false 'Unknown terminal status is incomplete'
    if ($unknown.Unknown -eq 0) { throw 'Unknown terminal status must be counted.' }

    $replayFilter = Get-WorkflowReplayFilterExpression -Failures @([pscustomobject]@{
        BinaryId = 'multi_launcher::lib(quoted)'; TestName = 'case::with/slash "quote"!?'
    })
    if (!$replayFilter.Contains('binary_id(') -or !$replayFilter.Contains('test(') -or !$replayFilter.Contains('quoted')) {
        throw "Replay filter lost the exact binary/case identity: $replayFilter"
    }
    Assert-WorkflowThrows { Get-WorkflowReplayFilterExpression -Failures @() } 'Empty failed-test replay'
    Assert-WorkflowThrows { Get-WorkflowReplayFilterExpression -Failures @(
        [pscustomobject]@{ BinaryId = 'binary'; TestName = 'case' }, [pscustomobject]@{ BinaryId = 'binary'; TestName = 'case' }
    ) } 'Duplicate failed-test replay identity'
}
finally {
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
    $fullPath = [IO.Path]::GetFullPath($testRoot)
    if ($fullPath.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { Remove-Item -LiteralPath $fullPath -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Host 'Structured result, suite lifecycle, retry, ambiguity, unknown timeout, partial-output, slow-case, and replay-filter fixtures passed.'
