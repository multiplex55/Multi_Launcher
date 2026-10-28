<#
.SYNOPSIS
Runs one of the repository's fixed Cargo workflows from the repository root.

.DESCRIPTION
The wrapper fixes the manifest and selects the launcher or one test target.
Forward additional Cargo/Nextest options with the explicit -CargoArguments
string array. Conflicting manifest, package, target, workspace, and Cargo compiler-profile selectors are rejected. Use Cargo directly for other targets or profiles.

.PARAMETER Preset
One of the legacy release, debug, lib-test, or integration-test presets, or a
fixed workflow build/test phase. Build presets select multi_launcher; test
presets select the library or one integration-test target.

.PARAMETER TestTarget
Cargo integration-test target name. Required only for integration-test and
rejected for the other presets.

.PARAMETER Packages
Catalog-owned Cargo packages to prepare for a fast-dev-tests target.
If omitted, the root package remains the default; inventory and run phases use
the prepared binary metadata and do not accept package selectors.

.PARAMETER CargoArguments
Additional options passed as individual native process arguments. Pass one
string array, for example @('-E', 'test(history_prepare_)').

.PARAMETER PrintCommand
Print the executable, argument array, and working directory as JSON without
starting Cargo.

.PARAMETER LinkerMode
Opt in to the active Rust toolchain's bundled Windows LLD linker for the
fast-dev-build preset or fast-dev-tests prepare phase only.

.EXAMPLE
& .\dev\track_b\cargo.ps1 -Preset release -PrintCommand

.EXAMPLE
& .\dev\track_b\cargo.ps1 -Preset lib-test -CargoArguments @('-E', 'test(history_prepare_)')

.EXAMPLE
& .\dev\track_b\cargo.ps1 -Preset integration-test -TestTarget history
#>
#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('release', 'debug', 'lib-test', 'integration-test', 'quick-check', 'full-debug', 'fast-dev-build', 'fast-dev-tests', 'iteration', 'production')]
    [string]$Preset,

    [string]$TestTarget,

    [ValidateSet('prepare', 'inventory', 'run')]
    [string]$WorkflowPhase,

    [ValidateSet('lib', 'integration')]
    [string]$TargetKind,

    [string]$FilterExpression,

    [string]$BinariesMetadataPath,

    [string]$ReplayRecordingPath,

    [string[]]$Packages,

    [ValidatePattern('^[A-Za-z0-9_][A-Za-z0-9_.-]*$')]
    [string]$RunnerProfile,

    [string[]]$TestArguments = @(),

    [string[]]$CargoArguments = @(),

    [ValidateSet('bundled-lld')]
    [string]$LinkerMode,

    [switch]$PrintCommand
)

$ErrorActionPreference = 'Stop'

function Assert-NoConflictingCargoArguments {
    param([string[]]$Values, [string]$SelectedPreset)

    $longSelectors = @(
        '--manifest-path', '--package', '--workspace', '--exclude',
        '--bin', '--bins', '--example', '--examples', '--test', '--tests',
        '--bench', '--benches', '--lib', '--all-targets', '--all',
        '--target', '--target-dir', '--cargo-profile', '--release'
    )
    $buildPreset = $SelectedPreset -in @('release', 'debug', 'quick-check', 'full-debug', 'fast-dev-build', 'iteration', 'production')
    $shortSelectors = @('-p')
    if ($buildPreset) {
        $longSelectors += '--profile'
        $shortSelectors += @('-m', '-b', '-e', '-t')
    }

    foreach ($value in $Values) {
        if ($null -eq $value) { throw 'CargoArguments cannot contain null elements.' }
        if ($value -ceq '--') { break }

        foreach ($selector in $longSelectors) {
            if ($value -ceq $selector -or $value.StartsWith(($selector + '='), [StringComparison]::Ordinal)) {
                throw "CargoArguments cannot override fixed selector $selector. Use Cargo directly for another target or profile."
            }
        }

        foreach ($selector in $shortSelectors) {
            if ($value -ceq $selector -or $value.StartsWith($selector, [StringComparison]::Ordinal)) {
                throw "CargoArguments cannot override fixed selector $selector. Use Cargo directly for another target or package."
            }
        }

        if ($buildPreset -and $value -cmatch '^-(?:q|v)*(?:m|p|b|e|t)(?:=.*|.+)?$') {
            throw 'CargoArguments cannot override a fixed package, manifest, or target selector, including short-option clusters. Use Cargo directly for another target.'
        }
        if (!$buildPreset -and $value -cmatch '^-(?:q|v)*p(?:=.*|.+)?$') {
            throw 'CargoArguments cannot override the fixed package selector, including short-option clusters. Use Cargo directly for another package.'
        }
        if ($value -cmatch '^-(?:q|v)*r(?:q|v)*$' -or $value -cmatch '^-(?:q|v)*r(?:=.*|.+)$') {
            throw 'CargoArguments cannot override the fixed Cargo compiler profile with -r/--release. Use Cargo directly for another profile.'
        }
    }
}

