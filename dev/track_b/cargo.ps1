<#
.SYNOPSIS
Runs one of the repository's fixed Cargo workflows from the repository root.

.DESCRIPTION
The wrapper fixes the manifest and selects the launcher or one test target.
Forward additional Cargo/Nextest options with the explicit -CargoArguments
string array. Conflicting manifest, package, target, workspace, and Cargo compiler-profile selectors are rejected. Use Cargo directly for other targets or profiles.

.PARAMETER Preset
One of release, debug, lib-test, or integration-test. Build presets select
multi_launcher; test presets select the library or one integration-test target.

.PARAMETER TestTarget
Cargo integration-test target name. Required only for integration-test and
rejected for the other presets.

.PARAMETER CargoArguments
Additional options passed as individual native process arguments. Pass one
string array, for example @('-E', 'test(history_prepare_)').

.PARAMETER PrintCommand
Print the executable, argument array, and working directory as JSON without
starting Cargo.

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
    [ValidateSet('release', 'debug', 'lib-test', 'integration-test')]
    [string]$Preset,

    [string]$TestTarget,

    [string[]]$CargoArguments = @(),

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
    $buildPreset = $SelectedPreset -in @('release', 'debug')
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
} elseif (![string]::IsNullOrWhiteSpace($TestTarget)) {
    throw '-TestTarget is valid only with the integration-test preset.'
}

if ($null -eq $CargoArguments) { $CargoArguments = @() }
Assert-NoConflictingCargoArguments -Values $CargoArguments -SelectedPreset $Preset

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..') -ErrorAction Stop).ProviderPath
$manifestPath = (Resolve-Path -LiteralPath (Join-Path $repositoryRoot 'Cargo.toml') -ErrorAction Stop).ProviderPath
$cargoApplication = Get-Command -Name 'cargo.exe' -CommandType Application -ErrorAction Stop | Select-Object -First 1
if ($null -eq $cargoApplication) { throw 'cargo.exe was not found as a native application on PATH.' }
$cargoPath = [IO.Path]::GetFullPath($(if ($cargoApplication.Source) { $cargoApplication.Source } else { $cargoApplication.Path }))

$selectedArguments = switch ($Preset) {
    'release' { @('build', '--release', '--bin', 'multi_launcher') }
    'debug' { @('build', '--bin', 'multi_launcher') }
    'lib-test' { @('nextest', 'run', '--lib') }
    'integration-test' { @('nextest', 'run', '--test', $TestTarget) }
}
$argumentList = @($selectedArguments) + @('--manifest-path', $manifestPath) + @($CargoArguments)

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