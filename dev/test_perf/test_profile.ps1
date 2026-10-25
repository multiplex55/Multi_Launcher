#requires -Version 7.0
[CmdletBinding()]
param([switch]$ParserOnly)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

function Assert-Equal {
    param([object]$Actual, [object]$Expected, [string]$Label)
    if ($Actual -ne $Expected) { throw "$Label expected '$Expected', got '$Actual'." }
}

function Invoke-ParserIdentityFixtures {
    $inventoryCases = @(
        [pscustomobject]@{ BinaryId = 'multi_launcher'; PackageName = 'multi_launcher'; BinaryName = 'multi_launcher'; Kind = 'lib'; TestName = 'xml_toolkit_session::tests::done' },
        [pscustomobject]@{ BinaryId = 'multi_launcher::bookmarks_plugin'; PackageName = 'multi_launcher'; BinaryName = 'bookmarks_plugin'; Kind = 'test'; TestName = 'alias_roundtrip' },
        [pscustomobject]@{ BinaryId = 'multi_launcher::clipboard_modifiers'; PackageName = 'multi_launcher'; BinaryName = 'clipboard_modifiers'; Kind = 'test'; TestName = 'adjustment' }
    )
    $identityMap = New-TestPerfInventoryIdentityMap -Cases $inventoryCases
    $rows = [System.Collections.Generic.List[object]]::new()
    $errors = [System.Collections.Generic.List[string]]::new()
    $fixtureMessages = @(
        @{ type = 'suite'; event = 'started'; nextest = @{ crate = 'multi_launcher'; test_binary = 'bookmarks_plugin'; kind = 'test' } },
        @{ type = 'test'; event = 'started'; name = 'multi_launcher::bookmarks_plugin$alias_roundtrip' },
        @{ type = 'suite'; event = 'started'; nextest = @{ crate = 'multi_launcher'; test_binary = 'multi_launcher'; kind = 'lib' } },
        @{ type = 'test'; event = 'ok'; name = 'multi_launcher::bookmarks_plugin$alias_roundtrip'; exec_time = 0.01 },
        @{ type = 'test'; event = 'started'; name = 'multi_launcher::multi_launcher$xml_toolkit_session::tests::done' },
        @{ type = 'suite'; event = 'started'; nextest = @{ crate = 'multi_launcher'; test_binary = 'clipboard_modifiers'; kind = 'test' } },
        @{ type = 'test'; event = 'ok'; name = 'multi_launcher::multi_launcher$xml_toolkit_session::tests::done'; exec_time = 0.02 },
        @{ type = 'test'; event = 'ok'; name = 'multi_launcher::clipboard_modifiers$adjustment'; exec_time = 0.03 }
    )
    foreach ($message in $fixtureMessages) {
        Receive-TestPerfProcessLine -Line ($message | ConvertTo-Json -Compress -Depth 5) -OutputMode json `
            -CaseRows $rows -ParseErrors $errors -IdentityMap $identityMap
    }
    Assert-Equal $errors.Count 0 'Interleaved parser fixture errors'
    Assert-Equal $rows.Count 3 'Started announcements are not counted as completed cases'
    Assert-Equal $rows[0].BinaryName 'bookmarks_plugin' 'Interleaved bookmarks target attribution'
    Assert-Equal $rows[0].Kind 'test' 'Interleaved bookmarks kind attribution'
    Assert-Equal $rows[1].BinaryName 'multi_launcher' 'Interleaved library target attribution'
    Assert-Equal $rows[1].Kind 'lib' 'Interleaved library kind attribution'
    Assert-Equal $rows[2].BinaryName 'clipboard_modifiers' 'Interleaved clipboard target attribution'
    Assert-Equal $rows[2].IdentityStatus 'resolved' 'Interleaved identity status'

    $ambiguousCases = @(
        [pscustomobject]@{ BinaryId = 'multi_launcher::same::lib'; PackageName = 'multi_launcher'; BinaryName = 'same'; Kind = 'lib'; TestName = 'duplicate' },
        [pscustomobject]@{ BinaryId = 'multi_launcher::same::test'; PackageName = 'multi_launcher'; BinaryName = 'same'; Kind = 'test'; TestName = 'duplicate' }
    )
    $ambiguousMap = New-TestPerfInventoryIdentityMap -Cases $ambiguousCases
    $ambiguousLine = @{ type = 'test'; event = 'ok'; name = 'multi_launcher::same$duplicate' } | ConvertTo-Json -Compress
    $ambiguous = Convert-TestPerfJsonMessage -Line $ambiguousLine -IdentityMap $ambiguousMap
    Assert-Equal $ambiguous.IdentityStatus 'ambiguous' 'Ambiguous identity status'
    Assert-Equal $ambiguous.BinaryId $null 'Ambiguous binary id remains unknown'
    Assert-Equal $ambiguous.TargetName $null 'Ambiguous target remains unknown'
    Assert-Equal $ambiguous.Kind $null 'Ambiguous kind remains unknown'
    Assert-Equal $ambiguous.IdentityCandidates.Count 2 'Ambiguous candidates are retained'

    $unknownLine = @{ type = 'test'; event = 'ok'; name = 'multi_launcher::missing$case' } | ConvertTo-Json -Compress
    $unknown = Convert-TestPerfJsonMessage -Line $unknownLine -IdentityMap $identityMap
    Assert-Equal $unknown.IdentityStatus 'unknown' 'Unmatched identity status'
    Assert-Equal $unknown.BinaryId $null 'Unmatched binary id remains unknown'
}

Invoke-ParserIdentityFixtures
if ($ParserOnly) {
    Write-Host 'Pure parser identity fixtures passed.'
    return
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
$eventName = 'multi_launcher::multi_launcher$' + $Value
$suite = [ordered]@{ type = 'suite'; event = 'started'; nextest = @{ crate = 'multi_launcher'; test_binary = 'multi_launcher::multi_launcher$gui'; kind = 'lib' } }
$started = [ordered]@{ type = 'test'; event = 'started'; name = $eventName; nextest = @{ status = 'running' } }
$finished = [ordered]@{ type = 'test'; event = $event; name = $eventName; exec_time = 0.015; nextest = @{ status = $status; time = @{ exec_time = 0.015 } } }
foreach ($message in @($suite, $started, $finished)) { [Console]::WriteLine(($message | ConvertTo-Json -Compress -Depth 5)) }
if ($Case -eq 'failure') { exit 7 }
exit 0
'@
$fixtureSource | Set-Content -LiteralPath $fixture -Encoding utf8
'Start-Sleep -Seconds 30' | Set-Content -LiteralPath $childFixture -Encoding utf8

$inventoryPath = Join-Path $testRoot 'inventory.json'
@'
{"test-count":3,"rust-suites":{"multi_launcher::lib":{"package-name":"multi_launcher","binary-id":"multi_launcher::lib","binary-name":"multi_launcher","kind":"lib","testcases":{"gui::tests::passes":{"ignored":false,"filter-match":{"status":"matches"}},"gui::tests::ignored":{"ignored":true,"filter-match":{"status":"matches"}},"gui::tests::filtered":{"ignored":false,"filter-match":{"status":"not-matches"}}}}}}
'@ | Set-Content -LiteralPath $inventoryPath -Encoding utf8
$inventory = @(Convert-TestPerfInventory -Path $inventoryPath)
Assert-Equal $inventory.Count 3 'Inventory case count'
Assert-Equal @($inventory | Where-Object Ignored).Count 1 'Ignored inventory count'
Assert-Equal (Get-TestPerfSelectedCaseCount -Cases $inventory) 1 'Runnable filtered inventory count'
Assert-Equal (Get-TestPerfSelectedCaseCount -Cases @([pscustomobject]@{Ignored=$false;FilterStatus='not-matches'})) 0 'No-match inventory count'

$quotedValue = 'spaces and "quotes" stay one argument'
$quotedIdentity = @([pscustomobject]@{ BinaryId = 'multi_launcher'; PackageName = 'multi_launcher'; BinaryName = 'multi_launcher'; Kind = 'lib'; TestName = $quotedValue })
$success = Invoke-TestPerfProcess -ExecutablePath $pwshPath -Arguments @('-NoProfile', '-File', $fixture, '-Case', 'success', '-Value', $quotedValue) `
    -WorkingDirectory $testRoot -TimeoutSeconds 60 -OwnedRunnerPath $ownedRunnerPath -StdoutPath (Join-Path $testRoot 'success.out') `
    -StderrPath (Join-Path $testRoot 'success.err') -OutputMode json -InventoryCases $quotedIdentity
Assert-Equal $success.Status 'PASS' 'Synthetic success status'
Assert-Equal $success.PerCaseResults[0].TestName ('multi_launcher::multi_launcher$' + $quotedValue) 'Native argument quoting'
Assert-Equal $success.PerCaseResults[0].DurationSeconds 0.015 'Per-case duration'
Assert-Equal $success.PerCaseResults[0].BinaryId 'multi_launcher' 'Inventory binary metadata'
Assert-Equal $success.PerCaseResults[0].PackageName 'multi_launcher' 'Inventory package metadata'
Assert-Equal $success.PerCaseResults[0].BinaryName 'multi_launcher' 'Inventory target metadata'
Assert-Equal $success.PerCaseResults[0].Kind 'lib' 'Suite kind metadata'
Assert-Equal $success.PerCaseResults[0].IdentityStatus 'resolved' 'Inventory identity resolution'
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