if ($Preset -eq 'integration-test') {
    if ([string]::IsNullOrWhiteSpace($TestTarget) -or $TestTarget -notmatch '^[A-Za-z0-9_][A-Za-z0-9_.-]*$') {
        throw 'Integration-test preset requires a Cargo test target name in -TestTarget.'
    }
} elseif ($Preset -eq 'fast-dev-tests') {
    if ([string]::IsNullOrWhiteSpace($WorkflowPhase)) {
        throw 'The fast-dev-tests preset requires -WorkflowPhase prepare, inventory, or run.'
    }
    if ([string]::IsNullOrWhiteSpace($TargetKind)) {
        throw 'The fast-dev-tests preset requires -TargetKind lib or integration.'
    }
    if ($TargetKind -eq 'integration' -and ($TestTarget -notmatch '^[A-Za-z0-9_][A-Za-z0-9_.-]*$')) {
        throw 'The integration target workflow requires a Cargo test target name.'
    }
    if ($TargetKind -eq 'lib' -and ![string]::IsNullOrWhiteSpace($TestTarget)) {
        throw '-TestTarget is valid only for integration target workflows.'
    }
    if ($null -eq $Packages) { $Packages = @() }
    foreach ($package in $Packages) {
        if ([string]::IsNullOrWhiteSpace($package) -or $package -notmatch '^[A-Za-z0-9][A-Za-z0-9_-]*$') {
            throw 'Packages must contain valid Cargo package names.'
        }
        if ($package -cnotin @('multi_launcher', 'multi_launcher_table_core')) {
            throw "Package '$package' is not owned by the fast-dev-tests catalog workflow."
        }
    }
    if (@($Packages | Select-Object -Unique).Count -ne $Packages.Count) {
        throw 'Packages cannot contain duplicate names.'
    }
    if ($WorkflowPhase -ne 'prepare' -and $Packages.Count -gt 0) {
        throw '-Packages is valid only for the fast-dev-tests prepare phase.'
    }
    if ($TargetKind -eq 'integration' -and $Packages.Count -gt 0 -and
        ($Packages.Count -ne 1 -or $Packages[0] -cne 'multi_launcher')) {
        throw 'Integration test preparation is fixed to the multi_launcher package.'
    }
    if ($WorkflowPhase -eq 'prepare' -and (! [string]::IsNullOrWhiteSpace($FilterExpression) -or ! [string]::IsNullOrWhiteSpace($BinariesMetadataPath) -or $TestArguments.Count -gt 0)) {
        throw 'The prepare phase does not accept a filter, binaries metadata, or test arguments.'
    }
    if ($WorkflowPhase -eq 'inventory' -and ([string]::IsNullOrWhiteSpace($BinariesMetadataPath) -or $TestArguments.Count -gt 0)) {
        throw 'The inventory phase requires -BinariesMetadataPath and does not accept test arguments.'
    }
    if ($WorkflowPhase -eq 'run' -and [string]::IsNullOrWhiteSpace($BinariesMetadataPath)) {
        throw 'The run phase requires -BinariesMetadataPath.'
    }
    if ($WorkflowPhase -eq 'run' -and [string]::IsNullOrWhiteSpace($FilterExpression)) {
        throw 'The run phase requires an exact-selection -FilterExpression.'
    }
    if ($WorkflowPhase -ne 'run' -and ![string]::IsNullOrWhiteSpace($RunnerProfile)) {
        throw '-RunnerProfile is valid only for the run phase.'
    }
    if ($null -eq $TestArguments) { $TestArguments = @() }
    foreach ($argument in $TestArguments) {
        if ($null -eq $argument) { throw 'TestArguments cannot contain null elements.' }
    }
} elseif (![string]::IsNullOrWhiteSpace($TestTarget) -or ![string]::IsNullOrWhiteSpace($WorkflowPhase) -or
    ![string]::IsNullOrWhiteSpace($TargetKind) -or ![string]::IsNullOrWhiteSpace($FilterExpression) -or
    ![string]::IsNullOrWhiteSpace($BinariesMetadataPath) -or ![string]::IsNullOrWhiteSpace($RunnerProfile) -or
    $TestArguments.Count -gt 0 -or ($null -ne $Packages -and $Packages.Count -gt 0)) {
    throw '-TestTarget is valid only with the integration-test preset.'
}

