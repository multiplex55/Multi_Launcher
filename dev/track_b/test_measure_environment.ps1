#requires -Version 7.0
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) ('track-b-environment-state-' + [guid]::NewGuid().ToString('N'))))

$ErrorActionPreference = 'Stop'
$measureScript = Join-Path $PSScriptRoot 'measure.ps1'
$sourceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).ProviderPath
$pwshPath = (Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$testRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
if ($testRoot.Equals($sourceRoot, [StringComparison]::OrdinalIgnoreCase) -or
    $testRoot.StartsWith($sourceRoot.TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Environment-state test evidence must be outside the source worktree.'
}
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$runEvidence = Join-Path $testRoot 'measure-runs'
[IO.Directory]::CreateDirectory($runEvidence) | Out-Null
$childFixture = Join-Path $testRoot 'environment-state-child.ps1'
$environmentName = 'TRACK_B_ENV_STATE_' + [guid]::NewGuid().ToString('N').ToUpperInvariant()

@'
param([Parameter(Mandatory = $true)][string]$Name)
$variables = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
$present = $variables.Contains($Name)
$value = if ($present) { [string]$variables[$Name] } else { $null }
[Console]::Out.WriteLine((ConvertTo-Json -InputObject ([pscustomobject]@{
    Present = $present
    IsEmpty = $present -and $value -ceq ''
    Value = $value
}) -Compress))
'@ | Set-Content -LiteralPath $childFixture -Encoding utf8NoBOM

function Test-EnvironmentVariablePresent {
    param([Parameter(Mandatory = $true)][string]$Name)
    return [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($Name)
}

function Remove-EnvironmentVariable {
    param([Parameter(Mandatory = $true)][string]$Name)
    if (Test-EnvironmentVariablePresent -Name $Name) {
        Remove-Item -LiteralPath ('Env:{0}' -f $Name) -ErrorAction Stop
    }
    if (Test-EnvironmentVariablePresent -Name $Name) {
        throw "Environment variable remained present after removal: $Name"
    }
}

function Assert-EnvironmentState {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][bool]$ExpectedPresent,
        [AllowNull()][string]$ExpectedValue,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $variables = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
    $present = $variables.Contains($Name)
    if ($present -ne $ExpectedPresent) {
        throw "$Label presence mismatch: expected $ExpectedPresent, got $present."
    }
    if ($ExpectedPresent -and [string]$variables[$Name] -cne $ExpectedValue) {
        throw "$Label value mismatch (value content is intentionally omitted)."
    }
}

function Invoke-EnvironmentMeasurement {
    param(
        [Parameter(Mandatory = $true)][string]$Scenario,
        [AllowNull()][object]$OverrideValue
    )
    $arguments = @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $childFixture, '-Name', $environmentName)
    $overrides = @{}
    $overrides[$environmentName] = $OverrideValue
    $record = & $measureScript `
        -Executable $pwshPath `
        -ArgumentList $arguments `
        -ScenarioId $Scenario `
        -EvidenceDirectory $runEvidence `
        -WorkingDirectory $testRoot `
        -SourceRoot $sourceRoot `
        -Profile 'environment-state-test' `
        -CacheState 'unknown' `
        -EnvironmentOverrides $overrides `
        -FixtureMetadata @{ kind = 'controlled-environment-state'; scenario = $Scenario } `
        -FixturePaths @($childFixture) `
        -Stage 'provenance' `
        -TimeoutSeconds 60 `
        -CleanupTimeoutSeconds 5
    if (!$record.Succeeded -or $record.ExitCode -ne 0 -or !$record.CleanupVerified -or !$record.OutputDrainVerified) {
        throw "Environment fixture '$Scenario' did not complete cleanly; exit=$($record.ExitCode), reason=$($record.ExecutionReason)."
    }
    $child = Get-Content -LiteralPath $record.StdoutPath -Raw | ConvertFrom-Json -ErrorAction Stop
    return [pscustomobject]@{ Record = $record; Child = $child }
}

$originalVariables = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
$originalWasPresent = $originalVariables.Contains($environmentName)
$originalValue = if ($originalWasPresent) { [string]$originalVariables[$environmentName] } else { $null }
$marker = 'controlled-child-override'
$cases = [System.Collections.Generic.List[string]]::new()
try {
    Remove-EnvironmentVariable -Name $environmentName
    $absentRun = Invoke-EnvironmentMeasurement -Scenario 'unset-restores-absence' -OverrideValue $marker
    if (!$absentRun.Child.Present -or $absentRun.Child.Value -cne $marker) {
        throw 'A non-null override was not visible in the child process.'
    }
    Assert-EnvironmentState -Name $environmentName -ExpectedPresent $false -ExpectedValue $null -Label 'Restore originally absent value'
    $cases.Add('unset to value to absent')

    [Environment]::SetEnvironmentVariable($environmentName, '', [EnvironmentVariableTarget]::Process)
    Assert-EnvironmentState -Name $environmentName -ExpectedPresent $true -ExpectedValue '' -Label 'Establish originally empty value'
    $emptyRun = Invoke-EnvironmentMeasurement -Scenario 'empty-restores-empty' -OverrideValue $marker
    if (!$emptyRun.Child.Present -or $emptyRun.Child.Value -cne $marker) {
        throw 'A non-null override was not visible after an originally empty value.'
    }
    Assert-EnvironmentState -Name $environmentName -ExpectedPresent $true -ExpectedValue '' -Label 'Restore originally empty value'
    $cases.Add('empty to value to empty')

    [Environment]::SetEnvironmentVariable($environmentName, 'restore-after-null-override', [EnvironmentVariableTarget]::Process)
    $nullRun = Invoke-EnvironmentMeasurement -Scenario 'null-override-removes-only-for-child' -OverrideValue $null
    if ($nullRun.Child.Present) {
        throw 'A null override was expected to remove the variable for the child process.'
    }
    Assert-EnvironmentState -Name $environmentName -ExpectedPresent $true -ExpectedValue 'restore-after-null-override' -Label 'Restore value after null override'
    $cases.Add('null override removes; restores original value')

    $summary = [pscustomobject][ordered]@{
        Result = 'PASS'
        EvidenceDirectory = $testRoot
        VariableName = $environmentName
        Cases = @($cases)
        MeasureRunDirectories = @($absentRun.Record.EvidenceDirectory, $emptyRun.Record.EvidenceDirectory, $nullRun.Record.EvidenceDirectory)
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $testRoot 'environment-state-test.json') -Encoding utf8
    $summary
}
finally {
    if ($originalWasPresent) {
        [Environment]::SetEnvironmentVariable($environmentName, $originalValue, [EnvironmentVariableTarget]::Process)
    }
    else {
        Remove-EnvironmentVariable -Name $environmentName
    }
}
