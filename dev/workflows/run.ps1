#requires -Version 7.0
<#
.SYNOPSIS
Resolves and runs one fixed developer workflow or previews it without execution.

.DESCRIPTION
Test previews require a successful matching discovery receipt. Discover builds
only the requested Cargo target, inventories catalog cases, and writes a receipt.
Run prepares target metadata, validates one exact case, and executes only that
case. Replay takes only an original failed workflow receipt and runs the
confirmed failed binary-qualified case. Every native child runs through the
checked measurement owner.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Preview', 'Discover', 'Run', 'Replay')]
    [string]$Action,

    [string]$Mode,

    [string]$SelectionName,

    [ValidateSet('lib', 'integration')]
    [string]$TargetKind,

    [string]$DiscoveryReceipt,

    [string]$ReplayReceipt,

    [string]$RunnerProfile,

    [ValidateSet('unknown', 'no-op', 'edited-source', 'preparation', 'cold')]
    [string]$WorkKind = 'unknown',

    [ValidateSet('unknown', 'warmed', 'clean', 'branch-switched', 'new-profile')]
    [string]$CacheState = 'unknown',

    [string]$BaselineSha,

    [string]$ControlledChangeId,

    [string[]]$ControlledChangePaths = @(),

    [string[]]$TestArguments = @(),

    [string]$EvidenceDirectory
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'engine.ps1')

try {
    $request = New-WorkflowRequestFromCli -Action $Action -Mode $Mode -SelectionName $SelectionName `
        -TargetKind $TargetKind -DiscoveryReceipt $DiscoveryReceipt -ReplayReceipt $ReplayReceipt -RunnerProfile $RunnerProfile `
        -WorkKind $WorkKind -CacheState $CacheState -BaselineSha $BaselineSha `
        -ControlledChangeId $ControlledChangeId -ControlledChangePaths @($ControlledChangePaths) `
        -TestArguments @($TestArguments)
    $result = Invoke-WorkflowAction -Request $request -EvidenceDirectory $EvidenceDirectory
}
catch {
    $result = [pscustomobject][ordered]@{
        Status = 'INVALID_REQUEST'
        ExitCode = 2
        Error = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
    }
}

$result | ConvertTo-Json -Depth 20
exit ([int]$result.ExitCode)
