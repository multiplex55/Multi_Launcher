#requires -Version 7.0

function Get-TestPerfField {
    param([AllowNull()][object]$Object, [Parameter(Mandatory)][string]$Name)

    if ($null -eq $Object) { return $null }
    if ($Object -is [System.Collections.IDictionary]) {
        foreach ($key in $Object.Keys) {
            if ([string]$key -ieq $Name) { return $Object[$key] }
        }
        return $null
    }
    $property = $Object.PSObject.Properties | Where-Object Name -ieq $Name | Select-Object -First 1
    if ($property) { return $property.Value }
    return $null
}

function Convert-TestPerfJsonMessage {
    param(
        [Parameter(Mandatory)][string]$Line,
        [AllowNull()][System.Collections.IDictionary]$SuiteMetadata
    )

    $message = ConvertFrom-Json -InputObject $Line -AsHashtable -ErrorAction Stop
    $type = [string](Get-TestPerfField $message 'type')
    $event = [string](Get-TestPerfField $message 'event')
    $nextest = Get-TestPerfField $message 'nextest'
    $status = [string](Get-TestPerfField $nextest 'status')
    if ([string]::IsNullOrWhiteSpace($status) -or $status -eq 'running') { $status = $event }
    $status = switch -Regex ($status.ToLowerInvariant()) {
        '^(ok|pass|passed)$' { 'PASS'; break }
        '^(fail|failed)$' { 'FAIL'; break }
        '^(timeout|timed_out)$' { 'TIMEOUT'; break }
        '^ignored$' { 'IGNORED'; break }
        '^filtered[_-]out$' { 'FILTERED'; break }
        default { $status.ToUpperInvariant() }
    }

    $duration = Get-TestPerfField $message 'exec_time'
    $timing = Get-TestPerfField $nextest 'time'
    if ($null -eq $duration) { $duration = Get-TestPerfField $timing 'exec_time' }
    if ($null -eq $duration) { $duration = Get-TestPerfField $timing 'duration' }
    if ($null -ne $duration) {
        $number = 0.0
        if ([double]::TryParse([string]$duration, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$number)) {
            $duration = $number
        }
        else { $duration = $null }
    }

    $binaryId = [string](Get-TestPerfField $nextest 'test_binary')
    if ([string]::IsNullOrWhiteSpace($binaryId)) { $binaryId = [string](Get-TestPerfField $nextest 'binary') }
    if ([string]::IsNullOrWhiteSpace($binaryId) -and $SuiteMetadata) {
        $binaryId = [string](Get-TestPerfField $SuiteMetadata 'test_binary')
        if ([string]::IsNullOrWhiteSpace($binaryId)) { $binaryId = [string](Get-TestPerfField $SuiteMetadata 'binary') }
    }
    $crate = [string](Get-TestPerfField $nextest 'crate')
    $kind = [string](Get-TestPerfField $nextest 'kind')
    if ($SuiteMetadata) {
        if ([string]::IsNullOrWhiteSpace($crate)) { $crate = [string](Get-TestPerfField $SuiteMetadata 'crate') }
        if ([string]::IsNullOrWhiteSpace($kind)) { $kind = [string](Get-TestPerfField $SuiteMetadata 'kind') }
    }

    return [pscustomobject][ordered]@{
        Type = $type
        Event = $event
        TestName = [string](Get-TestPerfField $message 'name')
        BinaryId = $binaryId
        Crate = $crate
        Kind = $kind
        Status = $status
        DurationSeconds = $duration
    }
}

function Convert-TestPerfInventory {
    param([Parameter(Mandatory)][string]$Path)

    $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace($raw)) { throw 'Nextest inventory output was empty.' }
    $document = ConvertFrom-Json -InputObject $raw -AsHashtable -ErrorAction Stop
    $suites = Get-TestPerfField $document 'rust-suites'
    if ($null -eq $suites) { throw 'Nextest JSON did not contain rust-suites.' }

    $cases = [System.Collections.Generic.List[object]]::new()
    foreach ($suiteEntry in $suites.GetEnumerator()) {
        $suite = $suiteEntry.Value
        $binaryId = [string](Get-TestPerfField $suite 'binary-id')
        if ([string]::IsNullOrWhiteSpace($binaryId)) { $binaryId = [string]$suiteEntry.Key }
        $testcases = Get-TestPerfField $suite 'testcases'
        if ($null -eq $testcases) { continue }
        foreach ($testEntry in $testcases.GetEnumerator()) {
            $case = $testEntry.Value
            $filter = Get-TestPerfField $case 'filter-match'
            $filterStatus = [string](Get-TestPerfField $filter 'status')
            $cases.Add([pscustomobject][ordered]@{
                BinaryId = $binaryId
                TestName = [string]$testEntry.Key
                Ignored = [bool](Get-TestPerfField $case 'ignored')
                FilterStatus = if ($filterStatus) { $filterStatus } else { $null }
            })
        }
    }
    return @($cases)
}

