#requires -Version 7.0
param([string]$EvidenceDirectory = (Join-Path $env:TEMP ('owned-process-fixtures-' + [guid]::NewGuid())))
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
$wrapper = Join-Path $PSScriptRoot 'run_owned_process.ps1'
$fixture = Join-Path $EvidenceDirectory 'fixture.ps1'
@'
param([string]$Mode, [string]$ReadyFile)
if ($Mode -eq 'nonzero') { Write-Output 'fixture exits7'; exit 7 }
$child = Start-Process -FilePath (Get-Command pwsh.exe).Source -ArgumentList @('-NoProfile', '-Command', '"Start-Sleep -Seconds 30"') -WindowStyle Hidden -PassThru
[pscustomobject]@{ParentPid = $PID; ParentCreated = ([Diagnostics.Process]::GetCurrentProcess().StartTime); ChildPid = $child.Id; ChildCreated = $child.StartTime} | ConvertTo-Json | Set-Content -LiteralPath $ReadyFile
Write-Output ('fixture childPID=' + $child.Id)
Start-Sleep -Seconds 30
'@ | Set-Content -LiteralPath $fixture

function Invoke-Fixture($Name, $Mode, $Timeout) {
    $directory = Join-Path $EvidenceDirectory $Name
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $ready = Join-Path $directory 'fixture-identities.json'
    & $wrapper -Executable (Get-Command pwsh.exe).Source -Arguments @('-NoProfile', '-File', ('"' + $fixture + '"'), '-Mode', $Mode, '-ReadyFile', ('"' + $ready + '"')) -TimeoutSeconds $Timeout -EvidenceDirectory $directory | Out-Null
    Get-Content -LiteralPath (Join-Path $directory 'result.json') -Raw | ConvertFrom-Json
}
function Assert-Contained($Result, $Directory, [switch]$RequireTree) {
    if ($Result.RootAlive -or @($Result.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) { throw 'Recorded process cleanup was incomplete' }
    $identitiesFile = Join-Path $Directory 'fixture-identities.json'
    if ($RequireTree -and !(Test-Path -LiteralPath $identitiesFile)) { throw 'Fixture child did not publish its identity before cleanup' }
    if (Test-Path -LiteralPath $identitiesFile) {
        $identities = Get-Content -LiteralPath $identitiesFile -Raw | ConvertFrom-Json
        if ($identities.ParentPid -ne $Result.RunnerPid -or $identities.ChildPid -eq $identities.ParentPid) { throw 'Fixture identities do not belong to the launched root' }
        $parentCreated = ([datetime]$identities.ParentCreated).ToUniversalTime()
        $childCreated = ([datetime]$identities.ChildCreated).ToUniversalTime()
        $runStarted = ([datetime]$Result.Started).ToUniversalTime()
        $runEnded = ([datetime]$Result.Ended).ToUniversalTime()
        $rootRecord = $Result.OwnedProcesses | Where-Object ProcessId -eq $Result.RunnerPid
        if (!$rootRecord -or [Math]::Abs(($parentCreated - ([datetime]$rootRecord.CreationDate).ToUniversalTime()).Ticks) -gt 100 -or
            $parentCreated -lt $runStarted -or $childCreated -lt $parentCreated -or $childCreated -gt $runEnded) { throw 'Fixture creation identities are stale or outside this run' }
        foreach ($role in @('Parent', 'Child')) {
            $expectedId = $identities.($role + 'Pid')
            $created = $identities.($role + 'Created')
            try {
                $live = [Diagnostics.Process]::GetProcessById($expectedId)
                if ([Math]::Abs(($live.StartTime.ToUniversalTime() - $created.ToUniversalTime()).Ticks) -le 100) { throw "Fixture $role survived cleanup" }
                $live.Dispose()
            } catch [ArgumentException] {} # The exact fixture identity is absent.
        }
        'Verified fixture parent/child identities absent' | Set-Content -LiteralPath (Join-Path $Directory 'fixture-post-cleanup.log')
    }
}

$timeoutResult = Invoke-Fixture 'timeout' 'tree' 3
if ($timeoutResult.Reason -ne 'timeout') { throw "Unexpected timeout fixture reason: $($timeoutResult.Reason)" }
Assert-Contained $timeoutResult (Join-Path $EvidenceDirectory 'timeout') -RequireTree
$failedResult = Invoke-Fixture 'nonzero' 'nonzero' 5
if ($failedResult.Reason -ne 'failed exit' -or $failedResult.ExitCode -ne 7) { throw 'Failed exit evidence was not preserved' }
Assert-Contained $failedResult (Join-Path $EvidenceDirectory 'nonzero')

$fallbackResult = & {
    # Simulate inventory failure after the controlled child has started, without
    # altering the wrapper or relying on restricted access in the environment.
    $inventoryFixtureReady = Join-Path $EvidenceDirectory 'inventory-failure/fixture-identities.json'
    function Get-CimInstance {
        $deadline = (Get-Date).AddSeconds(4)
        while (!(Test-Path -LiteralPath $inventoryFixtureReady) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 20 }
        throw 'controlled process inventory failure'
    }
    Invoke-Fixture 'inventory-failure' 'tree' 6
}
if ($fallbackResult.Reason -notlike 'containment monitor failure:*') { throw 'Inventory failure was not recorded' }
Assert-Contained $fallbackResult (Join-Path $EvidenceDirectory 'inventory-failure') -RequireTree
Write-Output "All controlled fixtures contained; evidence: $EvidenceDirectory"
