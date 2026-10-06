#requires -Version 7.0
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string[]]$Arguments,
    [ValidateRange(1, 86400)][int]$TimeoutSeconds = 30,
    [Parameter(Mandatory = $true)][string]$EvidenceDirectory,
    [ValidateSet('Hidden', 'Normal')][string]$WindowStyle = 'Hidden'
)
$ErrorActionPreference = 'Stop'
if (!$IsWindows) { throw 'This containment script requires Windows.' }
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
$started = Get-Date
$timeoutClock = [Diagnostics.Stopwatch]::StartNew()
$stdout = Join-Path $EvidenceDirectory 'stdout.log'
$stderr = Join-Path $EvidenceDirectory 'stderr.log'
$processLog = Join-Path $EvidenceDirectory 'processes.jsonl'
$resultLog = Join-Path $EvidenceDirectory 'result.json'
$owned = @{}
$runner = $null
$rootId = $null
$rootStarted = $null
$reason = 'completed'
$exitCode = $null
$rootAlive = $false
$postCleanup = @()

function Write-Diagnostic($Name, $Message) {
    try { $Message | Out-String | Add-Content -LiteralPath (Join-Path $EvidenceDirectory $Name) }
    catch { Write-Output "Could not persist $Name" }
}
function Record-Process($Candidate) {
    if (!$owned.ContainsKey([int]$Candidate.ProcessId)) {
        $record = [pscustomobject]@{
            ProcessId = [int]$Candidate.ProcessId
            ParentProcessId = [int]$Candidate.ParentProcessId
            CreationDate = $Candidate.CreationDate
            Name = $Candidate.Name
            CommandLine = $Candidate.CommandLine
        }
        $owned[[int]$Candidate.ProcessId] = $record
        $record | ConvertTo-Json -Compress | Add-Content -LiteralPath $processLog
    }
}
function Same-Identity($Candidate, $Record) {
    # CIM rounds creation time to microseconds; Process.StartTime is finer.
    return $Candidate.ProcessId -eq $Record.ProcessId -and
        [Math]::Abs(($Candidate.CreationDate.ToUniversalTime() - $Record.CreationDate.ToUniversalTime()).Ticks) -le 100
}
function Save-Events {
    $events = @(Get-WinEvent -FilterHashtable @{LogName = 'Application'; StartTime = $started; Id = 1000, 1001} -ErrorAction SilentlyContinue | Where-Object {
        $xml = [xml]$_.ToXml()
        $data = @($xml.Event.EventData.Data | ForEach-Object { $_.'#text' })
        $data -contains [IO.Path]::GetFileName($Executable) -or ($data | Where-Object {
            $value = $_
            @($owned.Keys | Where-Object { $value -eq [string]$_ -or $value -eq ('0x{0:x}' -f $_) }).Count -gt 0
        }).Count -gt 0
    } | ForEach-Object { [pscustomobject]@{Id = $_.Id; TimeCreated = $_.TimeCreated; Message = $_.Message; Xml = $_.ToXml()} })
    ConvertTo-Json -InputObject $events -Depth 6 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'windows-events.json')
}
function Check-RecordedProcess($Record) {
    $state = 'Unknown'
    $detail = $null
    try {
        $process = [Diagnostics.Process]::GetProcessById($Record.ProcessId)
        $state = if (Same-Identity ([pscustomobject]@{ProcessId = $process.Id; CreationDate = $process.StartTime}) $Record) { 'Alive' } else { 'Reused' }
        $process.Dispose()
    }
    catch [ArgumentException] { $state = 'Absent' }
    catch { $detail = $_.Exception.Message }
    [pscustomobject]@{ProcessId = $Record.ProcessId; CreationDate = $Record.CreationDate; State = $state; Detail = $detail}
}

