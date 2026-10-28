#requires -Version 7.0
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) ('track b harness self test ' + [guid]::NewGuid().ToString('N'))))

$ErrorActionPreference = 'Stop'
$measureScript = Join-Path $PSScriptRoot 'measure.ps1'
$sourceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).ProviderPath
$pwshPath = (Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$testRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
$workingDirectory = Join-Path $testRoot 'working directory with spaces'
$evidenceRoot = Join-Path $testRoot 'measurement evidence with spaces'
$fixturePath = Join-Path $testRoot 'native child fixture.ps1'
$timingPath = Join-Path $testRoot 'stale timing report.html'
$environmentName = 'TRACK_B_SELF_TEST_' + [guid]::NewGuid().ToString('N').ToUpperInvariant()

if ($testRoot.StartsWith($sourceRoot.TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    $testRoot.Equals($sourceRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Self-test evidence must be outside the source worktree.'
}

[IO.Directory]::CreateDirectory($workingDirectory) | Out-Null
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
@'
param(
    [string]$EnvName,
    [string]$Mode,
    [int]$ExitCode,
    [string]$TimingPath,
    [string]$Value,
    [string]$TargetFlag,
    [string]$QuotedValue,
    [string]$FilterValue
)
if ($Mode -eq 'update-timing') {
    [IO.File]::WriteAllText($TimingPath, 'updated by this invocation')
}
$result = [ordered]@{
    WorkingDirectory = (Get-Location).Path
    EnvironmentValue = [Environment]::GetEnvironmentVariable($EnvName, 'Process')
    Value = $Value
    TargetFlag = $TargetFlag
    QuotedValue = $QuotedValue
    FilterValue = $FilterValue
}
[Console]::Out.WriteLine((ConvertTo-Json -InputObject $result -Compress))
[Console]::Error.WriteLine('fixture stderr line')
if ($Mode -eq 'sleep') { Start-Sleep -Seconds 30 }
exit $ExitCode
'@ | Set-Content -LiteralPath $fixturePath -Encoding utf8NoBOM
[IO.File]::WriteAllText($timingPath, 'old timing report')

$originalEnvironmentValue = [Environment]::GetEnvironmentVariable($environmentName, 'Process')
$originalCargoTargetDirectory = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
$initialLocation = (Get-Location).Path
[Environment]::SetEnvironmentVariable($environmentName, 'parent value', 'Process')

function Invoke-TestMeasurement {
    param([string]$Scenario, [string]$Mode, [int]$ChildExitCode, [int]$Timeout = 10, [switch]$UnsetTargetOverride)
    $quotedValue = 'embedded "quote" stays inside one argument'
    $filterValue = "module::filter[2], punctuation + && text"
    $arguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-File', $fixturePath,
        '-EnvName', $environmentName, '-Mode', $Mode, '-ExitCode', [string]$ChildExitCode,
        '-TimingPath', $timingPath, '-Value', 'one argument with spaces',
        '-TargetFlag', '--target value with spaces', '-QuotedValue', $quotedValue,
        '-FilterValue', $filterValue
    )
    $overrides = @{ $environmentName = 'child value' }
    $targetDirectory = Join-Path $workingDirectory 'target cache'
    if ($UnsetTargetOverride) {
        $overrides['CARGO_TARGET_DIR'] = $null
        $targetDirectory = $null
    }
    $measurement = & $measureScript `
        -Executable $pwshPath `
        -ArgumentList $arguments `
        -ScenarioId $Scenario `
        -EvidenceDirectory $evidenceRoot `
        -WorkingDirectory $workingDirectory `
        -SourceRoot $sourceRoot `
        -BaselineSha 'self-test-baseline' `
        -Profile 'self-test' `
        -CacheState 'warmed' `
        -TargetDirectory $targetDirectory `
        -PreviousCommand 'pwsh child fixture with spaces' `
        -SourceFilesModified 1 `
        -FingerprintFlags @('--test-fingerprint') `
        -TargetFlags @('--target', 'value with spaces') `
        -EnvironmentOverrides $overrides `
        -FixtureMetadata @{ kind = 'controlled-self-test'; scenario = $Scenario } `
        -FixturePaths @($fixturePath) `
        -CargoTimingsPath $timingPath `
        -Stage 'test' `
        -TimeoutSeconds $Timeout `
        -CleanupTimeoutSeconds 3
    return [pscustomobject]@{ Record = $measurement; Arguments = $arguments }
}

function Assert-Equal {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -cne $Expected) { throw "${Label}: expected '$Expected', got '$Actual'." }
}

try {
    $firstNoOp = Invoke-TestMeasurement -Scenario 'repeated no-op' -Mode 'no-op' -ChildExitCode 0
    $secondNoOp = Invoke-TestMeasurement -Scenario 'repeated no-op' -Mode 'no-op' -ChildExitCode 0
    $unsetTargetOverride = $null
    try {
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', (Join-Path $testRoot 'caller target with spaces'), 'Process')
        $unsetTargetOverride = Invoke-TestMeasurement -Scenario 'unset target override' -Mode 'no-op' -ChildExitCode 0 -UnsetTargetOverride
    }
    finally {
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $originalCargoTargetDirectory, 'Process')
    }
    $failed = Invoke-TestMeasurement -Scenario 'native exit 7' -Mode 'failure' -ChildExitCode 7
    $timedOut = Invoke-TestMeasurement -Scenario 'native timeout' -Mode 'sleep' -ChildExitCode 0 -Timeout 1
    $updatedTiming = Invoke-TestMeasurement -Scenario 'updated timing report' -Mode 'update-timing' -ChildExitCode 0

    foreach ($run in @($firstNoOp, $secondNoOp)) {
        $record = $run.Record
        if (!$record.Succeeded -or $record.ExitCode -ne 0) { throw 'The zero-exit fixture was not reported as successful.' }
        if ($record.Stage -ne 'test' -or !$record.DeadlineUtc -or !$record.CleanupVerified -or !$record.OutputDrainVerified) {
            throw 'The measurement omitted its bounded execution stage, deadline, cleanup, or output-drain receipt.'
        }
        if (@($record.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
            throw 'A successful measurement retained an owned process identity.'
        }
        if ($record.ArgumentList.Count -ne $run.Arguments.Count) { throw 'The recorded argument count changed.' }
        for ($index = 0; $index -lt $run.Arguments.Count; $index++) {
            Assert-Equal -Actual $record.ArgumentList[$index] -Expected $run.Arguments[$index] -Label "Argument $index"
        }
        if (!(Test-Path -LiteralPath $record.StdoutPath) -or !(Test-Path -LiteralPath $record.StderrPath)) { throw 'A native output file is missing.' }
        $childOutput = Get-Content -LiteralPath $record.StdoutPath -Raw | ConvertFrom-Json
        Assert-Equal -Actual $childOutput.WorkingDirectory -Expected $workingDirectory -Label 'Child working directory'
        Assert-Equal -Actual $childOutput.EnvironmentValue -Expected 'child value' -Label 'Child environment override'
        Assert-Equal -Actual $childOutput.Value -Expected 'one argument with spaces' -Label 'Argument containing spaces'
        Assert-Equal -Actual $childOutput.TargetFlag -Expected '--target value with spaces' -Label 'Target flag containing spaces'
        Assert-Equal -Actual $childOutput.QuotedValue -Expected 'embedded "quote" stays inside one argument' -Label 'Argument containing an embedded quote'
        Assert-Equal -Actual $childOutput.FilterValue -Expected 'module::filter[2], punctuation + && text' -Label 'Filter punctuation argument'
        if ($record.SourceSha -notmatch '^[0-9a-f]{40}$' -or $record.DirtyFiles -isnot [array]) { throw 'Source provenance was not recorded.' }
        if ($record.ComparisonContext.SchemaVersion -ne 1 -or $record.ComparisonContext.Scenario.WorkKind -ne 'unknown' -or
            $record.ComparisonContext.Scenario.CacheState -ne 'warmed') { throw 'Versioned comparison context or explicit work/cache classification is missing.' }
        if ($record.ComparisonContext.Tools.InvokedExecutable.Sha256 -notmatch '^[0-9a-f]{64}$' -or
            $record.ComparisonContext.Tools.Cargo.Status -ne 'not-applicable' -or
            $record.ComparisonContext.Tools.InvokedExecutable.VersionProbe -ne 'not-run for generic executable') {
            throw 'Generic measurement incorrectly ran or omitted its executable identity.'
        }
        $targetEnvironment = @($record.ComparisonContext.Environment.Values | Where-Object Name -ceq 'CARGO_TARGET_DIR')
        if ($targetEnvironment.Count -ne 1 -or $targetEnvironment[0].State -ne 'absent') {
            throw 'Environment provenance did not preserve the explicit absent target-dir state.'
        }
        if ($record.Fixtures.Count -ne 1 -or $record.Fixtures[0].Sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Fixture hash metadata was not recorded.' }
        if ($null -ne $record.CargoTimings.SnapshotPath -and (Test-Path -LiteralPath $record.CargoTimings.SnapshotPath)) {
            throw 'An unchanged stale timing report was copied.'
        }
        Assert-Equal -Actual $record.CargoTimings.Disposition -Expected 'unchanged-not-copied' -Label 'Stale timing disposition'
        if (!(Test-Path -LiteralPath $record.ManifestPath)) { throw 'Measurement manifest is missing.' }
    }
    if ($firstNoOp.Record.EvidenceDirectory -eq $secondNoOp.Record.EvidenceDirectory) { throw 'Repeated runs reused an evidence directory.' }
    if ($firstNoOp.Record.StdoutPath -eq $secondNoOp.Record.StdoutPath -or $firstNoOp.Record.ManifestPath -eq $secondNoOp.Record.ManifestPath) {
        throw 'Repeated runs reused output paths.'
    }
    if ($failed.Record.Succeeded -or $failed.Record.ExitCode -ne 7) { throw 'Native exit 7 was not propagated as a failed measurement.' }
    if ($failed.Record.ExecutionReason -ne 'failed exit' -or !$failed.Record.CleanupVerified -or !$failed.Record.OutputDrainVerified) {
        throw 'The exit-7 measurement did not preserve its failure reason and cleanup receipt.'
    }
    if ($timedOut.Record.Succeeded -or !$timedOut.Record.TimedOut -or $timedOut.Record.ExecutionReason -ne 'timeout' -or
        !$timedOut.Record.CleanupVerified -or !$timedOut.Record.OutputDrainVerified) {
        throw 'The timed-out measurement was not reported as a bounded, cleaned-up failure.'
    }
    if (@($timedOut.Record.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
        throw 'The timed-out measurement retained an owned process identity.'
    }
    Assert-Equal -Actual $unsetTargetOverride.Record.TargetDirectory `
        -Expected (Join-Path $workingDirectory 'target') -Label 'Unset CARGO_TARGET_DIR resolution'
    Assert-Equal -Actual $unsetTargetOverride.Record.TargetDirectorySource `
        -Expected 'environment-override' -Label 'Unset CARGO_TARGET_DIR provenance'
    if ($updatedTiming.Record.ExitCode -ne 0 -or $updatedTiming.Record.CargoTimings.Disposition -ne 'captured-new-or-updated') {
        throw 'A timing report updated by this invocation was not captured.'
    }
    Assert-Equal -Actual (Get-Content -LiteralPath $updatedTiming.Record.CargoTimings.SnapshotPath -Raw) `
        -Expected 'updated by this invocation' -Label 'Captured timing contents'
    Assert-Equal -Actual ([Environment]::GetEnvironmentVariable($environmentName, 'Process')) `
        -Expected 'parent value' -Label 'Caller environment restoration'
    Assert-Equal -Actual (Get-Location).Path -Expected $initialLocation -Label 'Caller location restoration'

    [pscustomobject]@{
        Result = 'PASS'
        EvidenceDirectory = $evidenceRoot
        Cases = @('exit 0', 'exit 7', 'timeout and cleanup receipt', 'paths and arguments with spaces, quotes, and filter punctuation', 'environment and location restoration', 'unique repeated no-op paths', 'stale timing suppression', 'updated timing snapshot')
    }
}
finally {
    [Environment]::SetEnvironmentVariable($environmentName, $originalEnvironmentValue, 'Process')
    [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $originalCargoTargetDirectory, 'Process')
}