function Get-TestPerfSelectedCaseCount {
    param([AllowEmptyCollection()][object[]]$Cases = @())

    return @($Cases | Where-Object { !$_.Ignored -and $_.FilterStatus -eq 'matches' }).Count
}

function Test-TestPerfPathWithin {
    param([Parameter(Mandatory)][string]$Candidate, [Parameter(Mandatory)][string]$Directory)

    $trimCharacters = [char[]]@([char]92, [char]47)
    $candidatePath = [IO.Path]::GetFullPath($Candidate).TrimEnd($trimCharacters)
    $directoryPath = [IO.Path]::GetFullPath($Directory).TrimEnd($trimCharacters)
    $prefix = $directoryPath + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.Equals($directoryPath, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function ConvertTo-WindowsProcessArgument {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Argument)

    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashes = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]92) { $backslashes++; continue }
        if ($character -eq '"') {
            [void]$builder.Append([char]92, (2 * $backslashes + 1))
            [void]$builder.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) { [void]$builder.Append([char]92, $backslashes); $backslashes = 0 }
        [void]$builder.Append($character)
    }
    if ($backslashes -gt 0) { [void]$builder.Append([char]92, (2 * $backslashes)) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Invoke-TestPerfCommandCapture {
    param(
        [Parameter(Mandatory)][string]$ExecutablePath,
        [string[]]$Arguments = @(),
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [ValidateRange(1, 60)][int]$TimeoutSeconds = 15
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $ExecutablePath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        if ($null -eq $argument) { throw 'Native argument arrays cannot contain null values.' }
        $startInfo.ArgumentList.Add([string]$argument)
    }

    $process = [Diagnostics.Process]::new()
    try {
        $process.StartInfo = $startInfo
        if (!$process.Start()) { throw "Could not start executable: $ExecutablePath" }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill($true)
            $process.WaitForExit(3000) | Out-Null
            return [pscustomobject]@{ ExitCode = $null; Stdout = $null; Stderr = $null; TimedOut = $true }
        }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        return [pscustomobject]@{ ExitCode = $process.ExitCode; Stdout = $stdout; Stderr = $stderr; TimedOut = $false }
    }
    finally { $process.Dispose() }
}

