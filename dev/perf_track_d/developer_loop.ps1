#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('check', 'test-build', 'app-build')][string]$Family,
    [string]$Profile = 'dev',
    [switch]$SkipPreparation
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$evidence = Join-Path $env:TEMP 'Multi_Launcher/perf-track-d'
$measure = Join-Path $root 'dev/track_b/measure.ps1'
$baseline = 'cbe104abc10ff03af2fd9897e88a4a331a7f4197'
$filter = 'test(=history::tests::pin_roundtrip_and_toggle)'
$command = switch ($Family) {
    'check' { @('check', '--lib', '--offline', '--timings', '--profile', $Profile) }
    'test-build' { @('nextest', 'run', '--lib', '--offline', '-E', $filter, '--no-run', '--cargo-profile', $Profile) }
    'app-build' { @('build', '--bin', 'multi_launcher', '--offline', '--timings', '--profile', $Profile) }
}
function Invoke-Sample([string]$Phase, [int]$Index, [string]$Cache) {
    $record = & $measure -Executable cargo.exe -ArgumentList $command `
        -WorkingDirectory $root -ScenarioId "d1-$Family-$Profile-$Phase-$Index" `
        -Profile $Profile -CacheState $Cache -BaselineSha $baseline `
        -EvidenceDirectory $evidence -FixturePaths @('src/history.rs', 'src/gui/mod.rs')
    $record | Select-Object ScenarioId, WallSeconds, ExitCode, Succeeded | Format-Table | Out-Host
    if (!$record.Succeeded) { throw "Measurement failed; inspect $($record.EvidenceDirectory)" }
}
if (!$SkipPreparation) { Invoke-Sample 'preparation' 0 'unknown' }
foreach ($i in 1..3) { Invoke-Sample 'noop' $i 'warmed' }
# One leaf edit and one large-owner edit, each exactly restored even on failure.
$paths = @('src/history.rs', 'src/gui/mod.rs')
foreach ($i in 0..1) {
    $path = Join-Path $root $paths[$i]
    $before = [IO.File]::ReadAllBytes($path)
    $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
    try {
        $comment = [Text.Encoding]::UTF8.GetBytes("`n// Track D controlled invalidation sample $i.`n")
        [IO.File]::WriteAllBytes($path, [byte[]]($before + $comment))
        Invoke-Sample ('edit-' + ($i + 1)) 1 'warmed'
    }
    finally {
        [IO.File]::WriteAllBytes($path, $before)
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $hash) {
            throw "Failed to restore edit fixture $($paths[$i])"
        }
    }
}
if ($Family -eq 'test-build') {
    $execution = & $measure -Executable cargo.exe `
        -ArgumentList @('nextest', 'run', '--lib', '--offline', '-E', $filter, '--cargo-profile', $Profile) `
        -WorkingDirectory $root -ScenarioId "d1-test-execution-$Profile" -Profile $Profile `
        -CacheState warmed -BaselineSha $baseline -EvidenceDirectory $evidence
    $execution | Select-Object ScenarioId, WallSeconds, ExitCode, Succeeded | Format-Table | Out-Host
    if (!$execution.Succeeded) { throw "Selected test failed: $($execution.EvidenceDirectory)" }
}