try {
    $runner = Start-Process -FilePath $Executable -ArgumentList $Arguments -WindowStyle $WindowStyle -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    $rootId = $runner.Id
    $rootStarted = $runner.StartTime
    # Persist the launched identity before any inventory query, even for a
    # short-lived root. The retained Process handle also anchors cleanup.
    Record-Process ([pscustomobject]@{ProcessId = $rootId; ParentProcessId = $PID; CreationDate = $rootStarted; Name = [IO.Path]::GetFileName($Executable); CommandLine = $Executable + ' ' + ($Arguments -join ' ')})
    while ($true) {
        $snapshot = @(Get-CimInstance Win32_Process)
        do {
            $priorCount = $owned.Count
            $snapshot | Where-Object {
                $candidate = $_
                $parent = $snapshot | Where-Object ProcessId -eq $candidate.ParentProcessId
                $matchingParent = $parent -and $owned.ContainsKey([int]$parent.ProcessId) -and (Same-Identity $parent $owned[[int]$parent.ProcessId])
                $exitedRoot = !$parent -and $candidate.ParentProcessId -eq $rootId -and $runner.HasExited -and $candidate.CreationDate -ge $rootStarted -and $candidate.CreationDate -le $runner.ExitTime
                ($matchingParent -or $exitedRoot) -and $candidate.CreationDate -ge $rootStarted
            } | ForEach-Object { Record-Process $_ }
        } while ($owned.Count -ne $priorCount)
        $crashDetected = $false
        foreach ($reporter in $snapshot) {
            if ($reporter.Name -match '^WerFault(Secure)?\.exe$' -and $reporter.CreationDate -ge $started -and $reporter.CommandLine -match '-p\s+(\d+)') {
                $targetId = [int]$Matches[1]
                if (!$owned.ContainsKey($targetId)) { continue }
                $target = $snapshot | Where-Object ProcessId -eq $targetId
                if ($target -and (Same-Identity $target $owned[$targetId])) {
                    Record-Process $reporter
                    $reason = 'owned crash reporter'
                    $crashDetected = $true
                    break
                }
                # Historical PID alone cannot prove ownership after target exit.
                [pscustomobject]@{ReporterPid = $reporter.ProcessId; CreationDate = $reporter.CreationDate; TargetPid = $targetId; CommandLine = $reporter.CommandLine; Ownership = 'uncertain; target identity no longer live'} | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'uncertain-reporters.jsonl')
            }
        }
        if ($crashDetected) { break }
        $alive = @($snapshot | Where-Object { $owned.ContainsKey([int]$_.ProcessId) -and (Same-Identity $_ $owned[[int]$_.ProcessId]) })
        $runner.Refresh()
        if ($runner.HasExited -and $alive.Count -eq 0) { break }
        if ($timeoutClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { $reason = 'timeout'; break }
        Start-Sleep -Milliseconds 200
    }
}
catch { $reason = if ($runner) { 'containment monitor failure: ' + $_.Exception.Message } else { 'start failure: ' + $_.Exception.Message } }
finally {
    # Preserve evidence before termination; evidence-query failures cannot
    # bypass cleanup or final result publication.
    try { Save-Events } catch { Write-Diagnostic 'event-collection-error.log' $_ }
    if ($runner) {
        try {
            $runner.Refresh()
            if ($runner.HasExited -and $runner.ExitCode -ne 0 -and $reason -eq 'completed') { $reason = 'failed exit' }
        } catch { $reason = 'process handle inspection failed'; Write-Diagnostic 'handle-error.log' $_ }
        if ($reason -ne 'completed') {
            try {
                $snapshot = @(Get-CimInstance Win32_Process)
                $snapshot | Where-Object { $owned.ContainsKey([int]$_.ProcessId) -and (Same-Identity $_ $owned[[int]$_.ProcessId]) } | ForEach-Object {
                    & taskkill.exe /PID ([int]$_.ProcessId) /T /F 2>&1 | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'termination.log')
                }
            } catch { Write-Diagnostic 'cleanup-query-error.log' $_ }
            try {
                $runner.Refresh()
                if (!$runner.HasExited) {
                    $runner.Kill($true)
                    $runner.WaitForExit(3000) | Out-Null
                    Write-Diagnostic 'termination.log' 'Killed launched root through original process handle'
                }
            } catch { Write-Diagnostic 'cleanup-handle-error.log' $_ }
            foreach ($record in $owned.Values) {
                try {
                    $remaining = [Diagnostics.Process]::GetProcessById($record.ProcessId)
                    if (Same-Identity ([pscustomobject]@{ProcessId = $remaining.Id; CreationDate = $remaining.StartTime}) $record) {
                        $remaining.Kill($true)
                        $remaining.WaitForExit(3000) | Out-Null
                    }
                    $remaining.Dispose()
                }
                catch [ArgumentException] {} # Already absent.
                catch { Write-Diagnostic 'cleanup-handle-error.log' $_ }
            }
        }
        try { $runner.Refresh(); $rootAlive = !$runner.HasExited; if (!$rootAlive) { $exitCode = $runner.ExitCode } }
        catch { $rootAlive = $true; Write-Diagnostic 'handle-error.log' $_ }
    }
    $postCleanup = @($owned.Values | ForEach-Object { Check-RecordedProcess $_ })
    try { ConvertTo-Json -InputObject $postCleanup -Depth 4 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'post-cleanup-processes.json') }
    catch { Write-Diagnostic 'post-cleanup-persistence-error.log' $_; $reason = 'post-cleanup persistence failed' }
    if (@($postCleanup | Where-Object { $_.State -in @('Alive', 'Unknown') }).Count -gt 0 -and $reason -eq 'completed') { $reason = 'cleanup incomplete or uncertain' }
    $result = [pscustomobject]@{RunnerPid = $rootId; Started = $started; Ended = (Get-Date); Reason = $reason; ExitCode = $exitCode; RootAlive = $rootAlive; Executable = $Executable; Arguments = $Arguments; WindowStyle = $WindowStyle; OwnedProcesses = @($owned.Values); PostCleanup = $postCleanup}
    try { $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resultLog }
    catch { Write-Output ($result | ConvertTo-Json -Depth 6); $reason = 'result persistence failed' }
    Write-Output $resultLog
}
if ($reason -ne 'completed' -or $exitCode -ne 0 -or $rootAlive) { exit 1 }
exit 0