if ($null -eq $CargoArguments) { $CargoArguments = @() }
if ($Preset -eq 'fast-dev-tests') {
    if (@($CargoArguments).Count -gt 0) { throw 'fast-dev-tests uses fixed workflow arguments; pass only test-binary options through -TestArguments.' }
} else {
    if (@($TestArguments).Count -gt 0) { throw '-TestArguments is valid only with the fast-dev-tests preset.' }
    if (![string]::IsNullOrWhiteSpace($WorkflowPhase) -or ![string]::IsNullOrWhiteSpace($TargetKind) -or
        ![string]::IsNullOrWhiteSpace($FilterExpression) -or ![string]::IsNullOrWhiteSpace($BinariesMetadataPath) -or
        ![string]::IsNullOrWhiteSpace($RunnerProfile)) { throw 'Workflow phase arguments are valid only with fast-dev-tests.' }
    Assert-NoConflictingCargoArguments -Values $CargoArguments -SelectedPreset $Preset
}

$useBundledLld = $LinkerMode -ieq 'bundled-lld'
if ($useBundledLld) {
    $linkerModeSupported = $Preset -ceq 'fast-dev-build' -or
        ($Preset -ceq 'fast-dev-tests' -and $WorkflowPhase -ceq 'prepare')
    if (!$linkerModeSupported) {
        throw '-LinkerMode bundled-lld is valid only for fast-dev-build or fast-dev-tests -WorkflowPhase prepare.'
    }
    if ($Preset -eq 'fast-dev-build' -and @($CargoArguments | Where-Object {
        $_ -ceq '--config' -or $_.StartsWith('--config=', [StringComparison]::Ordinal)
    }).Count -gt 0) {
        throw 'The bundled LLD mode owns Cargo --config arguments; conflicting CargoArguments are not allowed.'
    }
}

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..') -ErrorAction Stop).ProviderPath
$manifestPath = (Resolve-Path -LiteralPath (Join-Path $repositoryRoot 'Cargo.toml') -ErrorAction Stop).ProviderPath
$cargoApplication = Get-Command -Name 'cargo.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
if ($null -eq $cargoApplication) { throw 'cargo.exe was not found as a native application on PATH.' }
$cargoPath = [IO.Path]::GetFullPath($(if ($cargoApplication.Source) { $cargoApplication.Source } else { $cargoApplication.Path }))

$linkerModeArguments = @()
if ($useBundledLld) {
    . (Join-Path $PSScriptRoot 'linker_mode.ps1')
    Assert-NoConflictingBundledLldConfiguration -RepositoryRoot $repositoryRoot -CargoExecutable $cargoPath
    $bundledLinkerPath = Resolve-ActiveSysrootBundledLld -RepositoryRoot $repositoryRoot
    $linkerModeArguments = @(New-BundledLldCargoArguments -Preset $Preset -WorkflowPhase $WorkflowPhase -LinkerPath $bundledLinkerPath)
}

