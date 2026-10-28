#requires -Version 7.0
[CmdletBinding(DefaultParameterSetName = 'Legacy')]
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    # Compatibility contract: each value is already encoded for the Windows
    # command line. This preserves the historical Start-Process behavior.
    [Parameter(ParameterSetName = 'Legacy')][AllowEmptyCollection()][string[]]$Arguments = @(),
    # New callers should pass unencoded native arguments through this parameter.
    [Parameter(Mandatory = $true, ParameterSetName = 'Raw')][AllowEmptyCollection()][string[]]$ArgumentList,
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 30,
    [ValidateRange(1, 60)][int]$CleanupTimeoutSeconds = 5,
    [string]$Stage = 'unspecified',
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [ValidateSet('Hidden', 'Normal')][string]$WindowStyle = 'Hidden',
    [string]$ResultPath,
    [switch]$ReturnObject
)

$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'This containment script requires Windows.' }

$fullEvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
$stdoutPath = Join-Path $fullEvidenceDirectory 'stdout.log'
$stderrPath = Join-Path $fullEvidenceDirectory 'stderr.log'
$processLog = Join-Path $fullEvidenceDirectory 'processes.jsonl'
$postCleanupPath = Join-Path $fullEvidenceDirectory 'post-cleanup-processes.json'
$eventPath = Join-Path $fullEvidenceDirectory 'windows-events.json'
if ([string]::IsNullOrWhiteSpace($ResultPath)) { $ResultPath = Join-Path $fullEvidenceDirectory 'result.json' }
$recordingErrors = [System.Collections.Generic.List[string]]::new()
$diagnosticErrors = [System.Collections.Generic.List[string]]::new()
$run = $null

try {
    [IO.Directory]::CreateDirectory($fullEvidenceDirectory) | Out-Null
    . (Join-Path $PSScriptRoot 'workflows\owned_process.ps1')
    $runParameters = @{
        Executable = $Executable
        WorkingDirectory = (Get-Location).Path
        TimeoutSeconds = $TimeoutSeconds
        CleanupTimeoutSeconds = $CleanupTimeoutSeconds
        Stage = $Stage
        HiddenWindow = $WindowStyle -eq 'Hidden'
        StdoutPath = $stdoutPath
        StderrPath = $stderrPath
    }
    if ($PSCmdlet.ParameterSetName -eq 'Raw') {
        $runParameters.ArgumentList = @($ArgumentList)
    }
    else {
        $runParameters.PreEncodedArguments = @($Arguments)
    }
    $run = Invoke-CheckedOwnedProcess @runParameters
}
catch {
    if ($null -eq $run) {
        $run = [pscustomobject][ordered]@{
            Stage = $Stage
            StartedUtc = $null
            EndedUtc = [DateTimeOffset]::UtcNow.ToString('o')
            DeadlineUtc = $null
            TimeoutSeconds = $TimeoutSeconds
            TimeoutReason = $null
            TimedOut = $false
            ElapsedSeconds = 0
            Executable = $Executable
            ArgumentList = if ($PSCmdlet.ParameterSetName -eq 'Raw') { @($ArgumentList) } else { @($Arguments) }
            ArgumentsPreEncoded = $PSCmdlet.ParameterSetName -eq 'Legacy'
            WorkingDirectory = (Get-Location).Path
            RunnerPid = $null
            ProcessCreated = $false
            ProcessStarted = $false
            ProcessExited = $false
            ExitCode = $null
            Reason = 'recorder or runner initialization failure: ' + $_.Exception.Message
            Succeeded = $false
            CleanupVerified = $false
            CleanupErrors = @()
            OutputDrainVerified = $false
            StdoutPath = $stdoutPath
            StderrPath = $stderrPath
            StdoutBytesRead = 0
            StdoutBytesRecorded = 0
            StderrBytesRead = 0
            StderrBytesRecorded = 0
            RecordingErrors = @($_.Exception.Message)
            OwnedProcesses = @()
            PostCleanup = @()
            RootAlive = $false
        }
    }
}

