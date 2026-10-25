#requires -Version 7.0
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) ('track-b comparison fixtures ' + [guid]::NewGuid().ToString('N'))))

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'comparison.ps1')

function Assert-ComparisonEqual {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -cne $Expected) { throw "$Label expected '$Expected', got '$Actual'." }
}

function New-ComparisonFixture {
    param(
        [string]$Fingerprint = ('a' * 64),
        [string]$SourceHash = ('b' * 64),
        [string]$Commit = 'commit-a',
        [string]$WorkKind = 'unknown',
        [string]$CacheState = 'unknown',
        [string]$ControlledChangeId,
        [string[]]$ControlledChangePaths = @(),
        [object[]]$ChangedFiles = @(),
        [bool]$Succeeded = $true,
        [int]$ExitCode = 0,
        [bool]$TimedOut = $false,
        [bool]$ExecutionCompleted = $true,
        [double]$WallSeconds = 10,
        [string]$TargetDirectory = 'target/debug'
    )
    return [pscustomobject][ordered]@{
        SchemaVersion = 1
        Succeeded = $Succeeded
        ExitCode = if ($TimedOut) { $null } else { $ExitCode }
        TimedOut = $TimedOut
        ProcessStarted = $true
        ExecutionCompleted = $ExecutionCompleted
        WallSeconds = $WallSeconds
        CargoTimings = [pscustomobject]@{ Disposition = 'unchanged-not-copied'; SnapshotPath = $null }
        ComparisonContext = [pscustomobject][ordered]@{
            SchemaVersion = 1
            CompatibilityCompleteness = $true
            CompatibilityFingerprintSha256 = $Fingerprint
            Host = [pscustomobject]@{ MachineNameSha256 = 'host-a'; OSArchitecture = 'X64' }
            Tools = [pscustomobject]@{ Cargo = [pscustomobject]@{ Version = 'cargo 1' }; Rustc = [pscustomobject]@{ Version = 'rustc 1' } }
            Selection = [pscustomobject]@{ Package = 'multi_launcher'; TargetKind = 'lib'; Features = @('default') }
            Profile = 'dev'
            EffectiveTargetDirectory = [pscustomobject]@{ Path = $TargetDirectory; Source = 'default' }
            Environment = [pscustomobject]@{ Values = @([pscustomobject]@{ Name = 'RUSTFLAGS'; State = 'absent'; ValueSha256 = $null }) }
            Configuration = [pscustomobject]@{ Sha256 = 'config-a'; Files = @() }
            Invocation = [pscustomobject]@{ ArgumentsSha256 = 'args-a'; FingerprintFlags = @(); TargetFlags = @() }
            Source = [pscustomobject]@{ Status = 'known'; Sha256 = $SourceHash; Commit = $Commit; BaselineSha = $null; ChangeMapStatus = 'unavailable-no-base'; ChangedFiles = $ChangedFiles }
            Scenario = [pscustomobject]@{ WorkKind = $WorkKind; CacheState = $CacheState; ControlledChangeId = $ControlledChangeId; ControlledChangePaths = $ControlledChangePaths }
        }
    }
}

