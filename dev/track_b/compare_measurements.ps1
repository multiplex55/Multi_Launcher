#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateCount(2, 50)][string[]]$MeasurementPath,
    [string]$OwnedEvidenceRoot,
    [ValidateRange(1, 100000)][int]$MaximumEvidenceFiles = 5000,
    [ValidateRange(1, 100000)][int]$MaximumEvidenceEntries = 10000,
    [ValidateRange(1, 1099511627776)][long]$MaximumEvidenceBytes = 5368709120,
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'comparison.ps1')

function Test-ComparisonPathWithinRoot {
    param([string]$Candidate, [string]$Root)
    $candidatePath = [IO.Path]::GetFullPath($Candidate)
    $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/'))
    $prefix = $rootPath + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.Equals($rootPath, [StringComparison]::OrdinalIgnoreCase)
}

$comparison = Compare-TrackBMeasurements -MeasurementPath @($MeasurementPath)
if (![string]::IsNullOrWhiteSpace($OwnedEvidenceRoot)) {
    $resolvedRoot = [IO.Path]::GetFullPath($OwnedEvidenceRoot)
    foreach ($path in $MeasurementPath) {
        if (!(Test-ComparisonPathWithinRoot -Candidate $path -Root $resolvedRoot)) {
            throw 'Retention reporting requires every comparison record to be inside the explicitly supplied evidence root.'
        }
    }
    $retention = Get-TrackBEvidenceRetentionReport -OwnedEvidenceRoot $resolvedRoot `
        -MaximumFiles $MaximumEvidenceFiles -MaximumEntries $MaximumEvidenceEntries -MaximumBytes $MaximumEvidenceBytes
    $comparison | Add-Member -NotePropertyName EvidenceRetention -NotePropertyValue $retention
}

$json = ConvertTo-Json -InputObject $comparison -Depth 16
if (![string]::IsNullOrWhiteSpace($OutputPath)) {
    $resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
    if ($MeasurementPath | Where-Object { [IO.Path]::GetFullPath($_).Equals($resolvedOutput, [StringComparison]::OrdinalIgnoreCase) }) {
        throw 'Comparison output cannot overwrite an input measurement.'
    }
    $parentDirectory = [IO.Path]::GetDirectoryName($resolvedOutput)
    if (!(Test-Path -LiteralPath $parentDirectory -PathType Container)) { throw 'Comparison output directory must already exist.' }
    $stream = [IO.File]::Open($resolvedOutput, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json + [Environment]::NewLine)
        $stream.Write($bytes, 0, $bytes.Length)
    }
    finally { $stream.Dispose() }
}
[Console]::Out.WriteLine($json)
