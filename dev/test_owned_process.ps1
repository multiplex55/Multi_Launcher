#requires -Version 7.0
[CmdletBinding()]
param([string]$EvidenceDirectory = (Join-Path $env:TEMP ('owned-process-fixtures-' + [guid]::NewGuid().ToString('N'))))

$ErrorActionPreference = 'Stop'
$evidenceRoot = [IO.Path]::GetFullPath($EvidenceDirectory)
[IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
$wrapper = Join-Path $PSScriptRoot 'run_owned_process.ps1'
$ownerPath = Join-Path $PSScriptRoot 'workflows\owned_process.ps1'
$fixture = Join-Path $evidenceRoot 'controlled fixture.ps1'
$pwshPath = (Get-Command pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$caseReceipts = [System.Collections.Generic.List[object]]::new()

@'
param(
    [string]$Mode,
    [string]$ReadyFile,
    [string]$Value,
    [string]$Filter,
    [string]$EmbeddedQuote
)

if ($Mode -eq 'child') {
    [Console]::Out.WriteLine('descendant inherited output marker')
    [Console]::Out.Flush()
    [Console]::Error.WriteLine('descendant inherited error marker')
    [Console]::Error.Flush()
    Start-Sleep -Seconds 30
    exit 0
}
if ($Mode -eq 'nonzero') {
    [Console]::Out.WriteLine('fixture exits7')
    exit 7
}
if ($Mode -eq 'success') {
    [pscustomobject]@{ Value = $Value; Filter = $Filter; EmbeddedQuote = $EmbeddedQuote } |
        ConvertTo-Json -Compress | Write-Output
    [Console]::Error.WriteLine('fixture stderr line')
    exit 0
}
if ($Mode -eq 'marker') {
    [IO.File]::WriteAllText($Value, 'child user code ran')
    exit 0
}
if ($Mode -in @('tree', 'short-parent')) {
    $childExecutable = (Get-Command pwsh.exe -CommandType Application | Select-Object -First 1).Source
    $native = [Diagnostics.ProcessStartInfo]::new([string]$childExecutable)
    $native.UseShellExecute = $false
    $native.CreateNoWindow = $true
    foreach ($argument in @('-NoProfile', '-File', $PSCommandPath, '-Mode', 'child')) {
        $native.ArgumentList.Add([string]$argument)
    }
    $child = [Diagnostics.Process]::Start($native)
    try {
        $parent = [Diagnostics.Process]::GetCurrentProcess()
        [ordered]@{
            ParentPid = $PID
            ParentCreated = $parent.StartTime.ToUniversalTime().ToString('o')
            ChildPid = $child.Id
            ChildCreated = $child.StartTime.ToUniversalTime().ToString('o')
        } | ConvertTo-Json -Compress | Set-Content -LiteralPath $ReadyFile -Encoding utf8NoBOM
        [Console]::Out.WriteLine('parent output before child cleanup')
        [Console]::Out.Flush()
        Start-Sleep -Milliseconds 200
        if ($Mode -eq 'short-parent') { exit 0 }
        Start-Sleep -Seconds 30
        exit 0
    }
    finally {
        if ($child) { $child.Dispose() }
    }
}
throw "Unknown controlled fixture mode: $Mode"
'@ | Set-Content -LiteralPath $fixture -Encoding utf8NoBOM

. $ownerPath
$ownerNativeSource = [IO.File]::ReadAllText($ownerPath)
$wrapperSource = [IO.File]::ReadAllText($wrapper)
if (!$ownerNativeSource.Contains('startup.StartupInfo.Flags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;') -or
    !$ownerNativeSource.Contains('startup.StartupInfo.ShowWindow = hiddenWindow ? (ushort)0 : (ushort)1;') -or
    !$ownerNativeSource.Contains('(hiddenWindow ? CREATE_NO_WINDOW : 0u)') -or
    !$wrapperSource.Contains("[ValidateSet('Hidden', 'Normal')][string]`$WindowStyle = 'Hidden'") -or
    !$wrapperSource.Contains("HiddenWindow = `$WindowStyle -eq 'Hidden'")) {
    throw 'Hidden/default and Normal window startup settings do not honor STARTF_USESHOWWINDOW and the wrapper contract.'
}
$windowStartupContract = [pscustomobject]@{
    Result = 'PASS'
    Hidden = 'STARTF_USESHOWWINDOW, ShowWindow=0, CREATE_NO_WINDOW'
    Normal = 'STARTF_USESHOWWINDOW, ShowWindow=1, no CREATE_NO_WINDOW'
    WrapperDefault = 'Hidden'
}

function Save-OwnerCaseReceipt {
    param([string]$Name, [string]$Directory, [object]$Result)
    $receiptPath = Join-Path $Directory 'owner-result.json'
    $owned = @($Result.OwnedProcesses | ForEach-Object {
        [pscustomobject]@{ ProcessId = $_.ProcessId; CreationDate = $_.CreationDate }
    })
    $postCleanup = @($Result.PostCleanup | ForEach-Object {
        [pscustomobject]@{ ProcessId = $_.ProcessId; CreationDate = $_.CreationDate; State = $_.State }
    })
    $receipt = [pscustomobject][ordered]@{
        Case = $Name
        Stage = $Result.Stage
        StartedUtc = if ($Result.StartedUtc) { $Result.StartedUtc } else { $null }
        EndedUtc = if ($Result.EndedUtc) { $Result.EndedUtc } else { $null }
        DeadlineUtc = $Result.DeadlineUtc
        TimeoutSeconds = $Result.TimeoutSeconds
        TimedOut = $Result.TimedOut
        TimeoutReason = $Result.TimeoutReason
        ExitCode = $Result.ExitCode
        Reason = $Result.Reason
        Succeeded = $Result.Succeeded
        ProcessCreated = $Result.ProcessCreated
        ProcessStarted = $Result.ProcessStarted
        ProcessExited = $Result.ProcessExited
        CleanupVerified = $Result.CleanupVerified
        CleanupErrors = @($Result.CleanupErrors)
        OutputDrainVerified = $Result.OutputDrainVerified
        OutputBytes = [pscustomobject]@{
            StdoutRead = $Result.StdoutBytesRead
            StdoutRecorded = $Result.StdoutBytesRecorded
            StderrRead = $Result.StderrBytesRead
            StderrRecorded = $Result.StderrBytesRecorded
        }
        RecordingErrors = @($Result.RecordingErrors)
        OutputHandlesReleasedImmediately = if ($null -ne $Result.PSObject.Properties['OutputHandlesReleasedImmediately']) {
            [bool]$Result.OutputHandlesReleasedImmediately
        } else { $null }
        OwnedProcesses = $owned
        PostCleanup = $postCleanup
        StdoutPath = $Result.StdoutPath
        StderrPath = $Result.StderrPath
        ResultReceiptPath = $receiptPath
        ArgumentsPreEncoded = $Result.ArgumentsPreEncoded
    }
    $json = ConvertTo-Json -InputObject $receipt -Depth 8 -ErrorAction Stop
    [IO.File]::WriteAllText($receiptPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $script:caseReceipts.Add($receipt)
    return $receiptPath
}

function Invoke-OwnedFixture {
    param(
        [string]$Name,
        [string]$Mode,
        [int]$TimeoutSeconds = 10,
        [int]$CleanupTimeoutSeconds = 3,
        [string[]]$Arguments = @()
    )
    $directory = Join-Path $evidenceRoot $Name
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    $stdout = Join-Path $directory 'stdout.log'
    $stderr = Join-Path $directory 'stderr.log'
    $ready = Join-Path $directory 'fixture-identities.json'
    $fixtureArguments = @('-NoProfile', '-File', $fixture, '-Mode', $Mode)
    if ($Mode -in @('tree', 'short-parent')) { $fixtureArguments += @('-ReadyFile', $ready) }
    $fixtureArguments += $Arguments
    $result = Invoke-CheckedOwnedProcess -Executable $pwshPath -ArgumentList $fixtureArguments `
        -WorkingDirectory $evidenceRoot -Stage "test:$Name" -TimeoutSeconds $TimeoutSeconds `
        -CleanupTimeoutSeconds $CleanupTimeoutSeconds -StdoutPath $stdout -StderrPath $stderr
    Save-OwnerCaseReceipt -Name $Name -Directory $directory -Result $result | Out-Null
    return [pscustomobject]@{ Result = $result; Directory = $directory; Ready = $ready }
}

function Assert-ResultCleanup {
    param([object]$Result, [string]$Label)
    if (!$Result.CleanupVerified -or !$Result.OutputDrainVerified) { throw "$Label cleanup or output drain was not verified." }
    if (@($Result.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0) {
        throw "$Label owned identity remained alive or uncertain after cleanup."
    }
}

function Assert-FixtureIdentitiesAbsent {
    param([string]$ReadyPath, [string]$RunnerPath, [string]$Label)
    if (!(Test-Path -LiteralPath $ReadyPath -PathType Leaf)) { throw "$Label fixture did not publish parent and child identities." }
    $fixtureIdentity = Get-Content -LiteralPath $ReadyPath -Raw | ConvertFrom-Json
    if ($fixtureIdentity.ParentPid -eq $fixtureIdentity.ChildPid) { throw "$Label fixture identities are not distinct." }
    foreach ($role in @('Parent', 'Child')) {
        $pidValue = [int]$fixtureIdentity.($role + 'Pid')
        $expectedCreation = ([datetime]$fixtureIdentity.($role + 'Created')).ToUniversalTime()
        try {
            $process = [Diagnostics.Process]::GetProcessById($pidValue)
            try {
                if ([Math]::Abs(($process.StartTime.ToUniversalTime() - $expectedCreation).Ticks) -le 100) {
                    throw "$Label fixture $role identity $pidValue survived cleanup."
                }
            }
            finally { $process.Dispose() }
        }
        catch [ArgumentException] { } # PID is absent; the identity is retired.
    }
    $fixtureIdentity | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $RunnerPath 'fixture-post-cleanup.json')
}

function Assert-OutputLogsReleasedImmediately {
    param([string[]]$Paths)
    foreach ($path in $Paths) {
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "Expected output log was not created: $path" }
        $exclusive = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        try { } finally { $exclusive.Dispose() }
        Remove-Item -LiteralPath $path -Force -ErrorAction Stop
        [IO.File]::WriteAllBytes($path, [byte[]]::new(0))
    }
    return $true
}

$rawValue = 'one argument with spaces'
$rawFilter = "module::test-name[1], punctuation + && filter"
$rawQuote = 'embedded "quote" stays inside one argument'

$success = Invoke-OwnedFixture -Name 'raw-arguments-success' -Mode 'success' -Arguments @(
    '-Value', $rawValue, '-Filter', $rawFilter, '-EmbeddedQuote', $rawQuote
)
if (!$success.Result.Succeeded -or $success.Result.Reason -ne 'completed' -or $success.Result.ExitCode -ne 0) {
    throw 'The controlled successful child did not produce a successful receipt.'
}
if ($success.Result.Stage -ne 'test:raw-arguments-success' -or !$success.Result.DeadlineUtc -or
    $success.Result.ArgumentsPreEncoded -or $success.Result.ArgumentList.Count -ne 11) {
    throw 'The raw execution contract did not retain stage, deadline, or argument boundaries.'
}
Assert-ResultCleanup $success.Result 'Successful child'
$successOutput = Get-Content -LiteralPath $success.Result.StdoutPath -Raw | ConvertFrom-Json
if ($successOutput.Value -cne $rawValue -or $successOutput.Filter -cne $rawFilter -or $successOutput.EmbeddedQuote -cne $rawQuote) {
    throw 'Spaces, embedded quotes, or filter punctuation changed across the raw argument boundary.'
}
if ($success.Result.StdoutBytesRead -ne $success.Result.StdoutBytesRecorded -or
    $success.Result.StderrBytesRead -ne $success.Result.StderrBytesRecorded) {
    throw 'The successful child output byte receipts do not match the recorded logs.'
}

$failed = Invoke-OwnedFixture -Name 'exit-seven' -Mode 'nonzero' -TimeoutSeconds 5
if ($failed.Result.Reason -ne 'failed exit' -or $failed.Result.ExitCode -ne 7 -or $failed.Result.Succeeded) {
    throw 'Exit 7 was not retained as a distinct failed-exit result.'
}
Assert-ResultCleanup $failed.Result 'Exit-7 child'

$timeout = Invoke-OwnedFixture -Name 'parent-child-timeout' -Mode 'tree' -TimeoutSeconds 2 -CleanupTimeoutSeconds 3
if ($timeout.Result.Reason -ne 'timeout' -or !$timeout.Result.TimedOut -or
    $timeout.Result.TimeoutReason -ne 'stage deadline elapsed') {
    throw "The parent/child fixture did not produce a deadline timeout: $($timeout.Result.Reason)."
}
Assert-ResultCleanup $timeout.Result 'Timed-out parent/child tree'
Assert-FixtureIdentitiesAbsent $timeout.Ready $timeout.Directory 'Timed-out parent/child tree'

$shortParent = Invoke-OwnedFixture -Name 'short-parent-descendant' -Mode 'short-parent' -TimeoutSeconds 8 -CleanupTimeoutSeconds 1
if ($shortParent.Result.Reason -ne 'owned descendants survived the root process' -or
    !$shortParent.Result.ProcessExited -or $shortParent.Result.ExitCode -ne 0) {
    throw "The short-parent fixture did not report a surviving descendant: $($shortParent.Result.Reason)."
}
Assert-ResultCleanup $shortParent.Result 'Short parent with descendant'
Assert-FixtureIdentitiesAbsent $shortParent.Ready $shortParent.Directory 'Short parent with descendant'
$descendantOutput = [IO.File]::ReadAllText($shortParent.Result.StdoutPath)
if (!$descendantOutput.Contains('parent output before child cleanup') -or
    $shortParent.Result.StdoutBytesRead -ne $shortParent.Result.StdoutBytesRecorded) {
    throw 'Output inherited from the short-lived parent was not drained and recorded.'
}

$ownershipDirectory = Join-Path $evidenceRoot 'denied-ownership'
[IO.Directory]::CreateDirectory($ownershipDirectory) | Out-Null
$ownershipMarker = Join-Path $ownershipDirectory 'must-not-run.txt'
$ownershipArguments = @('-NoProfile', '-File', $fixture, '-Mode', 'marker', '-Value', $ownershipMarker)
$ownershipStdout = Join-Path $ownershipDirectory 'stdout.log'
$ownershipStderr = Join-Path $ownershipDirectory 'stderr.log'
$ownershipDenied = & {
    . $ownerPath
    function Add-CheckedOwnedProcessToJob {
        param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
        throw 'controlled Job Object assignment denial'
    }
    Invoke-CheckedOwnedProcess -Executable $pwshPath -ArgumentList $ownershipArguments `
        -WorkingDirectory $evidenceRoot -Stage 'ownership-denial' -TimeoutSeconds 5 -CleanupTimeoutSeconds 2 `
        -StdoutPath $ownershipStdout -StderrPath $ownershipStderr
}
$ownershipDenied | Add-Member -NotePropertyName OutputHandlesReleasedImmediately `
    -NotePropertyValue (Assert-OutputLogsReleasedImmediately @($ownershipStdout, $ownershipStderr)) -Force
Save-OwnerCaseReceipt -Name 'denied-ownership' -Directory $ownershipDirectory -Result $ownershipDenied | Out-Null
if ($ownershipDenied.Reason -notlike 'ownership failure:*' -or $ownershipDenied.Succeeded -or
    !$ownershipDenied.ProcessCreated -or $ownershipDenied.ProcessStarted -or (Test-Path -LiteralPath $ownershipMarker)) {
    throw 'Denied ownership did not fail closed before allowing the executable to run.'
}
Assert-ResultCleanup $ownershipDenied 'Denied ownership'
if (!$ownershipDenied.OutputHandlesReleasedImmediately) {
    throw 'Ownership denial returned before its output log handles were immediately released.'
}

$preResumeDirectory = Join-Path $evidenceRoot 'pre-resume-deadline'
$preResumeScript = Join-Path $preResumeDirectory 'deadline boundary fixture.ps1'
$preResumeMarker = Join-Path $preResumeDirectory 'must-not-run.txt'
$preResumeReportPath = Join-Path $preResumeDirectory 'caller-loop-and-owner-result.json'
[IO.Directory]::CreateDirectory($preResumeDirectory) | Out-Null
@'
param(
    [string]$OwnerPath,
    [string]$Executable,
    [string]$FixturePath,
    [string]$WorkingDirectory,
    [string]$MarkerPath,
    [string]$ReportPath
)
$ErrorActionPreference = 'Stop'
. $OwnerPath
$script:preResumeInventoryCalls = 0
function Get-CheckedOwnedProcessIds {
    param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
    $script:preResumeInventoryCalls++
    if ($script:preResumeInventoryCalls -eq 1) { Start-Sleep -Milliseconds 1300 }
    return [MultiLauncher.Dev.OwnedProcessNative]::GetProcessIds($Owner)
}

$result = $null
$invocationReturned = $false
$callerLoopCompleted = $false
for ($iteration = 0; $iteration -lt 1; $iteration++) {
    $result = Invoke-CheckedOwnedProcess -Executable $Executable `
        -ArgumentList @('-NoProfile', '-File', $FixturePath, '-Mode', 'marker', '-Value', $MarkerPath) `
        -WorkingDirectory $WorkingDirectory -Stage 'pre-resume-deadline' -TimeoutSeconds 1 -CleanupTimeoutSeconds 3 `
        -StdoutPath (Join-Path $WorkingDirectory 'pre-resume-stdout.log') `
        -StderrPath (Join-Path $WorkingDirectory 'pre-resume-stderr.log')
    $invocationReturned = $true
}
$callerLoopCompleted = $true
$report = [pscustomobject][ordered]@{
    InvocationReturned = $invocationReturned
    CallerLoopCompleted = $callerLoopCompleted
    Result = $result
}
[IO.File]::WriteAllText($ReportPath, (ConvertTo-Json -InputObject $report -Depth 12 -ErrorAction Stop), [Text.UTF8Encoding]::new($false))
'@ | Set-Content -LiteralPath $preResumeScript -Encoding utf8NoBOM

$preResumeOuterDirectory = Join-Path $preResumeDirectory 'outer-wrapper'
$preResumeOuter = & $wrapper -Executable $pwshPath -ArgumentList @(
    '-NoProfile', '-File', $preResumeScript,
    '-OwnerPath', $ownerPath,
    '-Executable', $pwshPath,
    '-FixturePath', $fixture,
    '-WorkingDirectory', $preResumeDirectory,
    '-MarkerPath', $preResumeMarker,
    '-ReportPath', $preResumeReportPath
) -Stage 'test:pre-resume-deadline-caller-escape' `
    -TimeoutSeconds 15 -CleanupTimeoutSeconds 3 -EvidenceDirectory $preResumeOuterDirectory -ReturnObject
Save-OwnerCaseReceipt -Name 'pre-resume-deadline-outer-wrapper' -Directory $preResumeOuterDirectory -Result $preResumeOuter | Out-Null
if (!$preResumeOuter.Succeeded -or $preResumeOuter.ExitCode -ne 0) {
    throw 'The bounded caller did not finish the deterministic pre-resume deadline regression fixture.'
}
if (!(Test-Path -LiteralPath $preResumeReportPath -PathType Leaf)) {
    throw 'The pre-resume timeout escaped the fixture host or omitted its returned receipt.'
}
$preResumeReport = Get-Content -LiteralPath $preResumeReportPath -Raw | ConvertFrom-Json
if (!$preResumeReport.InvocationReturned -or !$preResumeReport.CallerLoopCompleted -or !$preResumeReport.Result) {
    throw 'The pre-resume timeout did not return normally through its caller loop with a structured result.'
}
$preResumeResult = $preResumeReport.Result
$preResumeInnerDirectory = Join-Path $preResumeDirectory 'inner-owner'
[IO.Directory]::CreateDirectory($preResumeInnerDirectory) | Out-Null
Save-OwnerCaseReceipt -Name 'pre-resume-deadline' -Directory $preResumeInnerDirectory -Result $preResumeResult | Out-Null
if ($preResumeResult.Reason -ne 'timeout' -or !$preResumeResult.TimedOut -or
    $preResumeResult.TimeoutReason -ne 'stage deadline elapsed before process resume' -or
    $preResumeResult.Succeeded -or !$preResumeResult.ProcessCreated -or $preResumeResult.ProcessStarted -or
    !$preResumeResult.ProcessExited -or !$preResumeResult.CleanupVerified -or !$preResumeResult.OutputDrainVerified) {
    throw 'The expired pre-resume deadline did not return a false-success-safe cleanup receipt.'
}
if (@($preResumeResult.PostCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0 -or
    (Test-Path -LiteralPath $preResumeMarker)) {
    throw 'The pre-resume deadline left an owned identity or allowed child user code to run.'
}

$monitorDirectory = Join-Path $evidenceRoot 'monitor-denied'
[IO.Directory]::CreateDirectory($monitorDirectory) | Out-Null
$monitorReady = Join-Path $monitorDirectory 'fixture-identities.json'
$monitorArguments = @('-NoProfile', '-File', $fixture, '-Mode', 'tree', '-ReadyFile', $monitorReady)
$monitorDenied = & {
    . $ownerPath
    $script:controlledMonitorCalls = 0
    function Get-CheckedOwnedProcessIds {
        param([Parameter(Mandatory)][MultiLauncher.Dev.OwnedChild]$Owner)
        $script:controlledMonitorCalls++
        if ($script:controlledMonitorCalls -gt 1) {
            $readyDeadline = [Diagnostics.Stopwatch]::StartNew()
            while (!(Test-Path -LiteralPath $monitorReady) -and $readyDeadline.Elapsed.TotalSeconds -lt 3) {
                Start-Sleep -Milliseconds 20
            }
            throw 'controlled owned process inventory failure'
        }
        return [MultiLauncher.Dev.OwnedProcessNative]::GetProcessIds($Owner)
    }
    Invoke-CheckedOwnedProcess -Executable $pwshPath -ArgumentList $monitorArguments `
        -WorkingDirectory $evidenceRoot -Stage 'monitor-denial' -TimeoutSeconds 8 -CleanupTimeoutSeconds 2 `
        -StdoutPath (Join-Path $monitorDirectory 'stdout.log') -StderrPath (Join-Path $monitorDirectory 'stderr.log')
}
Save-OwnerCaseReceipt -Name 'denied-monitor' -Directory $monitorDirectory -Result $monitorDenied | Out-Null
if ($monitorDenied.Reason -notlike 'containment monitor failure:*' -or $monitorDenied.Succeeded -or
    !(Test-Path -LiteralPath $monitorReady)) {
    throw 'A denied live process inventory did not become a contained failure.'
}
Assert-ResultCleanup $monitorDenied 'Denied monitor'
Assert-FixtureIdentitiesAbsent $monitorReady $monitorDirectory 'Denied monitor'

$storageDirectory = Join-Path $evidenceRoot 'output-storage-denied'
[IO.Directory]::CreateDirectory($storageDirectory) | Out-Null
$storageMarker = Join-Path $storageDirectory 'must-not-run.txt'
$badOutput = Invoke-CheckedOwnedProcess -Executable $pwshPath -ArgumentList @(
    '-NoProfile', '-File', $fixture, '-Mode', 'success', '-Value', $storageMarker
) -WorkingDirectory $evidenceRoot -Stage 'output-storage-denial' -TimeoutSeconds 5 -CleanupTimeoutSeconds 2 `
    -StdoutPath (Join-Path $storageDirectory 'missing' 'stdout.log') -StderrPath (Join-Path $storageDirectory 'stderr.log')
Save-OwnerCaseReceipt -Name 'output-storage-denied' -Directory $storageDirectory -Result $badOutput | Out-Null
if ($badOutput.Reason -notlike 'output recording failure:*' -or $badOutput.Succeeded -or
    $badOutput.RecordingErrors.Count -eq 0 -or $badOutput.ProcessCreated -or (Test-Path -LiteralPath $storageMarker)) {
    throw 'An output-storage failure launched or falsely succeeded as the child.'
}

$compatibilityDirectory = Join-Path $evidenceRoot 'legacy-wrapper-contract'
[IO.Directory]::CreateDirectory($compatibilityDirectory) | Out-Null
$preEncoded = @(
    '-NoProfile', '-File', [MultiLauncher.Dev.OwnedProcessNative]::QuoteRawArgument($fixture),
    '-Mode', 'success', '-Value', [MultiLauncher.Dev.OwnedProcessNative]::QuoteRawArgument($rawValue),
    '-Filter', [MultiLauncher.Dev.OwnedProcessNative]::QuoteRawArgument($rawFilter),
    '-EmbeddedQuote', [MultiLauncher.Dev.OwnedProcessNative]::QuoteRawArgument($rawQuote)
)
$eventQueryMarker = Join-Path $compatibilityDirectory 'event-provider-was-called.txt'
$legacy = & {
    # A provider may stall instead of throwing. The bounded wrapper must not
    # invoke it at all; the marker catches even a swallowed provider failure.
    function Get-WinEvent {
        [IO.File]::WriteAllText($eventQueryMarker, 'unexpected provider invocation')
        throw 'Controlled unavailable event provider must not be queried.'
    }
    & $wrapper -Executable $pwshPath -Arguments $preEncoded -Stage 'legacy-test-perf-contract' `
        -TimeoutSeconds 8 -EvidenceDirectory $compatibilityDirectory -ReturnObject
}
Save-OwnerCaseReceipt -Name 'legacy-wrapper-contract' -Directory $compatibilityDirectory -Result $legacy | Out-Null
if (!$legacy.Succeeded -or !$legacy.ArgumentsPreEncoded -or $legacy.WindowStyle -ne 'Hidden' -or
    !$legacy.Started -or !$legacy.Ended -or $legacy.Arguments.Count -ne $preEncoded.Count) {
    throw 'The legacy pre-encoded wrapper contract or historical receipt fields changed.'
}
$legacyOutput = Get-Content -LiteralPath $legacy.StdoutPath -Raw | ConvertFrom-Json
if ($legacyOutput.Value -cne $rawValue -or $legacyOutput.Filter -cne $rawFilter -or $legacyOutput.EmbeddedQuote -cne $rawQuote) {
    throw 'Legacy pre-encoded arguments were silently reinterpreted.'
}
$persistedLegacy = Get-Content -LiteralPath $legacy.ResultPath -Raw | ConvertFrom-Json
if ((Test-Path -LiteralPath $eventQueryMarker) -or
    $legacy.EventCollectionStatus -cne 'NotCollected' -or
    $persistedLegacy.EventCollectionStatus -cne 'NotCollected' -or
    [string]::IsNullOrWhiteSpace($persistedLegacy.EventCollectionReason) -or
    [IO.File]::ReadAllText($legacy.EventLogPath).Trim() -cne '[]') {
    throw 'Optional event reporting queried a provider, omitted its receipt, or misrepresented uncollected events.'
}

$resultStorageDirectory = Join-Path $evidenceRoot 'result-storage-denied'
[IO.Directory]::CreateDirectory($resultStorageDirectory) | Out-Null
$invalidResultPath = Join-Path $resultStorageDirectory 'result-is-a-directory'
[IO.Directory]::CreateDirectory($invalidResultPath) | Out-Null
$unpersistedFailure = & $wrapper -Executable $pwshPath `
    -Arguments @('-NoProfile', '-File', [MultiLauncher.Dev.OwnedProcessNative]::QuoteRawArgument($fixture), '-Mode', 'nonzero') `
    -Stage 'result-storage-denial' -TimeoutSeconds 8 -EvidenceDirectory $resultStorageDirectory `
    -ResultPath $invalidResultPath -ReturnObject
Save-OwnerCaseReceipt -Name 'result-storage-denied' -Directory $resultStorageDirectory -Result $unpersistedFailure | Out-Null
if ($unpersistedFailure.Succeeded -or $unpersistedFailure.ExitCode -ne 7 -or
    $unpersistedFailure.Reason -ne 'failed exit' -or $unpersistedFailure.RecordingErrors.Count -eq 0) {
    throw 'Result storage failure masked exit 7 or produced a successful receipt.'
}
Assert-ResultCleanup $unpersistedFailure 'Unpersisted exit-7 child'

$summary = [pscustomobject][ordered]@{
    Result = 'PASS'
    EvidenceDirectory = $evidenceRoot
    Cases = @($caseReceipts)
    CaseCount = $caseReceipts.Count
    EvidenceRole = 'Controlled process fixtures; owner-result.json is the per-case immutable receipt.'
    WindowStartupContract = $windowStartupContract
    CaseNames = @(
        'raw argv spaces, quotes, and filter punctuation',
        'exit 7 with cleanup receipt',
        'parent/child timeout and independent identity retirement',
        'short parent with surviving descendant and inherited output',
        'denied ownership before resume',
        'ownership denial immediately releases output logs without garbage collection',
        'pre-resume deadline returns a receipt without child execution or caller escape',
        'hidden/default and Normal window startup contract',
        'denied live identity inventory with Job cleanup',
        'output storage denial before launch',
        'legacy pre-encoded wrapper and test_perf receipt compatibility',
        'optional event provider never invoked; empty artifact and explicit not-collected receipt',
        'result storage failure preserves the original exit failure'
    )
}
$summaryJson = ConvertTo-Json -InputObject $summary -Depth 12 -ErrorAction Stop
[IO.File]::WriteAllText((Join-Path $evidenceRoot 'suite-summary.json'), $summaryJson + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
$summary
