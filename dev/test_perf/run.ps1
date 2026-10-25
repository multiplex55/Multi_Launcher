#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Library', 'Integration')][string]$TargetKind,
    [string]$Target,
    [string]$Filter,
    [string]$CargoProfile
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

if ($TargetKind -eq 'Integration' -and [string]::IsNullOrWhiteSpace($Target)) {
    throw 'Integration mode requires -Target <cargo-test-target>.'
}
if ($TargetKind -eq 'Library' -and ![string]::IsNullOrWhiteSpace($Target)) {
    throw 'Library mode does not accept -Target.'
}
if ($CargoProfile -and $CargoProfile -notmatch '^[A-Za-z0-9_-]+$') {
    throw 'CargoProfile may contain only letters, digits, underscore and hyphen.'
}

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifestPath = Join-Path $repoRoot 'Cargo.toml'
$cargo = Get-Command -Name cargo.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
if (!$cargo) { throw 'cargo.exe was not found on PATH.' }
$cargoPath = if ($cargo.Source) { $cargo.Source } else { $cargo.Path }

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\test-perf-run'
[IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
$runDirectory = Join-Path $temporaryRoot ([guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($runDirectory) | Out-Null
$metadataPath = Join-Path $runDirectory 'binaries-metadata.json'

$targetArguments = [System.Collections.Generic.List[string]]::new()
$targetArguments.Add('--manifest-path'); $targetArguments.Add($manifestPath)
$targetArguments.Add('-p'); $targetArguments.Add('multi_launcher')
if ($TargetKind -eq 'Library') { $targetArguments.Add('--lib') }
else { $targetArguments.Add('--test'); $targetArguments.Add($Target) }
if ($CargoProfile) { $targetArguments.Add('--cargo-profile'); $targetArguments.Add($CargoProfile) }

Push-Location $repoRoot
$runExitCode = 0
try {
    $targetDescription = if ($TargetKind -eq 'Library') { 'Target: multi_launcher library (--lib)' } else { "Target: multi_launcher integration test --test $Target" }
    Write-Host $targetDescription
    Write-Host 'Build phase: resolve and build only the selected test target.'
    $buildArguments = @('nextest', 'list', '--list-type', 'binaries-only', '--message-format', 'json') + @($targetArguments)
    & $cargoPath @buildArguments 1> $metadataPath
    $buildExitCode = [int]$LASTEXITCODE
    if ($buildExitCode -ne 0) {
        Write-Warning "Selected target build/metadata phase failed with exit code $buildExitCode."
    }
    else {
        $metadata = Get-Content -LiteralPath $metadataPath -Raw -ErrorAction Stop | ConvertFrom-Json -AsHashtable -ErrorAction Stop
        if ($null -eq (Get-TestPerfField $metadata 'rust-binaries')) {
            throw 'Nextest binary-only listing did not produce reusable rust-binaries metadata.'
        }

        $shouldRun = $true
        if ($Filter) {
            Write-Host 'Selection phase: verify that the exact case filter matches runnable cases.'
            $selectionPath = Join-Path $runDirectory 'selected-tests.json'
        $selectionArguments = @('nextest', 'list', '--message-format', 'json', '--manifest-path', $manifestPath, '--binaries-metadata', $metadataPath, '-E', $Filter)
            & $cargoPath @selectionArguments 1> $selectionPath
            $selectionExitCode = [int]$LASTEXITCODE
            if ($selectionExitCode -ne 0) {
                Write-Warning "Selection listing failed with exit code $selectionExitCode."
                $runExitCode = $selectionExitCode
                $shouldRun = $false
            }
            else {
                $selectedCases = @(Convert-TestPerfInventory -Path $selectionPath)
                $selectedCount = Get-TestPerfSelectedCaseCount -Cases $selectedCases
                if ($selectedCount -eq 0) {
                    Write-Warning 'The selected target/filter has no runnable test cases; execution was not started.'
                    $runExitCode = 4
                    $shouldRun = $false
                }
                else { Write-Host "Selection phase: $selectedCount runnable case(s) matched." }
            }
        }

        if ($shouldRun) {
            Write-Host 'Execution phase: Nextest native progress follows.'
    $runArguments = @('nextest', 'run', '--manifest-path', $manifestPath, '--binaries-metadata', $metadataPath, '--message-format', 'human')
            if ($Filter) { $runArguments += @('-E', $Filter) }
            & $cargoPath @runArguments
            $runExitCode = [int]$LASTEXITCODE
        }
    }
}
catch {
    [Console]::Error.WriteLine(('Test run helper failed: {0}' -f $_.Exception.Message))
    $runExitCode = 1
}
finally {
    Pop-Location
}
$global:LASTEXITCODE = $runExitCode
exit $runExitCode