function Invoke-TestPerfProcess {
    param(
        [Parameter(Mandatory)][string]$ExecutablePath,
        [string[]]$Arguments = @(),
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [ValidateRange(1, 900)][int]$TimeoutSeconds,
        [Parameter(Mandatory)][string]$OwnedRunnerPath,
        [Parameter(Mandatory)][string]$StdoutPath,
        [Parameter(Mandatory)][string]$StderrPath,
        [ValidateSet('human', 'json', 'inventory')][string]$OutputMode = 'human',
        [hashtable]$EnvironmentOverrides = @{}
    )

    if (!(Test-Path -LiteralPath $OwnedRunnerPath -PathType Leaf)) { throw "Owned process runner was not found: $OwnedRunnerPath" }
    $powerShell = Get-Command -Name pwsh.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1
    $powerShellPath = if ($powerShell.Source) { $powerShell.Source } else { $powerShell.Path }
    $evidenceDirectory = Join-Path ([IO.Path]::GetDirectoryName($StdoutPath)) ('owned-process-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($evidenceDirectory) | Out-Null
    $requestPath = Join-Path $evidenceDirectory 'request.json'
    $childScriptPath = Join-Path $evidenceDirectory 'invoke_owned_runner.ps1'
    $wrapperStdoutPath = Join-Path $evidenceDirectory 'wrapper.stdout.log'
    $wrapperStderrPath = Join-Path $evidenceDirectory 'wrapper.stderr.log'
    $request = [ordered]@{
        RunnerPath = $OwnedRunnerPath
        ExecutablePath = $ExecutablePath
        Arguments = @($Arguments)
        EncodedArguments = @($Arguments | ForEach-Object { ConvertTo-WindowsProcessArgument -Argument ([string]$_) })
        TimeoutSeconds = $TimeoutSeconds
        EvidenceDirectory = $evidenceDirectory
        EnvironmentOverrides = $EnvironmentOverrides
    }
    $request | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $requestPath -Encoding utf8
    @'
param([Parameter(Mandatory)][string]$RequestPath)
$ErrorActionPreference = 'Stop'
$request = Get-Content -LiteralPath $RequestPath -Raw | ConvertFrom-Json -AsHashtable -ErrorAction Stop
foreach ($name in $request.EnvironmentOverrides.Keys) {
    [Environment]::SetEnvironmentVariable([string]$name, $request.EnvironmentOverrides[$name], 'Process')
}
& $request.RunnerPath -Executable $request.ExecutablePath -Arguments @($request.EncodedArguments) `
    -TimeoutSeconds ([int]$request.TimeoutSeconds) -EvidenceDirectory $request.EvidenceDirectory
'@ | Set-Content -LiteralPath $childScriptPath -Encoding utf8

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $powerShellPath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in @('-NoProfile', '-File', $childScriptPath, '-RequestPath', $requestPath)) { $startInfo.ArgumentList.Add($argument) }

    $process = [Diagnostics.Process]::new()
    $caseRows = [System.Collections.Generic.List[object]]::new()
    $parseErrors = [System.Collections.Generic.List[string]]::new()
    $suiteMetadata = [ordered]@{}
    $startedUtc = [DateTimeOffset]::UtcNow
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $started = $false
    $timedOut = $false
    $exitCode = $null
    $cleanup = 'not-started'
    $runnerError = $null
    $process.StartInfo = $startInfo

    try {
        if (!$process.Start()) { throw "Could not start executable: $ExecutablePath" }
        $started = $true
        $waitMilliseconds = [Math]::Min([int]::MaxValue, ($TimeoutSeconds + 45) * 1000)
        if (!$process.WaitForExit($waitMilliseconds)) {
            $timedOut = $true
            $cleanup = 'outer-timeout-kill-tree-requested'
            $process.Kill($true)
            $process.WaitForExit(5000) | Out-Null
            if ($process.HasExited) { $exitCode = $process.ExitCode; $cleanup = 'outer-timeout-kill-tree-root-exited-descendants-unverified' }
            else { $cleanup = 'outer-timeout-cleanup-incomplete' }
        }
        else {
            $process.WaitForExit()
            $exitCode = $process.ExitCode
        }
    }
    catch {
        $runnerError = '{0}: {1}' -f $_.Exception.GetType().FullName, $_.Exception.Message
        if ($started) {
            try {
                if (!$process.HasExited) { $process.Kill($true); $process.WaitForExit(5000) | Out-Null }
                $exitCode = if ($process.HasExited) { $process.ExitCode } else { $null }
                $cleanup = if ($process.HasExited) { 'outer-runner-kill-tree-root-exited-descendants-unverified' } else { 'outer-runner-cleanup-incomplete' }
            }
            catch { $cleanup = 'outer-runner-cleanup-error' }
        }
        else { $cleanup = 'start-failed' }
    }
    finally {
        $watch.Stop()
        $process.Dispose()
    }

    $runnerResultPath = Join-Path $evidenceDirectory 'result.json'
    $runnerResult = $null
    if (Test-Path -LiteralPath $runnerResultPath -PathType Leaf) {
        try { $runnerResult = Get-Content -LiteralPath $runnerResultPath -Raw | ConvertFrom-Json -AsHashtable -ErrorAction Stop }
        catch { $runnerError = "Could not parse owned process result: $($_.Exception.Message)" }
    }
    $runnerStdoutPath = Join-Path $evidenceDirectory 'stdout.log'
    $runnerStderrPath = Join-Path $evidenceDirectory 'stderr.log'
    if (Test-Path -LiteralPath $runnerStdoutPath -PathType Leaf) { Copy-Item -LiteralPath $runnerStdoutPath -Destination $StdoutPath -Force }
    else { [IO.File]::WriteAllText($StdoutPath, '', [Text.UTF8Encoding]::new($false)) }
    if (Test-Path -LiteralPath $runnerStderrPath -PathType Leaf) { Copy-Item -LiteralPath $runnerStderrPath -Destination $StderrPath -Force }
    else { [IO.File]::WriteAllText($StderrPath, '', [Text.UTF8Encoding]::new($false)) }

    if ($null -ne $runnerResult) {
        $exitCode = $runnerResult.ExitCode
        if ($runnerResult.Reason -eq 'timeout') { $timedOut = $true }
        $cleanup = [string]$runnerResult.Reason
        $states = @($runnerResult.PostCleanup | ForEach-Object { [string]$_.State })
        if ($states.Count -gt 0) { $cleanup += '; post-cleanup=' + ($states -join ',') }
    }
    elseif ($null -eq $runnerError -and !$timedOut) { $runnerError = 'Owned process runner did not publish result.json.' }

    if (Test-Path -LiteralPath $StdoutPath -PathType Leaf) {
        foreach ($line in [IO.File]::ReadLines($StdoutPath)) {
            if ($OutputMode -eq 'json') { Receive-TestPerfProcessLine -Line $line -OutputMode $OutputMode -CaseRows $caseRows -ParseErrors $parseErrors -SuiteMetadata $suiteMetadata }
            elseif ($OutputMode -ne 'inventory' -and $line.Length -gt 0) { Write-Host $line }
        }
    }
    if (Test-Path -LiteralPath $StderrPath -PathType Leaf) {
        foreach ($line in [IO.File]::ReadLines($StderrPath)) { if ($line.Length -gt 0) { Write-Host "[stderr] $line" -ForegroundColor DarkGray } }
    }

    $status = if ($timedOut) { 'TIMEOUT' } elseif ($null -ne $runnerError) { 'RUNNER_ERROR' } elseif ($null -ne $runnerResult -and $runnerResult.Reason -eq 'completed' -and $exitCode -eq 0) { 'PASS' } elseif ($null -ne $runnerResult -and $runnerResult.Reason -eq 'failed exit') { 'FAIL' } else { 'RUNNER_ERROR' }
    return [pscustomobject][ordered]@{
        StartedUtc = $startedUtc.ToString('o')
        EndedUtc = [DateTimeOffset]::UtcNow.ToString('o')
        WallSeconds = [Math]::Round($watch.Elapsed.TotalSeconds, 6)
        ExitCode = $exitCode
        Status = $status
        TimedOut = $timedOut
        ProcessStarted = $started
        ProcessCleanup = $cleanup
        ProcessEvidenceDirectory = $evidenceDirectory
        OwnedProcessCount = if ($runnerResult) { @($runnerResult.OwnedProcesses).Count } else { $null }
        PostCleanupStates = if ($runnerResult) { @($runnerResult.PostCleanup) } else { @() }
        RunnerError = $runnerError
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
        PerCaseResults = @($caseRows)
        ParserErrors = @($parseErrors)
    }
}

function Receive-TestPerfProcessLine {
    param(
        [Parameter(Mandatory)][string]$Line,
        [Parameter(Mandatory)][string]$OutputMode,
        [Parameter(Mandatory)][AllowEmptyCollection()][System.Collections.Generic.List[object]]$CaseRows,
        [Parameter(Mandatory)][AllowEmptyCollection()][System.Collections.Generic.List[string]]$ParseErrors,
        [Parameter(Mandatory)][System.Collections.IDictionary]$SuiteMetadata
    )

    if ($OutputMode -eq 'json') {
        try {
            $rawMessage = ConvertFrom-Json -InputObject $Line -AsHashtable -ErrorAction Stop
            $type = [string](Get-TestPerfField $rawMessage 'type')
            if ($type -eq 'suite') {
                $suite = Get-TestPerfField $rawMessage 'nextest'
                if ($suite) {
                    foreach ($key in $suite.Keys) { $SuiteMetadata[$key] = $suite[$key] }
                }
                $event = [string](Get-TestPerfField $rawMessage 'event')
                Write-Host ("[nextest] suite {0} {1}" -f $event, [string](Get-TestPerfField $SuiteMetadata 'test_binary'))
            }
            elseif ($type -eq 'test') {
                $message = Convert-TestPerfJsonMessage -Line $Line -SuiteMetadata $SuiteMetadata
                if ($message.Event -ne 'started') {
                    $CaseRows.Add($message)
                    $duration = if ($null -eq $message.DurationSeconds) { '' } else { ' {0:N3}s' -f $message.DurationSeconds }
                    Write-Host ("[case] {0} {1}{2}" -f $message.Status, $message.TestName, $duration)
                }
                else { Write-Host ("[case] RUN {0}" -f $message.TestName) }
            }
        }
        catch {
            $ParseErrors.Add($_.Exception.Message)
            Write-Host "[unparsed nextest output] $Line"
        }
    }
    elseif ($OutputMode -ne 'inventory' -and $Line.Length -gt 0) { Write-Host $Line }
}
