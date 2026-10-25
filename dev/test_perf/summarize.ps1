#requires -Version 7.0
[CmdletBinding()]
param(
    [string[]]$ReceiptPath = @(),
    [string]$EvidenceDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'Multi_Launcher\test-perf'),
    [ValidateRange(1, 100)][int]$SlowestCount = 20
)

$ErrorActionPreference = 'Stop'
if ($ReceiptPath.Count -eq 0) {
    if (!(Test-Path -LiteralPath $EvidenceDirectory -PathType Container)) { throw "Evidence directory does not exist: $EvidenceDirectory" }
    $latest = Get-ChildItem -LiteralPath $EvidenceDirectory -Filter 'receipt.json' -File -Recurse |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (!$latest) { throw "No receipt.json files were found under $EvidenceDirectory" }
    $ReceiptPath = @($latest.FullName)
}

$summaries = [System.Collections.Generic.List[object]]::new()
foreach ($path in $ReceiptPath) {
    $resolved = (Resolve-Path -LiteralPath $path -ErrorAction Stop).ProviderPath
    $receipt = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json -ErrorAction Stop
    if ($receipt.SchemaVersion -ne 1 -or $receipt.Harness -ne 'multi-launcher-test-perf') {
        throw "Unsupported test performance receipt: $resolved"
    }
    $cases = @($receipt.Results.PerCase)
    $slowest = @($cases | Where-Object { $null -ne $_.DurationSeconds } |
        Sort-Object { [double]$_.DurationSeconds } -Descending | Select-Object -First $SlowestCount |
        ForEach-Object { [pscustomobject]@{ BinaryId = $_.BinaryId; TestName = $_.TestName; Status = $_.Status; DurationSeconds = $_.DurationSeconds } })
    $summaries.Add([pscustomobject][ordered]@{
        Receipt = $resolved
        Mode = $receipt.Mode
        Status = $receipt.Status
        ExitCode = $receipt.ExitCode
        SourceSha = $receipt.Source.Sha
        SourceDirty = $receipt.Source.Dirty
        Target = $receipt.Target
        Filter = $receipt.Filter
        InventoryCases = $receipt.Inventory.TotalCases
        IgnoredCases = $receipt.Inventory.IgnoredCases
        Results = $receipt.Results
        Phases = @($receipt.Phases | ForEach-Object { [pscustomobject]@{ Name = $_.Name; Status = $_.Status; WallSeconds = $_.WallSeconds; ExitCode = $_.ExitCode; TimedOut = $_.TimedOut; ProcessCleanup = $_.ProcessCleanup } })
        SlowestCases = $slowest
        CasesWithoutDuration = @($cases | Where-Object { $null -eq $_.DurationSeconds })
        ParserErrors = @($receipt.Phases | ForEach-Object { $_.ParserErrors } | Where-Object { $_ })
    })
}

[pscustomobject][ordered]@{ SchemaVersion = 1; ReceiptCount = $summaries.Count; Runs = @($summaries) } |
    ConvertTo-Json -Depth 12