$selectedArguments = switch ($Preset) {
    'release' { @('build', '--release', '--bin', 'multi_launcher') }
    'debug' { @('build', '--bin', 'multi_launcher') }
    'quick-check' { @('check', '--lib') }
    'full-debug' { @('build', '--bin', 'multi_launcher') }
    'fast-dev-build' { @('build', '--profile', 'fast-dev', '--bin', 'multi_launcher') }
    'iteration' { @('build', '--profile', 'iteration', '--bin', 'multi_launcher') }
    'production' { @('build', '--release', '--bin', 'multi_launcher') }
    'lib-test' { @('nextest', 'run', '--lib') }
    'integration-test' { @('nextest', 'run', '--test', $TestTarget) }
    'fast-dev-tests' {
        $targetArguments = @()
        if ($WorkflowPhase -eq 'prepare') {
            $preparePackages = if ($Packages.Count -gt 0) { @($Packages) } else { @('multi_launcher') }
            foreach ($package in $preparePackages) { $targetArguments += @('-p', [string]$package) }
            if ($TargetKind -eq 'lib') { $targetArguments += '--lib' }
            else { $targetArguments += @('--test', $TestTarget) }
        }
        else {
            if ($TargetKind -eq 'lib') { $targetArguments = @('--lib') }
            else { $targetArguments = @('--test', $TestTarget) }
        }
        switch ($WorkflowPhase) {
            'prepare' { @('nextest', 'list', '--list-type', 'binaries-only', '--message-format', 'json', '--cargo-profile', 'fast-dev') + $targetArguments }
            'inventory' {
                $inventoryArguments = @('nextest', 'list', '--message-format', 'json', '--binaries-metadata', [IO.Path]::GetFullPath($BinariesMetadataPath))
                if (![string]::IsNullOrWhiteSpace($FilterExpression)) { $inventoryArguments += @('-E', $FilterExpression) }
                $inventoryArguments
            }
            'run' {
                $runArguments = @('nextest', 'run', '--binaries-metadata', [IO.Path]::GetFullPath($BinariesMetadataPath))
                if (![string]::IsNullOrWhiteSpace($ReplayRecordingPath)) { $runArguments += @('-R', [IO.Path]::GetFullPath($ReplayRecordingPath)) }
                $runArguments += @('--message-format', 'libtest-json-plus', '--message-format-version', '0.1', '-E', $FilterExpression)
                if (![string]::IsNullOrWhiteSpace($RunnerProfile)) { $runArguments += @('-P', $RunnerProfile) }
                if ($TestArguments.Count -gt 0) { $runArguments += @('--') + @($TestArguments) }
                $runArguments
            }
        }
    }
}
$argumentList = @($selectedArguments)
if ($useBundledLld) {
    if ($Preset -ceq 'fast-dev-build') {
        # Keep `build` first so the checked MSVC auxiliary-process classifier can
        # recognize the explicit Cargo command. Cargo accepts --config here.
        # Add the fixed application package while preserving the preset selectors.
        $argumentList = @($selectedArguments[0]) + @($linkerModeArguments) + @('-p', 'multi_launcher') + @($selectedArguments[1..($selectedArguments.Count - 1)])
    } else {
        # Nextest is an external Cargo subcommand: Cargo consumes its own
        # pre-subcommand globals before dispatch. Place its Cargo overrides
        # after `nextest list`, where Nextest forwards them to its Cargo build.
        # Insert into the existing selected argv to preserve catalog package/target selectors.
        $argumentList = @($selectedArguments[0..1]) + @($linkerModeArguments) + @($selectedArguments[2..($selectedArguments.Count - 1)])
    }
}
if ($Preset -eq 'fast-dev-tests' -and $WorkflowPhase -eq 'run') {
    # Cargo's manifest selector belongs to Cargo, before Nextest's `--` boundary.
    $separatorIndex = [Array]::IndexOf([string[]]$argumentList, '--')
    if ($separatorIndex -ge 0) {
        $argumentList = @($argumentList[0..($separatorIndex - 1)]) + @('--manifest-path', $manifestPath) + @($argumentList[($separatorIndex)..($argumentList.Count - 1)])
    }
    else { $argumentList += @('--manifest-path', $manifestPath) }
}
else {
    $argumentList += @('--manifest-path', $manifestPath)
}
if ($Preset -ne 'fast-dev-tests') { $argumentList += @($CargoArguments) }

if ($PrintCommand) {
    [pscustomobject]@{
        Preset = $Preset
        Executable = $cargoPath
        ArgumentList = @($argumentList)
        WorkingDirectory = $repositoryRoot
        Started = $false
    } | ConvertTo-Json -Depth 4
    return
}

$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $cargoPath
$startInfo.WorkingDirectory = $repositoryRoot
$startInfo.UseShellExecute = $false
foreach ($argument in $argumentList) {
    if ($null -eq $argument) { throw 'Native argument arrays cannot contain null values.' }
    $startInfo.ArgumentList.Add([string]$argument)
}

$process = [Diagnostics.Process]::new()
try {
    $process.StartInfo = $startInfo
    if (!$process.Start()) { throw "Could not start native executable: $cargoPath" }
    $process.WaitForExit()
    $nativeExitCode = $process.ExitCode
} finally {
    $process.Dispose()
}
exit $nativeExitCode
