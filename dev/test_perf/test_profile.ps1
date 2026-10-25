#requires -Version 7.0
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

function Assert-Equal {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -ne $Expected) { throw "$Label expected '$Expected', got '$Actual'." }
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('Multi Launcher\test perf self test-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$fixture = Join-Path $testRoot 'fixture.ps1'
$childFixture = Join-Path $testRoot 'child.ps1'
$ownedRunnerPath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\run_owned_process.ps1'))
$profileEvidence = Join-Path $testRoot 'profile evidence'
$blockedPath = Join-Path $testRoot 'empty path'
[IO.Directory]::CreateDirectory($blockedPath) | Out-Null
$pwsh = Get-Command -Name pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1
$pwshPath = if ($pwsh.Source) { $pwsh.Source } else { $pwsh.Path }

$fixtureSource = @'
param([string]$Case, [string]$Value, [string]$ChildPath)
if ($Case -eq 'timeout') {
    $childInfo = [Diagnostics.ProcessStartInfo]::new()
    $childInfo.FileName = (Get-Command pwsh.exe).Source
    $childInfo.UseShellExecute = $false
    $childInfo.CreateNoWindow = $true
    $childInfo.ArgumentList.Add('-NoProfile')
    $childInfo.ArgumentList.Add('-File')
    $childInfo.ArgumentList.Add($ChildPath)
    $child = [Diagnostics.Process]::Start($childInfo)
    [Console]::WriteLine("CHILD_PID=$($child.Id)")
    Start-Sleep -Seconds 30
    exit 0
}
$event = if ($Case -eq 'failure') { 'failed' } else { 'ok' }
$status = if ($Case -eq 'failure') { 'fail' } else { 'pass' }
$suite = [ordered]@{ type = 'suite'; event = 'started'; nextest = @{ crate = 'multi_launcher'; test_binary = 'multi_launcher::multi_launcher$gui'; kind = 'lib' } }
$started = [ordered]@{ type = 'test'; event = 'started'; name = $Value; nextest = @{ status = 'running' } }
$finished = [ordered]@{ type = 'test'; event = $event; name = $Value; exec_time = 0.015; nextest = @{ status = $status; time = @{ exec_time = 0.015 } } }
foreach ($message in @($suite, $started, $finished)) { [Console]::WriteLine(($message | ConvertTo-Json -Compress -Depth 5)) }
if ($Case -eq 'failure') { exit 7 }
exit 0
'@
$fixtureSource | Set-Content -LiteralPath $fixture -Encoding utf8
'Start-Sleep -Seconds 30' | Set-Content -LiteralPath $childFixture -Encoding utf8

$inventoryPath = Join-Path $testRoot 'inventory.json'
@'
{"test-count":3,"rust-suites":{"multi_launcher::lib":{"binary-id":"multi_launcher::lib","testcases":{"gui::tests::passes":{"ignored":false,"filter-match":{"status":"matches"}},"gui::tests::ignored":{"ignored":true,"filter-match":{"status":"matches"}},"gui::tests::filtered":{"ignored":false,"filter-match":{"status":"not-matches"}}}}}}
'@ | Set-Content -LiteralPath $inventoryPath -Encoding utf8
$inventory = @(Convert-TestPerfInventory -Path $inventoryPath)
Assert-Equal $inventory.Count 3 'Inventory case count'
Assert-Equal @($inventory | Where-Object Ignored).Count 1 'Ignored inventory count'
Assert-Equal (Get-TestPerfSelectedCaseCount -Cases $inventory) 1 'Runnable filtered inventory count'
Assert-Equal (Get-TestPerfSelectedCaseCount -Cases @([pscustomobject]@{Ignored=$false;FilterStatus='not-matches'})) 0 'No-match inventory count'

$quotedValue = 'spaces and "quotes" stay one argument'
$success = Invoke-TestPerfProcess -ExecutablePath $pwshPath -Arguments @('-NoProfile', '-File', $fixture, '-Case', 'success', '-Value', $quotedValue) `
    -WorkingDirectory $testRoot -TimeoutSeconds 60 -OwnedRunnerPath $ownedRunnerPath -StdoutPath (Join-Path $testRoot 'success.out') `
    -StderrPath (Join-Path $testRoot 'success.err') -OutputMode json
Assert-Equal $success.Status 'PASS' 'Synthetic success status'
Assert-Equal $success.PerCaseResults[0].TestName $quotedValue 'Native argument quoting'
Assert-Equal $success.PerCaseResults[0].DurationSeconds 0.015 'Per-case duration'
Assert-Equal $success.PerCaseResults[0].BinaryId 'multi_launcher::multi_launcher$gui' 'Suite binary metadata'
Assert-Equal $success.PerCaseResults[0].Crate 'multi_launcher' 'Suite crate metadata'
Assert-Equal $success.PerCaseResults[0].Kind 'lib' 'Suite kind metadata'
Assert-Equal $success.PerCaseResults.Count 1 'Started events are not counted as completed cases'

$failure = Invoke-TestPerfProcess -ExecutablePath $pwshPath -Arguments @('-NoProfile', '-File', $fixture, '-Case', 'failure', '-Value', 'synthetic::failure') `
    -WorkingDirectory $testRoot -TimeoutSeconds 60 -OwnedRunnerPath $ownedRunnerPath -StdoutPath (Join-Path $testRoot 'failure.out') `
    -StderrPath (Join-Path $testRoot 'failure.err') -OutputMode json
Assert-Equal $failure.ExitCode 7 'Synthetic native exit code'
Assert-Equal $failure.Status 'FAIL' 'Synthetic failure status'
Assert-Equal $failure.PerCaseResults[0].Status 'FAIL' 'Synthetic failed case status'

$timeout = Invoke-TestPerfProcess -ExecutablePath $pwshPath -Arguments @('-NoProfile', '-File', $fixture, '-Case', 'timeout', '-Value', '', '-ChildPath', $childFixture) `
    -WorkingDirectory $testRoot -TimeoutSeconds 2 -OwnedRunnerPath $ownedRunnerPath -StdoutPath (Join-Path $testRoot 'timeout.out') `
    -StderrPath (Join-Path $testRoot 'timeout.err') -OutputMode human
Assert-Equal $timeout.Status 'TIMEOUT' 'Synthetic timeout status'
if ($timeout.ProcessCleanup -notmatch '^timeout; post-cleanup=Absent(,Absent)+$') {
    throw "Owned runner did not verify cleanup of the timed-out process tree: $($timeout.ProcessCleanup)"
}
$childLine = Get-Content -LiteralPath $timeout.StdoutPath | Where-Object { $_ -match '^CHILD_PID=(\d+)$' } | Select-Object -First 1
if (!$childLine) { throw 'Timeout fixture did not publish its child process ID.' }
$childPid = [int]($childLine -replace '^CHILD_PID=', '')
Start-Sleep -Milliseconds 200
if (Get-Process -Id $childPid -ErrorAction SilentlyContinue) { throw "Owned timeout child $childPid remained alive." }

$profileScript = Join-Path $PSScriptRoot 'profile.ps1'
$profileResult = Invoke-TestPerfProcess -ExecutablePath $pwshPath -Arguments @('-NoProfile', '-File', $profileScript, '-Mode', 'SelectedBuild', '-TargetKind', 'Library', '-EvidenceDirectory', $profileEvidence) `
    -WorkingDirectory $testRoot -TimeoutSeconds 60 -OwnedRunnerPath $ownedRunnerPath -StdoutPath (Join-Path $testRoot 'profile.out') `
    -StderrPath (Join-Path $testRoot 'profile.err') -OutputMode human -EnvironmentOverrides @{ Path = $blockedPath }
Assert-Equal $profileResult.ExitCode 1 'Profile harness error exit code'
$receiptLine = Get-Content -LiteralPath $profileResult.StdoutPath | Where-Object { $_ -match '^Receipt: ' } | Select-Object -Last 1
if (!$receiptLine) { throw 'A harness startup failure did not publish its receipt.' }
$receiptPath = $receiptLine -replace '^Receipt: ', ''
if (!(Test-Path -LiteralPath $receiptPath -PathType Leaf)) { throw "Expected failure receipt was not written: $receiptPath" }
$receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json -ErrorAction Stop
Assert-Equal $receipt.Status 'FAIL' 'Failure receipt status'
if ([string]::IsNullOrWhiteSpace([string]$receipt.HarnessError)) { throw 'Failure receipt omitted the harness error.' }

Write-Host "Synthetic profile tests passed. Evidence: $testRoot"