$root = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $root) { throw "Fixture path already exists; refusing to reuse it: $root" }
[IO.Directory]::CreateDirectory($root) | Out-Null
try {
    $firstPath = Join-Path $root 'first.json'
    $secondPath = Join-Path $root 'second.json'
    $thirdPath = Join-Path $root 'third.json'
    $first = New-ComparisonFixture -WallSeconds 10
    $second = New-ComparisonFixture -WallSeconds 8
    Set-Content -LiteralPath $firstPath -Value (ConvertTo-Json -InputObject $first -Depth 12) -Encoding utf8
    Set-Content -LiteralPath $secondPath -Value (ConvertTo-Json -InputObject $second -Depth 12) -Encoding utf8
    $pair = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $secondPath)
    Assert-ComparisonEqual $pair.Status 'COMPARABLE' 'Compatible pair classification'
    Assert-ComparisonEqual $pair.LastMinusFirstSeconds -2 'Whole-command wall delta'
    Assert-ComparisonEqual $pair.LastMinusFirstPercent -20 'Whole-command wall percentage'
    Assert-ComparisonEqual $pair.ImprovementClaim $false 'No unsupported improvement claim'
    Assert-ComparisonEqual $pair.Samples[0].TimingFreshness 'unchanged-not-copied' 'Stale timing remains explicit'

    $thirdEnvironment = New-ComparisonFixture -Fingerprint ('c' * 64)
    $thirdEnvironment.ComparisonContext.Environment.Values = @(
        [pscustomobject]@{ Name = 'CARGO_BUILD_RUSTFLAGS'; State = 'set'; ValueSha256 = 'third-sample-rustflags-hash' }
    )
    $thirdEnvironmentPath = Join-Path $root 'third-environment.json'
    Set-Content -LiteralPath $thirdEnvironmentPath -Value (ConvertTo-Json -InputObject $thirdEnvironment -Depth 12) -Encoding utf8
    $threeWayMismatch = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $secondPath, $thirdEnvironmentPath)
    Assert-ComparisonEqual $threeWayMismatch.Status 'INCOMPATIBLE_OR_INCOMPLETE' 'Third sample configuration mismatch'
    $thirdDetail = @($threeWayMismatch.ConfigurationDifferenceDetails | Where-Object { $_.Property -ceq 'Environment' -and $_.SampleIndex -eq 3 })
    if ($thirdDetail.Count -ne 1 -or $thirdDetail[0].SamplePath -cne [IO.Path]::GetFullPath($thirdEnvironmentPath) -or
        $thirdDetail[0].ComparedToSamplePath -cne [IO.Path]::GetFullPath($firstPath) -or
        !(ConvertTo-Json -InputObject $thirdDetail[0].To -Depth 8 -Compress).Contains('CARGO_BUILD_RUSTFLAGS')) {
        throw 'Difference details did not identify the third sample and its inherited Cargo flag mismatch.'
    }

    $differentConfig = New-ComparisonFixture -Fingerprint ('c' * 64) -WallSeconds 9
    Set-Content -LiteralPath $thirdPath -Value (ConvertTo-Json -InputObject $differentConfig -Depth 12) -Encoding utf8
    $mismatch = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $thirdPath)
    Assert-ComparisonEqual $mismatch.Status 'INCOMPATIBLE_OR_INCOMPLETE' 'Configuration mismatch classification'
    Assert-ComparisonEqual $mismatch.LastMinusFirstSeconds $null 'Incompatible records have no timing delta'

    $differenceCases = @(
        [pscustomobject]@{ Name = 'host'; Property = 'Host'; Token = 'OSArchitecture'; Mutate = { param($record) $record.ComparisonContext.Host.OSArchitecture = 'ARM64' } }
        [pscustomobject]@{ Name = 'tools'; Property = 'Tools'; Token = 'cargo 2'; Mutate = { param($record) $record.ComparisonContext.Tools.Cargo.Version = 'cargo 2' } }
        [pscustomobject]@{ Name = 'selection'; Property = 'Selection'; Token = 'fixture-feature'; Mutate = { param($record) $record.ComparisonContext.Selection.Features = @('default', 'fixture-feature') } }
        [pscustomobject]@{ Name = 'profile'; Property = 'Profile'; Token = 'release'; Mutate = { param($record) $record.ComparisonContext.Profile = 'release' } }
        [pscustomobject]@{ Name = 'target-directory'; Property = 'EffectiveTargetDirectory'; Token = 'target/fixture'; Mutate = { param($record) $record.ComparisonContext.EffectiveTargetDirectory.Path = 'target/fixture' } }
        [pscustomobject]@{ Name = 'environment'; Property = 'Environment'; Token = 'RUSTFLAGS'; Mutate = { param($record) $record.ComparisonContext.Environment.Values = @([pscustomobject]@{ Name = 'RUSTFLAGS'; State = 'set'; ValueSha256 = 'fixture-rustflags-hash' }) } }
        [pscustomobject]@{ Name = 'configuration'; Property = 'Configuration'; Token = 'Cargo.toml'; Mutate = { param($record) $record.ComparisonContext.Configuration.Sha256 = 'config-b'; $record.ComparisonContext.Configuration.Files = @([pscustomobject]@{ Name = 'Cargo.toml'; State = 'present'; Sha256 = 'fixture-cargo-toml-hash' }) } }
    )
    foreach ($differenceCase in $differenceCases) {
        $leftRecord = New-ComparisonFixture
        $rightRecord = New-ComparisonFixture -Fingerprint ('c' * 64)
        $null = $differenceCase.Mutate.Invoke($rightRecord)
        $leftDifferencePath = Join-Path $root ($differenceCase.Name + '-left.json')
        $rightDifferencePath = Join-Path $root ($differenceCase.Name + '-right.json')
        Set-Content -LiteralPath $leftDifferencePath -Value (ConvertTo-Json -InputObject $leftRecord -Depth 12) -Encoding utf8
        Set-Content -LiteralPath $rightDifferencePath -Value (ConvertTo-Json -InputObject $rightRecord -Depth 12) -Encoding utf8
        $difference = Compare-TrackBMeasurements -MeasurementPath @($leftDifferencePath, $rightDifferencePath)
        Assert-ComparisonEqual $difference.Status 'INCOMPATIBLE_OR_INCOMPLETE' "$($differenceCase.Name) mismatch classification"
        if ($difference.ConfigurationDifferences -cnotcontains $differenceCase.Property) { throw "Configuration difference omitted $($differenceCase.Property)." }
        $detail = @($difference.ConfigurationDifferenceDetails | Where-Object Property -ceq $differenceCase.Property)
        if ($detail.Count -ne 1) { throw "Expected one structured detail for $($differenceCase.Property)." }
        $detailJson = ConvertTo-Json -InputObject $detail[0] -Depth 12 -Compress
        if (!$detailJson.Contains([string]$differenceCase.Token)) { throw "Structured $($differenceCase.Property) detail omitted the changed field '$($differenceCase.Token)'." }
        if ((ConvertTo-Json -InputObject $detail[0].From -Depth 12 -Compress) -ceq (ConvertTo-Json -InputObject $detail[0].To -Depth 12 -Compress)) {
            throw "Structured $($differenceCase.Property) detail did not preserve before/after values."
        }
    }

    $missingPath = Join-Path $root 'missing-measurement.json'
    $missing = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $missingPath)
    Assert-ComparisonEqual $missing.Samples[1].Disposition 'missing' 'Missing measurement classification'
    Assert-ComparisonEqual $missing.Status 'INCOMPATIBLE_OR_INCOMPLETE' 'Missing record prevents comparison'

    $unavailableRecord = New-ComparisonFixture
    $unavailableRecord.PSObject.Properties.Remove('CargoTimings')
    $unavailablePath = Join-Path $root 'unavailable-timing.json'
    Set-Content -LiteralPath $unavailablePath -Value (ConvertTo-Json -InputObject $unavailableRecord -Depth 12) -Encoding utf8
    $unavailableTiming = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $unavailablePath)
    Assert-ComparisonEqual $unavailableTiming.Samples[1].TimingFreshness 'unavailable' 'Missing timing receipt label'
    $freshRecord = New-ComparisonFixture
    $freshRecord.CargoTimings.Disposition = 'captured-new-or-updated'
    $freshPath = Join-Path $root 'fresh-timing.json'
    Set-Content -LiteralPath $freshPath -Value (ConvertTo-Json -InputObject $freshRecord -Depth 12) -Encoding utf8
    $freshTiming = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $freshPath)
    Assert-ComparisonEqual $freshTiming.Samples[1].TimingFreshness 'captured-new-or-updated' 'Fresh timing receipt label'

    $failedPath = Join-Path $root 'failed.json'
    Set-Content -LiteralPath $failedPath -Value (ConvertTo-Json -InputObject (New-ComparisonFixture -Succeeded $false -ExitCode 7) -Depth 12) -Encoding utf8
    $failedPair = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $failedPath)
    Assert-ComparisonEqual $failedPair.Samples[1].Disposition 'failed' 'Failed sample is classified'
    Assert-ComparisonEqual $failedPair.LastMinusFirstSeconds $null 'Failed sample does not produce a delta'

    $timeoutPath = Join-Path $root 'timeout.json'
    Set-Content -LiteralPath $timeoutPath -Value (ConvertTo-Json -InputObject (New-ComparisonFixture -Succeeded $false -TimedOut $true -ExecutionCompleted $false) -Depth 12) -Encoding utf8
    $timeout = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $timeoutPath)
    Assert-ComparisonEqual $timeout.Samples[1].Disposition 'interrupted-timeout' 'Timeout disposition'

    $legacyPath = Join-Path $root 'legacy.json'
    Set-Content -LiteralPath $legacyPath -Value '{"Succeeded":true,"ExitCode":0,"WallSeconds":1}' -Encoding utf8
    $legacy = Compare-TrackBMeasurements -MeasurementPath @($firstPath, $legacyPath)
    Assert-ComparisonEqual $legacy.Samples[1].Disposition 'legacy-unknown' 'Old record limitation'

    $sourceA = New-ComparisonFixture -SourceHash ('d' * 64) -WorkKind 'no-op' -ControlledChangeId 'change-1' `
        -ControlledChangePaths @('src/a.rs') -ChangedFiles @(
            [pscustomobject]@{ Path = 'src/a.rs'; State = 'present'; Sha256 = 'before' },
            [pscustomobject]@{ Path = 'src/shared-dirty.rs'; State = 'present'; Sha256 = 'stable-unrelated' }
        )
    $sourceB = New-ComparisonFixture -SourceHash ('e' * 64) -WorkKind 'edited-source' -ControlledChangeId 'change-1' `
        -ControlledChangePaths @('src/a.rs') -ChangedFiles @(
            [pscustomobject]@{ Path = 'src/a.rs'; State = 'present'; Sha256 = 'after' },
            [pscustomobject]@{ Path = 'src/shared-dirty.rs'; State = 'present'; Sha256 = 'stable-unrelated' }
        ) -WallSeconds 11
    $sourceA.ComparisonContext.Source.Commit = 'same-commit'
    $sourceB.ComparisonContext.Source.Commit = 'same-commit'
    $sourcePairA = Join-Path $root 'controlled-a.json'
    $sourcePairB = Join-Path $root 'controlled-b.json'
    Set-Content -LiteralPath $sourcePairA -Value (ConvertTo-Json -InputObject $sourceA -Depth 12) -Encoding utf8
    Set-Content -LiteralPath $sourcePairB -Value (ConvertTo-Json -InputObject $sourceB -Depth 12) -Encoding utf8
    $controlled = Compare-TrackBMeasurements -MeasurementPath @($sourcePairA, $sourcePairB)
    Assert-ComparisonEqual $controlled.Status 'COMPARABLE' 'Controlled source pair'

    $unrelated = New-ComparisonFixture -SourceHash ('f' * 64) -WorkKind 'edited-source' -ControlledChangeId 'change-1' `
        -ControlledChangePaths @('src/a.rs') -ChangedFiles @(
            [pscustomobject]@{ Path = 'src/a.rs'; State = 'present'; Sha256 = 'after' },
            [pscustomobject]@{ Path = 'src/shared-dirty.rs'; State = 'present'; Sha256 = 'changed-unrelated' }
        ) -WallSeconds 9
    $unrelated.ComparisonContext.Source.Commit = 'same-commit'
    $unrelatedPath = Join-Path $root 'unrelated.json'
    Set-Content -LiteralPath $unrelatedPath -Value (ConvertTo-Json -InputObject $unrelated -Depth 12) -Encoding utf8
    $unrelatedPair = Compare-TrackBMeasurements -MeasurementPath @($sourcePairA, $unrelatedPath)
    Assert-ComparisonEqual $unrelatedPair.Status 'INCOMPATIBLE_OR_INCOMPLETE' 'Undeclared source difference rejection'

    $threeA = Join-Path $root 'noise-a.json'
    $threeB = Join-Path $root 'noise-b.json'
    $threeC = Join-Path $root 'noise-c.json'
    foreach ($entry in @(@($threeA, 10), @($threeB, 10.1), @($threeC, 20))) {
        Set-Content -LiteralPath $entry[0] -Value (ConvertTo-Json -InputObject (New-ComparisonFixture -WallSeconds $entry[1]) -Depth 12) -Encoding utf8
    }
    $noise = Compare-TrackBMeasurements -MeasurementPath @($threeA, $threeB, $threeC)
    Assert-ComparisonEqual $noise.Variability.Noisy $true 'Three-sample noisy classification'
    Assert-ComparisonEqual $noise.ImprovementClaim $false 'Noisy group does not claim improvement'

    $secretName = 'TRACK_B_SECRET_FIXTURE_' + [guid]::NewGuid().ToString('N')
    $safeEnvironment = Get-TrackBEnvironmentSnapshot -Overrides @{ RUSTFLAGS = ''; $secretName = 'private fixture value' }
    Assert-ComparisonEqual (@($safeEnvironment.Values | Where-Object Name -ceq 'RUSTFLAGS')[0].State) 'empty' 'Empty environment override state'
    $secretJson = ConvertTo-Json -InputObject $safeEnvironment -Depth 8
    if ($secretJson.Contains('private fixture value') -or !$safeEnvironment.UnknownOverrides.Contains($secretName)) { throw 'Unknown environment override leaked or was treated as complete.' }

    $cargoEnvironmentNames = @(
        'CARGO_BUILD_RUSTFLAGS',
        'CARGO_TARGET_FIXTURE_TRIPLE_RUSTFLAGS',
        'CARGO_TARGET_FIXTURE_TRIPLE_LINKER'
    )
    $cargoEnvironmentBefore = @{}
    foreach ($name in $cargoEnvironmentNames) {
        $variables = [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process)
        $cargoEnvironmentBefore[$name] = [pscustomobject]@{ Present = $variables.Contains($name); Value = [Environment]::GetEnvironmentVariable($name, 'Process') }
    }
    try {
        foreach ($name in $cargoEnvironmentNames) {
            if ([Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($name)) {
                Remove-Item -LiteralPath ('Env:{0}' -f $name) -ErrorAction Stop
            }
            if ([Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($name)) {
                throw "Could not establish an absent fixture state for $name."
            }
        }
        $absentCargoEnvironment = Get-TrackBEnvironmentSnapshot
        [Environment]::SetEnvironmentVariable('CARGO_BUILD_RUSTFLAGS', '', 'Process')
        $emptyCargoEnvironment = Get-TrackBEnvironmentSnapshot
        Assert-ComparisonEqual (@($emptyCargoEnvironment.Values | Where-Object Name -ceq 'CARGO_BUILD_RUSTFLAGS')[0].State) 'empty' 'Inherited empty Cargo build flags state'
        Assert-ComparisonEqual (@($absentCargoEnvironment.Values | Where-Object Name -ceq 'CARGO_BUILD_RUSTFLAGS')[0].State) 'absent' 'Inherited absent Cargo build flags state'
        if ($absentCargoEnvironment.Sha256 -ceq $emptyCargoEnvironment.Sha256) { throw 'Absent and empty inherited Cargo values shared a fingerprint.' }

        [Environment]::SetEnvironmentVariable('CARGO_BUILD_RUSTFLAGS', '--cfg fixture-private-value-one', 'Process')
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_FIXTURE_TRIPLE_RUSTFLAGS', '-C link-arg=fixture-private-value-two', 'Process')
        [Environment]::SetEnvironmentVariable('CARGO_TARGET_FIXTURE_TRIPLE_LINKER', 'C:\fixture-private\linker.exe', 'Process')
        $firstInheritedCargoEnvironment = Get-TrackBEnvironmentSnapshot
        $firstInheritedJson = ConvertTo-Json -InputObject $firstInheritedCargoEnvironment -Depth 8 -Compress
        foreach ($name in @('CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_FIXTURE_TRIPLE_RUSTFLAGS', 'CARGO_TARGET_FIXTURE_TRIPLE_LINKER')) {
            $captured = @($firstInheritedCargoEnvironment.Values | Where-Object Name -ceq $name)
            if ($captured.Count -ne 1 -or $captured[0].State -ne 'set' -or [string]::IsNullOrWhiteSpace([string]$captured[0].ValueSha256)) {
                throw "Inherited Cargo provenance omitted the hashed state for $name."
            }
        }
        if ($firstInheritedJson.Contains('fixture-private-value-one') -or $firstInheritedJson.Contains('fixture-private-value-two') -or $firstInheritedJson.Contains('C:\fixture-private\linker.exe')) {
            throw 'Inherited Cargo setting values leaked into the retained environment snapshot.'
        }
        [Environment]::SetEnvironmentVariable('CARGO_BUILD_RUSTFLAGS', '--cfg fixture-private-value-changed', 'Process')
        $changedInheritedCargoEnvironment = Get-TrackBEnvironmentSnapshot
        if ($firstInheritedCargoEnvironment.Sha256 -ceq $changedInheritedCargoEnvironment.Sha256) { throw 'Changing inherited Cargo flags did not change the environment fingerprint.' }
    }
    finally {
        foreach ($name in $cargoEnvironmentNames) {
            $previous = $cargoEnvironmentBefore[$name]
            if ($previous.Present) { [Environment]::SetEnvironmentVariable($name, [string]$previous.Value, 'Process') }
            elseif ([Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]::Process).Contains($name)) {
                Remove-Item -LiteralPath ('Env:{0}' -f $name) -ErrorAction Stop
            }
        }
    }
    $emptyConfig = Join-Path $root 'empty-config'
    [IO.Directory]::CreateDirectory($emptyConfig) | Out-Null
    $configA = Get-TrackBConfigurationSnapshot -SourceRoot $emptyConfig -WorkingDirectory $emptyConfig -ArgumentList @()
    $configB = Get-TrackBConfigurationSnapshot -SourceRoot $emptyConfig -WorkingDirectory $emptyConfig -ArgumentList @()
    Assert-ComparisonEqual $configA.Sha256 $configB.Sha256 'Stable configuration fingerprint'

    $selectionA = @{ Mode = 'quick-check'; Features = @{ DefaultFeatures = $true; Explicit = @('fixture') }; TargetKind = 'lib'; Package = 'multi_launcher' }
    $selectionB = @{ Package = 'multi_launcher'; TargetKind = 'lib'; Features = @{ Explicit = @('fixture'); DefaultFeatures = $true }; Mode = 'quick-check' }
    $selectionJsonA = ConvertTo-Json -InputObject (ConvertTo-TrackBCanonicalValue -Value $selectionA) -Depth 10 -Compress
    $selectionJsonB = ConvertTo-Json -InputObject (ConvertTo-TrackBCanonicalValue -Value $selectionB) -Depth 10 -Compress
    Assert-ComparisonEqual (Get-TrackBTextSha256 -Text $selectionJsonA) (Get-TrackBTextSha256 -Text $selectionJsonB) 'Stable selection fingerprint across hashtable order'

    $probeExecutable = Join-Path $root 'version-probe-fixture.exe'
    [IO.File]::WriteAllText($probeExecutable, 'controlled probe fixture', [Text.UTF8Encoding]::new($false))
    $probeCalls = [System.Collections.Generic.List[object]]::new()
    $captureProbe = {
        param($path, $arguments, $timeoutSeconds)
        $probeCalls.Add([pscustomobject]@{ Path = $path; ArgumentType = $arguments.GetType().FullName; Arguments = @($arguments); TimeoutSeconds = $timeoutSeconds })
        return [pscustomobject]@{ ExitCode = 0; Stdout = 'rustc 1.99.1 (controlled fixture)' }
    }
    $toolIdentity = Get-TrackBToolIdentity -Path $probeExecutable -VersionArguments @('--version', '--verbose') -ProcessProbe $captureProbe
    Assert-ComparisonEqual $toolIdentity.Status 'available' 'Multi-argument compiler probe identity'
    Assert-ComparisonEqual $probeCalls.Count 1 'Compiler probe invocation count'
    Assert-ComparisonEqual $probeCalls[0].ArgumentType 'System.String[]' 'Compiler probe preserves flat argument array'
    Assert-ComparisonEqual $probeCalls[0].Arguments.Count 2 'Compiler probe argument count'
    Assert-ComparisonEqual $probeCalls[0].Arguments[0] '--version' 'Compiler probe first argument'
    Assert-ComparisonEqual $probeCalls[0].Arguments[1] '--verbose' 'Compiler probe second argument'
    Assert-ComparisonEqual $probeCalls[0].TimeoutSeconds 10 'Compiler probe remains bounded'

    $retention = Get-TrackBEvidenceRetentionReport -OwnedEvidenceRoot $root -MaximumFiles 2 -MaximumBytes 1
    Assert-ComparisonEqual $retention.LimitExceeded $true 'Retention quota is reported'
    Assert-ComparisonEqual $retention.Action 'report-only; no files are deleted or rewritten' 'Retention never deletes evidence'
    Assert-ComparisonEqual (Test-Path -LiteralPath $firstPath -PathType Leaf) $true 'First sample retained'
    Assert-ComparisonEqual (Test-Path -LiteralPath $failedPath -PathType Leaf) $true 'Failure sample retained'

    $entryLimitRoot = Join-Path $root 'entry-limit-fixture'
    [IO.Directory]::CreateDirectory($entryLimitRoot) | Out-Null
    for ($index = 0; $index -lt 20; $index++) { [IO.Directory]::CreateDirectory((Join-Path $entryLimitRoot ('directory-{0:D2}' -f $index))) | Out-Null }
    $entryLimited = Get-TrackBEvidenceRetentionReport -OwnedEvidenceRoot $entryLimitRoot -MaximumFiles 10 -MaximumEntries 3 -MaximumBytes 1000
    Assert-ComparisonEqual $entryLimited.Truncated $true 'Retention scan reports entry-bound truncation'
    Assert-ComparisonEqual $entryLimited.TruncationReason 'entry-limit' 'Retention scan identifies its bound'
    Assert-ComparisonEqual $entryLimited.EntryCount 4 'Retention scan stops after one over-limit observation'
    Assert-ComparisonEqual $entryLimited.CensusComplete $false 'Truncated scan does not claim a complete census'

    [pscustomobject]@{ Result = 'PASS'; EvidenceDirectory = $root; Cases = @('compatible timing delta', 'field-level host/tool/selection/profile/target/environment/configuration differences across every sample', 'missing record', 'unavailable/stale/fresh timing labels', 'failed and timed-out exclusion', 'legacy unknown', 'controlled source path delta with shared unrelated dirty files', 'undeclared source delta rejection', 'noisy group', 'secret-safe inherited Cargo environment identity and absent/empty state', 'hashed configuration', 'bounded report-only retention') }
}
finally {
    if (Test-Path -LiteralPath $root -PathType Container) {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]]@('\', '/')) + [IO.Path]::DirectorySeparatorChar
        if ($root.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { [IO.Directory]::Delete($root, $true) }
    }
}