$owned = @($run.OwnedProcesses)
try {
    $lines = foreach ($identity in $owned) { ConvertTo-Json -InputObject $identity -Compress -Depth 5 }
    [IO.File]::WriteAllLines($processLog, [string[]]@($lines), [Text.UTF8Encoding]::new($false))
}
catch { $recordingErrors.Add(('Process identity log: {0}' -f $_.Exception.Message)) }

try {
    $postJson = ConvertTo-Json -InputObject @($run.PostCleanup) -Depth 5
    [IO.File]::WriteAllText($postCleanupPath, $postJson + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
catch { $recordingErrors.Add(('Post-cleanup receipt: {0}' -f $_.Exception.Message)) }

# Keep the legacy artifact, but never query an event provider on the receipt
# publication path: even a limited query can wait indefinitely on that service.
$eventCollectionStatus = 'NotCollected'
$eventCollectionReason = 'Optional Application events are not queried by the bounded process runner.'
try {
    [IO.File]::WriteAllText($eventPath, '[]' + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
catch { $diagnosticErrors.Add(('Empty event artifact: {0}' -f $_.Exception.Message)) }

foreach ($errorText in $run.RecordingErrors) { $recordingErrors.Add([string]$errorText) }
foreach ($errorText in $run.CleanupErrors) { $recordingErrors.Add([string]$errorText) }
$finalReason = [string]$run.Reason
if ($recordingErrors.Count -gt 0) {
    if ($finalReason -eq 'completed') { $finalReason = 'result recording failure' }
    $run.Succeeded = $false
}
$run | Add-Member -NotePropertyName ResultPath -NotePropertyValue $ResultPath -Force
$run | Add-Member -NotePropertyName ProcessLogPath -NotePropertyValue $processLog -Force
$run | Add-Member -NotePropertyName PostCleanupPath -NotePropertyValue $postCleanupPath -Force
$run | Add-Member -NotePropertyName EventLogPath -NotePropertyValue $eventPath -Force
$run | Add-Member -NotePropertyName EventCollectionStatus -NotePropertyValue $eventCollectionStatus -Force
$run | Add-Member -NotePropertyName EventCollectionReason -NotePropertyValue $eventCollectionReason -Force
$run | Add-Member -NotePropertyName DiagnosticErrors -NotePropertyValue @($diagnosticErrors) -Force
$run | Add-Member -NotePropertyName RecordingErrors -NotePropertyValue @($recordingErrors) -Force
$run | Add-Member -NotePropertyName Reason -NotePropertyValue $finalReason -Force
$run | Add-Member -NotePropertyName Started -NotePropertyValue $(if ($run.StartedUtc) { ([DateTimeOffset]$run.StartedUtc).LocalDateTime } else { $null }) -Force
$run | Add-Member -NotePropertyName Ended -NotePropertyValue $(if ($run.EndedUtc) { ([DateTimeOffset]$run.EndedUtc).LocalDateTime } else { $null }) -Force
$run | Add-Member -NotePropertyName Arguments -NotePropertyValue $(if ($PSCmdlet.ParameterSetName -eq 'Raw') { @($ArgumentList) } else { @($Arguments) }) -Force
$run | Add-Member -NotePropertyName WindowStyle -NotePropertyValue $WindowStyle -Force

$resultPersisted = $false
try {
    $json = ConvertTo-Json -InputObject $run -Depth 8
    [IO.File]::WriteAllText($ResultPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $resultPersisted = $true
}
catch {
    $recordingErrors.Add(('Result receipt: {0}' -f $_.Exception.Message))
    $run.Succeeded = $false
    if ($run.Reason -eq 'completed') { $run.Reason = 'result recording failure' }
    $run.RecordingErrors = @($recordingErrors)
}

if ($ReturnObject) { return $run }
Write-Output $ResultPath
if (!$run.Succeeded -or !$resultPersisted) { exit 1 }
exit 0
