#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputPath,
    [Parameter(Mandatory)][DateTimeOffset]$StartedAfterUtc,
    [Parameter(Mandatory)][DateTimeOffset]$EndedBeforeUtc
)
$ErrorActionPreference = 'Stop'
if ($EndedBeforeUtc -le $StartedAfterUtc) { throw 'Measurement window must have positive duration.' }
$evidence = Join-Path $env:TEMP 'Multi_Launcher/perf-track-d'
$rows = @(Get-ChildItem -LiteralPath $evidence -Filter measurement.json -Recurse |
    ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json } |
    Where-Object { $_.ScenarioId -like 'd1-*' -and
        [DateTimeOffset]$_.StartedUtc -ge $StartedAfterUtc -and
        [DateTimeOffset]$_.EndedUtc -le $EndedBeforeUtc } | Sort-Object StartedUtc)
$samples = @($rows | ForEach-Object {
    [ordered]@{
        scenario = $_.ScenarioId
        source_sha = $_.SourceSha
        command = @('cargo') + $_.ArgumentList
        profile = $_.Profile
        cache = $_.CacheState
        started_utc = $_.StartedUtc
        wall_seconds = $_.WallSeconds
        exit_code = $_.ExitCode
        success = $_.Succeeded
        fixture_hashes = @($_.Fixtures | ForEach-Object { [ordered]@{ path = $_.SuppliedPath; sha256 = $_.Sha256 } })
    }
})
$metrics = @($rows | Where-Object { $_.Succeeded -and $_.ScenarioId -match '-noop-' } |
    Group-Object { ($_.ScenarioId -replace '-noop-\d+$', '') + ':' + $_.SourceSha + ':' + ($_.ArgumentList -join '|') } | ForEach-Object {
        $values = @($_.Group.WallSeconds | Sort-Object)
        [ordered]@{
            scenario = $_.Name
            count = $values.Count
            median_seconds = $values[[int][Math]::Floor($values.Count / 2)]
            min_seconds = $values[0]
            max_seconds = $values[-1]
        }
    })
[ordered]@{
    version = 1
    track = 'D1'
    measurement_window = @{ start_utc = $StartedAfterUtc.ToString('o'); end_utc = $EndedBeforeUtc.ToString('o') }
    scenario = 'selected developer commands'
    baseline_sha = 'cbe104abc10ff03af2fd9897e88a4a331a7f4197'
    candidate_sha_or_diff = 'no production changes; controlled comments described in developer_loop.ps1'
    profile = 'per sample'
    settings_hash = $null
    host = @{ os = [Environment]::OSVersion.VersionString; logical_processors = [Environment]::ProcessorCount; arch = 'x86_64-pc-windows-msvc'; rust = '1.99.0'; nextest = '0.9.135'; ram = $null; power = $null }
    samples = $samples
    metrics = $metrics
    limitations = @('Process wall time is not isolated linking or test execution.', 'CIM inventory denied; RAM and power unavailable.', 'Three no-op observations do not estimate population p95.', 'All original dev baseline samples predate per-file hash capture; exact restored comment recipe is recorded.')
    decision = 'BASELINE_ONLY'
} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $OutputPath
